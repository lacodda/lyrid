use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};

/// Default bind address when `LYRID_ADDR` is not set.
const DEFAULT_ADDR: &str = "0.0.0.0:8080";

/// Default sender for the two letters this service sends.
const DEFAULT_MAIL_FROM: &str = "lyrid <no-reply@localhost>";

/// Runtime configuration, read from the environment.
#[derive(Debug, Clone)]
pub struct Config {
    /// Address the HTTP server binds to (`LYRID_ADDR`).
    pub addr: SocketAddr,
    /// `PostgreSQL` connection string (`DATABASE_URL`).
    pub database_url: String,
    /// Whether session cookies are marked `Secure` (`LYRID_SECURE_COOKIE`).
    ///
    /// Off by default because the stand is plain HTTP on a home network, and
    /// a `Secure` cookie there is never sent at all -- an account nobody can
    /// log into is not more secure, it is broken. Production over HTTPS turns
    /// it on.
    pub secure_cookie: bool,
    /// Directory holding the built SPA and the tile pyramid (`LYRID_STATIC`).
    ///
    /// Unset in development, where Vite serves those and proxies the API here.
    /// Set on a stand, where this process is the only thing listening — which
    /// is the difference the stand exists to expose.
    pub static_dir: Option<PathBuf>,
    /// Where this service is reached from outside (`LYRID_PUBLIC_URL`).
    ///
    /// Needed because the links in a letter are read in a mail client, not in
    /// the browser that made the request: there is no page to be relative to.
    /// The bind address cannot stand in for it -- a stand binds `0.0.0.0` and
    /// is reached by a name, and behind a reverse proxy the two share nothing
    /// at all.
    pub public_url: String,
    /// SMTP connection URL, credentials included (`LYRID_SMTP_URL`).
    ///
    /// Unset on a stand with no route to the internet, where letters are
    /// logged rather than sent -- a supported configuration, not a fallback.
    pub smtp_url: Option<String>,
    /// Who the letters come from (`LYRID_MAIL_FROM`).
    pub mail_from: String,
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
        // Anything but an explicit "true" leaves it off: a typo in a compose
        // file must not silently switch on a flag that makes every login fail
        // over plain HTTP.
        let secure_cookie = lookup("LYRID_SECURE_COOKIE").is_some_and(|value| value.trim().eq_ignore_ascii_case("true"));

        // Trailing slashes are trimmed here rather than at every join: a link
        // built from a URL someone typed with a slash must not come out with
        // two, because a doubled slash is a different path to most routers
        // and a 404 in a letter is unfixable from the recipient's side.
        let public_url = lookup("LYRID_PUBLIC_URL")
            .map(|url| url.trim().trim_end_matches('/').to_string())
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| format!("http://{addr}"));
        let smtp_url = lookup("LYRID_SMTP_URL").map(|url| url.trim().to_string()).filter(|url| !url.is_empty());
        let mail_from = lookup("LYRID_MAIL_FROM")
            .map(|from| from.trim().to_string())
            .filter(|from| !from.is_empty())
            .unwrap_or_else(|| DEFAULT_MAIL_FROM.to_string());

        Ok(Self {
            addr,
            database_url,
            secure_cookie,
            static_dir,
            public_url,
            smtp_url,
            mail_from,
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
    fn cookies_are_insecure_unless_told_otherwise() {
        // The stand is plain HTTP; defaulting to Secure would make every
        // login there fail with no visible error at all.
        let config = Config::from_lookup(env(&[("DATABASE_URL", "postgres://localhost/lyrid")])).unwrap();
        assert!(!config.secure_cookie);
    }

    #[test]
    fn only_an_explicit_true_turns_secure_cookies_on() {
        let on = |value: &str| {
            Config::from_lookup(env(&[("DATABASE_URL", "postgres://localhost/lyrid"), ("LYRID_SECURE_COOKIE", value)]))
                .unwrap()
                .secure_cookie
        };
        assert!(on("true"));
        assert!(on("TRUE"));
        assert!(on(" true "));
        // A value meant as "off" must never read as "on".
        assert!(!on("false"));
        assert!(!on("0"));
        assert!(!on(""), "an empty value is not a yes");
        assert!(!on("yes"), "only the documented word counts");
    }

    #[test]
    fn the_public_url_falls_back_to_the_bind_address() {
        // Right in development, where the two are the same thing. Wrong on a
        // stand -- which is why the variable exists and why a deployment sets
        // it.
        let config = Config::from_lookup(env(&[("DATABASE_URL", "postgres://localhost/lyrid"), ("LYRID_ADDR", "127.0.0.1:8080")])).unwrap();
        assert_eq!(config.public_url, "http://127.0.0.1:8080");
    }

    #[test]
    fn a_trailing_slash_in_the_public_url_does_not_double_up() {
        // The links in a letter are built by joining a path onto this. A
        // doubled slash is a different path to most routers, and a 404 inside
        // a letter cannot be fixed by whoever received it.
        let with = |value: &str| {
            Config::from_lookup(env(&[("DATABASE_URL", "postgres://localhost/lyrid"), ("LYRID_PUBLIC_URL", value)]))
                .unwrap()
                .public_url
        };
        assert_eq!(with("https://lyrid.example/"), "https://lyrid.example");
        assert_eq!(with("https://lyrid.example///"), "https://lyrid.example");
        assert_eq!(with("  https://lyrid.example  "), "https://lyrid.example");
    }

    #[test]
    fn no_smtp_url_is_a_configuration_and_not_a_mistake() {
        // The home stand has no route to the internet. Letters are logged
        // there, and that has to be reachable without setting anything.
        let config = Config::from_lookup(env(&[("DATABASE_URL", "postgres://localhost/lyrid")])).unwrap();
        assert!(config.smtp_url.is_none());
        assert_eq!(config.mail_from, DEFAULT_MAIL_FROM);
    }

    #[test]
    fn an_empty_smtp_url_reads_as_unset() {
        // A compose file that declares the variable and leaves it blank means
        // "no server", not "a server called empty string".
        let config = Config::from_lookup(env(&[("DATABASE_URL", "postgres://localhost/lyrid"), ("LYRID_SMTP_URL", "   ")])).unwrap();
        assert!(config.smtp_url.is_none());
    }

    #[test]
    fn rejects_a_malformed_bind_address() {
        let error = Config::from_lookup(env(&[("DATABASE_URL", "postgres://localhost/lyrid"), ("LYRID_ADDR", "not-an-address")])).unwrap_err();
        assert!(error.to_string().contains("LYRID_ADDR"));
    }
}
