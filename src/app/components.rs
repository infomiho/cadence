use super::*;

/// Shared building blocks for Cadence views.
///
/// These are free functions rather than methods so that every view entity can
/// reach them, and they take the palette by value so a view always draws with
/// the appearance resolved for the frame it is rendering.
pub(super) fn button(palette: CadencePalette, id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id)
        .tab_stop(true)
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .focus(|style| {
            style
                .border_2()
                .border_color(rgb(palette.focus_ring))
                .rounded(px(12.))
        })
}

pub(super) fn icon(name: &'static str, size: f32, color: u32) -> Icon {
    Icon::new(name)
        .with_size(px(size))
        .text_color(color)
        .weight(SymbolWeight::Semibold)
        .symbol_scale(SymbolScale::Large)
        .rendering_mode(RenderingMode::Monochrome)
}

pub(super) fn pill(
    palette: CadencePalette,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    primary: bool,
) -> Stateful<Div> {
    let (background, foreground) = if primary {
        (rgb(palette.text_primary), rgb(palette.surface))
    } else {
        (rgb(palette.control), rgb(palette.text_primary))
    };
    button(palette, id)
        .h(px(40.))
        .px(px(16.))
        .rounded(px(40.))
        .bg(background)
        .text_color(foreground)
        .text_size(px(15.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .hover(move |style| {
            style.bg(if primary {
                rgb(palette.accent_hover)
            } else {
                rgb(palette.control_hover)
            })
        })
        .child(label.into())
}

pub(super) fn icon_button(
    palette: CadencePalette,
    id: impl Into<ElementId>,
    name: &'static str,
) -> Stateful<Div> {
    icon_button_with(palette, id, name, 17., SymbolWeight::Semibold)
}

pub(super) fn icon_button_with(
    palette: CadencePalette,
    id: impl Into<ElementId>,
    name: &'static str,
    size: f32,
    weight: SymbolWeight,
) -> Stateful<Div> {
    button(palette, id)
        .size(px(40.))
        .flex_none()
        .rounded(px(20.))
        .text_color(rgb(palette.text_primary))
        .hover(|style| style.bg(rgb(palette.control)))
        .active(|style| style.bg(rgb(palette.control_hover)))
        .child(icon(name, size, palette.text_primary).weight(weight))
}

pub(super) fn menu_item(
    palette: CadencePalette,
    id: impl Into<ElementId>,
    name: &'static str,
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
        .h(px(36.))
        .px(px(10.))
        .justify_start()
        .gap(px(10.))
        .rounded(px(8.))
        .text_size(px(13.))
        .text_color(rgb(color))
        .hover(|style| style.bg(rgb(palette.control_hover)))
        .child(icon(name, 15., color))
        .child(label)
}

pub(super) fn text_menu_item(
    palette: CadencePalette,
    id: impl Into<ElementId>,
    label: &'static str,
) -> Stateful<Div> {
    button(palette, id)
        .w_full()
        .h(px(36.))
        .px(px(12.))
        .justify_start()
        .rounded(px(8.))
        .text_size(px(13.))
        .text_color(rgb(palette.text))
        .hover(|style| style.bg(rgb(palette.control_hover)))
        .child(label)
}

pub(super) fn menu_surface(palette: CadencePalette) -> Div {
    div()
        .occlude()
        .w(px(220.))
        .p(px(6.))
        .rounded(px(14.))
        .bg(rgb(palette.surface_raised))
        .border_1()
        .border_color(rgb(palette.border))
        .shadow_lg()
        .flex()
        .flex_col()
        .text_size(px(13.))
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
        .text_size(px(13.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(rgb(palette.text_muted))
        .child(text.into())
}

pub(super) fn empty_state(palette: CadencePalette, text: impl Into<SharedString>) -> Div {
    div()
        .p(px(24.))
        .rounded(px(20.))
        .border_1()
        .border_color(rgb(palette.border))
        .text_color(rgb(palette.text_muted))
        .child(text.into())
}

pub(super) fn artwork(
    palette: CadencePalette,
    image_cache: &Entity<image_cache::BoundedImageCache>,
    url: Option<&str>,
    size: f32,
    radius: f32,
    fallback_icon: &'static str,
) -> gpui::AnyElement {
    let frame = div()
        .size(px(size))
        .flex_none()
        .rounded(px(radius))
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
                    .rounded(px(radius))
                    .object_fit(gpui::ObjectFit::Cover),
            )
            .into_any_element()
    } else {
        frame
            .flex()
            .items_center()
            .justify_center()
            .child(icon(fallback_icon, size * 0.3, palette.text_primary))
            .into_any_element()
    }
}

pub(super) fn profile_avatar(
    url: Option<&str>,
    initials: impl Into<SharedString>,
) -> gpui::AnyElement {
    let avatar = Avatar::new().with_size(px(40.)).border_0().name(initials);
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
