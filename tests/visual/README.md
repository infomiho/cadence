# Native visual checks

Run on macOS with Metal available:

```sh
cargo test --test ui_rendering --locked
```

The runner renders production views twice per case and requires identical RGBA pixels. Failures write expected, actual and difference images to `target/ui-artifacts`. CI runs this target explicitly and uploads failure images, references, the lockfile and OS details.

The 48 cases cover library and queue states, search text and focus, empty and filled setup inputs, autoplay on and off, and the closed and open mascot selector. Each runs in light and dark appearances at 1280 × 820 and 900 × 820 points. Queue cases cover closed and open states with idle, hover and pointer-down input.

`baseline` contains the original gpui-kit 0.6.0 UI rendered with bundled Inter 3.19, reduced motion, active windows, a deterministic clock, local artwork placeholders and paused playback. `upgrade-0.6.0` retains the initial system-font captures used to verify the dependency bump. Font pinning changes variable-font weight resolution, so these reference sets use distinct rendering conditions. Both were captured from the original UI. Their manifests record the environment and font hashes.

Capture used Rust 1.97.1 and gpui-pre 0.3.4 on macOS 27.0 (26A5425a), with Metal at 2× scale. The historical kit manifest forwarded `test-support` to its platform dependency to expose the renderer. The bundled fonts retain their [upstream license](fonts/LICENSE.txt).

CI uses macOS 15. Native rasterization can vary across OS and GPU versions. If output differs, compare both source revisions under identical conditions. Do not accept a tolerance or replace references with the changed app's output.

To record references in a separate directory from an approved source revision:

```sh
CADENCE_UI_BASELINE=/tmp/cadence-reference cargo test --test ui_rendering --locked -- --record
```
