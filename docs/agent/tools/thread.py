#!/usr/bin/env python3
"""usage: thread.py DID RECORD.json NAME OUT_DIR "X,Z X,Z ..." [options]

A glowing thread (a path, a root, a cable) laid on the real ground, as the
Understory's mycelial threads are (see ../region.md, "Long things"): the
waypoints are smoothed into a curve, resampled every --step metres, each
sample set --lift metres above the ground there (read from `render
--terrain-report`), and cut into spines of 16 points (the cap), each starting
where the last ended. The generator is placed unsnapped near the middle of
what it spans, so no point passes the 100 m clamp (the script refuses a
thread that would).

Writes OUT_DIR/NAME.gen.json (for `room set /generators/NAME --file`) and
OUT_DIR/NAME.place.json (an absolute placement to append at /placements/-).

options:
  --radius R          thread radius in metres (default 0.07)
  --material FILE     the spines' material as wire JSON (default: a pale green glow)
  --tail-material F   a second material for the last --tail-m metres (a thread
  --tail-m M          turning the colour of the place it arrives at)
  --lift M            height above the ground (default 0.05)
  --step M            sample spacing (default 2.5)
"""
import json
import math
import os
import sys

import agentlib

GLOW = {"base_color": [5500, 9500, 6500], "emission_color": [2800, 8600, 5000], "emission_strength": 11000}
MAX_POINTS = 16
CLAMP_M = 95.0  # under MAX_PRIM_DIM_M (100 m) with room for the lift


def catmull_rom(pts, per_seg=20):
    """A smooth curve through every waypoint (ends repeated as their own neighbours)."""
    ext = [pts[0]] + pts + [pts[-1]]
    out = []
    for i in range(1, len(ext) - 2):
        p0, p1, p2, p3 = ext[i - 1], ext[i], ext[i + 1], ext[i + 2]
        for k in range(per_seg):
            t = k / per_seg
            t2, t3 = t * t, t * t * t
            out.append(tuple(0.5 * ((2 * p1[j]) + (-p0[j] + p2[j]) * t
                                    + (2 * p0[j] - 5 * p1[j] + 4 * p2[j] - p3[j]) * t2
                                    + (-p0[j] + 3 * p1[j] - 3 * p2[j] + p3[j]) * t3) for j in range(2)))
    out.append(pts[-1])
    return out


def resample(line, step):
    out = [line[0]]
    carry = 0.0
    for a, b in zip(line, line[1:]):
        seg = math.dist(a, b)
        d = step - carry
        while d <= seg:
            t = d / seg
            out.append((a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t))
            d += step
        carry = seg - (d - step)
    if math.dist(out[-1], line[-1]) > step * 0.3:
        out.append(line[-1])
    return out


def ground(did, record, xz):
    args = ["--world", did, "--world-record", record, "--terrain-report"]
    args += [f"--at={x:.2f},{z:.2f}" for x, z in xz]
    out, err = agentlib.render(*args, timeout=300)
    start = out.find("{")
    if start < 0:
        sys.exit(f"no terrain report: {out[-300:]} {err[-300:]}")
    report = json.loads(out[start:])
    return [p["ground_m"] for p in report["points"]], report["water"]["level_m"]


def option(args, name, default=None, conv=str):
    if name in args:
        i = args.index(name)
        value = conv(args[i + 1])
        del args[i:i + 2]
        return value
    return default


def main():
    args = sys.argv[1:]
    radius = option(args, "--radius", 0.07, float)
    material = option(args, "--material", None)
    tail_material = option(args, "--tail-material", None)
    tail_m = option(args, "--tail-m", 0.0, float)
    lift = option(args, "--lift", 0.05, float)
    step = option(args, "--step", 2.5, float)
    if len(args) != 5:
        sys.exit(__doc__)
    did, record, name, out_dir, spec = args
    way = [tuple(float(c) for c in p.split(",")) for p in spec.split()]
    if len(way) < 2:
        sys.exit("need at least two waypoints")
    mat = json.load(open(material)) if material else GLOW
    tail = json.load(open(tail_material)) if tail_material else None

    xz = resample(catmull_rom(way), step)
    heights, water = ground(did, record, xz)
    wet = [(round(x, 1), round(z, 1)) for (x, z), h in zip(xz, heights) if h < water]
    if wet:
        print(f"warning: {len(wet)} samples are under the water ({water} m), e.g. {wet[:4]}")

    xs, zs = [p[0] for p in xz], [p[1] for p in xz]
    ox, oz = (min(xs) + max(xs)) / 2, (min(zs) + max(zs)) / 2
    oy = min(heights)
    far = max(math.hypot(x - ox, z - oz) for x, z in xz)
    if far > CLAMP_M:
        sys.exit(f"the thread reaches {far:.1f} m from its middle ({ox:.1f}, {oz:.1f}): past the 100 m "
                 f"spine clamp - split it into two threads")

    length = sum(math.dist(a, b) for a, b in zip(xz, xz[1:]))
    pts = [[x - ox, h + lift - oy, z - oz] for (x, z), h in zip(xz, heights)]
    # where the tail starts, by distance along the thread
    run, tail_from = 0.0, len(pts)
    if tail and tail_m > 0:
        for i in range(1, len(xz)):
            run += math.dist(xz[i - 1], xz[i])
            if length - run <= tail_m:
                tail_from = i
                break
    spines = []
    i = 0
    while i < len(pts) - 1:
        m = tail if i >= tail_from else mat
        end = min(i + MAX_POINTS - 1, len(pts) - 1)
        if i < tail_from < end:
            end = tail_from  # the colour changes at a spine's joint
        spines.append({"$type": "network.symbios.gen.spine", "material": m, "solid": False,
                       "resolution": 6, "samples_per_segment": 3,
                       "points": [{"position": [int(round(c * 10000)) for c in p],
                                   "radius": int(round(radius * 10000))} for p in pts[i:end + 1]]})
        i = end
    # The 2 cm root stands at the thread's lowest ground, which floats wherever
    # the ground under the origin is lower (session 878's floating report found
    # one): hide it 0.2 m down and lift its spines back by as much.
    root_drop = 2000
    for s in spines:
        for p in s["points"]:
            p["position"][1] += root_drop
    gen = {"$type": "network.symbios.gen.cuboid", "size": [200, 200, 200], "solid": False,
           "material": mat, "transform": {"translation": [0, -root_drop, 0]}, "children": spines}
    place = {"$type": "network.symbios.place.absolute", "avoid_water_clearance": 0, "generator_ref": name,
             "snap_to_terrain": False,
             "transform": {"translation": [int(round(ox * 10000)), int(round(oy * 10000)), int(round(oz * 10000))]}}
    os.makedirs(out_dir, exist_ok=True)
    gp, pp = os.path.join(out_dir, f"{name}.gen.json"), os.path.join(out_dir, f"{name}.place.json")
    json.dump(gen, open(gp, "w"))
    json.dump(place, open(pp, "w"))
    print(f"{name}: {length:.0f} m, {len(pts)} samples, {len(spines)} spines, origin ({ox:.1f}, {oy:.2f}, {oz:.1f}), "
          f"farthest point {far:.0f} m, ground {min(heights):.1f}..{max(heights):.1f} m")
    print(f"wrote {gp} ({len(json.dumps(gen, separators=(',', ':')))} bytes) and {pp}")


if __name__ == "__main__":
    main()
