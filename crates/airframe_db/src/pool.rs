use std::sync::Arc;
use std::time::Duration;

use crate::connection::DbConnection;
use crate::error::{AirframeDbError, Result};

/// A simple pool that constructs new connections on demand using a cloned factory.
/// This is not a resource-tracking pool; it’s a light helper to standardize
/// connection acquisition for adapters that are cheap to connect or maintain
/// their own internal pooling.
pub struct NewConnPool<F, C>
where
    F: Fn() -> Result<C> + Send + Sync + 'static,
    C: DbConnection,
{
    factory: Arc<F>,
    _marker: std::marker::PhantomData<C>,
}

impl<F, C> Clone for NewConnPool<F, C>
where
    F: Fn() -> Result<C> + Send + Sync + 'static,
    C: DbConnection,
{
    fn clone(&self) -> Self {
        Self {
            factory: self.factory.clone(),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<F, C> NewConnPool<F, C>
where
    F: Fn() -> Result<C> + Send + Sync + 'static,
    C: DbConnection,
{
    pub fn new(factory: F) -> Self {
        Self {
            factory: Arc::new(factory),
            _marker: std::marker::PhantomData,
        }
    }

    /// Optionally perform a ping on construction to validate configuration.
    pub fn validate_with_ping(self) -> Result<Self> {
        let conn = (self.factory)()?;
        conn.ping()?;
        Ok(self)
    }
}

impl<F, C> crate::connection::DbPool for NewConnPool<F, C>
where
    F: Fn() -> Result<C> + Send + Sync + 'static,
    C: DbConnection,
{
    type Conn = C;
    fn get(&self) -> Result<Self::Conn> {
        (self.factory)()
    }
}

/// A helper to run a ping loop for readiness checks.
pub fn wait_until_ready<P: crate::connection::DbPool>(
    pool: &P,
    retries: u32,
    delay: Duration,
) -> Result<()> {
    let mut attempts = 0;
    loop {
        let conn = pool.get();
        match conn {
            Ok(c) => {
                if c.ping().is_ok() {
                    return Ok(());
                }
            }
            Err(_) => { /* fallthrough to retry */ }
        }
        attempts += 1;
        if attempts > retries {
            return Err(AirframeDbError::RetryExhausted);
        }
        std::thread::sleep(delay);
    }
}
