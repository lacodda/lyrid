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

The universe is assembled locally from open dumps and datasets — MusicBrainz, ListenBrainz, Discogs, Wikidata, Wikipedia, AcousticBrainz — with similarity and the sky layout computed by lyrid itself. Listening goes out to the services the canon already links to, and to the artist's own channel embedded in place — never through a streaming API on the critical path ([ADR 0011](https://github.com/lacodda/lyrid/blob/main/docs/adr/0011-listening-from-the-canon.md)); scrobbling connects via ListenBrainz.

## Status

Pre-alpha, and the sky is on screen. A MusicBrainz full export builds the canon (~3M artists with countries, years, release groups and URL relationships); the ListenBrainz relations dataset adds ~6M similarity edges and the brightness derived from them; Discogs, Wikidata and Wikipedia fill in genres, facts and prose. The force-directed layout places 206,636 stars, the tile pyramid cuts them into static files, and the WebGL2 renderer draws all of them at 60 FPS on integrated graphics — measured, with roughly five times that as the ceiling ([ADR 0009](https://github.com/lacodda/lyrid/blob/main/docs/adr/0009-renderer-measured.md)).

It runs on a small machine, sky and all. `lyrid slice` cuts the canon to the brightest artists, keeping 92% of the similarity graph from 3.4% of the artists ([ADR 0010](https://github.com/lacodda/lyrid/blob/main/docs/adr/0010-a-slice-for-a-small-stand.md)); the slice and the tile pyramid are then moved onto a deployed stand rather than rebuilt there. Measured on a Raspberry Pi 4 sharing the machine with other services: a 900 MB database, and the whole pyramid — 250 tiles, 6.4 MB — served in 1.6 s, after which the renderer no longer touches the network. Click a star and the card tells you what it is: the Wikipedia lead with its CC BY-SA credit, where the act formed and when, labels, who shaped them and whom they shaped, the discography, and where to go and listen — links the canon already holds, plus the artist's own YouTube channel playing in place. Nothing on the card calls a streaming API, which is why none of it can be taken down by one ([ADR 0011](https://github.com/lacodda/lyrid/blob/main/docs/adr/0011-listening-from-the-canon.md)). Any view can be sent to someone else: the address carries both the open card and the camera (`/star/54#-59.17,-69.55,12`), and the frame saves as a poster.

Accounts arrived with v0.10: an email and a password, Argon2id hashes, and sessions as rows in the database so that signing out actually ends one ([ADR 0012](https://github.com/lacodda/lyrid/blob/main/docs/adr/0012-accounts-with-a-password.md)). An account remembers the mode you chose — creative or exploration, chosen once and never changed — along with your marker and where you left the sky. Nothing is gated behind it: the map, the card and the search answer a visitor exactly as they answer an account, and a saved camera is dropped rather than trusted when the sky has been rebuilt under it. The fog of war comes next. Watch this repository.

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

## Documentation

[lacodda.github.io/lyrid](https://lacodda.github.io/lyrid) — guides, reference, and the architecture decision records.

## License

[MIT](https://github.com/lacodda/lyrid/blob/main/LICENSE)
