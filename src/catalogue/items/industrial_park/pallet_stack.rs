//! Pallet stack — an Industrial-Park prop. A stack of wooden pallets beside a
//! couple of shipping crates, waiting on the loading apron.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_y, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp4, Generator};
use crate::seeded_defaults::ThemeArchetype;

use super::{cladding, timber};

/// Pale pallet wood.
const PALLET_WOOD: [f32; 3] = [0.60, 0.46, 0.28];
/// Crate wood.
const CRATE_WOOD: [f32; 3] = [0.48, 0.36, 0.22];

pub struct PalletStack;

impl CatalogueEntry for PalletStack {
    fn slug(&self) -> &'static str {
        "pallet_stack"
    }
    fn name(&self) -> &'static str {
        "Pallet Stack"
    }
    fn description(&self) -> &'static str {
        "Stack of wooden pallets beside a couple of shipping crates."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::INDUSTRIAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.5,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // One pallet = bottom deck slats + three stringers + a slatted top deck,
    // so the fork gaps read. Returned as a rigid subtree (root = the centre
    // stringer) so a yawed copy keeps its slats aligned.
    let pallet = |base_y: f32, yaw: Fp4| -> Generator {
        let deck = || timber(PALLET_WOOD);
        let block = || timber([0.5, 0.38, 0.22]);
        let mut p = prim(
            solid(cuboid_tapered([1.2, 0.07, 0.1], 0.0, block())),
            [0.0, base_y + 0.07, 0.0],
            yaw,
        );
        // The two outer stringers.
        for sz in [-0.45_f32, 0.45] {
            p.children.push(prim(
                solid(cuboid_tapered([1.2, 0.07, 0.1], 0.0, block())),
                [0.0, 0.0, sz],
                id_quat(),
            ));
        }
        // Bottom deck — three slats.
        for sz in [-0.45_f32, 0.0, 0.45] {
            p.children.push(prim(
                solid(cuboid_tapered([1.2, 0.035, 0.12], 0.0, deck())),
                [0.0, -0.055, sz],
                id_quat(),
            ));
        }
        // Top deck — five slats with gaps.
        for i in 0..5 {
            let sz = -0.45 + i as f32 * 0.225;
            p.children.push(prim(
                solid(cuboid_tapered([1.2, 0.035, 0.13], 0.0, deck())),
                [0.0, 0.055, sz],
                id_quat(),
            ));
        }
        p
    };

    // Stack of five pallets with a slight yaw jitter (the first is the root,
    // so it stays square).
    let mut prims = vec![pallet(0.0, id_quat())];
    let jitter = [0.06_f32, -0.05, 0.08, -0.04];
    for (k, j) in jitter.iter().enumerate() {
        prims.push(pallet((k + 1) as f32 * 0.155, quat_y(*j)));
    }

    // A battened shipping crate alongside.
    let mut crate_box = prim(
        solid(cuboid_tapered([1.2, 1.0, 1.1], 0.0, timber(CRATE_WOOD))),
        [1.6, 0.5, 0.2],
        quat_y(0.18),
    );
    let batten = || timber([0.34, 0.25, 0.15]);
    for (bx, bz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        crate_box.children.push(prim(
            solid(cuboid_tapered([0.08, 1.02, 0.08], 0.0, batten())),
            [bx * 0.58, 0.0, bz * 0.53],
            id_quat(),
        ));
    }
    for by in [-0.3_f32, 0.3] {
        crate_box.children.push(prim(
            solid(cuboid_tapered([1.22, 0.08, 0.08], 0.0, batten())),
            [0.0, by, 0.56],
            id_quat(),
        ));
    }
    prims.push(crate_box);

    // A smaller steel crate stacked on it.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.9, 0.8, 0.9],
            0.0,
            cladding([0.5, 0.5, 0.46]),
        )),
        [1.7, 1.4, 0.1],
        quat_y(-0.14),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&PalletStack.build(""), "pallet_stack");
    }
}
