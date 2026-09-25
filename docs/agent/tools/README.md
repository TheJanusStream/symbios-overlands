# Helper scripts

Ready to run, so a session starts working instead of rewriting them (session
876 rewrote all of these from the docs, as 874 had before it, because they
lived in a scratchpad that is gone at the session's end). Python 3 with
Pillow; they find the repo from their own place in it and use the binaries
built with `--profile test-release`. With more than one session saved,
`export AGENT_ACCOUNT=<the agent's handle>` first: each script adds
`--account` to every command it runs (the flag goes anywhere on the line).

| Script | What it does |
|---|---|
| `watch.py SINCE [QUIET_MINUTES]` | the admin's channel ([../chat.md](../chat.md)): waits, then prints every event, a `WAKE:` line, `NEXT=<seq>` and a `RE-ARM:` line - the exact command to run next (NEXT as it is, never NEXT + 1); reads the admin's DID from `status` |
| `rec.py pull room\|avatar OUT.json` | the saved record as the source to build on |
| `rec.py compose SRC.json EDITS OUT.json` | the edits folded into a copy, for offline renders |
| `rec.py apply room\|avatar EDITS` | each edit sent live, one line per answer: `adjusted_at`, `z_fighting`, `record_size`; a `/-` line is rewritten to where it landed |
| `views.py DID RECORD.json OUT.png "LABEL X Y Z LOOK [DOWN]" ...` | offline pictures from where a person stands, looking a compass way (Y as `~` = the ground there + 1.7 m, `~3` = + 3 m; DOWN tilts below level: 40 sees your feet, negative looks up; a spec `@admin` or `@admincam` sees what the admin sees, from the live `status`); tiled and labelled |
| `stack.py OUT.png PICTURE...` | pictures stacked, for before and after |
| `ground.py DID RECORD.json X,Z ... [--footprint R]` | the ground at each point, one line each: height, above or under the water, slope, downhill, contour yaw, splat layer shares by index |
| `thread.py DID RECORD.json NAME OUT_DIR "X,Z X,Z ..."` | a glowing thread laid on the real ground through waypoints: smoothed, sampled every 2.5 m, cut into 16-point spines, placed unsnapped at its middle (refuses past the 100 m clamp); `--tail-material`/`--tail-m` turn its last metres another colour |
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

## The loop they make

```bash
T=<repo>/docs/agent/tools
$T/rec.py pull room src/room.json                 # after every save, too
python3 builders/piece.py                         # your builder: writes gen/piece.json, place/piece.json
$T/rec.py compose src/room.json edits.txt try.json
$T/views.py <your DID> try.json look.png "pool 6 12 77 270" "close -210 34 58 262"
$T/rec.py apply room edits.txt                    # then look live, then save
```

Session 876 built four landmarks, an outer forest and their paths this way
in about an hour: each piece a builder script, sized with `render
--generator` (it prints `subject size`), placed by the ground's numbers
(`render --terrain-report --at=X,Z --footprint R`), seen offline from the
places people stand, then applied, looked at live, saved.
