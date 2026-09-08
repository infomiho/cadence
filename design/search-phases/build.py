"""Regenerate the static search wireframes and their three HTML review pages.

SVG primitives and viewer structure follow the wireframe skill. No dependencies.
"""
from pathlib import Path
from html import escape as esc
from textwrap import wrap

ROOT = Path(__file__).parent
OUT = ROOT / 'wireframes'


def text(x, y, value, size=14, muted=False, weight=400, anchor='start'):
    return f'<text x="{x}" y="{y}" font-size="{size}" font-weight="{weight}" text-anchor="{anchor}" stroke="none" fill="{"#666" if muted else "#000"}">{esc(value)}</text>'


def rect(x, y, w, h, fill='#fff', radius=0):
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{radius}" fill="{fill}"/>'


def line(x, y, x2, y2):
    return f'<line x1="{x}" y1="{y}" x2="{x2}" y2="{y2}"/>'


def button(x, y, w, label):
    return f'<g transform="translate({x},{y})">{rect(0,0,w,40,radius=4)}{text(round(w/16)*8,24,label,anchor="middle")}</g>'


def art(x, y, size=40):
    return rect(x,y,size,size,'#e6e6e6')+line(x,y,x+size,y+size)+line(x+size,y,x,y+size)


def row(y, title, artist, meta='Track', selected=False, action='Play'):
    # Adapted list-row primitive: stable title/artist positions across all states.
    return (f'<g data-region="result-row" transform="translate(264,{y})">'
            +rect(0,0,984,64,'#e6e6e6' if selected else '#fff')
            +art(16,8)+text(72,24,title,weight=600)+text(72,48,artist,12,True)
            +text(696,32,meta,12,True)+button(864,8,104,action)+'</g>')


def note(n, x, y):
    return f'<g transform="translate({x},{y})"><circle cx="0" cy="0" r="12" fill="#fff" stroke="#d33" stroke-dasharray="4 2"/><text x="0" y="0" dominant-baseline="central" font-size="12" font-weight="700" text-anchor="middle" stroke="none" fill="#d33">{n}</text></g>'


def svg(title, content, height=800):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="{height}" viewBox="0 0 1280 {height}" '
            'font-family="-apple-system, system-ui, sans-serif" fill="#fff" stroke="#000" stroke-width="1.5" role="img">'
            f'<!-- Desktop canvas: 1280 × {height}. Static wireframe. --><title>{esc(title)}</title>'
            +content+'</svg>')


TRACK = ('Teardrop', 'Massive Attack · Mezzanine', 'Liked song')
ANGEL = ('Angel', 'Massive Attack · Mezzanine', 'Opened this session')
REMOTE = ('Teardrop', 'José González · In Our Nature', 'Track')
REMOTE2 = ('Teardrop — Live', 'Massive Attack · Live recording', 'Track')


def screen(slug, title, trigger, query='', local=None, remote=None, status='', hint='',
           local_title='Your music', remote_title='On Spotify', action='Search Spotify',
           selected=False, mode='', footer='', annotations=None, why='', branch=False, next_step=''):
    return dict(slug=slug,title=title,trigger=trigger,query=query,local=local or [],remote=remote or [],
                status=status,hint=hint,local_title=local_title,remote_title=remote_title,action=action,
                selected=selected,mode=mode,footer=footer,annotations=annotations or [],why=why,
                branch=branch,next_step=next_step)


PHASES = [
 dict(number=1, name='Skateboard', title='Find your library instantly',
      summary='Search familiar music as you type. Ask Spotify for more when you need it.',
      change='Adds immediate library matches and repeat-query reuse. Catalog search stays explicit.',
      assumptions='Signed in with Spotify connected and a previously loaded library. Examples show fixture music, not your account data.',
      question='Should the proposed “Your music” section sit above Spotify results, as shown, or should it be a filter within the existing Tracks tab?',
      screens=[
       screen('01-start','Start with your library','Open search with Command-K.',
              local=[TRACK,('Roads','Portishead · Dummy','Liked song')],local_title='Liked songs',mode='start',
              hint='Type a song, artist, album or playlist.',status='Search Spotify when you want to explore beyond your library.',
              annotations=['The search field gets focus. No catalog request is sent on entry.','Existing liked songs give the page useful content before a query.','Spotify stays an explicit action in this phase.'],
              why='The first version uses data Cadence already has.',next_step='Type “massive teard”.'),
       screen('02-local','A familiar song appears','Type “massive teard”; local matching runs on each change.',query='massive teard',local=[TRACK],
              hint='1 match in your library',status='Explore the Spotify catalog for “massive teard”.',selected=True,
              annotations=['Matching spans title and artist; no need to type the exact title.','This is a proposed local-result row within Cadence’s existing Search results page.','Return sends the query to Spotify; playing any row still requires a connected Spotify session.'],
              why='Finding a known song can finish before any catalog request.',next_step='Press Return to see more.'),
       screen('03-request','Keep the local answer visible','Press Return in the existing toolbar search field.',query='massive teard',local=[TRACK],selected=True,
              hint='1 match in your library',status='Searching Spotify for “massive teard”…',mode='loading',
              annotations=['The submitted query remains visible in the existing toolbar field.','The local row remains visible while Spotify responds.','Loading belongs to the Spotify section, not the whole page.'],
              why='A slower catalog response does not block a usable local result.',next_step='Spotify returns additional matches.'),
       screen('04-results','Expand into the catalog','The submitted catalog request completes.',query='massive teard',local=[TRACK],remote=[REMOTE2],selected=True,
              hint='1 match in your library',status='More matches for “massive teard”.',
              annotations=['The screen still answers the submitted query.','The familiar song keeps its position.','Additional Spotify rows appear below; duplicate IDs appear once.'],
              why='Repeating this exact query can reuse a fresh memory entry without a new request.',next_step='Play a result or change the query.'),
       screen('05-no-local','No match in this library','Branch: type a query absent from the loaded library.',query='nala sinephro',
              hint='No library matches for “nala sinephro”.',status='Search Spotify to find music outside your library.',mode='empty',branch=True,
              annotations=['The query is preserved in the toolbar so it can be submitted unchanged.','The empty state describes only local-library coverage.','Press Return to search Spotify; this is not a catalog “no results” state.'],
              why='A small local collection must not imply that the song does not exist.',next_step='Submit the catalog search; continue at screen 3.'),
      ]),
 dict(number=2,name='Bicycle',title='One search, arriving in layers',
      summary='Familiar matches arrive first. Spotify fills in the rest automatically, while the result you selected stays put.',
      change='Adds automatic catalog search and a temporary collection of music opened during this session.',
      assumptions='Signed in with Spotify connected and library data loaded. Times describe event order, not measured latency.',
      question='The proposed “Already in Cadence” section includes library music and things opened this session. Should those sources stay together or have separate headings?',
      screens=[
       screen('01-typing','The first answer is local','Type “teardrop”; remote work waits for a short typing pause.',query='teardrop',local=[TRACK],
              local_title='Already in Cadence',hint='1 match · from your library',status='Finding more on Spotify…',action='Search now',mode='loading',selected=True,
              annotations=['Local matching happens immediately; only the catalog request is delayed.','The local row stays visible while the remote search is pending.','The catalog section has its own progress state. Return skips the typing pause.'],
              why='The same field serves familiar-music lookup and discovery.',next_step='Spotify responds after the typing pause.'),
       screen('02-enriched','Add results without moving the target','The catalog response arrives.',query='teardrop',local=[TRACK],remote=[REMOTE,REMOTE2],
              local_title='Already in Cadence',hint='1 match · from your library',status='More matches for “teardrop”.',action='Search now',selected=True,
              annotations=['Results are accepted only for the current query.','The local row does not move when new catalog rows arrive.','New catalog rows append in Spotify order, excluding duplicate IDs.'],
              why='Late results must not change what the next Return press will play.',next_step='Open Mezzanine from the first result, then search for “angel”.'),
       screen('03-known','Visited music becomes searchable','After opening Mezzanine, type “angel” in the same session.',query='angel',local=[ANGEL],
              local_title='Already in Cadence',hint='1 match · from an album you opened',status='Finding more on Spotify…',action='Search now',mode='loading',
              annotations=['This query differs from the earlier “teardrop” query.','Angel appears from metadata obtained when Mezzanine was opened.','The exact catalog query still runs; local coverage is not a complete catalog.'],
              why='Normal browsing expands what Cadence can answer locally without a separate download.',next_step='Return to the previous “teardrop” query.'),
       screen('04-repeat','A repeat search is already here','Return to “teardrop” while the exact-query entry is fresh.',query='teardrop',local=[TRACK],remote=[REMOTE,REMOTE2],
              local_title='Already in Cadence',hint='1 match · from your library',status='More matches for “teardrop”.',action='Search now',
              annotations=['The exact query and account context match an eligible stored response.','Local results come from the current session collection.','Catalog rows appear without a loading flash; no cache terminology is needed in the app.'],
              why='A revisit looks like an ordinary finished search.',next_step='Select a result, or continue with a new query.'),
       screen('05-unavailable','A Spotify failure stays contained','Branch: the current catalog request fails.',query='teardrop',local=[TRACK],
              local_title='Already in Cadence',hint='1 match · from your library',status='Spotify search is unavailable. Try again in a moment.',action='Try again',mode='error',branch=True,
              annotations=['The query survives the failed request.','Local metadata remains browsable. Playback still depends on the playback connection.','The failure and retry action stay beside the Spotify section.'],
              why='One failed source should not erase the useful answer from another.',next_step='Retry when available; retain the local selection.'),
      ]),
 dict(number=3,name='Full system',title='Your music, ready when you return',
      summary='A bounded collection survives app restarts. Search becomes more forgiving while Cadence gradually refreshes eligible metadata.',
      change='Adds persistence, broader local coverage, better matching and gradual background refresh without changing the library sidebar.',
      assumptions='Returning screens use eligible metadata bound to the signed-in account. First-run and signed-out states are separate branches.',
      question='When catalog matches arrive, should Cadence preserve the visible order until the next query, as shown, or re-rank the entire list immediately?',
      screens=[
       screen('01-return','Resume without an import screen','Reopen Cadence with the same signed-in account.',query='',
              local=[TRACK,('Angel','Massive Attack · Mezzanine','Recently opened')],mode='start',action='Search now',
              hint='Search songs, artists, albums and playlists.',status='Results from Spotify appear here.',footer='Refreshing familiar music in the background…',
              annotations=['The existing toolbar search is available as soon as eligible saved metadata loads.','Known tracks survive the app restart.','Spotify remains available for unfamiliar music when Cadence is connected.','Refreshing happens quietly and does not postpone search.'],
              why='Returning users should benefit immediately from previous browsing.',next_step='Type “massiv teardop”.'),
       screen('02-forgiving','Find music despite imperfect spelling','Type “massiv teardop”.',query='massiv teardop',local=[TRACK],selected=True,
              hint='Quick matches · loading more results…',status='Searching Spotify for “massiv teardop”…',action='Search now',mode='loading',
              annotations=['Keep the original input visible; do not silently rewrite the catalog query.','The first row is a quick preview from eligible saved metadata.','The list communicates that more Spotify results are still arriving, without naming an implementation source.'],
              why='Familiar music should be findable even when the name is only partly remembered.',next_step='Keep searching while Cadence refreshes eligible metadata.'),
       screen('03-refresh','Keep results fresh quietly','Eligible familiar-music metadata refreshes while the search stays open.',query='massiv teardop',local=[TRACK],
              hint='Quick matches · loading more results…',status='Searching Spotify for “massiv teardop”…',action='Search now',mode='loading',
              annotations=['The query stays in the toolbar while the refresh runs.','Refreshing is deliberately not a new control or section in the Search page.','Only eligible, already-known metadata can make the quick preview faster.'],
              why='Freshness improves early results without changing the visible search model.',next_step='Spotify completes the search.'),
       screen('04-result','Complete without a reshuffle','The catalog response completes for a forgiving local match.',query='massiv teardop',local=[TRACK],remote=[REMOTE,REMOTE2],
              hint='3 results',status='Results for “massiv teardop”.',action='Search now',mode='result',selected=True,
              annotations=['The original query remains visible; Cadence does not silently rewrite it.','The first visible quick preview holds its position while new rows are appended.','Every row is a Spotify result; local metadata only made the first answer available sooner.'],
              why='Patching results into one stable list avoids the visual jump of replacing one source with another.',next_step='Play a result while Spotify is connected, or keep searching.'),
       screen('05-first-run','First launch stays usable','Branch: the library is still loading for a new account.',query='teardrop',local=[],remote=[('Teardrop','Massive Attack · Mezzanine','Track'),REMOTE],
              local_title='Your library',hint='No match in the part of your library loaded so far.',status='Matches for “teardrop”.',action='Search now',mode='import',branch=True,
              footer='Loading your library · 150 of 820 liked songs',
              annotations=['Search stays active during the initial import.','Partial coverage is explicitly identified; it is not a definitive empty result.','Catalog search can answer before the local collection is complete.','Progress is nonblocking; counts are illustrative fixtures.'],
              why='A new user should not have to wait for a whole library before trying search.',next_step='New library items become searchable as they load.'),
       screen('06-signed-out','Cadence blocks the workspace when Spotify is not connected','Branch: the saved Spotify session is unavailable.',query='teardrop',local=[],
              local_title='Your library',hint='Spotify connection required.',status='Sign in to search and play music.',action='Open sign-in',mode='signed-out',branch=True,
              annotations=['This follows Cadence’s existing signed-out scrim behavior.','Local metadata is not presented as an offline-search or offline-playback mode.','Signing in is required before Cadence exposes the workspace again.'],
              why='A local search cache must not imply that Spotify playback works without a connection.',next_step='Finish signing in, then return to the search flow.'),
      ]),
]

# The review now presents only the final product state.
PHASES = [PHASES[-1]]


def render_screen(phase, s):
    # Current Cadence chrome: library-only sidebar; search lives in the toolbar.
    parts=[rect(0,0,1280,800),rect(0,0,232,704),rect(232,0,1048,72)]
    parts += [art(24,48,32),text(72,72,'Cadence',20,weight=600),text(24,136,'Library',12,True)]
    for y,label in [(184,'Liked Songs'),(232,'Favorites'),(280,'Playlists'),(328,'Recently played')]:
        parts.append(text(40,y,label))
    parts += [text(24,392,'Pinned Playlists',12,True),text(40,432,'Night drive'),text(40,472,'Sunday morning')]
    parts += ['<!-- 1: Existing toolbar search field -->',rect(260,16,520,40,radius=8),'<circle cx="284" cy="36" r="6"/>',line(288,40,296,48),
              text(312,41,s['query'] or 'Search Spotify',14,not bool(s['query'])),rect(728,24,36,20,'#e6e6e6',4),text(746,39,'⌘ K',12,anchor='middle'),
              '<circle cx="1228" cy="36" r="20" fill="#e6e6e6"/>',text(264,120,'Search results',28,weight=700),text(264,152,f'Results for {s["query"] or "…"}',14,True)]
    parts += [line(264,208,1248,208),text(280,192,'Tracks',14,weight=600),line(272,208,328,208),text(360,192,'Playlists',14,True)]
    # One user-facing Spotify result list. The first rows may be rendered from
    # eligible local metadata, but no source boundary appears in the product.
    parts += ['<!-- Proposed: one stable Spotify result list -->',text(264,256,'Results',20,weight=600),text(264,280,s['hint'],12,True)]
    visible_rows = s['local'][:2] + s['remote'][:2]
    for i,r in enumerate(visible_rows):
        parts.append(row(296+i*64,*r,selected=s['selected'] and i==0))
    continuation_y = 296 + len(visible_rows)*64
    if s['mode']=='loading':
        parts += [rect(264,continuation_y,984,48,'#f7f7f7'),text(288,continuation_y+30,'Loading more results…',14,True)]
    elif not visible_rows:
        parts += [rect(264,296,984,96),text(288,336,'Your library is still loading.' if s['mode']=='import' else 'No results yet.',20,weight=600),
                  text(288,368,'Spotify results are ready while your library loads.' if s['mode']=='import' else 'Press Return to search Spotify.',14,True)]
    parts += [text(264,680,s['status'],12,True)]
    if s['footer']:
        parts += ['<!-- 4: Background refresh status -->',rect(264,640,984,40),text(288,664,s['footer'],12,True)]
    parts += [rect(0,704,1280,96),art(24,728,48),text(88,744,'Nothing playing',14,weight=600),text(88,768,'Choose a track to listen',12,True),
              button(536,728,56,'‹'),button(616,728,56,'▷'),button(696,728,56,'›'),line(824,752,1048,752),text(1120,760,'Queue',14,True)]
    if s['mode']=='signed-out':
        parts += [rect(0,0,1280,800,'#fff'),rect(376,272,528,216,radius=8),text(640,328,'Signed out of Spotify',20,weight=600,anchor='middle'),
                  text(640,368,'Finish signing in to keep listening.',14,True,anchor='middle'),button(576,400,128,'Open sign-in')]
    points=[(792,16),(248,304),(248,440),(248,664)]
    parts += ['<g data-region="annotations">']+[note(i+1,*points[i]) for i in range(len(s['annotations']))]+['</g>']
    return ''.join(parts)


def wire(src,label,alt):
    return f'<button class="wire" type="button" data-zoom data-caption="{esc(label,quote=True)}" aria-label="Enlarge {esc(label,quote=True)}"><span class="label"><span>{esc(label)}</span><span>Enlarge ↗</span></span><img src="{src}" alt="{esc(alt,quote=True)}" width="1280" height="800"></button>'


def layout(title, nav, body):
    return f'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{esc(title)} · Cadence wireframes</title><link rel="stylesheet" href="viewer.css"></head>
<body><a class="skip" href="#content">Skip to content</a><div class="shell">
<details class="toc" open><summary class="toc-summary"><span><span class="crumb">Cadence · Search phases</span><br><span class="title">Browse the flow</span></span><span class="chevron" aria-hidden="true"></span></summary>
<nav class="toc-body" aria-label="Review navigation"><a class="brand" href="index.html">Cadence / Search</a><h2>System</h2><a href="final.html">Final state</a>{nav}</nav></details>
<main id="content">{body}<footer class="footer">Design study · 1280 × 800 desktop screens · Fictional example library<br>Monochrome wireframes. Red dashed numbers are reviewer annotations, not product UI. These pages review states; the depicted music controls are not a working player.</footer></main></div>
<dialog id="lightbox" aria-labelledby="zoom-caption"><div class="zoom-toolbar"><p id="zoom-caption"></p><button type="button" id="zoom-close" autofocus>Close ×</button></div><div class="zoom-body"><img id="zoom-image" alt=""></div></dialog>
<script src="viewer.js"></script></body></html>'''


def build():
    OUT.mkdir(exist_ok=True)
    cards=[]
    for p in PHASES:
        n=p['number']; page='final'; frames=[]; sections=[]; nav='<h2>Flow</h2><a href="#flow">Overview</a><h2>Screens</h2>'
        for i,s in enumerate(p['screens']):
            filename=f'{page}-{s["slug"]}.svg'; content=render_screen(p,s); frames.append(content)
            (OUT/filename).write_text(svg(s['title'],content))
            label=f'{i+1:02} · {s["title"]}'
            nav+=f'<a href="#s{i+1}">{esc(label)}</a>'
            notes=''.join(f'<li><b>{j+1}</b><span>{esc(a)}</span></li>' for j,a in enumerate(s['annotations']))
            sections.append(f'''<section id="s{i+1}"><p class="lede">{'Alternate path' if s['branch'] else 'Main flow'} · Screen {i+1:02}</p>
<h2>{esc(s['title'])}</h2><p class="desc">{esc(s['trigger'])}</p><div class="grid">{wire('wireframes/'+filename,label,s['title']+'. '+s['hint']+' '+s['status'])}
<div class="notes"><h3>What to notice</h3><ol class="annotations">{notes}</ol><div class="why">{esc(s['why'])}</div><p class="next"><b>Next:</b> {esc(s['next_step'])}</p><a class="source-link" href="wireframes/{filename}">Open SVG ↗</a></div></div></section>''')
        # Four main-path thumbnails left-to-right, plus alternate-path thumbnails below.
        flow=[rect(0,0,1280,656),text(32,40,'Main flow',20,weight=600)]
        for i,content in enumerate(frames):
            x=32+(i%4)*320; y=64 if i<4 else 384
            flow += [f'<g transform="translate({x},{y}) scale(0.2)">{content}</g>']
            flow += [text(x,y+192+j*24,part,14,weight=600) for j,part in enumerate(wrap(f'{i+1:02} · {p["screens"][i]["title"]}',32))]
            if i<3:
                flow += [line(x+264,y+80,x+296,y+80),f'<polygon points="{x+296},{y+80} {x+288},{y+72} {x+288},{y+88}" fill="#000" stroke="none"/>']
        flow += [text(32,352,'Alternate paths',20,weight=600),text(32,640,'Main sequence above. Branches below show a different starting condition or a failed request.',14,True)]
        flowfile=f'{page}-flow.svg'; (OUT/flowfile).write_text(svg(p['title']+' — flow overview',''.join(flow),656))
        nav+='<h2>Review</h2><a href="#decision">Decision to review</a>'
        hero=f'''<header class="hero"><div class="crumb">cadence-th3 · Final system</div><h1>{p['title']}</h1><p>{p['summary']}</p><div class="pills"><span class="pill">{len(frames)} screens</span><span class="pill">Desktop · 1280 × 800</span><span class="pill">Lo-fi · monochrome</span></div><div class="phase-change"><b>What the final system includes</b><br>{p['change']}</div></header>'''
        overview=f'''<section id="flow" class="flow-section"><p class="lede">At a glance</p><h2>The complete flow</h2><p class="desc">Read the main sequence left to right. Scroll down for full-size screens and alternate paths.</p>{wire('wireframes/'+flowfile,'Final-system flow overview','Four sequential search screens, followed by alternate paths.')}<p class="assumption">{p['assumptions']}</p></section>'''
        decision=f'''<section id="decision"><p class="lede">For review</p><h2>One interaction decision</h2><p class="desc">{p['question']}</p></section>'''
        (ROOT/f'{page}.html').write_text(layout(p['title'],nav,hero+overview+''.join(sections)+decision))
        cards.append(f'''<article class="phase-card"><p class="lede">Final system</p><h2>{p['title']}</h2><p>{p['summary']}</p><a href="final.html"><img src="wireframes/{page}-{p['screens'][1]['slug']}.svg" alt="Preview: {p['title']}" width="1280" height="800"><span>Explore {len(frames)} screens →</span></a></article>''')
    index='<header class="hero"><div class="crumb">cadence-th3 · Search design study</div><h1>Your music, ready when you return.</h1><p>The final layered-search system: eligible local metadata answers familiar queries while Spotify supplies catalog discovery.</p><div class="pills"><span class="pill">Final system</span><span class="pill">6 desktop screens</span><span class="pill">Static wireframes · zoom to inspect</span></div></header><div class="phase-cards">'+''.join(cards)+'</div>'
    (ROOT/'index.html').write_text(layout('Final search system','',index))
    print('Generated 2 HTML pages and 7 SVG files.')


if __name__=='__main__':
    build()
