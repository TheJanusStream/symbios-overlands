//! Tyre stack — a Sports/Recreation *poor* prop. A leaning stack of training
//! tyres with one rolled off to the side. The improvised gear of the
//! municipal rec ground.

use crate::catalogue::items::util::{assemble, id_quat, prim, quat_x, solid, torus};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::painted;

/// Worn black rubber of the tyres.
const RUBBER: [f32; 3] = [0.10, 0.10, 0.11];

pub struct TireStack;

impl CatalogueEntry for TireStack {
    fn slug(&self) -> &'static str {
        "tire_stack"
    }
    fn name(&self) -> &'static str {
        "Tyre Stack"
    }
    fn description(&self) -> &'static str {
        "A leaning stack of training tyres with one rolled off to the side."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SportsRec]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SPORTS_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Bottom tyre — the root, lying flat.
        prim(
            solid(torus(0.18, 0.42, painted(RUBBER))),
            [0.0, 0.18, 0.0],
            id_quat(),
        ),
    ];

    // Three more tyres stacked with a slight lean.
    for (k, off) in [(1usize, 0.05_f32), (2, 0.1), (3, 0.16)] {
        prims.push(prim(
            solid(torus(0.18, 0.42, painted(RUBBER))),
            [off, 0.18 + k as f32 * 0.3, off * 0.5],
            id_quat(),
        ));
    }

    // One tyre rolled off to the side, stood on its edge.
    prims.push(prim(
        solid(torus(0.18, 0.42, painted(RUBBER))),
        [1.0, 0.42, -0.4],
        quat_x(1.5),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&TireStack.build(""), "tire_stack");
    }
}
