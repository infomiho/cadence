use super::{BackendProbe, initialize, settle, track, workspace};
use crate::app::{Route, Workspace, appearance, assets, onboarding, services};
use crate::backend::{BackendCommand, BackendEvent};
use crate::model;
use crate::storage::{MascotPreference, ThemePreference};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    App, AppContext, HeadlessAppContext, InputEvent as _, KeyUpEvent, Keystroke, NoopTextSystem,
    WeakEntity, Window, WindowHandle, px, size,
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
        self.update(|_, cx| {
            services::AppServices::player(cx).update(cx, |player, cx| {
                player.handle_backend_event(
                    BackendEvent::PlaybackSnapshotLoaded {
                        current,
                        next: vec![track(1)],
                        position_ms: 0,
                    },
                    cx,
                );
            });
        });
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
