//! Tyre stack — a Sports/Recreation *poor* prop. A leaning stack of training
//! tyres with one rolled off to the side. The improvised gear of the
//! municipal rec ground.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::painted;

/// Worn black rubber of the tyres.
const RUBBER: [f32; 3] = [0.10, 0.10, 0.11];
/// Sun-greyed tread band around each worn tyre.
const TREAD: [f32; 3] = [0.27, 0.27, 0.28];
/// Faded webbing of the binding straps.
const STRAP: [f32; 3] = [0.20, 0.17, 0.13];

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
    // Bottom tyre's worn tread band.
    prims.push(prim(
        torus(0.06, 0.45, painted(TREAD)),
        [0.0, 0.18, 0.0],
        id_quat(),
    ));

    // Three more tyres stacked with a slight lean, each with a tread band.
    for (k, off) in [(1usize, 0.05_f32), (2, 0.1), (3, 0.16)] {
        let c = [off, 0.18 + k as f32 * 0.3, off * 0.5];
        prims.push(prim(
            solid(torus(0.18, 0.42, painted(RUBBER))),
            c,
            id_quat(),
        ));
        prims.push(prim(torus(0.06, 0.45, painted(TREAD)), c, id_quat()));
    }

    // Two frayed straps cinching the stack, knotted over the top.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.06, 1.35, 0.1], 0.0, painted(STRAP))),
            [sx * 0.13, 0.72, 0.0],
            quat_x(sx * 0.12),
        ));
    }

    // One tyre rolled off to the side, stood on its edge.
    let edge = [1.0_f32, 0.42, -0.4];
    prims.push(prim(
        solid(torus(0.18, 0.42, painted(RUBBER))),
        edge,
        quat_x(1.5),
    ));
    prims.push(prim(torus(0.06, 0.45, painted(TREAD)), edge, quat_x(1.5)));

    // A fifth tyre lying flat nearby.
    prims.push(prim(
        solid(torus(0.18, 0.42, painted(RUBBER))),
        [-0.9, 0.18, 0.5],
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
        assert_sanitize_stable(&TireStack.build(""), "tire_stack");
    }
}
