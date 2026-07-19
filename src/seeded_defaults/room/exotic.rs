//! Theme-gated exotic palette layer (#900/#903).
//!
//! The realistic-first palette deriver (#901) retired the old "own
//! planet" any-hue roam for every seeded room. This module is where the
//! surreal look survives, deliberately and bounded: rooms whose
//! [`ThemeArchetype`] is fantastical — [`AlienOrganic`], [`AlienMonolithic`],
//! [`Fantasy`], and [`Cyberpunk`] at reduced strength — get their sky,
//! fog, cloud-shadow and water channels *leaned* (OkLCH shortest-arc,
//! lightness-preserving) toward a per-theme exotic hue after the
//! realistic derive.
//!
//! Bounds that keep the room legible:
//!
//! - Lightness is never touched, so the splat ordering, the fog depth
//!   read and the water depth gradient all survive.
//! - The sun (and its fog glow twin) stays on the realistic blackbody
//!   locus — an alien sky is lit by a real-looking sun, which is what
//!   sells the "wrong sky over familiar light" effect.
//! - Terrain layers stay realistic; only [`AlienOrganic`] / [`Fantasy`]
//!   add a small chroma lift to vegetation (creep-world lushness), with
//!   hue and lightness untouched.
//! - Every other theme is a strict no-op: the palette is byte-identical
//!   to the realistic derive.
//!
//! [`AlienOrganic`]: ThemeArchetype::AlienOrganic
//! [`AlienMonolithic`]: ThemeArchetype::AlienMonolithic
//! [`Fantasy`]: ThemeArchetype::Fantasy
//! [`Cyberpunk`]: ThemeArchetype::Cyberpunk

use rand_chacha::ChaCha8Rng;

use super::palette::RoomPalette;
use crate::seeded_defaults::oklch::{oklch_to_srgb, srgb_to_oklch, wrap_hue_deg};
use crate::seeded_defaults::scene::{ThemeArchetype, range_f32};

/// Per-theme exotic lean targets. `sky_*` also drives fog and the cloud
/// underside; `water_*` drives all three water channels.
struct ExoticProfile {
    /// OkLCH hue the sky family leans toward.
    sky_hue: f32,
    /// Lean fraction for the sky body (fog/clouds use scaled-down copies).
    sky_t: f32,
    /// Chroma boost on the sky body.
    sky_chroma: f32,
    /// OkLCH hue the water channels lean toward.
    water_hue: f32,
    /// Lean fraction for water.
    water_t: f32,
    /// Chroma boost on water.
    water_chroma: f32,
    /// Chroma-only lift on vegetation (hue/lightness untouched).
    grass_chroma: f32,
}

/// The exotic profile for `theme`, or `None` for every theme that keeps
/// the pure realistic palette.
fn exotic_profile(theme: ThemeArchetype) -> Option<ExoticProfile> {
    use ThemeArchetype::*;
    match theme {
        // Biolume creep-world: teal-green sky over green-glowing water,
        // vegetation a touch lusher than nature allows.
        AlienOrganic => Some(ExoticProfile {
            sky_hue: 165.0,
            sky_t: 0.45,
            sky_chroma: 0.03,
            water_hue: 160.0,
            water_t: 0.50,
            water_chroma: 0.03,
            grass_chroma: 0.03,
        }),
        // Void-geometry world: deep indigo-violet air and water.
        AlienMonolithic => Some(ExoticProfile {
            sky_hue: 290.0,
            sky_t: 0.45,
            sky_chroma: 0.03,
            water_hue: 280.0,
            water_t: 0.45,
            water_chroma: 0.02,
            grass_chroma: 0.0,
        }),
        // Arcane twilight: a violet-rose cast on the air, water pulled a
        // touch deeper blue, faintly enchanted greens.
        Fantasy => Some(ExoticProfile {
            sky_hue: 315.0,
            sky_t: 0.35,
            sky_chroma: 0.03,
            water_hue: 250.0,
            water_t: 0.30,
            water_chroma: 0.02,
            grass_chroma: 0.015,
        }),
        // Neon-noir: a restrained magenta wash — most of the cyberpunk
        // look comes from nightfall + the kit's emissives, so the
        // palette lean stays the lightest of the four.
        Cyberpunk => Some(ExoticProfile {
            sky_hue: 330.0,
            sky_t: 0.22,
            sky_chroma: 0.02,
            water_hue: 220.0,
            water_t: 0.15,
            water_chroma: 0.01,
            grass_chroma: 0.0,
        }),
        _ => None,
    }
}

/// Move `from` toward `to` along the shortest arc of the hue circle by
/// fraction `t`; result wrapped to `[0, 360)`.
fn nudge_hue(from: f32, to: f32, t: f32) -> f32 {
    let from = wrap_hue_deg(from);
    let mut delta = wrap_hue_deg(to) - from;
    if delta > 180.0 {
        delta -= 360.0;
    } else if delta < -180.0 {
        delta += 360.0;
    }
    wrap_hue_deg(from + delta * t.clamp(0.0, 1.0))
}

/// Lean an sRGB colour a bounded fraction `hue_t` toward `target_hue`
/// (OkLCH, shortest arc) and add `chroma_boost`, leaving **lightness
/// untouched** so depth/ordering reads survive. The OkLCH→sRGB
/// conversion clamps back into gamut.
fn lean(rgb: [f32; 3], target_hue: f32, hue_t: f32, chroma_boost: f32) -> [f32; 3] {
    let [l, c, h] = srgb_to_oklch(rgb);
    let h2 = nudge_hue(h, target_hue, hue_t);
    let c2 = (c + chroma_boost).max(0.0);
    oklch_to_srgb([l, c2, h2])
}

/// [`lean`] for an RGBA channel — biases the colour, preserves the alpha.
fn lean4(rgba: [f32; 4], target_hue: f32, hue_t: f32, chroma_boost: f32) -> [f32; 4] {
    let [r, g, b] = lean([rgba[0], rgba[1], rgba[2]], target_hue, hue_t, chroma_boost);
    [r, g, b, rgba[3]]
}

/// Apply the theme-gated exotic lean to a realistically-derived palette.
/// A strict identity for non-exotic themes (no RNG draws either, so the
/// palette stream length only grows on the gated themes).
pub(super) fn apply_exotic_theme(
    mut p: RoomPalette,
    theme: ThemeArchetype,
    rng: &mut ChaCha8Rng,
) -> RoomPalette {
    let Some(x) = exotic_profile(theme) else {
        return p;
    };
    // Per-room strength variation: some alien rooms lean harder into
    // the strangeness than others.
    let s = range_f32(rng, 0.8, 1.0);

    let sky_t = x.sky_t * s;
    p.sky_color = lean(p.sky_color, x.sky_hue, sky_t, x.sky_chroma);
    p.fog_color = lean4(p.fog_color, x.sky_hue, sky_t * 0.8, x.sky_chroma * 0.7);
    p.fog_extinction = lean(p.fog_extinction, x.sky_hue, sky_t * 0.7, 0.0);
    p.fog_inscattering = lean(p.fog_inscattering, x.sky_hue, sky_t * 0.5, 0.0);
    // The sunlit cloud face stays sun-tinted; only the shadowed
    // underside picks up the alien sky bounce.
    p.cloud_shadow = lean(p.cloud_shadow, x.sky_hue, sky_t * 0.6, 0.0);

    let water_t = x.water_t * s;
    p.water_shallow = lean4(p.water_shallow, x.water_hue, water_t, x.water_chroma);
    p.water_deep = lean4(p.water_deep, x.water_hue, water_t, x.water_chroma);
    p.water_scatter = lean(p.water_scatter, x.water_hue, water_t, x.water_chroma);

    if x.grass_chroma > 0.0 {
        p.grass_dry = lean(p.grass_dry, 0.0, 0.0, x.grass_chroma);
        p.grass_moist = lean(p.grass_moist, 0.0, 0.0, x.grass_chroma);
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_defaults::scene::{BiomeArchetype, SceneCharacter};

    /// Scene with pinned biome + theme so each axis is isolated.
    fn scene_for(theme: ThemeArchetype, s: u64) -> SceneCharacter {
        let mut scene = SceneCharacter::for_seed(s);
        scene.biome = BiomeArchetype::Lush;
        scene.theme = theme;
        scene
    }

    fn hue_dist(a: f32, b: f32) -> f32 {
        let d = (wrap_hue_deg(a) - wrap_hue_deg(b)).abs();
        d.min(360.0 - d)
    }

    /// Every non-exotic theme yields a palette byte-identical to any
    /// other non-exotic theme — the layer is a strict gate, not a bias
    /// every room pays for.
    #[test]
    fn non_exotic_themes_are_identity() {
        let exotic = [
            ThemeArchetype::AlienOrganic,
            ThemeArchetype::AlienMonolithic,
            ThemeArchetype::Fantasy,
            ThemeArchetype::Cyberpunk,
        ];
        for s in 0u64..6 {
            let baseline = RoomPalette::from_scene(&scene_for(ThemeArchetype::Medieval, s), s);
            for theme in ThemeArchetype::ALL {
                if exotic.contains(&theme) {
                    continue;
                }
                let p = RoomPalette::from_scene(&scene_for(theme, s), s);
                assert_eq!(
                    p, baseline,
                    "{theme:?} must keep the pure realistic palette"
                );
            }
        }
    }

    /// The four exotic themes actually change the palette, and the sky
    /// hue moves toward the theme's target relative to the realistic
    /// baseline.
    #[test]
    fn exotic_sky_leans_toward_target() {
        for (theme, target) in [
            (ThemeArchetype::AlienOrganic, 165.0),
            (ThemeArchetype::AlienMonolithic, 290.0),
            (ThemeArchetype::Fantasy, 315.0),
            (ThemeArchetype::Cyberpunk, 330.0),
        ] {
            for s in 0u64..12 {
                let base = RoomPalette::from_scene(&scene_for(ThemeArchetype::Medieval, s), s);
                let p = RoomPalette::from_scene(&scene_for(theme, s), s);
                assert_ne!(p, base, "{theme:?} seed {s} must change the palette");
                let [_, _, bh] = srgb_to_oklch(base.sky_color);
                let [_, _, xh] = srgb_to_oklch(p.sky_color);
                assert!(
                    hue_dist(xh, target) < hue_dist(bh, target),
                    "{theme:?} seed {s}: sky hue {bh} -> {xh} should approach {target}"
                );
            }
        }
    }

    /// The sun (and its fog-glow twin) and the solid terrain layers stay
    /// on the realistic derive — the alien look is in the air and the
    /// water, not the light source or the ground.
    #[test]
    fn exotic_preserves_sun_and_solid_ground() {
        for theme in [
            ThemeArchetype::AlienOrganic,
            ThemeArchetype::AlienMonolithic,
            ThemeArchetype::Fantasy,
            ThemeArchetype::Cyberpunk,
        ] {
            for s in 0u64..12 {
                let base = RoomPalette::from_scene(&scene_for(ThemeArchetype::Medieval, s), s);
                let p = RoomPalette::from_scene(&scene_for(theme, s), s);
                assert_eq!(p.sun_color, base.sun_color, "{theme:?} sun must stay real");
                assert_eq!(p.fog_sun_color, base.fog_sun_color);
                assert_eq!(p.cloud_sunlit, base.cloud_sunlit);
                assert_eq!(p.dirt_dry, base.dirt_dry, "{theme:?} soil must stay real");
                assert_eq!(p.rock_stone, base.rock_stone);
                assert_eq!(p.snow_dry, base.snow_dry);
            }
        }
    }

    /// Lightness is preserved through the lean, so the water depth
    /// gradient still reads (shallow brighter than deep) and every
    /// channel stays finite + in gamut.
    #[test]
    fn exotic_stays_in_gamut_and_keeps_depth_read() {
        for theme in [
            ThemeArchetype::AlienOrganic,
            ThemeArchetype::AlienMonolithic,
            ThemeArchetype::Fantasy,
            ThemeArchetype::Cyberpunk,
        ] {
            for s in 0u64..12 {
                let p = RoomPalette::from_scene(&scene_for(theme, s), s);
                for c in [
                    p.sky_color,
                    p.fog_extinction,
                    p.fog_inscattering,
                    p.cloud_shadow,
                    p.water_scatter,
                    p.grass_dry,
                    p.grass_moist,
                ] {
                    assert!(
                        c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
                        "{theme:?} seed {s} out of gamut: {c:?}"
                    );
                }
                let lum4 = |c: [f32; 4]| c[0] + c[1] + c[2];
                assert!(
                    lum4(p.water_deep) < lum4(p.water_shallow) + 1e-3,
                    "{theme:?} seed {s}: deep water must stay darker than shallow"
                );
            }
        }
    }
}
