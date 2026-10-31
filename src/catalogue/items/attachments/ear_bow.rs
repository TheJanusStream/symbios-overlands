//! Ear bow - a beast's Crown (#1807, owner decision D9). Cut for a
//! four-legged body: a ribbon bow that stands on the skull between the ears,
//! scaled to the span between them (`src/player/beast_seat.rs`).

use crate::catalogue::items::util::{
    cuboid_tapered, id_quat, nest, prim, prim_scaled, quat_z, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole, WearFit};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};

/// The authored span between the ears, in worn metres - the declared measure,
/// about a dog's.
const EAR_SPAN: f32 = 0.14;

/// Ribbon red, with a satin's sheen.
const RIBBON: [f32; 3] = [0.72, 0.08, 0.16];

/// How far each loop leans up out of the level, in radians: a bow that droops
/// reads as a mistake at avatar distance.
const LOOP_LEAN: f32 = 0.30;

fn ribbon() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(RIBBON),
        roughness: Fp(0.35),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        ..Default::default()
    }
}

pub struct EarBow;

impl CatalogueEntry for EarBow {
    fn slug(&self) -> &'static str {
        "ear_bow"
    }
    fn name(&self) -> &'static str {
        "Ear Bow"
    }
    fn description(&self) -> &'static str {
        "A satin ribbon bow - cut for a four-legged body, it sits between the ears and fits \
         itself to them."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Attachment
    }
    fn wear_socket(&self) -> Option<symbios_avatar::Socket> {
        Some(symbios_avatar::Socket::Crown)
    }
    fn wear_fit(&self) -> Option<WearFit> {
        Some(WearFit::Beast { across: EAR_SPAN })
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.3,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// **Stands on the origin**, the bow's face on `+Z`: the knot sunk to the
/// origin as the root, two flattened loops leaning up out of it, and two short
/// tails falling behind the knot to rest on the skull. Every piece sinks into
/// the knot, intersecting and never flush.
fn build_tree() -> Generator {
    let knot = prim(
        solid(sphere(0.014, 2, ribbon())),
        [0.0, 0.015, 0.0],
        id_quat(),
    );
    let mut parts = Vec::new();
    for side in [-1.0f32, 1.0] {
        parts.push(prim_scaled(
            solid(sphere(0.034, 2, ribbon())),
            [side * 0.036, 0.024, 0.0],
            quat_z(side * LOOP_LEAN),
            [1.0, 0.58, 0.28],
        ));
        parts.push(prim(
            solid(cuboid_tapered([0.02, 0.03, 0.011], 0.2, ribbon())),
            [side * 0.012, 0.004, -0.006],
            quat_z(side * 0.5),
        ));
    }
    nest(knot, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize_and_draws_no_face_twice() {
        assert_sanitize_stable(&EarBow.build(""), "ear_bow");
        assert_no_z_fighting(&EarBow.build(""), "ear_bow");
        assert_nothing_floats(&EarBow.build(""), "ear_bow");
    }

    #[test]
    fn a_bow_is_worn_at_the_crown_and_declares_the_span_it_was_cut_for() {
        assert_eq!(EarBow.wear_socket(), Some(symbios_avatar::Socket::Crown));
        let fit = EarBow.wear_fit().expect("a beast's cut");
        assert_eq!(fit.beast_mm(), 140, "whole millimetres on the wire");
        assert_eq!(fit.band_mm(), 0, "no head band for a person's dresser");
    }
}
