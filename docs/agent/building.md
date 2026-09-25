# Building

The agent edits its own world only. Edits are live for everyone at once and
kept only once saved ([saving.md](saving.md)).

## Two ways to build

- **A catalogue entry:** `A place <slug> --at X Z --yaw D`. Search with
  `A catalogue <words>` - it matches names AND descriptions, so search by
  material and mood (`corrugated`, `rust`, `scrap`, `salvage`, `plank`,
  `marble`) as well as by the thing (`garage`). The listing has no sizes.
- **A building of your own:** write a generator as JSON with
  `A room set /generators/<name> '<json>'`, then `A place <name> --at X Z
  --yaw D`. `place` takes a generator already in the record by its name.
  Every placement of one generator shares it: editing the generator changes
  every copy, and a catalogue entry placed twice is one generator.

An unplaced generator is kept but drawn nowhere: a safe place to try JSON.
`A undo` takes it out again.

## The JSON

The record's wire form, as the World Editor's Raw JSON tab shows it:

- Every decimal is a whole number of ten-thousandths (1.5 m is `15000`);
  rotations are quaternions `[x, y, z, w]` scaled the same way. A value with
  a decimal point is refused.
- A primitive:
  `{"$type": "network.symbios.gen.cuboid", "size": [x, y, z], "solid": true,
  "material": {...}, "transform": {"translation": [...], "rotation": [...]},
  "children": [...]}`. `solid` is required; it means "has a collider" -
  everything is drawn either way.
- Kinds used so far: `cuboid` (`size`), `cylinder` (`radius`, `height`,
  `resolution` 3..128, axis along +Y), `sphere` (`radius`, `resolution` =
  icosphere subdivisions 0..6, about 20 x 4^n triangles), `torus`
  (`major_radius`, `minor_radius`; lies flat, axis +Y - a quarter turn about
  X stands it on its tread).
- `torture` reshapes a primitive: `hollow` (the bore as a fraction of the
  radius: a sphere becomes a shell), `profile_cut` `[begin, end]` (a sphere's
  latitude band: `[5000, 10000]` a dome, `[7500, 10000]` a shallow cap - a
  dish), `path_cut`, `taper`, `bend`, `twist`. Read `TortureParams` in
  `src/pds/generator.rs` for the rest.
- **Children are placed relative to their parent's centre**, turned by the
  parent's rotation; the parent's `size` does not scale them, but its
  `transform.scale` does - a flattened root flattened a whole log into its
  soil bed. Work in absolute positions in a script and subtract the root's
  position last; a root with no transform (a tiny part hidden inside
  another) keeps every child's offset a plain world-frame one.
- **The generator's origin stands on the ground** at the placement's point:
  local y = 0 is the ground there. On dunes or a slope, sink walls about a
  metre below 0 so no gap shows under them.

Write buildings as a script that prints the JSON (every number derived from
a few named dimensions) and keep it: the same script rebuilds after a
restart and makes a fix one edit rather than twenty.

## Frames and yaw

`--yaw` is degrees clockwise seen from above: 0 turns a thing's front (its
local -Z) to world -Z, 90 to +X. Design a building with its door on local
-Z and give the yaw the door should face. With `t = -yaw` in radians, a
point `(x, z)` in the building's frame lands at

```text
world_x = at_x + x * cos(t) + z * sin(t)
world_z = at_z - x * sin(t) + z * cos(t)
```

Keep that as a helper and place everything else - dressing, fences - in the
building's frame through it.

## Materials

- **Borrow them.** Place a catalogue entry with the look you want (or find
  one in the record) and `A room get /generators/<slug>`: its `material`
  objects carry textures and weathering you would not guess. `undo` the
  place if you only wanted the material. The post-apocalyptic `scrap_wall`
  alone holds two corrugated rusts, rusted steel plate, grey plank and tyre
  rubber.
- A field left out takes its default (a roughness of 0.5 is the default and
  is dropped from what the world keeps).
- Colour is the admin's call. Keep one material per visible surface; two
  shades side by side read as a patch, and the lighter corrugated rust read
  as "too bright and yellow" next to the darker one.

## Sizes, footprints, clearances

- `catalogue` gives no sizes, but the render tool does (#1448):
  `render --catalogue <slug>` or `render --generator piece.json` prints
  `subject size X x Y x Z m (x, y, z), from [..] to [..]` - the box its
  meshes fill, from its origin - before it renders. Use it for every piece
  before arranging it, then compute its box in your building's frame. A small script that says "which
  of my walls does this cut" pays for itself: in the live garage the yard
  junk cut 13 cm through a side wall, invisible from the front.
- Building around a person: an interior of 5.2 x 7 m held a 2 x 3 m buggy
  with room to drive out. `status.peers[].facing` says which way they face -
  do not guess it from a picture (the live guess was wrong).
- Step out of a footprint before building on it: a body inside new walls is
  stuck, and a new collider can shove it.

## Z-fighting: two faces in one place

Two faces in one plane, facing one way, flicker as anyone moves. A still
`look` barely shows it; the admin sees it at once.

- `room set` answers `z_fighting`: each pair of primitives drawing faces in
  one place, by pointer, with the visible area, largest first. It leaves out
  faces that face each other (pressed between two solids), patches buried
  inside a third primitive, and patches below the generator's origin. After
  writing a generator, an empty list is the all-clear; fix every entry
  before calling a build done.
- Joints that avoid it: panels beside a door stop at the door's height and
  the header sits ON them; wall tops run 3 cm INTO an 8 cm roof; a wall's end
  stops 2 cm inside the wall it butts against, never flush with its outer
  face; two bars crossing at one height get different heights, or their
  crossing is buried in a post; door jambs run up into the header.
- The catalogue has its own (#1440 - 187 of 395 entries): not yours to fix
  unless asked, but never copy a joint from one without checking it.

## The sanitiser

Every write is sanitised as a saved record would be. `adjusted: true` comes
with `adjusted_at`, the pointer of each field that was changed (a sphere
asked for resolution 24 was kept at 6). Read `adjusted_at`; there is no need
to diff `kept`.

## Fixing a catalogue item where it stands

A placed catalogue entry is a generator in the record like any other:
`A room set /generators/<name>/children/N/transform '<json>'` edits this
world's copy only (the catalogue itself is code, and every placement of the
entry in this world changes). List a generator's children - type, size,
translation - to find the part. Defects seen live and fixed that way (#1439):
`scrap_wall`'s tyre lay half through the wall; `radio_mast`'s antenna floated
above its open lattice, and its dish was pierced by the legs with the feed
horn floating beside it. Look at props from close up and from below before
calling them done.

## A build, start to finish

1. Look at what the build should suit - the admin's avatar, the place
   ([looking.md](looking.md)) - and choose a style; say it in one line.
2. Step out of the footprint.
3. Script the generator; `room set` it; read `z_fighting` and `adjusted_at`.
4. `place` it; look from the front, a side and close up; crop and enlarge.
5. Dress it from the catalogue, placed in the building's frame; check every
   piece's box against the walls.
6. Report what is there and that it is unsaved; save only when asked.
