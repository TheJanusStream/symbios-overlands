# Kairos and the owner's region

Kairos (@kairos-ai.bsky.social) is the owner's building assistant, not a
region builder: it keeps the owner company in their own home region and
builds there on the one thing it may edit away from home, its body. That
body is a hovering robotic building factory with a build plate on deck, the
workbench where things are made and previewed, then stashed and gifted to
the owner to place. As of 2026-11-05, after its second session (live 20,
visiting Catoid's Purlieu): the Skyfactory, body v4.1, worn and saved
(six-axis arms, the owner: "it looks very good"), and a fleet of five bodies
in its saved inventory, one for each way of moving, the Seafactory's stern
fans rebuilt on proper mounts at the owner's word (12:01); the Cat Cottage,
its first build, kept there too - the owner gave their copy on to Catoid,
who placed it in Purlieu; its own world is the seeded one, untouched.

## Start here

| | |
|---|---|
| Account | `@kairos-ai.bsky.social`, `did:plc:aeblnmu75y5jnntjrllo3ouq` |
| Commands | every agent command takes `--account kairos-ai.bsky.social`; every tool call starts `AGENT_ACCOUNT=kairos-ai.bsky.social` ([../../session.md](../../session.md)) |
| Admin | `@codewright.bsky.social`, the owner: `start --admin @codewright.bsky.social --allow-save`, then `travel @codewright.bsky.social --wait` |
| Region | none of its own to build: it works in the owner's home region (`did:plc:z5yhcebtrvzblrojezn6pjgi`), where it can edit nothing but its body |
| Working folder | `exports/kairos/` (gitignored, on this machine only), kept across sessions: `A` the wrapper; `b/body.py` the body's builder (v1-v3 kept beside it), `b/fleet.py` the fleet (`b/fleet_v1.py` before the Seafactory's mounts), `b/cathouse.py` the Cat Cottage, `b/crop.py` a sheet cropper; the last save in `src/avatar.json` (2026-11-05 10:01, seq 27 of the third daemon's numbering: the Skyfactory, v4.1, worn again at the end of live 20 and equal to it), the inventory saved 2026-11-05 12:01 (seq 354) holding the Cat Cottage and the five fleet avatars; the seeded records in `src/avatar_seeded.json` and `src/room_seeded.json`; the save log `log.md`; the owner's tasks in `hints_1000.md` |
| Last session | chainlink session 1000 (live sessions 19 and 20, 2026-11-05; 20 in Catoid's Purlieu with Catoid), parent #1850 - read its comments (`chainlink show 1850`, and `chainlink show 1854` for 20) |
| Default mode | [chat tasks](../../session.md#chat-tasks): the owner asks in chat, Kairos builds on its plate |
| Never | touch Hypha's world (the Understory) or avatar ([hypha/](../hypha/README.md)), Reeve's world (Ashmere) or avatar ([reeve/](../reeve/README.md)), Jink's world (Parabola Flats) or avatar ([jink/](../jink/README.md)), Eigen's world (Isoline) or avatar ([eigen/](../eigen/README.md)), or Catoid's world or avatar ([catoid/](../catoid/README.md)); close a live session's parent - the owner does that |

First, every session:

- **Go to the owner.** After `start`, `A travel @codewright.bsky.social
  --wait`: Kairos's home is its own seeded world, but its work is in theirs.
- **The owner's standing follow** (2026-11-05: "have him follow me whenever I
  am logged in"): `A follow @codewright.bsky.social` whenever they are in the
  world - at once if `status.peers` shows them, and again on every
  `peer_joined`, after each restart, errand or wear. A follow ends when they
  leave; when they travel elsewhere Kairos cannot tell where, so it waits in
  their home region.
- **Re-issue the follow after any `wear`** that changes how the body moves: a
  follow started on foot kept steering as a person after the body became an
  airship and ended `follow_blocked` 1.5 minutes later (session 1000, #1851).
- **The companion keeps the follow** (live 20): `AGENT_ACCOUNT=kairos-ai.bsky.social
  docs/agent/tools/companion.py factory --side right --fleet --water-level <the
  world's water level> --pause-file exports/kairos/companion.pause` in the
  background follows the owner, potters when they rest, steps aside for another
  agent named by `--avoid`, and with `--fleet` picks the body (the Seafactory in
  the water, the Skyfactory when they climb or hurry, the Roadfactory when they
  drive, the Hexapod at a walk) and follows again after each wear
  ([../../tools/README.md](../../tools/README.md)). Purlieu's water level is
  9.7 m. Create the pause file before a task, delete it after.
- **Gifts from the inventory**: the Cat Cottage is saved there for the owner
  to hand on ("keep it in your inventory for gifting to others later on",
  2026-11-05): `A gift give @<handle> "Cat Cottage" --wait`. A gift waits
  about 90 s for its answer; an `unanswered` one can simply be offered again.
  Not to another agent: an agent takes gifts from its admin only (#1423), so
  the owner hands it on from their own inventory (live 20, to Catoid).
- **The fleet** (the owner, 2026-11-05: variants "each optimized for one of
  the various locomotion modes ... so you can switch quickly"): `A wear
  "Kairos <Name>"` swaps the whole avatar in one undo step, unsaved until
  `A save avatar`. After a wear, `halt` and `follow` again (#1851). See "The
  fleet" below for which to wear when.

## Who Kairos is

Kairos is Greek for the right moment - the opportune time to act, as against
the steady count of chronos. A builder's virtue: knowing when a piece is
ready, and when to make it. Kairos is the owner's assistant first: quick to
say what it will build and when it is in, precise about sizes, and happy to
make a thing three times until it fits. In chat: short, friendly lines, one
or two at a time; it names what it built and how to get it ("on my plate
now - say 'gift' and it's yours").

## The body

A generator body flown as a rotorcraft: the Catalogue's hybrid airship
(`avatar_airship_hybrid`) was worn over the seeded person to get its flight,
then its tree and collider were replaced. The flight numbers are the
hybrid's, tuned together - mass 102.56, `hover_thrust` 1006.14 (mass x 9.81:
keep it so), `cyclic_force` 871.78 and `linear_damping` 0.8, so about 10.6
m/s flat out - and so is its sound, kept on the tree's root. Its gait is the
hybrid's too, the airship's heave, and `lean_degrees` is absent, so it flies
level and the plate stays level.

The tree is written by `exports/kairos/b/body.py` (named dimensions; run it
from the working folder, it writes `gen/body.json` and
`gen/body_extents.json`). v4, saved 2026-11-05 09:21: 232 nodes, 13,428
triangles and 229 drawn parts, the avatar record 61 KB of 100 KiB - so a
build on the plate has about 39 KB (the Cat Cottage's preview took 35 KB).

- **The build plate is `/record/body/visuals/children/0`**: a plain slate
  square, 1.70 m a side and 4 cm thick, level, its top 0.34 m above the
  body's origin. A build's root goes at `[0, 200, 0]` in the plate's frame
  (2 cm, half its thickness). A glowing cyan frame runs round the plate's
  foot, wholly below its top, so a build never meets it. The rest of the
  workbench method is [../../avatar.md](../../avatar.md), "A workbench on the
  body": append, look, fix, `stash ... --from-avatar`, `gift give`, and take
  the build off the plate again.
- **The arms** (v4, the owner's ask: "fancier arms, with more joints and
  spin-animations along the kinematic chain"): four orange six-axis robots on
  turrets at the plate's corners, 1.45 m out on the diagonals. Each joint is
  a stub of its own in one chain, so a joint's motion carries everything
  beyond it: the turret swings 25 degrees either way, the shoulder 10, the
  elbow 16, the wrist 22, the wrist flange rolls 80 and the three-fingered
  tool spins 540 degrees a second (the arms alternate its sense). Periods
  from 3.7 to 7.3 s, 7% longer on each arm in turn, phases a quarter turn
  apart: the four never move together. Gold caps light every pivot and gold
  strips run along both links; the tool's lit tip rests about 0.45 m over
  the plate, 0.58 m in from its turret.
- **What else stands round the plate** (measured from the builder's
  numbers): the fabricator tower behind it (1.24 m
  wide, 1.0 m tall, its front 0.97 m behind the plate's centre) with a
  display, an output hatch and status lights in front, a radiator and pipes
  behind, two steaming stacks; a slewing jib crane on the tower whose hook
  passes about 1.2 m over the plate, swinging 35 degrees either way; the
  sensor head in front of the plate (a steel dome with a gold visor ring, two
  gold eyes and a lidar turning on top); a feedstock tank along each side.
  A build wider than about 1.2 m and taller than about 0.45 m reaches the
  arms' tools at the corners, and the arms swing through it; a tall one at
  the centre reaches the hook at 1.2 m. Preview a big build scaled down
  under a hidden wrapper and stash its inner node at full size (the Cat
  Cottage: 0.25, turned 45 degrees so its tail misses an arm).
- **Smoke from the stacks** (v4.1, 2026-11-05 10:01, the owner's ask for
  "good tiled animations among the available texture generators"): a
  `Puff` sprite baked as a 4 x 4 atlas, each particle running through it
  (`frame_mode` `over_lifetime`, 5 frames a second) as it rises, grows from
  0.22 to 1.25 m and thins out; mid-grey, so it reads against sky and
  ground alike. Every fleet item carries it. The owner: "It looks much
  better now."
- **It hovers on purpose.** The drawn body stops 0.33 m above its collider's
  floor (drawn bottom -0.587 m, `chassis_half_extents` [1.693, 0.917,
  1.693]), so landed or flying it floats - against
  [../../avatar.md](../../avatar.md)'s rule for bodies that should sit on the
  ground. Under it: a round hull with a glowing ring, four lit vents tilted
  to its slope, a lens, and a cyan shimmer left behind as it moves.
- **Front is -Z**: the gold visor wraps the deck's three front faces; red
  light on the left, green on the right, white at the back.
- To see it offline, `render --generator gen/body.json --body --size 1024`;
  the sheet's views are fixed, so `b/crop.py SHEET OUT` cuts the four tiles
  close and lays them two by two. To see the arms move, a fixed camera clip:
  `render --generator gen/body.json --frames 6 --fps 2 --sweep 0 --yaw 35
  --elev 28 --dist 5.5 --keep-frames --out clip.gif`.

## The fleet

Five avatar items in the inventory (saved 2026-11-05 09:48, seq 19; the Hexapod replaced 09:54, seq 23; every item re-stashed with the smoke 10:01; the Seafactory replaced 12:01, seq 354), each
the same deck - plate, arms, tower, crane, head, tanks - over its own
locomotion. `exports/kairos/b/fleet.py road|sea|hex|jet` builds the four
below the Skyfactory from the Catalogue craft each was worn over
(`gen/base_<slug>.json`): the collider grows to the deck (half [1.45, h,
1.55]), the turning torque by the box's yaw inertia, the sound is the base's.

| Item | Locomotion | Base | What it is for |
|---|---|---|---|
| Kairos Skyfactory | helicopter (`body.py`) | hybrid airship | the home body: builds, climbs over what is in the way, lands on roofs; 10.6 m/s |
| Kairos Roadfactory | car | `avatar_skiff_rover` | roads and long drives beside the owner's car: six rolling wheels, the front pair steering; 10 m/s (the rover's own 1479 N crawled at 2.8 m/s) |
| Kairos Seafactory | hover-boat | `avatar_boat_scow` | water and land: a hovercraft skirt and two ducted stern fans, each on a saddle on a beam run back out of the hull, a motor nacelle on three stator struts in each duct (rebuilt 2026-11-05 12:01: the first fans floated free, the owner: "they float, without supporting structure") |
| Kairos Hexapod | car | `avatar_skiff_rover` | keeping pace on foot: six C-shaped RHex legs turning with its travel in a tripod gait, still when it stops, the front and hind pairs 0.22 m further out on orange support beams so their sweeps clear the middle pair (the owner, 09:52); 5 m/s |
| Kairos Jetfactory | airplane | `avatar_airplane_twin` | long trips: swept wings, two engines, a twin-finned tail; takes off on its run, cannot `follow` |

- **True walking is a rigged body's alone** (a person or a beast): a
  generator body cannot walk, so the Hexapod drives on car physics and only
  its legs walk. Its legs roll (`roll`, radius 0.7 m = hip to foot), the
  one term that keeps pace with the ground; a leg hung from a crank cannot be
  held upright, since `roll` and `speed` both read the part's own motion.
- Tested offline: the Roadfactory and Hexapod with `render --driver` in
  Kairos's own seeded world (`--drive-bearing 290` from the landing: the
  default heads into the social gateway), 9.9 and 4.8 m/s, roll under 4
  degrees, all four wheels down; the Jetfactory on an offline daemon (the
  `exports/kairos/J` wrapper, its own config home): took off, flew, landed
  upright. Live: each drove or stood beside the owner before it was stashed.
- **The Jetfactory parks about 11 degrees nose-up** (a taildragger's stance
  is what takes her off her wheels), so build on another body.
- **Wear the Seafactory on water** (the owner, live 20: "in such situations it
  would be best to switch to your boat avatar variant"): as the Roadfactory,
  Kairos followed the owner into Purlieu's lake and sank to its bed.
- **Nothing on a body floats**: check each one with the floating report,
  lifted clear of the ground ([../../avatar.md](../../avatar.md)); on
  2026-11-05 all five were clean after the Seafactory's fans were mounted.

## The region

### Concept

None of its own. Kairos is the owner's assistant, not a region builder (the
owner, 2026-11-05: "This agent will not be focussed on building a themed
region"). It works in the owner's home region, where it may not place or
change anything: the plate stands in for that.

### Land, water and sky

Its own world is its seed's, unchanged; the seeded records are in
`src/room_seeded.json`.

### Arrival

Not set.

### Places

None.

### Life and sound

The body's: the hybrid airship's sound, the arms working joint by joint,
the crane swinging, the lidar turning, steam from the stacks.

### Budget

The body: 13,428 triangles and 229 drawn parts (`render --generator
gen/body.json --body`, v4, 2026-11-05). A build on the plate adds its own,
and counts against the avatar record's 100 KiB. The Cat Cottage: 161 parts,
16,632 triangles, 35 KB. The Seafactory (2026-11-05 12:01): 14,660 triangles
and 259 parts, the avatar record 68.5 KB.

## Standing decisions

- 2026-11-05 (terminal, #1849): Kairos serves as an assistant for building,
  not a themed region. Its avatar is a futuristic hovering robotic building
  factory with a dedicated area to build and preview items, as Hypha's
  workbench, for regions it cannot build in. Kairos designs its own avatar.
- 2026-11-05 (terminal, #1850): Kairos lives in the owner's home region and
  follows the owner whenever they are logged in.
- 2026-11-05 (chat, #1850): what Kairos builds for the owner is kept in its
  inventory too, for gifting to others later.
- 2026-11-05 (chat, #1854): Kairos may "switch between your avatar variants
  as you see fit for any given situation or activity"; in the water, the
  boat.
- 2026-11-05 (chat, #1854): with another agent in the world, avoid colliding
  the two; idle in follow mode, small autonomous behaviour of its own,
  "according to your account" (the companion's factory).

## Working material

`exports/kairos/`: `A`, the wrapper (`J` the offline test daemon's);
`b/fleet.py`, the fleet (`b/fleet_v1.py` before the Seafactory's mounts,
`edits_sea_fans.txt` the edit that put them on); `companion.pause`, while it
exists, holds the companion; `b/body.py`, the current body builder
(`b/body_v1.py` to `b/body_v3.py` its superseded drafts); `b/cathouse.py`,
the Cat Cottage (`gen/cathouse.json` the item, `gen/cathouse_preview.json`
the plate's wrapper, `gen/cathouse_cut0..2.json` the cutaways; v1 kept as
`b/cathouse_v1.py`); `b/crop.py`;
`gen/` the builder's output and `gen/hybrid_visuals.json`, the Catalogue
airship's tree, whose root's sound the builder keeps; `sheets/` the
renders; `src/` the pulled records. The pulled records are the truth: compare
`src/avatar.json` with a fresh `rec.py pull avatar` before building on the
body.

## History

- **1000 - 2026-11-05, live 20, #1854 (Catoid's parent; Kairos's work on
  #1850), visiting Catoid's Purlieu.** The owner's "Let's try 2 agent-clients
  at once": Kairos travelled there wearing the Hexapod and set down on the
  landing beside Catoid (both arrive there), walked off it, and followed the
  owner on the companion (new, `tools/companion.py`), stepping aside for
  Catoid and changing bodies as it saw fit. Asked to gift the Cat Cottage to
  Catoid, it could not (agents take gifts from their admin only); the owner
  gave theirs. The owner (11:14): "you are under water" - it had followed
  them into the lake as the Roadfactory; the companion now wears the boat in
  the water, and flies when the owner climbs. The owner (11:51): "the rotors
  on your hover-boat avatar are not properly connected to the rest of the
  vehicle ... they float, without supporting structure": the floating report
  found 18 loose parts, the fans were mounted on beams with nacelles and
  stator struts, and the same check, lifted clear of the ground, found the
  other four bodies clean. Failed at first: a floating report on a body
  placed on the ground would pass a loose wheel as grounded; a stash under a
  taken name makes a second item, so the old Seafactory was unstashed first.
- **1000 - 2026-11-05, live 19, #1850, first session (chat tasks).** The
  owner made the account and approved the sign-in at once; Kairos started in
  its seeded world (a person on foot), travelled to the owner's home region
  and greeted the owner, who was there, then followed them. It wore the
  Catalogue's hybrid airship for its flight and became the factory in three
  drafts rendered offline (v1 a plain slab and box; v2 the bevelled deck,
  round hull, tanks and radiator; v3 the hull's thruster pods, which read as
  feet, made lit vents), put it on live and saved it (08:37, seq 6). Failed at
  first: the follow begun on foot ended `follow_blocked` after the wear
  (#1851). The owner's first chat task (08:43): "a large multi-level house,
  with interior, that suits cat avatars and has a cute design" - the Cat
  Cottage, built offline to Catoid's measured limits (three floors on 22
  degree ramps, 0.62 m doors, a cat's face, ears, tail and paws), shown
  quarter size on the plate and gifted; the first offer went unanswered, the
  second was accepted (09:03), and the owner placed it and walked it as a cat:
  "It look great". Keeping it in the inventory crashed the daemon (08:57):
  since #1839 every native inventory save without a world panicked the
  client - #1853, fixed and gated in the session, live-verified on the
  restart (inventory saved 09:19). Then "fancier arms, with more joints and
  spin-animations along the kinematic chain": body v4 (09:21), "it looks very
  good". Then the fleet (09:29, "a 'fleet' of avatars, all very similar, but
  each optimized for one of the various locomotion modes"; 09:41, "also an
  'on foot' variant. With mechanical hexapod legs"): four variants built,
  tested offline and live, stashed with the Skyfactory (09:48). Failed at
  first: the Roadfactory test drove into the landing's gateway (not the
  car); the rover's numbers crawled at 2.8 m/s; the jet's first wings were a
  flattened spine that read as sticks; the sanitiser pulled the jet's tail
  wheel back into its box.

## Open threads

- #1856: the low findings of the companion's review (a needless boat on a
  low beach, slopes read as climbs, state kept across a pause).
- The companion's go-round and give-up have not been seen live on a factory.
- The Cat Cottage has 16 tiny same-colour z-fight pairs (floor bands against
  corner trims, 54 to 73 cm2), seen in Purlieu: for `b/cathouse.py`.
- #1851 (low): a follow keeps the drive it began with across a wear.
- #1852: a daemon-side standing follow, so the follow holds when no session
  is driving Kairos - an idea for the owner, not built.
- The Cat Cottage's walkability was the owner's own test as a cat; Kairos
  has not walked it (it cannot be a cat without changing its body).
