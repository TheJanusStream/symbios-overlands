//! Barn — the Rural/Farmland landmark. The classic red barn: board-and-batten
//! siding on a fieldstone foundation under a gambrel roof, with big white-
//! trimmed doors, a hayloft and hoist beam, a lit window, and a cupola with a
//! weathervane. Chaff drifts off the loft on the golden-hour air. It anchors
//! the farmstead and reads as the red barn across the home region.

use crate::catalogue::items::util::{assemble, cone, cuboid_tapered, id_quat, prim, quat_z, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BARN_RED, LAMP_WARM, ROOF_GREY, STONE_GREY, TRIM_WHITE, barn_board, enamel, fx, glass,
    metal_roof, stone,
};

pub struct Barn;

impl CatalogueEntry for Barn {
    fn slug(&self) -> &'static str {
        "barn"
    }
    fn name(&self) -> &'static str {
        "Barn"
    }
    fn description(&self) -> &'static str {
        "Classic red gambrel-roofed barn with white-trimmed doors and a cupola."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::RuralFarmland]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FARM_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 13.0,
            min_spawn_dist: 45.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let l = 14.0_f32;
    let w = 10.0_f32;
    let foot_h = 0.6;
    let wall_h = 6.0;
    let wall_top = foot_h + wall_h;
    let front = w * 0.5;

    let mut prims = vec![
        // Fieldstone foundation — the root.
        prim(
            solid(cuboid_tapered(
                [l + 1.0, foot_h, w + 1.0],
                0.0,
                stone(STONE_GREY),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
        // Red board-and-batten body.
        prim(
            solid(cuboid_tapered([l, wall_h, w], 0.0, barn_board(BARN_RED))),
            [0.0, foot_h + wall_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // White corner trim.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            cuboid_tapered([0.3, wall_h, 0.3], 0.0, barn_board(TRIM_WHITE)),
            [sx * l * 0.5, foot_h + wall_h * 0.5, sz * w * 0.5],
            id_quat(),
        ));
    }

    // Gambrel roof: a steep lower band and a shallow upper peak.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 1.0, 2.0, w + 1.5],
            0.22,
            metal_roof(ROOF_GREY),
        )),
        [0.0, wall_top + 1.0, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 1.0, 2.2, w - 1.5],
            0.6,
            metal_roof(ROOF_GREY),
        )),
        [0.0, wall_top + 2.0 + 1.1, 0.0],
        id_quat(),
    ));
    let ridge = wall_top + 4.2;

    // Big white-trimmed sliding barn doors on the −Z front (the camera face),
    // each Z-braced with battens. `f` is the hero face; proud trim sits a touch
    // further toward −Z so nothing is coplanar with the door leaf.
    let f = -front;
    // Sliding-door track rail spanning both leaves.
    prims.push(prim(
        solid(cuboid_tapered(
            [7.0, 0.2, 0.12],
            0.0,
            barn_board(TRIM_WHITE),
        )),
        [0.0, foot_h + 5.0, f - 0.18],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        // Door leaf.
        prims.push(prim(
            solid(cuboid_tapered(
                [3.0, 4.6, 0.2],
                0.0,
                barn_board([0.42, 0.10, 0.08]),
            )),
            [sx * 1.6, foot_h + 2.3, f],
            id_quat(),
        ));
        // Top / mid / bottom horizontal battens.
        for ty in [0.4_f32, 2.3, 4.2] {
            prims.push(prim(
                cuboid_tapered([3.0, 0.22, 0.06], 0.0, barn_board(TRIM_WHITE)),
                [sx * 1.6, foot_h + ty, f - 0.12],
                id_quat(),
            ));
        }
        // Mirrored diagonal brace → the classic barn-door Z-brace.
        prims.push(prim(
            cuboid_tapered([5.0, 0.2, 0.05], 0.0, barn_board(TRIM_WHITE)),
            [sx * 1.6, foot_h + 2.3, f - 0.15],
            quat_z(sx * 0.92),
        ));
    }
    // White door frame surround.
    prims.push(prim(
        cuboid_tapered([6.6, 0.3, 0.1], 0.0, barn_board(TRIM_WHITE)),
        [0.0, foot_h + 4.75, f - 0.05],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.3, 4.8, 0.1], 0.0, barn_board(TRIM_WHITE)),
            [sx * 3.25, foot_h + 2.4, f - 0.05],
            id_quat(),
        ));
    }

    // Hayloft door and hoist beam up in the gable, on the −Z front.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0, 2.0, 0.2],
            0.0,
            barn_board([0.42, 0.10, 0.08]),
        )),
        [0.0, wall_top + 0.6, f],
        id_quat(),
    ));
    for ty in [0.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([2.0, 0.18, 0.06], 0.0, barn_board(TRIM_WHITE)),
            [0.0, wall_top + 0.1 + ty, f - 0.12],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([0.3, 0.3, 1.8], 0.0, barn_board(TRIM_WHITE))),
        [0.0, wall_top + 1.8, f - 0.9],
        id_quat(),
    ));

    // Lit window on the side — the emissive trim.
    prims.push(prim(
        cuboid_tapered([1.3, 1.3, 0.2], 0.0, glass(LAMP_WARM, 3.0)),
        [-l * 0.5 - 0.05, foot_h + 3.5, -1.5],
        id_quat(),
    ));
    // Two warm-lit windows flanking the doors on the −Z front, with white
    // surrounds, so the lit barn reads head-on (the side window can fall
    // outside the contact-sheet angles).
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([1.7, 1.9, 0.1], 0.0, barn_board(TRIM_WHITE)),
            [sx * 5.0, foot_h + 3.2, f - 0.04],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([1.4, 1.6, 0.2], 0.0, glass(LAMP_WARM, 2.6)),
            [sx * 5.0, foot_h + 3.2, f],
            id_quat(),
        ));
    }

    // Cupola with a weathervane on the ridge.
    prims.push(prim(
        solid(cuboid_tapered([1.3, 1.3, 1.3], 0.0, barn_board(TRIM_WHITE))),
        [0.0, ridge + 0.5, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cone(1.0, 0.9, 4, metal_roof(ROOF_GREY))),
        [0.0, ridge + 1.4, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.06, 1.2, 0.06],
            0.0,
            enamel([0.2, 0.2, 0.22]),
        )),
        [0.0, ridge + 2.4, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([1.0, 0.18, 0.05], 0.3, enamel([0.2, 0.2, 0.22])),
        [0.15, ridge + 2.9, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: chaff drifting off the hayloft.
    root.children
        .push(fx::chaff_drift([0.0, wall_top + 0.8, f - 1.5], 0xC4AF_DA11));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Barn.build(""), "barn");
    }

    #[test]
    fn has_lamp() {
        assert!(crate::catalogue::items::util::has_emissive(&Barn.build("")));
    }
}
