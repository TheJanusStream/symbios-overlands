//! Lifeguard tower — a Coastal-Resort secondary. A plank lookout cabin
//! hoisted on four braced posts above the sand, with a lit observation
//! window, a red rescue cross on its flank, a warm eave lamp and a pennant
//! on the roof. A boarding ramp runs down to the beach.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the cabin deck.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    AWNING_RED, AWNING_WHITE, BUOY_RED, DECK_PALE, DECK_WOOD, GLASS_AQUA, LAMP_WARM, STEEL_GREY,
    canvas, enamel, glass, plank, steel,
};

pub struct LifeguardTower;

impl CatalogueEntry for LifeguardTower {
    fn slug(&self) -> &'static str {
        "lifeguard_tower"
    }
    fn name(&self) -> &'static str {
        "Lifeguard Tower"
    }
    fn description(&self) -> &'static str {
        "Raised plank lookout cabin with a rescue cross, eave lamp and rooftop pennant."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.0,
            min_spawn_dist: 28.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let deck_y = 3.2_f32;
    let cabin_h = 1.8_f32;
    let cabin_y = deck_y + 0.15 + cabin_h * 0.5;
    let roof_y = deck_y + 0.15 + cabin_h + 0.2;

    let mut prims = vec![
        // Plank deck — the root, raised on the posts.
        prim(
            solid(cuboid_tapered([3.0, 0.3, 3.0], 0.0, plank(DECK_PALE))),
            [0.0, deck_y, 0.0],
            id_quat(),
        ),
    ];

    // Four braced posts.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cylinder_tapered(0.14, deck_y, 8, 0.0, plank(DECK_WOOD))),
                [sx * 1.2, deck_y * 0.5, sz * 1.2],
                id_quat(),
            ));
        }
    }
    // Cross-braces at mid height.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([2.4, 0.1, 0.1], 0.0, plank(DECK_WOOD))),
            [0.0, deck_y * 0.5, sz * 1.2],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, 0.1, 2.4], 0.0, plank(DECK_WOOD))),
            [sx * 1.2, deck_y * 0.5, 0.0],
            id_quat(),
        ));
    }

    // Cabin box with a lit observation window facing +Z.
    prims.push(prim(
        solid(cuboid_tapered([3.0, cabin_h, 2.6], 0.0, plank(DECK_PALE))),
        [0.0, cabin_y, -0.2],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([2.4, 1.1, 0.15], 0.0, glass(GLASS_AQUA, 1.0)),
        [0.0, cabin_y + 0.1, 1.1],
        id_quat(),
    ));

    // Slanted shed roof.
    prims.push(prim(
        solid(cuboid_tapered([3.4, 0.3, 3.0], 0.0, plank(DECK_WOOD))),
        [0.0, roof_y, -0.1],
        quat_x(0.22),
    ));

    // Red rescue cross on the +X flank (two crossing enamel bars).
    prims.push(prim(
        cuboid_tapered([0.05, 0.85, 0.26], 0.0, enamel(BUOY_RED)),
        [1.53, cabin_y, -0.2],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.05, 0.26, 0.85], 0.0, enamel(BUOY_RED)),
        [1.53, cabin_y, -0.2],
        id_quat(),
    ));

    // Warm eave lamp — the tower's emissive trim.
    prims.push(prim(
        cuboid_tapered([0.3, 0.3, 0.2], 0.0, glow(LAMP_WARM, 2.5)),
        [0.0, deck_y + 0.15 + cabin_h, 1.3],
        id_quat(),
    ));

    // Rooftop pennant on a short steel pole.
    prims.push(prim(
        solid(cylinder_tapered(0.05, 1.4, 6, 0.0, steel(STEEL_GREY))),
        [1.2, roof_y + 0.9, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.9, 0.6, 0.04], 0.0, canvas(AWNING_RED, AWNING_WHITE)),
        [1.6, roof_y + 1.3, 0.0],
        id_quat(),
    ));

    // Boarding ramp down to the sand off the +Z side.
    prims.push(prim(
        solid(cuboid_tapered([1.2, 0.2, 3.4], 0.0, plank(DECK_WOOD))),
        [0.0, deck_y * 0.5, 2.4],
        quat_x(0.95),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&LifeguardTower.build(""), "lifeguard_tower");
    }
}
