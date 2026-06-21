//! Container stack — a Cyberpunk *poor* secondary. Two weathered shipping
//! containers stacked askew — end doors with locking rods, a side ladder, a
//! rooftop tank and a dim neon strip; makeshift undercity housing/storage
//! ringing the scrap shanty.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONTAINER_BLUE, CONTAINER_RUST, NEON_CYAN, NEON_LIME, corrugated, metal};

pub struct ContainerStack;

impl CatalogueEntry for ContainerStack {
    fn slug(&self) -> &'static str {
        "container_stack"
    }
    fn name(&self) -> &'static str {
        "Container Stack"
    }
    fn description(&self) -> &'static str {
        "Two weathered shipping containers stacked askew with a dim neon strip."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CYBER_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 4.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let ch = 2.5_f32;
    let door = [0.16, 0.17, 0.20]; // near-black door panel
    let mut prims = vec![
        // Lower container (root).
        prim(
            solid(cuboid_tapered(
                [3.6, ch, 2.3],
                0.0,
                corrugated(CONTAINER_BLUE),
            )),
            [0.0, ch * 0.5, 0.0],
            id_quat(),
        ),
        // Upper container, shifted and tilted.
        prim(
            solid(cuboid_tapered(
                [3.3, ch, 2.2],
                0.0,
                corrugated(CONTAINER_RUST),
            )),
            [0.35, ch * 1.5, 0.15],
            quat_x(0.05),
        ),
        // Dim neon strip down the side.
        prim(
            cuboid_tapered([0.15, ch * 1.6, 0.15], 0.0, glow(NEON_LIME, 3.0)),
            [1.9, ch * 0.9, 0.0],
            id_quat(),
        ),
        // A small lit porthole on the lower container — a sign the undercity
        // housing is occupied. Proud of the face so it can't z-fight the body.
        prim(
            cuboid_tapered([0.5, 0.5, 0.06], 0.0, glow(NEON_CYAN, 2.0)),
            [-0.6, ch * 0.55, 1.2],
            id_quat(),
        ),
    ];

    // Freight end doors on the lower container's +X face: a flat door panel
    // with the signature pair of vertical locking rods and cam handles.
    let xf = 1.8_f32;
    prims.push(prim(
        solid(cuboid_tapered([0.06, ch * 0.92, 2.1], 0.0, metal(door))),
        [xf, ch * 0.5, 0.0],
        id_quat(),
    ));
    for dz in [-0.7_f32, -0.25, 0.25, 0.7] {
        prims.push(prim(
            solid(cylinder_tapered(
                0.05,
                ch * 0.86,
                6,
                0.0,
                metal([0.3, 0.3, 0.32]),
            )),
            [xf + 0.05, ch * 0.5, dz],
            id_quat(),
        ));
    }
    for dz in [-0.48_f32, 0.48] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.12, 0.12, 0.1],
                0.0,
                metal([0.3, 0.3, 0.32]),
            )),
            [xf + 0.1, ch * 0.55, dz],
            id_quat(),
        ));
    }

    // A welded ladder up the -X face of the lower container.
    for dz in [-0.18_f32, 0.18] {
        prims.push(prim(
            solid(cylinder_tapered(
                0.035,
                ch * 0.9,
                5,
                0.0,
                metal([0.4, 0.4, 0.42]),
            )),
            [-1.82, ch * 0.5, dz],
            id_quat(),
        ));
    }
    for k in 0..6 {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.06, 0.04, 0.44],
                0.0,
                metal([0.4, 0.4, 0.42]),
            )),
            [-1.82, 0.4 + 0.35 * k as f32, 0.0],
            id_quat(),
        ));
    }

    // A rusted water tank + a vent box on the upper container's roof.
    prims.push(prim(
        solid(cylinder_tapered(
            0.45,
            0.8,
            12,
            0.0,
            corrugated([0.5, 0.42, 0.32]),
        )),
        [0.7, ch * 2.0 + 0.4, 0.4],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.7, 0.4, 0.6],
            0.0,
            metal([0.5, 0.51, 0.53]),
        )),
        [-0.3, ch * 2.0 + 0.2, -0.3],
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
        assert_sanitize_stable(&ContainerStack.build(""), "container_stack");
    }

    #[test]
    fn has_neon() {
        assert!(crate::catalogue::items::util::has_emissive(
            &ContainerStack.build("")
        ));
    }
}
