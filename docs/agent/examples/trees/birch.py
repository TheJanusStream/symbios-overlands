#!/usr/bin/env python3
"""Builder for the Understory's pale_birch (a silver birch, Betula pendula).

usage: build.py [VARIANT OUT.json [key=value ...]]
       build.py            -> the shipped tree: variant a, BASE, pale_birch.json

Writes one network.symbios.gen.lsystem generator in wire form (every
decimal a whole number of ten-thousandths). Grammar units are metres
BEFORE the root transform scale (P["scale"], 1.0 here).

Variants (all tried against turntables, 15 m / 50 m play views and the
world; see the notes at each):
  a  SHIPPED. Developmental spiral: two trunk nodes a step, limbs that
     lengthen, sink and droop with age, hanging Twig-card strands.
  b  REJECTED. Prospective (explicit state) with weeping whips drawn as
     thin tube segments bent by a strong tropism: lovely arching limbs,
     but 5,672 triangles and a crown that reads as bare winter wood at
     50 m - tubes buy far less leaf than cards per triangle.
  a + sympodial=0 leaf_scale=0.25 (parameter set "C") REJECTED: opposite
     leaves on the Twig card hold at distance but hang like fern fronds
     or ash leaves, not birch twigs.
Other rejected settings: Bark/Marble textures for the trunk (grey mottle
or salt-and-pepper, never birch); 5-segment limbs and 2-segment
branchlets (over budget).

Second round, after an independent critique (r3/ holds the test renders):
  - Smaller leaves REJECTED again: 12 leaves at 0.24-0.25 of the card
    (the same leaf area as 8 at 0.30), 10 at 0.27-0.28 and 14 at 0.24
    all left crowns at about 55 m in the world views (views.py tiles) as
    bare white skeletons - the mipmaps average the smaller leaves' alpha
    below the mask. 8 at 0.30 holds. What made the old strand read as a
    pinnate leaf (ash, wisteria) is fixed instead: a slight sympodial
    zigzag in the stem (stem_curve 0.008), leaves angled down the hanging
    twig (leaf_angle 2.2), the two cards of a strand rolled 55 deg apart,
    and weaker veins (leaf normal 0.35, venules 0.1).
  - Edge-on cards: stem half-width 0.006 in a light grey-brown, so a card
    seen edge-on is a faint line, not a black rod.
  - Bark: fewer, larger Lichen colonies (patch_scale 1.75, 3 octaves,
    coverage 0.32) read as black lenticel dashes at 15 m; vertex-colour
    rings give a black foot (0.12 to 0.34 m) and two dark bands under the
    lowest limbs, which carry where the texture has mipped to grey.
  - Crown: droop was 2.2 deg/step on joint weights summing 4.2, so even
    young limbs lay level (a fishbone). Now 1.3 deg/step capped at 11 on
    weights 0.8/1.2/1.6: young limbs ascend, old ones arch over. Longer
    limbs (0.75 at birth, +0.4/step, cap 2.35) that die back from age 5,
    branchlets that die back with them (they used to outgrow the dying
    limb as 1.3 m bare wires), strands that shrink on old low limbs and
    lose their second strand from age 9, and strand reach varied per node
    and per limb: an ovoid crown, widest a little under half way up,
    rounded at the foot. A leaf tuft on each young leader node and faster
    whitening (0.5/step) end the bare brown leader.

Materials (two parts per copy): slot 0 bark - a LICHEN texture, white
"rock" with dark colonies, squashed along each tube by `;(3)` into
horizontal lenticel dashes; new wood is tinted dark red-brown by a
4-argument vertex colour that a growth rule whitens each step, the fine
branchlets stay dark. Slot 1 foliage - a Twig texture: a thin zigzag stem
with 8 alternate deltoid, serrate leaves, used as hanging cards (a card
lies in the plane of the turtle heading and local X; after `$` X is
level, so ^ hangs it near vertical - every card template is kept at
least ~45 deg off level, so no sun glare).
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
    scale=1.0,
    iterations=12,
    mesh_resolution=5,
    elasticity=0.06,
    seed=1,
    # --- trunk
    hb=2.3,              # bare trunk under the first branch
    ih=0.27,             # internode (two per step)
    wt=0.03,             # newest trunk internode width
    vt=1.13,             # trunk thickening per step
    lean=7.0,            # one-shot trunk lean at the foot (deg)
    lc=0.35,             # per-internode pull back toward vertical (deg)
    # Vertex-colour rings up the bare trunk, (height m, grey 0..1): the black
    # foot of an old birch, then two dark bands below the lowest limbs. Each
    # ring is one extra tube segment (10 triangles), so they are few, and a
    # band is three rings close together so its edges stay crisp.
    foot=[(0.0, 0.12), (0.34, 0.12), (0.56, 0.4), (0.85, 0.95),
          (1.34, 1.0), (1.42, 0.22), (1.5, 1.0),
          (2.02, 1.0), (2.1, 0.16), (2.2, 0.2), (2.3, 1.0)],
    base_dark=0.13,      # (variant b only) grey at the trunk foot
    # --- main branches
    segs=[0.32, 0.27, 0.23, 0.18],
    jw=[0, 0.8, 1.2, 1.6],     # droop weight at the base and at joints 1-3
    l0=0.75,             # branch length at birth
    dl=0.4,              # growth per step
    lmax=2.35,           # length cap
    a0=34.0,             # angle from vertical at birth
    da=4.0,              # sinks per step
    amax=68.0,
    arch=1.3,            # joint droop per step (deg, times the joint weight):
    archmax=11.0,        # slow, so young limbs ascend and only old ones arch over
    bw=0.012,            # branch width at birth
    vb=1.16,             # branch thickening per step
    dk=0.25,             # die-back per step past gd (shaded low limbs)
    gd=5.0,
    fine=(0.42, 0.34, 0.30),  # vertex colour of the fine wood (x bark)
    limb=(0.97, 0.96, 0.94),  # vertex colour of the white limbs and trunk
    young=(0.42, 0.33, 0.28), # vertex colour of new wood
    whiten=0.5,               # fraction of the way to white per step
    # --- hanging twigs (cards)
    c0=0.4,              # card scale at birth
    dc=0.06,             # card growth per step
    cm=0.72,             # card cap
    kd=0.05,             # strands on old low limbs shrink by this per step ...
    kg=6.0,              # ... past this age (shade), so the crown's foot rounds in
    kf=9.0,              # strands at least this old lose their second strand
    hp=112.0,            # hang pitch below the branch heading
    tw=55.0,             # roll between the two cards of a strand (not a flat ladder)
    s0=0.3, ds=0.14, sm=1.3,   # secondary branchlet size: birth, growth, cap
    sdk=0.1,             # branchlets die back with their limb past gd
    sp=14.0,             # branchlet droop below level (deg)
    st=0.55,             # branchlets smaller than this are cards only
    sy=50.0,             # secondary branchlet yaw off the limb
    qn=4.0,              # leader nodes younger than this carry a leaf tuft
    # --- foliage texture
    leaf_base=(0.40, 0.56, 0.16),
    leaf_edge=(0.52, 0.64, 0.22),
    stem=(0.40, 0.30, 0.25),
    leaf_pairs=8,        # 8 leaves at 0.30 of the card: smaller leaves (12 at
    leaf_scale=0.30,     # 0.24-0.25, 10 at 0.27-0.28) left bare crowns at 55 m
    petiole=0.06,
    leaf_angle=2.2,      # leaves point down the hanging twig, as on a pendula
    leaf_normal=0.35,
    venule=0.1,
    vein_count=7.0,
    stem_half_width=0.006,
    stem_curve=0.008,    # a slight zigzag: a twig, not a pinnate leaf
    sympodial=1,
    tint=(1.0, 1.0, 1.0),
    # --- bark texture
    bark_tex="Lichen",   # white "rock" with dark patches = birch lenticels
    bark_tint=(0.95, 0.95, 0.93),
    bark_light=(0.94, 0.93, 0.90),
    bark_dark=(0.10, 0.09, 0.09),
    bark_scale=1.75,
    patch_oct=3,
    coverage=0.32,
    bark_vs=3.0,         # `;` V-scale: squashes the marks into horizontal dashes
    bark_warp_u=0.1,
    bark_warp_v=0.9,
    bark_rot=0.0,
    bark_uv=1.0,
    bark_normal=1.2,
    wood_only=0,
)


def foliage_material(P):
    return {
        "base_color": [w(c) for c in P["tint"]],
        "roughness": 10000,
        "texture": {
            "$type": "Twig",
            "leaf": {
                "color_base": wl(P["leaf_base"]),
                "color_edge": wl(P["leaf_edge"]),
                "serration_strength": w(P.get("serration", 0.16)),
                "vein_angle": w(2.2),
                "micro_detail": w(0.2),
                "normal_strength": w(P["leaf_normal"]),
                "lobe_count": 0,
                "lobe_depth": 0,
                "lobe_sharpness": w(1.0),
                "petiole_length": w(P["petiole"]),
                "petiole_width": w(0.018),
                "midrib_width": w(0.07),
                "vein_count": w(P["vein_count"]),
                "venule_strength": w(P["venule"]),
            },
            "stem_color": wl(P["stem"]),
            "stem_half_width": w(P["stem_half_width"]),
            "leaf_pairs": int(P["leaf_pairs"]),
            "leaf_angle": w(P["leaf_angle"]),
            "leaf_scale": w(P["leaf_scale"]),
            "stem_curve": w(P["stem_curve"]),
            "sympodial": bool(P["sympodial"]),
        },
    }


def bark_material(P):
    if P["bark_tex"] == "Lichen":
        tex = {
            "$type": "Lichen",
            "color_rock": wl(P["bark_light"]),
            "color_lichen_a": wl(P["bark_dark"]),
            "color_lichen_b": wl(P.get("bark_dark2", (0.3, 0.27, 0.25))),
            "color_rim": wl(P.get("bark_rim", (0.6, 0.58, 0.55))),
            "coverage": w(P.get("coverage", 0.22)),
            "patch_scale": w(P["bark_scale"]),
            "patch_octaves": int(P.get("patch_oct", 4)),
            "rim_width": w(P.get("rim", 0.1)),
            "species_scale": w(1.5),
            "grain_scale": w(30.0),
            "grain_strength": w(0.25),
            "relief": w(0.4),
            "normal_strength": w(P["bark_normal"]),
        }
    elif P["bark_tex"] == "Marble":
        tex = {
            "$type": "Marble",
            "color_base": wl(P["bark_light"]),
            "color_vein": wl(P["bark_dark"]),
            "scale": w(P["bark_scale"]),
            "warp_strength": w(P.get("marble_warp", 0.8)),
            "vein_frequency": w(P.get("vein_freq", 2.0)),
            "vein_sharpness": w(P.get("vein_sharp", 5.0)),
            "roughness": w(0.4),
            "normal_strength": w(P["bark_normal"]),
        }
    else:
        tex = {
            "$type": "Bark",
            "color_light": wl(P["bark_light"]),
            "color_dark": wl(P["bark_dark"]),
            "scale": w(P["bark_scale"]),
            "warp_u": w(P["bark_warp_u"]),
            "warp_v": w(P["bark_warp_v"]),
            "normal_strength": w(P["bark_normal"]),
            "furrow_multiplier": w(P.get("furrow", 0.4)),
        }
    m = {
        "base_color": [w(c) for c in P["bark_tint"]],
        "roughness": 9000,
        "texture": tex,
        "uv_scale": w(P["bark_uv"]),
    }
    if P["bark_rot"]:
        m["uv_rotation"] = w(P["bark_rot"])
    return m


def generator(P, src, fin):
    mats = {"0": bark_material(P)}
    if not P["wood_only"]:
        mats["1"] = foliage_material(P)
    return {
        "$type": "network.symbios.gen.lsystem",
        "angle": 300000,
        "elasticity": w(P["elasticity"]),
        "finalization_code": fin,
        "iterations": int(P["iterations"]),
        "materials": mats,
        "mesh_resolution": int(P["mesh_resolution"]),
        "prop_mappings": {"0": "Twig"},
        "prop_scale": 10000,
        "seed": str(int(P["seed"])),
        "source_code": src,
        "step": 10000,
        "transform": {"scale": [w(P["scale"])] * 3},
        "tropism": [0, -10000, 0],
        "width": 1000,
    }


def rgb(c):
    return "'(%s,%s,%s)" % (f(c[0]), f(c[1]), f(c[2]))


def young(P):
    """A 4-argument colour: new wood, whitened each step by rule g10."""
    c = P["young"]
    return "'(%s,%s,%s,1)" % (f(c[0]), f(c[1]), f(c[2]))


# ------------------------------------------ variant A: developmental spiral
A_NODES = [
    # (fraction along the limb, kind, roll jitter | side, strand factor q)
    # K = hanging twig cluster, S = secondary branchlet (side +-1).
    # q (times the limb's vigour m) sets how far a strand's lower card
    # reaches: short near the trunk, longest on the outer half, so strands on
    # one limb - and between limbs - end at different heights instead of a
    # ruled hem.
    (0.18, "K", 0, 0.72), (0.29, "S", 1, 0), (0.37, "K", 160, 0.86),
    (0.46, "K", -40, 0.8), (0.55, "S", -1, 0), (0.62, "K", 100, 1.04),
    (0.70, "K", -120, 0.92), (0.78, "K", 30, 1.12), (0.86, "K", 200, 0.98),
    (0.93, "K", -70, 1.08)]


def limb_body(P, nodes, segs, jw):
    """A main limb as drawn at birth: `segs` tube segments (fractions of
    its length) that re-lengthen with age (F/f carry fraction, age,
    vigour), joint droops ^(0,0,weight) that deepen with age, node markers
    placed by moves (no extra tubes) and a tip marker."""
    body = young(P) + "$!(bw,1)"
    if jw[0]:
        body += "^(0,0,%s)" % f(jw[0])
    start = 0.0
    ni = 0
    for k, s in enumerate(segs):
        end = start + s
        if k > 0:
            body += "!(bw*%s,1)^(0,0,%s)" % (f(1 - 0.8 * start), f(jw[k]))
        seg_nodes = ""
        while ni < len(nodes) and nodes[ni][0] <= end + 1e-9:
            t, kind, j, q = nodes[ni]
            d = t - start
            if kind == "K":
                seg_nodes += "[f(l0*m*%s,%s,0,m)K(c0,%s,m*%s,0)]" % (f(d), f(d), f(j), f(q))
            else:
                seg_nodes += "[f(l0*m*%s,%s,0,m)S(s0,%s,0)]" % (f(d), f(d), f(j))
            ni += 1
        body += seg_nodes + "F(l0*m*%s,%s,0,m)" % (f(s), f(s))
        start = end
    return body + "T(c0)"


def growth_rules(P):
    # limb length by age: grows to the cap, then shaded low limbs die back
    L = "max(0.35*lm,min(lm,l0+dl*(g+1))-%s*max(0,g+1-%s))" % (f(P["dk"]), f(P["gd"]))
    # strand size by age: grows to the cap, then shrinks on old low limbs
    C = "max(0.6*c0,min(cm,c0+dc*(g+1))-%s*max(0,g+1-%s))" % (f(P["kd"]), f(P["kg"]))
    return [
        "g1: F(x,r,g,m) -> F(r*m*%s,r,g+1,m)" % L,
        "g2: f(x,r,g,m) -> f(r*m*%s,r,g+1,m)" % L,
        "g3: &(a,g,j) -> &(min(am,a0+j+da*(g+1)),g+1,j)",
        "g4: ^(a,g,k) -> ^(min(ax,ar*(g+1))*k,g+1,k)",
        "g5: !(w,t) : t > 0 -> !(w*vb,t)",
        "g6: !(w,t) : t < 0 -> !(w*vt,t)",
        "g7: K(c,j,q,g) -> K(%s,j,q,g+1)" % C,
        "g8: T(c) -> T(min(cm,c+dc))",
        "g9: S(c,s,g) -> S(max(s0,min(sm,s0+ds*(g+1))-%s*max(0,g+1-%s)),s,g+1)" % (f(P["sdk"]), f(P["gd"])),
        # new wood is dark red-brown and whitens over a few years: the
        # 4-argument colour is the only one the grammar ages
        "g10: '(r,g,b,a) -> '(min(%s,r+%s),min(%s,g+%s),min(%s,b+%s),a)" % (
            f(P["limb"][0]), f(P["whiten"] * (P["limb"][0] - P["young"][0])),
            f(P["limb"][1]), f(P["whiten"] * (P["limb"][1] - P["young"][1])),
            f(P["limb"][2]), f(P["whiten"] * (P["limb"][2] - P["young"][2]))),
        "g11: Q(g) -> Q(g+1)",
    ]


def defines(P):
    return [
        "#define l0 %s" % f(P["l0"]), "#define dl %s" % f(P["dl"]), "#define lm %s" % f(P["lmax"]),
        "#define a0 %s" % f(P["a0"]), "#define da %s" % f(P["da"]), "#define am %s" % f(P["amax"]),
        "#define ar %s" % f(P["arch"]), "#define ax %s" % f(P["archmax"]),
        "#define c0 %s" % f(P["c0"]), "#define dc %s" % f(P["dc"]), "#define cm %s" % f(P["cm"]),
        "#define s0 %s" % f(P["s0"]), "#define ds %s" % f(P["ds"]), "#define sm %s" % f(P["sm"]),
        "#define vb %s" % f(P["vb"]), "#define vt %s" % f(P["vt"]),
        "#define ih %s" % f(P["ih"]), "#define wt %s" % f(P["wt"]), "#define bw %s" % f(P["bw"]),
    ]


def omega(P):
    """The bare trunk: one tube segment per colour ring in P["foot"] (the
    foot ring and every ring above take the colour set before their F), the
    lean after the first, then white to the first limb at hb."""
    rings = P["foot"]
    out = ";(%s)" % f(P["bark_vs"])
    y = 0.0
    for n, (h, g) in enumerate(rings[1:]):
        out += "'(%s)!(wt,-1)F(%s)" % (f(g), f(h - y))
        if n == 0 and P.get("lean", 0):
            out += "&(%s)" % f(P["lean"])
        y = h
    if P["hb"] - y > 0.01:
        out += "'(1)!(wt,-1)F(%s)" % f(P["hb"] - y)
    return "omega: " + out + "A(0)"


def card(pre, s):
    return "[,(1)%s~(0,%s)]" % (pre, s)


def strand(pre, s, k=0.75, tw=0.0):
    """Two cards end to end (overlapping by the lower card's bare stalk),
    the lower one rolled `tw` about the hang: a long hanging twig that is
    not one flat ladder of leaves."""
    roll = "/(%s)" % f(tw) if tw else ""
    k = k if isinstance(k, str) else f(k)
    return "[,(1)%s~(0,%s)f(%s*0.82)%s~(0,%s*%s)]" % (pre, s, s, roll, s, k)


def organ_rules(P):
    """Finalization: markers become cards (and the fine branchlet tube).
    After `$` a limb's local X is level, so ^ pitches straight down past
    the limb's heading: every hanging card is near vertical (never level,
    so no sun glare) and a roll about it turns its face."""
    hp = P["hp"]
    tw = P["tw"]
    fine = rgb(P["fine"])
    st = P["st"]
    if P["wood_only"]:
        return ("K(c,j,q,g) : * -> \nT(c) : * -> \nB(m) : * -> \nQ(g) : * -> \nA(k) : * -> !(0.02)F(0.5)\n"
                "S(c,s,g) : c >= %s -> %s!(0.009)+(s*%s)$F(c)\nS(c,s,g) : c < %s -> " % (
                    f(st), fine, f(P["sy"]), f(st)))
    sw = "S(c,s,g) : c >= %s -> " % f(st) + fine + "!(0.009)+(s*%s)$^(%s)" % (f(P["sy"]), f(P["sp"]))
    s_cards = (strand("^(%s)/(40)" % f(hp - 5), "c*0.6", tw=-tw) + card("^(%s)/(-60)" % f(hp - 15), "c*0.55"))
    s_tip = (strand("^(%s)/(-30)" % f(hp - 25), "c*0.65", tw=tw) + card("^(%s)/(80)" % f(hp - 45), "c*0.55") +
             card("^(40)/(20)", "c*0.5"))
    # the node's q sets how far the strand's lower card reaches, so strands
    # end at different heights while no leaf grows past the cap's size
    k_main = strand("^(%s)/(j)" % f(hp), "c", "q*0.75", tw=tw)
    kf = f(P["kf"])
    fin = [
        "K(c,j,q,g) : g < %s -> " % kf + k_main +
        strand("^(%s)/(j+95)" % f(hp - 15), "c*0.8", 0.6, -tw) + card("^(%s)/(j-110)" % f(hp + 12), "c*0.7"),
        # old shaded limbs low in the crown: sparser
        "K(c,j,q,g) : g >= %s -> " % kf + k_main + card("^(%s)/(j-110)" % f(hp + 12), "c*0.7"),
        # limb tip: the whip end hangs too (steep rolls keep every card off level)
        "T(c) : * -> " + card("^(45)/(70)", "c*0.8") + strand("^(%s)/(60)" % f(hp * 0.8), "c*0.9", tw=tw) +
        card("^(%s)/(-50)" % f(hp * 0.9), "c") + card("^(70)/(-110)", "c*0.7"),
        # secondary branchlet: out to the side in the limb's spread; curtains
        sw + "[f(c*0.45)" + s_cards + "]F(c)" + s_tip,
        "S(c,s,g) : c < %s -> +(s*%s)$^(%s)" % (f(st), f(P["sy"]), f(P["sp"])) + s_cards + s_tip,
        # the pair of limbs born on the last step: a small shoot
        "B(m) : * -> $" + card("^(10)", "0.42") + card("^(60)/(90)", "0.4") + card("^(80)/(-80)", "0.38"),
        # a leaf tuft on each young leader node, so the top of the crown is
        # leafy to the tip instead of a bare brown antenna
        "Q(g) : g < %s -> " % f(P["qn"]) + card("&(38)", "0.44") + card("/(150)&(42)", "0.4") +
        card("/(-100)&(34)", "0.36"),
        "Q(g) : g >= %s -> " % f(P["qn"]),
        # leader tip
        ("A(k) : * -> !(0.02)[" + card("&(35)", "0.45") + card("/(120)&(40)", "0.45") +
         card("/(240)&(38)", "0.42") + "]F(0.28)" + card("/(60)&(22)", "0.42") +
         card("/(180)&(25)", "0.4") + card("/(300)&(20)", "0.4") + "!(0.012)F(0.2)" +
         card("/(30)&(10)", "0.36") + card("/(200)", "0.38")),
    ]
    return "\n".join(fin)


def variant_a(P):
    """Retrospective growth, two trunk nodes a step, a limb at most nodes
    (137.5 spiral). A limb is born short and steep; each step it lengthens
    (to a cap, then low shaded limbs die back), sinks and droops a little
    at each joint, so old limbs arch out and the young top ascends. Limb
    nodes carry hanging-twig markers K (with their own length factor and
    age: they shrink again low in the crown) and branchlet markers S whose
    size grows with age; finalization turns K into hanging Twig-card
    strands and S into a thin dark branchlet with its own curtain. Each
    leader node carries a marker Q that is a leaf tuft while it is young."""
    lines = defines(P) + [omega(P)]

    # The trunk frame never rolls: each limb takes its spiral azimuth from
    # the node counter k (137.5 k), so a one-shot base lean stays in one
    # plane and a small ^ per internode (lc) pulls it back up against the
    # tropism - a sweep, not a hook.
    def br(m, j, dk):
        a = "a0" if j == 0 else ("a0+%s" % f(j) if j > 0 else "a0-%s" % f(-j))
        return "[/(137.5*(k+%d))&(%s,0,%s)B(%s)]" % (dk, a, f(j), f(m))
    I = young(P) + "!(wt,-1)^(%s)F(ih)" % f(P["lc"])

    def tuft(dk):
        return "[/(137.5*(k+%d)+70)Q(0)]" % dk
    lines += [
        "a1: 0.5 : A(k) -> %s%s%s%s%s%sA(k+2)" % (I, br(1.0, 0, 0), tuft(0), I, br(0.86, 4, 1), tuft(1)),
        "a2: 0.3 : A(k) -> %s%s%s%s%sA(k+2)" % (I, br(1.08, -3, 0), tuft(0), I, tuft(1)),
        "a3: 0.2 : A(k) -> %s%s%s%s%s[/(97)%s]%sA(k+2)" % (
            I, br(0.92, 2, 0), tuft(0), I, br(1.0, -2, 1), br(0.7, 6, 1)[1:-1], tuft(1)),
    ]
    lines.append("b1: B(m) -> " + limb_body(P, A_NODES, P["segs"], P["jw"]))
    lines += growth_rules(P)
    return generator(P, "\n".join(lines), organ_rules(P))


# ------------------------------------------ variant B: prospective, whips
B_DEFAULTS = dict(
    elasticity=0.2,      # high: whips of short segments droop, the trunk is a fixpoint
    ih=0.27,
    hb=2.4,
    wt=0.19,             # trunk width at the base
    lb=2.3,              # longest limb (mid crown)
    whip=1.1,            # whip length on the longest limb
    wc=0.34,             # card scale along whips
    a_bot=62.0, a_top=28.0,
)


def variant_b(P):
    """Prospective (explicit state): A(n) lays node n of N with its limb
    sized at birth by a mesotonic profile (longest limbs mid-crown); a limb
    L expands next step into four segments with rising droop and four
    whips W; finalization draws each whip as three thin segments that the
    strong global tropism bends into a hanging curve, cards along it."""
    N = 2 * int(P["iterations"]) - 2
    lines = [
        "#define N %d" % N, "#define ih %s" % f(P["ih"]), "#define wt %s" % f(P["wt"]),
        "#define lb %s" % f(P["lb"]), "#define ab %s" % f(P["a_bot"]), "#define at %s" % f(P["a_top"]),
        "#define wh %s" % f(P["whip"]),
        "omega: ;(%s)'(%s)!(wt)F(0.3)'(%s)!(wt*0.97)F(0.45)'(1)!(wt*0.95)F(%s)/(23)A(0)" % (
            f(P["bark_vs"]), f(P["base_dark"]), f((P["base_dark"] + 1) / 2), f(P["hb"] - 0.75)),
    ]
    # u = n/N in 0..1 bottom to top; profile peaks at u ~ 0.3
    prof = "(0.45+0.55*sin(3.1416*min(1,(n/N)*1.25+0.12)))"
    wid = "(wt*0.9*(1-0.85*n/N))"
    ang = "(ab-(ab-at)*n/N)"
    lines += [
        # two nodes a step (n counts nodes, 0..2N)
        "a1: 0.6 : A(n) -> !(%s)F(ih)[&(%s)L(lb*%s*(1-0.6*n/N),%s*0.3)]/(137.5)!(%s)F(ih)[&(%s+4)L(lb*%s*0.9*(1-0.6*n/N),%s*0.3)]/(137.5)A(n+2)" % (wid, ang, prof, wid, wid, ang, prof, wid),
        "a2: 0.25 : A(n) -> !(%s)F(ih*0.8)[&(%s+5)L(lb*%s*0.8*(1-0.6*n/N),%s*0.28)]/(151)[&(%s-4)L(lb*%s*0.7*(1-0.6*n/N),%s*0.25)]/(137.5)!(%s)F(ih)/(137.5)A(n+2)" % (wid, ang, prof, wid, ang, prof, wid, wid),
        "a3: 0.15 : A(n) -> !(%s)F(ih*1.1)/(137.5)!(%s)F(ih)[&(%s)L(lb*%s*(1-0.6*n/N),%s*0.3)]/(137.5)A(n+2)" % (wid, wid, ang, prof, wid),
    ]
    W = "W(l*%s,%s)"
    lines.append(
        "l1: L(l,w) -> %s$!(w)F(l*0.3)[+(45)W(l*0.5,0)][-(40)W(l*0.45,1)]!(w*0.75)^(8)F(l*0.28)"
        "[+(-50)W(l*0.55,1)][+(35)W(l*0.5,0)]!(w*0.5)^(12)F(l*0.24)[-(30)W(l*0.45,0)]!(w*0.35)^(16)F(l*0.18)T(l)" % rgb(P["limb"]))
    src = "\n".join(lines)
    fine = rgb(P["fine"])
    wc = P["wc"]

    def cd(pre, s):
        return "[,(1)%s~(0,%s)]" % (pre, s)
    if P["wood_only"]:
        fin = ["W(l,k) : * -> %s!(0.008)$^(55)F(l*0.4)F(l*0.35)F(l*0.25)" % fine, "T(l) : * -> ", "A(n) : * -> !(0.02)F(0.4)",
               "L(l,w) : * -> "]
    else:
        fin = [
            # a whip: pitched below the limb, three short segments droop into a hang;
            # cards hang along it and off its end
            ("W(l,k) : * -> %s!(0.008)$^(55)F(l*0.4)" % fine + cd("^(70)/(40)", "%s" % f(wc)) +
             cd("^(90)/(-50)", "%s" % f(wc * 0.9)) + "F(l*0.35)" + cd("^(60)/(80)", "%s" % f(wc)) +
             cd("^(80)/(-20)", "%s" % f(wc)) + "F(l*0.25)" + "[,(1)^(30)~(0,%s)f(%s)~(0,%s)]" % (f(wc * 1.2), f(wc * 1.2), f(wc)) +
             cd("^(60)/(90)", "%s" % f(wc))),
            ("T(l) : * -> " + "[,(1)^(40)~(0,%s)f(%s)~(0,%s)]" % (f(wc * 1.2), f(wc * 1.2), f(wc)) +
             cd("^(90)/(60)", f(wc)) + cd("^(100)/(-60)", f(wc))),
            ("A(n) : * -> !(0.02)[" + cd("&(35)", "0.45") + cd("/(120)&(40)", "0.45") +
             cd("/(240)&(38)", "0.42") + "]F(0.3)" + cd("/(60)&(22)", "0.42") +
             cd("/(180)&(25)", "0.4") + cd("/(300)&(20)", "0.4") + "!(0.012)F(0.2)" +
             cd("/(30)&(10)", "0.36") + cd("/(200)", "0.38")),
            "L(l,w) : * -> $" + cd("^(15)", "0.42") + cd("^(60)/(90)", "0.4") + cd("^(80)/(-80)", "0.38"),
        ]
    return generator(P, src, "\n".join(fin))


VARIANTS = {"a": variant_a, "b": variant_b}
DEFAULTS = {"b": B_DEFAULTS}


def main():
    if len(sys.argv) < 3:
        sys.argv = [sys.argv[0], "a", "pale_birch.json"]
    variant = sys.argv[1]
    out = sys.argv[2]
    P = dict(BASE)
    P.update(DEFAULTS.get(variant, {}))
    for kv in sys.argv[3:]:
        k, v = kv.split("=", 1)
        old = P.get(k)
        if isinstance(old, list):
            P[k] = [float(x) for x in v.split(",")]
        elif isinstance(old, tuple):
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
