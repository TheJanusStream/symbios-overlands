//! Town hall — the Civic/Campus landmark and the kit's lit hero. A
//! neoclassical stone hall behind a marble columned portico and pediment,
//! crowned by a verdigris copper dome lantern, its tall windows and flanking
//! lamps glowing over the steps. ~14 m wide, so it anchors the quarter and
//! reads as the seat of the town from across the home region. Its windows,
//! lamps and lit lantern are the trim escalation's ruin pass snuffs to a
//! dark, shuttered hall.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the base.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, foundation_block, glow, id_quat, prim, solid,
    sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CLOCK_LIT, CONCRETE_GREY, COPPER_VERDIGRIS, GLASS_TINT, LAMP_WARM, MARBLE_WHITE, STEEL_GREY,
    STONE_PALE, WINDOW_WARM, concrete, copper, fx, glass, marble, steel, stone,
};

pub struct TownHall;

impl CatalogueEntry for TownHall {
    fn slug(&self) -> &'static str {
        "town_hall"
    }
    fn name(&self) -> &'static str {
        "Town Hall"
    }
    fn description(&self) -> &'static str {
        "Neoclassical stone hall with a marble portico, copper dome lantern and lit windows."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CivicCampus]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CAMPUS_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 14.0,
            min_spawn_dist: 52.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.9_f32;
    let body_h = 6.0_f32;
    let body_z = -0.5_f32;
    let body_top = base_h + body_h;

    let mut prims = vec![
        // Marble stylobate base — the root.
        prim(
            solid(cuboid_tapered(
                [16.0, base_h, 12.0],
                0.0,
                marble(MARBLE_WHITE),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    prims.push(foundation_block(16.0, 12.0, [0.0, 0.0], 1.5));

    // Stone hall body.
    prims.push(prim(
        solid(cuboid_tapered([13.0, body_h, 9.0], 0.0, stone(STONE_PALE))),
        [0.0, base_h + body_h * 0.5, body_z],
        id_quat(),
    ));
    // Lit tall windows across the front behind the colonnade.
    prims.push(prim(
        cuboid_tapered([10.0, 2.8, 0.2], 0.0, glass(GLASS_TINT, 1.3)),
        [0.0, base_h + 2.6, 4.05],
        id_quat(),
    ));
    // Bronze entrance doors.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.2, 3.0, 0.3],
            0.0,
            copper(COPPER_VERDIGRIS),
        )),
        [0.0, base_h + 1.5, 4.1],
        id_quat(),
    ));

    // Front steps descending to the quad.
    for k in 0..3 {
        let kf = k as f32;
        prims.push(prim(
            solid(cuboid_tapered(
                [12.0 - kf * 0.4, 0.3, 1.0],
                0.0,
                marble(MARBLE_WHITE),
            )),
            [0.0, base_h - 0.15 - kf * 0.3, 5.6 + kf * 0.9],
            id_quat(),
        ));
    }

    // Marble colonnade across the front.
    for x in [-5.0_f32, -3.0, -1.0, 1.0, 3.0, 5.0] {
        prims.push(prim(
            solid(cylinder_tapered(
                0.5,
                body_h - 1.0,
                14,
                0.04,
                marble(MARBLE_WHITE),
            )),
            [x, base_h + (body_h - 1.0) * 0.5, 5.0],
            id_quat(),
        ));
    }
    // Entablature beam and pediment over the colonnade.
    prims.push(prim(
        solid(cuboid_tapered([12.5, 0.9, 1.4], 0.0, marble(MARBLE_WHITE))),
        [0.0, base_h + body_h - 0.5, 4.8],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([12.5, 1.8, 1.4], 0.85, marble(MARBLE_WHITE))),
        [0.0, base_h + body_h + 0.4, 4.8],
        id_quat(),
    ));

    // Roof slab + copper dome lantern with a lit cupola.
    prims.push(prim(
        solid(cuboid_tapered(
            [13.4, 0.4, 9.4],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, body_top + 0.2, body_z],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(
            2.0,
            1.6,
            16,
            0.06,
            copper(COPPER_VERDIGRIS),
        )),
        [0.0, body_top + 1.2, body_z],
        id_quat(),
    ));
    // Lit cupola ring — emissive.
    prims.push(prim(
        cuboid_tapered([3.0, 0.7, 3.0], 0.0, glow(WINDOW_WARM, 1.8)),
        [0.0, body_top + 1.4, body_z],
        id_quat(),
    ));
    prims.push(prim(
        solid(cone(2.2, 1.8, 16, copper(COPPER_VERDIGRIS))),
        [0.0, body_top + 2.9, body_z],
        id_quat(),
    ));
    // Gilt finial orb atop the dome.
    prims.push(prim(
        sphere(0.3, 3, glow(CLOCK_LIT, 2.0)),
        [0.0, body_top + 4.0, body_z],
        id_quat(),
    ));

    // Flanking entrance lamps on steel posts — emissive globes.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.1, 2.2, 8, 0.0, steel(STEEL_GREY))),
            [sx * 5.5, base_h + 1.1, 6.0],
            id_quat(),
        ));
        prims.push(prim(
            sphere(0.3, 3, glow(LAMP_WARM, 3.0)),
            [sx * 5.5, base_h + 2.4, 6.0],
            id_quat(),
        ));
    }

    let mut root = assemble(prims);
    // Signature life: a calm quad bed and drifting seed-fluff out front.
    root.audio = fx::campus_calm();
    root.children
        .push(fx::seed_drift([0.0, 1.5, 9.0], 0x0C1F_5A11));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&TownHall.build(""), "town_hall");
    }

    #[test]
    fn has_lit_windows_and_lamps() {
        assert!(super::super::has_emissive(&TownHall.build("")));
    }
}
