mod api;
mod app;
mod auth;
mod config;
mod import;
mod layout;
mod slice;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

/// A music universe: a canonical sky of artists and genres you explore through
/// real listening.
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the HTTP server.
    Serve,
    /// Build the canonical universe from an open data dump.
    #[command(subcommand)]
    Import(ImportCommand),
    /// Project the similarity graph into sky coordinates and cut the tile
    /// pyramid. Requires the similarity import to have run first.
    Layout(layout::build::Args),
    /// Cut the canon down to the brightest artists, for a stand too small to
    /// hold all of it. Requires the layout to have run first.
    Slice(slice::Args),
}

#[derive(Subcommand)]
enum ImportCommand {
    /// Import artists, release groups and URL relationships from a full
    /// export of MusicBrainz (mbdump.tar.bz2).
    #[allow(clippy::doc_markdown, reason = "this text is user-facing --help output, not rustdoc")]
    Musicbrainz(import::musicbrainz::Args),
    /// Import the artist similarity graph from the ListenBrainz relations
    /// dataset. Requires the MusicBrainz import to have run first.
    #[allow(clippy::doc_markdown, reason = "this text is user-facing --help output, not rustdoc")]
    Listenbrainz(import::listenbrainz::Args),
    /// Import genres, styles and labels from the Discogs monthly XML dumps.
    /// Requires the MusicBrainz import to have run first: artists are joined
    /// through its `discogs` URL relationships.
    #[allow(clippy::doc_markdown, reason = "this text is user-facing --help output, not rustdoc")]
    Discogs(import::discogs::Args),
    /// Import biographical facts and influence links from the Wikidata dump.
    /// Requires the MusicBrainz import to have run first: entities are matched
    /// to the canon by their MusicBrainz id.
    #[allow(clippy::doc_markdown, reason = "this text is user-facing --help output, not rustdoc")]
    Wikidata(import::wikidata::Args),
    /// Import Wikipedia lead paragraphs, with their CC BY-SA attribution.
    /// Requires the Wikidata import to have run first: the article titles come
    /// from its sitelinks.
    #[allow(clippy::doc_markdown, reason = "this text is user-facing --help output, not rustdoc")]
    Wikipedia(import::wikipedia::Args),
}

#[tokio::main]
async fn main() -> Result<()> {
    // Loaded before the logger so RUST_LOG can live in .env too. A missing
    // file is normal: in production the environment is set by the deployment.
    match dotenvy::dotenv() {
        Ok(_) | Err(dotenvy::Error::Io(_)) => {}
        Err(error) => return Err(error).context("failed to read .env"),
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("lyrid=info,tower_http=info")))
        .init();

    let config = config::Config::from_env()?;
    let cli = Cli::parse();

    match cli.command {
        // Running the binary with no arguments serves, which is what a
        // container image or a systemd unit expects.
        None | Some(Command::Serve) => serve(&config).await,
        Some(Command::Import(ImportCommand::Musicbrainz(args))) => {
            let pool = connect(&config).await?;
            import::musicbrainz::run(&pool, &args).await
        }
        Some(Command::Import(ImportCommand::Listenbrainz(args))) => {
            let pool = connect(&config).await?;
            import::listenbrainz::run(&pool, &args).await
        }
        Some(Command::Import(ImportCommand::Discogs(args))) => {
            let pool = connect(&config).await?;
            import::discogs::run(&pool, &args).await
        }
        Some(Command::Import(ImportCommand::Wikidata(args))) => {
            let pool = connect(&config).await?;
            import::wikidata::run(&pool, &args).await
        }
        Some(Command::Import(ImportCommand::Wikipedia(args))) => {
            let pool = connect(&config).await?;
            import::wikipedia::run(&pool, &args).await
        }
        Some(Command::Slice(args)) => {
            let pool = connect(&config).await?;
            slice::run(&pool, &args).await
        }
        Some(Command::Layout(args)) => {
            let pool = connect(&config).await?;
            layout::build::run(&pool, &args).await
        }
    }
}

/// Opens the pool and brings the schema up to date. Every entry point does
/// this: an importer against an outdated schema fails in less obvious ways
/// than one that migrates first.
async fn connect(config: &config::Config) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await
        .context("failed to connect to PostgreSQL")?;
    sqlx::migrate!().run(&pool).await.context("failed to apply database migrations")?;
    Ok(pool)
}

async fn serve(config: &config::Config) -> Result<()> {
    let pool = connect(config).await?;

    let listener = TcpListener::bind(config.addr)
        .await
        .with_context(|| format!("failed to bind {}", config.addr))?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), addr = %config.addr, "lyrid listening");

    axum::serve(listener, app::router(pool, config.secure_cookie, config.static_dir.as_deref()))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;
    Ok(())
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "failed to install the shutdown signal handler");
        return;
    }
    tracing::info!("shutting down");
}
