//! Urn — an AncientClassical prop. A large terracotta amphora on a marble
//! foot beside a smaller one, handled and bellied: the storage vessels of a
//! classical household set out by a wall.

use crate::catalogue::items::util::{assemble, cylinder_tapered, id_quat, prim, solid, torus};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{MARBLE_WHITE, TERRACOTTA, marble, terracotta};

pub struct Urn;

impl CatalogueEntry for Urn {
    fn slug(&self) -> &'static str {
        "urn"
    }
    fn name(&self) -> &'static str {
        "Urn"
    }
    fn description(&self) -> &'static str {
        "Large handled terracotta amphora on a marble foot beside a smaller one."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AncientClassical]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ANCIENT_BAND
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

/// A bellied amphora of total height `h` at `center`: a marble foot, a
/// tapered terracotta body, a neck, and two handle hoops.
fn amphora(center: [f32; 3], h: f32) -> Generator {
    let r = h * 0.28;
    let mut a = prim(
        solid(cylinder_tapered(
            r * 0.4,
            h * 0.12,
            10,
            0.0,
            marble(MARBLE_WHITE),
        )),
        center,
        id_quat(),
    );
    // Bellied body (children rebased into the foot's local frame: y up).
    a.children.push(prim(
        solid(cylinder_tapered(
            r,
            h * 0.6,
            14,
            0.35,
            terracotta(TERRACOTTA),
        )),
        [0.0, h * 0.42, 0.0],
        id_quat(),
    ));
    // Neck.
    a.children.push(prim(
        solid(cylinder_tapered(
            r * 0.45,
            h * 0.3,
            12,
            -0.2,
            terracotta(TERRACOTTA),
        )),
        [0.0, h * 0.85, 0.0],
        id_quat(),
    ));
    // Two handle hoops at the shoulder.
    for sx in [-1.0_f32, 1.0] {
        a.children.push(prim(
            torus(0.04, r * 0.5, terracotta(TERRACOTTA)),
            [sx * r * 0.7, h * 0.7, 0.0],
            id_quat(),
        ));
    }
    a
}

fn build_tree() -> Generator {
    let mut prims = vec![amphora([0.0, 0.0, 0.0], 1.6)];
    prims.push(amphora([0.9, 0.0, 0.3], 1.0));
    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Urn.build(""), "urn");
    }
}
