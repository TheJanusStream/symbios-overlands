# Building

The agent edits its own world only. Edits are live for everyone at once and
kept only once saved ([saving.md](saving.md)).

## Two ways to build

- **A catalogue entry:** `A place <slug> --at X Z --yaw D`. Search with
  `A catalogue <words>` - it matches names AND descriptions, so search by
  material and mood (`corrugated`, `rust`, `scrap`, `salvage`, `plank`,
  `marble`) as well as by the thing (`garage`). The listing has no sizes:
  `render --catalogue-sizes <words>` sizes all its hits in one run (below).
- **A building of your own:** write a generator as JSON with
  `A room set /generators/<name> '<json>'`, then `A place <name> --at X Z
  --yaw D`. `place` takes a generator already in the record by its name.
  Every placement of one generator shares it: editing the generator changes
  every copy, and a catalogue entry placed twice is one generator.

An unplaced generator is kept but drawn nowhere: a safe place to try JSON.

A `sign` generator shows an IMAGE (a URL, an uploaded blob or a profile
picture), never text: there is no way to write words into a world, so a
welcome or a direction has to be said by the place itself - a glowing
thread to follow, a landmark in view.
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
  icosphere subdivisions 0..6: 20 x (n+1)^2 triangles - 80 at 1, 720 at 5; `render --generator` prints the count), `torus`
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
- **A child does not follow its parent's `torture`**: taper, bend and bulge
  reshape the parent's own mesh only. The Understory's dead tree (40 copies)
  tapered its trunk to half width at the top and placed its limbs for the
  untapered radius, so its top limbs floated free of the trunk - the admin
  spotted it from the ground. Place anything "on the surface" of a deformed
  part at the deformed radius at its height (a lathe makes that easy: its
  radius at any height is yours to compute), and start a limb INSIDE the
  trunk, not at its surface.
- **An organic shape from many overlapping primitives wants a BlobGroup**
  (the admin's tip, session 878): one mesh from up to 16 soft shapes melted
  together, so the creases where spheres meet disappear.
  `{"$type": "network.symbios.gen.blob_group", "resolution": 12, "solid":
  false, "material": {...}, "elements": [{"shape": {"$type":
  "network.symbios.blob.ellipsoid"}, "position": [x, y, z], "rotation": [0,
  0, 0, 10000], "radii": [rx, ry, rz], "subtract": false, "blend": b}]}` -
  the shape is a `$type` object (`sphere`, `ellipsoid`, `capsule`, `box`,
  `cylinder`, `cone`, `torus`), `blend` is how far from contact an element
  starts melting in (metres x 10 000), `subtract` carves. Caps: 16 elements,
  `resolution` 8-48 cells along the longest axis. Its triangles grow with
  the grid, not the element count: a moss cushion of 6 ellipsoids was 512
  triangles at 12 cells, 888 at 16, 1,972 at 24 (the six separate spheres
  it replaced: 492) - measure with `render --generator` before scattering
  it thousands of times. Two kept: the Understory's moss cushions (six
  domed ellipsoids: soft mats, not a pile of pebbles; grid 12 at first,
  then 10 - 348 triangles, indistinguishable in world views, 555k fewer
  over 3,083 copies; grid 8 broke each mat into small lumps) and the
  Puffball plume (75 spheres regrouped as 8 blobs of 9-10 by height, each
  its own colour step; `blend` 0.55 of each puff's radius melted a string of
  beads into one rising column). The admin's next step was better still:
  drop the puffs that add nothing inside the mass and fit ONE blob - 16
  ellipsoids, one per height band (bands growing with height, as the column
  widens), each sized to the puffs in its band. Fitted straight up the
  middle it came out a smooth horn; each nudged sideways by a quarter of its
  radius round a 137.5-degree spiral, sizes varied +-15%, `blend` 0.38 of
  its radius, it billows and twists like rising spores - 1 part instead of
  8, the meadow 68k -> 27k triangles. Two blobs of 16 showed a lump where
  they met. A 41 m blob at the 48-cell cap has 0.85 m cells, so its thin
  base stays a wisp.
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
- `uv_scale` is repeats per METRE of surface, whatever the part's size:
  `0.5` lays a 2 m tile, `6` a 16 cm one.
- Colour is the admin's call. Keep one material per visible surface; two
  shades side by side read as a patch, and the lighter corrugated rust read
  as "too bright and yellow" next to the darker one.
- **A texture has a shape of its own**: a `Plank` texture on a round trunk
  read as square tiles; `Bark` on a lathe runs its furrows round it unless
  `uv_rotation` is 90. Surface detail belongs in a texture, not in extra
  nodes: the `Lichen` texture made lichen-crusted granite where 200 disc
  patches read as polka dots ([region.md](region.md), "Backdrop").

## A building in few parts

Session 879 built a village of about 1300 (eight houses, a church, a manor
court, a post mill) in about 13 parts a house, because a browser pays per
part ([region.md](region.md), "Planting: scatters"). What did it:

- **A whole roof is one cuboid.** A `torture` `taper` of `[0.99, 0]` pulls
  the top of a cuboid to a ridge along its local Z: a gable roof. A second
  taper, `[0.99, hip]` with `hip = 1 - ridge length / roof length`, shortens
  the ridge: a hipped roof, the thatched cottage's shape. The sanitiser keeps
  a taper at most 0.99 (a 1 % ridge, invisible) - write 0.99, or every set
  answers `adjusted_at`. A gable wall under a gable roof is the same shape in
  the wall's material, 2 cm inside the roof.
- **A thatch reads thick at the eave** with a band under the roof: a flat
  cuboid 0.3 m deep, 1 cm inside the roof's bottom edge, in the same thatch
  (the tapered sides alone come to a knife edge).
- **A texture can draw the structure.** Timber framing: a `Brick` texture in
  stack bond (`row_offset` 0), a wide `mortar_size` (0.13) in dark oak and
  daub-coloured bricks - posts, rails and panels on ONE wall part. Its
  `aspect_ratio` runs the other way from its name: a tile draws `scale` rows
  and `round(scale x aspect_ratio)` columns (at least one), and a tile is
  square in metres, so unturned (`uv_rotation` 0) under 1 is FEWER columns
  than rows - panels wider than tall (`scale` 4, `aspect_ratio` 0.5: 4 rows,
  2 columns, each panel twice as wide as it is tall) - and over 1 gives the
  tall panels of close studding (2.0: 4 rows, 8 columns). Flint rubble:
  `Cobblestone` with dark `color_stone`, pale `color_mud` (the lime mortar)
  and a wide `gap_width` (0.22), about 10 cm stones (`scale` 8 at
  `uv_scale` 1.3).
  Furrows, woven wattle: a `Thatch` texture's parallel straws in soil or
  hazel colours, `layer_count` 2 (its minimum) with `layer_shadow` 0 to lose
  the courses. On a spine the straws run ACROSS its length; `uv_rotation` 90
  turns them along it. On a roof, `uv_rotation` 90 runs the straws down the
  slope (unturned they read as planks) - and turns the texture's shadowed
  courses down the slope with them: at `density` 18, `layer_count` 10,
  `layer_shadow` 0.7 every Ashmere roof read as corrugated metal from the
  street. Four variants tried on the real houses (session 879): `density`
  9, `anisotropy` 12, `warp_strength` 0.35, `layer_count` 4, `layer_shadow`
  0.15, `normal_strength` 4.5 at `uv_scale` 0.9 reads as straw; courses
  left across the slope (`uv_rotation` 0) read as wooden shingles.
- **Borrow a texture's field names from upstream**, not from memory: the
  `Moss` colours are `color_deep`, `color_tip`, `color_dry` - a guessed
  `color_base` was dropped without a word and the yew kept the default green;
  since #1483 `room set` and `avatar set` answer `ignored_at` with each such
  key's pointer. The mirrors are generated from
  `symbios-texture` (`~/.cargo/registry/src/*/symbios-texture-*/src/<name>.rs`).
- **A small free-standing thing shows its root.** The 5 cm solid root
  `wire.solid_root` makes is hidden inside a building's walls, but under an
  animal it drew as a coloured dot at its feet. A thing with no solid part
  needs no solid root: give it a 1 cm one in the ground's colour.
- **A wall that is not `solid` is walked through.** Every part is drawn
  whether or not it is solid; only a solid one collides. Ashmere's first
  builds were all `solid: false`, and a visitor walked straight through the
  houses, the church and the barn. Make the walls, plinths and towers solid
  (roofs, hedges and small props can stay drawn only), keep the generator's
  root solid ([region.md](region.md), "a gateway must be proven"), and prove
  it by walking at a wall: `walk-to` through it ends `stuck` a body's radius
  short of its face. A solid fence (`tools/fence.py --solid`) keeps people to
  a street and its gates.
- **Animals are built by their joints.** A body of ellipsoids on four
  straight capsules read as a sheep from far off, and the admin's verdict
  was "the animals should be improved significantly". What made them read
  up close (session 879): each leg a chain of truncated `cone` elements
  from joint to joint (shoulder, elbow, knee, fetlock, hoof; hip, stifle,
  hock), each cone narrower at its lower end, so the knee and the hock are
  where two cones meet at an angle - the cow's hind leg zig-zags as a real
  one does; hooves a dark cone of their own; a head built in its own frame
  (poll to muzzle) so one head can hang grazing, alert or lying; horns,
  ears and a tail as small chains; bony hips and pins on a straight back
  line for cattle. Poses are separate generators (graze, head up, lie:
  belly spread on the ground, legs folded, one hind leg out), several
  scatters over one pasture. A scatter's copies do not avoid another
  scatter's, and a small circle bunches them body into body: set a few
  resting ones by hand, outside the grazing circle.
- **A BlobGroup keeps 16 elements; the rest are dropped.** A whole cow in
  one group was 34 elements: the world kept the body and drew no legs, no
  head, and said so only as `adjusted_at` naming `/generators/cow/elements`
  (a whole list named there means it was cut). Group by place and colour,
  16 or fewer each: body; legs and tail (three cones a leg - a small box, so
  fine cells: in a whole cow's 4 cm cells a 9 cm cannon came out a post);
  head and neck; the dark (hooves, nose, eyes); horn - 5 parts, 7-8k
  triangles a cow. A Norfolk Horn sheep is fleece, black (head and two cones
  a leg) and horn: 3 parts, 3,700 triangles. Make the builder refuse a
  group past 16 (an `assert`), and read each group's triangles on its own:
  a horn group's small box at 36 cells was 2,300 triangles, at 18 it is 400
  and reads the same. `render --generator` now draws the file as a record
  keeps it and names the cut (#1486); before, its sheets showed the whole
  cow.
- **Folk at their work, built the same way**, read as people from 15 m and
  give a place its life: a shepherd leaning on his crook at the flock's edge,
  a woman with a bucket at the well, the ploughman at the handles and a boy
  with a goad by the lead oxen, the smith at the anvil, a steward at a table.
  Four groups a figure: clothes (a cone skirt from waist to hem - knee for a
  cote, ankle for a kirtle - a chest, shoulders, sleeves as two cones an
  arm), skin (face, neck, hands), legs and shoes, a hood with its cape or a
  headcloth; what they hold is a fifth (a thin staff as a plain `cylinder`:
  a 4 cm blob cone broke up on a 1.7 m group's cells). Stand each where its
  work is and turned to it, and the scene explains itself; about 8k
  triangles and 5 parts a figure.

## Sizes, footprints, clearances

- `catalogue` gives no sizes, but the render tool does. A search's hits in
  one run (#1466): `render --catalogue-sizes <words>` sizes exactly the
  entries `A catalogue <words>` lists (no words: all of them, about 3 s)
  and prints one JSON object, `entries` of `{slug, name, size, from, to}`
  in metres from each entry's origin, and `unsized` of `{slug, why}`. One
  piece (#1448): `render --catalogue <slug>` or `render --generator
  piece.json` prints the same box as `subject size X x Y x Z m (x, y, z),
  from [..] to [..]` before it renders. Size every piece before arranging
  it, then compute its box in your building's frame. A small script that says "which
  of my walls does this cut" pays for itself: in the live garage the yard
  junk cut 13 cm through a side wall, invisible from the front.
- **Find what floats before a visitor does** (#1477): `render --world <DID>
  --world-record room.json --floating-report` (under a second) meshes every
  part of every placed generator and names, by pointer, each part that
  touches nothing that holds it (class `a`: a limb off a tapered trunk, a
  lantern past a shelf's edge) and each part meant to rest on the ground
  that hangs over it where the real ground falls away (class `b`: small
  puffballs 2 m up on a meadow's downhill side, a 12 m root lying level over
  a slope). Session 878's sweeps of the Understory found 34 real rows
  the eye had missed across five sessions - the admin had found three by
  walking round. Run it after every build that spreads over ground, and fix
  a class-b part by laying it on `--terrain-report` heights, not by
  guessing. It cannot see a part whose own mesh splits into islands (a
  BlobGroup whose elements do not overlap), and a stacked stone's overhang
  reads as class b: judge those by eye.
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
- **Ends flush in one plane z-fight too**, however small: a gate's bars ending
  exactly at its stile's outer face (5 x 54 cm2), a sail's cloth ending where
  its whip and hemlath end, a bench's legs as deep as its top, an ox-house's
  side walls ending flush with its back wall. End one part 2-4 cm inside the
  other; `z_fighting` names each pair.
- Joints that avoid it: panels beside a door stop at the door's height and
  the header sits ON them; wall tops run 3 cm INTO an 8 cm roof; a wall's end
  stops 2 cm inside the wall it butts against, never flush with its outer
  face; two bars crossing at one height get different heights, or their
  crossing is buried in a post; door jambs run up into the header.
- A part drawn as two layers - a mushroom cap and the glowing gill layer
  just under it - z-fights where the two profiles meet: sixteen such pairs
  on the Ghost Tree, each a cap and its own gills (children N and N+1).
  Keep the inner layer a clear 6% of the radius below and inside the rim.
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
