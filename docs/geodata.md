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
  - a coarse **far field** out to the square's edge, as a real horizon with
    no colliders.
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
  `parse_trees`, `parse_furniture`, P2.2);
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
  draws one), draw another, change the side keeping the centre, or move one
  edge.
  - Moving one edge holds the other where it is (`nearest_keeping`). Only a
    square that fits nowhere on its row moves both ways.
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
- **Editor.** The Region source section says what the land use does and
  shows the water level Berlin set.

The horizon (P1.3, #1585, `src/terrain/geo/far.rs`) is the square beyond
the core, drawn coarse to its edge. It rides in the `GeoGround` beside the
core's land use.

- **Far field.** One more render of the terrain and of the land use over the
  whole square, 64 to 256 pixels a side: 40 m a pixel up to a 10 km square,
  coarser beyond (74 m at 19 km). It has the city's hills, its land use on
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
- **Not walked.** The far field has no collider; P4 (#1591) walks it. It
  looks like ground, so invisible walls stand just outside the core's
  edges, from 50 m under its lowest ground to 500 m over its highest. They
  carry no `TerrainMesh`, so the pick rays that ask for the ground pass
  them by, and they are on a collision layer of their own that particles'
  bounces leave out. The camera, which may orbit out over the far field,
  keeps clear of its hills (`FinishedHeightMap::view_height_at`).
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
  sky cuboid stands past the far field's farthest edge from anywhere in
  the core. The camera's far plane is 25 km; Bevy's projection is infinite
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
  other lot takes a secondary building, a bigger one where Berlin's stands
  taller. A building is drawn no bigger than its lot holds, whichever way
  it turns, down to half its catalogue size. Sounds, particles, signs,
  portals and gateways are stripped: a kilometre off they only cost.
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

- **Data.** Twelve WFS pages over the core, one per layer: GeoServer refuses
  a query over several types with a box. All dl-de/zero.
  - The ALKIS buildings (`alkis_gebaeude:gebaeude`): `uuid`, function
    (`gfk`), storeys above ground (`aog`), and whether a feature is a
    building or a part of one (`bezeich`).
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
  section - only raise its peak storeys. An underground car park stands
  nothing. Whatever reaches within 3 m of the core's edge is left out, and
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
  where it can stand. Every other
  building of 30 m2 or more takes the theme's secondaries, bigger where
  Berlin's stands taller. A footprint is filled in its box along its
  longest edge: a row of entries for every 30 m of its depth (at most 4),
  each fitted to its row's depth and set side by side down the length, a
  copy wherever a slot's middle lies on the footprint. A row fronts the
  street side; of several, the outer two front their own sides, as a
  block's houses front the streets either side of it. A landmark stands at
  the box's middle with rows either side of it; one that fits nowhere,
  whose box's middle is off the footprint (a courtyard), or that would
  reach the landing gives way to rows. A building's picks are seeded by
  its uuid. Every copy stands on its voxel shell as its collider; past
  12,000 near entities a copy is its shell, at most 3,000.
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
  terrain, land use, streets, buildings, trees, furniture, horizon and
  ring, each only where all of its answers were had. A save writes the
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

The ignored test `live_gdi_berlin_round_trip_decodes_and_is_kept` exercises
the live path end to end: the real client, the disk store, and the decoders.
It then checks that a second visit is answered from the store alone. Fire it
by hand when the live path is in question.

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
| #1591 | P4 walkable area past the core (revive terrain streaming) |

The Berlin share of the source draw was held at 0 until P2 made a themed
Berlin worth landing in; since P3.1 it is one seeded room in four.
