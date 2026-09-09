use super::*;
use gpui_kit::{AnyWindowHandle, HeadlessAppContext, WindowHandle};

#[allow(dead_code)]
pub(crate) mod rendering;

mod interactions;

pub(super) struct BackendProbe {
    pub commands: tokio::sync::mpsc::Receiver<BackendCommand>,
    pub volume: tokio::sync::watch::Receiver<f32>,
}

pub(super) fn initialize(cx: &mut App, theme: ThemePreference) -> BackendProbe {
    gpui_kit::init(cx);
    let (backend, commands, volume) = Backend::isolated();
    services::AppServices::init_isolated(
        cx,
        backend,
        AppPreferences {
            theme,
            ..AppPreferences::default()
        },
    );
    bootstrap::bind_keys(cx);
    BackendProbe { commands, volume }
}

pub(super) fn track(index: usize) -> model::Track {
    model::Track {
        provider: model::Provider::Spotify,
        source_id: format!("track-{index}"),
        spotify_uri: Some(format!("spotify:track:track-{index}")),
        isrc: None,
        title: format!("Song {}", index + 1),
        artist: "Cadence Ensemble".into(),
        artists: Vec::new(),
        album: "An afternoon in September".into(),
        album_ref: None,
        duration_ms: 240_000,
        artwork_url: None,
    }
}

pub(super) fn ready(cx: &mut App) {
    services::AppServices::session(cx).update(cx, |session, cx| {
        session.handle_backend_event(
            BackendEvent::SpotifyConfigured {
                generation: 0,
                client_id: "0123456789abcdef0123456789abcdef".into(),
                source: ClientIdSource::Saved,
            },
            cx,
        );
        session.handle_backend_event(BackendEvent::CatalogReady { generation: 0 }, cx);
    });
    services::AppServices::library(cx).update(cx, |library, cx| {
        library.handle_backend_event(
            BackendEvent::LibraryLoaded {
                generation: 0,
                liked_tracks: vec![track(0), track(1), track(1), track(2)],
                playlists: Vec::new(),
            },
            0,
            cx,
        );
        library.handle_backend_event(
            BackendEvent::LocalStateLoaded {
                favorites: vec![track(0)],
                pinned_playlists: Vec::new(),
                recently_played: Vec::new(),
            },
            0,
            cx,
        );
    });
    services::AppServices::player(cx).update(cx, |player, cx| {
        player.handle_backend_event(
            BackendEvent::PlaybackSnapshotLoaded {
                current: track(0),
                next: vec![track(1), track(2)],
                position_ms: 60_000,
            },
            cx,
        );
    });
}

pub(super) fn workspace(
    cx: &mut HeadlessAppContext,
    width: f32,
    height: f32,
) -> (WindowHandle<Root>, Entity<Workspace>) {
    cx.update(ready);
    let mut workspace = None;
    let window = cx
        .open_window(size(px(width), px(height)), |window, cx| {
            let view = cx.new(|cx| Workspace::new(window, cx));
            workspace = Some(view.clone());
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("workspace window");
    (window, workspace.expect("workspace entity"))
}

pub(super) fn settle(cx: &mut HeadlessAppContext, window: AnyWindowHandle) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            window.draw(cx).clear(cx);
        })
        .expect("draw fixture");
    }
}
