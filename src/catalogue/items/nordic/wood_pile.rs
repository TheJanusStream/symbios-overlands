//! Wood pile — a Nordic *poor* prop. A neat stack of split logs laid
//! end-out, their sawn faces showing the growth rings, beside a chopping
//! stump with the axe still buried in it: the winter fuel of a croft.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cone, cylinder_tapered, id_quat, plane, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{IRON_DARK, WOOD_DARK, WOOD_WARM, iron, log_end, timber};

pub struct WoodPile;

impl CatalogueEntry for WoodPile {
    fn slug(&self) -> &'static str {
        "wood_pile"
    }
    fn name(&self) -> &'static str {
        "Wood Pile"
    }
    fn description(&self) -> &'static str {
        "Stacked split firewood laid end-out beside a chopping stump with a buried axe."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Nordic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::NORDIC_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // The log bodies are timber; the growth rings are a separate alpha card
    // laid on the sawn face (#940). `log_end` masks away everything outside
    // its round slice, so wrapping it around a cylinder erases the barrel —
    // it only works on a flat quad. Only the -Z faces are capped: the pile
    // is laid end-out, so the far ends are never in shot, and a second card
    // per log would double the prop's node count for nothing.
    const LOG_R: f32 = 0.15;
    const LOG_LEN: f32 = 0.85;
    // Card sits just proud of the sawn face so it cannot z-fight the cap.
    let face_z = -(LOG_LEN * 0.5) - 0.005;

    // Chopping stump (root), with its sawn top capped the same way.
    let stump_r = 0.44;
    let stump_h = 0.72;
    let mut prims = vec![
        prim(
            solid(cylinder_tapered(
                stump_r,
                stump_h,
                12,
                0.05,
                timber(WOOD_WARM),
            )),
            [1.35, stump_h * 0.5, 0.0],
            id_quat(),
        ),
        // Stump top faces +Y, which is the plane's own normal — no rotation.
        // The taper narrows the top, so the card matches the smaller radius.
        prim(
            plane(
                [stump_r * 2.0 * 0.95, stump_r * 2.0 * 0.95],
                log_end(WOOD_WARM),
            ),
            [1.35, stump_h + 0.005, 0.0],
            id_quat(),
        ),
    ];

    // Stacked split logs laid horizontally with their ring-faces out toward
    // the -Z front. A grid in X (across) and Y (up), each row nudged.
    let cols = 5;
    let rows = 4;
    for r in 0..rows {
        let y = 0.17 + r as f32 * 0.3;
        let shove = if r % 2 == 0 { 0.0 } else { 0.15 };
        let n = if r % 2 == 0 { cols } else { cols - 1 };
        for c in 0..n {
            let x = -1.35 + c as f32 * 0.3 + shove;
            let tone = if (r + c) % 2 == 0 {
                WOOD_WARM
            } else {
                WOOD_DARK
            };
            prims.push(prim(
                solid(cylinder_tapered(LOG_R, LOG_LEN, 10, 0.0, timber(tone))),
                [x, y, 0.0],
                quat_x(FRAC_PI_2),
            ));
            // Ring face on the -Z end. `quat_x(-FRAC_PI_2)` turns the
            // plane's +Y normal to face -Z.
            prims.push(prim(
                plane([LOG_R * 2.0, LOG_R * 2.0], log_end(tone)),
                [x, y, face_z],
                quat_x(-FRAC_PI_2),
            ));
        }
    }

    // Axe buried in the stump: a leaning haft with the iron head at its
    // lower end, half-sunk into the sawn top.
    //
    // The head has to be placed ON the haft's axis, which is easy to get
    // wrong by eye: `quat_x(HAFT_TILT)` swings the haft's +Y toward +Z, so
    // its *lower* end travels to negative Z. The head used to sit at
    // `z = +0.2` — the opposite side of the stump from the end it belongs
    // to — which went unnoticed while the mis-masked stump was see-through
    // (#940). Deriving the position from the tilt keeps the two joined.
    const HAFT_TILT: f32 = 0.35;
    const HAFT_LEN: f32 = 1.0;
    let haft_mid_y = 1.0;
    let (sin_t, cos_t) = HAFT_TILT.sin_cos();
    // Walk down the haft axis from its midpoint to just under the stump top,
    // so the head straddles the surface instead of vanishing inside it.
    let drop = (haft_mid_y - (stump_h - 0.06)) / cos_t;
    prims.push(prim(
        solid(cylinder_tapered(0.04, HAFT_LEN, 6, 0.0, timber(WOOD_WARM))),
        [1.35, haft_mid_y, 0.0],
        quat_x(HAFT_TILT),
    ));
    prims.push(prim(
        solid(cone(0.1, 0.3, 6, iron(IRON_DARK))),
        [1.35, haft_mid_y - drop * cos_t, -drop * sin_t],
        quat_x(HAFT_TILT),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&WoodPile.build(""), "wood_pile");
    }
}
