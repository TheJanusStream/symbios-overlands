#!/usr/bin/env python3
"""usage: ground.py DID RECORD.json X,Z [X,Z ...] [--footprint R]

The ground under each point, one line a point, from `render --terrain-report`
(see ../region.md): its height, how far under the water it is, its slope and
downhill way, the `--yaw` that lays a long thing along the contour, and the
splat layers' shares there (the largest is the `biome` a scatter filters by).
With --footprint R, also what a thing R metres wide rests on and how far the
ground falls beneath it. Negative coordinates are fine: `ground.py ... -40,12`.
"""
import json
import sys

import agentlib


def main():
    args = sys.argv[1:]
    footprint = []
    if "--footprint" in args:
        i = args.index("--footprint")
        footprint = ["--footprint", args[i + 1]]
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
        )


if __name__ == "__main__":
    main()
