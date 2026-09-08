# GitHub Actions for a Rust macOS desktop app

Historical assessment before the cache changes. See [Rust CI caching for Cadence](rust-ci-caching.md) for the implemented cache strategy.

Researched 2026-09-08. Question: what is the best GitHub Actions setup for Cadence, a single-crate GPUI app built and released on `macos-15` runners, and which concrete changes make CI and releases faster, cheaper and safer? Sources are the GitHub Docs (caching, billing, runners, security, secrets, environments, rulesets), the READMEs and TypeScript sources of Swatinem/rust-cache, actions/cache, actions/checkout, dtolnay/rust-toolchain and Mozilla-Actions/sccache-action, the sccache docs, the Cargo and rustc references, the rustc performance book, the rustup book, the nextest docs, Apple's Xcode 15 release notes, and the run logs and cache listing of the `infomiho/cadence` repository read with `gh` on 2026-09-08.

## Summary

- Every release build so far has been cold. rust-cache keys the cache by GitHub job id, so the `release` job looks for `v0-rust-release-...` keys while `main` only ever stores `v0-rust-check-...` keys, and GitHub never lets a tag run restore a cache saved by another tag. The v0.5.0 run logged "No cache found." and then spent 43 seconds saving a 981 MB cache that no future run can read. [Section 1](#1-what-the-current-workflows-do-and-what-the-runs-show), [Section 2.4](#24-github-cache-scoping-limits-and-eviction)
- The repository is over its cache quota: 6 caches, 10.57 GB against a 10 GB limit, of which 3.9 GB are pull-request caches that only re-runs of the same pull request can use. GitHub now evicts by last access, which will start deleting the caches `main` depends on. [Section 1](#1-what-the-current-workflows-do-and-what-the-runs-show), [Section 2.4](#24-github-cache-scoping-limits-and-eviction)
- rust-cache hashes every installed toolchain into the key, including the runner image's preinstalled Rust 1.98.0, so the next image update that bumps Rust invalidates all caches even though Cadence pins 1.97.1. Uninstalling the image's `stable` toolchain before the cache step removes that dependency. [Section 2.1](#21-how-rust-cache-builds-its-key)
- The fix for cold releases is a `shared-key: release` cache seeded on `main` by a release-profile build, restored on tags with `save-if: false`. Caches on the default branch are visible to every ref. Only dependency artifacts are cached, so the seed only needs to rerun when `Cargo.lock`, `Cargo.toml` or the toolchain change, plus weekly to dodge the 7-day eviction. [Section 2.5](#25-seeding-the-release-cache)
- A warm CI run is 2.4 minutes, of which 82 seconds is downloading the 2.1 GB cache; a cold run is 16.9 minutes. `cargo clippy` and `cargo test` do not share dependency artifacts (check metadata versus full rlibs), which is a known Cargo limitation, so the cold run compiles the dependency graph twice. [Section 1](#1-what-the-current-workflows-do-and-what-the-runs-show), [Section 3.5](#35-clippy-test-and-artifact-sharing)
- `lto = "thin"` and `codegen-units = 1` are reasonable for a shipped desktop binary; the expensive part is unavoidable per release, and the dependency compile they sit on top of is what caching removes. `debug = "line-tables-only"` with Cargo's macOS default `split-debuginfo = "unpacked"` leaves the line tables in object files that never reach the DMG, so it currently costs compile time without giving shipped backtraces line numbers. [Section 3.1](#31-release-profile-settings)
- sccache, alternative linkers, `-Zthreads` and nextest each give little here: sccache uses the same branch-scoped GitHub cache and cannot cache the final link; lld and mold are Linux linkers and the perf book says the macOS system linker is fast enough; `-Zthreads` is nightly only; nextest's gains come from many test binaries and Cadence has one. [Section 2.6](#26-sccache), [Section 3.4](#34-linkers-on-macos), [Section 3.6](#36-nextest)
- `macos-latest` now means macOS 26 on Apple silicon, and `macos-15` is a 3-core M1 with 7 GB RAM. Standard runner minutes are free for this public repository; larger runners are always billed and cannot use included minutes. [Section 4.4](#44-runners-and-cost)
- Security gaps in the release workflow: actions pinned to mutable tags, no `timeout-minutes` (default 360 minutes of macOS time if notarization hangs), the `contents: write` token persisted in the checkout, the keychain created inside the workspace rather than `$RUNNER_TEMP`, and no environment or tag ruleset gating who can trigger a signed release. Secret handling itself matches GitHub's Apple certificate guide. [Section 5](#5-security)

## 1. What the current workflows do and what the runs show

`.github/workflows/ci.yml` runs one `check` job on `macos-15` for pushes to `main` and pull requests: `actions/checkout@v7`, `Swatinem/rust-cache@v2` with defaults, then `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings` and `cargo test --locked`. It sets `permissions: contents: read` and a `ci-${{ github.ref }}` concurrency group with `cancel-in-progress: true`.

`.github/workflows/release.yml` runs one `release` job on `v*` tags with `permissions: contents: write`: checkout, rust-cache with defaults, a version check, certificate import into a temporary keychain, `cargo build --release --locked` plus `scripts/package-app.sh`, `scripts/notarize.sh`, `gh release create`, and keychain removal under `if: always()`.

`Cargo.toml` sets `[profile.release] lto = "thin"`, `codegen-units = 1`, `debug = "line-tables-only"` and `[profile.dev.package."*"] opt-level = 2`. `rust-toolchain.toml` pins `channel = "1.97.1"` with `profile = "minimal"` and the `rustfmt` and `clippy` components. `Cargo.lock` lists 1048 packages, none from git sources. The crate has 93 `#[test]` functions in 20 modules and no `tests/` directory, so there is a single test binary.

Step timings from `gh run view` (seconds):

| Run | Kind | rust-cache restore | fmt | clippy | test | release build | save | Total |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 34219185911 (main, Cargo.lock changed) | CI cold | 66 | 2 | 283 | 570 | | 56 | 16.9 min |
| 34238482270 (main) | CI warm | 82 (2068 MB, full match) | 1 | 18 | 25 | | 1 ("Cache up-to-date") | 2.4 min |
| 33867982702 (v0.4.3) | release | 11 (no cache) | | | | 894 | 54 | 16.4 min |
| 34238486907 (v0.5.0) | release | 9 ("No cache found.") | | | | 486 | 43 | 9.7 min |

The v0.5.0 log shows why the release cache missed. rust-cache printed its restore key as `v0-rust-release-Darwin-arm64-d107d33f` and its cache key as `v0-rust-release-Darwin-arm64-d107d33f-85d583e6`; the only caches on `main` at that moment were `v0-rust-check-Darwin-arm64-d107d33f-85d583e6` and an older `...-818ff775`. The prefix `v0-rust-release-` matches nothing on `main`, and the v0.4.3 cache with the `release` prefix lives on `refs/tags/v0.4.3`, which a v0.5.0 run may not read (section 2.4).

Cache listing after the v0.5.0 release (`gh cache list`, `gh api repos/infomiho/cadence/actions/cache/usage`: 6 caches, 10,566,228,136 bytes):

| Size | Ref | Key | Created | Last accessed |
| --- | --- | --- | --- | --- |
| 2169 MB | refs/heads/main | v0-rust-check-Darwin-arm64-d107d33f-85d583e6 | 09-08 11:26 | 09-08 14:40 |
| 981 MB | refs/tags/v0.5.0 | v0-rust-release-Darwin-arm64-d107d33f-85d583e6 | 09-08 14:38 | 09-08 14:38 |
| 2170 MB | refs/pull/5/merge | v0-rust-check-Darwin-arm64-d107d33f-8e3a5792 | 09-08 11:25 | 09-08 14:31 |
| 1985 MB | refs/heads/main | v0-rust-check-Darwin-arm64-d107d33f-818ff775 | 09-04 11:53 | 09-08 11:11 |
| 1747 MB | refs/pull/11/merge | v0-rust-check-Darwin-arm64-d107d33f-57390e57 | 09-04 11:47 | 09-04 11:47 |
| 1514 MB | refs/tags/v0.4.3 | v0-rust-release-Darwin-arm64-d107d33f-818ff775 | 09-04 11:43 | 09-04 11:43 |

The `d107d33f` segment is the environment hash. The v0.5.0 log lists what went into it: "Rust Versions: 1.97.1 aarch64-apple-darwin 8bab26f4..., 1.98.0 aarch64-apple-darwin 88d9e12a..." and the `CARGO_INCREMENTAL` variable. The 1.98.0 entry is the `stable` toolchain preinstalled on the image ("Rust 1.98.0", "Rustup 1.29.0" in the [macos-15-arm64 image README](https://raw.githubusercontent.com/actions/runner-images/main/images/macos/macos-15-arm64-Readme.md), image version 20260829.0321.1, default Xcode 16.4).

## 2. Caching

### 2.1 How rust-cache builds its key

From [src/config.ts](https://raw.githubusercontent.com/Swatinem/rust-cache/master/src/config.ts): the key starts with `prefix-key` (default `v0-rust`); then either the `shared-key` input or the `key` input followed by `$GITHUB_JOB` when `add-job-id-key` is `true` (lines 70-90); then the runner OS and architecture; then, when `add-rust-environment-hash-key` is `true`, a hash of the Rust versions and environment. `getRustVersions` runs `rustc -vV` and `rustup toolchain list --quiet`, then `rustup run <toolchain> rustc -vV` for every listed toolchain and sorts the results (lines 390-415), which is why the image's 1.98.0 toolchain appears in the hash. Environment variables whose names start with `CARGO`, `CC`, `CFLAGS`, `CXX`, `CMAKE` or `RUST` are hashed too, plus any prefixes given in `env-vars` (lines 110-112). Everything up to this point is the `restoreKey` (line 133). The final `cacheKey` appends a hash of `.cargo/config.toml`, `rust-toolchain`, `rust-toolchain.toml`, every workspace `Cargo.toml` with package versions normalised to `0.0.0` and path dependencies stripped, and `Cargo.lock` filtered to external packages (lines 154-263). A version bump in `Cargo.toml` therefore does not change the key; a dependency change does.

The README states the consequence: "The action will try to restore from a previous `Cargo.lock` version as well, so lockfile updates should only re-build changed dependencies." [README](https://raw.githubusercontent.com/Swatinem/rust-cache/master/README.md). In [src/restore.ts](https://raw.githubusercontent.com/Swatinem/rust-cache/master/src/restore.ts) a partial match logs `full match: false`, pre-cleans the target directory with the timestamp check, and sets the `cache-hit` output to `false` (lines 45-62). The action also exports `CARGO_INCREMENTAL=0` at restore time (line 30).

### 2.2 What it caches and what it throws away

Cached paths are `$CARGO_HOME/registry` and `$CARGO_HOME/git`, plus `$CARGO_HOME/bin`, `.crates.toml` and `.crates2.json` when `cache-bin` is `true`, plus each workspace's `target` directory when `cache-targets` is `true` (config.ts lines 265-281). The README: "This action currently caches the following files/directories: `~/.cargo` (installed binaries, the cargo registry, cache, and git dependencies), `./target` (build artifacts of dependencies)." Before saving it removes "Any files in `~/.cargo/bin` that were present before the action ran (for example `rustc`). Dependencies that are no longer used. Anything that is not a dependency. Incremental build artifacts. Any build artifacts with an `mtime` older than one week." and "the workspace crates themselves are not cached since doing so is generally not effective. For this reason, this action automatically sets `CARGO_INCREMENTAL=0`". [README](https://raw.githubusercontent.com/Swatinem/rust-cache/master/README.md).

[src/cleanup.ts](https://raw.githubusercontent.com/Swatinem/rust-cache/master/src/cleanup.ts) walks the target directory and treats any directory containing `build`, `.fingerprint` or `deps` as a profile directory (lines 19-30), so `target/debug` and `target/release` are cleaned individually and both end up in the same archive; there is no separate cache per profile. Inside a profile directory only `build`, `.fingerprint` and `deps` survive (line 62), and inside those only entries belonging to current dependencies (lines 66-79). The one-week rule is `ONE_WEEK = 7 * 24 * 3600 * 1000` applied to `mtime` (lines 241-266). [src/save.ts](https://raw.githubusercontent.com/Swatinem/rust-cache/master/src/save.ts) skips saving with "Cache up-to-date." when the restored key equals the computed key (lines 24-28), which matches the 1-second post step in the warm run.

### 2.3 The inputs that matter

From [action.yml](https://raw.githubusercontent.com/Swatinem/rust-cache/master/action.yml) and the README:

- `shared-key`: "A cache key that is used instead of the automatic `job`-based key, and is stable over multiple jobs."
- `key`: "An additional cache key that is added alongside the automatic `job`-based cache key and can be used to further differentiate jobs."
- `add-job-id-key` (default `true`) and `add-rust-environment-hash-key` (default `true`) toggle the two automatic parts.
- `save-if` (default `true`): "Determines whether the cache should be saved. If `false`, the cache is only restored. Useful for jobs where the matrix is additive e.g. additional Cargo features, or when only runs from `master` should be saved to the cache." The README's example is `save-if: ${{ github.ref == 'refs/heads/master' }}`.
- `cache-on-failure` (default `false`): "Cache even if the build fails."
- `cache-all-crates` (default `false`): "If `true` all crates will be cached, otherwise only dependent crates."
- `cache-workspace-crates` (default `false`): "If `true` the workspace crates will be cached."
- `workspaces`: "Paths to multiple Cargo workspaces and their target directories, separated by newlines", in the form `. -> target`.
- `cache-targets` (default `true`): "Determines whether workspace targets are cached."
- `lookup-only` (default `false`): "Check if a cache entry exists without downloading the cache", exposing the `cache-hit` output.
- `cache-hit` output: "A boolean value that indicates an exact match was found."

Effectiveness, per the README: "This action only caches the _dependencies_ of a crate, so it is more effective if the dependency / own code ratio is higher" and "Usage with Stable Rust is the most effective, as a cache is tied to the Rust version." Cadence is one crate over 1047 dependencies on a pinned stable, the best case.

### 2.4 GitHub cache scoping, limits and eviction

[Dependency caching reference](https://docs.github.com/en/actions/reference/dependency-caching-reference): "By default, the limit is 10 GB per repository, but this limit can be increased by enterprise owners, organization owners, or repository administrators." "GitHub will remove any cache entries that have not been accessed in over 7 days." Over quota, "the cache eviction policy will create space by deleting the caches in order of last access date, from oldest to most recent." Access rules: "Workflow runs can restore caches created in either the current branch or the default branch (usually `main`)." "If a workflow run is triggered for a pull request, it can also restore caches created in the base branch, including base branches of forked repositories." "Workflow runs cannot restore caches created for child branches or sibling branches." "Workflow runs also cannot restore caches created for different tag names." "When a cache is created by a workflow run triggered on a pull request, the cache is created for the merge ref (`refs/pull/.../merge`). Because of this, the cache will have a limited scope and can only be restored by re-runs of the pull request." Restore keys: "When a key doesn't match directly, the action searches for keys prefixed with the restore key. If there are multiple partial matches for a restore key, the action returns the most recently created cache." And "You cannot change the contents of an existing cache. Instead, you can create a new cache with a new key."

Two consequences for Cadence. First, a tag run can only read caches saved on that same tag or on `main`, so the only way to warm a release is to save a release-profile cache on `main`. Second, the two pull-request caches (3.9 GB) are dead weight once the pull request merges, and the [manage caches guide](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manage-caches) warns that when "caches limited to a specific branch are using a lot of storage quota, it may cause caches from the `default` branch to be created and deleted at a high frequency"; it offers a `gh cache list --ref $BRANCH` plus `gh cache delete` workflow on pull request close as the remedy. Not saving pull-request caches at all (`save-if`) avoids the problem at the source.

Plain [actions/cache](https://raw.githubusercontent.com/actions/cache/main/README.md) exposes the same primitives (`key`, `path`, `restore-keys`, `lookup-only`, `fail-on-cache-miss`, separate `actions/cache/restore` and `actions/cache/save`) but none of the Cargo-specific cleanup, so a hand-rolled cache of `target` grows with every stale artifact. rust-cache is the right layer for this repository.

### 2.5 Seeding the release cache

Combining the facts above: use `shared-key: release` in both the seed job and the release job so the key no longer embeds the job id; run the seed on `main` so the cache lands on the default branch; give the release job `save-if: false` so it stops writing unreadable tag-scoped caches. Because rust-cache never caches the workspace crate, the seed's output depends only on `Cargo.lock`, `Cargo.toml`, `rust-toolchain.toml`, `.cargo/config.toml` and the `CARGO*`/`RUST*` environment, so a `paths` filter on those files is sufficient, plus a weekly `schedule` so the entry is accessed inside the 7-day window and a `workflow_dispatch` for manual re-seeding. The seed and release jobs must set the same `CARGO*`, `RUST*`, `CC*`, `CXX*` and `CMAKE*` variables or their environment hashes diverge (section 2.1). `KEYCHAIN_PATH` and `CADENCE_CODESIGN_IDENTITY` do not match those prefixes.

If dependencies change on `main` and a tag is pushed before the seed finishes, the release run still falls back through the restore key to the most recent `release` cache on `main` and rebuilds only the changed dependencies.

### 2.6 sccache

[sccache-action](https://raw.githubusercontent.com/Mozilla-Actions/sccache-action/main/README.md) is configured with `SCCACHE_GHA_ENABLED: "true"` and `RUSTC_WRAPPER: "sccache"`, and stats come from `${SCCACHE_PATH} --show-stats`. The [GHA backend doc](https://raw.githubusercontent.com/mozilla/sccache/main/docs/GHA.md) authenticates with `ACTIONS_RESULTS_URL` and `ACTIONS_RUNTIME_TOKEN`, meaning it stores objects in the same Actions cache service with the same branch scoping and the same 10 GB quota, and notes "In case sccache reaches the rate limit of the service, the build will continue, but the storage might not be performed." The [Rust doc](https://raw.githubusercontent.com/mozilla/sccache/main/docs/Rust.md) lists what cannot be cached: "Crates that invoke the system linker cannot be cached" (bin, dylib, cdylib and proc-macro crates), and "rustc's incremental compilation needs to be disabled". For a single-crate app whose dependency graph is already captured whole by rust-cache, sccache adds per-object network round trips and cannot cache the final LTO link; it only pays off where rust-cache misses often, for example matrices with many feature combinations. Not recommended here.

## 3. Build speed levers

### 3.1 Release profile settings

The Cargo [profiles reference](https://doc.rust-lang.org/cargo/reference/profiles.html) gives the release defaults as `opt-level = 3`, `debug = false`, `strip = "none"`, `lto = false`, `panic = 'unwind'`, `incremental = false`, `codegen-units = 16`. Cadence overrides three:

- `lto = "thin"`: "Performs 'thin' LTO. This is similar to 'fat', but takes substantially less time to run while still achieving performance gains similar to 'fat'." The default `false` "Performs 'thin local LTO' which performs 'thin' LTO on the local crate only across its codegen units. No LTO is performed if codegen units is 1 or opt-level is 0." The [rustc codegen options](https://doc.rust-lang.org/rustc/codegen-options/index.html) add: "For larger projects like the Rust compiler, ThinLTO can even result in better performance than fat LTO." The [perf book](https://nnethercote.github.io/perf-book/build-configuration.html): LTO "can improve runtime speed by 10-20% or more, and also reduce binary size, at the cost of worse compile times."
- `codegen-units = 1`: rustc: "Setting this to 1 may improve the performance of generated code, but may be slower to compile." Cargo: "The default is 256 for incremental builds, and 16 for non-incremental builds." The perf book: it "can improve runtime speed and reduce binary size at the cost of increased compile times." Because dependencies inherit the release profile, every dependency crate is also compiled with one codegen unit, which serialises LLVM work per crate; on a 3-core runner Cargo still runs three crates in parallel, so the loss is smaller than on a wide machine. This cost is paid once per dependency change when the cache is warm.
- `debug = "line-tables-only"`: "Generates the minimal amount of debug info for backtraces with filename/line number info, but not anything else." Cargo's `split-debuginfo` default "is `unpacked` on macOS for profiles that have debug information otherwise enabled." rustc: `unpacked` means "On macOS this means the original object files will contain debug information"; `packed` means "on macOS this is a `*.dSYM` folder"; `off` "On macOS this options prevents the final execution of `dsymutil`". So the line tables live in `target/release/deps/*.o` and nothing collects them into the binary that `scripts/package-app.sh` copies into `Cadence.app`. The setting currently buys compile time for no shipped benefit. Either add `split-debuginfo = "packed"` and upload the resulting `.dSYM` as a release asset for symbolicating crash reports, or drop `debug` from the release profile.
- Not set, and not worth setting: `panic = "abort"` "might reduce binary size and increase runtime speed slightly, and may even reduce compile times slightly" (perf book) but a panic in any thread then kills the whole app; "Tests, benchmarks, build scripts, and proc macros ignore the `panic` setting" (Cargo). `strip = "symbols"` shrinks the binary but "On platforms which depend on this symbol table for backtraces, profiling, and similar, this can affect them so negatively as to make the trace incomprehensible" (rustc). `embed-bitcode` is managed by Cargo: "Cargo uses `-C embed-bitcode=no` whenever possible" and it must stay on for LTO (rustc).
- Overrides cannot touch LTO: "Overrides cannot specify the `panic`, `lto`, or `rpath` settings" (Cargo), so `[profile.release.package."*"]` cannot exclude dependencies from thin LTO, though it could raise their `codegen-units`.

Any profile key can be tried from CI without editing `Cargo.toml` through `CARGO_PROFILE_<name>_<key>` variables such as `CARGO_PROFILE_RELEASE_CODEGEN_UNITS` ([environment variables](https://doc.rust-lang.org/cargo/reference/environment-variables.html)); note these enter the rust-cache key (section 2.1). `cargo build --timings` writes `target/cargo-timings/cargo-timing.html` showing per-crate durations and concurrency ([cargo build](https://doc.rust-lang.org/cargo/commands/cargo-build.html)); one run on a warm cache tells exactly how long the Cadence crate plus LTO link takes, which is the floor a warm release can reach.

### 3.2 Incremental compilation and `CARGO_INCREMENTAL`

Cargo: "Incremental compilation is only used for workspace members and 'path' dependencies" and "The incremental value can be overridden globally with the `CARGO_INCREMENTAL` environment variable". rustc: "Using incremental compilation inhibits certain optimizations (for example by increasing the amount of codegen units) and is therefore not recommended for release builds." Release is already non-incremental by default. For the dev profile in CI, rust-cache sets `CARGO_INCREMENTAL=0` itself (section 2.1) because incremental artifacts are workspace-only and never cached. Nothing to add.

### 3.3 `--locked`, git fetching and nightly-only flags

`--locked` "Asserts that the exact same dependencies and versions are used as when the existing `Cargo.lock` file was originally generated" and errors when "Cargo attempted to change the lock file due to a different dependency resolution. It may be used in environments where deterministic builds are desired, such as in CI pipelines." ([cargo build](https://doc.rust-lang.org/cargo/commands/cargo-build.html)). Both workflows already pass it to every cargo command.

`CARGO_NET_GIT_FETCH_WITH_CLI` "Enables the use of the `git` executable to fetch" ([environment variables](https://doc.rust-lang.org/cargo/reference/environment-variables.html)); it only affects git dependencies, and `Cargo.lock` has none.

`-Z threads=N` parallelises the compiler front end: "compile times can be reduced by up to 50%, though the effects vary widely" and "dev builds are likely to see bigger improvements than release builds", but it is nightly only and "there are some known bugs, including deadlocks" ([parallel rustc post](https://blog.rust-lang.org/2023/11/09/parallel-rustc/)). Cadence is on stable 1.97.1, so it does not apply.

### 3.4 Linkers on macOS

The perf book: "lld ... has been the default linker on Linux since Rust 1.90", "mold: Linux only", "wild: Linux only", and for macOS "an alternative linker isn't necessary because the system linker is fast" ([perf book](https://nnethercote.github.io/perf-book/build-configuration.html)). The [Rust 1.90.0 announcement](https://blog.rust-lang.org/2025/09/18/Rust-1.90.0/) scopes the lld default to `x86_64-unknown-linux-gnu`, and the rustc `link-self-contained` docs say "only the `-linker` opt-out is stable on the `x86_64-unknown-linux-gnu` target". rustc lists a `ld64.lld` linker flavor for Apple targets, but there is no stable self-contained linker path for `aarch64-apple-darwin`. Apple's own linker is already the fast one: [Xcode 15 release notes](https://developer.apple.com/documentation/xcode-release-notes/xcode-15-release-notes): "A new linker has been written to significantly speed up static linking. It's the default for all macOS, iOS, tvOS and visionOS binaries ... The classic linker can still be explicitly requested using -ld64, and will be removed in a future release." Its known issues are weak-symbol crashes on macOS 12 and older and weak imports from LTO objects, both with `-Wl,-ld_classic` as the workaround. The `macos-15` image defaults to Xcode 16.4, so the new linker is in use. Nothing to change.

### 3.5 Clippy, test and artifact sharing

Cargo's profile selection table: `cargo build`, `cargo check` and `cargo rustc` use `dev`; `cargo test` uses `test`, which "inherits the settings from the `dev` profile" ([profiles](https://doc.rust-lang.org/cargo/reference/profiles.html)). `cargo check` "will essentially compile the packages without performing the final step of code generation, which is faster than running `cargo build`" and "Some diagnostics and errors are only emitted during code generation, so they inherently won't be reported with `cargo check`" ([cargo check](https://doc.rust-lang.org/cargo/commands/cargo-check.html)). Clippy works "As with `cargo check`" ([Clippy usage](https://doc.rust-lang.org/clippy/usage.html)). The check-mode metadata and the build-mode rlibs are different artifacts: Cargo maintainer Eric Huss in [cargo#3501](https://github.com/rust-lang/cargo/issues/3501): "The `.rmeta` files created by `cargo check` are not the same as those created by `cargo build` (their contents differ)", and in 2026 the issue is still open, with `-Zfine-grain-locking` assuming "the artifacts of `cargo check` and `cargo build` are not shared". That is the 283 s clippy plus 570 s test split in the cold run: both passes walk all 1047 dependencies. Both sets of artifacts are already kept in the one 2.1 GB cache, so on a warm run the duplication only costs download size. Splitting clippy and test into parallel jobs would cut a cold run to roughly the test duration but double the cache download on warm runs and double the stored caches (section 4.1).

`cargo build` from `CLAUDE.md` is subsumed by `cargo test`, which builds the same dev-profile dependencies plus the test harness; CI does not need a separate build step.

### 3.6 nextest

[nextest](https://nexte.st/) is "Up to 3× faster than `cargo test`, with a modern interface, per-test isolation, and first-class CI support"; it runs "each individual test in a separate process, in parallel", builds with the equivalent of `cargo test --no-run` first, and does not run doctests: "run doctests in a separate step with `cargo test --doc`" ([how it works](https://nexte.st/docs/design/how-it-works/)). The [benchmarks](https://nexte.st/docs/benchmarks/) page attributes the gains to workspaces with many test binaries: "cargo test can only run them serially, while nextest can run those tests in parallel", with figures from 1.37x (penumbra) to 3.38x (crucible) on a 16-core machine, excluding build time. Cadence has one test binary whose warm `cargo test` step, including compilation, takes 25 s on a 3-core runner, so the payoff is small today. Retries, per-test timeouts and JUnit output are the reasons to adopt it later; installation is `- uses: taiki-e/install-action@nextest` ([pre-built binaries](https://nexte.st/docs/installation/pre-built-binaries/)).

## 4. Job structure, toolchain and runners

### 4.1 One job or several

Runs on `macos-15` are free for this public repository (section 4.4), so the trade is wall time against cache traffic. The warm CI run spends 82 s of its 144 s downloading the cache, so a second cached job adds another 80 s of transfer for no gain when warm; when cold, parallel `clippy` and `test` jobs finish in about the longer of the two (570 s) instead of their sum (853 s). Two jobs also need two caches (`key: clippy`, `key: test`, or two `shared-key` values), each roughly as large as today's combined one, against a quota that is already exceeded. The cheapest structural win is ordering: `cargo fmt --all --check` needs no cache, so running it before the rust-cache step, or in its own cache-less job, fails a formatting mistake in about a minute instead of after the download.

### 4.2 Concurrency and timeouts

[Concurrency docs](https://docs.github.com/en/actions/writing-workflows/choosing-what-your-workflow-does/control-the-concurrency-of-workflows-and-jobs): "Only a single job or workflow using the same concurrency group will run at a time." The recommended group is `${{ github.workflow }}-${{ github.ref }}` with `cancel-in-progress: true`; the current `ci-${{ github.ref }}` is equivalent. The docs also show `cancel-in-progress: ${{ !contains(github.ref, 'release/') }}` for keeping release refs uncancelled and `queue: max` for serialising deployments, which "cannot combine ... with `cancel-in-progress: true`". Tags are distinct refs, so a per-ref group in the release workflow is harmless and prevents a re-pushed tag from running twice.

[Workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax): `jobs.<job_id>.timeout-minutes` defaults to 360 minutes. Neither workflow sets it. `xcrun notarytool submit --wait` in `scripts/notarize.sh` blocks on Apple's service; a hang would consume six hours of runner time before GitHub intervenes.

### 4.3 Toolchain installation

rustup picks the toolchain by, in order, a `+toolchain` argument, `RUSTUP_TOOLCHAIN`, directory overrides, then `rust-toolchain.toml`, then the default ([overrides](https://rust-lang.github.io/rustup/overrides.html)), and `RUSTUP_AUTO_INSTALL` "(default: 1) When set to `1`, installs the active toolchain when it is absent" ([environment variables](https://rust-lang.github.io/rustup/environment-variables.html)). The image ships rustup 1.29.0 with Rust 1.98.0, so the first proxy call in the checkout, which is rust-cache's `rustc -vV`, downloads 1.97.1 with `rustfmt` and `clippy`; this is inside the 66 to 82 s rust-cache step. [dtolnay/rust-toolchain](https://raw.githubusercontent.com/dtolnay/rust-toolchain/master/README.md) selects the toolchain from the action ref ("`dtolnay/rust-toolchain@1.89.0` pulls in 1.89.0") and exposes a `cachekey` output; using it would install a second toolchain and duplicate the pin already in `rust-toolchain.toml`. It is not needed. The one toolchain-related change worth making is removing the image's `stable` toolchain before rust-cache runs, so the key stops depending on the image (section 2.1); with the toolchain file in place every cargo call still resolves to 1.97.1.

### 4.4 Runners and cost

[GitHub-hosted runners reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners): the arm64 macOS runner behind `macos-latest`, `macos-14`, `macos-15` and `macos-26` is "3 (M1)" CPUs, "7 GB" RAM, "14 GB" SSD, for public and private repositories alike; the Intel variants (`macos-15-intel`, `macos-26-intel`) are 4 CPUs and 14 GB. The [runner-images README](https://raw.githubusercontent.com/actions/runner-images/main/README.md) maps `macos-latest` to "macOS 26 Arm64" (`macos-26`, `macos-26-xlarge`), keeps `macos-15` and `macos-15-xlarge` as arm64 macOS 15, and lists macOS 14 as deprecated. Pinning `macos-15` therefore already avoids a silent jump to macOS 26 and its Xcode 26 default. The [larger runners reference](https://docs.github.com/en/actions/reference/runners/larger-runners) lists `macos-15-large` as Intel 12 CPU 30 GB and `macos-15-xlarge` as arm64 M2 5 CPU 14 GB.

[Actions runner pricing](https://docs.github.com/en/billing/reference/actions-runner-pricing): Linux 2-core x64 $0.006 per minute, Linux 2-core arm64 $0.005, Windows 2-core $0.010, "macOS 3-core or 4-core (M1 or Intel)" $0.062, macOS 12-core $0.077, macOS 5-core M2 Pro $0.102; "GitHub rounds the minutes and partial minutes each job uses up to the nearest whole minute"; "Included minutes cannot be used for larger runners" and larger runners "are not free for public repositories". [About billing for GitHub Actions](https://docs.github.com/en/billing/concepts/product-billing/github-actions): "The use of standard GitHub-hosted runners is free: In public repositories" and "Larger runners are always charged for, even when used by public repositories or when you have quota available from your plan." `infomiho/cadence` is public, so today's macOS minutes cost nothing; the cost argument only bites if the repository goes private (about ten times a Linux minute) or moves to an xlarge runner. Jobs that do not need macOS, such as formatting, `cargo deny` or `cargo audit`, belong on `ubuntu-latest` regardless.

### 4.5 Supply-chain checks

[cargo-deny-action](https://raw.githubusercontent.com/EmbarkStudios/cargo-deny-action/main/README.md) checks "advisories, bans, licenses, sources" via `- uses: EmbarkStudios/cargo-deny-action@v2` with `command: check` and `arguments`, and recommends running advisories separately so a "sudden announcement of a new advisory" does not fail unrelated CI. [rustsec/audit-check](https://raw.githubusercontent.com/rustsec/audit-check/main/README.md) runs `cargo audit`, needs `checks: write` and `issues: write`, and on a `schedule` "will check if there any new advisories appear for crate dependencies" and open issues. With `THIRD_PARTY_NOTICES.md` in the bundle, the licenses check is the one with a concrete payoff. Optional, Linux runner, weekly schedule.

## 5. Security

### 5.1 Pinning actions

[Security hardening](https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions): "Pinning an action to a full-length commit SHA is currently the only way to use an action as an immutable release." "Pinning to a particular SHA helps mitigate the risk of a bad actor adding a backdoor to the action's repository, as they would need to generate a SHA-1 collision for a valid Git object payload." On tags: "there is risk to this approach even if you trust the author, because a tag can be moved or deleted if a bad actor gains access to the repository storing the action." The workflow syntax reference agrees: "Using the commit SHA of a released action version is the safest for stability and security." `.github/dependabot.yml` already has the `github-actions` ecosystem, and Dependabot "will now update the semver version in comments when updating Actions workflows with a commit SHA version" ([GitHub changelog, 2022-10-31](https://github.blog/changelog/2022-10-31-dependabot-now-updates-comments-in-github-actions-workflows-referencing-action-versions/)), so SHA pins stay maintainable. Resolved on 2026-09-08 with `gh api`: `actions/checkout@v7` is `3d3c42e5aac5ba805825da76410c181273ba90b1` (v7.0.1); `Swatinem/rust-cache@v2` is `6323deb102c322ba6fcbdcafc7e3dddab59af2b6` (v2.9.2), the same SHA the release run resolved; `taiki-e/install-action@v2` is `d438492cf8a250514fa2d34b30bc3c0dc37c65ff` (v2.87.8); `EmbarkStudios/cargo-deny-action@v2` is `3c6349835b2b7b196a839186cb8b78e02f7b5f25` (v2.1.1).

### 5.2 Token permissions and checkout credentials

Security hardening: "It's good security practice to set the default permission for the `GITHUB_TOKEN` to read access only for repository contents. The permissions can then be increased, as required, for individual jobs within the workflow file." Workflow syntax: "if you specify the access for any of these permissions, all of those that are not specified are set to 'none'." CI already has `contents: read`; release needs `contents: write` for `gh release create` and nothing else, which it has. [actions/checkout](https://raw.githubusercontent.com/actions/checkout/main/README.md) v7 requires Node 24; its `persist-credentials` input, "Whether to configure the token or SSH key with the local git config", defaults to `true`, and since v6 the credential is stored "in a separate file under $RUNNER_TEMP instead of directly in .git/config". Neither workflow pushes, so `persist-credentials: false` keeps the write token out of later steps (`scripts/*.sh`, cargo build scripts).

### 5.3 Secrets and the signing keychain

[Using secrets](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/use-secrets): "avoid passing secrets between processes from the command line, whenever possible" and pass them through environment variables or STDIN; binary blobs are stored as base64 (`base64 -w 0 cert.der`) and decoded in the job with `base64 --decode`; secrets are capped at 48 KB ([secrets reference](https://docs.github.com/en/actions/reference/security/secrets)). Security hardening adds: "If your secret is transformed in some way (such as Base64 or URL-encoded), be sure to register the new value as a secret too" and "Mask all sensitive information that is not a GitHub secret by using `::add-mask::VALUE`." GitHub's [Apple certificate guide](https://docs.github.com/actions/use-cases-and-examples/deploying/installing-an-apple-certificate-on-macos-runners-for-xcode-development) creates the keychain at `$RUNNER_TEMP/app-signing.keychain-db`, notes "A new keychain will be created on the runner, so the password for the new keychain can be any new random string", runs `security create-keychain`, `set-keychain-settings -lut 21600`, `unlock-keychain`, `import ... -P ... -A -t cert -f pkcs12 -k`, `set-key-partition-list -S apple-tool:,apple: -k`, `list-keychain -d user -s`, and deletes the keychain in a final step under `if: ${{ always() }}`, which matters because self-hosted runners are not destroyed after the job.

Against that guide, `release.yml` does the right things: the certificate and API key come in through `env:` and `printf '%s' | base64 --decode`, never on a command line; the keychain password is `openssl rand -hex 16` and is never printed; `set-key-partition-list` is present so `codesign` can use the key without a UI prompt; the keychain is deleted under `if: always()`. Three deviations: the keychain lives at `${{ github.workspace }}/release.keychain-db` inside the checkout rather than `$RUNNER_TEMP`; the decoded `certificate.p12` and `AuthKey.p8` are not removed after use (harmless on a hosted runner, wrong on a self-hosted one); and the derived signing identity is written to `$GITHUB_ENV`, which is fine because a Developer ID name is not secret.

### 5.4 Gating the release

[Managing environments](https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/manage-environments): environments offer "Required reviewers" ("up to 6 people or teams", "only one of the required reviewers needs to approve"), a "Wait timer", and "Deployment branches and tags" with "Protected branches only" or "Selected branches and tags", where "Name patterns must be configured for branches or tags individually." "Environment secrets ... are only available to workflow jobs that use the environment", and jobs cannot access them "until any configured rules (for example, required reviewers) pass." Environments are available to public repositories on every plan. Moving the five Apple secrets into a `release` environment restricted to tags matching `v*` means a workflow on any other ref, including one introduced by a compromised dependency update PR, can never read them. [Rulesets](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets) restrict who may create the tags in the first place: "Only users with bypass permissions can create branches or tags whose name matches the pattern you specify."

### 5.5 Review of the current release.yml

| Item | Status | Reference |
| --- | --- | --- |
| Actions referenced by mutable tag | Gap | 5.1 |
| `permissions: contents: write`, nothing more | OK | 5.2 |
| Checkout persists the write token | Gap | 5.2 |
| No `timeout-minutes` (default 360) | Gap | 4.2 |
| Secrets passed through `env`, decoded from base64 | OK | 5.3 |
| Random keychain password, never echoed | OK | 5.3 |
| `set-key-partition-list`, `list-keychains`, cleanup under `if: always()` | OK | 5.3 |
| Keychain inside `github.workspace` instead of `$RUNNER_TEMP` | Minor | 5.3 |
| Decoded `.p12` and `.p8` left on disk | Minor | 5.3 |
| No `environment`, secrets readable by any run of this workflow | Gap | 5.4 |
| No tag ruleset restricting who can push `v*` | Gap | 5.4 |
| rust-cache saves a tag-scoped cache nobody can restore | Waste | 2.4 |
| `cargo build --release --locked`, `--verify-tag` on release creation | OK | 3.3 |
| No `${{ }}` interpolation inside `run:` scripts | OK | 5.2 |

## Implications for Cadence

Recommended changes, in priority order. Each cites the section it rests on.

### 1. Seed a release cache on main and stop saving tag caches (sections 2.1, 2.4, 2.5)

New `.github/workflows/release-cache.yml`:

```yaml
name: Release cache

on:
  push:
    branches: [main]
    paths:
      - Cargo.lock
      - Cargo.toml
      - rust-toolchain.toml
      - .github/workflows/release-cache.yml
  schedule:
    - cron: "17 6 * * 1"
  workflow_dispatch:

permissions:
  contents: read

concurrency:
  group: release-cache
  cancel-in-progress: true

jobs:
  seed:
    runs-on: macos-15
    timeout-minutes: 40
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
        with:
          persist-credentials: false
      - run: rustup toolchain uninstall stable
      - uses: Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6 # v2.9.2
        with:
          shared-key: release
      - run: cargo build --release --locked
```

In `release.yml`, replace the cache step with:

```yaml
      - run: rustup toolchain uninstall stable
      - uses: Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6 # v2.9.2
        with:
          shared-key: release
          save-if: false
```

The weekly schedule keeps the entry accessed within the 7-day eviction window. The `paths` filter is safe because rust-cache ignores workspace crates and normalises package versions, so version-bump commits do not need a reseed. Expected effect: a tag build drops from 8 to 15 minutes of compilation to the time for the Cadence crate plus the thin-LTO link, which one `cargo build --release --timings` run on a warm cache will quantify. If the extra workflow feels like too much, the same seed can be a second job in `ci.yml` gated on `github.event_name == 'push'`, at the price of a release-profile compile of the Cadence crate on every push to `main`.

### 2. Stop storing pull-request caches and free the quota (sections 1, 2.4)

In `ci.yml`:

```yaml
      - uses: Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6 # v2.9.2
        with:
          shared-key: check
          save-if: ${{ github.ref == 'refs/heads/main' }}
```

Pull requests keep restoring the `main` cache (base-branch access) and rebuild only their changed dependencies; nothing is written to `refs/pull/*/merge`. Delete the existing tag and pull-request caches once with `gh cache delete` so `main`'s entries are not evicted first. With one `check` cache (about 2.2 GB) and one `release` cache (about 1 GB) per lockfile generation, the repository stays well inside 10 GB even while an old and a new generation overlap.

### 3. Remove the image's stable toolchain from the cache key (sections 1, 2.1, 4.3)

Add `- run: rustup toolchain uninstall stable` after checkout in every job that uses rust-cache, as shown above. Without it, the environment hash `d107d33f` changes the next time the `macos-15` image ships a newer Rust, and every cache in the repository goes cold at once even though the build still uses 1.97.1. The alternative, `add-rust-environment-hash-key: false` with a manual `key`, loses the lockfile fallback and the `RUSTFLAGS` hashing and is worse.

### 4. Pin actions to commit SHAs and drop persisted credentials (sections 5.1, 5.2)

Replace `actions/checkout@v7` with `actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1` and `Swatinem/rust-cache@v2` with `Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6 # v2.9.2` in both workflows, and add `with: persist-credentials: false` to every checkout. Dependabot's existing `github-actions` entry will keep both the SHA and the comment current.

### 5. Add timeouts and a per-ref concurrency group to the release (section 4.2)

```yaml
concurrency:
  group: release-${{ github.ref }}
  cancel-in-progress: false

jobs:
  release:
    runs-on: macos-15
    timeout-minutes: 45
```

and `timeout-minutes: 30` on the CI job. A stuck notarization then costs at most 45 minutes instead of 6 hours.

### 6. Gate the signing secrets behind a release environment and a tag ruleset (section 5.4)

Create a `release` environment, move `APPLE_CERTIFICATE_P12`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_API_KEY_P8`, `APPLE_API_KEY_ID` and `APPLE_API_ISSUER_ID` into it, set "Deployment branches and tags" to the tag pattern `v*`, and add `environment: release` to the job. Add a ruleset for tags matching `v*` with "Restrict creations" so only the bypass list can start a release. Required reviewers are optional for a single-maintainer project; the environment scoping is the part that removes secrets from every other workflow run.

### 7. Move the keychain to `$RUNNER_TEMP` and delete decoded key files (section 5.3)

Set `KEYCHAIN_PATH: ${{ runner.temp }}/release.keychain-db`, and `rm -f "$certificate"` after `security import` and `rm -f "$APPLE_API_KEY_PATH"` after `scripts/notarize.sh`. This matches GitHub's Apple certificate guide and costs nothing.

### 8. Decide what release debug info is for (section 3.1)

Either add `split-debuginfo = "packed"` under `[profile.release]` and upload `target/release/spotify-gpui-client.dSYM` as a release asset for symbolicating user crash reports, or remove `debug = "line-tables-only"`. Today the setting produces object-file line tables that never leave the runner.

### 9. Fail fast on formatting (section 4.1)

Move `cargo fmt --all --check` above the rust-cache step in `ci.yml`. A formatting failure then ends the job in about a minute instead of after an 80-second cache download. Keeping clippy and test in one job remains the right call while the repository is a single crate with a single test binary.

### 10. Optional: license and advisory checks on a Linux runner (sections 4.4, 4.5)

A scheduled `cargo-deny` job on `ubuntu-latest` checking `licenses` and `advisories` keeps `THIRD_PARTY_NOTICES.md` honest without spending macOS minutes. nextest, sccache, alternative linkers and `-Zthreads` are not worth adding for this crate (sections 2.6, 3.3, 3.4, 3.6).
