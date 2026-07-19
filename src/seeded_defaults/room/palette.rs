//! DID-seeded room palette derivation — realistic-first (#900/#901).
//!
//! Produces every colour the room consumes (terrain biomes, water, sky,
//! fog, sun, clouds) by sampling the OkLCH gamut inside per-biome
//! **realism bands**: each [`BiomeArchetype`] carries a
//! [`BiomePaletteProfile`] whose per-channel hue/chroma/lightness bands
//! describe the archetype's real-world colour identity (deep greens and
//! brown soil for Lush, red strata for Badlands, cold grey granite for
//! Alpine, …). The seed roams *within* those bands, so two same-biome
//! rooms still read as distinct places — different greens, different
//! earths, different skies — without ever leaving plausibility.
//!
//! Cross-room coherence comes from three shared modulators rather than a
//! free hue anchor:
//!
//! - [`SceneCharacter::base_hue_deg`] is reduced to a small harmonising
//!   cast (± [`HARMONY_CAST_DEG`]°) applied across the terrain channels,
//!   so a room's layers drift together instead of independently.
//! - `temperature` pulls vegetation and soil toward straw/rust when warm
//!   and toward cooler greens when cold, and gently lifts chroma on warm
//!   rooms.
//! - `time_of_day_bias` drives a physically-grounded light chain: the
//!   sun slides along a blackbody-like locus (near-white noon → amber
//!   horizon), the sky dims and desaturates toward the horizon hours,
//!   and fog is derived as a sky/sun mix instead of a free hue.
//!
//! Role still constrains *lightness* so the splat layers read in the
//! expected order (snow brighter than rock, moist grass darker than
//! dry); the dark/moist variants are derived from the sampled primary by
//! construction, so the ordering cannot invert.
//!
//! The pre-#900 deriver sampled hue relative to a uniformly-random
//! anchor with up to ±180° jitter ("any-hue sky is the point") — that
//! fantasy-first behaviour now lives on only as the bounded, theme-gated
//! exotic layer (#903); every seeded room's base palette is realistic.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::oklch::{oklch_to_srgb, wrap_hue_deg};
use crate::seeded_defaults::scene::{BiomeArchetype, SceneCharacter, range_f32, unit_f32};

/// Sub-stream salt for the palette RNG, distinct from other derivers
/// so each (terrain shape, atmosphere, textures) advances its own RNG
/// state independently. Changing a sibling deriver won't drift the
/// palette.
const PALETTE_STREAM_SALT: u64 = 0xC010_C010_C010_C010;

/// Half-span (degrees) of the room-wide harmonising hue cast derived
/// from [`SceneCharacter::base_hue_deg`]. Every terrain/water channel
/// shifts by the same signed cast (sky and snow by half), so a room's
/// layers lean warm or cool together — the per-DID individuality that
/// used to come from the free hue anchor, kept small enough that no
/// channel leaves its realism band by more than this.
const HARMONY_CAST_DEG: f32 = 10.0;

/// Fully-derived room palette. Every field is sRGB in `[0, 1]`, ready
/// to drop into a `Color::srgb*` constructor or an `Fp3`/`Fp4` PDS
/// record field.
#[derive(Clone, Debug, PartialEq)]
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
    /// Raised stone face — the lighter of the two rock colours. Maps
    /// (after a deliberate swap, see [`crate::seeded_defaults`] →
    /// `apply_palette_to_material`) onto `SovereignRockConfig::
    /// color_dark`, which despite its name is what the texture crate
    /// renders as the stone face (UI label "Color Stone").
    pub rock_stone: [f32; 3],
    /// Crack / gap between stones — much darker than the face so the
    /// ridge pattern reads as shadow. Maps onto
    /// `SovereignRockConfig::color_light` (UI label "Color Gaps"); the
    /// texture crate uses ridged-multifractal noise where peak ridges
    /// become the gap lines, hence the inverted-looking name.
    pub rock_gap: [f32; 3],
    pub snow_dry: [f32; 3],
    pub snow_moist: [f32; 3],
}

impl RoomPalette {
    /// Derive a full palette from the scene character. The `room_seed`
    /// argument is the bare DID hash; this function salts it before
    /// constructing the palette RNG so sibling derivers don't share
    /// state with the palette.
    ///
    /// The realistic derive is followed by the theme-gated exotic lean
    /// (see [`super::exotic`]) — a strict identity for every
    /// non-fantastical theme.
    pub fn from_scene(scene: &SceneCharacter, room_seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ PALETTE_STREAM_SALT);
        let palette = derive(scene, &mut rng);
        super::exotic::apply_exotic_theme(palette, scene.theme, &mut rng)
    }
}

// ---------------------------------------------------------------------------
// Per-biome realism profiles
// ---------------------------------------------------------------------------

/// OkLCH sampling band for one palette channel's *primary* variant (the
/// dry / stone-face / shallow-water reading). Secondary variants (moist,
/// gap, deep) are derived from the sampled primary by fixed darkening
/// offsets so the splat lightness ordering holds by construction.
struct ChannelBand {
    /// Hue band, degrees. Never wraps 360 — every realism band sits
    /// inside a contiguous arc.
    hue: (f32, f32),
    /// Chroma band. Sampling is low-biased (see [`chroma_span`]) so the
    /// typical room sits near the muted end and the vivid end is the
    /// in-band tail.
    chroma: (f32, f32),
    /// Lightness band for the primary variant.
    l: (f32, f32),
}

const fn band(hue: (f32, f32), chroma: (f32, f32), l: (f32, f32)) -> ChannelBand {
    ChannelBand { hue, chroma, l }
}

/// The realism bands for every palette channel of one biome. Mirrors
/// the [`biome_splat_profile`](super::terrain) table pattern: the
/// archetype's documented colour identity, expressed as sampling bands.
struct BiomePaletteProfile {
    grass: ChannelBand,
    dirt: ChannelBand,
    rock: ChannelBand,
    snow: ChannelBand,
    /// Shallow-water band; deep water is derived darker from it.
    water: ChannelBand,
    /// Sky band; `l` is the noon lightness, scaled down toward the
    /// horizon hours by the deriver.
    sky: ChannelBand,
}

/// Per-biome palette bands. OkLCH hue landmarks for orientation:
/// red ≈ 29°, orange ≈ 55°, yellow ≈ 110°, green ≈ 142°, cyan ≈ 195°,
/// blue ≈ 264°.
fn biome_palette_profile(b: BiomeArchetype) -> BiomePaletteProfile {
    use BiomeArchetype::*;
    match b {
        // Temperate verdure: mid greens over brown loam, clear blue sky.
        Lush => BiomePaletteProfile {
            grass: band((118.0, 142.0), (0.07, 0.14), (0.18, 0.32)),
            dirt: band((55.0, 80.0), (0.04, 0.09), (0.38, 0.52)),
            rock: band((55.0, 95.0), (0.01, 0.05), (0.42, 0.58)),
            snow: band((230.0, 260.0), (0.004, 0.02), (0.88, 0.97)),
            water: band((200.0, 230.0), (0.05, 0.12), (0.48, 0.62)),
            sky: band((235.0, 258.0), (0.06, 0.12), (0.72, 0.80)),
        },
        // Sun-bleached straw over ochre earth under a pale dusty sky.
        Arid => BiomePaletteProfile {
            grass: band((95.0, 118.0), (0.04, 0.09), (0.28, 0.42)),
            dirt: band((62.0, 85.0), (0.06, 0.12), (0.45, 0.60)),
            rock: band((55.0, 82.0), (0.03, 0.08), (0.44, 0.60)),
            snow: band((230.0, 260.0), (0.002, 0.01), (0.88, 0.96)),
            water: band((195.0, 225.0), (0.04, 0.10), (0.46, 0.60)),
            sky: band((240.0, 262.0), (0.045, 0.09), (0.74, 0.82)),
        },
        // Short alpine turf, cold grey granite, crisp high-altitude blue.
        Alpine => BiomePaletteProfile {
            grass: band((125.0, 150.0), (0.05, 0.10), (0.18, 0.30)),
            dirt: band((50.0, 75.0), (0.03, 0.07), (0.36, 0.50)),
            rock: band((235.0, 275.0), (0.004, 0.03), (0.42, 0.58)),
            snow: band((232.0, 258.0), (0.006, 0.025), (0.90, 0.97)),
            water: band((208.0, 238.0), (0.05, 0.11), (0.46, 0.60)),
            sky: band((238.0, 262.0), (0.07, 0.13), (0.72, 0.80)),
        },
        // Dark warm basalt and reddish ash; scorched sparse scrub. The
        // warm rock/soil identity that used to be a post-hoc signature
        // lean is encoded directly in the bands.
        Volcanic => BiomePaletteProfile {
            grass: band((85.0, 108.0), (0.03, 0.07), (0.16, 0.26)),
            dirt: band((30.0, 55.0), (0.04, 0.10), (0.30, 0.42)),
            rock: band((22.0, 48.0), (0.02, 0.07), (0.26, 0.40)),
            snow: band((240.0, 270.0), (0.002, 0.012), (0.82, 0.92)),
            water: band((200.0, 230.0), (0.03, 0.08), (0.44, 0.56)),
            sky: band((235.0, 260.0), (0.04, 0.08), (0.68, 0.76)),
        },
        // Bright sand, tropical-leaning water, holiday-blue sky.
        Coastal => BiomePaletteProfile {
            grass: band((112.0, 138.0), (0.06, 0.12), (0.20, 0.33)),
            dirt: band((72.0, 92.0), (0.04, 0.09), (0.52, 0.68)),
            rock: band((60.0, 92.0), (0.02, 0.05), (0.44, 0.60)),
            snow: band((230.0, 258.0), (0.003, 0.012), (0.88, 0.96)),
            water: band((188.0, 218.0), (0.07, 0.14), (0.50, 0.64)),
            sky: band((232.0, 255.0), (0.07, 0.12), (0.73, 0.81)),
        },
        // Frost-muted scrub, cold grey stone, washed-out pale sky.
        Tundra => BiomePaletteProfile {
            grass: band((98.0, 128.0), (0.03, 0.07), (0.22, 0.34)),
            dirt: band((55.0, 80.0), (0.03, 0.06), (0.36, 0.48)),
            rock: band((238.0, 278.0), (0.004, 0.028), (0.40, 0.55)),
            snow: band((233.0, 258.0), (0.008, 0.03), (0.88, 0.96)),
            water: band((210.0, 240.0), (0.04, 0.09), (0.44, 0.58)),
            sky: band((235.0, 258.0), (0.04, 0.09), (0.72, 0.80)),
        },
        // Saturated deep greens over dark loam; green-tinged water. The
        // vivid-green signature is the band itself now.
        Jungle => BiomePaletteProfile {
            grass: band((132.0, 152.0), (0.10, 0.17), (0.16, 0.28)),
            dirt: band((45.0, 70.0), (0.05, 0.10), (0.32, 0.45)),
            rock: band((60.0, 100.0), (0.02, 0.05), (0.40, 0.55)),
            snow: band((230.0, 258.0), (0.003, 0.015), (0.88, 0.96)),
            water: band((172.0, 202.0), (0.06, 0.12), (0.44, 0.58)),
            sky: band((230.0, 254.0), (0.06, 0.11), (0.70, 0.78)),
        },
        // Broadleaf woodland: leaf-litter browns under mixed greens.
        TemperateForest => BiomePaletteProfile {
            grass: band((118.0, 142.0), (0.07, 0.13), (0.18, 0.31)),
            dirt: band((48.0, 74.0), (0.05, 0.09), (0.36, 0.50)),
            rock: band((50.0, 92.0), (0.01, 0.045), (0.42, 0.57)),
            snow: band((230.0, 258.0), (0.004, 0.018), (0.88, 0.96)),
            water: band((200.0, 230.0), (0.05, 0.11), (0.47, 0.60)),
            sky: band((235.0, 258.0), (0.06, 0.11), (0.72, 0.80)),
        },
        // Cold conifer taiga: blue-leaning greens, cool grey stone.
        Boreal => BiomePaletteProfile {
            grass: band((138.0, 160.0), (0.05, 0.10), (0.16, 0.27)),
            dirt: band((46.0, 72.0), (0.03, 0.07), (0.34, 0.46)),
            rock: band((232.0, 272.0), (0.004, 0.03), (0.40, 0.55)),
            snow: band((233.0, 258.0), (0.005, 0.025), (0.89, 0.96)),
            water: band((208.0, 238.0), (0.04, 0.10), (0.44, 0.57)),
            sky: band((235.0, 258.0), (0.05, 0.10), (0.71, 0.79)),
        },
        // Reed greens over dark peat; murky green-brown standing water.
        Wetland => BiomePaletteProfile {
            grass: band((108.0, 133.0), (0.06, 0.11), (0.18, 0.30)),
            dirt: band((58.0, 88.0), (0.03, 0.06), (0.28, 0.40)),
            rock: band((60.0, 100.0), (0.01, 0.04), (0.40, 0.54)),
            snow: band((230.0, 258.0), (0.003, 0.014), (0.87, 0.95)),
            water: band((128.0, 168.0), (0.03, 0.08), (0.38, 0.50)),
            sky: band((233.0, 255.0), (0.04, 0.09), (0.70, 0.78)),
        },
        // Fresh rolling grassland under a bright open sky.
        Meadow => BiomePaletteProfile {
            grass: band((112.0, 140.0), (0.08, 0.15), (0.20, 0.33)),
            dirt: band((55.0, 80.0), (0.05, 0.09), (0.40, 0.54)),
            rock: band((50.0, 92.0), (0.01, 0.04), (0.42, 0.57)),
            snow: band((230.0, 258.0), (0.004, 0.016), (0.88, 0.96)),
            water: band((200.0, 230.0), (0.05, 0.11), (0.48, 0.62)),
            sky: band((235.0, 260.0), (0.07, 0.12), (0.73, 0.81)),
        },
        // Golden dry grass, warm earth, big pale-blue sky.
        Savanna => BiomePaletteProfile {
            grass: band((88.0, 108.0), (0.06, 0.12), (0.26, 0.40)),
            dirt: band((55.0, 80.0), (0.06, 0.11), (0.44, 0.58)),
            rock: band((45.0, 78.0), (0.03, 0.07), (0.42, 0.58)),
            snow: band((230.0, 258.0), (0.002, 0.01), (0.88, 0.95)),
            water: band((195.0, 225.0), (0.04, 0.09), (0.46, 0.60)),
            sky: band((238.0, 262.0), (0.06, 0.11), (0.74, 0.82)),
        },
        // Stratified red rock and rust-red soil; sparse scrub.
        Badlands => BiomePaletteProfile {
            grass: band((82.0, 104.0), (0.03, 0.07), (0.24, 0.36)),
            dirt: band((35.0, 60.0), (0.06, 0.12), (0.38, 0.52)),
            rock: band((26.0, 52.0), (0.05, 0.11), (0.38, 0.54)),
            snow: band((230.0, 258.0), (0.002, 0.01), (0.87, 0.94)),
            water: band((190.0, 220.0), (0.03, 0.08), (0.44, 0.58)),
            sky: band((240.0, 264.0), (0.05, 0.10), (0.73, 0.81)),
        },
        // Blue-tinted ice and grey moraine; the ice-blue signature is
        // encoded in the snow/water bands directly.
        Glacial => BiomePaletteProfile {
            grass: band((128.0, 155.0), (0.02, 0.05), (0.20, 0.30)),
            dirt: band((235.0, 270.0), (0.004, 0.025), (0.34, 0.46)),
            rock: band((232.0, 268.0), (0.008, 0.04), (0.40, 0.56)),
            snow: band((228.0, 250.0), (0.015, 0.045), (0.90, 0.97)),
            water: band((212.0, 240.0), (0.06, 0.13), (0.46, 0.60)),
            sky: band((230.0, 254.0), (0.06, 0.11), (0.72, 0.80)),
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

/// Centre-biased symmetric jitter: `±span`, but the magnitude is squared so
/// most draws pull toward `0` — a channel usually stays near its anchor
/// and only the rare tail diverges the full `span`. Consumes one draw,
/// like a plain uniform jitter, so the palette stays deterministic and
/// the RNG stream is byte-identical in length.
fn jitter(rng: &mut ChaCha8Rng, span: f32) -> f32 {
    let u = range_f32(rng, -1.0, 1.0);
    u * u.abs() * span
}

/// Low-biased `[lo, hi)` chroma sample: squares a uniform draw so most
/// rooms land near `lo` (muted, naturalistic) and only the rare tail
/// reaches `hi` (vivid — but still inside the biome's realism band).
fn chroma_span(rng: &mut ChaCha8Rng, (lo, hi): (f32, f32)) -> f32 {
    let u = unit_f32(rng);
    lo + u * u * (hi - lo)
}

/// Uniform sample from an inclusive-exclusive band tuple.
fn sample(rng: &mut ChaCha8Rng, (lo, hi): (f32, f32)) -> f32 {
    range_f32(rng, lo, hi)
}

/// Linear interpolation between `a` (at `t=0`) and `b` (at `t=1`).
/// `t` is clamped to `[0, 1]` for safety against arithmetic mishaps in
/// upstream callers.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// Move `from` toward `to` along the shortest arc of the hue circle by
/// fraction `t` (`0` = unchanged, `1` = exactly `to`); result wrapped to
/// `[0, 360)`. Used to mix the fog hue between the sky and sun hues.
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

// ---------------------------------------------------------------------------
// Master deriver
// ---------------------------------------------------------------------------

fn derive(scene: &SceneCharacter, rng: &mut ChaCha8Rng) -> RoomPalette {
    let profile = biome_palette_profile(scene.biome);

    // The per-DID hue anchor, reduced to a small signed cast shared by
    // every terrain/water channel — the room leans warm or cool as a
    // whole instead of each layer roaming independently.
    let cast = (scene.base_hue_deg / 180.0 - 1.0) * HARMONY_CAST_DEG;
    // Centre-bias the warm/cool axis toward neutral so a strong cast is
    // the rare tail, not the average room. Local to colour — the scene
    // axis itself (read by audio, etc.) is unchanged.
    let temp = {
        let t = scene.temperature; // -1 cool ↔ +1 warm
        t * t.abs()
    };
    let tod = scene.time_of_day_bias.abs(); // proximity to horizon, 0..1

    // ---------------- SUN ----------------
    // Blackbody-like locus: near-white warm at noon, amber toward the
    // horizon hours. The fog sun-glow shares the RGB (with a 0.5 alpha)
    // so the key light and its atmospheric glow read as one source.
    let sun_l = (lerp(0.97, 0.85, tod) + jitter(rng, 0.02)).clamp(0.75, 0.99);
    let sun_c = (lerp(0.03, 0.12, tod) + jitter(rng, 0.015)).max(0.008);
    let sun_h = lerp(95.0, 65.0, tod) + jitter(rng, 8.0);
    let sun_rgb = col(sun_l, sun_c, sun_h);
    let sun_color = sun_rgb;
    let fog_sun_color = col4(sun_rgb, 0.5);

    // ---------------- SKY ----------------
    // Blue band from the biome profile; dimmer and slightly desaturated
    // toward the horizon hours (the warm dawn/dusk gradient lives in the
    // fog mix below, not in the sky body).
    let sky_h = sample(rng, profile.sky.hue) + cast * 0.5;
    let sky_c = chroma_span(rng, profile.sky.chroma) * lerp(1.0, 0.75, tod);
    let sky_l = (sample(rng, profile.sky.l) * lerp(1.0, 0.72, tod)).clamp(0.30, 0.90);
    let sky_color = col(sky_l, sky_c, sky_h);

    // ---------------- FOG ----------------
    // Fog is scattered light: its hue sits between the sky and the sun,
    // pulled sunward as the sun drops (warm dawn/dusk haze), and its
    // chroma is damped below either source.
    let fog_mix = (0.25 + 0.40 * tod + jitter(rng, 0.08)).clamp(0.10, 0.80);
    let fog_h = nudge_hue(sky_h, sun_h, fog_mix);
    let fog_c = (lerp(sky_c, sun_c, fog_mix) * 0.75).max(0.012);
    let fog_l = (lerp(0.62, 0.52, tod) + jitter(rng, 0.05)).clamp(0.35, 0.75);
    let fog_rgb = col(fog_l, fog_c, fog_h);
    let fog_color = col4(fog_rgb, 1.0);

    // Extinction (light *lost* to absorption): a step darker and
    // slightly off-hue from the fog body.
    let fog_extinction = col(
        (fog_l - 0.08 + jitter(rng, 0.03)).max(0.05),
        (fog_c + jitter(rng, 0.008)).max(0.005),
        fog_h + jitter(rng, 12.0),
    );
    // Inscattering (light *gained* from the sun direction): a step
    // brighter, pulled further toward the sun's hue.
    let fog_inscattering = col(
        (fog_l + 0.20 + jitter(rng, 0.03)).min(0.95),
        (fog_c * 0.6).max(0.015),
        nudge_hue(fog_h, sun_h, 0.5),
    );

    // ---------------- CLOUDS ----------------
    // Near-neutral vapour: the sunlit top leans faintly toward the sun
    // hue, the shadowed underside toward the sky hue.
    let cloud_sunlit = col(
        (0.94 + jitter(rng, 0.03)).min(0.98),
        chroma_span(rng, (0.006, 0.03)),
        sun_h + jitter(rng, 15.0),
    );
    let cloud_shadow = col(
        0.58 + jitter(rng, 0.05),
        chroma_span(rng, (0.006, 0.035)),
        sky_h + jitter(rng, 12.0),
    );

    // ---------------- WATER ----------------
    // Per-volume colours: shallow (head-on) low-alpha, deep (grazing)
    // high-alpha. Deep is derived darker from the sampled shallow so the
    // depth gradient always reads the right way.
    let water_h = sample(rng, profile.water.hue) + cast;
    let water_c = chroma_span(rng, profile.water.chroma);
    let shallow_l = sample(rng, profile.water.l);
    let water_shallow_rgb = col(
        shallow_l,
        (water_c + jitter(rng, 0.012)).max(0.005),
        water_h + jitter(rng, 8.0),
    );
    let deep_l = (shallow_l * range_f32(rng, 0.26, 0.38)).clamp(0.06, 0.24);
    let water_deep_rgb = col(
        deep_l,
        (water_c + jitter(rng, 0.012)).max(0.005),
        water_h + jitter(rng, 10.0),
    );
    let water_shallow = col4(water_shallow_rgb, range_f32(rng, 0.18, 0.28));
    let water_deep = col4(water_deep_rgb, range_f32(rng, 0.85, 0.92));

    // Room-global subsurface scatter (`water_scatter_color` on
    // `Environment`): crest glow — a touch brighter-chroma than the
    // body, still on the water hue.
    let water_scatter = col(
        range_f32(rng, 0.38, 0.52),
        (water_c * 1.15).min(profile.water.chroma.1 + 0.03),
        water_h + jitter(rng, 15.0),
    );

    // ---------------- TERRAIN BIOMES ----------------
    // Warm rooms pull vegetation and soil toward straw/rust (lower OkLCH
    // hue) and lift vegetation chroma a touch; cool rooms push greener.
    let veg_warm_shift = temp * 8.0;

    let grass_h = sample(rng, profile.grass.hue) + cast - veg_warm_shift;
    let grass_c = chroma_span(rng, profile.grass.chroma) + temp.max(0.0) * 0.015;
    let grass_l = sample(rng, profile.grass.l);
    let grass_dry = col(grass_l, grass_c, grass_h + jitter(rng, 6.0));
    let grass_moist = col(
        (grass_l - range_f32(rng, 0.08, 0.14)).max(0.05),
        grass_c + 0.01,
        grass_h + jitter(rng, 5.0),
    );

    let dirt_h = sample(rng, profile.dirt.hue) + cast - temp * 6.0;
    let dirt_c = chroma_span(rng, profile.dirt.chroma);
    let dirt_l = sample(rng, profile.dirt.l);
    let dirt_dry = col(dirt_l, dirt_c, dirt_h + jitter(rng, 5.0));
    let dirt_moist = col(
        (dirt_l - range_f32(rng, 0.16, 0.24)).max(0.08),
        dirt_c + 0.01,
        dirt_h + jitter(rng, 5.0),
    );

    let rock_h = sample(rng, profile.rock.hue) + cast;
    let rock_c = chroma_span(rng, profile.rock.chroma);
    let rock_l = sample(rng, profile.rock.l);
    let rock_stone = col(rock_l, rock_c, rock_h + jitter(rng, 6.0));
    let rock_gap = col(
        (rock_l - range_f32(rng, 0.28, 0.38)).max(0.05),
        (rock_c + chroma_span(rng, (0.0, 0.025))).min(profile.rock.chroma.1 + 0.03),
        rock_h + jitter(rng, 10.0),
    );

    let snow_h = sample(rng, profile.snow.hue) + cast * 0.5;
    let snow_c = chroma_span(rng, profile.snow.chroma);
    let snow_l = sample(rng, profile.snow.l);
    let snow_dry = col(snow_l, snow_c * 0.6, snow_h + jitter(rng, 8.0));
    let snow_moist = col(
        (snow_l - range_f32(rng, 0.10, 0.14)).max(0.60),
        snow_c,
        snow_h + jitter(rng, 6.0),
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
        rock_stone,
        rock_gap,
        snow_dry,
        snow_moist,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_defaults::hash::fnv1a_64;
    use crate::seeded_defaults::oklch::srgb_to_oklch;
    use crate::seeded_defaults::scene::BiomeArchetype;

    fn finite_rgb(c: [f32; 3]) -> bool {
        c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    }
    fn finite_rgba(c: [f32; 4]) -> bool {
        c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    }

    /// Scene for (biome, seed) with the biome forced — the palette must
    /// hold its realism bands for every biome at every seed, not just
    /// the biome the seed happens to roll. The theme is pinned to a
    /// non-fantastical one so the band assertions test the realistic
    /// derive, not the theme-gated exotic lean (covered in
    /// [`super::super::exotic`]'s own tests).
    fn scene_for(biome: BiomeArchetype, s: u64) -> SceneCharacter {
        let mut scene = SceneCharacter::for_seed(s);
        scene.biome = biome;
        scene.theme = crate::seeded_defaults::scene::ThemeArchetype::Medieval;
        scene
    }

    #[test]
    fn all_channels_are_finite_in_gamut() {
        for biome in BiomeArchetype::ALL {
            for s in 0u64..4 {
                let scene = scene_for(biome, s);
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
                assert!(finite_rgb(p.rock_stone));
                assert!(finite_rgb(p.rock_gap));
                assert!(finite_rgb(p.snow_dry));
                assert!(finite_rgb(p.snow_moist));
            }
        }
    }

    #[test]
    fn sun_color_follows_fog_sun_glow() {
        // Seeded rooms key the directional sun colour off the fog
        // sun-glow tint, so the two share RGB (the glow carries a 0.5
        // alpha the opaque sun drops). Sweep seeds to be sure it isn't a
        // lucky single-seed collision.
        for s in 0u64..32 {
            let scene = SceneCharacter::for_seed(s);
            let p = RoomPalette::from_scene(&scene, s);
            assert_eq!(
                p.sun_color,
                [p.fog_sun_color[0], p.fog_sun_color[1], p.fog_sun_color[2]],
                "sun colour diverged from fog sun-glow at seed {s}"
            );
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

    #[test]
    fn moist_darker_than_dry() {
        // The splat blend reads "moist soil under dry soil". The moist
        // variants are derived darker by construction, so this holds
        // per-seed (luminance-sum compare survives the gamut clamp).
        for biome in BiomeArchetype::ALL {
            for s in 0u64..8 {
                let p = RoomPalette::from_scene(&scene_for(biome, s), s);
                let lum = |c: [f32; 3]| c[0] + c[1] + c[2];
                assert!(
                    lum(p.grass_moist) < lum(p.grass_dry) + 1e-3,
                    "{biome:?} {s}: moist grass not darker"
                );
                assert!(
                    lum(p.dirt_moist) < lum(p.dirt_dry) + 1e-3,
                    "{biome:?} {s}: moist dirt not darker"
                );
                assert!(
                    lum(p.snow_moist) < lum(p.snow_dry) + 1e-3,
                    "{biome:?} {s}: moist snow not darker"
                );
                assert!(
                    lum(p.rock_gap) < lum(p.rock_stone) + 1e-3,
                    "{biome:?} {s}: rock gap not darker than face"
                );
            }
        }
    }

    /// Vegetation is never blue-dominant: for every biome and seed, the
    /// blue channel of grass is (weakly) the smallest — greens through
    /// golden straw, never magenta meadows.
    #[test]
    fn vegetation_never_blue() {
        for biome in BiomeArchetype::ALL {
            for s in 0u64..24 {
                let p = RoomPalette::from_scene(&scene_for(biome, s), s);
                for (label, c) in [("grass_dry", p.grass_dry), ("grass_moist", p.grass_moist)] {
                    assert!(
                        c[2] <= c[0].max(c[1]) + 0.02,
                        "{biome:?} seed {s} {label} is blue-dominant: {c:?}"
                    );
                }
            }
        }
    }

    /// The sky body always reads blue: the blue channel dominates for
    /// every biome and seed. No more green or magenta skies outside the
    /// theme-gated exotic layer.
    #[test]
    fn sky_reads_blue() {
        for biome in BiomeArchetype::ALL {
            for s in 0u64..24 {
                let p = RoomPalette::from_scene(&scene_for(biome, s), s);
                let c = p.sky_color;
                assert!(
                    c[2] >= c[0] && c[2] >= c[1] - 0.01,
                    "{biome:?} seed {s} sky not blue-dominant: {c:?}"
                );
            }
        }
    }

    /// The sun stays on the warm-white locus: hue in the yellow-orange
    /// arc, chroma bounded, never blue-heavy. Sweeps time-of-day via the
    /// natural per-seed scene so both noon and horizon rooms are hit.
    #[test]
    fn sun_stays_warm_white() {
        for s in 0u64..48 {
            let scene = SceneCharacter::for_seed(s);
            let p = RoomPalette::from_scene(&scene, s);
            let [_, c, h] = srgb_to_oklch(p.sun_color);
            assert!(
                (45.0..=115.0).contains(&h),
                "seed {s}: sun hue {h} off the blackbody arc ({:?})",
                p.sun_color
            );
            assert!(c <= 0.16, "seed {s}: sun chroma {c} too vivid");
            assert!(
                p.sun_color[0] >= p.sun_color[2],
                "seed {s}: sun blue-heavy {:?}",
                p.sun_color
            );
        }
    }

    /// Clouds are near-neutral vapour, and the sunlit face is brighter
    /// than the shadowed underside.
    #[test]
    fn clouds_near_neutral() {
        for biome in BiomeArchetype::ALL {
            for s in 0u64..12 {
                let p = RoomPalette::from_scene(&scene_for(biome, s), s);
                let [_, sunlit_c, _] = srgb_to_oklch(p.cloud_sunlit);
                let [_, shadow_c, _] = srgb_to_oklch(p.cloud_shadow);
                assert!(
                    sunlit_c <= 0.05,
                    "{biome:?} {s}: sunlit cloud too colourful {sunlit_c}"
                );
                assert!(
                    shadow_c <= 0.06,
                    "{biome:?} {s}: cloud shadow too colourful {shadow_c}"
                );
                let lum = |c: [f32; 3]| c[0] + c[1] + c[2];
                assert!(lum(p.cloud_sunlit) > lum(p.cloud_shadow));
            }
        }
    }

    /// Snow stays bright and near-neutral (a faint cool tint at most) in
    /// every biome — pink ice is exotic-layer territory now.
    #[test]
    fn snow_bright_and_near_neutral() {
        for biome in BiomeArchetype::ALL {
            for s in 0u64..24 {
                let p = RoomPalette::from_scene(&scene_for(biome, s), s);
                let c = p.snow_dry;
                let spread = c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2]);
                assert!(
                    c.iter().all(|v| *v >= 0.55),
                    "{biome:?} {s}: snow_dry too dark {c:?}"
                );
                assert!(
                    spread <= 0.18,
                    "{biome:?} {s}: snow_dry too tinted {c:?} (spread {spread})"
                );
            }
        }
    }

    /// Water is never red-dominant — cyan through blue, or the murky
    /// green-browns of wetland, but no magenta lagoons.
    #[test]
    fn water_never_red_dominant() {
        for biome in BiomeArchetype::ALL {
            for s in 0u64..24 {
                let p = RoomPalette::from_scene(&scene_for(biome, s), s);
                for (label, c) in [
                    (
                        "shallow",
                        [p.water_shallow[0], p.water_shallow[1], p.water_shallow[2]],
                    ),
                    ("deep", [p.water_deep[0], p.water_deep[1], p.water_deep[2]]),
                    ("scatter", p.water_scatter),
                ] {
                    assert!(
                        c[0] <= c[1].max(c[2]) + 0.02,
                        "{biome:?} seed {s} water {label} red-dominant: {c:?}"
                    );
                }
            }
        }
    }

    /// Every sampled channel respects its biome's chroma ceiling (the
    /// gamut clamp can only reduce chroma, so measuring the sRGB output
    /// is a sound upper-bound check). Allowance covers the temperature
    /// lift, the moist/gap chroma bumps, and round-trip noise.
    #[test]
    fn chroma_ceilings_respected() {
        const ALLOW: f32 = 0.045;
        for biome in BiomeArchetype::ALL {
            let profile = biome_palette_profile(biome);
            for s in 0u64..24 {
                let p = RoomPalette::from_scene(&scene_for(biome, s), s);
                let checks: [(&str, [f32; 3], f32); 5] = [
                    ("grass_dry", p.grass_dry, profile.grass.chroma.1),
                    ("dirt_dry", p.dirt_dry, profile.dirt.chroma.1),
                    ("rock_stone", p.rock_stone, profile.rock.chroma.1),
                    ("snow_dry", p.snow_dry, profile.snow.chroma.1),
                    (
                        "water_shallow",
                        [p.water_shallow[0], p.water_shallow[1], p.water_shallow[2]],
                        profile.water.chroma.1,
                    ),
                ];
                for (label, rgb, ceiling) in checks {
                    let [_, c, _] = srgb_to_oklch(rgb);
                    assert!(
                        c <= ceiling + ALLOW,
                        "{biome:?} seed {s} {label} chroma {c} exceeds {ceiling} + {ALLOW}"
                    );
                }
            }
        }
    }

    /// Biome identity survives the in-band roam: red badlands earth vs
    /// cool alpine stone, vivid jungle greens vs frost-muted tundra
    /// scrub, murky-dark wetland water vs bright coastal water. Averaged
    /// across seeds so single-roll jitter can't flake the assertion.
    #[test]
    fn biome_identities_read_through() {
        let avg = |biome: BiomeArchetype, f: &dyn Fn(&RoomPalette) -> f32| -> f32 {
            let mut total = 0.0;
            for s in 0u64..32 {
                total += f(&RoomPalette::from_scene(&scene_for(biome, s), s));
            }
            total / 32.0
        };

        // Badlands dirt is warm-red; alpine dirt is a cooler earth.
        let warmth = |p: &RoomPalette| p.dirt_dry[0] - p.dirt_dry[2];
        assert!(
            avg(BiomeArchetype::Badlands, &warmth) > avg(BiomeArchetype::Alpine, &warmth),
            "badlands dirt should read warmer than alpine"
        );

        // Jungle grass is more chromatic than tundra scrub.
        let grass_chroma = |p: &RoomPalette| srgb_to_oklch(p.grass_dry)[1];
        assert!(
            avg(BiomeArchetype::Jungle, &grass_chroma) > avg(BiomeArchetype::Tundra, &grass_chroma),
            "jungle grass should be more vivid than tundra"
        );

        // Wetland water is darker than coastal water.
        let water_lum =
            |p: &RoomPalette| p.water_shallow[0] + p.water_shallow[1] + p.water_shallow[2];
        assert!(
            avg(BiomeArchetype::Wetland, &water_lum) < avg(BiomeArchetype::Coastal, &water_lum),
            "wetland water should read darker/murkier than coastal"
        );

        // Glacial snow carries the ice-blue tint: blue meets-or-beats red.
        let snow_blue = |p: &RoomPalette| p.snow_dry[2] - p.snow_dry[0];
        assert!(
            avg(BiomeArchetype::Glacial, &snow_blue) >= 0.0,
            "glacial snow should lean blue"
        );
    }

    /// Same biome, different seeds: the in-band roam still produces
    /// visibly different palettes (hue spread over the band plus the
    /// harmonising cast), so same-archetype rooms don't collapse into
    /// one look.
    #[test]
    fn same_biome_rooms_still_vary() {
        let mut min_h = f32::MAX;
        let mut max_h = f32::MIN;
        for s in 0u64..32 {
            let p = RoomPalette::from_scene(&scene_for(BiomeArchetype::Lush, s), s);
            let [_, _, h] = srgb_to_oklch(p.grass_dry);
            min_h = min_h.min(h);
            max_h = max_h.max(h);
        }
        assert!(
            max_h - min_h > 10.0,
            "lush grass hue spread across seeds too narrow: {min_h}..{max_h}"
        );
    }
}
