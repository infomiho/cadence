use super::*;

use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
};

/// What the last push to the system told it, so a once-a-second position tick
/// does not re-send artwork and titles that have not changed.
#[derive(Debug, PartialEq, Eq)]
struct Published {
    track: Option<(model::Provider, String)>,
    playing: bool,
    position_seconds: u64,
}

/// Publishes now-playing information to macOS and accepts the transport
/// commands it sends back, so Cadence can be driven without its window.
pub(super) struct SystemMediaControls {
    controls: MediaControls,
    published: Option<Published>,
}

impl SystemMediaControls {
    /// Attaches to the system controls, forwarding their commands to `player`.
    ///
    /// Returns `None` when the platform refuses them, which is not fatal: the
    /// app simply goes without media keys.
    pub(super) fn attach(player: Entity<player::Player>, cx: &mut App) -> Option<Self> {
        let mut controls = MediaControls::new(PlatformConfig {
            display_name: "Cadence",
            dbus_name: "cadence",
            hwnd: None,
        })
        .ok()?;

        let (commands, incoming) = async_channel::unbounded();
        // The system calls this from its own thread, so hand the event to the
        // foreground executor rather than touching the player here.
        controls
            .attach(move |event| {
                let _ = commands.send_blocking(event);
            })
            .ok()?;

        cx.spawn(async move |cx| {
            while let Ok(event) = incoming.recv().await {
                let applied = cx.update(|cx| {
                    player.update(cx, |player, cx| apply(event, player, cx));
                });
                if applied.is_err() {
                    break;
                }
            }
        })
        .detach();

        Some(Self {
            controls,
            published: None,
        })
    }

    /// Pushes the player's state to the system, skipping what it already knows.
    pub(super) fn sync(&mut self, player: &player::Player) {
        let now_playing = player.now_playing();
        let published = Published {
            track: now_playing.map(|track| (track.provider, track.source_id.clone())),
            playing: player.playing(),
            position_seconds: u64::from(player.position_ms() / 1000),
        };
        if self.published.as_ref() == Some(&published) {
            return;
        }
        let track_changed = self
            .published
            .as_ref()
            .is_none_or(|previous| previous.track != published.track);
        self.published = Some(published);

        let Some(track) = now_playing else {
            let _ = self.controls.set_playback(MediaPlayback::Stopped);
            return;
        };
        if track_changed {
            let _ = self.controls.set_metadata(MediaMetadata {
                title: Some(&track.title),
                album: Some(&track.album),
                artist: Some(&track.artist),
                cover_url: track.artwork_url.as_deref(),
                duration: Some(Duration::from_millis(u64::from(track.duration_ms))),
            });
        }
        let progress = Some(MediaPosition(Duration::from_millis(u64::from(
            player.position_ms(),
        ))));
        let _ = self.controls.set_playback(if player.playing() {
            MediaPlayback::Playing { progress }
        } else {
            MediaPlayback::Paused { progress }
        });
    }
}

/// The player action a system command asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Transport {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Seek(u32),
}

/// Translates a system command, ignoring the ones Cadence does not offer.
fn transport_for(event: MediaControlEvent) -> Option<Transport> {
    match event {
        MediaControlEvent::Play => Some(Transport::Play),
        MediaControlEvent::Pause | MediaControlEvent::Stop => Some(Transport::Pause),
        MediaControlEvent::Toggle => Some(Transport::Toggle),
        MediaControlEvent::Next => Some(Transport::Next),
        MediaControlEvent::Previous => Some(Transport::Previous),
        MediaControlEvent::SetPosition(MediaPosition(position)) => Some(Transport::Seek(
            position.as_millis().min(u128::from(u32::MAX)) as u32,
        )),
        _ => None,
    }
}

fn apply(event: MediaControlEvent, player: &mut player::Player, cx: &mut Context<player::Player>) {
    match transport_for(event) {
        Some(Transport::Play) => player.set_playing(true, cx),
        Some(Transport::Pause) => player.set_playing(false, cx),
        Some(Transport::Toggle) => player.toggle(cx),
        Some(Transport::Next) => player.next(cx),
        Some(Transport::Previous) => player.previous(cx),
        Some(Transport::Seek(position_ms)) => player.seek(position_ms, cx),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{MediaControlEvent, MediaPosition, Published, Transport, transport_for};
    use spotify_gpui_client::model::Provider;
    use std::time::Duration;

    fn published(source_id: &str, playing: bool, position_seconds: u64) -> Published {
        Published {
            track: Some((Provider::Spotify, source_id.to_owned())),
            playing,
            position_seconds,
        }
    }

    #[test]
    fn system_commands_map_to_transport_actions() {
        assert_eq!(
            transport_for(MediaControlEvent::Play),
            Some(Transport::Play)
        );
        assert_eq!(
            transport_for(MediaControlEvent::Pause),
            Some(Transport::Pause)
        );
        assert_eq!(
            transport_for(MediaControlEvent::Stop),
            Some(Transport::Pause)
        );
        assert_eq!(
            transport_for(MediaControlEvent::Toggle),
            Some(Transport::Toggle)
        );
        assert_eq!(
            transport_for(MediaControlEvent::Next),
            Some(Transport::Next)
        );
        assert_eq!(
            transport_for(MediaControlEvent::Previous),
            Some(Transport::Previous)
        );
    }

    #[test]
    fn seek_position_is_converted_to_milliseconds() {
        assert_eq!(
            transport_for(MediaControlEvent::SetPosition(MediaPosition(
                Duration::from_millis(4_500)
            ))),
            Some(Transport::Seek(4_500))
        );
    }

    #[test]
    fn seek_position_is_clamped_rather_than_wrapping() {
        assert_eq!(
            transport_for(MediaControlEvent::SetPosition(MediaPosition(
                Duration::from_secs(u64::from(u32::MAX))
            ))),
            Some(Transport::Seek(u32::MAX))
        );
    }

    #[test]
    fn commands_cadence_does_not_offer_are_ignored() {
        assert_eq!(transport_for(MediaControlEvent::Raise), None);
        assert_eq!(transport_for(MediaControlEvent::SetVolume(0.5)), None);
    }

    #[test]
    fn a_position_tick_within_the_same_second_is_not_republished() {
        assert_eq!(published("track", true, 12), published("track", true, 12));
    }

    #[test]
    fn crossing_a_second_or_pausing_is_republished() {
        assert_ne!(published("track", true, 12), published("track", true, 13));
        assert_ne!(published("track", true, 12), published("track", false, 12));
        assert_ne!(published("track", true, 12), published("other", true, 12));
    }
}
