//! Bonsai — a Feudal-Japan prop. A miniature tree trained in a shallow
//! glazed pot: a gnarled, leaning trunk holding two cloud-pruned foliage
//! pads. A small touch of cultivated nature on a veranda or garden wall.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, prim_scaled, quat_x, quat_y, solid,
    sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{TIMBER_BROWN, TIMBER_DARK, rough_stone, timber};

/// Cloud-pruned foliage green.
const FOLIAGE_GREEN: [f32; 3] = [0.20, 0.38, 0.18];
/// Warm glazed-ceramic pot — light enough to read apart from the dark soil.
const POT_GLAZE: [f32; 3] = [0.34, 0.26, 0.22];

pub struct Bonsai;

impl CatalogueEntry for Bonsai {
    fn slug(&self) -> &'static str {
        "bonsai"
    }
    fn name(&self) -> &'static str {
        "Bonsai"
    }
    fn description(&self) -> &'static str {
        "Miniature trained tree in a shallow glazed pot."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::FeudalJapan]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FEUDAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.8,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Shallow glazed tray pot — the root.
        prim(
            solid(cuboid_tapered(
                [0.9, 0.2, 0.58],
                0.12,
                rough_stone(POT_GLAZE),
            )),
            [0.0, 0.1, 0.0],
            id_quat(),
        ),
        // Flared rim lip of the tray.
        prim(
            solid(cuboid_tapered(
                [0.96, 0.06, 0.64],
                0.0,
                rough_stone(POT_GLAZE),
            )),
            [0.0, 0.19, 0.0],
            id_quat(),
        ),
        // Dark soil mounded just above the rim.
        prim(
            cuboid_tapered([0.78, 0.07, 0.46], 0.15, timber(TIMBER_DARK)),
            [0.0, 0.24, 0.0],
            id_quat(),
        ),
    ];

    // Gnarled leaning trunk in two segments.
    prims.push(prim(
        solid(cylinder_tapered(0.07, 0.55, 6, 0.2, timber(TIMBER_BROWN))),
        [0.0, 0.6, 0.0],
        quat_x(0.18),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.05, 0.45, 6, 0.3, timber(TIMBER_BROWN))),
        [0.12, 1.0, 0.05],
        quat_x(-0.3),
    ));
    // A side branch.
    prims.push(prim(
        solid(cylinder_tapered(0.04, 0.4, 6, 0.2, timber(TIMBER_BROWN))),
        [-0.18, 0.85, 0.0],
        quat_y(1.2),
    ));

    // Cloud-pruned foliage pads — flattened ellipsoids, not round balls.
    prims.push(prim_scaled(
        sphere(0.36, 3, timber(FOLIAGE_GREEN)),
        [0.2, 1.28, 0.05],
        id_quat(),
        [1.3, 0.5, 1.3],
    ));
    prims.push(prim_scaled(
        sphere(0.28, 3, timber(FOLIAGE_GREEN)),
        [-0.28, 1.02, 0.0],
        id_quat(),
        [1.35, 0.45, 1.35],
    ));
    prims.push(prim_scaled(
        sphere(0.22, 3, timber(FOLIAGE_GREEN)),
        [0.04, 1.52, 0.02],
        id_quat(),
        [1.3, 0.45, 1.3],
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Bonsai.build(""), "bonsai");
    }
}
