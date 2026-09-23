<p align="center">
  <img src="https://raw.githubusercontent.com/lacodda/lyrid/main/assets/banner.svg" alt="lyrid — a music universe" width="720">
</p>

> A music universe: a canonical sky of artists and genres you explore through real listening.

<p align="center">
  <a href="https://github.com/lacodda/lyrid/releases/latest"><img src="https://img.shields.io/github/v/release/lacodda/lyrid?style=flat-square" alt="Release"></a>
  <a href="https://github.com/lacodda/lyrid/actions"><img src="https://img.shields.io/github/actions/workflow/status/lacodda/lyrid/ci.yml?style=flat-square" alt="CI"></a>
  <a href="https://github.com/lacodda/lyrid/blob/main/LICENSE"><img src="https://img.shields.io/github/license/lacodda/lyrid?style=flat-square" alt="License"></a>
</p>

**lyrid** turns the music world into a night sky. Every artist is a star, every genre a nebula, similarity forms the routes between them — one canonical map for everyone, with a personal fog of war over it. You light up your own sky by actually listening: previews are scans, full listens are landings, and your scrobbles fuel the journey.

Named after the Lyrids — the meteor shower radiating from Lyra, the lyre of Orpheus: music falling from the sky as stars.

## Two modes

- **Exploration** — fog of war, a starship with fuel, light as currency, quests and hidden treasures. Chosen once, played for years.
- **Creative** — the whole sky open from the first minute: a daily instrument for studying music.

Only the creative mode is built today, so it is the one every account starts in. The choice between them arrives with the fog, when there is something to choose between.

## Built on open data

The universe is assembled locally from open dumps and datasets — MusicBrainz, ListenBrainz, Discogs, Wikidata, Wikipedia, AcousticBrainz — with similarity and the sky layout computed by lyrid itself. Listening goes out to the services the canon already links to, and to the artist's own channel embedded in place — never through a streaming API on the critical path ([ADR 0011](https://github.com/lacodda/lyrid/blob/main/docs/adr/0011-listening-from-the-canon.md)); scrobbling connects via ListenBrainz.

## Status

Pre-alpha, and the sky is on screen: 206,636 artists laid out and rendered in WebGL2 at 60 FPS, running on a Raspberry Pi with accounts, email verification, password recovery and a pressable privacy charter (export and delete, both immediate). The interface is built from the line's design system, speaks English and Russian, and the stars in view are listed as text beside the canvas — so the map is reachable by keyboard and by a screen reader. The observer's instruments are in: an era lens and a time machine, genre names on the sky, why each neighbour is one, a side-by-side comparison of any two stars, and routes you can send as a link. Only the creative mode is built; the fog of war comes next. See the [CHANGELOG](https://github.com/lacodda/lyrid/blob/main/CHANGELOG.md) for what landed in each version, and [How lyrid is put together](https://lacodda.github.io/lyrid/guides/architecture/) for the architecture.

## Documentation

[lacodda.github.io/lyrid](https://lacodda.github.io/lyrid) — guides, reference, and the architecture decision records.

## License

MIT (c) [Kirill Lakhtachev](https://lacodda.com)
