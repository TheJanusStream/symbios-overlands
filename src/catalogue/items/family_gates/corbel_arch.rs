//! Corbel Arch Gateway - a family gate for Mesoamerica (#1796), stood
//! beside the theme's own. A Maya corbel-vaulted arch in painted plaster:
//! two piers rise to a wooden vault beam at the springing, the courses above
//! step in toward each other to a capstone - the stepped false arch - and a
//! red frieze over it carries a jade feathered serpent. Two small stone
//! censers smoulder at its feet.
//!
//! `themes()` stays empty, as `civic_gateway`'s does: the gate reaches the
//! Mesoamerican pool through `kindred_gates`.

use crate::catalogue::items::fx::Emitter;
use crate::catalogue::items::mesoamerican::{
    FIRE_ORANGE, JADE_GREEN, LIMESTONE_PALE, OBSIDIAN_BLACK, STONE_GREY, STUCCO_CREAM, STUCCO_RED,
    TIMBER_BROWN, jade, limestone, obsidian, painted, timber,
};
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, footing, glow, id_quat, lathe, prim, prim_scaled, quat_z, solid,
    sphere, spine,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{EmitterShape, Fp, Fp3, Generator, GeneratorKind, ParticleBlendMode};

pub struct CorbelArchGateway;

impl CatalogueEntry for CorbelArchGateway {
    fn slug(&self) -> &'static str {
        "corbel_arch_gateway"
    }
    fn name(&self) -> &'static str {
        "Corbel Arch Gateway"
    }
    fn description(&self) -> &'static str {
        "A stepped Maya corbel arch in painted plaster under a feathered-serpent frieze."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn footprint(&self) -> Footprint {
        super::gate_footprint()
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Half the opening between the piers' inner faces (m).
const HALF_OPENING: f32 = 1.3;
/// Top of the stone platform the arch stands on.
const SLAB_TOP: f32 = 0.35;
/// The springing line, where the corbelling begins.
const SPRING: f32 = 3.35;
/// Underside of the vault beam, the head of the opening.
const BEAM_UNDER: f32 = 3.05;
/// Half the arch's width, and its depth through the way.
const HALF_W: f32 = 3.2;
const DEPTH: f32 = 1.6;
/// Each corbel course's height, and how far it steps in.
const COURSE_H: f32 = 0.42;
const STEP: f32 = 0.25;
const COURSES: usize = 4;

fn build_tree() -> Generator {
    let plaster = || painted(STUCCO_CREAM);
    let vault_top = SPRING + COURSE_H * (COURSES as f32 + 1.0);

    let mut prims = vec![footing(2.0 * HALF_W + 0.1, 2.8, [0.0, 0.0], 3.5)];
    // The stone platform, a little wider than the arch so the piers' feet
    // share no face with its sides.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0 * HALF_W + 0.1, SLAB_TOP, 2.8],
            0.0,
            limestone(LIMESTONE_PALE),
        )),
        [0.0, SLAB_TOP * 0.5, 0.0],
        id_quat(),
    ));

    for sx in [-1.0_f32, 1.0] {
        // The pier, sunk 2 cm into the platform.
        let pier_w = HALF_W - HALF_OPENING;
        prims.push(prim(
            solid(cuboid_tapered(
                [pier_w, SPRING - SLAB_TOP + 0.02, DEPTH],
                0.0,
                plaster(),
            )),
            [
                sx * (HALF_OPENING + pier_w * 0.5),
                (SPRING + SLAB_TOP - 0.02) * 0.5,
                0.0,
            ],
            id_quat(),
        ));
        // A red-painted dado round the pier's foot, a centimetre shy of the
        // opening.
        prims.push(prim(
            solid(cuboid_tapered(
                [pier_w + 0.03, 0.5, DEPTH + 0.1],
                0.0,
                painted(STUCCO_RED),
            )),
            [
                sx * (HALF_OPENING + 0.01 + (pier_w + 0.03) * 0.5),
                SLAB_TOP + 0.24,
                0.0,
            ],
            id_quat(),
        ));
        // The corbel courses, each stepping in over the one below.
        for k in 0..COURSES {
            let inner = HALF_OPENING - STEP * (k as f32 + 1.0);
            let y0 = SPRING + COURSE_H * k as f32;
            prims.push(prim(
                solid(cuboid_tapered(
                    [HALF_W - inner, COURSE_H, DEPTH],
                    0.0,
                    plaster(),
                )),
                [sx * (HALF_W + inner) * 0.5, y0 + COURSE_H * 0.5, 0.0],
                id_quat(),
            ));
        }
        prims.extend(censer(
            [sx * 1.9, SLAB_TOP, -1.1],
            0x1796 + (sx + 1.0) as u64,
        ));
    }
    // The capstone course closing the vault, full width.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0 * HALF_W, COURSE_H, DEPTH],
            0.0,
            plaster(),
        )),
        [0.0, vault_top - COURSE_H * 0.5, 0.0],
        id_quat(),
    ));
    // The wooden vault beam across the springing, its ends in the piers,
    // 2 cm under the first course.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0 * HALF_OPENING + 0.3, SPRING - 0.02 - BEAM_UNDER, 0.6],
            0.0,
            timber(TIMBER_BROWN),
        )),
        [0.0, (SPRING - 0.02 + BEAM_UNDER) * 0.5, 0.0],
        id_quat(),
    ));

    // The red frieze over the vault, a cornice over it.
    let frieze_h = 1.0;
    let frieze_front = -(DEPTH * 0.5 + 0.05);
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0 * HALF_W + 0.1, frieze_h, DEPTH + 0.1],
            0.0,
            painted(STUCCO_RED),
        )),
        [0.0, vault_top + frieze_h * 0.5, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0 * HALF_W + 0.3, 0.22, DEPTH + 0.25],
            0.0,
            painted(STUCCO_CREAM),
        )),
        [0.0, vault_top + frieze_h + 0.11, 0.0],
        id_quat(),
    ));
    prims.extend(serpent(vault_top + frieze_h * 0.5, frieze_front));

    // The walk-in zone under the vault beam, its edges bedded 4 cm in the
    // piers, the beam and the platform (#1006).
    let veil_h = BEAM_UNDER - SLAB_TOP + 0.08;
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.0 * HALF_OPENING + 0.08, veil_h, 0.5]),
        },
        [0.0, SLAB_TOP - 0.04 + veil_h * 0.5, 0.0],
        id_quat(),
    ));

    assemble(prims)
}

/// The feathered serpent along the frieze's face (`front`), centred on
/// `mid_y`: a jade body undulating from its tail at the left to its head at
/// the right, a quetzal plume rising off each crest, and an obsidian eye.
fn serpent(mid_y: f32, front: f32) -> Vec<Generator> {
    let z = front - 0.06;
    let n = 12;
    let station = |i: usize| {
        let x = -2.7 + 0.42 * i as f32;
        let y = mid_y + 0.2 * (i as f32 * 0.95).sin();
        [x, y, z]
    };
    let points: Vec<([f32; 3], f32)> = (0..n)
        .map(|i| (station(i), 0.06 + 0.09 * i as f32 / (n - 1) as f32))
        .collect();
    let mut parts = vec![prim(
        spine(&points, 10, 3, jade(JADE_GREEN)),
        [0.0, 0.0, 0.0],
        id_quat(),
    )];
    // Plumes off the body, leaning back toward the tail.
    for i in (1..n - 1).step_by(2) {
        let [x, y, _] = station(i);
        let r = 0.06 + 0.09 * i as f32 / (n - 1) as f32;
        parts.push(prim(
            cuboid_tapered([0.16, 0.42, 0.05], 0.6, jade([0.16, 0.44, 0.30])),
            [x - 0.06, y + r + 0.17, z],
            quat_z(0.35),
        ));
    }
    // The head: an upper jaw, a gaping lower jaw, an eye and a crest.
    let [hx, hy, _] = station(n - 1);
    let head_x = hx + 0.36;
    parts.push(prim(
        solid(cuboid_tapered([0.62, 0.38, 0.3], 0.15, jade(JADE_GREEN))),
        [head_x, hy + 0.1, z],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.52, 0.12, 0.24], 0.0, jade([0.16, 0.44, 0.30])),
        [head_x - 0.02, hy - 0.18, z],
        quat_z(-0.3),
    ));
    parts.push(prim(
        sphere(0.06, 3, obsidian(OBSIDIAN_BLACK)),
        [head_x + 0.05, hy + 0.18, z - 0.14],
        id_quat(),
    ));
    for (k, lean) in [0.2_f32, 0.6, 1.0].into_iter().enumerate() {
        parts.push(prim(
            cuboid_tapered([0.14, 0.46, 0.05], 0.6, jade(JADE_GREEN)),
            [head_x - 0.18 - 0.06 * k as f32, hy + 0.46, z],
            quat_z(lean),
        ));
    }
    parts
}

/// A small stone censer standing at `foot`: a waisted stand under a bowl
/// of embers, a thread of copal smoke rising off them.
fn censer(foot: [f32; 3], seed: u64) -> Vec<Generator> {
    let [x, y, z] = foot;
    let h = 0.6;
    vec![
        prim(
            solid(lathe(
                &[
                    (0.17, 0.0),
                    (0.13, 0.08),
                    (0.08, 0.3),
                    (0.14, 0.42),
                    (0.21, 0.55),
                    (0.21, h),
                    (0.0, h),
                ],
                14,
                false,
                limestone(STONE_GREY),
            )),
            [x, y - 0.01, z],
            id_quat(),
        ),
        prim_scaled(
            sphere(0.17, 3, glow(FIRE_ORANGE, 3.0)),
            [x, y + h - 0.02, z],
            id_quat(),
            [1.0, 0.3, 1.0],
        ),
        Emitter {
            shape: EmitterShape::Cone {
                half_angle: Fp(0.15),
                height: Fp(0.1),
            },
            rate: 4.0,
            burst: 0,
            max: 24,
            life: (2.0, 3.5),
            speed: (0.2, 0.4),
            gravity: -0.04,
            accel: [0.04, 0.1, 0.0],
            drag: 0.6,
            size: (0.08, 0.45),
            start_color: [0.80, 0.78, 0.74, 0.3],
            end_color: [0.86, 0.85, 0.82, 0.0],
            blend: ParticleBlendMode::Alpha,
            sprite: crate::pds::SovereignTextureConfig::Puff(crate::pds::SovereignPuffConfig {
                seed: seed as u32,
                color_base: Fp3([0.82, 0.80, 0.76]),
                color_shadow: Fp3([0.52, 0.50, 0.47]),
                ..Default::default()
            }),
        }
        .at([x, y + h + 0.05, z], seed),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_a_family_gate() {
        super::super::assert_family_gate(&CorbelArchGateway);
    }
}
