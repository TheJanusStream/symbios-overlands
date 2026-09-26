#!/usr/bin/env python3
"""usage: views.py DID RECORD.json OUT.png "LABEL X Y Z LOOK [DOWN]" ...

Offline pictures of a world record from where a person would stand: one render
per view, the camera AT world (X, Y, Z) looking toward compass LOOK in degrees
(0 north = -Z, 90 east = +X, 180 south, 270 west), tiled two wide, each
labelled. Y is a WORLD height - or `~` for the ground there plus 1.7 m (eye
height), `~2.5` for the ground plus 2.5 m, read in one terrain report. LABEL has no
spaces. A spec "@admin [DOWN]" sees from where the admin stands, the way they
face; "@admincam [DOWN]" from the game's camera behind them - read from the
live `status`. "@landing [DOWN]" sees from where visitors arrive, the way they
face, and "@landingcam [DOWN]" what a new visitor's screen first shows (the
game's camera behind them) - both from RECORD's default_landing, no daemon
needed. DOWN (default 2) tilts the view that many degrees below level: 30-50
to see what lies at your feet - a level view misses anything low and close.

Built on the render tool's orbit rig: a camera 1 m behind its focus, 2 degrees
up, whose yaw turns the other way from a compass.
"""
import json
import math
import os
import sys
import tempfile

from PIL import Image, ImageDraw

import agentlib

TILE = (960, 480)


def main():
    if len(sys.argv) < 5:
        sys.exit(__doc__)
    did, record, out, specs = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4:]
    parsed = [spec.split() for spec in specs]
    if any(f[0].startswith("@") for f in parsed):
        # "@admin [DOWN]": where the admin stands, eyes 1.6 m up, looking the way they face;
        # "@admincam [DOWN]": the game's own camera behind them (11 m back, 4.7 m up)
        status = agentlib.result(agentlib.agent("status"), "status")
        admin = next((p for p in status.get("peers", []) if p.get("admin")), None)
        if admin is None or not admin.get("placed"):
            sys.exit("the admin is not in the world, or not placed")
        (ax, ay, az), (fx, fz) = admin["position"], admin["facing"]
        look = math.degrees(math.atan2(fx, -fz)) % 360
        for i, f in enumerate(parsed):
            if f[0] in ("@admin", "@admincam"):
                back, up = (11.0, 4.7) if f[0] == "@admincam" else (0.0, 1.6)
                down = f[1] if len(f) > 1 else ("20" if back else "2")
                parsed[i] = [f[0][1:], f"{ax - fx * back:.2f}", f"{ay + up:.2f}", f"{az - fz * back:.2f}",
                             f"{look:.1f}", down]
    if any(f[0] in ("@landing", "@landingcam") for f in parsed):
        # "@landing [DOWN]": a visitor's eyes where they arrive - the record's default_landing,
        # facing its yaw (counter-clockwise, see ../region.md "Arrivals"); "@landingcam [DOWN]":
        # the game's camera behind them there, the first picture a new visitor's screen shows
        land = json.load(open(record)).get("default_landing") or {}
        lx, lz = (v / 1e4 for v in land.get("pos", [0, 0]))
        yaw = math.radians(land.get("yaw_deg", 0) / 1e4)
        fx, fz = -math.sin(yaw), -math.cos(yaw)
        look = math.degrees(math.atan2(fx, -fz)) % 360
        rep, _ = agentlib.render("--world", did, "--world-record", record, "--terrain-report", f"--at={lx:.2f},{lz:.2f}")
        ground = json.loads(rep[rep.find("{"):])["points"][0]["ground_m"]
        for i, f in enumerate(parsed):
            if f[0] in ("@landing", "@landingcam"):
                # the camera orbits 12 m out from a focus about 1 m over the ground: 11 m back, 5.7 m up
                back, up = (11.0, 5.7) if f[0] == "@landingcam" else (0.0, 1.7)
                down = f[1] if len(f) > 1 else ("20" if back else "2")
                parsed[i] = [f[0][1:], f"{lx - fx * back:.2f}", f"{ground + up:.2f}", f"{lz - fz * back:.2f}",
                             f"{look:.1f}", down]
    on_ground = [f for f in parsed if f[2].startswith("~")]
    if on_ground:
        # "~" = the ground there + 1.7 m (eye height), "~2.5" = + 2.5 m: one terrain report for all
        args = ["--world", did, "--world-record", record, "--terrain-report"]
        args += [f"--at={f[1]},{f[3]}" for f in on_ground]
        rep, _ = agentlib.render(*args)
        points = json.loads(rep[rep.find("{"):])["points"]
        for f, p in zip(on_ground, points):
            f[2] = f"{p['ground_m'] + float(f[2][1:] or 1.7):.2f}"
    tiles = []
    with tempfile.TemporaryDirectory() as tmp:
        for i, fields in enumerate(parsed):
            label, x, y, z, look = fields[:5]
            down = fields[5] if len(fields) > 5 else "2"
            path = os.path.join(tmp, f"view{i}.png")
            agentlib.render("--world", did, "--world-record", record, f"--focus={x},{y},{z}", "--dist", "1",
                            f"--elev={down}", "--yaw", str((-float(look)) % 360), "--width", "1280",
                            "--height", "640", "--out", path)
            if not os.path.exists(path):
                sys.exit(f"view {label} did not render (is the render tool built?)")
            im = Image.open(path).convert("RGB").resize(TILE)
            ImageDraw.Draw(im).text((8, 8), label, fill=(255, 255, 255))
            tiles.append(im)
    rows = (len(tiles) + 1) // 2
    sheet = Image.new("RGB", (TILE[0] * 2, TILE[1] * rows))
    for i, t in enumerate(tiles):
        sheet.paste(t, ((i % 2) * TILE[0], (i // 2) * TILE[1]))
    sheet.save(out)
    print(out)


if __name__ == "__main__":
    main()
