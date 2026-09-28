# Helper scripts

Ready to run, so a session starts working instead of rewriting them (session
876 rewrote all of these from the docs, as 874 had before it, because they
lived in a scratchpad that is gone at the session's end). Python 3 with
Pillow; they find the repo from their own place in it and use the binaries
built with `--profile test-release`. With more than one session saved,
start each call with `AGENT_ACCOUNT=<the agent's handle>`: each script adds
`--account` to every command it runs (the flag goes anywhere on the line).
While a sub-agent works on the render tool, copy the binary aside and start
each call with `AGENT_RENDER=<the copy>` as well: the scripts then use it,
and a rebuild (with mutant guards in, or half-linked) does not change the
tool under you. Do not `export` either where each command runs in a fresh
shell ([../session.md](../session.md#starting)).

| Script | What it does |
|---|---|
| `watch.py SINCE [QUIET_MINUTES]` | the admin's channel ([../chat.md](../chat.md)): waits, then prints every event, a `WAKE:` line, `NEXT=<seq>` and a `RE-ARM:` line - the exact command to run next (NEXT as it is, never NEXT + 1); reads the admin's DID from `status` |
| `rec.py pull room\|avatar OUT.json` | the saved record as the source to build on |
| `rec.py compose SRC.json EDITS OUT.json` | the edits folded into a copy, for offline renders |
| `rec.py save room\|avatar OUT.json [--log LOG "NOTE"] [--hold POINTER]` | save, wait, pull the saved record as the new source and, with `--log`, append `- HH:MM (seq N) NOTE` with the save event's OWN time; a refused save exits 1 and pulls and logs nothing; `--hold` keeps a live mood trial the admin has not judged out of the save (the part at POINTER set back to OUT.json's value, the rest saved, the trial set again) |
| `rec.py apply room\|avatar EDITS` | each edit sent live, one line per answer: `adjusted_at`, `ignored_at`, `z_fighting`, `record_size`, a `WARNING` line naming each key the record does not have (`ignored_at`, #1483: a misspelled or guessed field, dropped) and one naming what the z-fighting check did not finish (`z_fighting_unchecked`: the daemon's check stops at 3 s, #1503; a generator, or a placement whose grammar seed draws it, #1505); a `/-` line is rewritten to where it landed; it exits 3 after a `CHECK:` line when any answer was adjusted, ignored a key, z-fights or left z-fighting unchecked, so `apply ... && save ...` stops for you to read them (session 883 saved a raised value and 2.7 m2 of z-fighting unread that way) |
| `views.py DID RECORD.json OUT.png "LABEL X Y Z LOOK [DOWN]" ...` | offline pictures from where a person stands, looking a compass way or at a point (LOOK `TX,TZ`) (Y as `~` = the ground there + 1.7 m, `~3` = + 3 m; DOWN tilts below level: 40 sees your feet, negative looks up; a spec `@admin` or `@admincam` sees what the admin sees, from the live `status`; `@landing` and `@landingcam` what an arriving visitor's eyes and screen show, from the record's `default_landing`); tiled and labelled |
| `stack.py OUT.png PICTURE...` | pictures stacked, for before and after |
| `compare.py DID SRC.json EDITS OUT.png "SPEC" ...` | before and after in one command: EDITS composed into a copy of SRC, each views.py SPEC rendered from both, one row per spec (source left, edited right) |
| `ground.py DID RECORD.json X,Z ... [--footprint R] [--lay YAW]` | the ground at each point, one line each: height, above or under the water, slope, downhill, contour yaw, splat layer shares by index; `--lay YAW` adds the wire `rotation` that lays a flat thing (a bed, a pool) on the ground there turned YAW degrees clockwise seen from above, as `place --yaw` and the printed contour yaw count it (90 faces local -Z to +X; on level ground exactly `place --yaw YAW`; at the contour yaw its local +X runs along the contour) - a level one on a slope stands proud downhill |
| `thread.py DID RECORD.json NAME OUT_DIR "X,Z X,Z ..."` | a glowing thread laid on the real ground through waypoints: smoothed, sampled every 2.5 m, cut into 16-point spines, placed unsnapped at its middle (refuses past the 100 m clamp); `--tail-material`/`--tail-m` turn its last metres another colour; `--flat F` squashes it into a lane or path (half width `--radius`, its TOP `--lift` over LEVEL ground - on a grade g along it the top stands about `radius x (sqrt(F^2 + g^2) - F)` higher, 0.14 m at 10% for `--radius 2.5 --flat 0.06`; refuses when a point it writes, divided by F from its hidden root, passes the clamp and says how far to flatten instead), `--resolution` its sides, `--taper-start`/`--taper-end` narrow its ends into the ground (to join lanes by overlapping, or fade one out); `--solid` makes the drawn spines collide (with a short `--segment`, 4-5 points, so each hull hugs the ground), and `--collider N` instead keeps the drawn lane long and not solid and adds a hidden solid copy under it, one part per N points (Ashmere's lanes use `--collider 5`) - split a long lane rather than flatten it less (a larger `--flat`) to pass the clamp: a thicker lens z-fought its own collider on a steep stretch (session 883); `--ride R,F,L[,OFF]` lays a thin thread (a rut, a verge) ON a flat lane of radius R, flatness F and lift L whose middle is OFF metres to the thread's left as it runs (negative: its right; under R), `--lift` then its clearance over the lane's top: it reads the ground under the lane's middle and takes the lens's fall on the local grade, and lands within about 1 cm (`test_tools.py` meshes it as the engine does) - before session 883's review it read the ground under itself and left Ashmere's ruts from 6 cm under the street's top to 5 cm over it, which the account's `b/ride_fix.py` (the lane's drawn mesh rebuilt as the engine sweeps it) then seated; where two lanes overlap it sees only one; its hidden root goes under the ground at its origin |
| `fence.py DID RECORD.json NAME OUT_DIR "X,Z X,Z ... gap X,Z ..."` | a fence on the real ground: panels (`--panel` m, default 3) each one cuboid tilted to the ground between its ends and sunk `--sink`, alternate panels 1.2 cm thicker and 1.5 cm taller so neighbours never share a face, `--stakes` at the joints, `gap` ends a run (a gate); wattle hurdles by default, any `--material` |
| `hedge.py DID RECORD.json NAME OUT_DIR "X,Z X,Z ..."` | a hedgerow (or any low green run) on the real ground: one soft ellipsoid every `--step` m, long along the run, sunk into the ground, gathered 16 to a BlobGroup (one drawn part per ~28 m at the default step, plus its hidden root, drawn too: a 30 m hedge is three parts), sizes jittered; `--height`, `--width`, `--material`, `--smooth` |
| `fan.py DID RECORD.json NAME OUT_DIR CX,CZ [--colour R,G,B]` | where glowing threads reach a place, each forks into a fan of finer filaments toward and past its centre, on the real ground, stopping at the shore; one arrival per thread (a place's own spines lying wholly inside `--within` are not arrivals); warns past half the record budget |
| `near.py RECORD.json X,Z ... [--within R] [--box L,W,YAW]` | what already stands within R m (default 8) of each point, nearest first, by the placements' origins (a scatter with its radius, or a rect scatter's half sizes) - before setting a thing by hand; no render. `--box` measures from a footprint instead: X,Z its centre, L along its own X, W along its own Z, turned YAW clockwise as `place --yaw` counts it; each distance is from the footprint's edge (0 inside), and a scatter reaching into it says by how much (`reaches N m in`) - session 885's point reading passed a 9 m cart lodge 8.6 m from an apple garth's centre while its end stood 3.3 m inside the garth |
| `clearings.py DID RECORD.json OUT.png X,Z [--dist D] [--only gen,...]` | where scattered things stand: each scattered generator swapped for a glowing pole of its own colour, seen from straight above - compose your build in and check no pole is inside it |
| `wire.py` | import it in a builder: metres to the wire's units, quaternions, `lathe` / `spine` / `sphere` / `mat` / `solid_root` |

An EDITS file holds one `POINTER FILE` line per edit, the file holding the
value as JSON (relative to the EDITS file). A pointer ending `/-` appends,
and the answer names where it landed (#1470): `apply` rewrites that line of
EDITS in place to it (`/placements/-` becomes `/placements/150`, the rest of
the file untouched) and prints `rewrote`, so a re-run sets it instead of
adding a second copy. `compose` takes the index one past the end as an
append, so a source pulled before the save still composes. A daemon older
than #1470 does not say where; `apply` then prints a WARNING and leaves the
line alone.

`test_tools.py` tests them offline - no agent, daemon or render tool:
`python3 -m unittest discover -s docs/agent/tools -p 'test_*.py'`. No gate
runs it: run it after changing a tool. Its `thread.py` tests mesh what the
tool writes as the engine sweeps a spine (a port of
`src/world_builder/prim/sweeps.rs`), so a change there needs the port
changed too (session 883).

## The loop they make

```bash
T=<repo>/docs/agent/tools
$T/rec.py pull room src/room.json                 # after every save, too
python3 builders/piece.py                         # your builder: writes gen/piece.json, place/piece.json
$T/rec.py compose src/room.json edits.txt try.json
$T/views.py <your DID> try.json look.png "pool 6 12 77 270" "close -210 34 58 262"
$T/rec.py apply room edits.txt                    # then look live, then:
$T/rec.py save room src/room.json --log log.md "what was kept"   # save, re-pull, timed log line
```

Session 876 built four landmarks, an outer forest and their paths this way
in about an hour: each piece a builder script, sized with `render
--generator` (it prints `subject size`), placed by the ground's numbers
(`render --terrain-report --at=X,Z --footprint R`), seen offline from the
places people stand, then applied, looked at live, saved.
