# GPUI Kit 0.6.1

Verified against the release and tagged testing guide on 2026-09-09.

## Upgrade benefits

The release adds headless interaction/layout tests, fixes selected ghost-button backgrounds, improves Select accessibility and dismissal callbacks, and adds preferred Button tooltip placement. Most editor and Markdown changes only matter to applications using those features. [Release notes](https://github.com/longbridge/gpui-kit/releases/tag/v0.6.1)

`Button::tooltip_placement(Placement::Top)` works with ordinary and action tooltips. Placement remains a preference, with flipping and clamping near window edges. Custom div controls do not gain this Button API automatically. [Tooltip change](https://github.com/longbridge/gpui-kit/pull/2971)

## Application testing

Enable `test-support` on a development dependency resolving to the same GPUI Kit version and source as the application. No extra testing crate, GPUI dependency, fork or Cargo patch is needed. Headless tests still compile native platform dependencies. Use explicit imports in test modules because `use gpui_kit::*` can shadow Rust's ordinary test macro. [Tagged guide](https://github.com/longbridge/gpui-kit/blob/v0.6.1/website/docs/test.md)

Tests use `#[gpui_kit::test]`, `TestAppContext`, and `gpui_kit::test::TestWindowExt`. Render the production view with controlled external dependencies. Kit controls register themselves. Native div targets require `TestSupportExt` and `.test_support()`, which preserves their native type in ordinary builds. Put observation before `.track_focus(&handle)` when testing focus. [Tagged guide](https://github.com/longbridge/gpui-kit/blob/v0.6.1/website/docs/test.md)

Use stable element IDs, `find`/`try_find`, and `within` for repeated controls. Native interaction helpers cover clicks, keyboard input, scrolling and dragging. Assert application results alongside control state. Snapshots expose accessibility values and geometry, not arbitrary rendered text. Missing boolean properties return `None`, which does not mean false or enabled. Disabled behavior needs an attempted interaction and an unchanged result. [Tagged guide](https://github.com/longbridge/gpui-kit/blob/v0.6.1/website/docs/test.md)

Render a frame before querying. Use `TestAppContext::update_window`, because typed `WindowHandle::update` already borrows the root and cannot safely redraw it in that callback. Deferred work needs bounded `wait_for` outside the window update. The helper does not simulate network services. Virtualized rows become queryable after scrolling paints them. [Tagged guide](https://github.com/longbridge/gpui-kit/blob/v0.6.1/website/docs/test.md)

## Coverage limits

Headless state/layout tests do not establish pixel correctness, packaged-app behavior or full OS IME composition. Pixel checks use a separate renderer, currently macOS Metal only. Its executable needs a custom main-thread harness, and upstream explicitly selects its rendering target in CI. Ordinary `cargo test` does not run that upstream target. Visual regression needs reviewed images and controlled fonts, size, theme, focus and animation state. [Tagged guide](https://github.com/longbridge/gpui-kit/blob/v0.6.1/website/docs/test.md)

For Cadence, the implication is a production-view fixture with isolated services and captured commands before meaningful UI tests. Custom player, queue and navigation controls need observation targets. Upgrading alone does not give these controls the styling or accessibility fixes of Kit Button. Headless tests can protect interaction behavior, while a native visual pass remains necessary for polish.
