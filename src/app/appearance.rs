use super::*;

/// The resolved look of the app, shared by every view.
///
/// Views read the palette here at render time rather than holding a copy, so a
/// theme change reaches all of them without any of them going stale.
pub(super) struct Appearance {
    preference: ThemePreference,
    palette: CadencePalette,
}

impl gpui::Global for Appearance {}

impl Appearance {
    /// Registers the appearance and applies it to the window that resolved it.
    pub(super) fn init(preference: ThemePreference, window: &mut Window, cx: &mut App) {
        let dark_mode = resolve_dark_mode(preference, window.appearance());
        cx.set_global(Self {
            preference,
            palette: palette_for(dark_mode),
        });
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
    Theme::change(
        if dark_mode {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
        Some(window),
        cx,
    );
}
