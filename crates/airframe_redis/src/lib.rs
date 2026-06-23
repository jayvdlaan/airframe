//! Redis-backed `ByteCache` adapter for Airframe's data layer.
//!
//! `airframe_redis` implements `airframe_data`'s `ByteCache` over Redis, with
//! per-namespace key scoping, optional default TTL via `SETEX`, and a builder to
//! configure the connection URL, namespace, and TTL. An optional Airframe module
//! registers the cache (and an optional health check).
//!
//! # Key pieces
//! - [`AirframeRedisError`] — the crate error type.
//! - `RedisModule` — Airframe module (feature `module`) registering the cache.
//! - The Redis `ByteCache` implementation is available under the `driver` feature.
use std::time::Duration;

use airframe_data::cache::ByteCache;
use airframe_data::error::Result as DataResult;
use airframe_data::key::Key;
use thiserror::Error;

#[cfg(feature = "module")]
pub mod module;
#[cfg(feature = "module")]
pub use module::{RedisModule, ServiceRegistryRedisExt};

/// Crate identity string.
pub const CRATE: &str = "airframe_redis";

/// Simple readiness check placeholder.
pub fn ping() -> bool {
    true
}

#[derive(Debug, Error)]
pub enum AirframeRedisError {
    #[error("Redis error: {0}")]
    Redis(String),
    #[error("Timeout")]
    Timeout,
    #[error("Retry exhausted")]
    RetryExhausted,
}

#[cfg(feature = "driver")]
impl From<redis::RedisError> for AirframeRedisError {
    fn from(e: redis::RedisError) -> Self {
        AirframeRedisError::Redis(e.to_string())
    }
}

/// Format the fully-qualified Redis key with a namespace prefix.
/// Exposed for tests and utilities that need to compute the exact Redis key
/// without requiring a live Redis connection.
pub fn format_namespaced_key(namespace: &str, key: &Key) -> String {
    format!("{}::{}", namespace, key.as_str())
}

/// Simple retry policy and helper for transient errors like reconnection.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub backoff_ms: u64,
}

/// Retry a synchronous operation up to max_retries times with a fixed backoff.
pub fn retry<T>(
    policy: RetryPolicy,
    mut op: impl FnMut() -> Result<T, AirframeRedisError>,
) -> Result<T, AirframeRedisError> {
    let mut attempts = 0u32;
    loop {
        match op() {
            Ok(v) => return Ok(v),
            Err(_e) => {
                attempts += 1;
                if attempts > policy.max_retries {
                    return Err(AirframeRedisError::RetryExhausted);
                }
                std::thread::sleep(std::time::Duration::from_millis(policy.backoff_ms));
            }
        }
    }
}

/// Coarse-grained synchronous timeout wrapper.
pub fn run_with_timeout<T>(
    d: std::time::Duration,
    op: impl FnOnce() -> Result<T, AirframeRedisError> + Send + 'static,
) -> Result<T, AirframeRedisError>
where
    T: Send + 'static,
{
    use std::sync::mpsc;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(op());
    });
    match rx.recv_timeout(d) {
        Ok(res) => res,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(AirframeRedisError::Timeout),
        Err(_) => Err(AirframeRedisError::RetryExhausted),
    }
}

// Implementation when the real Redis driver is enabled
#[cfg(feature = "driver")]
mod imp {
    use super::*;

    /// Builder to construct a Redis-backed ByteCache.
    #[derive(Clone, Debug)]
    pub struct RedisByteCacheBuilder {
        url: String,
        namespace: String,
        default_ttl: Option<Duration>,
        reuse_connection: bool,
    }

    impl RedisByteCacheBuilder {
        pub fn new(url: impl Into<String>) -> Self {
            Self {
                url: url.into(),
                namespace: "default".into(),
                default_ttl: None,
                reuse_connection: false,
            }
        }
        pub fn namespace(mut self, ns: impl Into<String>) -> Self {
            self.namespace = ns.into();
            self
        }
        pub fn default_ttl(mut self, ttl: Duration) -> Self {
            self.default_ttl = Some(ttl);
            self
        }
        /// If enabled, the cache will try to reuse a single connection per instance.
        pub fn reuse_connection(mut self, reuse: bool) -> Self {
            self.reuse_connection = reuse;
            self
        }
        /// Apply a transformation to the builder (handy for conditional chaining).
        pub fn apply<F>(self, f: F) -> Self
        where
            F: FnOnce(Self) -> Self,
        {
            f(self)
        }
        pub fn build(self) -> Result<RedisByteCache, AirframeRedisError> {
            let client = redis::Client::open(self.url.as_str())?;
            let conn = if self.reuse_connection {
                match client.get_connection() {
                    Ok(c) => Some(std::sync::Arc::new(std::sync::Mutex::new(c))),
                    Err(_) => None,
                }
            } else {
                None
            };
            Ok(RedisByteCache {
                client,
                namespace: self.namespace,
                default_ttl: self.default_ttl,
                conn,
            })
        }
    }

    /// A Redis-backed ByteCache with optional namespace and default TTL.
    #[derive(Clone)]
    pub struct RedisByteCache {
        pub(crate) client: redis::Client,
        namespace: String,
        default_ttl: Option<Duration>,
        conn: Option<std::sync::Arc<std::sync::Mutex<redis::Connection>>>,
    }

    impl RedisByteCache {
        pub(crate) fn full_key(&self, key: &Key) -> String {
            super::format_namespaced_key(&self.namespace, key)
        }
        fn get_conn(&self) -> std::result::Result<redis::Connection, redis::RedisError> {
            if let Some(shared) = &self.conn {
                // Hold the lock briefly to hint reuse; then drop explicitly
                drop(shared.lock());
            }
            self.client.get_connection()
        }
        /// Paginated SCAN helper: returns a vector of pages, each with up to `count` keys.
        pub fn scan_keys_paginated(&self, count: u32) -> DataResult<Vec<Vec<Key>>> {
            let mut con = self
                .get_conn()
                .map_err(|_| airframe_data::error::AirframeDataError::InvalidState)?;
            let mut cursor: u64 = 0;
            let pattern = format!("{}::*", self.namespace);
            let mut pages: Vec<Vec<Key>> = Vec::new();
            loop {
                let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                    .cursor_arg(cursor)
                    .arg("MATCH")
                    .arg(&pattern)
                    .arg("COUNT")
                    .arg(count)
                    .query(&mut con)
                    .map_err(|_| airframe_data::error::AirframeDataError::InvalidState)?;
                let mut page = Vec::new();
                for full in keys {
                    if let Some(rest) = full.strip_prefix(&format!("{}::", self.namespace)) {
                        if let Ok(k) = Key::new(rest) {
                            page.push(k);
                        }
                    }
                }
                if !page.is_empty() {
                    pages.push(page);
                }
                if next_cursor == 0 {
                    break;
                }
                cursor = next_cursor;
            }
            Ok(pages)
        }
    }

    impl ByteCache for RedisByteCache {
        fn put_bytes(&self, key: &Key, bytes: &[u8]) -> DataResult<()> {
            let full = self.full_key(key);
            let mut con = self
                .get_conn()
                .map_err(|_| airframe_data::error::AirframeDataError::InvalidState)?;
            if let Some(ttl) = self.default_ttl {
                let secs = ttl.as_secs() as usize;
                redis::cmd("SETEX")
                    .arg(&full)
                    .arg(secs)
                    .arg(bytes)
                    .query::<String>(&mut con)
                    .map_err(|_| airframe_data::error::AirframeDataError::InvalidState)?;
            } else {
                redis::cmd("SET")
                    .arg(&full)
                    .arg(bytes)
                    .query::<String>(&mut con)
                    .map_err(|_| airframe_data::error::AirframeDataError::InvalidState)?;
            }
            Ok(())
        }

        fn get_bytes(&self, key: &Key) -> DataResult<Option<Vec<u8>>> {
            let full = self.full_key(key);
            let mut con = self
                .client
                .get_connection()
                .map_err(|_| airframe_data::error::AirframeDataError::InvalidState)?;
            let data: Option<Vec<u8>> = redis::cmd("GET")
                .arg(&full)
                .query(&mut con)
                .map_err(|_| airframe_data::error::AirframeDataError::InvalidState)?;
            Ok(data)
        }

        fn remove(&self, key: &Key) -> DataResult<()> {
            let full = self.full_key(key);
            let mut con = self
                .client
                .get_connection()
                .map_err(|_| airframe_data::error::AirframeDataError::InvalidState)?;
            let _: i64 = redis::cmd("DEL")
                .arg(&full)
                .query(&mut con)
                .map_err(|_| airframe_data::error::AirframeDataError::InvalidState)?;
            Ok(())
        }

        fn contains(&self, key: &Key) -> DataResult<bool> {
            let full = self.full_key(key);
            let mut con = self
                .client
                .get_connection()
                .map_err(|_| airframe_data::error::AirframeDataError::InvalidState)?;
            let exists: i64 = redis::cmd("EXISTS")
                .arg(&full)
                .query(&mut con)
                .map_err(|_| airframe_data::error::AirframeDataError::InvalidState)?;
            Ok(exists > 0)
        }

        fn list(&self) -> DataResult<Vec<Key>> {
            let mut con = self
                .client
                .get_connection()
                .map_err(|_| airframe_data::error::AirframeDataError::InvalidState)?;
            let pattern = format!("{}::*", self.namespace);
            let mut cmd = redis::cmd("SCAN");
            cmd.cursor_arg(0);
            cmd.arg("MATCH");
            cmd.arg(&pattern);
            let mut iter: redis::Iter<String> = cmd
                .iter(&mut con)
                .map_err(|_| airframe_data::error::AirframeDataError::InvalidState)?;
            let mut out = Vec::new();
            while let Some(full) = iter.next() {
                if let Some(rest) = full.strip_prefix(&format!("{}::", self.namespace)) {
                    if let Ok(k) = Key::new(rest) {
                        out.push(k);
                    }
                }
            }
            Ok(out)
        }
    }
}

// Stub implementation when the driver is disabled
#[cfg(not(feature = "driver"))]
mod imp {
    use super::*;

    #[derive(Clone, Debug)]
    #[allow(dead_code)]
    pub struct RedisByteCacheBuilder {
        url: String,
        namespace: String,
        default_ttl: Option<Duration>,
        reuse_connection: bool,
    }

    impl RedisByteCacheBuilder {
        pub fn new(url: impl Into<String>) -> Self {
            Self {
                url: url.into(),
                namespace: "default".into(),
                default_ttl: None,
                reuse_connection: false,
            }
        }
        pub fn namespace(mut self, ns: impl Into<String>) -> Self {
            self.namespace = ns.into();
            self
        }
        pub fn default_ttl(mut self, _ttl: Duration) -> Self {
            self.default_ttl = None;
            self
        }
        pub fn reuse_connection(mut self, _reuse: bool) -> Self {
            self.reuse_connection = false;
            self
        }
        pub fn apply<F>(self, f: F) -> Self
        where
            F: FnOnce(Self) -> Self,
        {
            f(self)
        }
        pub fn build(self) -> Result<RedisByteCache, AirframeRedisError> {
            Err(AirframeRedisError::RetryExhausted)
        }
    }

    #[derive(Clone)]
    pub struct RedisByteCache {
        _phantom: (),
    }

    impl ByteCache for RedisByteCache {
        fn put_bytes(&self, _key: &Key, _bytes: &[u8]) -> DataResult<()> {
            Err(airframe_data::error::AirframeDataError::InvalidState)
        }
        fn get_bytes(&self, _key: &Key) -> DataResult<Option<Vec<u8>>> {
            Err(airframe_data::error::AirframeDataError::InvalidState)
        }
        fn remove(&self, _key: &Key) -> DataResult<()> {
            Err(airframe_data::error::AirframeDataError::InvalidState)
        }
        fn contains(&self, _key: &Key) -> DataResult<bool> {
            Err(airframe_data::error::AirframeDataError::InvalidState)
        }
        fn list(&self) -> DataResult<Vec<Key>> {
            Err(airframe_data::error::AirframeDataError::InvalidState)
        }
    }
}

pub use imp::{RedisByteCache, RedisByteCacheBuilder};
