use super::{BackendProbe, initialize, settle, track, workspace};
use crate::app::{Route, Workspace, appearance, assets, onboarding, services};
use crate::backend::{BackendCommand, BackendEvent};
use crate::model;
use crate::storage::{MascotPreference, ThemePreference};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    App, AppContext, HeadlessAppContext, InputEvent as _, KeyUpEvent, Keystroke, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, NoopTextSystem, Point, WeakEntity, Window,
    WindowHandle, point, px, size,
};
use std::sync::Arc;
use std::time::Duration;

struct Fixture {
    cx: HeadlessAppContext,
    window: WindowHandle<Root>,
    workspace: Option<WeakEntity<Workspace>>,
    backend: BackendProbe,
}

impl Fixture {
    fn at_width(width: f32) -> Self {
        let mut fixture = Self::new(false);
        fixture
            .cx
            .update_window(fixture.window.into(), |_, window, cx| {
                window.resize(size(px(width), px(820.)));
                let _ = cx;
            })
            .expect("fixture window");
        settle(&mut fixture.cx, fixture.window.into());
        fixture
    }

    fn new(settings: bool) -> Self {
        let mut cx = HeadlessAppContext::with_asset_source(
            Arc::new(NoopTextSystem),
            Arc::new(assets::CadenceAssets),
        );
        let backend = cx.update(|cx| initialize(cx, ThemePreference::Dark));
        let (window, workspace) = workspace(&mut cx, 1280., 820.);
        if settings {
            cx.update(|cx| workspace.update(cx, |workspace, cx| workspace.open_settings(cx)));
        }
        settle(&mut cx, window.into());
        Self {
            cx,
            window,
            workspace: Some(workspace.downgrade()),
            backend,
        }
    }

    fn setup() -> Self {
        let mut cx = HeadlessAppContext::with_asset_source(
            Arc::new(NoopTextSystem),
            Arc::new(assets::CadenceAssets),
        );
        let backend = cx.update(|cx| {
            let backend = initialize(cx, ThemePreference::Light);
            services::AppServices::session(cx).update(cx, |session, cx| {
                session.handle_backend_event(BackendEvent::SetupRequired, cx);
            });
            backend
        });
        let window = cx
            .open_window(size(px(1280.), px(820.)), |window, cx| {
                appearance::Appearance::attach(window, cx);
                let onboarding = cx.new(|cx| onboarding::Onboarding::new(window, cx));
                cx.new(|cx| Root::new(onboarding, window, cx))
            })
            .expect("setup window");
        settle(&mut cx, window.into());
        Self {
            cx,
            window,
            workspace: None,
            backend,
        }
    }

    fn route(&mut self) -> Route {
        let workspace = self.workspace.clone().expect("workspace fixture");
        self.update(|_, cx| {
            workspace
                .upgrade()
                .expect("live workspace")
                .read(cx)
                .router
                .route()
        })
    }

    fn play(&mut self, current: model::Track) {
        self.player_event(BackendEvent::PlaybackSnapshotLoaded {
            current,
            next: vec![track(1)],
            position_ms: 0,
        });
    }

    fn player_event(&mut self, event: BackendEvent) {
        self.update(|_, cx| {
            services::AppServices::player(cx).update(cx, |player, cx| {
                player.handle_backend_event(event, cx);
            });
        });
    }

    fn queued_source_ids(&mut self) -> Vec<String> {
        self.update(|_, cx| {
            services::AppServices::player(cx)
                .read(cx)
                .queue()
                .iter()
                .map(|track| track.source_id.clone())
                .collect()
        })
    }

    fn marked_played(&mut self) -> String {
        match self.backend.commands.try_recv().expect("history request") {
            BackendCommand::MarkPlayed(track) => track.source_id,
            other => panic!("expected a history request, got {other:?}"),
        }
    }

    fn requested_artist(&mut self) -> String {
        match self.backend.commands.try_recv().expect("artist request") {
            BackendCommand::LoadArtist { source_id, .. } => source_id,
            other => panic!("expected an artist request, got {other:?}"),
        }
    }

    fn requested_album(&mut self) -> String {
        match self.backend.commands.try_recv().expect("album request") {
            BackendCommand::LoadAlbum { source_id, .. } => source_id,
            other => panic!("expected an album request, got {other:?}"),
        }
    }

    fn update<R>(&mut self, f: impl FnOnce(&mut Window, &mut App) -> R) -> R {
        let result = self
            .cx
            .update_window(self.window.into(), |_, window, cx| f(window, cx))
            .expect("fixture window");
        settle(&mut self.cx, self.window.into());
        result
    }

    fn wait_for(&mut self, predicate: impl Fn(&mut Window, &mut App) -> bool) {
        for _ in 0..20 {
            if self.update(&predicate) {
                return;
            }
            self.cx.advance_clock(Duration::from_millis(10));
        }
        panic!("UI did not settle within 200 ms");
    }

    fn no_commands(&mut self) {
        assert!(matches!(
            self.backend.commands.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
    }

    /// The pointer half of a scrub, split so a test can act between the press
    /// and the release. `TestWindowExt::drag` only does the whole gesture.
    fn mouse_down(&mut self, position: Point<gpui_kit::Pixels>) {
        self.update(|window, cx| {
            window.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
        });
    }

    fn mouse_move_to(&mut self, position: Point<gpui_kit::Pixels>) {
        self.update(|window, cx| {
            window.dispatch_event(
                MouseMoveEvent {
                    position,
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
        });
    }

    fn mouse_up(&mut self, position: Point<gpui_kit::Pixels>) {
        self.update(|window, cx| {
            window.dispatch_event(
                MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers: Default::default(),
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
        });
    }

    fn press(&mut self, key: &str) {
        self.update(|window, cx| {
            let keystroke = Keystroke::parse(key).expect("fixture key");
            window.dispatch_keystroke(keystroke.clone(), cx);
            window.dispatch_event(KeyUpEvent { keystroke }.to_platform_input(), cx);
        });
    }
}

#[test]
fn search_shortcut_focuses_input_and_enter_submits_text_without_toggling_playback() {
    let mut fixture = Fixture::new(false);
    fixture.no_commands();
    fixture.update(|window, cx| window.press("cmd-k", cx));
    fixture.update(|window, cx| {
        assert_eq!(window.find("search-input").focused(), Some(true));
        window.input("Blue", cx);
        window.press("space", cx);
        window.input("Train", cx);
        assert_eq!(window.find("search-input").value(), Some("Blue Train"));
    });
    fixture.no_commands();
    fixture.update(|window, cx| window.press("enter", cx));
    match fixture.backend.commands.try_recv().expect("search command") {
        BackendCommand::SearchCatalog { query, .. } => assert_eq!(query, "Blue Train"),
        command => panic!("unexpected command: {command:?}"),
    }
    fixture.no_commands();
    fixture.update(|_, cx| assert!(!services::AppServices::player(cx).read(cx).playing()));
}

#[test]
fn client_id_validation_keeps_focus_and_only_submits_valid_input() {
    let mut fixture = Fixture::setup();
    fixture.update(|window, cx| {
        window.click("client-id-input", cx);
        window.input("not-a-client-id", cx);
    });
    fixture.update(|window, cx| window.press("enter", cx));
    fixture.no_commands();
    fixture.update(|window, cx| window.click("save-spotify-client-id", cx));
    fixture.update(|window, cx| {
        assert_eq!(window.find("client-id-input").focused(), Some(true));
        assert_eq!(
            window.find("client-id-input").value(),
            Some("not-a-client-id")
        );
        assert!(
            services::AppServices::session(cx)
                .read(cx)
                .setup_error()
                .is_some()
        );
    });
    fixture.no_commands();
    fixture.update(|window, cx| {
        window.click("client-id-input", cx);
        window.press("cmd-a", cx);
        window.input("0123456789abcdef0123456789abcdef", cx);
    });
    fixture.update(|window, cx| {
        assert!(
            services::AppServices::session(cx)
                .read(cx)
                .setup_error()
                .is_none()
        );
        window.press("enter", cx);
    });
    match fixture
        .backend
        .commands
        .try_recv()
        .expect("configuration command")
    {
        BackendCommand::ConfigureSpotify { client_id, .. } => {
            assert_eq!(client_id, "0123456789abcdef0123456789abcdef");
        }
        command => panic!("unexpected command: {command:?}"),
    }
    fixture.no_commands();
}

#[test]
fn autoplay_switch_updates_preferences_and_rendered_state() {
    let mut fixture = Fixture::new(true);
    fixture.update(|window, cx| {
        assert_eq!(window.find("settings-autoplay").checked(), Some(true));
        window.click("settings-autoplay", cx);
    });
    fixture.update(|window, cx| {
        assert!(!services::AppServices::preferences(cx).autoplay);
        assert_eq!(window.find("settings-autoplay").checked(), Some(false));
        window.click("settings-autoplay", cx);
    });
    fixture.update(|window, cx| {
        assert!(services::AppServices::preferences(cx).autoplay);
        assert_eq!(window.find("settings-autoplay").checked(), Some(true));
    });
    fixture.no_commands();
}

#[test]
fn mascot_select_commits_enter_and_preserves_selection_on_escape_and_outside_click() {
    let mut fixture = Fixture::new(true);
    fixture.update(|window, cx| window.within("settings-mascot").click("input", cx));
    fixture.update(|window, cx| {
        assert_eq!(window.find("settings-mascot").expanded(), Some(true));
        window.press("down", cx);
    });
    fixture.update(|window, cx| window.press("enter", cx));
    fixture.wait_for(|window, _| window.find("settings-mascot").expanded() == Some(false));
    fixture.update(|window, cx| {
        assert_eq!(
            services::AppServices::preferences(cx).mascot,
            MascotPreference::RomeoVespa
        );
        assert_eq!(window.find("settings-mascot").value(), Some("Vespa Romeo"));
        window.within("settings-mascot").click("input", cx);
    });
    fixture.update(|window, cx| window.press("down", cx));
    fixture.update(|window, cx| window.press("escape", cx));
    fixture.wait_for(|window, _| window.find("settings-mascot").expanded() == Some(false));
    fixture.update(|window, cx| {
        assert_eq!(
            services::AppServices::preferences(cx).mascot,
            MascotPreference::RomeoVespa
        );
        assert_eq!(window.find("settings-mascot").value(), Some("Vespa Romeo"));
        window.within("settings-mascot").click("input", cx);
    });
    fixture.update(|window, cx| window.press("down", cx));
    fixture.update(|window, cx| window.click("volume", cx));
    fixture.wait_for(|window, _| window.find("settings-mascot").expanded() == Some(false));
    fixture.update(|window, cx| {
        assert_eq!(
            services::AppServices::preferences(cx).mascot,
            MascotPreference::RomeoVespa
        );
        assert_eq!(window.find("settings-mascot").value(), Some("Vespa Romeo"));
    });
}

#[test]
fn queue_toggle_and_close_button_control_the_panel_without_backend_commands() {
    let mut fixture = Fixture::new(false);
    fixture.update(|window, cx| {
        assert!(window.try_find("close-queue").is_none());
        window.click("queue-toggle", cx);
    });
    fixture.update(|window, cx| {
        assert!(window.find("close-queue").visible());
        window.click("queue-toggle", cx);
    });
    fixture.update(|window, cx| {
        assert!(window.try_find("close-queue").is_none());
        window.click("queue-toggle", cx);
    });
    fixture.update(|window, cx| window.click("close-queue", cx));
    fixture.update(|window, _| assert!(window.try_find("close-queue").is_none()));
    fixture.no_commands();
}

#[test]
fn queue_keyboard_activation_preserves_playback_and_reports_expansion() {
    let mut fixture = Fixture::new(false);
    fixture.press("space");
    assert!(matches!(
        fixture
            .backend
            .commands
            .try_recv()
            .expect("playback shortcut"),
        BackendCommand::Resume
    ));
    let bounds = fixture.update(|window, _| window.find("queue-toggle").bounds());
    fixture.press("cmd-k");
    fixture.press("tab");
    fixture.update(|window, _| {
        let button = window.find("queue-toggle");
        assert_eq!(button.focused(), Some(true));
        assert_eq!(button.role(), Some(gpui_kit::Role::Button));
        assert_eq!(button.label(), Some("Queue"));
        assert_eq!(button.expanded(), Some(false));
        assert_eq!(button.bounds(), bounds);
    });

    for (key, expanded) in [("space", true), ("enter", false), ("space", true)] {
        fixture.press(key);
        fixture.update(|window, cx| {
            assert_eq!(window.find("queue-toggle").expanded(), Some(expanded));
            assert_eq!(window.find("queue-toggle").focused(), Some(true));
            assert_eq!(window.find("queue-toggle").bounds(), bounds);
            assert_eq!(window.try_find("close-queue").is_some(), expanded);
            assert!(services::AppServices::player(cx).read(cx).playing());
        });
        fixture.no_commands();
    }
}

#[test]
fn duplicate_track_actions_preserve_row_index_and_favorite_does_not_start_playback() {
    let mut fixture = Fixture::new(false);
    fixture.update(|window, cx| window.hover(("spotify-track", 2usize), cx));
    fixture.update(|window, cx| window.click(("track-actions", 2usize), cx));
    fixture.update(|window, cx| {
        assert!(window.try_find(("track-menu-play", 1usize)).is_none());
        window.click(("track-menu-play", 2usize), cx);
    });
    match fixture.backend.commands.try_recv().expect("play command") {
        BackendCommand::PlayContext { tracks, index } => {
            assert_eq!(index, 2);
            assert_eq!(tracks.len(), 4);
            assert_eq!(tracks[1].source_id, tracks[2].source_id);
            assert_eq!(tracks[index].source_id, track(1).source_id);
        }
        command => panic!("unexpected command: {command:?}"),
    }
    fixture.no_commands();
    fixture.update(|window, cx| window.click(("spotify-favorite", 3usize), cx));
    match fixture
        .backend
        .commands
        .try_recv()
        .expect("favorite command")
    {
        BackendCommand::SetFavorite {
            track: favorite,
            favorite: true,
        } => assert_eq!(favorite.source_id, track(2).source_id),
        command => panic!("unexpected command: {command:?}"),
    }
    fixture.no_commands();
    fixture.update(|_, cx| {
        services::AppServices::library(cx).update(cx, |library, cx| {
            library.handle_backend_event(
                BackendEvent::LocalStateLoaded {
                    favorites: vec![track(0), track(2)],
                    pinned_playlists: Vec::new(),
                    recently_played: Vec::new(),
                },
                0,
                cx,
            );
        });
    });
    fixture.update(|window, cx| window.click(("spotify-favorite", 3usize), cx));
    match fixture
        .backend
        .commands
        .try_recv()
        .expect("unfavorite command")
    {
        BackendCommand::SetFavorite {
            track: favorite,
            favorite: false,
        } => assert_eq!(favorite.source_id, track(2).source_id),
        command => panic!("unexpected command: {command:?}"),
    }
    fixture.no_commands();
}

#[test]
fn mute_sends_volume_and_restores_the_previous_level() {
    let mut fixture = Fixture::new(false);
    let initial = fixture.update(|_, cx| services::AppServices::player(cx).read(cx).volume());
    fixture.update(|window, cx| window.click("volume", cx));
    assert_eq!(*fixture.backend.volume.borrow_and_update(), 0.);
    fixture.update(|window, cx| window.click("volume", cx));
    assert_eq!(*fixture.backend.volume.borrow_and_update(), initial);
    fixture.no_commands();
}

fn artist_ref(name: &str, source_id: &str) -> model::ArtistRef {
    model::ArtistRef {
        name: name.into(),
        source_id: Some(source_id.into()),
        spotify_uri: Some(format!("spotify:artist:{source_id}")),
    }
}

/// A track Spotify fully described, with two credited artists and an album.
fn credited_track() -> model::Track {
    let mut track = track(0);
    track.artist = "Frankie Valli, The Four Seasons".into();
    track.artists = vec![
        artist_ref("Frankie Valli", "artist-valli"),
        artist_ref("The Four Seasons", "artist-seasons"),
    ];
    track.album_ref = Some(model::AlbumRef {
        name: "Grease".into(),
        source_id: Some("album-grease".into()),
        spotify_uri: Some("spotify:album:album-grease".into()),
        artwork_url: None,
    });
    track
}

#[test]
fn player_bar_opens_each_credited_artist_and_the_album_separately() {
    let mut fixture = Fixture::new(false);
    fixture.play(credited_track());
    fixture.no_commands();

    fixture.update(|window, cx| window.click(("player-artist", 1usize), cx));
    assert_eq!(fixture.requested_artist(), "artist-seasons");
    assert_eq!(fixture.route(), Route::Artist);

    fixture.update(|window, cx| window.click(("player-artist", 0usize), cx));
    assert_eq!(fixture.requested_artist(), "artist-valli");
    assert_eq!(fixture.route(), Route::Artist);

    fixture.update(|window, cx| window.click("player-artwork", cx));
    assert_eq!(fixture.requested_album(), "album-grease");
    assert_eq!(fixture.route(), Route::Album);

    // Dropping the album reply above failed that load, so the title retries it.
    fixture.update(|window, cx| window.click("player-title", cx));
    assert_eq!(fixture.requested_album(), "album-grease");
    assert_eq!(fixture.route(), Route::Album);
    fixture.no_commands();
}

#[test]
fn player_bar_title_opens_the_album_from_the_keyboard() {
    let mut fixture = Fixture::new(false);
    fixture.play(credited_track());
    fixture.press("cmd-k");
    assert_eq!(fixture.route(), Route::Search);

    fixture.press("tab");
    fixture.update(|window, _| {
        let title = window.find("player-title");
        assert_eq!(title.focused(), Some(true));
        assert_eq!(title.role(), Some(gpui_kit::Role::Link));
    });
    fixture.press("enter");
    assert_eq!(fixture.requested_album(), "album-grease");
    assert_eq!(fixture.route(), Route::Album);
    fixture.no_commands();
}

#[test]
fn player_bar_credits_without_references_stay_plain() {
    let mut fixture = Fixture::new(false);
    fixture.update(|window, _| {
        assert!(window.try_find("player-artwork").is_none());
        assert!(window.try_find("player-title").is_none());
        assert!(window.try_find(("player-artist", 0usize)).is_none());
    });
    fixture.no_commands();
}

#[test]
fn unavailable_live_track_keeps_the_queue_and_skips_ahead() {
    let mut fixture = Fixture::new(false);
    fixture.player_event(BackendEvent::Playing {
        spotify_uri: "spotify:track:track-0".into(),
    });
    assert_eq!(fixture.marked_played(), "track-0");
    fixture.player_event(BackendEvent::TrackFailed {
        spotify_uri: "spotify:track:track-0".into(),
        error: "Spotify cannot play this track".into(),
    });
    fixture.update(|_, cx| {
        let player = services::AppServices::player(cx).read(cx);
        assert_eq!(
            player.now_playing().map(|track| track.source_id.as_str()),
            Some("track-0")
        );
        assert!(!player.playing());
    });
    assert_eq!(fixture.queued_source_ids(), ["track-1", "track-2"]);
    assert!(matches!(
        fixture.backend.commands.try_recv(),
        Ok(BackendCommand::Next)
    ));
    fixture.no_commands();
}

#[test]
fn stale_track_failure_does_not_skip_ahead() {
    let mut fixture = Fixture::new(false);
    fixture.player_event(BackendEvent::TrackFailed {
        spotify_uri: "spotify:track:track-9".into(),
        error: "Spotify cannot play this track".into(),
    });
    assert_eq!(fixture.queued_source_ids(), ["track-1", "track-2"]);
    fixture.no_commands();
}

#[test]
fn track_failure_during_reconnect_does_not_skip_ahead() {
    let mut fixture = Fixture::new(false);
    fixture.player_event(BackendEvent::PlaybackReconnecting);
    fixture.player_event(BackendEvent::TrackFailed {
        spotify_uri: "spotify:track:track-0".into(),
        error: "Spotify cannot play this track".into(),
    });
    assert_eq!(fixture.queued_source_ids(), ["track-1", "track-2"]);
    fixture.no_commands();
}

#[test]
fn history_is_recorded_once_the_track_plays_and_only_once() {
    let mut fixture = Fixture::new(false);
    fixture.player_event(BackendEvent::PlaybackContext {
        current: track(3),
        next: vec![track(4)],
    });
    fixture.no_commands();
    fixture.player_event(BackendEvent::Playing {
        spotify_uri: "spotify:track:track-3".into(),
    });
    assert_eq!(fixture.marked_played(), "track-3");
    fixture.player_event(BackendEvent::Playing {
        spotify_uri: "spotify:track:track-3".into(),
    });
    fixture.no_commands();
}

#[test]
fn a_track_that_fails_to_play_is_not_recorded() {
    let mut fixture = Fixture::new(false);
    fixture.player_event(BackendEvent::PlaybackContext {
        current: track(3),
        next: vec![track(4)],
    });
    fixture.player_event(BackendEvent::TrackFailed {
        spotify_uri: "spotify:track:track-3".into(),
        error: "Spotify cannot play this track".into(),
    });
    assert!(matches!(
        fixture.backend.commands.try_recv(),
        Ok(BackendCommand::Next)
    ));
    fixture.no_commands();
}

#[test]
fn a_track_resumed_after_restart_is_recorded_when_it_plays() {
    let mut fixture = Fixture::new(false);
    fixture.player_event(BackendEvent::Playing {
        spotify_uri: "spotify:track:track-0".into(),
    });
    assert_eq!(fixture.marked_played(), "track-0");
    fixture.no_commands();
}

#[test]
fn a_reconnect_mid_track_records_history_once() {
    let mut fixture = Fixture::new(false);
    fixture.player_event(BackendEvent::PlaybackContext {
        current: track(3),
        next: vec![track(4)],
    });
    fixture.player_event(BackendEvent::PlaybackReconnecting);
    fixture.player_event(BackendEvent::Playing {
        spotify_uri: "spotify:track:track-3".into(),
    });
    fixture.no_commands();
    fixture.player_event(BackendEvent::PlaybackReconnected);
    assert!(matches!(
        fixture.backend.commands.try_recv(),
        Ok(BackendCommand::RestorePlayback { .. })
    ));
    fixture.player_event(BackendEvent::PlaybackRestored {
        position_ms: 0,
        playing: true,
    });
    fixture.player_event(BackendEvent::Playing {
        spotify_uri: "spotify:track:track-3".into(),
    });
    assert_eq!(fixture.marked_played(), "track-3");
    fixture.player_event(BackendEvent::Playing {
        spotify_uri: "spotify:track:track-3".into(),
    });
    fixture.no_commands();
}

/// The whole point of the scrub gesture: the position follows the pointer, and
/// the backend hears about it once, when the gesture ends.
#[test]
fn scrubbing_seeks_once_on_release_at_the_position_under_the_pointer() {
    let mut fixture = Fixture::new(false);
    fixture.player_event(BackendEvent::PlaybackContext {
        current: track(3),
        next: Vec::new(),
    });
    fixture.no_commands();

    let bounds = fixture.update(|window, _| window.find("progress-slider").bounds());
    let at = |fraction: f32| {
        point(
            bounds.origin.x + bounds.size.width * fraction,
            bounds.center().y,
        )
    };
    fixture.update(|window, cx| window.drag(at(0.25), at(0.75), cx));

    // track(3) runs 240s, so releasing three quarters along asks for 3:00.
    match fixture.backend.commands.try_recv().expect("a seek") {
        BackendCommand::Seek(position_ms) => assert!(
            position_ms.abs_diff(180_000) < 2_000,
            "expected roughly 180000ms, got {position_ms}"
        ),
        other => panic!("expected a seek, got {other:?}"),
    }
    fixture.no_commands();
}

/// The seek maps through the track's painted bounds, so it stays correct when
/// the window is a size the layout constants were never written for.
#[test]
fn scrubbing_maps_through_the_painted_track_at_any_window_width() {
    for width in [1280., 1000., 760.] {
        let mut fixture = Fixture::at_width(width);
        fixture.player_event(BackendEvent::PlaybackContext {
            current: track(3),
            next: Vec::new(),
        });
        fixture.no_commands();

        let bounds = fixture.update(|window, _| window.find("progress-slider").bounds());
        let middle = point(bounds.origin.x + bounds.size.width * 0.5, bounds.center().y);
        fixture.update(|window, cx| window.drag(middle, middle, cx));

        match fixture.backend.commands.try_recv().expect("a seek") {
            BackendCommand::Seek(position_ms) => assert!(
                position_ms.abs_diff(120_000) < 2_000,
                "at width {width}: expected roughly 120000ms, got {position_ms}"
            ),
            other => panic!("expected a seek, got {other:?}"),
        }
    }
}

#[test]
fn arrow_keys_seek_once_the_scrubber_has_focus() {
    let mut fixture = Fixture::new(false);
    fixture.player_event(BackendEvent::PlaybackContext {
        current: track(3),
        next: Vec::new(),
    });
    fixture.no_commands();

    fixture.update(|window, cx| window.click("progress-slider", cx));
    fixture.backend.commands.try_recv().expect("the click seek");
    fixture.update(|window, cx| window.press("right", cx));

    match fixture.backend.commands.try_recv().expect("a seek") {
        BackendCommand::Seek(position_ms) => assert!(
            position_ms.abs_diff(125_000) < 2_000,
            "expected roughly 125000ms, got {position_ms}"
        ),
        other => panic!("expected a seek, got {other:?}"),
    }
}

/// Escape during a scrub abandons it. No seek was ever sent, so there is
/// nothing to undo: commanding one would rewind playback by the drag's length.
#[test]
fn escape_during_a_scrub_abandons_it_without_seeking() {
    let mut fixture = Fixture::new(false);
    fixture.player_event(BackendEvent::PlaybackContext {
        current: track(3),
        next: Vec::new(),
    });
    fixture.no_commands();

    let bounds = fixture.update(|window, _| window.find("progress-slider").bounds());
    let at = |fraction: f32| {
        point(
            bounds.origin.x + bounds.size.width * fraction,
            bounds.center().y,
        )
    };
    fixture.mouse_down(at(0.25));
    fixture.mouse_move_to(at(0.75));
    fixture.press("escape");
    fixture.no_commands();

    fixture.mouse_up(at(0.75));
    fixture.no_commands();
}

/// A gesture belongs to the track it began on. If that track goes away the
/// press must not commit against whatever is playing by the time it is released.
#[test]
fn a_scrub_whose_track_ends_mid_gesture_does_not_seek_the_next_one() {
    let mut fixture = Fixture::new(false);
    fixture.player_event(BackendEvent::PlaybackContext {
        current: track(3),
        next: Vec::new(),
    });
    fixture.no_commands();

    let bounds = fixture.update(|window, _| window.find("progress-slider").bounds());
    let at = |fraction: f32| {
        point(
            bounds.origin.x + bounds.size.width * fraction,
            bounds.center().y,
        )
    };
    fixture.mouse_down(at(0.25));
    fixture.mouse_move_to(at(0.75));

    fixture.player_event(BackendEvent::PlaybackContext {
        current: track(7),
        next: Vec::new(),
    });
    fixture.mouse_up(at(0.75));
    fixture.no_commands();
}

/// Guards the split press/move/release helpers the cancel tests rely on: the
/// preview has to actually follow the pointer, or those tests would pass for
/// the wrong reason. gpui's first qualifying move only opens the drag, so the
/// preview tracks from the second move onwards.
#[test]
fn a_scrub_preview_follows_the_pointer_before_it_commits() {
    let mut fixture = Fixture::new(false);
    fixture.player_event(BackendEvent::PlaybackContext {
        current: track(3),
        next: Vec::new(),
    });
    fixture.no_commands();

    let bounds = fixture.update(|window, _| window.find("progress-slider").bounds());
    let at = |fraction: f32| {
        point(
            bounds.origin.x + bounds.size.width * fraction,
            bounds.center().y,
        )
    };
    fixture.mouse_down(at(0.25));
    fixture.mouse_move_to(at(0.5));
    fixture.mouse_move_to(at(0.75));
    fixture.no_commands();
    fixture.mouse_up(at(0.75));

    match fixture.backend.commands.try_recv().expect("a seek") {
        BackendCommand::Seek(position_ms) => assert!(
            position_ms.abs_diff(180_000) < 2_000,
            "expected roughly 180000ms, got {position_ms}"
        ),
        other => panic!("expected a seek, got {other:?}"),
    }
}

/// End means the end. Landing there finishes the track and moves on, which is
/// what every other player does.
#[test]
fn seeking_to_the_end_lands_on_the_end() {
    let mut fixture = Fixture::new(false);
    fixture.player_event(BackendEvent::PlaybackContext {
        current: track(3),
        next: Vec::new(),
    });
    fixture.no_commands();

    fixture.update(|window, cx| window.click("progress-slider", cx));
    fixture.backend.commands.try_recv().expect("the click seek");
    fixture.press("end");

    match fixture.backend.commands.try_recv().expect("a seek") {
        BackendCommand::Seek(position_ms) => assert_eq!(position_ms, 240_000),
        other => panic!("expected a seek, got {other:?}"),
    }
}
