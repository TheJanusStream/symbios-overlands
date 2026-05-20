//! Seeded atmosphere derivers: water dynamics, clouds, sun, fog.
//!
//! Sits in the slot between the palette deriver (already coloured
//! every channel) and the consumer record fields. Reads
//! [`SceneCharacter`] for archetype biases — alpine pulls the sky
//! clear, volcanic hazes the fog, archipelago/coastal make water
//! choppier — and writes its outputs into [`WaterDynamics`] (per-
//! volume) and [`Atmosphere`] (room-global) for the wiring layer to
//! drop onto the PDS record.
//!
//! Sun position is sampled in spherical coordinates: altitude is
//! pulled toward the horizon by `time_of_day_bias`, azimuth is fully
//! random. Cartesian conversion (radius × spherical → world XYZ) is
//! straight trigonometry — see [`spherical_to_world`].

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::scene::{BiomeArchetype, LandformArchetype, SceneCharacter, range_f32};

/// Sub-stream salt distinct from palette / terrain / textures.
const ATMOSPHERE_STREAM_SALT: u64 = 0xA1A2_A3A4_A5A6_A7A8;

/// Per-volume water dynamics — apply to a [`crate::pds::WaterSurface`].
#[derive(Clone, Copy, Debug)]
pub struct WaterDynamics {
    pub wave_direction: [f32; 2],
    pub wave_scale: f32,
    pub wave_speed: f32,
    pub wave_choppiness: f32,
    pub foam_amount: f32,
    pub roughness: f32,
}

impl WaterDynamics {
    pub fn from_scene(scene: &SceneCharacter, room_seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ ATMOSPHERE_STREAM_SALT ^ 0xAB17);
        derive_water(scene, &mut rng)
    }
}

/// Room-global atmosphere — applies to a [`crate::pds::Environment`].
/// Per-volume water dynamics live separately on [`WaterDynamics`]; the
/// `water_normal_scale_*` and `water_sun_glitter` knobs here are
/// scene-wide, not per-puddle.
#[derive(Clone, Copy, Debug)]
pub struct Atmosphere {
    pub sun_position: [f32; 3],
    pub sun_illuminance: f32,
    pub ambient_brightness: f32,

    pub fog_visibility: f32,
    pub fog_sun_exponent: f32,

    pub water_normal_scale_near: f32,
    pub water_normal_scale_far: f32,
    pub water_sun_glitter: f32,

    pub cloud_cover: f32,
    pub cloud_density: f32,
    pub cloud_softness: f32,
    pub cloud_speed: f32,
    pub cloud_scale: f32,
    pub cloud_height: f32,
    pub cloud_wind_dir: [f32; 2],
}

impl Atmosphere {
    pub fn from_scene(scene: &SceneCharacter, room_seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ ATMOSPHERE_STREAM_SALT);
        derive_atmosphere(scene, &mut rng)
    }
}

// ---------------------------------------------------------------------------
// Water deriver
// ---------------------------------------------------------------------------

fn derive_water(scene: &SceneCharacter, rng: &mut ChaCha8Rng) -> WaterDynamics {
    // Landform decides how lively the water reads: archipelago and
    // coastal rooms get more chop / faster speed, alpine and tundra
    // calm down.
    let (chop_lo, chop_hi, speed_lo, speed_hi, scale_lo, scale_hi) = match scene.landform {
        LandformArchetype::Archipelago => (0.30, 0.55, 0.9, 1.5, 0.7, 1.1),
        LandformArchetype::Valleys => (0.20, 0.40, 0.8, 1.3, 0.5, 0.9),
        LandformArchetype::Rolling => (0.15, 0.35, 0.6, 1.1, 0.5, 0.9),
        LandformArchetype::Craggy => (0.25, 0.50, 0.8, 1.4, 0.6, 1.0),
        LandformArchetype::Mesa => (0.15, 0.35, 0.7, 1.2, 0.4, 0.8),
    };
    // Alpine and tundra biomes settle the water further.
    let (chop_lo, chop_hi, speed_lo, speed_hi) = match scene.biome {
        BiomeArchetype::Alpine | BiomeArchetype::Tundra => {
            (chop_lo * 0.6, chop_hi * 0.7, speed_lo * 0.7, speed_hi * 0.8)
        }
        BiomeArchetype::Coastal => (
            (chop_lo + 0.05_f32).min(0.6),
            (chop_hi + 0.10_f32).min(0.6),
            speed_lo,
            (speed_hi + 0.2_f32).min(1.8),
        ),
        _ => (chop_lo, chop_hi, speed_lo, speed_hi),
    };

    let wave_dir_x = range_f32(rng, -1.0, 1.0);
    let wave_dir_z = range_f32(rng, -1.0, 1.0);
    // Avoid the zero vector — sanitise would clamp it, but skipping
    // the dead band here keeps the visible output well-distributed.
    let wave_direction = if wave_dir_x * wave_dir_x + wave_dir_z * wave_dir_z < 0.05 {
        [1.0, 0.3]
    } else {
        [wave_dir_x, wave_dir_z]
    };

    WaterDynamics {
        wave_direction,
        wave_scale: range_f32(rng, scale_lo, scale_hi),
        wave_speed: range_f32(rng, speed_lo, speed_hi),
        wave_choppiness: range_f32(rng, chop_lo, chop_hi),
        foam_amount: range_f32(rng, 0.15, 0.40),
        roughness: range_f32(rng, 0.10, 0.18),
    }
}

// ---------------------------------------------------------------------------
// Atmosphere deriver
// ---------------------------------------------------------------------------

fn derive_atmosphere(scene: &SceneCharacter, rng: &mut ChaCha8Rng) -> Atmosphere {
    // -- Sun position (spherical → Cartesian) -------------------------------
    // Altitude is pulled toward the horizon by `time_of_day_bias`: at
    // `±1` we hover around 20° elevation (dawn / dusk), at `0` we're
    // near 60° (high sun). Azimuth is uniformly random.
    let tod = scene.time_of_day_bias.abs(); // proximity to horizon, 0..1
    let altitude_deg = lerp(60.0, 18.0, tod) + range_f32(rng, -8.0, 8.0);
    let azimuth_deg = range_f32(rng, 0.0, 360.0);
    let sun_position = spherical_to_world(altitude_deg, azimuth_deg, 75.0);

    // -- Sun illuminance + ambient -----------------------------------------
    let sun_illuminance = lerp(20_000.0, 9_000.0, tod) + range_f32(rng, -1_500.0, 1_500.0);
    let ambient_brightness = lerp(450.0, 250.0, tod) + range_f32(rng, -50.0, 50.0);

    // -- Fog ----------------------------------------------------------------
    // Alpine = far-seeing, volcanic = hazy, coastal = humid mid-distance.
    let (vis_lo, vis_hi) = match scene.biome {
        BiomeArchetype::Alpine => (400.0, 600.0),
        BiomeArchetype::Tundra => (350.0, 550.0),
        BiomeArchetype::Lush => (300.0, 450.0),
        BiomeArchetype::Coastal => (250.0, 400.0),
        BiomeArchetype::Arid => (300.0, 500.0),
        BiomeArchetype::Volcanic => (180.0, 320.0),
    };
    let fog_visibility = range_f32(rng, vis_lo, vis_hi);
    let fog_sun_exponent = range_f32(rng, 18.0, 50.0);

    // -- Water-global -------------------------------------------------------
    let water_normal_scale_near = range_f32(rng, 0.55, 1.20);
    let water_normal_scale_far = range_f32(rng, 0.05, 0.14);
    let water_sun_glitter = range_f32(rng, 1.2, 2.8);

    // -- Clouds -------------------------------------------------------------
    // Biome biases cover; tundra/alpine overcast more, arid clearer.
    let (cover_lo, cover_hi) = match scene.biome {
        BiomeArchetype::Tundra | BiomeArchetype::Alpine => (0.45, 0.75),
        BiomeArchetype::Arid => (0.10, 0.35),
        BiomeArchetype::Volcanic => (0.30, 0.60),
        _ => (0.25, 0.60),
    };
    let cloud_cover = range_f32(rng, cover_lo, cover_hi);
    let cloud_density = range_f32(rng, 0.60, 0.95);
    let cloud_softness = range_f32(rng, 0.10, 0.28);
    let cloud_speed = range_f32(rng, 1.5, 8.0);
    let cloud_scale = range_f32(rng, 180.0, 460.0);
    let cloud_height = range_f32(rng, 180.0, 380.0);
    let wd_x = range_f32(rng, -1.0, 1.0);
    let wd_z = range_f32(rng, -1.0, 1.0);
    let cloud_wind_dir = if wd_x * wd_x + wd_z * wd_z < 0.05 {
        [1.0, 0.3]
    } else {
        [wd_x, wd_z]
    };

    Atmosphere {
        sun_position,
        sun_illuminance,
        ambient_brightness,
        fog_visibility,
        fog_sun_exponent,
        water_normal_scale_near,
        water_normal_scale_far,
        water_sun_glitter,
        cloud_cover,
        cloud_density,
        cloud_softness,
        cloud_speed,
        cloud_scale,
        cloud_height,
        cloud_wind_dir,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// Spherical → world Cartesian. `altitude_deg` is the angle above the
/// horizon (0° at horizon, 90° at zenith); `azimuth_deg` rotates
/// around the Y axis (0° → +X). Returns a point at `radius` distance
/// from the origin, suitable as the eye for a `looking_at(origin)`
/// directional light transform.
fn spherical_to_world(altitude_deg: f32, azimuth_deg: f32, radius: f32) -> [f32; 3] {
    let alt = altitude_deg.to_radians();
    let az = azimuth_deg.to_radians();
    let horiz = radius * alt.cos();
    [horiz * az.cos(), radius * alt.sin(), horiz * az.sin()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let scene = SceneCharacter::for_seed(42);
        let a = Atmosphere::from_scene(&scene, 42);
        let b = Atmosphere::from_scene(&scene, 42);
        assert_eq!(a.cloud_cover, b.cloud_cover);
        assert_eq!(a.sun_position, b.sun_position);
        assert_eq!(a.fog_visibility, b.fog_visibility);
    }

    #[test]
    fn water_finite_across_landforms() {
        for landform in LandformArchetype::ALL {
            for biome in BiomeArchetype::ALL {
                for s in 0u64..3 {
                    let mut scene = SceneCharacter::for_seed(s);
                    scene.landform = landform;
                    scene.biome = biome;
                    let w = WaterDynamics::from_scene(&scene, s);
                    assert!(w.wave_scale.is_finite() && w.wave_scale > 0.0);
                    assert!(w.wave_speed.is_finite() && w.wave_speed > 0.0);
                    assert!(
                        w.wave_choppiness.is_finite() && (0.0..=1.0).contains(&w.wave_choppiness)
                    );
                    assert!(w.foam_amount.is_finite() && (0.0..=1.0).contains(&w.foam_amount));
                    assert!(w.roughness.is_finite() && (0.0..=1.0).contains(&w.roughness));
                    let mag = w.wave_direction[0].powi(2) + w.wave_direction[1].powi(2);
                    assert!(mag > 0.0, "wave_direction collapsed to zero: {w:?}");
                }
            }
        }
    }

    #[test]
    fn atmosphere_finite_across_biomes() {
        for biome in BiomeArchetype::ALL {
            for s in 0u64..3 {
                let mut scene = SceneCharacter::for_seed(s);
                scene.biome = biome;
                let a = Atmosphere::from_scene(&scene, s);
                assert!(a.sun_illuminance > 0.0);
                assert!(a.ambient_brightness > 0.0);
                assert!(a.fog_visibility > 0.0);
                assert!((0.0..=1.0).contains(&a.cloud_cover));
                assert!((0.0..=1.0).contains(&a.cloud_density));
                assert!(a.cloud_height > 0.0);
                assert!(a.cloud_scale > 0.0);
                let sp = a.sun_position;
                let mag = sp[0].powi(2) + sp[1].powi(2) + sp[2].powi(2);
                assert!(mag > 1.0, "sun_position degenerate: {sp:?}");
            }
        }
    }

    #[test]
    fn spherical_unit_vector_check() {
        // Altitude 90° = straight up.
        let p = spherical_to_world(90.0, 0.0, 10.0);
        assert!(p[0].abs() < 1e-4);
        assert!((p[1] - 10.0).abs() < 1e-3);
        assert!(p[2].abs() < 1e-4);

        // Altitude 0°, azimuth 0° = +X horizon.
        let p = spherical_to_world(0.0, 0.0, 10.0);
        assert!((p[0] - 10.0).abs() < 1e-3);
        assert!(p[1].abs() < 1e-4);
        assert!(p[2].abs() < 1e-4);
    }
}
