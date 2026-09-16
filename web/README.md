# cadence-web

Landing page and release notes for Cadence.

A small server renders the pages once at boot from the GitHub releases API,
keeps them in memory, and refreshes them every 30 minutes. HTML is served with
a short cache lifetime, the embedded assets are fingerprinted so they can be
cached for a year, and `/download` redirects to the latest DMG.

## Run locally

```sh
cargo run
```

Serves on `http://localhost:3000`.

| Variable | Default | Purpose |
| --- | --- | --- |
| `PORT` | `3000` | Listen port |
| `GITHUB_REPO` | `infomiho/cadence` | Repository to read releases from |
| `GITHUB_TOKEN` | unset | Raises the GitHub API rate limit |
| `SITE_URL` | `https://cadence.miho.dev` | Absolute URLs in meta tags |

## Generated assets

- `static/mascot.svg` comes from the app's pixel map. Regenerate it with
  `./web/og/generate-mascot.py` whenever `src/app/player_mascot.rs` changes.
- `static/og.png` is a 1200x630 capture of `og/card.html`. Regenerate it with
  `./web/og/render-og.sh` after editing the card. Set `CHROME` if Chrome is not
  at the default macOS path.

## Deploy

Built from `web/Dockerfile` with the repository root as the build context, so
the site can embed the shared `assets/` artwork. Coolify injects `PORT`.
