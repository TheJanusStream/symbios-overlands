//! Tyre stack - the mark of a place for a car (#1782): three stacks of tyres
//! side by side, their tops painted white, at the outside of a track's
//! bend where a car that runs wide meets rubber.

use super::{PAINT_WHITE, RUBBER, paint, rubber};
use crate::catalogue::items::util::{cylinder_tapered, id_quat, nest, prim, solid, torus};
use crate::catalogue::{CatalogueEntry, Craft, Footprint, StructureRole};
use crate::pds::Generator;

/// How many tyres high a stack stands, and the stacks side by side.
const HIGH: usize = 4;
const STACKS: [f32; 3] = [-0.56, 0.0, 0.56];
/// A tyre's ring radius and its section's (m).
const RING: f32 = 0.18;
const SECTION: f32 = 0.095;

pub struct TyreStack;

impl CatalogueEntry for TyreStack {
    fn slug(&self) -> &'static str {
        "tyre_stack"
    }
    fn name(&self) -> &'static str {
        "Tyre stack"
    }
    fn description(&self) -> &'static str {
        "Three stacks of tyres, their tops painted white: the barrier at a car track's bend."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Port
    }
    fn serves(&self) -> Option<Craft> {
        Some(Craft::Car)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 10.0,
        }
    }
    fn build(&self, _local_did: &str) -> Generator {
        tyre_stack()
    }
}

/// The stacks (see the module docs), standing on the origin.
pub fn tyre_stack() -> Generator {
    let tall = SECTION * 2.0 * HIGH as f32;
    // A dark core filling each stack's bore, the root the tyres nest on.
    let core = |x: f32| {
        prim(
            solid(cylinder_tapered(RING, tall, 12, 0.0, paint(RUBBER))),
            [x, tall * 0.5, 0.0],
            id_quat(),
        )
    };
    let mut parts = Vec::new();
    for &x in &STACKS[1..] {
        parts.push(core(x));
    }
    for &x in &STACKS {
        for k in 0..HIGH {
            let material = if k + 1 == HIGH {
                paint(PAINT_WHITE)
            } else {
                rubber()
            };
            parts.push(prim(
                solid(torus(SECTION, RING, material)),
                [x, SECTION * (2 * k + 1) as f32, 0.0],
                id_quat(),
            ));
        }
    }
    nest(core(STACKS[0]), parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure;

    /// The stacks stand on the ground, HIGH tyres tall.
    #[test]
    fn the_stacks_stand_on_the_ground() {
        let bounds = measure::tree_bounds(&tyre_stack()).expect("it draws");
        assert!(bounds.min.y.abs() < 0.01, "{}", bounds.min.y);
        let tall = SECTION * 2.0 * HIGH as f32;
        assert!((bounds.max.y - tall).abs() < 0.01, "{}", bounds.max.y);
    }
}
