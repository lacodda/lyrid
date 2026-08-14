<p align="center">
  <img src="https://raw.githubusercontent.com/lacodda/lyrid/main/assets/banner.svg" alt="lyrid — a music universe" width="720">
</p>

# lyrid

> A music universe: a canonical sky of artists and genres you explore through real listening.

**lyrid** turns the music world into a night sky. Every artist is a star, every genre a nebula, similarity forms the routes between them — one canonical map for everyone, with a personal fog of war over it. You light up your own sky by actually listening: previews are scans, full listens are landings, and your scrobbles fuel the journey.

Named after the Lyrids — the meteor shower radiating from Lyra, the lyre of Orpheus: music falling from the sky as stars.

## Two modes

- **Exploration** — fog of war, a starship with fuel, light as currency, quests and hidden treasures. Chosen once, played for years.
- **Creative** — the whole sky open from the first minute: a daily instrument for studying music.

## Built on open data

The universe is assembled locally from open dumps and datasets — MusicBrainz, ListenBrainz, Discogs, Wikidata, Wikipedia, AcousticBrainz — with similarity and the sky layout computed by lyrid itself. Listening goes through official previews and embeds; scrobbling connects via ListenBrainz.

## Status

Pre-alpha. The foundation is in place: an axum server with a database-backed `/health`, a React SPA shell, a documentation site, and release rails. The universe itself comes next — the MusicBrainz import, similarity, and the sky layout. Watch this repository.

## Development

Requires Rust (see `rust-version` in `Cargo.toml`), Node LTS with pnpm, and Docker for the development database.

```sh
docker compose up -d db          # PostgreSQL on :5432
cp .env.example .env             # DATABASE_URL points at it out of the box
cargo run                        # the API on :8080; /health reports the database

cd web && pnpm install && pnpm dev        # the SPA
cd docs/site && pnpm install && pnpm dev  # the documentation site
```

## Documentation

[lacodda.github.io/lyrid](https://lacodda.github.io/lyrid) — guides, reference, and the architecture decision records.

## License

[MIT](https://github.com/lacodda/lyrid/blob/main/LICENSE)
