use super::*;
use gpui_kit::component::theme::{ThemeColor, ThemeTokens};

/// The resolved look of the app, shared by every view.
///
/// Views read the palette here at render time rather than holding a copy, so a
/// theme change reaches all of them without any of them going stale.
pub(super) struct Appearance {
    preference: ThemePreference,
    palette: CadencePalette,
}

impl gpui_kit::Global for Appearance {}

impl Appearance {
    /// Resolves the appearance for `window`, adopting the stored preference the
    /// first time. A window opened later joins the appearance already in use
    /// rather than resetting it to whatever was on disk at launch.
    pub(super) fn attach(window: &mut Window, cx: &mut App) {
        if !cx.has_global::<Self>() {
            let preference = services::AppServices::preferences(cx).theme;
            cx.set_global(Self {
                preference,
                palette: palette_for(false),
            });
        }
        let dark_mode = resolve_dark_mode(Self::preference(cx), window.appearance());
        cx.global_mut::<Self>().palette = palette_for(dark_mode);
        apply_theme_mode(dark_mode, window, cx);
    }

    pub(super) fn palette(cx: &App) -> CadencePalette {
        cx.global::<Self>().palette
    }

    pub(super) fn preference(cx: &App) -> ThemePreference {
        cx.global::<Self>().preference
    }

    pub(super) fn set_preference(preference: ThemePreference, window: &mut Window, cx: &mut App) {
        let dark_mode = resolve_dark_mode(preference, window.appearance());
        let appearance = cx.global_mut::<Self>();
        appearance.preference = preference;
        appearance.palette = palette_for(dark_mode);
        apply_theme_mode(dark_mode, window, cx);
    }

    /// Re-resolves against the system appearance, for when it changes underneath
    /// us. Reports whether the preference actually follows the system.
    pub(super) fn follow_system(window: &mut Window, cx: &mut App) -> bool {
        if cx.global::<Self>().preference != ThemePreference::System {
            return false;
        }
        let dark_mode = is_dark_appearance(window.appearance());
        cx.global_mut::<Self>().palette = palette_for(dark_mode);
        apply_theme_mode(dark_mode, window, cx);
        true
    }
}

fn palette_for(dark_mode: bool) -> CadencePalette {
    if dark_mode {
        CadencePalette::DARK
    } else {
        CadencePalette::LIGHT
    }
}

fn apply_theme_mode(dark_mode: bool, window: &mut Window, cx: &mut App) {
    let mode = if dark_mode {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    Theme::change(mode, None, cx);
    let theme = Theme::global_mut(cx);
    project_palette(palette_for(dark_mode), &mut theme.colors);
    theme.tokens = ThemeTokens::from(&theme.colors);
    Theme::sync_base(cx);
    window.refresh();
}

/// Paints the component library's controls with Cadence's roles, so an input,
/// switch or select sits in a Cadence view without its own palette.
fn project_palette(palette: CadencePalette, colors: &mut ThemeColor) {
    colors.background = rgb(palette.canvas).into();
    colors.foreground = rgb(palette.text_primary).into();
    colors.muted_foreground = rgb(palette.text_muted).into();
    colors.border = rgb(palette.border).into();
    colors.input = rgb(palette.border).into();
    colors.ring = rgb(palette.focus_ring).into();
    colors.primary = rgb(palette.text_primary).into();
    colors.primary_hover = rgb(palette.accent_hover).into();
    colors.primary_foreground = rgb(palette.on_accent).into();
    colors.secondary = rgb(palette.control).into();
    colors.secondary_hover = rgb(palette.control_hover).into();
    colors.secondary_foreground = rgb(palette.text_primary).into();
    colors.danger = rgb(palette.destructive).into();
    colors.danger_foreground = rgb(palette.on_destructive).into();
    colors.popover = rgb(palette.surface_raised).into();
    colors.popover_foreground = rgb(palette.text).into();
    colors.accent = rgb(palette.control_hover).into();
    colors.list_hover = rgb(palette.surface_hover).into();
    colors.list_active = rgb(palette.selection).into();
    colors.link = rgb(palette.link).into();
}
