//! Creep patch — an Alien-Organic prop. A spreading mat of fleshy creep
//! swelling in rounded lobes, glowing nodules budding from it and a couple of
//! little tendril-nubs writhing up. Scatter clutter carpeting the colony
//! floor; the nodules are emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, glow, id_quat, prim, prim_scaled, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BIOLUME_GREEN, FLESH_PINK, FLESH_RED, flesh, tendril};

pub struct CreepPatch;

impl CatalogueEntry for CreepPatch {
    fn slug(&self) -> &'static str {
        "creep_patch"
    }
    fn name(&self) -> &'static str {
        "Creep Patch"
    }
    fn description(&self) -> &'static str {
        "Spreading mat of fleshy creep with a few glowing nodules."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienOrganic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ORGANIC_BAND
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
    // Thin creep slick — the root, a flat cylinder mat. CRITICAL: the root
    // must carry an IDENTITY scale — assemble() reparents the nodules + nubs
    // under it and Bevy propagates the root's scale to all children, so a
    // flattened (non-uniform-scale) sphere root would squash the glowing
    // nodules flat into the mat (the root-SCALE sibling of the rotated-root
    // gotcha). The flattening scale lives only on the non-root creep bulges.
    let mut prims = vec![prim(
        solid(cylinder_tapered(1.0, 0.16, 16, 0.0, flesh(FLESH_RED))),
        [0.0, 0.08, 0.0],
        id_quat(),
    )];
    // Rounded creep bulges swelling up from the mat (round blobs — not
    // z-fight), each a different swell height so the patch reads as knobbly.
    for (cx, cz, r, sy) in [
        (0.55_f32, 0.2_f32, 0.55_f32, 0.62_f32),
        (-0.5, 0.4, 0.5, 0.72),
        (0.15, -0.6, 0.52, 0.54),
    ] {
        prims.push(prim_scaled(
            solid(sphere(r, 5, flesh(FLESH_RED))),
            [cx, 0.1, cz],
            id_quat(),
            [1.0, sy, 1.0],
        ));
    }

    // Glowing nodules budding from the bulges — deep green, sitting clearly
    // proud on top of the swells so they read (now un-squashed: the root is
    // identity-scale).
    for (cx, cz, y, r) in [
        (0.55_f32, 0.2_f32, 0.52_f32, 0.3_f32),
        (-0.5, 0.4, 0.58, 0.26),
        (0.15, -0.6, 0.46, 0.24),
        (-0.2, -0.15, 0.32, 0.2),
    ] {
        prims.push(prim(
            solid(sphere(r, 4, glow(BIOLUME_GREEN, 2.0))),
            [cx, y, cz],
            id_quat(),
        ));
    }

    // Little tendril-nubs writhing up out of the mat.
    prims.push(tendril(
        [0.65, 0.05, -0.35],
        1.4,
        0.12,
        0.4,
        3,
        0.5,
        flesh(FLESH_PINK),
    ));
    prims.push(tendril(
        [-0.6, 0.05, -0.2],
        3.6,
        0.11,
        0.38,
        3,
        0.55,
        flesh(FLESH_PINK),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CreepPatch.build(""), "creep_patch");
    }
}
