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

Pre-alpha. The stars are in and the routes between them: a MusicBrainz full export builds the canon (~3M artists with countries, years, release groups and URL relationships), and the ListenBrainz relations dataset adds ~7M similarity edges with the brightness derived from them. The sky layout and the renderer come next. Watch this repository.

## Development

Requires Rust (see `rust-version` in `Cargo.toml`), Node LTS with pnpm, and Docker for the development database.

```sh
docker compose up -d db          # PostgreSQL on :5432
cp .env.example .env             # DATABASE_URL points at it out of the box
cargo run -- serve               # the API on :8080; /health reports the database

# Fill the universe from a MusicBrainz full export (~7 GB, downloaded once):
cargo run -- import musicbrainz --dump ./mbdump.tar.bz2

# Add the similarity graph (~117 MB, CC0):
cargo run -- import listenbrainz --dump ./artist-credit-relations.tar.bz2

# Add genres, styles and labels (~1.15 GB, CC0):
cargo run -- import discogs --masters ./discogs_masters.xml.gz --labels ./discogs_labels.xml.gz

# Add biography and influence, streamed straight from Wikidata (nothing stored):
cargo run -- import wikidata

cd web && pnpm install && pnpm dev        # the SPA
cd docs/site && pnpm install && pnpm dev  # the documentation site
```

## Documentation

[lacodda.github.io/lyrid](https://lacodda.github.io/lyrid) — guides, reference, and the architecture decision records.

## License

[MIT](https://github.com/lacodda/lyrid/blob/main/LICENSE)
