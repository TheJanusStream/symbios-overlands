//! DID-seeded room palette derivation.
//!
//! Produces every colour the room consumes (terrain biomes, water, sky,
//! fog, sun, clouds) by sampling the OkLCH gamut, coherently anchored to the
//! [`SceneCharacter`]: every channel's hue is sampled relative to
//! `base_hue_deg`, with chroma biased by `temperature`.
//!
//! **The distribution favours realism.** Chroma offsets are low-biased and
//! hue jitter is centre-biased (see [`chroma_span`] / [`jitter`]), so the
//! *typical* room reads naturalistic — muted, with each channel near its
//! anchor — while vivid, wildly-divergent "own planet" palettes (cyan tundra,
//! magenta meadows) still occur, just as the rare tail rather than the
//! average. Role still constrains *lightness* so the splat layers read in the
//! expected order (snow brighter than rock, moist grass darker than dry, etc.);
//! hue and chroma roam, but gently by default.
//!
//! **Signature-biome coupling.** Three biomes whose colour *is* their
//! identity — Volcanic (lava red), Jungle (vivid green), Glacial (ice blue) —
//! get a small, bounded nudge toward that signature *after* the roam (see
//! [`biome_palette_bias`] / [`SIGNATURE_COUPLING`]). It leans hue + lifts
//! chroma on the relevant channels only, never lightness, so it reads as a
//! lean rather than a lock — the roamed palette still shows through, and every
//! other biome stays pure roam.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::oklch::{oklch_to_srgb, srgb_to_oklch, wrap_hue_deg};
use crate::seeded_defaults::scene::{BiomeArchetype, SceneCharacter, range_f32, unit_f32};

/// Sub-stream salt for the palette RNG, distinct from other derivers
/// so each (terrain shape, atmosphere, textures) advances its own RNG
/// state independently. Changing a sibling deriver won't drift the
/// palette.
const PALETTE_STREAM_SALT: u64 = 0xC010_C010_C010_C010;

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
    pub fn from_scene(scene: &SceneCharacter, room_seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ PALETTE_STREAM_SALT);
        let palette = derive(scene, &mut rng);
        // Gentle, bounded coupling toward a signature-biome chroma (#499) —
        // layered *after* the hue-roam so Volcanic/Jungle/Glacial read on-
        // signature without losing their "own planet" individuality; a no-op
        // for every other biome.
        biome_palette_bias(palette, scene.biome)
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
/// (coherent, naturalistic) and only the rare tail diverges the full `span`.
/// Consumes one draw, like a plain uniform jitter, so the palette stays
/// deterministic and the RNG stream is byte-identical in length.
fn jitter(rng: &mut ChaCha8Rng, span: f32) -> f32 {
    let u = range_f32(rng, -1.0, 1.0);
    u * u.abs() * span
}

/// Low-biased `[lo, hi)` chroma offset: squares a uniform draw so most rooms
/// land near `lo` (muted, naturalistic) and only the rare tail reaches `hi`
/// (vivid). One draw, like [`range_f32`] — deterministic and stream-stable.
fn chroma_span(rng: &mut ChaCha8Rng, lo: f32, hi: f32) -> f32 {
    let u = unit_f32(rng);
    lo + u * u * (hi - lo)
}

// ---------------------------------------------------------------------------
// Master deriver
// ---------------------------------------------------------------------------

fn derive(scene: &SceneCharacter, rng: &mut ChaCha8Rng) -> RoomPalette {
    let base_hue = scene.base_hue_deg;
    // Centre-bias the warm/cool cast toward neutral so a strong colour cast is
    // the rare tail, not the average room. Local to colour — the scene axis
    // itself (read by audio, etc.) is unchanged.
    let temp = {
        let t = scene.temperature; // -1 cool ↔ +1 warm
        t * t.abs()
    };
    let tod = scene.time_of_day_bias.abs(); // proximity to horizon, 0..1

    // Hue "axes" rooted at base_hue: each role samples its own absolute
    // hue with very wide jitter, so two same-archetype rooms can land
    // on completely different colour worlds while still feeling like
    // intentional palettes (every channel still relates to base_hue
    // somehow, even after the jitter folds them around).
    let vegetation_hue = base_hue + jitter(rng, 150.0);
    let water_hue = base_hue + 180.0 + jitter(rng, 130.0);
    let rock_hue = base_hue + 60.0 + jitter(rng, 150.0);
    let snow_hue = base_hue + 30.0 + jitter(rng, 180.0); // snow is "free"
    let sky_hue = base_hue + jitter(rng, 180.0); // any-hue sky is the point
    let dirt_hue = vegetation_hue + jitter(rng, 90.0);

    // Warm temperatures lift saturation across every channel; cool
    // temperatures let it drop low. With the centre-biased `temp` above, the
    // floor sits low for most rooms — the naturalistic default — and only a
    // strongly-warm room lifts it.
    let chroma_floor = 0.06 + temp.max(0.0) * 0.04;

    // ---------------- SUN ----------------
    // The directional sun colour is tied to the fog's sun-glow tint
    // (`fog_sun_rgb`, derived in the FOG block below) so a seeded room's
    // key light and its atmospheric glow read as the same source. We
    // still burn the historical sun-drift draws here so this retargeting
    // doesn't ripple through the rest of the palette — sky, fog, water
    // and terrain stay byte-identical for a given seed.
    let _sun_drift = col(
        lerp(0.95, 0.86, tod) + jitter(rng, 0.04),
        chroma_floor + chroma_span(rng, 0.02, 0.14),
        base_hue + 30.0 + jitter(rng, 220.0),
    );

    // ---------------- SKY ----------------
    // No more "blue sky always": the sky picks its own hue from the
    // room's tonic. Lightness still tracks time-of-day so dusk reads
    // dimmer than noon.
    let sky_color = col(
        lerp(0.78, 0.55, tod) + jitter(rng, 0.06),
        chroma_floor + chroma_span(rng, 0.04, 0.18),
        sky_hue,
    );

    // ---------------- FOG ----------------
    // Fog is the most visible atmosphere channel; lean its hue toward
    // the sky (rooms read coherently) but allow a healthy wander.
    let fog_hue = sky_hue + jitter(rng, 60.0);
    let fog_chroma = chroma_floor + chroma_span(rng, 0.04, 0.18);
    let fog_l = lerp(0.62, 0.50, tod);
    let fog_rgb = col(fog_l + jitter(rng, 0.06), fog_chroma, fog_hue);
    let fog_color = col4(fog_rgb, 1.0);

    // Extinction (light *lost* to absorption): a step darker and
    // slightly off-hue from the fog body.
    let fog_extinction = col(
        (fog_l - 0.08 + jitter(rng, 0.04)).max(0.05),
        fog_chroma + jitter(rng, 0.02),
        fog_hue + jitter(rng, 30.0),
    );
    // Inscattering (light *gained* from the sun direction): a step
    // brighter and pulled toward the sun's hue.
    let fog_inscattering = col(
        (fog_l + 0.20 + jitter(rng, 0.04)).min(0.97),
        (fog_chroma * 0.5 + jitter(rng, 0.03)).max(0.02),
        base_hue + jitter(rng, 120.0),
    );
    // Fog-sun colour: a saturated tint behind directional light. In
    // fantasy mode this can be any colour the deriver lands on.
    let fog_sun_rgb = col(
        0.88 + jitter(rng, 0.06),
        chroma_floor + chroma_span(rng, 0.04, 0.20),
        base_hue + jitter(rng, 180.0),
    );
    let fog_sun_color = col4(fog_sun_rgb, 0.5);

    // Lighting & Sky "Sun colour" follows the Distance Fog "Sun glow"
    // tint (see the SUN note above): the same RGB, at full opacity.
    let sun_color = fog_sun_rgb;

    // ---------------- CLOUDS ----------------
    // Sunlit top is bright; shadowed underside is mid. Hue is loose —
    // a magenta cloud against a green sky is fair game.
    let cloud_sunlit = col(
        0.93 + jitter(rng, 0.04),
        chroma_floor + chroma_span(rng, 0.02, 0.12),
        base_hue + jitter(rng, 180.0),
    );
    let cloud_shadow = col(
        0.55 + jitter(rng, 0.06),
        chroma_floor + chroma_span(rng, 0.02, 0.14),
        sky_hue + jitter(rng, 90.0),
    );

    // ---------------- WATER ----------------
    // Per-volume colours: shallow (head-on) low-alpha, deep (grazing)
    // high-alpha. Both at the water hue, perturbed independently.
    let water_chroma = chroma_floor + chroma_span(rng, 0.06, 0.22);
    let water_shallow_rgb = col(
        range_f32(rng, 0.48, 0.65),
        water_chroma + jitter(rng, 0.03),
        water_hue + jitter(rng, 25.0),
    );
    let water_deep_rgb = col(
        range_f32(rng, 0.12, 0.22),
        water_chroma + jitter(rng, 0.03),
        water_hue + jitter(rng, 35.0),
    );
    let water_shallow = col4(water_shallow_rgb, range_f32(rng, 0.18, 0.28));
    let water_deep = col4(water_deep_rgb, range_f32(rng, 0.85, 0.92));

    // Room-global subsurface scatter (`water_scatter_color` on
    // `Environment`): picks up the crests with a strong hue shift
    // from the body water.
    let water_scatter = col(
        range_f32(rng, 0.38, 0.55),
        water_chroma + jitter(rng, 0.04),
        water_hue + jitter(rng, 80.0),
    );

    // ---------------- TERRAIN BIOMES ----------------
    // Vegetation: any-hue grass with high chroma. Moist is darker than
    // dry so the splat blend still reads the right way; hue stays
    // anchored to `vegetation_hue` (the deriver picked one above).
    let grass_chroma = chroma_floor + chroma_span(rng, 0.06, 0.22);
    let grass_dry = col(
        range_f32(rng, 0.18, 0.32),
        grass_chroma,
        vegetation_hue + jitter(rng, 25.0),
    );
    let grass_moist = col(
        range_f32(rng, 0.08, 0.18),
        grass_chroma + jitter(rng, 0.02),
        vegetation_hue + jitter(rng, 20.0),
    );

    let dirt_chroma = chroma_floor + chroma_span(rng, 0.06, 0.20);
    let dirt_dry = col(
        range_f32(rng, 0.40, 0.55),
        dirt_chroma,
        dirt_hue + jitter(rng, 30.0),
    );
    let dirt_moist = col(
        range_f32(rng, 0.20, 0.32),
        dirt_chroma + jitter(rng, 0.02),
        dirt_hue + jitter(rng, 25.0),
    );

    // Rocks: low-to-mid chroma so they read as solid mass, but in
    // fantasy mode the rock can be lavender, teal, magenta… anything
    // the deriver picked for `rock_hue`. `rock_gap` lifts the chroma
    // (the crack often reads more colourful than the face) and drops
    // lightness so the ridged-multifractal seam still looks like shadow.
    let rock_chroma = chroma_floor + chroma_span(rng, 0.02, 0.14);
    let rock_stone = col(
        range_f32(rng, 0.40, 0.58),
        rock_chroma,
        rock_hue + jitter(rng, 20.0),
    );
    let rock_gap = col(
        range_f32(rng, 0.06, 0.20),
        rock_chroma + chroma_span(rng, 0.02, 0.10),
        rock_hue + jitter(rng, 40.0),
    );

    // Snow: bright and pale on average, but in fantasy mode it can be
    // a bold tint (pink ice, cyan frost). Stays the brightest layer so
    // the snow line still reads as snow against the rock face.
    let snow_chroma = chroma_floor + chroma_span(rng, 0.01, 0.16);
    let snow_dry = col(
        range_f32(rng, 0.88, 0.97),
        snow_chroma * 0.5,
        snow_hue + jitter(rng, 40.0),
    );
    let snow_moist = col(
        range_f32(rng, 0.74, 0.86),
        snow_chroma,
        snow_hue + jitter(rng, 30.0),
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

/// Linear interpolation between `a` (at `t=0`) and `b` (at `t=1`).
/// `t` is clamped to `[0, 1]` for safety against arithmetic mishaps in
/// upstream callers.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Signature-biome chroma coupling (#499)
// ---------------------------------------------------------------------------

/// Master dial for the signature-biome coupling, `0.0..=1.0`. `0.0` restores
/// the pure hue-roam (no coupling at all); `1.0` applies the per-channel
/// nudges below at full strength. Scales every hue lean + chroma lift, so this
/// is the one knob to turn if the coupling reads too strong / too weak in-app.
const SIGNATURE_COUPLING: f32 = 1.0;

// Signature target hues, in OkLCH degrees (red ≈ 30, orange ≈ 55, green ≈ 145,
// ice-blue ≈ 235).
const HUE_LAVA: f32 = 38.0;
const HUE_EMBER: f32 = 30.0;
const HUE_SCORCH: f32 = 58.0;
const HUE_FOLIAGE: f32 = 145.0;
const HUE_ICE: f32 = 235.0;

/// Move `from` toward `to` along the shortest arc of the hue circle by
/// fraction `t` (`0` = unchanged, `1` = exactly `to`); result wrapped to
/// `[0, 360)`.
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

/// Lean an sRGB colour a bounded fraction `hue_t` toward `target_hue` (OkLCH,
/// shortest arc) and add `chroma_boost`, leaving **lightness untouched** so
/// the splat lightness ordering (and the snow/rock read) survives. Both the
/// lean and the boost are scaled by [`SIGNATURE_COUPLING`]; the OkLCH→sRGB
/// conversion clamps back into gamut.
fn lean(rgb: [f32; 3], target_hue: f32, hue_t: f32, chroma_boost: f32) -> [f32; 3] {
    let [l, c, h] = srgb_to_oklch(rgb);
    let h2 = nudge_hue(h, target_hue, hue_t * SIGNATURE_COUPLING);
    let c2 = (c + chroma_boost * SIGNATURE_COUPLING).max(0.0);
    oklch_to_srgb([l, c2, h2])
}

/// [`lean`] for an RGBA channel — biases the colour, preserves the alpha.
fn lean4(rgba: [f32; 4], target_hue: f32, hue_t: f32, chroma_boost: f32) -> [f32; 4] {
    let [r, g, b] = lean([rgba[0], rgba[1], rgba[2]], target_hue, hue_t, chroma_boost);
    [r, g, b, rgba[3]]
}

/// Bounded post-roam nudge toward a biome's signature colour, for the three
/// biomes whose identity *is* their colour. Leans the relevant channels' hue
/// toward the signature and lifts their chroma; lightness is never touched, so
/// it is a lean, not a lock. Every other biome is returned unchanged — the
/// pure "own planet" roam. Deterministic (no RNG).
fn biome_palette_bias(mut p: RoomPalette, biome: BiomeArchetype) -> RoomPalette {
    match biome {
        // Lava red: the rock face + crack glow toward ember, the ground runs
        // warm-scorched, and the haze picks up a faint warm cast.
        BiomeArchetype::Volcanic => {
            p.rock_stone = lean(p.rock_stone, HUE_LAVA, 0.45, 0.04);
            p.rock_gap = lean(p.rock_gap, HUE_EMBER, 0.55, 0.05);
            p.dirt_dry = lean(p.dirt_dry, HUE_LAVA, 0.40, 0.03);
            p.dirt_moist = lean(p.dirt_moist, HUE_LAVA, 0.40, 0.03);
            p.grass_dry = lean(p.grass_dry, HUE_SCORCH, 0.30, 0.0);
            p.grass_moist = lean(p.grass_moist, HUE_SCORCH, 0.30, 0.0);
            p.fog_color = lean4(p.fog_color, HUE_LAVA, 0.22, 0.02);
        }
        // Vivid green: the vegetation leans hard to foliage with lifted
        // chroma (gentler on the dark moist layer, whose gamut is tighter);
        // soil leans a touch loamy-green and the haze reads humid.
        BiomeArchetype::Jungle => {
            p.grass_dry = lean(p.grass_dry, HUE_FOLIAGE, 0.50, 0.06);
            p.grass_moist = lean(p.grass_moist, HUE_FOLIAGE, 0.50, 0.04);
            p.dirt_dry = lean(p.dirt_dry, HUE_FOLIAGE, 0.22, 0.0);
            p.dirt_moist = lean(p.dirt_moist, HUE_FOLIAGE, 0.22, 0.0);
            p.fog_color = lean4(p.fog_color, HUE_FOLIAGE, 0.20, 0.02);
        }
        // Ice blue: snow, water and rock cool toward ice; sky + haze chill to
        // match. Hue lean carries the signature — chroma is *not* boosted on
        // the blue channels (their gamut is tight, so a lift only clamps and
        // skews the hue), bar a faint tint on the near-white snow.
        BiomeArchetype::Glacial => {
            p.snow_dry = lean(p.snow_dry, HUE_ICE, 0.40, 0.02);
            p.snow_moist = lean(p.snow_moist, HUE_ICE, 0.40, 0.03);
            p.water_shallow = lean4(p.water_shallow, HUE_ICE, 0.45, 0.0);
            p.water_deep = lean4(p.water_deep, HUE_ICE, 0.45, 0.0);
            p.water_scatter = lean(p.water_scatter, HUE_ICE, 0.45, 0.0);
            p.rock_stone = lean(p.rock_stone, HUE_ICE, 0.30, 0.0);
            p.rock_gap = lean(p.rock_gap, HUE_ICE, 0.30, 0.0);
            p.sky_color = lean(p.sky_color, HUE_ICE, 0.30, 0.0);
            p.fog_color = lean4(p.fog_color, HUE_ICE, 0.30, 0.01);
        }
        // Every other biome keeps the pure hue-roam.
        _ => {}
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_defaults::hash::fnv1a_64;
    use crate::seeded_defaults::scene::BiomeArchetype;

    fn finite_rgb(c: [f32; 3]) -> bool {
        c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    }
    fn finite_rgba(c: [f32; 4]) -> bool {
        c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    }

    #[test]
    fn all_channels_are_finite_in_gamut() {
        // Sweep biomes for parity with the old test surface — the new
        // deriver doesn't branch on biome, but covering every archetype
        // still catches any RNG sub-stream regression.
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
        // The splat blend reads "moist soil under dry soil" — that
        // ordering needs to survive the wider colour rolls. Average
        // luminance across many seeds rather than asserting per-seed
        // (any single roll can land tightly on either side).
        let mut dry_lum = 0.0;
        let mut moist_lum = 0.0;
        for s in 0u64..64 {
            let scene = SceneCharacter::for_seed(s);
            let p = RoomPalette::from_scene(&scene, s);
            dry_lum += p.grass_dry[0] + p.grass_dry[1] + p.grass_dry[2];
            moist_lum += p.grass_moist[0] + p.grass_moist[1] + p.grass_moist[2];
        }
        assert!(
            moist_lum < dry_lum,
            "moist grass should average darker than dry (moist={moist_lum} dry={dry_lum})"
        );
    }

    fn hue_dist(a: f32, b: f32) -> f32 {
        let d = (wrap_hue_deg(a) - wrap_hue_deg(b)).abs();
        d.min(360.0 - d)
    }

    /// The signature coupling is a no-op for every non-signature biome — those
    /// keep the pure hue-roam, byte-identical to the unbiased derive.
    #[test]
    fn bias_is_identity_for_non_signature_biomes() {
        for biome in BiomeArchetype::ALL {
            if matches!(
                biome,
                BiomeArchetype::Volcanic | BiomeArchetype::Jungle | BiomeArchetype::Glacial
            ) {
                continue;
            }
            for s in 0u64..6 {
                let mut scene = SceneCharacter::for_seed(s);
                scene.biome = biome;
                let mut rng = ChaCha8Rng::seed_from_u64(s ^ PALETTE_STREAM_SALT);
                let base = derive(&scene, &mut rng);
                assert_eq!(
                    base.clone(),
                    biome_palette_bias(base, biome),
                    "{biome:?} must stay pure roam"
                );
            }
        }
    }

    /// The `lean` primitive moves hue toward the target along the shortest arc
    /// and leaves lightness untouched (so the splat ordering survives). Tested
    /// on a low-chroma in-gamut sample so no clamp distorts the round trip.
    #[test]
    fn lean_moves_hue_toward_target_and_preserves_lightness() {
        let base = col(0.5, 0.05, 200.0); // teal-ish, comfortably in gamut
        let [bl, _, bh] = srgb_to_oklch(base);
        for target in [HUE_LAVA, HUE_FOLIAGE, HUE_ICE] {
            let leaned = lean(base, target, 0.5, 0.0);
            let [ll, _, lh] = srgb_to_oklch(leaned);
            assert!(
                hue_dist(lh, target) < hue_dist(bh, target),
                "hue should lean toward {target} ({bh} -> {lh})"
            );
            assert!(
                (ll - bl).abs() < 0.01,
                "lightness must be preserved ({bl} -> {ll})"
            );
        }
    }

    /// For the three signature biomes the bias actually changes the palette
    /// and every channel stays finite + in gamut.
    #[test]
    fn signature_bias_changes_palette_and_stays_in_gamut() {
        for biome in [
            BiomeArchetype::Volcanic,
            BiomeArchetype::Jungle,
            BiomeArchetype::Glacial,
        ] {
            for s in 0u64..24 {
                let mut scene = SceneCharacter::for_seed(s);
                scene.biome = biome;
                let mut rng = ChaCha8Rng::seed_from_u64(s ^ PALETTE_STREAM_SALT);
                let base = derive(&scene, &mut rng);
                let p = biome_palette_bias(base.clone(), biome);
                assert_ne!(base, p, "{biome:?} bias must change the palette");
                assert!(
                    finite_rgb(p.grass_dry)
                        && finite_rgb(p.grass_moist)
                        && finite_rgb(p.dirt_dry)
                        && finite_rgb(p.rock_stone)
                        && finite_rgb(p.rock_gap)
                        && finite_rgb(p.snow_dry)
                        && finite_rgb(p.snow_moist)
                        && finite_rgb(p.water_scatter)
                        && finite_rgb(p.sky_color)
                        && finite_rgba(p.water_shallow)
                        && finite_rgba(p.water_deep)
                        && finite_rgba(p.fog_color),
                    "{biome:?} seed {s} produced an out-of-gamut channel"
                );
            }
        }
    }
}
