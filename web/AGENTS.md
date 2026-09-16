Landing page and release notes for Cadence, an axum app. Read the root
`AGENTS.md` first.

## Architecture

The site is **prerendered**: pages are rendered from the GitHub releases API at
boot, held in memory, and refreshed every 30 minutes.

Cache in three tiers:

- Assets carry a content **fingerprint** (`?v=` hashed over every embedded
  asset) and are served `immutable, max-age=1y`.
- HTML is served `max-age=300`.
- `/download` is `no-store` and 302s to the latest DMG. Point the home download
  button at it, so a cached page never offers an old build.

Set `SITE_URL` to the canonical origin; meta tags use it.

## Design

The **palette** is the product's, not a generic Apple one. Copy tokens from
`CadencePalette` in `src/app/mod.rs`, one `light-dark()` per token, with
`color-scheme: light dark` on `:root`. Draw any new color from the palette.

System font stack. Reach for modern CSS before adding markup: `light-dark()`,
`clamp()`, logical properties, `text-wrap`, `:focus-visible`,
`@media (hover: hover)`, `prefers-reduced-motion`.

Body headings are demoted one level so they nest under the release title: one
`h1`, release titles `h2`, body `h3` and deeper. Decorative images take empty
`alt`.

## Assets are derived

Embed assets at compile time (`include_str!`, `include_bytes!`). Generate art
from its source of truth:

- `static/mascot.svg` from `./og/generate-mascot.py`, which reads
  `src/app/player_mascot.rs`.
- `static/og.png` from `./og/render-og.sh`, which captures `og/card.html`.

Regenerate after changing the source.

## Code

- Handlers return a response for every input: a bad redirect target falls back
  to `releases/latest` rather than panicking.
- Release bodies are maintainer-authored, so raw HTML passes through.
- Upstream URLs live in `github.rs` as the single source of truth.

## Deploy

`web/Dockerfile`, built from the repository root so the image can embed
`assets/`. Pin the builder and runtime to the same Debian suite to keep glibc
aligned, and compile dependencies against a stub before copying `src/` so
source edits do not rebuild them.

Coolify: Base Directory `/`, Dockerfile `/web/Dockerfile`, port 3000, health
check `/healthz`. A `v*` tag runs the release workflow, which calls the Coolify
webhook (`COOLIFY_WEBHOOK`, `COOLIFY_TOKEN`) to rebuild the site.

## Validate

```sh
cargo fmt --manifest-path web/Cargo.toml -- --check
cargo clippy --manifest-path web/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path web/Cargo.toml
```
