//! Coordinated palette derivation for a DID-seeded room.
//!
//! Produces every colour the room consumes (terrain biomes, water, sky,
//! fog, sun, clouds) from the shared [`SceneCharacter`] anchor in OkLCH
//! space. A single base-hue + temperature + biome anchor drives every
//! sample, so the palette is internally coherent: a warm Lush room
//! reads as a warm Lush room across grass, water, fog and clouds
//! rather than each channel rolling independently into mud.
//!
//! Each colour group documents its own L / C / H range and the biome
//! biases that shift it; the per-group jitter is small (a few % of the
//! span) so two seeds with the same archetype produce visibly-related
//! but not identical rooms.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::oklch::{oklch_to_srgb, wrap_hue_deg};
use crate::seeded_defaults::scene::{BiomeArchetype, SceneCharacter, range_f32};

/// Sub-stream salt for the palette RNG, distinct from other derivers
/// so each (terrain shape, atmosphere, textures) advances its own RNG
/// state independently. Changing a sibling deriver won't drift the
/// palette.
const PALETTE_STREAM_SALT: u64 = 0xC010_C010_C010_C010;

/// Fully-derived room palette. Every field is sRGB in `[0, 1]`, ready
/// to drop into a `Color::srgb*` constructor or an `Fp3`/`Fp4` PDS
/// record field.
#[derive(Clone, Debug)]
pub struct RoomPalette {
    // -- Lighting / sky / fog --
    pub sun_color: [f32; 3],
    pub sky_color: [f32; 3],
    pub fog_color: [f32; 4],
    pub fog_extinction: [f32; 3],
    pub fog_inscattering: [f32; 3],
    pub fog_sun_color: [f32; 4],

    // -- Clouds --
    pub cloud_sunlit: [f32; 3],
    pub cloud_shadow: [f32; 3],

    // -- Water (per-volume colours go on `WaterSurface`,
    //    `water_scatter` is room-global on `Environment`) --
    pub water_shallow: [f32; 4],
    pub water_deep: [f32; 4],
    pub water_scatter: [f32; 3],

    // -- Terrain splat layers --
    pub grass_dry: [f32; 3],
    pub grass_moist: [f32; 3],
    pub dirt_dry: [f32; 3],
    pub dirt_moist: [f32; 3],
    pub rock_light: [f32; 3],
    pub rock_dark: [f32; 3],
    pub snow_dry: [f32; 3],
    pub snow_moist: [f32; 3],
}

impl RoomPalette {
    /// Derive a full palette from the scene character. The `room_seed`
    /// argument is the bare DID hash; this function salts it before
    /// constructing the palette RNG so sibling derivers don't share
    /// state with the palette.
    pub fn from_scene(scene: &SceneCharacter, room_seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ PALETTE_STREAM_SALT);
        derive(scene, &mut rng)
    }
}

// ---------------------------------------------------------------------------
// Biome-specific palette traits
// ---------------------------------------------------------------------------

/// Per-biome palette shape: which absolute hues anchor each terrain
/// surface, how saturated the overall palette runs, how warm/cool the
/// snow line reads.
struct BiomeTraits {
    /// Grass anchor hue in degrees (OkLCH). Absolute — biome identity
    /// dominates the room's base_hue for vegetation.
    grass_hue_deg: f32,
    /// Grass chroma at full saturation.
    grass_chroma: f32,
    /// Dirt anchor hue.
    dirt_hue_deg: f32,
    /// Dirt chroma at full saturation.
    dirt_chroma: f32,
    /// Rock chroma scale (multiplier on a small base chroma).
    rock_chroma_scale: f32,
    /// Snow warmth bias: `-1` cool blue tint, `+1` warm cream tint.
    snow_warmth_bias: f32,
    /// Global chroma multiplier — Tundra/Volcanic mute everything,
    /// Lush/Coastal stay saturated.
    palette_chroma_scale: f32,
    /// Water anchor hue (degrees). Lush leans aqua, Volcanic darker,
    /// Tundra near-cyan.
    water_hue_deg: f32,
}

fn biome_traits(b: BiomeArchetype) -> BiomeTraits {
    match b {
        BiomeArchetype::Lush => BiomeTraits {
            grass_hue_deg: 130.0,
            grass_chroma: 0.16,
            dirt_hue_deg: 55.0,
            dirt_chroma: 0.14,
            rock_chroma_scale: 1.0,
            snow_warmth_bias: 0.0,
            palette_chroma_scale: 1.0,
            water_hue_deg: 200.0,
        },
        BiomeArchetype::Arid => BiomeTraits {
            grass_hue_deg: 90.0,
            grass_chroma: 0.10,
            dirt_hue_deg: 60.0,
            dirt_chroma: 0.18,
            rock_chroma_scale: 1.2,
            snow_warmth_bias: 0.6,
            palette_chroma_scale: 1.0,
            water_hue_deg: 210.0,
        },
        BiomeArchetype::Alpine => BiomeTraits {
            grass_hue_deg: 140.0,
            grass_chroma: 0.08,
            dirt_hue_deg: 45.0,
            dirt_chroma: 0.08,
            rock_chroma_scale: 0.5,
            snow_warmth_bias: -0.5,
            palette_chroma_scale: 0.7,
            water_hue_deg: 215.0,
        },
        BiomeArchetype::Volcanic => BiomeTraits {
            grass_hue_deg: 100.0,
            grass_chroma: 0.06,
            dirt_hue_deg: 30.0,
            dirt_chroma: 0.16,
            rock_chroma_scale: 0.4,
            snow_warmth_bias: 0.4,
            palette_chroma_scale: 0.8,
            water_hue_deg: 230.0,
        },
        BiomeArchetype::Coastal => BiomeTraits {
            grass_hue_deg: 125.0,
            grass_chroma: 0.14,
            dirt_hue_deg: 70.0,
            dirt_chroma: 0.10,
            rock_chroma_scale: 0.8,
            snow_warmth_bias: 0.5,
            palette_chroma_scale: 0.9,
            water_hue_deg: 195.0,
        },
        BiomeArchetype::Tundra => BiomeTraits {
            grass_hue_deg: 150.0,
            grass_chroma: 0.04,
            dirt_hue_deg: 40.0,
            dirt_chroma: 0.06,
            rock_chroma_scale: 0.3,
            snow_warmth_bias: -0.5,
            palette_chroma_scale: 0.5,
            water_hue_deg: 220.0,
        },
    }
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn col(l: f32, c: f32, h: f32) -> [f32; 3] {
    oklch_to_srgb([l, c, wrap_hue_deg(h)])
}

fn col4(rgb: [f32; 3], a: f32) -> [f32; 4] {
    [rgb[0], rgb[1], rgb[2], a]
}

/// Symmetric jitter helper: `±span` around the centre with a
/// `range_f32` draw. Keeps deriver code visually compact.
fn jitter(rng: &mut ChaCha8Rng, span: f32) -> f32 {
    range_f32(rng, -span, span)
}

// ---------------------------------------------------------------------------
// Master deriver
// ---------------------------------------------------------------------------

fn derive(scene: &SceneCharacter, rng: &mut ChaCha8Rng) -> RoomPalette {
    let bt = biome_traits(scene.biome);
    let base_hue = scene.base_hue_deg;
    let temp = scene.temperature; // -1 cool ↔ +1 warm
    let tod = scene.time_of_day_bias.abs(); // proximity to horizon, 0..1

    // ---------------- SUN ----------------
    // Sunlight runs warm by default; cool rooms only slightly desaturate it,
    // and near-horizon time-of-day pulls hue down toward amber (~25°).
    let sun_hue = lerp(70.0, 25.0, tod);
    let sun_chroma = lerp(0.04, 0.10, tod) + (temp.max(0.0) * 0.02);
    let sun_l = lerp(0.95, 0.88, tod);
    let sun_color = col(
        sun_l + jitter(rng, 0.02),
        sun_chroma + jitter(rng, 0.01),
        sun_hue + jitter(rng, 6.0),
    );

    // ---------------- SKY ----------------
    // Sky is a backdrop; the visible atmosphere is dominated by fog. Keep
    // sky cool-blue, biased very slightly by base_hue so two rooms with
    // very different bases differ in zenith tint.
    let sky_hue = lerp(245.0, 230.0, tod) + (base_hue - 180.0) * 0.05;
    let sky_chroma = 0.04 * bt.palette_chroma_scale;
    let sky_l = lerp(0.78, 0.62, tod);
    let sky_color = col(
        sky_l + jitter(rng, 0.03),
        sky_chroma + jitter(rng, 0.01),
        sky_hue + jitter(rng, 8.0),
    );

    // ---------------- FOG ----------------
    // Fog is the most visible atmosphere channel. Anchor near sky, but
    // slightly warmer; near-horizon TOD pushes pink/orange ("sunset
    // band"). Alpha is always 1.0 — fog colour is fully-opaque tint, the
    // visibility distance does the distance falloff.
    let fog_hue = lerp(sky_hue, 30.0, tod * 0.5);
    let fog_chroma = lerp(0.06, 0.12, tod) * bt.palette_chroma_scale;
    let fog_l = lerp(0.62, 0.55, tod);
    let fog_rgb = col(
        fog_l + jitter(rng, 0.03),
        fog_chroma + jitter(rng, 0.02),
        fog_hue + jitter(rng, 8.0),
    );
    let fog_color = col4(fog_rgb, 1.0);

    // Extinction (light *lost* to absorption): slightly cooler & duller
    // than fog colour.
    let fog_extinction = col(
        fog_l - 0.05 + jitter(rng, 0.02),
        fog_chroma + jitter(rng, 0.01),
        fog_hue - 12.0 + jitter(rng, 6.0),
    );
    // Inscattering (light *gained* from the sun direction): warmer, brighter.
    let fog_inscattering = col(
        (fog_l + 0.18 + jitter(rng, 0.03)).min(0.95),
        (fog_chroma * 0.5 + jitter(rng, 0.01)).max(0.02),
        sun_hue + jitter(rng, 10.0),
    );
    // Fog-sun colour: a warm tint mixed with the directional light.
    let fog_sun_rgb = col(0.9 + jitter(rng, 0.04), 0.06, sun_hue + jitter(rng, 6.0));
    let fog_sun_color = col4(fog_sun_rgb, 0.5);

    // ---------------- CLOUDS ----------------
    // Sunlit top inherits sun warmth; shadowed underside inherits sky cool.
    let cloud_sunlit = col(
        0.95 + jitter(rng, 0.02),
        0.04 + (temp.max(0.0) * 0.02) + jitter(rng, 0.01),
        sun_hue + jitter(rng, 6.0),
    );
    let cloud_shadow = col(
        0.62 + jitter(rng, 0.03),
        0.05 + jitter(rng, 0.01),
        sky_hue + jitter(rng, 6.0),
    );

    // ---------------- WATER ----------------
    // Per-volume colours: shallow (head-on) low-alpha, deep (grazing)
    // high-alpha. Both at the biome's water anchor hue, perturbed by
    // base_hue so users with the same biome still differ visibly.
    let water_hue = bt.water_hue_deg + (base_hue - 180.0) * 0.10;
    let water_shallow_rgb = col(
        0.55 + jitter(rng, 0.03),
        0.10 * bt.palette_chroma_scale + jitter(rng, 0.02),
        water_hue + jitter(rng, 6.0),
    );
    let water_deep_rgb = col(
        0.18 + jitter(rng, 0.03),
        0.08 * bt.palette_chroma_scale + jitter(rng, 0.02),
        water_hue + 15.0 + jitter(rng, 6.0),
    );
    let water_shallow = col4(water_shallow_rgb, range_f32(rng, 0.18, 0.28));
    let water_deep = col4(water_deep_rgb, range_f32(rng, 0.85, 0.92));

    // Room-global subsurface scatter (`water_scatter_color` on
    // `Environment`): greener than the water hue, picks up the crests.
    let water_scatter = col(
        0.42 + jitter(rng, 0.03),
        0.12 * bt.palette_chroma_scale + jitter(rng, 0.02),
        water_hue - 35.0 + jitter(rng, 8.0),
    );

    // ---------------- TERRAIN BIOMES ----------------
    let grass_dry = col(
        range_f32(rng, 0.18, 0.28),
        bt.grass_chroma * bt.palette_chroma_scale + jitter(rng, 0.02),
        bt.grass_hue_deg + jitter(rng, 6.0),
    );
    let grass_moist = col(
        range_f32(rng, 0.10, 0.16),
        bt.grass_chroma * bt.palette_chroma_scale + jitter(rng, 0.02),
        bt.grass_hue_deg - 6.0 + jitter(rng, 4.0),
    );

    let dirt_dry = col(
        range_f32(rng, 0.42, 0.52),
        bt.dirt_chroma * bt.palette_chroma_scale + jitter(rng, 0.02),
        bt.dirt_hue_deg + jitter(rng, 8.0),
    );
    let dirt_moist = col(
        range_f32(rng, 0.22, 0.32),
        bt.dirt_chroma * bt.palette_chroma_scale + jitter(rng, 0.02),
        bt.dirt_hue_deg - 4.0 + jitter(rng, 6.0),
    );

    // Rocks: low chroma, hue lightly biased by base_hue so two rooms with
    // the same biome read as different through their rocks.
    let rock_hue = base_hue + jitter(rng, 30.0);
    let rock_light = col(
        range_f32(rng, 0.42, 0.54),
        0.04 * bt.rock_chroma_scale + jitter(rng, 0.01),
        rock_hue,
    );
    let rock_dark = col(
        range_f32(rng, 0.20, 0.30),
        0.04 * bt.rock_chroma_scale + jitter(rng, 0.01),
        rock_hue + jitter(rng, 12.0),
    );

    // Snow: near-white, tinted by `snow_warmth_bias` and `temperature`
    // (warm cream vs cool blue).
    let snow_warmth = (bt.snow_warmth_bias + temp).clamp(-1.0, 1.0);
    let snow_hue = lerp(225.0, 60.0, (snow_warmth + 1.0) * 0.5);
    let snow_dry = col(
        range_f32(rng, 0.92, 0.97),
        0.02 + jitter(rng, 0.01),
        snow_hue + jitter(rng, 6.0),
    );
    let snow_moist = col(
        range_f32(rng, 0.80, 0.86),
        0.03 + jitter(rng, 0.01),
        snow_hue + jitter(rng, 6.0),
    );

    RoomPalette {
        sun_color,
        sky_color,
        fog_color,
        fog_extinction,
        fog_inscattering,
        fog_sun_color,
        cloud_sunlit,
        cloud_shadow,
        water_shallow,
        water_deep,
        water_scatter,
        grass_dry,
        grass_moist,
        dirt_dry,
        dirt_moist,
        rock_light,
        rock_dark,
        snow_dry,
        snow_moist,
    }
}

/// Linear interpolation between `a` (at `t=0`) and `b` (at `t=1`).
/// `t` is clamped to `[0, 1]` for safety against arithmetic mishaps in
/// upstream callers.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_defaults::hash::fnv1a_64;

    fn finite_rgb(c: [f32; 3]) -> bool {
        c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    }
    fn finite_rgba(c: [f32; 4]) -> bool {
        c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    }

    #[test]
    fn all_channels_are_finite_in_gamut() {
        // Cover every biome with a few seeds each — catches a deriver
        // that produces NaN or out-of-range channels under any branch.
        for biome in BiomeArchetype::ALL {
            for s in 0u64..4 {
                let mut scene = SceneCharacter::for_seed(s);
                scene.biome = biome;
                let p = RoomPalette::from_scene(&scene, s);

                assert!(
                    finite_rgb(p.sun_color),
                    "sun {biome:?} {s} {:?}",
                    p.sun_color
                );
                assert!(finite_rgb(p.sky_color));
                assert!(finite_rgba(p.fog_color));
                assert!(finite_rgb(p.fog_extinction));
                assert!(finite_rgb(p.fog_inscattering));
                assert!(finite_rgba(p.fog_sun_color));
                assert!(finite_rgb(p.cloud_sunlit));
                assert!(finite_rgb(p.cloud_shadow));
                assert!(finite_rgba(p.water_shallow));
                assert!(finite_rgba(p.water_deep));
                assert!(finite_rgb(p.water_scatter));
                assert!(finite_rgb(p.grass_dry));
                assert!(finite_rgb(p.grass_moist));
                assert!(finite_rgb(p.dirt_dry));
                assert!(finite_rgb(p.dirt_moist));
                assert!(finite_rgb(p.rock_light));
                assert!(finite_rgb(p.rock_dark));
                assert!(finite_rgb(p.snow_dry));
                assert!(finite_rgb(p.snow_moist));
            }
        }
    }

    #[test]
    fn deterministic() {
        let seed = fnv1a_64("did:plc:abc");
        let scene = SceneCharacter::for_seed(seed);
        let a = RoomPalette::from_scene(&scene, seed);
        let b = RoomPalette::from_scene(&scene, seed);
        assert_eq!(a.sun_color, b.sun_color);
        assert_eq!(a.grass_dry, b.grass_dry);
        assert_eq!(a.water_shallow, b.water_shallow);
    }

    #[test]
    fn distinct_seeds_distinct_palettes() {
        let a = RoomPalette::from_scene(&SceneCharacter::for_seed(1), 1);
        let b = RoomPalette::from_scene(&SceneCharacter::for_seed(2), 2);
        // At least one channel differs — both palettes coming out identical
        // would mean the scene character or RNG sub-stream is broken.
        let any_diff = a.grass_dry != b.grass_dry
            || a.sun_color != b.sun_color
            || a.water_shallow != b.water_shallow
            || a.fog_color != b.fog_color;
        assert!(any_diff);
    }
}
