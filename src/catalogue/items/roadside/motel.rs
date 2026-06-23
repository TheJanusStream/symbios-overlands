//! Motel — a Roadside secondary. A single-storey strip of rooms under a
//! corrugated walkway roof, doors and lit windows marching down the front,
//! with a tall neon MOTEL pylon and a red VACANCY sign at the corner.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the slab.

use crate::catalogue::items::modern_city::curtain_wall;
use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRICK_TAN, CONCRETE_GREY, CORRUGATED_GREY, ENAMEL_BLUE, ENAMEL_CREAM, GLASS_TINT, NEON_CYAN,
    NEON_RED, SIGN_AMBER, STEEL_GREY, brick, concrete, corrugated, enamel, fx, glass, sign_board,
    steel,
};

pub struct Motel;

impl CatalogueEntry for Motel {
    fn slug(&self) -> &'static str {
        "motel"
    }
    fn name(&self) -> &'static str {
        "Motel"
    }
    fn description(&self) -> &'static str {
        "Single-storey room strip under a walkway roof with a neon MOTEL pylon."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Roadside]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ROADSIDE_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let slab_h = 0.3_f32;
    let body_h = 3.0_f32;
    let body_y = slab_h + body_h * 0.5;
    let roof_top = slab_h + body_h;
    // Room block sits back at +Z so its doors face the −Z camera front.
    let front = -2.0_f32;

    let mut prims = vec![
        // Concrete slab — the root.
        prim(
            solid(cuboid_tapered(
                [16.0, slab_h, 7.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, slab_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Brick room block, set back so the doors face the front.
    prims.push(prim(
        solid(cuboid_tapered([14.0, body_h, 5.0], 0.0, brick(BRICK_TAN))),
        [0.0, body_y, 0.5],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [14.4, 0.4, 5.4],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, roof_top + 0.2, 0.5],
        id_quat(),
    ));

    // Doors + lit windows + numbered plaques marching down the −Z front.
    for k in 0..4 {
        let x = -6.0 + k as f32 * 3.0;
        prims.push(prim(
            solid(cuboid_tapered([1.0, 2.0, 0.14], 0.0, enamel(ENAMEL_BLUE))),
            [x - 0.6, slab_h + 1.0, front - 0.1],
            id_quat(),
        ));
        // Lit door-number plaque.
        prims.push(prim(
            cuboid_tapered([0.32, 0.22, 0.06], 0.0, glow(SIGN_AMBER, 1.8)),
            [x - 0.6, slab_h + 2.15, front - 0.16],
            id_quat(),
        ));
        // Lit window with a proud frame.
        prims.push(prim(
            solid(cuboid_tapered([1.4, 1.3, 0.12], 0.0, enamel(ENAMEL_CREAM))),
            [x + 0.7, slab_h + 1.5, front - 0.06],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([1.2, 1.1, 0.12], 0.0, glass(GLASS_TINT, 1.4)),
            [x + 0.7, slab_h + 1.5, front - 0.14],
            id_quat(),
        ));
    }

    // Glazed office bay at the +X end with a lit OFFICE sign.
    let office_x = 5.6_f32;
    for g in curtain_wall(
        [office_x, slab_h + 1.4, front - 0.22],
        [2.6, 1.8],
        (2, 1),
        -0.2,
        glass(GLASS_TINT, 1.6),
        steel(STEEL_GREY),
    ) {
        prims.push(g);
    }
    for g in sign_board(
        [office_x, slab_h + 2.7, front - 0.32],
        [2.4, 0.5],
        (3, 1),
        SIGN_AMBER,
        2.0,
        -1.0,
    ) {
        prims.push(g);
    }

    // Corrugated walkway roof on steel posts, projecting toward −Z.
    prims.push(prim(
        solid(cuboid_tapered(
            [15.0, 0.25, 2.6],
            0.0,
            corrugated(CORRUGATED_GREY),
        )),
        [0.0, roof_top + 0.05, front - 1.1],
        id_quat(),
    ));
    for x in [-6.0_f32, -3.0, 0.0, 3.0, 6.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.16, body_h, 0.16], 0.0, steel(STEEL_GREY))),
            [x, body_y, front - 2.1],
            id_quat(),
        ));
    }

    // Neon MOTEL pylon + red VACANCY sign at the −Z front corner.
    let px = -8.5_f32;
    let pz = -3.0_f32;
    prims.push(prim(
        solid(cuboid_tapered([0.35, 6.0, 0.35], 0.0, steel(STEEL_GREY))),
        [px, slab_h + 3.0, pz],
        id_quat(),
    ));
    // Cream backing blade, broad face toward the −Z road.
    prims.push(prim(
        solid(cuboid_tapered([1.7, 3.4, 0.3], 0.0, enamel(ENAMEL_CREAM))),
        [px, slab_h + 5.6, pz],
        id_quat(),
    ));
    // Stacked-letter MOTEL neon (cyan), proud of the blade, facing −Z.
    let mut motel = sign_board(
        [px, slab_h + 5.8, pz - 0.35],
        [1.3, 2.6],
        (1, 5),
        NEON_CYAN,
        2.4,
        -1.0,
    );
    motel[1].audio = fx::neon_buzz();
    prims.extend(motel);
    // Red VACANCY bar below.
    for g in sign_board(
        [px, slab_h + 3.9, pz - 0.35],
        [1.5, 0.6],
        (3, 1),
        NEON_RED,
        2.4,
        -1.0,
    ) {
        prims.push(g);
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Motel.build(""), "motel");
    }
}
