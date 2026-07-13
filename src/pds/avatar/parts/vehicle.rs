//! Styled vehicle part kits — crafted variants and ornaments for the boat /
//! airship / skiff families.
//!
//! Fills the previously-empty optional vehicle slots ([`PartSlot::Bow`] /
//! [`PartSlot::Stack`] / [`PartSlot::Exhaust`] / [`PartSlot::Ornament`]) and
//! adds style-specific variants for the body slots, plus cross-family
//! ornaments. Tagged by style and by ornateness / wear bands, so a steam funnel
//! only appears on a steampunk / industrial craft, a neon strip on a cyberpunk
//! one, and so on. Geometry uses the shared primitive vocabulary with torture
//! shaping; finish comes from the seeded
//! [`MaterialKit`](crate::seeded_defaults::MaterialKit).
//!
//! Every mood group (see the group consts) houses at least one of the 23
//! [`ThemeArchetype`]s, and every optional slot ships a **style-universal**
//! floor part (`boat_bow_bowsprit` / `boat_stack_vent` / `skiff_exhaust_tailpipe`
//! / `veh_orn_finial`, all empty-styles) so no theme's optional slots are ever
//! permanently bare — the styled and band-tagged parts then layer flavour on
//! top of that floor (#792).

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

use crate::pds::avatar::default_visuals::common::{
    cone, cuboid, cylinder, helix, id_quat, lathe, prim, quat_x, quat_xyzw, quat_y, quat_z, sphere,
    superellipsoid, torus, with_cut, with_shape,
};
use crate::pds::avatar::parts::defaults::airship::{
    GondolaDims, airship_colors, ctx_profile, dress_gondola, env_core, envelope_material,
    lathe_spindle, pod_pylon, push_env_gores, push_env_rings,
};
use crate::pds::avatar::parts::defaults::skiff::{
    push_wheel_fenders, skiff_colors, skiff_dims, skiff_wheel_anchors,
};
use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::ChassisFamily;
use crate::seeded_defaults::ThemeArchetype::{
    self, AlienMonolithic, AlienOrganic, AncientClassical, CivicCampus, CoastalResort, Cyberpunk,
    Fantasy, FeudalJapan, GothicHorror, IndustrialPark, Medieval, Mesoamerican, ModernCity, Nordic,
    PostApoc, Roadside, RuralFarmland, Solarpunk, SpaceOutpost, SportsRec, Steampunk, Suburban,
    WildWest,
};
use crate::seeded_defaults::{OrnatenessBand, OrnatenessTier, WearBand, WearTier};

use super::{PartCtx, PartDef, PartSlot};

const BOAT: &[ChassisFamily] = &[ChassisFamily::Boat];
const AIRSHIP: &[ChassisFamily] = &[ChassisFamily::Airship];
const SKIFF: &[ChassisFamily] = &[ChassisFamily::Skiff];
const VEHICLES: &[ChassisFamily] = &[
    ChassisFamily::Boat,
    ChassisFamily::Airship,
    ChassisFamily::Skiff,
];

// Mood groups — the vehicle-styling taxonomy. Each of the 23 `ThemeArchetype`s
// belongs to at least one group so no population is a "desert" with zero styled
// parts (#792). A theme may sit in several (a grimy neon craft is both NEON and
// GRUBBY); a part draws the group whose read it wants. NEON/STEAM/MARTIAL/REGAL/
// GRUBBY/HISTORIC are the originals, widened to fold the desert themes that fit
// them: FeudalJapan / Mesoamerican / GothicHorror → HISTORIC (old-world / ritual);
// AlienOrganic → NEON (bioluminescent); Roadside / RuralFarmland / Suburban →
// GRUBBY (worn, workaday, off-road ground craft). COASTAL is a new home for the
// seaside / sporting moods that fit none of the originals.
const NEON: &[ThemeArchetype] = &[
    Cyberpunk,
    SpaceOutpost,
    AlienMonolithic,
    Solarpunk,
    AlienOrganic,
];
const STEAM: &[ThemeArchetype] = &[Steampunk, IndustrialPark, ModernCity];
const MARTIAL: &[ThemeArchetype] = &[Medieval, Nordic, WildWest, PostApoc];
const REGAL: &[ThemeArchetype] = &[Fantasy, AncientClassical, CivicCampus];
// GRUBBY = grimy / worn / workaday ground craft: industrial soot, frontier
// scrap, and the ordinary agrarian / roadside / suburban beaters (buggies,
// knobby tyres, cargo decks). Widened past the original five so the farm /
// roadside / suburban desert themes get bespoke parts without losing the
// steampunk / industrial ones (a straight fold, no re-tag regression).
const GRUBBY: &[ThemeArchetype] = &[
    Steampunk,
    IndustrialPark,
    WildWest,
    PostApoc,
    Cyberpunk,
    Roadside,
    RuralFarmland,
    Suburban,
];
const HISTORIC: &[ThemeArchetype] = &[
    Medieval,
    Nordic,
    WildWest,
    PostApoc,
    Fantasy,
    AncientClassical,
    FeudalJapan,
    Mesoamerican,
    GothicHorror,
];
/// Seaside / leisure / sporting moods — resort cruisers, sport skiffs. Homes
/// CoastalResort / SportsRec, which fit none of the ground-craft groups.
const COASTAL: &[ThemeArchetype] = &[CoastalResort, SportsRec, Solarpunk];
/// Empty style list — a **style-universal** part, eligible for every theme (see
/// the module docstring). Used for the per-slot floor parts that guarantee no
/// optional vehicle slot is ever bare.
const UNIVERSAL: &[ThemeArchetype] = &[];

/// "Fancy" ornateness band (Adorned upward) — a figurehead, a pennant, a crest:
/// a plain avatar never rolls one, so the ornateness tier finally reads on the
/// optional-slot pick rather than every styled part being `ANY`/`ANY` (#792).
const FANCY: OrnatenessBand =
    OrnatenessBand::range(OrnatenessTier::Adorned, OrnatenessTier::Ornate);
/// "Worn or worse" wear band — a battering ram, sooted exhaust pipes: gear that
/// only reads on a used or beaten craft, never a factory-fresh one.
const WORN_PLUS: WearBand = WearBand::range(WearTier::Worn, WearTier::Battered);
/// Battered-only wear band — the beaten-up counterpart parts (a tattered
/// banner), so the top wear tier reads distinctly from merely-worn.
const BATTERED: WearBand = WearBand::only(WearTier::Battered);

fn shade(c: [f32; 3], f: f32) -> [f32; 3] {
    [c[0] * f, c[1] * f, c[2] * f]
}

fn darken(c: [f32; 3]) -> [f32; 3] {
    shade(c, 0.4)
}

/// Blueprint mast height (deck → masthead), or the pre-blueprint nominal (a
/// boat ctx always carries a blueprint; the fallback is defensive, and lets a
/// styled mast still build valid geometry when the round-trip test exercises it
/// against a non-boat seed).
fn mast_height(ctx: &PartCtx) -> f32 {
    ctx.boat().map_or(0.42, |b| b.mast_h)
}

/// Boat deck footprint multipliers `(dw, dl)` — the seeded beam / length over
/// the nominal, so a deck variant scales with its hull like the default deck.
fn deck_dims(ctx: &PartCtx) -> (f32, f32) {
    let (beam, length) = ctx.boat().map_or((0.5, 1.32), |b| (b.beam, b.hull_len));
    (beam / 0.5, length / 1.32)
}

// ---------------------------------------------------------------------------
// Boat
// ---------------------------------------------------------------------------

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
// Airship
// ---------------------------------------------------------------------------

fn teardrop_envelope(ctx: &PartCtx) -> Generator {
    // Steampunk teardrop — a single smooth Lathe spindle whose profile is a
    // SHARP nose over a FULL rounded tail with the waist biased forward (the
    // classic teardrop), no sphere↔cone junction (#791). Built as a child of a
    // hidden unscaled core (the assembler mounts the gondola / fins to the root,
    // which a root scale would fling). Shares the airship gore/ring/registry
    // surface pass so it reads as taut doped fabric, not a glossy blob.
    let c = airship_colors(ctx);
    let skin = envelope_material(c.envelope);
    let frame = ctx.materials.metal(c.frame);
    let stripe = ctx.materials.trim(c.stripe);
    let p = ctx_profile(ctx, "airship_envelope_teardrop");
    let mut env = env_core(&skin);
    env.children.push(lathe_spindle(&p, 0.0, skin));
    push_env_gores(&mut env, &p, 0.0, 7, &frame, Some(&stripe));
    push_env_rings(&mut env, &p, 0.0, 2, &frame);
    // Pointed nose finial just past the sharp nose.
    env.children.push(prim(
        sphere(0.1, 3, stripe),
        [0.0, 0.0, p.nose_z() + 0.04],
        id_quat(),
    ));
    env
}

fn pod_ducted(ctx: &PartCtx) -> Generator {
    // NEON engine pod: a ducted fan — a short fat nacelle behind a cowl ring
    // that shrouds a glowing fan disc, so the tech airships read as fan-driven.
    // Wears the ship's `accent` metal + a normalized glow so it sits in the
    // two-hue scheme (#789) rather than drawing a fresh colour.
    let c = airship_colors(ctx);
    let body = ctx.materials.metal(c.accent);
    let ring = ctx.materials.metal(c.frame);
    let glow = ctx.materials.glow(c.window);

    // Short fat nacelle laid along the travel axis (+Z front).
    let mut p = prim(
        cylinder(0.15, 0.36, 14, body),
        [0.0, 0.0, 0.0],
        quat_xyzw(quat_x(FRAC_PI_2)),
    );
    // A fat shroud ring (the duct cowl) proud of the intake — a closed hoop, so
    // the pod reads as ducted, not an open airscrew like the default (#790
    // review: it looked identical to the default open-prop).
    p.children.push(prim(
        torus(0.05, 0.19, ring.clone()),
        [0.0, 0.0, 0.2],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    // A bright face-on fan disc filling the duct (a flat glowing cylinder facing
    // +Z), so the ducted read carries head-on where the pods are most visible.
    p.children.push(prim(
        cylinder(0.16, 0.02, 18, glow.clone()),
        [0.0, 0.0, 0.2],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    // A dark hub + spokes over the disc so it reads as a spinning fan, not a lamp.
    p.children.push(prim(
        sphere(0.045, 3, ring.clone()),
        [0.0, 0.0, 0.23],
        id_quat(),
    ));
    // (Spoke thickness stays ≥ the sanitiser's 0.01 min cuboid dim.)
    for size in [[0.28, 0.02, 0.012], [0.02, 0.28, 0.012]] {
        p.children.push(prim(
            cuboid(size, ring.clone()),
            [0.0, 0.0, 0.225],
            id_quat(),
        ));
    }
    // Tapered tail.
    p.children.push(prim(
        cone(0.1, 0.14, 12, ring.clone()),
        [0.0, 0.0, -0.22],
        quat_xyzw(quat_x(-FRAC_PI_2)),
    ));
    pod_pylon(&mut p, &ring);
    p
}

fn pod_screw(ctx: &PartCtx) -> Generator {
    // STEAM engine pod: a riveted nacelle driving a brass Archimedes screw
    // (Helix prim, #527) — the steampunk airscrew. The screw + spinner wear the
    // registry `stripe` (brass) pop; the boiler bands the `frame` metal.
    let c = airship_colors(ctx);
    let body = ctx.materials.metal(c.accent);
    let dark = ctx.materials.metal(c.frame);
    let brass = ctx.materials.trim(c.stripe);

    let mut p = prim(
        cylinder(0.13, 0.5, 12, body.clone()),
        [0.0, 0.0, 0.0],
        quat_xyzw(quat_x(FRAC_PI_2)),
    );
    // Brass screw at the front (Helix laid along Z via quat_x(90°)).
    p.children.push(prim(
        helix(0.11, 0.02, 0.11, 2.5, 16, brass.clone()),
        [0.0, 0.0, 0.18],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    // Spinner cone capping the screw shaft.
    p.children.push(prim(
        cone(0.07, 0.14, 10, brass),
        [0.0, 0.0, 0.34],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    // Two boiler bands round the barrel.
    for z in [-0.14f32, 0.06] {
        p.children.push(prim(
            torus(0.018, 0.135, dark.clone()),
            [0.0, 0.0, z],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
    // Tapered tail.
    p.children.push(prim(
        cone(0.1, 0.14, 12, body),
        [0.0, 0.0, -0.3],
        quat_xyzw(quat_x(-FRAC_PI_2)),
    ));
    pod_pylon(&mut p, &dark);
    p
}

fn gondola_basket(ctx: &PartCtx) -> Generator {
    // Open wicker basket (a balloon-style car): a floor + four low woven walls
    // around an OPEN top, ringed by a bright rim — the HISTORIC alternative to
    // the enclosed cabin. Built as explicit walls rather than a hollowed
    // superellipsoid, which read as a solid shortened box from the side/front
    // (#790 review); the open-topped box reads unmistakably as a tub. Hidden hub
    // root at origin so the shared dressing seats correctly (the env_core
    // pattern). No glazing (it's open); the dressing hangs lanterns + a view
    // dome; the shallow floor is its underside.
    let c = airship_colors(ctx);
    let wicker = ctx.materials.cloth(c.accent);
    let floor = ctx.materials.cloth(shade(c.accent, 0.8));
    let rim = ctx.materials.trim(c.stripe);
    let dims = GondolaDims {
        hw: 0.24,
        hh: 0.15,
        hl: 0.42,
        keel_y: -0.15,
    };
    let mut g = prim(
        cuboid([0.03, 0.03, 0.03], wicker.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Floor pan.
    g.children.push(prim(
        cuboid([dims.hw * 2.0, 0.05, dims.hl * 2.0], floor),
        [0.0, -dims.hh, 0.0],
        id_quat(),
    ));
    // Four low woven walls, standing from the floor to a low open rim (~0.7 of
    // full height), leaving the top open.
    let wall_h = dims.hh * 1.5;
    let wall_cy = -dims.hh + wall_h * 0.5;
    for sx in [-1.0f32, 1.0] {
        g.children.push(prim(
            cuboid([0.03, wall_h, dims.hl * 2.0], wicker.clone()),
            [sx * dims.hw, wall_cy, 0.0],
            id_quat(),
        ));
    }
    for sz in [-1.0f32, 1.0] {
        g.children.push(prim(
            cuboid([dims.hw * 2.0, wall_h, 0.03], wicker.clone()),
            [0.0, wall_cy, sz * dims.hl],
            id_quat(),
        ));
    }
    // Bright rim rails capping the open top.
    let top = -dims.hh + wall_h;
    for sx in [-1.0f32, 1.0] {
        g.children.push(prim(
            cuboid([0.028, 0.028, dims.hl * 2.05], rim.clone()),
            [sx * dims.hw, top, 0.0],
            id_quat(),
        ));
    }
    for sz in [-1.0f32, 1.0] {
        g.children.push(prim(
            cuboid([dims.hw * 2.05, 0.028, 0.028], rim.clone()),
            [0.0, top, sz * dims.hl],
            id_quat(),
        ));
    }
    dress_gondola(&mut g, ctx, dims);
    g
}

fn gondola_cargo(ctx: &PartCtx) -> Generator {
    // Girder cargo frame: an open box frame of girders over a floor plate,
    // holding a couple of lashed crates — the GRUBBY freight hauler. Built on a
    // hidden hub at the car centre (origin) so the shared dressing seats
    // correctly — the visible frame hangs off it (the env_core pattern; a
    // floor-plate root would shift every dressing child down by its offset).
    let c = airship_colors(ctx);
    let girder = ctx.materials.metal(c.frame);
    let plate = ctx.materials.body(shade(c.accent, 0.8));
    let crate_mat = ctx.materials.body(c.accent);
    let dims = GondolaDims {
        hw: 0.24,
        hh: 0.16,
        hl: 0.46,
        // The deck pan is the underside; seat the view port just below it.
        keel_y: -0.18,
    };
    let mut g = prim(
        cuboid([0.03, 0.03, 0.03], girder.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Floor plate.
    g.children.push(prim(
        cuboid([dims.hw * 2.0, 0.04, dims.hl * 2.0], plate),
        [0.0, -dims.hh, 0.0],
        id_quat(),
    ));
    // Four corner posts (spanning floor→roof) + a top perimeter frame.
    for sx in [-1.0f32, 1.0] {
        for sz in [-1.0f32, 1.0] {
            g.children.push(prim(
                cuboid([0.03, dims.hh * 2.0, 0.03], girder.clone()),
                [sx * dims.hw * 0.94, 0.0, sz * dims.hl * 0.94],
                id_quat(),
            ));
        }
        g.children.push(prim(
            cuboid([0.03, 0.03, dims.hl * 2.0], girder.clone()),
            [sx * dims.hw * 0.94, dims.hh, 0.0],
            id_quat(),
        ));
    }
    for sz in [-1.0f32, 1.0] {
        g.children.push(prim(
            cuboid([dims.hw * 2.0, 0.03, 0.03], girder.clone()),
            [0.0, dims.hh, sz * dims.hl * 0.94],
            id_quat(),
        ));
    }
    // A couple of lashed crates sitting on the deck.
    g.children.push(prim(
        cuboid([0.17, 0.15, 0.17], crate_mat.clone()),
        [-0.06, -dims.hh + 0.095, 0.13],
        id_quat(),
    ));
    g.children.push(prim(
        cuboid([0.13, 0.12, 0.13], crate_mat),
        [0.08, -dims.hh + 0.08, -0.15],
        id_quat(),
    ));
    dress_gondola(&mut g, ctx, dims);
    g
}

// ---------------------------------------------------------------------------
// Skiff
// ---------------------------------------------------------------------------

fn bubble_canopy(ctx: &PartCtx) -> Generator {
    // A sleek, elongated teardrop cockpit bubble — the sporty alternative to the
    // default boxy cabin greenhouse.
    let mut c = prim(
        sphere(0.3, 4, ctx.materials.glass(ctx.palette.secondary_accent)),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    c.transform.scale = Fp3([0.82, 0.6, 1.08]);
    // Glowing rim around the base.
    c.children.push(prim(
        torus(0.02, 0.3, ctx.materials.glow(ctx.palette.primary_accent)),
        [0.0, -0.2, 0.0],
        id_quat(),
    ));
    c
}

fn twin_pipes(ctx: &PartCtx) -> Generator {
    // Two stern exhaust stacks.
    let pipe = ctx.materials.metal(darken(ctx.palette.tertiary_accent));
    let mut e = prim(
        cylinder(0.04, 0.35, 10, pipe.clone()),
        [-0.08, 0.15, 0.0],
        id_quat(),
    );
    e.children.push(prim(
        cylinder(0.04, 0.35, 10, pipe),
        [0.16, 0.0, 0.0],
        id_quat(),
    ));
    e
}

fn exhaust_tailpipe(ctx: &PartCtx) -> Generator {
    // The style-universal exhaust floor (empty styles): a single chromed
    // tailpipe running aft with a flared tip, so every skiff carries an exhaust
    // regardless of theme (the twin pipes are grimy / worn; this is the plain
    // fitting under every craft).
    let chrome = ctx.materials.metal([0.6, 0.6, 0.64]);
    // Pipe laid along Z (its local +Y → +Z), centred so most of it emerges aft
    // (-Z) with the forward stub buried in the tail bodywork.
    let mut root = prim(
        cylinder(0.035, 0.34, 12, chrome.clone()),
        [0.0, 0.06, -0.12],
        quat_xyzw(quat_x(FRAC_PI_2)),
    );
    // Flared tip ring at the aft mouth: in the pipe-local frame the barrel axis
    // is +Y, and pipe-local -Y is the aft end, so the default torus wraps it.
    root.children.push(prim(
        torus(0.016, 0.045, chrome),
        [0.0, -0.17, 0.0],
        id_quat(),
    ));
    root
}

// --- Skiff chassis variants (#788) -----------------------------------------
//
// The Chassis slot shipped exactly one part; the family docstring promises
// "rover / dune-skiff / trike". Each variant is a full structural root sized
// from the shared [`skiff_dims`] contract, draws its own mudguards via
// [`push_wheel_fenders`] (so the guards match the assembler's wheels), and
// wears the value-floored [`skiff_colors`] scheme (#787). The trike collapses
// its front axle to a single centreline wheel — the assembler keys that off the
// `skiff_chassis_trike` slug.

fn skiff_headlamps(c: &mut Generator, ctx: &PartCtx, xs: &[f32], z: f32) {
    // A dark bezel ring + bright lens per position — shared 3D-relief lamp.
    let bezel = ctx.materials.metal([0.09, 0.09, 0.11]);
    let lamp = ctx.materials.glow([1.0, 0.95, 0.8]);
    for &x in xs {
        c.children.push(prim(
            cylinder(0.05, 0.03, 12, bezel.clone()),
            [x, 0.05, z],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
        c.children.push(prim(
            cylinder(0.036, 0.05, 12, lamp.clone()),
            [x, 0.05, z + 0.01],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
}

fn chassis_dune(ctx: &PartCtx) -> Generator {
    // A dune buggy: a low exposed pod on an open tube frame with a roll bar and
    // an exposed rear engine — no full bodywork.
    let colors = skiff_colors(ctx);
    let pod = ctx.materials.body(colors.body);
    let frame = ctx.materials.metal(colors.trim);
    let dark = ctx.materials.metal(colors.lower);
    let dims = skiff_dims(ctx);
    let (body_w, body_len, _, _, _, wheel_r) = dims;
    let (dw, dl) = (body_w / 0.76, body_len / 1.5);

    // Low exposed pod tub (open-cockpit feel).
    let mut c = prim(
        superellipsoid([body_w * 0.42, 0.085, body_len * 0.44], 0.4, 0.55, pod),
        [0.0, -0.02, 0.04 * dl],
        id_quat(),
    );
    // Exposed rear engine block.
    c.children.push(prim(
        superellipsoid([0.24 * dw, 0.09, 0.2 * dl], 0.5, 0.6, dark.clone()),
        [0.0, 0.06, -0.42 * dl],
        id_quat(),
    ));
    // Roll bar behind the cockpit: two posts + a top tube.
    for s in [-1.0f32, 1.0] {
        c.children.push(prim(
            cylinder(0.018, 0.28, 8, frame.clone()),
            [s * 0.22 * dw, 0.12, -0.14 * dl],
            id_quat(),
        ));
    }
    c.children.push(prim(
        cylinder(0.018, 0.46 * dw, 8, frame.clone()),
        [0.0, 0.26, -0.14 * dl],
        quat_xyzw(quat_z(FRAC_PI_2)),
    ));
    // Nerf-bar side tubes + a front brush-bar carrying the lamps.
    for s in [-1.0f32, 1.0] {
        c.children.push(prim(
            cylinder(0.014, 0.72 * dl, 6, frame.clone()),
            [s * 0.4 * dw, -0.05, 0.0],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
    c.children.push(prim(
        cylinder(0.016, 0.5 * dw, 8, frame),
        [0.0, 0.03, 0.5 * body_len],
        quat_xyzw(quat_z(FRAC_PI_2)),
    ));
    push_wheel_fenders(&mut c, &skiff_wheel_anchors(dims, false), wheel_r, &dark);
    skiff_headlamps(&mut c, ctx, &[-0.24 * dw, 0.24 * dw], 0.5 * body_len);
    c
}

fn chassis_trike(ctx: &PartCtx) -> Generator {
    // A three-wheeler: a wide two-wheel rear cabin tapering to a single-wheel
    // nose (the assembler collapses the front axle to centreline for this slug).
    let colors = skiff_colors(ctx);
    let body = ctx.materials.body(colors.body);
    let dark = ctx.materials.metal(colors.lower);
    let trim = ctx.materials.metal(colors.trim);
    let taillight = ctx.materials.glow([0.85, 0.12, 0.1]);
    let dims = skiff_dims(ctx);
    let (body_w, body_len, _, _, _, wheel_r) = dims;
    let (dw, dl) = (body_w / 0.76, body_len / 1.5);

    // Wide rear cabin (the canopy seats here).
    let mut c = prim(
        superellipsoid(
            [body_w * 0.48, 0.1, body_len * 0.34],
            0.42,
            0.5,
            body.clone(),
        ),
        [0.0, 0.0, -0.16 * dl],
        id_quat(),
    );
    // Narrow forward spine reaching the single nose wheel.
    c.children.push(prim(
        superellipsoid([0.19 * dw, 0.08, body_len * 0.4], 0.42, 0.55, body.clone()),
        [0.0, -0.015, 0.26 * dl],
        id_quat(),
    ));
    // A pointed nose fairing over the front wheel.
    c.children.push(prim(
        superellipsoid([0.15 * dw, 0.07, 0.14 * dl], 0.45, 0.6, dark.clone()),
        [0.0, 0.0, 0.52 * dl],
        id_quat(),
    ));
    // Side trim spear along the spine.
    for s in [-1.0f32, 1.0] {
        c.children.push(prim(
            cuboid([0.02, 0.03, 0.7 * dl], trim.clone()),
            [s * 0.2 * dw, 0.0, 0.1 * dl],
            id_quat(),
        ));
    }
    push_wheel_fenders(&mut c, &skiff_wheel_anchors(dims, true), wheel_r, &dark);
    // A single central headlamp on the nose.
    skiff_headlamps(&mut c, ctx, &[0.0], 0.56 * body_len);
    // Rear tail lamps.
    for sx in [-1.0f32, 1.0] {
        c.children.push(prim(
            cylinder(0.036, 0.04, 10, taillight.clone()),
            [sx * 0.26 * dw, 0.08, -0.5 * body_len],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
    c
}

fn chassis_armored(ctx: &PartCtx) -> Generator {
    // A plated rover (martial): angular armour panels over a boxy hull, a sloped
    // glacis, side skirts and a skid plate — faceted where the civilian body is
    // rounded.
    let colors = skiff_colors(ctx);
    let body = ctx.materials.body(colors.body);
    let plate = ctx.materials.metal(colors.lower);
    let trim = ctx.materials.metal(colors.trim);
    let taillight = ctx.materials.glow([0.85, 0.12, 0.1]);
    let dims = skiff_dims(ctx);
    let (body_w, body_len, _, _, _, wheel_r) = dims;
    let (dw, dl) = (body_w / 0.76, body_len / 1.5);

    // Boxy armoured hull (a faceted cuboid, gently tumblehomed).
    let mut c = prim(
        with_shape(
            cuboid([body_w, 0.2, body_len], body.clone()),
            [0.1, 0.06],
            [0.0, 0.0, 0.0],
            [0.0, 0.0],
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Sloped front glacis plate.
    c.children.push(prim(
        cuboid([0.72 * dw, 0.16, 0.03], plate.clone()),
        [0.0, 0.05, 0.5 * dl],
        quat_xyzw(quat_x(-0.5)),
    ));
    // Side skirt plates.
    for s in [-1.0f32, 1.0] {
        c.children.push(prim(
            cuboid([0.03, 0.16, 1.02 * dl], plate.clone()),
            [s * 0.5 * body_w, -0.02, 0.0],
            id_quat(),
        ));
    }
    // Armoured cabin block (the canopy seats on this).
    c.children.push(prim(
        cuboid([0.62 * dw, 0.14, 0.5 * dl], body),
        [0.0, 0.15, -0.14 * dl],
        id_quat(),
    ));
    // Underbody skid plate.
    c.children.push(prim(
        cuboid([0.74 * dw, 0.06, 1.06 * dl], plate.clone()),
        [0.0, -0.13, 0.0],
        id_quat(),
    ));
    // Bolt-strip trim across the glacis.
    c.children.push(prim(
        cuboid([0.5 * dw, 0.03, 0.03], trim),
        [0.0, 0.02, 0.52 * body_len],
        id_quat(),
    ));
    push_wheel_fenders(&mut c, &skiff_wheel_anchors(dims, false), wheel_r, &plate);
    // Slit headlamps set into the glacis (narrow, armoured).
    skiff_headlamps(&mut c, ctx, &[-0.24 * dw, 0.24 * dw], 0.5 * body_len);
    for sx in [-1.0f32, 1.0] {
        c.children.push(prim(
            cylinder(0.036, 0.04, 10, taillight.clone()),
            [sx * 0.24 * dw, 0.08, -0.5 * body_len],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
    c
}

// --- Skiff wheel variants (#788) --------------------------------------------
//
// The Wheel slot ships one part repeated to every corner, so a single variant
// changes all of a vehicle's wheels at once. Each keeps the outer radius at the
// blueprint `wheel_r` so it still seats in its guard.

fn wheel_spoked(ctx: &PartCtx) -> Generator {
    // A wagon wheel: a thin tyre with radial spoke bars and a proud hub.
    let tyre = ctx.materials.metal([0.07, 0.07, 0.08]);
    let rim = ctx.materials.metal(ctx.palette.secondary_accent);
    let hub = ctx.materials.trim(ctx.palette.tertiary_accent);
    let (_, _, _, _, _, wheel_r) = skiff_dims(ctx);
    let minor = wheel_r * 0.16;
    let major = wheel_r - minor;
    let mut w = prim(torus(minor, major, tyre), [0.0, 0.0, 0.0], id_quat());
    // Four crossing spoke bars (eight spokes) in the wheel plane.
    for i in 0..4 {
        w.children.push(prim(
            cuboid([major * 1.9, 0.02, 0.03], rim.clone()),
            [0.0, 0.0, 0.0],
            quat_xyzw(quat_y(i as f32 * FRAC_PI_4)),
        ));
    }
    // Proud hub cap.
    w.children.push(prim(
        cylinder(major * 0.26, 0.14, 12, hub),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    w
}

fn wheel_knobby(ctx: &PartCtx) -> Generator {
    // A fat off-road tyre studded with tread knobs around the crown.
    let tyre = ctx.materials.metal([0.08, 0.08, 0.09]);
    let rim = ctx.materials.metal(ctx.palette.secondary_accent);
    let hub = ctx.materials.trim(ctx.palette.tertiary_accent);
    let (_, _, _, _, _, wheel_r) = skiff_dims(ctx);
    let minor = wheel_r * 0.42;
    let major = wheel_r - minor;
    let mut w = prim(
        torus(minor, major, tyre.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    let n = 10;
    for i in 0..n {
        let ang = i as f32 / n as f32 * TAU;
        let (s, cc) = ang.sin_cos();
        let r = major + minor * 0.72;
        w.children.push(prim(
            cuboid([0.045, 0.055, 0.045], tyre.clone()),
            [cc * r, 0.0, s * r],
            quat_xyzw(quat_y(ang)),
        ));
    }
    // Deep rim plate + hub.
    w.children.push(prim(
        cylinder(major * 0.62, 0.16, 12, rim),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    w.children.push(prim(
        cylinder(major * 0.24, 0.18, 10, hub),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    w
}

fn wheel_glow(ctx: &PartCtx) -> Generator {
    // A hover-tech wheel: a slim tyre over a glowing hub disc on each face.
    let tyre = ctx.materials.metal([0.07, 0.07, 0.08]);
    let rim = ctx.materials.metal(ctx.palette.secondary_accent);
    let glow = ctx.materials.glow(ctx.palette.primary_accent);
    let (_, _, _, _, _, wheel_r) = skiff_dims(ctx);
    let minor = wheel_r * 0.24;
    let major = wheel_r - minor;
    let mut w = prim(torus(minor, major, tyre), [0.0, 0.0, 0.0], id_quat());
    // Rim ring + a glowing disc on each face.
    w.children.push(prim(
        cylinder(major * 0.86, 0.1, 20, rim),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    for s in [-1.0f32, 1.0] {
        w.children.push(prim(
            cylinder(major * 0.7, 0.02, 20, glow.clone()),
            [0.0, s * 0.06, 0.0],
            id_quat(),
        ));
    }
    w
}

// ---------------------------------------------------------------------------
// Cross-family ornaments
// ---------------------------------------------------------------------------

fn pennant(ctx: &PartCtx) -> Generator {
    let mut p = prim(
        cylinder(
            0.01,
            0.32,
            6,
            ctx.materials.metal(ctx.palette.tertiary_accent),
        ),
        [0.0, 0.16, 0.0],
        id_quat(),
    );
    p.children.push(prim(
        // 0.01 is the sanitiser's minimum cuboid dimension — a thinner flag
        // would be clamped and diverge from what peers render.
        cuboid(
            [0.18, 0.10, 0.01],
            ctx.materials.cloth(ctx.palette.primary_accent),
        ),
        [0.10, 0.10, 0.0],
        id_quat(),
    ));
    p
}

fn neon_strip(ctx: &PartCtx) -> Generator {
    prim(
        cuboid(
            [0.4, 0.02, 0.02],
            ctx.materials.glow(ctx.palette.primary_accent),
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    )
}

fn ornament_finial(ctx: &PartCtx) -> Generator {
    // The style-universal ornament floor (empty styles, every family): a little
    // turned finial — a pedestal topped by a banded orb. Being fully 3D it reads
    // from every angle (unlike a flat badge) so it works as a boat masthead knob,
    // a skiff hood mascot, or an airship nose crest, on any theme — the humble
    // accent that keeps every population's Ornament slot fillable (#792).
    let post = ctx.materials.metal(ctx.palette.secondary_accent);
    let orb = ctx.materials.trim(ctx.palette.tertiary_accent);
    let collar = ctx.materials.accent(ctx.palette.primary_accent);
    // Hidden hub at the mount so the post and orb share one un-translated frame —
    // a translated post-as-root would carry its +0.07 into the orb / collar (the
    // transform-inheritance gotcha), floating the orb off the pedestal top.
    let mut root = prim(
        cuboid([0.03, 0.03, 0.03], post.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Turned pedestal rising from the mount (top at y=0.14).
    root.children.push(prim(
        cylinder(0.025, 0.14, 10, post),
        [0.0, 0.07, 0.0],
        id_quat(),
    ));
    // Banded orb seated on the pedestal top (its lower half overlaps the post).
    root.children
        .push(prim(sphere(0.06, 3, orb), [0.0, 0.17, 0.0], id_quat()));
    // Collar ring at the orb waist (orb's Y axis → default torus).
    root.children.push(prim(
        torus(0.012, 0.062, collar),
        [0.0, 0.17, 0.0],
        id_quat(),
    ));
    root
}

fn ornament_tattered(ctx: &PartCtx) -> Generator {
    // The battered-only ornament counterpart (empty styles, wear = Battered): a
    // bent staff flying a ragged swallowtail banner, so a beaten-up craft flies a
    // tattered colour where a pristine one wouldn't — the top wear tier reads on
    // the ornament roll (#792). Cheap: the pennant staff, canted, with a torn
    // (deeply forked) darker cloth.
    let staff = ctx.materials.metal(darken(ctx.palette.secondary_accent));
    let cloth = ctx.materials.cloth(shade(ctx.palette.primary_accent, 0.55));
    // Hidden hub so the canted staff doesn't tumble the banner's placement.
    let mut root = prim(
        cuboid([0.02, 0.02, 0.02], staff.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Bent staff (canted aft a touch, as if weathered).
    root.children.push(prim(
        cylinder(0.01, 0.3, 6, staff),
        [0.0, 0.15, 0.0],
        quat_xyzw(quat_x(-0.14)),
    ));
    // Two ragged banner tongues of unequal length, both hung FLUSH at the staff
    // (hoist edge at x=0, z≈0) so the frayed fly ends read as a torn / forked
    // pennant, not scraps floating in front of the pole. The fork reads through
    // the differing length + height, not a forward-Z gap (the #792-review bug);
    // taper frays the fly to a torn point and only a gentle bend flutters the tip.
    for (w, h, z, y) in [
        (0.14f32, 0.075f32, 0.01f32, 0.19f32),
        (0.1, 0.05, -0.01, 0.11),
    ] {
        root.children.push(prim(
            with_shape(
                cuboid([w, h, 0.012], cloth.clone()),
                [0.5, 0.0],
                [0.03, 0.0, 0.02],
                [0.0, 0.0],
            ),
            [w * 0.5, y, z],
            id_quat(),
        ));
    }
    root
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

static BOW_RAM: PartDef = PartDef {
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
static BOW_FIGUREHEAD: PartDef = PartDef {
    slug: "boat_bow_figurehead",
    slot: PartSlot::Bow,
    chassis: BOAT,
    styles: REGAL,
    // A carved figurehead is a fancy fitting — only an adorned/ornate craft.
    ornateness: FANCY,
    wear: WearBand::ANY,
    build: bow_figurehead,
};
static BOW_BOWSPRIT: PartDef = PartDef {
    slug: "boat_bow_bowsprit",
    slot: PartSlot::Bow,
    chassis: BOAT,
    // Style-universal prow floor: every boat can carry a bowsprit.
    styles: UNIVERSAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: bow_bowsprit,
};
static SMOKESTACK: PartDef = PartDef {
    slug: "boat_stack_funnel",
    slot: PartSlot::Stack,
    chassis: BOAT,
    styles: STEAM,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: smokestack,
};
static STACK_THRUSTERS: PartDef = PartDef {
    slug: "boat_stack_thrusters",
    slot: PartSlot::Stack,
    chassis: BOAT,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: stack_thrusters,
};
static STACK_VENT: PartDef = PartDef {
    slug: "boat_stack_vent",
    slot: PartSlot::Stack,
    chassis: BOAT,
    // Style-universal stack floor: every boat can carry a vent.
    styles: UNIVERSAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: stack_vent,
};
static MAST_SQUARE_RIG: PartDef = PartDef {
    slug: "boat_mast_square_rig",
    slot: PartSlot::Mast,
    chassis: BOAT,
    // A square rig suits every historic tall-ship mood, not just the martial ones.
    styles: HISTORIC,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: mast_square_rig,
};
static MAST_ANTENNA: PartDef = PartDef {
    slug: "boat_mast_antenna",
    slot: PartSlot::Mast,
    chassis: BOAT,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: mast_antenna,
};
static MAST_DERRICK: PartDef = PartDef {
    slug: "boat_mast_derrick",
    slot: PartSlot::Mast,
    chassis: BOAT,
    styles: STEAM,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: mast_derrick,
};
static DECK_CARGO: PartDef = PartDef {
    slug: "boat_deck_cargo",
    slot: PartSlot::Deck,
    chassis: BOAT,
    styles: GRUBBY,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: deck_cargo,
};
static DECK_BENCH: PartDef = PartDef {
    slug: "boat_deck_bench",
    slot: PartSlot::Deck,
    chassis: BOAT,
    // An open lounge deck is the genteel / civic-ferry read.
    styles: REGAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: deck_bench,
};
static TEARDROP_ENVELOPE: PartDef = PartDef {
    slug: "airship_envelope_teardrop",
    slot: PartSlot::Envelope,
    chassis: AIRSHIP,
    styles: STEAM,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: teardrop_envelope,
};
static POD_DUCTED: PartDef = PartDef {
    slug: "airship_pod_ducted",
    slot: PartSlot::Pod,
    chassis: AIRSHIP,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: pod_ducted,
};
static POD_SCREW: PartDef = PartDef {
    slug: "airship_pod_screw",
    slot: PartSlot::Pod,
    chassis: AIRSHIP,
    styles: STEAM,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: pod_screw,
};
static GONDOLA_BASKET: PartDef = PartDef {
    slug: "airship_gondola_basket",
    slot: PartSlot::Gondola,
    chassis: AIRSHIP,
    styles: HISTORIC,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: gondola_basket,
};
static GONDOLA_CARGO: PartDef = PartDef {
    slug: "airship_gondola_cargo",
    slot: PartSlot::Gondola,
    chassis: AIRSHIP,
    styles: GRUBBY,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: gondola_cargo,
};
static BUBBLE_CANOPY: PartDef = PartDef {
    slug: "skiff_canopy_bubble",
    slot: PartSlot::Canopy,
    chassis: SKIFF,
    // The sleek teardrop bubble is the sporty / seaside cockpit read (it homes
    // the COASTAL group's skiffs; the tech craft keep the default greenhouse).
    styles: COASTAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: bubble_canopy,
};
static TWIN_PIPES: PartDef = PartDef {
    slug: "skiff_exhaust_twin_pipes",
    slot: PartSlot::Exhaust,
    chassis: SKIFF,
    styles: GRUBBY,
    ornateness: OrnatenessBand::ANY,
    // Sooted twin stacks read on a used / beaten skiff, not a fresh one.
    wear: WORN_PLUS,
    build: twin_pipes,
};
static EXHAUST_TAILPIPE: PartDef = PartDef {
    slug: "skiff_exhaust_tailpipe",
    slot: PartSlot::Exhaust,
    chassis: SKIFF,
    // Style-universal exhaust floor: every skiff can carry a tailpipe.
    styles: UNIVERSAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: exhaust_tailpipe,
};
static CHASSIS_DUNE: PartDef = PartDef {
    slug: "skiff_chassis_dune",
    slot: PartSlot::Chassis,
    chassis: SKIFF,
    // An open buggy is the workaday / off-road read — grimy industrial /
    // frontier craft plus the agrarian / roadside / suburban beaters GRUBBY now
    // folds in.
    styles: GRUBBY,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: chassis_dune,
};
static CHASSIS_TRIKE: PartDef = PartDef {
    slug: "skiff_chassis_trike",
    slot: PartSlot::Chassis,
    chassis: SKIFF,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: chassis_trike,
};
static CHASSIS_ARMORED: PartDef = PartDef {
    slug: "skiff_chassis_armored",
    slot: PartSlot::Chassis,
    chassis: SKIFF,
    styles: MARTIAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: chassis_armored,
};
static WHEEL_SPOKED: PartDef = PartDef {
    slug: "skiff_wheel_spoked",
    slot: PartSlot::Wheel,
    chassis: SKIFF,
    styles: HISTORIC,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: wheel_spoked,
};
static WHEEL_KNOBBY: PartDef = PartDef {
    slug: "skiff_wheel_knobby",
    slot: PartSlot::Wheel,
    chassis: SKIFF,
    // Fat off-road tyres are the grimy / frontier / farm / roadside read.
    styles: GRUBBY,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: wheel_knobby,
};
static WHEEL_GLOW: PartDef = PartDef {
    slug: "skiff_wheel_glow",
    slot: PartSlot::Wheel,
    chassis: SKIFF,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: wheel_glow,
};
static PENNANT: PartDef = PartDef {
    slug: "veh_orn_pennant",
    slot: PartSlot::Ornament,
    chassis: VEHICLES,
    styles: REGAL,
    // A flown pennant is a fancy flourish — an adorned / ornate craft only.
    ornateness: FANCY,
    wear: WearBand::ANY,
    build: pennant,
};
static NEON_STRIP: PartDef = PartDef {
    slug: "veh_orn_neon_strip",
    slot: PartSlot::Ornament,
    chassis: VEHICLES,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: neon_strip,
};
static ORNAMENT_FINIAL: PartDef = PartDef {
    slug: "veh_orn_finial",
    slot: PartSlot::Ornament,
    chassis: VEHICLES,
    // Style-universal ornament floor for every vehicle family: no population's
    // Ornament slot is ever bare.
    styles: UNIVERSAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: ornament_finial,
};
static ORNAMENT_TATTERED: PartDef = PartDef {
    slug: "veh_orn_tattered",
    slot: PartSlot::Ornament,
    chassis: VEHICLES,
    styles: UNIVERSAL,
    ornateness: OrnatenessBand::ANY,
    // The beaten-up counterpart to the finial / pennant — battered craft only.
    wear: BATTERED,
    build: ornament_tattered,
};

/// Every styled vehicle part. The four style-universal floors (`bowsprit` /
/// `smokestack`-slug's peer `vent` / `tailpipe` / `finial`) sit alongside the
/// mood-tagged and band-tagged variants; the outfit deriver draws from the
/// union, so every theme fills every optional slot from the floor up (#792).
pub(super) static ENTRIES: &[&dyn super::BodyPart] = &[
    &BOW_RAM,
    &BOW_FIGUREHEAD,
    &BOW_BOWSPRIT,
    &SMOKESTACK,
    &STACK_THRUSTERS,
    &STACK_VENT,
    &MAST_SQUARE_RIG,
    &MAST_ANTENNA,
    &MAST_DERRICK,
    &DECK_CARGO,
    &DECK_BENCH,
    &TEARDROP_ENVELOPE,
    &POD_DUCTED,
    &POD_SCREW,
    &GONDOLA_BASKET,
    &GONDOLA_CARGO,
    &BUBBLE_CANOPY,
    &TWIN_PIPES,
    &EXHAUST_TAILPIPE,
    &CHASSIS_DUNE,
    &CHASSIS_TRIKE,
    &CHASSIS_ARMORED,
    &WHEEL_SPOKED,
    &WHEEL_KNOBBY,
    &WHEEL_GLOW,
    &PENNANT,
    &NEON_STRIP,
    &ORNAMENT_FINIAL,
    &ORNAMENT_TATTERED,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::avatar::parts::{optional_slots, parts_for, parts_for_avatar};
    use crate::seeded_defaults::{OrnatenessTier, WearTier};

    /// The three vehicle families (the humanoid is a separate kit).
    const FAMILIES: [ChassisFamily; 3] = [
        ChassisFamily::Boat,
        ChassisFamily::Airship,
        ChassisFamily::Skiff,
    ];

    #[test]
    fn universal_floors_only_fill_optional_slots() {
        let ctx = PartCtx::for_seed(13);
        for part in ENTRIES {
            assert!(!part.chassis().is_empty(), "{} no chassis", part.slug());
            let a = part.build(&ctx);
            let b = part.build(&ctx);
            assert_eq!(a, b, "{} non-deterministic", part.slug());
            if part.styles().is_empty() {
                // A style-universal part is a per-slot floor; it must fill an
                // OPTIONAL slot for every family it serves (a required slot
                // already carries its universal default — an untagged body-slot
                // variant would be an authoring slip, not an intentional floor).
                for &fam in part.chassis() {
                    assert!(
                        optional_slots(fam).contains(&part.slot()),
                        "{} is style-universal but fills required slot {:?} for {fam:?}",
                        part.slug(),
                        part.slot()
                    );
                }
            }
        }
    }

    #[test]
    fn every_theme_fills_every_optional_vehicle_slot() {
        // The vehicle analogue of `every_required_slot_is_fillable_for_every_style`
        // (#792): after folding the nine desert themes into mood groups and
        // shipping a style-universal floor per optional slot, no
        // (family, optional slot, theme) query is ever empty.
        for chassis in FAMILIES {
            for &slot in optional_slots(chassis) {
                for style in ThemeArchetype::ALL {
                    assert!(
                        parts_for(chassis, slot, style).next().is_some(),
                        "{chassis:?}/{slot:?}/{style:?} has no vehicle part"
                    );
                }
            }
        }
    }

    #[test]
    fn optional_slots_have_a_floor_at_every_tier() {
        // Stronger than the style-level guarantee: the band-tagged variants
        // layer on top of an `ANY`/`ANY` style-universal floor, so the
        // band-gated pool an avatar actually draws from is non-empty at every
        // ornateness/wear tier too — no roll hits an empty pool.
        for chassis in FAMILIES {
            for &slot in optional_slots(chassis) {
                for style in ThemeArchetype::ALL {
                    for o in OrnatenessTier::ALL {
                        for w in WearTier::ALL {
                            assert!(
                                parts_for_avatar(chassis, slot, style, o, w)
                                    .next()
                                    .is_some(),
                                "{chassis:?}/{slot:?}/{style:?} empty at {o:?}/{w:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn every_theme_belongs_to_a_mood_group() {
        // Fold guarantee: every archetype sits in at least one styling group, so
        // it draws at least one styled part *somewhere* (on some chassis / slot),
        // not only the universal floors. It does NOT promise a styled BODY variant
        // on every chassis — e.g. a COASTAL boat still draws the default hull + the
        // universal floors, since COASTAL's only body part is the sporty skiff
        // canopy; per-chassis body coverage is the bespoke-parts job (#793).
        for style in ThemeArchetype::ALL {
            let grouped = [NEON, STEAM, MARTIAL, REGAL, GRUBBY, HISTORIC, COASTAL]
                .iter()
                .any(|g| g.contains(&style));
            assert!(grouped, "{style:?} belongs to no mood group");
        }
    }

    #[test]
    fn ornateness_and_wear_bands_gate_optional_vehicle_parts() {
        // The tier axes are no longer inert (#792): fancy prow / pennant show
        // only on adorned+ craft, the ram / twin pipes only on worn+ craft, and
        // the tattered banner only on battered craft.
        let has = |chassis, slot, style, o, w, slug: &str| {
            parts_for_avatar(chassis, slot, style, o, w).any(|p| p.slug() == slug)
        };
        use ChassisFamily::{Boat, Skiff};
        use OrnatenessTier::{Ornate, Plain};
        use WearTier::{Battered, Pristine, Worn};
        // Fancy figurehead — gated by ornateness (Adorned upward); Fantasy is REGAL.
        assert!(!has(
            Boat,
            PartSlot::Bow,
            Fantasy,
            Plain,
            Worn,
            "boat_bow_figurehead"
        ));
        assert!(has(
            Boat,
            PartSlot::Bow,
            Fantasy,
            Ornate,
            Worn,
            "boat_bow_figurehead"
        ));
        // Battering ram — gated by wear (Worn upward).
        assert!(!has(
            Boat,
            PartSlot::Bow,
            Medieval,
            Ornate,
            Pristine,
            "boat_bow_ram"
        ));
        assert!(has(
            Boat,
            PartSlot::Bow,
            Medieval,
            Ornate,
            Worn,
            "boat_bow_ram"
        ));
        // Sooted twin pipes — worn upward.
        assert!(!has(
            Skiff,
            PartSlot::Exhaust,
            Cyberpunk,
            Ornate,
            Pristine,
            "skiff_exhaust_twin_pipes"
        ));
        assert!(has(
            Skiff,
            PartSlot::Exhaust,
            Cyberpunk,
            Ornate,
            Battered,
            "skiff_exhaust_twin_pipes"
        ));
        // Tattered banner — battered only.
        assert!(!has(
            Boat,
            PartSlot::Ornament,
            Medieval,
            Ornate,
            Worn,
            "veh_orn_tattered"
        ));
        assert!(has(
            Boat,
            PartSlot::Ornament,
            Medieval,
            Ornate,
            Battered,
            "veh_orn_tattered"
        ));
        // The universal floor is always there regardless of tier.
        assert!(has(
            Boat,
            PartSlot::Bow,
            FeudalJapan,
            Plain,
            Pristine,
            "boat_bow_bowsprit"
        ));
    }

    #[test]
    fn steam_boat_can_fit_a_funnel_stack() {
        let stacks: Vec<&str> = parts_for_avatar(
            ChassisFamily::Boat,
            PartSlot::Stack,
            ThemeArchetype::Steampunk,
            OrnatenessTier::Adorned,
            WearTier::Worn,
        )
        .map(|p| p.slug())
        .collect();
        assert!(stacks.contains(&"boat_stack_funnel"), "got {stacks:?}");
    }
}
