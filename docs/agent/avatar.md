# Your own body

The avatar is the one record the agent may edit in any world, and everyone
sees its edits at once. Two kinds of body: a **rigged** person (a sculpt and
worn items, kept in records of their own) and a **generator** body - every
vehicle: a car, a hover-boat, an airship, an airplane - whose whole shape is
one generator tree at `/record/body/visuals`, the same JSON as a building's
([building.md](building.md)).

## Choosing how it moves

`/record/locomotion` decides. For building and inspecting, a rotorcraft (the
`helicopter` preset every airship flies) is the best body found so far: a
`walk-to` climbs over whatever is in the way instead of ending `stuck`, and
it lands on its point - a roof or a hill to look down at a build from.
Keep what the seed gave unless there is a reason: the numbers are tuned
together, and `hover_thrust` must stay `mass x 9.81` or the craft sinks or
climbs at idle.

## Shaping a generator body

- **The collider is a box**, `locomotion.chassis_half_extents`, centred on
  the tree's origin. Nothing in the tree collides. Put the lowest point of
  the shape at `-half_y`, or the body floats above the ground it lands on
  or sinks into it; a thin part that trails (a tail, a thread) stays above
  `-half_y` too, or it cuts the ground when landed.
- **Front is local -Z**: what trails goes to +Z.
- **Make the root the node at the origin with no transform**, so every
  child's translation is a body-space position.
- **Caps** (the sanitiser's, for a body): any one dimension 16 m, the
  product of scales down any branch 4, 1024 nodes, 16 points on a `lathe`
  or `spine`.
- `lathe` stations run bottom to top **in list order**, so one profile can
  go out along an underside and back over a dome (a mushroom cap); a
  station of radius 0 closes it. `torus` needs `major_resolution` and
  `minor_resolution`. A refused JSON names where it failed: the generator
  node (`in the node at ...`, #1446), or any other place (`at ...`, #1457).
- **Colours are sRGB.** A glow reads when it is deep and saturated, with
  `base_color` and `emission_color` the same (the catalogue's mushroom glow
  is `[0.28, 0.86, 0.50]` at strength about 1.4); a pale colour driven
  emissive washes out to white.
- A `particles` node gives a body a trail: `inherit_velocity: 0` leaves what
  it emits behind as it moves (a spore drift in the body's wake).

## Seeing it before anyone else does

`look` puts the camera about 11 m behind the body - too far to judge detail.
Render the tree offline instead, from four sides, in about 20 seconds:

```bash
BEVY_ASSET_ROOT=<repo> <repo>/target/test-release/render \
    --generator visuals.json --out sheet.png
```

where `visuals.json` is the tree as `avatar set /record/body/visuals`
takes it. Then `A avatar set /record/body/visuals --file visuals.json` and
`A avatar set /record/locomotion/chassis_half_extents '[x, y, z]'` (two
steps of the avatar's undo), and `look` once to see it in the world.
Write the body as a script that prints the JSON from named dimensions, as a
building is.

## A workbench on the body

Away from its own world the agent can edit nothing but its avatar. So give
a generator body a flat surface to build on - a node whose children are the
builds - and a build made there is stashed and gifted like anything else:

1. `A avatar set /record/body/visuals/children/<bench>/children/- '<json>'`
   - the build's root sits at the bench's top surface (children are placed
   from their parent's centre, so that is half the bench's thickness up).
   An empty bench has no `children` in the record at all (an empty list is
   left out); an append makes it (#1458 - before that, the first build on
   an empty bench was refused with "nothing is at").
2. Look at it, fix it, `avatar set` again.
3. `A stash "<name>" --from-avatar /record/body/visuals/children/<bench>/children/<n>`:
   the item stands on the build's own origin, wherever it sat on the body.
4. `A gift give @admin "<name>" --wait`.
5. Take the build off the body again (`avatar set` of the bench's
   `children` without it), or it rides along everywhere.

Hypha's bench is `/record/body/visuals/children/0`, a disc of 0.55 m radius
sunk into a shallow depression at the centre of its cap and coloured like
the cap's dark centre (session 874: a proud, lighter 0.75 m disc read as a
lid); a build's root goes at `[0, 150, 0]` (1.5 cm, the disc's half
thickness) in the disc's frame. Keep the bench level when the cap is not:
a stashed build keeps the turn it had on the body.

## Making a body look grown, not turned

Session 874 rebuilt Hypha's cap after the admin asked for "more detail" and
"a more natural cap". What made the difference, and what went wrong first:

- **Break the symmetry a little.** A cap tilted 1 degree and scaled to a
  0.95 oval stops reading as a turned disc. Anything that must fit the cap
  (a depression the bench sits in) needs room for the oval and the tilt.
- **Detail at a size that reads.** From the game's camera, 11 m back, a
  2-4 cm scale is invisible; 10-20 cm fibrous scales (flattened spheres,
  long axis radial, lying along the surface, crowding toward the centre)
  read as a honey fungus. Sink a decoration by LESS than its own half
  height - sunk by more, only slivers showed.
- **One lathe per continuous surface.** A stem built as three colour bands
  at equal radii showed a shading seam at every joint (it looked stacked
  like cushions); a colour sleeve 1 cm proud read as a flower pot. One
  lathe with a fibrous texture looked like a stem.
- **Lathe stations run counter-clockwise in (radius, height)** - out along
  the underside, back over the top - or the faces point inward. A hanging
  skirt (a ring on a stem) is listed underside first.
- **A lathe's texture runs round it**: a `Bark` texture's furrows came out
  as horizontal rings until `uv_rotation` 90 (degrees; wire 900000) turned
  them lengthwise.
- **Gills deepest at the stem, rising into the flesh at the margin** - hung
  at one depth, 88 plates showed as a picket fence under the rim.
- **Inspection renders**: a turntable frames the whole tree, so trailing
  threads made the mushroom tiny; render a copy without them. A negative
  `--elev` needs the `=` (`--elev=-12`, looking up at the gills).
- **Re-run safety**: a body script that keeps parts from the saved body
  must find them by something they alone have (the threads' glow), not by
  kind - the new young caps' stalks were spines too.
