#!/usr/bin/env python3
"""Builder for a Japanese flowering cherry in full blossom (Prunus
serrulata, a 'Kanzan'-like park cherry in mid April, the leaves just
breaking), the tree that replaces the catalogue's `lsys_flowering_tree`.

usage: cherry.py [VARIANT [OUT.json [key=value ...]]]
       cherry.py          -> the shipped tree: variant a, BASE, cherry.json
                             in the current directory
       cherry.py w white.json    -> the white-blossom re-skin (pear, hawthorn)
       cherry.py s summer.json   -> the summer-green re-skin

Writes one network.symbios.gen.lsystem generator in wire form (every
decimal a whole number of ten-thousandths). Grammar units are metres
(P["scale"] is 1.0).

The shipped tree (iterations 10, seed 1) is 5.93 m tall above the ground
and 7.3 x 6.2 m across, a vase of four scaffolds leaving a 1.5 m bole at
44-54 deg from vertical, 3,308 triangles in 3 parts at mesh_resolution 5
(950 of them wood). Seed 2 is another individual of the same size (6.17 m,
7.3 x 6.3 m, 3,346 triangles). Iterations are age, and the cards grow with
it: 4 is a 2.8 m sapling 3.3 m across (1,238 triangles), 5 is 3.3 m
(1,986), 6 is 4.0 m (2,602), 8 is 5.0 m (3,164), 9 is 5.6 m, 11 is 6.2 m
and 12 (the cap) 6.4 m and 8.2 m across; from 9 up the cost is the same
3,308. Measure with `render --generator cherry.json` (the `subject size`
line; its height includes the 0.1 m of trunk sunk below the origin,
`sink`).

It is the apple's grammar (apple.py beside this file, itself oak.py's:
a dichotomous sympodial crown grown by age, every internode by one law
`r*m*min(lm, l0+dl*age)`, the last fork drawn by its cards alone, every
shoot started at its own width and every limb launched from the foot so
the trunk is one tube) reshaped into a vase and dressed in blossom:
  - No leader (`leader` vigour 0): the trunk ends where the four
    scaffolds leave it, so the centre stays open and the crown is wider
    than tall. Its fork shoots climb on the upper flank at 32 deg and
    spread on the lower at 38, and an elasticity of 0.07 arches the outer
    shoots out and down.
  - Blossom along the limbs, not only at their ends: every internode that
    ends in a fork, from the first fork on, carries four spurs (an elbow
    carries none: a bracket before its bend opens a wedge in the tube, see
    oak.py), hung back along it from its end
    (fractions 0.2, 0.45, 0.7, 0.95 of its length, rolled about 137.5 deg
    apart) by an `f` that grows with it, so no bracket breaks a tube. Each
    spur is a 2-card spray; each tip a 5-card mass of blossom. Half the
    tips put two small bronze-green leaf sprays in the mass's two foremost
    places and carry the blossom on round them, seven cards in all: the
    leaves just breaking.
  - The cards grow with the tree: every apex and spur carries the plant's
    age n (A(v,s,d,n), K(v,n), stepped by `st`, `ag` and `kg` even after
    the crown stops forking), and finalization scales each card by
    min(1, 0.4 + 0.075 n): 0.7 at 4, whole from 8. Without it a sapling
    wore the adult's blossom and read as a ball on a stick.

Variants (judged on 448 px sheets, the 12 m play view, 30, 50 and 150 m
play views beside the apple, and eye-level shots at 2-6 m):
  a  SHIPPED (see above).
  w  RE-SKIN, white blossom: the petal palette alone (see WHITE).
  s  RE-SKIN, summer green: slot 3's Flower card swapped for a Twig card
     of 8 leaf pairs at leaf_scale 0.5 (7 pairs at 0.32, the apple's,
     left the crown thin and see-through at 30 m), slot 2 tinted green.
  l  REJECTED (leader vigour 0.75, lean 25): the leader grows up through
     the vase and puts a top-knot of blossom above a flat crown, at every
     seed; the first draft (leader 0.8, lean 18) reached 6.8 m with it.
  g  REJECTED, the first blossom card (a 3 x 2 Flower atlas, petals 0.92
     long and 0.7 wide, curled, a dark red centre and throat): at 12 m
     giant flowers 0.2 m across, each a dark-centred dot, so the crown
     read as speckled mauve sequins; and with the shipped card counts it
     thins to bare sticks on a 200 px sheet (under the first draft's
     7-card masses and 3-card sprays it had looked solid at 448 px).
  d  REJECTED (petals 0.85 x 0.7 on the 4 x 3 atlas): smaller, separate
     flowers look right up close but the crown vanishes at distance -
     cards draw at alpha mask 0.5, so a card under half covered mips
     away. The shipped petals (1.0 long, 0.95 wide) overlap, and the crown
     holds to 150 m, better than the apple's.
  m  REJECTED (spray_n 3, +648 triangles; with cs 0.5-0.55 and 6-7-card
     masses as well, +826 to +1,004): a denser cloud that hides the dark
     limbs inside it, for no gain at 30 m.
  r  REJECTED bark (`;(8)` on the trunk, `;(12/v)` on the limbs, grain
     0.25, normal 1.2): crisp lenticel streaks on the trunk, but on the
     limbs a twisted-rope herringbone at 2-6 m.
  u  REJECTED bark (one `;(3)` for every tube): dashes across the trunk,
     but along every limb (see the engine notes).
  b  REJECTED bark (the Bark texture, red-brown, `;(3)`): smooth plain
     red-brown at 3 m, no bands at all (its colour follows its FBM only,
     as oak.py found).
  Also tried: a fifth, upright scaffold (pitch 28, vigour 0.75) made the
  same top-knot as a leader; scaffold pitches 38-48 with elasticity 0.055
  and wider lower shoots (fork_b 30,44 and fork_roll 30,115) changed the
  silhouette by a few per cent and were not kept; low-contrast petals
  (curl 0, throat near the base colour, normal 0.6) are what turned the
  sequins into a cloud, more than the atlas size did. Rejected settings:
  leaf cards of 3 pairs at leaf_scale 0.3 in darker bronze (0.42, 0.40,
  0.14) on a third of the tips, and even on every tip, were dark specks
  at 12 m; 4 pairs at 0.45, brighter, on half the tips show as a few
  olive-bronze sprigs among the pink.

Materials (three parts per copy): slot 0 bark - a Lichen texture used as
bark, the birch's recipe: a dark glossy red-brown face (roughness 0.6)
whose pale tan colonies are the lenticels, squashed by the `;` V-scale
into horizontal dashes (`;(4)` on the trunk, `;(4/v)` on a branch of
vigour v); slot 2 leaves - a Twig card of 4 pairs of small toothed
bronze-green leaves; slot 3 blossom - a Flower atlas of 4 rows x 3
columns, twelve 5-petalled notched pink flowers to a Twig card (0.7 x 1.0,
so each cell is nearly square and the flowers round). Slot 1 is empty, so
the numbering means what the playbook proposes (0 bark, 2 leaf, 3
flower). Every colour lives in the texture under a white base colour, so
a re-skin re-tints the texture itself.

Engine facts found here (see also docs/lsystem-playbook.md):
  - A sprite atlas on a card is drawn whole: an L-system card maps the
    whole texture, so a Flower of variant_rows x variant_cols is a
    cluster of that many flowers on one card (particles are what pick one
    cell). Twig cards are 0.7 x 1.0, so 4 rows x 3 columns keeps the
    flowers round. Cards bake at 256 px (config::textures::SURFACE), about
    64 x 85 px a flower here.
  - The `;` V-scale fights the U wrap: U runs once round every tube
    whatever its girth, so a pattern squashed across a 0.35 m trunk is
    stretched along a 0.1 m twig. One `;` cannot serve both: setting
    `;(c/v)` in each internode (width goes with vigour) keeps the dashes
    across every tube. Too high a V-scale turns fine texture detail into a
    rope-like herringbone.
  - A module stops counting once no rule rewrites it: an apex past its
    last fork keeps the age it had there unless a rule (`ag` here) steps
    it on, so a per-plant counter needs a rewrite at every stage.
  - A finalization card scale may be an expression of the marker's
    parameters, `~(0,0.6*min(1,ka+kb*n))`, against growth-block defines.
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
    iterations=10,        # = age: 4-5 a sapling, 12 the cap
    mesh_resolution=5,
    elasticity=0.07,      # the apple's 0.06 and a little: the outer shoots arch out and down
    seed=1,
    # --- growth law of every internode: r * m * min(lm, l0 + dl * age)
    l0=0.4, dl=0.1, lm=1.3,
    # --- trunk
    wt=0.11, vt=1.11,     # 0.3 m at 10
    sink=0.1,             # trunk base below the origin, buried on a slope
    bole=[(0.12, 1.4), (0.6, 1.1), (1.5, 1.0)],   # (height at maturity, width factor): a 1.5 m bole
    lean=4.0,             # one-shot lean after the foot
    # scaffolds: (after bole segment, azimuth, pitch from vertical, vigour,
    # generations short of gens)
    limbs=[(2, 20, 46, 1.0, 0), (2, 110, 54, 0.95, 0), (2, 205, 44, 1.0, 0), (2, 295, 52, 0.95, 0)],
    leader=(25, 0.0, 2),  # (lean, vigour, generations short); vigour 0: no leader, an open vase
    # --- branching (as apple.py; the upper shoot climbs, the lower spreads)
    gens=5, ghost=1,
    bw=0.075, vb=1.12,
    kf=(0.84, 0.8), fork_roll=(35.0, 105.0), fork_b=(32.0, 38.0),
    k_elb=0.9, elb_roll=40.0, elb=22.0,
    p_fork=0.62, p_elb=0.24, p_stall=0.14,
    dsplit=2, crook=16.0,
    collar_d=2,
    shoot_start=1.0,
    # --- blossom (slot 3) and leaves (slot 2); a Twig card is 0.7 x 1.0 before cs
    card_age=(0.4, 0.075),   # card scale x min(1, ka + kb * age): 0.7 at 4, full from 8
    cs=0.6, back=0.3, clump_n=5,        # tip mass: card scale, swept back, cards
    dspray=1, spray=0.75, spray_n=2, spray_fwd=0.1,   # spurs from generation 1: scale (x cs), cards
    # spurs along each forking internode: (fraction back from its end, roll, pitch)
    spurs=[(0.2, 40.0, 60.0), (0.45, 177.5, 55.0), (0.7, 315.0, 60.0), (0.95, 92.5, 58.0)],
    leaf_p=0.5, leaf_n=2, leaf_cs=0.5,   # share of tips with a few leaf cards
    # sRGB; a low-contrast flower (throat near the petal, no curl, a soft
    # normal) reads as a cloud, a dark centre as sequins
    petal_base=(0.95, 0.58, 0.74), petal_edge=(0.97, 0.70, 0.82), petal_throat=(0.94, 0.56, 0.72),
    center=(0.92, 0.50, 0.62), petals=5, notch=0.12,
    # 12 flowers a card; petals long and wide enough to overlap, or the
    # cards mip away under the 0.5 alpha mask at distance
    fl_rows=4, fl_cols=3, petal_len=1.0, petal_w=0.95, petal_curl=0.0, fl_normal=0.6,
    leaf_base=(0.52, 0.50, 0.18), leaf_edge=(0.62, 0.44, 0.20),
    stem=(0.35, 0.22, 0.16),
    leaf_pairs=4, leaf_scale=0.45, leaf_angle=1.6,
    blossom_kind="flower",   # "leaf": slot 3 a Twig leaf card (preset s, summer)
    summer_base=(0.20, 0.36, 0.12), summer_edge=(0.26, 0.42, 0.14), summer_scale=0.5,
    # --- bark
    bark_kind="lichen",   # "lichen" or "bark" (the Bark texture)
    bark_rough=0.6,
    bark_face=(0.26, 0.11, 0.10),     # sRGB: the glossy red-brown face
    len_a=(0.58, 0.48, 0.40), len_b=(0.46, 0.38, 0.32), len_rim=(0.20, 0.10, 0.08),   # lenticels
    len_scale=1.75, len_cover=0.25, len_rim_w=0.1, len_species=1.5, len_grain=0.0, len_normal=0.6,
    bark_light=(0.36, 0.20, 0.16), bark_dark=(0.06, 0.03, 0.03),
    bark_scale=3.0, bark_normal=4.0, bark_warp_u=0.6, bark_warp_v=0.05,
    furrow=0.3, furrow_u=1.0, furrow_v=1.0, furrow_shape=1.0,
    bark_uv=2.0, bark_vs=4.0, bark_vs_limb=4.0,   # `;` on the trunk; on a branch bark_vs_limb / vigour
    wood_only=0,
)

# white blossom: the petal palette of a pear or a hawthorn
WHITE = dict(petal_base=(0.97, 0.96, 0.94), petal_edge=(0.99, 0.98, 0.97), petal_throat=(0.94, 0.92, 0.86),
             center=(0.80, 0.62, 0.55))

VARIANTS = {
    "a": {},
    # the two re-skins, as presets (see the docstring)
    "w": WHITE,
    "s": dict(blossom_kind="leaf", leaf_base=(0.20, 0.36, 0.12), leaf_edge=(0.26, 0.42, 0.14)),
    # rejected (see the docstring)
    "l": dict(leader=(25.0, 0.75, 2)),
    "g": dict(fl_rows=3, fl_cols=2, petal_len=0.92, petal_w=0.7, petal_curl=0.3, fl_normal=1.5,
              petal_throat=(0.88, 0.36, 0.54), center=(0.62, 0.18, 0.30)),
    "d": dict(petal_len=0.85, petal_w=0.7),
    "m": dict(spray_n=3),
    "r": dict(bark_vs=8.0, bark_vs_limb=12.0, len_grain=0.25, len_normal=1.2),
    "u": dict(bark_vs=3.0, bark_vs_limb=0.0, len_grain=0.25, len_normal=1.2),
    "b": dict(bark_kind="bark", bark_vs=3.0, bark_vs_limb=0.0, bark_light=(0.5, 0.26, 0.2),
              bark_dark=(0.1, 0.04, 0.04), bark_scale=5.0),
}


def leaf_texture(P, base, edge, pairs, scale):
    return {
        "$type": "Twig",
        "leaf": {
            "color_base": wl(base),
            "color_edge": wl(edge),
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
        "leaf_pairs": int(pairs),
        "leaf_angle": w(P["leaf_angle"]),
        "leaf_scale": w(scale),
        "stem_curve": w(0.01),
        "sympodial": True,
    }


def flower_texture(P):
    return {
        "$type": "Flower",
        "seed": 3,
        "variant_rows": int(P["fl_rows"]),
        "variant_cols": int(P["fl_cols"]),
        "petal": {
            "seed": 5,
            "variant_rows": 1,
            "variant_cols": 1,
            "color_base": wl(P["petal_base"]),
            "color_edge": wl(P["petal_edge"]),
            "color_throat": wl(P["petal_throat"]),
            "length": w(P["petal_len"]),
            "width": w(P["petal_w"]),
            "peak": w(0.65),
            "tip_notch": w(P["notch"]),
            "curl": w(P["petal_curl"]),
            "asymmetry": w(0.15),
            "normal_strength": w(P["fl_normal"]),
        },
        "petal_count": int(P["petals"]),
        "center_radius": w(0.12),
        "center_color": wl(P["center"]),
        "dot_density": w(0.5),
        "normal_strength": w(P["fl_normal"]),
    }


def bark_material(P):
    if P["bark_kind"] == "lichen":
        # the birch's recipe (lsys_birch): a Lichen texture's bare "rock" is
        # the bark, its colonies the lenticels, squashed into horizontal
        # dashes along every tube by the `;` V-scale
        return {
            "base_color": [10000, 10000, 10000],
            "roughness": w(P["bark_rough"]),
            "texture": {
                "$type": "Lichen",
                "patch_scale": w(P["len_scale"]),
                "patch_octaves": 3,
                "coverage": w(P["len_cover"]),
                "rim_width": w(P["len_rim_w"]),
                "species_scale": w(P["len_species"]),
                "color_rock": wl(P["bark_face"]),
                "color_lichen_a": wl(P["len_a"]),
                "color_lichen_b": wl(P["len_b"]),
                "color_rim": wl(P["len_rim"]),
                "grain_scale": w(30.0),
                "grain_strength": w(P["len_grain"]),
                "relief": w(0.4),
                "normal_strength": w(P["len_normal"]),
            },
            "uv_scale": w(P["bark_uv"]),
        }
    return {
        "base_color": [10000, 10000, 10000],
        "roughness": 9000,
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
        if P["leaf_p"] > 0:
            mats["2"] = {"base_color": [10000, 10000, 10000], "roughness": 10000,
                         "texture": leaf_texture(P, P["leaf_base"], P["leaf_edge"], P["leaf_pairs"], P["leaf_scale"])}
        if P["blossom_kind"] == "leaf":
            # the summer re-skin: every blossom card a full leaf spray
            mats["3"] = {"base_color": [10000, 10000, 10000], "roughness": 10000,
                         "texture": leaf_texture(P, P["summer_base"], P["summer_edge"], 8, P["summer_scale"])}
        else:
            mats["3"] = {"base_color": [10000, 10000, 10000], "roughness": 9000, "texture": flower_texture(P)}
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


def card(pre, s, slot):
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


def mass(cs, back, n, fwd=0.0, slot=3, skip=0):
    """n cards of the tip mass, each scaled by the age factor `min(1, ka +
    kb * n)` of the marker it replaces (the module's own n)."""
    out = "$" + ("f(%s)" % f(fwd) if fwd else "")
    for i in MASS_ORDER[skip:skip + n]:
        rho, beta, sig, k, b = MASS[i]
        pre = ("f(-%s)" % f(back)) if b else ""
        out += card(pre + "/(%s)&(%s)/(%s)" % (f(rho), f(beta), f(sig)), "%s*min(1,ka+kb*n)" % f(cs * k), slot)
    return out


def start(width):
    """Begin a fresh tube here at `width`, as oak.py's `start`."""
    return "!(%s/vb,1)f(0)" % width


def variant_a(P):
    """apple.py's grammar as a vase in blossom (see the module docstring).
    A(v,s,d,n): an apex of vigour v, side s, generation d and plant age n;
    K(v,n) a blossom spur; F/f carry (length, fraction, age, vigour) and
    re-lengthen every step; `!(w,t)` thickens every step."""
    G = int(P["gens"])
    lines = [
        "#define l0 %s" % f(P["l0"]), "#define dl %s" % f(P["dl"]), "#define lm %s" % f(P["lm"]),
        "#define bw %s" % f(P["bw"]), "#define vb %s" % f(P["vb"]), "#define vt %s" % f(P["vt"]),
        "#define wt %s" % f(P["wt"]), "#define G %d" % G,
        "#define ka %s" % f(P["card_age"][0]), "#define kb %s" % f(P["card_age"][1]),
    ]
    lm = P["lm"]
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
                om += "[%s/(%s)&(%s)%sA(%s,%d,%d,0)]" % (
                    up, f(az), f(pitch), start("bw*%s" % f(v)), f(v), 1 if k % 2 == 0 else -1, int(g))
    om += trunk
    if P["leader"] and P["leader"][1] > 0:
        om += "&(%s)A(%s,1,%d,0)" % (f(P["leader"][0]), f(P["leader"][1]), int(P["leader"][2]))
    lines.append("omega: " + om)

    # The `;` V-scale per branch: U wraps once round a tube, so on a thin
    # branch the texture is squeezed round it and the lenticels run along
    # it; a V-scale rising as the vigour falls keeps them across every tube.
    vs = (";(%s/v)" % f(P["bark_vs_limb"])) if P["bark_vs_limb"] else ""

    def internode(split, ghost):
        if ghost:
            return "f(l0*v,1,0,v)$"
        if split:
            return ("%s!(bw*v,1)F(l0*v*0.5,0.5,0,v)$/(-s*70)&(%s)"
                    "!(bw*v*0.93,1)F(l0*v*0.5,0.5,0,v)$" % (vs, f(P["crook"])))
        return "%s!(bw*v,1)F(l0*v,1,0,v)$" % vs
    # spurs: blossom sprays hung back along the internode just drawn, from
    # its end, by an `f` that grows with it (no bracket inside a tube)
    spur = "".join("[f(-l0*v*%s,-%s,0,v)/(s*%s)&(%s)K(v,n+1)]" % (f(fr), f(fr), f(ro), f(pi))
                   for fr, ro, pi in P["spurs"])
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
        sp = spur if lo >= dp else ""
        collar = "F(0.01)" if (lo < int(P["collar_d"]) and not ghost) else ""
        st = "" if ghost else start("bw*v*%s" % f(k1 * P["shoot_start"] * (0.93 if lo < ds else 1.0)))
        lines += [
            "f%d: %s : A(v,s,d,n) : %s -> %s%s[/(s*%s)&(%s)%sA(v*%s,s,d+1,n+1)]%s/(-s*%s)&(%s)A(v*%s,-s,d+1,n+1)" % (
                n + 1, f(P["p_fork"]), guard, I, sp, f(r1), f(b1), st, f(k1), collar, f(r2), f(b2), f(k2)),
            "e%d: %s : A(v,s,d,n) : %s -> %s/(s*%s)&(%s)A(v*%s,-s,d+0.5,n+1)" % (
                n + 1, f(P["p_elb"]), guard, I, f(P["elb_roll"]), f(P["elb"]), f(P["k_elb"])),
        ]
    lines += [
        "st: %s : A(v,s,d,n) : d < G -> A(v,s,d,n+1)" % f(P["p_stall"]),
        # every apex and spur carries the plant's age n, so the cards can
        # grow with the tree (a sapling's are smaller)
        "ag: A(v,s,d,n) : d >= G -> A(v,s,d,n+1)",
        "kg: K(v,n) -> K(v,n+1)",
        "g1: F(x,r,g,m) -> F(r*m*min(lm,l0+dl*(g+1)),r,g+1,m)",
        "g2: f(x,r,g,m) -> f(r*m*min(lm,l0+dl*(g+1)),r,g+1,m)",
        "g3: !(w,t) : t > 0 -> !(w*vb,t)",
        "g4: !(w,t) : t < 0 -> !(w*vt,t)",
    ]
    cs = P["cs"]
    if P["wood_only"]:
        fin = ["A(v,s,d,n) : * -> ", "K(v,n) : * -> "]
    else:
        tip = "A(v,s,d,n) : * -> "
        m = mass(cs, P["back"], int(P["clump_n"]))
        lp = P["leaf_p"]
        fin = []
        if lp > 0:
            # a leafy tip: its first cards (out ahead of the tip) are the
            # breaking leaves, the blossom cards take the places after them
            ln = int(P["leaf_n"])
            lv = mass(P["leaf_cs"], P["back"], ln, slot=2)
            mb = mass(cs, P["back"], int(P["clump_n"]), skip=ln)
            fin.append("%s : %s%s" % (f(1 - lp), tip, m))
            fin.append("%s : %s%s%s" % (f(lp), tip, lv, mb))
        else:
            fin.append(tip + m)
        fin.append("K(v,n) : * -> " + mass(cs * P["spray"], P["back"] * 0.6, int(P["spray_n"]), P["spray_fwd"]))
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
    out = args[1] if len(args) > 1 else "cherry.json"
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
