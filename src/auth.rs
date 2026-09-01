//! Passwords, session tokens and the cookie that carries them.
//!
//! Deliberately free of database and HTTP types: what is worth testing here
//! is the rules -- what counts as an address, what a cookie header means, how
//! a session cookie is written -- and rules tested through a request handler
//! are rules tested through everything else too.

use anyhow::{Context, Result, anyhow};
use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};

/// Name of the cookie holding the session token.
pub const COOKIE: &str = "lyrid_session";

/// How long a session lasts. Long enough that a daily instrument does not ask
/// for a password every week; short enough that an abandoned browser does not
/// stay logged in forever.
pub const SESSION_DAYS: i64 = 30;

/// The shortest password accepted. A floor, not a policy: composition rules
/// ("one digit, one symbol") push people towards `Password1!` and are worth
/// less than length.
pub const MIN_PASSWORD: usize = 10;

/// The two modes, chosen once and never changed (Vision, principle 5).
pub const MODES: [&str; 2] = ["explore", "create"];

/// Hashes a password for storage.
///
/// Argon2id with the crate's defaults, which are the OWASP-recommended
/// parameters. The salt is random per password and travels inside the PHC
/// string, so nothing else has to be stored beside it.
pub fn hash_password(password: &str) -> Result<String> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|error| anyhow!("failed to hash a password: {error}"))
}

/// Checks a password against a stored hash.
///
/// A malformed stored hash is a failure to verify, never a pass: a row
/// corrupted into nonsense must not become an account anyone can enter.
pub fn verify_password(password: &str, stored: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else {
        return false;
    };
    Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()
}

/// A fresh session token: 256 bits from the operating system's generator,
/// hex-encoded so it survives a cookie header unescaped.
pub fn session_token() -> String {
    use std::fmt::Write as _;

    let bytes: [u8; 32] = rand::random();
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(token, "{byte:02x}");
    }
    token
}

/// The `Set-Cookie` value that starts a session.
///
/// `HttpOnly` keeps the token out of reach of scripts, so an XSS hole cannot
/// read it; `SameSite=Lax` means a link from elsewhere still arrives logged
/// in while a cross-site form post does not carry the session. `Secure` is
/// conditional because the stand is plain HTTP on a home network: a cookie
/// marked `Secure` there is simply never sent, and an account nobody can log
/// into is not more secure, it is broken.
pub fn set_cookie(token: &str, secure: bool) -> String {
    let max_age = SESSION_DAYS * 24 * 60 * 60;
    let mut value = format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}");
    if secure {
        value.push_str("; Secure");
    }
    value
}

/// The `Set-Cookie` value that ends one. Same attributes as the cookie it
/// replaces -- a browser matches on name, path and flags, and a clearing
/// cookie that differs is a second cookie rather than a deletion.
pub fn clear_cookie(secure: bool) -> String {
    let mut value = format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    if secure {
        value.push_str("; Secure");
    }
    value
}

/// Pulls the session token out of a `Cookie` header.
///
/// Browsers send every cookie for the origin in one header, in no promised
/// order, and other software sets cookies on the same origin. So this looks
/// for its own name among many rather than assuming it is alone or first.
pub fn token_from_cookies(header: &str) -> Option<&str> {
    header.split(';').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name.trim() == COOKIE).then(|| value.trim()).filter(|value| !value.is_empty())
    })
}

/// Normalises an address for storage and comparison.
///
/// Case-folded because `Ada@example.com` and `ada@example.com` are one
/// mailbox, and two accounts for one person is a support problem the UNIQUE
/// constraint can prevent for free.
pub fn normalise_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// What the server accepts as an address.
///
/// Not a validator of RFC 5322 -- that grammar admits addresses no mail
/// system will deliver to, and rejecting a real address is worse than
/// accepting an undeliverable one. This checks the shape a person recognises
/// and leaves deliverability to the confirmation mail that arrives with the
/// privacy charter (v0.11).
pub fn looks_like_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && domain.contains('.')
        && !email.contains(char::is_whitespace)
        // One '@' only: "a@b@c.com" is not an address.
        && !domain.contains('@')
}

/// Why a registration was refused, in words the client shows as they are.
pub fn check_credentials(email: &str, password: &str) -> Result<()> {
    if !looks_like_email(email) {
        return Err(anyhow!("that does not look like an email address"));
    }
    // Counted in characters rather than bytes: a passphrase in Cyrillic is
    // twice the bytes of one in ASCII, and a floor that varies by alphabet is
    // not a floor.
    if password.chars().count() < MIN_PASSWORD {
        return Err(anyhow!("a password needs at least {MIN_PASSWORD} characters"));
    }
    Ok(())
}

/// Validates a mode as it arrives from a client.
pub fn check_mode(mode: &str) -> Result<()> {
    MODES.contains(&mode).then_some(()).with_context(|| format!("unknown mode: {mode}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_verifies_against_its_own_hash() {
        let hash = hash_password("a long enough passphrase").unwrap();
        assert!(verify_password("a long enough passphrase", &hash));
        assert!(!verify_password("a long enough passphras", &hash));
    }

    #[test]
    fn the_same_password_hashes_differently_every_time() {
        // A per-password salt is what stops one leaked table from revealing
        // that two accounts share a password.
        let first = hash_password("a long enough passphrase").unwrap();
        let second = hash_password("a long enough passphrase").unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn the_hash_is_never_the_password() {
        let hash = hash_password("a long enough passphrase").unwrap();
        assert!(!hash.contains("a long enough passphrase"));
        assert!(hash.starts_with("$argon2id$"));
    }

    #[test]
    fn a_corrupted_hash_does_not_open_the_account() {
        // The failure mode that matters: garbage in the column must not
        // verify against anything, least of all an empty password.
        assert!(!verify_password("", ""));
        assert!(!verify_password("anything", "not a phc string"));
        assert!(!verify_password("anything", "$argon2id$"));
    }

    #[test]
    fn tokens_do_not_repeat() {
        let first = session_token();
        assert_eq!(first.len(), 64, "256 bits, hex-encoded");
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(first, session_token());
    }

    #[test]
    fn a_session_cookie_cannot_be_read_by_a_script() {
        let cookie = set_cookie("abc", false);
        assert!(cookie.contains("HttpOnly"), "{cookie}");
        assert!(cookie.contains("SameSite=Lax"), "{cookie}");
        assert!(cookie.starts_with("lyrid_session=abc;"), "{cookie}");
        // The stand is plain HTTP: a Secure cookie there is never sent at all.
        assert!(!cookie.contains("Secure"), "{cookie}");
        assert!(set_cookie("abc", true).contains("; Secure"));
    }

    #[test]
    fn clearing_matches_the_cookie_it_replaces() {
        // A browser matches a replacement on name, path and flags. A clearing
        // cookie that differs adds a second cookie instead of removing one --
        // and the session would look ended while still being sent.
        let set = set_cookie("abc", true);
        let clear = clear_cookie(true);
        for attribute in ["Path=/", "HttpOnly", "SameSite=Lax", "Secure"] {
            assert!(set.contains(attribute) && clear.contains(attribute), "{attribute} differs: {set} / {clear}");
        }
        assert!(clear.contains("Max-Age=0"));
    }

    #[test]
    fn the_token_is_found_among_other_cookies() {
        // The header carries every cookie for the origin, in no promised
        // order, so the name has to be searched for rather than assumed first.
        assert_eq!(token_from_cookies("lyrid_session=abc"), Some("abc"));
        assert_eq!(token_from_cookies("theme=dark; lyrid_session=abc; other=1"), Some("abc"));
        assert_eq!(token_from_cookies("theme=dark;lyrid_session=abc"), Some("abc"));
        assert_eq!(token_from_cookies("theme=dark"), None);
        assert_eq!(token_from_cookies(""), None);
    }

    #[test]
    fn a_cookie_whose_name_merely_ends_in_ours_is_not_ours() {
        // "not_lyrid_session" ends with the name; a suffix match would read
        // someone else's cookie as a session.
        assert_eq!(token_from_cookies("not_lyrid_session=abc"), None);
        assert_eq!(token_from_cookies("xlyrid_session=abc; lyrid_session=real"), Some("real"));
    }

    #[test]
    fn an_empty_session_cookie_is_no_session() {
        // A cleared cookie can arrive as an empty value before the browser
        // drops it; treating that as a token would send "" to the database.
        assert_eq!(token_from_cookies("lyrid_session="), None);
        assert_eq!(token_from_cookies("lyrid_session=; other=1"), None);
    }

    #[test]
    fn one_mailbox_is_one_account() {
        assert_eq!(normalise_email("  Ada@Example.COM "), "ada@example.com");
    }

    #[test]
    fn an_address_needs_the_shape_of_one() {
        assert!(looks_like_email("ada@example.com"));
        assert!(looks_like_email("ada+lyrid@mail.example.co.uk"));
        assert!(!looks_like_email("ada"));
        assert!(!looks_like_email("ada@"));
        assert!(!looks_like_email("@example.com"));
        assert!(!looks_like_email("ada@example"), "a domain without a dot is not deliverable");
        assert!(!looks_like_email("ada@.com"));
        assert!(!looks_like_email("ada@example."));
        assert!(!looks_like_email("ada @example.com"));
        assert!(!looks_like_email("a@b@example.com"));
    }

    #[test]
    fn a_short_password_is_refused_by_characters_not_bytes() {
        // Ten Cyrillic characters are twenty bytes; a byte-counted floor
        // would quietly demand half as much of one alphabet as of another.
        // Ten Cyrillic characters: twenty bytes, so a byte count would pass
        // this and the nine-character one below it too.
        assert!(check_credentials("ada@example.com", "тайнаялира").is_ok());
        assert!(check_credentials("ada@example.com", "тайнаялир").is_err());
        assert!(check_credentials("ada@example.com", "0123456789").is_ok());
        assert!(check_credentials("ada@example.com", "012345678").is_err());
        assert!(check_credentials("ada@example.com", "short").is_err());
        assert!(check_credentials("not an address", "a long enough passphrase").is_err());
    }

    #[test]
    fn only_the_two_modes_exist() {
        assert!(check_mode("explore").is_ok());
        assert!(check_mode("create").is_ok());
        assert!(check_mode("creative").is_err());
        assert!(check_mode("").is_err());
    }
}
