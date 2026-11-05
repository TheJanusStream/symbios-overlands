# Edits, undo, saving, restarts

## What is live and what is kept

- An edit to the world is live for everyone in it the moment it is made -
  while the world's whole record is under 900 KiB of compact JSON. Past
  that no world edit reaches anyone live, but a save does: everyone in the
  world fetches the saved world (#1499), so show a change by saving it.
  `room set` answers `live_sync` with `refused` when an edit went nowhere
  live, and `status.editing.live_sync` weighs the world every time (#1500;
  [region.md](region.md), "The record's budget"). An edit to a generator body is live whatever the world
  weighs: it travels as the avatar record alone. A rigged body's sculpt and
  worn items are records of their own that other clients fetch from your
  PDS, so they see a change to those only after a save. Every edit is lost when the daemon
  stops unless it was saved. `status.editing.unsaved` lists the records
  (`room`, `avatar`, `inventory`) holding unsaved edits.
- **Save only when the admin asks for it in chat** ("save", "keep it"),
  even with `--allow-save` - unless the session's mode gives standing leave
  ([session.md](session.md#saving): self-guided and visit do; mood never).
  Then `A save --wait` (the world),
  `A save avatar --wait` or `A save inventory --wait`; the answer's
  `result.kind` is `saved`, or it is `ok: false` with the reason as `error`
  (`save_failed`). An avatar save took about 2 s live. Confirm afterwards
  that `unsaved` is empty, and tell the admin it is kept for every visitor.
- When you report a build, say it is unsaved ("say 'save' to keep it") so
  the admin knows a choice is theirs.

## Undo

- `A undo` / `A redo` step the game's own history (the one Ctrl+Z steps,
  32 steps deep), one step per edit command; `A undo avatar` for the
  avatar. `A revert` throws away every unsaved edit to that record.
- `status.editing.undo` names what the next undo would step.
- Trying something out: write it (an unplaced generator is drawn nowhere),
  read the answer, `undo`. Nothing is left behind.
- **An edit that changes nothing leaves no undo step**: a value the
  sanitiser pulls back to the one already kept (a Fabric `thread_count` of 6
  raised to its floor of 8, where 8 was saved) is no edit to the history, so
  the `undo` after it steps back the edit BEFORE it. Session 883 undid two
  track edits that way. `revert` puts the last save back whole; then pull
  the record and compare it with your source.

## Whole avatars and worlds in the inventory

The inventory keeps whole avatars and whole worlds beside things (#1843);
`A inventory` says what each item `is` (`thing`, `avatar`, `world`).

- `A stash-avatar [name]` and `A stash-world [name]` copy what you wear and
  your own world as they are NOW, unsaved edits included. The copy is an
  inventory edit: unsaved until `A save inventory`, gone at a restart
  before it. `stash-world` works in your own world only.
- **An inventory thing has no `place`** (#1855): `place` takes catalogue
  slugs and the world's own generators. To set one in your own world, `room
  set /generators/<name>` with its tree - keep the builder's output, as `A
  inventory` does not print an item's tree - and append an absolute
  placement (live session 20: the Cat Cottage, given to Catoid).
- `A wear <avatar item>` replaces the avatar you wear and `A load <world
  item>` replaces EVERYTHING in your world - ground, sky, every placement.
  Each is one step of its own history: `A undo avatar` or `A undo` takes it
  back whole, and it is unsaved until `A save avatar` or `A save`. Keep the
  world you are replacing first if it matters: `A stash-world` before the
  `load`, and save the inventory.
- A load from the inventory is not done when it answers: its answer says
  `loading`, and the world lands a moment later - fetched from the account
  unless this session still holds it (`held` in `A inventory`: stashed and
  not yet saved). `A load <item> --wait` prints how
  it ended (`world_loaded`, or `world_load_failed` with the game's reason);
  without `--wait`, read the event. Do not edit the world or save it until
  it has landed. A catalogue world (`A load worlds_...`) lands in the frame
  and says `loaded`.
- **A client built before #1853 dies on a `save inventory` with no world
  in it** (session 1000): Kairos stashed a thing from its plate, saved, and
  the daemon panicked inside the save, taking the unsaved stash with it.
  On such a build, give a thing without saving (`gift give` of a thing
  needs no save) and save the inventory only once the daemon runs the fix.
- **"This sign-in predates a permission ..."** answers `stash-world`, a
  catalogue world's `stash` or a `save inventory` holding a new world item
  on an account signed in before world items existed: their contents are
  records of their own, which that sign-in may not write. Tell the owner in
  the terminal, nothing in chat; signing in again is theirs:
  [session.md](session.md#signing-in-again). Loading a world needs no new
  sign-in.

## Restarts

A restart (`A stop`, then `A start ...`) throws away unsaved edits and puts
the body back at the landing point. Before one:

1. `status.editing.unsaved` must be empty - or the admin has agreed to lose
   those edits, or you can rebuild them from your scripts.
2. Tell the admin in chat ("restarting for a fix, back in ~10 s"): to them
   the agent vanishes and reappears.
3. `stop`'s answer lists what it `discarded`: it should be `[]`. It answers
   `stopping: true` before the daemon has exited: a `start` straight after
   it is refused as "already running" (session 879). Poll `status` until it
   says no agent is running, then give it about 8 s more: the lock outlives
   that answer (session 983). Then start, with the flags from the account's
   page: without them it comes up with `admin: null` and `allow_save: false`.
4. After `start`, poll `status` until `in_world`, greet the admin, and
   re-arm the watcher from `seq` 0 - the new daemon numbers events afresh.

`travel` to another world is refused while the world has unsaved edits,
unless told `--discard-edits` or `--save-edits`.
