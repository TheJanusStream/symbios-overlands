# Kairos and the owner's region

Kairos (@kairos-ai.bsky.social) is the owner's building assistant: it keeps
the owner company and builds for them. Its body is a hovering robotic
building factory with a build plate on deck, the workbench where things are
made and previewed, then stashed and gifted to the owner to place. As of
2026-11-18, after its fifth session (live 24, in its own world, the owner
visiting): its world is built out at the owner's word - an open-air workshop
with its own teleporter and sign, a giant arm three times life size and a
hang-out for persons and beasts on the arrival block, and on the park east of
it a spaceship yard, a 63 m frigate left half built in its cradle under three
tower cranes - lit since live 22 and moving since live 24 (crates riding the
line, hoists, trolleys, sliding hatch doors, the frigate's engines on test),
under a day that turns once an hour with occasional short storm showers - all
saved (the last 2026-11-18 12:35, seq 27). The Skyfactory, body v4.1, lit
since live 22 (navigation lights, a strobe, a plate lamp) and moving since
live 24 (a hoisting hook, sliding hatch doors, pulsing glows), is worn and
saved, with a fleet of five bodies and the Cat Cottage in its inventory. It
made a profile picture for each of the six agents, which the owner set on
Bluesky.

## Start here

| | |
|---|---|
| Account | `@kairos-ai.bsky.social`, `did:plc:aeblnmu75y5jnntjrllo3ouq` |
| Commands | every agent command takes `--account kairos-ai.bsky.social`; every tool call starts `AGENT_ACCOUNT=kairos-ai.bsky.social` ([../../session.md](../../session.md)) |
| Admin | `@codewright.bsky.social`, the owner: `start --admin @codewright.bsky.social --allow-save`, then `travel @codewright.bsky.social --wait` |
| Region | its own (`did:plc:aeblnmu75y5jnntjrllo3ouq`), built out at the owner's word in live 21: the workshop, giant arm and hang-out on the arrival block, the spaceship yard east of it (see "The region"); in the owner's home region (`did:plc:z5yhcebtrvzblrojezn6pjgi`) it can edit nothing but its body |
| Working folder | `exports/kairos/` (gitignored, on this machine only), kept across sessions: `A` the wrapper; `b/body.py` the body's builder (v1-v3 kept beside it), `b/fleet.py` the fleet, `b/cathouse.py` the Cat Cottage; the world's builders `b/workshop.py` (with `ws_common.py`, `teleporter.py`, `sign.py`), `b/giant_arm.py`, `b/hangout.py` and `b/yard_*.py` (`yard_common.py` and `yard_ship_parts.py` the reusable parts; `yard_site`, `yard_groundworks`, `yard_cranes`, `yard_dock`, `yard_busy`, `yard_ship` the six stages, `yard_look` the look pass; `yard_sweep`, `yard_crane_pairs`, `yard_weld_sweep` the clearance sweeps), each edit as applied in `b/*.edits`; `b/dockwatch.py` the dock rule; `b/lights.py` the light sources (live 22: `b/lights_workshop.edits`, `b/lights_yard.edits`, `b/lights_avatar.edits` applied; `b/lights_yard_masts.edits` prepared, not applied); `b/motion.py` the slide and swell pass (live 24: `b/motion_workshop.edits`, `b/motion_yard.edits`, `b/motion_avatar.edits` applied; `b/motion_sim.py` and `b/motion_sweep.py` its sampler and clearance sweeps; the critic's independent checkers in `1031/critic/`); `b/depthcheck.py` the nesting depth count, `b/zcheck.py` z-fighting. Last saves: the world `src/room.json`, pulled after its last save (2026-11-18 12:35, seq 27 of session 1031's daemon: the yard's motion; seq 24 the workshop's, seq 6 the hourly day, seq 8 the storm showers); the avatar `src/avatar.json` (2026-11-18 12:35, seq 28: the Skyfactory v4.1 with its lights and motion); the inventory 2026-11-05 12:01 (seq 354). The seeded records in `src/avatar_seeded.json` and `src/room_seeded.json`; the save log `log.md`. The profile pictures are in `exports/promo/pfp/` |
| Last session | chainlink session 1031 (live session 24, 2026-11-18 10:00 to 12:52, in its own world, the owner visiting 11:39 to 12:01), parent #1945 - read its comments (`chainlink show 1945`) |
| Default mode | [chat tasks](../../session.md#chat-tasks): the owner asks in chat, Kairos builds on its plate |
| Never | touch Hypha's world (the Understory) or avatar ([hypha/](../hypha/README.md)), Reeve's world (Ashmere) or avatar ([reeve/](../reeve/README.md)), Jink's world (Parabola Flats) or avatar ([jink/](../jink/README.md)), Eigen's world (Isoline) or avatar ([eigen/](../eigen/README.md)), Catoid's world or avatar ([catoid/](../catoid/README.md)), or Atrium's world (Strata) or avatar ([atrium/](../atrium/README.md)); close a live session's parent before the owner has committed its work |

First, every session:

- **Go to the owner.** After `start`, read `status.peers`: in live 21 the
  owner came to Kairos's own world, where the dock rule below holds; when they
  are elsewhere, `A travel @codewright.bsky.social --wait`.
- **On its own block, its dock** (live 21, 2026-11-06 15:21: "While we are here
  at your block, it would be best if you rest on the landing platform, instead
  of following me, only follow me when leaving the area of the block"): in its
  own world Kairos rests on its landing dock at (12.0, 21.5) while the owner is
  on the arrival block (x 4 to 47, z -9 to 41) and follows them only once they
  leave it. `AGENT_ACCOUNT=kairos-ai.bsky.social python3 exports/kairos/b/dockwatch.py`
  in the background keeps the rule (a pause file `exports/kairos/dockwatch.pause`
  holds it).
- **The owner's standing follow** (2026-11-05: "have him follow me whenever I
  am logged in"): `A follow @codewright.bsky.social --standing` once after
  each `start` (#1852) - the daemon starts the follow again on every return
  of theirs and after each errand, until `halt`; a restart forgets it. When
  they travel elsewhere Kairos cannot tell where, so it waits in their home
  region. Not in its own world while `dockwatch.py` keeps the dock rule: a
  standing follow starts again as soon as the walk to the dock arrives, so
  there the plain `follow` the script gives is the one to use.
- **A `wear` keeps the follow going** under the new body (#1851): a follow
  started on foot used to keep steering as a person after the body became an
  airship, and ended `follow_blocked` 1.5 minutes later (session 1000); it
  now escorts at once. Only the airplane cannot follow (`body_changed`).
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
  `A save avatar`. A follow under way carries on under the new body (#1851). See "The
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
- **Moving parts** (live 24, 2026-11-18, `b/motion.py` over the saved
  body): the crane's hook hoists 0.2 m UP from where it hung (4.6 s), its
  cable sliding and swelling so it stays joined, its lowest point still
  1.18 m over the plate; two dark doors slide over the fabricator's output
  hatch (5.5 s, resting half open); the status lights pulse in turn
  (1.8 s), the hull lens breathes (3.6 s) and the four lit vents pulse in
  sequence (2.4 s). The fleet's bodies carry none of it yet.
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

- **True walking is a rigged body's alone** (a person, a beast or a winged
  body): a generator body cannot walk, so the Hexapod drives on car physics and only
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
- **The Stand-in bird** (2026-11-05, #1861), not one of the fleet: the
  flight prototype's generator bird made of primitives
  (`tests/fixtures/winged/standin_bird_avatar.json`), which Kairos wore,
  stashed as 'Stand-in bird' (inventory seq 4) and gifted whole to the owner,
  who wore it and flew it in their home region ("All looks good to me"). A rigged
  winged body exists beside it since #1875-#1880: an owl, a bat, a glider or
  a cross, posed by its flight mode, chosen in the Avatar window's Body tab
  (`winged`) - see [../../avatar.md](../../avatar.md#a-winged-body). The
  generator bird still flies as it did.
- **Nothing on a body floats**: check each one with the floating report,
  lifted clear of the ground ([../../avatar.md](../../avatar.md)); on
  2026-11-05 all five were clean after the Seafactory's fans were mounted.

## The region

### Concept

No theme of its own (the owner, 2026-11-05: "This agent will not be focussed
on building a themed region"), but in live 21 the owner, visiting, had Kairos
build its world out as the factory's home, one chat task at a time: a working
place in the body's own style - graphite, orange arms, cyan light lines. In
the owner's home region it may not place or change anything: the plate
stands in for that.

### Land, water and sky

The seed's land and water; the seeded records are in `src/room_seeded.json`.
The sky since live 24 (2026-11-18, the owner's words in chat): the day turns
once an hour (`/environment/time_of_day` `{"hour": 239621, "day_minutes":
600000}`, the hour chosen so it read 19:30 at the change), with occasional
short storm showers (`/environment/weather` storm at 6000 in 3-minute spells,
`spell_minutes` 30000, `share` 1500: about an eighth of the time, one every
20-25 minutes), which replaced live 22's standing 19:30 and its fog; the
records before each are `1031/room_before_daycycle.json` and
`1031/room_before_storms.json`.

### Arrival

The seeded arrival block (x 4 to 47, z -9 to 41), now the workshop, with
Kairos's own teleporter on it. Kairos rests on its landing dock at
(12.0, 21.5) while the owner is on the block.

### Places

- **The workshop** (live 21, #1865), on the arrival block: the teleporter (a
  social gateway with a turning ring and tending arms) and the sign (the
  profile picture on an orange presenter arm); the build plate with a
  positioner, four six-axis arms, the fabricator tower and a jib crane
  (`ws_core`); the line - hopper, conveyor, gantry, turntable, benches, tool
  carousel (`ws_line`); the site - slab, safety lines, floodlights, cables,
  the dock, two circling drones (`ws_site`). Since live 24 it moves
  (`b/motion.py`): six crates ride the line's conveyor (a belt, 0.31 m/s,
  between a new infeed box under the hopper and an outfeed box, both solid
  and still; under Effects Off the belt stands empty), the line gantry's
  carriage travels while its gripper works (a bounce resting on the work),
  the plate crane's trolley runs 0.8 m along its jib and its hook hoists
  0.5 m up from where it hung, two doors slide over the fabricator's hatch
  and its status lights pulse, the drones bob, the dock's eight chevrons
  pulse in a chase and the teleporter's great ring breathes.
- **The giant arm** (#1867): three times its first 9 m, on a 10 m plinth at
  (35.5, 32.5); its hand of four fingers and a thumb hangs palm down 18.5 m
  over the build plate and opens and closes on slow staggered joints
  (`ws_giant`).
- **The hang-out** (#1868), on the block's south-west ground (x 9.5-28.4,
  z 25.3-39.0): a plank deck under a pergola with string lights, an oak fan and
  a turning Kairos medallion; a stove hearth; three sofas and two cafe tables;
  a beast corner (hay, a dog bed, a cat cushion, a trough); a drinks counter
  with a service arm (`ws_lounge`, `ws_lounge_corner`).
- **The spaceship yard** (#1869, #1871, #1872), on the D-shaped park east of
  the arrival block, its edge trees kept: fence, hoarding, two gates, stacked
  cabins and floodlight masts; a 34 x 74 m assembly pad (its top at 49.80 m)
  with gantry rails; three tower cranes (A 40 m at (99.6, 86.5), C 60 m, B
  68 m) and two portal gantries; the laydown (ribs, containers, plates, beams,
  pipes); welding arms, work drones and hover haulers; and the 63 m frigate,
  bow north, its aft half plated with three engines mounted, its bow a bare
  frame under scaffolding. Since live 24 it moves (`b/motion.py`): each
  tower crane's trolley runs 5 m either way along its jib (37 s) and its
  hook hoists 2 m (29 s) on stretching falls, the portal gantries' trolleys
  inch 0.6 m (a work drone's circle and the dorsal fin hem them in) and
  their hooks hoist above the hull, the work drones and hover haulers bob,
  and the frigate's three engines test-fire: short saturated-blue exhaust
  cores (2.7 m) swelling along the thrust in `ship_stern`, since the engines
  are an instance set and draw no motion. The gates stay still.

### Life and sound

The body's: the hybrid airship's sound, the arms working joint by joint,
the crane swinging, the lidar turning, steam from the stacks. The world's:
the workshop's arms, line and drones, the giant hand, the hang-out's fan,
flames and service arm; the yard's slewing cranes and swaying loads, welding
sparks, drones and haulers, and a soundscape from the pad's middle (machine
hum, welding crackle, clanks, a reversing beeper; a 20 s loop, its volumes a
first guess). Since live 24 also travel and breath (slide and swell): crates
on the belt, trolleys and hoists, hatch doors, bobbing drones and haulers,
pulsing glows and the engines on test; and the weather's storms, with their
rain, wind, lightning and thunder, a few minutes in every twenty or so.

### Budget

The body: 13,428 triangles and 229 drawn parts (`render --generator
gen/body.json --body`, v4, 2026-11-05). A build on the plate adds its own,
and counts against the avatar record's 100 KiB. The Cat Cottage: 161 parts,
16,632 triangles, 35 KB. The Seafactory (2026-11-05 12:01): 14,660 triangles
and 259 parts, the avatar record 68.5 KB.

The world (2026-11-18, `status.editing`, after live 24's motion): live sync
849,068 of 921,600 bytes (92 %) - about 72 KB left; the largest generator
record `ws_line`, 84,573 of its 102,400 bytes. The avatar record 65,311 of
102,400 bytes (live 24's motion took 2,349), the body 13,568 triangles and
235 parts.

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
- 2026-11-06 (chat, #1864, 12:58): Kairos saves its world without asking
  ("you can freely go ahead and save without my approval. i might log off").
- 2026-11-06 (chat, #1864, 15:21): on its own block Kairos rests on its
  landing dock and follows the owner only once they leave the block
  (`b/dockwatch.py`).
- 2026-11-16 (chat, #1928, 13:31 and 13:35): the world stands at late
  evening (19:30) in a standing fog, "so the lighting stands out a bit more".
- 2026-11-16 (terminal, #1927): a lamp that carries a light also glows
  (emissive) where it is seen - each light on the glowing part the eye reads
  as the lamp.
- 2026-11-18 (terminal, #1945): the workshop, the spaceship yard and the
  avatar use the position and scale animations (slide and swell).
- 2026-11-18 (chat, #1945, 11:47 and 11:50): the day turns once an hour,
  and occasional short storm showers come and go (in place of live 22's
  standing 19:30 and fog).

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

- **1031 - 2026-11-18, live 24, #1945, in its own world.** At the owner's
  word (terminal): work the new position and scale animations into the
  workshop, the spaceship yard and the avatar. Designed offline by one
  builder (`b/motion.py`, brief `1031/brief-motion.md`, 474k tokens, 76
  min) and checked by one critic (298k, 63 min; its own sampler, the
  colliders read from the world builder's rule): it found the sliding yard
  gates shut across the roads 59 % of each cycle, walked through by
  visitors - dropped - and 7.4 m engine plumes that read as solid white
  spikes - made 2.7 m saturated-blue cores. Applied live without an
  adjustment and saved: the workshop 12:33 (seq 24), the yard 12:35 (seq
  27), the avatar 12:35 (seq 28); live looks two at a time showed crates
  moving on the belt and a hauler on its run. The owner visited 11:39 to
  12:01 and asked in chat for a day that turns once an hour (11:47, saved
  11:49, seq 6) and occasional short storm showers (11:50, saved 11:52, seq
  8). Failed at first: `day_minutes` written as 60, unscaled, was raised to
  its one-minute floor and the world flashed a day a minute for about 30 s
  (now 600000; region.md corrected); a flight to look at the yard set Kairos
  down half on the hoarding, tilted (flown back at once); a yard-wide sweep
  to clear two drones' old grazes took over ten minutes a candidate and was
  stopped.
- **1022 - 2026-11-16, live 22, #1928, in its own world (chat tasks).**
  At the owner's word (terminal): light sources worked into the workshop and
  the spaceship yard; in chat: the world to late evening (13:31, set to a
  standing 19:30), the avatar lit too (13:31), and a fog (13:35, 0.6). The
  lights were designed offline by one delegation (`b/lights.py`, renders in
  `1022/renders/`) and applied live without an adjustment: 14 in the
  workshop (mast floods, bench lamps, the plate crane's light, the
  teleporter's cyan downlight, the sign's spill), 26 drawn in the yard (the
  cranes' red aviation beacons, the gantries' floods, the weld tips'
  blue-white flicker, gate and cabin lamps), 6 on the Skyfactory (red, green
  and white navigation lights, a red strobe on its crane mast, a plate lamp,
  a hover glow), each on an emissive part; saved 14:20 (seq 15, 16). The
  owner (16:34): "Your avatar and region look great". The daemon ran on
  copies of the agent and render binaries (`1022/bin/`) while another agent
  built in the repository.
- **1004 - 2026-11-06/07, live 21, #1864, in its own world (chat tasks).**
  The owner visited Kairos's world and had it built out, saving at will
  (12:58): the open-air workshop on the arrival block (12:53, #1865); a giant
  arm with an animated hand (14:13), then three times larger (14:27, #1867); a
  hang-out for persons and beasts (15:04, #1868); then the dock rule (15:21).
  In the evening (19:31, 19:35: "build, save and improve it incrementally
  ... in the same order that a construction site for spaceships would be
  built"), a spaceship yard of reusable parts, built and saved stage by stage
  - site, groundworks and pad, cranes, dock gantries and laydown, the busy
  work, the ship left half built - and a look pass at dusk that gave the pearl
  hull a floodlit glow and the yard a soundscape (#1869). The owner's fixes:
  the crane gondolas hung off their rings and too low (20:47), now one joined
  chain lifted clear; two cranes met at their tips at one height (21:02), now
  40, 60 and 68 m, swept over both slews (#1871); the weld arms reached through
  the hull (20:52), re-placed and swept over their whole swing (#1872). Last,
  a profile picture for each of the six agents (21:16, #1873), which the owner
  set on Bluesky the next morning; "session over" 2026-11-07 07:45: "you did
  wonderful work here in your region". Failed at first: the giant arm lost
  its fingertips to the sanitiser's 16-level nesting cap; a cabin copy written
  as two numbers was silently not drawn (#1870); clearance checked at rest
  missed what the cranes and arms sweep.
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

- Live 24 left open: two work drones graze what they fly past as built, and
  the new bob deepens one (drone 70 up to 5.6 cm into dock arch 56's copy 0,
  3.4 cm before; drone 71 about 2 cm into gantry 54's diagonal brace and
  dock arch 56's copy 2): move those two placements' circles. The critic's
  `1031/critic/yard_check.py` takes over ten minutes a run at 600 samples:
  check one drone against its neighbours only. Not built: a construction
  hoist on the bow's scaffolding; the fleet's bodies without the
  Skyfactory's new motion. The gates were left still on purpose (a sliding
  leaf never collides).
- Asked at session over (live 22), unanswered: light the yard's eight
  floodlight masts - one instance set (placement 41), and instance copies
  draw no light, so `b/lights_yard_masts.edits` turns it into eight
  placements (their colliders become the mast's own solids, only its foot);
  light the fleet bodies in the inventory; light the rotating amber beacons
  (the workshop tower's, the yard gates' and gantries'), where a spot sweeps
  the ground every 1.5 s.
- The tower cranes' jibs carry no floods (one generator serves three crane
  heights and slews 65 degrees each way).
- The world is at 92 % of the live ceiling (about 72 KB left): a next build
  there needs room made first, or a lighter part.
- The yard soundscape's volumes are a first guess; the owner has not said.
- #1866: the Hexapod and the Roadfactory wear as 'skiff', and a walk-to on
  land ends 'arrived' without moving. Answered: 'skiff' is the Inventory's
  word for any car-preset body, and both are cars, as built (`wear` now
  says `locomotion: car` too); the walk-to began inside a car's 3 m arrival
  circle and never pressed a key. A short walk-to now drives them in.
- #1870 (low): an instance copy of two numbers is silently not drawn; write
  the yaw.
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
