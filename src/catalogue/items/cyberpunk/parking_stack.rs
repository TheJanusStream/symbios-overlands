//! Parking stack — a Cyberpunk secondary. An open multi-deck concrete slab
//! tower on corner pillars, each deck neon-edged, served by a spiral ramp
//! around a stair/lift core and dotted with parked cars. The low, wide
//! counterpoint to the megatower's height.

use crate::catalogue::items::util::{
    cuboid_tapered, foundation_block, glow, helix, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, NEON_MAGENTA, concrete, metal};

pub struct ParkingStack;

impl CatalogueEntry for ParkingStack {
    fn slug(&self) -> &'static str {
        "parking_stack"
    }
    fn name(&self) -> &'static str {
        "Parking Stack"
    }
    fn description(&self) -> &'static str {
        "Open multi-deck parking structure with a spiral ramp and parked cars."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CYBER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// A small parked-car silhouette — a low two-box body with red taillights,
/// added to a deck for scale and read.
fn parked_car(root: &mut Generator, x: f32, y: f32, z: f32, sx: f32) {
    let paint = [0.10_f32, 0.11, 0.14];
    root.children.push(prim(
        solid(cuboid_tapered([1.9, 0.55, 0.9], 0.0, metal(paint))),
        [x, y + 0.28, z],
        id_quat(),
    ));
    root.children.push(prim(
        cuboid_tapered([1.0, 0.4, 0.82], 0.15, metal(paint)),
        [x - sx * 0.1, y + 0.7, z],
        id_quat(),
    ));
    for dz in [-0.3_f32, 0.3] {
        root.children.push(prim(
            cuboid_tapered([0.08, 0.1, 0.12], 0.0, glow([0.9, 0.1, 0.08], 5.0)),
            [x + sx * 0.95, y + 0.35, z + dz],
            id_quat(),
        ));
    }
}

fn build_tree() -> Generator {
    // A parking structure is board-formed concrete, not glossy metal.
    let conc = [0.30_f32, 0.31, 0.34];
    let slab_h = 0.4;
    let (w, depth) = (11.0_f32, 8.0_f32);

    let mut root = prim(
        solid(cuboid_tapered([w, slab_h, depth], 0.0, concrete(conc))),
        [0.0, slab_h * 0.5, 0.0],
        id_quat(),
    );
    let rel = |ground_y: f32| ground_y - slab_h * 0.5;

    let mut base = foundation_block(w, depth, [0.0, 0.0], 2.5);
    base.transform.translation.0[1] -= slab_h * 0.5;
    root.children.push(base);

    // Corner pillars.
    let total_h = 9.0_f32;
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            root.children.push(prim(
                solid(cuboid_tapered([0.6, total_h, 0.6], 0.0, concrete(conc))),
                [
                    sx * (w * 0.5 - 0.6),
                    rel(slab_h + total_h * 0.5),
                    sz * (depth * 0.5 - 0.6),
                ],
                id_quat(),
            ));
        }
    }

    // Stair/lift core at one end, with a lit doorway slot.
    let core_x = -w * 0.5 + 1.7;
    root.children.push(prim(
        solid(cuboid_tapered(
            [2.4, total_h, 2.4],
            0.0,
            concrete([0.26, 0.27, 0.30]),
        )),
        [core_x, rel(slab_h + total_h * 0.5), 0.0],
        id_quat(),
    ));
    root.children.push(prim(
        cuboid_tapered([0.1, 1.8, 0.9], 0.0, glow(NEON_CYAN, 3.0)),
        [core_x + 1.25, rel(slab_h + 1.0), 0.0],
        id_quat(),
    ));

    // Spiral ramp climbing the core — a glowing helical guide reading as the
    // structure's signature car ramp.
    let ramp_turns = 3.0_f32;
    let ramp_pitch = (total_h - 1.0) / ramp_turns;
    root.children.push(prim(
        helix(1.9, 0.16, ramp_pitch, ramp_turns, 24, glow(NEON_CYAN, 5.0)),
        [
            core_x,
            rel(slab_h + 0.6 + ramp_turns * ramp_pitch * 0.5),
            0.0,
        ],
        id_quat(),
    ));

    // Decks, each with a neon edge band just beneath it and a couple of parked
    // cars on the open bay (the side away from the core).
    let decks = 3;
    for d in 0..decks {
        let dy = slab_h + total_h * (d as f32 + 1.0) / (decks as f32 + 0.5);
        root.children.push(prim(
            solid(cuboid_tapered(
                [w - 1.0, 0.3, depth - 1.0],
                0.0,
                concrete(conc),
            )),
            [0.0, rel(dy), 0.0],
            id_quat(),
        ));
        // Neon edge band, proud of the pillars to avoid coplanar z-fight.
        root.children.push(prim(
            cuboid_tapered([w - 0.3, 0.18, depth - 0.3], 0.0, glow(NEON_CYAN, 5.0)),
            [0.0, rel(dy - 0.25), 0.0],
            id_quat(),
        ));
        // Parked cars sitting on this deck (skip the top deck — open roof).
        if d < decks - 1 {
            for (cx, cz, sgn) in [(2.4_f32, -1.6_f32, 1.0_f32), (3.4, 1.6, -1.0)] {
                parked_car(&mut root, cx, rel(dy + 0.15), cz, sgn);
            }
        }
    }

    // Lit "P" entry sign on a post at the front corner.
    root.children.push(prim(
        solid(cuboid_tapered([0.2, 2.6, 0.2], 0.0, metal(DARK_METAL))),
        [w * 0.5 - 0.4, rel(slab_h + 1.3), depth * 0.5 + 0.3],
        id_quat(),
    ));
    root.children.push(prim(
        cuboid_tapered([0.9, 1.0, 0.12], 0.0, glow(NEON_MAGENTA, 4.0)),
        [w * 0.5 - 0.4, rel(slab_h + 3.0), depth * 0.5 + 0.36],
        id_quat(),
    ));

    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ParkingStack.build(""), "parking_stack");
    }

    #[test]
    fn has_neon() {
        assert!(crate::catalogue::items::util::has_emissive(
            &ParkingStack.build("")
        ));
    }
}
