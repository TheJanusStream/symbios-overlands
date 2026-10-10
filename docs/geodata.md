# Geodata regions

A geodata region is a square of real Berlin at real scale, dressed in the
region's theme: Berlin's ground, water, blocks, buildings and trees, built
from the catalogue the region would use anyway. The data is GDI Berlin's
(<https://gdi.berlin.de>), fetched when the region is visited. Epic #1580;
the API research and the owner's decisions are on #1579.

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
- **Every size**, 250 m to 19 km (log-uniform, so each doubling of size is
  equally likely). The owner asked for 20 km; Berlin's shape allows 19 (see
  [Coverage](#coverage-and-the-square)).
- **Real scale, 1 m = 1 m both ways.** What matters is a realistic
  perspective standing on the ground, not the relief seen from afar.
  Compressing a large square onto today's ~1 km world would either shrink
  the hills to mounds or make the slopes twenty times steeper. So the
  square is the region's real extent, and the detail falls off with
  distance instead:
  - a walkable street-level **core** about today's world (~1 km, centred on
    spawn, at most the square);
  - a **middle ring** at block level;
  - a coarse **far field** out to the square's edge, as a real horizon -
    with no colliders, at first: the owner chose to walk it on 2026-10-09
    (P4.1, below).
- **Fetched at visit**, with the app's own cache, not baked into packs.
  Requests are sized to the pixels needed: the GDI viewer's zoom-fitting
  loading, used as distance LOD.
- **Saves keep seed + edits**, adopted per item (copy-on-write). Untouched
  content is derived again on every load. An item the owner moved or
  restyled becomes ordinary record content with a back-reference to its
  stable source id, and its original is suppressed. Content hashes of the
  fetched layers are stored at save, so a stale cache can be detected.

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
  middle ring reads the surface model less the ground instead (#1587).
- **No-store.** WMS answers say `Cache-Control: no-cache, no-store`, so the
  browser keeps nothing: the app caches by request URL itself (#1582).
  That is why the builders emit one canonical string per request.
- **Raw elevation exists but is heavy:** 2 km GeoTIFF tiles of 16 MB,
  uncompressed float32, one row per strip. They support HTTP `Range`, so
  any row span can be read exactly (~1 MB per km^2 at 4 m). Kept in reserve
  should the core ever need more than the decoded classes give.

## Coverage and the square

A square may lie only wholly inside Berlin, so every layer has data under
all of it. `crates/geodata/tools/coverage.py` rasterises the ALKIS state and
borough boundaries into a 250 m grid: 13,787 cells (862 km^2) count as
inside, each with its borough. That grid is checked in as a 5 KB run-length
table. "Wholly inside" holds to the tool's 25 m sampling: a cell may reach
about one sample past the border, a sliver the decoders fill from its
neighbours.

`Coverage::place` picks uniformly among *every* whole-metre position where a
square of the drawn size fits. It counts, per corner cell, the offsets that
give each footprint, so no part of Berlin is likelier than another and no
draw is rejected. A 19 km square has 751 positions; 19.01 km has none.
Squares from fixed draws are pinned in a test, and an independent Python
implementation (`crates/geodata/tools/place_check.py`) reproduces them. Changing the draw mapping moves every
seeded region and needs a migration, not a re-bless.

The square's two draws come from the region's own salted stream in the
app (`ChaCha8Rng::seed_from_u64(room_seed ^ SALT)`, like every other seeded
stream), after the draw of the room's source (P3.1, below). The crate maps
them to a square and owns no randomness.

## Code

`crates/geodata` (P0.1, #1581) is pure, with no I/O and no Bevy, so the app
and the wasm worker share it:

- `square`: `GeoSquare` and the log-uniform size draw;
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
`geo_source`: `{dataset, min_e, min_n, size_m}`, in whole metres. When it is
absent, the field is elided, so every older record and every seeded room
stays byte-identical. The bytes are pinned in
`tests/fixtures/geo_source_wire.jsonl`.

- **Sanitiser.** A Berlin square has its side snapped to a whole 10 m
  within 250 m - 19 km (`snap_size`) and is moved, if it must be, to the
  nearest position wholly inside Berlin (`Coverage::nearest`, exact). A
  dataset id that is not a plain name drops the source. Another dataset's
  square is kept for the build that can draw it, though keys this build does
  not know are dropped on save, as from every record field.
- **Editor.** The World Editor's Environment tab has a Region source
  section. It can turn Berlin on (a square drawn the way a seeded region
  draws one), draw another, change the side keeping the middle, or move the
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

1. The job fetches the core's answers: the terrain legend and one render of
   the core - the terrain config's `grid_size` points `cell_scale` apart,
   centred on the square, with pixel centres on the grid points - the
   land-use legend and one render of the same box (P1.4), a page each
   of the street and carriageway axes over it (#1595), and the street
   level's twelve pages (P2.2). The core is never
   wider than the square (`core_grid`), so a 250 m square is a 250 m
   world, and the core always lies inside Berlin, where the data is.
2. It decodes them on the compute pool.
3. It lands the result as a finished `TerrainTask`, so the procedural
   pipeline's own landing builds the world digest, the log, the mesh, the
   collider and the swap.

- **Heights** are metres above sea level, with no datum shift. Berlin's
  ground never lies below 26 m, so where Berlin has no water a seeded water
  plane stays under it.
- **Rebuilds.** The terrain fingerprint covers `geo_source`'s square, so
  switching to Berlin or moving the square rebuilds the ground in place, as
  a terrain edit does. The owner's edits and the layer hashes (P3.2) do not
  move the ground, and rebuild nothing.
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
  hold different heightmaps; the world digest reports it. P3.2's layer
  hashes in the record (#1590) are the cure: a peer whose stored answers
  hash otherwise than the owner's save fetches them again.
- **Tools.** The native tools' `rebuild_heightmap_for_record` fetches and
  decodes the same way, blocking. The render tool takes
  `--world N --geo-square E,N,SIZE` to render a region from real Berlin.
  Its `--terrain-report` read the Teufelsberg square's highest ground as
  119.0 m (the summit is about 120 m).

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
  `geodata::water::settle` takes one level for the core, the median height
  of a body's pixels, and shapes the ground to it:
  - every body within 3 m of the level is carved below it, its bed falling
    a metre per two from the shore, to 3 m;
  - all other ground is kept 0.2 m above it, so the one plane floods
    nothing that is not water: an underpass, a building pit, a lock's lower
    basin;
  - the shore follows the water's blurred outline rather than the map's
    2 m pixels, whose staircase showed from the ground as a row of teeth
    along the far bank. Within that two-pixel margin the shore's profile
    replaces the two rules above. Water narrower than about two pixels is
    smoothed away with the steps;
  - the water runs on under its bridges (#1595). The land use maps a bridge
    as the street it carries, so a run of street space along a row or a
    column, at most 40 m, with water at both its ends and one of Berlin's
    streets drawn over it, counts as water. Settled as land, such a strip
    stood as a dam across the river. A footbridge or a rail bridge, which
    no street deck covers, and every bridge of a region whose streets could
    not be had, stays the dam it was, and can be walked.

  Lifting the ground is only a repair for the few low spots a plane would
  flood, so the body that sets the level must be one the plane can stand
  at. The largest bodies are tried in turn, and the first is taken whose
  level sinks no more than 2 % of the core over a metre under it and that
  keeps some water once its shore is smoothed. A pond on a hill over a
  valley would lift the whole valley to its surface; a lake on the plateau
  that is larger than the river below it leaves the level to the river.
  Where no body qualifies, nothing is settled.
- **One plane.** The record still says whether there is water - a record
  with no water child draws none, and the beds lie dry - and Berlin says
  where: the terrain's own water planes are drawn at Berlin's level, and
  every reader of the water line (the damp ground, the dry-land walks, the
  scatter bands, the streets, the editor and the tools) asks
  `drawn_water_level` for it. A body more than 3 m off the level, a lake on
  the plateau, is drawn dry; a region with water at several levels needs
  bounded water planes, a later step. The streets' decks bridge the water
  (#1595).
- **Failure.** If only the land use cannot be had, Berlin's terrain lands
  without it, coloured by the record's altitude bands and with no water,
  and the fallback says so as it does for the terrain.
- **Editor.** The Region source section shows the water level Berlin set.

The horizon (P1.3, #1585, `src/terrain/geo/far.rs`) is the square beyond
the core, drawn coarse to its edge. It rides in the `GeoGround` beside the
core's land use.

- **Far field.** One more render of the terrain and of the land use over the
  whole square, 64 to 256 pixels a side: about 40 m a pixel from a 2.6 km
  square to a 10 km one, finer below (at least 64 pixels a side, so about
  17 m at the smallest square that has a far field) and coarser beyond
  (74 m at 19 km). It has the city's hills, its land use on
  the region's layers, and its water at the core's level:
  `geodata::water::settle_to` carves the far bodies within 3 m of that level
  and keeps the rest of the far ground above it, under the same two
  refusals as `settle`. A square the core nearly fills, leaving a ring under
  two far pixels wide, gets no far field.
- **Seam.** The far mesh's grid lines are the far pixel centres plus the
  core's four edges. A far cell along the core takes the core's boundary
  vertices on its edge, from the one nearest each of its corners, at the
  core's own heights, and fans them out to its far corners. So the two
  meshes share their boundary vertex for vertex, with no far vertex partway
  along a core edge (a T-junction the rasteriser need not close). There the
  far mesh also takes the core mesh's own normals, so the light runs on
  across the seam.
- **Layers.** Three of the four splat layers tile by the mesh's UVs. The far
  mesh's UVs are the core's mapping run on past its edges, so its tiles are
  the core's size and phase, and a weight-map transform in the splat
  uniforms (`weight_uv_scale`, `weight_uv_offset`) maps them onto the far
  weight map.
- **Walked** (P4.1, #1596, below). The far field stands on a collider of
  its own triangles, and invisible walls end the world just outside its
  edge, from 50 m under the lowest ground to 500 m over the highest. They
  carry no `TerrainMesh`, so the pick rays that ask for the ground pass
  them by, and they are on a collision layer of their own that particles'
  bounces leave out.
- **Water.** Where the far field took the core's water, the terrain's water
  plane spans the far field, so the river runs on to the horizon.
- **Haze and sky.** Round a far field that landed, the fog visibility is at
  least the square's side (`fog_visibility(record, heightmap)`, which the
  drawn fog, the draw distance and the shadow reach all read). At the
  square's edge a far thing keeps about a seventh of its contrast, so the
  city fades into the air there rather than ending; a hill halfway out
  keeps more than a third. Half the side was tried first and lost the
  edge, and a hill 5 km out with it. A region whose horizon could not be
  had keeps its own fog, which hides the end of its walkable ground. The
  sky cuboid stands past the far field's farthest edge from anywhere on
  it, the far field being walked (P4.1). The camera's far plane is 25 km; Bevy's projection is infinite
  reverse-Z, so that bounds culling only.
- **Seen.** A 12 km square round Charlottenburg shows the Teufelsberg
  4.5 km out as a small forested bump on the horizon, as an 80 m hill is at
  that distance, and the Spree's water continuing past the core.
- **Timing.** The core lands once its own answers are in (its terrain,
  land use, streets and street level, eighteen in all) and the far
  field's are too, or 10 s after its own, whichever is first: the horizon
  is decoration, and gets one retry's grace, not the loading screen. The
  grace runs on the wall clock (`Time<Real>`), since it waits on the
  network; on the render tool's stepped session clock it ran out before a
  real answer could come. Without its far renders a region keeps its
  walkable ground and says its horizon is missing.
- **Cost.** Two more renders (the legends are the core's), a decode on the
  compute pool, and one more mesh with no CPU copy and one more material.
  The far mesh of the largest square builds in 0.24 s native, a quarter of
  the core mesh's 0.93 s. The tools read the walkable ground only:
  `rebuild_terrain_for_record` fetches no far field.
- **Limits.** At 40 m a pixel streets turn into blurred bands of stone, and
  water narrower than about 80 m is smoothed away. Seen from above, the far
  field's land use is plainly coarser than the core's.

The middle ring (P2.1, #1587; `src/terrain/geo/ring.rs`,
`src/terrain/derived/ring.rs`) is Berlin's buildings round the walkable
ground, drawn as the region's own catalogue buildings. It rides in the
`GeoGround` beside the far field, and is spawned by the derived stage
(P2.2), after the walkable ground's plans.

- **Data.** Over a box reaching 1 km past the core on every side (at most
  the square), two renders at 4 m a pixel: the land use, and the surface
  model `c_dom` (ATKIS DOM, ground with everything on it, 2 m classes from
  -12 m to 306 m; the Berliner Dom stands 90 m over its square). Less the
  far field's ground, the surface is how high each thing stands. A pixel
  of built-up land standing 3 m or more is building.
- **Lots.** The ring is cut into 30 m lots; a lot is built where 35 % of it
  is building. Its building stands at the middle of the lot's building
  pixels (at most 7.5 m off the lot's middle) and faces the nearest street
  (down the slope of the distance to street space). How high nine tenths of
  it stand picks the building it takes. The lots nearest the walls come first, at most 6,000; a kilometre
  of central Berlin round a seeded core is about 4,000. On the recorded
  Museumsinsel square, 128 of the 145 lots have an ALKIS building within the
  reach of their own building; most of the rest are courtyards of tall
  trees, which a surface model cannot tell from roofs.
- **Buildings.** The theme's own, as the road layer grows its lots: the same
  pools by the room's prosperity and escalation, finish and ruin. The
  tallest lots - 40 m or more, no two within 200 m, at most 8 - take the
  theme's landmarks: Berlin's church towers, domes and high-rises. Every
  other lot takes one of the theme's street buildings (#1598, see "Street
  buildings" below), shaped to the lot: its kind and storeys by Berlin's
  height there (near its ridge, so 2.5 m of crown off it and 3.4 m a storey:
  a pitched Altbau reads six storeys, a flat slab eight), its ground floor
  trading on one lot in three, at the biggest of its kind's fits that keeps
  to the lot. A theme
  without them takes a secondary building, a bigger one where Berlin's
  stands taller, drawn no bigger than its lot holds, whichever way it
  turns, down to half its catalogue size. Sounds, particles, signs, portals
  and gateways are stripped: a kilometre off they only cost.
- **Near and far.** A kilometre of Berlin is ~4,000 buildings of a handful
  to hundreds of parts each, and in a browser every part costs CPU each
  frame. Each distinct building (an entry at a drawn scale) is spawned once,
  hidden, and baked: its parts merged into one mesh per material, carried
  by the affine product of their transforms as the renderer carries them,
  and a far form, the building filled into 1 m voxels and its outer faces
  merged into rectangles, each in its parts' base colour - a few hundred
  triangles (at most 2,048, then one colour), no holes where a facade is
  tiled from small parts. Within 200 m of the walls, and while the near
  copies stay inside 9,000 entities, a copy is the merged meshes, a few
  entities. Past that it is the far form, one entity casting no shadow, at
  most 4,000. Copies share their building's meshes. A 4 km square round
  the Museumsinsel reached both caps: 8,990 near entities and 4,000 far
  copies of its 5,838 lots, its outermost 1,200 or so left undrawn.
- **Spawning.** Under one root hung on the terrain, so it goes with it, as a
  remote avatar's visuals are spawned (no collider, editor marker or room
  entity): 4 ms a frame, and at most two templates, one bake or 256 copies
  of it. A template's full-detail meshes stay in the shared generator
  caches, beside the room's own buildings of the same entries, until the
  room compile's sweep. None of it is in the record or saved. The ring
  waits on its own answers, apart from the far field's: a ring that is
  late or cannot be had says so, and the horizon stays.

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

- **Data.** Thirteen WFS pages over the core, one per layer: GeoServer
  refuses a query over several types with a box. All dl-de/zero.
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
- **The derived stage.** None of it is in the record. Each item is named by
  where it comes from, a `SourceId` (`alkis:<uuid>`, `tree:<gisid>`,
  `furniture:<id>`, and `ring:<x>,<z>` for the ring, which has no ids), and
  every entity drawn from it carries it as a `DerivedItem`: what P3.2's
  edits name (#1590). The record's own content comes first: nothing
  derived stands within 12 m of the landing, nor within 2 m of an absolute
  placement's ground reach. Each part is a plan, spawned as the ring's is:
  templates, bakes, then copies, 4 ms a frame, the walkable ground first,
  and each plan's copies nearest the landing first, so its budget keeps
  those.
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
  to rows. A building's picks are seeded by its uuid. Every copy stands on
  its voxel shell as its collider; past 12,000 near entities a copy is its
  shell, at most 3,000.
- **Trees.** The owner chose Berlin's species over the region's biome: each
  genus is the catalogue species nearest it (a linden, a maple or an ash a
  dense oval crown, a plane or a chestnut a spreading one, an oak an oak, a
  pine a pine), and a genus the table does not name is a broadleaf crown.
  Each species is grown once, its growth held to the seeded stands'
  per-tree budget, and each copy scaled to its tree's measured height (10 m
  where none is measured). It stands on a trunk as thick as its girth, up
  to 3 m of it. Its foliage keeps its wind sway: the bake reads a swaying
  part through the wind material the wind system swapped onto it.
- **Furniture.** A prop is one of a kind by the words of its slug (a street
  lamp, a gas lamp and a stone lantern are lamps), searched among the
  room's props and secondaries by its prosperity and escalation, each drawn
  no bigger than its kind's reach. A kind the theme has no prop for is left
  out; most themes have a few. A Modern City room at prosperity 0.71 and
  escalation 0.14 draws lamps, benches, bins (its dumpster), shelters (its
  transit stop) and fountains, a medieval one lanterns and benches. An item
  takes one of its kind's matches by its own id. The 1,500 nearest the
  landing are drawn, cut at the player's draw distance as ground cover is; a street
  lamp is turned so its arm reaches over the carriageway. A pole stands on
  a post of its own, the rest on their boxes.
- **Seen.** The Museumsinsel's square kilometre in that Modern City room:
  451 building copies (6,193 entities), 1,081 trees (2,206) and 855 items
  of furniture (3,042), all near.
- **Failure.** Each layer stands on its own page: one that cannot be had or
  read is left out alone, and said, and the rest stand. The walkable ground
  waits for these pages as it does for its terrain.
- **Limits.** An L-shaped or courtyard building is filled from its box, so
  its slots fall only where their middles are on it. An item stands where
  the survey placed it, even where a drawn carriageway, wider than the real
  one, reaches over the pavement.

The source draw (P3.1, #1589; `src/seeded_defaults/room/source.rs`) is how
a seeded room comes to be a Berlin region, and how an owner keeps one.

- **The draw.** A stream of its own, salted apart from every other, so
  nothing a seed decided before it moves. Its first draw picks the kind:
  one seed in four draws Berlin (`BERLIN_SHARE`, the owner's choice,
  2026-10-09). Its next two draw the square, whatever the kind, so a seed's
  square stays where it is if the share ever changes. The draws of six
  seeds are pinned (`seeded_sources_are_pinned`); changing them moves
  every seeded room drawn from them, which needs a migration. Seed 253, the
  README's world, stays procedural; seed 3 draws a 17.3 km square.
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
  editor's outline, the terrain report and the derived stage all read
  where a structure stands through one resolver (`AnchorGround`), and the
  spawn, a return to spawn, a fall and a portal arrival through one landing
  walk (`landing_ashore`). Berlin's buildings keep clear of where the gate
  stands, not of where its record put it. On procedural ground nothing of
  this changes: the water walk is the one it was, and a landing stays where
  its owner or its seed put it.
- **The theme.** The derived stage dresses Berlin in the theme of the seed
  the room was built from - the seed its base terrain carries, which on
  Berlin's ground shapes nothing else - not of the owner's DID. A room
  re-rolled to a theme is that theme's Berlin; `--world <seed>` renders the
  seed's own.
- **The seed row.** The World editor's re-roll section gains the source
  axes as pins, hunted as the scene's are: the ground (procedural or
  Berlin), the square's size (Small to 1 km, Medium to 5 km, Large to 19 km,
  about a third of draws each) and the borough its middle lies in. A size
  or a borough asks for Berlin, so with the ground pinned procedural they
  are refused, and say why. Every size class reaches every borough; the
  rarest pair, a large square in Marzahn-Hellersdorf, is one large square
  in about 100 - one Berlin square in about 340 - so even that hunt takes a
  few hundredths of a second. A
  square lock keeps the room's own ground - its exact square, drawn, moved
  or resized in the Region source section, or its own terrain - across
  re-rolls, which then draw everything else.
- **The login screen.** Its backdrop stays procedural (owner, 2026-10-09):
  the login screen fetches nothing.
- **Tools.** `--describe` prints each room's ground; the terrain report's
  landing is where a body sets down, with its record's spot beside it
  where the two differ.

The owner's edits (P3.2, #1590; `src/terrain/derived/edit.rs`) keep what
the owner changed of what is drawn, and the layer hashes keep what it was
drawn from.

- **Picking.** With the World Editor open, a click on one of the walkable
  ground's buildings, trees or items of street furniture picks it (owner,
  2026-10-09), whichever tab is up: the editor names it as Berlin records
  it - "A residential building, of 5 storeys, 720 m2", "A linden (Tilia),
  12 m tall", "A street lamp" - with its id, and offers Remove and Make it
  this world's own. A copy's merged meshes keep nothing on the CPU for the
  mesh ray to read, so the click is tested against its collider - a
  building's shell, a prop's box, a tree's trunk - and its copy's box,
  which takes a tree by its crown. The owner's own content wins a click
  unless a derived collider truly stands in front of it, never to a box (a
  bench under a crown stays the bench's); over anything else the nearer
  derived hit wins where it is nearer than what the editor's own rays hit.
  Only an item on screen is picked - not one past its draw distance - and
  a box the camera stands in is not one the click enters. The ring's lots
  carry no stable ids, and stay as drawn (owner, 2026-10-09).
- **Remove.** The item's id goes into `geo_source.removed`, and it is drawn
  no more.
- **Make it this world's own.** A copy as drawn (owner, 2026-10-09): the
  same catalogue items at the same place, turn and size - those drawn, near
  or far, not those the plan's budgets left out - as ordinary record
  content - snapped absolute placements, their height an offset
  from the ground, so a building's sunk foundations stay sunk - of
  generators named `<id>#<n>`, one to each catalogue building and size its
  copies are, which the copies share. A footprint's row comes over whole.
  The size rides in the generator's root, as a placement drops its own.
  The id goes into `geo_source.adopted`, and the original is drawn no
  more. Past the record's counts (256 items, 1,024 placements) or its
  100 KiB size budget the copy is refused, and the panel says why; before
  its plan has grown and baked the item, too. A copy keeps no clear disc
  round it, as a placement does, so its neighbours stand as they stood -
  while the walkable ground draws its item; once the square has moved off
  it, the copy is a placement like any other.
- **Restore.** The Region source section lists every item removed or made
  the world's own, by what Berlin records of it, each with Restore: the
  item is drawn from Berlin again, and an adopted item's copy - every
  placement of its generators, and the generators - goes. Each edit is one
  undo step.
- **Live.** The edits apply as the record changes, with no rebuild: the
  terrain fingerprint covers only the square. A suppressed item's entities
  are despawned and what they cost their plan's budgets given back; a
  restored one is drawn again from its plan, which stays resident for its
  terrain's life for that, or when the spawn reaches it. A visitor's world
  follows the owner's as any live edit does.
- **Ids.** A list holds at most 1,024 ids, each 1 to 96 of ASCII letters,
  digits and `.`, `_`, `:`, `-`, sorted and deduplicated by the sanitiser;
  an id in both lists is kept as adopted, which has content. An id the
  square's data no longer holds is kept and ignored, and listed as not on
  the walkable ground. Moving the square keeps the edits - an id names its
  item wherever the square lies - and a re-roll, which replaces everything
  the world holds, drops them, the square lock or not.
- **Layer hashes.** Each answer is hashed as it settles, on the I/O pool:
  64-bit FNV-1a over its bytes, a page of features' own `"timeStamp"` left
  out, the one thing that changes between two fetches of the same data. A
  layer's hash folds its answers' in the order the job asks for them:
  terrain, land use, streets, buildings, blocks, trees, furniture, horizon
  and ring, each only where all of its answers were had. A record saved
  before a layer existed holds no hash for it, so says nothing changed. A save writes the
  drawn layers' hashes into `geo_source.layers` (16 hex digits each) on
  the way out, where the record names the square they were drawn for - and
  only there: the editor's record never holds them, or every save would
  leave behind a difference no edit made.
- **Stale caches.** A load whose record holds hashes weighs its answers
  against them: a layer whose stored answers hash otherwise, and were kept
  more than a day ago, is fetched once more past the store, and the ground
  drawn from what the network says - or, should the network fail, from
  what was kept. If the fresh answer still differs, Berlin has changed
  since the save: it is drawn as Berlin has it now, the Region source
  section says which layers changed, and the owner's next save records
  them. An answer kept within the day is Berlin as it stands, so a
  difference there is not fetched again on every visit, and a layer is
  never fetched twice over in one load.

The walkable horizon (P4.1, #1596; owner, 2026-10-09: "horizon first, then
detail" and "build anywhere walkable") lets a body walk and drive from the
core out to the square's edge, on the far field and through the ring. A
full-detail patch follows the body out there (P4.2, #1597, below).

- **Ground.** The far field stands on colliders of its own mesh's triangles
  (`FarField::colliders`), the core's hole and the seam to the core's
  boundary vertices included: the plain cells - between four pixel
  centres, off the core - as a heightfield over the pixel grid, whose split
  is the mesh's, and the cells the core's edges cross or bend, with the
  fans along the core, as a small triangle mesh. A test casts rays down at
  random points against both and reads the drawn height to a millimetre.
  Both take parry's internal-edge fix, as the core's heightfield does
  (#1538), so a wheel does not meet the edge between two triangles as a
  wall; where the two colliders meet, and at the core's edge, it does not
  apply. If parry refused the triangles the far field would be drawn and
  not walked, its walls at the core's edge as before. It joins the terrain's static body and carries
  `FarGround`, which the rays that ask for the ground take as ground beside
  the core's `TerrainMesh` (the editor's pick and context menu, the
  inventory drop). The walls stand at the far field's edge.
- **The ground as drawn.** Every reader of the ground's height past the
  core reads the far mesh's own triangle there
  (`FarField::drawn_height_at`): a far cell is two triangles, not the
  bilinear patch its pixels suggest, and a cell along the core fans the
  core's boundary vertices out. A test samples it against a brute-force
  search of the built mesh. So a body set down out there - a spawn, a
  return to spawn, a portal's arrival, a "Go to" - lands on the collider,
  not under it, and the placement snap, the editor's gizmo and snap
  toggle, the camera, the agents' senses, the footsteps, dust and marks
  of the contact classifier, and a scatter's slope and land use all agree
  with it. A sample costs about a third of a microsecond, allocating
  nothing.
- **Building.** An owner may build anywhere walkable: a placement past the
  core sits on the far ground - on a detail patch where one has loaded
  (P4.2, below). The walks that move a landing or a seeded
  structure off water, streets and steep ground read the core's land use
  alone, so past the core they do not run - a landing or an item out there
  stands where its owner put it.
- **The ring.** Its buildings stand on their voxel shells, near or far, and
  keep clear of what the record keeps (`Kept`): its landing and its
  absolute placements, as the walkable ground's plans do. A lot given up
  for one keeps the picks of the lots after it.
- **The world's edge.** The recovery reads the walkable world's edge - the
  far field's, where a region has one - for its "left the world" rule, and
  the ground as drawn for its "fell through" rule; a boat's lift fades at
  the same edge. A "Go to" sets a body down inside the walls. The sky
  stands past the square's far side from anywhere on it.
- **Cost.** At the largest square (256 far pixels round a 512-point core)
  the far colliders build in about 3 ms, beside the far mesh's 0.23 s, and
  keep about a megabyte: one triangle mesh of it all kept 34 MB, its
  internal-edge fix the most of it, on a web heap that never shrinks.
- **Limits.** Where no detail patch stands (P4.2, below), the far ground
  is the far field's: about 17 to 74 m a pixel, its land use painted, no
  streets meshed, and no buildings past the ring (a kilometre beyond the
  core); the ring draws near forms only within 200 m of the core's edge.
  The core's streets and street level stop 3 m inside its edge. The
  region's water plane spans the far field only where the far field took
  the core's water; elsewhere its beds lie dry. A road network grows on
  the core. The core and the far field are two colliders, so their seam
  is not smoothed as an internal edge is. The far mesh casts no shadows. A
  peer whose far field missed its grace has the core as its world, as
  before.

The detail patch (P4.2, #1597; owner, 2026-10-09: the patch "as big as
the core", and round it "the coarse ground only") brings Berlin at full
detail to wherever a body walks or drives past the core
(`src/terrain/geo/patch/`).

- **Where.** A patch is a square of the core's own lattice: its points are
  the core's carried on past its edges, as many as the core's, or fewer
  where the ground between the core and the far field's edge is narrower
  (none under 250 m). It never overlaps the core: one that would, or that
  falls short of it by less than a step, stands against it, sharing the
  core's boundary vertices. Its place snaps to a lattice of 250 m, so a
  body coming back to a place asks for the patch it had there, which the
  store answers.
- **When.** A body within 200 m of the core's edge, or past it, asks for
  a patch; one 300 m deep in the core lets it go. A patch serves until its
  body has moved three quarters of a step from where it asked for it, so
  a new patch comes every 250 m or so of travel, and a body pacing about
  the halfway line does not fetch two by turns. One patch is on its way at
  a time, and lands before the next is asked for.
- **What.** A patch is fetched as a core is - the terrain and the land use
  rendered over its box at the core's pixel, its streets, buildings, trees
  and street furniture - and decoded as a core is. Its water settles to
  the region's one level where the region's water plane spans the far
  field; there its dry ground is kept above the level whether it has water
  of its own or not, so the plane floods nothing it does not call water.
  Where its terrain or land use cannot be had, it does not land, and is
  not asked for again until the body asks for another; a street level
  layer or the streets lost leave the rest standing.
- **The seams.** Against the core the patch takes the core's own boundary
  heights and normals, and its streets run on 3 m past the shared edge to
  where the core's end, so a street crossing onto it runs on unbroken.
  Against the far field the patch is cut out of the far mesh in the shader
  (`SPLAT_HOLE`), and a prepass shader of its own (`splat_prepass.wgsl`)
  cuts the same hole from the depth prepass. The far material is drawn as
  a mask from its spawn, its hole empty while no patch stands, so a patch
  landing compiles nothing: a pipeline still compiling is not drawn, and
  the horizon would blink out round the first patch. What that costs is
  the far mesh's early depth test, its shader run for fragments hidden
  behind nearer ground too; the core's ground and a patch's stay opaque.
  The far cells the patch's edge crosses keep their colliders,
  and within them the patch draws the far field's own triangles at its
  points; from there it eases into Berlin's heights over 40 m, and from
  the core's boundary over 6 m. The far colliders leave out every cell the
  patch fills whole, and the patch stands on a heightfield of its own.
- **Readers.** The patch rides in the far field (`FarField::patch`, one
  slot every clone of the ground shares), so every reader of the ground as
  drawn reads it where it lies, with no reader of its own: the placement
  snap - a footprint rests on the patch's highest point inside it - the
  camera, the recovery, the contact classifier, a scatter's land use and
  slope. The rays that ask for the ground take the patch as ground.
- **The record.** What of the record stands on a patch - a snapped item's
  footprint, a snapped grid, a scatter's bounds - is set down again when a
  patch lands under it or goes, and nothing else is rebuilt: the patch's
  stamp rides in those placements' compile fingerprints. A body on the
  ground a landing patch replaces is lifted by as much as the ground rises
  under it. A patch landing restarts a placement the compile is midway
  through building, wherever it stands; at a patch every 250 m or so,
  that delays a long one, it does not starve it.
- **Derived content.** The patch's buildings, trees and street furniture
  are drawn as the core's and spawned after the core's and the ring's
  plans, a slice a frame; the ring's lots under the patch, and within half
  a lot of it, are not drawn while it stands. The owner's edits apply to
  the patch's items as to the core's: an item removed or made the world's
  own is not drawn on any patch that holds it.
- **Cost.** Measured on a 942 m core (427 points a side; `did:plc:berlin53`
  over Mitte), a patch is fetched in what a core's fetch costs and built in
  about 0.15 s on the compute pool - its mesh's tangents computed from the
  grid, where mikktspace took a second - and lands in a millisecond or
  two. On the web the compute pool is the main thread, so the build waits
  a frame between its stages, the longest about 50 ms natively.
- **Limits.** Each client loads its own patch round its own body: two
  visitors past the core see detail round themselves, and the coarse
  ground round each other, and an item placed past the core stands on
  whichever ground each client has there. A building straddling the core's
  edge is drawn by neither (each reads only what lies wholly inside). A
  patch's streets that reach past the core's corner along the shared edge
  run 3 m out over the far field. A body that reaches the far field before
  its patch has landed drops onto the patch's ground where that lies
  lower, a metre or two at most. Streets drape over a dry cutting rather
  than bridging it, on a patch as on the core.

The ignored test `live_gdi_berlin_round_trip_decodes_and_is_kept` exercises
the live path end to end: the real client, the disk store, and the decoders.
It then checks that a second visit is answered from the store alone. Fire it
by hand when the live path is in question.

## Street buildings (#1598, #1600)

The owner's decisions (2026-10-09): every theme has three shape-grammar
buildings of Berlin's street types in its own dress - a Roman insula, a
timber-framed burgher house, a neon tenement, a chitin tower house - and a
Berlin footprint's and a ring lot's secondary buildings are drawn from them
alone, each copy shaped to its footprint; landmarks are as they were.
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
their lit windows - and a plan grows at most 96 templates on the walkable
ground (64 in the ring; a detail patch is a plan of its own, its templates
filed under the core's keys, so a patch over the same room finds most of
them cached): past half of them a copy takes its fit's first seed, and past
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

## Phases

Each phase is a sub-issue of #1580, in build order:

| Issue | Phase |
| --- | --- |
| #1581 | P0.1 `crates/geodata`: square, coverage, catalogue, requests, decoders |
| #1582 | P0.2 fetch + cache service (native + wasm), host allow-list, size caps |
| #1583 | P1.1 record schema `GeoSource` + sanitiser, wire fixtures, editor slice |
| #1584 | P1.2 real-scale terrain core from `c_dgm1`, collider, world digest |
| #1585 | P1.3 far-field terrain ring to the square's edge, no collider, fog |
| #1586 | P1.4 ground splat from land use, water from the water layers |
| #1587 | P2.1 block-level buildings in the middle ring |
| #1595 | Berlin's streets on the core, meshed by the road networks' mesher |
| #1588 | P2.2 derived-content stage with stable ids (moved from P2.1); street-level core: footprints, trees, street furniture |
| #1589 | P3.1 the DID draws the source; seed-row locks; exact-square lock |
| #1590 | P3.2 owner edits over derived content; layer hashes |
| #1591 | P4 walkable area past the core: P4.1 walkable horizon (#1596), P4.2 detail patch (#1597) |
| #1598 | Street buildings: three per theme, shaped to Berlin's footprints |
| #1600 | Block types: a detached house and a hall per theme, placed by how each block is built up |

The Berlin share of the source draw was held at 0 until P2 made a themed
Berlin worth landing in; since P3.1 it is one seeded room in four.
