# Overlay migration with visual parity

Examined gpui-kit v0.6.1 tagged source and Cadence overlays. These are source-level feasibility findings, not demonstrated screenshot parity.

## Best candidates

- **Confirmation dialog: use `gpui_kit::base::Dialog`.** Its `backdrop` and `popup` accept application elements, so the existing centered 440px card, 24px padding, 16px radius, Solar icons, typography and buttons can remain. The host supplies focus trapping, dialog semantics, keyboard actions and dismissal callback ordering. Wire initial focus and restoration deliberately and disable backdrop dismissal if preserving the existing behavior. `base::AlertDialog` is also worth evaluating for the destructive confirmation. The styled Dialog has overrideable surface styling and custom content, but imposes positioning, content wrappers and animation. Its default placement is one tenth of viewport height, unlike Cadence's vertical centering. [Base Dialog](https://github.com/longbridge/gpui-kit/blob/v0.6.1/crates/base/src/dialog.rs), [styled Dialog](https://github.com/longbridge/gpui-kit/blob/v0.6.1/crates/component/src/dialog/dialog.rs), [current confirmation](../../src/app/chrome.rs)

- **Action notice: use `gpui_kit::base::Toast` if semantic coverage is useful.** It implements Styled, ParentElement and interaction traits, renders the application-owned content unchanged, and adds `Role::Alert` and test support. Transfer the existing banner style onto the root to avoid an extra layout wrapper. It does not provide timing or notification management by itself. Keep the current 360px width, top 76px/right 24px position, 14px radius, 13px type and permanently visible 32px dismiss button. [Base Toast](https://github.com/longbridge/gpui-kit/blob/v0.6.1/crates/base/src/toast.rs), [current notice](../../src/app/components.rs)

## Conditional candidates

- **Queue drawer: `base::Sheet` preserves arbitrary surface presentation, but changes focus behavior.** It accepts `surface` and `overlay`, with no imposed title bar or animation. The current 420px drawer and custom header could remain. However, it always installs a focus trap and anchors its host to the viewport. Cadence's queue is currently a custom panel within the application layout, so adopting modal focus semantics needs a deliberate product decision and careful bounds matching. Do not migrate only for consistency. [Base Sheet](https://github.com/longbridge/gpui-kit/blob/v0.6.1/crates/base/src/sheet.rs), [current drawer](../../src/app/player_bar.rs)

- **Menu positioning: `base::Popup` can host existing menu surfaces.** It owns measurement, deferred layering, anchor calculation and window-edge snapping. It explicitly leaves open state, interaction, content and motion to the caller. Therefore it does not supply keyboard menu navigation or replace account/track menu state management. Its fixed 8px window margin and first-frame measurement can change placement, so test edge positions and the opening frame. [Base Popup](https://github.com/longbridge/gpui-kit/blob/v0.6.1/crates/base/src/popup.rs), [account menu](../../src/app/chrome.rs), [track menu](../../src/app/track_list.rs)

## Styled components that fail the strict parity bar

| Component | Benefit | Parity obstacle |
| --- | --- | --- |
| PopupMenu | Arrow navigation, confirmation, Escape, focus restoration, menu semantics, outside dismissal, submenus | No general Styled implementation for the menu. The items wrapper hardcodes `p_1` and `gap_y_0p5`. Rows impose 8px horizontal padding, 20px or 26px standard height and themed rounding. Custom element items can grow taller but remain inside these wrappers. Cadence uses 6px surface padding, no row gap, 36px rows and 10px or 12px row padding. Account profile header also needs custom presentation. |
| Sheet | Shared dismissal and focus management | Mandatory title bar with fixed spacing and Kit close button, body wrapper and 150ms slide animation. Cannot replace or hide the entire title bar through its public API. |
| Notification | Stack management, timed dismissal, hover/focus pause and multiple placements | Styled root and custom content are available, but the internal close button is always emitted, positioned top-right and visible only on hover. It uses the Kit icon. Cadence has an always-visible inline Solar close button. Animation also changes the appearance over time. |

Sources: [PopupMenu render and API](https://github.com/longbridge/gpui-kit/blob/v0.6.1/crates/component/src/menu/popup_menu.rs), [Sheet render and API](https://github.com/longbridge/gpui-kit/blob/v0.6.1/crates/component/src/sheet.rs), [Notification render and API](https://github.com/longbridge/gpui-kit/blob/v0.6.1/crates/component/src/notification.rs), [Cadence shared visuals](../../src/app/components.rs).

## Acceptance

For any chosen migration, compare fixed native screenshots before and after in both appearances, at the same window size, scale, data and animation phase. Include open, hovered, focused and pressed states. Preserve existing tooltip presence, colors and motion. Headless tests verify interaction and layout contracts but do not establish pixel parity. Check modal focus, dismissal callback count and restoration separately from the visual comparison.
