#!/usr/bin/env python3
"""Records as scripts (see ../region.md, "Working fast").

  rec.py pull room|avatar OUT.json         `get ""`'s value, the saved source to build on
  rec.py compose SRC.json EDITS OUT.json   fold the edits into a copy, for offline renders
  rec.py apply room|avatar EDITS           send each edit live with `set --file`, one summary
                                           line per answer (adjusted_at, z_fighting, record_size)

EDITS holds one `POINTER FILE` per line (the file holds the value as JSON,
relative to the EDITS file's folder); `#` starts a comment. A pointer ending
in `/-` APPENDS: apply it once, save, pull the source again, then address what
it added by index - a second apply appends it a second time.
"""
import json
import os
import sys

import agentlib


def edits(path):
    base = os.path.dirname(os.path.abspath(path))
    for line in open(path):
        line = line.split("#", 1)[0].strip()
        if line:
            ptr, f = line.split(None, 1)
            yield ptr, f if os.path.isabs(f) else os.path.join(base, f)


def set_ptr(doc, ptr, value):
    """RFC 6901 set, making an elided empty list (a bench's `children`) as the client does."""
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
        if last == "-":
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
        for ptr, f in edits(ed):
            doc = set_ptr(doc, ptr, json.load(open(f)))
        json.dump(doc, open(out, "w"), indent=1)
        print(f"composed {out}")
    elif cmd == "apply":
        record, ed = sys.argv[2], sys.argv[3]
        for ptr, f in edits(ed):
            r = agentlib.result(agentlib.agent(record, "set", ptr, "--file", f), f"set {ptr}")
            zf = r.get("z_fighting") or []
            size = r.get("record_size") or {}
            print(f"ok {ptr}: adjusted_at={r.get('adjusted_at') or []} z_fighting={len(zf)} "
                  f"largest={size.get('largest')} {size.get('bytes')}/{size.get('budget_bytes')}")
            for pair in zf[:8]:
                print("   zf", json.dumps(pair))
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
