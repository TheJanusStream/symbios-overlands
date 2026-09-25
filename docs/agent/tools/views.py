#!/usr/bin/env python3
"""usage: views.py DID RECORD.json OUT.png "LABEL X Y Z LOOK" ...

Offline pictures of a world record from where a person would stand: one render
per view, the camera AT world (X, Y, Z) looking toward compass LOOK in degrees
(0 north = -Z, 90 east = +X, 180 south, 270 west), tiled two wide, each
labelled. Y is a WORLD height: read the ground with `render --terrain-report
--at=X,Z` and add about 1.7 m for eye height. LABEL has no spaces.

Built on the render tool's orbit rig: a camera 1 m behind its focus, 2 degrees
up, whose yaw turns the other way from a compass.
"""
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
    tiles = []
    with tempfile.TemporaryDirectory() as tmp:
        for i, spec in enumerate(specs):
            label, x, y, z, look = spec.split()
            path = os.path.join(tmp, f"view{i}.png")
            agentlib.render("--world", did, "--world-record", record, f"--focus={x},{y},{z}", "--dist", "1",
                            "--elev", "2", "--yaw", str((-float(look)) % 360), "--width", "1280",
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
