//! Kindling pile — a Medieval *poor* prop. Tied faggot bundles of brushwood
//! leaning against a chopping block with a billhook buried in it, and a few
//! loose sticks at the foot: the gathered winter fuel of a cottar's yard.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cone, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
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
        "Tied faggot bundles leaning on a chopping block with a buried billhook."
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

/// A tied faggot of brushwood at `center`, leaning by `lean` (rotation
/// about X); the binding withy is a child in its local frame.
fn faggot(center: [f32; 3], lean: f32) -> Generator {
    let mut f = prim(
        solid(cylinder_tapered(0.22, 1.5, 7, 0.1, timber(WOOD_DARK))),
        center,
        quat_x(lean),
    );
    // Two binding withies around the bundle.
    for dy in [0.4_f32, -0.4] {
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
        solid(cylinder_tapered(0.38, 0.65, 12, 0.05, timber(WOOD_OAK))),
        [0.8, 0.32, 0.0],
        id_quat(),
    )];

    // Three faggot bundles leaning together.
    prims.push(faggot([-0.4, 0.75, -0.35], 0.1));
    prims.push(faggot([-0.55, 0.75, 0.25], -0.12));
    prims.push(faggot([-0.15, 0.75, 0.0], 0.05));

    // Billhook buried in the block: an oak haft and a hooked iron head.
    prims.push(prim(
        solid(cylinder_tapered(0.035, 0.8, 6, 0.0, timber(WOOD_OAK))),
        [0.8, 0.9, 0.0],
        quat_x(0.3),
    ));
    prims.push(prim(
        solid(cone(0.08, 0.28, 6, iron(IRON_DARK))),
        [0.8, 0.62, 0.15],
        quat_x(1.2),
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
