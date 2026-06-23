//! Beach umbrella — a Coastal-Resort prop. A striped canvas parasol on a
//! steel pole planted in a little disc of rippled sand. The beach furniture
//! that scatters the resort foreshore.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_y, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{AWNING_RED, AWNING_WHITE, SAND_TAN, STEEL_GREY, canvas, sand, steel};

pub struct BeachUmbrella;

impl CatalogueEntry for BeachUmbrella {
    fn slug(&self) -> &'static str {
        "beach_umbrella"
    }
    fn name(&self) -> &'static str {
        "Beach Umbrella"
    }
    fn description(&self) -> &'static str {
        "Striped canvas parasol on a steel pole in a patch of rippled sand."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.6,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Sand apron disc — the root.
        prim(
            solid(cylinder_tapered(1.6, 0.1, 20, 0.0, sand(SAND_TAN))),
            [0.0, 0.05, 0.0],
            id_quat(),
        ),
        // Anchor hub where the pole bites the sand.
        prim(
            solid(cylinder_tapered(0.18, 0.14, 12, 0.2, steel(STEEL_GREY))),
            [0.0, 0.14, 0.0],
            id_quat(),
        ),
        // Steel pole.
        prim(
            solid(cylinder_tapered(0.05, 2.6, 8, 0.0, steel(STEEL_GREY))),
            [0.0, 1.35, 0.0],
            id_quat(),
        ),
        // Striped canvas canopy (apex up).
        prim(
            cone(1.5, 0.7, 16, canvas(AWNING_RED, AWNING_WHITE)),
            [0.0, 2.6, 0.0],
            id_quat(),
        ),
        // Hanging valance rim around the canopy edge.
        prim(
            torus(0.07, 1.45, canvas(AWNING_RED, AWNING_WHITE)),
            [0.0, 2.3, 0.0],
            id_quat(),
        ),
        // Finial on the apex.
        prim(
            solid(sphere(0.1, 4, steel(STEEL_GREY))),
            [0.0, 3.02, 0.0],
            id_quat(),
        ),
    ];

    // Radial ribs spoking out under the canopy.
    for k in 0..8 {
        let ang = k as f32 * std::f32::consts::FRAC_PI_4;
        prims.push(prim(
            cuboid_tapered([0.05, 0.05, 1.3], 0.0, steel(STEEL_GREY)),
            [ang.sin() * 0.66, 2.42, ang.cos() * 0.66],
            quat_y(ang),
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
        assert_sanitize_stable(&BeachUmbrella.build(""), "beach_umbrella");
    }
}
