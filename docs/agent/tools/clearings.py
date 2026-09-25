#!/usr/bin/env python3
"""usage: clearings.py DID RECORD.json OUT.png X,Z [--dist D] [--only gen,gen,...]

Where do the scattered things stand? Scatters know no clearings (see
../region.md, "Planting: scatters"): a build in a forest can have trees
through it. This renders a copy of RECORD in which every generator a scatter
places is swapped for a short, fat, glowing pole of its own colour (fog pushed
out, clouds off), seen from straight above X,Z from --dist metres (default
90) - compose your build into RECORD first, then move or turn it until no
pole stands inside it. Prints the legend (generator: colour, scatter count).
In the picture +X is right and +Z is down; at --dist 90 about 8 px a metre.
--only limits the poles to the named generators (say, the trees).
"""
import json
import os
import sys
import tempfile

import agentlib

PALETTE = [(1, 0, 0), (0, 0.3, 1), (1, 1, 0), (1, 0, 1), (0, 1, 1), (1, 0.5, 0), (0.5, 1, 0), (1, 1, 1),
           (0.6, 0.3, 1), (0, 1, 0.5)]
NAMES = ["red", "blue", "yellow", "magenta", "cyan", "orange", "lime", "white", "violet", "mint"]


def main():
    args = sys.argv[1:]
    dist, only = "90", None
    if "--dist" in args:
        i = args.index("--dist"); dist = args[i + 1]; del args[i:i + 2]
    if "--only" in args:
        i = args.index("--only"); only = set(args[i + 1].split(",")); del args[i:i + 2]
    if len(args) != 4:
        sys.exit(__doc__)
    did, record, out, at = args
    x, z = (float(c) for c in at.split(","))
    room = json.load(open(record))
    counts = {}
    for p in room["placements"]:
        if p.get("$type", "").endswith("scatter"):
            ref = p["generator_ref"]
            if only is None or ref in only:
                counts[ref] = counts.get(ref, 0) + p.get("count", 0)
    for k, ref in enumerate(sorted(counts)):
        c = PALETTE[k % len(PALETTE)]
        m = {"base_color": [int(v * 10000) for v in c], "emission_color": [int(v * 10000) for v in c],
             "emission_strength": 30000}
        room["generators"][ref] = {"$type": "network.symbios.gen.cylinder", "radius": 12000, "height": 30000,
                                   "resolution": 8, "solid": False, "material": m}
        print(f"{ref}: {NAMES[k % len(NAMES)]} ({counts[ref]} scattered)")
    env = room.setdefault("environment", {})
    env["fog_visibility"] = 50000000
    env["cloud_cover"] = 0
    env["cloud_density"] = 0
    with tempfile.TemporaryDirectory() as tmp:
        copy = os.path.join(tmp, "markers.json")
        json.dump(room, open(copy, "w"))
        rep, _ = agentlib.render("--world", did, "--world-record", copy, "--terrain-report", f"--at={x},{z}")
        ground = json.loads(rep[rep.find("{"):])["points"][0]["ground_m"]
        agentlib.render("--world", did, "--world-record", copy, f"--focus={x},{ground},{z}", "--dist", dist,
                        "--elev", "89", "--yaw", "0", "--out", out)
    if not os.path.exists(out):
        sys.exit("the plan did not render (is the render tool built?)")
    print(out)


if __name__ == "__main__":
    main()
