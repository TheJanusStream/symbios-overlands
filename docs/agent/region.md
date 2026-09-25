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

- **Numbers in milliseconds.** The ground is `crates/gen-jobs`'
  deterministic heightmap job: a small scratch program linking that crate
  (`GenJob::Heightmap(params).run()`) gives every height of the map in about
  0.3 s - percentiles to set the water by, the landing's own height, the
  share that floods, and a plan view to judge seeds by. World (x, z) is
  cell `((x + half) / cell, (z + half) / cell)`, `half = (grid - 1) x cell / 2`.
  Session 873 scanned 18 seeds as a contact sheet in 7 s and found a pool
  beside the landing on the 11th.
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
matches, the third layer is drawn.

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

## Ambient sound

`/environment/ambient_audio` is a sequence: `instruments` (each an `id` and a
`patch` whose `graph` holds `nodes` and `output`) and `tracks` of `events`
(`instrument_id`, `time_beats`, `gate_beats`, `pitch_multiplier`,
`volume`); at the seeded 60 bpm a beat is a second, and the loop is
`duration_beats` long. A malformed patch is refused without saying where -
check each instrument against a seeded one.

## Working fast

Keep every step a script and chain them in one command that regenerates
each intermediate before it applies (a stale middle file cost a round trip
here). Save at each milestone the admin could want kept, and say what is
next.

## Arrivals

`default_landing` is `{pos: [x, z], yaw_deg}`. Its yaw turns
**counter-clockwise** seen from above - `place --yaw` turns clockwise - so
to face a point `(dx, dz)` away, `yaw_deg = atan2(-dx, -dz)` in degrees.
A body already standing when the ground changes stays where it was: after
reshaping, `walk-to` the landing again.
