//! World Banner - a standard flying this world's own banner (#1783): a tall
//! pole on a stone footing, gilt-banded and crowned with a spear point, and
//! from a crossbar at its head, on rings, the world's banner - its two
//! colours, its division and its mark, drawn from the world's record
//! ([`crate::banner`]), so the owner's change to it recolours this one with
//! every other. A world with no banner of its own flies undyed linen.
//!
//! A seeded world stands one at its arrival, across the view from its gate
//! and ahead of its monument; an owner may stand more. An identity monument,
//! and themeless, as the cross-theme gate is: it is reached by its slug,
//! never drawn into a settlement's pools nor as a theme's monument.
//!
//! The banner is a [`Sign`](GeneratorKind::Sign) whose source is
//! [`SignSource::WorldBanner`],
//! double-sided as cloth is - seen from behind its mark is reversed, as a
//! real banner's is - lit by the world's light rather than shown unlit as a
//! portrait is, and cut to its swallowtail by its image's own alpha.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    cone, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{
    AlphaModeKind, Fp, Fp2, Fp3, Generator, GeneratorKind, SignSource, SovereignMaterialSettings,
    SovereignTextureConfig,
};

use super::{BRONZE, GOLD, STONE, WOOD, bronze, stone, wood};

/// The pole's height over its footing (m).
const POLE_H: f32 = 5.2;
const POLE_R: f32 = 0.07;
/// The footing's top over the ground, and how far it is sunk below it - as
/// deep as a monument's must reach ([`required_depth`] of its clearance).
///
/// [`required_depth`]: crate::catalogue::items::foundation::required_depth
const FOOT_TOP: f32 = 0.1;
const FOOT_SINK: f32 = 1.0;
/// The crossbar the banner hangs from: its height over the ground, its
/// length and how far forward of the pole (toward the -Z front) it holds
/// the cloth.
const BAR_Y: f32 = 4.85;
const BAR_LEN: f32 = 1.5;
const FORWARD: f32 = -0.12;
/// The banner's size (m): two wide by three high, as it is drawn.
pub const BANNER: [f32; 2] = [1.3, 1.95];
/// The gilt orb the spear point stands in, and the point.
const ORB_R: f32 = 0.1;
const POINT_R: f32 = 0.075;
const POINT_H: f32 = 0.32;
const POINT_SINK: f32 = 0.045;

pub struct WorldBanner;

impl CatalogueEntry for WorldBanner {
    fn slug(&self) -> &'static str {
        "world_banner"
    }
    fn name(&self) -> &'static str {
        "World Banner"
    }
    fn description(&self) -> &'static str {
        "A tall standard flying this world's own banner - its colours and its mark, as the \
         world's owner sets them."
    }
    /// An identity monument, as the owner's portrait is: never settlement
    /// dressing, and themeless, so no theme's monument query reaches it.
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 6.0,
        }
    }
    fn build(&self, _local_did: &str) -> Generator {
        world_banner()
    }
}

/// The standard (see the module docs).
pub fn world_banner() -> Generator {
    let pole_top = FOOT_TOP + POLE_H;
    let orb_y = pole_top + 0.03;
    let banner_top = BAR_Y - 0.05;
    let mut prims = vec![
        // The footing, the root: flat, so nothing above inherits a tilt.
        prim(
            solid(cuboid_tapered(
                [0.7, FOOT_TOP + FOOT_SINK, 0.7],
                0.12,
                stone(STONE),
            )),
            [0.0, (FOOT_TOP - FOOT_SINK) * 0.5, 0.0],
            id_quat(),
        ),
        prim(
            solid(cylinder_tapered(POLE_R, POLE_H, 10, 0.1, wood(WOOD))),
            [0.0, FOOT_TOP + POLE_H * 0.5, 0.0],
            id_quat(),
        ),
        // The bar, forward of the pole and through it.
        prim(
            solid(cuboid_tapered([BAR_LEN, 0.07, 0.07], 0.0, wood(WOOD))),
            [0.0, BAR_Y, FORWARD * 0.5],
            id_quat(),
        ),
        // The spear point seated in its orb, apex up (the cone's own +Y).
        prim(sphere(ORB_R, 3, bronze(GOLD)), [0.0, orb_y, 0.0], id_quat()),
        prim(
            cone(POINT_R, POINT_H, 8, bronze(GOLD)),
            [0.0, orb_y + ORB_R - POINT_SINK + POINT_H * 0.5, 0.0],
            id_quat(),
        ),
        // Gilt bands round the pole, and a collar where the bar crosses it.
        prim(
            torus(0.022, POLE_R + 0.015, bronze(GOLD)),
            [0.0, 0.7, 0.0],
            id_quat(),
        ),
        prim(
            torus(0.022, POLE_R + 0.015, bronze(GOLD)),
            [0.0, BAR_Y - 0.3, 0.0],
            id_quat(),
        ),
        prim(
            solid(cylinder_tapered(0.09, 0.16, 10, 0.0, bronze(BRONZE))),
            [0.0, BAR_Y, 0.0],
            id_quat(),
        ),
        banner_panel([0.0, banner_top - BANNER[1] * 0.5, FORWARD]),
    ];
    // Gilt knobs on the bar's ends, and the rings the cloth hangs on - each
    // ring's own axis (+Y) turned onto the bar's, so the bar runs through it.
    for side in [-1.0f32, 1.0] {
        prims.push(prim(
            sphere(0.06, 3, bronze(GOLD)),
            [side * (BAR_LEN * 0.5 + 0.03), BAR_Y, FORWARD * 0.5],
            id_quat(),
        ));
    }
    for i in 0..4 {
        let x = -BANNER[0] * 0.42 + BANNER[0] * 0.84 * i as f32 / 3.0;
        prims.push(prim(
            torus(0.012, 0.055, bronze(BRONZE)),
            [x, BAR_Y - 0.01, FORWARD],
            crate::catalogue::items::civic::quat_z(FRAC_PI_2),
        ));
    }
    super::assemble(prims)
}

/// The cloth: this world's banner, front on -Z, standing up by
/// `quat_x(FRAC_PI_2)` as a portrait panel does (see
/// [`pfp_panel`](crate::catalogue::items::util::pfp_panel) for why that
/// turn and no other draws it the right way round and up).
fn banner_panel(center: [f32; 3]) -> Generator {
    let kind = GeneratorKind::Sign {
        source: SignSource::WorldBanner,
        size: Fp2(BANNER),
        uv_repeat: Fp2([1.0, 1.0]),
        uv_offset: Fp2([0.0, 0.0]),
        material: SovereignMaterialSettings {
            // White, so the banner's own colours are its colours.
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(0.9),
            metallic: Fp(0.0),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::None,
            ..Default::default()
        },
        double_sided: true,
        alpha_mode: AlphaModeKind::Mask { cutoff: Fp(0.5) },
        unlit: false,
        texture_filter: crate::pds::TextureFilter::default(),
    };
    prim(kind, center, quat_x(FRAC_PI_2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{assert_no_tilted_parents, assert_sanitize_stable};

    fn walk(g: &Generator, at: [f32; 3], f: &mut dyn FnMut(&Generator, [f32; 3])) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        f(g, here);
        for c in &g.children {
            walk(c, here, f);
        }
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&WorldBanner.build(""), "world_banner");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&WorldBanner.build(""), "world_banner");
    }

    /// It flies this world's banner - one, cut to its swallowtail, seen from
    /// both sides - over the heads of those walking under it, from the bar.
    #[test]
    fn it_flies_the_worlds_banner_overhead() {
        let mut panels = Vec::new();
        walk(&WorldBanner.build(""), [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Sign {
                source,
                size,
                double_sided,
                alpha_mode,
                ..
            } = &g.kind
            {
                panels.push((
                    source.clone(),
                    size.0,
                    *double_sided,
                    alpha_mode.clone(),
                    at,
                ));
            }
        });
        let [(source, size, double_sided, alpha_mode, at)] = &panels[..] else {
            panic!("one banner, got {}", panels.len());
        };
        assert_eq!(*source, SignSource::WorldBanner);
        assert!(*double_sided);
        assert!(matches!(alpha_mode, AlphaModeKind::Mask { .. }));
        let (top, bottom) = (at[1] + size[1] * 0.5, at[1] - size[1] * 0.5);
        assert!(bottom > 2.5, "its foot hangs at {bottom} m");
        assert!(
            (BAR_Y - 0.1..BAR_Y).contains(&top),
            "it hangs from the bar at {BAR_Y}, its head at {top}"
        );
        assert!(size[0] < BAR_LEN, "the bar is wider than the cloth");
        // Two wide by three high, as it is drawn.
        let drawn = crate::banner::WIDTH as f32 / crate::banner::HEIGHT as f32;
        assert!((size[0] / size[1] - drawn).abs() < 1e-3);
    }
}
