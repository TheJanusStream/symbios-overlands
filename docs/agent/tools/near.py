#!/usr/bin/env python3
"""usage: near.py RECORD.json X,Z [X,Z ...] [--within R]

What already stands near each point, before a thing is set there by hand: every placement
whose point (an absolute placement's translation, a scatter's or grid's centre) is within R
metres (default 8) of X,Z, nearest first, with its generator and its index - no render, no
daemon, a second. A scatter is named with its reach, since its copies spread round its centre:
a circle's radius, a rect's half sizes along its own axes (its `extents`).
Session 879 set a horse by eye inside a cottage 2.8 m from its origin; this would have said so.
Reads the record's origins only, not the drawn footprints: a long thing (a lane, a hedge) is
placed near its middle and may pass closer than it lists - look at `clearings.py` for scatters
and a `views.py` picture for the rest.
"""
import json
import math
import sys


def main():
    args = sys.argv[1:]
    within = 8.0
    if "--within" in args:
        i = args.index("--within")
        within = float(args[i + 1])
        del args[i : i + 2]
    if len(args) < 2:
        sys.exit(__doc__)
    record = json.load(open(args[0]))
    places = record.get("placements", [])
    for spec in args[1:]:
        x, z = (float(v) for v in spec.split(","))
        near = []
        for i, p in enumerate(places):
            t = p.get("transform", {}).get("translation")
            b = p.get("bounds", {})
            if t:
                px, pz, what = t[0] / 1e4, t[2] / 1e4, ""
            elif b.get("center"):
                px, pz = b["center"][0] / 1e4, b["center"][1] / 1e4
                if b.get("type") == "rect" or "extents" in b:
                    # a rect has no radius: reading one named every rect scatter "radius 0 m"
                    ex, ez = (c / 1e4 for c in b.get("extents", [0, 0]))
                    what = f" (scatter, rect half sizes {ex:g} x {ez:g} m)"
                else:
                    what = f" (scatter, radius {b.get('radius', 0) / 1e4:g} m)"
            else:
                continue
            d = math.hypot(px - x, pz - z)
            if d <= within:
                near.append((d, f"{p.get('generator_ref', '?')} #{i} at ({px:.1f}, {pz:.1f}){what}"))
        near.sort()
        print(f"({x:g}, {z:g}): " + ("; ".join(f"{d:.1f} m {s}" for d, s in near) if near else f"nothing within {within:g} m"))


if __name__ == "__main__":
    main()
