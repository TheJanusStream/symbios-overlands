#!/usr/bin/env python3
"""usage: near.py RECORD.json X,Z [X,Z ...] [--within R] [--box L,W,YAW]

What already stands near each point, before a thing is set there by hand: every placement
whose point (an absolute placement's translation, a scatter's or grid's centre) is within R
metres (default 8) of X,Z, nearest first, with its generator and its index - no render, no
daemon, a second. A scatter is named with its reach, since its copies spread round its centre:
a circle's radius, a rect's half sizes along its own axes (its `extents`).
Session 879 set a horse by eye inside a cottage 2.8 m from its origin; this would have said so.

--box L,W,YAW measures from a footprint instead of a point: X,Z is the centre of a rectangle
L long on its own X and W wide on its own Z, turned YAW degrees clockwise seen from above as
`place --yaw` counts it (0 keeps its X east). Each distance is then from the rectangle's edge (0
inside it), and a scatter whose reach comes into the rectangle says how far (`reaches N m
in`): session 885 sited a 9 m cart lodge 8.6 m from an apple garth's centre, which the point
reading passed, while its west end stood 3.3 m inside the garth's 7 m circle. A circle scatter
reaches its radius every way; a rect scatter is measured as the rectangle it is, turned by its
`rotation` as the world scatters it, and one clear of the footprint is listed by its gap.

Reads the record's origins only, not the drawn footprints: a long thing (a lane, a hedge) is
placed near its middle and may pass closer than it lists - look at `clearings.py` for scatters
and a `views.py` picture for the rest.
"""
import json
import math
import sys


def box_distance(px, pz, x, z, box):
    """How far (px, pz) is from the rectangle `box` = (L, W, yaw clockwise) centred on (x, z);
    0 inside it. Without a box, from the point (x, z)."""
    if not box:
        return math.hypot(px - x, pz - z)
    length, width, yaw = box
    # world from local is yrot(-yaw) (place --yaw is clockwise), so local is yrot(yaw) of world
    t = math.radians(yaw)
    dx, dz = px - x, pz - z
    lx = dx * math.cos(t) + dz * math.sin(t)
    lz = -dx * math.sin(t) + dz * math.cos(t)
    return math.hypot(max(abs(lx) - length / 2, 0.0), max(abs(lz) - width / 2, 0.0))


def rect_gap(a, b):
    """How far apart two turned rectangles are, each (x, z, half_x, half_z, yaw) turned as
    box_distance turns a box: (gap, 0) apart, (0, overlap) overlapping - the overlap is the
    least move across an edge that parts them, as a circle's radius less its distance is."""
    def axes(r):
        t = math.radians(r[4])
        return (math.cos(t), math.sin(t)), (-math.sin(t), math.cos(t))
    overlap = math.inf
    for n in axes(a) + axes(b):
        half = sum(r[2] * abs(n[0] * u[0] + n[1] * u[1]) + r[3] * abs(n[0] * v[0] + n[1] * v[1])
                   for r in (a, b) for u, v in [axes(r)])
        overlap = min(overlap, half - abs((a[0] - b[0]) * n[0] + (a[1] - b[1]) * n[1]))
    if overlap > 0:
        return 0.0, overlap
    def corners(r):
        (ux, uz), (vx, vz) = axes(r)
        return [(r[0] + i * r[2] * ux + j * r[3] * vx, r[1] + i * r[2] * uz + j * r[3] * vz)
                for i in (-1, 1) for j in (-1, 1)]
    # apart, the nearest two points of two rectangles include a corner of one of them
    return min([box_distance(px, pz, b[0], b[1], (2 * b[2], 2 * b[3], b[4])) for px, pz in corners(a)]
               + [box_distance(px, pz, a[0], a[1], (2 * a[2], 2 * a[3], a[4])) for px, pz in corners(b)]), 0.0


def near(record, x, z, within=8.0, box=None):
    """Each placement near (x, z), nearest first, as (distance, description)."""
    found = []
    for i, p in enumerate(record.get("placements", [])):
        t = p.get("transform", {}).get("translation")
        b = p.get("bounds", {})
        reach, rect = 0.0, None
        if t:
            px, pz, what = t[0] / 1e4, t[2] / 1e4, ""
        elif b.get("center"):
            px, pz = b["center"][0] / 1e4, b["center"][1] / 1e4
            if b.get("type") == "rect" or "extents" in b:
                # a rect has no radius: reading one named every rect scatter "radius 0 m"
                ex, ez = (c / 1e4 for c in b.get("extents", [0, 0]))
                what = f" (scatter, rect half sizes {ex:g} x {ez:g} m)"
                # the world's sampler turns local X to (cos r, sin r): box_distance's frame at yaw r.
                # Read as the circle through its corners, a field 5 m clear "reached 21.3 m in"
                rect = (px, pz, ex, ez, math.degrees(b.get("rotation", 0) / 1e4))
            else:
                reach = b.get("radius", 0) / 1e4
                what = f" (scatter, radius {reach:g} m)"
        else:
            continue
        d = box_distance(px, pz, x, z, box)
        if box and rect:
            gap, overlap = rect_gap((x, z, box[0] / 2, box[1] / 2, box[2]), rect)
            reach = d + overlap if overlap else d - gap  # reach - d is how far in, d - reach the gap
        if box and reach > d:
            what += f", reaches {reach - d:.1f} m in"
        if d - (reach if box else 0.0) <= within:
            found.append((d, f"{p.get('generator_ref', '?')} #{i} at ({px:.1f}, {pz:.1f}){what}"))
    found.sort()
    return found


def main():
    args = sys.argv[1:]
    within, box = 8.0, None
    if "--within" in args:
        i = args.index("--within")
        within = float(args[i + 1])
        del args[i : i + 2]
    if "--box" in args:
        i = args.index("--box")
        box = tuple(float(v) for v in args[i + 1].split(","))
        if len(box) != 3:
            sys.exit("--box takes L,W,YAW")
        del args[i : i + 2]
    if len(args) < 2:
        sys.exit(__doc__)
    record = json.load(open(args[0]))
    for spec in args[1:]:
        x, z = (float(v) for v in spec.split(","))
        found = near(record, x, z, within, box)
        head = f"({x:g}, {z:g})" + (f" box {box[0]:g} x {box[1]:g} yaw {box[2]:g}" if box else "")
        print(head + ": " + ("; ".join(f"{d:.1f} m {s}" for d, s in found) if found else f"nothing within {within:g} m"))


if __name__ == "__main__":
    main()
