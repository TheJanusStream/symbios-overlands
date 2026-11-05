#!/usr/bin/env python3
"""The stand-in bird for the flight prototype (#1861).

Prints the two fixtures beside this script, every number derived from the
named dimensions below:

* standin_bird_visuals.json - the generator body, as
  `avatar set /record/body/visuals` takes it;
* standin_bird_avatar.json - a whole avatar record: that body on the on-foot
  preset with a flight section, as `avatar set /record` takes it.

Body space: the origin is the capsule's middle, front is -Z, the feet stand
at -HEIGHT / 2, the collider's bottom. Wire numbers are ten-thousandths.

Run: python3 tests/fixtures/winged/standin_bird.py
"""

import json
import math
import os

# The capsule: the walk holds the body up by it, so the feet are its bottom.
CAPSULE_RADIUS = 0.2
CAPSULE_LENGTH = 0.35
HEIGHT = CAPSULE_LENGTH + 2 * CAPSULE_RADIUS  # 0.75 m
FEET_Y = -HEIGHT / 2

# The drawn bird.
LEG_RADIUS = 0.025
LEG_LENGTH = 0.18
LEG_ASIDE = 0.07
FOOT = (0.08, 0.02, 0.12)
BODY = (0.2, 0.18, 0.28)  # the ellipsoid's semi-axes
BODY_AT = (0.0, FEET_Y + LEG_LENGTH + BODY[1] - 0.03, 0.02)
HEAD_RADIUS = 0.13
HEAD_AT = (0.0, BODY_AT[1] + 0.24, -0.2)
EYE_RADIUS = 0.03
PUPIL_RADIUS = 0.016
BEAK_RADIUS = 0.035
BEAK_LENGTH = 0.1
SHOULDER_ASIDE = BODY[0] * 0.85
SHOULDER_AT_Y = BODY_AT[1] + 0.05
WING = (0.42, 0.025, 0.2)  # span, thickness, chord
TIP = (0.2, 0.02, 0.14)
FLAP_DEGREES = 30.0  # either side of level
FLAP_PERIOD_S = 0.6  # the sanitiser's floor is 0.5
TAIL = (0.16, 0.02, 0.18)
TAIL_LIFT_DEGREES = 15.0

BROWN = [0.45, 0.32, 0.2]
DARK = [0.28, 0.19, 0.12]
CREAM = [0.93, 0.87, 0.72]
BLACK = [0.05, 0.04, 0.04]
ORANGE = [0.95, 0.6, 0.15]


def w(x):
    """A metre (or a degree, or a share) on the wire."""
    return int(round(x * 10000))


def w3(v):
    return [w(c) for c in v]


def quat_x(degrees):
    """A turn about +X, as a wire quaternion."""
    half = math.radians(degrees) / 2
    return [w(math.sin(half)), 0, 0, w(math.cos(half))]


def material(colour, roughness=0.8):
    return {"base_color": w3(colour), "roughness": w(roughness)}


def transform(at=None, rotation=None, scale=None):
    t = {}
    if at is not None:
        t["translation"] = w3(at)
    if rotation is not None:
        t["rotation"] = rotation
    if scale is not None:
        t["scale"] = w3(scale)
    return t


def node(kind, colour, at=None, rotation=None, scale=None, children=None, spin=None, **fields):
    n = {"$type": "network.symbios.gen." + kind, **fields, "solid": False}
    n["material"] = material(colour)
    t = transform(at, rotation, scale)
    if t:
        n["transform"] = t
    if children:
        n["children"] = children
    if spin:
        n["spin"] = spin
    return n


def sphere(radius, colour, at=None, scale=None, resolution=3, **rest):
    return node("sphere", colour, at=at, scale=scale, radius=w(radius), resolution=resolution, **rest)


def cuboid(size, colour, at=None, rotation=None):
    return node("cuboid", colour, at=at, rotation=rotation, size=w3(size))


def wing(side):
    """One wing: a shoulder joint that swings about the body's fore-aft
    axis, the wing and its darker tip hung from it, outward along +X for the
    right wing (side 1) and -X for the left (side -1). The left wing's axis
    is reversed, so both rise together."""
    out = WING[0] / 2
    tip_out = WING[0] + TIP[0] / 2 - 0.02
    return sphere(
        0.045,
        BROWN,
        at=(side * SHOULDER_ASIDE, SHOULDER_AT_Y, -0.02),
        children=[
            cuboid(WING, BROWN, at=(side * out, 0.0, 0.02)),
            cuboid(TIP, DARK, at=(side * tip_out, 0.0, 0.05)),
        ],
        spin={
            "axis": [0, 0, w(side)],
            "terms": [
                {
                    "$type": "network.symbios.spin.swing",
                    "amplitude": w(FLAP_DEGREES),
                    "period": w(FLAP_PERIOD_S),
                }
            ],
        },
    )


def leg(side):
    x = side * LEG_ASIDE
    return node(
        "cylinder",
        ORANGE,
        at=(x, FEET_Y + LEG_LENGTH / 2, 0.0),
        radius=w(LEG_RADIUS),
        height=w(LEG_LENGTH),
        resolution=8,
        children=[cuboid(FOOT, ORANGE, at=(0.0, -LEG_LENGTH / 2 + FOOT[1] / 2, -0.03))],
    )


def eye(side):
    hx, hy, hz = HEAD_AT
    at = (hx + side * 0.065, hy + 0.035, hz - HEAD_RADIUS * 0.82)
    return sphere(
        EYE_RADIUS,
        CREAM,
        at=at,
        resolution=2,
        children=[sphere(PUPIL_RADIUS, BLACK, at=(side * 0.004, 0.002, -EYE_RADIUS * 0.75), resolution=1)],
    )


def visuals():
    # The root stands at the origin with no transform, hidden in the body,
    # so every part's translation is a body-space position.
    return sphere(
        0.05,
        BROWN,
        resolution=1,
        children=[
            sphere(1.0, BROWN, at=BODY_AT, scale=BODY),
            sphere(HEAD_RADIUS, BROWN, at=HEAD_AT),
            eye(1),
            eye(-1),
            node(
                "cone",
                ORANGE,
                at=(HEAD_AT[0], HEAD_AT[1] - 0.03, HEAD_AT[2] - HEAD_RADIUS - BEAK_LENGTH / 2 + 0.02),
                # Apex from +Y to -Z: a quarter turn about X, backwards.
                rotation=quat_x(-90.0),
                radius=w(BEAK_RADIUS),
                height=w(BEAK_LENGTH),
                resolution=12,
            ),
            cuboid(TAIL, DARK, at=(0.0, BODY_AT[1] + 0.02, BODY_AT[2] + BODY[2] + 0.05), rotation=quat_x(-TAIL_LIFT_DEGREES)),
            wing(1),
            wing(-1),
            leg(1),
            leg(-1),
        ],
    )


def avatar():
    return {
        "$type": "network.symbios.overlands.avatar",
        "body": {
            "$type": "network.symbios.overlands.avatar#generator",
            "visuals": visuals(),
        },
        "locomotion": {
            "$type": "network.symbios.locomotion.humanoid",
            "capsule_radius": w(CAPSULE_RADIUS),
            "capsule_length": w(CAPSULE_LENGTH),
            "mass": w(8.0),
            "linear_damping": w(0.3),
            "walk_speed": w(3.0),
            "acceleration": w(12.0),
            "jump_impulse": w(32.0),
            "swim_speed": w(1.5),
            "swim_vertical_speed": w(1.2),
            "wading_speed_factor": w(0.5),
            "stop_damping": w(20.0),
            "turn_rate": w(12.0),
            # `FlightParams::for_height(0.75)`, the defaults.
            "flight": {
                "flight_power": w(1.0),
                "glide_ratio": w(5.0),
                "trim_speed": w(6.5),
                "stall_speed": w(3.5),
                "beat_speed": w(2.0),
                "wingbeat_rate": w(3.0),
                "stamina_beats": w(24.0),
                "stamina_recovery": w(8.0),
                "takeoff_speed": w(3.0),
                "max_bank": w(50.0),
                "roll_rate": w(180.0),
                "pitch_rate": w(90.0),
            },
        },
    }


if __name__ == "__main__":
    here = os.path.dirname(os.path.abspath(__file__))
    for name, value in (("standin_bird_visuals.json", visuals()), ("standin_bird_avatar.json", avatar())):
        with open(os.path.join(here, name), "w") as out:
            json.dump(value, out, indent=1)
            out.write("\n")
