# Moving

The agent moves as a player does - by the keys, in straight lines, with no
path-finding. A walk ends `arrived` (within about a metre), `stuck` (no
closer for a while: something is in the way), `halted`, or `replaced`.
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
3.5 m from the car.

## Getting somewhere

- A straight line into a building ends `stuck`. Route with waypoints round
  it: one `walk-to` per leg.
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
  the body arrives on the line's axis, then turn it to the exact bearing:
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
- **`status.height_m` reads 0.0 when the physics has the body on a
  contact - and at speed that includes a speculative one**, which avian
  opens before anything touches (about 23 cm out at 15 m/s, #1528). So a
  0.0 on a kicker at speed is not proof of a scrape: session 893 read one
  and took it for the chassis on the ramp, where the bench showed Jink's
  Cyclecar 19 mm clear (the default car 72 mm). On landing it was real -
  the box hit the ground and the car fell from 15 to 4-6 m/s - until
  #1524's bump stop.
- A landing that yaws the body sends every later jump off its line: read
  the report's positions before blaming a ramp.

## Bodies that are not feet

A car or hover-boat turns on the spot before it drives; an airship flies a
`walk-to` and lands on the point, never ending in the air - it climbs over
a building in its way instead of ending `stuck` against it, which makes it
the quickest body for work round a build; an airplane takes
off, lands on a straight final and cannot `follow`. The details, and the
offline stand-ins that let you try each body, are in
[../building.md](../building.md) under "Agent client".
