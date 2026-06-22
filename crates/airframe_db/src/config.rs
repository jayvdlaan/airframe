use std::collections::HashMap;

/// Parsed database connection string with common fields.
///
/// `Debug` is implemented manually to redact the password and the original
/// (which embeds the password), so connection strings never leak credentials
/// through log lines or `{:?}` formatting.
#[derive(Clone, PartialEq, Eq)]
pub struct ConnectionString {
    pub scheme: String,
    pub user: Option<String>,
    pub password: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub params: HashMap<String, String>,
    /// Original input for reference/debugging.
    pub original: String,
}

impl std::fmt::Debug for ConnectionString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionString")
            .field("scheme", &self.scheme)
            .field("user", &self.user)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("params", &self.params)
            .field("original", &"<redacted>")
            .finish()
    }
}

/// Mask any `user:password@` credentials in a connection URL so it is safe to log.
///
/// `mysql://user:secret@host/db` becomes `mysql://user:***@host/db`. URLs without
/// embedded credentials are returned unchanged.
pub fn redact_url(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    let after = scheme_end + 3;
    let Some(at_rel) = url[after..].find('@') else {
        return url.to_string();
    };
    let at = after + at_rel;
    let creds = &url[after..at];
    let masked = match creds.split_once(':') {
        Some((user, _pass)) => format!("{user}:***"),
        None => creds.to_string(),
    };
    format!("{}{}@{}", &url[..after], masked, &url[at + 1..])
}

impl ConnectionString {
    pub fn is_in_memory_sqlite(&self) -> bool {
        self.scheme == "sqlite"
            && self.host.is_none()
            && self.database.as_deref() == Some(":memory:")
    }
}

/// Minimal pool configuration with sensible defaults.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PoolConfig {
    pub max_size: Option<u32>,
    pub connect_timeout_ms: Option<u64>,
}

impl PoolConfig {
    /// Validate constraints and return an adjusted config (e.g., clamp zeros to None).
    pub fn validated(self) -> Result<Self, crate::AirframeDbError> {
        if let Some(ms) = self.max_size {
            if ms == 0 {
                return Err(crate::AirframeDbError::InvalidState);
            }
        }
        if let Some(to) = self.connect_timeout_ms {
            if to == 0 {
                return Err(crate::AirframeDbError::InvalidState);
            }
        }
        Ok(self)
    }
}

/// Parse a database connection string into parts.
/// Supported examples:
/// - sqlite::memory:
/// - mysql://user:pass@localhost:3306/mydb?ssl=off&pool=5
pub fn parse_connection_string(input: &str) -> Result<ConnectionString, crate::AirframeDbError> {
    // Special-case sqlite::memory:
    if input.starts_with("sqlite::memory:") || input == "sqlite::memory:" {
        return Ok(ConnectionString {
            scheme: "sqlite".into(),
            user: None,
            password: None,
            host: None,
            port: None,
            database: Some(":memory:".into()),
            params: HashMap::new(),
            original: input.to_string(),
        });
    }

    // Generic scheme://... parser
    let (scheme, rest) = match input.split_once("://") {
        Some((s, r)) => (s.to_string(), r),
        None => return Err(crate::AirframeDbError::InvalidState),
    };

    let mut user: Option<String> = None;
    let mut password: Option<String> = None;
    let mut host_port_db = rest;

    // Credentials part user[:pass]@
    if let Some(at_pos) = rest.find('@') {
        let (cred, remainder) = rest.split_at(at_pos);
        host_port_db = &remainder[1..]; // skip '@'
        if let Some((u, p)) = cred.split_once(':') {
            user = Some(percent_decode(u));
            password = Some(percent_decode(p));
        } else if !cred.is_empty() {
            user = Some(percent_decode(cred));
        }
    }

    // Split path (db) and query
    let mut params = HashMap::new();
    let (host_port_path, query) = match host_port_db.split_once('?') {
        Some((a, q)) => (a, Some(q)),
        None => (host_port_db, None),
    };

    if let Some(q) = query {
        for pair in q.split('&') {
            if pair.is_empty() {
                continue;
            }
            let (k, v) = match pair.split_once('=') {
                Some((k, v)) => (k, v),
                None => (pair, ""),
            };
            params.insert(percent_decode(k), percent_decode(v));
        }
    }

    // host[:port]/database (database may contain slashes for some schemes, but we keep it simple here)
    let (host_port, database) = match host_port_path.split_once('/') {
        Some((hp, db)) => (hp, Some(percent_decode(db))),
        None => (host_port_path, None),
    };

    let mut host: Option<String> = None;
    let mut port: Option<u16> = None;
    if !host_port.is_empty() {
        if let Some((h, p)) = host_port.rsplit_once(':') {
            host = Some(percent_decode(h));
            if let Ok(parsed) = p.parse::<u16>() {
                port = Some(parsed);
            } else {
                return Err(crate::AirframeDbError::InvalidState);
            }
        } else {
            host = Some(percent_decode(host_port));
        }
    }

    Ok(ConnectionString {
        scheme,
        user,
        password,
        host,
        port,
        database,
        params,
        original: input.to_string(),
    })
}

fn percent_decode(s: &str) -> String {
    // Minimal percent-decoding: %HH hex
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &s[i + 1..i + 3];
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sqlite_memory() {
        let cs = parse_connection_string("sqlite::memory:").unwrap();
        assert_eq!(cs.scheme, "sqlite");
        assert!(cs.host.is_none());
        assert_eq!(cs.database.as_deref(), Some(":memory:"));
        assert!(cs.is_in_memory_sqlite());
    }

    #[test]
    fn parse_mysql_full() {
        let cs = parse_connection_string("mysql://user:pass@localhost:3306/mydb?ssl=off&pool=5")
            .unwrap();
        assert_eq!(cs.scheme, "mysql");
        assert_eq!(cs.user.as_deref(), Some("user"));
        assert_eq!(cs.password.as_deref(), Some("pass"));
        assert_eq!(cs.host.as_deref(), Some("localhost"));
        assert_eq!(cs.port, Some(3306));
        assert_eq!(cs.database.as_deref(), Some("mydb"));
        assert_eq!(cs.params.get("ssl").map(String::as_str), Some("off"));
        assert_eq!(cs.params.get("pool").map(String::as_str), Some("5"));
    }

    #[test]
    fn parse_with_percent_decoding() {
        let cs = parse_connection_string("mysql://us%65r:p%61ss@h%6Fst/db%32").unwrap();
        assert_eq!(cs.user.as_deref(), Some("user"));
        assert_eq!(cs.password.as_deref(), Some("pass"));
        assert_eq!(cs.host.as_deref(), Some("host"));
        assert_eq!(cs.database.as_deref(), Some("db2"));
    }

    #[test]
    fn parse_errors() {
        assert!(parse_connection_string("not-a-url").is_err());
        assert!(parse_connection_string("mysql://host:badport/db").is_err());
    }

    #[test]
    fn pool_config_validation() {
        let ok = PoolConfig {
            max_size: Some(1),
            connect_timeout_ms: Some(1),
        }
        .validated()
        .unwrap();
        assert_eq!(ok.max_size, Some(1));
        assert!(PoolConfig {
            max_size: Some(0),
            connect_timeout_ms: None
        }
        .validated()
        .is_err());
        assert!(PoolConfig {
            max_size: None,
            connect_timeout_ms: Some(0)
        }
        .validated()
        .is_err());
    }
}
