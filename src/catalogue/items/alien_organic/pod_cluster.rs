//! Pod cluster — an Alien-Organic secondary. A clutch of swollen fleshy
//! egg-pods budding from a lobed creep mound, the ripest aglow, veined nubs
//! pushing up between them. The brood of the colony; the lit pods are emissive
//! trim the ruin pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the mound (the root,
//! `id_quat`).

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, glow, id_quat, prim, prim_scaled, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{FLESH_PINK, FLESH_RED, SAC_GLOW, egg_pod, flesh, fx};

pub struct PodCluster;

impl CatalogueEntry for PodCluster {
    fn slug(&self) -> &'static str {
        "pod_cluster"
    }
    fn name(&self) -> &'static str {
        "Pod Cluster"
    }
    fn description(&self) -> &'static str {
        "Clutch of fleshy egg-pods on stalks from a creep mound, the ripest glowing."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienOrganic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ORGANIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Creep mound — the root, a flat cylinder disc. CRITICAL: the root must
    // carry an IDENTITY scale, because assemble() reparents every pod under it
    // and Bevy propagates the root's scale to all children — a flattened
    // (non-uniform-scale) sphere root would squash every pod flat (the
    // root-SCALE sibling of the rotated-root gotcha). The low rounded swells
    // are non-root children, where a flattening scale only affects themselves.
    let mut prims = vec![prim(
        solid(cylinder_tapered(1.15, 0.34, 16, 0.32, flesh(FLESH_RED))),
        [0.0, 0.17, 0.0],
        id_quat(),
    )];
    // Rounded creep swells on the mound (round blobs — not z-fight).
    for (cx, cz, r) in [
        (0.65_f32, 0.2_f32, 0.5_f32),
        (-0.55, 0.4, 0.45),
        (0.2, -0.6, 0.46),
    ] {
        prims.push(prim_scaled(
            solid(sphere(r, 5, flesh(FLESH_RED))),
            [cx, 0.18, cz],
            id_quat(),
            [1.0, 0.42, 1.0],
        ));
    }

    // Egg-pods budding from the mound — varied size, tall, the ripest aglow,
    // clustered tight so they read as an upright clutch.
    for (px, pz, r, tall, lit) in [
        (-0.5_f32, 0.18_f32, 0.66_f32, 1.95_f32, true),
        (0.5, -0.25, 0.56, 1.9, false),
        (0.18, 0.6, 0.46, 2.0, true),
        (-0.32, -0.5, 0.42, 1.9, false),
    ] {
        let pod_mat = if lit {
            glow(SAC_GLOW, 2.0)
        } else {
            flesh(FLESH_PINK)
        };
        prims.push(egg_pod([px, 0.3, pz], r, tall, pod_mat, flesh(FLESH_RED)));
    }

    // Veined nubs pushing up between the pods.
    for (cx, cz) in [(0.0_f32, -0.05_f32), (-0.2, 0.45), (0.6, 0.3)] {
        prims.push(prim(
            solid(cylinder_tapered(0.12, 0.5, 6, 0.6, flesh(FLESH_PINK))),
            [cx, 0.4, cz],
            id_quat(),
        ));
    }

    let mut root = assemble(prims);
    // Signature life: spores drifting off the brood.
    root.children
        .push(fx::spore_drift([0.0, 1.0, 0.0], 0x0A11_9012));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&PodCluster.build(""), "pod_cluster");
    }
}
