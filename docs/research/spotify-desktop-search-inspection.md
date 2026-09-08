# Spotify desktop search: installed-code inspection and Cadence implications

Inspected 2026-09-06 for Beads `cadence-859`. This is static analysis, not a latency benchmark. Public API capabilities and constraints are recorded in [the API research](spotify-search-api-2026-09.md). The proposed local metadata layer is developed in [local search with API enrichment](local-search-api-layering.md).

## Evidence and reproducibility

The installed `/Applications/Spotify.app` identifies itself in `Contents/Info.plist` as **1.2.96.518**. It includes `Chromium Embedded Framework.framework`. The desktop shell is CEF-based; describing it as Electron is inaccurate for this installation.

`Contents/Resources/Apps/xpui.spa` is a ZIP archive with 543 entries, including 162 JavaScript files and no `.map` files. Its SHA-256 is `c605c792c8f74004bd4f1b197b5244a9d64696455da74fe4fd221502683336a6`. JavaScript, JSON and HTML were extracted to `/tmp/cadence-spotify-inspection`; `search-evidence.json` there records key snippets and their offsets. The installed app was not modified or launched for this inspection.

The principal evidence is `xpui-routes-search.js`, SHA-256 `94611bb6bcf453286e8302f7cec4776b440ad04f8e8e7852283026a758a447f8`. The shipped `ui-licenses.html` lists `@tanstack/react-query@5.90.20` and `@tanstack/query-core@5.90.20`, consistent with the query methods used by the search route. License entries for persistence packages alone do not establish that search results are persisted to disk.

The archive's `index.html` references `/xpui-snapshot.js`, which is not a plain JavaScript entry in this archive. The installed CEF resources include `v8_context_snapshot.arm64.bin`, containing query-client method names. Some imported shared modules were not available as ordinary JS in the extracted files. This limits claims about global defaults, the input's debounce interval, transport internals, and native caching.

To reproduce the core inspection without modifying the installation:

```python
import hashlib
import zipfile
from pathlib import Path

archive = Path('/Applications/Spotify.app/Contents/Resources/Apps/xpui.spa')
print(hashlib.sha256(archive.read_bytes()).hexdigest())
with zipfile.ZipFile(archive) as bundle:
    source = bundle.read('xpui-routes-search.js').decode()
    for needle in ('searchPageResults', 'searchCategoryResults',
                   'ensureQueryData', 'staleTime', 'searchDesktop'):
        start = 0
        while (pos := source.find(needle, start)) >= 0:
            print(needle, pos, source[max(0, pos-100):pos+350])
            start = pos + len(needle)
```

## What the shipped search code does

Offsets below are zero-based character offsets into the original minified `xpui-routes-search.js`, as decoded above. They are locators for this exact build, not stable symbols across releases.

| Observation | Evidence locator | Meaning |
| --- | --- | --- |
| Main query uses `ensureQueryData`, keyed by `searchPageResults`, query text, result-size and feature options | approximately 54,950–55,700 | Repeated request identities can reuse cached data. |
| Main query sets `gcTime:9e5` and `staleTime:9e5` | approximately 55,590 | Both configured intervals are 900,000 ms, or 15 minutes. |
| Category queries use `searchCategoryResults`, query, category, page size and feature options | approximately 8,650–9,510 | Tracks/albums/etc. have their own paginated query identity. |
| Category query sets `gcTime:6e5`, `staleTime:3e5` | approximately 9,470 | Ten-minute inactive retention; five-minute freshness. |
| Category cleanup truncates cached `pages` and `pageParams` to their first two entries | approximately 9,510–9,650 | It retains useful initial pages while trimming deep pagination. |
| Main request forwards an abort signal and uses an effect-local flag before publishing results | approximately 55,190–55,850 | Late results from an obsolete effect cannot overwrite current UI state. Signal plumbing alone does not prove query changes abort every network request. |
| Old results and their `queryForResults` stay in component state during the next request | approximately 54,350–55,950 | Existing results can remain visible while a new result is fetched. |
| A persisted query named `searchDesktop` is passed through the request abstraction | approximately 42,010 and 48,230 | This is an internal GraphQL-style operation, not a call to documented `/v1/search`. |
| A feature-dependent alternate `getSearchResultsList` path invokes `searchTopResultsList` | approximately 46,778 and 49,090 | There is more than one search implementation; static presence does not identify the user's active experiment. |

The alternate path can request 50 results while the other path requests 10, with different top-result counts. These internal parameters are not evidence that a public Development Mode client can exceed its documented limits.

**Freshness is not a hard deletion deadline or a polling interval.** TanStack distinguishes `staleTime` from inactive garbage collection. Its v5 `ensureQueryData` returns existing data; stale revalidation requires an additional option. This call site does not explicitly set `revalidateIfStale`, so it would be inaccurate to say Spotify necessarily refreshes search every 15 minutes or uses stale-while-revalidate here. Global defaults were not recovered. [TanStack defaults](https://tanstack.com/query/latest/docs/framework/react/guides/important-defaults), [TanStack maintainer's explanation of imperative methods](https://github.com/TanStack/query/discussions/9135)

The archive's `index.html` also declares preconnect hints for `api.spotify.com`, `api-partner.spotify.com`, `spclient.wg.spotify.com` and artwork hosts. This shows connection setup is considered, but does not measure whether those hints improve a particular search.

## Cadence's current search path

Source inspected in this workspace:

- [Toolbar](../../src/app/chrome.rs): typing updates query state; Return submits. There is no live-search debounce to tune in the current flow.
- [Search page](../../src/app/catalog.rs): holds the current track and playlist results, but no map of previous queries. It retains old nonempty lists during a new request; the heading uses the input query while list identity uses `results_query`, so retained results are not consistently identified by their originating query.
- [Catalog backend](../../src/backend.rs), `CatalogFetches::search`: starts track and playlist requests concurrently with `tokio::try_join!`. A new search aborts the previous backend task. Results are delivered as one tuple after both requests succeed; one category's failure fails the combined reply.
- [Spotify adapter](../../src/spotify.rs), `search_tracks` and `search_playlists`: makes two public API calls, each requesting 10 items. No application search-result cache or conditional-request handling appears in these functions. The adapter already shares a rate-limit cooldown gate.

For a successful cold search, this structure waits approximately `max(track request, playlist request)` plus local overhead, not their sum. It nevertheless delays tracks when playlists are slower. Resubmitting the same query starts both requests again.

## Recommended caching design

This is an engineering proposal, not an implemented feature or a claim about a universal industry standard.

1. **Cache the exact request and render hits immediately.** Start with bounded in-memory storage. Include account/session generation, effective market, trimmed query, types, offset, limit and any result-affecting options in the key. Preserve quotes, operators, punctuation and accents. Use the same query normalization for sending and keying. Drop the account's cached state on logout; reject late writes from an old account generation.
2. **Separate freshness, stale display, and eviction.** Honor actual response cache directives and validators first. Where permitted, a provisional fallback could use five-minute freshness, fifteen-minute maximum age and a 100-query or byte-budget bound. These are tuning hypotheses, not Spotify-mandated TTLs. Fresh hits need no request. Stale entries may display while one refresh runs only when allowed; expired entries are removed. Do not reset content age merely because it was read.
3. **Deduplicate by key and keep latest-query UI semantics.** Repeated submissions should share one in-flight request. Cancel obsolete queued work; abort an active request when nobody needs it. Check query/account generation before updating the screen. A request already accepted by Spotify may still count against its limits after cancellation.
4. **Fetch only the categories the interaction needs.** For the current Tracks/Playlists tabs, request the selected tab first and fetch the other on selection. If a future screen displays both at once, the public API supports comma-separated types in one request. Combining calls reduces request count but is not proof of lower latency; independent category replies are another option when partial display matters.
5. **For live search, resolve local work before debounce.** Show a fresh exact-query hit immediately. Search already-loaded library metadata for clearly identified local suggestions. Debounce only a remote cache miss, starting around 150–200 ms and measuring the tradeoff. Return should flush the pending delay. Algolia recommends around 200 ms as a starting point; this is not a recovered Spotify setting. [Algolia performance guidance](https://www.algolia.com/doc/guides/building-search-ui/going-further/improve-performance/js)
6. **Never treat a prefix page as the catalog.** Ten results for `radio` are not a complete index for `radiohead`. Reuse them only as provisional suggestions, then obtain the exact query. Local-library search also has narrower coverage and different ranking from Spotify's catalog.
7. **Spend requests on demonstrated intent.** Load further pages on demand. Avoid enumerating possible prefixes or prefetching entire catalogs. Keep empty successful results distinct from transport errors; a timeout or 429 must not become cached “no results.” Retain eligible cached content during transient refresh failures.

Temporary metadata caching and conditional HTTP requests are documented, but the desktop client's internal endpoint and cache settings do not grant equivalent public API permissions. A `304` still requires a round trip, and there is no evidence here that it is free of quota cost. See [the public API findings](spotify-search-api-2026-09.md) for current limits and headers.

## What the performance claim can honestly be

| Interaction | What caching can remove | Remaining limit |
| --- | --- | --- |
| Repeat an exact recent query | Network waiting when a usable cache entry exists | Local lookup and rendering; Spotify also caches. |
| Find a track in already-loaded library data | Remote search entirely for that local match | Narrower coverage/ranking; distinguish from catalog results. |
| Type a novel catalog query | Duplicate/obsolete requests and avoidable UI waiting | Public endpoint latency, quota, and retrieval quality. |
| Revalidate an expired result | Potential response transfer/parsing via ETag | A network round trip remains. |

The code supports “caching is a real way to avoid repeated waits.” It supports neither “Cadence is faster than Spotify” nor “using an API makes Cadence inevitably slower.” Spotify uses APIs and caches, and it has internal services unavailable through the documented public contract.

A fair benchmark would record input-to-first-useful-result and input-to-final-catalog-result separately. Compare cold novel queries, warm repeats, backspacing and local-library matches on the same account/network/market. Record median/p95, cache-hit rate, requests per completed search, throttling, and time spent in input delay, HTTP and rendering. Count partial local suggestions separately from authoritative catalog completion. No such measurements were made in this research.
