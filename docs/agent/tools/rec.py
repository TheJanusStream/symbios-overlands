#!/usr/bin/env python3
"""Records as scripts (see ../region.md, "Working fast").

  rec.py pull room|avatar OUT.json         `get ""`'s value, the saved source to build on
  rec.py compose SRC.json EDITS OUT.json   fold the edits into a copy, for offline renders
  rec.py apply room|avatar EDITS           send each edit live with `set --file`, one summary
                                           line per answer (adjusted_at, z_fighting, record_size)
  rec.py save room|avatar OUT.json [--log LOG "NOTE"]
                                           save, wait for it to land, pull the saved record to
                                           OUT.json (the new source to build on) and, with --log,
                                           append "- HH:MM (seq N) NOTE" to LOG - the time is the
                                           save event's own, never a guess (session 878 logged
                                           guessed times three hours off)

EDITS holds one `POINTER FILE` per line (the file holds the value as JSON,
relative to the EDITS file's folder); `#` starts a comment. A pointer ending
in `/-` APPENDS. `apply` rewrites that line of EDITS in place to the index the
answer says it landed at (`/placements/-` becomes `/placements/150`) and says
so, so a re-run sets it instead of appending a second copy; the rest of the
file is left as it was. A daemon older than #1470 does not say where; `apply`
then warns and leaves the line alone.
"""
import datetime
import json
import os
import sys

import agentlib


def edits(path):
    """Each edit as (pointer, value file, the line's index in the file). Read whole first:
    `apply` rewrites the file as it goes."""
    base = os.path.dirname(os.path.abspath(path))
    with open(path) as fh:
        lines = fh.readlines()
    for n, line in enumerate(lines):
        line = line.split("#", 1)[0].strip()
        if line:
            ptr, f = line.split(None, 1)
            yield ptr, f if os.path.isabs(f) else os.path.join(base, f), n


def rewrite_pointer(path, n, ptr, landed):
    """Line n of the EDITS file with its pointer `ptr` changed to `landed`; every other byte kept."""
    with open(path, newline="") as fh:
        lines = fh.readlines()
    line = lines[n]
    lead = len(line) - len(line.lstrip())
    if not line[lead:].startswith(ptr):
        raise SystemExit(f"{path} line {n + 1} no longer starts with {ptr}; not rewritten")
    lines[n] = line[:lead] + landed + line[lead + len(ptr):]
    with open(path, "w", newline="") as fh:
        fh.writelines(lines)


def set_ptr(doc, ptr, value):
    """RFC 6901 set as the client does it: `-` or the index one past the end appends, and an
    elided empty list (a bench's `children`) is made."""
    if ptr == "":
        return value
    toks = [t.replace("~1", "/").replace("~0", "~") for t in ptr.split("/")[1:]]
    cur = doc
    for i, t in enumerate(toks[:-1]):
        nxt = cur[int(t)] if isinstance(cur, list) else cur.get(t)
        if nxt is None:
            nxt = [] if toks[i + 1] == "-" or toks[i + 1].isdigit() else {}
            if isinstance(cur, list):
                cur[int(t)] = nxt
            else:
                cur[t] = nxt
        cur = nxt
    last = toks[-1]
    if isinstance(cur, list):
        if last == "-" or int(last) == len(cur):
            cur.append(value)
        else:
            cur[int(last)] = value
    else:
        cur[last] = value
    return doc


def main():
    cmd = sys.argv[1] if len(sys.argv) > 1 else ""
    if cmd == "pull":
        record, out = sys.argv[2], sys.argv[3]
        value = agentlib.result(agentlib.agent(record, "get", ""), f"{record} get")["value"]
        json.dump(value, open(out, "w"), indent=1)
        print(f"pulled {record} -> {out} ({os.path.getsize(out)} bytes)")
    elif cmd == "compose":
        src, ed, out = sys.argv[2:5]
        doc = json.load(open(src))
        for ptr, f, _ in edits(ed):
            doc = set_ptr(doc, ptr, json.load(open(f)))
        json.dump(doc, open(out, "w"), indent=1)
        print(f"composed {out}")
    elif cmd == "apply":
        record, ed = sys.argv[2], sys.argv[3]
        for ptr, f, n in edits(ed):
            r = agentlib.result(agentlib.agent(record, "set", ptr, "--file", f), f"set {ptr}")
            zf = r.get("z_fighting") or []
            size = r.get("record_size") or {}
            landed = r.get("pointer") or ptr
            at = f" -> {landed}" if landed != ptr else ""
            print(f"ok {ptr}{at}: adjusted_at={r.get('adjusted_at') or []} z_fighting={len(zf)} "
                  f"largest={size.get('largest')} {size.get('bytes')}/{size.get('budget_bytes')}")
            for pair in zf[:8]:
                print("   zf", json.dumps(pair))
            if ptr.endswith("/-"):
                if r.get("appended") and not landed.endswith("/-"):
                    # Written at once: a later edit that fails exits, and this append is live.
                    rewrite_pointer(ed, n, ptr, landed)
                    print(f"   rewrote {ed} line {n + 1}: {ptr} -> {landed} (a re-run sets it)")
                else:
                    print(f"   WARNING: the answer does not say where {ptr} landed (a daemon "
                          f"older than #1470?); a re-run appends it again")
    elif cmd == "save":
        args = sys.argv[2:]
        log = note = None
        if "--log" in args:
            i = args.index("--log")
            if len(args) < i + 3:
                sys.exit("--log takes a LOG file and a NOTE")
            log, note = args[i + 1], args[i + 2]
            del args[i:i + 3]
        if len(args) != 2:
            sys.exit(__doc__)
        record, out = args
        answer = agentlib.agent("save", record, "--wait")
        if not answer.get("ok"):
            # A failed save answers ok: false with the save_failed event as its result.
            sys.exit(f"save {record} FAILED: {answer.get('error')} - nothing pulled, nothing logged")
        event = answer["result"]
        when = datetime.datetime.fromtimestamp(event["at"]).strftime("%H:%M")
        value = agentlib.result(agentlib.agent(record, "get", ""), f"{record} get")["value"]
        json.dump(value, open(out, "w"), indent=1)
        print(f"saved {record} at {when} (seq {event.get('seq')}); pulled -> {out} "
              f"({os.path.getsize(out)} bytes)")
        if log:
            with open(log, "a") as fh:
                fh.write(f"- {when} (seq {event.get('seq')}) {note}\n")
            print(f"logged to {log}")
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
