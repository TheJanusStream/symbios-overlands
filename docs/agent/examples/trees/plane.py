#!/usr/bin/env python3
"""Builder for the London plane (Platanus x hispanica, a street and park
plane in high summer), the tree for the catalogue entry
`lsys_sympodial_tree` (owner's choice, 2026-10-23).

usage: plane.py [VARIANT [OUT.json [key=value ...]]]
       plane.py               -> the shipped tree: variant a, BASE, plane.json
                                 in the current directory
       plane.py autumn x.json -> the autumn colours; blossom_pale the spring
                                 flush (both are tints of the same tree)
       plane.py a x.json card_test=1   -> one big leaf card beside a bark
                                 tube: a texture preview

Writes one network.symbios.gen.lsystem generator in wire form (every
decimal a whole number of ten-thousandths). Grammar units are metres
BEFORE the root transform scale (P["scale"], 1.02 here).

The shipped tree (iterations 10, seed 6) stands 14.7 m above the ground
and spreads 16.7 x 14.9 m, 7,648 triangles in 2 parts at mesh_resolution
5. Its bole is clean to the first limbs at 3.8 m (placed), the lowest
foliage about 4.5 m up, the trunk sunk 0.3 m below the origin (`sink`) so
the foot stays buried on a slope. Iterations are age: 5 is a 9.0 m
sapling-crowned young tree, 10.0 x 9.0 m (3,366 triangles); 6 is 10.2 m
(4,632); 7 is 11.5 m (5,910); 8 is 12.6 m (6,720); 9 is 13.8 m, 15.5 m
across (7,314); 11 is 15.8 m, 18.0 x 16.1 m (7,808), so a seeded stand's
+1 stays inside the 8,000 budget. Read the age sweep as growth: the bole
lengthens and thickens, the limbs lengthen and spread, the dome widens
from a round head into a broad flat-bottomed dome. Measure with `render
--generator plane.json` (the `subject size` line; its height includes the
0.3 m of sunk trunk).

Variants (judged on 448 px four-angle sheets, 1,400 px sheets cropped to
the trunk, age sweeps, five seeds and a play view beside the oak at 45 m):
  a  SHIPPED. The oak's grammar (oak.py) reshaped into a plane: one law
     for every internode, `r*m*min(lm, l0+dl*age)`; five heavy scaffold
     limbs launched from the foot and walked up the bole with `f` (so the
     trunk is one tube), leaving it at 3.8-4.9 m and ASCENDING at 34-44
     deg from vertical, round the trunk at 20/95/165/250/325 deg so the
     dome is even; a weaker leader (vigour 0.85) so the top rounds instead
     of standing up as a knob; every apex forks into two shoots rolled
     65 and 120 deg off straight up (both on the flanks, so a rising limb
     spreads sideways rather than climbing or drooping), or bends at an
     elbow, or stalls a year. Branch internodes run at 0.82 of the trunk's
     law (`lb`), so the crown is 15 m, not 18, while the bole keeps its
     length. Tip masses of 9 cards, 7-card side sprays at every fork, the
     last fork drawn by the cards alone (`ghost`).
  b  REJECTED, the first draft (four limbs at 34-44 deg, a full-vigour
     leader, branches at the trunk's law, 11-card masses and 8-card
     sprays, small-patch bark, narrow spiky leaves; seed 1): 18.0 m tall
     and 19.3 m across, 7,866 triangles; the leader stood up through the
     dome as a separate lobe, the limbs were thin poles beside the bole,
     and the crown was a cauliflower of four lumps.
  c  REJECTED bark (a with patch_scale=2 patch_oct=4 bark_uv=2, judged
     with b's paler colours): fine speckle, a pale tan trunk at 40 m with
     no camouflage to read.
  Seeds tried at a's settings (iterations 10): 1 lopsided, its crown
  overhanging one side (15.5 m, 7,742 triangles - the second-seed sheet);
  2 a squarer, denser crown at 9,206 triangles (over budget); 3 and 4 even
  but tall-headed (7,966 and 7,986); 5 8,468; 7 8,366. Seed 6 has the
  broadest, flattest-bottomed dome and is the cheapest.
Rejected settings, and why:
  - lobe_depth 0.6 with lobe_sharpness 0.6 (b): spiky, cross-shaped
    leaves. On card tests lobe_count 2 read as an oak's pinnate lobes and
    1.25 as little different from 1.5 at depth 0.5. Shipped: lobe_count 1.5, depth 0.4, sharpness 1.0 (see the
    engine facts) - a broad leaf with three to five points, five a card at
    0.44 of the card, much bigger than the oak's eight at 0.27.
  - Four limbs (b, and b's limbs at a's lengths): the crown fell into
    four lumps with a gap between two of them; five round it out. A sixth
    would cost tips the budget does not have.
  - Branch width bw 0.2 (b): thin poles; 0.28 gives heavy limbs about
    half the bole's width where they leave it. A trunk at wt 0.30 read
    slender under a 16 m dome; 0.33 (width 1.03 at age 10, x vt per
    year) reads stout.
  - A foot flare of 1.5 over 0.2 m then 1.15 (b): a square plinth with a
    ledge. Now 1.35, 1.15, 1.04 over 0.15, 0.5 and 1.1 m: a smooth swell.
  - Shipping at seed 1 (the first draft's): lopsided.
  - Seed balls (the hanging "bobbles"): real ones are 2.5-3.5 cm, a dot
    at 12 m even drawn several times life size, and a fruit slot is a
    third part; left out.

Materials (two parts per copy): slot 0 bark - a LICHEN texture used as
camouflage bark: the "rock" is the old olive-grey bark, the two colony
tints the cream and pale khaki-green where it has flaked, the margin a
khaki rim, coverage 0.42, broad ragged colonies (patch_scale 1.3, 3
octaves), one tile round a tube (`uv_scale` 1) and `;(0.8)` so the patches
run a little up the trunk; slot 1 foliage - a Twig card of five big,
alternate, lobed and lightly toothed leaves, mid-dark green. The variants
re-tint the textures themselves (tint_twig on 1, tint_lichen on 0).

Engine facts found here (see also docs/lsystem-playbook.md and oak.py's):
  - The tube mesher maps V by arc length over the tube's circumference
    (bevy_symbios mesher.rs), so one texture tile is as tall as the tube
    is round: a bark pattern scales with girth, its patches six times
    larger on a 1 m bole than on a 0.15 m limb. `uv_scale` and `;`
    multiply that. For a plane this is the right way round.
  - The Leaf texture's lobes are a cosine along the midrib,
    `cos(v * (2 * lobe_count + 1) * pi)`, not a palmate fan: lobe_count
    1.5 puts a pair of broad side lobes at mid-blade, a pair of small
    basal ones and a narrow tip, which on a Twig card reads as a maple or
    plane leaf. The palmate Broadleaf texture is one leaf a card, far too
    few leaves for a crown on the budget, and tint_twig/tint_leaf leave
    its colours alone.
  - Triangle cost follows the seed: the fork/elbow/stall choices change
    the number of tips, and with them the cards, by +-20% at one setting
    (7,648 to 9,206 over seeds 1-7). Check the budget at the shipped seed
    and at iterations + 1.
  - The play view stands its camera 12 m off, inside a 15 m tree's crown;
    `--dist 45` frames a tree beside the oak.
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
    scale=1.02,
    iterations=10,        # = age; 5 a young 9 m tree, 11 the oldest a seeded stand draws
    mesh_resolution=5,
    elasticity=0.03,      # droop per tube segment: the ascending limbs arc out a little
    seed=6,               # the broadest, cheapest dome of seeds 1-7
    # --- growth law of every internode (trunk included): length at age g is
    #     r * m * min(lm, l0 + dl * g)
    l0=0.9, dl=0.25, lm=3.4,
    # --- trunk
    wt=0.33, vt=1.12,     # newest trunk width, x vt per step: a stout bole
    sink=0.3,             # trunk base below the origin, buried on a slope
    # bole: (height at maturity, width factor): a swelling foot, clean to
    # the first limbs at 3.7 m (3.8 m placed), the leader running on
    bole=[(0.15, 1.35), (0.5, 1.15), (1.1, 1.04), (3.7, 1.0), (4.2, 0.96), (4.8, 0.92), (5.5, 0.88)],
    # scaffold limbs: (after bole segment, azimuth, pitch from vertical,
    # vigour, generations short of gens)
    limbs=[(3, 20, 40, 1.0, 0), (3, 165, 38, 0.95, 0), (4, 95, 44, 0.95, 0),
           (4, 250, 42, 1.0, 0), (5, 325, 34, 0.9, 1)],
    limb_flare=1.1,       # a limb's first ring over its first internode's width
    leader=(3, 0.85, 0),  # (lean, vigour, generations short): weaker than a limb so the top rounds
    burrs=[],             # epicormic tufts (azimuth, size): a plane's bole is clean
    # --- branching
    gens=6,               # forks to a tip
    ghost=1,              # the last fork is cards only
    bw=0.28, vb=1.13,     # branch width at birth (x vigour), x vb per step: heavy limbs
    lb=0.82,              # branch internode length over the trunk's law
    kf=(0.82, 0.78),      # vigour of a fork's two shoots
    fork_roll=(65.0, 120.0),  # roll off straight up: both flanks, so a limb spreads
    fork_b=(32.0, 32.0),
    k_elb=0.9, elb_roll=35.0, elb=20.0,
    p_fork=0.62, p_elb=0.24, p_stall=0.14,
    dsplit=3, crook=12.0,
    collar_d=3,
    shoot_start=1.0,
    # --- foliage
    cs=2.1,               # tip card scale: smaller cards thin the crown at distance (oak.py)
    back=0.8,
    clump_n=9,            # cards a tip mass
    dspray=0,
    spray=0.85, spray_n=7,  # side spray at each fork: card scale (x cs), cards
    spray_roll=140.0, spray_pitch=55.0, spray_fwd=0.4,
    spray_at=0.55,
    leaf_base=(0.05, 0.26, 0.06),
    leaf_edge=(0.08, 0.30, 0.07),
    stem=(0.36, 0.33, 0.20), stem_w=0.008,
    leaf_pairs=5,         # five big leaves a card (alternate: the Twig is sympodial)
    leaf_scale=0.44, leaf_angle=1.9,
    lobes=1.5, lobe_depth=0.4, lobe_sharp=1.0, serration=0.12,   # 3-5 points: maple-like
    veins=7.0, midrib=0.05, leaf_normal=0.3,
    # --- bark: a Lichen texture as camouflage bark
    bark_rock=(0.42, 0.40, 0.31),     # the old grey-olive bark
    bark_patch_a=(0.80, 0.76, 0.57),  # fresh cream where it flaked
    bark_patch_b=(0.58, 0.61, 0.37),  # pale green
    bark_rim=(0.52, 0.48, 0.34),      # khaki at a flake's edge
    # colony cover, broad ragged flakes (lower patch_scale = bigger)
    coverage=0.42, patch_scale=1.3, patch_oct=3, rim=0.12, species_scale=2.0,
    grain=0.12, relief=0.25, bark_normal=1.0,
    bark_uv=1.0,          # one tile round a tube; the tile's height follows girth
    bark_vs=0.8,          # `;` V-scale: < 1 stretches the flakes a little up the tubes
    wood_only=0,
    card_test=0,
)

VARIANTS = {
    "a": {},
    "b": dict(scale=1.0, seed=1, wt=0.30, bw=0.2, lb=1.0, clump_n=11, spray_n=8,
              bole=[(0.2, 1.5), (0.6, 1.15), (3.4, 1.0), (3.9, 0.96), (4.5, 0.92), (5.2, 0.88)],
              limbs=[(2, 20, 40, 1.0, 0), (3, 140, 36, 0.95, 0), (3, 255, 44, 1.0, 0),
                     (4, 330, 34, 0.9, 0)],
              leader=(3, 1.0, 0), leaf_scale=0.40, lobe_depth=0.6, lobe_sharp=0.6,
              bark_rock=(0.47, 0.46, 0.37), bark_patch_a=(0.82, 0.78, 0.60),
              bark_patch_b=(0.64, 0.67, 0.47), bark_rim=(0.58, 0.52, 0.36),
              coverage=0.5, patch_scale=2.5, patch_oct=4, bark_uv=2.0),
    "c": dict(patch_scale=2.0, patch_oct=4, bark_uv=2.0),
    # the two named re-skins, as colour presets (the catalogue applies them
    # with tint_twig on slot 1 and tint_lichen on slot 0)
    "autumn": dict(leaf_base=(0.52, 0.40, 0.15), leaf_edge=(0.45, 0.28, 0.11)),
    "blossom_pale": dict(leaf_base=(0.52, 0.60, 0.26), leaf_edge=(0.60, 0.66, 0.30),
                         bark_rock=(0.55, 0.53, 0.43), bark_patch_a=(0.88, 0.85, 0.68),
                         bark_patch_b=(0.70, 0.72, 0.50), bark_rim=(0.62, 0.58, 0.44)),
}


def leaf_texture(P, base, edge):
    return {
        "$type": "Twig",
        "leaf": {
            "color_base": wl(base),
            "color_edge": wl(edge),
            "serration_strength": w(P["serration"]),
            "vein_angle": w(2.0),
            "micro_detail": w(0.2),
            "normal_strength": w(P["leaf_normal"]),
            "lobe_count": w(P["lobes"]),
            "lobe_depth": w(P["lobe_depth"]),
            "lobe_sharpness": w(P["lobe_sharp"]),
            "petiole_length": w(0.06),
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
    tex = {
        "$type": "Lichen",
        "seed": 11,
        "color_rock": wl(P["bark_rock"]),
        "color_lichen_a": wl(P["bark_patch_a"]),
        "color_lichen_b": wl(P["bark_patch_b"]),
        "color_rim": wl(P["bark_rim"]),
        "coverage": w(P["coverage"]),
        "patch_scale": w(P["patch_scale"]),
        "patch_octaves": int(P["patch_oct"]),
        "rim_width": w(P["rim"]),
        "species_scale": w(P["species_scale"]),
        "grain_scale": w(30.0),
        "grain_strength": w(P["grain"]),
        "relief": w(P["relief"]),
        "normal_strength": w(P["bark_normal"]),
    }
    return {"base_color": [10000, 10000, 10000], "roughness": 9000, "texture": tex,
            "uv_scale": w(P["bark_uv"])}


def generator(P, src, fin):
    mats = {"0": bark_material(P)}
    if not P["wood_only"]:
        mats["1"] = {"base_color": [10000, 10000, 10000], "roughness": 10000,
                     "texture": leaf_texture(P, P["leaf_base"], P["leaf_edge"])}
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


MASS = [
    (0, 25, -90, 1.0, 0), (180, 30, -90, 0.95, 0), (60, 45, 40, 1.0, 0), (120, 50, -40, 0.95, 0),
    (240, 50, 40, 0.95, 0), (300, 45, -40, 1.0, 0), (0, 75, -90, 0.9, 0), (180, 80, -90, 0.85, 0),
    (45, 95, -90, 0.9, 0), (315, 95, -90, 0.9, 0),
    (30, 110, -80, 0.9, 1), (150, 120, 75, 0.85, 1), (210, 115, -80, 0.85, 1), (330, 110, 80, 0.9, 1),
    (90, 135, -20, 0.8, 1), (270, 135, 20, 0.8, 1),
]
MASS_ORDER = [0, 1, 2, 3, 4, 5, 8, 9, 10, 12, 6, 7, 11, 13, 14, 15]


def mass(cs, back, n, fwd=0.0, slot=1):
    out = "$" + ("f(%s)" % f(fwd) if fwd else "")
    for i in MASS_ORDER[:n]:
        rho, beta, sig, k, b = MASS[i]
        pre = ("f(-%s)" % f(back)) if b else ""
        out += card(pre + "/(%s)&(%s)/(%s)" % (f(rho), f(beta), f(sig)), f(cs * k), slot)
    return out


def start(width):
    return "!(%s/vb,1)f(0)" % width


def variant_a(P):
    G = int(P["gens"])
    lines = [
        "#define l0 %s" % f(P["l0"]), "#define dl %s" % f(P["dl"]), "#define lm %s" % f(P["lm"]),
        "#define bw %s" % f(P["bw"]), "#define vb %s" % f(P["vb"]), "#define vt %s" % f(P["vt"]),
        "#define wt %s" % f(P["wt"]), "#define G %d" % G, "#define lb %s" % f(P["lb"]),
    ]
    lm = P["lm"]
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
            return "f(l0*v*lb,1,0,v*lb)$"
        if split:
            return "!(bw*v,1)F(l0*v*lb*0.5,0.5,0,v*lb)%s!(bw*v*0.93,1)F(l0*v*lb*0.5,0.5,0,v*lb)$" % crook
        return "!(bw*v,1)F(l0*v*lb,1,0,v*lb)$"

    spray = "[f(-l0*v*lb*%s,-%s,0,v*lb)/(s*%s)&(%s)K(v)]" % (
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
        fin = ["A(v,s,d) : * -> ", "K(v) : * -> "]
    else:
        fin = ["A(v,s,d) : * -> " + mass(cs, P["back"], n),
               "K(v) : * -> " + mass(cs * P["spray"], P["back"] * 0.6, int(P["spray_n"]), P["spray_fwd"])]
    # The burrs' tufts only where the bole carries burrs: the shipped plane's
    # is clean, and a rule nothing emits is dead weight in the record.
    if P["burrs"]:
        fin.append("J(c) : * -> " + ("" if P["wood_only"]
                   else mass(cs * 0.6, 0.3, 7, 0.55).replace("~(0,", "~(0,c*")))
    return generator(P, "\n".join(lines), "\n".join(fin))


def card_test(P):
    """One big upright card of the leaf slot beside a short tube of bark:
    a texture preview."""
    src = "omega: ;(%s)!(1.2)F(3)[,(1)f(-1.5)&(90)f(1)^(90)~(0,4)]" % f(P["bark_vs"])
    return generator(P, src, "")


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
    out = args[1] if len(args) > 1 else "plane.json"
    P = dict(BASE)
    P.update(VARIANTS[variant])
    for kv in args[2:]:
        parse(P, kv)
    g = card_test(P) if P["card_test"] else variant_a(P)
    with open(out, "w") as fh:
        json.dump(g, fh, indent=1)
    print("wrote", out, "src", len(g["source_code"]), "fin", len(g["finalization_code"]))


if __name__ == "__main__":
    main()
