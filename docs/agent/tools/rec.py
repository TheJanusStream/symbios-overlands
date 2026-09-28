#!/usr/bin/env python3
"""Records as scripts (see ../region.md, "Working fast").

  rec.py pull room|avatar OUT.json         `get ""`'s value, the saved source to build on
  rec.py compose SRC.json EDITS OUT.json   fold the edits into a copy, for offline renders
  rec.py apply room|avatar EDITS           send each edit live with `set --file`, one summary
                                           line per answer (adjusted_at, ignored_at, z_fighting,
                                           record_size), a WARNING line naming each key the
                                           record does not have (ignored_at, #1483: dropped) and
                                           one naming what the z-fighting check ran out of time
                                           in (z_fighting_unchecked, #1503: it stops at 3 s);
                                           exits 3 after a CHECK line when any answer was adjusted,
                                           ignored a key, z-fights or left z-fighting unchecked,
                                           so `apply && save` stops there
  rec.py save room|avatar OUT.json [--log LOG "NOTE"] [--hold POINTER]
                                           save, wait for it to land, pull the saved record to
                                           OUT.json (the new source to build on) and, with --log,
                                           append "- HH:MM (seq N) NOTE" to LOG - the time is the
                                           save event's own, never a guess (session 878 logged
                                           guessed times three hours off). --hold keeps a live
                                           trial (a mood change the admin has not judged) out of
                                           the save: the part at POINTER is set back to its value
                                           in OUT.json (the last save you pulled), the rest saved,
                                           then the trial set again, live and unsaved

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


def set_file(record, ptr, value, what):
    """One `set --file`: a value as large as an audio recipe is past what an argument may hold."""
    import tempfile
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as fh:
        json.dump(value, fh)
    try:
        return agentlib.result(agentlib.agent(record, "set", ptr, "--file", fh.name), what)
    finally:
        os.unlink(fh.name)


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
        # the labels are the CHECK line's words: an answer naming dropped keys was still written
        flagged = {"adjusted": 0, "ignored a key": 0, "z-fight": 0, "left z-fighting unchecked": 0}
        for ptr, f, n in edits(ed):
            r = agentlib.result(agentlib.agent(record, "set", ptr, "--file", f), f"set {ptr}")
            zf = r.get("z_fighting") or []
            size = r.get("record_size") or {}
            landed = r.get("pointer") or ptr
            at = f" -> {landed}" if landed != ptr else ""
            ignored = r.get("ignored_at") or []
            flagged["adjusted"] += bool(r.get("adjusted_at"))
            flagged["ignored a key"] += bool(ignored)
            flagged["z-fight"] += bool(zf)
            # the check stops at 3 s and names what it did not finish (#1503): a big grammar reads
            # z_fighting=0 there without having been looked at
            unchecked = r.get("z_fighting_unchecked") or []
            flagged["left z-fighting unchecked"] += bool(unchecked)
            print(f"ok {ptr}{at}: adjusted_at={r.get('adjusted_at') or []} ignored_at={ignored} "
                  f"z_fighting={len(zf)} largest={size.get('largest')} "
                  f"{size.get('bytes')}/{size.get('budget_bytes')}")
            if ignored:
                # a key the record has no field for was dropped without a word (#1483): a guessed
                # texture field name left the default in place
                print(f"   WARNING: {len(ignored)} key(s) the record does not have, dropped: "
                      f"{', '.join(ignored)}")
            for pair in zf[:8]:
                print("   zf", json.dumps(pair))
            if unchecked:
                print(f"   WARNING: z-fighting not checked in time for: {', '.join(unchecked)} - set one "
                      f"generator at a time, or split a grammar too big to finish")
            if ptr.endswith("/-"):
                if r.get("appended") and not landed.endswith("/-"):
                    # Written at once: a later edit that fails exits, and this append is live.
                    rewrite_pointer(ed, n, ptr, landed)
                    print(f"   rewrote {ed} line {n + 1}: {ptr} -> {landed} (a re-run sets it)")
                else:
                    print(f"   WARNING: the answer does not say where {ptr} landed (a daemon "
                          f"older than #1470?); a re-run appends it again")
        if any(flagged.values()):
            # Every edit is live; the exit code only stops an `apply ... && save ...` chain, which saved a
            # sanitiser-raised value and then 2.7 m2 of z-fighting unread in session 883.
            print("CHECK: " + ", ".join(f"{v} edit(s) {k}" for k, v in flagged.items() if v)
                  + " - read the lines above, then save (or fix) on purpose")
            sys.exit(3)
    elif cmd == "save":
        args = sys.argv[2:]
        log = note = None
        if "--log" in args:
            i = args.index("--log")
            if len(args) < i + 3:
                sys.exit("--log takes a LOG file and a NOTE")
            log, note = args[i + 1], args[i + 2]
            del args[i:i + 3]
        hold = None
        if "--hold" in args:
            i = args.index("--hold")
            if len(args) < i + 2:
                sys.exit("--hold takes a POINTER")
            hold = args[i + 1]
            del args[i:i + 2]
        if len(args) != 2:
            sys.exit(__doc__)
        record, out = args
        trial = None
        if hold is not None:
            trial = agentlib.result(agentlib.agent(record, "get", hold), f"{record} get {hold}")["value"]
            saved = json.load(open(out))
            for tok in [t.replace("~1", "/").replace("~0", "~") for t in hold.split("/")[1:]]:
                saved = saved[int(tok)] if isinstance(saved, list) else saved[tok]
            if saved == trial:
                print(f"--hold {hold}: the live value is the saved one; nothing held")
                trial = None
            else:
                set_file(record, hold, saved, f"set {hold} back")
        answer = agentlib.agent("save", record, "--wait")
        if trial is not None:
            # the trial goes back live whatever the save did: it is the admin's to judge
            set_file(record, hold, trial, f"set {hold} again")
            print(f"held {hold} out of the save; the trial is live again, unsaved")
        if not answer.get("ok"):
            # A failed save answers ok: false with the save_failed event as its result.
            sys.exit(f"save {record} FAILED: {answer.get('error')} - nothing pulled, nothing logged")
        event = answer["result"]
        when = datetime.datetime.fromtimestamp(event["at"]).strftime("%H:%M")
        value = agentlib.result(agentlib.agent(record, "get", ""), f"{record} get")["value"]
        if trial is not None:
            # the live record holds the trial again; OUT is the SAVED record, so it takes the value saved
            value = set_ptr(value, hold, saved)
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
