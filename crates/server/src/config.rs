//! The configuration, read from the environment. The defaults are this service's to change, each in one
//! commit that says why; `tests` pins them so a change is deliberate.

use std::net::SocketAddr;
use std::num::NonZeroU32;

/// What the binary needs to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `DATABASE_URL`, required.
    pub database_url: String,
    /// `BIND_ADDR`, default `0.0.0.0:8080`.
    pub bind: SocketAddr,
    /// `DATABASE_MAX_CONNECTIONS`, default 10: a small fixed pool, bounded at the limited resource.
    pub max_connections: u32,
    /// `REQUEST_BODY_MAX_BYTES`, default 65,536, more than 0: the most bytes a request body may have. A larger
    /// body is refused with 413 `request.too-large`, `params.max` this value. Size it from the largest legal
    /// request times six, since JSON may send any character escaped as six (RFC 8259 §7).
    pub request_body_max_bytes: NonZeroU32,
}

/// A configuration value that is missing or unreadable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// A required variable is unset.
    #[error("{0} is not set")]
    Missing(&'static str),
    /// A variable does not parse.
    #[error("{0} is not valid")]
    Invalid(&'static str),
}

const DEFAULT_BIND: SocketAddr = SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 8080);
const DEFAULT_MAX_CONNECTIONS: u32 = 10;
const DEFAULT_REQUEST_BODY_MAX_BYTES: NonZeroU32 = match NonZeroU32::new(65_536) {
    Some(bytes) => bytes,
    None => NonZeroU32::MIN,
};

impl Config {
    /// Reads the configuration through `lookup`, normally `std::env::var`.
    ///
    /// # Errors
    /// [`ConfigError`] naming the variable.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let database_url = lookup("DATABASE_URL").ok_or(ConfigError::Missing("DATABASE_URL"))?;
        let bind = match lookup("BIND_ADDR") {
            Some(v) => v.parse().map_err(|_| ConfigError::Invalid("BIND_ADDR"))?,
            None => DEFAULT_BIND,
        };
        let max_connections = match lookup("DATABASE_MAX_CONNECTIONS") {
            Some(v) => v.parse().ok().filter(|n| *n > 0).ok_or(ConfigError::Invalid("DATABASE_MAX_CONNECTIONS"))?,
            None => DEFAULT_MAX_CONNECTIONS,
        };
        let request_body_max_bytes = match lookup("REQUEST_BODY_MAX_BYTES") {
            Some(v) => v.parse().map_err(|_| ConfigError::Invalid("REQUEST_BODY_MAX_BYTES"))?,
            None => DEFAULT_REQUEST_BODY_MAX_BYTES,
        };
        Ok(Self { database_url, bind, max_connections, request_body_max_bytes })
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, ConfigError};

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let owned: Vec<(String, String)> = pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
        move |key| owned.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    #[test]
    fn the_committed_defaults() {
        let config = Config::from_lookup(env(&[("DATABASE_URL", "postgres://x")])).unwrap();
        assert_eq!(config.bind.to_string(), "0.0.0.0:8080");
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.request_body_max_bytes.get(), 65_536);
    }

    #[test]
    fn the_body_limit_is_read_and_more_than_zero() {
        let read = Config::from_lookup(env(&[("DATABASE_URL", "x"), ("REQUEST_BODY_MAX_BYTES", "1048576")])).unwrap();
        assert_eq!(read.request_body_max_bytes.get(), 1_048_576);
        for refused in ["0", "-1", "64k", ""] {
            assert_eq!(
                Config::from_lookup(env(&[("DATABASE_URL", "x"), ("REQUEST_BODY_MAX_BYTES", refused)])),
                Err(ConfigError::Invalid("REQUEST_BODY_MAX_BYTES")),
                "{refused}"
            );
        }
    }

    #[test]
    fn a_missing_or_invalid_value_is_named() {
        assert_eq!(Config::from_lookup(env(&[])), Err(ConfigError::Missing("DATABASE_URL")));
        assert_eq!(
            Config::from_lookup(env(&[("DATABASE_URL", "x"), ("DATABASE_MAX_CONNECTIONS", "0")])),
            Err(ConfigError::Invalid("DATABASE_MAX_CONNECTIONS"))
        );
        assert_eq!(
            Config::from_lookup(env(&[("DATABASE_URL", "x"), ("BIND_ADDR", "nope")])),
            Err(ConfigError::Invalid("BIND_ADDR"))
        );
    }
}
