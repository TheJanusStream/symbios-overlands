//! Drone perch — a small Cyberpunk prop. A slim pole topped by a landing
//! pad and a hovering, glowing delivery drone; scattered through the
//! settlement as street clutter.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, foundation_block, glow, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_MAGENTA, fx, metal};

pub struct DronePerch;

impl CatalogueEntry for DronePerch {
    fn slug(&self) -> &'static str {
        "drone_perch"
    }
    fn name(&self) -> &'static str {
        "Drone Perch"
    }
    fn description(&self) -> &'static str {
        "Pole-mounted landing pad with a hovering glowing drone."
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
            clearance: 1.5,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let body = DARK_METAL;
    let slab_h = 0.2;

    let mut root = prim(
        solid(cuboid_tapered([1.2, slab_h, 1.2], 0.0, metal(body))),
        [0.0, slab_h * 0.5, 0.0],
        id_quat(),
    );
    let rel = |ground_y: f32| ground_y - slab_h * 0.5;

    let mut base = foundation_block(1.2, 1.2, [0.0, 0.0], 1.0);
    base.transform.translation.0[1] -= slab_h * 0.5;
    root.children.push(base);

    // Pole.
    let pole_h = 2.6;
    root.children.push(prim(
        solid(cylinder_tapered(0.12, pole_h, 8, 0.0, metal(body))),
        [0.0, rel(slab_h + pole_h * 0.5), 0.0],
        id_quat(),
    ));
    // Landing pad disc.
    root.children.push(prim(
        solid(cylinder_tapered(0.7, 0.15, 12, 0.0, metal(body))),
        [0.0, rel(slab_h + pole_h), 0.0],
        id_quat(),
    ));
    // Hovering glowing drone — whirring rotors are its signature sound.
    let mut drone = prim(
        sphere(0.35, 3, glow(NEON_MAGENTA, 8.0)),
        [0.0, rel(slab_h + pole_h + 0.7), 0.0],
        id_quat(),
    );
    drone.audio = fx::drone_whir();
    root.children.push(drone);

    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&DronePerch.build(""), "drone_perch");
    }

    #[test]
    fn has_neon() {
        assert!(super::super::has_emissive(&DronePerch.build("")));
    }
}
