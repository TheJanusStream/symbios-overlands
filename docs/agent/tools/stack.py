#!/usr/bin/env python3
"""usage: stack.py OUT.png PICTURE... - stack pictures one above another at 1024 wide.

For `look` pictures side by side with renders, or before and after. Paths are
taken as arguments because the looks folder's name holds `%3A` (from the DID),
which printf-style formatting chokes on.
"""
import sys

from PIL import Image


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    ims = [Image.open(p).convert("RGB") for p in sys.argv[2:]]
    ims = [im.resize((1024, int(im.height * 1024 / im.width))) for im in ims]
    sheet = Image.new("RGB", (1024, sum(im.height for im in ims)))
    y = 0
    for im in ims:
        sheet.paste(im, (0, y))
        y += im.height
    sheet.save(sys.argv[1])
    print(sys.argv[1])


if __name__ == "__main__":
    main()
