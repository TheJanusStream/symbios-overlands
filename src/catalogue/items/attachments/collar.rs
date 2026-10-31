//! Leather collar - a beast's Neck (#1807, owner decision D9). Cut for a
//! four-legged body: the band circles the origin and the fitted seat scales
//! its bore across and through to the neck it lands on, a cat's round one or
//! a horse's narrow deep one (`src/player/beast_seat.rs`). Placed as world
//! decor it lies flat at this authored size.
//!
//! Drawn at [`DRAFT`]x, the circlet's technique: the sanitiser's floors are
//! prim-local, and the band's wall and the tag's disc are thinner than them
//! worn.

use crate::catalogue::items::util::{
    cuboid_tapered, id_quat, nest, prim, prim_scaled, quat_x, solid, torus, tube, uv_for_scale,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole, WearFit};
use crate::pds::Generator;

use super::{brass, leather};

/// The band's authored bore, in worn metres - the declared measure, a
/// middling dog's neck, so the decor reads as a dog's collar.
const BORE: f32 = 0.20;

/// The size the collar is drawn at, relative to the size it is worn at.
const DRAFT: f32 = 10.0;

/// Harness-dark leather.
const HIDE: [f32; 3] = [0.20, 0.10, 0.05];

pub struct Collar;

impl CatalogueEntry for Collar {
    fn slug(&self) -> &'static str {
        "beast_collar"
    }
    fn name(&self) -> &'static str {
        "Leather Collar"
    }
    fn description(&self) -> &'static str {
        "A buckled leather collar with a brass tag - cut for a four-legged body, it fits \
         itself to the neck it lands on."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Attachment
    }
    fn wear_socket(&self) -> Option<symbios_avatar::Socket> {
        Some(symbios_avatar::Socket::Neck)
    }
    fn wear_fit(&self) -> Option<WearFit> {
        Some(WearFit::Beast { across: BORE })
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.4,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// **The band circles the origin**, in the X-Z plane, its `+Z` at the throat:
/// the buckle's plate sinks into the wall there, and a ring and a tag hang
/// from it, past the band's lower edge. The root is the band, at the origin,
/// carrying the one downscale.
fn build_tree() -> Generator {
    let scale = 1.0 / DRAFT;
    let bore = BORE * DRAFT / 2.0;
    let wall = 0.08;
    let height = 0.32;
    let outer = bore + wall;
    let band = prim_scaled(
        solid(tube(
            outer,
            bore,
            height,
            36,
            uv_for_scale(leather(HIDE), scale),
        )),
        [0.0, 0.0, 0.0],
        id_quat(),
        [scale; 3],
    );
    let metal = || uv_for_scale(brass(), scale);
    nest(
        band,
        vec![
            prim(
                solid(cuboid_tapered([0.34, 0.40, 0.08], 0.0, metal())),
                [0.0, 0.0, outer + 0.01],
                id_quat(),
            ),
            prim(
                solid(torus(0.025, 0.11, metal())),
                [0.0, -0.26, outer + 0.05],
                quat_x(std::f32::consts::FRAC_PI_2),
            ),
            prim(
                solid(cuboid_tapered([0.20, 0.22, 0.03], 0.25, metal())),
                [0.0, -0.46, outer + 0.05],
                id_quat(),
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
        assert_sanitize_stable(&Collar.build(""), "beast_collar");
        assert_no_z_fighting(&Collar.build(""), "beast_collar");
        assert_nothing_floats(&Collar.build(""), "beast_collar");
    }

    #[test]
    fn a_collar_is_worn_at_the_neck_and_declares_its_bore() {
        assert_eq!(Collar.wear_socket(), Some(symbios_avatar::Socket::Neck));
        let fit = Collar.wear_fit().expect("a beast's cut");
        assert_eq!(fit.beast_mm(), 200, "whole millimetres on the wire");
        assert_eq!(fit.band_mm(), 0, "no head band for a person's dresser");
    }
}
