//! Hover-boat family builder — the original default avatar, now with
//! four hull forms instead of one.
//!
//! The visual tree is a stylised steampunk/scifi vessel: a deck slab
//! bridging the hull arrangement picked by
//! [`HullForm`], a tapered central
//! mast crowned by a glowing finial, a pfp banner behind the mast,
//! brass deck rails, an optional prow ornament, and either flared
//! smokestacks (Steam / Hybrid) or a tilted solar panel + antenna
//! (Solar / Hybrid). Hull capsules carry an upward prow-rake bend so
//! the bow sweeps out of the water like a gondola.
//!
//! Colour assignments:
//!   deck            = primary_accent  (largest visible surface, plank)
//!   hulls           = secondary_accent
//!   mast / rails    = tertiary_accent
//!   bow jewel / finial = eye_color    (small "gem" slot)
//!   smokestacks / panel / antenna = hair_color (curated darker tone —
//!                     reads as metallic, not as accent paint)

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::Generator;
use crate::seeded_defaults::{AvatarBody, AvatarPalette, BowStyle, HullForm, VesselDesign};

use super::common::{
    brass_mat, capsule, cone, cuboid, cylinder, funnel_mat, glow_mat, id_quat, metal_mat, pastel,
    pfp_banner, plank_mat, prim, quat_x, quat_xyzw, quat_z, sphere, with_torture,
};

pub(super) fn build(did: &str) -> Generator {
    let palette = AvatarPalette::for_did(did);
    let body = AvatarBody::for_did(did);
    let vessel = VesselDesign::for_did(did);

    let deck_color = palette.primary_accent;
    let hull_color = palette.secondary_accent;
    let mast_color = palette.tertiary_accent;
    let jewel_color = palette.eye_color;
    let metal_color = palette.hair_color;

    // Two-level scaling: AvatarBody = avatar-wide size (humanoid-tight
    // band, ±15 %); VesselDesign = vessel-specific proportions.
    let h = body.height_scale;
    let w = body.shoulder_width_scale;
    let limb = body.limb_thickness_scale;
    let head = body.head_scale;

    let hull_r = 0.28 * limb * vessel.hull_radius_scale;
    let hull_len = 2.4 * h * vessel.hull_length_scale;
    let hull_x = 0.80 * w * vessel.hull_spread_scale;
    let hull_y = -0.30 * h * vessel.hull_drop_scale;

    // Hull form reshapes the deck footprint too: monohulls carry a
    // slim deck, barges an oversized one.
    let deck_width_mul = match vessel.hull_form {
        HullForm::Monohull => 0.70,
        HullForm::Catamaran | HullForm::Trimaran => 1.0,
        HullForm::Barge => 1.15,
    };
    let deck_x = 1.6 * w * vessel.hull_spread_scale * deck_width_mul;
    let deck_y = 0.12;
    let deck_z = 2.0 * h * vessel.hull_length_scale;
    let deck_half_z = deck_z * 0.5;

    let mast_height_mul = match vessel.hull_form {
        HullForm::Monohull => 1.10,
        HullForm::Barge => 0.80,
        _ => 1.0,
    };
    let mast_r = 0.05 * limb * vessel.mast_radius_scale;
    let mast_h = 1.4 * h * vessel.mast_height_scale * mast_height_mul;
    // Mast cylinder centre Y so the base rests on the deck top.
    let mast_origin_y = 0.5 * deck_y + mast_h * 0.5;

    // ---- Hull arrangement -------------------------------------------------
    // Capsules are laid along Z bow-first (local +Y → world -Z via the
    // -π/2 X rotation) so the prow-rake bend lifts the *bow* tip.
    let lay_bow_first = quat_xyzw(quat_x(-FRAC_PI_2));
    let make_hull = |x: f32, r: f32, len: f32, rake: f32| {
        prim(
            with_torture(
                capsule(r, len, metal_mat(hull_color)),
                0.0,
                0.0,
                [0.0, 0.0, rake],
            ),
            [x, hull_y, 0.0],
            lay_bow_first,
        )
    };

    let mut hulls: Vec<Generator> = Vec::new();
    match vessel.hull_form {
        HullForm::Monohull => {
            hulls.push(make_hull(0.0, hull_r * 1.5, hull_len, vessel.prow_rake));
        }
        HullForm::Catamaran => {
            hulls.push(make_hull(-hull_x, hull_r, hull_len, vessel.prow_rake));
            hulls.push(make_hull(hull_x, hull_r, hull_len, vessel.prow_rake));
        }
        HullForm::Trimaran => {
            hulls.push(make_hull(0.0, hull_r * 1.2, hull_len, vessel.prow_rake));
            // Outriggers: slimmer, shorter, pushed wider, raked less.
            let out_x = hull_x * 1.15;
            let out_rake = vessel.prow_rake * 0.6;
            hulls.push(make_hull(-out_x, hull_r * 0.55, hull_len * 0.7, out_rake));
            hulls.push(make_hull(out_x, hull_r * 0.55, hull_len * 0.7, out_rake));
        }
        HullForm::Barge => {
            // One shallow slab, sides flaring outward toward the deck
            // (negative taper widens the top). Base width is shrunk so
            // the flared crown meets the deck edge.
            let slab = with_torture(
                cuboid(
                    [deck_x * 0.88, 0.30 * h, deck_z * 0.95],
                    metal_mat(hull_color),
                ),
                0.0,
                -0.18,
                [0.0, 0.0, 0.0],
            );
            hulls.push(prim(slab, [0.0, hull_y * 0.7, 0.0], id_quat()));
        }
    }

    // ---- Mast subtree -----------------------------------------------------
    let mut mast_children: Vec<Generator> = Vec::new();
    mast_children.push(prim(
        sphere(0.10 * head, 3, glow_mat(jewel_color)),
        [0.0, mast_h * 0.5, 0.0],
        id_quat(),
    ));
    if vessel.archetype.has_antenna() {
        let antenna_h = 0.45 * mast_h;
        mast_children.push(prim(
            cylinder(0.015 * limb, antenna_h, 8, brass_mat(metal_color)),
            [0.0, mast_h * 0.5 + antenna_h * 0.5 + 0.08, 0.0],
            id_quat(),
        ));
    }
    // Pfp banner standing in YZ (normal ±X) — readable from both
    // sides like a heraldic banner.
    let flag_height = 0.55;
    let flag_width = 0.40;
    mast_children.push(pfp_banner(
        did,
        flag_height,
        flag_width,
        [0.0, mast_h * 0.2, flag_width * 0.5 + 0.05],
        quat_xyzw(quat_z(FRAC_PI_2)),
        pastel(deck_color),
    ));

    let mut mast = prim(
        with_torture(
            cylinder(mast_r, mast_h, 16, metal_mat(mast_color)),
            0.0,
            vessel.mast_taper,
            [0.0, 0.0, 0.0],
        ),
        [0.0, mast_origin_y, 0.0],
        id_quat(),
    );
    mast.children = mast_children;

    // ---- Deck rails -------------------------------------------------------
    // Two brass trim strips along the deck edges — small-area accent
    // that breaks up the deck slab without adding silhouette noise.
    let rail_x = deck_x * 0.5 - 0.04;
    let rail = |x: f32| {
        prim(
            cuboid([0.05, 0.08, deck_z * 0.92], brass_mat(mast_color)),
            [x, 0.5 * deck_y + 0.04, 0.0],
            id_quat(),
        )
    };

    // ---- Bow ornament (conditional on BowStyle) ---------------------------
    let bow_z = -deck_half_z - 0.10; // just past the deck front edge
    let bow_y = 0.5 * deck_y + 0.05;
    // Bevy `Cone` axis is +Y; rotate around X by -π/2 to point the
    // apex along -Z (forward).
    let bow_ornament: Option<Generator> = match vessel.bow_style {
        BowStyle::Spike => Some(prim(
            cone(
                0.06 * vessel.bow_scale,
                0.30 * vessel.bow_scale,
                12,
                brass_mat(metal_color),
            ),
            [0.0, bow_y + 0.05, bow_z],
            quat_xyzw(quat_x(-FRAC_PI_2)),
        )),
        BowStyle::Sphere => Some(prim(
            sphere(0.10 * vessel.bow_scale, 3, glow_mat(jewel_color)),
            [0.0, bow_y, bow_z],
            id_quat(),
        )),
        BowStyle::Beak => Some(prim(
            cone(
                0.10 * vessel.bow_scale,
                0.50 * vessel.bow_scale,
                12,
                brass_mat(metal_color),
            ),
            [0.0, bow_y, bow_z - 0.10],
            quat_xyzw(quat_x(-FRAC_PI_2)),
        )),
        BowStyle::None => None,
    };

    // ---- Smokestacks (Steam / Hybrid) -------------------------------------
    // Symmetric stern placement; per-vessel count, height jitter and
    // crown flare keep two Steam boats reading as distinct rigs.
    let mut smokestacks: Vec<Generator> = Vec::new();
    if vessel.archetype.has_smokestacks() && vessel.smokestack_count > 0 {
        // Fat, dark funnels: the old 0.055-radius stacks in the
        // hair-colour tone read as pale sticks from any distance.
        let stack_radius = 0.09 * limb;
        let stack_height = 0.40 * h * vessel.smokestack_height_scale;
        let stack_y = 0.5 * deck_y + stack_height * 0.5;
        let stack_z = deck_half_z - 0.30;
        let xs: &[f32] = match vessel.smokestack_count {
            1 => &[0.0],
            2 => &[-0.25, 0.25],
            _ => &[0.0, -0.30, 0.30],
        };
        for x in xs {
            smokestacks.push(prim(
                with_torture(
                    cylinder(stack_radius, stack_height, 12, funnel_mat(metal_color)),
                    0.0,
                    vessel.stack_flare,
                    [0.0, 0.0, 0.0],
                ),
                [*x * w, stack_y, stack_z],
                id_quat(),
            ));
        }
    }

    // ---- Solar panel (Solar / Hybrid) -------------------------------------
    let solar_panel: Option<Generator> = vessel.archetype.has_solar_panel().then(|| {
        prim(
            cuboid([0.65 * w, 0.03, 0.75 * h], brass_mat(metal_color)),
            [0.0, 0.5 * deck_y + 0.18, 0.25 * h],
            quat_xyzw(quat_x(vessel.solar_panel_tilt_rad)),
        )
    });

    // ---- Assemble the deck root and its children --------------------------
    let mut children: Vec<Generator> = Vec::with_capacity(10);
    children.extend(hulls);
    children.push(rail(-rail_x));
    children.push(rail(rail_x));
    if let Some(b) = bow_ornament {
        children.push(b);
    }
    children.push(mast);
    children.extend(smokestacks);
    if let Some(p) = solar_panel {
        children.push(p);
    }

    let mut deck = prim(
        cuboid([deck_x, deck_y, deck_z], plank_mat(deck_color)),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    deck.transform = Default::default();
    deck.children = children;
    deck
}
