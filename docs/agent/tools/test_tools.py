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
import near  # noqa: E402
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


    def test_a_grammar_left_unchecked_for_z_fighting_stops_the_chain(self):
        # past 3 s the daemon's check names what it did not finish (#1503): z_fighting=0 there is
        # not an all-clear, so the apply must not read as clean
        answer = {"ok": True, "result": {"changed": True, "adjusted_at": [], "z_fighting": [],
                                         "z_fighting_unchecked": ["/generators/town"], "ignored_at": [],
                                         "record_size": {"largest": "room", "bytes": 1, "budget_bytes": 2}}}
        with tempfile.TemporaryDirectory() as d:
            with open(os.path.join(d, "town.json"), "w") as fh:
                json.dump({}, fh)
            edits = os.path.join(d, "EDITS")
            with open(edits, "w") as fh:
                fh.write("/generators/town town.json\n")
            out = io.StringIO()
            with mock.patch.object(rec.agentlib, "agent", lambda *args: answer), \
                    mock.patch.object(sys, "argv", ["rec.py", "apply", "room", edits]), \
                    contextlib.redirect_stdout(out), self.assertRaises(SystemExit) as stopped:
                rec.main()
        self.assertEqual(stopped.exception.code, 3)
        text = out.getvalue()
        self.assertIn("z-fighting not checked in time for: /generators/town", text)
        self.assertIn("CHECK: 1 edit(s) left z-fighting unchecked", text)

    def test_the_usage_names_every_reason_apply_stops(self):
        # the usage text is what rec.py prints for a bad call: it listed three reasons to exit 3 after
        # the check's time limit (#1503) made a fourth (session 885's review)
        answer = {"ok": True, "result": {"changed": True, "adjusted_at": ["/generators/a/x"],
                                         "z_fighting": [{"a": "/generators/a", "b": "/generators/b"}],
                                         "z_fighting_unchecked": ["/generators/a"],
                                         "ignored_at": ["/generators/a/y"],
                                         "record_size": {"largest": "room", "bytes": 1, "budget_bytes": 2}}}
        with tempfile.TemporaryDirectory() as d:
            with open(os.path.join(d, "a.json"), "w") as fh:
                json.dump({}, fh)
            edits = os.path.join(d, "EDITS")
            with open(edits, "w") as fh:
                fh.write("/generators/a a.json\n")
            out = io.StringIO()
            with mock.patch.object(rec.agentlib, "agent", lambda *args: answer), \
                    mock.patch.object(sys, "argv", ["rec.py", "apply", "room", edits]), \
                    contextlib.redirect_stdout(out), self.assertRaises(SystemExit):
                rec.main()
        [check] = [line for line in out.getvalue().splitlines() if line.startswith("CHECK:")]
        labels = [part.split(" edit(s) ", 1)[1] for part in check[len("CHECK: "):].split(" - ")[0].split(", ")]
        self.assertEqual(len(labels), 4, check)
        usage = " ".join(rec.__doc__.split())
        for label in labels:
            self.assertIn(label, usage)

class Near(unittest.TestCase):
    """near.py's footprint reading (--box), session 885."""

    RECORD = {"placements": [
        {"$type": "network.symbios.place.absolute", "generator_ref": "post",
         "transform": {"translation": [100000, 0, 30000]}},
        {"$type": "network.symbios.place.scatter", "generator_ref": "apple",
         "bounds": {"type": "circle", "center": [120000, 0], "radius": 70000}},
    ]}

    def distance(self, name, box, within=20.0):
        return {s.split(" #")[0]: (d, s) for d, s in near.near(self.RECORD, 0.0, 0.0, within, box)}.get(name)

    def test_a_box_measures_from_its_edge_turned_as_place_yaw_counts(self):
        # 16 x 4 m round (0, 0), the post at (10, 3): past its east end by 2 m and off its axis by 1 m
        self.assertAlmostEqual(self.distance("post", (16.0, 4.0, 0.0))[0], math.hypot(2.0, 1.0))
        # place --yaw 90 turns the long side north-south: the post is 10 - 4/2 = 8 m off its side
        self.assertAlmostEqual(self.distance("post", (16.0, 4.0, 90.0))[0], 8.0)
        # clockwise seen from above turns the box's east end toward +Z (south), toward the post 16.7
        # degrees south of east: at 30 it lies near the axis, 2.2 m past the end; at -30 it is 5.6 m
        # off the side - which way the box turns decides it
        self.assertLess(self.distance("post", (16.0, 4.0, 30.0))[0], 2.5)
        self.assertGreater(self.distance("post", (16.0, 4.0, -30.0))[0], 5.0)

    def test_a_scatter_reaching_into_a_box_says_how_far(self):
        d, text = self.distance("apple", (16.0, 4.0, 0.0))
        self.assertAlmostEqual(d, 4.0)
        self.assertIn("reaches 3.0 m in", text)
        # measured from the box's centre alone the garth is 12 m off: the point reading's blind spot
        self.assertNotIn("reaches", self.distance("apple", None)[1])

    def test_a_scatter_is_listed_by_its_reach_from_a_box_but_its_centre_from_a_point(self):
        # the apple garth's edge is 4 - 7 < 0 m from the box, so a 1 m search lists it; a point search
        # from (0, 0) keeps the old rule, its centre 12 m away, outside 8 m
        self.assertIsNotNone(self.distance("apple", (16.0, 4.0, 0.0), within=1.0))
        self.assertIsNone(self.distance("apple", None, within=8.0))

    def test_a_thing_inside_a_box_is_0_m_off(self):
        # a 24 x 8 box round (0, 0) holds the post at (10, 3): 0 m, not the 1 m to its nearest side -
        # the horse set inside a cottage (session 879) is what the reading is for
        self.assertEqual(self.distance("post", (24.0, 8.0, 0.0))[0], 0.0)

    def test_only_a_reach_that_comes_in_is_named_and_the_edge_decides_the_listing(self):
        # the post 2.2 m off the 16 x 4 box reaches nothing in, so it says nothing
        self.assertNotIn("reaches", self.distance("post", (16.0, 4.0, 0.0))[1])
        # a 2 x 2 box: the garth's centre is 11 m off its edge, so its 7 m circle stops 4 m short
        box = (2.0, 2.0, 0.0)
        self.assertNotIn("reaches", self.distance("apple", box)[1])
        self.assertIsNotNone(self.distance("apple", box, within=4.5))
        self.assertIsNone(self.distance("apple", box, within=3.5))
        # without a box nothing reaches in: the note is the footprint reading's
        self.assertNotIn("reaches", near.near(self.RECORD, 12.0, 0.0, 8.0, None)[0][1])


class NearRect(unittest.TestCase):
    """near.py --box reads a rect scatter as the rectangle it is, turned by its `rotation` as the
    world's sampler turns it (session 885's review: read as the circle through its corners, a field 5 m
    clear 'reached 21.3 m in')."""

    # a 60 x 8 m field (half sizes 30 x 4) whose south edge is at z = -8
    FIELD = {"$type": "network.symbios.place.scatter", "generator_ref": "wheat",
             "bounds": {"type": "rect", "center": [0, -120000], "extents": [300000, 40000], "rotation": 0}}

    def record(self, rotation=0, center=None, extents=None):
        field = json.loads(json.dumps(self.FIELD))
        field["bounds"]["rotation"] = rotation
        if center:
            field["bounds"]["center"] = center
        if extents:
            field["bounds"]["extents"] = extents
        return {"placements": [field]}

    def test_a_field_clear_of_a_box_is_listed_by_its_gap(self):
        # the 10 x 6 box round (0, 0) ends at z = -3: the field is 5 m clear
        box = (10.0, 6.0, 0.0)
        self.assertEqual(near.near(self.record(), 0.0, 0.0, 4.9, box), [])
        [(_, text)] = near.near(self.record(), 0.0, 0.0, 5.1, box)
        self.assertNotIn("reaches", text)

    def test_a_field_turned_through_a_box_says_how_far_in(self):
        # a quarter turn (pi/2 on the wire) runs it north-south through the box: x in [-4, 4] against
        # the box's [-5, 5], so the least move that parts them is 4 + 5 = 9 m
        [(_, text)] = near.near(self.record(15708), 0.0, 0.0, 0.0, (10.0, 6.0, 0.0))
        self.assertIn("reaches 9.0 m in", text)

    def test_a_strip_beside_the_boxs_end_is_its_true_gap(self):
        # a 1 x 40 m strip at (8, -8) spans x 7.5 to 8.5; the box's east end is at x = 5: 2.5 m clear,
        # though its corners' circle would reach 20 m in
        strip = self.record(center=[80000, -80000], extents=[5000, 200000])
        self.assertEqual(near.near(strip, 0.0, 0.0, 2.4, (10.0, 6.0, 0.0)), [])
        self.assertEqual(len(near.near(strip, 0.0, 0.0, 2.6, (10.0, 6.0, 0.0))), 1)

    def test_a_turned_field_is_turned_as_the_world_scatters_it(self):
        # the box turned 30 degrees clockwise, the field turned the same 30 (0.5236 rad) with its
        # centre 12 m along the box's own +Z: side by side, 12 - 4 - 3 = 5 m clear
        t = math.radians(30)
        center = [round(-12 * math.sin(t) * 1e4), round(12 * math.cos(t) * 1e4)]
        box = (10.0, 6.0, 30.0)
        self.assertEqual(near.near(self.record(5236, center), 0.0, 0.0, 4.9, box), [])
        self.assertEqual(len(near.near(self.record(5236, center), 0.0, 0.0, 5.1, box)), 1)
        # turned the other way its long side swings into the box: the sign of the turn decides it
        [(_, text)] = near.near(self.record(-5236, center), 0.0, 0.0, 0.0, box)
        self.assertIn("reaches", text)


if __name__ == "__main__":
    unittest.main()
