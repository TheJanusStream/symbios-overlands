//! Windsock - the mark of a place for an airplane (#1782): a white pole with
//! a red top on a concrete footing, and on a swivel at its head a sock in
//! five orange and white bands, which turns downwind in the world's own
//! wind - where a pilot reads it from beside an airstrip.
//!
//! The sock is a vane (`SpinTerm::Vane`): every client turns it for itself,
//! so it is drawn without a collider.

use std::f32::consts::FRAC_PI_2;

use super::{CONCRETE, PAINT_WHITE, SIGNAL_RED, SOCK_ORANGE, STEEL, concrete, paint, steel};
use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, id_quat, nest, prim, quat_z, solid, strut, torus, turning,
    vane,
};
use crate::catalogue::{CatalogueEntry, Craft, Footprint, StructureRole};
use crate::pds::Generator;

/// The pole's height over its footing (m).
pub const POLE: f32 = 5.0;
/// The sock's length, its mouth's radius and its tail's (m).
pub const SOCK: f32 = 2.4;
const MOUTH: f32 = 0.32;
const TAIL: f32 = 0.12;
const BANDS: usize = 5;
const FOOT_T: f32 = 0.3;
/// How far from the swivel's axis the sock's mouth hangs (m).
const ARM: f32 = 0.25;

pub struct Windsock;

impl CatalogueEntry for Windsock {
    fn slug(&self) -> &'static str {
        "windsock"
    }
    fn name(&self) -> &'static str {
        "Windsock"
    }
    fn description(&self) -> &'static str {
        "Banded windsock on a swivel atop a white pole: it turns downwind in the world's \
         wind, beside a place to land an airplane."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Port
    }
    fn serves(&self) -> Option<Craft> {
        Some(Craft::Airplane)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 10.0,
        }
    }
    fn build(&self, _local_did: &str) -> Generator {
        windsock()
    }
}

/// The windsock (see the module docs).
pub fn windsock() -> Generator {
    let footing = prim(
        solid(cuboid_tapered([0.6, FOOT_T, 0.6], 0.15, concrete(CONCRETE))),
        [0.0, FOOT_T * 0.5 - 0.2, 0.0],
        id_quat(),
    );
    let foot_top = FOOT_T - 0.2;
    let pole_top = foot_top + POLE;
    let red = 0.6;
    let pole = prim(
        solid(cylinder_tapered(
            0.06,
            POLE - red,
            10,
            0.05,
            paint(PAINT_WHITE),
        )),
        [0.0, foot_top + (POLE - red) * 0.5, 0.0],
        id_quat(),
    );
    let cap = prim(
        solid(cylinder_tapered(0.057, red, 10, 0.05, paint(SIGNAL_RED))),
        [0.0, pole_top - red * 0.5, 0.0],
        id_quat(),
    );

    // The swivel on the pole's head, turning the sock downwind: its arm
    // out to the mouth's hoop, and the sock in bands along +X.
    let hub_y = pole_top + 0.05;
    let hub = prim(
        cylinder_tapered(0.08, 0.1, 10, 0.0, steel(STEEL)),
        [0.0, hub_y, 0.0],
        id_quat(),
    );
    let mut sock = vec![
        strut([0.0, hub_y, 0.0], [ARM, hub_y, 0.0], 0.025, 6, steel(STEEL)),
        prim(
            torus(0.02, MOUTH, steel(STEEL)),
            [ARM, hub_y, 0.0],
            quat_z(FRAC_PI_2),
        ),
    ];
    let band = SOCK / BANDS as f32;
    for k in 0..BANDS {
        let r0 = MOUTH + (TAIL - MOUTH) * k as f32 / BANDS as f32;
        let r1 = MOUTH + (TAIL - MOUTH) * (k + 1) as f32 / BANDS as f32;
        let colour = if k % 2 == 0 { SOCK_ORANGE } else { PAINT_WHITE };
        sock.push(prim(
            cylinder_tapered(r0, band, 12, 1.0 - r1 / r0, paint(colour)),
            [ARM + band * (k as f32 + 0.5), hub_y, 0.0],
            // Its top - the narrower end - turned from +Y to +X: the tail.
            quat_z(-FRAC_PI_2),
        ));
    }
    let swivel = turning(hub, [0.0, 1.0, 0.0], vane([1.0, 0.0, 0.0]), sock);
    nest(footing, vec![pole, cap, swivel])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure;

    /// The sock reaches out its length from the pole's head, turned by the
    /// wind about the pole, and the pole stands its height.
    #[test]
    fn the_sock_turns_downwind_off_the_pole_s_head() {
        let sock = windsock();
        let bounds = measure::tree_bounds(&sock).expect("it draws");
        assert!(
            (bounds.max.x - (ARM + SOCK)).abs() < 0.05,
            "the tail at {}",
            bounds.max.x
        );
        let swivel = sock.children.last().expect("the swivel");
        let spin = swivel.spin.as_ref().expect("it turns");
        assert!(matches!(
            spin.terms[..],
            [crate::pds::SpinTerm::Vane { .. }]
        ));
        let head = sock.transform.translation.0[1] + swivel.transform.translation.0[1];
        assert!((head - (FOOT_T - 0.2 + POLE + 0.05)).abs() < 1e-4, "{head}");
    }
}
