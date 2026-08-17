use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    ops::Range,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    Animation, AnimationExt as _, App, Application, Bounds, ClipboardItem, Context, Corner, Div,
    ElementId, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding, Pixels, SharedString,
    Stateful, Subscription, Window, WindowAppearance, WindowBounds, WindowOptions, actions,
    anchored, deferred, div, ease_out_quint, img, point, prelude::*, px, relative, rgb, size,
    uniform_list,
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
    backend::{Backend, BackendCommand, BackendEvent, BackendHandle},
    lifecycle::{Instance, InstanceLifecycle},
    model,
    spotify::{ClientIdSource, valid_client_id},
    storage::{AppPreferences, Store, ThemePreference},
};

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
        DismissOverlay
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
const CATALOG_STALE_TIME: Duration = Duration::from_secs(5 * 60);
const COMPACT_PLAYER_LEFT_WIDTH: f32 = 220.;
const COMPACT_PLAYER_RIGHT_WIDTH: f32 = 96.;

fn sidebar_transition_duration(
    current_width: f32,
    target_width: f32,
    expanded_width: f32,
) -> Duration {
    let remaining_fraction =
        ((target_width - current_width).abs() / (expanded_width - 72.)).clamp(0., 1.);
    Duration::from_millis((180. * remaining_fraction).round().max(60.) as u64)
}

fn interpolate_sidebar_width(from: f32, target: f32, delta: f32) -> f32 {
    from + (target - from) * delta
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

enum ConnectionState {
    Starting,
    Failed,
    SetupRequired,
    AuthorizationRequired,
    Connecting,
    Ready,
}

struct TrackActionContext {
    track: model::Track,
    playback_tracks: Arc<[model::Track]>,
    index: usize,
    favorite: bool,
    is_current_track: bool,
    has_playback_context: bool,
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

fn is_current_response(
    account_generation: u64,
    current_request_id: u64,
    response_generation: u64,
    response_request_id: u64,
) -> bool {
    account_generation == response_generation && current_request_id == response_request_id
}

fn catalog_data_is_fresh(loaded_at: Option<Instant>) -> bool {
    loaded_at.is_some_and(|loaded_at| loaded_at.elapsed() < CATALOG_STALE_TIME)
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

/// Stream of backend events awaiting a window to deliver them to.
type BackendEvents = tokio::sync::mpsc::UnboundedReceiver<BackendEvent>;

struct CadenceApp {
    backend: BackendHandle,
    account_generation: u64,
    connection_state: ConnectionState,
    last_error: Option<String>,
    action_notice: Option<String>,
    radio_request_id: u64,
    pending_radio_request: Option<u64>,
    search_results: Arc<[model::Track]>,
    search_playlists: Arc<[model::Playlist]>,
    search_loaded: bool,
    searching: bool,
    search_error: Option<String>,
    search_query: String,
    search_request_id: u64,
    search_input: gpui::Entity<InputState>,
    _search_subscription: Subscription,
    spotify_client_id_input: gpui::Entity<InputState>,
    _spotify_client_id_subscription: Subscription,
    spotify_client_id: Option<String>,
    spotify_client_id_source: Option<ClientIdSource>,
    spotify_setup_error: Option<String>,
    spotify_setup_needs_focus: bool,
    spotify_configuration_request_id: u64,
    pending_spotify_configuration: Option<u64>,
    spotify_configuration_blocked: bool,
    spotify_app_change_confirmation_open: bool,
    liked_tracks: Arc<[model::Track]>,
    library_loaded: bool,
    local_favorites: Arc<[model::Track]>,
    favorite_keys: HashMap<model::Provider, HashSet<String>>,
    pinned_playlists: Arc<[model::Playlist]>,
    local_state_loaded: bool,
    spotify_playlists: Arc<[model::Playlist]>,
    profile: Option<model::UserProfile>,
    recently_played: Arc<[model::Track]>,
    selected_spotify_playlist: Option<model::Playlist>,
    playlist_tracks: Arc<[model::Track]>,
    playlist_loaded: bool,
    playlist_error: Option<String>,
    playlist_request_id: u64,
    selected_artist_ref: Option<model::ArtistRef>,
    selected_artist: Option<model::Artist>,
    artist_tracks: Arc<[model::Track]>,
    artist_albums: Arc<[model::Album]>,
    artist_request_id: u64,
    artist_section: ArtistSection,
    artist_loaded: bool,
    artist_loading: bool,
    artist_error: Option<String>,
    artist_loaded_at: Option<Instant>,
    selected_album_ref: Option<model::AlbumRef>,
    selected_album: Option<model::Album>,
    album_tracks: Arc<[model::Track]>,
    album_request_id: u64,
    album_loaded: bool,
    album_loading: bool,
    album_error: Option<String>,
    album_loaded_at: Option<Instant>,
    player: Entity<player::Player>,
    player_bar: Entity<player_bar::PlayerBar>,
    queue_drawer: Entity<player_bar::QueueDrawer>,
    route: Route,
    focus_handle: FocusHandle,
    account_menu_open: bool,
    track_menu_open: Option<String>,
    playlist_origin: Route,
    artist_origin: Route,
    album_origin: Route,
    settings_origin: Route,
    search_kind: SearchKind,
    image_cache: Entity<image_cache::BoundedImageCache>,
    brand_mark: Arc<gpui::Image>,
    compact_layout: bool,
    sidebar_collapsed: bool,
    sidebar_transition_generation: u64,
    sidebar_visual_width: Rc<Cell<f32>>,
    sidebar_transition_from: f32,
    sidebar_transition_duration: Duration,
    palette: CadencePalette,
    _appearance_subscription: Subscription,
}

impl CadenceApp {
    fn observe_backend_events(mut backend_events: BackendEvents, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            while let Some(events) = receive_backend_event_batch(&mut backend_events).await {
                if this
                    .update(cx, |this, cx| this.handle_backend_events(events, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        preferences: AppPreferences,
        backend: BackendHandle,
        backend_events: BackendEvents,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search Spotify"));
        let search_subscription = cx.subscribe_in(
            &search_input,
            window,
            |this, input, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    this.search_query = input.read(cx).value().to_string();
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.submit_search(cx),
                _ => {}
            },
        );
        let spotify_client_id_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("32-character Client ID"));
        let spotify_client_id_subscription = cx.subscribe_in(
            &spotify_client_id_input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    this.spotify_setup_error = None;
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.configure_spotify(window, cx),
                _ => {}
            },
        );
        appearance::Appearance::init(preferences.theme, window, cx);
        let appearance_subscription = cx.observe_window_appearance(window, |this, window, cx| {
            this.update_system_appearance(window, cx);
        });
        window.focus(&focus_handle);
        let activation_events = services::AppServices::activations(cx);
        let window_handle = window.window_handle();
        cx.spawn(async move |_, cx| {
            while activation_events.recv().await.is_ok() {
                let _ = cx.update_window(window_handle, |_, window, cx| {
                    cx.activate(true);
                    window.activate_window();
                });
            }
        })
        .detach();
        let player = services::AppServices::player(cx);
        cx.subscribe(&player, |this, _, _: &player::PlaybackUnavailable, cx| {
            this.last_error = Some("Cadence backend is busy or not running".to_owned());
            cx.notify();
        })
        .detach();
        let player_bar = cx.new(|cx| player_bar::PlayerBar::new(cx));
        cx.subscribe(&player_bar, |this, bar, _: &player_bar::ToggleQueue, cx| {
            if bar.read(cx).queue_open() {
                this.account_menu_open = false;
                this.track_menu_open = None;
            }
            cx.notify();
        })
        .detach();
        let queue_drawer = cx.new(|cx| player_bar::QueueDrawer::new(cx));
        cx.subscribe(&queue_drawer, |this, _, _: &player_bar::CloseQueue, cx| {
            this.close_queue(cx);
        })
        .detach();
        Self::observe_backend_events(backend_events, cx);
        Self {
            backend,
            account_generation: 0,
            connection_state: ConnectionState::Starting,
            last_error: None,
            action_notice: None,
            radio_request_id: 0,
            pending_radio_request: None,
            search_results: Arc::default(),
            search_playlists: Arc::default(),
            search_loaded: false,
            searching: false,
            search_error: None,
            search_query: String::new(),
            search_request_id: 0,
            search_input,
            _search_subscription: search_subscription,
            spotify_client_id_input,
            _spotify_client_id_subscription: spotify_client_id_subscription,
            spotify_client_id: None,
            spotify_client_id_source: None,
            spotify_setup_error: None,
            spotify_setup_needs_focus: true,
            spotify_configuration_request_id: 0,
            pending_spotify_configuration: None,
            spotify_configuration_blocked: false,
            spotify_app_change_confirmation_open: false,
            liked_tracks: Arc::default(),
            library_loaded: false,
            local_favorites: Arc::default(),
            favorite_keys: HashMap::new(),
            pinned_playlists: Arc::default(),
            local_state_loaded: false,
            spotify_playlists: Arc::default(),
            profile: None,
            recently_played: Arc::default(),
            selected_spotify_playlist: None,
            playlist_tracks: Arc::default(),
            playlist_loaded: false,
            playlist_error: None,
            playlist_request_id: 0,
            selected_artist_ref: None,
            selected_artist: None,
            artist_tracks: Arc::default(),
            artist_albums: Arc::default(),
            artist_request_id: 0,
            artist_section: ArtistSection::Popular,
            artist_loaded: false,
            artist_loading: false,
            artist_error: None,
            artist_loaded_at: None,
            selected_album_ref: None,
            selected_album: None,
            album_tracks: Arc::default(),
            album_request_id: 0,
            album_loaded: false,
            album_loading: false,
            album_error: None,
            album_loaded_at: None,
            player,
            player_bar,
            queue_drawer,
            route: Route::LikedSongs,
            focus_handle,
            account_menu_open: false,
            track_menu_open: None,
            playlist_origin: Route::Playlists,
            artist_origin: Route::LikedSongs,
            album_origin: Route::LikedSongs,
            settings_origin: Route::LikedSongs,
            search_kind: SearchKind::Tracks,
            image_cache: services::AppServices::image_cache(cx),
            brand_mark: Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Png,
                include_bytes!("../../assets/cadence-mark.png").to_vec(),
            )),
            compact_layout: false,
            sidebar_collapsed: preferences.sidebar_collapsed,
            sidebar_transition_generation: 0,
            sidebar_visual_width: Rc::new(Cell::new(if preferences.sidebar_collapsed {
                72.
            } else {
                232.
            })),
            sidebar_transition_from: if preferences.sidebar_collapsed {
                72.
            } else {
                232.
            },
            sidebar_transition_duration: Duration::from_millis(1),
            palette: appearance::Appearance::palette(cx),
            _appearance_subscription: appearance_subscription,
        }
    }

    fn update_system_appearance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if appearance::Appearance::follow_system(window, cx) {
            cx.notify();
        }
    }

    fn set_sidebar_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        if self.sidebar_collapsed == collapsed {
            return;
        }
        let current_width = self.sidebar_visual_width.get();
        let expanded_width = if self.compact_layout { 200. } else { 232. };
        let target_width = if collapsed { 72. } else { expanded_width };
        self.sidebar_transition_from = current_width;
        self.sidebar_transition_duration =
            sidebar_transition_duration(current_width, target_width, expanded_width);
        self.sidebar_collapsed = collapsed;
        self.sidebar_transition_generation = self.sidebar_transition_generation.wrapping_add(1);
        if let Some(Err(error)) = services::AppServices::with_preferences(cx, |store| {
            store.set_sidebar_collapsed(collapsed)
        }) {
            self.last_error = Some(format!("Could not save sidebar preference: {error}"));
        }
        cx.notify();
    }

    fn set_theme_preference(
        &mut self,
        preference: ThemePreference,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        appearance::Appearance::set_preference(preference, window, cx);
        if let Some(Err(error)) = services::AppServices::with_preferences(cx, |store| {
            store.set_theme_preference(preference)
        }) {
            self.last_error = Some(format!("Could not save appearance preference: {error}"));
        }
        self.account_menu_open = false;
        cx.notify();
    }
}

mod actions;
mod appearance;
mod bootstrap;
mod catalog;
mod components;
mod events;
mod library;
mod onboarding;
mod player;
mod player_bar;
mod render;
mod services;
mod settings;
mod sidebar;

#[cfg(test)]
mod event_bridge_tests {
    use super::{
        BackendEvent, CATALOG_STALE_TIME, catalog_data_is_fresh, index_favorites,
        is_current_response, model, next_request_id, receive_backend_event_batch,
    };
    use std::time::{Duration, Instant};

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
    fn rejects_stale_account_and_navigation_responses() {
        assert!(is_current_response(4, 8, 4, 8));
        assert!(!is_current_response(4, 8, 3, 8));
        assert!(!is_current_response(4, 8, 4, 7));
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
        assert!(catalog_data_is_fresh(Some(Instant::now())));
        assert!(!catalog_data_is_fresh(Some(
            Instant::now() - CATALOG_STALE_TIME - Duration::from_secs(1)
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
        interpolate_sidebar_width, resolve_dark_mode, seek_for_pointer,
        sidebar_transition_duration, uses_compact_content_layout, uses_compact_player_layout,
        volume_for_pointer,
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
            sidebar_transition_duration(72., 232., 232.).as_millis(),
            180
        );
        assert_eq!(
            sidebar_transition_duration(152., 232., 232.).as_millis(),
            90
        );
        assert_eq!(
            sidebar_transition_duration(220., 232., 232.).as_millis(),
            60
        );
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
