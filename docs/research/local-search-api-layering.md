# Local search with Spotify API enrichment

Researched 2026-09-06 for `cadence-859`. This is a design proposal based on the current Cadence source and primary documentation, not an implementation or benchmark. It complements [the API research](spotify-search-api-2026-09.md) and [installed desktop inspection](spotify-desktop-search-inspection.md).

**Yes: Cadence can answer searches over music it already knows without a network round trip, then add Spotify catalog results.** The strongest opportunity is finding a familiar song, artist, album, or playlist while typing. A song absent from the local corpus still needs Spotify. The local layer improves time to a useful answer and resilience; it does not make Spotify's remote endpoint faster or establish that Cadence beats the official client.

## Existing foundation

Cadence already implements much of the data path needed for this idea:

| Data | Current state | Local-search opportunity |
| --- | --- | --- |
| Liked tracks | SQLite `liked_tracks_cache`, including track JSON; loaded into shared `Library` state | Search titles, artists, and album names immediately after cache hydration |
| Playlist summaries | SQLite `playlists_cache`; shared `Library` state | Search playlist names and owners; this does not include their tracks |
| Cadence favorites and pinned playlists | SQLite plus shared `Library` state | Useful intentional selections, subject to the account and retention treatment below |
| Cadence listening history | SQLite retains 500 distinct tracks; backend initially loads 100 | Recency can break otherwise similar local matches; this is Cadence history, not full Spotify listening history |
| Opened playlist/album contents | Held by the active catalog pages | Can feed a bounded session corpus as pages are visited |
| Catalog search results | Current tracks/playlists held by `SearchPage`; no historical query cache | Add temporary entity reuse and a separate exact-query result cache |

Evidence: [storage schema and methods](../../src/storage.rs), [Library state and events](../../src/app/library.rs), [backend loading](../../src/backend.rs), [catalog pages](../../src/app/catalog.rs). `serve_cached_library` sends persisted contents before the account's network loads finish. The normalized search representation does not exist yet.

`Track` already contains title, artist string, individual artist references, album name/reference, provider, source ID, and Spotify URI. Artist and album suggestions can therefore be derived from known tracks without an enrichment request, provided navigation only uses available IDs. `UserProfile` currently retains display name and artwork, not a stable account identifier. [Model](../../src/model.rs), [Spotify conversion](../../src/spotify.rs).

## Search interaction

Proposed behavior for typing `massive teard`:

1. Immediately search eligible, already-loaded metadata. If “Teardrop” by Massive Attack is in the corpus, show it under a small “Your music” section. Text and selection should not wait for artwork.
2. Independently check an exact-query catalog cache. Fresh hits can populate the Spotify section immediately. A cached page for `massive` is only another source of local candidates, not the authoritative response for `massive teard`.
3. On a remote miss, debounce the catalog request; Return submits immediately. Keep local search outside that delay. Cadence currently searches on Return, so a first increment can add local suggestions while preserving that remote trigger.
4. Add catalog results when they arrive. Keep the local section stable and keyboard focus attached to entity identity, so late results do not move the target under an imminent click or Return press.
5. If Spotify is unavailable or limited, retain eligible local matches and identify the catalog failure separately. An empty local section means no match in this device's current corpus, not that Spotify has no match.

This two-section design keeps ranking understandable without pretending Cadence's lexical score and Spotify's opaque ordering are comparable. For rows present in both sections, show the entity once in the local section, merge fresh metadata, and preserve Spotify's relative order among remaining remote rows. A future explicit “Spotify” scope can preserve the complete remote ordering. Exact-query caching, debounce, cancellation, and HTTP freshness rules are detailed in the [companion inspection](spotify-desktop-search-inspection.md).

## Matching and index choice

Start with a precomputed, deduplicated in-memory array derived from the shared library. Normalize searchable copies once when data changes, retaining original strings for display. Match query tokens across title, individual artists, album, and playlist name; rank exact title/name matches first, then title/name prefix and token matches, then matches only in secondary fields. Use favorites and recent use as bounded tie-breakers after textual relevance, followed by a stable ID tie-breaker. Evaluate case folding, diacritics, punctuation, non-Latin names, and multi-artist names with real examples. Do not remove syntax or accents from the remote request just because local matching normalizes them.

Spotify field-filter queries such as `year:1990-1999` should bypass local suggestions unless Cadence explicitly supports that filter with sufficient data. The current model does not preserve track release years or genres. A local text match must not falsely imply a filter was evaluated. The remote contract documents query filters but does not expose a portable relevance score. [Search reference](https://developer.spotify.com/documentation/web-api/reference/search).

An in-memory scan is the lowest-complexity starting point because the app already holds the corpus in memory. This is an engineering hypothesis, not a size-based performance guarantee. Build normalized data off the UI thread, cap returned results, and measure query/ranking work and allocations on realistic small and large libraries. A provisional target is local query work below 5 ms at p95 and first useful paint within 50 ms on the target Mac; both are proposed acceptance budgets, not measurements.

If measured scans exceed the budget, SQLite FTS5 fits the existing `rusqlite`/bundled SQLite stack. FTS5 provides token/prefix queries, weighted BM25, and Unicode tokenization; its trigram tokenizer is a separate option for substring search. These are different matching choices, and none supplies Spotify ranking or automatic typo tolerance. Verify the actual packaged SQLite supports FTS5 before adopting it. Keep an FTS index derived and rebuildable; update/delete it transactionally with the entity cache. Do not run JSON decoding or full-table `LIKE` scans for every keystroke. [Current dependency](../../Cargo.toml), [SQLite FTS5 documentation](https://www.sqlite.org/fts5.html).

## Identity, freshness, and lifecycle

Use `(provider, entity_type, source_id)` for entity identity inside an account namespace. Preserve multiple provenance memberships: removing a song from Liked Songs should remove that membership, not necessarily a still-valid occurrence in an opened playlist. Do not deduplicate different Spotify track IDs merely by title or ISRC; retaining the selected version matters. Relinking can substitute a playable track in a market, while current Development Mode responses omit `linked_from`, so do not make relinking aliases a required feature. [Track relinking](https://developer.spotify.com/documentation/web-api/concepts/track-relinking), [2026 migration](https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide).

A proposed searchable record needs identity, display metadata, normalized search fields, provenance, actual fetch/validation time, and an expiry. Query pages separately need exact request identity and ordered entity references. Evicting an entity must remove its index terms and invalidate or repair referring cached pages. Reading an old record, playing it, or matching an unchanged collection head must not reset the metadata's actual age.

For account ownership, bind persisted data to the account associated with the credential session. Spotify now documents `account_id` as the stable account-linking identifier; propagate it through Cadence's model before adding new durable account caches. On launch, cached account data can be used only with the corresponding stored session binding; do not wait for a profile network request on every query. Invalidate on account change and guard asynchronous updates with the existing generation concept. If effective market cannot be read in Development Mode, isolate by account/session and revalidate rather than inventing a market value. [May 2026 changelog](https://developer.spotify.com/documentation/web-api/references/changes/may-2026), [migration guide](https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide).

Current gaps matter: database cache rows lack account namespaces; playlists lack fetch timestamps; `cached_library` does not enforce an age limit. Logout clears the account's library cache, but the `Library::clear` contract intentionally retains Cadence favorites, pins, and history. That is not sufficient provenance to assume all embedded Spotify metadata is eligible for a new account's search. Keep independently created user intent distinct from Spotify-derived metadata and account associations; exclude unbound legacy data from automatic corpus expansion until its lifecycle is defined. [Store](../../src/storage.rs), [Library lifecycle](../../src/app/library.rs), [logout cleanup](../../src/backend.rs).

Spotify permits temporary metadata/artwork caching only as necessary for application performance/functionality, requires efforts toward current displayed data and deletion of older data, and disallows indefinite storage. Disconnecting requires deletion of the user's Spotify personal data. Therefore a bounded, expiring search corpus built from normal authorized usage is the plausible direction; a permanent personal or global Spotify mirror is not supported by those permissions. Exact retention durations require an explicit design decision consistent with actual response directives; the terms do not give blanket approval for any particular TTL. [Developer Terms, IV.3 and V.8](https://developer.spotify.com/terms).

## Acquisition and synchronization limits

Saved-track reads are paginated with up to 50 items per page and require `user-library-read`; current-user playlists are also paginated. Cadence already requests library and private/collaborative playlist scopes. Illustratively, fetching 10,000 liked tracks requires at least 200 saved-track page requests at the documented maximum, before playlists, probes, or retries. A first installation cannot conjure a complete local library immediately. Show partial progress if later streaming pages into an index, and never label a partial import complete. [Saved tracks](https://developer.spotify.com/documentation/web-api/reference/get-users-saved-tracks), [current-user playlists](https://developer.spotify.com/documentation/web-api/reference/get-a-list-of-current-users-playlists), [existing adapter](../../src/spotify.rs).

In affected Development Mode apps, playlist contents are available only for owned/collaborative playlists. Following a public or editorial playlist does not imply its tracks can populate this corpus. Several batch metadata endpoints were removed, so repeated eager enrichment by ID can be expensive. Initially ingest what normal library/page/search requests already return. Fetch permitted playlist contents when opened or explicitly needed, not every playlist as a search prerequisite. [2026 migration guide](https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide).

Cadence's library fingerprint compares only the first 50 liked IDs, first 50 playlist IDs/names/snapshots, and totals. This is a useful change heuristic, not a complete synchronization proof: a rename or track edit in playlist 80 can leave it unchanged; track metadata can change without saved IDs changing. Keep this cheap probe, but add age-based full reconciliation and record the actual time of successful complete validation. Per-playlist snapshots can avoid unnecessary content reloads for playlists that have been checked. Deletions must reach the search index. [Fingerprint implementation](../../src/backend.rs), [page size](../../src/spotify.rs), [Spotify rate-limit guidance](https://developer.spotify.com/documentation/web-api/concepts/rate-limits).

Search and synchronization should share the existing rate-limit gate. Prioritize foreground catalog requests over optional warming, apply `Retry-After`, and stop speculative work during quota exhaustion. Current public documentation offers no complete catalog download or general library change feed; the documented reads require reconciliation. Search freshness, unavailable tracks, and playback authorization remain server-dependent. Local searchable metadata is not offline audio. [API overview](https://developer.spotify.com/documentation/web-api), [rate limits](https://developer.spotify.com/documentation/web-api/concepts/rate-limits).

## Recommended implementation sequence

**First increment:** add “Your music” suggestions using the current authorized library's already-loaded tracks and playlist summaries; normalize once, scan locally, cap results, keep selection stable, and preserve Return for remote search. Add bounded exact-query memory caching independently. This tests whether useful familiar-music answers arrive sooner without an import pipeline or database migration.

**Second increment:** establish account binding, metadata age/retention, and deletion propagation; then ingest eligible history/favorites and recently opened pages into a bounded corpus. Publish local and remote results independently, add remote debounce if live catalog search is desired, and perform full library reconciliation when due. Keep the original cached collection visible during refresh only when its age/directives permit.

**Third increment, only with evidence:** add FTS5 if scan profiling warrants it; add durable page/entity caching if cross-launch cache misses dominate. Expand eligible playlist coverage only when measured search misses justify the request and storage cost.

Validate with known-library queries, repeated catalog queries, unfamiliar artists, typo/punctuation/non-Latin queries, large libraries, account changes, expiry, removal, offline behavior, and throttling. Compare time to first useful result separately from final Spotify response, and record local hit usefulness, requests per search, p50/p95 latency, memory, and index-update cost. The speed claim to earn is “familiar music appears immediately while catalog search continues,” with measured bounds.
