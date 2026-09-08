# Search phase wireframes

Open `index.html` in a browser, or open the final system directly:

- `final.html`: returning users, forgiving matching, refresh, first-run import and the existing signed-out blocking state; six screens.

Each phase includes a flow overview and numbered annotations. Select a wireframe to enlarge it; Escape, the Close button, or the backdrop closes the viewer. SVG controls depict product states and do not perform searches or playback. The review navigation and zoom controls are interactive.

The product canvases are 1280 × 800 desktop screens. They retain Cadence’s current chrome: the toolbar contains search; the sidebar contains Liked Songs, Favorites, Playlists and Recently played. All example music, account conditions and loading counts are fixtures. No Spotify connection is made.

The proposed search page is one stable Spotify result list. Eligible saved metadata may supply a quick preview while the catalog search runs; new Spotify rows append under a subtle “Loading more results…” continuation. The UI does not expose local and remote sources as separate sections. This does not imply offline playback: every play control represents Cadence’s normal connected Spotify playback path, and the signed-out screen blocks the workspace as the app does today.

Keep `viewer.css`, `viewer.js` and `wireframes/` alongside the HTML when sharing. No server, build step, external fonts or network dependencies are required to view the pages.

To regenerate the HTML and SVGs after changing the flow descriptions or primitives:

```sh
python3 design/search-phases/build.py
```

Viewer CSS derives from the wireframe skill's `assets/site-template.html`; SVG primitives derive from `references/components.md`. JavaScript uses native buttons, details and a modal dialog for keyboard navigation and focus restoration.

Validation: browser checks at 1440 × 900, 768 × 1024 and 390 × 844; no horizontal page overflow or broken images; collapsed mobile navigation, anchor positioning, zoom, Escape and focus return checked. SVGs parsed as XML and checked against the 8px coordinate grid and 12/14/20/28 type scale. Selected screens visually inspected in Chrome.
