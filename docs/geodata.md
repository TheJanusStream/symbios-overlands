# Geodata regions

A geodata region is a square of real Berlin at real scale - a Kiez, walked
at full detail to its edge - dressed in the region's theme: Berlin's ground,
water, blocks, buildings and trees, built from the catalogue the region
would use anyway. The data is GDI Berlin's (<https://gdi.berlin.de>),
fetched when the region is visited. Epic #1580; the API research and the
owner's decisions are on #1579. The Kiez overhaul is epic #1636 (research
#1635).

Everything used here is under the Datenlizenz Deutschland - Zero - Version
2.0 (<https://www.govdata.de/dl-de/zero-2-0>): any use, no attribution
required. The region info credits the source anyway. One layer seen during
research is not zero-licensed and must not be used: `rbs_strab` (CC BY 3.0
DE).

## Decisions (owner, 2026-10-08)

- **Themed**, not a faithful copy: the region's `ThemeArchetype` dresses
  real Berlin.
- **Drawn by the DID**, like the other seeded categories, and lockable in
  the seed GUI. The draw picks the source (procedural or Berlin) and, for
  Berlin, a square: a random size and a random position.
- **Real scale, 1 m = 1 m both ways.** What matters is a realistic
  perspective standing on the ground, not the relief seen from afar.
  Compressing a large square onto a ~1 km world would either shrink the
  hills to mounds or make the slopes twenty times steeper. So the square is
  the region's real extent.
- **A Kiez, at full detail to its edge** (owner, 2026-10-13, #1636): a
  seeded region's square is a kilometre, and the World Editor may set any
  side from 250 m to 2 km. The whole square is the walkable ground, at
  street level - its terrain, land use, water, streets, buildings, trees and
  street furniture - with no horizon, no ring of coarser city round it and
  nothing beyond its edge, as a procedural world has nothing beyond its
  own. Until then squares ran to 19 km round a walkable kilometre, with a
  far field, a middle ring and a roaming detail patch (see
  [Removed](#removed-by-the-kiez-overhaul-1637)).
- **Fetched at visit**, with the app's own cache, not baked into packs.
  Requests are sized to the pixels needed, as the GDI viewer's
  zoom-fitting loading is.
- **Seeded into the record** (owner, 2026-10-13 and 2026-10-14, #1636;
  #1647): what stands on a square - its buildings, trees and street
  furniture - is written into the room's record as instance sets once its
  street level lands, the same on every client, adopted on save and never
  derived again. The owner edits a seeded copy as any other. Every copy is
  seeded; what is drawn near, far or not at all is the instance builder's
  budgets'. Until #1647 it was derived again on every load, with the
  owner's edits over it and the layers' content hashes in the record
  (#1588, #1590).

## How the data is read

GDI Berlin is GeoServer in EPSG:25833 (ETRS89 / UTM 33N, metres). It sends
`Access-Control-Allow-Origin: *`, so a browser can fetch it directly. There
is no coverage service (WCS), and client styles (`SLD_BODY`) are ignored.
Two kinds of request carry everything:

- **WMS `GetMap` decoded through its JSON legend.** A render of any square,
  at any pixel size, costs one small request. `GetLegendGraphic` with
  `format=application/json` names every class's exact colour, so the PNG
  reads back as data:
  - The terrain layer `c_dgm1` is drawn in one-metre height classes.
    Decoding each pixel's class and then smoothing within it gives 0.26 m
    mean error against the raw 1 m model, for 9 KB per 256 x 256 render.
  - Land use (`c_ua_realnutz_2015`) and building storeys
    (`gebaeude_geschosse`) decode to class ids. GeoServer's
    `format_options=antialias:none` keeps fills crisp.
- **WFS `GetFeature` as GeoJSON**, for street-level vectors with exact
  attributes: footprints, trees, water, blocks. It supports bbox,
  `propertyName`, `count`/`startIndex` paging, and `resultType=hits` to size
  a request first.

Measured on the recorded fixtures (`crates/geodata/tests/decode.rs`):

| Layer | Decoded against | Agreement |
| --- | --- | --- |
| terrain, 4 m pixels | raw DGM1, 4 m block means | 0.26 m mean error (class midpoints: 0.42 m); false one-metre steps on flat ground 0.7 % (midpoints: 6 %) |
| land use, 2 m pixels | WFS blocks | 99.9 % of pixels |
| storeys, 2 m pixels | WFS ALKIS footprints | 99 % of building pixels, 98 % of open ground |
| water, 2 m pixels | the Spree's published level, 30.5 m | settled at 30.54 m |

Traps, each found the hard way:

- **Legends are Latin-1** while calling themselves JSON. The parser falls
  back to Latin-1 when a body is not UTF-8.
- **`intervals` colour maps are off by one in their labels.** An entry's
  colour covers the values above the previous entry's *quantity* up to its
  own; the human labels are a metre lower. The terrain legend's first
  opaque class spans -7 m to 26 m (Berlin's ground never goes that low), so
  the decoder treats a class far wider than typical as open-ended, within
  one typical width of its inner bound.
- **The storeys layer is street-level only.** Its outline is a pixel wide at
  any scale, so at 40 m per pixel two thirds of the render is outline. The
  middle ring read the surface model less the ground instead (#1587, until
  #1637).
- **No-store.** WMS answers say `Cache-Control: no-cache, no-store`, so the
  browser keeps nothing: the app caches by request URL itself (#1582).
  That is why the builders emit one canonical string per request.
- **Raw elevation exists but is heavy:** 2 km GeoTIFF tiles of 16 MB,
  uncompressed float32, one row per strip. They support HTTP `Range`, so
  any row span can be read exactly (~1 MB per km^2 at 4 m). Kept in reserve
  should the core ever need more than the decoded classes give.

## Coverage and the square

A square may lie only wholly inside Berlin, so every layer has data under
all of it. Regions take sides of 250 m to 2 km (`geodata::square`); the
coverage itself holds squares to 19 km, the largest Berlin's shape allows.
`crates/geodata/tools/coverage.py` rasterises the ALKIS state and
borough boundaries into a 250 m grid: 13,787 cells (862 km^2) count as
inside, each with its borough. That grid is checked in as a 5 KB run-length
table. "Wholly inside" holds to the tool's 25 m sampling: a cell may reach
about one sample past the border, a sliver the decoders fill from its
neighbours.

`Coverage::place` picks uniformly among *every* whole-metre position where a
square of a side fits. It counts, per corner cell, the offsets that
give each footprint, so no part of Berlin is likelier than another and no
draw is rejected. A 19 km square has 751 positions; 19.01 km has none.
Seeded squares from fixed draws are pinned in a test, and an independent
Python implementation (`crates/geodata/tools/place_check.py`) reproduces
them. Changing the draw mapping moves every seeded region and needs a
migration, not a re-bless; #1637 made one, with the owner's decision, when
every seeded square became a kilometre.

The square's draw comes from the region's own salted stream in the app
(`ChaCha8Rng::seed_from_u64(room_seed ^ SALT)`, like every other seeded
stream), after the draw of the room's source (P3.1, below):
`Coverage::seeded_square` places a kilometre where it says, every position
it fits as likely as any other, whatever is built there. The crate maps it
to a square and owns no randomness.

## Code

`crates/geodata` (P0.1, #1581) is pure, with no I/O and no Bevy, so the app
and the wasm worker share it:

- `square`: `GeoSquare` and the sides a region's square may have;
- `berlin`: the host, the layer catalogue, `LandUse` (22 classes),
  `StoreyBand`, `Borough`, and `Coverage`; the ATKIS street and carriageway
  axes, read from a page of GeoJSON by `parse_axes` (#1595); the ALKIS
  buildings, the tree inventory and the street furniture (`parse_buildings`,
  `parse_trees`, `parse_furniture`, P2.2); the Environmental Atlas's blocks
  by urban-structure type, and the families they sort into (`parse_blocks`,
  `Development`, #1600);
- `latlon`: a grid point's latitude and longitude and back (#1599);
- `features`: one page of GeoJSON features, read once for every layer;
- `request`: canonical `GetMap`, `GetLegendGraphic` and `GetFeature` URLs;
- `legend`: GeoServer JSON legends, as value ranges or fill classes;
- `raster`: PNG to RGBA; terrain to heights with clamped smoothing;
  categorical layers to class ids, with outlines resolved by neighbour vote;
- `water`: a core's water level, and the ground shaped around it (P1.4).

`tests/fixtures/README.md` lists the recorded answers and the truth rasters.
`tools/fixtures.sh` and `tools/truth.py` regenerate them.

`src/geodata` (P0.2, #1582) is the app's I/O half:

- `GeoRequest`: one request, carrying its byte cap. Its canonical URL is
  also its cache key. Only `https://gdi.berlin.de/services/` is asked, and
  no render past the decoders' pixel cap. An answer redirected off that
  prefix is refused: the browser follows redirects anywhere, and the
  native client to any public host. Each request also says what a valid
  answer looks like. GeoServer reports a bad request as an XML exception,
  sometimes under a success status, so nothing is kept or used without
  passing that check.
- `GeoStore`: where answers are kept between visits, and for at most 30
  days, because a URL's answer changes when Berlin updates its data
  without the URL changing.
  - Natively, it is a directory under the platform cache. An empty or
    relative `XDG_CACHE_HOME` is ignored, as the XDG spec says. There is
    one file per entry; each names its own URL and carries a checksum of
    its body, so a damaged file reads as no entry. Each write goes through
    a temporary file of its own and a rename. The directory is capped at
    256 MiB and drops the least recently used entries first; only the
    store's own files are counted or deleted.
  - On the web, it is the browser's Cache API (`store_browser.rs`). That
    file was verified in Chromium by compiling it into a scratch wasm crate.
  - Bumping `CACHE_EPOCH` discards everything kept under the old value.
- `GeoFetcher`: the resource the app asks. It merges identical requests and
  runs at most four fetches at once. It retries transient failures after 2 s
  and 8 s, three attempts in all. It counts progress for a loading screen.
  Answers come back by id.
  - Natively, the HTTP request runs on the shared Tokio runtime and is
    awaited, so it never holds one of Bevy's few I/O pool threads.
  - The frame system takes the resource mutably only when a fetch has
    finished or one may start. Frames spent waiting do not mark it
    changed.

The room record (P1.1, #1583) carries the source as an optional top-level
`geo_source`: `{dataset, min_e, min_n, size_m}`, in whole metres, and once
the square is seeded into the record its mark, `seeded`, a quoted decimal
(#1647). When it is absent, the field is elided, so every older record and
every seeded room stays byte-identical. The bytes are pinned in
`tests/fixtures/geo_source_wire.jsonl`.

- **Sanitiser.** A Berlin square has its side snapped to a whole 10 m
  within 250 m - 2 km (`snap_size`) and is moved, if it must be, to the
  nearest position wholly inside Berlin (`Coverage::nearest`, exact). A
  square saved wider than 2 km, before #1637, shrinks round its middle,
  where its full-detail ground lay; anything placed more than a kilometre
  from the middle now stands past the world's edge. A square moved or
  shrunk keeps its mark, which then names another square: it is seeded
  anew. A dataset id that is not a plain name drops the source. Another dataset's
  square is kept for the build that can draw it, though keys this build does
  not know are dropped on save, as from every record field.
- **Editor.** The World Editor's Environment tab has a Region source
  section. It can turn Berlin on (a kilometre, drawn the way a seeded
  region draws one), draw another square of the same side anywhere in
  Berlin, change the side keeping the middle - 250 m to 2 km - or move the
  middle by its latitude and longitude (#1599). Those are ETRS89's, about
  a metre from the WGS84 a web map shows, converted to and from the grid by
  `geodata::latlon` (Krueger's series, within a millimetre). The square's
  summary shows its grid corners on hover, the numbers `render
  --geo-square` takes.
  - A middle typed off the map lands where the square fits nearest
    (`Coverage::nearest`). The fields show what the owner asked while the
    square is the one that asking produced, so a slow drag of one leaves
    the other alone and a latitude dragged off the map and back brings the
    square back; they show six decimals, a tenth of a metre, so the number
    shown, written back, moves nothing.
  - Typed values apply when typing ends, never per keystroke, because a
    first digit is far off the map.
  - Every edit goes through the same rules as the sanitiser, so the
    sanitiser never rewrites it.

The terrain (P1.2, #1584, `src/terrain/geo.rs`) reads the source. A record
with a Berlin square starts a geodata job instead of the procedural
heightmap job.

1. The job fetches the ground's answers: the terrain legend and one render
   of the square - the terrain config's `grid_size` points, spaced to span
   the square's side, with pixel centres on the grid points - the land-use
   legend and one render of the same box (P1.4), a page each of the street
   and carriageway axes over it (#1595), and the street level's thirteen
   pages (P2.2). The spacing is the square's (`core_grid`, #1637): 512
   points span a kilometre at 1.95 m and 2 km at 3.9 m, and never closer
   than a metre, Berlin's terrain model's own grid, so a square of 512 m or
   less takes a point a metre and that many points. A 250 m square is a
   250 m world. The record's `cell_scale` is not read, as its height
   fields are not.
2. It decodes them on the compute pool.
3. It lands the result as a finished `TerrainTask`, so the procedural
   pipeline's own landing builds the world digest, the log, the mesh, the
   collider and the swap.

- **Heights** are metres above sea level, with no datum shift. Berlin's
  ground never lies below 26 m, so where Berlin has no water a seeded water
  plane stays under it.
- **Rebuilds.** The terrain fingerprint covers `geo_source`'s square, so
  switching to Berlin or moving the square rebuilds the ground in place, as
  a terrain edit does. The seeding mark (#1647) does not move the ground,
  and rebuilds nothing.
- **Failure.** If Berlin's terrain cannot be had - the service is
  unreachable after the fetcher's retries, or the answer does not decode -
  the region falls back to the procedural ground its terrain config
  describes, and says why. The loading screen's terrain row turns amber
  with the reason, and a toast says it in game. Nobody is left on a loading
  screen, or on stale ground, waiting for a service that is down. The
  loading screen counts the two answers while they arrive.
- **Ordering.** Regeneration and the cleanup paths run before the poll,
  with the sync point that ordering inserts. A decode that finishes in the
  frame the owner moves the square is dropped, never landed.
- **Known divergence.** Each peer fetches for itself and keeps a 30-day
  cache. If Berlin re-renders its data within that window, two peers can
  hold different heightmaps; the world digest reports it. A saved Kiez's
  buildings, trees and furniture are the record's (#1647), so peers differ
  only in the ground under them; before a save each client seeds from its
  own street level, and the owner's live broadcast carries the owner's.
  The layer hashes #1590 kept in the record for this went with the derived
  stage.
- **Tools.** The native tools' `rebuild_heightmap_for_record` fetches and
  decodes the same way, blocking. The render tool takes
  `--world N --geo-square E,N,SIZE` to render a region from real Berlin.
  Its `--terrain-report` read the Teufelsberg square's highest ground as
  119.0 m (the summit is about 120 m).
- **The world's edge.** Nothing is drawn or walked past the square, as
  nothing is past a procedural world's ground: the fog is the region's
  own, and the recovery's "left the world" rule reads the ground's edge.

The ground and the water (P1.4, #1586) come from the land-use render,
decoded on the core's own grid (`src/terrain/geo/ground.rs`). They ride in
the `FinishedHeightMap` beside the heights as a `GeoGround`, so the two land
together and go together.

- **Splat.** Each land-use class is drawn with one of the record's four
  material layers, by the role every seeded theme gives that layer, and the
  altitude rules are not read:

  | Layer | Classes |
  | --- | --- |
  | 0, green | forest, park, meadow, cemetery, allotments, weekend cottages, sport, tree nursery, the two vegetated fallows |
  | 1, earth | farmland, bare fallow, construction sites, water beds, and the built-up blocks until buildings stand on them |
  | 2, stone | street space, city squares, rail and airfields |

  The theme dresses Berlin: a meadow world's grass in the parks, a volcanic
  world's lava. The contact classifier reads the same weights, so dust and
  footstep sounds match the ground drawn.
- **Scatters.** A biome filter names natural ground. Built-up blocks,
  streets, squares, rail, sport grounds, construction sites and water count
  as no layer at all, so a seeded stand of trees grows in the parks, woods
  and fields and never across a street. A scatter's altitude band is not
  read on Berlin ground: a seeded treeline is a fraction of a procedural
  world's relief, tens of metres below Berlin's lowest ground, and would
  strip every stand. Where the street level has landed (P2.2), a scatter
  of trees keeps 8 m off each of Berlin's own.
- **Water.** The terrain layer draws water flat at its surface: the Spree at
  the Museumsinsel decodes to 30.54 m, its published level about 30.5 m.
  `geodata::water::settle` settles each body of water - its pixels,
  4-connected, eight at least - at its own level, the median height of its
  pixels (#1724), and shapes the ground to it:
  - its bed is carved below its level, falling a metre per two from the
    shore, to 3 m;
  - its shore follows the water's blurred outline rather than the map's
    2 m pixels, whose staircase showed from the ground as a row of teeth
    along the far bank, the land in it first kept 0.2 m over the level so
    the water's edge floods no low spot. Water narrower than about two
    pixels is smoothed away with the steps;
  - the water runs on under its bridges (#1595). The land use maps a bridge
    as the street it carries, so a run of street space along a row or a
    column, at most 40 m, with water at both its ends and one of Berlin's
    streets drawn over it, counts as water. Settled as land, such a strip
    stood as a dam across the river. A footbridge or a rail bridge, which
    no street deck covers, and every bridge of a region whose streets could
    not be had, stays the dam it was, and can be walked.

  Each body is drawn as its own surface over its own cells, never as one
  plane over the square, so the rest of the ground keeps Berlin's heights:
  an underpass or a building pit beside the river and below its level stays
  the hollow it is, a lake on the plateau lies at its own level over the
  valley, and so do the two reaches of a lock. Each body is shaped against
  the ground as drawn, so none depends on another: its own pixels are its
  alone, and a dam between two takes the higher of their banks. Until
  #1724 the core had one level, the largest body's that sank little of the
  core, and a body more than 3 m off it was drawn dry. Measured on squares
  fetched as the game fetches them, the one level against the bodies'
  (2026-10-23): round the Muehlendamm lock (600 m, 1.2 m pixels) the one
  plane took 32.5 m, carved the lower reach under it and lifted 7,639
  pixels to it, where the Spree now lies at 30.5 m below the lock and
  32.5 m above it; the Weisser See lies at 41.5 m either way; round the
  Orankesee (1 km) the one plane at its 50.5 m carved the Obersee 2 m under
  the Obersee's own 52.5 m; and 2 km round the Obersee it lifted 8,517
  pixels, 13 ha, where 82 are raised now, in shores. Fixtures recorded for
  #1724 (`crates/geodata/tests/fixtures`) pin the lock's two reaches, the
  Weisser See and the two lakes.
- **The water as drawn.** The record still says whether there is water - a
  record with no water child draws none, and the beds lie dry - and Berlin
  says where: its bodies are the region's water in place of a sea (#1724),
  and every reader of the water (the damp ground, the dry-land walks, the
  scatter bands, the streets, the editor and the tools) asks the level at
  its own point (`water::bodies::RoomWater`): within a body's reach - its
  water, and the bank reached from it standing under 4 m over its level -
  that body's, and elsewhere none. A further water child is a lake on
  Berlin's ground, as on any (#1760). The streets' decks bridge each body
  at its own level (#1595).
- **Failure.** If only the land use cannot be had, Berlin's terrain lands
  without it, coloured by the record's altitude bands and with no water,
  and the fallback says so as it does for the terrain.
- **Editor.** The Region source section shows the levels Berlin's water
  lies at: one, or the range of its bodies'.

The streets (#1595; `src/terrain/geo/streets.rs`) are Berlin's own on the
walkable ground, meshed by the road networks' mesher.

- **Data.** ATKIS's street axes (`atkis:b08_ax_strassenachse_l`) and
  carriageway axes (`atkis:b07_ax_fahrbahnachse_l`), dl-de/zero, one WFS
  page each over the core (a central core holds a few hundred, at about
  300 bytes each). An axis carries its carriageway width (`brf`), lanes
  (`fsz`), separation (`ftr`), function (`fkt`, 1808 a pedestrian zone),
  dedication (`wdm`) and a stable `uuid`. ATKIS cuts its axes at every
  junction and joins them at exactly shared end points. A street whose
  carriageways run apart is drawn as its carriageway axes; its own axis,
  which runs between them, is not. Both layers mark that separation, so
  the two pages are read apart. A pedestrian zone is no carriageway and is
  not drawn: the land use paints it as stone. The street sections
  `rbs_strab` are CC BY and are not used.
- **Graph.** The end points are welded into nodes, the lines cut 3 m inside
  the core's edge (a cut end is capped), and the lines meeting two to a node
  joined into the runs between junctions. A run's deck is as wide as its
  carriageway (the length-weighted mean of its lines'); with no width, as
  its lanes at 3.25 m; with neither, as its kind of street.
- **Mesh.** `urban::mesh_chains` - the tensor networks' own mesher from the
  chains on: junction truncation and hubs, the network-wide levelling, the
  ribbons with their curbs and skirt. `mesh_road_graph` is now the tensor
  front end over it. The mesh is built in the decode task and rides in the
  `GeoGround` until the terrain spawns, which takes it out: the surfaces are
  children of the terrain's root with their trimesh colliders, in the
  theme's road palette, and keep no CPU copy (a road network's neither).
- **Bridges.** The streets are read before the ground is settled, so the
  water runs on under the bridges they cross. Streets drape over a copy of
  the ground of their own: the higher of the terrain as drawn and as
  settled, so a quay keeps the height the settle eased down to the water,
  and over the water a span from bank to bank, along the shorter of the
  row and the column through each cell, at least 1.5 m over the level. A
  bridge is a deck from quay to quay, its skirt the bridge's side; on the
  Museumsinsel the lowest deck over a bridge stands 1.7 m over the Spree.
- **Failure.** Streets that cannot be had leave the ground without them,
  painted on its stone layer, and say so.

The street level (P2.2, #1588; `src/terrain/geo/street_level.rs`,
`src/terrain/derived/`) is the walkable ground's buildings, trees and street
furniture, each Berlin's own place and the region's own catalogue item.

- **Data.** Thirteen WFS pages over the square, one per layer: GeoServer
  refuses a query over several types with a box. All dl-de/zero. A square
  wider than 1,024 m asks them in tiles, as few as keep each within that
  (2 km: four of each), so no page is cut short and leaves an arbitrary
  hole - a 2 km square round Hermannplatz in one box held 3,000 of its
  6,418 buildings - and a feature two tiles both hold is read once, by its
  id; a layer one tile could not have is left out whole (#1637).
  - The ALKIS buildings (`alkis_gebaeude:gebaeude`): `uuid`, function
    (`gfk`), storeys above ground (`aog`), and whether a feature is a
    building or a part of one (`bezeich`).
  - The urban structure (`ua_stadtstruktur:b_stadtstruktur_differenziert_2024`,
    #1600): the Environmental Atlas's blocks for 2021-2024, the ground
    between the streets, each with its key (`schluessel`) and its
    urban-structure type (`typ`) - about fifty types, from the closed
    Wilhelminian block to the estate of slabs, the street of detached houses
    and the commercial area. The Museumsinsel's 600 m square has 32
    blocks, Hermannplatz's 2 km square 169: a square kilometre holds
    about 30 to 90.
  - The street and park trees (`baumbestand:strassenbaeume`,
    `baumbestand:anlagenbaeume`): `gisid`, genus (`gattung`), height,
    crown and girth.
  - The street lamps, from the lighting register (`beleuchtung:beleuchtung`,
    keyed by `leuchtstelle`). The street survey's own masts are not its
    lamps: a square kilometre of Prenzlauer Berg has 6 of them and 604 lamps
    in the register. A lamp is a lamp post or a gas lamp, which Berlin
    stands on posts - in ten central districts every lamp was one of these;
    its switch cabinets, light strips and catch-all are left out, and so
    would be a lamp hung on a wire or a wall.
  - The 2014 street survey (`strassenbefahrung`): benches, bins, bollards,
    shelters, signs, fountains, advertising columns, bike racks, keyed by
    `gis_id` (a sign by `sdatenid`). A bench is a line along its seat, a
    rack or a fountain an area: each keeps its long side.

  The densest layers over four central square kilometres (Prenzlauer Berg,
  Tiergarten, Wrangelkiez, Mitte): 1,281 buildings and building parts,
  1,276 street trees, 2,695 park trees, 1,765 bollards. A page holds 3,000
  (the trees' 6,000); a page cut short draws what it holds, and the log
  says so.
- **Reading.** A building stands whole; its parts - a dome, a high-rise
  section - only raise its peak storeys. It takes how its block is built
  up: the family of the urban-structure type of the block that holds its
  middle (`Development`, see "Street buildings"), or none where the block
  page could not be had. An underground car park stands nothing. Whatever reaches within 3 m of the core's edge is left out, and
  each layer comes nearest the core's middle first. Two items of a kind
  within half a metre of each other are one, the lower id kept whatever
  order the server sent them in: the sign survey records some signs twice,
  and two signs at one spot draw as one.
- **Facing.** A building or an item faces its street: across to the
  nearest carriageway the core draws, within 60 m, and elsewhere down the
  slope of the land use's distance to street space (which, a four-neighbour
  step count, only knows eight directions). A sign faces the traffic coming
  at it, which keeps to the right; a bench or a rack keeps its own line,
  fronting the street side.
- **The plans.** Buildings, trees and furniture are each a plan of the
  catalogue items they grow and the copies of them, nearest the landing
  first, which the seeder writes into the record (#1647, below). The
  record's own content comes first: nothing planned stands within 12 m of
  the landing, nor within 2 m of an absolute placement's ground reach (a
  placement of a seeded generator, which a seeding replaces, excepted).
- **Buildings.** A church, a cultural or public building of 1,200 m2 or
  more, or a building of 12 storeys takes one of the theme's landmarks - at
  most 6, no two within 150 m, places of worship first, each counted only
  where it can stand. Every other building of 30 m2 or more takes rows of
  the theme's street buildings (#1598, below). A footprint is filled in its
  box along its street front - the longest of its edges within 30 degrees of
  square to its street, rather than its longest edge, which on a house with
  a side wing is the wing: a row for every 30 m of its depth (at most 4), a
  copy wherever a slot's middle lies on the footprint. A row fronts the
  street side; of several, the outer two front their own sides, as a block's
  houses front the streets either side of it. Each copy is shaped to the
  footprint: its kind by Berlin's storeys, use, size and block (one of up to
  three storeys that is one house's size - a home, its garage, its shed, the
  corner shop, anything but a church or a museum - in a block of houses or
  garden plots is a detached house standing alone; one of one or two storeys
  over 500 m2 that is a works building, or in a block of works anything but
  a home, a hall; otherwise one or two storeys, or a shop, workshop, garage
  or utility of three, is a low building; three to seven a house; more a
  long block), Berlin's storeys snapped to its kind's, its ground floor
  trading where the use is a shop's, a workshop's, a garage's, a utility's
  or mixed (a hall's only where it is a shop's, a detached house's never);
  its depth the deepest of its kind's that fits the row, its front on the
  row's own edge as a Berlin house stands on the street line; its frontages
  rolled down the row, a tail its kind leaves taking the next kind down. A
  hall's rows, and an estate's, run along the box along the footprint's
  longest edge instead, as an estate's slabs turn their long sides to their
  lawns. A row too shallow for a kind's shallowest step, or too short for
  its narrowest, draws it smaller, in quarter-octaves down to half. A theme
  without its street buildings fills a box along its longest edge with its
  secondaries, each fitted to its row's depth, bigger where Berlin's stands
  taller. A landmark stands at the middle of the box along the longest edge,
  with rows either side of it; one that fits nowhere, whose box's middle is
  off the footprint (a courtyard), or that would reach the landing gives way
  to rows. A building's picks are seeded by its uuid. A walker meets every
  copy as its voxel shell.
- **Trees.** The owner chose Berlin's species over the region's biome: each
  genus is the catalogue species nearest it (a linden, a maple or an ash a
  dense oval crown, a plane or a chestnut a spreading one, an oak an oak, a
  pine a pine), and a genus the table does not name is a broadleaf crown.
  Each species is grown once, its growth held to the seeded stands'
  per-tree budget, and each copy scaled to its tree's measured height (10 m
  where none is measured) over the height its species grows to, from a
  pinned table: a seeded record must come out the same on every client,
  and an L-system's turtle turns through sines and cosines that do not. A
  walker meets its trunk, as wide as its tree's base, up to 3 m of it. Its
  foliage keeps its wind sway: the bake reads a swaying part through the
  wind material the wind system swapped onto it.
- **Furniture.** A prop is one of a kind by the words of its slug (a street
  lamp, a gas lamp and a stone lantern are lamps), searched among the
  room's props and secondaries by its prosperity and escalation, each drawn
  no bigger than its kind's reach. A kind the theme has no prop for is left
  out; most themes have a few. A Modern City room at prosperity 0.71 and
  escalation 0.14 draws lamps, benches, bins (its dumpster), shelters (its
  transit stop) and fountains, a medieval one lanterns and benches. An item
  takes one of its kind's matches by its own id. Every item is seeded
  (decision 9 on #1636); a street lamp is turned so its arm reaches over
  the carriageway. A walker meets a pole as its post, the rest as their
  boxes.
- **Seen** (#1647). Hermannplatz's kilometre, seed 518's Modern City room,
  seeds 4,086 copies in 22 sets: 1,054 building copies, 2,423 trees and
  609 items of furniture, exactly what the derived stage drew; Frohnau,
  Siemensstadt and Herzbergstrasse likewise (3,458, 1,946 and 4,642),
  each in 11 to 16 ms natively. Its 2 km square seeds 17,386 copies in 22
  sets, in about 25 ms.
- **Failure.** Each layer stands on its own page: one that cannot be had or
  read is said, and the square is not seeded until a visit has them all -
  the streets too, as they face its buildings and street furniture. A page
  the server cut short is drawn as it came, the same on every visit, and
  logged. The walkable ground waits for these pages as it does for its
  terrain.
- **Limits.** An L-shaped or courtyard building is filled from its box, so
  its slots fall only where their middles are on it. An item stands where
  the survey placed it, even where a drawn carriageway, wider than the real
  one, reaches over the pavement.

The source draw (P3.1, #1589; `src/seeded_defaults/room/source.rs`) is how
a seeded room comes to be a Berlin region, and how an owner keeps one.

- **The draw.** A stream of its own, salted apart from every other, so
  nothing a seed decided before it moves. Its first draw picks the kind:
  one seed in four draws Berlin (`BERLIN_SHARE`, the owner's choice,
  2026-10-09). Its third draws the square - a kilometre (#1637) - whatever
  the kind, so a seed's square stays where it is if the share ever
  changes; the second picked the square's side until #1637, and is still
  drawn so the third keeps its place. The draws of six seeds are pinned
  (`seeded_sources_are_pinned`); changing them moves every seeded room
  drawn from them, which needs a migration. Seed 253, the README's world,
  stays procedural; seed 3 draws Berlin.
- **Kept.** A seeded room's square is drawn afresh from its seed on every
  visit. Saving the room stores it in `geo_source`, which holds it exactly
  from then on, whatever the draw does later.
- **The room.** On a Berlin square the city is the settlement (owner,
  2026-10-09): the seeded landmark, secondaries and props are left out, and
  Berlin's buildings, in the room's theme, stand where they would have
  been. The gateway and the owner's monument stand near the square's
  middle, on the bearing the landmark would have stood on, the landing in
  front of the gate facing it. The seeded stands of trees and rocks and the
  ground cover stay, and keep to Berlin's parks and woods.
- **Open ground.** The record cannot know where Berlin's water and streets
  run, so where the gate, the monument or the landing would stand on them,
  each is walked to open dry ground: along its bearing through the origin
  first, as the compile walks every seeded structure off water, so the
  landing mostly keeps to the gate's forecourt; and where that finds none -
  a square centred far out on a lake - to the nearest, the landing turned
  to face its gate wherever that came to stand. The compile, the
  editor's outline, the terrain report and the seeder all read where a
  structure stands through one resolver (`AnchorGround`), and the
  spawn, a return to spawn, a fall and a portal arrival through one landing
  walk (`landing_ashore`). Berlin's buildings keep clear of where the gate
  stands, not of where its record put it. On procedural ground nothing of
  this changes: the water walk is the one it was, and a landing stays where
  its owner or its seed put it.
- **The theme.** The seeder dresses Berlin in the theme of the seed the
  room was built from - the seed its base terrain carries, which on
  Berlin's ground shapes nothing else - not of the owner's DID. A room
  re-rolled to a theme is that theme's Berlin; `--world <seed>` renders the
  seed's own.
- **The seed row.** The World editor's re-roll section gains the source
  axes as pins, hunted as the scene's are: the ground (procedural or
  Berlin) and the borough the square's middle lies in. A borough asks for
  Berlin, so with the ground pinned procedural it is refused, and says
  why. Every borough is reached within a few hundredths of a second. A
  square lock keeps the room's own ground - its exact square, drawn, moved
  or resized in the Region source section, or its own terrain - across
  re-rolls, which then draw everything else.
- **The login screen.** Its backdrop stays procedural (owner, 2026-10-09):
  the login screen fetches nothing.
- **Tools.** `--describe` prints each room's ground; the terrain report's
  landing is where a body sets down, with its record's spot beside it
  where the two differ.

The seeder (P3.2 of #1639, #1647; `src/terrain/derived/seed.rs`,
`seed_berlin`) writes a square's buildings, trees and street furniture into
the record. It retired the derived stage that drew them on every load, the
owner's edits over it (Remove, Make it this world's own, Restore; #1590)
and the layers' content hashes with their refetch of a stale cache. No
record was migrated: no saved Berlin region existed (decision 8 on #1636).

- **Sets.** One instance set per generator the plans grow: a street kind's
  one generator (#1646), its fits the set's variants, each with the seed
  the planner rolled for it; a landmark's or a prop's catalogue generator,
  grown as the lot layer grows one; a tree species, re-skin and all. Each
  is named `kiez_<slug>` (a re-skin's `kiez_<slug>_<re-skin>`), anchored at
  the origin, snapped, its solid as its plan says (shell, box, trunk). A
  copy is its pose: centimetres along X and Z, its turn clockwise in tenths
  of a degree, its lift above the ground under it, and its size. The sets
  are written buildings, trees, furniture, and no further than the record
  may hold - its copies, placements and items, its own counted first - so
  a cap cuts furniture first, the owner's record holds just what every
  peer keeps of it, and no item of the owner's makes way; a set past 8,192
  copies goes on in another of its generator, and a long set is saved in
  parts (#1645).
- **The mark.** `geo_source.seeded` is a fingerprint of the dataset, the
  square and the scene's seed. A record whose mark is its own is never
  seeded again, whatever its owner did to its sets since; any other - the
  square moved, a re-roll, or any edit of the seed the record's terrain
  carries, which picks the scene - has its seeded content, every `kiez_`
  generator with its traits and the placements of it, replaced (one undo
  step brings the old back). A re-roll under the square lock keeps the
  source unseeded, as it replaces everything else.
- **When.** Once a heightmap of the record's own terrain lands with a whole
  street level, the live record is seeded, and the stored one wherever it
  is a Berlin record without its mark: a fresh room, or one saved before its
  street level landed, reads saved, both seeded alike; a seeding the
  owner's square move or re-roll caused reads unsaved until a save. The
  room's default is seeded the same way, once a heightmap of its own
  terrain lands, so Reset to default is not offered to an untouched Kiez.
  The seeding folds into the undo step that caused it. A street level that
  left a layer out seeds nothing - the hole would be saved - and the
  fallback toast says so.
- **Visitors** seed the live record too: the seeding is a pure function of
  the record and the street level, the same on every client - everything
  it writes goes through `libm` (#1132), never a platform's maths. The
  stored record and the default are the owner's editor's baselines, and
  only the signed-in owner's client seeds or builds them.
- **Editing.** A seeded copy is edited as any set's copy (#1644): picked in
  the world, moved, duplicated, removed or detached. A detached one becomes
  an item of the owner's own whatever its size, its name the seeded item's
  without the prefix (`city_street_house_x1000`), so a new seeding leaves
  it. The World Editor will not rename a `kiez_` generator, nor any item
  into the prefix (a material slot's name may take it). Unchecking real
  Berlin takes the seeded content out with the square. The Region source
  section says whether the square's items are seeded yet.
- **Drawn.** The instance builder draws the sets (#1643). Its budgets -
  22,000 near entities and 5,000 far copies a room, the derived stage's
  summed - are shared by need (#1648): every copy of the room, nearest the
  landing first, is drawn near while the near budget lasts, then far while
  the far budget does, whatever set it is in. Each of the four measured
  kilometres draws every copy near - Hermannplatz in 17,609 near entities,
  the derived stage's own count - and the 2 km Hermannplatz everything
  within 623 m of the landing (1,355 of its 3,748 buildings, 2,949 of its
  11,119 trees), then its 5,000 nearest copies big enough to be seen far
  as their shells (1,065 of them buildings). A split by copies, as #1647
  first drew it, gave a Kiez's trees shares they never used: Hermannplatz
  drew 454 of its 1,054 buildings as shells.

The ignored test `live_gdi_berlin_round_trip_decodes_and_is_kept` exercises
the live path end to end: the real client, the disk store, and the decoders.
It then checks that a second visit is answered from the store alone. Fire it
by hand when the live path is in question.

## Street buildings (#1598, #1600)

The owner's decisions (2026-10-09): every theme has three shape-grammar
buildings of Berlin's street types in its own dress - a Roman insula, a
timber-framed burgher house, a neon tenement, a chitin tower house - and a
Berlin footprint's secondary buildings are drawn from them alone, each copy
shaped to its footprint; landmarks are as they were.
Elsewhere they are ordinary catalogue entries: in the inventory, and on a
road network's lots, at their kind's own fit. A seeded settlement and the
street furniture leave them out: a settlement stands its members apart on
open ground, and they are built to stand flush in a row.

The owner's decisions on #1600 (2026-10-10): the planner reads each block's
urban-structure type, and every theme has two more kinds - a detached house
and a hall - so a street of villas and a commercial area's sheds are drawn
as Berlin builds them; data under dl-de/zero only, and not the 1945
war-damage map.

| Kind | Berlin type | Frontage (m) | Depth (m) | Storeys | Own fit |
| --- | --- | --- | --- | --- | --- |
| House | the Altbau, the perimeter-block house | 12, 16, 20 | 8, 11, 14 | 3-7 | 16 x 14 x 5, trading |
| Block | the Gruenderzeit block, the Plattenbau slab | 24, 36, 48 | 8, 11, 14 | 6, 8, 10, 12 | 36 x 14 x 8 |
| Low | a cottage row, a shop, a workshop | 8, 12, 16, 24 | 6, 9, 13, 17 | 1, 2 | 12 x 13 x 2 |
| Detached | a house or a villa in its garden (#1600) | 8, 10, 12, 16 | 8, 10, 12, 16 | 1, 2, 3 | 10 x 10 x 2 |
| Hall | a works hall, a warehouse, a retail box (#1600) | 16, 24, 36, 48, 72, 96 | 12, 16, 24, 36, 48 | 1, 2 | 36 x 24 x 1 |

How a block is built up places its buildings (#1600). The atlas's types
sort into families (`geodata::berlin::Development`); their shares are of
Berlin's 794 km2 of blocks, from the whole atlas fetched on 2026-10-10:

| Family | Types | Share of Berlin | Placement |
| --- | --- | --- | --- |
| Perimeter | closed and semi-open block edges, the core (1-3, 6-8, 10, 29, 38) | 6.7 % | rows along the street front, flush |
| Estate | parallel rows, free rows, large estates, 1990s flats (9, 11, 72, 73) | 8.6 % | rows along the footprint's longest edge |
| Houses | detached homes, villas, villages, densified (21, 23-25) | 15.8 % | a building of one house's size - a home, a garage, a shed - stands alone as a detached house |
| RowHouses | row houses and duplexes (22) | 2.3 % | rows along the street front: a low building's terrace |
| Works | commercial, industrial, utility, sparse mixed (30-33) | 8.2 % | halls; any but a home of one or two storeys over 500 m2 is one |
| Civic | schools, hospitals, offices of state, culture, churches (12, 13, 17, 41, 43-47, 49, 51, 60) | 6.5 % | rows along the street front |
| Gardens | allotments, weekend plots, camping (37, 58, 59) | 5.0 % | as Houses |
| Open | forest, water, parks, farmland, railway, traffic, the rest | 47.0 % | rows along the street front |

A detached house stands alone at the middle of its street box, as wide
and deep as its steps fit, its front to its street: windows on all four
sides, its walls standing in from its lot's sides so its roof reaches out
over them. A hall stands flush like the rest, its gable ends its party
walls, and a row of halls takes the widest that fits, each; it trades
where its building is a shop's - a store's front under its sign rather
than a works' roller doors. Either stands only where it lands: three
quarters of each copy's lot on its footprint, the copies together covering
half of it. Halls are tried from the deepest the row holds down, so a long
wing narrower than its box takes shallower halls along its front, and a
footprint no hall lands on - a comb of wings, a T turned to its street -
takes the rows its storeys and use give it, as before #1600. Where the
block page could not be had, a building is placed as before #1600, but a
works building of hall size is a hall still.

A fit is snapped to its kind's steps, a storey count of two as near to the
lower, so copies share meshes: copies of one building at one fit, seed and
scale are one template, grown and baked once. Each fit is drawn with two
seeds, neighbours rolled apart - their cladding, their roofs and balconies,
their lit windows - and a plan grows at most 96 templates: past half of
them a copy takes its fit's first seed, and past
all of them the nearest grown of its building that does not outgrow its
slot - a shallower one standing back to keep its front on the street line
- or, where none fits, it is left out. A plan takes its buildings nearest
the landing first, so the budget goes to what a visitor sees first. A kilometre round Hermannplatz drew 946 copies from 98
distinct buildings, landmarks included, all of them near.

- **The conventions** every street grammar keeps, and the check that holds
  it to them, are in `src/catalogue/items/street/` (`mod.rs`'s docs and
  `check.rs`): the lot is `frontage x depth` with its street side local
  -Z, nothing past its sides, the front and back out 2 m at most, walls as
  high as the storeys and a crown 10 m over them at most, a door on the
  front up a step, a window in every storey, no door or window under the
  0.35 m sink, nothing z-fighting or floating, and the grammar inside a
  record's 16 KiB. A detached house also has a window on its back and on
  each side, where a street building's sides are its party walls.
- **A grammar is a `.cga` file** beside its spec: one statement a line, a
  line starting with white space continuing the one above, `//` lines
  dropped. The fit's declarations (`Storeys`, `Trade`, `Frontage`,
  `Depth`, `GroundH`, `FloorH`) go ahead of it and the shared openings
  (`Glazing`, `ShopGlazing`, `Wall`) after it.
- **One generator per kind** (#1646, P3.1 of #1639): a street building's
  `StreetSpec::generator()` is built at its kind's own fit and drawn at any
  other by the fit's grammar values (`fit_values`: `Frontage`, `Depth`,
  `Storeys`, `Trade`), which override the declarations - an instance set's
  variants, so a seeded Kiez needs one generator a kind however many fits
  its copies stand at. Its `Lot` rule sizes and centres the lot by the
  values at the head of its body, or of each `when` case: in the rule, not a
  rule ahead of it, because symbios-shape forks a shape's random stream for
  every rule it hands on to, and a rule ahead would re-roll every copy's
  cladding and picks. Its footing is a grammar of its own reading the same
  values. A seeded Kiez's street buildings are drawn from it (#1647), and a
  fought-over room drops no rubble in a street building's ground storey. The
  catalogue's own entry (the inventory, a road network's lots, the render
  tool) is still built at its fit, and tests hold the two bodies part for
  part the same at the sample fits and four seeds, and the footing to the
  box the primitive fills; an ignored sweep holds every fit (about 9,500,
  15 s under `test-release`).
- **Drafting without a rebuild.** `render --catalogue <slug> --street-fit
  F,D,S[,trade]` draws one at a fit, `--street-rules <file>` draws it with
  a rules file read at run time, `--street-seed N` at another seed, and
  `--street-check` holds it (or the draft) to every convention at its
  kind's smallest, largest and two middle fits and four seeds, prints each
  fault with the first part at fault, and its parts against its kind's
  budget (house 1,200, block 2,600, low 600, detached house 800, hall
  1,200). The ignored test
  `every_street_building_keeps_the_conventions_at_every_fit` holds every
  street building to the conventions at every fit its kind builds (#1600;
  about 9,500 derivations, 9 s under `test-release`): fire it by hand after
  a street grammar changes. Its first run found three of #1598's buildings
  failing at fits the four samples miss.

## Removed by the Kiez overhaul (#1637)

Until #1637 a square ran from 250 m to 19 km, and only about a kilometre
round its middle was walked at street level. Four parts served the rest;
the owner's Kiez decision (2026-10-13, #1636) retired them, and a region is
now its square, walked at full detail to its edge:

- **The far field and horizon** (P1.3, #1585): the square beyond the core,
  rendered at 17-74 m a pixel, its fog and sky opened to the square's side.
- **The middle ring** (P2.1, #1587): the theme's buildings on 30 m lots
  read from the surface model (`c_dom`), a kilometre past the core, near
  and far forms.
- **The walkable horizon** (P4.1, #1596): the far field's own colliders,
  invisible walls at the square's edge, and every reader of the ground
  taught to read past the core.
- **The detail patch** (P4.2, #1597): a core-sized street-level patch that
  followed a body past the core, cut into the far field by a shader hole
  (`SPLAT_HOLE`, `splat_prepass.wgsl`).

Their layer hashes (`horizon`, `ring`) went with every other layer hash in
#1647. Their history and the lessons from them are in git and on the issues;
the surface model stays in `crates/geodata`'s catalogue, unused.

## Phases

Each phase is a sub-issue of #1580, in build order:

| Issue | Phase |
| --- | --- |
| #1581 | P0.1 `crates/geodata`: square, coverage, catalogue, requests, decoders |
| #1582 | P0.2 fetch + cache service (native + wasm), host allow-list, size caps |
| #1583 | P1.1 record schema `GeoSource` + sanitiser, wire fixtures, editor slice |
| #1584 | P1.2 real-scale terrain core from `c_dgm1`, collider, world digest |
| #1585 | P1.3 far-field terrain ring to the square's edge, no collider, fog (retired by #1637) |
| #1586 | P1.4 ground splat from land use, water from the water layers |
| #1587 | P2.1 block-level buildings in the middle ring (retired by #1637) |
| #1595 | Berlin's streets on the core, meshed by the road networks' mesher |
| #1588 | P2.2 derived-content stage with stable ids (moved from P2.1); street-level core: footprints, trees, street furniture |
| #1589 | P3.1 the DID draws the source; seed-row locks; exact-square lock |
| #1590 | P3.2 owner edits over derived content; layer hashes (retired by #1647) |
| #1591 | P4 walkable area past the core: P4.1 walkable horizon (#1596), P4.2 detail patch (#1597) (retired by #1637) |
| #1598 | Street buildings: three per theme, shaped to Berlin's footprints |
| #1600 | Block types: a detached house and a hall per theme, placed by how each block is built up |

The Berlin share of the source draw was held at 0 until P2 made a themed
Berlin worth landing in; since P3.1 it is one seeded room in four.

The Kiez overhaul, epic #1636 (research #1635), in build order:

| Issue | Phase |
| --- | --- |
| #1637 | P1 Kiez squares: seeded 1 km, the editor to 2 km, the whole square walked at full detail; the far field, middle ring and detail patch removed |
| #1638 | P2 instance sets: one generator at many poses, each copy's seed and parameters, drawn through the bake: #1643 the set in the record and drawn merged (the bake moved to `world_builder::instances`), #1644 one copy picked, moved, deleted or detached in the World Editor, #1645 long copy lists in records of their own |
| #1639 | P3 Berlin's buildings, trees and furniture seeded into the record as instance sets, adopted on save: #1646 one generator per street kind, its fit passed in as grammar values, #1647 the seeder, the derived stage retired |
| #1640 | P4 a Kiez agent to flesh out a seeded region (later) |

Storing a region's ground with it waits for baked grounds (#1641).
