//! Data spire — a slim Cyberpunk secondary. A tall, sharply-tapered
//! dark-metal needle climbed by stacked emissive "data rings" and
//! capped with a glowing orb. Reads as comms / server infrastructure
//! ringing the megatower.

use crate::catalogue::items::util::{
    cuboid_tapered, foundation_block, glow, id_quat, prim, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, fx, metal};

pub struct DataSpire;

impl CatalogueEntry for DataSpire {
    fn slug(&self) -> &'static str {
        "data_spire"
    }
    fn name(&self) -> &'static str {
        "Data Spire"
    }
    fn description(&self) -> &'static str {
        "Slim tapered needle ringed with glowing data bands."
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

    // Tall tapered needle.
    let spire_h = 18.0;
    let spire_taper = 0.5;
    let base_w = 2.2;
    root.children.push(prim(
        solid(cuboid_tapered(
            [base_w, spire_h, base_w],
            spire_taper,
            metal(body),
        )),
        [0.0, rel(slab_h + spire_h * 0.5), 0.0],
        id_quat(),
    ));

    // Glowing data rings climbing the needle, radius following the taper.
    let rings = 6;
    for k in 0..rings {
        let t = (k as f32 + 0.5) / rings as f32;
        let ring_r = base_w * 0.5 * (1.0 - spire_taper * t) + 0.35;
        root.children.push(prim(
            torus(0.1, ring_r, glow(NEON_CYAN, 6.0)),
            [0.0, rel(slab_h + t * spire_h), 0.0],
            id_quat(),
        ));
    }

    // Cap orb.
    root.children.push(prim(
        sphere(0.55, 3, glow(NEON_CYAN, 9.0)),
        [0.0, rel(slab_h + spire_h + 0.3), 0.0],
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
