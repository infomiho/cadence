<p align="center">
  <img src="assets/cadence-mark.svg" width="96" height="96" alt="Cadence logo">
</p>

<h1 align="center">Cadence</h1>

<p align="center">
  A minimal Spotify player for macOS.<br>
  Native, responsive, and typically uses around 120 MB of RAM.
</p>

<p align="center">
  <a href="https://github.com/infomiho/cadence/actions/workflows/ci.yml"><img src="https://github.com/infomiho/cadence/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-black.svg" alt="MIT license"></a>
</p>

## Features

- Search tracks and playlists
- Play music locally with queue, history, favorites, and radio
- Browse native artist and album pages
- Light, dark, and system appearances
- Persistent playback and library state

## Built With

**Rust** + **GPUI** + **librespot** + **rspotify** + **SQLite** + **SDL2**

GPUI renders the native GPU-accelerated interface, librespot handles playback,
rspotify connects to the Spotify Web API, and SQLite keeps local state.

## Run

Cadence requires macOS, Spotify Premium, Rust, and Apple's Metal Toolchain.

1. Create an app in the [Spotify Developer Dashboard](https://developer.spotify.com/dashboard).
2. Add `http://127.0.0.1:8888/callback` and `http://127.0.0.1:8898/login` as redirect URIs.
3. Launch Cadence with your client ID:

```sh
SPOTIFY_CLIENT_ID="your-client-id" cargo run
```

For a stable local signature and fewer Keychain prompts during development:

```sh
SPOTIFY_CLIENT_ID="your-client-id" ./scripts/run-signed.sh
```

## Releases

Version tags publish an optimized macOS app and SHA-256 checksum on the
[releases page](https://github.com/infomiho/cadence/releases). Current builds
use an ad-hoc signature and are not notarized, so macOS may require using
**Open** from the app's context menu on first launch.

Release builds embed the repository's `SPOTIFY_CLIENT_ID` Actions secret. The
Spotify account still needs access to that app in the Spotify Developer
Dashboard while the app remains in development mode.

Cadence stores Spotify tokens in macOS Keychain. The logo attribution is in
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).

## License

[MIT](LICENSE)
