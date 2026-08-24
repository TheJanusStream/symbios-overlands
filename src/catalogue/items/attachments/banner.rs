//! Sashimono back banner — the silhouette hero (#1091): the wearable that
//! deliberately leaves the body's own envelope. A lacquered pole rises
//! from a back harness through whatever hair the wearer grew, and the
//! crimson cloth flies clear above the head — so the sheet across seeds is
//! a direct probe of silhouette against the greediest hair styles, which
//! is this hero's whole assignment (the triangle-corner half is #1092's
//! guard, measured with this worn).
//!
//! Chosen over wings on purpose: a banner asks the silhouette question
//! with a handful of prims and one fabric texture, where blob-built wings
//! would spend the very triangle corner the next slice exists to guard.
//!
//! Drawn as an oversized draft like its two siblings ([`super::circlet`],
//! [`super::lantern`]): the harness plate is the TRUE-scale, axis-aligned
//! root (the lantern's leaning-mast lesson — a rotated or scaled root
//! carries everything with it), and the whole pole-and-cloth assembly
//! hangs under it at 10×, downscaled by one uniform child transform. The
//! cloth is 5 mm — half the sanitiser's prim-local floor, drawn at 50 mm.
//! Its fabric weave passes through [`uv_for_scale`]: repeats are per
//! LOCAL metre, so an uncorrected draft would wear ten times the thread
//! count (trap 1, exercised here by the first textured draft).

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, id_quat, nest, prim, prim_scaled, quat_z, solid, uv_for_scale,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole, ThemeArchetype};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignFabricConfig, SovereignMaterialSettings,
    SovereignTextureConfig,
};

use super::aged_iron;

/// Draft ratio, shared convention with the other attachment heroes.
const DRAFT: f32 = 10.0;

/// Deep crimson, warp and weft a shade apart so the weave reads.
const CRIMSON_WARP: [f32; 3] = [0.52, 0.08, 0.09];
const CRIMSON_WEFT: [f32; 3] = [0.60, 0.11, 0.11];

/// Dark lacquered wood — the pole and crossbar.
fn lacquer() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.24, 0.10, 0.07]),
        roughness: Fp(0.42),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        ..Default::default()
    }
}

/// The banner cloth: plain-weave crimson fabric.
fn banner_cloth() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.56, 0.10, 0.10]),
        roughness: Fp(0.85),
        metallic: Fp(0.0),
        uv_scale: Fp(4.0),
        texture: SovereignTextureConfig::Fabric(SovereignFabricConfig {
            seed: 0x5A51_0130,
            color_warp: Fp3(CRIMSON_WARP),
            color_weft: Fp3(CRIMSON_WEFT),
            thread_count: Fp64(18.0),
            fuzz: Fp64(0.45),
            ..Default::default()
        }),
        ..Default::default()
    }
}

pub struct Sashimono;

impl CatalogueEntry for Sashimono {
    fn slug(&self) -> &'static str {
        "sashimono"
    }
    fn name(&self) -> &'static str {
        "Sashimono Banner"
    }
    fn description(&self) -> &'static str {
        "A back-mounted war banner on a lacquered pole, flying clear above the wearer's head."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Attachment
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::FeudalJapan]
    }
    fn wear_socket(&self) -> Option<symbios_avatar::Socket> {
        Some(symbios_avatar::Socket::Back)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.8,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Root = the harness plate at the attach origin, TRUE scale and
/// axis-aligned. The Back seat yaws the authored `+Z` face away from the
/// chest (`outward_yaw`), so authored `+Z` is "away from the body": the
/// pole stands off the plate toward `+Z` — behind the wearer — which is
/// its clearance against hair draped down the back, and the cloth flies
/// higher still.
fn build_tree() -> Generator {
    let scale = 1.0 / DRAFT;

    // Harness plate, its back face against the origin the engine seats
    // just outside the wearer's back.
    let plate = prim(
        solid(cuboid_tapered([0.11, 0.15, 0.014], 0.0, aged_iron())),
        [0.0, 0.0, 0.0],
        id_quat(),
    );

    // --- Pole and colours, drawn at 10× -----------------------------------
    // Sub-root = the pole, centred on the draft origin, spanning ±6.5
    // (1.3 m true). Everything else is authored in the pole's own
    // draft-local frame and pushed directly (the lantern's frame rule:
    // never `nest` draft children under a true-frame parent).
    let mut pole = prim_scaled(
        solid(cylinder_tapered(0.12, 13.0, 10, 0.0, lacquer())),
        // True frame: up from the lower back, standing 45 mm behind the
        // plate so the shaft clears hair lying against the back.
        [0.0, 0.25, 0.045],
        id_quat(),
        [scale; 3],
    );
    pole.children.extend([
        // Crossbar the cloth hangs from, along X near the pole's top.
        prim(
            solid(cylinder_tapered(0.09, 3.8, 10, 0.0, lacquer())),
            [0.0, 5.9, 0.0],
            quat_z(std::f32::consts::FRAC_PI_2),
        ),
        // The colours: a tall cloth hung from the crossbar, offset a
        // touch off the pole's own plane so nothing is coplanar.
        prim(
            solid(cuboid_tapered(
                [3.4, 4.4, 0.05],
                0.0,
                uv_for_scale(banner_cloth(), scale),
            )),
            [0.0, 3.6, 0.16],
            id_quat(),
        ),
        // Finial cap above the crossbar.
        prim(
            solid(cuboid_tapered([0.3, 0.35, 0.3], 0.6, lacquer())),
            [0.0, 6.6, 0.0],
            id_quat(),
        ),
    ]);

    // Strap block joining plate to pole, sunk into both.
    let strap = prim(
        solid(cuboid_tapered([0.05, 0.05, 0.05], 0.0, aged_iron())),
        [0.0, 0.02, 0.025],
        id_quat(),
    );

    nest(plate, vec![pole, strap])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Sashimono.build(""), "sashimono");
    }

    #[test]
    fn sashimono_is_wearable_at_the_back() {
        assert_eq!(Sashimono.wear_socket(), Some(symbios_avatar::Socket::Back));
        assert_eq!(Sashimono.role(), StructureRole::Attachment);
        assert_eq!(Sashimono.wear_fit(), None, "a banner is not fitted");
    }

    /// The silhouette contract: everything the banner is stands on the
    /// authored `+Z` side (behind the wearer, once the Back seat's yaw
    /// turns it out) or above the plate — nothing may reach forward of
    /// the harness into the body it is worn against.
    #[test]
    fn the_banner_stays_behind_its_harness_plate() {
        let tree = Sashimono.build("");
        fn walk(node: &Generator, z: f32, scale: f32, min_z: &mut f32) {
            let t = node.transform.translation.0;
            let s = node.transform.scale.0[0];
            let here = z + t[2] * scale;
            *min_z = min_z.min(here);
            for child in &node.children {
                walk(child, here, scale * s, min_z);
            }
        }
        let mut min_z = f32::MAX;
        walk(&tree, 0.0, 1.0, &mut min_z);
        assert!(
            min_z >= -0.01,
            "a banner part's origin reaches {min_z} m in front of the harness plane — \
             it would stand inside the wearer"
        );
    }
}
