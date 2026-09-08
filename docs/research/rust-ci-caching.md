# Rust CI caching for Cadence

Researched 2026-09-08. This focused assessment uses recent Actions logs and current primary documentation. Recommendations are specific to Cadence's macOS app and pinned Rust 1.97.1 toolchain.

## Measured baseline

| Run | Cache restore | Clippy compilation | Test compilation | Other evidence |
| --- | --- | --- | --- | --- |
| [34239698652](https://github.com/infomiho/cadence/actions/runs/34239698652) | 47 seconds, about 2.17 GB | 19.79 seconds | 23.81 seconds | Test execution took 0.31 and 0.01 seconds across two binaries |
| [34241359189](https://github.com/infomiho/cadence/actions/runs/34241359189) | 39 seconds, fallback hit | 15.59 seconds | 18.18 seconds | Existing dependencies were reused |
| [34241363592](https://github.com/infomiho/cadence/actions/runs/34241363592) | Miss | | | Release compilation took 10 minutes 29 seconds |
| [34238486907](https://github.com/infomiho/cadence/actions/runs/34238486907) | Miss | | | Release compilation took 7 minutes 58 seconds |

Cache transfer is comparable to the combined warm compiler work. Test execution is negligible. The priorities are reducing archive size, avoiding redundant saves, and giving release builds an accessible dependency cache.

## Recommended CI baseline

Keep one macOS job for build, Clippy and tests. Put formatting before cache restoration so formatting failures avoid the download. Retain an explicit `cargo build --locked` to match the repository's four documented quality gates. Clippy checks code without the final code generation performed by a normal build, which can reveal additional errors. [Cargo check](https://doc.rust-lang.org/cargo/commands/cargo-check.html)

Use `CARGO_PROFILE_DEV_DEBUG: line-tables-only` in CI. Cargo's default development profile generates full debug information. Line tables retain filename and line information for backtraces while omitting variable and parameter information. The test profile inherits development settings, and environment configuration can override manifest profiles. This targets artifact size without changing local debugging or release optimization. `debug = 0` is an alternative if losing source locations is acceptable. The size and timing improvement need measurement on hosted runners. [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html), [Cargo configuration](https://doc.rust-lang.org/cargo/reference/config.html#profile)

Keep `Swatinem/rust-cache` and save only from `refs/heads/main`. It already caches dependency artifacts, removes workspace and incremental artifacts, disables incremental compilation and supports fallback after dependency changes. The `save-if` input allows PRs to restore without saving. A PR that changes dependencies may consequently rebuild those changes on subsequent runs until merged. [rust-cache documentation](https://github.com/Swatinem/rust-cache)

PR caches belong to their merge ref and cannot be used by main or other PRs. Main-only writes avoid duplicating multi-gigabyte archives in those isolated scopes. GitHub also evicts caches after seven days without access and applies repository storage limits. [GitHub cache reference](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching)

## Release cache strategy

Different tag names cannot restore each other's caches. They can restore caches on the default branch. Existing CI and release jobs also use different default job-based cache prefixes, and development artifacts do not replace release-profile artifacts. Giving the jobs the same key alone does not produce a warm release. [GitHub cache reference](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching), [rust-cache documentation](https://github.com/Swatinem/rust-cache)

A release warmer needs its own `shared-key: release`, the release runner and compiler environment, and `cargo build --release --locked` on main. Tags restore that same cache with `save-if: false`. This trades same-tag rerun caching for less storage and upload time. The application and final link still run on each release because the default cache retains dependencies. [rust-cache documentation](https://github.com/Swatinem/rust-cache)

Use a dedicated warmer and build only when the cache action reports `cache-hit != 'true'`. A pure application version bump keeps the dependency key: rust-cache normalizes the manifest package version and excludes workspace package records from the lockfile hash. Installed toolchains and Cargo environment variables also enter its key, so the warmer and release must match those inputs. [rust-cache key implementation](https://raw.githubusercontent.com/Swatinem/rust-cache/master/src/config.ts)

Two useful trigger policies have different costs:

- Every main push, with compilation conditional on a cache miss, handles eviction and runner-image changes automatically but downloads the release archive even when no release is planned.
- Dependency and toolchain path changes plus manual dispatch avoid most warm-run downloads but can leave the release cache expired or mismatched after a runner-image update. Dispatch before tagging when release latency matters.

The implementation adopts the second policy, including changes to the warmer and release workflow in the path filter. It has no schedule. A cold seed moves several minutes of compilation earlier and adds another application link. It does not automatically reduce total compute. Measure a warmed release before treating the 8 to 10 minute cold duration as recoverable savings. These are workload recommendations inferred from the measured runs and cache behavior, rather than measured results of a warmer.

Wait for the seed to finish before pushing a release tag. After cache expiry or a runner-image change, start it manually with `gh workflow run release-cache.yml --ref main` once that workflow exists on main.

## Tools that do not address this bottleneck

`sccache` caches compiler invocations, but its Rust support requires incremental compilation to be disabled and cannot cache crates that invoke the system linker, including binaries and procedural macros. It therefore cannot eliminate Cadence's final app link. Keep the existing dependency cache until a controlled comparison shows compiler-cache hits offset added setup and transfer costs. [sccache Rust limitations](https://github.com/mozilla/sccache/blob/main/docs/Rust.md)

Nextest improves scheduling across test binaries and gives per-test process isolation. It still builds tests first, so it cannot materially shorten a workload with about 0.32 seconds of execution. Adopt it for isolation, timeouts or reporting if those become useful. Doctests currently require a separate `cargo test --doc` invocation. [Nextest execution model](https://nexte.st/docs/design/how-it-works/), [Nextest documentation](https://nexte.st/)

## Validation

Compare a cold main run and subsequent warm runs on the same runner and toolchain. Record archive bytes, restore and save durations, compiler-reported build times, test execution time and total job duration. Compare both exact and fallback hits. A new debug setting changes cache identity, so its first cold run cannot establish a regression. Also record release seed duration and final release compilation separately.
