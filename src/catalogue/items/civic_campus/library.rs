//! Library — a Civic/Campus secondary. A stone reading hall behind a small
//! marble colonnade, tall lit windows down its front and a balustraded flat
//! roof. The scholarly heart of the quad.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the base.

use crate::catalogue::items::modern_city::curtain_wall;
use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    COPPER_VERDIGRIS, GLASS_TINT, MARBLE_WHITE, STONE_PALE, column, copper, glass, marble, stone,
};

pub struct Library;

impl CatalogueEntry for Library {
    fn slug(&self) -> &'static str {
        "library"
    }
    fn name(&self) -> &'static str {
        "Library"
    }
    fn description(&self) -> &'static str {
        "Stone reading hall with a marble colonnade and tall lit windows."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CivicCampus]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CAMPUS_BAND
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
    let base_h = 0.6_f32;
    let body_h = 5.0_f32;
    let body_top = base_h + body_h;
    // Colonnade, windows, doors and steps face the -Z render front; the body
    // sits a touch back so the portico has room.
    let fz = -1.0_f32;

    let mut prims = vec![
        // Stone base — the root.
        prim(
            solid(cuboid_tapered(
                [12.0, base_h, 8.0],
                0.0,
                marble(MARBLE_WHITE),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Stone body.
    prims.push(prim(
        solid(cuboid_tapered([10.0, body_h, 6.5], 0.0, stone(STONE_PALE))),
        [0.0, base_h + body_h * 0.5, 0.3],
        id_quat(),
    ));
    // Tall lit windows across the front — a marble-mullioned glazed grid set
    // into the -2.95 front wall.
    prims.extend(curtain_wall(
        [0.0, base_h + 2.5, fz * 2.95],
        [8.0, 3.4],
        (5, 4),
        fz * 0.18,
        glass(GLASS_TINT, 1.2),
        marble(MARBLE_WHITE),
    ));
    // Bronze doors.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.8, 2.6, 0.3],
            0.0,
            copper(COPPER_VERDIGRIS),
        )),
        [0.0, base_h + 1.3, fz * 3.12],
        id_quat(),
    ));

    // Four marble columns across the front.
    for x in [-3.0_f32, -1.0, 1.0, 3.0] {
        prims.extend(column(
            x,
            fz * 3.4,
            base_h,
            body_h - 0.5,
            0.4,
            marble(MARBLE_WHITE),
        ));
    }
    // Entablature beam over the colonnade.
    prims.push(prim(
        solid(cuboid_tapered([9.0, 0.7, 1.0], 0.0, marble(MARBLE_WHITE))),
        [0.0, body_top - 0.35, fz * 3.3],
        id_quat(),
    ));

    // Roof parapet with a balustrade along the front edge.
    prims.push(prim(
        solid(cuboid_tapered([10.4, 0.5, 6.9], 0.0, marble(MARBLE_WHITE))),
        [0.0, body_top + 0.25, 0.3],
        id_quat(),
    ));
    // Balusters threaded on a top rail and a bottom rail (no coplanar plinth).
    for rail_y in [body_top + 0.55_f32, body_top + 1.05] {
        prims.push(prim(
            solid(cuboid_tapered([8.6, 0.12, 0.22], 0.0, marble(MARBLE_WHITE))),
            [0.0, rail_y, fz * 3.05],
            id_quat(),
        ));
    }
    for x in [-4.0_f32, -2.0, 0.0, 2.0, 4.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.26, 0.5, 0.26], 0.3, marble(MARBLE_WHITE))),
            [x, body_top + 0.8, fz * 3.05],
            id_quat(),
        ));
    }

    // Two front steps, each lower and further out so no treads are coplanar.
    for k in 0..2 {
        let kf = k as f32;
        prims.push(prim(
            solid(cuboid_tapered([9.0, 0.3, 0.9], 0.0, marble(MARBLE_WHITE))),
            [0.0, base_h - 0.2 - kf * 0.3, fz * (4.2 + kf * 0.8)],
            id_quat(),
        ));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Library.build(""), "library");
    }
}
