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
  keeps playing: it runs the code it started with. A change in `src/agent/`
  builds in about 1 min 45 s; a restart (stop, start, `in_world`) takes
  about 10 s, and the watcher dies with the old daemon - re-arm from 0.
- A fix to the command line alone (`src/agent/requests.rs`, `cli.rs`,
  `mod.rs` - how an answer is printed, a new flag the daemon already
  understands) is live with the build: every command is a fresh process.
  Only a daemon-side fix needs the restart below; batch those and restart
  once, at a pause between tasks.
- A build started before your last edit may have read the file half-way
  (rustc reads the sources early): after editing during a build, build
  again before trusting the binary.
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

## Checking that tests test something

Mutation-check every new rule at the end, all in one build: copy the
touched files aside (the work-check hook refuses `git checkout`/`restore`),
put each rule's breakage behind a guard -
`std::env::var("AGENT_MUTANT").as_deref() == Ok("z3")` - build once, run
each mutant's test with `AGENT_MUTANT=<id>` set (it must FAIL), then copy
the files back and compare checksums, and grep that no guard is left.

- A rule with no test surfaces here: write the test, then re-run.
- Writing the missing tests found a real bug (a mirrored part's triangles
  kept their reversed winding, so it was never compared) - they are worth
  writing even when the rule "obviously" works.
- A mutant can survive because another check does the same job for the
  fixture: a 1 mm box prefilter made a plane-distance check untestable with
  faces square to the axes. Give the test a fixture where only the rule
  under test can decide - here the same case turned 30 degrees.
- A mutant that removes only a fast path can be equivalent: break the RULE
  (every check that enforces it), not one line.
- A negated guard needs its parentheses: `!std::env::var(..).as_deref() ==
  Ok("m2")` is `(!var) == Ok(..)` and does not compile; write
  `!(std::env::var("AGENT_MUTANT").as_deref() == Ok("m2"))`.
- A test run with guards in relinks the binaries too. After restoring the
  files, rebuild and check `strings target/test-release/agent | grep -c
  AGENT_MUTANT` is 0 before the next restart of the live daemon.
- Session 874 ran 17 mutants over 7 rules in three builds of about two
  minutes each - cheap enough to do as each rule lands, not only at the end.
