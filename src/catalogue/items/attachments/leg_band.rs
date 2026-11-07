//! Leg band - a winged body's foot (#1879, the owner's choice). Cut for an
//! animal's leg, as a ringed bird's is: an enamelled band with a brass rim at
//! each edge and a small brass plate, which the fitted seat scales across and
//! through to the leg it lands on - a bird's tarsus, a bat's or a glider's
//! shin (`src/player/winged_seat.rs`). Placed as world decor it lies flat at
//! this authored size.
//!
//! Drawn at [`DRAFT`]x, the collar's technique: the sanitiser's floors are
//! prim-local, and the band's wall and the rims are thinner than them worn.

use crate::catalogue::items::util::{
    cuboid_tapered, id_quat, nest, prim, prim_scaled, solid, torus, tube, uv_for_scale,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole, WearFit};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};

use super::brass;

/// The band's authored bore, in worn metres - the declared measure, an
/// owl's feathered tarsus.
const BORE: f32 = 0.035;

/// The size the band is drawn at, relative to the size it is worn at.
const DRAFT: f32 = 10.0;

/// The enamel, a ringer's blue.
const ENAMEL: [f32; 3] = [0.10, 0.30, 0.72];

fn enamel() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(ENAMEL),
        roughness: Fp(0.30),
        metallic: Fp(0.10),
        uv_scale: Fp(1.0),
        ..Default::default()
    }
}

pub struct LegBand;

impl CatalogueEntry for LegBand {
    fn slug(&self) -> &'static str {
        "leg_band"
    }
    fn name(&self) -> &'static str {
        "Leg Band"
    }
    fn description(&self) -> &'static str {
        "An enamelled leg band with brass rims - cut for a winged body, it fits itself to the \
         leg it lands on."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Attachment
    }
    fn wear_socket(&self) -> Option<symbios_avatar::Socket> {
        Some(symbios_avatar::Socket::LeftFoot)
    }
    fn wear_fit(&self) -> Option<WearFit> {
        Some(WearFit::Beast { across: BORE })
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// **The band circles the origin**, in the X-Z plane, its `+Z` the side the
/// seat turns out of the body: a rim rings each edge, standing proud of the
/// wall inside and out, and a plate sinks into the wall at `+Z`. The root is
/// the band, at the origin, carrying the one downscale.
fn build_tree() -> Generator {
    let scale = 1.0 / DRAFT;
    let bore = BORE * DRAFT / 2.0;
    let wall = 0.03;
    let height = 0.12;
    let outer = bore + wall;
    let band = prim_scaled(
        solid(tube(outer, bore, height, 28, uv_for_scale(enamel(), scale))),
        [0.0, 0.0, 0.0],
        id_quat(),
        [scale; 3],
    );
    let metal = || uv_for_scale(brass(), scale);
    let mut parts = Vec::new();
    for edge in [-1.0f32, 1.0] {
        parts.push(prim(
            solid(torus(0.022, bore + wall * 0.5, metal())),
            [0.0, edge * height * 0.46, 0.0],
            id_quat(),
        ));
    }
    parts.push(prim(
        solid(cuboid_tapered([0.09, 0.06, 0.03], 0.0, metal())),
        [0.0, 0.0, outer + 0.005],
        id_quat(),
    ));
    nest(band, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize_and_draws_no_face_twice() {
        assert_sanitize_stable(&LegBand.build(""), "leg_band");
        assert_no_z_fighting(&LegBand.build(""), "leg_band");
        assert_nothing_floats(&LegBand.build(""), "leg_band");
    }

    #[test]
    fn a_band_is_worn_at_a_foot_and_declares_its_bore() {
        assert_eq!(
            LegBand.wear_socket(),
            Some(symbios_avatar::Socket::LeftFoot)
        );
        let fit = LegBand.wear_fit().expect("an animal's cut");
        assert_eq!(fit.beast_mm(), 35, "whole millimetres on the wire");
        assert_eq!(fit.band_mm(), 0, "no head band for a person's dresser");
    }
}
