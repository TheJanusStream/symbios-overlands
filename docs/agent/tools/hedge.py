#!/usr/bin/env python3
"""usage: hedge.py DID RECORD.json NAME OUT_DIR "X,Z X,Z ..." [options]

A hedgerow (or any low green run: a bank, a box hedge, a line of bushes) laid on the real
ground: the waypoints are joined by straight runs (--smooth for a curve), sampled every
--step metres, and each sample becomes one soft ellipsoid, long along the run, its foot sunk
--sink below the ground there (read from `render --terrain-report`). The ellipsoids are
gathered 16 to a BlobGroup (the engine's cap), each group one drawn part - a browser pays
per part - in which neighbouring ellipsoids melt into one another; each group overlaps the
next by one element so no seam shows, so a group spans 15 steps: one part per ~28 m at the
default step. The generator's 2 cm root, hidden under the ground, carries the material and
is drawn too: a 30 m hedge (17 ellipsoids) is two BlobGroups and the root, three parts.
Sizes jitter by --jitter so it reads grown, not extruded. The generator is placed unsnapped
near the middle of what it spans (refuses past the 100 m clamp, as thread.py does).

Writes OUT_DIR/NAME.gen.json and OUT_DIR/NAME.place.json (an absolute placement).

options:
  --height H        hedge height above the ground, m (default 1.7)
  --width W         hedge thickness, m (default 1.4)
  --step S          spacing of the ellipsoids along the run, m (default 1.9)
  --sink M          how far each ellipsoid's foot sits under the ground (default 0.25)
  --jitter J        size variation, a fraction (default 0.18)
  --resolution N    BlobGroup cells along its longest axis, 8-48 (default 44)
  --material FILE   the hedge's material as wire JSON (default: a hawthorn green Moss texture)
  --smooth          a Catmull-Rom curve through the waypoints instead of straight runs
  --seed N          the jitter's seed (default 1)
"""
import json
import math
import os
import random
import sys

import agentlib
from thread import catmull_rom, resample, ground, option, CLAMP_M


def lin(c):
    return ((c + 0.055) / 1.055) ** 2.4


HAWTHORN = {"base_color": [10000, 10000, 10000], "roughness": 9000, "uv_scale": 16000,
            "texture": {"$type": "Moss", "seed": 71,
                        "color_deep": [int(lin(c) * 10000) for c in (0.10, 0.17, 0.07)],
                        "color_tip": [int(lin(c) * 10000) for c in (0.26, 0.36, 0.14)],
                        "color_dry": [int(lin(c) * 10000) for c in (0.34, 0.16, 0.10)],
                        "dry_patches": 1500}}
MAX_ELEMENTS = 16


def straight(pts, per=20):
    out = []
    for a, b in zip(pts, pts[1:]):
        for k in range(per):
            t = k / per
            out.append((a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t))
    out.append(pts[-1])
    return out


def main():
    args = sys.argv[1:]
    height = option(args, "--height", 1.7, float)
    width = option(args, "--width", 1.4, float)
    step = option(args, "--step", 1.9, float)
    sink = option(args, "--sink", 0.25, float)
    jitter = option(args, "--jitter", 0.18, float)
    resolution = option(args, "--resolution", 44, int)
    material = option(args, "--material", None)
    seed = option(args, "--seed", 1, int)
    smooth = "--smooth" in args
    if smooth:
        args.remove("--smooth")
    if len(args) != 5:
        sys.exit(__doc__)
    did, record, name, out_dir, spec = args
    way = [tuple(float(c) for c in p.split(",")) for p in spec.split()]
    if len(way) < 2:
        sys.exit("need at least two waypoints")
    mat = json.load(open(material)) if material else HAWTHORN
    rnd = random.Random(seed)

    xz = resample(catmull_rom(way) if smooth else straight(way), step)
    xs, zs = [p[0] for p in xz], [p[1] for p in xz]
    ox, oz = (min(xs) + max(xs)) / 2, (min(zs) + max(zs)) / 2
    heights, water = ground(did, record, xz + [(ox, oz)])   # and the ground where the root goes
    origin_ground = heights.pop()
    wet = [(round(x, 1), round(z, 1)) for (x, z), h in zip(xz, heights) if h < water]
    if wet:
        print(f"warning: {len(wet)} samples are under the water ({water} m), e.g. {wet[:4]}")
    oy = min(heights)
    far = max(math.hypot(x - ox, z - oz) for x, z in xz)
    if far > CLAMP_M:
        sys.exit(f"the hedge reaches {far:.1f} m from its middle ({ox:.1f}, {oz:.1f}): past the 100 m "
                 f"clamp - split it into two hedges")

    elements = []
    for i, ((x, z), h) in enumerate(zip(xz, heights)):
        a, b = xz[max(i - 1, 0)], xz[min(i + 1, len(xz) - 1)]
        yaw = math.atan2(-(b[1] - a[1]), b[0] - a[0])        # turn local X along the run
        q = [0.0, math.sin(yaw / 2), 0.0, math.cos(yaw / 2)]
        f = 1 + rnd.uniform(-jitter, jitter)
        ry = (height + sink) / 2 * (1 + rnd.uniform(-jitter, jitter) * 0.6)
        rx = step * 0.85 * f
        rz = width / 2 * (1 + rnd.uniform(-jitter, jitter))
        cy = h - oy - sink + ry
        elements.append({"shape": {"$type": "network.symbios.blob.ellipsoid"},
                         "position": [int(round(c * 10000)) for c in (x - ox, cy, z - oz)],
                         "rotation": [int(round(c * 10000)) for c in q],
                         "radii": [int(round(c * 10000)) for c in (rx, ry, rz)],
                         "subtract": False, "blend": int(round(min(rx, rz) * 0.6 * 10000))})
    groups = []
    i = 0
    while i < len(elements):
        chunk = elements[i:i + MAX_ELEMENTS]
        groups.append({"$type": "network.symbios.gen.blob_group", "resolution": resolution, "solid": False,
                       "material": mat, "elements": chunk})
        if i + MAX_ELEMENTS >= len(elements):
            break
        i += MAX_ELEMENTS - 1          # one element shared with the next group: no seam
    root_drop = int(round((oy - origin_ground + 0.2) * 10000))   # the root hidden under the ground there
    for g in groups:
        for e in g["elements"]:
            e["position"][1] += root_drop
    gen = {"$type": "network.symbios.gen.cuboid", "size": [200, 200, 200], "solid": False, "material": mat,
           "transform": {"translation": [0, -root_drop, 0]}, "children": groups}
    place = {"$type": "network.symbios.place.absolute", "avoid_water_clearance": 0, "generator_ref": name,
             "snap_to_terrain": False,
             "transform": {"translation": [int(round(ox * 10000)), int(round(oy * 10000)), int(round(oz * 10000))]}}
    os.makedirs(out_dir, exist_ok=True)
    gp, pp = os.path.join(out_dir, f"{name}.gen.json"), os.path.join(out_dir, f"{name}.place.json")
    json.dump(gen, open(gp, "w"))
    json.dump(place, open(pp, "w"))
    length = sum(math.dist(p, q) for p, q in zip(xz, xz[1:]))
    # the root is a drawn part too (a cuboid with the material), counted as --triangle-report counts it
    print(f"{name}: {length:.0f} m, {len(elements)} ellipsoids in {len(groups)} BlobGroups, "
          f"{len(groups) + 1} parts with the root, origin "
          f"({ox:.1f}, {oy:.2f}, {oz:.1f}), ground {min(heights):.1f}..{max(heights):.1f} m")
    print(f"wrote {gp} ({len(json.dumps(gen, separators=(',', ':')))} bytes) and {pp}")


if __name__ == "__main__":
    main()
