//! Kindling pile — a Medieval *poor* prop. Tied faggot bundles of brushwood
//! leaning against a chopping block with a felling axe buried in it, split
//! log rounds waiting their turn, and a few loose sticks at the foot: the
//! gathered winter fuel of a cottar's yard.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_z, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{IRON_DARK, WOOD_DARK, WOOD_OAK, iron, timber};

pub struct KindlingPile;

impl CatalogueEntry for KindlingPile {
    fn slug(&self) -> &'static str {
        "kindling_pile"
    }
    fn name(&self) -> &'static str {
        "Kindling Pile"
    }
    fn description(&self) -> &'static str {
        "Tied faggot bundles leaning on a chopping block with a buried axe and split log rounds."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Medieval]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MEDIEVAL_POOR
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

/// A tied faggot of brushwood at `center`, leaning by `lean` (rotation about
/// X); two binding withies are children in its local frame.
fn faggot(center: [f32; 3], lean: f32, len: f32) -> Generator {
    let mut f = prim(
        solid(cylinder_tapered(0.22, len, 7, 0.1, timber(WOOD_DARK))),
        center,
        quat_x(lean),
    );
    for dy in [len * 0.27, -len * 0.27] {
        f.children.push(prim(
            torus(0.03, 0.23, iron(IRON_DARK)),
            [0.0, dy, 0.0],
            id_quat(),
        ));
    }
    f
}

fn build_tree() -> Generator {
    // Chopping block — the root.
    let mut prims = vec![prim(
        solid(cylinder_tapered(0.4, 0.7, 12, 0.05, timber(WOOD_OAK))),
        [0.8, 0.35, 0.0],
        id_quat(),
    )];

    // Three faggot bundles of varying length leaning together.
    prims.push(faggot([-0.4, 0.75, -0.35], 0.1, 1.5));
    prims.push(faggot([-0.6, 0.7, 0.25], -0.12, 1.3));
    prims.push(faggot([-0.12, 0.78, 0.0], 0.05, 1.6));

    // Felling axe buried in the block: an oak haft and a steel head.
    prims.push(prim(
        solid(cylinder_tapered(0.04, 0.9, 6, 0.0, timber(WOOD_OAK))),
        [0.8, 0.95, 0.05],
        quat_x(0.32),
    ));
    // Wedge-shaped steel head seated on the block top.
    prims.push(prim(
        solid(cuboid_tapered([0.1, 0.22, 0.34], 0.45, iron(IRON_DARK))),
        [0.8, 0.74, -0.12],
        quat_x(-0.5),
    ));

    // Two split log rounds waiting to be chopped.
    prims.push(prim(
        solid(cylinder_tapered(0.26, 0.5, 10, 0.0, timber(WOOD_OAK))),
        [0.95, 0.25, 0.75],
        quat_z(FRAC_PI_2),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.24, 0.46, 10, 0.0, timber(WOOD_DARK))),
        [1.35, 0.24, 0.5],
        id_quat(),
    ));

    // A few loose split sticks at the foot, lying along Z.
    for (sx, sz, sy) in [
        (0.2_f32, -0.6_f32, 0.08_f32),
        (0.35, 0.55, 0.08),
        (0.1, 0.0, 0.2),
    ] {
        prims.push(prim(
            solid(cylinder_tapered(0.06, 1.0, 6, 0.0, timber(WOOD_DARK))),
            [sx, sy, sz],
            quat_x(FRAC_PI_2),
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
        assert_sanitize_stable(&KindlingPile.build(""), "kindling_pile");
    }
}
