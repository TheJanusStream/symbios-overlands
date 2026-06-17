//! Dome observatory — a tapered concrete drum crowned by a metal
//! dome with a viewing slit, a doorway, and a gallery railing. The
//! "scientist's outpost" landmark: at home on mesas, alpine ridges,
//! and arid plateaus where the sky is the attraction.
//!
//! Frame convention mirrors the lighthouse: the drum is the root with
//! its base at the generator origin; dome, slit, railing, and door are
//! children positioned relative to the drum centre.

use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignConcreteConfig, SovereignMaterialSettings,
    SovereignMetalConfig, SovereignTextureConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, foundation_disc, glow, id_quat, prim, quat_x, solid, sphere,
    torus,
};

pub struct Observatory;

impl CatalogueEntry for Observatory {
    fn slug(&self) -> &'static str {
        "observatory"
    }
    fn name(&self) -> &'static str {
        "Observatory"
    }
    fn description(&self) -> &'static str {
        "Concrete drum crowned by a slitted metal dome, gallery railing, and doorway."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn prosperity_band(&self) -> ProsperityBand {
        ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich)
    }

    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AncientClassical]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 4.5,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn concrete_mat() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.62, 0.61, 0.58]),
        roughness: Fp(0.85),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Concrete(SovereignConcreteConfig {
            formwork_lines: Fp64(4.0),
            formwork_depth: Fp64(0.08),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn dome_mat() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.55, 0.58, 0.62]),
        roughness: Fp(0.35),
        metallic: Fp(0.85),
        uv_scale: Fp(1.0),
        // Brushed, not StandingSeam: the seam ridges wrap a sphere's
        // UV as wobbly horizontal rings.
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: bevy_symbios_texture::metal::MetalStyle::Brushed,
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn build_tree() -> Generator {
    let drum_h = 3.6;
    let drum_r = 3.2;

    let mut root = prim(
        solid(cylinder_tapered(drum_r, drum_h, 24, 0.06, concrete_mat())),
        [0.0, drum_h * 0.5, 0.0],
        id_quat(),
    );
    let rel = |ground_y: f32| ground_y - drum_h * 0.5;

    // Buried foundation, re-anchored from the entry ground frame into
    // the drum-root frame.
    let mut base = foundation_disc(drum_r + 0.3, 3.0);
    base.transform.translation.0[1] -= drum_h * 0.5;
    root.children.push(base);

    // Dome: a metal sphere wider than the drum crown, centred a touch
    // below the crown so its equator belt overhangs cleanly. (Sizing
    // it ~equal to the tapered crown radius made the two surfaces
    // coplanar at the seam — a z-fighting jagged ring.)
    let dome_cy = drum_h - 0.3;
    let dome_r = drum_r * 1.04;
    root.children.push(prim(
        solid(sphere(dome_r, 3, dome_mat())),
        [0.0, rel(dome_cy), 0.0],
        id_quat(),
    ));

    // Viewing slit: a dark shutter housing lying flush along the dome
    // meridian, from the crown down the front face. Its centre sits on
    // the 45° surface point; the +45° X-rotation aligns its long axis
    // with the meridian *tangent* `(0, 0.707, 0.707)` so the housing
    // hugs the surface. (The -45° twin of this rotation is the surface
    // *normal* — that variant stuck out of the crown like a monolith.)
    let slit_offset = dome_r * std::f32::consts::FRAC_1_SQRT_2;
    root.children.push(prim(
        cuboid_tapered([0.55, dome_r * 0.9, 0.45], 0.0, void_mat()),
        [0.0, rel(dome_cy + slit_offset), -slit_offset],
        quat_x(std::f32::consts::FRAC_PI_4),
    ));

    // Catwalk: a walkway disc ringing the mid-drum with a railing
    // torus on five posts — grounded against the wall instead of the
    // old free-floating hoop at the crown.
    let walk_y = 2.1;
    let walk_r = drum_r + 0.30;
    let rail_h = 0.55;
    root.children.push(prim(
        solid(cylinder_tapered(walk_r, 0.10, 24, 0.0, concrete_mat())),
        [0.0, rel(walk_y), 0.0],
        id_quat(),
    ));
    root.children.push(prim(
        torus(0.045, walk_r - 0.08, dome_mat()),
        [0.0, rel(walk_y + rail_h), 0.0],
        id_quat(),
    ));
    for i in 0..5 {
        let a = i as f32 * std::f32::consts::TAU / 5.0;
        root.children.push(prim(
            cylinder_tapered(0.035, rail_h, 8, 0.0, dome_mat()),
            [
                a.sin() * (walk_r - 0.08),
                rel(walk_y + rail_h * 0.5),
                a.cos() * (walk_r - 0.08),
            ],
            id_quat(),
        ));
    }

    // Doorway: dark recess + lintel lamp at the drum base front, both
    // tucked under the catwalk (door top and lamp stay below the
    // walkway disc at `walk_y`).
    root.children.push(prim(
        cuboid_tapered([1.1, 1.9, 0.3], 0.0, void_mat()),
        [0.0, rel(0.95), -(drum_r - 0.05)],
        id_quat(),
    ));
    root.children.push(prim(
        sphere(0.14, 2, glow([0.95, 0.85, 0.55], 4.0)),
        [0.0, rel(1.85), -(drum_r + 0.08)],
        id_quat(),
    ));

    root
}

/// Near-black recess material — door mouths and the dome slit.
fn void_mat() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.03, 0.03, 0.04]),
        roughness: Fp(1.0),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Observatory.build(""), "observatory");
    }
}
