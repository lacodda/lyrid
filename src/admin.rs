//! Who may read the stand's admin page.
//!
//! Granted and taken away from the server's command line and nowhere else:
//! there is no route that makes an admin, so holding a shell on the stand is
//! what it takes to become one. On a stand that is
//! `docker compose exec server lyrid admin grant someone@example.com`.

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use sqlx::PgPool;

use crate::auth;

#[derive(Subcommand)]
pub enum Command {
    /// Let an account read the admin page.
    Grant {
        /// The account's address.
        email: String,
    },
    /// Take that back.
    Revoke {
        /// The account's address.
        email: String,
    },
}

pub async fn run(pool: &PgPool, command: &Command) -> Result<()> {
    let (email, admin) = match command {
        Command::Grant { email } => (email, true),
        Command::Revoke { email } => (email, false),
    };
    // Addresses are stored folded, so the one typed here is folded the same
    // way before it is looked for.
    let email = auth::normalise_email(email);
    let changed = sqlx::query("UPDATE app_user SET admin = $2 WHERE email = $1")
        .bind(&email)
        .bind(admin)
        .execute(pool)
        .await
        .context("failed to change the account")?
        .rows_affected();
    if changed == 0 {
        bail!("no account is registered with {email}");
    }
    if admin {
        tracing::info!(%email, "the account may read the admin page");
    } else {
        tracing::info!(%email, "the account may no longer read the admin page");
    }
    Ok(())
}
