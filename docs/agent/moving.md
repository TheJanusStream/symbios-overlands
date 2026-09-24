# Moving

The agent moves as a player does - by the keys, in straight lines, with no
path-finding. A walk ends `arrived` (within about a metre), `stuck` (no
closer for a while: something is in the way), `halted`, or `replaced`.
Add `--wait` to get the ending as the command's answer.

## Going to a player

"Come over here" has no command of its own. `follow @handle` walks to
within 3 m but keeps following until `halt`, so for "come here":

1. Read the player's `position` from `status.peers[]` (skip anyone
   `placed: false` - they have sent no position yet).
2. `walk-to` the point 2 m short of them on the line from you, `--wait`.
3. `face @handle --wait` so you end up facing them, as a person would
   (`already_facing: true` when you are).

30 m took 19 s on foot.

Use `follow` when the task is to stay with them ("follow me", an escort);
it runs to catch up and waits for a sleeping tab rather than walking to its
stand-in.

## Getting somewhere

- A straight line into a building ends `stuck`. Route with waypoints round
  it: one `walk-to` per leg.
- **Keep clear of portals and gateways** (`status.nearby[]` with kind
  `portal` or `gateway`): they lead to other worlds. The landing point sits
  right in front of the world's social gateway.
- Step out of anything you are about to build on.
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
`walk-to` and lands on the point, never ending in the air; an airplane takes
off, lands on a straight final and cannot `follow`. The details, and the
offline stand-ins that let you try each body, are in
[../building.md](../building.md) under "Agent client".
