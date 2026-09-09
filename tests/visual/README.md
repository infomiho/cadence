# Native visual checks

Run on macOS with Metal available and the same rendering environment as the baseline:

```sh
cargo test --test ui_rendering --locked
```

The runner renders production views twice per case and requires identical RGBA pixels. Failures write expected, actual and difference images to `target/ui-artifacts`. CI runs this target explicitly and uploads only failed cases, the reference manifest, the lockfile and OS details. Diagnostic PNGs use lossless compression and retain every pixel. Passing runs upload no visual artifact.

The 48 full-window cases cover library and queue states, search text and focus, empty and filled setup inputs, autoplay on and off, and the closed and open mascot selector. Each runs in light and dark appearances at 1280 × 820 and 900 × 820 points. Queue cases cover closed and open states with idle, hover and pointer-down input.

Eight additional cases cover the focused queue button, closed and open, in both appearances and layouts. These capture the 40-point control with four points of surrounding space. Keyboard focus adds a two-point circular border using `focus_ring`, preserving the icon, geometry and selected background. These detail references were captured locally on macOS 27, with separate provenance in the manifest. They do not replace any full-window reference.

The full-window references in `baseline` are reviewed gpui-kit 0.6.1 captures from hosted macOS 15.7.9 arm64, rendered with bundled Inter 3.19, reduced motion, active windows, a deterministic clock, local artwork placeholders and paused playback. All 48 repeat exactly and match an independent hosted run. The manifest records the source revision, capture runs, environment, lockfile hash and font hashes.

The 0.6.0 to 0.6.1 upgrade was separately verified with zero pixel differences on macOS 27. `upgrade-0.6.0` retains the initial system-font references. These use different font conditions from the CI baseline. The bundled fonts retain their [upstream license](fonts/LICENSE.txt).

CI uses macOS 15. Native rasterization can vary across OS and GPU versions, so another macOS version can fail even when the UI is unchanged. For migration parity, compare both source revisions under identical conditions. Keep exact comparison and review baseline changes explicitly.

To record references in a separate directory from an approved source revision:

```sh
CADENCE_UI_BASELINE=/tmp/cadence-reference cargo test --test ui_rendering --locked -- --record
```

To capture on the CI platform, run **Actions → Regenerate screenshots → Run workflow** on the desired branch. The artifact contains repeatable PNGs, the source revision, environment details and lockfile. It expires after 14 days.

Review the images before replacing `tests/visual/baseline/*.png`, update `manifest.json` with the recorded provenance, then commit the baseline through the usual review process. Approved images stay in the repository so CI does not depend on artifact retention. Regeneration does not prove migration parity or automatically approve a visual change.
