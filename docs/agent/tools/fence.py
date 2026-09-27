#!/usr/bin/env python3
"""usage: fence.py DID RECORD.json NAME OUT_DIR "X,Z X,Z ..." [options]

A fence laid on the real ground (a wattle hurdle fence by default; any material by --material):
the waypoints are joined by straight runs, cut into panels about --panel metres long, and each
panel is ONE cuboid from its start to its end, turned along the run and tilted to the ground's
slope between its two ends (read from `render --terrain-report`), sunk --sink below it so no gap
shows where the ground dips between. A stake stands at every panel end when --stakes is given.
A run can leave a gap: a waypoint written `gap` ends the fence there and the next point starts it
again (a toft's gate). The generator is placed unsnapped near the middle of what it spans (it
refuses past the 100 m clamp, as thread.py does).

Writes OUT_DIR/NAME.gen.json and OUT_DIR/NAME.place.json (an absolute placement).

options:
  --height H      fence height above the ground (default 1.15)
  --thick T       panel thickness (default 0.09)
  --panel P       panel length, m (default 3.0)
  --sink M        how far each panel runs under the ground (default 0.35)
  --stakes        a stake at every panel end
  --solid         the panels collide: people walk round the fence and through its gaps
  --material FILE the panels' material as wire JSON (default: woven hazel wattle)
"""
import json
import math
import os
import sys

import agentlib
from thread import ground, option, CLAMP_M


def lin(c):
    return ((c + 0.055) / 1.055) ** 2.4


def w(v):
    return int(round(v * 10000))


WATTLE = {"base_color": [10000, 10000, 10000], "roughness": 9500, "uv_scale": 22000,
          "texture": {"$type": "Thatch", "density": 60000, "anisotropy": 30000, "layer_count": 60000,
                      "layer_shadow": 6000, "color_straw": [w(lin(c)) for c in (0.42, 0.34, 0.24)],
                      "color_shadow": [w(lin(c)) for c in (0.20, 0.16, 0.11)], "seed": 121}}
STAKE = {"base_color": [w(c) for c in (0.30, 0.23, 0.15)], "roughness": 9000}


def quat(yaw, pitch):
    """Yaw about +Y, then pitch about the panel's own +Z... composed as q = qy * qz."""
    cy, sy = math.cos(yaw / 2), math.sin(yaw / 2)
    cz, sz = math.cos(pitch / 2), math.sin(pitch / 2)
    # qy = (0, sy, 0, cy); qz = (0, 0, sz, cz); qy*qz
    return [sy * sz, sy * cz, cy * sz, cy * cz]


def main():
    args = sys.argv[1:]
    height = option(args, "--height", 1.15, float)
    thick = option(args, "--thick", 0.09, float)
    panel = option(args, "--panel", 3.0, float)
    sink = option(args, "--sink", 0.35, float)
    material = option(args, "--material", None)
    stakes = "--stakes" in args
    if stakes:
        args.remove("--stakes")
    solid = "--solid" in args
    if solid:
        args.remove("--solid")
    if len(args) != 5:
        sys.exit(__doc__)
    did, record, name, out_dir, spec = args
    mat = json.load(open(material)) if material else WATTLE
    runs, cur = [], []
    for tok in spec.split():
        if tok == "gap":
            if len(cur) >= 2:
                runs.append(cur)
            cur = []
        else:
            cur.append(tuple(float(c) for c in tok.split(",")))
    if len(cur) >= 2:
        runs.append(cur)
    if not runs:
        sys.exit("need at least one run of two waypoints")
    # panel ends along every run
    ends = []
    for run in runs:
        pts = []
        for a, b in zip(run, run[1:]):
            n = max(1, round(math.dist(a, b) / panel))
            for k in range(n + (1 if b == run[-1] else 0)):
                t = k / n
                pts.append((a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t))
        ends.append(pts)
    flat = [p for r in ends for p in r]
    xs, zs = [p[0] for p in flat], [p[1] for p in flat]
    ox, oz = (min(xs) + max(xs)) / 2, (min(zs) + max(zs)) / 2
    heights, water = ground(did, record, flat + [(ox, oz)])
    origin_ground = heights.pop()
    oy = min(heights)
    far = max(math.hypot(x - ox, z - oz) for x, z in flat)
    if far > CLAMP_M:
        sys.exit(f"the fence reaches {far:.1f} m from its middle: past the 100 m clamp - split it")
    hmap = dict(zip(flat, heights))
    parts = []
    for r in ends:
        for k, (a, b) in enumerate(zip(r, r[1:])):
            ha, hb = hmap[a], hmap[b]
            dx, dz = b[0] - a[0], b[1] - a[1]
            run_len = math.hypot(dx, dz)
            length = math.sqrt(run_len ** 2 + (hb - ha) ** 2) + 0.04
            yaw = math.atan2(-dz, dx)
            pitch = math.atan2(hb - ha, run_len)
            tall = height + 0.015 * (k % 2)      # and 1.5 cm taller and deeper: their tops (and buried
            deep = sink + 0.015 * (k % 2)        # bottoms) met within a fraction of a mm
            mid = ((a[0] + b[0]) / 2 - ox, (ha + hb) / 2 + (tall - deep) / 2 - oy, (a[1] + b[1]) / 2 - oz)
            # neighbours overlap 4 cm at a joint: every other panel is 1.2 cm thicker, so two panels in
            # line never share a face (they z-fought, 80 cm2 a joint)
            parts.append({"$type": "network.symbios.gen.cuboid", "size": [w(length), w(tall + deep), w(thick + 0.012 * (k % 2))],
                          "solid": solid, "material": mat,
                          "transform": {"translation": [w(c) for c in mid], "rotation": [w(c) for c in quat(yaw, pitch)]}})
        if stakes:
            for i, p in enumerate(r):
                h = hmap[p]
                parts.append({"$type": "network.symbios.gen.cuboid", "size": [w(0.11), w(height + 0.2 + sink), w(0.11 + 0.03 * (i % 2))],
                              "solid": False, "material": STAKE,
                              "transform": {"translation": [w(p[0] - ox), w(h + (height + 0.2 - sink) / 2 - oy), w(p[1] - oz)]}})
    root_drop = w(oy - origin_ground + 0.2)
    for part in parts:
        part["transform"]["translation"][1] += root_drop
    # the root is solid when any panel is: under a non-solid root every collider sat at its offset
    # from the world's origin (#1453)
    gen = {"$type": "network.symbios.gen.cuboid", "size": [200, 200, 200], "solid": solid, "material": STAKE,
           "transform": {"translation": [0, -root_drop, 0]}, "children": parts}
    place = {"$type": "network.symbios.place.absolute", "avoid_water_clearance": 0, "generator_ref": name,
             "snap_to_terrain": False, "transform": {"translation": [w(ox), w(oy), w(oz)]}}
    os.makedirs(out_dir, exist_ok=True)
    gp, pp = os.path.join(out_dir, f"{name}.gen.json"), os.path.join(out_dir, f"{name}.place.json")
    json.dump(gen, open(gp, "w"))
    json.dump(place, open(pp, "w"))
    print(f"{name}: {len(runs)} run(s), {len(parts) + 1} parts with the root, origin ({ox:.1f}, {oy:.2f}, {oz:.1f})")
    print(f"wrote {gp} and {pp}")


if __name__ == "__main__":
    main()
