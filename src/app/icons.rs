//! The app's icon set: Solar glyphs bundled under `assets/icons/cadence`,
//! drawn through gpui-kit's `Icon` so they take the text colour and size of
//! wherever they sit.

use gpui_kit::{SharedString, component::IconNamed};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CadenceIcon {
    Heart,
    HeartFilled,
    Star,
    StarFilled,
    Clock,
    ClockFilled,
    Search,
    Playlist,
    PlaylistFilled,
    Queue,
    MusicNote,
    Person,
    ChevronLeft,
    Close,
    SkipBack,
    Pause,
    Play,
    SkipForward,
    Key,
    Settings,
    SignOut,
    Pin,
    PinFilled,
    SpeakerMuted,
    Speaker,
    More,
    SystemAppearance,
    Sun,
    Moon,
    Copy,
    ArrowUpRight,
}

impl CadenceIcon {
    #[cfg(test)]
    pub(super) const ALL: [Self; 31] = [
        Self::Heart,
        Self::HeartFilled,
        Self::Star,
        Self::StarFilled,
        Self::Clock,
        Self::ClockFilled,
        Self::Search,
        Self::Playlist,
        Self::PlaylistFilled,
        Self::Queue,
        Self::MusicNote,
        Self::Person,
        Self::ChevronLeft,
        Self::Close,
        Self::SkipBack,
        Self::Pause,
        Self::Play,
        Self::SkipForward,
        Self::Key,
        Self::Settings,
        Self::SignOut,
        Self::Pin,
        Self::PinFilled,
        Self::SpeakerMuted,
        Self::Speaker,
        Self::More,
        Self::SystemAppearance,
        Self::Sun,
        Self::Moon,
        Self::Copy,
        Self::ArrowUpRight,
    ];

    fn file_name(self) -> &'static str {
        match self {
            Self::Heart => "heart",
            Self::HeartFilled => "heart-filled",
            Self::Star => "star",
            Self::StarFilled => "star-filled",
            Self::Clock => "clock",
            Self::ClockFilled => "clock-filled",
            Self::Search => "search",
            Self::Playlist => "playlist",
            Self::PlaylistFilled => "playlist-filled",
            Self::Queue => "queue",
            Self::MusicNote => "music-note",
            Self::Person => "person",
            Self::ChevronLeft => "chevron-left",
            Self::Close => "close",
            Self::SkipBack => "skip-back",
            Self::Pause => "pause",
            Self::Play => "play",
            Self::SkipForward => "skip-forward",
            Self::Key => "key",
            Self::Settings => "settings",
            Self::SignOut => "sign-out",
            Self::Pin => "pin",
            Self::PinFilled => "pin-filled",
            Self::SpeakerMuted => "speaker-muted",
            Self::Speaker => "speaker",
            Self::More => "more",
            Self::SystemAppearance => "system-appearance",
            Self::Sun => "sun",
            Self::Moon => "moon",
            Self::Copy => "copy",
            Self::ArrowUpRight => "arrow-up-right",
        }
    }
}

impl IconNamed for CadenceIcon {
    fn path(self) -> SharedString {
        format!("icons/cadence/{}.svg", self.file_name()).into()
    }
}
