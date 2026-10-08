"""Reproduce the pinned squares of the geodata tests without the Rust code.

`square::tests::size_draw_is_pinned` and
`berlin::coverage::tests::squares_from_draws_are_pinned` pin what fixed draws
give, because a seeded region must keep its square across releases. This is
an independent implementation of the same contract, read from the generated
table, so a change to the Rust that moves a square shows up as a
disagreement with a second reading rather than as a re-blessed pin:

- the size: log-uniform over 250 m..19 km from the draw's top 53 bits,
  rounded half away from zero to a whole 10 m;
- the position: every whole-metre south-west corner at which the square
  lies on inside cells, counted per corner cell in two offset classes per
  axis, walked north class, east class, row, column; the draw scaled into
  that count by the high half of a 128-bit product.

Run from the repository root (pure Python, no packages):

    python3 -I crates/geodata/tools/place_check.py \
        crates/geodata/src/berlin/coverage_table.rs

It prints the squares and exits non-zero if any differs from the pins.
"""

import math
import re
import sys

CELL, ORIGIN_E, ORIGIN_N, COLS = 250, 368_000, 5_798_000, 200
SIZE_MIN, SIZE_MAX, STEP = 250, 19_000, 10

PINS = [
    ((0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210), (396_482, 5_833_873, 250)),
    ((0x8000_0000_0000_0000, 0x8000_0000_0000_0000), (402_454, 5_806_171, 2_180)),
    ((0xDEAD_BEEF_F00D_CAFE, 0x0BAD_C0DE_1234_5678), (378_010, 5_809_802, 10_810)),
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


def size_from_draw(draw):
    unit = (draw >> 11) / float(1 << 53)
    size = SIZE_MIN * math.exp(unit * math.log(SIZE_MAX / SIZE_MIN))
    steps = math.floor(size / STEP + 0.5)
    return min(max(steps * STEP, SIZE_MIN), SIZE_MAX)


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
    for (size_draw, place_draw), pinned in PINS:
        got = place(walk, size_from_draw(size_draw), place_draw)
        ok &= got == pinned
        print(f"{size_draw:#018x} -> {got} {'ok' if got == pinned else f'PINNED {pinned}'}")
    largest = sum(weight for *_, weight in walk(SIZE_MAX))
    beyond = sum(weight for *_, weight in walk(SIZE_MAX + STEP))
    print(f"placements: {SIZE_MAX} m {largest}, {SIZE_MAX + STEP} m {beyond}")
    ok &= largest > 0 and beyond == 0
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
