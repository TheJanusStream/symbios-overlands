# Your region: land, water, sky

A world's own settings live in its record beside the things placed in it:
`/generators/base_terrain` (the ground, with the water as its child),
`/environment` (sun, sky, fog, clouds, the water's look, the ambient sound),
`/default_landing` (where and which way visitors arrive) and `/geo_source`
(a square of real Berlin the ground is built from - see "Real Berlin
ground" below). All of it is edited with `room set` and kept with `save`,
like a building.

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

**A seeded world's land may be the wrong kind entirely.** Reeve's (session
879) was a 69 m rough `DiamondSquare` mountain; an English lowland manor
wanted soft hills. `FbmNoise`, `height_scale` 30 m, `base_frequency` 2.2,
`octaves` 5, `persistence` 0.42, 60,000 erosion drops gave rolling
lowland; a seed scan of 24 seeds (6 s) found one with a mere below a gentle
ridge, and dropping the water from 6 to 5.2 m shrank the mere to a lake with
an outflow. FBM makes blobby lakes, never a river: plan water mills and
streams out ([the water is one flat plane](#the-land-is-a-recipe-not-a-sculpt)).

**Mesa country** (Jink's Parabola Flats, session 893): `VoronoiTerracing`
with `voronoi_num_terraces` 2 and `voronoi_num_seeds` 50 lays big flat cells
at two levels - a floor at 0 and tabletop mesas at half the `height_scale` -
and thermal erosion slumps their walls: 60 iterations left 12 m cliffs, 250
at talus 0.02 rolled them into 11-degree hills, and 120 at talus 0.06 with
`height_scale` 30 gave 15 m mesas whose walls reach 19 degrees over 60 m,
still drivable. Six terraces over 300 seeds made a patchwork of small
cliffs, and FBM made rolling hills with no flat floor. The seed scan's
`p50` names the seed with the most floor (0.22 m: over half the map at 0).
A mesa 15 m high reads as a thin strip on the horizon from 400 m: height on
the skyline comes from placed rock ([Backdrop](#backdrop-making-the-far-places-read)).

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

**A terrain layer can be low vegetation.** Ashmere's heath layer was a flat
grey-brown `Ground` and read as bare earth; the same layer in heather colours
read as red clay - a flat colour cannot make a carpet. A `Moss` layer did:
cushions 0.8 m across (`cushion_scale` 14 on Ashmere's tile of 10.8 m - a
tile is the world's width over 90, 11.4 m on the default 1022 m world),
faded grey-purple tips over dark stems, rusty `dry_patches` 0.4 - the whole
hilltop read as heather past its flower, for no parts at all (session 883).

**A ground pattern is sized per tile, and a tile bakes at 512 px**: 11.4 m
on the default world, so about 2.2 cm a pixel. Parabola Flats' `CrackedEarth`
pan drew 1.4 m plates (`scale` 8 plates a tile) with 14 cm cracks
(`crack_width` 0.012 of the tile) and the default curl lifting every edge:
from eye height, giant crazy paving in every frame. A dry lake's crust is
plates a hand or two across with thin cracks. The texture's envelope stops at
20 plates a tile (0.57 m); cracks under about two pixels (`crack_width` below
0.0045) draw as a jagged zipper, so take 0.0045, a small curl (0.08 over
0.01), `normal_strength` 0.7 and a paler crust (session 895, Jink's
`b/ground_playa.py`). A bigger bake would cost every browser visitor GPU
memory for four layers; widen the cracks instead.

**A procedural texture's colours are LINEAR**, unlike a material's
`base_color` (sRGB): the generator converts them when it bakes. A litter
meant as dark brown sRGB `(0.36, 0.25, 0.13)` is written
`((c + 0.055) / 1.055) ^ 2.4` each: `(0.107, 0.051, 0.015)`. Written as sRGB
it bakes out light tan, and a hillside reads as sand.

**Sunlit ground seen toward a low sun is partly sheen, not texture** (#1467).
The terrain's roughness (0.85) and reflectance are fixed in code. At the
default reflectance of 0.5, looking toward the Understory's 20-degree sun,
about 85% of a sunlit patch's brightness was specular sheen in the sun's
own colour, so five litter colours up to 45% darker measured the same
(sunlit (122, 101, 70) each; (102, 84, 58) after the ripple fix below) and
the hills stayed tan. Since 2026-09-27 the terrain's reflectance is 0.25 in
every world (the owner's choice, `MATERIAL_REFLECTANCE` in `src/config.rs`):
that sunlit patch reads (74, 59, 38), browner, and shade darkens too, from
(30, 28, 24) to (18, 16, 10), because the environment's specular goes with
the reflectance - shade readings from before that date do not compare. A low
sun still puts some sheen on the ground. Test a colour change in SHADE, or
paint a layer pure green for one render: if the sunlit patch barely moves,
the colour is not your lever. What did help: the ripple. A ForestFloor `litter_scale` is capped
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
- **The haze's tint is its `fog_extinction` and `fog_inscattering` colours**,
  not only `fog_color`: a seeded world's green pair kept Ashmere's sky and
  distance teal after `fog_color` was set blue-grey. Set all three.
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
  exactly. Ask before applying one when you are saving as you go: a trial
  holds every other save until the admin answers. Ashmere's was offered in
  chat, applied on "Sure, go ahead", kept on "keep it" a minute later (a
  Michaelmas evening: the sun at 14 degrees WSW, warm, a 1,000 m haze) - then
  written into the land builder too, so a re-run of it cannot revert the
  light. When the admin goes quiet with a trial live and other work needs
  saving, set the trial's field back to its saved value, save, and set the
  trial again: the rest is kept and the trial stays theirs to judge
  (session 879 held a sound trial that way). `tools/rec.py save room OUT
  --hold /environment/ambient_audio` does all three.

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
- **A scatter keeps drawing until it has `count`**, and gives up after
  `count` x 10 tries (`world_builder/compile/census.rs`, `scatter_yields`).
  So a filter costs copies only when it refuses more than about nine points
  in ten: moss hugging the waterline (`above_water_band` 0.03..0.9 m) kept 49
  of 260 because the band refused almost the whole circle; widened to 1.2 m it
  kept 871 of 1,300. Short of that, every copy asked for is placed - Ashmere's
  reeds, banded -0.55..+0.25 m round a mere with a wide shallow margin, placed
  all 2,600 asked for, far out into the shallows; 900 made a reed bed. Read
  the report's `copies` against `requested` either way.
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
  triangles, and reads fuller. Spelled out (session 879, Ashmere's reeds,
  grass and tussocks, parts 8,765 -> 5,415 with the same look): a `lathe`
  of two stations `{radius 0.1, height 0}` and `{radius 0.1, height 0.628}`
  (the height equal to the circumference, so the card spans it once),
  `resolution` 3 and `smooth` false - three flat faces, one atlas cell
  each, 12 triangles (smooth, or more sides, was 42-84 a tuft: 280k over
  3,350 tufts) - the card's own texture with `variant_rows` 1 and
  `variant_cols` 3 added, `uv_scale` 15915, `torture.taper` [-0.6, -0.6],
  and `transform.scale` [width / 0.2, height / 0.628, width / 0.2]; a reed
  ring wider than about half the card's width falls apart into sub-clumps. The Understory went from about 48,700 parts
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
  the catalogue's usual 8, a broadleaf is 9,578 triangles, at 5 it is 6,176
  (conifer 5,100 -> 3,570, the #910 birch 5,166 -> 3,888, bush 3,958 ->
  3,070; measured in session 878). The catalogue's `lsys_birch` is Ashmere's
  birch since #1496, 4,316 triangles at 5, and the six plants that came with
  it (oak, young oak, apple, hazel, gorse, yew) all ship at 5. The
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
- **Emission adds its colour flat over the whole surface**, panes and
  mullions alike: on dark glass even `emission_strength` 0.05 of warm white
  read grey-brown, and 0.3 made a glass lobby a beige wall - it looks
  exactly like a texture whose colours were ignored (Eigen's Spire,
  session 903). Glass gets no emission and reads by its reflections; "lit"
  comes from small lit pieces (a canopy soffit, an open entrance, a framed
  sign).
- **A texture on a twisted or tapered part (`torture`) shears with the
  deform**: a curtain-wall grid laid on one twisted box read "crooked" to
  the owner. Build a twist from straight pieces instead - one box a floor,
  each turned and tapered at its own height, a band over each joint - and a
  box projection centred on each piece puts a grid line mid-floor unless
  `uv_offset` (metres) moves it half a floor (b/spire.py v3).
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
the loop is `duration_beats` long. **Beats are decimals on the wire, x10,000
like every other**: the seeded recipe's `duration_beats` of 340000 is a
34-beat loop - 34 s - not 340. The bake stops at the loop's end plus its
`loop_crossfade_beats` tail, and an event that starts later never sounds:
Ashmere's builder took the seed's 340000 for 340 s and set its animal calls
at 37-323 beats, so only one rook call ever played (found 2026-09-27).
Check every event's `time_beats` against `duration_beats` before a set.
Session 883 brought the calls inside a 2-minute loop (the owner's yes): its
cost is its length, by the arithmetic of the code's constants (not measured) -
16-bit mono at 22,050 Hz stores 44 KB a second (5.3 MB
for 2 minutes) and the bake mixes in 32-bit floats (10.6 MB), which a wasm
client never gives back - so 340 s would have cost about 45 MB. At their
first volumes (0.11-0.30 under a 0.16 breeze) the owner heard "a little
honk once in a long while"; at 2.2 times "the cow's moo is very loud"; kept
at 1.6 times with the cow at 1.1. Derive such a fix from the ORIGINAL sound
kept aside, not the saved record: re-running the fitting on a record that
already held it squeezed the calls into 40 s and raised them again. A
refused set names where (#1457): a
field that does not read ends `... at /environment/ambient_audio/...`, and a
node `kind` this build does not know - which reads in as `Unknown` and cannot
be written back - is named by the value you set.

- **A seeded world's sound may not suit the place.** Reeve's came with a
  fiddle theme, a bass line and a humming "siren" drone; a manor of 1300 kept
  the wind bed and gusts and got a church bell and rooks instead. A bell is
  its strike partials as `Sine` nodes (hum 0.5, prime 1, tierce 1.19, quint
  1.5, nominal 2 times the note, falling amplitudes) summed into one `Gain`
  whose `gain` input is an `Adsr` (attack 4 ms, decay about 4.5 s, sustain
  0) driven by a `Gate`, then a `Reverb`; each stroke is an event a fraction
  of a beat long. A rook's caw is a `Sawtooth` plus a little `WhiteNoise`
  through a `BiquadBandpass` near 1.1 kHz under a quick envelope, events in
  twos and threes. The agent cannot hear: a new sound is a mood change, so
  offer it live and unsaved and let the admin's ears decide. Ashmere's bell
  (nine strokes meant once in 340 s; in the 34-beat loop, eight of them every
  34 s) was "pretty annoying over time" and went;
  the loop repeats for as long as anyone stays, so a sound people notice once
  is one they hear every few minutes. Keep punctuation sparse, soft and
  varied (the rooks: short, quiet, at uneven times). The seeded wind was
  "uncomfortably loud and violent" too (a high-passed hiss at volume 0.5 with
  gusts): a breeze is `PinkNoise` through a `BiquadLowpass` near 380 Hz whose
  cutoff and gain breathe on slow `Lfo`s (0.05-0.07 Hz), at volume 0.16.
  The admin then asked for "a few animal sounds ... not in a pattern ...
  just single calls": a few events a loop at uneven times, as one-off
  calls. Oscillators take a `freq` input and filters a `cutoff_hz` /
  `center_hz` one (`input_ports` in bevy_symbios_audio's `ui/graph.rs`), so
  a moo is a `Sawtooth` near 112 Hz under a low-pass whose cutoff a second,
  slower `Adsr` opens (+480 Hz: "mmm-ooo") while lifting the pitch a little
  (+14 Hz), and a bleat a `Sawtooth` near 330 Hz with a 7.5 Hz `Lfo` on its
  `freq` (+16 Hz) and on a tremolo `Gain`, through a band near 950 Hz.
- **Borrow a catalogue item's sound** as you borrow its material: any node
  may carry `audio`, played from where the node is; the medieval
  `blacksmith`'s glowing forge part carries a fire's crackle over a low roar
  (`render --dump --catalogue blacksmith`, the node with an `audio` key), and
  copied onto a smithy's coals it needs no authoring.

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

**A second ceiling: the whole world, sent live.** Every live edit is sent to
the players in the world as the WHOLE record in one message, and past
`MAX_RELIABLE_PAYLOAD_BYTES` (900 KiB of compact JSON) the sender refuses it
(#1123) - so no live edit reaches anyone. A save does: the game tells the
world it was saved, and every player in it fetches the saved world from the
account's PDS (#1499) - the first save at once, then at most one fetch every
10 s however often the world is saved. The per-record gauge stays green all
the while. Ashmere was past it before session 883 began (1.2 MiB, refused
at the owner's first join) and ended it at 1.36 MiB: before #1499 the owner
heard a new sound only after a save and a trip through the gate, and asked
"Maybe you need to save, for me to hear it". Session 885's grammar buildings
took it to 1.60 MiB, and the two cuts below brought it back to 1.45 MiB.
The agent's answers say when the world is past it (#1500): `room set`
answers `live_sync` (`bytes`, `ceiling_bytes`, `refused`) when an edit went
nowhere live, and `status.editing.live_sync` weighs the world every time.
Past 900 KiB, show the owner a change by saving it; a player whose build
predates #1499 still sees it only after stepping out and back.

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
- **A generator nothing places still weighs**: a save keeps every
  generator, placed or not. Strike them out with ONE `room set /generators
  FILE`, FILE the pulled record's whole `generators` map without them (grep
  the record for each name first: no placement's `generator_ref` may name
  it). Ashmere's ten unplaced cottage generators were 58 KB (session 885).
  A whole-map set of Ashmere is past 1 MiB, which the control socket
  refused with a bare "Broken pipe" until #1511 (16 MiB since).
- **Buildings of one size can share one generator**, each placement
  carrying its own grammar seed (#1505; [building.md](building.md),
  "Buildings by shape grammar"): Ashmere's fifteen grammar houses on five
  generators saved 98 KB.

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
- **A flat lane's spines z-fight at their joints.** `thread.py` starts each
  16-point spine where the last ended, so on a lane (`--flat`) the two lens
  tops share a ring and a plane: `room set` named 0.06-0.5 m2 at every joint
  of session 893's circuit. Raise every other spine 8 mm - its points are
  written divided by F, so add 8 mm / F to their y - and a loop laid as
  overlapping pieces at alternating lifts needs an EVEN number of pieces,
  or the last and the first share a lift where they close.
- The sanitiser renormalises quaternions on the wire's grid: a rotation can
  come back one unit off in its last digit (`adjusted_at` names it). That is
  harmless, but a rebuilt piece then differs from the saved one by that unit.

## Ramps and jumps for wheels

Session 893 built Parabola Flats' Jump Line and drove it with `drive`
([moving.md](moving.md#driving-a-run-stunts-measured)) on #1524's air model.
A jump is for cars and hover-boats: they ride it on suspension rays cast
straight down, which push on any collider but a sensor, so every part of a
ramp is solid.

- **Size a jump from a driven run, not from arithmetic.** The first
  landings were laid by the ballistic sum; driven, the small jump came
  down past its landing's end and the medium at the very end of its own,
  bleeding half its speed. `drive W@20 none@4 --wait` down the line gives
  each jump's airtime, where it left and landed, and its landing pitch;
  lay the landing where the car comes down (Jink's Cyclecar at 14 m/s:
  0.70 s off a 15 degree, 1 m lip; 1.33 s off a 24 degree, 2.4 m one).
- **A tabletop's deck at lip height is not a jump**: the car rode its
  springs across it (the small jump, 14 m/s over a 1 m lip) and never
  left it. A deck 0.5 m below the lip gave a 0.70 s flight.
- **Wide enough to hold a line**: a car on a held key drifts, and a
  landing that yaws it sends every later jump off its line. 6 m ramps could
  not hold one over three jumps; 9 m did.
- **A gap's landing is a hump, not a wall**: a steep face (30 degrees up
  to 1.4 m) and a gentle run-out (14 degrees), so a car that comes up
  short meets a slope.
- **A curved lead-in** is two shallower wedges in front of the kicker, at
  0.3 and 0.62 of its angle, meeting the slope at 12% and 35% of the lip's
  height, each 1 cm narrower a side than what it meets so no side faces
  share a plane (Jink's `b/jumps.py`, `kicker`). Session 893 added them for
  a scrape that was most likely not one: `height_m` read 0.0 on a straight
  kicker, which a speculative contact did too until #1528, and on #1524's bench
  the small kicker at 15 m/s never brought the Cyclecar's box within 19 mm
  of the ramp. The landing is where a box hits.
- **After an `apply`, `look` until `world_building` is false before the
  next run**: one started seconds after an apply (15:34) drove the whole
  line without a jump, most likely on ramps still being rebuilt.
- **A kicker on a slope** (session 895's Mesa Drop, Jink's
  `b/mesa_drop.py`): fit a plane to the ground under the kicker's footprint
  (`--terrain-report` on a grid), tilt the generator's frame to it through
  the placement (`snap_to_terrain` off, the plane's own height at the lip),
  and sink every part past the deepest point where the real ground falls
  below the plane; put the run on a ridge's crest, where the ground is level
  across (`--terrain-report` across the line, not along it). A small kicker
  launches nothing: 15 degrees over a 0.6 m lip flew 0.6 s and never rose
  above the lip, the springs took the kick; the medium jump's size (20
  degrees, 1.6 m) flew 1.23 s off the brow with 3.35 m under it. Give the
  kicker a back ramp: a car climbing from below met the lip's 1.8 m back
  face and ended `stuck`.
- **A drop that lands on the natural slope lands hard**: the car levels
  itself in the air while a mesa wall slopes 15-19 degrees, so it came down
  at 9.75 m/s, about 5.8 m/s into the slope (the Big One's landing hump is
  about 4) - it held, 94% of the speed kept. Softer needs a landing ramp
  steeper than the slope where the car comes down, which on a slope means a
  tabletop: a ramp's top standing above the ground is a wall to a car that
  falls short.

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
- **Rock formations are one BlobGroup of blended boxes.** Session 893's
  sandstone buttes: faceted lathes read as storage tanks with rims, rounded
  boxes (`bevel`) as brick buildings (a `Rock` texture on a flat wall is
  brickwork), and a BlobGroup of 3-6 `box` elements blended 2-5 m over an
  `ellipsoid` talus mound read as eroded sandstone - a Monument Valley
  mitten, a castle, a stepped spire - at 1 part and 3-5k triangles each. A
  single tall rounded column reads as something else entirely up close:
  build spires from boxes stepped and tilted a few degrees.
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

## Lanes, hedges, fields, smoke

- **A lane is a flattened thread** (`tools/thread.py --flat`): a spine squashed
  in height is a wide, low lens on the ground. Set it so only its top arc
  breaks the grass - a 4 m radius flattened to 0.08 with its top 4 cm up
  shows a 3.9 m lane whose edge rises 4 cm over a metre. A narrower, higher
  lens (2.3 m, flat 0.05) drew a dark lip along each edge wherever the ground
  fell away sideways. A lens shows few of its sides: give it 24-28.
- **A hedge is BlobGroups along the ground** (`tools/hedge.py`): the hawthorn
  hedge on three sides of a 90 x 75 m manor court is three hedges of 12
  BlobGroups, 15 parts with their roots. A 1.7 m oak paling there
  read as a dark fortress wall in shade; the hedge read as countryside.
- **Open-field strips (ridge and furrow)** are flattened threads side by side:
  8.6 m ridges on a 9.4 m pitch leave a furrow of grass between them; each
  follows the reversed-S an ox team's turn left in real selions; the last two
  samples at each end shrink the radius so the ridge narrows and dives into
  the headland (cut square, their ends read as planks). Stubble is a pale
  `Ground` texture; fresh ploughing a `Thatch` texture in soil colours
  ([building.md](building.md), "A building in few parts"). Twenty selions in
  two furlongs were 40 parts and 26k triangles, and from the arrival camera
  the strips on the slope beyond the village read at once as a medieval field.
- **A fence on falling ground** is `tools/fence.py`: wattle panels (a
  `Thatch` texture in hazel colours) each tilted to the ground between its
  ends, stakes at the joints, `gap` in the waypoints for a gate; 6 m panels
  made the street's two frontage fences 68 parts. A hurdle fence 1.15 m high
  encloses a toft without hiding it; the same idea in a 1.7 m oak paling round
  a whole court read as a stockade.
- **A single flat blob over sloping ground is a terrace**: a trodden-earth
  yard as one wide flattened BlobGroup stood proud on the downhill side with a
  shadowed edge. Anything wide and flat must follow the ground piece by piece
  (thread.py, hedge.py, fence.py do) or be left out.
- **A lane's material cannot make a street look used.** A spine's texture
  runs round its ring (U, in metres) and along it (V), and a `Ground`
  texture's soil patches are low-contrast: a 4 m tile with metre patches
  (session 883) changed nothing visible from the arrival. Ruts did: two dark,
  wet strips a cart's gauge (1.45 m) apart down the middle, wandering a
  little, each a thin flat thread laid ON the lane (`thread.py --ride`) and
  then seated exactly on its drawn top (the account's `b/ride_fix.py`).
  `--ride` then read the ground under the rut, not under the lane's middle,
  so on the street's side slope the formula left them from 6 cm under its
  top to 5 cm over it, where they read as kerbs; since session 883's review
  it reads the lane's middle (its OFF is signed) and lands within about
  1 cm. Where two lanes overlap it still sees only one. `--floating-report`
  then names ruts as floating: they rest on the lane, which it does not
  count.
- **A flat lens over a slope stands proud of it**: its top rises about
  `radius x (sqrt(F^2 + g^2) - F)` on a grade g, 0.18 m for a trodden yard
  (radius 4.5, flat 0.05) on a manor court's 7% - it buried the scattered hens
  and covered the court's path, and was not kept. Wide flat things suit
  gentle ground only.
- **Join two lanes by overlapping them**, each tapered where the other takes
  over (`thread.py --taper-start/--taper-end`) and one 1.5 cm higher: end to
  end, their square end caps drew a crease across the street.
- **Smoke from a hearth** is a `particles` node on the roof: a narrow upward
  `cone` emitter (a `sphere` one sprays puffs outward and down), `Puff`
  texture, big pale puffs (1.1 -> 4.8 m, alpha 0.26 fading to 0) rising
  slowly (speed 0.12-0.3, `gravity_multiplier` -0.035) for 8-12 s at 3-5 a
  second. Small, dark, fast puffs drew thin streaks shooting straight up.

## Filling the land round a village

Session 879's village and its fields filled the middle 300 m of a world about
970 m square; the admin: "there is still a lot of empty space". What worked:

- **Find the empty ground from straight above**: an aerial `--world` shot
  from 700 m is all fog; `tools/clearings.py DID RECORD OUT.png X,Z --dist
  600` pushes the fog out and looks straight down. Two tiles (north and south
  halves) showed the village, its fields and bare grass everywhere else.
- **Fill it with what the place's own economy put there**, each a small
  set of pieces that reads at a glance: for an English manor of 1300, a
  rabbit warren (a flint lodge, long turf pillow mounds, coneys), a turbary on
  the fen (strips of cut-over peat, turves drying in small piles and long
  stacks, the cutter's cot), the lord's deer park (a pale on a bank, a gate, a
  lodge, fallow deer), a wood with a charcoal burner, worts beds in the
  crofts, horses. A far piece earns its place by its silhouette from where
  people stand: a tall flint lodge on a rise reads from the road at 250 m.
- **Buildings of a kind come from a grammar** once one exists
  ([building.md](building.md), "Buildings by shape grammar"): a holding
  along a road or a byre in a toft is a footprint and a seed per building.
  Session 885 set Ashmere's tofts five outbuildings and the road west a
  tenant's holding (house, byre, cart lodge round a yard) that way in
  minutes each; the time went on siting them - `near.py --box` against the
  apple garths, the pig scatter and the hedge lines.
- **A long enclosure goes in arcs**: `fence.py` and `hedge.py` refuse a
  generator reaching past 100 m from its middle, so a 720 m oval park pale is
  five arcs, a bank (`hedge.py` with a turf-coloured `Ground` material, 0.55 m
  high, 3 m wide) under each. Size a gate's gap along the line the pale runs
  at that point: on an oval of half-axes A (x) and B (z), the south point's gap
  is `asin(half_gap / A)` of arc - dividing by B made it 7.9 m, not 5.4.
- **Lay small flat things ON the ground**: a level 4 m garden bed on a 4 degree
  croft stood 0.4 m proud at its downhill end like a board. `ground.py ...
  --lay YAW` prints the wire rotation that tips a thing's local Y to the
  ground's normal, turned YAW degrees clockwise seen from above as `place
  --yaw` turns it (90 faces local -Z to +X; on level ground it is `place
  --yaw YAW`'s rotation) - and as its printed `contour_yaw` counts, so
  `--lay` at that yaw runs local +X along the contour. Dark still water on
  a flat cuboid mirrored the low sun and lay on the grass like pale metal at
  any roughness under 0.5 - a peat cutting drawn as matte dark peat read
  true - up close. A generator material has no reflectance field, so any
  flat dark top seen toward a low sun still takes its sheen: from 35 m the
  turbary's three cuttings read as pale plates on the lawn. Cut as rows of
  narrow trenches with turf showing between (as turbaries were worked) they
  read as cut peat near to; toward the sun they still shine.
- **Scatters do not avoid each other, or your buildings**: a second pose of a
  beast in its own scatter over the same pasture can stand inside the first;
  give each its own ground, or place the few by hand, and check a hand-placed
  spot against the record's placements before rendering (`tools/near.py`: a
  horse set by eye stood inside a cottage 2.8 m off). `views.py` takes a point to look at (`TX,TZ` for
  LOOK): +Z is SOUTH, and five views by a hand-worked bearing looked away.
- **Pick things the primitives can draw**: a bean row of blobs was a green
  caterpillar however lumpy; late September gave the truer answer anyway (the
  beans are in, the bed is dug). A `LogEnd` texture is a card, clear outside
  each log's end: on a box it drew floating discs - a woodpile is capsules.

## A city: the road network

A street plan is not placed piece by piece: it is a
`network.symbios.gen.road_network` child of the terrain generator (beside
the water), and every client traces it from the land at load - the
tensor field's contour lines become the major streets, the fall lines the
minor ones, and a dead-flat area a grid. What its lots grow IS written into
the record: the first client to see the network injects theme-catalogue
buildings onto its lots (`lot_building_...` generators and placements),
and a save keeps them. Isoline (Eigen, session 903) is the first region
built this way; its builder is `exports/eigen/b/city.py`.

- **The land decides the plan.** On the seeded 56 m DiamondSquare slope the
  trace was a tangle of wobbly lanes; on a gentle FBM slope (36 m, 4-8
  degrees) the same network traced boulevards along the contours stepping
  down to the shore, crossed by straight fall-line streets. Choose the land
  for the streets before anything else (`--seed-scan`).
- **Try it offline**: `render --world <DID> --world-record copy.json`
  traces the streets and grows the lots exactly as the game does (the log
  line says `streets=` and `buildings=`), and `--triangle-report` counts the
  streets (`streets`, a row a network: three parts) and the buildings the
  lots will grow (`grown`: how many the record did not carry yet). A
  network put live grows its buildings into the live record at once;
  `revert` takes them out.
- **The fields that make a city** (session 903, #1552-#1555): `avoid_water`
  (streets end at the shore and no lot touches the water - without it they
  run across the lake bed), `major_spacing`/`minor_spacing` (70/35 m read
  as a city; 95/55 as a town; 60/30 as clutter), and in `lots`:
  `theme_override`, `tier_bias` `network.symbios.lot_bias.downtown` (a
  building on every lot, landmarks on the top 15%), `escalation` 0 and
  `prosperity` (the room DID's own scene otherwise - Eigen's rolled 0.73,
  which grew barricades, sandbags, wreckage and leaning ruins), `fit` (each
  building drawn at its lot's size; off, at its catalogue size whatever the
  lot), `lot_area` (the largest lot, m2: 400 grows house plots, a few
  thousand a downtown) and `focus` (a core in room XZ: the landmarks stand
  nearest it). Without a core the BIGGEST lots take the landmarks, and
  those lie at the district's ragged edge: Isoline's first trial stood its
  megatowers on the outskirts and small blocks on the waterfront.
- **Shape the plan with a street field** (#1556): `field` on the network.
  Its `basis` lays designer fields over the land's own - a
  `network.symbios.road_basis.ring` (major streets ring its `center`,
  minor streets run straight out from it) or a
  `network.symbios.road_basis.grid` (major streets along its `bearing`, a
  compass bearing: 0 north-south, 90 east-west; minor streets square to
  them) - each reaching `radius` m from its centre, its `strength` (1 pulls
  as hard as the land) fading to nothing at the edge; beyond every field
  the land decides alone. `terrain_weight` (default 1) trades the land
  against the fields inside their reach (0: the fields alone decide there),
  `smoothing` (m, default 0) reads the street directions off land blurred
  over that scale, so streets sweep along a hillside instead of turning at
  every bump, and `keep_out` lists discs (`center`, `radius`) no street
  enters and no lot grows a building in. Centres are room metres (X, Z),
  like the district's own. The sanitiser clamps what it cannot take, with
  no warning: at most 8 fields and 16 discs, centres within 1,024 m of the
  origin on each axis, a field's radius 5-1,024 m and strength 0-10, a
  disc's radius 2-512 m, smoothing up to 100 m, terrain weight up to 10,
  and a bearing folded into 0-180 (180 is the grid at 0). On the wire
  every value is x10,000, as everywhere: a ring reaching 150 m from the
  point (40, -25) at strength 1 is
  `"field": {"basis": [{"$type": "network.symbios.road_basis.ring",
  "center": [400000, -250000], "radius": 1500000, "strength": 10000}]}`.
  Any field edit re-traces the whole district and regrows its lots.
- **Tidy the plan with the layout revision** (#1558): `layout_revision` on
  the network. 0 - every network saved before it, and what a network
  without the field reads as - is the plan its lots were grown from, byte
  for byte. 1 tidies the traced streets: junctions joined by a street
  shorter than 2.5 major footprints (about 10 m) or a third of
  `minor_spacing`, whichever is less, merge into one - never into a
  cluster wider than that, so a wide street on a dense plan merges the
  junctions of one junction, not a district; two streets running side by
  side (within 15 degrees, for 20 m and 40% of the shorter) closer than a
  quarter of `minor_spacing` or than their curbs plus 2 m - but never half
  of `minor_spacing` or more, and never with room for a lot (6 m and a 2 m
  sidewalk either side) between their curbs - go down to one; a loop
  street round less than a quarter of a nominal block and a second street
  between the same two junctions three or more times as long as the first
  (a detour, not the far side of a thin block) are opened, judged on the
  plan as traced - once the district edge has cut the streets leaving a
  real block's corners, that block reads as a loop - and so is a face
  smaller than 5% of a nominal block; a dead end shorter than half
  `minor_spacing`, and never longer than four street widths (33 m at the
  default widths), goes; each street takes one road class; and streets end
  at the drawn district's edge instead of running on past it - so no block
  is closed by a street nobody sees, and the blocks at the rim grow no
  lots (Isoline: 44 lots at 0, 29 at 1 - cutting its plan to the drawn
  district alone leaves 29). Every lot then keeps 2 m clear of every
  street's curb (its sides facing a neighbouring lot keep their own
  setbacks), no street prop stands on a street, and no building or prop is
  grown larger than its lot: a lot too small for every building of its mix
  grows nothing, and the road panel counts those. Small lots pay most for
  the clearance - at the default 400 m2 lot area half the lots are
  7 to 8 m across their short side, and one facing a major street gives up
  3 m of it - so a dense plan of wide streets grows few lots or none:
  raise `lots.lot_area`, or narrow the streets. 2 (#1563) is 1 derived
  with portable maths: every client - the native agent, a browser - grows
  the same district from it, where at 0 and 1 a lot sitting on a threshold
  can be kept on one platform and dropped on another until the district is
  saved. The editor's new networks take 2, an older one shows `Upgrade to
  revision 2` on its road panel, and in a record it is
  `"layout_revision": 2` - a plain number, not x10,000. Changing it re-traces the district and regrows its lots, as any
  layout edit does. A revision this build does not know reads as its
  latest. How the streets are drawn - junction decks, curbs - is not part
  of it: every client meshes the plan itself, so a meshing fix reaches
  every network.
- **Streets run through what you placed.** The trace knows nothing of the
  room's placements: Isoline's streets ran through the Spire's podium on
  the shore. Stand a landmark where no street goes - in a keep-out disc
  (above), in the water with `avoid_water` on, or outside the district. A
  street can still graze a disc's rim by a few metres where it snaps onto
  a junction beside it, a street passing the disc keeps only its
  centreline outside (its deck, curb and street furniture reach a few
  metres further in), and a lot is dropped only when its centre is in the
  disc, so give a landmark's disc a margin beyond its footprint.
- **Pave it.** Between the streets is the terrain's own splat: sand reads
  as a desert with roads on it. A splat layer can be any texture: `Pavers`
  in the city's height band (pale, `color_stone` about 0.66) made it a
  city (`exports/eigen/b/ground.py`).
- **Count before saving**: the catalogue's city buildings are part-heavy (the
  supertall tower `neon_megatower` is 37 parts and the helix tower
  `data_spire` 48 as the record counts them since #1559; the old neon
  megatower was 58). Isoline's 61 buildings and its streets are
  2,938 parts and 819k triangles; 400 m2 lots would have grown 160
  buildings. The lots' generators are written into the record (one per
  building and drawn scale): 418 KB of compact JSON in all, 45% of the live
  ceiling.
- **A grown building can stand on your arrival line.** A holo billboard
  grew on Isoline's axis and hid the Spire from the first landing: check
  the landing's sight line against the lot placements (their positions are
  in the record) and its clearance from every footprint before choosing it.

- **A field the deployed client does not know traces differently there.**
  Every client traces the streets from the record, but the lot buildings
  are saved: save a network with a new field (a street field, a
  `layout_revision`) before the owner deploys it, and a visitor on the
  deployed client draws the old streets under the new buildings. Say so
  when you save one, and tell the owner it waits on their deploy.

## Real Berlin ground

`/geo_source` is `{dataset: "berlin", min_e, min_n, size_m}`: a square of
Berlin in whole metres of EPSG:25833 (ETRS89 / UTM 33N), its south-west
corner and side. With it, the ground is the city's own terrain at real scale
and real altitude: the heightmap keeps `base_terrain`'s `grid_size` and
`cell_scale`, centred on the square, and the height fields are no longer
read. Heights are metres above sea level, 26-123 m. `room set /geo_source
null` goes back to the generated ground.

- **The square is checked.** Its side snaps to whole 10 m between 250 m and
  19 km, and a square that is not wholly inside Berlin is moved to the
  nearest place it fits. `room get /geo_source` after the set shows where
  it landed.
- **The ground is fetched when the world loads** from Berlin's map service
  (gdi.berlin.de) and kept on the device for 30 days. Everyone in the world
  fetches the same data. If the service cannot be reached, the world falls
  back to `base_terrain`'s generated ground and says why: an amber
  terrain row while loading, a warning toast in game. If only the land use
  cannot be had, Berlin's terrain stays, coloured by the altitude rules
  and dry, and says that.
- **A small square is a small world.** The ground spans `grid_size` points
  `cell_scale` apart, but never more than the square's side: a 250 m square
  gives a ground about 250 m across.
- **The land use paints the ground, by layer role.** Parks, woods,
  meadows, cemeteries, allotments and sport grounds take the first of
  `base_terrain`'s four material layers; built-up blocks, farmland, bare
  fallow, construction sites and river beds the second; streets, squares
  and rail the third. The altitude rules are not read. To change how Berlin
  looks, change the layers' textures, not the rules.
- **Scatters keep to natural ground.** A scatter's `biomes` filter reads the
  layer a point is painted with, and built-up blocks, streets, squares,
  rail, sport grounds, construction sites and water count as no layer at
  all. So a stand with `biomes [0, 1]` grows in parks, woods and fields,
  never on a street. Its `altitude_band` is not read on Berlin ground.
- **Berlin sets the water level.** Where the square has a river or lake,
  every water child under `base_terrain` is drawn at Berlin's level (the
  Spree at the Museumsinsel: 30.5 m), whatever its `transform.translation` y
  says; with no water child there is no water, and the beds lie dry. The
  beds are carved below the level, all other ground is kept above it, and
  a body more than 3 m off the level (a lake on the plateau over a river)
  is drawn dry. The level is the largest body's that does not sink much of
  the square under it: a pond on a hill leaves the valley below it alone,
  and if no body can set a level, the square has no water. Bridges are
  causeways at the waterline until buildings come. The World Editor's
  Region source shows the level.
- **Blocks and buildings are not Berlin's yet.**

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
picture from the record.

**Mock the massing before building.** Before anything was authored, plain
boxes where each building would stand (a church, cottages, a hall, a barn, a
mill), composed into a copy and rendered `@landingcam`, showed in minutes that
Ashmere's planned landing - on a flat plateau 40 m back from its brow - saw
flat grass, a looming church and a sliver of the mere: a brow hides the whole
slope below it. At the brow the same camera looked down the street to the
mere. Turning the landing's yaw 12 degrees then brought the mill on the far
hill fully into the frame; the horizon sits near the top of the arrival
frame, so a far landmark shows there or not at all. Keep a gateway (or anything tall) either more than
about 13 m behind the landing, so the camera sits in front of it, or off
the line behind it - moved 8 m back along the same line, the gate still
faces the landing, and walking in still read `picker: open`.
