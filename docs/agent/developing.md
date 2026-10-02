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
- A fix to the render tool (`src/render_tool/`) is live with its build:
  every render is a fresh process, and the daemon never runs it.
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
- **A daemon-side check can run before any restart**: copy the new binary
  aside and start it as an OFFLINE daemon with its own `XDG_CONFIG_HOME`
  (its stand-in's socket is named for its own DID, so it runs beside the
  live daemon), then `room set` the live world's generators into its
  seeded world as unplaced ones (`/generators/check_<name>`) and read the
  answers. Session 885 checked all 23 of Ashmere's grammar buildings with
  #1503's new z_fighting that way while its critic was still at work,
  found 52 pairs, fixed the grammars and saved - the live daemon never
  restarted. Stop it after, and `pgrep -fa` by its full path.
- Retry the task that failed, live, and say how it went.

## What each fix needs

- A test that fails without the fix. Check it: undo the fix, watch the test
  fail, restore. A test that still passes with the fix gone tests nothing.
- A record (the tracker's sub-issue): which task exposed it, the cause, the
  fix, the test, the live retry.
- The user-facing sentence in [../building.md](../building.md)'s "Agent
  client" section if the fix changes what a command answers.

## Measure a change before making it

A decided change can still have consequences nobody listed. #1454's
decision ("an item is scaled by its root prim") looked like a two-line move
of a number; a probe first - an `#[ignore]` test over 300 seeded worlds, run
with `cargo nextest run ... --run-ignored only --no-capture` - showed it
would have stood 161 pairs of buildings inside each other, a landmark 17 m
over its own gate, because the siting spaced them by the unscaled size. The
probe became the fix's two regression tests. Numbers first, then the change.

## Delegating to a sub-agent

Session 877 handed four self-contained jobs to one background sub-agent at a
time and kept playing meanwhile: a render-tool mode with its tests and
mutants (46 min), a scattered fern iterated offline over dozens of renders
(50 min), an agent-command fix with its tests and a helper-script change
(41 min), a triangle report. The main session spent 5-10k tokens on each
(brief, review, one look live) against the sub-agent's 180-410k, and built
and saved seventeen world changes in the same hours.

**Since 2026-09-28 the pipeline is lean, the owner's decision** after
session 885 checked the same work up to four times: builder -> critic, no
automatic fixer; the critic reads and breaks the riskiest rules itself but
re-runs no gate and copies no repository; sub-agents run at effort `high`
(`medium` for mechanical work), with fmt, clippy and their targeted tests;
the seven-line gate runs once, at the end, in the main session; the end
review covers only code no critic has seen, with one reviewer; mutation
checks only for rules that protect data or security. The rules are in
[session.md](session.md#delegating); the history below is why.

- **What to hand over**: work whose result fits in a few lines - a code fix
  with its test and mutation check, a builder iterated against a brief
  (it returns the JSON's path and ONE picture), a survey, the session-end
  gate. Keep the live daemon, the admin and the choice of what to build.
- **The brief is everything it knows**: the goal and how to tell it is
  done, paths, the issue number, the gates to run, and the traps by name:
  test-release never `--release`; no git stash/checkout (copy aside); no
  em-dashes; commits are the owner's; no `chainlink session`/`quick` (it
  would move the focus - comment on the given issue instead); never touch
  the live daemon (an offline one with its own `XDG_CONFIG_HOME` and
  `XDG_RUNTIME_DIR`, stopped and pgrep-checked).
- **Name the files it may edit, and stay off them** until it reports. A
  sub-agent editing a tool you are running (rec.py) changes it under you:
  copy the tool aside and run the copy (with `PYTHONPATH` at tools/ for
  `agentlib`).
- **Check before bringing it live**: read the key function, re-run its
  tests, `strings` the binary for mutant guards, then restart and retry the
  task live yourself. Sub-agents find more than they were sent for - the
  avatar set's sanitise order, a u16 handle overflow a 65,536-strong scatter
  crashes - so read the whole report and file what it found.
- **Record each delegation's cost and what it saved** on the parent issue.
- **Give it a second pair of eyes.** Session 878 ran each delegation as a
  short pipeline, still one agent at a time: a builder, then an independent
  critic briefed to find what is wrong (not to confirm), then a fixer only
  if the critic found something. In six such pipelines the critic found
  real defects five times, each of which its builder had reported as done
  with every gate green: comments claiming a Bevy panic that cannot happen
  (the shadow fix); long parts over falling ground passing a
  least-clearance rule, and water counted as ground at every placement
  (the floating report); per-copy scale jitter ignored, so jittered plants
  were culled as small, and a cut line sweeping the login screen (the
  culling); and five visual faults each on the spruce and the birch that
  the designers' own pictures showed - glare on level cards, cards hanging
  as planks, the wrong tree's leaves up close, bark marks gone past 5 m.
  Only the parts count came back SOUND. A pipeline cost 0.5-1M sub-agent
  tokens and 50-145 minutes; the main session spent 5-12k on each brief
  and its own check. Brief the critic with the builder's report AND the
  original brief, tell it to break the two most important rules itself,
  and let it write nothing but its verdict. Session 879's oak and apple
  made it six in seven: the critic found oak limbs leaving the trunk as
  sawn-off stubs, a khaki crown in the world's own light, smooth bark and
  apples drawn as split discs, all in pictures the builder had judged; the
  fixer then found the Twig texture caps leaves at 8 (the oak's 11 were
  always drawn as 8). 1.38M sub-agent tokens, 2 h 38 min. Session 883's
  catalogue plants (#1496) made it seven in eight: the builder reproduced
  seven plants exactly and passed the gate; the critic found that the new
  birch, shipped at the iteration cap, grew seeded stands one year past it
  (#1497), five untrue or weak details, and that seeded birch woods read as
  bare poles from 150 m - a question for the owner, not a defect. 1.27M
  sub-agent tokens, 2 h 52 min. A fix outside the fixer's files came back as
  a tested patch for the main session to apply (`patch -p1`; the hook
  refuses `git apply`). The same session's beasts made it eight in nine:
  the builder remade Ashmere's sheep and cattle and judged its own
  pictures; the critic found nine real faults in them, among them cattle
  fore legs starting below the chest with grass showing through, hip
  craters, glossy ball hooves, a grazing ewe's head apart from its neck,
  lying beasts floating and cell sizes misstated; and the
  fixer, fixing those, found a see-through hole in the grazing cow's neck
  (13 inward-facing triangles, found by porting the blob mesher to count
  them). 1.67M sub-agent tokens, 2 h 45 min. Session 885's two pipelines
  (#1503 1.23M tokens, 3 h 27 min; #1505 1.46M, 3 h 24 min) made it ten in
  eleven - the critics found sliver false pairs, a 23 s all-pairs loop,
  lockstep smoke and an untested nested grammar - but each critic re-ran
  the whole gate in its own copy of the repository, and each fixer ran it
  again: since 2026-09-28 the critic reads and tests, and the main session
  decides on a fix round.
- **Check every tool the brief names is safe where the sub-agent sits.**
  The tree brief pointed the builder at `views.py @landingcam`, which then
  looked the admin up through the live daemon for any `@` spec, so a
  sub-agent told never to run the agent binary ran it once (it refused:
  two sessions saved, no `--account`). Fixed in views.py; before briefing,
  run each named tool once the way the sub-agent will, or name only
  offline ones.
- **Review what shipped before closing it.** Session 879 ended with a
  review workflow over its seven code changes: one reviewer per group of
  changes, then an independent verifier per finding told to refute it.
  28 findings, 24 confirmed, in 19 minutes and 1.45M sub-agent tokens. Every
  group had at least one real fault that its own tests and mutants had
  passed, and the worst were the fix missing its own case one step over:
  `ignored_at` (#1483) named a misspelt key but not one inside an object
  that reads back as its default, the very silence it was for; #1484 fixed
  `wear satchel` and left `take-off satchel` answering as before; `render
  --rigged` called the fetched sculpt's own `$type` a misspelt key on every
  avatar from the PDS. A fix's test pins the case that exposed it - ask of
  each fix which sibling (the other verb, the default-valued parent, the
  record as fetched rather than as written) it has not seen. Session 883's
  review (13 findings, all confirmed, in 28 minutes and 2.55M sub-agent
  tokens) found a tool the session had used live wrong in its geometry:
  `thread.py --ride` read the ground under the rut rather than under the
  lane's middle, which a side slope turns into centimetres. The session had
  seen the ruts sit badly and blamed a steep brow, because the fixing
  script printed its largest correction unsigned: the "6.5 cm proud" it
  reported was a buried point raised. Print a correction with its sign.
  Session 885's review was the one that cost too much: 25 agents, 4.05M
  sub-agent tokens and 41 minutes over two delegations that had already
  had their critics, each verifier re-proving one finding with standalone
  copies and benchmarks; it found 20 real faults, 3 of them medium, and its
  fix round then ran builder, critic and fixer again. Since 2026-09-28 the
  end review covers only code no critic has seen, with one reviewer and no
  verifier per finding: the main session reads each finding before acting,
  fixes medium and worse, and files the rest.
- **When a finding is a pattern, grep for its siblings.** The tools
  fixer->verifier pass (301k tokens, 17 min) fixed hedge.py's part count,
  which left out its root, and the verifier found fence.py printing the same
  undercount and region.md repeating it as a fact.
- **Measure a physics change on the game's own ground.** Session 895's
  builder (676k tokens, 3 h 46 min, eight issues) tuned a softer bump stop
  over placed ramps and blocks, where it was right; its critic (307k, about
  67 min) dropped cars onto a heightfield, as the terrain is, and found the
  softer stop let the box reach the ground, where cells' internal edges
  stopped a car dead - 65 of 176 landings for skiffs on the old damping,
  twice HEAD's. The cure was the terrain collider (#1538), not the stop;
  re-measured by the same critic in four minutes, 0 of 176. Brief a physics
  builder to sweep over the terrain's heightfield too.
- **Small fixes need no sub-agent.** With the one slot taken by a long
  visual job, two small fixes (#1476, #1472: a directory rule and a
  per-frame handle cache, each with a failing test and a mutant) took the
  main session about 20 minutes together, restart included - quicker than
  briefing them.

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

- A new strict check can break a tool that round-trips records:
  `render --generator` writing a read value back to diff it panicked on an
  open-union stand-in (#1487). Test the unknown-variant path of anything
  that reads and writes records.
- A value clamped onto a bound that is also its default is left out of the
  written record like every default, so a diff reads it as dropped (session
  879's review, m12).
- `pgrep -x agent` matches an unrelated system process: match the full
  binary path (`target/test-release/agent run`).
- `pgrep -f PATTERN` and `pkill -f PATTERN` inside a compound shell line
  match that shell's own command line, which contains the pattern: a
  `while pgrep -f X` wait never ended, and a `pkill -f` killed the command
  running it (session 878, twice; 879 stopped its own gate script that
  way). Anchor the pattern (`^/full/path/...`), wait on a file the job
  writes instead, or `pgrep -fa` first and `kill` the PIDs.
- With no em-dashes, an aside is set off by ` - `, and a wrapped line that
  starts with `- ` in a `///` or `//!` comment is a Markdown list item:
  clippy's `doc_lazy_continuation` fails the gate on the line after it.
  Rewrap so the dash is not first on a line.
- The first write of a never-saved world re-quantises every generator onto
  the wire's grid, so "what changed" must compare against the old record
  settled the same way.
- The gate at the end is [../../CLAUDE.md](../../CLAUDE.md)'s seven lines
  plus `cargo test --profile test-release --lib` twice - never plain
  `--release`. Run the wasm line early: a new `pub(crate) use` that only
  native code reads (the render tool) is an unused import on wasm, a red
  CI job under `-D warnings` - gate it `#[cfg(not(target_arch = "wasm32"))]`.

## Checking that tests test something

Every fix gets a test that fails without it: undo the fix, watch the test
fail, restore. Since 2026-09-28 (the owner's lean pipeline) a mutation
check is only for a rule that protects data or security - the record's
format, the sanitiser, a cache, a bound. Mutation-check those at the end,
all in one build: copy the
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
- A mutant can be equivalent for a plain reason: #1463's secondary spacing
  bound (x1.10) survived every fixture because the 4-12 m gap already
  absorbs a 10% growth. Keep code that is right by construction, and write
  the equivalence into the issue rather than a test that pretends.
- A test run with guards in relinks the binaries too. After restoring the
  files, rebuild and check `strings target/test-release/agent | grep -c
  AGENT_MUTANT` is 0 before the next restart of the live daemon.
- **Restore with a fresh mtime.** `shutil.copy2` (and `cp -p`) puts back the
  file's OLD modification time, older than the mutant build, so cargo takes
  the mutant binaries as up to date and "rebuild" rebuilds nothing: #1496's
  fixer tried it on a scratch crate and kept a MUTANT binary after a
  0.00 s build. Restore with `shutil.copyfile` or plain `cp`, or `touch`
  the restored files; the `strings` check above is what catches it.
- Session 874 ran 17 mutants over 7 rules in three builds of about two
  minutes each - cheap enough to do as each rule lands, not only at the end.
