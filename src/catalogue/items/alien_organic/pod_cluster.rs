//! Pod cluster — an Alien-Organic secondary. A clutch of fleshy egg-pods
//! swelling on short stalks from a creep mound, the ripest ones glowing. The
//! brood of the colony; the lit pods are emissive trim the ruin pass can
//! darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the mound.

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, glow, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{FLESH_PINK, FLESH_RED, SAC_GLOW, flesh, fx};

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

/// One pod (stalk + ovoid) returned for the assemble list; `lit` pods glow.
fn pod(pos: [f32; 3], scale: f32, lit: bool) -> Generator {
    let stalk_h = 0.8 * scale;
    let mut stalk = prim(
        solid(cylinder_tapered(
            0.2 * scale,
            stalk_h,
            6,
            0.3,
            flesh(FLESH_RED),
        )),
        pos,
        id_quat(),
    );
    let pod_mat = if lit {
        glow(SAC_GLOW, 2.2)
    } else {
        flesh(FLESH_PINK)
    };
    stalk.children.push(prim(
        solid(sphere(0.6 * scale, 3, pod_mat)),
        [0.0, stalk_h * 0.5 + 0.5 * scale, 0.0],
        id_quat(),
    ));
    stalk
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Creep mound — the root.
        prim(
            solid(cylinder_tapered(2.4, 0.4, 16, 0.3, flesh(FLESH_RED))),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
    ];

    prims.push(pod([-1.0, 0.4, 0.3], 1.3, true));
    prims.push(pod([0.8, 0.4, -0.4], 1.1, false));
    prims.push(pod([0.3, 0.4, 1.0], 0.9, true));
    prims.push(pod([-0.6, 0.4, -1.0], 0.8, false));

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
