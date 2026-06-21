//! E-waste pile — a Cyberpunk *poor* prop. A heap of dead CRT monitors, a
//! gutted PC tower, snapped circuit boards, a tossed keyboard and tangled
//! cabling, with one cracked panel still faintly glowing; undercity street
//! clutter.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, glow, helix, id_quat, prim, quat_mul, quat_x, quat_y, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, RUST_BROWN, fx, metal, rust};

pub struct EwastePile;

impl CatalogueEntry for EwastePile {
    fn slug(&self) -> &'static str {
        "ewaste_pile"
    }
    fn name(&self) -> &'static str {
        "E-Waste Pile"
    }
    fn description(&self) -> &'static str {
        "Heap of dead monitors, boards and cabling with one cracked panel glowing."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CYBER_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.3,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

const BEIGE: [f32; 3] = [0.58, 0.55, 0.46];
const PCB_GREEN: [f32; 3] = [0.10, 0.32, 0.16];
const CABLE: [f32; 3] = [0.05, 0.05, 0.06];

/// A dead CRT monitor — a casing with a dark (or faintly lit) glass face.
fn monitor(pos: [f32; 3], tilt_x: f32, tilt_y: f32, case: [f32; 3], lit: bool) -> Generator {
    let mut body = prim(
        solid(cuboid_tapered([0.5, 0.44, 0.46], 0.0, metal(case))),
        pos,
        quat_mul(quat_y(tilt_y), quat_x(tilt_x)),
    );
    let glass = if lit {
        glow(NEON_CYAN, 2.2)
    } else {
        metal([0.03, 0.04, 0.05])
    };
    // Recessed dark bezel + glass face on the front (+Z).
    body.children.push(prim(
        cuboid_tapered([0.42, 0.36, 0.05], 0.0, metal([0.04, 0.04, 0.05])),
        [0.0, 0.02, 0.22],
        id_quat(),
    ));
    body.children.push(prim(
        cuboid_tapered([0.34, 0.28, 0.04], 0.0, glass),
        [0.0, 0.02, 0.25],
        id_quat(),
    ));
    body
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Base mound of crushed e-waste (root).
        prim(
            solid(cuboid_tapered([1.5, 0.5, 1.3], 0.2, rust(RUST_BROWN))),
            [0.0, 0.25, 0.0],
            id_quat(),
        ),
        // Two dead CRT monitors tossed on the heap, one still faintly lit.
        monitor([0.18, 0.78, 0.0], -0.3, 0.4, DARK_METAL, true),
        monitor([-0.32, 0.62, 0.25], 0.45, -0.6, BEIGE, false),
    ];

    // A gutted PC tower lying on its side.
    prims.push(prim(
        solid(cuboid_tapered([0.62, 0.24, 0.5], 0.0, metal(BEIGE))),
        [-0.45, 0.62, -0.35],
        quat_x(0.12),
    ));
    // Snapped green circuit boards jutting out at angles.
    for (p, ax, ay) in [
        ([0.5_f32, 0.66_f32, 0.4_f32], 0.7_f32, 0.5_f32),
        ([0.35, 0.84, -0.3], 0.9, -0.7),
    ] {
        prims.push(prim(
            solid(cuboid_tapered([0.42, 0.03, 0.3], 0.0, metal(PCB_GREEN))),
            p,
            quat_mul(quat_y(ay), quat_x(ax)),
        ));
    }
    // A tossed keyboard.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.46, 0.05, 0.18],
            0.0,
            metal([0.12, 0.12, 0.13]),
        )),
        [0.4, 0.55, -0.05],
        quat_y(0.5),
    ));
    // Tangled cabling — a coil and a loose loop draped over the heap.
    prims.push(prim(
        helix(0.16, 0.04, 0.06, 3.0, 12, metal(CABLE)),
        [-0.1, 0.6, 0.4],
        quat_x(1.4),
    ));
    prims.push(prim(
        torus(0.04, 0.2, metal(CABLE)),
        [0.55, 0.52, 0.1],
        quat_x(1.2),
    ));

    // The cracked panel still faintly lit — it fizzes and throws sparks.
    let mut panel = prim(
        cuboid_tapered([0.5, 0.04, 0.34], 0.0, glow(NEON_CYAN, 2.5)),
        [0.05, 0.95, 0.1],
        quat_x(-0.45),
    );
    panel.audio = fx::electric_crackle();
    prims.push(panel);
    prims.push(fx::spark_burst([0.05, 1.0, 0.1], 0xEA57_E000));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&EwastePile.build(""), "ewaste_pile");
    }

    #[test]
    fn has_neon() {
        assert!(crate::catalogue::items::util::has_emissive(
            &EwastePile.build("")
        ));
    }
}
