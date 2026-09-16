//! Sending the two letters this service sends.
//!
//! There are exactly two: "confirm this address" and "here is a way back into
//! your account". Both are short, both carry one link, and both are plain
//! text — a service that sends you a picture of a button when it means to
//! send you a link is a service whose mail lands in spam.
//!
//! **A stand with no mail server logs instead of sending.** That is not a test
//! double bolted on: the home stand has no route to the internet, and a mailer
//! that fails there would make every registration on it look broken. Leaving
//! `LYRID_SMTP_URL` unset is a supported configuration, and the letter goes to
//! the log where the owner can read the link out of it and click it. Which
//! also means the log is a place secrets appear, and the one deployment where
//! that is true is the one nobody else can read.

use anyhow::{Context, Result};
use lettre::message::header::ContentType;
use lettre::transport::smtp::AsyncSmtpTransport;
use lettre::{AsyncTransport, Message, Tokio1Executor};

/// How a letter leaves the building.
#[derive(Clone)]
pub enum Mailer {
    /// A real SMTP server, from `LYRID_SMTP_URL`.
    Smtp {
        transport: Box<AsyncSmtpTransport<Tokio1Executor>>,
        from: String,
    },
    /// No server configured: the letter is written to the log instead.
    ///
    /// The address it would have gone to and the link it carries are both in
    /// there, because the point is that someone reading the log can finish
    /// what the letter started.
    Log,
}

impl std::fmt::Debug for Mailer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Hand-written because the SMTP transport holds credentials and a
        // derived Debug would put them in any log line that formats the
        // config -- which `Config` does on startup.
        match self {
            Self::Smtp { from, .. } => f.debug_struct("Smtp").field("from", from).finish_non_exhaustive(),
            Self::Log => f.write_str("Log"),
        }
    }
}

impl Mailer {
    /// Builds a mailer from an SMTP URL, or the logging one when there is none.
    ///
    /// The URL carries the credentials (`smtps://user:pass@host:465`), which is
    /// what lets the whole configuration be one environment variable rather
    /// than five that can disagree with each other.
    pub fn from_url(url: Option<&str>, from: String) -> Result<Self> {
        let Some(url) = url else {
            return Ok(Self::Log);
        };
        let transport = AsyncSmtpTransport::<Tokio1Executor>::from_url(url)
            // The URL is not quoted into the error: it holds the password.
            .context("LYRID_SMTP_URL is not a usable SMTP URL (expected e.g. smtps://user:password@smtp.example.com:465)")?
            .build();
        Ok(Self::Smtp {
            transport: Box::new(transport),
            from,
        })
    }

    /// Sends one letter, or logs it.
    ///
    /// Errors are returned rather than swallowed so the caller can decide —
    /// and the callers here deliberately decide differently: a failed
    /// confirmation letter must not undo a registration, while a failed reset
    /// letter is the whole of what was asked for.
    pub async fn send(&self, to: &str, subject: &str, body: &str) -> Result<()> {
        match self {
            Self::Log => {
                tracing::info!(to, subject, "no SMTP server configured, so the letter was logged instead of sent:\n\n{body}\n");
                Ok(())
            }
            Self::Smtp { transport, from } => {
                let message = Message::builder()
                    .from(from.parse().with_context(|| format!("LYRID_MAIL_FROM is not a usable address: {from}"))?)
                    .to(to.parse().with_context(|| format!("not a usable recipient address: {to}"))?)
                    .subject(subject)
                    .header(ContentType::TEXT_PLAIN)
                    .body(body.to_string())
                    .context("the letter could not be built")?;
                transport.send(message).await.context("the letter could not be sent")?;
                Ok(())
            }
        }
    }
}

/// The letter asking someone to confirm the address they signed up with.
pub fn confirmation(link: &str) -> (&'static str, String) {
    (
        "Confirm your address for lyrid",
        format!(
            "Welcome to lyrid.

Confirm this address so we can send you a way back in if you ever lose your password:

{link}

The link works once, and for a day. If you did not sign up for lyrid, nothing
happens if you ignore this — the account was made with this address, but it
cannot be used to reach you until you click.

lyrid — a music universe"
        ),
    )
}

/// The letter carrying a way back into an account.
pub fn reset(link: &str) -> (&'static str, String) {
    (
        "Reset your lyrid password",
        format!(
            "Someone asked to reset the password for the lyrid account at this address.

If it was you, here is the way back in:

{link}

The link works once, and for an hour. If it was not you, ignore this letter:
your password has not changed, and whoever asked cannot see this message.

lyrid — a music universe"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_smtp_url_means_the_log() {
        let mailer = Mailer::from_url(None, "lyrid@example.com".to_string()).unwrap();
        assert!(matches!(mailer, Mailer::Log), "an unset URL must be a supported configuration, not an error");
    }

    #[test]
    fn a_malformed_smtp_url_is_refused_at_startup() {
        // Loudly, and at boot: a mailer that only fails on the first
        // registration fails in front of a user rather than in front of
        // whoever deployed it.
        let error = Mailer::from_url(Some("not a url"), "lyrid@example.com".to_string()).unwrap_err();
        assert!(error.to_string().contains("LYRID_SMTP_URL"));
    }

    #[test]
    fn the_smtp_url_never_reaches_the_error_message() {
        // The URL holds the password. An error quoting it would put the
        // password into the log of every server that failed to start.
        let error = Mailer::from_url(Some("smtp://ada:hunter2@"), "lyrid@example.com".to_string()).unwrap_err();
        let text = format!("{error:?}");
        assert!(!text.contains("hunter2"), "the SMTP password leaked into an error: {text}");
    }

    #[test]
    fn debug_does_not_print_the_transport() {
        // `Config` is logged on startup and holds this; a derived Debug would
        // print the credentials with it.
        let mailer = Mailer::from_url(Some("smtps://ada:hunter2@smtp.example.com:465"), "lyrid@example.com".to_string()).unwrap();
        let text = format!("{mailer:?}");
        assert!(!text.contains("hunter2"), "the SMTP password leaked into Debug: {text}");
        assert!(text.contains("lyrid@example.com"), "the sender is not a secret and is worth seeing: {text}");
    }

    #[test]
    fn both_letters_carry_their_link_and_say_how_long_it_lasts() {
        // A one-time link that does not say it is one-time produces a support
        // question the second time it is clicked.
        for (_, body) in [
            confirmation("https://lyrid.example/confirm?token=abc"),
            reset("https://lyrid.example/reset?token=abc"),
        ] {
            assert!(body.contains("token=abc"), "{body}");
            assert!(body.contains("works once"), "{body}");
        }
    }

    #[test]
    fn the_reset_letter_reassures_whoever_did_not_ask() {
        // Anyone can type someone else's address into the form, so this
        // letter reaches people who did not ask for it. It has to tell them
        // nothing has happened.
        let (_, body) = reset("https://lyrid.example/reset?token=abc");
        assert!(body.contains("your password has not changed"), "{body}");
    }
}
