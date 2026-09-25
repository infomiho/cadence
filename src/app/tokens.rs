//! Cadence's named sizes for the relationships the GPUI rem scale does not
//! cover. Each value is in rems, so it zooms with the theme's base font.

use gpui_kit::Rems;

pub(super) const CAPTION_TEXT: Rems = Rems(0.6875);
pub(super) const BODY_TEXT: Rems = Rems(0.8125);
pub(super) const PILL_TEXT: Rems = Rems(0.9375);
pub(super) const STEP_TITLE_TEXT: Rems = Rems(1.1875);
pub(super) const TASK_TITLE_TEXT: Rems = Rems(1.75);
pub(super) const DRAWER_TITLE_TEXT: Rems = Rems(2.);
pub(super) const PAGE_TITLE_TEXT: Rems = Rems(2.5);
pub(super) const PAGE_TITLE_LINE_HEIGHT: Rems = Rems(2.75);

pub(super) const CONTROL_RADIUS: Rems = Rems(0.625);
/// A surface that wraps controls, concentric with the control radius inside it.
pub(super) const CONTAINER_RADIUS: Rems = Rems(0.875);
pub(super) const PANEL_RADIUS: Rems = Rems(1.25);

pub(super) const CONTROL_ICON: Rems = Rems(1.0625);
pub(super) const FIELD_ICON: Rems = Rems(1.);
pub(super) const PLAYBACK_ICON: Rems = Rems(1.);
pub(super) const MENU_ICON: Rems = Rems(0.9375);
pub(super) const COLUMN_HEADER_ICON: Rems = Rems(0.75);
pub(super) const INLINE_ICON: Rems = Rems(0.6875);

pub(super) const INLINE_ICON_GAP: Rems = Rems(0.3125);
/// Room between flush content and the focus ring drawn inside its control.
pub(super) const FOCUS_RING_CLEARANCE: Rems = Rems(0.375);
pub(super) const PAGE_HEADING_GAP: Rems = Rems(0.4375);

pub(super) const TOOLBAR_HEIGHT: Rems = Rems(4.5);
pub(super) const AVATAR_SIZE: Rems = Rems(2.5);
pub(super) const SEARCH_FIELD_WIDTH: Rems = Rems(32.5);
pub(super) const COMPACT_SEARCH_FIELD_WIDTH: Rems = Rems(21.25);
pub(super) const MENU_WIDTH: Rems = Rems(13.75);
pub(super) const DIALOG_WIDTH: Rems = Rems(27.5);
pub(super) const NOTICE_BANNER_WIDTH: Rems = Rems(22.5);
pub(super) const NOTICE_BANNER_TOP: Rems = Rems(4.75);
pub(super) const SETTINGS_MAX_WIDTH: Rems = Rems(47.5);
pub(super) const SETTINGS_FIELD_WIDTH: Rems = Rems(13.75);

pub(super) const NAV_ROW_HEIGHT: Rems = Rems(2.625);
pub(super) const BRAND_LABEL_GAP: Rems = Rems(1.03125);
/// Clears the traffic lights above the sidebar's first row.
pub(super) const SIDEBAR_TOP_INSET: Rems = Rems(3.25);

pub(super) const PLAYLIST_ROW_HEIGHT: Rems = Rems(4.75);
pub(super) const ALBUM_CARD_HEIGHT: Rems = Rems(15.25);
pub(super) const QUEUE_DRAWER_WIDTH: Rems = Rems(26.25);
pub(super) const QUEUE_ROW_HEIGHT: Rems = Rems(3.875);
pub(super) const QUEUE_CURRENT_ROW_HEIGHT: Rems = Rems(4.5);

pub(super) const ONBOARDING_MIN_HEIGHT: Rems = Rems(40.);
pub(super) const ONBOARDING_RAIL_WIDTH: Rems = Rems(26.25);
pub(super) const ONBOARDING_RAIL_PADDING: Rems = Rems(3.5);
pub(super) const ONBOARDING_HEADLINE_OFFSET: Rems = Rems(3.5);
pub(super) const ONBOARDING_FORM_MAX_WIDTH: Rems = Rems(45.);
pub(super) const ONBOARDING_ACTION_WIDTH: Rems = Rems(13.75);
pub(super) const STATUS_CARD_WIDTH: Rems = Rems(26.25);

/// The square an artwork image fills and the corner radius it is clipped to.
#[derive(Clone, Copy)]
pub(super) struct ArtworkSize {
    pub(super) size: Rems,
    pub(super) radius: Rems,
}

pub(super) const TRACK_ARTWORK: ArtworkSize = ArtworkSize {
    size: Rems(2.5),
    radius: Rems(0.5),
};
pub(super) const PLAYLIST_ROW_ARTWORK: ArtworkSize = ArtworkSize {
    size: Rems(3.),
    radius: Rems(0.625),
};
pub(super) const QUEUE_CURRENT_ARTWORK: ArtworkSize = ArtworkSize {
    size: Rems(3.),
    radius: Rems(0.5),
};
pub(super) const PLAYER_ARTWORK: ArtworkSize = ArtworkSize {
    size: Rems(3.5),
    radius: Rems(0.75),
};
pub(super) const ALBUM_CARD_ARTWORK: ArtworkSize = ArtworkSize {
    size: Rems(9.5),
    radius: Rems(0.875),
};
pub(super) const ARTIST_ARTWORK: ArtworkSize = ArtworkSize {
    size: Rems(9.),
    radius: Rems(4.5),
};
pub(super) const COLLECTION_ARTWORK: ArtworkSize = ArtworkSize {
    size: Rems(11.),
    radius: Rems(1.75),
};
