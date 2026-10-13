"""Reproduce the pinned squares of the geodata tests without the Rust code.

`berlin::coverage::tests::seeded_squares_are_pinned` pins what fixed draws
give, because a seeded region must keep its square across releases. This is
an independent implementation of the same contract, read from the generated
table, so a change to the Rust that moves a square shows up as a
disagreement with a second reading rather than as a re-blessed pin:

- the side: a seeded square is a kilometre (#1637);
- the position: every whole-metre south-west corner at which the square
  lies on inside cells, counted per corner cell in two offset classes per
  axis, walked north class, east class, row, column; the draw scaled into
  that count by the high half of a 128-bit product.

Run from the repository root (pure Python, no packages):

    python3 -I crates/geodata/tools/place_check.py \
        crates/geodata/src/berlin/coverage_table.rs

It prints the squares and exits non-zero if any differs from the pins.
"""

import re
import sys

CELL, ORIGIN_E, ORIGIN_N, COLS = 250, 368_000, 5_798_000, 200
SEEDED, STEP = 1_000, 10
# The largest square that fits wholly inside Berlin: the state's shape.
LARGEST = 19_000

PINS = [
    (0xFEDC_BA98_7654_3210, (394_562, 5_832_645, SEEDED)),
    (0x8000_0000_0000_0000, (385_176, 5_817_043, SEEDED)),
    (0x0BAD_C0DE_1234_5678, (401_077, 5_806_920, SEEDED)),
]


def load(path):
    with open(path, encoding="utf-8") as f:
        rows = re.findall(r'^    "([^"]+)",$', f.read(), re.M)
    inside = []
    for row in rows:
        cells = []
        for symbol, count in re.findall(r"([.a-l])(\d+)", row):
            cells += [symbol != "."] * int(count)
        assert len(cells) == COLS, "row width"
        inside.append(cells)
    return inside


def walker(inside):
    rows = len(inside)
    table = [[0] * (COLS + 1) for _ in range(rows + 1)]
    for r in range(rows):
        for c in range(COLS):
            table[r + 1][c + 1] = (
                (not inside[r][c]) + table[r][c + 1] + table[r + 1][c] - table[r][c]
            )

    def clear(col, row, w, h):
        return table[row + h][col + w] + table[row][col] - table[row][col + w] - table[row + h][col] == 0

    def walk(size):
        fewest = -(-size // CELL)
        tight = fewest * CELL - size + 1
        classes = [(fewest, 0, tight), (fewest + 1, tight, CELL - tight)]
        for span_n, base_n, count_n in classes:
            for span_e, base_e, count_e in classes:
                if count_e == 0 or count_n == 0 or span_e > COLS or span_n > rows:
                    continue
                for row in range(rows - span_n + 1):
                    for col in range(COLS - span_e + 1):
                        if clear(col, row, span_e, span_n):
                            yield col, row, base_e, count_e, base_n, count_e * count_n

    return walk


def place(walk, size, draw):
    total = sum(weight for *_, weight in walk(size))
    if total == 0:
        return None
    target = (draw * total) >> 64
    before = 0
    for col, row, base_e, count_e, base_n, weight in walk(size):
        if target < before + weight:
            index = target - before
            return (
                ORIGIN_E + col * CELL + base_e + index % count_e,
                ORIGIN_N + row * CELL + base_n + index // count_e,
                size,
            )
        before += weight
    raise AssertionError("target past the total")


def main():
    walk = walker(load(sys.argv[1]))
    ok = True
    for place_draw, pinned in PINS:
        got = place(walk, SEEDED, place_draw)
        ok &= got == pinned
        print(f"{place_draw:#018x} -> {got} {'ok' if got == pinned else f'PINNED {pinned}'}")
    largest = sum(weight for *_, weight in walk(LARGEST))
    beyond = sum(weight for *_, weight in walk(LARGEST + STEP))
    print(f"placements: {LARGEST} m {largest}, {LARGEST + STEP} m {beyond}")
    ok &= largest > 0 and beyond == 0
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
