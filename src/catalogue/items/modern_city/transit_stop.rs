//! Transit stop — a Modern-City secondary. A raised concrete platform under
//! a steel-and-glass canopy, with benches and a lit sign pylon: the light-
//! rail / bus interchange that anchors the street grid.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, GLASS_TEAL, SIGNAL_GREEN, STEEL_GREY, concrete, glass, steel};

pub struct TransitStop;

impl CatalogueEntry for TransitStop {
    fn slug(&self) -> &'static str {
        "transit_stop"
    }
    fn name(&self) -> &'static str {
        "Transit Stop"
    }
    fn description(&self) -> &'static str {
        "Raised platform under a glass canopy with benches and a lit sign pylon."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CITY_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let plat_h = 0.6;
    let post_h = 3.0;
    let canopy_y = plat_h + post_h;

    let mut prims = vec![
        // Raised concrete platform — the root.
        prim(
            solid(cuboid_tapered(
                [9.0, plat_h, 3.2],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, plat_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Canopy posts.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.22, post_h, 0.22], 0.0, steel(STEEL_GREY))),
                [sx * 3.6, plat_h + post_h * 0.5, sz * 1.1],
                id_quat(),
            ));
        }
    }
    // Glass-and-steel canopy roof.
    prims.push(prim(
        solid(cuboid_tapered([8.4, 0.2, 3.4], 0.0, steel(STEEL_GREY))),
        [0.0, canopy_y + 0.1, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([7.6, 0.08, 2.8], 0.0, glass(GLASS_TEAL, 0.0)),
        [0.0, canopy_y + 0.02, 0.0],
        id_quat(),
    ));

    // Two benches.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [2.4, 0.12, 0.6],
                0.0,
                steel([0.4, 0.42, 0.45]),
            )),
            [sx * 2.0, plat_h + 0.5, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered(
                [2.4, 0.5, 0.12],
                0.0,
                steel([0.4, 0.42, 0.45]),
            )),
            [sx * 2.0, plat_h + 0.75, -0.3],
            id_quat(),
        ));
    }

    // Lit sign pylon at one end.
    prims.push(prim(
        solid(cuboid_tapered([0.3, 4.2, 0.3], 0.0, steel(STEEL_GREY))),
        [4.3, plat_h + 2.1, 1.2],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([1.4, 0.9, 0.12], 0.0, glow(SIGNAL_GREEN, 2.5)),
        [4.3, plat_h + 3.6, 1.2],
        id_quat(),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&TransitStop.build(""), "transit_stop");
    }
}
