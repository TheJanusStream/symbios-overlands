# Moving

The agent moves as a player does - by the keys, in straight lines, with no
path-finding. A walk ends `arrived` (within about a metre on foot; a car
or a hover-boat at rest within 3 m, or within half the way it set out from
where that is nearer - never under a metre - so a short walk-to still drives
it), `stuck` (no closer for a while: something is in the way - turning nearer
the point counts as getting somewhere too), `halted`, or `replaced`.
Add `--wait` to get the ending as the command's answer.

## Going to a player

"Come over here" is one command (#1456): `A walk-to @handle` walks to
3 m short of where they stand, on the line from you (`--distance` to
change it), then turns to face them, and waits for both - its answer has
the `walk` and the `face` endings. Already that close, it only turns. A
player who is not in the world, or not yet placed (a sleeping tab, never
sent a position), is refused by name rather than walked to.

30 m took 19 s on foot.

Use `follow` when the task is to stay with them ("follow me", an escort);
it runs to catch up and waits for a sleeping tab rather than walking to its
stand-in. An airship escorts 8 m up and lands beside them once they have
stood still for five seconds: a 130 m drive was escorted live and ended
3.5 m from the car. A follow only steers straight at them: with a wall or an
enclosure between you it stops and the watcher wakes, so walk through the gap
first and follow from there (`exports/catoid/b/rejoin.py` does that with a
list of Purlieu's gaps). [tools/companion.py](tools/README.md) goes round by
itself: a follow standing still 4 s short of the admin steps 3 m to one side
and follows again, the other side and further out each time it blocks again
(after six tries it watches them until they move on), and a body that cannot
fly waits at the foot of what the admin has climbed
(live session 20: Catoid against a fallen oak, then the Cat Tree's trunk).

A movement takes up the body the agent wears now (#1851). Live 19: Kairos
followed the owner on foot, wore an airship, and the follow went on pressing
a walker's keys on the landed craft until it said `follow_blocked` 1.5
minutes later. Now a `wear` or a locomotion edit mid-movement begins it again
from where the body is, under the new body: a follow on foot becomes an
escort once the body flies, a walk-to flies, a `face` swings round. What the
new body cannot do ends `body_changed`: a follow in an airplane (which
cannot follow), and a `drive`, whose keys were meant for the car it began
on. No `halt` and `follow` again after a wear.

### A standing follow

"Follow me while you do not need to be somewhere else" (the owner, live 17)
is `follow @handle --standing` (#1852). The follow ends as any follow does -
`peer_left` when they go, a tab asleep two minutes included - but the aim
stays: with nothing else under way the daemon starts it again whenever the
player is in the agent's world, and says `follow_resumed` (the new follow's
`goal_id`). So it starts again when they come back, and after each errand
put in its place - a `walk-to`, a `face`, a `drive`, a `travel` home and back
to a world they are in - with no command. Taken while they are away, it
waits for them (its answer has `goal_id: null` and why it waits).
`status.movement.standing` (`peer`, `keep_m`, `run`) is there whenever one
is set, whatever is under way; with nothing under way `status.movement` is
`doing: "waiting_to_follow"` with the reason in `waiting` (the player is
not in this world; the body is an airplane). Only `halt` ends it (its answer
says `standing_ended`), or another `follow`, which replaces it - even one
refused (a player not in the world, a `--standing` one the body cannot take
up): the refusal ends with `the standing follow of <did> has ended`, and a
follow already under way runs on until it ends but does not start again.
**A restart forgets it**: the daemon keeps no movement across one, so after
a `start` give the `follow --standing` again. It does not follow anyone to
another world: the agent cannot see where a player went.

## Turning

`face` turns a person in short steps, each re-aimed by however far the last
came to rest from the way asked (a slope pushes a step sideways), and ends
`faced` within 10 degrees, `facing_off_deg` in its ending. A beast on four
legs turns to the same 10 degrees (#1820), more slowly: the physics turns
its long body, its nose and tail sweeping no faster than about 4 m/s, so its
steps hold on until it faces the way asked - up to half a turn at that rate
- and a turn may take up to a second longer than a person's (Catoid's
tabby turns at up to about 10 rad/s, a longer horse slower). Each step
holds the walk (up to about 1.4 s for the horse), so a turn walks a little:
a person 0.05-0.11 m, a dog, a cat or the tabby 0.15-0.55 m, the longer
horse 0.4 m for 45 degrees, 0.9 m for 90 and 1.7 m for a half turn. Face
before a narrow approach, not on it. Live 17's `face` on the tabby ended
`stuck` 60 degrees off: a beast let go mid-turn spun on by itself toward a
creep its own spin made, and each step overshot; let go now, a beast stops
turning where it faces, as it stops walking - under the keys in the game
too.

## Getting somewhere

- A straight line into a building ends `stuck`. Route with waypoints round
  it: one `walk-to` per leg.
- **`walk-to` stops about a metre short**, so it cannot line a body up with
  anything narrow: aim each leg a metre PAST its point along the way the body
  travels, or walk it along a wall. Catoid missed the Cat Cottage's 0.64 m
  ramp from in front of it, and walked the Cat Tree's ramp tower corner to
  corner in 8 legs aimed a metre past each corner (live session 20).
- **Solid fences and walls close straight lines.** Once Ashmere's street
  fences were made solid, a walk-to from the green to the gate ended
  `stuck` against a fence twice; one leg per bend of the street, waypoints on
  its centre line, went through in seven `arrived`. Keep a list of a world's
  lanes as waypoints (the builder scripts have them) and walk them.
- **Keep clear of portals and gateways** (`status.nearby[]` with kind
  `portal` or `gateway`): they lead to other worlds. The landing point sits
  right in front of the world's social gateway.
- Step out of anything you are about to build on.
- **A flying walk-to lands on whatever is at the point**, a landmark's top
  included: asked for a spot 11 m from the Spore Spires' centre, the airship
  came down on the spire cluster and its eye-level `look` saw only the tops
  of the stalks (session 878). To look AT a landmark, pick open ground
  beside it - `--terrain-report --at` says what is there - and check
  `status.position`'s height after landing.
- A restart puts the body back at the landing point: walk back before
  looking at what you built.

## Where things are

- `status.position`, `status.facing` (world `[x, z]` unit direction, in
  hundredths - good to about a degree) and `status.heading_deg` (the same as
  a compass bearing, clockwise from north, to the hundredth: aim by it) are
  the agent's; `status.peers[]` gives each player `position`, `facing`,
  `distance_m` and `ahead_m` / `right_m` in the agent's own frame.
- `status.nearby[]` names the nearest 12 placed things within 80 m: where
  they are drawn, what kind, and whose name it is (`named_by`).
- `placements --within M` lists the world's things by index with where each
  is drawn - the index `move` and `remove` take.

## Driving a run: stunts, measured

`drive` (#1527) is for a body on wheels - a car or a hover-boat - driven
at something: a ramp, a bend, a gap. It holds keys segment by segment,
`KEYS@SECONDS` in order (`W@6 W+D@0.4 none@2`: throttle 6 s, throttle and
right 0.4 s, let go 2 s; `none` lets go of everything), and its
`movement_ended` carries a `report` of the run - top speed, distance, how
far it tipped, and each jump: airtime, where it left and landed, how far
and how high, its speed leaving and landing, how fast it came down and its
pitch and roll as it landed. That report is how a stunt is judged: read
the hang time, do not guess it.

- **Line up first, precisely.** A run holds its keys and nothing else: no
  aiming. `walk-to` a point behind the start and then the start itself, so
  the body arrives on the line's axis. A car's walk-to brakes onto its point
  and ends `arrived` at rest within 3 m of it (#1536); before, it let go at
  that circle's edge and rolled 7-11 m on, once to within 9 m of a gateway,
  and Jink's `b/place.py` (short throttle-and-brake moves) was written to
  get round it. A walk-to that sets out nearer than 6 m comes within half
  its way instead, never under a metre (#1866): in live 21 Kairos's Hexapod
  and Roadfactory, at rest 2.97-2.99 m from the teleporter's deck, began
  inside the 3 m circle and each ended a walk-to there `arrived` at once,
  with not a key pressed. One that cannot close in - a wall between - ends
  `stuck` with what it had left. Then turn it to the exact bearing:
  `face` stops within 10 degrees, 10 m off at 60 m, which misses a 6 m
  ramp. Short A/D pulses as `drive` segments (`D@0.05 none@1.2`, then read
  `status.heading_deg`) turn a car at rest about 2.4 degrees a pulse - but
  the keys change only on the daemon's frames, 30 a second, so a 0.05 s
  pulse lasts one frame or two, and one under 33 ms can turn nothing
  (session 893: 24 pulses of 0.027 s left the car where it was). Read the
  heading after every pulse. Jink's
  `exports/jink/b/aim.py BEARING` does it, and `b/trace.py` polls `status`
  through a run for the trajectory itself.
- **Off the ground**, for a car, is what its traction says: none of its
  wheels down, a wheel counting while its ray meets what is below it
  within the suspension's rest length and 15 cm (0.75 m for Jink's
  Cyclecar), and its underside more than 10 cm up. So a car skimming a
  tabletop's deck lower than that lands on it, for the report as for its
  grip. A hover-boat is off the ground while its underside is higher than
  its suspension's rest length and 5 cm. Either for two frames running.
- **A landing on the roof reads level in pitch and roll** - each folds at
  90 degrees - so read the jump's `landing_tilt_deg`: 0 on the wheels, 180
  on the roof.
- **`status.height_m` reads 0.0 only when the body is within a
  centimetre of what is under it** (#1528). Avian opens a contact before
  anything touches - as far out as the body could close in a step, 23 cm
  at 15 m/s - and until #1528 that read 0.0 too: session 893 read one on a
  kicker at speed and took it for the chassis on the ramp, where the bench
  showed Jink's Cyclecar 19 mm clear (the default car 72 mm). A 0.0 at
  speed is now the box on what it rides over. On landing it was real - the
  box hit the ground and the car fell from 15 to 4-6 m/s - until #1524's
  bump stop. Since #1535 the stop lets the box touch in the hardest
  landings rather than throw the car back up, and since #1538 a box that
  meets the terrain slides on: before it, a box touching the ground at
  speed met an edge between the heightfield's cells as a wall and lost
  most of its speed in one step (13.6 to 5.2 m/s, and 15.8 to 1.4 in one
  case).
- A landing that yaws the body sends every later jump off its line: read
  the report's positions before blaming a ramp.
- **Tricks in the air are short presses.** The air keys are strong and
  the air's angular damping weak, so a turn carries on after the key is
  let go. Measured on Jink's Cyclecar off the Moon Shot's 2 s (session
  955, `render --driver` logs):
  - D for 0.25 s yawed the car 48 degrees, and 0.6 s yawed it 126.
  - Q for 1.1 s rolled it right round to land upright.
  - S for 1.0 s took it about 290 degrees round a backflip. It came down
    nose-high on its rear wheels and stayed upright.

  The self-levelling finishes a roll or a flip that is nearly round. A key
  held as the wheels leave counts only once pressed again (Q/E excepted),
  so press a trick's key in the air. A landing's grip swings the car's path
  onto the heading it lands on, at a cost in speed
  ([region.md](region.md#ramps-and-jumps-for-wheels), the hip).

## Bodies that are not feet

A four-legged body (`status.locomotion` reads `four_legged`, see
[avatar.md](avatar.md)) is on feet: it walks a `walk-to` and arrives as a person
does, at its own speeds. Its collider is its own size, so a horse, 0.89 m wide,
does not fit a gap a person passes.

A winged body (`status.locomotion` reads `winged`, see
[avatar.md](avatar.md#a-winged-body)) is on feet too as far as the agent goes:
`walk-to` and `follow` walk it and it arrives as a person does, a bird upright
on two legs, a bat or a glider on four, in a collider of its own size. Under
the keys it flies - Space takes it off, Space beats, no key glides, W and S
pitch, A and D bank, Shift dives - and lands on the top of whatever its path
closes on, a roof or a deck as well as the ground; a glider only leaps and
glides. No command flies it (a walk never presses Space, and `drive` takes
only a body on wheels), but a walk that takes it off a ledge has it open its
wings after a moment's fall, and the walk's W - and Shift, when it runs - then
put its nose down and dive it: walk it round drops. `status.flight` reads its
mode, `ground` while it walks. In water it swims and cannot take off.

A car or hover-boat turns on the spot before it drives, at its own pace -
the default car about 25 degrees a second, so a point behind it takes some
7 s of swinging before it closes (#1541); an airship or a helicopter (one
preset, #1614) flies a
`walk-to` and lands on the point, never ending in the air - it climbs over
a building in its way instead of ending `stuck` against it, which makes it
the quickest body for work round a build. Landed, a rotorcraft rests on its
own weight (#1612): it settles flat on a slope up to 30 degrees, Shift and
the push keys do nothing, A and D still turn it on the spot, and only Space
lifts it off. It lands only at rest - slow over the ground, no key held - and
every set-down (a spawn, a restart, a move to spawn or to a selection, an
arrival in another world) starts it landed, so it comes down onto what is
under it. An airplane flies as a light airplane does (#1615) - it banks to
turn and holds its height hands-off - so it takes off on a run of some 35 m,
lands on a straight final, rolls about 20 m on its brakes, and cannot
`follow`. A seeded one (#1617) sits tail-down on her three wheels and takes
off from them on power alone; the pilot flies her the same way. The
details, and the
offline stand-ins that let you try each body, are in
[../building.md](../building.md) under "Agent client".
