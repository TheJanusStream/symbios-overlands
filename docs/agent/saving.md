# Edits, undo, saving, restarts

## What is live and what is kept

- Every edit is live for everyone in the world the moment it is made, and
  lost when the daemon stops unless it was saved. `status.editing.unsaved`
  lists the records (`room`, `avatar`, `inventory`) holding unsaved edits.
- **Save only when the admin asks for it in chat** ("save", "keep it"),
  even with `--allow-save`. Then `A save --wait` (the world), `A save avatar`
  or `A save inventory`; the answer is the `saved` event, or `save_failed`
  with a reason. Confirm afterwards that `unsaved` is empty, and tell the
  admin it is kept for every visitor.
- When you report a build, say it is unsaved ("say 'save' to keep it") so
  the admin knows a choice is theirs.

## Undo

- `A undo` / `A redo` step the game's own history (the one Ctrl+Z steps,
  32 steps deep), one step per edit command; `A undo avatar` for the
  avatar. `A revert` throws away every unsaved edit to that record.
- `status.editing.undo` names what the next undo would step.
- Trying something out: write it (an unplaced generator is drawn nowhere),
  read the answer, `undo`. Nothing is left behind.

## Restarts

A restart (`A stop`, then `A start ...`) throws away unsaved edits and puts
the body back at the landing point. Before one:

1. `status.editing.unsaved` must be empty - or the admin has agreed to lose
   those edits, or you can rebuild them from your scripts.
2. Tell the admin in chat ("restarting for a fix, back in ~10 s"): to them
   the agent vanishes and reappears.
3. `stop`'s answer lists what it `discarded`: it should be `[]`.
4. After `start`, poll `status` until `in_world`, greet the admin, and
   re-arm the watcher from `seq` 0 - the new daemon numbers events afresh.

`travel` to another world is refused while the world has unsaved edits,
unless told `--discard-edits` or `--save-edits`.
