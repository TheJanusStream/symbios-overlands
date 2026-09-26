# Your region: land, water, sky

A world's own settings live in its record beside the things placed in it:
`/generators/base_terrain` (the ground, with the water as its child),
`/environment` (sun, sky, fog, clouds, the water's look, the ambient sound)
and `/default_landing` (where and which way visitors arrive). All of it is
edited with `room set` and kept with `save`, like a building.

## Clearing a seeded world

A new account's world is seeded: trees, ground cover, a settlement, a
landmark, the gateway, the owner's monument. To start over, two edits:
`room set /placements` with only the terrain's placement, and `room set
/generators` with only `base_terrain` - the water rides along as its child.
Take a copy of `room get` first; `revert` undoes it all until a save.

## The land is a recipe, not a sculpt

There is no brush. The ground is generated from `base_terrain`'s fields:
`generator_kind` (`FbmNoise` for soft hills, `DiamondSquare` for rough
ridging, `VoronoiTerracing` for steps), `seed`, `height_scale` (metres),
`base_frequency` (bigger = smaller hills), `octaves`, `persistence`
(smaller = smoother), and hydraulic and thermal erosion (drops carve gullies
and fill valley floors). You shape land by choosing a recipe and scanning
seeds until one falls the way you want - the same seed always gives the
same ground.

The water is **one flat plane**: the water child's `transform.translation`
y. Everything below it floods, so a pool is the deepest hollow under a level
set just above its floor.

## Seeing it before anyone does

- **Numbers in half a second**: `render --world <your DID> --world-record
  record.json --terrain-report --at=X,Z ... [--footprint R] [--plan
  plan.png --focus=X,Z --span M]` (#1449) reads the ground the game stands
  things on - the same heightmap job and the same footprint rule - and
  prints height percentiles, the water line and the share it floods, the
  landing's ground, slope and facing, where every placement stands (a seeded
  one walked off water says `moved_from`), and at each `--at` point its
  ground, slope, downhill way, the `--yaw` that lays a long thing along the
  contour, and with `--footprint` what a thing that wide rests on and how
  far the ground falls beneath it. `--plan` draws it from above (+X right,
  +Z down): water blue, a grid, the landing white with its facing tick,
  placements amber by index (a gateway or portal magenta), scatter bounds
  as rings, your points red. A negative X needs the `=`.
- **Choosing a seed**: add `--seed-scan N` (seeds 0..N) or `--seed-scan
  A..B` to the report: each seed's heights, flooded share and landing
  ground as JSON, and with `--plan sheet.png` a contact sheet of small
  plans, one tile a seed, labelled. 16 seeds took 4.6 s; the Understory's
  seed 11 shows its pool beside the landing (session 873 found it with a
  scratch program on the same job).
- **Pictures from anywhere, offline**:
  `render --world <your DID> --world-record record.json --focus=X,Z --dist D
  --elev DEG --yaw DEG` compiles the record in the file (`room get`'s
  `value`) as the game does - about 2 s. A negative focus needs the `=`.
  Render a copy with `fog_visibility` pushed out and `cloud_cover` 0 to read
  the land; render the real settings to judge the mood.
- Bring each good step live with one `room set` per part, then `look`.

## Ground textures

`base_terrain.material` holds four `layers` (any texture: `Moss`,
`ForestFloor`, `Rock`, `Ground`, `Lichen`, `Gravel`, ...) and four `rules`,
one per layer: `height_min..height_max` as a fraction of `height_scale`,
`slope_min..slope_max` where slope is `1 - normal.y` (0 flat, about 0.29 at
45 degrees, 1 a cliff), and `sharpness` (2 is a clean edge). Where no rule
matches, the third layer is drawn. Every rule that matches is blended, by
weight: two rules both covering the high ground draw it half and half.

**A rule reaches past its band.** Its weight fades over a skirt a third of
its half-range wide OUTSIDE the band, so a wide band has a wide skirt. The
Understory's rock rule was written `slope 0.22..10` ("anything steeper than
0.22"); slope never passes 1, and that band's skirt reached down to level
ground, where rock kept 27-43% of every texel. Its rippled pattern was the
"sand" of two sessions, and a moss retune "changed little" because 41% of
the moss ground was rock. Capped at `slope_max` 1.0 (10000 on the wire),
rock left gentle ground entirely and the hollow read as moss. Cap every band
at 1.0 for slope and 1.0 for height.

**Read the blend, do not guess it**: `--terrain-report --at=X,Z` prints
each point's `layers` - every layer's share of the blend there, from the
game's own weight map (#1461) - and `biome`, the largest, which is the
index a scatter's `biome_filter` compares. The render's eye is a poor judge
of a ground texture: under a low warm sun three different textures measured
within 5% of each other, and a thumbnail hid a 41% drop in ripple. Measure
what changed (the share, or the spread of a Laplacian over a ground patch
in two renders of one view) before judging by eye.

A scatter's `biome_filter` lists the layers it may grow on, by that largest
share: ferns filtered to `[0, 1]` never grow where the high ground's layer 3
is largest, which is why the Understory's plateau had none.

**A procedural texture's colours are LINEAR**, unlike a material's
`base_color` (sRGB): the generator converts them when it bakes. A litter
meant as dark brown sRGB `(0.36, 0.25, 0.13)` is written
`((c + 0.055) / 1.055) ^ 2.4` each: `(0.107, 0.051, 0.015)`. Written as sRGB
it bakes out light tan, and a hillside reads as sand.

**Sunlit ground seen toward a low sun is mostly sheen, not texture** (#1467).
The terrain's roughness (0.85) and reflectance are fixed in code; looking
toward the Understory's 20-degree sun, about 85% of a sunlit patch's
brightness was specular sheen in the sun's own colour, so five litter
colours up to 45% darker measured the same (sunlit (122, 101, 70) each) and
the hills stayed tan. Test a colour change in SHADE, or paint a layer pure
green for one render: if the sunlit patch barely moves, the colour is not
your lever. What did help: the ripple. A ForestFloor `litter_scale` is capped
at 24 a tile (11.4 m), so its leaves are ~47 cm and its normal map draws
them as dunes; `normal_strength` 0.8 -> 0.5 (the sanitiser's floor) with
`leaf_thickness` 0.15 halved the ripple (a Laplacian spread 35 -> 17 live),
and grass tufts on the open tops broke up the rest.

## Water, sky and light

- The water's look is in the water child's `surface` (`deep_color`,
  `shallow_color` with alpha, `roughness`, `reflectance` - 0.3 by default,
  stylised glossy - waves, wakes) and in `/environment`
  (`water_normal_scale_near/far`, `water_sun_glitter`,
  `water_shore_foam_width`, `water_scatter_color`).
- **Shallows read as mud.** The shallow colour's alpha lets the lake bed
  show through where the water is under a metre deep, so a brown
  `shallow_color` draws every shallow margin as a mud flat - the Understory's
  pool "shore" of bare brown was lake bed under 0.3-0.9 m of water (read it
  with `ground.py`: `UNDER 0.5`). Plant what grows there instead: a second
  reed scatter banded to the waterline (`above_water_band` -0.9..+0.2 m,
  2,800 reeds at 4 triangles each, `clumping` 0.55) turned the flats into a
  reed margin. The pool's first reed scatter, banded -0.6..+3.5 m, had
  spread its 120 reeds up the whole bank instead.
- **The water mirrors no trees.** Its shader reflects the sky's light and
  the fog, never the scene (no screen-space reflections), so still water
  under a grey sky is a grey sheet whatever its settings. Do not tune it
  hoping for reflections; dress its margin.
- **The normal scales are tiling FREQUENCIES (per metre), not strengths**:
  the ripple's strength is fixed in the shader. The Understory's pool at
  `near` 0.35 drew 3 m ripples and read as a choppy grey sea; `near` 12,
  `far` 1.5 made them fine enough to fade with distance, and the pool read as
  a still mirror with a sun path (session 877). Still water: a HIGH near
  frequency, small `wave_scale`, low roughness.
- Mood is mostly fog: `fog_visibility` in metres, `fog_color`,
  `fog_sun_color` (the glow toward the sun) and `fog_sun_exponent`; a
  saturated fog colour turns the whole sky one flat tint. `sun_position` is
  where the light comes from (its angle above the horizon is its height).
- **Mood is the admin's.** Dimming the Understory toward dusk (sun 9,000 ->
  2,500 then 4,000 lux, darker fog and sky) made the glowing threads read a
  little stronger and the woods much darker; the admin judged both steps
  too dark and kept the original. Offer a light change live and UNSAVED,
  one step at a time, say how to undo it, and make no other edit meanwhile
  (a save would take the trial with it); `revert` restores the saved record
  exactly.

## Planting: scatters

A forest is a few generators and a scatter placement each:
`{"$type": "network.symbios.place.scatter", "generator_ref", "count",
"bounds": {"type": "circle", "center", "radius"}, "local_seed",
"biome_filter": {"biomes": [...], "water": "Above"|"Below"|"Both"},
"naturalness": {...}, "float_on_water"}`.

- **Borrow the species.** The catalogue's `lsys_*` trees and `gc_*` ground
  cover are ready: `render --dump --catalogue <slug>` prints one's JSON; put
  it in `/generators` under your own name (a root `transform.scale` makes
  old giants) and scatter it. `render --catalogue <slug>` shows it, framed
  to fit, so the pictures do not compare sizes.
- **A borrowed species may not survive a close look.** The catalogue's
  Monopodial Conifer, the Understory's commonest tree (900 copies), read up
  close as a weeping tangle of flat combed planks; the admin said so from
  the ground (session 878). Re-authored offline by a designer, an
  independent critic and a refiner (one sub-agent at a time), it became a
  conical tiered spruce at 3,788 triangles and the admin's verdict was
  "much better". What made the difference, for the next tree: grow whorls
  by AGE (each step adds a trunk section and a whorl; older branches
  lengthen and sink toward level, so the cone comes free); ROLL every
  needle card at least 45 degrees off level (level cards flare white toward
  a low sun - there is no reflectance setting to stop it); put trunk-fill
  cards at every whorl (else the trunk shows through the crown); vary
  branch lengths 0.76-1.15 and add a rare 3-branch whorl (all copies share
  one mesh, so the variety must be inside it). Engine limits met: a rule
  may produce at most 128 symbols (`TooLarge`, no line number); the
  Needle texture's `pair_count` caps at 24; `variant_rows`/`variant_cols`
  tile the WHOLE atlas across one card; thin needles fade with distance as
  mipmaps average their alpha, so make them wide and dense; `base_color`
  multiplies the texture, so a dark tint on dark texture colours goes near
  black. The pale birch (269 copies, a leafy stick) went the same way next,
  to a silver birch at 4,316 triangles: a `Lichen` texture squashed 3:1
  along the trunk makes birch lenticels (the `Bark` and `Marble` textures
  gave grey mottle), and vertex-colour rings give the black foot and dark
  bands that still read when the texture has blurred to grey at 15 m. Leaf
  size is bounded from below by distance: smaller leaves (tried 12 at 0.24
  m and more) left birches 55 m away as bare white skeletons, because the
  mipmaps average thin leaves into transparency. More engine facts: after
  `$`, `^` pitches down and `&` up; a 2-argument colour symbol does nothing
  (the 4-argument form can be aged by a growth rule); a Twig texture's
  stems curl unless `stem_curve` is 0. Both builders are kept, with every
  variant they rejected, in [examples/trees/](examples/trees/README.md):
  start the next tree from one of them.
- **Borrow the habitat settings** from the seeder,
  `src/seeded_defaults/room/groundcover.rs` and `scatters.rs`: reeds wade
  (`water: Both`, `above_water_band [-0.6, 3.5]`), lily pads float
  (`water: Below`, band `[-3, -0.25]`, `float_on_water: true`, no tilt),
  ferns and moss keep to damp low ground, trees clump at 0.35 and stop at
  30 degrees of slope.
- **`biomes` are your own splat layers**, by index. Filtering the trees to
  every layer but the one round the water leaves a clearing that follows the
  land.
- **`count` is spread over the whole circle.** 170 trees in a 440 m circle
  read as a thin line on a ridge; the same count within 230 m of where
  people stand reads as forest. Ground cover wants thousands where people
  look (3000 ferns in a 210 m circle). Put the density inside the fog's
  reach and thin it beyond.
- **Do the cover arithmetic**: count x one plant's footprint / the circle's
  area. 1,400 half-metre grass tufts in an 85 m circle covered 1.5% of the
  ground and did not show; the same count at 1.6x scale, spread evenly
  (`clumping` 0.3), read as tufted heath. `clumping` 0.75 gathers plants into
  groups and leaves bare stretches that a single view can miss entirely.
- **Scaling a small plant up changes what it reads as**: a heather clump
  (domed lumps with a purple Moss texture) grown to 2.4 m read as a pile of
  cobbles from above; its fine texture did not scale with it.
- **A scattered thing's cost is its triangles times its count**: the
  Understory's `fern_clump` is placed 10,700 times, so a replacement must be
  a few cards, not an L-system. Read it before bringing a change live:
  `render --world <your DID> --world-record record.json --triangle-report`
  (about 2 s) prints the world's total, then each generator's triangles for
  one copy times its copies, then each placement's cost, dearest first; a
  scatter's copies are the ones its sampler really places, which can be
  fewer than its `count`. One generator's count is on the `subject size`
  line of `render --generator FILE`. An icosphere of resolution n is
  20 x (n+1)^2 triangles (80 at 1, 720 at 5), not 20 x 4^n.
- **A scatter can place nothing and say nothing.** The report's `copies`
  against `requested` finds it: a broadleaf stand laid over the north lake
  placed 0 of 30 (all water, and its `above_water_band` refused the shore).
  Check a new scatter's placed count before bringing it live.
- **`count` is how many points a scatter draws, not how many it keeps.**
  Every filter - water, band, slope, biome - drops points, and a narrow band
  drops most: moss hugging the waterline (`above_water_band` 0.03..0.9 m)
  kept 49 of 260, and widening the band to 1.2 m kept 871 of 1,300. Raise
  the count until the report's `copies` is what you meant.
- **Most visitors play in a browser, and a browser pays per PART** (the
  admin, session 878: "optimized for both clients... most visitors will be
  using WASM"). Every primitive of every placed copy spawns as its own
  entity (a primitive with several materials, one per material), and the
  browser's single thread culls, extracts and batches each one every frame
  - so a scatter's cost is copies x primitives, not only triangles. A moss
  cushion of a root cube and six spheres was 7 parts x 3,083; as one
  BlobGroup it is 1. A grass or reed tuft of two crossed cards was 2 parts;
  as one lathe cylinder with a 1 x 3 atlas of tufts wrapped once round it
  (the fern's recipe: `variant_rows` 1, `variant_cols` 3, `uv_scale` 15915
  on a local radius of 0.1 m, `taper` negative to flare) it is 1 part, 36
  triangles, and reads fuller. The Understory went from about 48,700 parts
  (estimated from the record's structure at the session's start) to 38,579
  measured (`--triangle-report` counts `parts` since #1479) while gaining
  reed beds, heath and fans - and ground cover is still about 78% of them:
  heath 10,800, ferns 10,700, moss 3,083, reeds 3,064, rosettes 2,140.
  Prefer one primitive with a texture over several primitives, and read
  `parts` beside `triangles` before planting thousands of anything.
- **Small scattered things are no longer drawn far away** (#1480, the
  admin's decision): a scattered or gridded copy up to 2 m (by its own
  size, jitter included) is cut at the player's Settings > Draw distance >
  Ground cover (150 m by default, 50-400 m or Unlimited), up to 4 m at twice
  that, anything bigger never; every cut stops at the room's fog. Measured
  on the Understory, it cut the parts drawn each frame by 40-46% at the
  landing, the pool and the Coral Glade. So ground cover far from where
  people stand now costs little, and cover where they stand costs as much
  as ever: plant the thousands where people walk, not across the whole map.
  A render tool `--world` picture shows what a default-settings visitor
  sees - an aerial shot from 200 m up shows no ground cover near its focus.
- **Swaying plants had a ceiling that crashed clients** (#1472, fixed in
  session 878): every swaying card (grass, reeds, ferns, leaves) took its own
  strong handle on its shared wind material, the game counted those in 16
  bits, and at 65,536 the client aborted ('attempt to add with overflow' in
  bevy_asset) - the agent's daemon included. It counted CARDS, not copies:
  the heath tuft was two crossed planes, so a whole-land heath of 30,000
  crashed the offline render at half the expected count. Clients built
  since the fix take one handle per material per frame; a visitor on an
  older build still has the ceiling, so keep copies x cards of one swaying
  generator under 65,535 until everyone has the fix, and render a big
  scatter offline before bringing it live - the render tool panics just as
  a client would.
- **A tree's cost is mostly its branch cylinders.** An L-system's
  `mesh_resolution` is the number of sides every branch and twig gets; at
  the catalogue's 8, a broadleaf is 9,578 triangles, at 5 it is 6,176
  (conifer 5,100 -> 3,570, birch 5,166 -> 3,888, bush 3,958 -> 3,070). The
  Understory's four species at 5 took the world from 19.2M to 15.1M
  triangles (session 878), and renders from 3.5 m to the horizon showed no
  difference - the leaves and the bark texture carry the look. Try 5 before
  cutting a forest's count.
- **Scatters know no clearings.** A scatter skips water, steep ground, the
  wrong splat layer and road districts - never your buildings. To build in a
  forest, find where its trees stand: `tools/clearings.py` renders a copy of
  the record with each scattered generator swapped for a short fat glowing
  pole of its own colour, from straight above, with your build composed in;
  move or turn the build until no pole is inside it. The Windthrow's first
  site had three trees through its trunk.
- `max_particles` is capped at 512. Small particles (under ~15 cm) do not
  show in a still `look` at any range: they are for people moving through.

## Dressing: one-off pieces

- An absolute placement snaps to the ground (`snap_to_terrain` is true by
  default, and `translation` y is then a lift above it); set it false for a
  world height - an emitter floating over a pool.
- **A spread-out piece is snapped at its origin only.** Everything else in
  it stands at the height you wrote in its own frame, so where the real
  ground falls away, its outer parts float. The Puffball Meadow's small
  puffballs and earthstars, up to 24 m from its centre at one authored
  height, floated up to 2.1 m on its downhill side; the admin saw it from
  the ground (session 878). Author a piece wider than a few metres on the
  real ground: read `--terrain-report --at` under every part that should
  touch it (one call takes them all) and add `ground - ground at the
  origin` to its height - or sink it by the drop, or split the piece into
  placements that each snap.
- **A face turned from the sun is lit by ambient light alone**, so its
  colour hardly shows: a root plate's soil lightened five times in linear
  colour still read nearly black from the shaded side (the Windthrow, seen
  from the pool, faces away from the low WSW sun). Glow is what reads
  there - its violet veins did.
- A long thing on a slope lies along the contour or one end floats: with
  the gradient `(dx, dz)`, the clockwise yaw that lays local +X along the
  contour is `atan2(dz, dx) + 90` degrees.
- **Children inherit their parent's whole transform, scale included**: a
  flattened root flattens everything on it. Give a piece a tiny root with no
  transform, hidden inside a part, and scale only leaves.
- A world's gateway is any generator with a
  `network.symbios.gen.gateway` child: `size` is the walk-in zone, centred
  on its translation. Build the frame round it in the world's own style.
- **A gateway must be proven by walking into it.** Hypha's first gate looked
  right and did nothing (#1453): its generator's ROOT was a tiny non-solid
  node, and under a non-solid root every collider - the walk-in zone
  included - collides at its offset from the world's origin instead of where
  it is drawn, in every client built before #1453's fix. Make the root of
  any generator with a gateway, a portal or a solid part itself `solid`
  (a few centimetres, buried), then walk or fly in and read `status.zone`:
  `{"kind": "gateway", "picker": "open"}` is the proof; `null` inside it
  means the zone is not where it is drawn.
- Let people walk in: stand the zone from about 0.4 m BELOW the ground to
  above head height, 0.8-1 m deep, on the path arrivals take (seeded worlds
  put it a few metres behind the landing, facing it). A hand-placed gate
  wants `avoid_water: false`: with it true (as copied from a seeded
  gateway) the game stands the gate on the highest ground within its
  footprint, and on a 14-degree slope the zone floated 0.8 m up.

## Ambient sound

`/environment/ambient_audio` is a sequence: `recipe` holds `instruments`
(each an `id` and a `patch` whose `graph` holds `nodes` and `output`) and
`tracks` of `events` (`instrument_id`, `time_beats`, `gate_beats`,
`pitch_multiplier`, `volume`); at the seeded 60 bpm a beat is a second, and
the loop is `duration_beats` long. A refused set names where (#1457): a
field that does not read ends `... at /environment/ambient_audio/...`, and a
node `kind` this build does not know - which reads in as `Unknown` and cannot
be written back - is named by the value you set.

## The record's budget

A save writes a world as a MANIFEST (placements, environment, landing,
traits) plus ONE RECORD PER GENERATOR, and the 100 KiB budget
(`SOFT_RECORD_BUDGET_BYTES`) is on each of those, not on the world as a
whole. Session 874 weighed the whole assembled world (91 KB) and rationed
landmarks for nothing: its largest record was the 20 KB manifest. Read the
real numbers instead - every `room set` and `avatar set` answer carries
`record_size` (`largest`: the record, in the words a refused save would use;
`bytes`; `budget_bytes`; `over` past it), and `status.editing.record_size`
has the room, the avatar and the inventory (#1455). The avatar is ONE record,
so a detailed body spends its budget fastest (Hypha's: 42 KB).

Still worth knowing:

- **More placements of a generator already there** are the cheapest thing
  to add (about 200 bytes of manifest each). Round the landing, four extra
  scatters of the world's own ferns, moss and bushes (1,230 plants) turned a
  sandy-looking slope into forest floor.
- **A small generator placed many times** beats one tree of copies: the
  fairy ring as two tuft generators and 11 placements - each placement snaps
  to its own ground, so the ring sits on a slope without burying its
  downhill side.
- An absolute placement's `transform.scale` does NOTHING (#1454): put a
  scale in the generator's own `transform` (its children inherit it), or in
  a node inside it.

## A landmark from the world's own species

The Mother Tree (session 874) is a hand-built trunk - a flared lathe, twisted
ridge spines, buttress-root spines, bracket-fungus shelves, glowing threads -
with a CROWN that is a child node: a copy of the forest's own broadleaf
L-system, its own `seed`, a thicker `width` (old limbs) and a bigger scale,
its origin lifted into the hand-built trunk so its own trunk never shows.
The crown matches the forest because it is the forest's grammar.

- Scan seeds as renders: `render --generator crown.json` prints the
  subject's size (`subject size X x Y x Z m`), so a table of 16 seeds and a
  contact sheet of their first tiles chose a broad, drooping crown in a
  minute. The first seed tried grew a long bare trunk and a small head.
- Lay roots and threads on the real ground: ask `--terrain-report --at=X,Z`
  for every point along them and set each point's height from the answer.
- Where it stands: somewhere visitors see from the landing (across the pool,
  116 m, it tops the treeline), on ground the report says is flat.

## Long things: paths and threads

The mycelial web (session 874) is one generator of glowing spines running
round the pool on land between the landmarks - the network, and a way to
walk to each landmark by following the glow. How it was laid:

- Waypoints by hand from `--plan`, each pushed away from the water until
  `--terrain-report` says it is 0.35 m above the water line, then a smooth
  curve through them resampled every 2.5 m, each sample 5 cm above the
  ground there; 16 points to a spine (the cap), each spine starting where
  the last ended. Between samples a spine dips into bumps - it reads as a
  thread surfacing and diving, as mycelium does.
- **A spine's points are clamped to 100 m from its generator's origin**
  (`MAX_PRIM_DIM_M`, and so is every primitive's size). A web written in
  world coordinates from a generator at (0, 0) had its far end - 108 m out
  - flattened onto the 100 m line; `adjusted_at` named the points. Place
  the generator (unsnapped, `snap_to_terrain: false`) near the middle of
  what it spans and write the points relative to it.
- The sanitiser renormalises quaternions on the wire's grid: a rotation can
  come back one unit off in its last digit (`adjusted_at` names it). That is
  harmless, but a rebuilt piece then differs from the saved one by that unit.

## Working fast

Keep every step a script and chain them in one command that regenerates
each intermediate before it applies (a stale middle file cost a round trip
here). Save at each milestone the admin could want kept, and say what is
next.

The scripts for it ship in [tools/](tools/README.md): `rec.py pull` (the
saved record, refreshed after every save), a builder per piece writing its
JSON with `wire.py`, an EDITS file of `pointer file` lines, `rec.py compose`
folding them into a copy for offline renders, `views.py` for pictures from
where people stand, and `rec.py apply` sending each edit live. A `room set
/placements/-` APPENDS, and its answer names where it landed (`"pointer":
"/placements/150", "appended": true`, #1470); `rec.py apply` rewrites that
EDITS line to the index, so a re-run sets the placement instead of adding it
again.
Session 876 laid four landmarks, an outer forest and the paths between them
this way in about an hour, each saved as it went.

## Backdrop: making the far places read

"The places further out still look very empty; from the main area they
serve as backdrop and as invitations to explore" (the admin, session 876).
What made the Understory's outer ring read, and what did not:

- **Test what the main area can see before building anything.** A ring of
  24 colour-coded 50 m mock columns (emissive, two radii, every 30 degrees)
  composed into a copy and rendered from the pool, the landing and the Old
  Snag's lookout showed in five minutes which directions show over the
  treeline and which a tree wall hides (the west needed 55-60 m to clear
  it). Broadleaf trees stand 23 m, conifers 10 m: a far thing must top them.
- **In fog, only glow travels.** `fog_visibility` is where 5% of a thing's
  contrast is left, so at 300 m visibility a plain shape 250 m away keeps
  about a tenth - a faint silhouette, not a landmark - while emissive
  geometry reads through the mist: a 30 m glowing column at 290 m showed
  plainly from the pool. Particles do not:
  at 260 m a 2 m mote is a pixel. A material has no alpha, so a spore cloud
  is opaque emissive puffs: overlapping lumps of uneven size at each level
  read as a plume, a single twisting stack read as pancakes. Space the
  levels CLOSER than a puff's own vertical radius: the Puffball plume's
  first 30 cream puffs sat 3-4 m apart, 1 m tall each, and read as bread
  rolls; 75 deep-gold puffs every 1.9 m, growing and leaning downwind
  (radius and drift both rising with the square of the height), billowed
  over the treeline as a spore cloud (session 877). Keep the colour
  saturated to the top and fade the emission strength instead.
- **Surface detail belongs in a texture, not in nodes.** Lichen as disc
  patches on the Lichen Tors read as polka dots, then as sparse spots once
  cut to fit the record budget (200 discs, 30 KB); the `Lichen` texture
  (rock, two species, pale rims; `coverage`, `patch_scale`,
  `species_scale`) on the granite itself read as crusted stone at no node
  cost. Look for a texture before modelling a pattern.
- **A beacon toward the low sun loses.** The fog glows brightest round the
  sun, and a violet column there washed out to a faint lilac stick (a wisp
  over the Windthrow, SW of the landing, dropped). Put glow beacons away
  from the sun's bearing, or skip them: a place reached by its thread is
  still an invitation. Close up, 2-3 puffs a level read as a string of beads.
- **One colour per place**, so they are told apart through the mist: the
  Spore Spires green (west), the Ghost Grove pale mint (north), the
  Puffball Meadow's plume gold (east), the Great Ring's gills amber (south),
  the Windthrow violet (south-west), the Lichen Tors scarlet (north-west),
  the Indigo Shallows blue (north-east lake). A thread arriving at a place
  turns its colour for its last 25 m (`thread.py --tail-material`).
- **The outer land needs its forest, not only landmarks.** Stands of the
  world's own trees on the ridges the main area sees (a few hundred trees,
  the lighter conifers and birches first: scatter trees are never culled by
  distance) plus a thin fill out to the edges. A glade in a stand: move the
  stand's `bounds` off it (`room set /placements/N/bounds`).
- **Check the corners**: forest scatters are circles, so a map's corners
  lie outside them. The Understory's two outer lakes (490 and 530 m from
  the pool) stood in bare grass until stands of the world's own trees were
  scattered round them (`above_water_band` keeps them off the shore).
  Frame a view, never fill it: trees go behind and beside where people
  stand to look, not between them and the thing to see - the first try
  stood a conifer on the Drowned Wood's viewing shore.
- **Join the neighbours too.** Threads only out from the pool make a star;
  ten more between neighbouring places (Coral Glade -> Great Ring ->
  Windthrow, Spore Spires -> Lichen Tors -> Ghost Grove, Indigo Shallows ->
  Puffball Meadow) made a circuit of the edge a visitor can walk, each
  thread's tail in the colour of the place it reaches.
- **Where a thread arrives, let it fan out.** A thread that simply stops at
  a place tells nothing; forked into a fan of finer filaments in the
  place's colour, spreading under its fruit bodies, it says the network
  fruits where it reaches new ground (session 878, all nine places,
  `tools/fan.py`, about 4-6k triangles a place). The Great Ring's amber fan
  also filled its dark, empty centre. A fan toward a lake stops at the
  shore - the Drowned Wood's is a short cord.
- **Judge an approach from the thread visitors walk, not from a straight
  line.** A first review from 35-45 m out on the line from the pool called
  three places hidden; walked back 40 m along each place's own arriving
  thread, two of the three were in plain view - the viewpoint had stood
  inside a stand. The third was real: the Coral Glade's thread ran through
  its own north conifer stand, and moving that stand's `bounds` 37 m east
  opened the glade from 40 m out.
- **A path to each**: glowing threads from the pool's web out to every
  site, laid 5 cm above the ground sampled every 2.5 m, meandering, each
  thread its own generator placed unsnapped at its midpoint (the 100 m spine
  clamp). From the shore a thread winding off toward a glow over the trees
  is the invitation.
- **Things standing in water** (the Drowned Wood: ghost snags in the north
  lake's shallows): a grid of `--terrain-report --at` points gives each
  one's `under_water_m`; a snapped absolute placement stands on the lake
  bed, and `avoid_water` false (the default for a hand placement) keeps it
  there. The pool's own reed and lily scatters, copied with new `bounds`,
  dress the new shore.
- **The same thing in a line reads as fence posts** - the first ten snags
  followed the shallow band and looked planted. Groups of two and three,
  a lone one further out, deeper and shallower, read as grown.
- **Check each one live from the main area** at the end: from the pool's
  centre, `look --view eyes --at` north, east, south and west showed all
  four sites over the treeline, as the renders had.

## Arrivals

`default_landing` is `{pos: [x, z], yaw_deg}`. Its yaw turns
**counter-clockwise** seen from above - `place --yaw` turns clockwise - so
to face a point `(dx, dz)` away, `yaw_deg = atan2(-dx, -dz)` in degrees.
A body already standing when the ground changes stays where it was: after
reshaping, `walk-to` the landing again.

**Judge the arrival from the camera, not from eye height.** A new visitor's
first picture comes from the game's camera, about 11 m behind them and
5-6 m above the ground, and the camera avoids only the terrain - never a
building. The Understory's gateway stood 7 m behind the landing, so every
arrival looked through its pillars and translucent veil, and a taller body's
camera sat under its caps; five sessions of reviews from eye height missed
it (session 878). `tools/views.py ... "@landingcam"` renders that first
picture from the record. Keep a gateway (or anything tall) either more than
about 13 m behind the landing, so the camera sits in front of it, or off
the line behind it - moved 8 m back along the same line, the gate still
faces the landing, and walking in still read `picker: open`.
