#!/usr/bin/env python3
"""Builder for Ashmere's village oak (Quercus robur, an open-grown English
hedgerow and field oak, late September).

usage: oak.py [VARIANT [OUT.json [key=value ...]]]
       oak.py            -> the shipped tree: variant a, BASE, oak.json in
                            the current directory
       oak.py y young_oak.json   -> the young-oak preset

Writes one network.symbios.gen.lsystem generator in wire form (every
decimal a whole number of ten-thousandths). Grammar units are metres
BEFORE the root transform scale (P["scale"], 1.08 here).

The shipped tree (iterations 11, seed 1) is 21.4 m wide and 16.7 m tall
above the ground, its first limbs at 3.1 m and its bole running to 4.75 m,
7,674 triangles in 3 parts at mesh_resolution 5. Iterations are age: 5 is
9.8 m tall and 12.2 m wide (3,614 triangles), 6 is 11.1 m (5,112), 7 is
12.3 m (6,326), 8 is 13.5 m (7,082), 12 (the sanitiser's cap) a 17.3 m
veteran (7,744). For a young oak use preset y (iterations 5 with 1.6 m
tip cards: 9.2 m tall, 11.2 m wide, 3,614 triangles): at 2.1 m the cards
make a 9 m tree's leaves look far too big at 15 m. Measure with `render
--generator oak.json` (the `subject size` line; its height includes the
0.3 m of trunk sunk below the origin, `sink`).

Variants (judged on play views, age sheets and the world in Ashmere's
evening light, at 5, 15, 50 and 150 m):
  a  SHIPPED. Dichotomous sympodial crown on a short bole: five scaffold
     limbs leave the bole at 3.1-4.3 m (after the 1.08 scale) at 64-76
     deg from vertical and every apex forks into two unequal shoots, one
     on the upper flank and one on the lower (the pair does not climb, so
     the crown's skirt comes down), sometimes bends at an elbow without
     forking, sometimes stalls a year. At `gens` forks it stops. The last
     fork is drawn by the cards alone (`ghost`): twice the tips for no
     tube. Each tip is an 11-card mass, every fork carries an 8-card side
     spray (so the limbs are leafy along their outer half, not only at
     their ends), and two epicormic tufts sit on the bole. About a sixth of
     the tip masses are bronzing (slot 2, weight 0.18): late September.
  y  PRESET, young oak: a with iterations=5 cs=1.6.
  b  REJECTED (a with gens=5 ghost=0 dspray=9 clump_n=16 fork_roll=45,115
     burrs= bronze=0: five forks, 16-card masses, no sprays; 5,512
     triangles, 2 parts): the tips sit on a thin shell high on long bare
     limbs - at 50 m an acacia's or a stone pine's umbrella, and
     see-through at 150 m. Adding the sixth fork as cards (b gens=6
     ghost=1, 7,960 triangles) closed the crown; spending the cards on
     sprays instead of bigger masses (a: 11-card masses and sprays, 7,674;
     a with clump_n=10, 7,304) moved foliage down the limbs for the same
     cost.
  d  REJECTED, the first materials (a's geometry with the designer's Bark
     texture and leaf colours; the critic's "before"): the crown averaged
     hue 39-40 in the world's light, khaki against grass at 57-59, and the
     bark read as smooth pale planed timber at 5-15 m.
  Earlier grammars, not kept (the designer's; not re-measured):
  - Vigour-thresholded sympodial (an apex forks, or makes a lateral and
    kinks away, or runs on; it stops below a vigour): a fishbone of bare
    poles with a tuft on each lateral, the low limbs drooping into the
    ground.
  - The same with `$` before every split and the shoots yawed (a
    horizontal fan): flat layered plates - a bonsai or an umbrella pine.
  - Laterals rolled onto the upper side and flanks (epitony): 3-D, but a
    spindly shrub; the vigour threshold kills a lateral in one step.
  - This grammar with both fork shoots on the upper flanks (fork_roll
    72,72): every fork climbs, and the crown is a vase of long straight
    poles under a flat-topped canopy.
Rejected settings, and why:
  - Cards 1.25-1.55 m (leaves 0.4-0.5 m): crowns read as sparse sticks
    with tufts. 1.8 m cards (cs=1.8) left the crown visibly thinner at 50
    m and more see-through at 150 m, so the mature tree keeps 2.1 m cards
    with leaves at 0.27 of the card - about 0.6 m, several times life
    size, the price of a crown that does not mip away at distance (the
    birch found the same). The young preset takes 1.6 m cards.
  - Scaffold limbs at 78-86 deg (near level): with the tropism they sag to
    the ground and the tree reads as a haystack.
  - A bole of 2.2 m to the first limbs (2.4 m placed): short of a field
    oak's 3-5 m; now 2.9 m in the grammar, 3.1 m placed.
  - Bark from the Bark texture (preset d, and brighter or darker Bark
    colours at scale 2-5): its colour follows its FBM only, the furrows are
    normal-map relief, and with the sun behind the viewer the relief does
    not show - smooth tan timber with a fine planed grain. A CrackedEarth
    texture stretched up the tubes (bark_kind=crack) drew dark fissures
    into the colour but read as snakeskin or crazy paving. Shipped: a Rock
    texture with its colours swapped (bark_kind=rock), so the ridged
    multifractal's sharp ridges are near-black fissures across a grey
    face, stretched 5x up the tubes by `;(0.2)`: rugged, fissured, grey-
    brown in the evening light.
  - Leaf colours near equal red and green (preset d: edge 0.22,0.25,0.09)
    and bronze on every leaf edge: the crown went khaki or brown. Every
    Leaf texture adds its vein brightness to the colour (see the engine
    facts), which on a dark leaf is most of the colour, so the leaf's own
    colour is set with almost no red (edge 0.05,0.28,0.06): the crown now
    measures hue 56-58 at value 0.24-0.32, as green as the sunlit grass and
    darker. Bluer or darker greens (0,0.16,0.08) barely moved the hue: at
    that darkness the haze and the warm light decide it.

Second round, after an independent critique (the designer's shipped tree
is preset d's materials on the old limb joints):
  - Limb bases and thick forks were flat-ended stubs: a shoot's tube
    started at its parent's width (a limb as wide as the trunk, its
    pentagonal end standing out of the bark; a fork's first shoot a sleeve
    over its parent). Every shoot now starts its own tube at its own
    width (`start`), inside its parent, and every limb is launched from
    the foot, so the trunk is one tube with no seam round it at the limb
    heights.
  - Khaki crown, pale smooth bark, short bole, fern-frond veins seen from
    below (vein_count 5, midrib 0.08, leaf normal 0.4 -> 9, 0.05, 0.3) and
    dark edge-on card stems (stem half-width 0.012 -> 0.008, paler):
    see the settings above.
  - 11 leaves a Twig card: the sanitiser clamps `leaf_pairs` to 8, so the
    world always drew 8; the file now says 8.
  - A pinched foot, found on the way: the `f(-sink)` that starts the trunk
    tube came before the first `!`, so the tube's buried first ring had
    the default 0.1 m width and the foot was a cone up to the first node
    (plain to see on the apple's). The trunk width is now set first.
  - Left as it is: a tube restarted after a `]` turns its pentagon to the
    turtle's roll while its parent's rings follow the tube, so where the
    two meet at a fork a thin sliver of sky can show between them - a few
    centimetres, found in a 4 m studio zoom of the apple, in no world
    frame at 5 m. The old sleeves hid it. On the apple a collar at every
    fork (collar_d=5) did not close it (on the oak collar_d=4 costs 710
    triangles); shoot_start=1.4 hides it by bringing the sleeve back. A
    fork shoot starts at 0.93 of its width on the split (thick)
    internodes, so its ring stays inside the parent's last, narrower ring.

Materials (three parts per copy): slot 0 bark - the swapped Rock texture,
`;(0.2)` so the fissures run up the tubes; slot 1 foliage - a Twig card
of 8 alternate, lobed, untoothed oak leaves; slot 2 the same card in
bronze. `bronze=0` drops slot 2: a two-part tree.

Engine facts found here (see also docs/lsystem-playbook.md):
  - A finalization rule may carry a stochastic weight like a growth rule:
    `0.78 : A(...) : * -> ...` and `0.22 : A(...) : * -> ...` pick per
    module, so one marker can become two materials.
  - A tube ends at every `[` and `]`: the tube drawn after a `]` starts
    square to its own heading while the one before it ended square to
    the old heading, so a bend right after a bracket opens a wedge-shaped
    hole on the outside of the bend (a limb's elbow showed sky at 15 m).
    Within one unbroken run of F the joints are mitred and close. So an
    elbow here carries no bracket, sprays ride on forks (whose second
    shoot starts a new tube anyway), and on the thick limbs that second
    shoot first draws a 1 cm collar along the parent's heading. Pushing a
    fork's shoots from the internode's base instead (walking up it with
    `f`) only moved the hole to the bend before it.
  - A `[` starts the branch's tube at the parent's width, whatever `!`
    follows it; the first ring is squared to the branch's first segment.
    `!(w)f(0)` inside the bracket starts a fresh tube at width w (the `[`
    point becomes a strand of one point, which draws nothing). The `!`
    here carries the growth parameter and is aged one step ahead of the
    apex's own `!`, so the two stay equal as the tree grows.
  - An `f` carrying the same growth parameters as the trunk's F segments
    lands where they do at every age, so a limb pushed at the foot can
    walk up to its height and leave the trunk unbroken. (The tropism turns
    the heading after an F but not after an f: on a straight trunk the
    paths are identical, on apple.py's leaning one they part by
    millimetres.)
  - The sanitiser pulls every texture field into its registry envelope
    (symbios-texture's registry.rs): Twig `leaf_pairs` 1-8, Bark
    `normal_strength` 0-8 and `furrow_shape` 0.1-2. A world or room
    record is drawn clamped, and since #1486 so is `render --generator`:
    it draws the file as a room record keeps it (sanitised, the clamps
    applied) and names each place not drawn as written above the size
    line - read those lines, the sheet shows the clamped value.
  - The Leaf texture adds vein brightness to its colour in linear RGB:
    up to (0.18, 0.135, 0.045) on the midrib and veins, about (0.027,
    0.020, 0.007) averaged over the blade whatever `vein_count` is. On a
    dark leaf that is most of the colour, and it is yellow.
  - The Bark texture's colour comes only from its FBM; its furrows exist
    only in the normal map.
  - The Leaf texture's colour runs from the midrib colour to the edge
    colour, so a deeply lobed leaf is mostly edge colour.
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
    scale=1.08,
    iterations=11,        # = age; 5 is a 9.8 m young oak (preset y), 12 the cap
    mesh_resolution=5,
    elasticity=0.035,     # droop per tube segment; the long low limbs sag
    seed=1,
    # --- growth law of every internode (trunk included): length at age g is
    #     r * m * min(lm, l0 + dl * g) - slow, so the age lever reaches
    #     full size near 11, not 5
    l0=0.9, dl=0.25, lm=3.4,
    # --- trunk
    wt=0.30,              # newest trunk width; x vt per step (1.1 m at 11)
    vt=1.12,
    # bole: (height at maturity, width factor): a flared foot, limbs from
    # 2.9 m, the bole running on to 4.4 m (x 1.08 placed)
    sink=0.3,             # trunk base below the origin: the flared foot
                          # (0.9 m out) stays buried on a 19 deg slope
    bole=[(0.2, 1.6), (0.6, 1.2), (2.9, 1.0), (3.5, 0.96), (4.0, 0.92), (4.4, 0.88)],
    # scaffold limbs: (after bole segment, azimuth, pitch from vertical,
    # vigour, generations short of gens) - low and wide, uneven round the
    # trunk so the crown is lopsided
    limbs=[(2, 20, 74, 1.0, 0), (2, 160, 68, 0.95, 0), (3, 250, 76, 1.0, 0),
           (4, 330, 64, 0.9, 1), (4, 95, 72, 0.9, 1)],
    limb_flare=1.1,       # a limb's first ring over its first internode's width
    leader=(6, 0.85, 1),  # (lean, vigour, generations short): weak, so the top rounds
    burrs=[(70, 0.55), (215, 0.6)],   # epicormic tufts on the bole: (azimuth, size)
    # --- branching
    gens=6,               # forks to a tip
    ghost=1,              # the last `ghost` generations are cards, not tubes
    bw=0.2,               # branch width at birth (x vigour), x vb per step
    vb=1.13,
    kf=(0.8, 0.76),       # vigour of the fork's two shoots
    fork_roll=(45.0, 100.0),  # roll of each shoot off straight up: one upper, one lower flank
    fork_b=(34.0, 34.0),  # their angle off the parent
    k_elb=0.88, elb_roll=35.0, elb=24.0,   # an elbow: bend without forking
    p_fork=0.6, p_elb=0.24, p_stall=0.16,
    dsplit=3, crook=20.0,  # internodes of generation < dsplit: two segments with a crook
    collar_d=3,           # forks of generation < collar_d draw the 1 cm collar (see variant_a)
    shoot_start=1.0,      # a fork shoot's first ring over its own width (see start)
    # --- foliage
    cs=2.1,               # tip card scale (a Twig card is 0.7 x 1.0 before it); 1.6 for a young oak
    back=0.8,             # swept-back cards sit this far behind the tip
    clump_n=11,           # cards a tip mass
    dspray=0,             # internodes of generation >= dspray carry a side spray
    spray=0.85, spray_n=8,  # spray card scale (x cs) and cards
    spray_roll=140.0, spray_pitch=55.0, spray_fwd=0.4,  # sprays go out on the lower flank
    spray_at=0.55,        # how far along its internode a spray leaves
    bronze=0.18,          # share of tip masses in the bronze slot (0 = two parts)
    leaf_base=(0.03, 0.24, 0.06),   # almost no red: the veins add a yellow cast
    leaf_edge=(0.05, 0.28, 0.06),
    bronze_base=(0.14, 0.22, 0.04),
    bronze_edge=(0.24, 0.26, 0.04),
    stem=(0.30, 0.28, 0.18), stem_w=0.008,
    leaf_pairs=8,         # the sanitiser's cap: 11 was drawn as 8
    leaf_scale=0.27, leaf_angle=2.0,
    lobes=4.0, lobe_depth=0.34, lobe_sharp=0.7,
    veins=9.0, midrib=0.05, leaf_normal=0.3,   # fine veins: 5 read as fern fronds from below
    # --- bark
    bark_kind="rock",     # "rock" shipped; "bark" and "crack" rejected (see the docstring)
    bark_light=(0.29, 0.29, 0.28), bark_dark=(0.03, 0.03, 0.03),   # face, fissures (sRGB)
    rock_scale=5.0, rock_oct=6, rock_att=3.0, rock_normal=2.0,
    # the Bark texture (bark_kind "bark", preset d)
    bark_scale=2.0, bark_normal=8.0, bark_warp_u=0.12, bark_warp_v=0.6,
    furrow=1.0, furrow_u=2.5, furrow_v=0.25, furrow_shape=0.6,
    # the CrackedEarth texture (bark_kind "crack")
    crack_plates=10.0, crack_w=0.02, crack_curl=0.1, crack_var=0.2, crack_grain=0.3, crack_normal=4.0,
    bark_uv=2.0,          # whole repeats round a tube (a fraction leaves a seam)
    bark_vs=0.2,          # `;` V-scale: < 1 stretches the pattern up the tubes
    wood_only=0,
)

# Presets: the young oak, and the rejected variants from the same grammar
VARIANTS = {
    "a": {},
    "b": dict(gens=5, ghost=0, dspray=9, clump_n=16, fork_roll=(45.0, 115.0), bronze=0,
              burrs=[]),
    "y": dict(iterations=5, cs=1.6),
    # the designer's materials as the world drew them (Bark normal 10 and
    # furrow_shape 2.5 clamped to 8 and 2)
    "d": dict(bark_kind="bark", bark_light=(0.45, 0.44, 0.45), bark_dark=(0.0, 0.0, 0.0),
              furrow=0.9, furrow_u=5.0, furrow_v=0.35, furrow_shape=2.0, bark_vs=0.4,
              leaf_base=(0.08, 0.20, 0.09), leaf_edge=(0.22, 0.25, 0.09), bronze=0.22,
              bronze_base=(0.22, 0.23, 0.09), bronze_edge=(0.34, 0.28, 0.10),
              stem=(0.30, 0.24, 0.16), stem_w=0.012, veins=5.0, midrib=0.08, leaf_normal=0.4),
}


def leaf_texture(P, base, edge):
    return {
        "$type": "Twig",
        "leaf": {
            "color_base": wl(base),
            "color_edge": wl(edge),
            "serration_strength": 0,
            "vein_angle": w(2.0),
            "micro_detail": w(0.2),
            "normal_strength": w(P["leaf_normal"]),
            "lobe_count": w(P["lobes"]),
            "lobe_depth": w(P["lobe_depth"]),
            "lobe_sharpness": w(P["lobe_sharp"]),
            "petiole_length": w(0.04),
            "petiole_width": w(0.02),
            "midrib_width": w(P["midrib"]),
            "vein_count": w(P["veins"]),
            "venule_strength": w(0.1),
        },
        "stem_color": wl(P["stem"]),
        "stem_half_width": w(P["stem_w"]),
        "leaf_pairs": int(P["leaf_pairs"]),
        "leaf_angle": w(P["leaf_angle"]),
        "leaf_scale": w(P["leaf_scale"]),
        "stem_curve": w(0.01),
        "sympodial": True,
    }


def bark_material(P):
    if P["bark_kind"] == "crack":
        # dried-mud plates stretched up the tubes: dark fissures between
        # long ridges, in the albedo as well as the normal map
        tex = {
            "$type": "CrackedEarth",
            "seed": 5,
            "scale": w(P["crack_plates"]),
            "jitter": w(0.9),
            "crack_width": w(P["crack_w"]),
            "crack_depth": w(1.2),
            "curl": w(P["crack_curl"]),
            "curl_reach": w(0.03),
            "plate_variance": w(P["crack_var"]),
            "grain_scale": w(30.0),
            "grain_strength": w(P["crack_grain"]),
            "color_plate": wl(P["bark_light"]),
            "color_crack": wl(P["bark_dark"]),
            "normal_strength": w(P["crack_normal"]),
        }
    elif P["bark_kind"] == "rock":
        # ridged multifractal: its sharp ridges are drawn in the "gap"
        # colour, so a near-black there makes a network of thin dark
        # fissures across a grey face; stretched up the tubes by `;`
        tex = {
            "$type": "Rock",
            "seed": 7,
            "scale": w(P["rock_scale"]),
            "octaves": int(P["rock_oct"]),
            "attenuation": w(P["rock_att"]),
            "color_light": wl(P["bark_dark"]),
            "color_dark": wl(P["bark_light"]),
            "normal_strength": w(P["rock_normal"]),
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
            "furrow_multiplier": w(P["furrow"]),
            "furrow_scale_u": w(P["furrow_u"]),
            "furrow_scale_v": w(P["furrow_v"]),
            "furrow_shape": w(P["furrow_shape"]),
        }
    return {"base_color": [10000, 10000, 10000], "roughness": 9500, "texture": tex,
            "uv_scale": w(P["bark_uv"])}


def generator(P, src, fin):
    mats = {"0": bark_material(P)}
    if not P["wood_only"]:
        mats["1"] = {"base_color": [10000, 10000, 10000], "roughness": 10000,
                     "texture": leaf_texture(P, P["leaf_base"], P["leaf_edge"])}
        if P["bronze"] > 0:
            mats["2"] = {"base_color": [10000, 10000, 10000], "roughness": 10000,
                         "texture": leaf_texture(P, P["bronze_base"], P["bronze_edge"])}
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


def card(pre, s, slot=1):
    return "[,(%d)%s~(0,%s)]" % (slot, pre, s)


# A 16-card mass round a tip: (roll, pitch, card roll, scale, swept back).
# After `$` a roll rho then a pitch & sends a card rho degrees round from
# straight up; the card roll after it was chosen (a card simulator over tip
# headings from 30 deg down to 85 deg up) to keep every face at least ~50
# deg off level where it can - a level card flares white toward a low sun.
MASS = [
    (0, 25, -90, 1.0, 0), (180, 30, -90, 0.95, 0), (60, 45, 40, 1.0, 0), (120, 50, -40, 0.95, 0),
    (240, 50, 40, 0.95, 0), (300, 45, -40, 1.0, 0), (0, 75, -90, 0.9, 0), (180, 80, -90, 0.85, 0),
    (45, 95, -90, 0.9, 0), (315, 95, -90, 0.9, 0),
    (30, 110, -80, 0.9, 1), (150, 120, 75, 0.85, 1), (210, 115, -80, 0.85, 1), (330, 110, 80, 0.9, 1),
    (90, 135, -20, 0.8, 1), (270, 135, 20, 0.8, 1),
]
# The order the first n are taken in: six round the tip, two out to the
# sides, two swept back, then the rest.
MASS_ORDER = [0, 1, 2, 3, 4, 5, 8, 9, 10, 12, 6, 7, 11, 13, 14, 15]


def mass(cs, back, n, fwd=0.0, slot=1):
    out = "$" + ("f(%s)" % f(fwd) if fwd else "")
    for i in MASS_ORDER[:n]:
        rho, beta, sig, k, b = MASS[i]
        pre = ("f(-%s)" % f(back)) if b else ""
        out += card(pre + "/(%s)&(%s)/(%s)" % (f(rho), f(beta), f(sig)), f(cs * k), slot)
    return out


def start(width):
    """Begin a fresh tube here at `width` (the width its first internode is
    drawn at). This `!` is written a step before the apex writes its own
    and grows by vb every step, so it is set at width / vb and the two stay
    equal; the zero move starts the tube, so the `[` point (at the
    parent's width) is a strand of one point and draws nothing."""
    return "!(%s/vb,1)f(0)" % width


def variant_a(P):
    """The shipped grammar (see the module docstring). A(v,s,d): an apex of
    vigour v (its internode's length and width factor), side s (+-1, flips
    at every split so the axis zigzags away from each shoot) and generation
    d (forks so far; an elbow adds half). F/f carry (length, fraction, age,
    vigour) and re-lengthen every step; `!(w,t)` thickens every step, trunk
    (t < 0) and branch (t > 0) at their own rates."""
    G = int(P["gens"])
    lines = [
        "#define l0 %s" % f(P["l0"]), "#define dl %s" % f(P["dl"]), "#define lm %s" % f(P["lm"]),
        "#define bw %s" % f(P["bw"]), "#define vb %s" % f(P["vb"]), "#define vt %s" % f(P["vt"]),
        "#define wt %s" % f(P["wt"]), "#define G %d" % G,
    ]
    lm = P["lm"]
    # The trunk is ONE tube: a bracket ends the tube it interrupts (the
    # next starts afresh, its bark pattern restarting - a seam round the
    # trunk), so no limb is pushed from part-way up it. Every limb and burr
    # is launched from the foot instead and moves up to its height with an
    # `f` that grows exactly as the bole's F segments do. The trunk starts
    # `sink` below the placement's origin: a copy snaps to the ground at
    # its origin only, so on a slope the downhill side of the foot would
    # otherwise float.
    om = ";(%s)!(wt*%s,-1)f(-%s)" % (f(P["bark_vs"]), f(P["bole"][0][1]), f(P["sink"]))
    y, rs, trunk = -P["sink"], 0.0, ""
    for i, (h, fl) in enumerate(P["bole"]):
        r = (h - y) / lm
        y, rs = h, rs + r
        trunk += "!(wt*%s,-1)F(%s*l0,%s,0,1)" % (f(fl), f(r), f(r))
        up = "f(%s*l0,%s,0,1)" % (f(rs), f(rs))
        if i == 2:
            for az, sz in P["burrs"]:
                om += "[%s/(%s)&(80)J(%s)]" % (up, f(az), f(sz))
        for k, (after, az, pitch, v, g) in enumerate(P["limbs"]):
            if after == i:
                om += "[%s/(%s)&(%s)%sA(%s,%d,%d)]" % (
                    up, f(az), f(pitch), start("bw*%s*%s" % (f(v), f(P["limb_flare"]))), f(v),
                    1 if k % 2 == 0 else -1, int(g))
    om += trunk + "&(%s)A(%s,1,%d)" % (f(P["leader"][0]), f(P["leader"][1]), int(P["leader"][2]))
    lines.append("omega: " + om)

    crook = "$/(-s*70)&(%s)" % f(P["crook"])

    def internode(split, ghost):
        if ghost:
            return "f(l0*v,1,0,v)$"
        if split:
            return "!(bw*v,1)F(l0*v*0.5,0.5,0,v)%s!(bw*v*0.93,1)F(l0*v*0.5,0.5,0,v)$" % crook
        return "!(bw*v,1)F(l0*v,1,0,v)$"

    # Tubes break at every bracket: the tube after a `]` starts square to
    # its own heading while the one before it ended square to the old one,
    # so a bend right after a bracket opens a wedge-shaped hole on its
    # outside (seen at 15 m in a limb's elbow; pushing a fork's shoots from
    # the internode's base instead moved the hole to the bend before it).
    # So an elbow carries no bracket; the spray rides on the fork, whose
    # continuation is cut anyway; and on the thick limbs (d < dsplit) the
    # fork's continuation first draws a 1 cm collar along the parent's
    # heading, so the cut falls on a straight run and the bend is a mitred
    # joint. The first shoot starts its own tube at its own width (`start`):
    # from a bare `[` it would start at the parent's width, a fat sleeve
    # standing out of the parent (the critic's "pipe fitting").
    spray = "[f(-l0*v*%s,-%s,0,v)/(s*%s)&(%s)K(v)]" % (
        f(1 - P["spray_at"]), f(1 - P["spray_at"]), f(P["spray_roll"]), f(P["spray_pitch"]))
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
        fork = "%s%s[/(s*%s)&(%s)%sA(v*%s,s,d+1)]%s" % (I, sp, f(r1), f(b1), st, f(k1), collar)
        lines += [
            "f%d: %s : A(v,s,d) : %s -> %s/(-s*%s)&(%s)A(v*%s,-s,d+1)" % (
                n + 1, f(P["p_fork"]), guard, fork, f(r2), f(b2), f(k2)),
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
    n = int(P["clump_n"])
    if P["wood_only"]:
        fin = ["A(v,s,d) : * -> ", "K(v) : * -> ", "J(c) : * -> "]
    else:
        tip = "A(v,s,d) : * -> "
        fin = []
        if P["bronze"] > 0:
            fin.append("%s : %s" % (f(1 - P["bronze"]), tip) + mass(cs, P["back"], n))
            fin.append("%s : %s" % (f(P["bronze"]), tip) + mass(cs, P["back"], n, slot=2))
        else:
            fin.append(tip + mass(cs, P["back"], n))
        fin.append("K(v) : * -> " + mass(cs * P["spray"], P["back"] * 0.6, int(P["spray_n"]), P["spray_fwd"]))
        # an epicormic tuft: a small mass pushed out through the bark
        fin.append("J(c) : * -> " + mass(cs * 0.6, 0.3, 7, 0.55).replace("~(0,", "~(0,c*"))
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
    out = args[1] if len(args) > 1 else "oak.json"
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
