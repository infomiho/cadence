# Scrubber prototype

`PLAN.md` holds the diagnosis and the proposal. The prototype is the argument for it.

```sh
python3 -m http.server 8731 --directory design/scrubber
```

Then open `http://localhost:8731/index.html`. No build step or network access.

Four bars share one simulated backend that behaves like librespot: position is
reported every 250 ms and seeks apply only after a round trip.

- **A** reproduces the bar as it ships today
- **B** is Spotify's approach, committing the seek on release
- **C** is YouTube's, growing on hover and seeking continuously
- **D** is the proposal

Two toggles at the top make the defects reproducible: a slow backend exposes the
snap-back, and the fullscreen switch applies the 224 px origin error to A.

Measured geometry and timing for B and C come from YouTube's and Spotify's live
stylesheets; sources are cited in `PLAN.md`.
