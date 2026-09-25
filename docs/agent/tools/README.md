# Helper scripts

Ready to run, so a session starts working instead of rewriting them (session
876 rewrote all of these from the docs, as 874 had before it, because they
lived in a scratchpad that is gone at the session's end). Python 3 with
Pillow; they find the repo from their own place in it and use the binaries
built with `--profile test-release`. With more than one session saved,
`export AGENT_ACCOUNT=<the agent's handle>` first: each script adds
`--account` at the end of every command.

| Script | What it does |
|---|---|
| `watch.py SINCE [QUIET_MINUTES]` | the admin's channel ([../chat.md](../chat.md)): waits, then prints every event, a `WAKE:` line and `NEXT=<seq>`; reads the admin's DID from `status` |
| `rec.py pull room\|avatar OUT.json` | the saved record as the source to build on |
| `rec.py compose SRC.json EDITS OUT.json` | the edits folded into a copy, for offline renders |
| `rec.py apply room\|avatar EDITS` | each edit sent live, one line per answer: `adjusted_at`, `z_fighting`, `record_size` |
| `views.py DID RECORD.json OUT.png "LABEL X Y Z LOOK" ...` | offline pictures from where a person stands, looking a compass way; tiled and labelled |
| `stack.py OUT.png PICTURE...` | pictures stacked, for before and after |
| `wire.py` | import it in a builder: metres to the wire's units, quaternions, `lathe` / `spine` / `sphere` / `mat` / `solid_root` |

An EDITS file holds one `POINTER FILE` line per edit, the file holding the
value as JSON (relative to the EDITS file). A pointer ending `/-` appends:
apply it once, save, pull the source again, and address what it added by
index from then on.

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
