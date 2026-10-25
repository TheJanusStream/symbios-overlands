//! Helipad - a place for a helicopter (#1782): a round concrete pad on a
//! skirt that reaches down to the ground round it, a yellow ring painted on
//! it, a white H across its middle and eight edge lights that are lit from
//! dusk to dawn.
//!
//! Its top stands [`TOP`] over its origin, which a seeded world stands on
//! the highest ground under it; its skirt reaches [`SKIRT`] under the
//! origin, so a pad on ground falling away that far shows no daylight - a
//! seeded one stands only where its ground falls no further
//! (`world_builder::compile::pad::place_stands`).

use super::{CONCRETE, PAINT_WHITE, PAINT_YELLOW, STEEL, concrete, lamp, paint, steel};
use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, id_quat, nest, prim, solid, tube,
};
use crate::catalogue::{CatalogueEntry, Craft, Footprint, StructureRole};
use crate::pds::Generator;

/// The pad's radius (m).
pub const RADIUS: f32 = 6.5;
/// How high its top stands over its origin (m).
pub const TOP: f32 = 0.3;
/// How far its skirt reaches under its origin (m).
pub const SKIRT: f32 = 4.0;

/// The ring's middle radius and width (m), and the H's arms.
const RING: [f32; 2] = [4.6, 0.45];
const H_TALL: f32 = 4.4;
const H_WIDE: f32 = 2.8;
const H_BAR: f32 = 0.55;
/// How thin a coat of paint stands (m): proud of the concrete, never in it.
const COAT: f32 = 0.012;
const LIGHTS: usize = 8;

pub struct Helipad;

impl CatalogueEntry for Helipad {
    fn slug(&self) -> &'static str {
        "helipad"
    }
    fn name(&self) -> &'static str {
        "Helipad"
    }
    fn description(&self) -> &'static str {
        "Round concrete pad with a painted ring and H and edge lights lit from dusk to \
         dawn: a place to set a helicopter down."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Port
    }
    fn serves(&self) -> Option<Craft> {
        Some(Craft::Helicopter)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: RADIUS + 0.5,
            min_spawn_dist: 20.0,
        }
    }
    fn build(&self, _local_did: &str) -> Generator {
        helipad()
    }
}

/// The pad (see the module docs).
pub fn helipad() -> Generator {
    let height = TOP + SKIRT;
    let slab = prim(
        solid(cylinder_tapered(
            RADIUS,
            height,
            48,
            0.0,
            concrete(CONCRETE),
        )),
        [0.0, TOP - height * 0.5, 0.0],
        id_quat(),
    );
    let coat_y = TOP + COAT * 0.5;
    let mut parts = vec![prim(
        tube(
            RING[0] + RING[1] * 0.5,
            RING[0] - RING[1] * 0.5,
            COAT,
            48,
            paint(PAINT_YELLOW),
        ),
        [0.0, coat_y, 0.0],
        id_quat(),
    )];
    // The H: two uprights along Z and the bar between them.
    for side in [-1.0, 1.0] {
        parts.push(prim(
            cuboid_tapered([H_BAR, COAT, H_TALL], 0.0, paint(PAINT_WHITE)),
            [side * (H_WIDE - H_BAR) * 0.5, coat_y, 0.0],
            id_quat(),
        ));
    }
    parts.push(prim(
        cuboid_tapered([H_WIDE - H_BAR * 2.0, COAT, H_BAR], 0.0, paint(PAINT_WHITE)),
        [0.0, coat_y, 0.0],
        id_quat(),
    ));
    // The edge lights, round its rim: a steel base and a lit dome.
    for k in 0..LIGHTS {
        let a = k as f32 * std::f32::consts::TAU / LIGHTS as f32;
        // libm (#1132): where a light stands is in the record every peer
        // draws.
        let (x, z) = (
            libm::sinf(a) * (RADIUS - 0.3),
            libm::cosf(a) * (RADIUS - 0.3),
        );
        parts.push(prim(
            cylinder_tapered(0.09, 0.08, 10, 0.0, steel(STEEL)),
            [x, TOP + 0.04, z],
            id_quat(),
        ));
        parts.push(prim(
            cylinder_tapered(0.07, 0.07, 10, 0.4, lamp([0.55, 1.0, 0.6], 4.0)),
            [x, TOP + 0.115, z],
            id_quat(),
        ));
    }
    nest(slab, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure;

    /// Its top stands at TOP and its skirt reaches SKIRT under its origin;
    /// its paint stands proud of the concrete, and its lights glow only from
    /// dusk to dawn.
    #[test]
    fn the_pad_stands_its_top_and_its_skirt_and_lights_the_night() {
        let pad = helipad();
        let slab = &measure::solids(&pad)
            .into_iter()
            .max_by(|a, b| a.bounds.max.x.total_cmp(&b.bounds.max.x))
            .expect("it draws")
            .bounds;
        assert!((slab.max.y - TOP).abs() < 1e-3, "top {}", slab.max.y);
        assert!((slab.min.y + SKIRT).abs() < 1e-3, "skirt {}", slab.min.y);
        assert!((slab.max.x - RADIUS).abs() < 0.01);
        let bounds = measure::tree_bounds(&pad).expect("it draws");
        assert!(bounds.max.y > TOP, "the lights stand on it");
        let slab_y = pad.transform.translation.0[1];
        for child in &pad.children {
            let material = child.kind.material().expect("a primitive");
            if material.emission_strength.0 > 0.0 {
                assert!(material.dusk_to_dawn, "a light lit at noon");
            }
            let y = slab_y + child.transform.translation.0[1];
            assert!(y > TOP, "every coat and light stands proud: {y}");
        }
    }
}
