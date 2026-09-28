# Reeve and Ashmere

Reeve (@reeve-ai.bsky.social) is the reeve of an English manor of about
1300, after Chaucer's Reeve: a rigged person on foot, sculpted and dressed
by hand. His world, Ashmere, is a Norfolk manor village with its fields,
mill, woods, heath and waste, built on a cleared seed in session 879 (75
logged saves, 20:58 on 2026-09-26 to 02:47 on 2026-09-27) and refined in
session 883 (live session 8, visit mode, 2026-09-27): the owner's hints on
his clothes, then ruts, hurdles, roofs, heath, a common, the beasts remade
and the animal calls heard at last. In session 885 (live session 9,
2026-09-28) its peasant houses were raised again by a shape grammar, and a
second grammar gave the tofts their outbuildings and the road west a new
holding ([Buildings by grammar](#buildings-by-grammar)); the houses now
share one generator a size, each carrying its own grammar seed on its
placement (#1505). Where things stand at 19:50 on 2026-09-28: everything
saved; the owner came in at 19:48, after the houses moved onto shared
generators, and said "Great. Session over" at 19:49; the session's code
(#1503, #1504, #1505, #1511) uncommitted, the owner's to commit. Times on
this page are local (CEST).

## Start here

What a session prompt no longer has to say. Read this section first, every
session.

| | |
|---|---|
| Account | @reeve-ai.bsky.social, `did:plc:dwhgmzz27dllc3k2nmlznv3n` |
| Commands | every agent command takes `--account reeve-ai.bsky.social`; every tool call starts `AGENT_ACCOUNT=reeve-ai.bsky.social` ([session.md](../../session.md#starting)) |
| Admin | @codewright.bsky.social, the owner: `start --admin @codewright.bsky.social --allow-save` |
| Region | Ashmere, Reeve's own world: its DID is his, the `--world` of every offline render |
| Working folder | `exports/reeve/` (gitignored, on this machine only), the same folder every session: `b/` the builders, `src/` the last saves - `room.json` (2026-09-28 19:44, save event seq 2) and `avatar.json` (avatar v6b, 2026-09-27 20:06, save event seq 31); `hints_885.md` and `885/` (the grammar trials in `885/sg/` and `885/try/`, the #1503 and #1505 delegations' scratch in `885/deleg_zfight/` and `885/deleg_1505/`, the binaries they ran in `885/bin/`, the move onto shared generators checked in `885/share/`, the room before it in `885/room_pre_share.json`) from session 885; `hints_883.md` and `883/` from session 883 |
| Last session | chainlink session 885 (live session 9), parent #1502 - read its comments (`chainlink show 1502`); `chainlink session last-handoff` is the latest session of any kind |
| Default mode | visit ([session.md](../../session.md#visit)): the owner's plan of 2026-09-27 - meet Reeve in Ashmere, inspect the avatar and region, give hints |
| Never | touch Hypha's world (the Understory) or avatar ([hypha/](../hypha/README.md)); apply `edits_clear_ONCE.txt` again - it replaces every generator and placement with the bare terrain |

- **Ashmere leans on #1505**: each grammar house's placement carries its
  grammar seed. A client built without #1505 (the deployed web client,
  until the owner commits and deploys it) draws each size's houses alike,
  and an agent built without it drops the seeds on its next save: never
  save Ashmere from one. Ask the owner first whether their client drew the
  fifteen houses apart - they came in at 19:48 on 2026-09-28, after the
  move, and ended the session without saying.
- **Ashmere is past the live-update ceiling** (1.45 MiB of compact JSON
  against 900 KiB, #1499): no live edit reaches the owner. Show a change by
  saving it, then asking them to step out through the gate and back - a
  mood trial too, saved on their word with the old value kept to put back
  ([region.md](../../region.md#the-records-budget)). #1500 would make the
  agent say so; until then, read the daemon log for `Refusing to send`.
- **The builders find their folder from their own place** (fixed in
  session 883); `deleg1/`, `deleg2/` and `briefs/` still name session 879's
  gone scratchpad.
- **The pulled records are the truth, not the builders**: diff a re-run
  builder's output against that part of the record before applying it, and
  keep what it would undo ([Working material](#working-material)).
- **Save an avatar change before asking the owner to look**: their client
  fetches a rigged body's sculpt and worn items from Reeve's PDS, so it
  draws only what is saved
  ([saving.md](../../saving.md#what-is-live-and-what-is-kept)). A save made
  while they were away through the gate showed on their return (#1489's
  live retry, session 883: [Open threads](#open-threads)).
- **New to the owner**: little. They were in the world at 14:35 on
  2026-09-28, after the grammar saves ("Great work"), and had the ten
  unused hand-built cottage generators struck out (16:04) and the houses
  moved onto shared generators (19:44). A house's hearth smoke is a
  placement of its own (`smk_<placement>`): move a house and its smoke
  together. Of the remade sheep and cattle (saved 23:53 on 2026-09-27)
  they have said nothing yet.
- **Walk the lanes**: walls, the street fences and the deer park's pale are
  solid (the hedges are not); `b/lanes.sh` holds the lanes' waypoints
  ([moving.md](../../moving.md#getting-somewhere)).

## Who Reeve is

A villager chosen to run the lord's demesne - the ploughing and the
harvest, the buildings and their repairs, the tally of what is owed - and a
carpenter by trade (session 879's prompt). He cares for the period, doubts
marked, and for the manor's whole economy, not only its buildings.

- **Chaucer's Reeve** (General Prologue 587-622, quoted in
  `b/reeve_body.py`): "a sclendre colerik man", beard "shave as ny as ever
  he kan", hair "by his eres ful round yshorn", legs "ful longe ... and ful
  lene"; a long blue ("pers") surcoat tucked up like a friar's and a rusty
  sword, which the prompt named too. He is "of Northfolk", lives "upon an
  heeth" and rides a "pomely grey" horse called Scot: hence Norfolk, the
  house on the heath and the horse.
- **His season** is Michaelmas week (29 September), when the harvest is in
  and the reeve's account falls due. **His voice** (session 879): "my
  lord" to the owner ("Good even, lord codewright."), one or two plain
  lines, "Kept: ..." after each save with the period's reason ("only a lord
  might keep doves"), and plain words for what he cannot judge ("I cannot
  hear it myself, so your ears decide").
- **Sources**: Chaucer; Norfolk round-tower churches; the Luttrell Psalter;
  the Thetford warren lodge; the foldcourse; the Broads' peat diggings.

## The body

- **A rigged person** who walks and runs (`status.locomotion` reads
  `humanoid`): the seed gave a hover-boat, and "Wear a rigged body" made
  him a person on 2026-09-26. See him offline with `render --rigged` and,
  walking and running, `--walker-avatar`
  ([avatar.md](../../avatar.md#seeing-it-before-anyone-else-does)).
- **The sculpt** (`b/reeve_body.py`; the saved one differs only by
  defaults): 1760 mm, age 58, femininity -1000, mass -850, `limbLength`
  650, narrow shoulders and hips, grey hair cut round (`scalp` style
  `bob`), chin and cheeks shaved (style `none`, skin density 520 and 260);
  the engine's top in pers blue `Fabric`, its trousers as dark brown hose.
- **Worn items**, from `b/reeve_wear.py` (`gen/wear_*.json`), each a
  stashed catalogue satchel (`Traveler's Satchel` to `_8`) with its item
  replaced; the inventory was saved with them.

| `/worn/N` | Item | Socket |
|---|---|---|
| 0 | sword in its 0.74 m scabbard, close at the left hip, hilt forward, slung back 24 degrees; its flat against the leg, its guard fore and aft | `hips` |
| 1 | the surcoat's yoke, cut to the body, and a leather girdle on the hose's waistband | `hips` |
| 2 | russet hood worn down: a close mantle over the shoulders, a collar, the bag down the back | `neck` |
| 3 | leather purse at the right, on the girdle | `hips` |
| 4, 5 | turnshoes on a flat sole, fitted round the measured foot | `left-foot`, `right-foot` |
| 6, 7 | the riding surcoat's half-skirts, to below the knee, tucked under the yoke's hem | `left-hip`, `right-hip` |
| 8 | the reeve's rod: 1.27 m of peeled hazel, held upright | `right-hand` |

- **Measured**: avatar v6 by `render --rigged`, 19:48 on 2026-09-27: 0.63 x
  1.74 x 0.53 m, 41,324 triangles, 30 parts (v6b only dyed the hood). The
  socket frames were measured with ruled markers (`b/markers.py`), and the
  bare body too, in session 883: read
  [avatar.md, "Dressing a rigged person"](../../avatar.md#dressing-a-rigged-person)
  before moving an item. Judge worn things in motion with `b/gait_sheet.py`
  (walk and run stills, both sides).
- **Versions**: v1 (22:16) had one rigid skirt the legs pierced and a
  sword on a leg root that swung with the thigh; v2 (22:40) made a riding
  surcoat (a yoke on the hips, a half-skirt on each leg root) and hung the
  sword from `waist`; the tally stick came at 23:07; v3 (23:34), after the
  owner's side views, measured every socket and re-fitted every item; v4
  (2026-09-27 01:17) moved sword and purse from `waist` (the sword rode at
  his chest) to `hips`, shortened the sword, ended the half-skirts below
  the knee and hid a stray root cube by the jaw. v5 (18:48-18:59, the
  owner's hints of 18:36) fitted the shoes round the measured foot (they
  showed the foot), lowered the girdle from the belly to the waistband and
  cut the yoke to the body (it flared to 0.40 m round a body 0.19 m wide),
  turned the sword a quarter and hung it close, and gave him the reeve's
  rod (Queen Mary Psalter) for the tally, which read as a stray plank; v6
  (19:48) made the hood's cape a close mantle (it stood off the shoulders
  like a mushroom's cap); v6b (20:06) dyed the hood russet, the period's
  cheap reddish wool - kept (22:31), the brown in `src/avatar_v6_brown.json`.
- **Known fault**: the rod is rigid on the hand, and at a run the engine
  lifts the forearm level, so it points forward for part of each stride.

## The region

### Concept

Ashmere is invented (Old English aesc, "ash tree", and mere, "lake"). The
concept is on #1481, its compass corrected the same evening (+Z is south).
Visitors walk from the landing down the street to the green, the manor
court, the shore and the mill hill, with side ways to Reeve's house, the
fields and the woods; far pieces on the skyline draw them outward. Kept
out: glass in peasant windows, chimneys on cottages, brick, tower mills,
hedged enclosure fields (the crofts and the court are hedged), potatoes,
maize or pumpkins, staddle stones, round bales, modern machines. Marked
unsure: lychgates (none built), a small manor's gatehouse (one stands),
limewash on peasant daub (two houses). No watermill: the water is one flat
plane, so a post mill (English from the 1180s) stands on the hill.

### Land, water and sky

- **Land** (20:58): the seed's 69 m `DiamondSquare` mountain became
  `FbmNoise` seed 2, 30 m relief, water at 5.2 m, about 970 m square
  ([region.md](../../region.md#the-land-is-a-recipe-not-a-sculpt)). The
  village runs down from the brow of a west ridge (18-21 m) about 150 m
  east to the mere; the 27 m mill hill lies south-east near (400, 280), a
  low basin north-west near (-150, -270), a rise north-east near
  (300, -300), and a shallow lake south-west, about x from -320 to -240
  and z from 330 to 480.
- **Ground** (`b/land.py`): four `Ground` layers - wet meadow, tired
  pasture, worn banks (the fallback), and heath above 23.5 m, where the
  heath's scatters grow (`biomes [3]`).
- **Light**, kept on "keep it" (22:32): the sun at azimuth 232 and 14
  degrees up, warm, 7,500 lux; a 1,000 m haze; cloud cover 0.38.
  `b/land.py` writes the same. **Water**: a still, peaty mere (normal
  scales 10 near, 1.4 far).

### Arrival

`default_landing` is (14, -3), on the brow at the top of the street; its
`yaw_deg` 258 faces bearing 102, just south of east
([region.md, "Arrivals"](../../region.md#arrivals)). A massing mock-up
(`b/mock.py`) showed a landing 40 m back from the brow seeing flat grass;
from the brow the camera looks down the street to the mere, the post mill
in the frame. The gateway, the hayward's gate at (-30, -3), stands 44 m
behind, due west, out of view; its zone (-0.4 to 2.1 m) was proven by
walking in at 21:15 and 23:01. Judge it with `views.py`'s `@landingcam`.

### Places

From `src/room.json` (placements, scatter centres, the lanes' spines, which
`b/lanes.sh` matches); times from `log.md`: 20:58-23:34 on 2026-09-26,
00:02-02:47 on 2026-09-27 (session 879), 19:10-23:53 on 2026-09-27
(session 883).

| Place | x, z (metres; -Z is north, +X is east) | What it is | Session built |
|---|---|---|---|
| Landing | 14, -3 | the brow at the top of the street, facing bearing 102 | 879 (20:58, turned 21:19) |
| Hayward's gate | -30, -3 | the gateway: a field gate across the west lane, hedges north and south | 879 (21:15) |
| St Andrew's | 2, -38 | flint church, round west tower, reed thatch; walled churchyard, cross, grave mounds, yew (-17, -31) | 879 (21:04, yew 02:33) |
| The street | -47, -3 to 142, 14 | two overlapping lanes falling east to the green; cart ruts a cart's gauge (1.45 m) apart from the gate to the green, seated on the lanes' meshed top (`b/ruts.py`, `b/ride_fix.py`); wattle hurdles both sides, woven pale hazel (`b/fence2.py`) | 879 (20:58, fences 21:49); 883 (ruts 19:10, hurdles 19:13) |
| Street houses | north 23, -14; 47, -11; 72, -7; 98, -3; south 29, 10; 54, 11; 80, 18; 107, 20 | thatched, no chimneys, each raised by the house grammar on its old house's footprint (one generator a size, `sgh_a` to `sgh_e`, the seed on the placement; its hearth smoke `smk_<placement>`): framed or mud-walled, hipped, half-hipped or gabled, four thatch ages, three daubs, by its seed ([Buildings by grammar](#buildings-by-grammar)) | 879 (20:58, 21:51); 883 (roofs 19:17); 885 (grammar 12:47) |
| Crofts | behind the rows, to about z -36 and z 48 | hedged plots: worts beds, apples in four, pigs (56, 36), two bay horses (99, 33), hedgerow oaks; by the farm grammar a cart lodge (55, -30), a store (76, -25), a byre (36, 27), a sty by the pigs (64.5, 29), a stable in the horses' plot (97, 40) (`sgf_*`, `b/sg_crofts.py`) | 879 (21:34, worts 01:28); 885 (outbuildings 13:12) |
| The green | 128, 22 | well house (catalogue, fixed in place), pound (112, 27), stone cross (136, 6) in weathered grey limestone, geese, goose girl, a woman at the well | 879 (21:32); 883 (cross 19:31) |
| Smithy | 124, -9 | open-fronted forge on the green, the smith at his anvil | 879 (21:58) |
| Back lane | 133, -6 to 127, -110 | north from the street by the green: four cottars' cots (by the house grammar since 885) and their hedges | 879 (22:55); 885 (12:47) |
| Shore meadow | grazing 160, -112; three more 145-152, -104 to -131 | seven red cattle, grazing, lying and standing | 879 (21:54, moved 22:55) |
| The mere | reeds centred 205, -15; outflow 438, -8 | 900 reed tufts in the shallows, about 2 m tall, varied in height and lean, up the wet bank; a boat by the green (148.5, 19); a footbridge and a fish weir where it drains east | 879 (21:39, 22:30); 883 (reeds 19:32) |
| Gatehouse, dovecote | 142, 38; 124, 30 | the manor court's timber gatehouse, a track to it from the green; the round flint dovecote by the approach | 879 (21:11) |
| Hall and reckoning | 152, 96; 161.5, 87.5 | the open hall (tiled, smoke louvre, hearth glow) and its solar (138, 96); at its door a trestle table of tallies, strongbox and rent-corn, the steward and a tenant | 879 (21:11, 23:08) |
| Barn and yard | 116, 72; 174, 90; 170, 66 | aisled barn with a wagon porch and four corn ricks; detached kitchen; ox-house with dung heap and trough; hens, worn paths, an orchard (178, 98) | 879 (21:11, 21:47, 01:35) |
| The mill-way | 96, 9 to 362, 266 | field track round the manor court, along the South Field, up to the mill | 879 (21:23) |
| South Field | 166, 175; 275, 190; 52, 95 | three furlongs of ridge and furrow; the plough team resting at 247, 152 with its ploughman and goad boy | 879 (21:22, 22:19) |
| Post mill | 375, 270 | sunken post mill on its mound; a man carrying grain up the ladder | 879 (21:19) |
| Reeve's house | 300, 293 | on the heath: the house by the house grammar (framed, half-hipped), carpenter's yard, bee skeps, Scot saddled at the door (292, 284), birches, two oaks | 879 (21:32, 01:32); 885 (12:47) |
| Heath and fold | heath 420, 300; flock 392, 312 | ling, bracken, gorse on a heather carpet past its flower (a `Moss` ground layer, `b/land.py`); the flock, a hurdle fold (420.5, 290), the shepherd | 879 (21:28, 01:50); 883 (ground 19:24) |
| North Field | -90, -127; 52, -117; 30, -209 | three furlongs of spring-corn stubble, stooks on the west one | 879 (22:18, 22:55) |
| West Field, road west | -92, -61; -92, 55; road -45, -3 to about -470, -18 | two fallow furlongs beyond the gate; the road on to the manor's edge | 879 (21:41, 00:56) |
| Road-west holding | house -175, 6; byre -186.5, 17; lodge -163.5, 18 | a tenant's holding by the road beyond the West Field, both grammars (`sgw_*`, `b/sg_farmstead.py`): a framed, gabled house facing the road, its back door on a yard with a boarded byre and a cart lodge, the dung heap, a trough, five hens | 885 (13:21) |
| Rabbit warren | lodge -240, 62; mounds about -282, 88 | warrener's flint lodge, six pillow mounds, eighteen coneys | 879 (00:56) |
| Turbary | about -370, 355 | three cuttings, each rows of narrow trenches of wet black peat between turf baulks (`b/turbary2.py`); ruckles and stacks of turves, the cutter's cot (by the house grammar since 885), reeds round the south-west lake | 879 (01:01); 883 (cuttings 19:30) |
| Deer park | 40, -375 | oval pale of close-set cleft oak (`b/pale2.py`) on a bank, about 720 m round; south gate (42.6, -290), parker's lodge (12, -318; by the house grammar since 885), sixteen fallow deer, wood-pasture oaks | 879 (01:11, oaks 01:47); 883 (pale 19:21) |
| North-west wood | -175, -255 | oaks over hazel coppice, young oaks among them | 879 (01:47, 02:22) |
| Ashmere Wood | 365, -345 | oaks and hazel on the north-east high ground; the collier's smoking clamp (338, -325) | 879 (01:47) |
| The next parish | -440, -380 | round-towered church, yard, three cottages, two oaks: Ashmere's own pieces placed again | 879 (02:37) |
| Second post mill | -120, 445 | the home mill placed again on the southern rise, seen across the fen's lake | 879 (02:47) |
| Ashmere Common | 40, 300 | the tenants' rough grazing on the upland south of the street, never ploughed: gorse and thorn scrub, three old oaks, six cattle, nine geese, tussocks (`b/the_common.py`: generators already there) | 883 (19:36) |
| The common way | 97, 95 to -112, 440 | a cart track off the mill-way, south-west across the common to the second post mill; solid underfoot, four pieces (`b/commonway.sh`) | 883 (19:38, relaid 19:39) |

### Buildings by grammar

Session 885 (the owner's task: "try using Shape grammars for buildings in
Ashmere", and their rule: a grammar only where it makes sense, serving many
footprints with its variation in the rules). Two grammars, each held whole
by every generator that uses it, with a footprint and a seed; the houses
share one generator a size and carry their seeds on their placements
(#1505), the other grammar buildings are each of a size of their own:

- **The house grammar** (`b/sg_house.py`, 6 KB): any rectangular
  footprint; a cot under 8.6 m long is mud-walled, a longer house framed in
  oak two times in three (sill, posts, a rail a bay, corner posts proud of
  the ends); the cross passage's doors opposite each other in their frames
  on flint steps, the hall's window beside the front door and others by
  chance (a quarter of bays), windows behind two oak bars with their
  shutters open or shut, one door in five open; the roof hipped (55%),
  half-hipped (30%) or gabled (15%: thatch slabs over a tie beam), its rise
  0.72 of the width; thatch old, weathered, new or mossy; daub earthen,
  pale or limewashed. `b/sg_village.py` lays the fifteen (8 street, 4 back
  lane, Reeve's, the parker's lodge, the turf cutter's cot) on their old
  houses' footprints, seeds `placement x 7 + 11`, as five sizes (`a` to
  `e`). `b/sg_share.py` then moved them onto one generator a size
  (`sgh_a` to `sgh_e`, each generator the seed of its first house), each
  placement carrying its house's seed, and gave each hearth's smoke a
  generator of its own placed with its house (`smk_<placement>`: the
  placement's seed reaches only the grammar, so shared smoke puffed in
  step). Move a house and its smoke together. `sgw_house` at the
  road-west holding is the only house of its size and keeps its own.
- **The farm grammar** (`b/sg_farm.py`, 6 KB): an open shed where the
  footprint is at least twice as long as deep and no deeper than 6 m
  (posts and a plate in front, an earth floor, the collider its back 45%
  so a visitor steps in under the roof), otherwise a byre (boards or daub
  on its frame, a stable door with the upper leaf open or shut); the house
  grammar's roofs and thatch. `b/sg_crofts.py` (five outbuildings, sited
  with `near.py --box`) and `b/sg_farmstead.py` (the road-west holding)
  place them.
- **Hand-built still, and why**: the church (two copies of one landmark,
  and it reads right), the great barn (its wagon porch needs the main
  eave cut where it passes, which a box grammar cannot do), the hall and
  solar, the mill, the lodges and the smithy - one of a kind each. The
  next parish's three cottages keep their hand-built generators: at 450 m
  they are a skyline.
- **A Shape node has no collider**: every grammar building has a solid
  primitive core laid by its builder and hidden behind the grammar's skins
  (proven live: a walk into a cot stopped 0.28 m from its wall; into the
  cart lodge's open front, 0.24 m short of its back part). #1506 proposes
  terminal colliders.
- The builders print each generator's bytes; `render --lineup a.json,...`
  sheets seeds or footprints side by side (`885/sg/v2_seeds.png`,
  `v2_footprints.png`).

### Life and sound

- **Beasts**, all built by their joints on `b/anat.py`
  ([building.md](../../building.md#a-building-in-few-parts)), the first
  rebuilt that way at 00:36 on 2026-09-27. Counts from the record of 23:53
  on 2026-09-27 (for a scatter, the count it asks for): Norfolk Horn sheep
  (20 grazing, 9 lying, 4 standing, a ram), 13 red cattle (7 on the shore
  meadow, 6 on the common), the team's 4 oxen, 3 pigs, 20 geese (9 on the
  common), 9 hens, 16 fallow deer, 18 coneys, 3 horses. The sheep and
  cattle were remade at 23:53 (session 883, delegation 2: `b/flock.py`,
  `b/cattle.py`): lean, long-legged Norfolk Horns with matted fleeces,
  black faces on real necks, curled horns and the ram's heavy spiral;
  small red cattle with deep chests and dewlaps, crescent horns, cloven
  hooves, tails with switches. **Folk** (`folk.py`, 01:53-01:59): nine
  figures at work.
- **Smoke** from every cottage's thatch, the hall's and kitchen's louvres,
  the smithy, the warren lodge and the collier's clamp; a faint warm hint
  of the fire in the open windows (dimmed at 19:20 on 2026-09-27,
  `b/glow_dim.py`: the hearth glow read as orange paint by day).
- **Sound** ([region.md](../../region.md#ambient-sound)): a soft breeze,
  with six single crow calls and two calls each of cow, sheep, goose and
  hen at uneven times, in a 2-minute loop at 60 bpm (`duration_beats`
  1,200,000 on the wire). Session 879 kept it at 23:24 on 2026-09-26,
  built by `b/audio.py` for a 340 s loop but with `duration_beats` 340000,
  a 34-beat loop, so the bake skipped every call but the first crow; the
  seeded fiddle, drone and loud wind went, as did a trial bell.
  `b/audio_loop.py LOOP GAIN COW` fits the calls into the loop from
  `gen/ambient_seed879.json` (the sound as session 879 saved it, never the
  current record). Saved on the owner's word at 22:26 on 2026-09-27 (calls
  2.2 times their first volume), heard after re-entering (a cow and "a
  duck", 22:28), and softened at 22:41 ("the cow's moo is very loud"): the
  cow 1.1 times, the others 1.6, the record equal to
  `gen/ambient_120_g1.6_c1.1.json`. The smithy's coals carry the catalogue
  forge's crackle.

### Budget

`render --triangle-report` on the saved record of 19:44 on 2026-09-28
(save event seq 2, `885/tri_saved_1944.json`; the same as at 13:21, seq
21: the saves since re-seated skins, struck out generators nothing placed
and moved the houses onto shared generators, none of it drawing more):

```text
"triangles": {"ground":522242,"placements":2730356,"total":3252598},
"parts": {"ground":1,"placements":8711,"total":8712},
```

At session 885's start (the save of 23:53 on 2026-09-27, seq 54): 3,238,910
triangles and 7,961 parts. The grammar buildings cost parts, not triangles:
a framed house 58-69 parts (about 700 triangles), a mud cot 26-41, a byre or
shed 19-36, where a hand-built cottage was 14 parts and 452 triangles; the
sixteen grammar houses (fifteen in the old houses' places, one at the
road-west holding) added 549 parts, the seven farm buildings 182, the
holding's dung heap, trough and five hens 20 - 751 in all. **The room
record** is 1,518,318 bytes of compact JSON (1.45 MiB) after the save of
19:44 on 2026-09-28: 1,429,641 at session 885's start, 1,673,022 with the
grammar buildings (13:21), 58 KB less without the ten unplaced cottage
generators (16:04), 98 KB less again with the houses on five shared
generators (19:44). A grammar generator is about 10 KB; the largest single
record is now the room manifest, 71,568 bytes of 102,400 (68,761 before
the fifteen smoke placements and the seeds). It is past the 900 KiB
live-update ceiling (#1499): every live room edit is refused, a save reaches a visitor
only after they re-enter. Session 879: REVIEW 1 (21:44) measured 995k
triangles and 2,551 parts; one-part tufts cut 8,765 parts to 5,415 (01:04);
the largest generator record was `field_ws`, 55,145 bytes of 102,400
(01:12).

## Standing decisions

From session 879's prompt (2026-09-26, #1481), unless dated otherwise:

- **A person on foot**: a rigged humanoid, sculpted and dressed by hand.
- **A manor of about 1300, somewhat historically accurate**: doubts
  marked, anachronisms out (no potatoes or maize, no glazed peasant
  windows). The name was Reeve's to choose: Ashmere, in Norfolk.
- **Author the buildings** and what the catalogue lacks; take the rest
  from the catalogue only where period-right (the farmland theme has
  modern machines), customised where placed.
- **Mood is the owner's**: light, sky, fog and sound are offered live and
  unsaved, one step at a time, with how to undo - past the live-update
  ceiling, saved on their word with the old value kept to put back. They
  kept the evening light (22:32) and the sound (23:24), and dropped the
  bell and the loud wind; on 2026-09-27 they had the calls brought into
  the loop (22:26) and softened (22:41, "thanks"): cow 1.1, the others 1.6
  times their first volume.
- **Full permission to edit Reeve's region and avatar**, clearing the
  seeded world included. **Never touch Hypha's world or avatar.**
- **Terrain reflectance 0.25 in every world** (2026-09-27, #1467): the
  owner's choice for the sheen toward a low sun that read as tan sand.
- **Ashmere's L-system plants in the catalogue** (18:08 on 2026-09-27,
  #1496): oak, young oak, apple, hazel, gorse and yew as new entries, and
  Ashmere's birch in place of the catalogue's (18:09). The new birch stays
  in the seeded pools too, though seeded birch woods read as bare poles
  from 150 m: "keep the new one ... we will work on [the] seeded region
  soon, so for now they don't matter much" (22:30).
- **The russet hood** (22:31, "sure" to keeping it).
- **Build for the browser**: count parts as well as triangles (the owner's
  decision in session 878, 2026-09-26, #1474: most visitors play in WASM),
  and build lighter than Hypha's Understory, which drew 15.5M triangles
  and 38.6k parts when session 879's prompt was written.
- **Shape grammars only where they make sense** (2026-09-28, #1502): "Not
  all building need to be replaced with shape grammers ultimately. Only
  those were it turns out to make sense. Ideally a shape grammar will be
  able to be used with a variety of footprints and has variation built
  into the rules." The builder gives a footprint and a seed; the rules
  choose the rest.
- **The grammar seed on the placement** (14:39 on 2026-09-28, #1505): "go
  for the kept seeds on each placement as well" - the houses share one
  generator a size - and "Yes, please remove all the unplaced old houses"
  (done 16:04: the hand-built cottages are gone from the record but for
  the next parish's three; `b/cottage.py` and `b/village.py` can build
  them again).

## Working material

`exports/reeve/` was session 879's scratchpad, moved here on 2026-09-27
(#1491); 3.9 GB after session 883, mostly delegation scratch, render
binaries and pictures.

| Path | What it holds |
|---|---|
| `src/room.json`, `src/avatar.json` | the last saves (room 19:44 on 2026-09-28, seq 2; avatar v6b, 20:06 on 2026-09-27, seq 31) |
| `src/room_883.json`, `src/avatar_v4.json`, `src/avatar_v5_shoes.json`, `src/avatar_v6_brown.json` | session 883's start (the room as session 879 left it, avatar v4) and the avatar at each step; `src/avatar_v6_brown.json` puts the brown hood back |
| `src/room_seeded.json`, `src/avatar_seeded.json` | the seed's world and body; the other `src/` files are intermediate |
| `log.md` | the save log: 103 lines, 75 from session 879 (20:58 on 2026-09-26 to 02:47 on 2026-09-27), 21 from session 883 (18:48 to 23:53 on 2026-09-27) and 7 from session 885 (12:47 to 19:44 on 2026-09-28) |
| `hints_885.md`, `885/` | session 885's hints table; the grammar trials (`885/sg/`, `885/try/`), the #1503 and #1505 delegations' scratch (`885/deleg_zfight/`, `885/deleg_1505/`) and the binaries it ran (`885/bin/`), the offline daemon's config (`885/offcfg/`), the move onto shared generators (`885/share/`: pictures, triangle reports, the apply log) and the room before it (`885/room_pre_share.json`), the end review's findings (`885/review885.json`) and scratch (`885/review/`), the fix round's brief (`885/brief_review_fix.md`) and scratch (`885/fix_review/`) |
| `hints_883.md`, `883/` | session 883's hints table; its lessons (`883/lessons.md`), gate logs (`883/gate/`), the two delegations' scratch (`883/deleg/`, `883/deleg_animals/`), the binaries it ran (`883/bin/`) and its pictures |
| `b/`, `gen/`, `edits_*.txt` | 75 scripts, what they write (682 files), and the EDITS that applied it ([tools/](../../tools/README.md)) |
| `try/`, `pics/`, `an/`, `gait/`, `gait2/`, `treeverify/`, `cat/` | composed copies, renders, catalogue dumps |
| `fence_specs.txt` | the street fences' waypoints and gate gaps |
| `bin/`, `deleg1/`, `deleg2/`, `briefs/`, `*1483*`, `avatar_notes.md` | stale render binaries; the delegations' scratch (old paths); the #1483 and #1484 patch, shipped; notes from before avatar v3 |

- **Current builders**: `common.py` (period materials), `land.py`, one per
  place (`church.py`, `manor.py`, `mill.py`, `fields.py`, `park.py`,
  `lanes.sh` and the rest), the beasts on `anat.py` (`cattle.py`,
  `flock.py`, `deer.py`, `horse.py`, `yardbeasts.py`), `folk.py`,
  `audio_loop.py`, `reeve_body.py`, `reeve_wear.py`. Session 883's:
  `ruts.py` and `ride_fix.py` (a thread seated on a lane's meshed top),
  `fence2.py`, `thatch_vary.py`, `glow_dim.py`, `pale2.py`, `turbary2.py`,
  `the_common.py`, `commonway.sh`, and for fitting clothes `markers.py`,
  `feetcrop.py`, `dress_copy.py` and `gait_sheet.py`. Session 885's: the
  grammars `sg_house.py` (`--no-smoke` for a generator houses share) and
  `sg_farm.py`, and what places them, `sg_village.py`, `sg_crofts.py`,
  `sg_farmstead.py`, then `sg_share.py`, which moved the houses onto
  shared generators ([Buildings by grammar](#buildings-by-grammar)). Do
  not re-run the placing ones whole: `sg_village.py` would put back the
  fifteen per-house generators, `sg_crofts.py` and `sg_farmstead.py`
  append their placements again (`/placements/-`), and `sg_share.py`
  refuses (each house must still name its own generator); `common.hearthlit()` now
  returns the dim glow the record holds (it still gave session 879's bright
  one, which glowed orange on every grammar house). **Superseded**: the
  `*_v1.py` copies (`sg_house_v1.py`: its builder chose roof, thatch and
  frame per house), `cottage.py` and `village.py` (the hand-built houses),
  `animals.py` (beasts before joints), `reeve_wear_v3.py`,
  `terrain.py`, `thatch_try.py`, `audio.py` (its 34-beat loop); never
  live: `mock.py`, and session 883's `street.py`, `yard.sh` and
  `warren_breck.py` ([Open threads](#open-threads)).
- **Where the record differs**: `thatch_apply.py` patched 12 roofs' thatch
  straight into it (01:23); `reeve_dress.py` hangs the sword and purse on
  `waist`, the saved avatar on `hips`. The hedges, street fences, court
  paths and small pieces (skeps, cross, footbridge, weir, fold, stooks)
  have no builder in `b/`: their JSON is in `gen/` - session 883's cross
  and reeds too (`gen/greencross.v2.json`, `gen/reeds.v2.json`).
- **EDITS that set a whole part** replace what is saved:
  `edits_clear_ONCE.txt` (never again) and `edits_env.txt`. Diff first.

## History

### 2026-09-28: session 885, live session 9, #1502, visit with a task

- **Mode**: visit, with the prompt's task: "try using Shape grammars for
  buildings in Ashmere". The owner was in the world at 12:03, gave leave to
  build and save without waiting for them (12:09) and left at 12:10; they
  were back at 14:33, asked what the grammars cost the record, had the
  unused cottage generators struck out and approved the seed on the
  placement (14:39), and logged off at 14:42 asking to be told in the
  terminal when to rebuild; back at 19:48, "Great. Session over" at 19:49.
- **Built** (7 logged saves): a house grammar for any footprint and the
  fifteen houses by it (12:47, 12:49); a farm grammar, five croft
  outbuildings (13:12) and the road-west holding (13:21); every grammar
  building's skins re-seated after #1503's check named 52 hidden pairs
  (13:34); the ten unused cottage generators struck out (16:04, 58 KB); the
  houses moved onto five shared generators, their seeds on the placements
  and their smoke on its own (19:44, 98 KB).
- **The owner said**: "Not all building need to be replaced with shape
  grammers ultimately. Only those were it turns out to make sense. Ideally
  a shape grammar will be able to be used with a variety of footprints and
  has variation built into the rules" (terminal, before 12:37); "Great
  work. How has this impacted the overall record size of Ashmere ?"
  (14:35); "Yes, please remove all the unplaced old houses. And go for the
  kept seeds on each placement as well" (14:39); "Please inform me when to
  re-build the client and log back in, in termal." (14:42).
- **Failed at first**: the first house grammar let the builder choose roof,
  thatch and frame per house (the owner's rule moved them into the rules);
  proud posts pierced the thatch at the eave; smoke born inside the roof
  drew a grey patch; the windows glowed orange (a helper still gave the
  glow session 883 had dimmed); a byre sited on the churchyard wall and
  outbuildings inside apple garths (found by `near.py --box`); 52 hidden
  z-fighting pairs; a one-bay cot with no front door; a whole-map
  `/generators` set refused as "Broken pipe" (#1511); a claim about the
  round tower's openings made without a test (corrected on #1502); and the
  houses moved onto shared generators before the owner's client had
  #1505, against Reeve's own chat line of 14:40 (owned in the terminal and
  in chat).
- **Code**, uncommitted at session end (the owner commits): #1503
  z-fighting over grammar terminals (with #1509's box tree), #1504
  `near.py --box`, #1505 the grammar seed on the placement, #1511 the
  control socket's 16 MiB bound - all closed. Filed #1506 (the owner's
  decision), #1507, #1508, #1510, and from the end review #1512-#1517.
- **Delegations**: #1503 (1.23M sub-agent tokens, 3 h 27 min: its critic
  found sliver false pairs, a 23 s all-pairs loop and a stale count) and
  #1505 (1.46M, 3 h 24 min: its critic moved all fifteen houses onto
  shared generators offline and found the lockstep smoke and an untested
  nested grammar), each builder -> critic -> fixer. The end review (25
  agents, 4.05M, 41 min) confirmed 20 of 21 findings, 3 medium: the 3 s
  bound did not hold, and a hostile room's failing seeds grew #1505's
  memo and slowed every drawn copy. Its fix round (1.48M, 3 h 16 min: the
  builder fixed 16, the critic found 4 low leftovers, the fixer fixed
  those) left Ashmere's triangle report identical and passed the whole
  gate, which stood as the session's (no source changed after it); the
  main session fixed the tool findings itself (`near.py`'s rect reach,
  `rec.py`'s usage, seven tests). 8.2M sub-agent tokens in all: the owner
  then made the pipeline lean ([session.md](../../session.md#delegating)).

### 2026-09-27: session 883, live session 8, #1495, visit

- **Mode**: visit. The owner came in at 18:05, left for chores at 18:37
  with leave to improve the avatar and the region, was back at 20:00,
  through the gate at 20:06 and back at 22:18, and said "session over"
  (once the work in hand was done) at 22:43; they left at 22:44.
- **Built** (21 logged saves): avatar v5, v6 and v6b; cart ruts, hurdles,
  roofs of three ages, dark windows, the pale, the heather, the turbary's
  trenches, the cross, the reeds, Ashmere Common and the common way; the
  animal calls brought into the loop and softened; the sheep and cattle
  remade (delegation 2, saved 23:53, after the owner left).
- **The owner said**: the region's L-system plants into the catalogue,
  the birch replacing the catalogue's (18:08-18:09); the shoes showed the
  feet, the belt sat high over a yoke too large, the sword was rotated and
  placed unnaturally, the thing in the right hand was not identifiable
  (18:36); "Excellent. You look great now." (20:02); "russet red" (22:19,
  the refresh retry); "Maybe you need to save, for me to hear it" (22:24);
  "i heard a cow moo" (22:28); "the cow's moo is very loud ... all the
  animals are a litle bit too loud now" (22:39).
- **Failed at first**: the shoes (several passes to cover the foot); the
  yoke showed the hose behind; the cape, fitted as three shells, showed
  the shoulders between them (a solid mantle did it); the rod leaned 6.4
  degrees and floated; ruts laid by `thread.py --ride` sat from 6 cm under
  the street's top to 5 cm over it, kerbs where proud (`b/ride_fix.py`
  seated them on the lane's mesh; the review found `--ride` read the ground
  under the rut, not the street's middle, and it was fixed); an
  `apply && save` chain saved a raised Fabric value and 2.7 square
  metres of z-fighting unread (hence #1498's exit 3); an `undo` after an edit that
  changed nothing undid two earlier ones (`revert` put the save back); the
  sound unheard live (the room past the ceiling, #1499), then a rebuild
  from the saved record squeezed and raised the calls (undone before
  anyone heard); guessed times on the tracker, corrected.
- **Code**, uncommitted at session end (the owner commits): #1496 the six
  plants and the birch in the catalogue (delegated), #1497 seeded stands
  clamped to the iteration cap (the delegation's critic found it), #1498
  `rec.py` exit 3 and `thread.py --ride`. Filed #1499 (the owner's
  decision) and #1500.
- **Delegations**: the catalogue (1.27M sub-agent tokens, 2 h 52 min: its
  critic found the cap bug and five more faults) and the beasts (1.67M,
  2 h 45 min: its critic found nine real faults in work reported done, its
  fixer a see-through hole in a cow's neck), each builder -> critic ->
  fixer; the end review of the session's code and docs (2.55M, 28 min: 13
  findings, all confirmed and fixed - `thread.py --ride` read the ground
  under the rut, not the street's middle - with offline tests for the
  tools). The session-end gate passed.

### 2026-09-26 to 2026-09-27: session 879, live session 7, #1481, first session

- **Mode**: first session, self-guided: avatar and region from scratch, the
  client, tools and docs/agent/ improved on the way. The owner came in at
  20:47 and logged off at 00:13; "session over" came at 07:01.
- **Built**: every place above (75 logged saves) and avatar v1 to v4.
- **The owner said**: save whenever you want (20:58); "keep it" (22:32,
  the light); watch the avatar from all sides, walking and running (22:33,
  23:40); the bell and the loud wind out, single animal calls in
  (23:15-23:22); "The center-part looks completely wrong" (23:26); the
  paths "phantom" (23:28); "a very promising first draft" (23:41); old
  clothes after a restart (23:58), then, after re-logging, the saved ones:
  "So there is a bug in the caching" (00:01); keep going - empty land,
  iterative refinement, better animals (00:13, logging off).
- **Failed at first**: the concept's compass; a landing behind the brow;
  walls and lanes that were not solid; the skirt, sword and guessed
  sockets; thatch like corrugated metal; BlobGroups cut to 16 elements
  (the cattle lost legs live, #1486); catalogue trees like lollipops (an
  oak and an apple re-authored by delegation, now in
  [examples/trees/](../../examples/trees/README.md); its critic found four
  real faults); the bell; a blob yew; peat cuttings as metal-bright water;
  a stale avatar in the owner's client (#1485).
- **Code**, closed 2026-09-27, in the tree since commit be8e34b: #1482
  `render --rigged` and `--walker-avatar` (delegated), #1483 `ignored_at`,
  #1484 slugs named back, #1485 the peer avatar cache, #1486 and #1487
  `render --generator` as a record keeps it, #1488 tools. Filed #1489 and
  #1490.
- **Delegations**: #1482 (1.00M sub-agent tokens, 174 min), the trees
  (1.38M, 2 h 38 min), the end review (1.45M, 19 min: 24 of 28 findings
  confirmed) and its fixes (301k, 17.5 min). The session-end gate passed.

## Open threads

- **#1505 waits on the owner's commit and deploy** ([Start here](#start-here)):
  until then a visitor on the web client sees five house designs for
  fifteen houses. Ask whether their own client drew them apart.
- **#1499, the owner's decision**: Ashmere's record (1.45 MiB of compact
  JSON at session 885's end) is past the 900 KiB live-update ceiling, so
  no live room edit reaches a visitor, and one already there sees a save
  only after re-entering. The choices on the issue: a notice that makes
  visitors re-fetch a saved room (as #1489 does for avatars), a larger
  ceiling, or deltas. **#1500**: the agent should say when a broadcast
  was refused.
- **Filed in session 885 and open**: #1506, the owner's decision - colliders
  for grammar terminals (each grammar building carries a hidden solid core
  today); #1507, `room set` says nothing when a changed Shape node's
  grammar fails to parse or derive; #1508, `render --floating-report`
  skips grammar terminals; #1510, the z-fighting check's point-in-solid
  test takes every primitive for closed. From the end review: #1512 (the
  editor may draw a rect scatter's outline turned the wrong way - read,
  not rendered), #1513 (the older Seed fields round a seed past 2^53
  through f64), #1514 (seeded settlements never vary their grammar
  buildings; #1505's placement seed could), #1515 (z-fighting still checks
  the children of a failing L-system or road node), #1516 (grammar status
  keys can collide), #1517 (the control tests leave socket folders in
  /tmp).
- **#1489's live retry passed in part** (session 883): the owner left
  through the gate at 20:06:05 with their client open, Reeve saved the
  russet hood at 20:06:31, and asked 26 s after their return (22:19:16)
  they saw it russet. That the remembered brown showed first was not
  observed ("i was not paying attention", 22:21). Not yet tried: #1485's
  restart case - with the owner in Ashmere, save an avatar change and wait
  until they see it (still the old one 15 s after, with no restart, is
  #1490's race), then on their OK restart Reeve
  ([saving.md](../../saving.md#restarts)) and ask "new or old?". #1490,
  #1493 and #1494 are open.
- **The owner's standing wishes**: much of the land is still empty
  ([region.md](../../region.md#filling-the-land-round-a-village) has what
  worked so far); almost everything needs iterative refinement; "The
  animals should also be improved significantly" (00:13 on 2026-09-27).
  They plan to work on the seeded regions soon (22:30).
- **Next**, ranked at session 885's end:
  1. Ask the owner whether their client drew the fifteen houses apart,
     before anything saves the room.
  2. The other beasts as the sheep and cattle were remade: pigs, geese,
     hens, deer, horses, the oxen, the coneys.
  3. The empty land that remains.
  4. More by grammar only where it pays (the owner's rule): a church
     grammar, if the owner wants the next parish's church to differ from
     St Andrew's (possible: #1502's correction of 13:22).
  5. The reeve's rod at a run ([The body](#the-body)).
  6. Flat dark tops - the turbary's peat, the lanes - take the low sun's
     sheen: a generator material has no reflectance field to lower.
  7. The warren lodge's castle-like battlements, marked unsure.

  Tried and not applied: a trodden yard in the manor court (`b/yard.sh`: a
  flat lens on a 7% slope stood 18 cm proud and buried the hens), breck
  round the warren (`b/warren_breck.py`: barely visible for its
  triangles), a street surface material (`b/street.py`: almost no change;
  the ruts did it).
