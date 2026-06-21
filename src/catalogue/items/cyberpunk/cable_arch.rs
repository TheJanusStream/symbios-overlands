//! Cable gantry — a Cyberpunk street prop. Two heavy utility pylons carrying
//! an overhead cable tray of bundled power conduits across a walkway, hung
//! with junction boxes, a caged worklight, and routed cable drops. Grimy
//! functional infrastructure rather than decorative trim; frames the gaps
//! between the bigger structures.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, foundation_block, glow, id_quat, prim, quat_z, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, NEON_LIME, NEON_MAGENTA, fx, metal};

pub struct CableArch;

/// Warning-amber for hazard banding — the one warm note in the cold neon kit.
const HAZARD: [f32; 3] = [1.0, 0.62, 0.08];

impl CatalogueEntry for CableArch {
    fn slug(&self) -> &'static str {
        "cable_arch"
    }
    fn name(&self) -> &'static str {
        "Cable Gantry"
    }
    fn description(&self) -> &'static str {
        "Twin utility pylons carrying an overhead bundle of power conduits."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CYBER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// A horizontal pipe running left-to-right along X (a Y-axis cylinder laid on
/// its side), length `len`.
fn conduit(radius: f32, len: f32, mat: crate::pds::SovereignMaterialSettings) -> Generator {
    prim(
        cylinder_tapered(radius, len, 10, 0.0, mat),
        [0.0, 0.0, 0.0],
        quat_z(FRAC_PI_2),
    )
}

fn build_tree() -> Generator {
    let body = DARK_METAL;
    let slab_h = 0.2;
    let foot = 5.2_f32;
    let depth = 1.6_f32;
    let px = 2.2_f32; // pylon x
    let pyl_h = 4.0_f32;
    let pw = 0.55_f32; // pylon width (X)
    let pd = 0.7_f32; // pylon depth (Z)
    let top = slab_h + pyl_h; // gantry springing height

    // Podium slab — the root; its base sits at the generator origin.
    let mut root = prim(
        solid(cuboid_tapered([foot, slab_h, depth], 0.0, metal(body))),
        [0.0, slab_h * 0.5, 0.0],
        id_quat(),
    );
    let rel = |ground_y: f32| ground_y - slab_h * 0.5;

    let mut base = foundation_block(foot, depth, [0.0, 0.0], 1.5);
    base.transform.translation.0[1] -= slab_h * 0.5;
    root.children.push(base);

    // ---- Pylons -----------------------------------------------------------
    for (side, sx) in [(0usize, -1.0_f32), (1, 1.0)] {
        // Splayed base plate.
        root.children.push(prim(
            solid(cuboid_tapered([pw + 0.5, 0.34, pd + 0.4], 0.0, metal(body))),
            [sx * px, rel(slab_h + 0.17), 0.0],
            id_quat(),
        ));
        // Column.
        root.children.push(prim(
            solid(cuboid_tapered([pw, pyl_h, pd], 0.0, metal(body))),
            [sx * px, rel(slab_h + pyl_h * 0.5), 0.0],
            id_quat(),
        ));
        // Hazard band near the base.
        root.children.push(prim(
            cuboid_tapered([pw + 0.07, 0.3, pd + 0.07], 0.0, glow(HAZARD, 2.5)),
            [sx * px, rel(slab_h + 0.95), 0.0],
            id_quat(),
        ));

        // Junction box bolted to the outer face, with a column of status LEDs.
        // One box carries the transformer hum.
        let box_x = sx * (px + pw * 0.5 + 0.17);
        let mut jbox = prim(
            solid(cuboid_tapered([0.34, 0.85, 0.5], 0.0, metal(body))),
            [box_x, rel(slab_h + pyl_h * 0.52), 0.0],
            id_quat(),
        );
        if side == 0 {
            jbox.audio = fx::transformer_hum();
        }
        root.children.push(jbox);
        let led_x = sx * (px + pw * 0.5 + 0.35);
        for (j, c) in [NEON_CYAN, NEON_LIME, NEON_MAGENTA].into_iter().enumerate() {
            let dy = 0.25 - 0.25 * j as f32;
            root.children.push(prim(
                sphere(0.055, 2, glow(c, 6.0)),
                [led_x, rel(slab_h + pyl_h * 0.52 + dy), 0.0],
                id_quat(),
            ));
        }

        // Two cable drops routing down the outer face from the gantry to the
        // junction box; the rear one is frayed and spits the occasional spark.
        for (d, (dz, c, lit)) in [(-0.2_f32, NEON_CYAN, false), (0.2, NEON_CYAN, true)]
            .into_iter()
            .enumerate()
        {
            let drop_top = top;
            let drop_bot = slab_h + pyl_h * 0.52 + 0.4;
            let drop_h = drop_top - drop_bot;
            let drop_x = sx * (px + pw * 0.5 + 0.09);
            let mat = if lit {
                glow(c, 3.5)
            } else {
                metal(shade_body(body))
            };
            root.children.push(prim(
                cylinder_tapered(0.06, drop_h, 8, 0.0, mat),
                [drop_x, rel((drop_top + drop_bot) * 0.5), dz],
                id_quat(),
            ));
            if side == 0 && d == 0 {
                root.children
                    .push(fx::spark_burst([drop_x, rel(drop_bot), dz], 0xCAB1_5A1A));
            }
        }
    }

    // ---- Overhead gantry + cable tray ------------------------------------
    // Cross girder spanning pylon-top to pylon-top.
    root.children.push(prim(
        solid(cuboid_tapered([foot + 0.2, 0.42, 0.62], 0.0, metal(body))),
        [0.0, rel(top + 0.21), 0.0],
        id_quat(),
    ));
    // Cable-tray floor + side rails riding on the girder.
    root.children.push(prim(
        solid(cuboid_tapered([foot, 0.1, 0.74], 0.0, metal(body))),
        [0.0, rel(top + 0.47), 0.0],
        id_quat(),
    ));
    for sz in [-1.0_f32, 1.0] {
        root.children.push(prim(
            cuboid_tapered([foot, 0.16, 0.06], 0.0, metal(body)),
            [0.0, rel(top + 0.55), sz * 0.36],
            id_quat(),
        ));
    }

    // Bundled power conduits running the length of the tray — dark pipes, two
    // of them carrying a thin glowing data line.
    let lo = body;
    let mut push_conduit = |y: f32, z: f32, r: f32, m| {
        let mut c = conduit(r, foot - 0.1, m);
        c.transform.translation = crate::pds::Fp3([0.0, rel(y), z]);
        root.children.push(c);
    };
    push_conduit(top + 0.6, -0.22, 0.12, metal(lo));
    push_conduit(top + 0.62, 0.04, 0.13, metal(lo));
    push_conduit(top + 0.6, 0.27, 0.11, metal(lo));
    push_conduit(top + 0.84, 0.0, 0.08, metal(lo));
    // Glowing data lines hugging the front of two conduits.
    push_conduit(top + 0.62, 0.18, 0.045, glow(NEON_CYAN, 6.0));
    push_conduit(top + 0.92, 0.0, 0.035, glow(NEON_MAGENTA, 6.0));

    // ---- Caged worklight hung under the girder ---------------------------
    root.children.push(prim(
        solid(cylinder_tapered(0.04, 0.45, 6, 0.0, metal(body))),
        [0.0, rel(top - 0.22), 0.0],
        id_quat(),
    ));
    root.children.push(prim(
        cylinder_tapered(0.26, 0.1, 12, 0.0, metal(body)),
        [0.0, rel(top - 0.48), 0.0],
        id_quat(),
    ));
    root.children.push(prim(
        sphere(0.17, 3, glow([1.0, 0.95, 0.8], 7.0)),
        [0.0, rel(top - 0.64), 0.0],
        id_quat(),
    ));

    root
}

/// A darker shade of the body colour for unlit cable runs.
fn shade_body(c: [f32; 3]) -> [f32; 3] {
    [c[0] * 0.7, c[1] * 0.7, c[2] * 0.7]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CableArch.build(""), "cable_arch");
    }

    #[test]
    fn has_neon() {
        assert!(crate::catalogue::items::util::has_emissive(
            &CableArch.build("")
        ));
    }
}
