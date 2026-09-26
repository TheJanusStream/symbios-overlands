#!/usr/bin/env python3
"""usage: fan.py DID RECORD.json NAME OUT_DIR CX,CZ [options]

A mycelial fan: where glowing threads reach a place, each forks into a fan of
finer glowing filaments spreading toward and past the place's centre (CX, CZ)
- the network visibly fruiting where it reaches new ground (see
../region.md, "Backdrop"). The arriving threads are found in RECORD: every
placed generator whose children are all spines (what thread.py makes, and
the pool's web) with a spine end within --within metres of the centre. From
each end a cord heads for the centre and forks --depth times (two ways, now
and then one), wandering a little, sampled every 1.2 m on the real ground
(read from `render --terrain-report`), tapering; a branch that would run
under water stops at the shore, and its forks with it.

Writes OUT_DIR/NAME.gen.json (for `room set /generators/NAME --file`) and
OUT_DIR/NAME.place.json (an absolute placement to append at /placements/-,
unsnapped at the centre).

options:
  --colour R,G,B   the place's glow, sRGB 0..1 (default the network's green)
  --strength S     emission strength (default 1.5)
  --within M       how far from the centre a thread end may be (default 36)
  --past M         how far past the centre the fans reach (default 8)
  --depth N        forks after the first cord (default 4: up to 31 branches a thread)
  --radius R       the cord's radius where it starts, m (default 0.05)
  --wander DEG     how far a filament may turn at each 1.2 m step (default 11)
  --seed TEXT      the branching's random seed (default NAME)
"""
import json
import math
import os
import random
import sys

import agentlib

STEP = 1.2      # metres between spine points
LIFT = 0.05     # above the ground
MAX_POINTS = 16


def option(args, name, default=None, conv=str):
    if name in args:
        i = args.index(name)
        value = conv(args[i + 1])
        del args[i:i + 2]
        return value
    return default


def thread_ends(room, centre, within, skip):
    """World (x, z) where each glowing thread arrives within `within` m: one end per thread.

    A thread is a placed generator whose children are all glowing spines and
    which reaches in from outside the circle; one lying wholly inside it (a
    place's own rhizomorphs, a fan already there) is decoration, not an
    arrival. Its arrival is the spine end nearest the centre - a chained
    thread's inner joints are not arrivals.
    """
    gens, ends = room["generators"], []
    dist = lambda x, z: math.hypot(x - centre[0], z - centre[1])  # noqa: E731
    for p in room["placements"]:
        g = p.get("generator_ref", "")
        kids = (gens.get(g) or {}).get("children") or []
        if g == skip or not p.get("$type", "").endswith("absolute"):
            continue
        if not kids or not all(k.get("$type", "").endswith("spine")
                               and (k.get("material") or {}).get("emission_strength") for k in kids):
            continue
        t = [v / 1e4 for v in (p.get("transform") or {}).get("translation", [0, 0, 0])]
        world = lambda q: (t[0] + q["position"][0] / 1e4, t[2] + q["position"][2] / 1e4)  # noqa: E731
        if max(dist(*world(q)) for s in kids for q in s["points"]) <= within:
            continue
        near = min((world(q) for s in kids for q in (s["points"][0], s["points"][-1])), key=lambda e: dist(*e))
        if dist(*near) < within and all(math.hypot(near[0] - f[0], near[1] - f[1]) > 3 for f in ends):
            ends.append(near)
    return ends


def grow(rng, start, heading, length, radius, depth, max_depth, parent, branches, wander):
    n = max(2, int(length / STEP) + 1)
    pts, (x, z), h = [start], start, heading
    for _ in range(n - 1):
        h += math.radians(rng.uniform(-wander, wander))
        x, z = x + math.sin(h) * STEP, z + math.cos(h) * STEP
        pts.append((x, z))
    me = len(branches)
    branches.append({"pts": pts, "r0": radius, "r1": radius * 0.75, "parent": parent})
    if depth >= max_depth:
        return
    spread = rng.uniform(22, 34)
    forks = [-spread, spread] if rng.random() < 0.8 else [rng.choice([-1, 1]) * spread * 0.5]
    for a in forks:
        grow(rng, (x, z), h + math.radians(a + rng.uniform(-6, 6)), length * rng.uniform(0.68, 0.82),
             radius * 0.75, depth + 1, max_depth, me, branches, wander)


def main():
    args = sys.argv[1:]
    colour = [float(c) for c in option(args, "--colour", "0.28,0.86,0.50").split(",")]
    strength = option(args, "--strength", 1.5, float)
    within = option(args, "--within", 36.0, float)
    past = option(args, "--past", 8.0, float)
    depth = option(args, "--depth", 4, int)
    radius = option(args, "--radius", 0.05, float)
    wander = option(args, "--wander", 11.0, float)
    seed = option(args, "--seed", None)
    if len(args) != 5:
        sys.exit(__doc__)
    did, record, name, out_dir, centre = args
    cx, cz = (float(c) for c in centre.split(","))
    room = json.load(open(record))
    rng = random.Random(seed or name)

    ends = thread_ends(room, (cx, cz), within, name)
    if not ends:
        sys.exit(f"no glowing thread ends within {within} m of ({cx}, {cz})")
    branches = []
    for ex, ez in ends:
        d = math.hypot(cx - ex, cz - ez)
        grow(rng, (ex, ez), math.atan2(cx - ex, cz - ez), (d + past) / 3.0, radius, 0, depth, None, branches, wander)

    xz = [p for b in branches for p in b["pts"]] + [(cx, cz)]
    out, err = agentlib.render("--world", did, "--world-record", record, "--terrain-report",
                               *[f"--at={x:.2f},{z:.2f}" for x, z in xz], timeout=300)
    if out.find("{") < 0:
        sys.exit(f"no terrain report: {out[-300:]} {err[-300:]}")
    ground = json.loads(out[out.find("{"):])["points"]
    k = 0
    for b in branches:
        b["g"] = ground[k:k + len(b["pts"])]
        k += len(b["pts"])
    oy = ground[-1]["ground_m"]

    # a branch stops where it would run under water; its forks go with it
    cut = set()
    for i, b in enumerate(branches):
        if b["parent"] in cut:
            cut.add(i)
            b["pts"], b["g"] = [], []
            continue
        dry = next((j for j, g in enumerate(b["g"]) if g.get("under_water_m", 0) > 0.02), len(b["g"]))
        if dry < len(b["pts"]):
            b["pts"], b["g"] = b["pts"][:dry], b["g"][:dry]
            cut.add(i)

    mat = {"base_color": [round(c * 1e4) for c in colour], "emission_color": [round(c * 1e4) for c in colour],
           "emission_strength": round(strength * 1e4), "roughness": 6000}
    root_drop = 0.2  # the root hides under the ground; spine heights are lifted back by as much
    spines = []
    for b in branches:
        n = len(b["pts"])
        for s in range(0, n - 1, MAX_POINTS - 1):
            seg = range(s, min(n, s + MAX_POINTS))
            if len(seg) < 2:
                continue
            spines.append({"$type": "network.symbios.gen.spine", "material": mat, "resolution": 5,
                           "samples_per_segment": 2, "solid": False, "points": [
                               {"position": [round((b["pts"][j][0] - cx) * 1e4),
                                             round((b["g"][j]["ground_m"] + LIFT - oy + root_drop) * 1e4),
                                             round((b["pts"][j][1] - cz) * 1e4)],
                                "radius": round((b["r0"] + (b["r1"] - b["r0"]) * j / max(1, n - 1)) * 1e4)}
                               for j in seg]})
    gen = {"$type": "network.symbios.gen.cuboid", "size": [200, 200, 200], "solid": False,
           "transform": {"translation": [0, -round(root_drop * 1e4), 0]}, "children": spines}
    place = {"$type": "network.symbios.place.absolute", "generator_ref": name, "snap_to_terrain": False,
             "transform": {"translation": [round(cx * 1e4), round(oy * 1e4), round(cz * 1e4)]}}
    os.makedirs(out_dir, exist_ok=True)
    gp, pp = os.path.join(out_dir, f"{name}.gen.json"), os.path.join(out_dir, f"{name}.place.json")
    json.dump(gen, open(gp, "w"))
    json.dump(place, open(pp, "w"))
    print(f"{name}: {len(ends)} arriving threads, {len(branches)} branches ({len(cut)} stopped at water), "
          f"{len(spines)} spines, centre ground {oy:.2f} m")
    size = len(json.dumps(gen, separators=(',', ':')))
    print(f"wrote {gp} ({size} bytes) and {pp}")
    if size > 60000:
        print(f"warning: {size} bytes is over half the 100 KiB record budget - fewer --depth or a smaller --within")


if __name__ == "__main__":
    main()
