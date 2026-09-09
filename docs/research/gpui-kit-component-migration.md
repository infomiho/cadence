# GPUI Kit component migration and visual parity

Cadence can adopt more Kit behavior while keeping its presentation. The strongest fit is the unstyled `gpui_kit::base` layer. The styled `gpui_kit::component` layer remains useful where its existing appearance is already part of Cadence, including Input, Select and Switch.

This assessment compares application source with gpui-kit v0.6.1 source. It establishes feasibility, not demonstrated pixel parity. No application code was changed and no before/after renders were produced. Implementation tracking lives in `cadence-p14`, with this assessment recorded as `cadence-wqf`.

## The visual contract

For existing states, matching means identical rendered pixels under controlled conditions. It includes typography and line height, icon assets and sizes, control bounds, padding, radii, borders, shadows, clipping, hover and pressed colors, selected-plus-hover precedence, loading content, popup placement and motion. A matching idle screenshot is insufficient.

The migration must not add tooltips, change colors on selection, introduce default focus rings, replace Solar icons, change empty-state copy, or add component animations. Those are visual changes even when independently desirable. Existing migration tickets that request such improvements need to keep that work separate from this parity requirement.

New keyboard paths can expose states that have no existing rendered baseline. Their focus presentation must be specified and tested explicitly. It would be inaccurate to call those new states a verified before/after match. Pointer interactions must continue to produce the same visuals.

## Why the base layer fits

Kit exposes GPUI, unstyled base controls and styled components through one dependency. Base controls accept application-owned styling and content. This allows behavior adoption without taking a component theme or replacing the layout. It also means a broad theme migration is not a prerequisite for every control migration. [Kit facade](https://github.com/longbridge/gpui-kit/blob/v0.6.1/crates/kit/src/lib.rs), [base layer](https://github.com/longbridge/gpui-kit/blob/v0.6.1/crates/base/src/lib.rs)

## Control assessment

| Cadence surface | Useful adoption | Benefit | Visual feasibility and recommendation |
| --- | --- | --- | --- |
| Transport, queue trigger, mute, favorites, close and ordinary action buttons | Base Button, with Base Toggle considered for actual on/off controls | Owned focus, named button/toggle semantics, controlled activation and disabled behavior | Strong candidate. Preserve each existing child and every state style. Migrate leaf controls individually before changing the shared helper. |
| Spotify app confirmation | Base Dialog with the current popup and backdrop | Focus containment, dialog semantics, dismissal lifecycle and focus restoration | Strong candidate. Keep the centered 440px card, scrim, 24px padding, 16px radius and current buttons. Initial focus can affect pixels and needs explicit treatment. |
| External Spotify links | Base Link | Link semantics, URL activation and owned focus | Small, useful candidate. Keep existing text or button presentation and URL behavior. Verify no duplicate activation from retaining the old handler. |
| Light, dark and system appearance choices | Base Radio and RadioGroup | Expresses one selected choice while retaining custom card content | Conditional. Preserve the 44px cards, gaps, icons, selected font weights and backgrounds. Group arrow navigation must be verified or implemented, not inferred from its name. |
| Progress and volume | Base Slider primitives | Shared pointer/value handling and accessible value operations | Conditional. Stronger fit than styled Slider, but not a complete keyboard solution. Verify geometry and event behavior before adopting. |
| Account and track action menus | Styled PopupMenu would supply the most behavior | Directional navigation, disabled-item handling, Escape and focus restoration | Do not migrate to the styled menu under this requirement. Its fixed wrappers and row layout differ. Base Popup supplies positioning only, so it does not solve menu keyboard behavior. Keep current menu surfaces. |
| Sidebar navigation | Base Button around existing row contents | Focus and named navigation controls without replacing custom rendering | Preserve the existing animated fills, label fading and widths. Styled SidebarMenuItem is a poor match for that composition. |
| Queue panel | Existing custom drawer, optionally base primitives for its controls | Button improvements can be isolated without changing panel behavior | Keep the panel. Styled Sheet changes chrome and motion. Base Sheet traps focus and uses viewport positioning, which is not a neutral replacement for this nonmodal panel above the player. |
| Action notice | Existing banner, optionally Base Toast | Alert semantics and observation support | Low migration value. Base Toast does not add timing or queue management by itself. Styled Notification changes the close control and animation. Assess appropriate announcement semantics before choosing Toast. |
| Track and playlist rows, virtualized lists, artwork, page frames and mascot | Existing custom elements | No compelling additional benefit from wholesale replacement | Keep their layout, occurrence identity, virtualization and animation. A track row contains several independent actions and should not become a generic button just because it uses the shared helper today. |
| Search input, Client ID input, mascot Select, autoplay Switch, Avatar, Icon and Spinner | Existing Kit components | Already adopted | Exercise their real application behavior after the upgrade. No replacement is needed. |

Application evidence: [shared controls](../../src/app/components.rs), [player and queue](../../src/app/player_bar.rs), [confirmation and account menu](../../src/app/chrome.rs), [track controls](../../src/app/track_row.rs), [track menus](../../src/app/track_list.rs), [sidebar](../../src/app/sidebar.rs), [settings](../../src/app/settings.rs), [onboarding](../../src/app/onboarding.rs). Upstream customization details and source citations are in [control findings](gpui-kit-control-customization.md) and [overlay findings](gpui-kit-overlay-customization.md).

## Important implementation boundaries

### Buttons are not a safe global substitution

The shared `components::button` returns a `Stateful<Div>` and serves both leaf controls and compound rows. It sets `tab_stop(true)` and a focus border, but does not establish a focus handle. In the currently locked GPUI source, `tab_stop` only changes traversal metadata and `focus` only defines a style. Native GPUI synthesizes Enter/Space clicks for focused elements, so the gap is not the absence of a native keyboard activation mechanism.

Base Button supplies focus ownership and an accessible button role, but also supplies a neutral line height of 1. Explicitly preserve inherited text metrics. Copying the current `.focus(border_2 + rounded(12px))` onto a newly focusable button can reveal a border on pointer activation that was previously dormant. A blind helper replacement therefore cannot satisfy the visual contract. [Base Button](https://github.com/longbridge/gpui-kit/blob/v0.6.1/crates/base/src/button.rs)

Keep nested action propagation correct. Activating a favorite or menu trigger must not also activate the track row. Cadence's global Space binding excludes inputs, not every interactive control. Test that focusing and activating a migrated control does not also toggle playback. [Bindings](../../src/app/bootstrap.rs), [playback action](../../src/app/actions.rs)

### Sliders need more than matching colors

Cadence progress is a 5px rail with a 3px radius, no thumb, and click-to-seek. Volume has a 4px rail, a 12px thumb and a 24px interaction area. Its thumb position is based on the available width minus the thumb width. The styled Slider instead draws a 6px rail, a 16px thumb and an animated hover ring. Matching just the outer dimensions cannot preserve the current appearance. [Current sliders](../../src/app/player_bar.rs), [styled Slider](https://github.com/longbridge/gpui-kit/blob/v0.6.1/crates/component/src/slider.rs)

Base Slider exposes separately styled parts, but accessible increment/decrement operations are not proof of keyboard arrow support. A migration still needs explicit focus and key handling, correct pointer-to-value bounds, endpoints, seek delivery policy and backend synchronization during dragging. Test zero, midpoint and maximum values as well as compact mode. [Base Slider](https://github.com/longbridge/gpui-kit/blob/v0.6.1/crates/base/src/slider.rs)

### Overlay appearance and behavior are separate

The current queue is within the main content region and leaves the player available. A viewport sheet with focus trapping changes that contract even if its surface can be made to look identical. Keep the existing panel shell.

The confirmation is a modal use case and has a much better fit. Preserve outside-click policy and cancellation behavior while explicitly wiring focus restoration in both the main and onboarding hosts. For menus, retain row dimensions, headers, separators, disabled colors, anchors and edge behavior. Replacing only the positioning primitive offers little benefit because the track menu already snaps to the window. [Overlay details](gpui-kit-overlay-customization.md)

## Evidence required for acceptance

The dependency upgrade and component migration need separate comparisons so upstream rendering changes can be attributed correctly. Capture the unchanged application baseline, verify the upgrade, then compare each component migration against that baseline. Updating baseline images merely to accept a changed appearance does not establish parity.

Native comparison must use the same OS, renderer, device scale, fonts, window dimensions and appearance. Fixtures should control data, artwork, timestamps, cursor location, focus and animation time. Capture the unchanged baseline repeatedly first. If identical inputs do not produce repeatable pixels, fix that instability before using a zero-difference gate. Do not mask the migrated control or hide differences behind a broad tolerance.

The state matrix includes both appearances, compact and regular layouts, resting, hovered, pressed, selected, selected-plus-hover, loading and existing disabled states. Menus and dialogs need opening, open, dismissal and window-edge cases. Sidebar and mascot motion need deterministic intermediate frames wherever a changed control participates in them. Newly reachable keyboard focus states require their own explicit expected appearance.

Headless tests should exercise production views and assert actions, state, focus and layout with controlled services. They cannot establish pixel equivalence. Native Metal rendering checks use a separate harness, and their command must be included in CI alongside the existing build, formatting, Clippy and unit-test gates. [Testing guide](https://github.com/longbridge/gpui-kit/blob/v0.6.1/website/docs/test.md)

The smallest useful proof is one leaf control, such as the queue trigger, with exact state images and interaction tests. Buttons and the confirmation are the strongest initial migration candidates. Sliders and grouped settings are conditional candidates. The styled menu, sidebar, drawer and notification replacements do not meet the visual constraint as straightforward substitutions.
