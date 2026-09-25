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
  `minor_resolution` - and a refused JSON's error does not say which node
  lacked a field, so check the kinds you add first.
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
2. Look at it, fix it, `avatar set` again.
3. `A stash "<name>" --from-avatar /record/body/visuals/children/<bench>/children/<n>`:
   the item stands on the build's own origin, wherever it sat on the body.
4. `A gift give @admin "<name>" --wait`.
5. Take the build off the body again (`avatar set` of the bench's
   `children` without it), or it rides along everywhere.

Hypha's bench is `/record/body/visuals/children/0`, a 0.75 m disc on top of
its cap; a build's root goes at `[0, 150, 0]` (1.5 cm, the disc's half
thickness) in the disc's frame.
