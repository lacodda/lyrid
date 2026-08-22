use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};

/// Default bind address when `LYRID_ADDR` is not set.
const DEFAULT_ADDR: &str = "0.0.0.0:8080";

/// Runtime configuration, read from the environment.
#[derive(Debug, Clone)]
pub struct Config {
    /// Address the HTTP server binds to (`LYRID_ADDR`).
    pub addr: SocketAddr,
    /// `PostgreSQL` connection string (`DATABASE_URL`).
    pub database_url: String,
    /// Directory holding the built SPA and the tile pyramid (`LYRID_STATIC`).
    ///
    /// Unset in development, where Vite serves those and proxies the API here.
    /// Set on a stand, where this process is the only thing listening — which
    /// is the difference the stand exists to expose.
    pub static_dir: Option<PathBuf>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// The environment is passed in as a lookup so tests can supply their own.
    fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let addr = lookup("LYRID_ADDR").unwrap_or_else(|| DEFAULT_ADDR.to_string());
        let addr = addr.parse().with_context(|| format!("LYRID_ADDR is not a valid socket address: {addr}"))?;
        let database_url = lookup("DATABASE_URL").context("DATABASE_URL is not set (e.g. postgres://lyrid:lyrid@localhost:5432/lyrid)")?;
        let static_dir = lookup("LYRID_STATIC").filter(|path| !path.trim().is_empty()).map(PathBuf::from);
        Ok(Self {
            addr,
            database_url,
            static_dir,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| v.to_string())
    }

    #[test]
    fn defaults_the_bind_address() {
        let config = Config::from_lookup(env(&[("DATABASE_URL", "postgres://localhost/lyrid")])).expect("config should build with only DATABASE_URL set");
        assert_eq!(config.addr, DEFAULT_ADDR.parse().unwrap());
        assert_eq!(config.database_url, "postgres://localhost/lyrid");
    }

    #[test]
    fn reads_the_bind_address_override() {
        let config = Config::from_lookup(env(&[("DATABASE_URL", "postgres://localhost/lyrid"), ("LYRID_ADDR", "127.0.0.1:9090")]))
            .expect("config should accept a valid override");
        assert_eq!(config.addr, "127.0.0.1:9090".parse().unwrap());
    }

    #[test]
    fn requires_database_url() {
        let error = Config::from_lookup(env(&[])).unwrap_err();
        assert!(error.to_string().contains("DATABASE_URL"));
    }

    #[test]
    fn serves_no_static_files_unless_told_where() {
        // Development is the unset case: Vite serves the SPA and proxies here.
        let config = Config::from_lookup(env(&[("DATABASE_URL", "postgres://localhost/lyrid")])).unwrap();
        assert!(config.static_dir.is_none());
    }

    #[test]
    fn reads_the_static_directory() {
        let config = Config::from_lookup(env(&[("DATABASE_URL", "postgres://localhost/lyrid"), ("LYRID_STATIC", "/srv/lyrid")])).unwrap();
        assert_eq!(config.static_dir.as_deref(), Some(std::path::Path::new("/srv/lyrid")));
    }

    #[test]
    fn treats_an_empty_static_directory_as_unset() {
        // An unset variable and one set to nothing mean the same thing; a
        // compose file that leaves it blank should not make the server serve
        // the process's working directory.
        let config = Config::from_lookup(env(&[("DATABASE_URL", "postgres://localhost/lyrid"), ("LYRID_STATIC", "  ")])).unwrap();
        assert!(config.static_dir.is_none());
    }

    #[test]
    fn rejects_a_malformed_bind_address() {
        let error = Config::from_lookup(env(&[("DATABASE_URL", "postgres://localhost/lyrid"), ("LYRID_ADDR", "not-an-address")])).unwrap_err();
        assert!(error.to_string().contains("LYRID_ADDR"));
    }
}
