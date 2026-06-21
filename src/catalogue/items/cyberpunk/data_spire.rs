//! Data spire — a slim Cyberpunk secondary. A tall, sharply-tapered
//! round dark-metal needle orbited by a glowing double-helix data stream,
//! banded with data rings, haloed by a hollow data ring near the crown and
//! capped with a glowing dome beacon. Reads as comms / server infrastructure
//! ringing the megatower.

use std::f32::consts::PI;

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, foundation_block, glow, helix, id_quat, prim, quat_y, solid,
    sphere, torus, tube, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, NEON_MAGENTA, fx, metal};

pub struct DataSpire;

impl CatalogueEntry for DataSpire {
    fn slug(&self) -> &'static str {
        "data_spire"
    }
    fn name(&self) -> &'static str {
        "Data Spire"
    }
    fn description(&self) -> &'static str {
        "Slim tapered needle orbited by a glowing double-helix data stream."
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
            clearance: 5.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let body = DARK_METAL;
    let slab_h = 0.4;

    let mut root = prim(
        solid(cuboid_tapered([3.0, slab_h, 3.0], 0.0, metal(body))),
        [0.0, slab_h * 0.5, 0.0],
        id_quat(),
    );
    let rel = |ground_y: f32| ground_y - slab_h * 0.5;

    let mut base = foundation_block(3.0, 3.0, [0.0, 0.0], 2.0);
    base.transform.translation.0[1] -= slab_h * 0.5;
    root.children.push(base);

    // Tall tapered *round* needle — a cylinder (not a box), so the helix coil
    // clears it cleanly at every height instead of stabbing through the
    // corners of a square shaft.
    let spire_h = 18.0_f32;
    let needle_r0 = 0.72_f32;
    let needle_taper = 0.32;
    // Needle radius at height fraction `t` (0 = base, 1 = crown).
    let needle_r = |t: f32| needle_r0 * (1.0 - needle_taper * t);
    root.children.push(prim(
        solid(cylinder_tapered(
            needle_r0,
            spire_h,
            16,
            needle_taper,
            metal(body),
        )),
        [0.0, rel(slab_h + spire_h * 0.5), 0.0],
        id_quat(),
    ));

    // Glowing double-helix data stream orbiting the needle — two counter-
    // phased strands (offset half a turn) reading as a rising data feed. Its
    // radius clears the needle's widest point within the coil span (plus the
    // wire thickness), so it orbits the mast without ever intersecting it.
    let coil_turns = 6.0_f32;
    let coil_pitch = 2.2_f32;
    let coil_h = coil_turns * coil_pitch;
    let coil_y0 = slab_h + 2.4;
    let coil_center = coil_y0 + coil_h * 0.5;
    let coil_wire = 0.09_f32;
    let coil_r = needle_r((coil_y0 - slab_h) / spire_h) + coil_wire + 0.13;
    for (phase, c) in [(0.0_f32, NEON_CYAN), (PI, NEON_MAGENTA)] {
        root.children.push(prim(
            helix(coil_r, coil_wire, coil_pitch, coil_turns, 20, glow(c, 6.0)),
            [0.0, rel(coil_center), 0.0],
            quat_y(phase),
        ));
    }

    // Accent data-band rings hugging the needle at a few heights.
    let rings = 3;
    for k in 0..rings {
        let t = (k as f32 + 0.5) / rings as f32;
        let ring_r = needle_r(t) + 0.12;
        root.children.push(prim(
            torus(0.07, ring_r, glow(NEON_CYAN, 6.0)),
            [0.0, rel(slab_h + t * spire_h), 0.0],
            id_quat(),
        ));
    }

    // A hollow data-halo ring floating around the crown.
    root.children.push(prim(
        tube(1.05, 0.9, 0.16, 28, glow(NEON_CYAN, 6.0)),
        [0.0, rel(slab_h + spire_h - 2.6), 0.0],
        id_quat(),
    ));

    // Glowing dome beacon cap (a profile-cut upper hemisphere), a slim antenna
    // mast, and a tip beacon orb.
    let top = slab_h + spire_h;
    root.children.push(prim(
        with_cut(
            sphere(0.62, 3, glow(NEON_CYAN, 8.0)),
            [0.0, 1.0],
            [0.5, 1.0],
            0.0,
        ),
        [0.0, rel(top), 0.0],
        id_quat(),
    ));
    let mast_h = 2.4_f32;
    root.children.push(prim(
        solid(cylinder_tapered(0.08, mast_h, 6, 0.3, metal(body))),
        [0.0, rel(top + 0.62 + mast_h * 0.5), 0.0],
        id_quat(),
    ));
    root.children.push(prim(
        sphere(0.28, 3, glow(NEON_MAGENTA, 10.0)),
        [0.0, rel(top + 0.62 + mast_h + 0.2), 0.0],
        id_quat(),
    ));

    // Signature life: faint data static drifting up the upper spire.
    root.children.push(fx::rising_motes(
        [0.0, rel(slab_h + spire_h * 0.62), 0.0],
        NEON_CYAN,
        0xDA7A_5217,
    ));

    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&DataSpire.build(""), "data_spire");
    }

    #[test]
    fn has_neon() {
        assert!(crate::catalogue::items::util::has_emissive(
            &DataSpire.build("")
        ));
    }
}
