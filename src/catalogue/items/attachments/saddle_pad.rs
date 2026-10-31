//! Saddle pad - a beast's Back (#1807, owner decision D9). Cut for a
//! four-legged body: an arc of quilted cloth lying along the spine, which the
//! fitted seat scales across and down to the barrel it lands on, with a low
//! leather seat over the spine and a contrasting edge at each end
//! (`src/player/beast_seat.rs`).

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    id_quat, nest, prim, quat_x, solid, superellipsoid, tube, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole, WearFit};
use crate::pds::{Fp, Fp3, Generator, GeneratorKind, SovereignMaterialSettings};

use super::leather;

/// The arc's authored inner width, in worn metres - the declared measure.
const WIDTH: f32 = 0.60;

/// Half of the arc the cloth keeps, in turns of its circle either side of the
/// spine: a little under a quarter turn each way, so its edges fall down the
/// flanks and stop well short of the belly.
const CLOTH_ARC: f32 = 0.17;

/// The same for the edging, a touch longer so its ends stand clear of the
/// cloth's, never flush with them.
const TRIM_ARC: f32 = 0.175;

/// Facets of the cloth's arc and of an edging's: the whole wearable budget is
/// spent on a sweep's walls, and the seat gets the difference.
const CLOTH_FACETS: u32 = 10;
const TRIM_FACETS: u32 = 6;

/// How long the pad runs along the spine.
const LENGTH: f32 = 0.46;

/// How far under its inner radius the cloth's wall runs, into the barrel: the
/// barrel narrows toward the shoulder and the flank, and a wall on the fitted
/// radius shows daylight under the pad there.
const SUNK: f32 = 0.07;

/// Deep madder cloth.
const CLOTH: [f32; 3] = [0.38, 0.07, 0.08];
/// The edging, a pale wool.
const EDGE: [f32; 3] = [0.78, 0.68, 0.46];
/// The seat's leather.
const SEAT: [f32; 3] = [0.24, 0.13, 0.07];

fn wool(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.92),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        ..Default::default()
    }
}

pub struct SaddlePad;

impl CatalogueEntry for SaddlePad {
    fn slug(&self) -> &'static str {
        "saddle_pad"
    }
    fn name(&self) -> &'static str {
        "Saddle Pad"
    }
    fn description(&self) -> &'static str {
        "A quilted cloth with a low leather seat - cut for a four-legged body, it fits itself \
         to the back it lands on."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Attachment
    }
    fn wear_socket(&self) -> Option<symbios_avatar::Socket> {
        Some(symbios_avatar::Socket::Back)
    }
    fn wear_fit(&self) -> Option<WearFit> {
        Some(WearFit::Beast { across: WIDTH })
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.6,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// An arc of a tube along `Z` about a centre below the origin, kept
/// `half_arc` turns either side of the top. A quarter turn about `X` stands a
/// tube's axis along `Z` and carries its `+Z` side up, and a tube's sweep is
/// at a quarter turn on its `+Z` side.
fn arc(
    outer: f32,
    inner: f32,
    length: f32,
    half_arc: f32,
    facets: u32,
    z: f32,
    material: SovereignMaterialSettings,
) -> Generator {
    prim(
        solid(with_cut(
            tube(outer, inner, length, facets, material),
            [0.25 - half_arc, 0.25 + half_arc],
            [0.0, 1.0],
            0.0,
        )),
        [0.0, -WIDTH / 2.0, z],
        quat_x(-FRAC_PI_2),
    )
}

/// **The underside runs through the origin at the spine**, the arc falling
/// away below `y = 0` and along `Z`. The seat is the root: a flattened dome
/// sunk through the cloth, axis-aligned, because a tilted root would spin
/// everything on it.
fn build_tree() -> Generator {
    let radius = WIDTH / 2.0;
    let mut dome = superellipsoid([0.13, 0.036, 0.16], 1.0, 1.0, leather(SEAT));
    if let GeneratorKind::Superellipsoid {
        latitudes,
        longitudes,
        ..
    } = &mut dome
    {
        *latitudes = 6;
        *longitudes = 12;
    }
    let seat = prim(solid(dome), [0.0, 0.022, 0.0], id_quat());
    nest(
        seat,
        vec![
            arc(
                radius + 0.018,
                radius - SUNK,
                LENGTH,
                CLOTH_ARC,
                CLOTH_FACETS,
                0.0,
                wool(CLOTH),
            ),
            arc(
                radius + 0.024,
                radius - SUNK + 0.01,
                0.05,
                TRIM_ARC,
                TRIM_FACETS,
                LENGTH / 2.0 - 0.02,
                wool(EDGE),
            ),
            arc(
                radius + 0.024,
                radius - SUNK + 0.01,
                0.05,
                TRIM_ARC,
                TRIM_FACETS,
                -LENGTH / 2.0 + 0.02,
                wool(EDGE),
            ),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize_and_draws_no_face_twice() {
        assert_sanitize_stable(&SaddlePad.build(""), "saddle_pad");
        assert_no_z_fighting(&SaddlePad.build(""), "saddle_pad");
        assert_nothing_floats(&SaddlePad.build(""), "saddle_pad");
    }

    #[test]
    fn a_pad_is_worn_on_the_back_and_declares_its_width() {
        assert_eq!(SaddlePad.wear_socket(), Some(symbios_avatar::Socket::Back));
        let fit = SaddlePad.wear_fit().expect("a beast's cut");
        assert_eq!(fit.beast_mm(), 600, "whole millimetres on the wire");
        assert_eq!(fit.band_mm(), 0, "no head band for a person's dresser");
    }

    /// The cloth's arc is on top of the origin and falls down both flanks:
    /// the sweep's zero and the quarter turn are easy to get a half turn out.
    #[test]
    fn the_cloth_lies_over_the_origin_and_not_under_it() {
        let tree = SaddlePad.build("");
        let cloth = &tree.children[0];
        let mesh = crate::world_builder::build_primitive_mesh(&cloth.kind).mesh;
        let positions = mesh
            .attribute(bevy::mesh::Mesh::ATTRIBUTE_POSITION)
            .and_then(|p| p.as_float3())
            .expect("positions");
        let turn = bevy::prelude::Quat::from_array(cloth.transform.rotation.0);
        let (mut low, mut high, mut side) = (f32::MAX, f32::MIN, 0.0f32);
        for p in positions {
            let worn = turn * bevy::prelude::Vec3::from_array(*p);
            let y = worn.y - WIDTH / 2.0;
            low = low.min(y);
            high = high.max(y);
            side = side.max(worn.x.abs());
        }
        assert!(
            high > 0.0 && high < 0.03,
            "the cloth's top stands {high:.3} m over the origin"
        );
        assert!(low < -0.1, "it falls only {low:.3} m down the flank");
        assert!(side > 0.2, "it covers only {side:.3} m of the back's width");
    }
}
