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
  --flat F            squash each spine's height by F (0 < F < 1): a lane or path, wide and low - its
                      half width is --radius and its thickness 2 x radius x F; --lift is then the
                      height of its TOP above the ground (a 5 m lane: --radius 2.5 --flat 0.06) on
                      LEVEL ground. On a grade g along it (0.1 for 10%) the rings lean with the
                      squashed curve and the lane is 2 x radius x sqrt(F^2 + g^2) thick, its top
                      about radius x (sqrt(F^2 + g^2) - F) higher: 0.14 m more at a 10% grade with
                      those numbers, 0.18 m up for --lift 0.04
  --resolution N      sides of each spine (default 6)
  --solid             the thread collides, so feet stand on a lane instead of sinking into it. A solid
                      spine collides as the CONVEX HULL of its points, which bridges any dip between its
                      ends; use it with a short --segment so each hull hugs the ground
  --segment N         points per spine (default 16, the cap): with --solid, 4-5 (10 m at a 2.5 m step)
  --collider N        keep the drawn spines long and NOT solid, and add under them a solid copy cut into
                      spines of N points, 2.5 cm lower and 6% narrower, so it never shows: feet stand on the
                      lane with no seam across it every N points (the joints of solid --segment spines
                      drew a faint arc across a lane). Costs one part per N points.
  --ride R,F,L[,OFF]  lay this thread ON a flat lane of radius R, flatness F and lift L (a rut, a verge,
                      a puddle strip along a lane), with the lane's middle OFF metres to this thread's
                      LEFT as it runs from its first waypoint to its last (a negative OFF: its right;
                      under R). A lane rests on the ground under its middle and does not tilt across,
                      so each sample reads the ground there, OFF metres over, not under itself (on a
                      side slope that was centimetres, the downhill rut buried). It goes up to the
                      lane's top on the local grade g - the lane rises R x (sqrt(F^2 + g^2) - F) there
                      and its top OFF out falls R x sqrt(F^2 + g^2) x (1 - sqrt(1 - (OFF/R)^2)), as
                      its section is that much taller on a grade; this thread's own rise is taken off
                      - and --lift is then the clearance over that top. A fixed --lift buried
                      Ashmere's ruts under the street wherever it steepened (0.19 m up at 10%). Still
                      up to about 1 cm out: the terrain report rounds the ground to 1 cm, a ring's
                      flat facets sit a few mm under the round section this assumes (more toward the
                      lane's edge and on a steep grade: +8 mm 3.5 m out on 20%), and the lane's curve
                      between its own samples is not the ground's (test_tools.py)
  --taper-start, --taper-end
                      the first (last) two samples shrink to 0.3 and 0.75 of the radius, so a flat lane
                      narrows and dives into the ground instead of ending in a square cut; two lanes
                      that overlap to join blend if one is set 1-2 cm higher (--lift), or their tops
                      share a plane and z-fight
"""
import json
import math
import os
import sys

import agentlib

GLOW = {"base_color": [5500, 9500, 6500], "emission_color": [2800, 8600, 5000], "emission_strength": 11000}
MAX_POINTS = 16
CLAMP_M = 95.0  # under MAX_PRIM_DIM_M (100 m) with room for the lift
POINT_CLAMP_M = 100.0  # the sanitiser clamps each spine point coordinate to +-100 m (primitive.rs)


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
    flat = option(args, "--flat", None, float)
    resolution = option(args, "--resolution", 6, int)
    segment = max(2, min(MAX_POINTS, option(args, "--segment", MAX_POINTS, int)))
    solid = "--solid" in args
    if solid:
        args.remove("--solid")
    collider = option(args, "--collider", None, int)
    ride = option(args, "--ride", None)
    taper_start = "--taper-start" in args
    taper_end = "--taper-end" in args
    for flag in ("--taper-start", "--taper-end"):
        if flag in args:
            args.remove(flag)
    if flat is not None and not 0 < flat < 1:
        sys.exit("--flat takes a factor between 0 and 1")
    if len(args) != 5:
        sys.exit(__doc__)
    did, record, name, out_dir, spec = args
    way = [tuple(float(c) for c in p.split(",")) for p in spec.split()]
    if len(way) < 2:
        sys.exit("need at least two waypoints")
    mat = json.load(open(material)) if material else GLOW
    tail = json.load(open(tail_material)) if tail_material else None

    lane = None
    if ride:
        vals = [float(v) for v in ride.split(",")]
        if len(vals) not in (3, 4):
            sys.exit("--ride takes R,F,L or R,F,L,OFF")
        lane = vals + [0.0] * (4 - len(vals))
        if not 0 < lane[0] or not 0 < lane[1] < 1:
            sys.exit("--ride takes the lane's radius R over 0 and its flatness F between 0 and 1")
        if abs(lane[3]) >= lane[0]:
            sys.exit(f"--ride OFF {lane[3]} is not under the lane's radius {lane[0]}: past the lane's edge "
                     f"there is no top to ride")

    xz = resample(catmull_rom(way), step)
    xs, zs = [p[0] for p in xz], [p[1] for p in xz]
    ox, oz = (min(xs) + max(xs)) / 2, (min(zs) + max(zs)) / 2
    # a lane rests on the ground under its middle and does not tilt across (its rings' sideways axis
    # stays level), so a thread riding it OFF out reads the ground at the lane's middle
    mids = []
    if lane and lane[3]:
        for i in range(len(xz)):
            a, b = max(0, i - 1), min(len(xz) - 1, i + 1)
            dx, dz = xz[b][0] - xz[a][0], xz[b][1] - xz[a][1]
            n = math.hypot(dx, dz) or 1.0
            # the left of a heading (dx, dz), with +X east and -Z north, is (dz, -dx)
            mids.append((xz[i][0] + lane[3] * dz / n, xz[i][1] - lane[3] * dx / n))
    # the ground at the origin too, where the hidden root goes (a bent path's middle is off the path)
    heights, water = ground(did, record, xz + mids + [(ox, oz)])
    origin_ground = heights.pop()
    heights, base = heights[:len(xz)], heights[len(xz):]
    wet = [(round(x, 1), round(z, 1)) for (x, z), h in zip(xz, heights) if h < water]
    if wet:
        print(f"warning: {len(wet)} samples are under the water ({water} m), e.g. {wet[:4]}")
    if mids:
        heights = base   # from here on, the ground the thread is laid from is the lane's

    # a flat spine's points are divided by --flat in its own frame, so its heights are measured from
    # their middle to keep them inside the 100 m clamp
    oy = (min(heights) + max(heights)) / 2 if flat else min(heights)
    far = max(math.hypot(x - ox, z - oz) for x, z in xz)
    if far > CLAMP_M:
        sys.exit(f"the thread reaches {far:.1f} m from its middle ({ox:.1f}, {oz:.1f}): past the 100 m "
                 f"spine clamp - split it into two threads")

    if flat:
        reach = max(math.sqrt((x - ox) ** 2 + (z - oz) ** 2 + ((h - oy) / flat) ** 2)
                    for (x, z), h in zip(xz, heights))
        if reach > CLAMP_M:
            sys.exit(f"flattened by {flat}, the lane's points reach {reach:.0f} m in its own frame (heights "
                     f"x {1 / flat:.0f}): past the 100 m clamp - split it, or flatten less (--flat {min(0.9, flat * reach / CLAMP_M * 1.05):.2f})")
    length = sum(math.dist(a, b) for a, b in zip(xz, xz[1:]))
    centre = lift - radius * flat if flat else lift   # a flat spine's TOP sits at the lift
    extra = [0.0] * len(xz)
    if lane:
        lane_r, lane_f, lane_lift, off = lane
        q = math.sqrt(1 - (off / lane_r) ** 2)   # the lens's height OFF out, as a share of its middle's
        path = mids or xz

        def rise(r, f, g):
            return r * (math.sqrt(f * f + g * g) - f) if f else 0.0
        for i in range(len(xz)):
            a, b = max(0, i - 1), min(len(xz) - 1, i + 1)
            g = abs(heights[b] - heights[a]) / (math.dist(path[a], path[b]) or 1.0)
            # on a grade the lane's section is R x sqrt(F^2 + g^2) tall, so its top falls that much
            # more OFF out than on level ground
            lens_fall = lane_r * math.sqrt(lane_f * lane_f + g * g) * (1 - q)
            extra[i] = lane_lift + rise(lane_r, lane_f, g) - lens_fall - rise(radius, flat or 0.0, g)
    pts = [[x - ox, h + centre + e - oy, z - oz] for (x, z), h, e in zip(xz, heights, extra)]
    # where the tail starts, by distance along the thread
    run, tail_from = 0.0, len(pts)
    if tail and tail_m > 0:
        for i in range(1, len(xz)):
            run += math.dist(xz[i - 1], xz[i])
            if length - run <= tail_m:
                tail_from = i
                break
    radii = [radius] * len(pts)
    for e, f in ((0, 0.3), (1, 0.75)):
        if taper_start and e < len(radii):
            radii[e] = radius * f
        if taper_end and e < len(radii):
            radii[-1 - e] = radius * f
    spines = []
    i = 0
    while i < len(pts) - 1:
        m = tail if i >= tail_from else mat
        end = min(i + segment - 1, len(pts) - 1)
        if i < tail_from < end:
            end = tail_from  # the colour changes at a spine's joint
        spines.append({"$type": "network.symbios.gen.spine", "material": m, "solid": solid,
                       "resolution": resolution, "samples_per_segment": 3,
                       "points": [{"position": [int(round(c * 10000)) for c in p],
                                   "radius": int(round(r * 10000))} for p, r in zip(pts[i:end + 1], radii[i:end + 1])]})
        i = end
    if collider:
        # a solid copy under the drawn lane: short spines (each collides as the convex hull of its points,
        # which must hug the ground), 1.5 cm lower and 3% narrower so the drawn lane covers it
        drop = 0.025   # metres; the loop below scales every spine point into the flattened frame
        # one station short of each end: an end cap in the drawn cap's plane z-fought (2 m2 on a lane end)
        # ...and never ending where a drawn spine ends: two caps in one station's plane z-fight too
        drawn_ends = set(range(0, len(pts), segment - 1)) | {len(pts) - 1}
        i, last = 1, len(pts) - 2
        while i < last:
            end = min(i + max(2, collider) - 1, last)
            while end in drawn_ends and end - 1 > i:
                end -= 1
            spines.append({"$type": "network.symbios.gen.spine", "material": mat, "solid": True,
                           "resolution": min(resolution, 12), "samples_per_segment": 2,
                           "points": [{"position": [int(round(p[0] * 10000)), int(round((p[1] - drop) * 10000)),
                                                    int(round(p[2] * 10000))],
                                       "radius": int(round(r * 0.94 * 10000))}
                                      for p, r in zip(pts[i:end + 1], radii[i:end + 1])]})
            i = end
        solid = True
    # The 2 cm root is hidden 0.2 m under the ground at the origin, and the
    # spines lifted back by as much: at the origin's own height it floated wherever the ground there
    # is lower (session 878's floating report found one at the lowest ground; session 879's at a flat
    # lane's middle height).
    root_drop = int(round((oy - origin_ground + 0.2) * 10000))
    for s in spines:
        for p in s["points"]:
            p["position"][1] += root_drop
            if flat:
                # the spine is scaled by flat in y, so its points are written divided by it
                p["position"][1] = int(round(p["position"][1] / flat))
        if flat:
            s["transform"] = {"scale": [10000, int(round(flat * 10000)), 10000]}
    # The reach check above measures from the heights' middle; the points are written from the hidden
    # root at the ground under the origin, divided by --flat. On a bent lane that ground can be a knoll
    # off the lane: a review of #1488 wrote every point of one past the clamp, which moves them and
    # floats the lane. So the values written are checked.
    worst = max(abs(c) / 10000 for s in spines for p in s["points"] for c in p["position"])
    if worst > POINT_CLAMP_M:
        if not flat:
            sys.exit(f"the thread's points are written up to {worst:.0f} m from its hidden root (under the "
                     f"ground at ({ox:.1f}, {oz:.1f}), {origin_ground:.1f} m): past the 100 m clamp - split it")
        drops = (0.0, 0.025) if collider else (0.0,)

        def written(f):
            # the largest height written flattened by f, as the loop above writes it
            return max(abs(h + e + lift - radius * f - origin_ground + 0.2 - d) / f
                       for h, e in zip(heights, extra) for d in drops)
        less = next((k / 100 for k in range(int(flat * 100) + 1, 91) if written(k / 100) <= CLAMP_M), None)
        hint = f"flatten less (--flat {less:.2f})" if less else "flatten less"
        sys.exit(f"flattened by {flat}, the lane's points are written up to {worst:.0f} m from its hidden root "
                 f"(heights x {1 / flat:.0f}, from the ground at ({ox:.1f}, {oz:.1f}), {origin_ground:.1f} m, "
                 f"under its origin): past the 100 m clamp - split it, or {hint}")
    # a solid part needs a solid root: under a non-solid one every collider sat at its offset from the
    # world's origin (#1453)
    gen = {"$type": "network.symbios.gen.cuboid", "size": [200, 200, 200], "solid": solid,
           "material": mat, "transform": {"translation": [0, -root_drop, 0]}, "children": spines}
    place = {"$type": "network.symbios.place.absolute", "avoid_water_clearance": 0, "generator_ref": name,
             "snap_to_terrain": False,
             "transform": {"translation": [int(round(ox * 10000)), int(round(oy * 10000)), int(round(oz * 10000))]}}
    os.makedirs(out_dir, exist_ok=True)
    gp, pp = os.path.join(out_dir, f"{name}.gen.json"), os.path.join(out_dir, f"{name}.place.json")
    for path, value in ((gp, gen), (pp, place)):
        with open(path, "w") as fh:
            json.dump(value, fh)
    print(f"{name}: {length:.0f} m, {len(pts)} samples, {len(spines)} spines, origin ({ox:.1f}, {oy:.2f}, {oz:.1f}), "
          f"farthest point {far:.0f} m, ground {min(heights):.1f}..{max(heights):.1f} m")
    print(f"wrote {gp} ({len(json.dumps(gen, separators=(',', ':')))} bytes) and {pp}")


if __name__ == "__main__":
    main()
