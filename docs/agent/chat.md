# Chat: the admin's channel

The admin gives tasks in in-game chat and watches the world while you work.
Chat is slow, public and asynchronous: plan for lines crossing yours.

## The watcher

Run this as a background process (so you are woken when it exits) and
re-arm it after every task. It waits on the event log, gathers a follow-up
line or two, then prints everything it saw and the cursor to resume from.

```python
#!/usr/bin/env python3
"""usage: watch.py SINCE [QUIET_MINUTES] - exits on the admin's chat or
gift, anyone arriving or leaving, a daemon restart, or a quiet spell."""
import datetime, json, subprocess, sys, time

AGENT = ["env", "BEVY_ASSET_ROOT=<repo>", "<repo>/target/test-release/agent"]
ACCOUNT = []                             # ["--account", "<handle>"] when two sessions are saved
ADMIN = "did:plc:<the admin's DID>"      # `A status` -> result.admin.did
since = int(sys.argv[1])
quiet_s = 60 * float(sys.argv[2] if len(sys.argv) > 2 else 15)
started, seen, why, settled = time.time(), [], [], False

def batch(wait):
    out = subprocess.run(AGENT + ["events", *ACCOUNT, "--since", str(since), "--wait", str(wait)],
                         capture_output=True, text=True)
    answer = json.loads(out.stdout or "{}")
    if not answer.get("ok"):
        print(f"WATCHER ERROR: {out.stdout} {out.stderr}\nNEXT={since}")
        sys.exit(1)
    return answer["result"]

while True:
    b = batch(60)
    if b["restarted"]: why.append("the daemon restarted: resume from 0")
    if b["missed"]: why.append(f"missed {b['missed']} events")
    for e in b["events"]:
        seen.append(e)
        who = "ADMIN" if e.get("from_did", e.get("did")) == ADMIN else "stranger"
        if e["kind"] in ("chat", "gift_offered", "peer_joined", "peer_left"):
            why.append(f"{e['kind']} ({who})")
    since = b["next"]
    if why and not settled:              # people type in bursts
        settled = True
        more = batch(5)
        seen += more["events"]; since = more["next"]
    if why or time.time() - started > quiet_s:
        for e in seen:
            at = datetime.datetime.fromtimestamp(e["at"]).strftime("%H:%M:%S")
            print(at, json.dumps(e, ensure_ascii=False))
        print("WAKE:", "; ".join(why) or "quiet")
        print(f"NEXT={since}")
        sys.exit(0)
```

- Start it with the harness's background mode, NOT with `&` and its output
  thrown away - a watcher nobody hears is no watcher.
- Resume from `NEXT`, or from a later `seq` if you read events yourself
  meanwhile. A `seq` from before a restart is ahead of the new daemon's
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
- A stranger arriving is `peer_joined` with another DID: carry on, mention
  it once. Their lines arrive as `chat_dropped` (who, never what) and their
  gift offers are declined unread (`gift_declined`).
