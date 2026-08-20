use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    ops::Range,
    rc::Rc,
    sync::Arc,
    time::{Duration, SystemTime},
};

use gpui::{
    Animation, AnimationExt as _, AnyElement, App, Application, Bounds, ClipboardItem, Context,
    Corner, Div, ElementId, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, KeyBinding,
    Pixels, RenderOnce, SharedString, Stateful, Subscription, Window, WindowAppearance,
    WindowBounds, WindowOptions, actions, anchored, deferred, div, ease_out_quint, img, point,
    prelude::*, px, relative, rgb, size, uniform_list,
};
use gpui_component::{
    Root, Sizable, Theme, WindowExt,
    avatar::Avatar,
    input::{Input, InputEvent, InputState},
    spinner::Spinner,
    theme::ThemeMode,
};
use gpui_symbols::{Icon, RenderingMode, SymbolScale, SymbolWeight};
use spotify_gpui_client::{
    backend::{Backend, BackendCommand, BackendEvent, BackendHandle, Reply},
    lifecycle::{Instance, InstanceLifecycle},
    model,
    spotify::{ClientIdSource, valid_client_id},
    storage::{AppPreferences, Store, ThemePreference},
};

use library_pages::LibrarySection;
use workspace::Workspace;

mod http;
mod image_cache;

actions!(
    cadence,
    [
        Tab,
        TabPrev,
        OpenSearch,
        TogglePlayback,
        Quit,
        CloseWindow,
        DismissOverlay,
        NoOp
    ]
);

#[derive(Clone, Copy)]
struct CadencePalette {
    canvas: u32,
    surface: u32,
    surface_raised: u32,
    surface_hover: u32,
    control: u32,
    control_hover: u32,
    selection: u32,
    text_primary: u32,
    text: u32,
    text_muted: u32,
    border: u32,
    focus_ring: u32,
    danger: u32,
    destructive: u32,
    on_destructive: u32,
    scrim: gpui::Hsla,
    link: u32,
    accent_hover: u32,
    on_accent: u32,
    media_border: gpui::Hsla,
}

impl CadencePalette {
    const LIGHT: Self = Self {
        canvas: 0xFBFAF9,
        surface: 0xFFFFFF,
        surface_raised: 0xF2F0ED,
        surface_hover: 0xF8F7F4,
        control: 0xF6F4EF,
        control_hover: 0xEAE6DD,
        selection: 0xD8ECFC,
        text_primary: 0x171717,
        text: 0x494440,
        text_muted: 0x757373,
        border: 0xE8E8E8,
        focus_ring: 0x848281,
        danger: 0xEF4444,
        destructive: 0xB42318,
        on_destructive: 0xFFFFFF,
        scrim: gpui::Hsla {
            h: 0.,
            s: 0.,
            l: 0.,
            a: 0.32,
        },
        link: 0x0066CC,
        accent_hover: 0x121212,
        on_accent: 0xFFFFFF,
        media_border: gpui::Hsla {
            h: 0.,
            s: 0.,
            l: 0.,
            a: 0.1,
        },
    };

    const DARK: Self = Self {
        canvas: 0x121212,
        surface: 0x1A1A1A,
        surface_raised: 0x292929,
        surface_hover: 0x242424,
        control: 0x303030,
        control_hover: 0x404040,
        selection: 0x183B56,
        text_primary: 0xF5F3EF,
        text: 0xD5D1CB,
        text_muted: 0xA09D99,
        border: 0x414141,
        focus_ring: 0xA8A5A1,
        danger: 0xF87171,
        destructive: 0xFF6961,
        on_destructive: 0x171717,
        scrim: gpui::Hsla {
            h: 0.,
            s: 0.,
            l: 0.,
            a: 0.56,
        },
        link: 0x2997FF,
        accent_hover: 0xFFFFFF,
        on_accent: 0x171717,
        media_border: gpui::Hsla {
            h: 0.,
            s: 0.,
            l: 1.,
            a: 0.12,
        },
    };
}

fn is_dark_appearance(appearance: WindowAppearance) -> bool {
    matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

fn resolve_dark_mode(preference: ThemePreference, appearance: WindowAppearance) -> bool {
    match preference {
        ThemePreference::System => is_dark_appearance(appearance),
        ThemePreference::Light => false,
        ThemePreference::Dark => true,
    }
}
const VOLUME_SLIDER_WIDTH: f32 = 120.;
const VOLUME_SLIDER_RIGHT_INSET: f32 = 144.;
const PLAYER_LEFT_WIDTH: f32 = 360.;
const PLAYER_CENTER_WIDTH: f32 = 440.;
const PLAYER_RIGHT_WIDTH: f32 = 240.;
const PROGRESS_SLIDER_WIDTH: f32 = 340.;
const PROGRESS_TIME_WIDTH: f32 = 36.;
const PROGRESS_GAP: f32 = 8.;
const COMPACT_BREAKPOINT: f32 = 960.;
const COMPACT_PLAYER_BREAKPOINT: f32 = 1136.;
/// Sized so the rail centre sits on the traffic-light cluster axis.
const COLLAPSED_SIDEBAR_WIDTH: f32 = 78.;
/// The sidebar container's padding; the top is overridden to clear the
/// traffic lights.
const SIDEBAR_CONTENT_PAD: f32 = 16.;
/// The brand row's expanded leading padding and its logo size.
const BRAND_ROW_PAD: f32 = 14.;
const BRAND_LOGO_SIZE: f32 = 32.;
/// A nav row's expanded leading padding, and the width of its glyph: the
/// glyph itself, not the 20pt box that holds it, which is the trap here.
const NAV_ROW_PAD: f32 = 12.;
const NAV_GLYPH_WIDTH: f32 = 17.;
/// Measured centre of the macOS traffic-light cluster with this window style
/// (close button at x 9, cluster spanning 9..69 on macOS 26). OS-version
/// dependent; the tests guard our constants against each other, not AppKit.
#[cfg(all(test, target_os = "macos"))]
const TRAFFIC_LIGHT_CLUSTER_CENTRE: f32 = 39.;
/// The hover-and-selection pill behind a collapsed sidebar row.
const SIDEBAR_FILL_COLLAPSED: f32 = 42.;
/// How far the collapsed pill sits in from the row's left edge.
const SIDEBAR_FILL_INSET: f32 = 2.;
const CATALOG_STALE_TIME: Duration = Duration::from_secs(5 * 60);
const COMPACT_PLAYER_LEFT_WIDTH: f32 = 220.;
const COMPACT_PLAYER_RIGHT_WIDTH: f32 = 96.;

fn sidebar_transition_duration(
    current_width: f32,
    target_width: f32,
    expanded_width: f32,
) -> Duration {
    let remaining_fraction = ((target_width - current_width).abs()
        / (expanded_width - COLLAPSED_SIDEBAR_WIDTH))
        .clamp(0., 1.);
    Duration::from_millis((180. * remaining_fraction).round().max(60.) as u64)
}

fn interpolate_sidebar_width(from: f32, target: f32, delta: f32) -> f32 {
    from + (target - from) * delta
}

/// Leading padding that puts a row's leading content on the collapsed rail
/// axis at progress 0 and back on its expanded padding at progress 1.
fn sidebar_row_pad(expanded_pad: f32, content_width: f32, progress: f32) -> f32 {
    let collapsed = COLLAPSED_SIDEBAR_WIDTH / 2. - SIDEBAR_CONTENT_PAD - content_width / 2.;
    collapsed + (expanded_pad - collapsed) * progress
}

/// The pill behind a sidebar row: a content-hugging box when collapsed, the
/// full row when expanded. Returns (width, left inset, leading padding).
fn sidebar_fill_geometry(
    expanded_pad: f32,
    content_width: f32,
    row_width: f32,
    progress: f32,
) -> (f32, f32, f32) {
    let left = SIDEBAR_FILL_INSET * (1. - progress);
    let width = SIDEBAR_FILL_COLLAPSED + (row_width - SIDEBAR_FILL_COLLAPSED) * progress;
    let pad = sidebar_row_pad(expanded_pad, content_width, progress) - left;
    (width, left, pad)
}

fn uses_compact_content_layout(window_width: f32) -> bool {
    window_width < COMPACT_BREAKPOINT
}

fn uses_compact_player_layout(window_width: f32) -> bool {
    window_width < COMPACT_PLAYER_BREAKPOINT
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    LikedSongs,
    Favorites,
    Recent,
    Search,
    Playlists,
    Playlist,
    Artist,
    Album,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchKind {
    Tracks,
    Playlists,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArtistSection {
    Popular,
    Discography,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConnectionState {
    Starting,
    Failed,
    SetupRequired,
    AuthorizationRequired,
    Connecting,
    Ready,
}

fn volume_for_pointer(pointer_x: f32, window_width: f32) -> f32 {
    ((pointer_x - (window_width - VOLUME_SLIDER_RIGHT_INSET)) / VOLUME_SLIDER_WIDTH).clamp(0., 1.)
}

fn seek_for_pointer(pointer_x: f32, window_width: f32, duration_ms: u32) -> u32 {
    let (center_left, slider_width) = if uses_compact_player_layout(window_width) {
        (24. + COMPACT_PLAYER_LEFT_WIDTH + 24., window_width - 500.)
    } else {
        (
            window_width / 2. + (PLAYER_LEFT_WIDTH - PLAYER_RIGHT_WIDTH - PLAYER_CENTER_WIDTH) / 2.,
            PROGRESS_SLIDER_WIDTH,
        )
    };
    let left = center_left + PROGRESS_TIME_WIDTH + PROGRESS_GAP;
    let fraction = ((pointer_x - left) / slider_width).clamp(0., 1.);
    (fraction * duration_ms as f32) as u32
}

fn format_duration(duration_ms: u32) -> String {
    let seconds = duration_ms / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn next_request_id(request_id: &mut u64) -> u64 {
    *request_id = request_id.wrapping_add(1);
    *request_id
}

/// Wall clock, not `Instant`: `Instant` does not advance while the machine is
/// asleep, so a sleep would leave stale data looking fresh.
fn catalog_data_is_fresh(loaded_at: Option<SystemTime>) -> bool {
    loaded_at.is_some_and(|loaded_at| {
        loaded_at
            .elapsed()
            .is_ok_and(|elapsed| elapsed < CATALOG_STALE_TIME)
    })
}

fn index_favorites(favorites: &[model::Track]) -> HashMap<model::Provider, HashSet<String>> {
    let mut index: HashMap<model::Provider, HashSet<String>> = HashMap::new();
    for track in favorites {
        index
            .entry(track.provider)
            .or_default()
            .insert(track.source_id.clone());
    }
    index
}

async fn receive_backend_event_batch(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<BackendEvent>,
) -> Option<Vec<BackendEvent>> {
    let first = events.recv().await?;
    let mut batch = vec![first];
    while let Ok(event) = events.try_recv() {
        batch.push(event);
    }
    Some(batch)
}

/// Stream of backend events from the worker thread.
type BackendEvents = tokio::sync::mpsc::UnboundedReceiver<BackendEvent>;

mod actions;
mod appearance;
mod bootstrap;
mod catalog;
mod chrome;
mod components;
mod events;
mod library;
mod library_pages;
mod media_controls;
mod onboarding;
mod page;
mod player;
mod player_bar;
mod router;
mod services;
mod session;
mod settings;
mod sidebar;
mod track_list;
mod track_row;
mod windows;
mod workspace;

#[cfg(test)]
mod event_bridge_tests {
    use super::{
        BackendEvent, CATALOG_STALE_TIME, catalog_data_is_fresh, index_favorites, model,
        next_request_id, receive_backend_event_batch,
    };
    use std::time::{Duration, SystemTime};

    fn track(provider: model::Provider, source_id: &str) -> model::Track {
        model::Track {
            provider,
            source_id: source_id.to_owned(),
            spotify_uri: None,
            isrc: None,
            title: source_id.to_owned(),
            artist: "Artist".to_owned(),
            artists: Vec::new(),
            album: "Album".to_owned(),
            album_ref: None,
            duration_ms: 1,
            artwork_url: None,
        }
    }

    #[tokio::test]
    async fn batches_events_that_are_already_queued() {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        sender.send(BackendEvent::SetupRequired).unwrap();
        sender
            .send(BackendEvent::CatalogReady { generation: 0 })
            .unwrap();

        let events = receive_backend_event_batch(&mut receiver).await.unwrap();

        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], BackendEvent::SetupRequired));
        assert!(matches!(
            events[1],
            BackendEvent::CatalogReady { generation: 0 }
        ));
    }

    #[tokio::test]
    async fn closes_after_all_senders_are_dropped() {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        drop(sender);

        assert!(receive_backend_event_batch(&mut receiver).await.is_none());
    }

    #[test]
    fn generations_advance_with_wrapping_request_ids() {
        let mut generation = u64::MAX;
        assert_eq!(next_request_id(&mut generation), 0);
        assert_eq!(next_request_id(&mut generation), 1);
    }

    #[test]
    fn catalog_data_expires_after_the_stale_time() {
        assert!(!catalog_data_is_fresh(None));
        assert!(catalog_data_is_fresh(Some(SystemTime::now())));
        assert!(!catalog_data_is_fresh(Some(
            SystemTime::now() - CATALOG_STALE_TIME - Duration::from_secs(1)
        )));
    }

    #[test]
    fn favorite_index_separates_providers_and_deduplicates_tracks() {
        let spotify = track(model::Provider::Spotify, "same-id");
        let tidal = track(model::Provider::Tidal, "same-id");

        let index = index_favorites(&[spotify.clone(), spotify, tidal]);

        assert_eq!(index[&model::Provider::Spotify].len(), 1);
        assert!(index[&model::Provider::Spotify].contains("same-id"));
        assert!(index[&model::Provider::Tidal].contains("same-id"));
    }
}

pub fn run() {
    bootstrap::run();
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::{
        BRAND_LOGO_SIZE, BRAND_ROW_PAD, COLLAPSED_SIDEBAR_WIDTH, NAV_GLYPH_WIDTH, NAV_ROW_PAD,
        SIDEBAR_CONTENT_PAD, SIDEBAR_FILL_COLLAPSED, SIDEBAR_FILL_INSET,
        TRAFFIC_LIGHT_CLUSTER_CENTRE, interpolate_sidebar_width, resolve_dark_mode,
        seek_for_pointer, sidebar_fill_geometry, sidebar_row_pad, sidebar_transition_duration,
        uses_compact_content_layout, uses_compact_player_layout, volume_for_pointer,
    };
    use gpui::WindowAppearance;
    use gpui_symbols::SfSymbol;
    use spotify_gpui_client::storage::ThemePreference;

    #[test]
    fn theme_preference_resolves_against_window_appearance() {
        assert!(resolve_dark_mode(
            ThemePreference::System,
            WindowAppearance::Dark
        ));
        assert!(!resolve_dark_mode(
            ThemePreference::System,
            WindowAppearance::Light
        ));
        assert!(!resolve_dark_mode(
            ThemePreference::Light,
            WindowAppearance::Dark
        ));
        assert!(resolve_dark_mode(
            ThemePreference::Dark,
            WindowAppearance::Light
        ));
    }

    #[test]
    fn pointer_position_is_clamped_to_volume_range() {
        let window_width = 1280.;

        assert_eq!(volume_for_pointer(1100., window_width), 0.);
        assert_eq!(volume_for_pointer(1196., window_width), 0.5);
        assert_eq!(volume_for_pointer(1300., window_width), 1.);
    }

    #[test]
    fn pointer_position_is_mapped_to_track_duration() {
        assert_eq!(seek_for_pointer(524., 1280., 200_000), 0);
        assert_eq!(seek_for_pointer(694., 1280., 200_000), 100_000);
        assert_eq!(seek_for_pointer(864., 1280., 200_000), 200_000);
        assert_eq!(seek_for_pointer(312., 720., 200_000), 0);
        assert_eq!(seek_for_pointer(422., 720., 200_000), 100_000);
        assert_eq!(seek_for_pointer(532., 720., 200_000), 200_000);
        assert_eq!(seek_for_pointer(541.5, 959., 200_000), 100_000);
        assert_eq!(seek_for_pointer(542., 960., 200_000), 100_000);
        assert_eq!(seek_for_pointer(629.5, 1135., 200_000), 100_000);
        assert_eq!(seek_for_pointer(622., 1136., 200_000), 100_000);
    }

    #[test]
    fn responsive_breakpoints_are_exclusive() {
        assert!(uses_compact_content_layout(959.));
        assert!(!uses_compact_content_layout(960.));
        assert!(uses_compact_player_layout(1135.));
        assert!(!uses_compact_player_layout(1136.));
    }

    #[test]
    fn sidebar_reversal_starts_from_the_sampled_width() {
        assert_eq!(interpolate_sidebar_width(150., 232., 0.), 150.);
        assert_eq!(interpolate_sidebar_width(150., 232., 1.), 232.);
        assert_eq!(interpolate_sidebar_width(150., 72., 0.), 150.);
    }

    #[test]
    fn sidebar_transition_duration_scales_with_remaining_distance() {
        assert_eq!(
            sidebar_transition_duration(COLLAPSED_SIDEBAR_WIDTH, 232., 232.).as_millis(),
            180
        );
        assert_eq!(
            sidebar_transition_duration(155., 232., 232.).as_millis(),
            90
        );
        assert_eq!(
            sidebar_transition_duration(220., 232., 232.).as_millis(),
            60
        );
    }

    #[test]
    fn collapsed_sidebar_row_pads_match_the_verified_geometry() {
        // The measured values from cadence-5ym; a change to the rail width,
        // the content pad, or a row's content size must be a conscious one.
        assert_eq!(sidebar_row_pad(BRAND_ROW_PAD, BRAND_LOGO_SIZE, 0.), 7.);
        assert_eq!(sidebar_row_pad(NAV_ROW_PAD, NAV_GLYPH_WIDTH, 0.), 14.5);
    }

    #[test]
    fn expanded_sidebar_rows_keep_their_padding() {
        assert_eq!(
            sidebar_row_pad(BRAND_ROW_PAD, BRAND_LOGO_SIZE, 1.),
            BRAND_ROW_PAD
        );
        assert_eq!(
            sidebar_row_pad(NAV_ROW_PAD, NAV_GLYPH_WIDTH, 1.),
            NAV_ROW_PAD
        );
    }

    #[test]
    fn traffic_lights_sit_on_the_collapsed_rail_axis() {
        assert_eq!(COLLAPSED_SIDEBAR_WIDTH / 2., TRAFFIC_LIGHT_CLUSTER_CENTRE);
    }

    #[test]
    fn collapsed_fill_centres_on_the_rail_axis() {
        // The pill hugs its content symmetrically only while its own centre
        // sits on the rail axis.
        assert_eq!(
            SIDEBAR_CONTENT_PAD + SIDEBAR_FILL_INSET + SIDEBAR_FILL_COLLAPSED / 2.,
            COLLAPSED_SIDEBAR_WIDTH / 2.
        );
    }

    #[test]
    fn sidebar_fill_hugs_content_collapsed_and_spans_the_row_expanded() {
        let (width, left, pad) = sidebar_fill_geometry(NAV_ROW_PAD, NAV_GLYPH_WIDTH, 200., 0.);
        assert_eq!((width, left), (SIDEBAR_FILL_COLLAPSED, SIDEBAR_FILL_INSET));
        assert_eq!(
            SIDEBAR_CONTENT_PAD + left + pad + NAV_GLYPH_WIDTH / 2.,
            TRAFFIC_LIGHT_CLUSTER_CENTRE
        );

        let (width, left, pad) = sidebar_fill_geometry(NAV_ROW_PAD, NAV_GLYPH_WIDTH, 200., 1.);
        assert_eq!((width, left, pad), (200., 0., NAV_ROW_PAD));
    }

    #[test]
    fn all_used_symbols_are_available() {
        let symbols = [
            "waveform",
            "heart",
            "heart.fill",
            "star",
            "star.fill",
            "clock",
            "clock.fill",
            "magnifyingglass",
            "music.note.list",
            "music.note",
            "person.fill",
            "chevron.left",
            "xmark",
            "backward.end.fill",
            "pause.fill",
            "play.fill",
            "forward.end.fill",
            "list.bullet",
            "key",
            "checkmark",
            "gearshape",
            "rectangle.portrait.and.arrow.right",
            "pin",
            "pin.fill",
            "speaker.slash.fill",
            "speaker.wave.2.fill",
            "ellipsis",
            "circle.lefthalf.filled",
            "sun.max",
            "moon",
        ];

        for symbol in symbols {
            assert!(
                SfSymbol::new(symbol).size(18.).render_rgba().is_some(),
                "SF Symbol `{symbol}` is unavailable"
            );
        }
    }
}
