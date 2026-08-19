use super::*;

/// What a page, or anything nested inside one, asks the workspace to do.
///
/// Pages own their own contents but not the route, the player context or the
/// notice bar, so every request that outlives a single page travels up as one
/// of these rather than reaching back into the workspace.
#[derive(Clone)]
pub(super) enum PageEvent {
    /// Fresh contents arrived, so any stale failure can be cleared.
    Loaded,
    Failed(String),
    OpenPlaylist(model::Playlist),
    OpenArtist(model::ArtistRef),
    OpenAlbum(model::AlbumRef),
    StartRadio(model::Track),
}

/// Re-emits every event a nested page part raises, unchanged.
pub(super) fn forward<Parent, Child>(
    child: &Entity<Child>,
    cx: &mut Context<Parent>,
) -> Subscription
where
    Parent: EventEmitter<PageEvent> + 'static,
    Child: EventEmitter<PageEvent> + 'static,
{
    cx.subscribe(child, |_, _, event: &PageEvent, cx| cx.emit(event.clone()))
}
