//! Drone perch — a small Cyberpunk prop. A slim pole topped by a marked
//! landing pad, with a quad-rotor delivery drone hovering above it;
//! scattered through the settlement as street clutter.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, foundation_block, glow, id_quat, prim, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, NEON_LIME, NEON_MAGENTA, fx, metal};

pub struct DronePerch;

impl CatalogueEntry for DronePerch {
    fn slug(&self) -> &'static str {
        "drone_perch"
    }
    fn name(&self) -> &'static str {
        "Drone Perch"
    }
    fn description(&self) -> &'static str {
        "Pole-mounted landing pad with a hovering quad-rotor drone."
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
        solid(cuboid_tapered([1.4, slab_h, 1.4], 0.0, metal(body))),
        [0.0, slab_h * 0.5, 0.0],
        id_quat(),
    );
    let rel = |ground_y: f32| ground_y - slab_h * 0.5;

    let mut base = foundation_block(1.4, 1.4, [0.0, 0.0], 1.0);
    base.transform.translation.0[1] -= slab_h * 0.5;
    root.children.push(base);

    // Pole + a control box bolted to it.
    let pole_h = 2.6_f32;
    root.children.push(prim(
        solid(cylinder_tapered(0.12, pole_h, 8, 0.0, metal(body))),
        [0.0, rel(slab_h + pole_h * 0.5), 0.0],
        id_quat(),
    ));
    root.children.push(prim(
        solid(cuboid_tapered([0.3, 0.5, 0.24], 0.0, metal(body))),
        [0.0, rel(slab_h + pole_h * 0.45), 0.2],
        id_quat(),
    ));
    root.children.push(prim(
        sphere(0.05, 2, glow(NEON_LIME, 6.0)),
        [0.1, rel(slab_h + pole_h * 0.45 + 0.12), 0.33],
        id_quat(),
    ));

    // Landing pad: a disc with a glowing edge ring + cross marking.
    let pad_y = slab_h + pole_h;
    root.children.push(prim(
        solid(cylinder_tapered(0.7, 0.15, 16, 0.0, metal(body))),
        [0.0, rel(pad_y), 0.0],
        id_quat(),
    ));
    root.children.push(prim(
        torus(0.04, 0.6, glow(NEON_CYAN, 6.0)),
        [0.0, rel(pad_y + 0.09), 0.0],
        id_quat(),
    ));
    for (sx, sz) in [(1.0_f32, 0.0_f32), (0.0, 1.0)] {
        root.children.push(prim(
            cuboid_tapered(
                [0.9 * sx + 0.06, 0.03, 0.9 * sz + 0.06],
                0.0,
                glow(NEON_CYAN, 4.0),
            ),
            [0.0, rel(pad_y + 0.09), 0.0],
            id_quat(),
        ));
    }

    // ---- Quad-rotor drone hovering above the pad -------------------------
    let h = rel(pad_y + 0.95); // hover height
    // Body + a rounded sensor turret underneath.
    root.children.push(prim(
        solid(cuboid_tapered([0.46, 0.18, 0.46], 0.0, metal(body))),
        [0.0, h, 0.0],
        id_quat(),
    ));
    root.children.push(prim(
        sphere(0.08, 3, glow(NEON_MAGENTA, 8.0)),
        [0.0, h - 0.14, 0.0],
        id_quat(),
    ));
    // Four arms reaching out to the rotor pods (plus-quad layout).
    for (ax, az) in [(1.0_f32, 0.0_f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        root.children.push(prim(
            solid(cuboid_tapered(
                [0.5 * ax.abs() + 0.08, 0.05, 0.5 * az.abs() + 0.08],
                0.0,
                metal(body),
            )),
            [ax * 0.3, h, az * 0.3],
            id_quat(),
        ));
        // Rotor housing + a faint translucent rotor-blur disc above it.
        root.children.push(prim(
            solid(cylinder_tapered(0.17, 0.06, 12, 0.0, metal(body))),
            [ax * 0.55, h, az * 0.55],
            id_quat(),
        ));
        root.children.push(prim(
            cylinder_tapered(0.16, 0.015, 16, 0.0, glow(NEON_CYAN, 0.7)),
            [ax * 0.55, h + 0.05, az * 0.55],
            id_quat(),
        ));
    }
    // Port/starboard nav lights — red/green like a real aircraft.
    root.children.push(prim(
        sphere(0.04, 2, glow([1.0, 0.15, 0.1], 7.0)),
        [-0.55, h + 0.07, 0.0],
        id_quat(),
    ));
    let mut nav = prim(
        sphere(0.04, 2, glow([0.2, 1.0, 0.2], 7.0)),
        [0.55, h + 0.07, 0.0],
        id_quat(),
    );
    // Whirring rotors are the drone's signature sound.
    nav.audio = fx::drone_whir();
    root.children.push(nav);

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
        assert!(crate::catalogue::items::util::has_emissive(
            &DronePerch.build("")
        ));
    }
}
