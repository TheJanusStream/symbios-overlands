#!/usr/bin/env python3
"""Builder for Ashmere's croft apple (Malus domestica, an old medieval
orchard tree in late September, the fruit ripe).

usage: apple.py [VARIANT [OUT.json [key=value ...]]]
       apple.py          -> the shipped tree: variant a, BASE, apple.json
                            in the current directory

Writes one network.symbios.gen.lsystem generator in wire form (every
decimal a whole number of ten-thousandths). Grammar units are metres
BEFORE the root transform scale (P["scale"], 1.0 here).

The shipped tree (iterations 10, seed 1) is 7.1 m wide and 5.55 m tall
above the ground on a 1.3 m bole leaning 11 deg, 3,890 triangles in 3
parts at mesh_resolution 5, with 65 apples (1,300 of the triangles;
`fruit=1,0,0,0` leaves a 2,590-triangle, two-part tree). Iterations are
age: 8 is a 4.8 m tree, 6 a 3.9 m one (3,210 triangles), 12 (the cap) a
6.1 m veteran; 8 and up cost the same 3,890. Measure with `render
--generator apple.json` (the `subject size` line; its height includes the
0.1 m of trunk sunk below the origin, `sink`).

It is the oak's grammar (oak.py beside this file: a dichotomous
sympodial crown grown by age, the last fork drawn by cards alone, a
card mass at every tip, a side spray at every fork past the first, every
shoot started at its own width and every limb launched from the foot)
scaled to a fruit tree: short internodes, four scaffolds from a leaning
bole, a higher elasticity so the laden outer twigs hang, 7-card tip
masses and 4-card sprays, and a fruit slot.

Variants (judged on play views and in the world, Ashmere's evening light,
at 5, 9, 15, 50 and 150 m):
  a  SHIPPED. Each apple is solid: a two-segment tube in the fruit slot
     (three rings, 20 triangles), 0.19 m across, narrow at the stalk end,
     full at the flank, narrow at the eye, its skin the rings' vertex
     colours - a red flank flushed over a yellow-green stalk end and a
     dark red eye, or (about one in seven) all yellow-green. A tip
     carries none, one red, one green or two red, by weight
     (0.45/0.3/0.1/0.15); each hangs out through the lower side of its
     tip's mass, pitched 48 or 65 deg below the tip's heading and
     0.55-0.6 m out, on a line back to the twig. 65 apples: red dots
     through the crown at 15 and 50 m, a few green ones.
  c  REJECTED, the first shipped fruit (a's wood with the designer's 8-card
     masses, 6-card sprays and card apples; 3,460 triangles): 125 apples,
     each two crossed Leaf cards carrying a SoftDisc sprite, hung 0.71 m
     ahead of the tip. The critic's findings: the second card, seen edge
     on, drew a line through the middle of almost every fruit at 5-15 m
     (creased peaches, sliced fruit); where a tip pointed up the apple
     floated above the leaves; the fruit measured hue 13-17, orange-red,
     no green at 15 or 50 m.
  b  REJECTED (c with apple=0.12 fruit=0.45,0.35,0,0.2 fruit_out=0.3
     spray_fruit=0; 3,252 triangles): 0.12 m card apples hung inside the
     leaf masses - at 15 m orange specks, most hidden; at 50 m none. A
     September apple tree is read by its fruit.
  Tube apples, tried: hung 0.5 m straight down from the tip (pitched 85-
  105 deg) they sat under the masses, hidden from the side - at 15 m a
  handful of dots; pitched 35 deg they stood on top of the crown. With
  8-card masses and 6-card sprays the budget held 50 apples and the tree
  did not read as laden; one card fewer a mass and two fewer a spray (370
  triangles) paid for 16 more, and the thinner masses show more of them.
  Not tried, by arithmetic: a Sphere prop is an ico(2) sphere, 320
  triangles an apple; a third ring (30 triangles) would round the apple
  at 5 m but cost a third of the fruit.
Rejected settings: a card apple's halo of (0.62, 0.66, 0.22) read orange
in the warm evening light. A core of (0.78, 0.08, 0.05) came out
salmon-pink in the middle of each card apple (a guess: the tonemap
desaturating a bright saturated red; not checked). Bark as pale as the
oak's first try reads as driftwood (the Bark texture's colour follows its
FBM only, so it wants near-black to mid-grey and a strong normal).

Materials (three parts per copy): slot 0 bark - a Bark texture, scaly
grey-brown (short plates: furrow_scale_v 1.0, `;(0.7)`); slot 1 foliage -
a Twig card of 7 alternate, oval, finely toothed leaves, mid green; slot 2
fruit - no texture at all, white, roughness 0.55: the tubes' vertex
colours are the skin (`'(r,g,b)` is linear RGB; `rgb` converts from
sRGB). A texture-less material slot draws vertex colour times base
colour.
"""
import json
import sys

S = 10000


def w(v):
    return int(round(v * S))


def lin(c):
    """sRGB -> linear, for procedural texture colours."""
    return ((c + 0.055) / 1.055) ** 2.4


def wl(rgb):
    return [w(lin(c)) for c in rgb]


def f(x):
    """A compact decimal for grammar text."""
    s = ("%.4f" % x).rstrip("0").rstrip(".")
    return s if s not in ("-0", "") else "0"


# ---------------------------------------------------------------- parameters
BASE = dict(
    scale=1.0,
    iterations=10,        # = age
    mesh_resolution=5,
    elasticity=0.06,      # higher than the oak's: the laden outer twigs hang
    seed=1,
    # --- growth law of every internode: r * m * min(lm, l0 + dl * age)
    l0=0.35, dl=0.1, lm=1.2,
    # --- trunk
    wt=0.11, vt=1.11,     # 0.35 m at 10
    sink=0.1,             # trunk base below the origin: the foot (0.22 m
                          # out) stays buried on a 24 deg slope
    bole=[(0.12, 1.4), (0.6, 1.1), (1.3, 1.0)],   # (height at maturity, width factor)
    lean=11.0,            # one-shot lean after the foot; the tropism arcs it
    # scaffolds: (after bole segment, azimuth, pitch from vertical, vigour,
    # generations short of gens) - more upright than the oak's
    limbs=[(2, 30, 50, 1.0, 0), (2, 150, 58, 0.95, 0), (2, 265, 46, 1.0, 0), (1, 330, 62, 0.85, 1)],
    leader=(8, 0.85, 1),
    # --- branching (as oak.py)
    gens=5, ghost=1,
    bw=0.075, vb=1.12,
    kf=(0.82, 0.78), fork_roll=(40.0, 110.0), fork_b=(32.0, 34.0),
    k_elb=0.9, elb_roll=40.0, elb=26.0,
    p_fork=0.62, p_elb=0.24, p_stall=0.14,
    dsplit=2, crook=18.0,
    collar_d=2,           # forks of generation < collar_d draw the 1 cm collar (see oak.py)
    shoot_start=1.0,      # a fork shoot's first ring over its own width (see start)
    # --- foliage
    cs=0.95, back=0.35, clump_n=7,   # 7-card masses, 4-card sprays: the cards given up pay for apples
    dspray=1, spray=0.8, spray_n=4, spray_roll=140.0, spray_pitch=55.0, spray_fwd=0.2,
    leaf_base=(0.22, 0.33, 0.14), leaf_edge=(0.28, 0.37, 0.15),
    stem=(0.35, 0.26, 0.18),
    leaf_pairs=7, leaf_scale=0.32, leaf_angle=1.9,
    # --- fruit
    fruit_kind="tube",    # "tube": the shipped solid apple; "card": crossed SoftDisc cards
    fruit=(0.45, 0.3, 0.1, 0.15),    # weights of a tip bearing none / a red / a green / two red
    spray_fruit=0.0,      # share of sprays bearing a red one
    fruit_at=[(20.0, 48.0, 0.6), (160.0, 65.0, 0.55)],   # (roll, pitch down, metres) from the tip
    apple=0.19,           # apple size (m): real ones are 7-8 cm; 0.19 reads at 50 m
    red=((0.52, 0.56, 0.16), (0.50, 0.06, 0.05), (0.36, 0.04, 0.04)),    # sRGB: stalk end, flank, eye
    green=((0.56, 0.62, 0.18), (0.54, 0.58, 0.14), (0.40, 0.48, 0.10)),
    fruit_rough=0.55,
    # the card apple (fruit_kind "card", presets b and c)
    fruit_out=0.75,       # how far out along the tip mass they hang (x cs)
    apple_core=(0.55, 0.06, 0.04), apple_halo=(0.30, 0.46, 0.12),   # a brighter red went salmon-pink
    # --- bark
    bark_light=(0.42, 0.40, 0.38), bark_dark=(0.02, 0.02, 0.02),
    bark_scale=3.0, bark_normal=8.0, bark_warp_u=0.2, bark_warp_v=0.4,
    furrow=0.9, furrow_u=3.0, furrow_v=1.0, furrow_shape=1.5,
    bark_uv=2.0, bark_vs=0.7,
    wood_only=0,
)

VARIANTS = {
    "a": {},
    "b": dict(fruit_kind="card", apple=0.12, fruit=(0.45, 0.35, 0.0, 0.2), fruit_out=0.3,
              spray_fruit=0.0, apple_halo=(0.62, 0.66, 0.22), clump_n=8, spray_n=6),
    "c": dict(fruit_kind="card", apple=0.16, fruit=(0.2, 0.45, 0.0, 0.35), spray_fruit=0.4,
              clump_n=8, spray_n=6),
}


def leaf_texture(P):
    return {
        "$type": "Twig",
        "leaf": {
            "color_base": wl(P["leaf_base"]),
            "color_edge": wl(P["leaf_edge"]),
            "serration_strength": w(0.14),
            "vein_angle": w(2.4),
            "micro_detail": w(0.2),
            "normal_strength": w(0.4),
            "lobe_count": 0,
            "lobe_depth": 0,
            "lobe_sharpness": w(1.0),
            "petiole_length": w(0.08),
            "petiole_width": w(0.018),
            "midrib_width": w(0.07),
            "vein_count": w(7.0),
            "venule_strength": w(0.15),
        },
        "stem_color": wl(P["stem"]),
        "stem_half_width": w(0.012),
        "leaf_pairs": int(P["leaf_pairs"]),
        "leaf_angle": w(P["leaf_angle"]),
        "leaf_scale": w(P["leaf_scale"]),
        "stem_curve": w(0.01),
        "sympodial": True,
    }


def fruit_texture(P):
    # the sprite's alpha falls off past core_radius and the colour leans to
    # the core as alpha squared, so the masked disc is red with a green rim
    return {
        "$type": "SoftDisc",
        "seed": 3,
        "variant_rows": 1,
        "variant_cols": 1,
        "color_core": wl(P["apple_core"]),
        "color_halo": wl(P["apple_halo"]),
        "core_radius": w(0.45),
        "falloff": w(1.2),
        "ellipticity": w(0.08),
        "scale_jitter": 0,
        "normal_strength": w(2.0),
    }


def bark_material(P):
    return {
        "base_color": [10000, 10000, 10000],
        "roughness": 9500,
        "texture": {
            "$type": "Bark",
            "color_light": wl(P["bark_light"]),
            "color_dark": wl(P["bark_dark"]),
            "scale": w(P["bark_scale"]),
            "warp_u": w(P["bark_warp_u"]),
            "warp_v": w(P["bark_warp_v"]),
            "normal_strength": w(P["bark_normal"]),
            "furrow_multiplier": w(P["furrow"]),
            "furrow_scale_u": w(P["furrow_u"]),
            "furrow_scale_v": w(P["furrow_v"]),
            "furrow_shape": w(P["furrow_shape"]),
        },
        "uv_scale": w(P["bark_uv"]),
    }


def generator(P, src, fin):
    mats = {"0": bark_material(P)}
    if not P["wood_only"]:
        mats["1"] = {"base_color": [10000, 10000, 10000], "roughness": 10000, "texture": leaf_texture(P)}
        if P["fruit_kind"] == "card":
            mats["2"] = {"base_color": [10000, 10000, 10000], "roughness": 9000, "texture": fruit_texture(P)}
        else:
            # no texture: the vertex colours are the apple's skin
            mats["2"] = {"base_color": [10000, 10000, 10000], "roughness": w(P["fruit_rough"])}
    return {
        "$type": "network.symbios.gen.lsystem",
        "angle": 300000,
        "elasticity": w(P["elasticity"]),
        "finalization_code": fin,
        "iterations": int(P["iterations"]),
        "materials": mats,
        "mesh_resolution": int(P["mesh_resolution"]),
        "prop_mappings": {"0": "Twig", "1": "Leaf"} if P["fruit_kind"] == "card" else {"0": "Twig"},
        "prop_scale": 10000,
        "seed": str(int(P["seed"])),
        "source_code": src,
        "step": 10000,
        "transform": {"scale": [w(P["scale"])] * 3},
        "tropism": [0, -10000, 0],
        "width": 1000,
    }


def card(pre, s, slot=1):
    return "[,(%d)%s~(0,%s)]" % (slot, pre, s)


# The tip mass of oak.py: (roll, pitch, card roll, scale, swept back),
# every face kept off level (a level card flares toward a low sun).
MASS = [
    (0, 25, -90, 1.0, 0), (180, 30, -90, 0.95, 0), (60, 45, 40, 1.0, 0), (120, 50, -40, 0.95, 0),
    (240, 50, 40, 0.95, 0), (300, 45, -40, 1.0, 0), (0, 75, -90, 0.9, 0), (180, 80, -90, 0.85, 0),
    (45, 95, -90, 0.9, 0), (315, 95, -90, 0.9, 0),
    (30, 110, -80, 0.9, 1), (150, 120, 75, 0.85, 1), (210, 115, -80, 0.85, 1), (330, 110, 80, 0.9, 1),
    (90, 135, -20, 0.8, 1), (270, 135, 20, 0.8, 1),
]
MASS_ORDER = [0, 1, 2, 3, 4, 5, 8, 9, 10, 12, 6, 7, 11, 13, 14, 15]


def mass(cs, back, n, fwd=0.0):
    out = "$" + ("f(%s)" % f(fwd) if fwd else "")
    for i in MASS_ORDER[:n]:
        rho, beta, sig, k, b = MASS[i]
        pre = ("f(-%s)" % f(back)) if b else ""
        out += card(pre + "/(%s)&(%s)/(%s)" % (f(rho), f(beta), f(sig)), f(cs * k))
    return out


def rgb(c):
    """A vertex colour: linear, from sRGB."""
    return "'(%s,%s,%s)" % tuple(f(lin(x)) for x in c)


def tube_apple(P, pre, kind):
    """One solid apple: a two-segment tube (three rings, 20 triangles at
    mesh_resolution 5) drawn from where `pre` leaves the turtle, along its
    heading - narrow at the stalk end, full at the flank, narrow at the eye
    - its skin the vertex colours of the three rings."""
    a = P["apple"]
    top, mid, eye = P[kind]
    return "[,(2)%s%s!(%s)f(0)%s!(%s)F(%s)%s!(%s)F(%s)]" % (
        pre, rgb(top), f(a * 0.55), rgb(mid), f(a), f(a * 0.45), rgb(eye), f(a * 0.5), f(a * 0.45))


def apple(P, pre, roll):
    """One apple: two crossed Leaf cards (0.5 x 0.8 before scaling) with
    the SoftDisc, hanging down from where `pre` leaves the turtle (a
    card's base sits at the turtle and it runs along the heading). The
    width is 1.6x the height so the disc is round; its masked edge spans
    about 62% of the card."""
    sy = P["apple"] / 0.8 / 0.62
    sc = "%s,%s,1" % (f(sy * 1.6), f(sy))
    return ("[,(2)%s/(%s)~(1,%s)]" % (pre, f(roll), sc) +
            "[,(2)%s/(%s)~(1,%s)]" % (pre, f(roll + 90), sc))


def start(width):
    """Begin a fresh tube here at `width`, as oak.py's `start`: from a bare
    `[` a shoot's tube starts at its parent's width, a fat stub."""
    return "!(%s/vb,1)f(0)" % width


def variant_a(P):
    """oak.py's grammar with a leaning bole and fruit. A(v,s,d): an apex of
    vigour v, side s, generation d; F/f carry (length, fraction, age,
    vigour) and re-lengthen every step; `!(w,t)` thickens every step."""
    G = int(P["gens"])
    lines = [
        "#define l0 %s" % f(P["l0"]), "#define dl %s" % f(P["dl"]), "#define lm %s" % f(P["lm"]),
        "#define bw %s" % f(P["bw"]), "#define vb %s" % f(P["vb"]), "#define vt %s" % f(P["vt"]),
        "#define wt %s" % f(P["wt"]), "#define G %d" % G,
    ]
    lm = P["lm"]
    # One tube for the trunk: every limb is launched from the foot and moves
    # up to its height (and through the lean) with growing `f`s, as in
    # oak.py. The trunk starts `sink` below the placement's origin: a copy
    # snaps to the ground at its origin only, so on a slope the downhill
    # side of the foot would otherwise float.
    om = ";(%s)!(wt*%s,-1)f(-%s)" % (f(P["bark_vs"]), f(P["bole"][0][1]), f(P["sink"]))
    y, trunk, path, rs = -P["sink"], "", "", 0.0
    for i, (h, fl) in enumerate(P["bole"]):
        r = (h - y) / lm
        y = h
        trunk += "!(wt*%s,-1)F(%s*l0,%s,0,1)" % (f(fl), f(r), f(r))
        rs += r
        up = path + "f(%s*l0,%s,0,1)" % (f(rs), f(rs))
        if i == 0 and P["lean"]:
            trunk += "&(%s)" % f(P["lean"])
            path, rs = up + "&(%s)" % f(P["lean"]), 0.0
            up = path
        for k, (after, az, pitch, v, g) in enumerate(P["limbs"]):
            if after == i:
                om += "[%s/(%s)&(%s)%sA(%s,%d,%d)]" % (
                    up, f(az), f(pitch), start("bw*%s" % f(v)), f(v), 1 if k % 2 == 0 else -1, int(g))
    om += trunk + "&(%s)A(%s,1,%d)" % (f(P["leader"][0]), f(P["leader"][1]), int(P["leader"][2]))
    lines.append("omega: " + om)

    def internode(split, ghost):
        if ghost:
            return "f(l0*v,1,0,v)$"
        if split:
            return ("!(bw*v,1)F(l0*v*0.5,0.5,0,v)$/(-s*70)&(%s)"
                    "!(bw*v*0.93,1)F(l0*v*0.5,0.5,0,v)$" % f(P["crook"]))
        return "!(bw*v,1)F(l0*v,1,0,v)$"
    # no bracket at an elbow, sprays on forks, a collar on thick forks, the
    # first shoot of a fork started at its own width: see oak.py (a tube
    # restarted after `]` right before a bend leaves a wedge-shaped hole on
    # the bend's outside)
    spray = "[f(-l0*v*0.45,-0.45,0,v)/(s*%s)&(%s)K(v)]" % (f(P["spray_roll"]), f(P["spray_pitch"]))
    k1, k2 = P["kf"]
    b1, b2 = P["fork_b"]
    r1, r2 = P["fork_roll"]
    ds, dp = int(P["dsplit"]), int(P["dspray"])
    gh = G - int(P["ghost"])
    cuts = sorted(set(c for c in (0, ds, dp, gh, G) if 0 <= c <= G))
    for n, (lo, hi) in enumerate(zip(cuts[:-1], cuts[1:])):
        guard = ("d >= %d & d < %d" % (lo, hi)) if lo > 0 else "d < %d" % hi
        ghost = lo >= gh
        I = internode(lo < ds, ghost)
        sp = spray if (lo >= dp and not ghost) else ""
        collar = "F(0.01)" if (lo < int(P["collar_d"]) and not ghost) else ""
        st = "" if ghost else start("bw*v*%s" % f(k1 * P["shoot_start"] * (0.93 if lo < ds else 1.0)))
        lines += [
            "f%d: %s : A(v,s,d) : %s -> %s%s[/(s*%s)&(%s)%sA(v*%s,s,d+1)]%s/(-s*%s)&(%s)A(v*%s,-s,d+1)" % (
                n + 1, f(P["p_fork"]), guard, I, sp, f(r1), f(b1), st, f(k1), collar, f(r2), f(b2), f(k2)),
            "e%d: %s : A(v,s,d) : %s -> %s/(s*%s)&(%s)A(v*%s,-s,d+0.5)" % (
                n + 1, f(P["p_elb"]), guard, I, f(P["elb_roll"]), f(P["elb"]), f(P["k_elb"])),
        ]
    lines += [
        "st: %s : A(v,s,d) : d < G -> A(v,s,d)" % f(P["p_stall"]),
        "g1: F(x,r,g,m) -> F(r*m*min(lm,l0+dl*(g+1)),r,g+1,m)",
        "g2: f(x,r,g,m) -> f(r*m*min(lm,l0+dl*(g+1)),r,g+1,m)",
        "g3: !(w,t) : t > 0 -> !(w*vb,t)",
        "g4: !(w,t) : t < 0 -> !(w*vt,t)",
    ]
    cs = P["cs"]
    if P["wood_only"]:
        fin = ["A(v,s,d) : * -> ", "K(v) : * -> "]
    else:
        tip = "A(v,s,d) : * -> "
        m = mass(cs, P["back"], int(P["clump_n"]))
        ks = mass(cs * P["spray"], P["back"] * 0.6, int(P["spray_n"]), P["spray_fwd"])
        w0, wr, wg, w2 = P["fruit"]
        kf = P["spray_fruit"]
        if P["fruit_kind"] == "card":
            # apples at the mass's outer edge: ahead of the tip, then pitched
            # down past it (after `$`, ^ turns down), crossed so they read
            # from every side
            a1 = apple(P, "$f(%s)^(100)" % f(cs * P["fruit_out"]), 20)
            a2 = apple(P, "$/(140)&(70)f(%s)$^(100)" % f(cs * P["fruit_out"]), 70)
            ag, ak = a1, apple(P, "$f(%s)^(100)" % f(P["spray_fwd"] + cs * 0.2), 40)
        else:
            # apples hang from the tip out through the lower side of its
            # mass: pitched down from the tip's heading (after `$`, ^ turns
            # down) and drawn on along that line, so each hangs among the
            # outer leaves on a line back to its twig, never above the mass
            (ro1, pi1, d1), (ro2, pi2, d2) = P["fruit_at"]
            p1 = "$/(%s)^(%s)f(%s)" % (f(ro1), f(pi1), f(d1))
            p2 = "$/(%s)^(%s)f(%s)" % (f(ro2), f(pi2), f(d2))
            a1, ag = tube_apple(P, p1, "red"), tube_apple(P, p1, "green")
            a2 = tube_apple(P, p2, "red" if wr >= wg else "green")
            ak = tube_apple(P, "$f(%s)^(100)f(0.3)" % f(P["spray_fwd"]), "red")
        fin = ["%s : %s%s" % (f(w0), tip, m)]
        for wt_, extra in ((wr, a1), (wg, ag), (w2, a1 + a2)):
            if wt_ > 0:
                fin.append("%s : %s%s%s" % (f(wt_), tip, m, extra))
        if kf > 0:
            fin.append("%s : K(v) : * -> %s" % (f(1 - kf), ks))
            fin.append("%s : K(v) : * -> %s%s" % (f(kf), ks, ak))
        else:
            fin.append("K(v) : * -> " + ks)
    return generator(P, "\n".join(lines), "\n".join(fin))


def parse(P, kv):
    k, v = kv.split("=", 1)
    old = P.get(k)
    if isinstance(old, list) and old and isinstance(old[0], tuple):
        P[k] = [tuple(float(x) for x in t.split(",")) for t in v.split(";")] if v else []
    elif isinstance(old, list):
        P[k] = [float(x) for x in v.split(",")] if v else []
    elif isinstance(old, tuple):
        P[k] = tuple(float(x) for x in v.split(","))
    elif isinstance(old, int) and not isinstance(old, bool):
        P[k] = int(v)
    else:
        try:
            P[k] = float(v)
        except ValueError:
            P[k] = v


def main():
    args = sys.argv[1:]
    variant = args[0] if args else "a"
    out = args[1] if len(args) > 1 else "apple.json"
    P = dict(BASE)
    P.update(VARIANTS[variant])
    for kv in args[2:]:
        parse(P, kv)
    g = variant_a(P)
    with open(out, "w") as fh:
        json.dump(g, fh, indent=1)
    print("wrote", out, "src", len(g["source_code"]), "fin", len(g["finalization_code"]))


if __name__ == "__main__":
    main()
