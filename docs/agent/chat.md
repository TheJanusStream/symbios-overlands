# Chat: the admin's channel

The admin gives tasks in in-game chat and watches the world while you work.
Chat is slow, public and asynchronous: plan for lines crossing yours.

## The watcher

`tools/watch.py SINCE [QUIET_MINUTES]` ([tools/](tools/README.md)) waits on
the event log and exits when the admin speaks or offers a gift, anyone comes
or goes, a follow is blocked, or the daemon restarted - after gathering a
follow-up line or two, since people type in bursts. It prints every event it
saw, a `WAKE:` line saying why, and `NEXT=<seq>` to resume from. It reads the
admin's DID from `status`, so nothing needs editing; with two sessions saved,
`export AGENT_ACCOUNT=<handle>` first.

```bash
AGENT_ACCOUNT=hypha-ai.bsky.social <repo>/docs/agent/tools/watch.py 0 30
```

- Start it with the harness's background mode, NOT with `&` and its output
  thrown away - a watcher nobody hears is no watcher.
- Resume from `NEXT`, or from a later `seq` if you read events yourself
  meanwhile. `NEXT` is the last seq already read: pass it as it is, never
  `NEXT + 1` - a `since` ahead of the log reads as a daemon restart, and the
  watcher replays every event from 0 (session 877, twice). Its last line,
  `RE-ARM:`, is the exact command to run next: copy it rather than compute. A `seq` from before a restart is ahead of the new daemon's
  numbering: the log says `restarted` and starts again from 0.
- A quiet spell exit (15 minutes by default) is a check-in, not an alarm:
  look at `status.peers` before saying anything. An admin who is waiting on
  your work is quiet and still `placed`, not `quiet`. When the admin has
  said "keep going", arm it with a longer spell (`watch.py N 30`) and work
  on between wakes.
- A `say` answer's `delivery` says who heard it: `{"reached": N}` connected
  players, `"nobody_here"` (alone - the admin has left), or
  `"not_connected"` (your own link is down; the line never left).

## Crossed lines

The admin keeps typing while you work. Seen live: a request ("match the
colouring of the side panels and the top panel"), then 33 s later the
preference that decided it ("the side panels look too bright and yellow") -
the agent acted on the first alone, matched the wrong way, and had to redo
it. So:

- Before acting on a line, and again before answering, read
  `A events --since <last seq>`: the next line may change the task.
- When a line arrives after you have changed something, compare its `at`
  with when your change went live. A complaint sent before your fix is about
  the old version - say that the lines crossed and that the fix should cover
  it, rather than fixing it twice.
- A correction line ("*and", "I meant...") belongs to the line before it.

## "Here in front of me"

The admin points by where they stand ("here in front of me is something that
is probably supposed to look like a dead tree"). Their `status.peers[]`
position and facing are enough: `tools/views.py DID RECORD OUT "@admin"`
renders what their eyes see and `"@admincam"` what their screen shows (the
camera behind them). Session 877 found a scattered dead tree that way in
one render - 40 copies, all with floating limbs - and had the fix live in
four minutes.

## Answering

- One or two lines per answer; 512 characters a line at most, no newlines.
  More than 8 lines at once and every peer drops the extras, then 1 a second
  (`say` refuses and says how long to wait). Longer detail goes to the
  operator's terminal: say "details in the terminal".
- Say what you did, how it went (well, or what failed and why), and what is
  left or unsaved. "It's live but unsaved - say 'save' to keep it."
- For a task that will take minutes, say what you are about to do first
  ("On it: ..."). Silence reads as nothing happening.
- Everything said is public. No paths, keys, log lines.

## Presence

- `status.peers[]`: `placed` false = never sent a position (a sleeping tab);
  `quiet` true = nothing heard lately. A backgrounded tab is swept after two
  minutes (`peer_left`) and returns on its first packet (`peer_joined`).
- If the admin leaves or goes quiet, say so once and wait - or, when they
  said to carry on without them ("don't let that stop you"), keep working
  and save as they allowed. Lines said while they are away reach nobody
  (`delivery: nobody_here`) and are not kept for them: write a two-line
  summary as you go and say it when they come back (`peer_joined`).
- A standing "follow me for the session" ends when the admin leaves (the
  follow's `movement_ended` says `peer_left`): start it again on their
  `peer_joined`. Break it off only to stand somewhere that shows your work
  better, then `walk-to @admin` (one command back, facing them) and
  `follow` again. `halt` first: a new movement replaces the follow anyway,
  but the halt's answer says what was stopped.
- A stranger arriving is `peer_joined` with another DID: carry on, mention
  it once. Their lines arrive as `chat_dropped` (who, never what) and their
  gift offers are declined unread (`gift_declined`).
