//! Radio mast — a Post-apocalyptic secondary. A tall scrap-lattice mast braced
//! with salvaged steel, an antenna rigged at the top and a blinking warning
//! light. The lifeline of the holdout; its light is emissive trim the ruin
//! pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the base.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, RUST_BROWN, SIGNAL_RED, STEEL_GREY, concrete, fx, rusted};

pub struct RadioMast;

impl CatalogueEntry for RadioMast {
    fn slug(&self) -> &'static str {
        "radio_mast"
    }
    fn name(&self) -> &'static str {
        "Radio Mast"
    }
    fn description(&self) -> &'static str {
        "Tall scrap-lattice mast with an antenna and a blinking warning light."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::PostApoc]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::POSTAPOC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 44.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.5_f32;
    let mast_h = 12.0_f32;
    let mast_top = base_h + mast_h;

    let mut prims = vec![
        // Concrete base — the root.
        prim(
            solid(cuboid_tapered(
                [2.0, base_h, 2.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Four tapering lattice legs.
    let spread = 0.9_f32;
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cylinder_tapered(0.1, mast_h, 6, 0.0, rusted(STEEL_GREY))),
            [sx * spread, base_h + mast_h * 0.5, sz * spread],
            id_quat(),
        ));
    }
    // Cross-braces at three heights.
    for h in [base_h + 3.0, base_h + 7.0, base_h + 10.5] {
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.06, 0.06, 2.0 * spread],
                    0.0,
                    rusted(STEEL_GREY),
                )),
                [sx * spread, h, 0.0],
                id_quat(),
            ));
        }
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [2.0 * spread, 0.06, 0.06],
                    0.0,
                    rusted(STEEL_GREY),
                )),
                [0.0, h, sz * spread],
                id_quat(),
            ));
        }
    }

    // Antenna whip + cross-element at the top.
    prims.push(prim(
        solid(cylinder_tapered(0.05, 3.0, 4, 0.0, rusted(RUST_BROWN))),
        [0.0, mast_top + 1.5, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.6, 0.06, 0.06], 0.0, rusted(RUST_BROWN))),
        [0.0, mast_top + 0.6, 0.0],
        id_quat(),
    ));
    // Blinking warning light — emissive.
    prims.push(prim(
        sphere(0.18, 3, glow(SIGNAL_RED, 3.0)),
        [0.0, mast_top + 3.1, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: desolate wind through the lattice.
    root.audio = fx::desolate_wind();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&RadioMast.build(""), "radio_mast");
    }
}
