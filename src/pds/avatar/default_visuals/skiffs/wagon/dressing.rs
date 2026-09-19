//! The wagon's ladder: secondary MASSES by ornateness and wear, the kind that
//! read at play distance, never trinkets (#1359 F6).
//!
//! - **Ornate**: a water cask on the near side on an iron strap (cart,
//!   buckboard). The hearse's urns and the chariot's pole finial are drawn by
//!   their own bodies, and the ox-cart's gilt ridge by its roof.
//! - **Worn**: a load in the bed - a crate and two sacks (cart, buckboard).
//!   A tilted cart hides a load under its canvas, so it wears a pale
//!   replacement board low on its near side instead.
//! - **Battered**: a spare wheel hung on the tailboard (cart, hearse). The
//!   buckboard has no tailboard to hang one on.
//!
//! Worn and Battered both carry the Worn mass, as the roadster's ladder does.

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::livery::WagonColours;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{OrnatenessTier, WagonBody, WearTier};

use super::super::super::common::{quat_x, quat_y, quat_z};
use super::super::dim;
use super::cart::tilted;
use super::running_gear::{WAGON_WHEEL, spoked_wheel};
use super::{FRONT_WHEEL, NEAR, WagonPlan, board, line, solid};

const UPRIGHT: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

pub(super) fn dress(
    kids: &mut Vec<Generator>,
    plan: &WagonPlan,
    c: &WagonColours,
    o: OrnatenessTier,
    w: WearTier,
) {
    let (l, hw) = (plan.length, plan.half_w());
    let (a, f) = plan.bed_z();
    let h = plan.depth();
    let loads = matches!(plan.body, WagonBody::Cart | WagonBody::Buckboard);
    let worn = w != WearTier::Pristine;
    if o == OrnatenessTier::Ornate && loads {
        let (r, ln) = (l * 0.040, l * 0.10);
        let z = (a + f) * 0.5;
        let cask = [
            (0.0, -ln * 0.5),
            (r * 0.86, -ln * 0.5),
            (r, 0.0),
            (r * 0.86, ln * 0.5),
            (0.0, ln * 0.5),
        ];
        kids.push(solid(
            &cask,
            16,
            true,
            &c.cask,
            [NEAR * (hw + r * 0.85), h * 0.45, z - l * 0.04],
            quat_x(FRAC_PI_2),
        ));
        let strap = dim(l * 0.005);
        kids.push(line(
            &[
                ([NEAR * (hw - l * 0.004), h * 0.9, z - l * 0.07], strap),
                (
                    [NEAR * (hw + r * 0.9), h * 0.45 + r * 0.98, z - l * 0.04],
                    strap,
                ),
                ([NEAR * (hw - l * 0.004), h * 0.9, z - l * 0.01], strap),
            ],
            5,
            &c.iron,
        ));
    }
    let under_canvas = plan.body == WagonBody::Cart && tilted(o);
    if under_canvas && worn {
        let th = dim(l * 0.012);
        kids.push(board(
            [th * 1.3, h * 0.50, (f - a) * 0.26],
            &c.patch,
            [NEAR * (hw - th * 0.1), h * 0.50, a + (f - a) * 0.52],
            UPRIGHT,
            th * 0.3,
        ));
    }
    if loads && !under_canvas && worn {
        let s = l * 0.075;
        kids.push(board(
            [s * 1.2, s, s],
            &c.cask,
            [-hw * 0.35, s * 0.5, a + l * 0.10],
            UPRIGHT,
            l * 0.004,
        ));
        let sack = [
            (0.0, -s * 0.45),
            (s * 0.40, -s * 0.40),
            (s * 0.48, 0.0),
            (s * 0.34, s * 0.42),
            (0.0, s * 0.5),
        ];
        // One sack stood up, one lying on its side.
        for (x, z, lying) in [
            (hw * 0.40, a + l * 0.09, false),
            (hw * 0.30, a + l * 0.19, true),
        ] {
            let turn = if lying { quat_z(FRAC_PI_2) } else { UPRIGHT };
            kids.push(solid(&sack, 10, true, &c.sack, [x, s * 0.40, z], turn));
        }
    }
    if w == WearTier::Battered && matches!(plan.body, WagonBody::Cart | WagonBody::Hearse) {
        // A spare front wheel hung flat against the tailboard, its axis
        // turned onto the machine's length.
        let r = plan.wheel_r * FRONT_WHEEL;
        let at = [0.0, h * 0.5 + r * 0.1, a - l * 0.024];
        spoked_wheel(kids, at, 0.0, quat_y(FRAC_PI_2), r, WAGON_WHEEL, c);
    }
}
