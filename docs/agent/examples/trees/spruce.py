#!/usr/bin/env python3
"""Builder for the Understory's dark_conifer (a dark spruce/fir L-system).

usage: build.py VARIANT OUT.json [key=value ...]

Variants: a (bottlebrush branches expanded whole in finalization), b
(prospective whorls, node markers), c (developmental: iterations = age)
and d (c after the critique: no level cards, level oldest branches, trunk
fill at every whorl, wider branch variety). The shipped tree is
`build.py d dark_conifer.json` with the defaults below; cardsim.py (in r2/)
checks every card template's tilt off level.

Writes the generator in wire form (every decimal a whole number of
ten-thousandths). Units in the grammar are metres BEFORE the root
transform scale (P["scale"]).
"""
import json
import math
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
    scale=1.5,
    iterations=12,       # = whorl count
    hb=0.55,             # bare trunk under the first whorl
    ih=0.44,             # whorl spacing (internode)
    ih_decay=0.012,      # internodes shorten toward the top (fraction per node)
    w0=0.20,             # trunk width at the base
    lb=1.55,             # lowest branch length
    lmin=0.28,           # top branch length floor
    cone_pow=1.0,        # 1 = straight cone flank, >1 concave, <1 convex
    p_bot=92.0,          # branch angle from vertical at the bottom whorl
    p_top=48.0,          # ... at the top whorl
    k=5,                 # branches per whorl (4..5 by stochastic rule)
    elasticity=0.07,
    card=0.62,           # card scale on the longest branch (Twig card is 0.7 x 1.0)
    card_min=0.30,
    splay=50.0,          # card splay from the branch axis (bottlebrush)
    mesh_resolution=5,
    # needle texture
    pair_count=24,
    needle_angle=48.0,
    needle_length=0.34,
    needle_width=0.022,
    length_taper=0.45,
    shoot_length=0.80,
    shoot_width=0.012,
    variant_cols=1,
    variant_rows=1,
    # colours (sRGB, 0..1); texture colours are converted to linear
    n_base=(0.06, 0.17, 0.15),
    n_tip=(0.13, 0.27, 0.25),
    fourth_from=0.5,     # nodes whose card scale reaches this carry a 4th shoot
    n_shoot=(0.20, 0.16, 0.10),
    tint=(1.0, 1.0, 1.0),
    bark=(0.30, 0.22, 0.16),
    bark_dark=(0.12, 0.08, 0.06),
    bark_light=(0.36, 0.27, 0.20),
)


def needle_material(P):
    m = {
        "base_color": [w(c) for c in P["tint"]],
        "roughness": 10000,
        "texture": {
            "$type": "Needle",
            "color_base": wl(P["n_base"]),
            "color_shoot": wl(P["n_shoot"]),
            "color_tip": wl(P["n_tip"]),
            "length_taper": w(P["length_taper"]),
            "needle_angle": w(P["needle_angle"]),
            "needle_length": w(P["needle_length"]),
            "needle_width": w(P["needle_width"]),
            "pair_count": int(P["pair_count"]),
            "shoot_length": w(P["shoot_length"]),
            "shoot_width": w(P["shoot_width"]),
            "variant_cols": int(P["variant_cols"]),
            "variant_rows": int(P["variant_rows"]),
        },
    }
    if P.get("normal_strength") is not None:
        m["texture"]["normal_strength"] = w(P["normal_strength"])
    return m


def bark_material(P):
    return {
        "base_color": [w(c) for c in P["bark"]],
        "roughness": 9500,
        "texture": {
            "$type": "Bark",
            "color_dark": wl(P["bark_dark"]),
            "color_light": wl(P["bark_light"]),
        },
        "uv_scale": 15000,
    }


def generator(P, src, fin, tropism=True):
    return {
        "$type": "network.symbios.gen.lsystem",
        "angle": 450000,
        "elasticity": w(P["elasticity"]),
        "finalization_code": fin,
        "iterations": int(P["iterations"]),
        "materials": {"0": bark_material(P), "1": needle_material(P)},
        "mesh_resolution": int(P["mesh_resolution"]),
        "prop_mappings": {"0": "Twig"},
        "prop_scale": 10000,
        "seed": str(P.get("seed", 1)),
        "source_code": src,
        "step": 10000,
        "transform": {"scale": [w(P["scale"])] * 3},
        "tropism": [0, -10000, 0] if tropism else None,
        "width": 1000,
    }


# ------------------------------------------------ variant A: bottlebrush whorls
def variant_a(P):
    """Prospective whorls; each branch expanded in finalization as a
    bottlebrush: 3-4 drawn segments, 3 cards per node splayed round the axis."""
    N = int(P["iterations"])
    lines = [
        "#define N %d" % N,
        "#define ih %s" % f(P["ih"]),
        "#define idk %s" % f(P["ih_decay"]),
        "#define w0 %s" % f(P["w0"]),
        "#define lb %s" % f(P["lb"]),
        "#define lm %s" % f(P["lmin"]),
        "#define pb %s" % f(P["p_bot"]),
        "#define pt %s" % f(P["p_top"]),
        "omega: !(w0)F(%s)/(23)A(0)" % f(P["hb"]),
    ]
    # u = i/(N-1): 0 at the bottom whorl, 1 at the top
    L = "(lm+(lb-lm)*(1-i/(N-1)))"
    pitch = "(pb-(pb-pt)*i/(N-1))"
    wid = "(w0*(1-0.85*i/N))"
    bw = "(w0*(1-0.85*i/N)*0.42)"
    inter = "!(%s)F(ih*(1-idk*i))" % wid

    def br(dl, dp):
        return "[&(%s+%s)B(%s*%s,%s)]" % (pitch, f(dp), L, f(dl), bw)

    # three whorl patterns for irregularity (topology: 5, 4, 5 with a gap)
    lines.append("a1: 0.45 : A(i) -> %s%s/(71)%s/(73)%s/(70)%s/(74)%s/(%s)A(i+1)" % (
        inter, br(1.0, 0), br(0.94, 4), br(1.04, -3), br(0.97, 2), br(1.01, -2), "137.5"))
    lines.append("a2: 0.35 : A(i) -> %s%s/(88)%s/(92)%s/(89)%s/(%s)A(i+1)" % (
        inter, br(1.02, 1), br(0.96, -3), br(1.0, 3), br(0.93, 0), "137.5"))
    lines.append("a3: 0.20 : A(i) -> %s%s/(70)%s/(144)%s/(72)%s/(%s)A(i+1)" % (
        inter, br(0.98, -2), br(1.05, 2), br(0.9, 4), br(1.0, 0), "137.5"))
    src = "\n".join(lines)

    sp = P["splay"]

    def cs(scale_expr):
        return scale_expr

    card = "(min(%s,%s+%s*l))" % (f(P["card"]), f(P["card_min"]), f((P["card"] - P["card_min"]) / max(P["lb"] - P["lmin"], 0.01)))

    def node(phi, s_mul, spl):
        out = ""
        for d in (0, 120, 240):
            out += "[,(1)/(%s)&(%s)~(0,%s*%s)]" % (f(phi + d), f(spl), card, f(s_mul))
        return out

    # long branch: 4 segments, 4 nodes + tip
    long_b = ("B(l,w) : l > 0.9 -> $!(w)F(l*0.3)" + node(10, 0.9, sp) +
              "!(w*0.75)F(l*0.27)" + node(147, 1.0, sp) +
              "!(w*0.5)F(l*0.23)" + node(285, 1.0, sp) +
              "!(w*0.3)F(l*0.2)" + node(62, 0.9, sp - 8) +
              "[,(1)~(0,%s*0.8)]" % card)
    mid_b = ("B(l,w) : l > 0.45 & l <= 0.9 -> $!(w)F(l*0.4)" + node(10, 0.95, sp) +
             "!(w*0.6)F(l*0.35)" + node(147, 1.0, sp) +
             "!(w*0.35)F(l*0.25)" + node(285, 0.9, sp - 10) +
             "[,(1)~(0,%s*0.8)]" % card)
    short_b = ("B(l,w) : l <= 0.45 -> $!(w)F(l*0.55)" + node(10, 1.0, sp - 5) +
               "!(w*0.5)F(l*0.45)" + node(150, 0.9, sp - 15) +
               "[,(1)~(0,%s*0.8)]" % card)
    leader = ("A(i) : * -> !(0.035)F(0.22)" +
              "[,(1)/(0)&(28)~(0,0.36)][,(1)/(95)&(30)~(0,0.36)][,(1)/(185)&(27)~(0,0.36)][,(1)/(275)&(31)~(0,0.36)]"
              "!(0.02)F(0.22)" +
              "[,(1)/(45)&(20)~(0,0.3)][,(1)/(165)&(18)~(0,0.3)][,(1)/(285)&(22)~(0,0.3)]"
              "[,(1)~(0,0.34)][,(1)/(90)~(0,0.34)]")
    fin = "\n".join([long_b, mid_b, short_b, leader])
    return generator(P, src, fin)


# ------------------------------------- variant B: spray branches, few tubes
def whorl_rules(P, L, pitch, bw, inter, k_opts=None):
    """Three stochastic whorl patterns: 5, 4 and 4-with-a-gap branches."""
    def br(dl, dp):
        return "[&(%s+%s)B(%s*%s,%s)]" % (pitch, f(dp), L, f(dl), bw)
    return [
        "a1: 0.45 : A(i) -> %s%s/(71)%s/(73)%s/(70)%s/(74)%s/(137.5)A(i+1)" % (
            inter, br(1.0, 0), br(0.94, 4), br(1.04, -3), br(0.97, 2), br(1.01, -2)),
        "a2: 0.35 : A(i) -> %s%s/(88)%s/(92)%s/(89)%s/(137.5)A(i+1)" % (
            inter, br(1.02, 1), br(0.96, -3), br(1.0, 3), br(0.93, 0)),
        "a3: 0.20 : A(i) -> %s%s/(70)%s/(144)%s/(72)%s/(137.5)A(i+1)" % (
            inter, br(0.98, -2), br(1.05, 2), br(0.9, 4), br(1.0, 0)),
    ]


def card(roll, splay, s, yaw=0.0):
    out = "[,(1)"
    if isinstance(roll, str):
        out += "/(%s)" % roll
    elif roll:
        out += "/(%s)" % f(roll)
    if yaw:
        out += "+(%s)" % f(yaw)
    if splay:
        out += ("&(%s)" if splay > 0 else "^(%s)") % f(abs(splay))
    return out + "~(0,%s)]" % s


def spray_node_rules(P, sp):
    """K(k,c,j): a node of shoots on a branch; c = card scale, j = roll
    jitter. k 0: two side-up shoots and one up; k 1: two side-down shoots
    and one short hanging shoot. Rolls are about the branch axis after `$`
    (roll 0 = up, 90 = side, 180 = down)."""
    su, sd = P["splay"], P.get("splay_down", P["splay"])
    up = P.get("up_roll", 70.0)
    dn = P.get("down_roll", 115.0)
    k0 = (card("j+%s" % f(up), su, "c") + card("j-%s" % f(up), su, "c") +
          card("j", su * 0.7, "c*0.8"))
    k1 = (card("j+%s" % f(dn), sd, "c") + card("j-%s" % f(dn), sd, "c") +
          card("j+180", sd * P.get("hang_splay", 0.45), "c*%s" % f(P.get("hang_scale", 0.65))))
    rules = []
    thr = P.get("fourth_from", 0.0)
    if P.get("fourth", 1):
        k0b = k0 + card("j+35", su * 0.5, "c*0.9")
        k1b = k1 + card("j-35", su * 0.5, "c*0.9")
        if thr > 0:
            # young (small-card) nodes carry three shoots, vigorous ones four
            t = f(thr)
            rules += ["K(k,c,j) : k < 0.5 & c < %s -> " % t + k0,
                      "K(k,c,j) : k >= 0.5 & c < %s -> " % t + k1,
                      "K(k,c,j) : k < 0.5 & c >= %s -> " % t + k0b,
                      "K(k,c,j) : k >= 0.5 & c >= %s -> " % t + k1b]
        else:
            k0, k1 = k0b, k1b
    if not rules:
        rules = ["K(k,c,j) : k < 0.5 -> " + k0, "K(k,c,j) : k >= 0.5 -> " + k1]
    return rules + ["T(c) : * -> [,(1)~(0,c*0.9)]" + card(0, 0, "c*0.75", 28) + card(0, 0, "c*0.75", -28)]


def branch_body(nodes, split, cs):
    """A branch as two tube segments (split = fraction in the first), with
    node markers at fractions `nodes` placed by moves - no extra tubes."""
    body = "$!(w)"
    seg2 = "!(w*0.45)"
    kind = 0
    jits = [0, 23, -17, 31, -9, 14, -26, 8, 19]
    for n, t in enumerate(nodes):
        node = "K(%d,%s,%d)" % (kind, cs, jits[n % len(jits)])
        if t <= split:
            body += "[f(l*%s)%s]" % (f(t), node)
        else:
            seg2 += "[f(l*%s)%s]" % (f(t - split), node)
        kind = 1 - kind
    return "%sF(l*%s)%sF(l*%s)T(%s)" % (body, f(split), seg2, f(1 - split), cs)


def variant_b(P):
    N = int(P["iterations"])
    lines = [
        "#define N %d" % N,
        "#define ih %s" % f(P["ih"]),
        "#define idk %s" % f(P["ih_decay"]),
        "#define w0 %s" % f(P["w0"]),
        "#define lb %s" % f(P["lb"]),
        "#define lm %s" % f(P["lmin"]),
        "#define pb %s" % f(P["p_bot"]),
        "#define pt %s" % f(P["p_top"]),
        "omega: !(w0)F(%s)/(23)A(0)" % f(P["hb"]),
    ]
    cp = P["cone_pow"]
    u = "(1-i/(N-1))"
    if abs(cp - 1.0) < 1e-6:
        L = "(lm+(lb-lm)*%s)" % u
    elif abs(cp - 2.0) < 1e-6:
        L = "(lm+(lb-lm)*%s*%s)" % (u, u)
    else:
        L = "(lm+(lb-lm)*sqrt(%s))" % u
    pitch = "(pb-(pb-pt)*i/(N-1))"
    wid = "(w0*(1-0.85*i/N))"
    bw = "(w0*(1-0.85*i/N)*0.42)"
    inter = "!(%s)[f(ih*0.5*(1-idk*i))J(i)]F(ih*(1-idk*i))" % wid
    lines += whorl_rules(P, L, pitch, bw, inter)
    k = (P["card"] - P["card_min"]) / max(P["lb"] - P["lmin"], 0.01)
    cs = "min(%s,%s+%s*l)" % (f(P["card"]), f(P["card_min"]), f(k))
    long_nodes = P.get("long_nodes", [0.16, 0.29, 0.42, 0.55, 0.67, 0.78, 0.88])
    mid_nodes = [0.2, 0.4, 0.6, 0.78]
    short_nodes = [0.3, 0.62]
    lines += [
        "b1: B(l,w) : l > 0.9 -> " + branch_body(long_nodes, 0.55, cs),
        "b2: B(l,w) : l > 0.45 & l <= 0.9 -> " + branch_body(mid_nodes, 0.55, cs),
        "b3: B(l,w) : l <= 0.45 -> " + branch_body(short_nodes, 0.6, cs),
    ]
    src = "\n".join(lines)
    fin = spray_node_rules(P, P["splay"]) + [
        # the top whorl is born on the last step: its buds get a short form
        "B(l,w) : * -> $!(w)[f(l*0.4)K(0,%s,0)]F(l)T(%s)" % (cs, cs),
        ("A(i) : * -> !(0.04)[f(0.06)" +
         card(0, 40, "0.42") + card(90, 42, "0.42") + card(180, 38, "0.42") + card(270, 41, "0.42") +
         "][f(0.26)" + card(45, 30, "0.38") + card(165, 28, "0.38") + card(285, 32, "0.38") +
         "]F(0.5)" + card(0, 0, "0.4") + card(90, 0, "0.4") + card(0, 14, "0.3") + card(180, 14, "0.3")),
        "J(i) : i < %s -> " % f(P.get("fill_from", 5)),
        ("J(i) : i >= %s -> " % f(P.get("fill_from", 5)) +
         card(0, 55, "0.42") + card(120, 52, "0.42") + card(240, 57, "0.42")),
    ]
    return generator(P, src, "\n".join(fin))


# ------------------------------ variant C: developmental (iterations = age)
def variant_c(P):
    """Retrospective growth: every organ is born small and keeps growing.
    A branch's segments and node moves carry (fraction, age, vigour) and are
    re-lengthened each step (linear: l0 + d*age); its pitch sinks with age;
    widths thicken (pipe-ish); card scale grows. The cone, the up-angled
    young top and the level old bottom all fall out of age alone."""
    N = int(P["iterations"])
    l0, d = P.get("l0", 0.28), P.get("dl", 0.125)
    a0, da, amax = P.get("a0", 46.0), P.get("da", 4.6), P.get("amax", 96.0)
    c0, dc, cmax = P["card_min"], P.get("dc", 0.034), P["card"]
    bw0, vb = P.get("bw0", 0.018), P.get("vb", 1.15)
    wt0, vt = P.get("wt0", 0.034), P.get("vt", 1.155)
    split = 0.55
    lines = [
        "#define l0 %s" % f(l0), "#define dl %s" % f(d),
        "#define a0 %s" % f(a0), "#define da %s" % f(da), "#define am %s" % f(amax),
        "#define c0 %s" % f(c0), "#define dc %s" % f(dc), "#define cm %s" % f(cmax),
        "#define vb %s" % f(vb), "#define vt %s" % f(vt),
        "#define ih %s" % f(P["ih"]), "#define wt %s" % f(wt0), "#define bw %s" % f(bw0),
        "omega: !(wt,-1)F(%s)/(23)A" % f(P["hb"]),
    ]

    def br(m, j):
        a = "a0" if j == 0 else ("a0+%s" % f(j) if j > 0 else "a0-%s" % f(-j))
        return "[&(%s,0,%s)B(%s)]" % (a, f(j), f(m))
    inter = "!(wt,-1)[f(ih*0.5)J(0)]F(ih)"
    lines += [
        "a1: 0.45 : A -> %s%s/(71)%s/(73)%s/(70)%s/(74)%s/(137.5)A" % (
            inter, br(1.0, 0), br(0.94, 4), br(1.04, -3), br(0.97, 2), br(1.01, -2)),
        "a2: 0.35 : A -> %s%s/(88)%s/(92)%s/(89)%s/(137.5)A" % (
            inter, br(1.02, 1), br(0.96, -3), br(1.0, 3), br(0.93, 0)),
        "a3: 0.20 : A -> %s%s/(70)%s/(144)%s/(72)%s/(137.5)A" % (
            inter, br(0.98, -2), br(1.05, 2), br(0.9, 4), br(1.0, 0)),
    ]
    # a newborn branch: two tube segments, node moves, card markers
    nodes = P.get("c_nodes", [0.16, 0.3, 0.44, 0.57, 0.69, 0.8, 0.9])
    jits = [0, 23, -17, 31, -9, 14, -26, 8, 19]
    body, seg2, kind = "$!(bw,1)", "!(bw*0.45,1)", 0
    for n, t in enumerate(nodes):
        if t <= split:
            body += "[f(l0*m*%s,%s,0,m)K(%d,c0,%d)]" % (f(t), f(t), kind, jits[n])
        else:
            seg2 += "[f(l0*m*%s,%s,0,m)K(%d,c0,%d)]" % (f(t - split), f(t - split), kind, jits[n])
        kind = 1 - kind
    lines.append("b1: B(m) -> %sF(l0*m*%s,%s,0,m)%sF(l0*m*%s,%s,0,m)T(c0)" % (
        body, f(split), f(split), seg2, f(1 - split), f(1 - split)))
    lines += [
        "g1: F(x,r,g,m) -> F(r*m*(l0+dl*(g+1)),r,g+1,m)",
        "g2: f(x,r,g,m) -> f(r*m*(l0+dl*(g+1)),r,g+1,m)",
        "g3: &(a,g,j) -> &(min(am,a0+j+da*(g+1)),g+1,j)",
        "g4: !(w,t) : t > 0 -> !(w*vb,t)",
        "g5: !(w,t) : t < 0 -> !(w*vt,t)",
        "g6: K(k,c,j) -> K(k,min(cm,c+dc),j)",
        "g7: T(c) -> T(min(cm,c+dc))",
        "g8: J(n) -> J(n+1)",
    ]
    src = "\n".join(lines)
    fin = spray_node_rules(P, P["splay"]) + [
        # the whorl born on the last step never grew: a bud-sized form
        "B(m) : * -> " + card(0, 10, "0.3") + card(0, 40, "0.24", 0),
        ("A : * -> !(wt)[f(0.05)" +
         card(0, 42, "0.4") + card(90, 44, "0.4") + card(180, 40, "0.4") + card(270, 43, "0.4") +
         "][f(0.19)" + card(45, 34, "0.36") + card(165, 32, "0.36") + card(285, 35, "0.36") +
         "][f(0.33)" + card(100, 24, "0.32") + card(220, 22, "0.32") + card(340, 25, "0.32") +
         "]F(0.44)" + card(0, 0, "0.3") + card(90, 0, "0.3") + card(0, 12, "0.24") + card(180, 12, "0.24")),
        "J(n) : n > %s -> " % f(P.get("fill_age", 6)),
        ("J(n) : n <= %s -> " % f(P.get("fill_age", 6)) +
         card(0, 55, "0.42") + card(120, 52, "0.42") + card(240, 57, "0.42")),
    ]
    return generator(P, src, "\n".join(fin))


# ------------------ variant D: C after the critique (no level cards, fuller)
# Overrides of BASE for variant d. Every card carries a roll so none lies
# level (a level card mirrors the low sun straight into the game camera);
# the oldest branches stop at `am` near horizontal and their underside
# sprays splay less; trunk-fill cards sit at every whorl; wider per-branch
# length factors and a sparse three-branch whorl break up the repetition.
D_DEFAULTS = dict(
    dl=0.118,                # branch growth per step (m); sets the base width
    fourth_from=0.46,        # nodes whose card scale reaches this carry a 4th card
    amax=87.0,
    c_nodes=[0.05, 0.19, 0.33, 0.47, 0.61, 0.75, 0.88],
    n_shoot=None,            # None = the needle base colour (hides the midrib)
    needle_angle=56.0,
    length_taper=0.25,
    normal_strength=0.5,
    rt=50.0,                 # roll on the tip cards
    rj=36.0,                 # roll on trunk-fill cards
    jp=48.0,                 # trunk-fill pitch off the trunk
    js=0.44,                 # trunk-fill card scale
    under_roll=100.0,
    under_pitch=30.0,
    hang_pitch=12.0,
    whorls="std",
)

WHORLS = {
    # (weight, [(length factor, pitch jitter, roll to the next branch)...])
    "std": [
        (0.40, [(1.0, 0, 71), (0.82, 4, 73), (1.12, -3, 70), (0.9, 2, 74), (1.05, -2, None)]),
        (0.30, [(1.1, 1, 88), (0.78, -3, 92), (1.0, 3, 89), (0.92, 0, None)]),
        (0.18, [(0.85, -2, 70), (1.15, 2, 144), (0.76, 4, 72), (1.02, 0, None)]),
        (0.12, [(1.08, 2, 95), (0.8, -3, 150), (1.12, 0, None)]),
    ],
}


def variant_d(P):
    N = int(P["iterations"])
    l0, d = P.get("l0", 0.28), P.get("dl", 0.125)
    a0, da, amax = P.get("a0", 46.0), P.get("da", 4.6), P["amax"]
    c0, dc, cmax = P["card_min"], P.get("dc", 0.034), P["card"]
    bw0, vb = P.get("bw0", 0.018), P.get("vb", 1.15)
    wt0, vt = P.get("wt0", 0.034), P.get("vt", 1.155)
    split = 0.55
    lines = [
        "#define l0 %s" % f(l0), "#define dl %s" % f(d),
        "#define a0 %s" % f(a0), "#define da %s" % f(da), "#define am %s" % f(amax),
        "#define c0 %s" % f(c0), "#define dc %s" % f(dc), "#define cm %s" % f(cmax),
        "#define vb %s" % f(vb), "#define vt %s" % f(vt),
        "#define ih %s" % f(P["ih"]), "#define wt %s" % f(wt0), "#define bw %s" % f(bw0),
        "omega: !(wt,-1)F(%s)/(23)A" % f(P["hb"]),
    ]

    def br(m, j):
        a = "a0" if j == 0 else ("a0+%s" % f(j) if j > 0 else "a0-%s" % f(-j))
        return "[&(%s,0,%s)B(%s)]" % (a, f(j), f(m))
    inter = "!(wt,-1)[f(ih*0.5)J(0)]F(ih)"
    for n, (wgt, bs) in enumerate(WHORLS[P["whorls"]]):
        body = ""
        for m, j, roll in bs:
            body += br(m, j) + ("/(%s)" % f(roll) if roll is not None else "")
        lines.append("a%d: %s : A -> %s%s/(137.5)A" % (n + 1, f(wgt), inter, body))
    nodes = P["c_nodes"]
    jits = [0, 23, -17, 31, -9, 14, -26, 8, 19]
    body, seg2, kind = "$!(bw,1)", "!(bw*0.45,1)", 0
    for n, t in enumerate(nodes):
        if t <= split:
            body += "[f(l0*m*%s,%s,0,m)K(%d,c0,%d)]" % (f(t), f(t), kind, jits[n])
        else:
            seg2 += "[f(l0*m*%s,%s,0,m)K(%d,c0,%d)]" % (f(t - split), f(t - split), kind, jits[n])
        kind = 1 - kind
    lines.append("b1: B(m) -> %sF(l0*m*%s,%s,0,m)%sF(l0*m*%s,%s,0,m)T(c0)" % (
        body, f(split), f(split), seg2, f(1 - split), f(1 - split)))
    lines += [
        "g1: F(x,r,g,m) -> F(r*m*(l0+dl*(g+1)),r,g+1,m)",
        "g2: f(x,r,g,m) -> f(r*m*(l0+dl*(g+1)),r,g+1,m)",
        "g3: &(a,g,j) -> &(min(am,a0+j+da*(g+1)),g+1,j)",
        "g4: !(w,t) : t > 0 -> !(w*vb,t)",
        "g5: !(w,t) : t < 0 -> !(w*vt,t)",
        "g6: K(k,c,j) -> K(k,min(cm,c+dc),j)",
        "g7: T(c) -> T(min(cm,c+dc))",
        "g8: J(n) -> J(n+1)",
    ]
    src = "\n".join(lines)

    rt, rj = P["rt"], P["rj"]

    def cd(pre, s, roll=0.0):
        """One card: `pre` turtle turns, then a roll about the card's own
        heading so its face is never level, then the prop."""
        r = ""
        if roll:
            r = "/(%s)" % f(roll)
        return "[,(1)%s%s~(0,%s)]" % (pre, r, s)
    # Rolls chosen with cardsim.py so every card's face stays at least ~45
    # degrees off level over branch pitches 46-95 and node jitters -26..31;
    # a post-pitch roll must turn the same way as the pre-pitch one or the
    # two cancel and the card comes back level.
    su = P["splay"]
    ur, up_ = P["under_roll"], P["under_pitch"]
    uc = P.get("under_counter", 15.0)
    k0 = (cd("/(j+70)&(%s)" % f(su), "c") + cd("/(j-70)&(%s)" % f(su), "c") +
          cd("/(j*0.4)&(%s)" % f(P.get("up_pitch", 30.0)), "c*0.8", P.get("up_roll", 52.0)))
    k1 = (cd("/(j+%s)&(%s)" % (f(ur), f(up_)), "c", -uc) +
          cd("/(j-%s)&(%s)" % (f(ur), f(up_)), "c", uc) +
          cd("/(j*0.4+180)&(%s)" % f(P["hang_pitch"]), "c*0.65", P.get("hang_roll", 55.0)))
    k0b = k0 + cd("/(j+35)&(%s)" % f(su * 0.5), "c*0.9", P.get("fourth_roll", 45.0))
    k1b = k1 + cd("/(j-35)&(%s)" % f(su * 0.5), "c*0.9", -P.get("fourth_roll", 45.0))
    t = f(P.get("fourth_from", 0.5))
    fin = [
        "K(k,c,j) : k < 0.5 & c < %s -> " % t + k0,
        "K(k,c,j) : k >= 0.5 & c < %s -> " % t + k1,
        "K(k,c,j) : k < 0.5 & c >= %s -> " % t + k0b,
        "K(k,c,j) : k >= 0.5 & c >= %s -> " % t + k1b,
        "T(c) : * -> " + cd("", "c*0.9", rt) + cd("+(28)", "c*0.75", rt + 2) + cd("+(-28)", "c*0.75", -rt - 2),
        # the whorl born on the last step never grew: a bud-sized form
        "B(m) : * -> " + cd("&(10)", "0.3", 45) + cd("^(10)", "0.24", -30),
        ("A : * -> !(wt)[f(0.05)" +
         card(0, 42, "0.4") + card(90, 44, "0.4") + card(180, 40, "0.4") + card(270, 43, "0.4") +
         "][f(0.19)" + card(45, 34, "0.36") + card(165, 32, "0.36") + card(285, 35, "0.36") +
         "][f(0.33)" + card(100, 24, "0.32") + card(220, 22, "0.32") + card(340, 25, "0.32") +
         "]F(0.44)" + card(0, 0, "0.3") + card(90, 0, "0.3") + card(0, 12, "0.24") + card(180, 12, "0.24")),
        ("J(n) : * -> " + cd("&(%s)" % f(P["jp"]), f(P["js"]), rj) +
         cd("/(120)&(%s)" % f(P["jp"] - 3), f(P["js"]), -rj) +
         cd("/(240)&(%s)" % f(P["jp"] + 2), f(P["js"]), rj)),
    ]
    if P.get("n_shoot") is None:
        P["n_shoot"] = P["n_base"]
    return generator(P, src, "\n".join(fin))


VARIANTS = {"a": variant_a, "b": variant_b, "c": variant_c, "d": variant_d}


def main():
    variant = sys.argv[1]
    out = sys.argv[2]
    P = dict(BASE)
    if variant == "d":
        P.update(D_DEFAULTS)
    for kv in sys.argv[3:]:
        k, v = kv.split("=", 1)
        old = P.get(k)
        if k == "n_shoot" or isinstance(old, tuple):
            P[k] = tuple(float(x) for x in v.split(","))
        elif isinstance(old, int) and not isinstance(old, bool):
            P[k] = int(v)
        else:
            try:
                P[k] = float(v)
            except ValueError:
                P[k] = v
    g = VARIANTS[variant](P)
    with open(out, "w") as fh:
        json.dump(g, fh, indent=1)
    print("wrote", out, "src", len(g["source_code"]), "fin", len(g["finalization_code"]))


if __name__ == "__main__":
    main()
