#!/usr/bin/env python3
"""usage: compare.py DID SRC.json EDITS OUT.png "SPEC" ...

Before and after, offline: EDITS folded into a copy of SRC (as `rec.py
compose` does), every SPEC rendered from both (a views.py spec: "LABEL X Y Z
LOOK [DOWN]", `~` heights, "@landing", "@landingcam", "@admin"...), and one
row per spec - the source on the left, the edited copy on the right - so a
change is judged in one picture, from the same eyes. Session 878 chained the
same four commands for this a dozen times.
"""
import os
import subprocess
import sys
import tempfile

from PIL import Image, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
TILE = (960, 480)  # views.py's tile


def sheet(did, record, out, specs):
    subprocess.run([sys.executable, os.path.join(HERE, "views.py"), did, record, out, *specs],
                   check=True, stdout=subprocess.DEVNULL)
    im = Image.open(out)
    return [im.crop(((i % 2) * TILE[0], (i // 2) * TILE[1], (i % 2 + 1) * TILE[0], (i // 2 + 1) * TILE[1]))
            for i in range(len(specs))]


def main():
    if len(sys.argv) < 6:
        sys.exit(__doc__)
    did, src, edits, out, specs = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5:]
    with tempfile.TemporaryDirectory() as tmp:
        after = os.path.join(tmp, "after.json")
        subprocess.run([sys.executable, os.path.join(HERE, "rec.py"), "compose", src, edits, after],
                       check=True, stdout=subprocess.DEVNULL)
        before_tiles = sheet(did, src, os.path.join(tmp, "before.png"), specs)
        after_tiles = sheet(did, after, os.path.join(tmp, "after.png"), specs)
    rows = Image.new("RGB", (TILE[0] * 2, TILE[1] * len(specs)))
    for i, (b, a) in enumerate(zip(before_tiles, after_tiles)):
        for j, (tile, word) in enumerate(((b, "before"), (a, "after"))):
            ImageDraw.Draw(tile).text((TILE[0] - 60, 8), word, fill=(255, 255, 255))
            rows.paste(tile, (j * TILE[0], i * TILE[1]))
    rows.save(out)
    print(out)


if __name__ == "__main__":
    main()
