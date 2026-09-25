//! Shared building blocks for Cadence views.
//!
//! These are free functions rather than methods so that every view entity can
//! reach them, and they take the palette by value so a view always draws with
//! the appearance resolved for the frame it is rendering.

use super::*;

/// The keyboard focus ring's stroke, the same physical width as `border_2`.
const FOCUS_RING_WIDTH: Pixels = px(2.);

/// Makes an identified element a control the keyboard can reach: a Tab stop
/// that Enter and Space activate. Like a macOS button, it does not take
/// keyboard focus from a pointer press, so Space after a click still controls
/// playback.
fn keyboard_control(element: Stateful<Div>) -> Stateful<Div> {
    element
        .focusable()
        .tab_stop(true)
        .key_context(CONTROL_KEY_CONTEXT)
        .on_mouse_down(gpui_kit::MouseButton::Left, |_, window, _| {
            window.prevent_default();
        })
}

/// A keyboard control that draws the focus ring inside its own edge, so the
/// ring follows the control's radius, takes no layout space and no clipping
/// ancestor can hide it.
pub(super) fn control(palette: CadencePalette, element: Stateful<Div>) -> Stateful<Div> {
    keyboard_control(element).focus_visible(|style| style.shadow(inset_focus_ring(palette)))
}

/// A centered control without a drawn focus state, for a control that shows
/// focus on an inner surface.
pub(super) fn bare_button(id: impl Into<ElementId>) -> Stateful<Div> {
    keyboard_control(div().id(id))
        .flex()
        .items_center()
        .justify_center()
}

pub(super) fn button(palette: CadencePalette, id: impl Into<ElementId>) -> Stateful<Div> {
    control(palette, div().id(id))
        .flex()
        .items_center()
        .justify_center()
}

/// A button whose own fill covers it. The ring sits outside the edge, where it
/// contrasts with the surface around the button rather than with the fill.
pub(super) fn filled_button(palette: CadencePalette, id: impl Into<ElementId>) -> Stateful<Div> {
    bare_button(id).focus_visible(|style| style.shadow(outset_focus_ring(palette)))
}

pub(super) fn inset_focus_ring(palette: CadencePalette) -> Vec<gpui_kit::BoxShadow> {
    vec![focus_ring_shadow(palette).inset()]
}

/// Painted under the element, so only an element with an opaque fill shows it
/// as a ring.
fn outset_focus_ring(palette: CadencePalette) -> Vec<gpui_kit::BoxShadow> {
    vec![focus_ring_shadow(palette)]
}

fn focus_ring_shadow(palette: CadencePalette) -> gpui_kit::BoxShadow {
    gpui_kit::BoxShadow::new(px(0.), px(0.), rgb(palette.focus_ring).into())
        .spread_radius(FOCUS_RING_WIDTH)
}

/// Whether a control that draws its own ring should show it: focused, and
/// reached from the keyboard.
pub(super) fn is_focus_visible(focus_handle: &FocusHandle, window: &Window) -> bool {
    focus_handle.is_focused(window) && window.last_input_was_keyboard()
}

/// The bordered frame of a text field, ringed while `input` has focus, however
/// focus got there, as a macOS text field does. The frame keeps its one-pixel
/// border and the ring grows inward from it, so the text does not move.
pub(super) fn text_field_frame(
    palette: CadencePalette,
    input: &Entity<InputState>,
    window: &Window,
    cx: &App,
) -> Div {
    let focused = input.read(cx).focus_handle(cx).is_focused(window);
    div()
        .flex()
        .items_center()
        .px_3p5()
        .rounded_xl()
        .border_1()
        .border_color(rgb(palette.border))
        .bg(rgb(palette.surface))
        .when(focused, |frame| {
            frame
                .border_color(rgb(palette.focus_ring))
                .shadow(inset_focus_ring(palette))
        })
}

/// A button that sits flush with the content around it: its padding makes
/// room for the focus ring and its negative margin takes that room back.
pub(super) fn flush_button(palette: CadencePalette, id: impl Into<ElementId>) -> Stateful<Div> {
    button(palette, id)
        .px(tokens::FOCUS_RING_CLEARANCE)
        .mx(-tokens::FOCUS_RING_CLEARANCE)
}

/// Rings a component that draws no focus state of its own while the keyboard
/// focus is inside it. The wrapper tracks `focus_handle` only to observe that
/// focus, so it must not be a Tab stop itself.
pub(super) fn focus_ring_around(
    palette: CadencePalette,
    focus_handle: &FocusHandle,
    window: &Window,
    cx: &App,
    component: impl IntoElement,
) -> Div {
    let focus_visible = is_focus_visible_within(focus_handle, window, cx);
    div()
        .track_focus(focus_handle)
        .key_context(CONTROL_KEY_CONTEXT)
        .rounded_full()
        .when(focus_visible, |wrapper| {
            wrapper.shadow(outset_focus_ring(palette))
        })
        .child(component)
}

/// Whether [`focus_ring_around`] draws its ring for `focus_handle`.
pub(super) fn is_focus_visible_within(
    focus_handle: &FocusHandle,
    window: &Window,
    cx: &App,
) -> bool {
    focus_handle.contains_focused(window, cx) && window.last_input_was_keyboard()
}

/// A link that reads like the copy around it until the pointer or keyboard
/// focus reaches it, so a line of credits stays quiet at rest.
///
/// Built on the base button with the link role because gpui-kit 0.6.6 cannot
/// observe its base link in tests. The button's neutral line height is set
/// back to the window's so the text keeps its metrics.
pub(super) fn link(
    palette: CadencePalette,
    id: impl Into<ElementId>,
    window: &Window,
) -> gpui_kit::base::Button {
    gpui_kit::base::Button::new(id)
        .role(gpui_kit::Role::Link)
        .line_height(window.text_style().line_height)
        .cursor_pointer()
        .hover(|style| style.underline().text_color(rgb(palette.text_primary)))
        .focus_visible(|style| style.underline().text_color(rgb(palette.text_primary)))
}

/// The transient banner for things that finished without a page to say so.
pub(super) fn action_notice_banner(
    palette: CadencePalette,
    message: SharedString,
    on_dismiss: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    deferred(
        div()
            .occlude()
            .absolute()
            .top(tokens::NOTICE_BANNER_TOP)
            .right_6()
            .w(tokens::NOTICE_BANNER_WIDTH)
            .min_h_12()
            .px_3p5()
            .py_2()
            .rounded(tokens::CONTAINER_RADIUS)
            .border_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.surface_raised))
            .shadow_lg()
            .flex()
            .items_center()
            .gap_2p5()
            .text_size(tokens::BODY_TEXT)
            .text_color(rgb(palette.text_primary))
            .child(div().flex_1().child(message))
            .child(
                icon_button(palette, "dismiss-action-notice", CadenceIcon::Close)
                    .size_8()
                    .on_click(on_dismiss),
            ),
    )
    .into_any_element()
}

pub(super) fn icon(glyph: CadenceIcon, size: Rems, color: u32) -> Icon {
    Icon::new(glyph).size(size).text_color(rgb(color))
}

pub(super) fn pill(
    palette: CadencePalette,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    primary: bool,
) -> Stateful<Div> {
    let hover_background = if primary {
        rgb(palette.accent_hover)
    } else {
        rgb(palette.control_hover)
    };
    pill_surface(palette, filled_button(palette, id), primary)
        .hover(move |style| style.bg(hover_background))
        .child(label.into())
}

/// A primary pill for an action already under way. It looks like [`pill`] at
/// rest but is not a control, so it is neither a Tab stop nor hoverable.
pub(super) fn pending_pill(
    palette: CadencePalette,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
) -> Stateful<Div> {
    let frame = div().id(id).flex().items_center().justify_center();
    pill_surface(palette, frame, true).child(label.into())
}

fn pill_surface(palette: CadencePalette, frame: Stateful<Div>, primary: bool) -> Stateful<Div> {
    let (background, foreground) = if primary {
        (rgb(palette.text_primary), rgb(palette.surface))
    } else {
        (rgb(palette.control), rgb(palette.text_primary))
    };
    frame
        .h_10()
        .px_4()
        .rounded_full()
        .bg(background)
        .text_color(foreground)
        .text_size(tokens::PILL_TEXT)
        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
}

pub(super) fn icon_button(
    palette: CadencePalette,
    id: impl Into<ElementId>,
    glyph: CadenceIcon,
) -> Stateful<Div> {
    icon_button_sized(palette, id, glyph, tokens::CONTROL_ICON)
}

pub(super) fn icon_button_sized(
    palette: CadencePalette,
    id: impl Into<ElementId>,
    glyph: CadenceIcon,
    size: Rems,
) -> Stateful<Div> {
    button(palette, id)
        .size_10()
        .flex_none()
        .rounded_full()
        .text_color(rgb(palette.text_primary))
        .hover(|style| style.bg(rgb(palette.control)))
        .active(|style| style.bg(rgb(palette.control_hover)))
        .child(icon(glyph, size, palette.text_primary))
}

pub(super) fn menu_item(
    palette: CadencePalette,
    id: impl Into<ElementId>,
    glyph: CadenceIcon,
    label: &'static str,
    destructive: bool,
) -> Stateful<Div> {
    let color = if destructive {
        palette.danger
    } else {
        palette.text
    };
    button(palette, id)
        .w_full()
        .h_9()
        .px_2p5()
        .justify_start()
        .gap_2p5()
        .rounded_lg()
        .text_size(tokens::BODY_TEXT)
        .text_color(rgb(color))
        .hover(|style| style.bg(rgb(palette.control_hover)))
        .child(icon(glyph, tokens::MENU_ICON, color))
        .child(label)
}

pub(super) fn text_menu_item(
    palette: CadencePalette,
    id: impl Into<ElementId>,
    label: &'static str,
) -> Stateful<Div> {
    text_menu_row(button(palette, id), label, palette.text)
        .hover(|style| style.bg(rgb(palette.control_hover)))
}

/// A menu item for an action that does not apply right now: muted, not
/// hoverable and not a Tab stop.
pub(super) fn disabled_text_menu_item(
    palette: CadencePalette,
    id: impl Into<ElementId>,
    label: &'static str,
) -> Stateful<Div> {
    let row = div().id(id).flex().items_center();
    text_menu_row(row, label, palette.text_muted)
}

fn text_menu_row(row: Stateful<Div>, label: &'static str, color: u32) -> Stateful<Div> {
    row.w_full()
        .h_9()
        .px_3()
        .justify_start()
        .rounded_lg()
        .text_size(tokens::BODY_TEXT)
        .text_color(rgb(color))
        .child(label)
}

/// The outlined secondary button used by the setup screens and settings.
pub(super) fn settings_button(
    palette: CadencePalette,
    id: impl Into<ElementId>,
    label: &'static str,
) -> Stateful<Div> {
    filled_button(palette, id)
        .h_10()
        .px_3p5()
        .rounded(tokens::CONTROL_RADIUS)
        .border_1()
        .border_color(rgb(palette.border))
        .bg(rgb(palette.surface))
        .text_size(tokens::BODY_TEXT)
        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
        .text_color(rgb(palette.text_primary))
        .hover(|style| style.bg(rgb(palette.control_hover)))
        .child(label)
}

pub(super) fn menu_surface(palette: CadencePalette) -> Div {
    div()
        .occlude()
        .w(tokens::MENU_WIDTH)
        .p_1p5()
        .rounded(tokens::CONTAINER_RADIUS)
        .bg(rgb(palette.surface_raised))
        .border_1()
        .border_color(rgb(palette.border))
        .shadow_lg()
        .flex()
        .flex_col()
        .text_size(tokens::BODY_TEXT)
}

/// A page subtitle that reports a background refresh without replacing the
/// contents already on screen.
pub(super) fn revalidating_detail(detail: impl Into<String>, refreshing: bool) -> String {
    let detail = detail.into();
    if refreshing {
        format!("{detail} · refreshing…")
    } else {
        detail
    }
}

pub(super) fn section_label(palette: CadencePalette, text: impl Into<SharedString>) -> Div {
    div()
        .text_size(tokens::BODY_TEXT)
        .font_weight(gpui_kit::FontWeight::MEDIUM)
        .text_color(rgb(palette.text_muted))
        .child(text.into())
}

pub(super) fn empty_state(palette: CadencePalette, text: impl Into<SharedString>) -> Div {
    div()
        .p_6()
        .rounded(tokens::PANEL_RADIUS)
        .border_1()
        .border_color(rgb(palette.border))
        .text_color(rgb(palette.text_muted))
        .child(text.into())
}

pub(super) fn artwork(
    palette: CadencePalette,
    image_cache: &Entity<image_cache::BoundedImageCache>,
    url: Option<&str>,
    artwork_size: tokens::ArtworkSize,
    fallback_icon: CadenceIcon,
) -> gpui_kit::AnyElement {
    let frame = div()
        .size(artwork_size.size)
        .flex_none()
        .rounded(artwork_size.radius)
        .overflow_hidden()
        .bg(rgb(palette.selection))
        .border_1()
        .border_color(palette.media_border);
    if let Some(url) = url {
        frame
            .child(
                img(url.to_owned())
                    .image_cache(image_cache)
                    .size_full()
                    .rounded(artwork_size.radius)
                    .object_fit(gpui_kit::ObjectFit::Cover),
            )
            .into_any_element()
    } else {
        frame
            .flex()
            .items_center()
            .justify_center()
            .child(icon(
                fallback_icon,
                artwork_size.size * 0.3,
                palette.text_primary,
            ))
            .into_any_element()
    }
}

pub(super) fn profile_avatar(
    url: Option<&str>,
    initials: impl Into<SharedString>,
    window: &Window,
) -> gpui_kit::AnyElement {
    let avatar_size = tokens::AVATAR_SIZE.to_pixels(window.rem_size());
    let avatar = Avatar::new()
        .with_size(avatar_size)
        .border_0()
        .name(initials);
    if let Some(url) = url {
        avatar.src(url.to_owned()).into_any_element()
    } else {
        avatar.into_any_element()
    }
}

pub(super) fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|part| part.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

/// The large title a page opens with.
pub(super) fn page_title(palette: CadencePalette, title: impl Into<SharedString>) -> Div {
    div()
        .text_size(tokens::PAGE_TITLE_TEXT)
        .line_height(tokens::PAGE_TITLE_LINE_HEIGHT)
        .font_weight(gpui_kit::FontWeight::MEDIUM)
        .text_color(rgb(palette.text_primary))
        .child(title.into())
}

/// The subtitle line under a page title.
pub(super) fn page_detail(palette: CadencePalette, detail: impl Into<SharedString>) -> Div {
    div()
        .text_sm()
        .text_color(rgb(palette.text_muted))
        .child(detail.into())
}

/// The title and subtitle every page opens with.
pub(super) fn page_heading(
    palette: CadencePalette,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
) -> Div {
    div()
        .flex()
        .items_end()
        .justify_between()
        .gap_6()
        .mb_6()
        .child(
            div()
                .flex()
                .flex_col()
                .gap(tokens::PAGE_HEADING_GAP)
                .child(page_title(palette, title))
                .child(page_detail(palette, detail)),
        )
}

/// The frame every page's contents sit in.
pub(super) fn page(id: impl Into<ElementId>) -> Stateful<Div> {
    div().id(id).size_full().min_h_0().flex().flex_col().p_8()
}
