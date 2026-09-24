# Improving the client while it runs

For a live session where the admin gives tasks and the agent improves the
client wherever a task goes badly.

## When a task goes badly

1. Tell the admin in one line what you saw.
2. Find the cause in the code - not in a guess about the code. Reproduce
   offline where you can (`A start --offline`, [README.md](README.md)).
3. Small fix: make it now. Anything big - a new mechanic, a protocol
   change - describe it in chat and file an issue instead of building it
   mid-session.
4. The problem is not always the client: the live z-fighting was the
   agent's own geometry. Fix the instance first (the admin is waiting),
   then ask what the client could have told you - there, `room set` now
   reports z-fighting, because no still picture would have.

## Bringing a fix live

- `cargo build --profile test-release --bin agent` while the old daemon
  keeps playing: it runs the code it started with.
- Before restarting, follow [saving.md](saving.md) (nothing unsaved; tell
  the admin; `stop`; `start`; greet; re-arm from `seq` 0).
- Retry the task that failed, live, and say how it went.

## What each fix needs

- A test that fails without the fix. Check it: undo the fix, watch the test
  fail, restore. A test that still passes with the fix gone tests nothing.
- A record (the tracker's sub-issue): which task exposed it, the cause, the
  fix, the test, the live retry.
- The user-facing sentence in [../building.md](../building.md)'s "Agent
  client" section if the fix changes what a command answers.

## Where things live

| Path | What |
|---|---|
| `src/agent/cli.rs` | the CLI's commands and flags |
| `src/agent/control/` | the socket protocol, the event log (`events.rs`) |
| `src/agent/daemon/serve.rs` | one request answered per frame |
| `src/agent/daemon/status.rs` | `status` |
| `src/agent/daemon/movement/` | walk, follow, face; flight and wings |
| `src/agent/daemon/edit/` | `place`/`move`/`remove`, `room`/`avatar` JSON, undo, save, inventory, `zfight.rs` |
| `src/agent/daemon/look.rs` | `look` |
| `src/agent/daemon/ui/` | `ui`: the game's windows through AccessKit |
| `src/agent/daemon/gifts.rs`, `speech.rs`, `travel.rs` | gifts, `say`, `travel` |

Edit-command tests run in a small app, `edit::harness::app_in(AGENT)`,
the agent standing in its own seeded world. Build test generators from wire
JSON (`serde_json::from_value`) - the same form `room set` takes.

## Traps

- `pgrep -x agent` matches an unrelated system process: match the full
  binary path (`target/test-release/agent run`).
- The first write of a never-saved world re-quantises every generator onto
  the wire's grid, so "what changed" must compare against the old record
  settled the same way.
- The gate at the end is [../../CLAUDE.md](../../CLAUDE.md)'s seven lines
  plus `cargo test --profile test-release --lib` twice - never plain
  `--release`.
