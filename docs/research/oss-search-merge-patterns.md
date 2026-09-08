# OSS search merge patterns

Research date: 2026-09-06. This is a source-first review of open-source music
apps. The useful distinction is whether an app is joining **different music
services** or showing data about the **same service** at different speeds.

## Concrete implementations

### wave-tui — cached online search, query-generation guard

[wave-tui](https://github.com/takemo101/wave-tui) is a Rust terminal radio
player. Its repository documents online-as-you-type search with three explicit
mechanisms: approximately 350 ms debounce, a cache keyed by query, and ignoring
stale in-flight searches. Its normal result list is then filtered by the active
Browse selection; it does not display a cache section beside a network section.
The project also retains its curated built-in catalog and saved favourites when
the online search is unavailable. [README, search behaviour](https://github.com/takemo101/wave-tui#controls)

**Takeaway for Cadence:** give every submitted query a monotonically increasing
generation. A response may update the view only if its generation and normalized
query still match. Cache lookup is immediate; Spotify request is debounced and
cancellable/ignorable. The cache is not a user-visible source.

### minitone — source groups are for genuinely different services

[minitone](https://github.com/ldgnu/minitone) searches YouTube, Radio Browser,
Navidrome, a local filesystem library, and favourites. It intentionally groups
results by source, supports moving between those groups, and describes its
search layer as “multi-source search + fuzzy rank.” [README, usage and architecture](https://github.com/ldgnu/minitone#usage)

**Takeaway for Cadence:** this is the counterexample to copy carefully. Its
groups communicate material differences: provider, playback resolver, and
availability. Cadence’s quick candidates and Web API candidates both identify
Spotify objects and use the same playback path, so source groups would expose an
implementation detail without helping a choice.

### Navidrome — make local search authoritative and rank it once

[Navidrome](https://github.com/navidrome/navidrome) is a local-library server,
so its search is one local index rather than a progressive merge. Recent project
release notes record a migration to FTS5, exact-match-before-prefix ranking, and
Unicode normalization fixes. [Release notes](https://github.com/navidrome/navidrome/releases)

**Takeaway for Cadence:** use a single normalized matching function for the
quick-preview index (case folding, whitespace/punctuation normalization, then
exact/prefix/subsequence scoring). Do not imply that its ranking is Spotify’s
ranking. It exists only to produce a fast, bounded preview.

## Recommended Cadence behaviour

1. The user submits a query. Create an immutable `SearchSnapshot` containing a
   normalized query, a generation number, and empty ordered result IDs.
2. Immediately score the bounded local metadata index and render up to 3–5
   **provisional Spotify results**. They look like ordinary rows. The only
   feedback is a quiet trailing loading row, such as “Searching Spotify…”.
3. Start the Spotify request after a short debounce. If another query wins,
   cancel it where possible and always reject its result by generation check.
4. When Spotify responds, use Spotify IDs to deduplicate. Preserve provisional
   rows that Spotify also returned, updating their metadata in place. Append
   unseen remote rows in Spotify’s returned order. Do not move a row the user
   can currently see or has selected.
5. When the request completes, remove the trailing loading row. The next query
   creates a new snapshot and may use a new order. If Spotify fails, retain the
   preview and replace the trailing row with a retryable failure message.

This is not a claim that local results are playable offline. Every row remains a
Spotify object; playback still requires Cadence’s connected Spotify session.

## Decision

Treat the quick data as **optimistic rows in one Spotify list**, not as a first
result set to be reconciled into the “real” list and not as a second source. The
only explicit state is that more Spotify results are still arriving. This takes
the stale-response and cache discipline from wave-tui, while avoiding minitone’s
appropriate-but-inapplicable provider grouping.
