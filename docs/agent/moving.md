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

- `status.position` and `status.facing` (world `[x, z]` unit direction) are
  the agent's; `status.peers[]` gives each player `position`, `facing`,
  `distance_m` and `ahead_m` / `right_m` in the agent's own frame.
- `status.nearby[]` names the nearest 12 placed things within 80 m: where
  they are drawn, what kind, and whose name it is (`named_by`).
- `placements --within M` lists the world's things by index with where each
  is drawn - the index `move` and `remove` take.

## Bodies that are not feet

A car or hover-boat turns on the spot before it drives; an airship flies a
`walk-to` and lands on the point, never ending in the air - it climbs over
a building in its way instead of ending `stuck` against it, which makes it
the quickest body for work round a build; an airplane takes
off, lands on a straight final and cannot `follow`. The details, and the
offline stand-ins that let you try each body, are in
[../building.md](../building.md) under "Agent client".
