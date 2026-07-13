//! Styled boat parts — bows, funnels / stacks, masts, and decks. See the
//! [`super`] module docstring for the mood-group / band tagging scheme and the
//! authoring frame (parts author front-`+Z`; the assembler yaws the craft 180°).

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::default_visuals::common::{
    cone, cuboid, cylinder, id_quat, lathe, prim, quat_x, quat_xyzw, quat_z, sphere, torus,
    with_cut, with_shape,
};
use crate::pds::avatar::parts::defaults::common::{darken, shade};
use crate::pds::generator::Generator;
use crate::seeded_defaults::{OrnatenessBand, WearBand};

use super::super::{PartCtx, PartDef, PartSlot};
use super::{
    BOAT, FANCY, GRUBBY, HISTORIC, MARTIAL, NEON, REGAL, STEAM, UNIVERSAL, WORN_PLUS, deck_dims,
    mast_height,
};

fn bow_ram(ctx: &PartCtx) -> Generator {
    // A forward-pointing ram cone. quat_x(+90°) sends the cone apex (local +Y)
    // to +Z — the authored bow direction (the assembler yaws the craft 180° so
    // +Z reads as travel-forward), matching the sibling hull prow in
    // `defaults::boat`. (A −90° here aimed the ram astern, base-first — #779.)
    prim(
        cone(
            0.12,
            0.5,
            10,
            ctx.materials.metal(ctx.palette.tertiary_accent),
        ),
        [0.0, 0.0, 0.2],
        quat_xyzw(quat_x(FRAC_PI_2)),
    )
}

fn bow_figurehead(ctx: &PartCtx) -> Generator {
    let mut f = prim(
        sphere(0.11, 3, ctx.materials.trim(ctx.palette.secondary_accent)),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    f.children.push(prim(
        cone(
            0.05,
            0.18,
            8,
            ctx.materials.accent(ctx.palette.primary_accent),
        ),
        [0.0, 0.12, 0.0],
        id_quat(),
    ));
    f
}

fn bow_bowsprit(ctx: &PartCtx) -> Generator {
    // The style-universal prow floor (empty styles): a forward-raked bowsprit
    // spar off the stem with a cap fitting and a bee-block collar, so every boat
    // carries *some* prow accent regardless of theme (the ram is martial, the
    // figurehead ceremonial — this is the plain workaday fitting between them).
    let spar = ctx.materials.metal(ctx.palette.secondary_accent);
    let cap = ctx.materials.trim(ctx.palette.tertiary_accent);
    // Hidden hub at the stem attachment; the visible raked spar hangs off it so
    // its rake never tumbles the sibling fittings (the transform-inheritance
    // gotcha the gondola / env-core roots dodge the same way).
    let mut root = prim(
        cuboid([0.03, 0.03, 0.03], cap.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Raked spar projecting forward (+Z, the authored bow direction), nose up.
    root.children.push(prim(
        cylinder(0.026, 0.42, 8, spar),
        [0.0, 0.06, 0.2],
        quat_xyzw(quat_x(FRAC_PI_2 - 0.12)),
    ));
    // Cap ball at the spar tip.
    root.children.push(prim(
        sphere(0.042, 3, cap.clone()),
        [0.0, 0.085, 0.41],
        id_quat(),
    ));
    // Bee-block collar where the spar meets the stem.
    root.children.push(prim(
        cuboid([0.08, 0.05, 0.06], cap),
        [0.0, 0.0, 0.03],
        id_quat(),
    ));
    root
}

fn smokestack(ctx: &PartCtx) -> Generator {
    // The real steamship funnel (STEAM / industrial): a tapered Lathe body of
    // revolution with a flared cap rim, a bright registry band, and a
    // soot-darkened lip — an actual smokestack, where the old `boat_stack_funnel`
    // was mislabelled glow-thruster pods (now split off as `stack_thrusters`,
    // #792). Keeps the `boat_stack_funnel` slug so a steam boat still fits a
    // funnel.
    let steel = ctx.materials.metal(ctx.palette.secondary_accent);
    let soot = ctx.materials.metal(darken(ctx.palette.secondary_accent));
    let band = ctx.materials.trim(ctx.palette.tertiary_accent);
    // Profile `(radius, height)` bottom→top: a fuller base, a gently tapering
    // barrel, a flared cap rim, then a small inward lip — the classic funnel
    // silhouette. Five stations, well under the sanitiser's 16-point ceiling.
    let profile = [
        (0.10, 0.0),
        (0.086, 0.06),
        (0.08, 0.40),
        (0.10, 0.47),
        (0.086, 0.50),
    ];
    let mut root = prim(lathe(&profile, 20, true, steel), [0.0, 0.0, 0.0], id_quat());
    // Soot-darkened cap lip at the mouth (the barrel's Y axis → default torus).
    root.children.push(prim(
        torus(0.014, 0.094, soot),
        [0.0, 0.485, 0.0],
        id_quat(),
    ));
    // Bright registry band around the barrel.
    root.children
        .push(prim(torus(0.012, 0.084, band), [0.0, 0.30, 0.0], id_quat()));
    root
}

fn stack_thrusters(ctx: &PartCtx) -> Generator {
    // Twin glow-thruster pods at the stern (NEON): a housing with two aft-facing
    // glowing exhaust bells — the hover-craft propulsion read that the old
    // `funnel` build actually drew, now honestly tagged NEON with its own slug
    // (`boat_stack_thrusters`) instead of masquerading as a steam funnel (#792).
    let housing = ctx.materials.metal(darken(ctx.palette.tertiary_accent));
    let glow = ctx.materials.glow(ctx.palette.tertiary_accent);
    let mut root = prim(
        cuboid([0.32, 0.16, 0.2], housing.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    for s in [-1.0f32, 1.0] {
        // Nozzle barrel poking aft (-Z).
        root.children.push(prim(
            cylinder(0.07, 0.14, 12, housing.clone()),
            [s * 0.09, 0.0, -0.13],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
        // Glowing exhaust core at the bell mouth.
        root.children.push(prim(
            cylinder(0.05, 0.04, 12, glow.clone()),
            [s * 0.09, 0.0, -0.19],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
    root
}

fn stack_vent(ctx: &PartCtx) -> Generator {
    // The style-universal stack floor (empty styles): a modest upright deck vent
    // with a conical rain cap and a collar band, so every boat can carry a stack
    // regardless of theme — the funnel is steam, the thrusters neon, this is the
    // plain workaday vent between them.
    let pipe = ctx.materials.metal(darken(ctx.palette.secondary_accent));
    let cowl = ctx.materials.metal(ctx.palette.tertiary_accent);
    // Hidden hub at the deck mount so the pipe / cap / collar all sit in one
    // un-translated frame — a translated pipe-as-root would carry its +0.14
    // offset into every child (the transform-inheritance gotcha), floating the
    // rain cap above the mouth. The hub is buried inside the pipe base.
    let mut root = prim(
        cuboid([0.03, 0.03, 0.03], pipe.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Upright barrel rising from the deck (base at the mount, mouth at y=0.28).
    root.children.push(prim(
        cylinder(0.05, 0.28, 12, pipe.clone()),
        [0.0, 0.14, 0.0],
        id_quat(),
    ));
    // Conical rain cap seated on the mouth (apex up, wider than the pipe so it
    // overhangs; its base overlaps the barrel top, no floating gap).
    root.children.push(prim(
        cone(0.085, 0.07, 12, cowl.clone()),
        [0.0, 0.3, 0.0],
        id_quat(),
    ));
    // Collar band partway up the barrel.
    root.children
        .push(prim(torus(0.012, 0.056, cowl), [0.0, 0.18, 0.0], id_quat()));
    root
}

fn mast_square_rig(ctx: &PartCtx) -> Generator {
    // A square-rigged mast (historic moods): a yard slung athwartships with a
    // broad sail hung below it. The sail fills the crossbar so it reads as a
    // square-rigger, never a bare "crucifix"; set across the boat it shows its
    // full face from ahead, where the fore-and-aft default sail is edge-on.
    let spar = ctx.materials.metal(ctx.palette.secondary_accent);
    let canvas = ctx.materials.cloth(ctx.palette.primary_accent);
    let flag = ctx.materials.accent(ctx.palette.tertiary_accent);
    let h = mast_height(ctx);

    let mut root = prim(
        cylinder(0.017, h, 8, spar.clone()),
        [0.0, h * 0.5, 0.0],
        quat_xyzw(quat_x(-0.03)),
    );
    // Yard: a crossbar laid athwartships (along X) high on the mast.
    let yard = h * 0.66;
    root.children.push(prim(
        cylinder(0.012, yard, 6, spar.clone()),
        [0.0, h * 0.28, 0.0],
        quat_xyzw(quat_z(FRAC_PI_2)),
    ));
    // Square sail hung below the yard (broad in X, thin in Z), leaned forward a
    // touch as if drawing wind. taper narrows the foot slightly.
    let sail_w = yard * 0.9;
    let sail_h = h * 0.52;
    root.children.push(prim(
        with_shape(
            cuboid([sail_w, sail_h, 0.012], canvas),
            [0.08, 0.0],
            [0.0, 0.0, 0.05],
            [0.0, 0.0],
        ),
        [0.0, h * 0.28 - sail_h * 0.5, 0.0],
        id_quat(),
    ));
    // Masthead pennant streaming aft.
    root.children.push(prim(
        cuboid([0.012, 0.05, 0.13], flag),
        [0.0, h * 0.46, -0.06],
        id_quat(),
    ));
    root
}

fn mast_antenna(ctx: &PartCtx) -> Generator {
    // A comms mast for tech moods: a pole bristling with whip antennas, a
    // canted dish, and a beacon — no sail, so it reads as a sensor cluster
    // rather than a crossbar.
    let pole = ctx.materials.metal(ctx.palette.tertiary_accent);
    let whip = ctx.materials.trim(ctx.palette.secondary_accent);
    let dish = ctx.materials.metal(darken(ctx.palette.primary_accent));
    let beacon = ctx.materials.glow(ctx.palette.primary_accent);
    let h = mast_height(ctx);

    let mut root = prim(
        cylinder(0.02, h, 8, pole.clone()),
        [0.0, h * 0.5, 0.0],
        id_quat(),
    );
    // Cross-spar with two boxy sensor pods.
    root.children.push(prim(
        cylinder(0.01, h * 0.4, 6, pole.clone()),
        [0.0, h * 0.16, 0.0],
        quat_xyzw(quat_z(FRAC_PI_2)),
    ));
    for s in [-1.0f32, 1.0] {
        root.children.push(prim(
            cuboid([0.04, 0.05, 0.05], whip.clone()),
            [s * h * 0.19, h * 0.16, 0.0],
            id_quat(),
        ));
    }
    // Whip antennas of staggered height poking above the masthead.
    for (dx, dz, hh) in [(-0.02, 0.02, 0.34), (0.025, -0.015, 0.26), (0.0, 0.03, 0.2)] {
        root.children.push(prim(
            cylinder(0.01, h * hh, 4, whip.clone()),
            [dx, h * 0.86 + h * hh * 0.5, dz],
            id_quat(),
        ));
    }
    // A canted dish part-way up (a shallow bowl via a profile-cut sphere).
    root.children.push(prim(
        with_cut(sphere(0.09, 3, dish), [0.0, 1.0], [0.0, 0.5], 0.0),
        [0.11, h * 0.36, 0.02],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    // Beacon at the very top.
    root.children.push(prim(
        cylinder(0.02, 0.03, 8, beacon),
        [0.0, h * 0.5, 0.0],
        id_quat(),
    ));
    root
}

fn mast_derrick(ctx: &PartCtx) -> Generator {
    // A cargo derrick for industrial moods: a stout king-post with a raking jib
    // boom and a hanging block-and-hook — working gear, not a crucifix.
    let steel = ctx.materials.metal(ctx.palette.secondary_accent);
    let tackle = ctx.materials.metal(darken(ctx.palette.tertiary_accent));
    let hook = ctx.materials.trim(ctx.palette.tertiary_accent);
    let h = mast_height(ctx);

    let mut root = prim(
        cylinder(0.024, h, 8, steel.clone()),
        [0.0, h * 0.5, 0.0],
        id_quat(),
    );
    // Raking jib boom from low on the post up-and-forward (+Z).
    let jib = h * 0.95;
    let ang = 0.8_f32; // rotate +Y toward +Z
    let (sa, ca) = ang.sin_cos();
    let base_y = h * 0.15;
    root.children.push(prim(
        cylinder(0.016, jib, 6, steel.clone()),
        [0.0, base_y + 0.5 * jib * ca, 0.5 * jib * sa],
        quat_xyzw(quat_x(ang)),
    ));
    // Block-and-hook hanging from the jib tip.
    let tip = [0.0, base_y + jib * ca, jib * sa];
    let drop = h * 0.35;
    root.children.push(prim(
        cylinder(0.01, drop, 4, tackle.clone()),
        [tip[0], tip[1] - drop * 0.5, tip[2]],
        id_quat(),
    ));
    root.children.push(prim(
        cuboid([0.05, 0.06, 0.04], hook),
        [tip[0], tip[1] - drop, tip[2]],
        id_quat(),
    ));
    // Winch drum at the foot.
    root.children.push(prim(
        cylinder(0.03, 0.1, 8, tackle),
        [0.0, base_y * 0.4, -0.06],
        quat_xyzw(quat_z(FRAC_PI_2)),
    ));
    root
}

fn deck_cargo(ctx: &PartCtx) -> Generator {
    // An open working deck (industrial / grubby moods): a low sole carrying
    // lashed crates and a raised hatch instead of a cabin trunk.
    let sole = ctx.materials.body(shade(ctx.palette.primary_accent, 0.7));
    let hatch = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.45));
    let crate_a = ctx.materials.body(ctx.palette.secondary_accent);
    let crate_b = ctx.materials.body(shade(ctx.palette.tertiary_accent, 0.8));
    let (dw, dl) = deck_dims(ctx);

    let mut deck = prim(
        cuboid([0.35 * dw, 0.045, 0.64 * dl], sole.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Raised cargo hatch amidships-forward.
    deck.children.push(prim(
        cuboid([0.26 * dw, 0.07, 0.24 * dl], hatch.clone()),
        [0.0, 0.045, 0.2 * dl],
        id_quat(),
    ));
    // A stack of lashed crates of staggered size / colour aft.
    deck.children.push(prim(
        cuboid([0.14 * dw, 0.13, 0.16 * dl], crate_a),
        [-0.07 * dw, 0.088, -0.16 * dl],
        id_quat(),
    ));
    deck.children.push(prim(
        cuboid([0.12 * dw, 0.1, 0.13 * dl], crate_b),
        [0.09 * dw, 0.072, -0.2 * dl],
        id_quat(),
    ));
    // Low bulwarks down each side of the working deck.
    for s in [-1.0f32, 1.0] {
        deck.children.push(prim(
            cuboid([0.02, 0.05, 0.5 * dl], hatch.clone()),
            [s * 0.17 * dw, 0.03, 0.0],
            id_quat(),
        ));
    }
    deck
}

fn deck_bench(ctx: &PartCtx) -> Generator {
    // An open leisure deck (regal / genteel moods): a low sole with a cushioned
    // lounge bench and a small helm console, no cabin trunk.
    let sole = ctx.materials.body(shade(ctx.palette.primary_accent, 0.72));
    let trimwork = ctx.materials.trim(ctx.palette.tertiary_accent);
    let cushion = ctx.materials.cloth(ctx.palette.secondary_accent);
    let (dw, dl) = deck_dims(ctx);

    let mut deck = prim(
        cuboid([0.34 * dw, 0.045, 0.62 * dl], sole.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Lounge bench aft: a cushioned seat with a low backrest.
    deck.children.push(prim(
        cuboid([0.28 * dw, 0.05, 0.1 * dl], cushion.clone()),
        [0.0, 0.05, -0.22 * dl],
        id_quat(),
    ));
    deck.children.push(prim(
        cuboid([0.28 * dw, 0.11, 0.03], cushion),
        [0.0, 0.09, -0.28 * dl],
        id_quat(),
    ));
    // Small helm console forward, with a wheel post.
    deck.children.push(prim(
        cuboid([0.18 * dw, 0.09, 0.06], sole),
        [0.0, 0.045, 0.24 * dl],
        id_quat(),
    ));
    deck.children.push(prim(
        torus(0.012, 0.05, trimwork.clone()),
        [0.0, 0.13, 0.22 * dl],
        quat_xyzw(quat_x(0.5)),
    ));
    // Bright toe-rail posts down each side (genteel trim).
    for s in [-1.0f32, 1.0] {
        for z in [0.1 * dl, -0.05 * dl] {
            deck.children.push(prim(
                cylinder(0.012, 0.08, 6, trimwork.clone()),
                [s * 0.17 * dw, 0.04, z],
                id_quat(),
            ));
        }
    }
    deck
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

pub(super) static BOW_RAM: PartDef = PartDef {
    slug: "boat_bow_ram",
    slot: PartSlot::Bow,
    chassis: BOAT,
    styles: MARTIAL,
    ornateness: OrnatenessBand::ANY,
    // A battering ram is rough gear — it reads on a used or beaten craft, not a
    // pristine parade boat (the wear tier now gates the pick).
    wear: WORN_PLUS,
    build: bow_ram,
};
pub(super) static BOW_FIGUREHEAD: PartDef = PartDef {
    slug: "boat_bow_figurehead",
    slot: PartSlot::Bow,
    chassis: BOAT,
    styles: REGAL,
    // A carved figurehead is a fancy fitting — only an adorned/ornate craft.
    ornateness: FANCY,
    wear: WearBand::ANY,
    build: bow_figurehead,
};
pub(super) static BOW_BOWSPRIT: PartDef = PartDef {
    slug: "boat_bow_bowsprit",
    slot: PartSlot::Bow,
    chassis: BOAT,
    // Style-universal prow floor: every boat can carry a bowsprit.
    styles: UNIVERSAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: bow_bowsprit,
};
pub(super) static SMOKESTACK: PartDef = PartDef {
    slug: "boat_stack_funnel",
    slot: PartSlot::Stack,
    chassis: BOAT,
    styles: STEAM,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: smokestack,
};
pub(super) static STACK_THRUSTERS: PartDef = PartDef {
    slug: "boat_stack_thrusters",
    slot: PartSlot::Stack,
    chassis: BOAT,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: stack_thrusters,
};
pub(super) static STACK_VENT: PartDef = PartDef {
    slug: "boat_stack_vent",
    slot: PartSlot::Stack,
    chassis: BOAT,
    // Style-universal stack floor: every boat can carry a vent.
    styles: UNIVERSAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: stack_vent,
};
pub(super) static MAST_SQUARE_RIG: PartDef = PartDef {
    slug: "boat_mast_square_rig",
    slot: PartSlot::Mast,
    chassis: BOAT,
    // A square rig suits every historic tall-ship mood, not just the martial ones.
    styles: HISTORIC,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: mast_square_rig,
};
pub(super) static MAST_ANTENNA: PartDef = PartDef {
    slug: "boat_mast_antenna",
    slot: PartSlot::Mast,
    chassis: BOAT,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: mast_antenna,
};
pub(super) static MAST_DERRICK: PartDef = PartDef {
    slug: "boat_mast_derrick",
    slot: PartSlot::Mast,
    chassis: BOAT,
    styles: STEAM,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: mast_derrick,
};
pub(super) static DECK_CARGO: PartDef = PartDef {
    slug: "boat_deck_cargo",
    slot: PartSlot::Deck,
    chassis: BOAT,
    styles: GRUBBY,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: deck_cargo,
};
pub(super) static DECK_BENCH: PartDef = PartDef {
    slug: "boat_deck_bench",
    slot: PartSlot::Deck,
    chassis: BOAT,
    // An open lounge deck is the genteel / civic-ferry read.
    styles: REGAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: deck_bench,
};
