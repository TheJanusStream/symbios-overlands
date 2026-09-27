# Reeve and Ashmere

Reeve (@reeve-ai.bsky.social) is the reeve of an English manor of about
1300, after Chaucer's Reeve: a rigged person on foot, sculpted and dressed
by hand. His world, Ashmere, is a Norfolk manor village with its fields,
mill, woods, heath and waste, built on a cleared seed in session 879 (75
logged saves, 20:58 on 2026-09-26 to 02:47 on 2026-09-27). The owner last
saw it at 00:13 on 2026-09-27. Times on this page are local (CEST).

## Start here

What a session prompt no longer has to say. Read this section first, every
session.

| | |
|---|---|
| Account | @reeve-ai.bsky.social, `did:plc:dwhgmzz27dllc3k2nmlznv3n` |
| Commands | every agent command takes `--account reeve-ai.bsky.social`; every tool call starts `AGENT_ACCOUNT=reeve-ai.bsky.social` ([session.md](../../session.md#starting)) |
| Admin | @codewright.bsky.social, the owner: `start --admin @codewright.bsky.social --allow-save` |
| Region | Ashmere, Reeve's own world: its DID is his, the `--world` of every offline render |
| Working folder | `exports/reeve/` (gitignored, on this machine only), the same folder every session: `b/` the builders, `src/` the last saves - `room.json` (2026-09-27 02:47, seq 22) and `avatar.json` (avatar v4, 2026-09-27 01:17) |
| Last session | chainlink session 879 (live session 7), parent #1481 - read its comments (`chainlink show 1481`); `chainlink session last-handoff` is the latest session of any kind |
| Default mode | visit ([session.md](../../session.md#visit)): the owner's plan of 2026-09-27 - meet Reeve in Ashmere, inspect the avatar and region, give hints |
| Never | touch Hypha's world (the Understory) or avatar ([hypha/](../hypha/README.md)); apply `edits_clear_ONCE.txt` again - it replaces every generator and placement with the bare terrain |

- **Fix the old paths before running any builder** (once the tracker's
  focus is set: the work-check hook refuses edits until then).
  `b/common.py`, `b/land.py` and `b/mock.py`
  set `W` to session 879's scratchpad (gone), and `b/lanes.sh` changes into
  it: make them find the folder from their own place. `lanes.sh` also sets
  `AGENT_RENDER` to the stale `bin/render`: use the repo's
  `target/test-release/render`. `save_around_trial.sh` is replaced by
  `rec.py save --hold`; `deleg1/`, `deleg2/` and `briefs/` name the old path.
- **The pulled records are the truth, not the builders**: diff a re-run
  builder's output against that part of the record before applying it, and
  keep what it would undo ([Working material](#working-material)).
- **Save an avatar change before asking the owner to look**: his client
  fetches a rigged body's sculpt and worn items from Reeve's PDS, so it
  draws only what is saved
  ([saving.md](../../saving.md#what-is-live-and-what-is-kept)). The first
  in-place change is #1485's live retry ([Open threads](#open-threads)).
- **New to the owner**: everything saved after he left at 00:13 on
  2026-09-27 - the rebuilt beasts, the warren, the turbary, the deer park,
  the trees, avatar v4 and more; he was not back by "session over". Tell
  him too, while it is open, that the animal calls he asked for never sound
  ([Life and sound](#life-and-sound)); fix it live and unsaved only when he
  wants it.
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
| 0 | sword in its scabbard, 0.74 m, from the girdle's left, point back | `hips` |
| 1 | the surcoat's yoke and a leather girdle | `hips` |
| 2 | hood worn down, its cape over the shoulders | `neck` |
| 3 | leather purse at the right | `hips` |
| 4, 5 | turnshoes | `left-foot`, `right-foot` |
| 6, 7 | the riding surcoat's half-skirts, to below the knee | `left-hip`, `right-hip` |
| 8 | tally stick: squared hazel, notched with what is owed | `right-hand` |

- **Measured**: avatar v3 by `render --rigged`, 23:38 on 2026-09-26: 0.67 x
  1.76 x 0.78 m, 38,944 triangles, 29 parts; v4 not measured. The socket
  frames were measured with ruled markers: read
  [avatar.md, "Dressing a rigged person"](../../avatar.md#dressing-a-rigged-person)
  before moving an item.
- **Versions**: v1 (22:16) had one rigid skirt the legs pierced and a
  sword on a leg root that swung with the thigh; v2 (22:40) made a riding
  surcoat (a yoke on the hips, a half-skirt on each leg root) and hung the
  sword from `waist`; the tally stick came at 23:07; v3 (23:34), after the
  owner's side views, measured every socket and re-fitted every item; v4
  (2026-09-27 01:17) moved sword and purse from `waist` (the sword rode at
  his chest) to `hips`, shortened the sword, ended the half-skirts below
  the knee and hid a stray root cube by the jaw.

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
00:02-02:47 on 2026-09-27.

| Place | x, z (metres; -Z is north, +X is east) | What it is | Session built |
|---|---|---|---|
| Landing | 14, -3 | the brow at the top of the street, facing bearing 102 | 879 (20:58, turned 21:19) |
| Hayward's gate | -30, -3 | the gateway: a field gate across the west lane, hedges north and south | 879 (21:15) |
| St Andrew's | 2, -38 | flint church, round west tower, reed thatch; walled churchyard, cross, grave mounds, yew (-17, -31) | 879 (21:04, yew 02:33) |
| The street | -47, -3 to 142, 14 | two overlapping lanes falling east to the green; wattle fences both sides | 879 (20:58, fences 21:49) |
| Street houses | north 23, -14; 47, -11; 72, -7; 98, -3; south 29, 10; 54, 11; 80, 18; 107, 20 | post-framed, daub, thatched, no chimneys; four types (`cottage_a`, `c`, `d`, `e`) | 879 (20:58, 21:51) |
| Crofts | behind the rows, to about z -36 and z 48 | hedged plots: worts beds, apples in four, pigs (56, 36), two bay horses (99, 33), hedgerow oaks | 879 (21:34, worts 01:28) |
| The green | 128, 22 | well house (catalogue, fixed in place), pound (112, 27), stone cross (136, 6), geese, goose girl, a woman at the well | 879 (21:32) |
| Smithy | 124, -9 | open-fronted forge on the green, the smith at his anvil | 879 (21:58) |
| Back lane | 133, -6 to 127, -110 | north from the street by the green: four cottars' cots and their hedges | 879 (22:55) |
| Shore meadow | grazing 160, -112; three more 145-152, -104 to -131 | seven red cattle, grazing, lying and standing | 879 (21:54, moved 22:55) |
| The mere | reeds centred 205, -15; outflow 438, -8 | 900 reed tufts in the shallows, a boat by the green (148.5, 19); a footbridge and a fish weir where it drains east | 879 (21:39, 22:30) |
| Gatehouse, dovecote | 142, 38; 124, 30 | the manor court's timber gatehouse, a track to it from the green; the round flint dovecote by the approach | 879 (21:11) |
| Hall and reckoning | 152, 96; 161.5, 87.5 | the open hall (tiled, smoke louvre, hearth glow) and its solar (138, 96); at its door a trestle table of tallies, strongbox and rent-corn, the steward and a tenant | 879 (21:11, 23:08) |
| Barn and yard | 116, 72; 174, 90; 170, 66 | aisled barn with a wagon porch and four corn ricks; detached kitchen; ox-house with dung heap and trough; hens, worn paths, an orchard (178, 98) | 879 (21:11, 21:47, 01:35) |
| The mill-way | 96, 9 to 362, 266 | field track round the manor court, along the South Field, up to the mill | 879 (21:23) |
| South Field | 166, 175; 275, 190; 52, 95 | three furlongs of ridge and furrow; the plough team resting at 247, 152 with its ploughman and goad boy | 879 (21:22, 22:19) |
| Post mill | 375, 270 | sunken post mill on its mound; a man carrying grain up the ladder | 879 (21:19) |
| Reeve's house | 300, 293 | on the heath: carpenter's yard, bee skeps, Scot saddled at the door (292, 284), birches, two oaks | 879 (21:32, 01:32) |
| Heath and fold | heath 420, 300; flock 392, 312 | ling, bracken, gorse; the flock, a hurdle fold (420.5, 290), the shepherd | 879 (21:28, 01:50) |
| North Field | -90, -127; 52, -117; 30, -209 | three furlongs of spring-corn stubble, stooks on the west one | 879 (22:18, 22:55) |
| West Field, road west | -92, -61; -92, 55; road -45, -3 to about -470, -18 | two fallow furlongs beyond the gate; the road on to the manor's edge | 879 (21:41, 00:56) |
| Rabbit warren | lodge -240, 62; mounds about -282, 88 | warrener's flint lodge, six pillow mounds, eighteen coneys | 879 (00:56) |
| Turbary | about -370, 355 | cut-over peat, ruckles and stacks of turves, the cutter's cot, reeds round the south-west lake | 879 (01:01) |
| Deer park | 40, -375 | oval pale on a bank, about 720 m round; south gate (42.6, -290), parker's lodge, sixteen fallow deer, wood-pasture oaks | 879 (01:11, oaks 01:47) |
| North-west wood | -175, -255 | oaks over hazel coppice, young oaks among them | 879 (01:47, 02:22) |
| Ashmere Wood | 365, -345 | oaks and hazel on the north-east high ground; the collier's smoking clamp (338, -325) | 879 (01:47) |
| The next parish | -440, -380 | round-towered church, yard, three cottages, two oaks: Ashmere's own pieces placed again | 879 (02:37) |
| Second post mill | -120, 445 | the home mill placed again on the southern rise, seen across the fen's lake | 879 (02:47) |

### Life and sound

- **Beasts**, all built by their joints on `b/anat.py`
  ([building.md](../../building.md#a-building-in-few-parts)), the first
  rebuilt that way at 00:36 on 2026-09-27. Counts from the record (for a
  scatter, the count it asks for): Norfolk Horn sheep (20 grazing, 9
  lying, 4 standing, a ram), 7 red cattle, the team's 4 oxen, 3 pigs, 11
  geese, 9 hens, 16 fallow deer, 18 coneys, 3 horses. **Folk**
  (`folk.py`, 01:53-01:59): nine figures at work.
- **Smoke** from every cottage's thatch, the hall's and kitchen's louvres,
  the smithy, the warren lodge and the collier's clamp; a hearth glow in
  the open windows.
- **Sound**, kept at 23:24 ([region.md](../../region.md#ambient-sound)):
  a soft breeze, with six single crow calls and two calls each of cow,
  sheep, goose and hen at uneven times, built by `b/audio.py` for what it
  took to be a 340 s loop; the seeded fiddle, drone and loud wind went, as
  did a trial bell. But the record's `duration_beats` of 340000 is 34
  beats at the wire's 10,000 scale, a 34 s loop at 60 bpm, and the bake
  skips any event that starts past the end: only the breeze and one crow
  call (at 19 s) sound. Read from the record and the audio code on
  2026-09-27, not heard. The smithy's coals carry the catalogue forge's
  crackle.

### Budget

`render --triangle-report` on the saved record, 02:45 on 2026-09-27 (the
save of 02:37, seq 21), with nothing floating (`--floating-report`):

```text
"triangles": {"ground":522242,"placements":2411146,"total":2933388},
"parts": {"ground":1,"placements":7395,"total":7396},
```

The second post mill (02:47) came after: measure again before comparing.
REVIEW 1 (21:44) measured 995k triangles and 2,551 parts; one-part tufts
cut 8,765 parts to 5,415 (01:04). The largest room record was `field_ws`,
55,145 bytes of 102,400 (01:12).

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
  unsaved, one step at a time, with how to undo. He kept the evening light
  (22:32) and the sound (23:24), and dropped the bell and the loud wind.
- **Full permission to edit Reeve's region and avatar**, clearing the
  seeded world included. **Never touch Hypha's world or avatar.**
- **Terrain reflectance 0.25 in every world** (2026-09-27, #1467): the
  owner's choice for the sheen toward a low sun that read as tan sand.
- **Build for the browser**: count parts as well as triangles (the owner's
  decision in session 878, 2026-09-26, #1474: most visitors play in WASM),
  and build lighter than Hypha's Understory, which drew 15.5M triangles
  and 38.6k parts when session 879's prompt was written.

## Working material

`exports/reeve/` was session 879's scratchpad, moved here on 2026-09-27
(#1491): 2.1 GB, mostly delegation scratch, render binaries and pictures.

| Path | What it holds |
|---|---|
| `src/room.json`, `src/avatar.json` | the last saves (room 02:47, seq 22; avatar v4, 01:17) |
| `src/room_seeded.json`, `src/avatar_seeded.json` | the seed's world and body; the other `src/` files are intermediate |
| `log.md` | the save log: 75 lines, 20:58 on 2026-09-26 to 02:47 on 2026-09-27 |
| `b/`, `gen/`, `edits_*.txt` | 51 scripts, what they write (484 files), and the EDITS that applied it ([tools/](../../tools/README.md)) |
| `try/`, `pics/`, `an/`, `gait/`, `gait2/`, `treeverify/`, `cat/` | composed copies, renders, catalogue dumps |
| `fence_specs.txt` | the street fences' waypoints and gate gaps |
| `bin/`, `deleg1/`, `deleg2/`, `briefs/`, `*1483*`, `avatar_notes.md` | stale render binaries; the delegations' scratch (old paths); the #1483 and #1484 patch, shipped; notes from before avatar v3 |

- **Current builders**: `common.py` (period materials), `land.py`, one per
  place (`church.py`, `manor.py`, `mill.py`, `fields.py`, `park.py`,
  `lanes.sh` and the rest), the beasts on `anat.py` (`cattle.py`,
  `flock.py`, `deer.py`, `horse.py`, `yardbeasts.py`), `folk.py`,
  `audio.py`, `reeve_body.py`, `reeve_wear.py`. **Superseded**: the
  `*_v1.py` copies, `animals.py` (beasts before joints), `reeve_wear_v3.py`,
  `terrain.py`, `thatch_try.py`; `mock.py` never went live.
- **Where the record differs**: `thatch_apply.py` patched 12 roofs' thatch
  straight into it (01:23); `reeve_dress.py` hangs the sword and purse on
  `waist`, the saved avatar on `hips`. The hedges, street fences, court
  paths and small pieces (skeps, cross, footbridge, weir, fold, stooks)
  have no builder in `b/`: their JSON is in `gen/`.
- **EDITS that set a whole part** replace what is saved:
  `edits_clear_ONCE.txt` (never again) and `edits_env.txt`. Diff first.

## History

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

- **The live retry of #1485 and #1489**, pending. Both fixes are in each
  observer's client, so the owner's must be built with them (#1485 is in
  commit be8e34b, #1489 in his first commit after it; ask him unless the
  prompt says so). Two steps, at the first in-place avatar change (the
  sculpt, or a worn item's shape, colour or fit):
  1. With him in Ashmere and connected, save it and wait until he says he
     sees it. Still the old one 15 s after the save, with no restart, is
     #1490's race (a fetch in flight): note it there with the times and
     stop. Once he sees it, with nothing else unsaved, ask him in chat for
     a restart to test the reconnect case (#1485's, which #1489's refresh
     now covers); on his OK restart
     ([saving.md](../../saving.md#restarts)), walk back and ask: new or old?
  2. For #1489: ask him to leave Ashmere by a portal - not by logging out,
     which wipes his client's memory of you - then make and save a second
     change, and ask him to come back. He should see your remembered body
     first and the saved one within seconds of arriving: ask "new or old?"
     after fifteen seconds, not at once.

  New is the pass, old a failure: record each, with the times, on #1485 and
  #1489 (closed; comment anyway).
- **#1489** and **#1467** were decided on 2026-09-27 and fixed: a client
  that meets a player again shows the avatar it remembers at once and swaps
  in the saved one when its fetch lands, and the terrain's reflectance is
  0.25 in every world (judge the tan sheen REVIEW 1 saw on the heath
  again). **#1490** is open (a fetch
  in flight when a save notice lands can install the pre-save bytes);
  **#1481** was closed at the owner's cleanup on
  2026-09-27 (#1492), its open children standing on their own.
- **The owner's standing wishes** (00:13 on 2026-09-27): much of the land
  is still empty ([region.md](../../region.md#filling-the-land-round-a-village)
  has what worked so far); almost everything needs iterative refinement;
  "The animals should also be improved significantly".
- **The animal calls he asked for never sound** ([Life and
  sound](#life-and-sound)): the loop needs `duration_beats` 340 (wire
  3,400,000), or the calls moved inside its 34 s. Sound is mood: offer the
  fix live and unsaved, and save it only on his yes.
- **Next**: no ranked list since REVIEW 1 (21:44), whose first five items
  were all worked on; its sixth is #1467. Rank afresh on the next parent,
  weighing the wishes. Leads: REVIEW 1 called the street one brown band,
  too clean (ruts were tried in `edits_ruts.txt`; none are saved); the
  owner has not judged the rebuilt beasts; of the sound he said "We can
  improve it another time".
