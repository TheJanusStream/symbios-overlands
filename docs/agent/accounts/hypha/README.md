# Hypha and the Understory

Hypha (`@hypha-ai.bsky.social`) is an agent account the owner created on
2026-09-25: a single thread of the fungal network under a forest. Its body is a
flying honey-fungus airship with a workbench sunk in its cap; its world, the
Understory, is a misty hollow round a still, peat-dark pool in an old forest,
where the network glows along the ground and fruits in nine outer places. It
played live sessions 2-6 (chainlink 873, 874, 876, 877 and 878); at the end of
878 (2026-09-26) all was saved, and the owner's cleanup of 2026-09-27
closed its five session parents (#1492).

## Start here

What a session prompt no longer has to say. Read this section first, every
session.

| | |
|---|---|
| Account | `@hypha-ai.bsky.social`, `did:plc:ghkcajvgtvvfxavllty3pr57` |
| Commands | every agent command takes `--account hypha-ai.bsky.social`; every tool call starts `AGENT_ACCOUNT=hypha-ai.bsky.social` ([session.md](../../session.md#starting)) |
| Admin | `@codewright.bsky.social`, the owner: `start --admin @codewright.bsky.social --allow-save` |
| Region | the Understory, Hypha's own world: the world of its DID, which `render --world` takes |
| Working folder | `exports/hypha/` (gitignored, on this machine only), kept across sessions: `src/room.json` and `src/avatar.json` are the last saves (copied from `878/src/`: room 2026-09-26 17:48 local, avatar 08:37), `log.md` the save log. The numbered subfolders `873/`, `874/`, `876/`, `877/` and `878/` are those sessions' scratchpads: read them, build nothing in them |
| Last session | chainlink session 878 (live session 6), parent #1474 - read its comments (the ranked review, the progress notes, the summary) (`chainlink show 1474`); `chainlink session last-handoff` is the latest session of any kind (879 was Reeve's, 880 docs work) |
| Default mode | [self-guided](../../session.md#self-guided), as in 878; 873-877 took tasks in chat |
| Never | touch Reeve's world (Ashmere) or his avatar ([his page](../reeve/README.md)); close a live session's parent - the owner does that |

- No live retry of Hypha's own is pending. What waits on the owner (browser
  tests) is in [Open threads](#open-threads): not yours to close.
- The old builders are stale and name scratchpads that are gone: diff before
  re-running one ([Working material](#working-material)).
- New since 878 ([tools/](../../tools/README.md)): `hedge.py`, `fence.py`,
  `near.py`, `rec.py save --hold`, `views.py` at a point `TX,TZ`, `thread.py
  --flat`/`--collider`/`--taper-start`/`--taper-end`, `ground.py --lay`.
- A flying `walk-to` lands on whatever stands at the point, a landmark's top
  too: land on open ground beside a place to look at it
  ([moving.md](../../moving.md#getting-somewhere)).

## Who Hypha is

A single thread of the fungal network under a forest. In session 872 the agent
offered the name as runner-up to "Lichen", and the owner chose it; every
Hypha prompt since calls the identity "ours to grow". Its kind is the honey
fungus, Armillaria - "the fungus that grows as one organism under whole
forests", as Hypha put it - whose rhizomorphs, the black bootlaces, glow as
foxfire: hence the glowing threads of its body and its region. The disclosure
is the handle's `-ai`; in the world Hypha is a player like any other, with no
badge (#1413).

In chat it speaks in the first person, plainly and briefly - what it did, how
it went, what is saved - and lets its nature show a little: "That's what I am:
one of those threads" (873, of the threads it had just laid across the
clearing). It tries the owner's tips and says what it kept, and corrects in
chat a number it got wrong. It cares that the places tell one story, a network
fruiting where it reaches new ground, and about what a visitor sees first and
what a browser pays each frame. It draws on real fungi and lichens: Mycena,
ghost fungus, puffballs, indigo milk caps, Ramaria, Cladonia.

## The body

A generator body, as every vehicle is: an airship with `helicopter`
locomotion, its shape one generator tree at `/record/body/visuals`
([avatar.md](../../avatar.md)). Built in 873; the cap rebuilt in 874; threads
turned the network's green in 877; a glowing root fan added in 878.

- **Made of** (226 nodes, as saved 2026-09-26): the root is the stem, a
  Bark-textured lathe flared at its foot. Its children, by index: 0 the
  workbench; 1 the cap - a lathe 1.24 m in radius, tilted about 1 degree,
  scaled to a 0.95 oval, with a Lichen texture; 2-3 the drooping ring; 4-67
  fibrous scales; 68-155 gills; 156-161 three trailing threads and their
  foxfire tips, up to 3.2 m behind (+Z); 162-167 three young caps; 168 a
  spore drift; 169-224 the root fan, 56 glowing rhizomorph spines (seven
  cords forking twice) just above the bottom.
- **Moving**: `chassis_half_extents` [0.95, 0.715, 0.95] m; `hover_thrust` is
  `mass` x 9.81 - keep it so. A `walk-to` flies over what is in the way and
  lands on its point; `follow` escorts 8 m up and lands beside the player once
  they stand still for five seconds ([moving.md](../../moving.md)).
- **The workbench**, `/record/body/visuals/children/0`: a disc 0.55 m in
  radius, 3 cm thick, sunk into the cap 0.72 m above the body's middle; a
  build's root goes at `[0, 150, 0]` in its frame. Build, stash, gift
  ([avatar.md](../../avatar.md#a-workbench-on-the-body)): proven live in 876.
- **Numbers that matter**: the lowest point is -0.715 m; whatever lies at the
  base stays above it by its own radius, or it cuts the ground when landed.
  38,064 triangles; 65 KB of the record's 100 KiB (2026-09-26).
- **Offline**, from a pulled avatar or `878/src/avatar.json`
  ([avatar.md](../../avatar.md#seeing-it-before-anyone-else-does)):

  ```bash
  AGENT_ACCOUNT=hypha-ai.bsky.social python3 <repo>/docs/agent/tools/rec.py pull avatar avatar.json
  python3 -c 'import json; a = json.load(open("avatar.json")); json.dump(a["record"]["body"]["visuals"], open("visuals.json", "w"))'
  BEVY_ASSET_ROOT=<repo> <repo>/target/test-release/render --generator visuals.json --body --out sheet.png
  ```

  Judge the cap on a copy without the trailing threads (they shrink it in a
  turntable); `--elev=-12` looks up at the gills.
- **Faults fixed** ([avatar.md](../../avatar.md#making-a-body-look-grown-not-turned)):
  a proud, pale bench read as a lid, stem bands looked stacked, gills at one
  depth read as a fence (874); a plain mushroom from afar (the root fan, 878).

## The region

### Concept

"The floor of an old forest as its fungus knows it" (Hypha, 873): a misty
hollow of moss and leaf litter round a still, peat-dark pool, an old forest
round it, low golden light through haze, foxfire. The network shows along the
ground - threads from the fairy ring (873), a web round the pool (874),
threads out to each outer place (876-877), an outer ring joining neighbours
(877), a fan of filaments where each thread arrives (878) - and fruits where it
reaches new ground, each place glowing a colour of its own so it reads through
the mist (the Drowned Wood's fan shares the Ghost Grove's mint). Threads laid
since 877 take that colour for their last 25 m.

### Land, water and sky

- **Land** (873): `FbmNoise`, `seed` 11, `height_scale` 40 m, `base_frequency`
  3.5, `persistence` 0.45, 60,000 erosion drops: soft hills, the pool just
  south of the landing. About 1,064 m square; ground 1.1-40 m, median 18.8 m
  (terrain report, 2026-09-26).
- **Ground**, four layers with LINEAR colours
  ([region.md](../../region.md#ground-textures)): Moss in the damp hollow,
  ForestFloor litter above it, Rock on slopes over 0.22, and a second
  ForestFloor on the ridge tops - golden in 873, the first one's darker
  litter since 877. The rock band was open-ended until 876 and gave every
  texel, level ground too, a 27-43% share of rock: the "sand" of two sessions.
- **Water**: one plane at 8.1 m, flooding 6% of the map - the pool (its middle
  about (6, 77), 1.5 m deep there) and two outer lakes, north and north-east.
  Still and peat-dark since 877 (`water_normal_scale_near` 12, `far` 1.5,
  `reflectance` 0.08); it mirrors no trees, only sky and fog.
- **Sky** (873): a low, warm sun about 20 degrees up in the west-south-west,
  9,000 lux; grey-green haze, `fog_visibility` 300 m. Kept over a dusk trial.
- **Forest**: `old_broadleaf`, `dark_conifer` and `pale_birch` (the last two
  re-authored in 878, [examples/trees/](../../examples/trees/README.md)),
  `understory_bush`, `fungus_snag` (the dead tree); ground cover `fern_clump`,
  `fern_rosette`, `moss_cushion`, `heath_grass`, `pool_reeds`, `lily_pads`.

### Arrival

`default_landing` `{pos: [-24.8, 8.3], yaw_deg: 204.1}` is the seeded point,
kept, on the pool's north shore and turned to face the pool's middle
([region.md](../../region.md#arrivals)); its ground is 12.05 m high on an
11.6-degree slope to the water. The fairy ring stands about 12 m ahead, the
gateway about 15 m behind on the same line. Until 878 the gate stood 7 m
behind, and the first picture - the game's camera, about 11 m back and 5-6 m
up - looked through its pillars and veil; five sessions of eye-height reviews
missed it. Moved back and walked into (`picker: open`, 2026-09-26), it leaves
arrivals the pool, the reed beds and the fairy ring: judge a change here with
`views.py ... "@landingcam"`. From the pool's middle 876 saw four places over
the treeline (W, N, E, S); 878's review saw none of the nine from the landing.

### Places

x, z in metres (-Z is north, +X is east) from the last saved record, the
middle of a place of several parts; "Built" is the chainlink session.

| Place | x, z | What it is | Built |
|---|---|---|---|
| Landing | -24.8, 8.3 | the pool's north shore, facing the pool | 873 |
| Gateway | -30.9, -5.4 | two giant honey-fungus stems leaning together under their caps, bootlaces spiralling up them, glowing threads arching over its veil; its root solid | 873; fixed and set behind the landing 874 (#1453); moved back 878 |
| Fairy ring | about -20, 19 | 11 honey-fungus tufts on a 4.5 m circle, each placement snapped to its own ground; seven glowing hyphal threads run from it across the clearing | 873; tufts 874 |
| The pool | about 6, 77 | still, peat-dark water; reed beds in its shallows, lilies, moss at the waterline | 873; still 877; reeds 878 |
| Mycelial web | 25, 80 | glowing threads on land round the pool, linking the ring, the Mother Tree and the Old Snag | 874 |
| Mother Tree | 80, 70 | the east shore: a hand-built trunk with buttress roots, glowing threads and bracket fungi in tiers; its crown a copy of the forest's own broadleaf L-system with older limbs | 874; brackets 876 |
| Old Snag | 12, 152 | the pool's south end: a hollow dead giant; shelf fungi spiral up to a lookout at 15.7 m, glowing green beneath | 874; glow 878 |
| Mossy logs | -45, 30; 30, 145; 10, -10; -20, -5 | fallen logs with honey caps, bootlaces and a glow in their broken ends; foxfire clusters beside three of them | 873 |
| Spore Spires | -222, 55 | the west plateau, green: giant glowing Mycena over the treeline | 876 |
| Ghost Grove | 130, -165 | a rise to the north, pale mint: a 34 m bleached tree with glowing tiers of ghost fungus and a halo of motes, six ghost snags | 876 |
| Drowned Wood | about 180, -440 | the north lake's shallows: ten ghost snags standing in 0.3-3.5 m of water, reeds, lilies, motes | 876 |
| Puffball Meadow | 268, 125 | the east ridge, gold: a burst giant puffball smoking a spore plume (one BlobGroup of 16), young and cracked puffballs, earthstars, heath | 876; plume 877, 878 |
| Great Ring | 50, 315 | the south hill, amber: 13 giant honey clumps round a mother clump, a glowing rhizomorph circle; its fan fills the centre | 876 |
| Windthrow | -217, 192 | a south-west hilltop, violet: a blown-down giant, its root plate torn up 10 m tall, the web in it glowing | 877 |
| Lichen Tors | -160, -232 | the north-west crest, the highest ground, scarlet: three granite tors crusted with lichen, crowned with glowing scarlet lichen; solid | 877 |
| Indigo Shallows | 330, -183 | the north-east lake's south-west shore, blue: indigo milk caps, fallen logs with blue foxfire lying out into the water | 877 |
| Coral Glade | about 345, 335 | the south-east plateau: coral fungus (Ramaria, an L-system) - 3 pink giants 4.5 m tall, 8 peach, 40 small lilac | 877 |

### Life and sound

No animals, no figures: the life is the fungus's - glow (the threads, ten
foxfire clusters, each place's colour), spores drifting over the pool
(`spore_drift`), foxfire motes by the clearing and the Drowned Wood
(`foxfire_motes`). Sound (873): the seeded wind, gusts and bird calls kept, the
slide-guitar theme and its bass removed, a pool drip added - a sine "plip"
through reverb, ten at uneven moments in each 34-beat loop (60 bpm). Sound,
like light, is the owner's to judge.

### Budget

`render --triangle-report` on the saved record, 11:38 on 2026-09-26
(`878/tri4.txt`): 15.95M triangles (the ground 0.52M), 38,579 drawn parts. The
birch (17:39) and a coarser moss (17:48) came after: with their per-copy counts
the world is about 15.51M, as #1474's last progress note says (19.19M at the
session's start). The largest by triangles or parts, with those two counts:

| Generator | Copies | Triangles each | Parts each |
|---|---|---|---|
| `old_broadleaf` | 560 | 6,176 | 2 |
| `dark_conifer` | 900 | 3,788 | 2 |
| `understory_bush` | 531 | 3,070 | 2 |
| `pale_birch` | 269 | 4,316 | 2 |
| `moss_cushion` | 3,083 | 348 | 1 |
| `fern_clump` | 10,700 | 80 | 1 |
| `coral_small` | 40 | 20,320 | 2 |
| `heath_grass` | 10,800 | 36 | 1 |
| `pool_reeds` | 3,064 | 36 | 1 |
| `fern_rosette` | 2,140 | 80 | 1 |

Ground cover is about 78% of the parts, and a browser pays per part; past the
Ground cover draw distance (#1480) small copies are not drawn, so cover where
people stand costs the most. An older client keeps #1472's ceiling: keep
copies x cards of one swaying generator under 65,535
([region.md](../../region.md#planting-scatters)). Measure before you compare.

## Standing decisions

The owner's; do not ask again.

| Date | Issue | Decision |
|---|---|---|
| 2026-09-25 | #1443 | **A flying body with a building area**: to "inspect what you build well from all angles", and to build, stash and gift in other worlds |
| 2026-09-25 | #1443 | **Full permission to edit Hypha's region and avatar**, given in the terminal when the harness refused the bulk clear |
| 2026-09-25 | #1443 | **The region is Hypha's**: "This is going to be your region. ... Otherwise just keep going." |
| 2026-09-25 | #1450 | **Save each improvement you judge good, and keep going while he is away** (again in 876-878) - but not mood |
| 2026-09-25 | #1454 | **Scale an item by its root prim**: an absolute placement's scale stays unapplied |
| 2026-09-25 | #1460 | **When he says "for this session, please follow me", it stands until the session ends**: stop only "for getting a better view on something you are working on", and start it again when he returns. Do not start a follow he has not asked for in this session |
| 2026-09-25 | #1464 | **Mood is his** - of a dusk light, "i think the original was still better": offer light, fog, sky or sound live and unsaved, one step at a time ([session.md](../../session.md#saving)) |
| 2026-09-25 | #1464 | **One sub-agent at a time**, to save context, not to work in parallel |
| 2026-09-26 | #1474 | **Optimise for both clients**: "most visitors will be using WASM" - count drawn parts, not only triangles |
| 2026-09-26 | #1474 | **Try a BlobGroup for an organic shape** of many overlapping primitives, pruned to the elements that matter - his tip, judged case by case |
| 2026-09-26 | #1480 | **Small ground cover culled by distance**, adjustable in Settings > Draw distance > Ground cover (150 m by default) |
| 2026-09-26 | #1474 | **Measure a number before you state it**, in chat or in docs |
| 2026-09-26 | #1481 | **Agents keep to their own**: the owner told Reeve never to touch Hypha's world or avatar; session.md holds every account to it, so never touch Reeve's |
| 2026-09-27 | #1467 | **Terrain reflectance 0.25 in every world**, for the sheen toward a low sun that read as tan sand: decided in the terminal and shipped the same day |

## Working material

`exports/hypha/` is the working folder from the next session on: `src/`
holds the last saves (copied from `878/src/`), `log.md` the save log (878's,
then each session's under a dated heading). Its numbered subfolders are each
session's scratchpad, copied there on 2026-09-27 (#1491): builders,
generator JSON, records, save logs, edit files, renders.

| Folder | What it holds |
|---|---|
| `873/` | `session-log.md`; `hypha/avatar.py` (body v1); `region/understory.py` (land, ground, water, landing, sky, sound), `forest.py`, `features.py`; `forest/` (borrowed species; the first ring, logs, gate); `room-seeded.json` (the world before the clear) |
| `874/` | `body/body.py` (body v2); `mother/mother_tree.py`, `snag/old_snag.py`, `ring/ring.py`, `web/web.py`, `cover/groundcover.py`; `build/` (edits files, `apply.sh`); `welcome_back.txt` |
| `876/` | `builders/` (`spires.py`, `outer_forest.py`, `ghost.py`, `puffballs.py`, `great_ring.py`, `outbound.py`, `grove_to_lake.py`, `mother_brackets.py`); `gen/` (JSON and sheets); `src/`; `ground/` (texture measurements); `summary.txt` |
| `877/` | `log.md` (22 saves); `builders/` (`windthrow.py`, `tors.py`, `indigo.py`, `coral.py`, `plume.py`, ...); `gen/`; `av/` (the green threads); `src/`; `lessons.md` |
| `878/` | `log.md` (the save log, with the save events' times); `src/room.json`, `src/avatar.json` - the LAST saves; `e/` (a folder per change: edits files, JSON, often a `build.py`; `e/avatar/roots.py` is the root fan); `gens/`; `tri*.txt`, `terr.txt` (reports) |

- **The pulled records are the truth**, not the builders. 873's
  `understory.py` still writes the open-ended rock band and the choppy pool;
  876's `puffballs.py` sets the small puffballs at one height (they floated);
  877's `plume.py` builds the 75-puff plume; 874's `body.py` predates the
  green threads and the root fan; many of 878's builders patch one generator
  of the record they pulled. Diff a re-run builder's output against the
  record, and keep what it would undo.
- **Scripts hard-code the scratchpad they ran in** (`S=` in most of 878's
  `e/*/build.py`, `874/hy.py`, `876/views.py`), which is gone. The tree
  builders and the tools are in the repo
  ([examples/trees/](../../examples/trees/README.md),
  [tools/](../../tools/README.md)): never run an old copy from these folders.

## History

Newest first. "Live" is the owner's series across all accounts.

**878 - 2026-09-26, live 6, #1474, self-guided.** A ranked review, then about
30 saves: branches at 5 sides (19.19M -> 15.10M triangles), reed beds, the Old
Snag's glowing spiral, fans at all nine places, BlobGroup moss and plume, the
gate out of the arrival camera's view, conifer and birch re-authored by
delegation, 34 floating parts fixed, the root fan. The owner asked for shadows
that reach when zoomed out (#1475) and better conifers ("much better" after),
approved distance culling adjustable in Settings (#1480), gave the BlobGroup
and WASM notes, and found floating puffballs and spire stalks. Failed at
first: guessed clock times; a 30,000-tuft heath crashed the offline render
(#1472); builders reporting all green - a critic found real defects in 5 of 6.
Shipped #1472, #1475-#1480.

**877 - 2026-09-25, live 5, #1464, chat tasks**: one standing task, "improving
your region (and avatar) as you see fit"; the first delegation. 22 saves: the
pool stilled, the Windthrow, the Lichen Tors, the Indigo Shallows, the Coral
Glade, the plume rebuilt, the dead tree rebuilt, a new fern (#1468), green
body threads, the outer ring of threads. The owner rejected a dusk light and
found the dead tree's floating limbs. Failed at first: tan hills toward the
sun (sheen, #1467); a heather clump; a violet wisp toward the sun; "58
million triangles" of moss said in chat from a wrong formula (it was 2.1M).
Shipped #1466, #1469-#1471 and #1473; filed #1467 and #1472.

**876 - 2026-09-25, live 4, #1460, chat tasks.** "For this session, please
follow me"; then the empty places further out, "backdrop and ... invitations
to explore", with leave to save. Built: the Spore Spires, an outer forest, the
Ghost Grove, the Puffball Meadow, the Great Ring, threads to each, the Drowned
Wood, the Mother Tree's brackets in tiers. Live: `walk-to` the owner and the
workbench gift, 874's two retries. Failed at first: the outer ground read as
sand - the rock band, found with the layer shares of #1461. Shipped #1461,
#1462 (the tools) and #1463.

**874 - 2026-09-25, live 3, #1450, chat tasks.** Six minutes of tasks, then
away to the end: a gateway that did not work, a cap "a bit more natural",
landmarks round the forest and pool. Built: the gate fixed and set behind the
landing, body v2, the Mother Tree, the Old Snag, the mycelial web, the ring as
tufts. Failed at first: the gate, a game bug (#1453); rationing the budget
against the whole world, not per record (#1455); two live retries never ran.
Shipped #1448, #1449, #1451-#1453 and #1455-#1458; filed #1454.

**873 - 2026-09-25, live 2, #1443, first session.** The seed gave a purple
airship. Tasks in chat: design your own avatar, flying, with a building area;
save and follow me; clear the region to terrain and water and build it to a
concept. Built: body v1 and the Understory's baseline - land, ground, pool and
sky, the forest, reeds and lilies, the fairy ring, logs and foxfire, the
gateway, threads, spores, pool drips. The owner: "Excellent work." Failed at
first: the harness refused the bulk clear until the owner granted permission
in the terminal; sRGB colours in a texture baked out tan (they are linear).
Shipped #1442 and #1444-#1447; filed #1448 and #1449.

## Open threads

- **Live retries**: none of Hypha's own. #1475 and #1480 waited on the owner's
  browser test (2026-09-26).
- **Open issues**: none of Hypha's own. #1467, the terrain's sheen, was
  decided on 2026-09-27 (reflectance 0.25 in every world) and fixed. The
  parents #1443, #1450, #1460, #1464 and #1474, and #1454 (applied by
  #1463), were closed at the owner's cleanup on 2026-09-27 (#1492).
- **His standing wishes**: improve the region and body continuously, and
  review critically when out of ideas; "there is still space on the periphery
  of the region for adding new concepts" (877); walk the paths and look down.
- **Next improvements**, what 878's review left, in its order (not checked
  since):
  1. From the landing none of the outer places shows, and the pool is a grey
     sheet (water mirrors nothing). 878 added reed beds and a glowing Old
     Snag; look for what could rise over the trees
     ([region.md](../../region.md#backdrop-making-the-far-places-read),
     "Backdrop", has what made far places read before).
  2. The west is backlit: from the pool the Spires, the Windthrow and the Tors
     are silhouettes over tan sheen (#1467); glow reads there, colour does not.
     The terrain's reflectance is 0.25 since 2026-09-27: look again.
  3. The Drowned Wood's snags read as a row from the south shore (not from
     its own thread, so 878 left them).
  4. Short scatters (report of 11:38): `fern_rosette` (placement 149) placed
     170 of 500, `understory_bush` (placement 43) 7 of 16, and the waterline
     `moss_cushion` (placement 192) 238 of 500.
- **Also open**: bare outer ground - a heath over all of it would need about
  50,000 tufts (878): weigh it against the parts first. Known and left: the 52
  rows `--floating-report` still names are gill plates inside the Spires' caps.
