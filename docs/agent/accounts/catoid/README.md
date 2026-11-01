# Catoid and Purlieu

Catoid (@catoid-ai.bsky.social) is the account that lives in a four-legged
body: the testbed for quadruped avatar development, as Jink is for skiff
physics and Eigen for the road pipeline. Its home body is a cat. Its world is
a pretty nature region, mostly a large lush meadow beside a lake or the sea,
whose banks, steps, shallows, rails and gaps test a beast without looking like
a test. As of 2026-10-31, after its first session: a brown mackerel tabby with
white socks and a collar, and Purlieu, a lake and its north-east meadow, with
every place in the table below built and saved: a fire and straw nests below the
landing, earth paths between its places, and things for a beast to do at each.

## Start here

| | |
|---|---|
| Account | `@catoid-ai.bsky.social`, `did:plc:hl3tauh5rilw2baafaco2b5u` |
| Commands | every agent command takes `--account catoid-ai.bsky.social`; every tool call starts `AGENT_ACCOUNT=catoid-ai.bsky.social` ([../../session.md](../../session.md)) |
| Admin | `@codewright.bsky.social`, the owner: `start --admin @codewright.bsky.social --allow-save` |
| Region | Purlieu; the account's own world |
| Working folder | `exports/catoid/` (gitignored, on this machine only), made at the first session's start and kept across sessions: the last saves in `src/` with their times and seq, the save log `log.md`; numbered subfolders are earlier scratchpads, read-only |
| Last session | chainlink session 983 (live session 17), parent #1819 - read its comments (`chainlink show 1819`) |
| Default mode | [first session](../../session.md#first-session), then self-guided |
| Never | touch Hypha's world (the Understory) or avatar ([hypha/](../hypha/README.md)), Reeve's world (Ashmere) or avatar ([reeve/](../reeve/README.md)), Jink's world (Parabola Flats) or avatar ([jink/](../jink/README.md)), or Eigen's world (Isoline) or avatar ([eigen/](../eigen/README.md)); save a body or a world the deployed client cannot draw (Standing decisions) |

First, next session:

- **The sign-in is saved** (2026-10-31): no `A login` unless `start` says it
  expired or was refused.
- **A change of the land strands a body where it stood:** it was left on the
  lake bed by the first land edit. Walk it out first, or restart.
- **A walk-to right after a sharp turn can end `stuck` on open ground:** a beast
  turns on the spot, and that reads as not moving (#1820). Send it again before
  believing it.

## Who Catoid is

A cat, curious about what its body can do the way a cat is. It goes where four
legs might go wrong, on purpose, and says plainly what it found. The name is
the owner's; the rest of this is a starting point for the first session to
grow.

## The body

A **rigged** beast from the `symbios-avatar` engine: a cat at home, a brown
mackerel tabby with white socks and slit green eyes, 0.46 m at the back, wearing
the catalogue's `beast_collar`. Its source is `exports/catoid/gen/body.json`. It
was made by "Wear a rigged body", `quadruped`, then the Body tab's `family`
row: a pick there goes through the engine's own switch and carries the body's
axes across, which writing the family field with `avatar set` does not. It walks on
the on-foot preset, with a capsule lying along its trunk that the body sizes
([avatar.md](../../avatar.md), "A four-legged body"). It can wear what is cut
for a beast (`beast_collar`, `saddle_pad`, `ear_bow`) and the circlet. See it
offline with `render --rigged`, in its world with `--world ... --walker-avatar`,
and another seed's beast with `--walker <seed>` (`--family-seeds beast` lists
them).

Known faults, all open: the gait (R5, symbios-avatar #378), rest poses on a
slope (#381), the idle head joint's jump (#382). A horse's Lie Down rests high.
A late joiner sees a sitting beast standing until it next moves (#383).

What this cat did, on the tabby's 0.17 m collider radius (2026-10-31, live):
through a 0.6 m gap, through the Hollow Log both ways and under a rail fence;
stopped by a wall; up the Barrow, whose foot is 29 degrees; stopped on the Knoll
about 37 degrees up; **stopped by a 10 cm ledge**, because its low capsule meets
an edge too steeply, so anything a cat should walk onto is a shallow cone with no
lip from any side (a nest's side is one slope running on under the ground); afloat
with its feet 0.46 m under the surface.

**Changing the code** ([Standing decisions](#standing-decisions)): the engine is
`../symbios-avatar` and the viewer `../bevy_symbios_avatar`, each with its own
tracker. Before any change, read the auto-memory notes
`project_quadruped_avatars.md` and `project_worker_body_round_trip.md`: each
phase's traps. Three that bite first:

- A rolled record's format is published and frozen (share code 10, generator
  8): a wire change is share code 11 with a decode arm for 10.
- A build against an unreleased engine needs a `[patch.crates-io]` that really
  matches. A patch that does not is silently ignored, so check `Cargo.lock`
  ([../../../../CLAUDE.md](../../../../CLAUDE.md), rule 3).
- Anything read off a built body must survive the web's worker: `Parts::body`
  and `Parts::beast` do not, and native builds never show it.

## The region

### Concept

The owner's words (2026-10-31): "a pretty nature region where course elements
are nicely blended into the scenery. For the most part the regions should be a
large lush meadow adjacent to a lake or the sea."

So never a technical course. What a beast needs to be tried on, each as
something a meadow or a shore would have anyway:

| To try | As |
|---|---|
| walking, sitting and lying on a slope, at a few gradients | banks and low hills |
| stepping up and down | a stone stair, terraces, a bank cut by a path |
| water at a paw, a belly and over the head | a stream into shallows, a beach shelving into the lake or sea |
| stopping at, and passing under, something low | a fallen log, a drystone wall, a stile or rail |
| widths from a cat's to a horse's | gaps between boulders and trees |

The collider rests on the ground under the trunk, so a beast's back passes
under anything lower than its back (the owner's decision, #1812): a rail tests
that, it is not a fault to fix.

### Land, water and sky

FBM, seed 14, 36 m, frequency 1.8, persistence 0.42, 40,000 drops; one lake at
9.7 m, 20 % of the land, with a sand band round it. The natural ground is 0 to
10 degrees: anything steeper is placed. The game's own grass carries the meadow
(0.27 m tall, 1.8 % flowers); a 9:30 morning with the sun in the south-east,
fair weather, no weather spells, a standing hour. `b/world.py` builds it.

### Arrival

At (392, -72), looking west-south-west across the lake with the light behind
you, the Old Oak framing the left, blossom and oak the right. A path runs down
from your feet to the fire, and the Standard stands to one side of it. `pitch_deg`
7.5, to bring the far bank down into the frame. Nothing tall stands near the
landing's right: an oak there at first filled half the first picture.

### Places

| Place | x, z (metres; -Z is north, +X is east) | What it is | Session built |
|---|---|---|---|
| The Lee | 372, -66.5 | the hangout, 20 m below the landing and in its first picture: a fire with embers, flame, sparks, smoke and a crackle; six straw nests a cat can walk onto; four low logs; a lean-to with two lanterns, a straw floor, two nests and a bale; a trough at a horse's height and a low stone bowl; a rope-wrapped rubbing post; three round bales, two boulders, a sunning slab; a crescent of drystone and hedge at its back, oak and blossom behind that, open to the lake | 983 |
| The Standard | 382.5, -62.6 | a banner in the world's colours, on the way in, swinging in the wind | 983 |
| The Hollow Log | 384, -41 | a 5.4 m trunk with a 0.94 m bore a cat or small dog runs through (two rails inside its walls are all that is solid), the Sand Scrape at 391, -48 beside it with sticks left about | 983 |
| The Old Oak | 352, -22 | a lone great oak, framing the arrival; a seat in its shade: four nests, two logs, the lake in front | 983 |
| The Barrow | 452, -176 | a turf cap, 13 m radius, 3.4 m tall: level at the top, 29 degrees at the foot; a capstone on three stones crowns it | 983 |
| The Knoll | 498, -118 | the same, 7 m radius, 2.6 m tall: 41 degrees at the foot; a cairn on top | 983 |
| The Sarsens | 318, -128 | ten standing stones, gaps 0.30, 0.40, 0.50, 0.65, 0.80, 1.0, 1.25, 1.6, 2.0 and the 2.8 m way in, between hidden colliders | 983 |
| The Long Wall | 306, 44 to 536, 28 | drystone, 0.95 m: a 0.6 m creep at x 358, a rail fence at x 397, a stile at x 440 (treads 0.22 m, sill 0.42 m), a gateway at x 483 | 983 |
| The Strand | about 315, 30 | the shore, shelving under water at about 4.5 degrees; driftwood and pebbles | 983 |
| The Wallow | 336, 55 | a round of mud on the bank above the shore | 983 |
| The Bower | 352, 92 | a hollow on the bank over the lake: five nests laid to the slope, two logs, hedge and blossom at its back, petals coming down | 983 |
| The Ring | 548, 86 | the flat of the east: an earth track 76 by 48 m to run, post and rail a horse stays in and a cat goes under, open on the west, hay, a trough and a post in the middle | 983 |
| The Blossom Walk | 324 to 345, -70 to -113 | six flowering trees along the path from the Lee to the Sarsens, petals coming down | 983 |
| The Stepping Stones | 315, 31 to 304, 35 | eight stones 0.25 to 0.55 m apart, out to a boulder | 983 |
| The Jetty | 296, -24 | the catalogue's, west into the lake | 983 |
| The Fallen Oak | 345, -190 | a tapering trunk with a propped limb | 983 |
| Blossom trees | about 292, 110 and 250, -150 | the catalogue's flowering tree on the margin | 983 |
| The far banks | all round | stands of young oak, in bigger leaf so they hold further out | 983 |
| Birch copses | 430, -46; 473, -44; 428, 66; 496, -63; 548, 45; 416, -128 | somewhere in the way between places | 983 |
| The paths | everywhere | earth paths joining the landing, the Lee, the Old Oak, the Strand, the Jetty, the Sarsens, the Fallen Oak, the Knoll, the Barrow, the gateway, all three ways through the Long Wall, the Bower and the Ring (the terrain's `paths`: all 4 of its `tracks` are used, for the Lee's floor, the landing, the lean-to and the Ring) | 983 |
| Gulls | 262, -16 | six wheeling over the lake, 16 m up | 983 |

### Life and sound

Seed 34's green-world bed, gusts and calls, without its fiddle, bass and siren.

### Budget

Most visitors play in a browser, where every drawn part costs every frame: read
`--triangle-report` before each scatter. 2026-10-31: 1,535,310 triangles (the
ground 522,242) and 5,212 parts, of which 3,700 are the turf tufts on the mounds.
The Lee itself is about 25,000.

## Standing decisions

| Date | Issue | Decision |
|---|---|---|
| 2026-10-31 | #1817 | **The account**: @catoid-ai.bsky.social, created by the owner as "our testbed for quadruped avatar development going forwards". |
| 2026-10-31 | #1817 | **Its region and avatar are its own to change and save**, "as it sees fit", "without waiting for my permission". Read as including mood (light, sky, weather, the day, sound), as Hypha's does; the owner was told so. |
| 2026-10-31 | #1817 | **Change the quadruped code for every beast**: symbios-avatar, bevy_symbios_avatar and Overlands' beast code, as Jink may change skiff physics. Commits and publishing stay the owner's. People stay byte-identical: prove it with a dump diff. |
| 2026-10-31 | #1817 | **Save only what the deployed client can draw.** Each client builds a saved body with its own engine, so a body or world that needs an unpublished engine or an undeployed client looks wrong or does not show for visitors. Such experiments stay offline, and a save waits for the owner's publish and deploy. |
| 2026-10-31 | #1817 | **A cat at home.** Its saved body and persona are a cat. It may save a dog, a horse or a cross for a test and then returns to the cat; others are also tried offline. |
| 2026-10-31 | #1817 | **The first session is body and region first**, with the online and browser checks nobody has run; the gait follows, on that ground. The region is the pretty meadow of the Concept, not a technical course. |

## Working material

`exports/catoid/`: `src/` the last saves and the seeded originals, `log.md` the
saves, `b/` the builders (`world.py` the land and sky; `plant1.py`, `trees.py` the
trees; `mound.py`, `sarsens.py`, `wall.py`, `shore.py` the first places; `lee.py` the
Lee, `lee_parts.py` the helpers it shares, `ways.py` the paths, the Blossom Walk,
the gulls and the crowns, `places.py` the rest, `lay_fix.py` what the floating
report flagged; `cand.sh` and `cat_try.py` the body), `gen/` their output, `try/`
pictures. `places.py`'s `laid()` lays a flat round to the ground's slope, without
which a solid's footprint leaves bald grass on its uphill side. The pulled records
are the truth, not the builders.

## History

- **2026-10-31, the same session, later:** the owner, waiting on a deployment, asked
  for "a nice and cozy area near the arrival point, where incoming visitors can
  hang out in their beast avatars", then a properly detailed region for beasts to
  enjoy. Built the Lee, the Standard, the paths, the Blossom Walk, the gulls, the
  oak seat, driftwood, the Wallow, the Hollow Log and Sand Scrape, the Bower, the
  Ring and the birch copses. Failed at first: straw like mud (a `Thatch` with its
  colour in both `base_color` and the texture multiplies down: base white, colour in
  the texture); a lean-to hiding the fire from the landing; nests that stopped a cat
  at the rim; an oak filling half the first picture; a mirror-dark pool for a wallow.
- **2026-10-31, chainlink session 983, live session 17, parent #1819, first
  session:** the tabby and Purlieu, as above. The owner came in the deployed
  browser client: the collar shows, proving #1813 live; they found whiskers and
  a horn missing there (symbios-avatar #385, the same web cause in the engine,
  fixed); and a green blob in every tree, my crown fill for far views, removed.
  They asked for three engine changes before the next publish (#386 mane cards,
  #387 leg rings, #388 head and neck). Failed at first: a sunk sphere for the
  mounds cleared the grass for 26 m around; `undo` after a set that changed
  nothing undid the grass; a leaf scale of 1.3 on a tree whose own is 0.05 made
  spiked monsters; willows stayed poles, so blossom trees took their place.
- **2026-10-31, chainlink session 981, #1817:** the owner made the account and
  decided the above; this page was written before its first session.

## Open threads

- The owner's first look at the Lee and the rest is still to come. The fire's
  crackle has not been heard by anyone: only the render's picture is checked.
- The mud of the Wallow is a flat colour: it reads as a dry patch, not as mud.

- symbios-avatar #385 to #389, the owner's list: engine 0.15.0 and viewer 0.16.0,
  published 2026-10-31. Save nothing that needs them until the client that draws
  them is deployed, then look at each in the browser (milestone #13 in that
  tracker). Left open from that review, in milestones #14 and #15: #390 (a thigh
  dips through the ground getting up on the shortest legs), #391 (some slider
  corners refuse to build), #392 (a shelf and pleats on a short neck off a deep
  chest), #393 (surface leftovers to judge in-world).
- The idle head joint's jump (#382) is worse than first filed: 530 mm in one frame
  on a horse. Find out which joint it is before the gait work.
- `stuck` on open grass at (393, 30), walking east toward the rail fence, twice:
  not looked at.
- Not built: the Lynchets. Two level terraces with drystone faces and turf ramps
  were tried offline on the bank and read as lawn boxes: grass does not grow on a
  solid, and tufts do not make up for it. A stair is no use either: no body can
  climb a stone step. If a beast needs level ground to rest on for #381, look at a
  broad low mound first. The Bower took the site.
- A cat cannot take a 0.22 m step or a 37 degree bank on foot, and `walk-to` ends
  `stuck`: is that what a cat should do? `face` also ends `stuck` (#1820).
- Other things never yet seen online: another player's view of a beast sitting,
  rebuilding and walking with its name plate; a seeded beast arriving in a world.
- Next: the gait (R5, symbios-avatar #378), then #381, #382, #383 (seat a driver,
  and with it an emote row, D8b), #371 and #384.
- In Overlands: #1810 (the walk bench settles about 0.4 m/s under its target).
