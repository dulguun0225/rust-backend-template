//! The configuration, read from the environment. The defaults are this service's to change, each in one
//! commit that says why; `tests` pins them so a change is deliberate.

use std::net::SocketAddr;

/// What the binary needs to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `DATABASE_URL`, required.
    pub database_url: String,
    /// `BIND_ADDR`, default `0.0.0.0:8080`.
    pub bind: SocketAddr,
    /// `DATABASE_MAX_CONNECTIONS`, default 10: a small fixed pool, bounded at the limited resource.
    pub max_connections: u32,
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
        Ok(Self { database_url, bind, max_connections })
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
