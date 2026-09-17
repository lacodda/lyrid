# Contributing to lyrid

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

# Add prose, reached through the multistream index (1.9 MB per article, not 27 GB):
cargo run -- import wikipedia --dump ./enwiki-multistream.xml.bz2 --index ./enwiki-index.txt.bz2

# Project the graph into a sky and cut the tile pyramid:
cargo run --release -- layout --tiles ./tiles

cd web && pnpm install && pnpm dev        # the SPA
cd docs/site && pnpm install && pnpm dev  # the documentation site
```
