//! Rot patch — an Alien-Organic *poor* prop. A dark slick of necrotic sludge
//! swelling in lobes and pocked with sagging blisters, a couple of them wet
//! and bulging. The decay clutter of the dying colony.

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, id_quat, prim, prim_scaled, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{flesh, membrane};

/// Dark necrotic sludge colour.
const SLUDGE: [f32; 3] = [0.22, 0.20, 0.16];
/// Sickly wet-blister sheen.
const BILE: [f32; 3] = [0.34, 0.34, 0.22];

pub struct RotPatch;

impl CatalogueEntry for RotPatch {
    fn slug(&self) -> &'static str {
        "rot_patch"
    }
    fn name(&self) -> &'static str {
        "Rot Patch"
    }
    fn description(&self) -> &'static str {
        "Dark slick of necrotic sludge pocked with sagging blisters."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienOrganic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ORGANIC_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.2,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Sludge slick — the root, a thin flat cylinder mat. CRITICAL: the root
    // must carry an IDENTITY scale — assemble() reparents the blisters under
    // it and Bevy propagates the root's scale to all children, so a flattened
    // (non-uniform-scale) sphere root would squash every blister flat into the
    // slick (the root-SCALE sibling of the rotated-root gotcha). The flattening
    // scale lives only on the non-root sludge lobes/blisters.
    let mut prims = vec![prim(
        solid(cylinder_tapered(1.05, 0.14, 16, 0.0, flesh(SLUDGE))),
        [0.0, 0.07, 0.0],
        id_quat(),
    )];
    // Overlapping sludge lobes.
    for (cx, cz, r) in [(0.6_f32, 0.25_f32, 0.55_f32), (-0.55, 0.4, 0.5)] {
        prims.push(prim_scaled(
            solid(sphere(r, 5, flesh(SLUDGE))),
            [cx, 0.08, cz],
            id_quat(),
            [1.0, 0.22, 1.0],
        ));
    }
    // Sagging blisters bulging from the rot — matte and wet, varied heights,
    // standing clearly proud of the slick (now un-squashed: the root is
    // identity-scale).
    for (cx, cz, r, wet) in [
        (0.25_f32, 0.1_f32, 0.36_f32, true),
        (-0.4, -0.25, 0.3, false),
        (0.55, -0.4, 0.26, true),
        (-0.15, 0.5, 0.22, false),
    ] {
        let mat = if wet {
            membrane(BILE)
        } else {
            flesh([0.30, 0.26, 0.20])
        };
        prims.push(prim_scaled(
            solid(sphere(r, 5, mat)),
            [cx, 0.12 + r * 0.5, cz],
            id_quat(),
            [1.0, 0.9, 1.0],
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
        assert_sanitize_stable(&RotPatch.build(""), "rot_patch");
    }
}
