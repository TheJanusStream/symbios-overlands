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

## Water, sky and light

- The water's look is in the water child's `surface` (`deep_color`,
  `shallow_color` with alpha, `roughness`, waves, wakes) and in
  `/environment` (`water_normal_scale_near/far`, `water_sun_glitter`,
  `water_shore_foam_width`, `water_scatter_color`). Still water: waves and
  normals small, roughness low.
- Mood is mostly fog: `fog_visibility` in metres, `fog_color`,
  `fog_sun_color` (the glow toward the sun) and `fog_sun_exponent`; a
  saturated fog colour turns the whole sky one flat tint. `sun_position` is
  where the light comes from (its angle above the horizon is its height).

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
- `max_particles` is capped at 512. Small particles (under ~15 cm) do not
  show in a still `look` at any range: they are for people moving through.

## Dressing: one-off pieces

- An absolute placement snaps to the ground (`snap_to_terrain` is true by
  default, and `translation` y is then a lift above it); set it false for a
  world height - an emitter floating over a pool.
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
/placements/-` APPENDS: run it once, save, pull the source again, then list
those placements by their new index - or every re-run adds them again.
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
  read as a plume, a single twisting stack read as pancakes.
- **One colour per place**, so they are told apart through the mist: the
  Spore Spires green (west), the Ghost Grove pale mint (north), the
  Puffball Meadow's plume gold (east), the Great Ring's gills amber (south).
- **The outer land needs its forest, not only landmarks.** Stands of the
  world's own trees on the ridges the main area sees (a few hundred trees,
  the lighter conifers and birches first: scatter trees are never culled by
  distance) plus a thin fill out to the edges. A glade in a stand: move the
  stand's `bounds` off it (`room set /placements/N/bounds`).
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
