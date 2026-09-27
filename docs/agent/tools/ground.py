#!/usr/bin/env python3
"""usage: ground.py DID RECORD.json X,Z [X,Z ...] [--footprint R] [--lay YAW]

The ground under each point, one line a point, from `render --terrain-report`
(see ../region.md): its height, how far under the water it is, its slope and
downhill way, the `--yaw` that lays a long thing along the contour, and the
splat layers' shares there (the largest is the `biome` a scatter filters by).
With --footprint R, also what a thing R metres wide rests on and how far the
ground falls beneath it. With --lay YAW, also the wire `rotation` that lays a flat
thing ON the ground there - local Y along the ground's normal, turned YAW degrees
CLOCKWISE seen from above, as `place --yaw` turns a thing: 0 faces its front (local -Z)
to world -Z, 90 to +X, the heading then tipped into the ground's plane. On level ground
it is `place --yaw YAW`'s rotation exactly, and --lay at the printed contour_yaw (the same
convention) runs local +X along the contour - for a bed, a pool, a hearth stone: a level
one on a 4 degree slope stood 0.4 m proud at its downhill end. Negative coordinates are
fine: `ground.py ... -40,12`.
"""
import json
import math
import sys

import agentlib


def main():
    args = sys.argv[1:]
    footprint = []
    if "--footprint" in args:
        i = args.index("--footprint")
        footprint = ["--footprint", args[i + 1]]
        del args[i : i + 2]
    lay = None
    if "--lay" in args:
        i = args.index("--lay")
        lay = float(args[i + 1])
        del args[i : i + 2]
    if len(args) < 3:
        sys.exit(__doc__)
    did, record, points = args[0], args[1], args[2:]
    cmd = ["--world", did, "--world-record", record, "--terrain-report", *footprint]
    cmd += [f"--at={p}" for p in points]
    out, err = agentlib.render(*cmd, timeout=120)
    start = out.find("{")
    if start < 0:
        sys.exit(f"no report: {out[-400:]} {err[-400:]}")
    report = json.loads(out[start:])
    water = report["water"]
    print(f"water level {water['level_m']} m, floods {water['flooded_share']:.0%}")
    for p in report["points"]:
        layers = " ".join(f"{l['layer']}:{l['texture']}:{l['share']:.2f}" for l in p.get("layers", []))
        under = water["level_m"] - p["ground_m"]
        wet = f" UNDER {under:.2f}" if under > 0 else f" above {-under:.2f}"
        rest = {k: v for k, v in p.items() if k not in ("x", "z", "ground_m", "slope_deg", "downhill", "contour_yaw_deg", "layers", "biome")}
        print(
            f"({p['x']:g}, {p['z']:g}) ground {p['ground_m']:.2f}{wet} slope {p['slope_deg']:.1f} "
            f"down {p['downhill']} contour_yaw {p['contour_yaw_deg']} biome {p['biome']} [{layers}]"
            + (f" {json.dumps(rest)}" if rest else "")
            + (f" lay {laid(lay, p['slope_deg'], p['downhill'])}" if lay is not None else "")
        )


def laid(yaw_deg, slope_deg, downhill):
    """The wire quaternion [x, y, z, w] x 10000 taking local Y to the ground's normal and local Z to
    where a clockwise yaw turns it, laid into the ground's plane."""
    s = math.radians(slope_deg)
    dx, dz = downhill if downhill else (1.0, 0.0)
    n = [math.sin(s) * dx, math.cos(s), math.sin(s) * dz]
    a = math.radians(yaw_deg)
    # local +Z under Ry(-yaw), the placement's turn (placements.rs `yaw_rotation`): a yaw of 90
    # faces local -Z to +X. Ry(+yaw) turned the other way, 90 facing -X.
    f = [-math.sin(a), 0.0, math.cos(a)]
    d = sum(f[i] * n[i] for i in range(3))
    z = [f[i] - d * n[i] for i in range(3)]
    zl = math.sqrt(sum(c * c for c in z))
    z = [c / zl for c in z]
    x = [n[1] * z[2] - n[2] * z[1], n[2] * z[0] - n[0] * z[2], n[0] * z[1] - n[1] * z[0]]
    m00, m01, m02, m10, m11, m12, m20, m21, m22 = x[0], n[0], z[0], x[1], n[1], z[1], x[2], n[2], z[2]
    tr = m00 + m11 + m22
    if tr > 0:
        k = math.sqrt(tr + 1.0) * 2
        q = [(m21 - m12) / k, (m02 - m20) / k, (m10 - m01) / k, 0.25 * k]
    elif m00 > m11 and m00 > m22:
        k = math.sqrt(1.0 + m00 - m11 - m22) * 2
        q = [0.25 * k, (m01 + m10) / k, (m02 + m20) / k, (m21 - m12) / k]
    elif m11 > m22:
        k = math.sqrt(1.0 + m11 - m00 - m22) * 2
        q = [(m01 + m10) / k, 0.25 * k, (m12 + m21) / k, (m02 - m20) / k]
    else:
        k = math.sqrt(1.0 + m22 - m00 - m11) * 2
        q = [(m02 + m20) / k, (m12 + m21) / k, 0.25 * k, (m10 - m01) / k]
    return [int(round(c * 10000)) for c in q]


if __name__ == "__main__":
    main()
