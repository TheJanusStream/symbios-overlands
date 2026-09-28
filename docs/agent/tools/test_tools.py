#!/usr/bin/env python3
"""Offline tests for the tools here: no agent, no daemon, no render tool.

    python3 -m unittest discover -s docs/agent/tools -p 'test_*.py'

thread.py's ground is stubbed with a plane, and what it writes is meshed as the engine meshes a spine
(a port of src/world_builder/prim/sweeps.rs and bevy_math's Catmull-Rom), so a thread riding a lane is
judged against the lane's drawn top, not against the formula that laid it. rec.py talks to a stubbed
agent.
"""
import contextlib
import io
import json
import math
import os
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rec  # noqa: E402
import thread  # noqa: E402

# bevy_math's cardinal spline at tension 0.5 (CubicCardinalSpline::new_catmull_rom), mirrored ends
S = 0.5
CARDINAL = [[0.0, 1.0, 0.0, 0.0], [-S, 0.0, S, 0.0], [2 * S, S - 3, 3 - 2 * S, -S], [-S, 2 - S, S - 2, S]]


def add(a, b):
    return [x + y for x, y in zip(a, b)]


def sub(a, b):
    return [x - y for x, y in zip(a, b)]


def mul(a, k):
    return [x * k for x in a]


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def unit(a):
    n = math.sqrt(dot(a, a))
    return [x / n for x in a] if n > 1e-12 else [0.0, 0.0, 0.0]


def rotate_arc(frm, to, v):
    """glam's Quat::from_rotation_arc(frm, to) applied to v (Rodrigues)."""
    axis = cross(frm, to)
    s = math.sqrt(dot(axis, axis))
    if s < 1e-12:
        return v
    k, ang = mul(axis, 1 / s), math.atan2(s, dot(frm, to))
    return add(add(mul(v, math.cos(ang)), mul(cross(k, v), math.sin(ang))), mul(k, dot(k, v) * (1 - math.cos(ang))))


def stations(points, per):
    """spine_stations: (position, radius, normal, binormal) along the curve through (xyz, radius) points."""
    ctrl = [list(p) + [max(r, 0.005)] for p, r in points]
    ext = [sub(mul(ctrl[0], 2), ctrl[1])] + ctrl + [sub(mul(ctrl[-1], 2), ctrl[-2])]
    segs = [[[sum(ext[i + k][d] * CARDINAL[row][k] for k in range(4)) for d in range(4)] for row in range(4)]
            for i in range(len(ext) - 3)]
    per = max(2, min(64, per))
    raw = []
    for i in range((len(ctrl) - 1) * per + 1):
        t = i / per
        j = max(0, min(len(segs) - 1, int(math.floor(t))))
        a, b, c, d = segs[j]
        u = t - j
        pos = [a[k] + (b[k] + (c[k] + d[k] * u) * u) * u for k in range(4)]
        vel = [b[k] + (2 * c[k] + 3 * d[k] * u) * u for k in range(4)]
        raw.append((pos[:3], max(pos[3], 0.005), unit(vel[:3])))
    tangents = [t if t != [0.0, 0.0, 0.0] else [0.0, 1.0, 0.0] for _, _, t in raw]
    seed = [0.0, 1.0, 0.0] if abs(tangents[0][1]) < 0.9 else [1.0, 0.0, 0.0]
    normal = unit(cross(seed, tangents[0]))
    out = []
    for i, (pos, r, _) in enumerate(raw):
        if i:
            normal = rotate_arc(tangents[i - 1], tangents[i], normal)
        normal = unit(sub(normal, mul(tangents[i], dot(normal, tangents[i]))))
        out.append((pos, r, normal, unit(cross(tangents[i], normal))))
    return out


def drawn_triangles(gen, place):
    """The outer wall of every drawn spine of a thread.py generator, in world metres."""
    at = add([c / 1e4 for c in place["transform"]["translation"]],
             [c / 1e4 for c in gen.get("transform", {}).get("translation", [0, 0, 0])])
    tris = []
    for s in gen["children"]:
        scale = [c / 1e4 for c in s.get("transform", {}).get("scale", [10000] * 3)]
        points = [([c / 1e4 for c in p["position"]], p["radius"] / 1e4) for p in s["points"]]
        res = max(3, min(64, s["resolution"]))
        rings = [[add(pos, mul(add(mul(n, math.cos(math.tau * j / res)), mul(b, math.sin(math.tau * j / res))), r))
                  for j in range(res + 1)] for pos, r, n, b in stations(points, s["samples_per_segment"])]
        world = [[[at[k] + v[k] * scale[k] for k in range(3)] for v in ring] for ring in rings]
        for r0, r1 in zip(world, world[1:]):
            for j in range(res):
                tris.append((r0[j], r1[j], r1[j + 1]))
                tris.append((r0[j], r1[j + 1], r0[j + 1]))
    return tris


def top_at(tris, x, z):
    """The highest crossing of the vertical line through (x, z) with the triangles, or None."""
    best = None
    for a, b, c in tris:
        if not (min(a[0], b[0], c[0]) <= x <= max(a[0], b[0], c[0])
                and min(a[2], b[2], c[2]) <= z <= max(a[2], b[2], c[2])):
            continue
        det = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2])
        if abs(det) < 1e-14:
            continue
        l1 = ((b[2] - c[2]) * (x - c[0]) + (c[0] - b[0]) * (z - c[2])) / det
        l2 = ((c[2] - a[2]) * (x - c[0]) + (a[0] - c[0]) * (z - c[2])) / det
        if min(l1, l2, 1 - l1 - l2) < -1e-9:
            continue
        y = l1 * a[1] + l2 * b[1] + (1 - l1 - l2) * c[1]
        best = y if best is None else max(best, y)
    return best


def lay(out, name, spec, options, ground):
    """Run thread.py on a stubbed ground; returns the drawn triangles of what it wrote."""
    argv = ["thread.py", "did:plc:test", "record.json", name, out, spec] + options
    with mock.patch.object(thread, "ground", lambda did, record, xz: ([ground(x, z) for x, z in xz], -1000.0)), \
            mock.patch.object(sys, "argv", argv), contextlib.redirect_stdout(io.StringIO()):
        thread.main()
    with open(os.path.join(out, f"{name}.gen.json")) as g, open(os.path.join(out, f"{name}.place.json")) as p:
        return drawn_triangles(json.load(g), json.load(p))


LANE = ["--radius", "4.0", "--flat", "0.08", "--lift", "0.055", "--resolution", "28"]
RUT_LIFT = 0.008


def rut(off):
    return ["--radius", "0.21", "--flat", "0.06", "--lift", str(RUT_LIFT), "--ride", f"4.0,0.08,0.055,{off}",
            "--resolution", "12", "--taper-start", "--taper-end"]


class Ride(unittest.TestCase):
    """thread.py --ride: a thread laid on a flat lane's drawn top, --lift over it."""

    def clearance(self, ground, z, off):
        with tempfile.TemporaryDirectory() as out:
            lane = lay(out, "lane", "-20,0 20,0", LANE, ground)
            thread_ = lay(out, "rut", f"-20,{z} 20,{z}", rut(off), ground)
        return [top_at(thread_, x, z) - top_at(lane, x, z) for x in (-6.0, -3.0, 0.0, 3.0, 6.0)]

    def test_a_rut_on_a_side_slope_reads_the_ground_under_the_lanes_middle(self):
        # a 3% grade along the lane and a 5% slope across it (rising to the south, +Z): the lane rests on the
        # ground under its middle, so a rut read from the ground under itself was 3.6 cm off, the downhill
        # one buried and the uphill one proud
        def ground(x, z):
            return 0.03 * x + 0.05 * z
        for z, off in ((0.725, 0.725), (-0.725, -0.725)):   # heading east, the middle is north: left of the
            for gap in self.clearance(ground, z, off):          # south rut, right of the north one
                self.assertAlmostEqual(gap, RUT_LIFT, delta=0.004, msg=f"rut at z {z}")

    def test_a_verge_on_a_grade_takes_the_lens_fall_of_the_graded_section(self):
        # on a 10% grade the lane's section is sqrt(F^2 + g^2) / F = 2.0 times as tall as on level ground, so
        # its top 3 m out falls twice as far: the level ground's fall left the thread 6.5 cm proud
        for gap in self.clearance(lambda x, z: 0.10 * x, 3.0, 3.0):
            self.assertAlmostEqual(gap, RUT_LIFT, delta=0.004)

    def test_a_ride_past_the_lanes_edge_is_refused(self):
        # OFF 4 on a 4 m lane was clamped to the lane's whole fall: the thread sat at the lane's centreline
        # height, under the ground, without a word
        with tempfile.TemporaryDirectory() as out, self.assertRaises(SystemExit) as refused:
            lay(out, "rut", "-20,4 20,4", rut(4.0), lambda x, z: 0.0)
        self.assertIn("past the lane's edge", str(refused.exception.code))


class Apply(unittest.TestCase):
    """rec.py apply's closing CHECK line."""

    def test_a_dropped_key_is_not_called_an_ignored_edit(self):
        # the daemon writes the edit, then names the keys the record has no field for: the edit is live
        answer = {"ok": True, "result": {"changed": True, "adjusted_at": [], "z_fighting": [],
                                         "ignored_at": ["/generators/yew/material/texture/color_base"],
                                         "record_size": {"largest": "room", "bytes": 1, "budget_bytes": 2}}}
        with tempfile.TemporaryDirectory() as d:
            with open(os.path.join(d, "yew.json"), "w") as fh:
                json.dump({}, fh)
            edits = os.path.join(d, "EDITS")
            with open(edits, "w") as fh:
                fh.write("/generators/yew yew.json\n")
            out = io.StringIO()
            with mock.patch.object(rec.agentlib, "agent", lambda *args: answer), \
                    mock.patch.object(sys, "argv", ["rec.py", "apply", "room", edits]), \
                    contextlib.redirect_stdout(out), self.assertRaises(SystemExit) as stopped:
                rec.main()
        self.assertEqual(stopped.exception.code, 3)
        check = [line for line in out.getvalue().splitlines() if line.startswith("CHECK:")]
        self.assertEqual(check, ["CHECK: 1 edit(s) ignored a key - read the lines above, then save (or fix) on "
                                 "purpose"])


if __name__ == "__main__":
    unittest.main()
