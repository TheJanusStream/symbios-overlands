#!/usr/bin/env python3
"""usage: watch.py SINCE [QUIET_MINUTES] - the admin's channel (see ../chat.md).

Waits on the event log from SINCE and exits when the admin speaks or offers a
gift, anyone arrives or leaves, a follow is blocked, the daemon restarted, or
after a quiet spell (15 minutes by default). Prints every event it saw, a WAKE
line saying why, and NEXT=<seq> to resume from. Run it in the harness's
background mode, not with `&`, so its exit wakes you.

The admin's DID is read from `status` once, so there is nothing to edit.
"""
import datetime
import json
import sys
import time

import agentlib

WAKE_KINDS = ("chat", "gift_offered", "peer_joined", "peer_left", "follow_blocked")


def batch(since, wait):
    answer = agentlib.agent("events", "--since", str(since), "--wait", str(wait))
    if not answer.get("ok"):
        print(f"WATCHER ERROR: {answer.get('error')}\nNEXT={since}")
        sys.exit(1)
    return answer["result"]


def main():
    since = int(sys.argv[1])
    quiet_s = 60 * float(sys.argv[2] if len(sys.argv) > 2 else 15)
    status = agentlib.result(agentlib.agent("status"), "status")
    admin = (status.get("admin") or {}).get("did")
    started, seen, why, settled = time.time(), [], [], False
    while True:
        b = batch(since, 60)
        if b["restarted"]:
            why.append("the daemon restarted: resume from 0")
        if b["missed"]:
            why.append(f"missed {b['missed']} events")
        for e in b["events"]:
            seen.append(e)
            who_did = e.get("from_did", e.get("did"))
            who = "ADMIN" if who_did == admin else ("stranger" if who_did else "self")
            if e["kind"] in WAKE_KINDS:
                why.append(f"{e['kind']} ({who})")
        since = b["next"]
        if why and not settled:  # people type in bursts: gather the next line too
            settled = True
            more = batch(since, 5)
            seen += more["events"]
            since = more["next"]
        if why or time.time() - started > quiet_s:
            for e in seen:
                at = datetime.datetime.fromtimestamp(e["at"]).strftime("%H:%M:%S")
                print(at, json.dumps(e, ensure_ascii=False))
            print("WAKE:", "; ".join(why) or "quiet")
            print(f"NEXT={since}")
            # the exact re-arm: NEXT as it is - NEXT + 1 reads as a daemon restart and replays from 0
            print(f"RE-ARM: {sys.argv[0]} {since} {sys.argv[2] if len(sys.argv) > 2 else 15}")
            return


if __name__ == "__main__":
    main()
