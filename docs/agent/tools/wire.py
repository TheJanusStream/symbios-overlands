"""Record wire-form helpers for builder scripts (see ../building.md, "The JSON").

Import it from a builder (`sys.path.insert(0, "<repo>/docs/agent/tools")`) and
write every piece from named dimensions in metres; these convert to the wire's
whole ten-thousandths. A builder prints or dumps the generator JSON that
`room set /generators/<name> --file` or `avatar set ... --file` takes.
"""
import json
import math

S = 10000  # wire scale: metres x 10 000


def w(v):
    return int(round(v * S))


def w3(v):
    return [w(c) for c in v]


def lin(c):
    """sRGB to LINEAR, for a procedural texture's colours (a material's base_color stays sRGB)."""
    return ((c + 0.055) / 1.055) ** 2.4


def norm(v):
    n = math.sqrt(sum(c * c for c in v))
    return [c / n for c in v]


def quat_axis_angle(axis, deg):
    ax = norm(axis)
    h = math.radians(deg) / 2
    s = math.sin(h)
    return [ax[0] * s, ax[1] * s, ax[2] * s, math.cos(h)]


def quat_mul(a, b):
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return [aw * bx + ax * bw + ay * bz - az * by,
            aw * by - ax * bz + ay * bw + az * bx,
            aw * bz + ax * by - ay * bx + az * bw,
            aw * bw - ax * bx - ay * by - az * bz]


def quat_from_to(u, v):
    """The shortest rotation taking unit vector u onto unit vector v (not for u == -v)."""
    ux, uy, uz = u
    vx, vy, vz = v
    q = [uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx, 1 + ux * vx + uy * vy + uz * vz]
    return norm(q)


def tf(t=None, r=None, s=None):
    out = {}
    if t is not None:
        out["translation"] = w3(t)
    if r is not None:
        out["rotation"] = [w(c) for c in r]
    if s is not None:
        out["scale"] = w3(s)
    return out


def mat(base, emit=None, strength=None, rough=0.8, texture=None, uv_scale=None):
    """A material: base_color is sRGB; a glow reads when emission matches a deep, saturated base."""
    m = {"base_color": w3(base), "roughness": w(rough)}
    if emit is not None:
        m["emission_color"] = w3(emit)
        m["emission_strength"] = w(strength)
    if texture is not None:
        m["texture"] = texture
    if uv_scale is not None:
        m["uv_scale"] = w(uv_scale)  # repeats per METRE of surface
    return m


def _with_tf(node, t, r, s):
    tr = tf(t, r, s)
    if tr:
        node["transform"] = tr
    return node


def solid_root(children):
    """A 5 cm solid cuboid at the ground point with NO transform: under a non-solid root every
    collider misplaced before #1453, and a transform here would move (and scale) every child."""
    return {"$type": "network.symbios.gen.cuboid", "size": w3([0.05, 0.05, 0.05]), "solid": True,
            "children": children}


def lathe(points, material, t=None, r=None, s=None, solid=False, resolution=24, smooth=True):
    """points: (radius, height) stations, counter-clockwise - out along the underside, back over the top."""
    return _with_tf({"$type": "network.symbios.gen.lathe", "material": material, "solid": solid,
                     "smooth": smooth, "resolution": resolution,
                     "points": [{"radius": w(rad), "height": w(h)} for rad, h in points]}, t, r, s)


def spine(points, material, resolution=8, samples=4, solid=False, t=None):
    """points: ([x, y, z], radius) - at most 100 m from the generator's origin, or clamped."""
    return _with_tf({"$type": "network.symbios.gen.spine", "material": material, "solid": solid,
                     "resolution": resolution, "samples_per_segment": samples,
                     "points": [{"position": w3(p), "radius": w(rad)} for p, rad in points]}, t, None, None)


def sphere(radius, material, t=None, r=None, s=None, res=2, solid=False):
    return _with_tf({"$type": "network.symbios.gen.sphere", "radius": w(radius), "resolution": res,
                     "solid": solid, "material": material}, t, r, s)


def dump(obj, path):
    json.dump(obj, open(path, "w"), indent=1)
    print(f"wrote {path} ({len(json.dumps(obj, separators=(',', ':')))} bytes compact)")
