//! A seeded world's climate (#1770): the weather its biome brings, coming
//! and going in spells on the world's own timeline, fair most of the time
//! (owner decision, epic #1767).
//!
//! **The kind** is drawn from the biome's table ([`kinds`]): rain and the
//! odd storm over the green biomes, snow over the cold ones, fog over the
//! wet ones, dust over the dry ones - and storms rare everywhere, because a
//! storm's lightning flashes for every visitor at the default Effects
//! setting (#1687). Under a dark signature hour - the blue hour's or the
//! night's ([`super::hour::Mood::is_dark`]) - there is no storm and the
//! weather comes lighter: rain over a world already near dark drew a black
//! first frame.
//!
//! **The spells** ([`crate::weather::severity_at`]) are value noise over
//! spells counted since 1970, salted by the weather's kind and spell length
//! and nothing else: two worlds set alike share one timeline and would rain
//! at the same moment worldwide. So the spell length is the world's own,
//! drawn on the wire grid's ten-thousandths of a minute; the share of the
//! time it is here is low, so an arrival most often finds fair weather.
//!
//! **The fair sky is thinned** ([`Climate::clear_the_fair_sky`]): a kind
//! that brings its own cloud closes the deck as it comes, so the seeded
//! cover - the sky between spells - is lighter than a world without weather
//! would carry, and the sun and the sky's light it dimmed are given back in
//! the seeder's own proportion (`atmosphere.rs`). Otherwise a rainy world
//! would stand under a heavy sky all day and a spell would barely show.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::pds::{Environment, Fp, Weather, WeatherKind};
use crate::seeded_defaults::scene::{BiomeArchetype, SceneCharacter, range_f32, unit_f32};

/// Sub-stream salt distinct from every other deriver's.
const CLIMATE_STREAM_SALT: u64 = 0xC11A_7E5E_ED00_1770;

/// How long a spell lasts, minutes: long enough that a visit sees a spell
/// come or go rather than flicker, short enough that it does.
const SPELL_MINUTES: (f32, f32) = (14.0, 48.0);

/// The share of a cloud-bringing weather's world's seeded cover its fair
/// sky keeps.
const FAIR_COVER: f32 = 0.6;

/// How the seeder dims the sun and the sky's light by cloud cover, from
/// clear to overcast (`atmosphere.rs`): what thinning the cover gives back.
const SUN_BY_COVER: f32 = 0.45;
const AMBIENT_BY_COVER: f32 = 0.15;

/// How often a biome's world has each kind of weather.
pub fn kinds(biome: BiomeArchetype) -> &'static [(WeatherKind, u32)] {
    use BiomeArchetype::*;
    use WeatherKind::*;
    match biome {
        Lush | TemperateForest | Meadow => &[(Rain, 62), (Overcast, 20), (Fog, 12), (Storm, 6)],
        Jungle => &[(Rain, 65), (Storm, 15), (Fog, 20)],
        Wetland => &[(Fog, 50), (Rain, 40), (Overcast, 10)],
        Coastal => &[(Fog, 40), (Rain, 50), (Storm, 10)],
        Boreal => &[(Snow, 55), (Rain, 25), (Fog, 20)],
        Alpine => &[(Snow, 70), (Fog, 25), (Storm, 5)],
        Tundra => &[(Snow, 80), (Fog, 20)],
        Glacial => &[(Snow, 85), (Fog, 15)],
        Arid => &[(Dust, 85), (Overcast, 15)],
        Badlands => &[(Dust, 85), (Storm, 15)],
        Savanna => &[(Dust, 45), (Rain, 40), (Storm, 15)],
        Volcanic => &[(Dust, 65), (Overcast, 25), (Storm, 10)],
    }
}

/// How heavily a weather comes under a dark hour - the blue hour's or the
/// night's - against the lit hours': a storm, or a downpour, over a world
/// already near dark draws a black frame for its first sight (#1770).
const DARK_INTENSITY: f32 = 0.7;

/// How heavily each kind comes at its heaviest, and the share of the time
/// it is here: a storm comes harder and far more rarely.
fn bands(kind: WeatherKind) -> ((f32, f32), (f32, f32)) {
    match kind {
        WeatherKind::Storm => ((0.55, 0.9), (0.06, 0.14)),
        WeatherKind::Fog => ((0.35, 0.7), (0.15, 0.3)),
        WeatherKind::Overcast => ((0.5, 0.85), (0.2, 0.35)),
        _ => ((0.35, 0.75), (0.15, 0.3)),
    }
}

/// A world's climate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Climate {
    pub kind: WeatherKind,
    /// How heavily it comes at its heaviest, 0 to 1.
    pub intensity: f32,
    /// The share of the time it is here, 0 to 1.
    pub share: f32,
    /// How long a spell lasts, minutes.
    pub spell_minutes: f32,
}

impl Climate {
    /// `scene`'s world's climate, from its own stream of `room_seed`. Under
    /// a `dark` hour - the world's signature hour is the blue hour's or the
    /// night's - its biome brings no storm, and its weather comes lighter
    /// ([`DARK_INTENSITY`]).
    pub fn from_scene(scene: &SceneCharacter, room_seed: u64, dark: bool) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ CLIMATE_STREAM_SALT);
        let table: Vec<(WeatherKind, u32)> = kinds(scene.biome)
            .iter()
            .filter(|(kind, _)| !(dark && *kind == WeatherKind::Storm))
            .copied()
            .collect();
        let total: u32 = table.iter().map(|(_, w)| w).sum();
        let mut at = unit_f32(&mut rng) * total as f32;
        let kind = table
            .iter()
            .find(|(_, w)| {
                let here = at < *w as f32;
                at -= *w as f32;
                here
            })
            .or(table.last())
            .map_or(WeatherKind::Rain, |(kind, _)| *kind);
        let ((i_lo, i_hi), (s_lo, s_hi)) = bands(kind);
        let lighter = if dark { DARK_INTENSITY } else { 1.0 };
        Self {
            kind,
            intensity: range_f32(&mut rng, i_lo, i_hi) * lighter,
            share: range_f32(&mut rng, s_lo, s_hi),
            spell_minutes: range_f32(&mut rng, SPELL_MINUTES.0, SPELL_MINUTES.1),
        }
    }

    /// The record's weather.
    pub fn weather(&self) -> Weather {
        Weather {
            kind: self.kind,
            intensity: Fp(self.intensity),
            spell_minutes: Fp(self.spell_minutes),
            share: Fp(self.share),
        }
    }

    /// Whether the weather closes the cloud deck as it comes (#1678): rain,
    /// a storm, snow and an overcast do; fog greys the sky a little and dust
    /// none.
    pub fn brings_cloud(&self) -> bool {
        matches!(
            self.kind,
            WeatherKind::Rain | WeatherKind::Storm | WeatherKind::Snow | WeatherKind::Overcast
        )
    }

    /// Thin `env`'s seeded cloud cover to the fair sky between spells, when
    /// the weather brings its own cloud, and give back the sun and the sky's
    /// light the cover had dimmed (see the module docs).
    pub fn clear_the_fair_sky(&self, env: &mut Environment) {
        if !self.brings_cloud() {
            return;
        }
        let was = env.cloud_cover.0.clamp(0.0, 1.0);
        let now = was * FAIR_COVER;
        let given = |by: f32| (1.0 - by * now) / (1.0 - by * was);
        env.cloud_cover = Fp(now);
        env.sun_illuminance = Fp(env.sun_illuminance.0 * given(SUN_BY_COVER));
        env.ambient_brightness = Fp(env.ambient_brightness.0 * given(AMBIENT_BY_COVER));
    }

    /// Write the climate into `env`: its weather, and its fair sky.
    pub fn apply(&self, env: &mut Environment) {
        self.clear_the_fair_sky(env);
        env.weather = Some(self.weather());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every biome's table is a weather for every draw, the cold biomes snow
    /// and the dry ones raise dust, and a storm is never a biome's commonest
    /// weather.
    #[test]
    fn each_biome_brings_its_own_weather_and_storms_are_rare() {
        for biome in BiomeArchetype::ALL {
            let table = kinds(biome);
            let total: u32 = table.iter().map(|(_, w)| w).sum();
            assert!(total > 0, "{biome:?}");
            let storm = table
                .iter()
                .find(|(k, _)| *k == WeatherKind::Storm)
                .map_or(0, |(_, w)| *w);
            assert!(storm * 3 < total, "{biome:?}: storms are a third or more");
        }
        for biome in [
            BiomeArchetype::Tundra,
            BiomeArchetype::Glacial,
            BiomeArchetype::Alpine,
        ] {
            assert_eq!(kinds(biome)[0].0, WeatherKind::Snow, "{biome:?}");
        }
        assert_eq!(kinds(BiomeArchetype::Arid)[0].0, WeatherKind::Dust);
    }

    /// A world's climate is its own and the same every time: a kind its
    /// biome brings, inside its bands, on a spell length no other of 300
    /// worlds with its kind shares - the timeline's salt is the kind and the
    /// spell length alone, read on the wire grid.
    #[test]
    fn every_world_rains_on_its_own_timeline() {
        let mut salts = std::collections::HashSet::new();
        for seed in 0..300u64 {
            let scene = SceneCharacter::for_seed(seed);
            let dark = crate::seeded_defaults::SignatureHour::from_scene(&scene, seed)
                .mood
                .is_dark();
            let climate = Climate::from_scene(&scene, seed, dark);
            assert_eq!(climate, Climate::from_scene(&scene, seed, dark));
            assert!(kinds(scene.biome).iter().any(|(k, _)| *k == climate.kind));
            let ((i_lo, i_hi), (s_lo, s_hi)) = bands(climate.kind);
            let lighter = if dark { DARK_INTENSITY } else { 1.0 };
            assert!(
                (i_lo * lighter..=i_hi * lighter).contains(&climate.intensity),
                "seed {seed}"
            );
            assert!((s_lo..=s_hi).contains(&climate.share), "seed {seed}");
            assert!(climate.share <= 0.35, "seed {seed}: fair most of the time");
            let mut weather = climate.weather();
            weather.spell_minutes = weather.spell_minutes.on_wire();
            assert!(
                salts.insert(crate::weather::salt(&weather)),
                "seed {seed}: another world shares its timeline"
            );
        }
    }

    /// Under a dark hour no world storms and every weather comes lighter;
    /// in the light the same biomes do storm, now and then.
    #[test]
    fn a_dark_hour_brings_no_storm_and_a_lighter_weather() {
        let stormy: Vec<SceneCharacter> = (0..2_000u64)
            .map(SceneCharacter::for_seed)
            .filter(|s| kinds(s.biome).iter().any(|(k, _)| *k == WeatherKind::Storm))
            .take(300)
            .collect();
        let mut lit_storms = 0;
        for (i, scene) in stormy.iter().enumerate() {
            let seed = i as u64;
            let dark = Climate::from_scene(scene, seed, true);
            assert_ne!(dark.kind, WeatherKind::Storm, "{:?}", scene.biome);
            assert!(dark.intensity <= 0.9 * DARK_INTENSITY + 1e-6);
            lit_storms +=
                usize::from(Climate::from_scene(scene, seed, false).kind == WeatherKind::Storm);
        }
        assert!(lit_storms > 0, "the lit hours storm too");
    }

    /// A weather that brings its own cloud thins the fair sky and gives the
    /// sun and the sky's light back in the seeder's proportion - as if the
    /// atmosphere had drawn the thinner cover; fog and dust leave the sky as
    /// it was.
    #[test]
    fn a_cloud_bringing_weather_clears_its_fair_sky() {
        let clear_sun = 20_000.0;
        let clear_ambient = 400.0;
        let env = Environment {
            cloud_cover: Fp(0.6),
            sun_illuminance: Fp(clear_sun * (1.0 - SUN_BY_COVER * 0.6)),
            ambient_brightness: Fp(clear_ambient * (1.0 - AMBIENT_BY_COVER * 0.6)),
            ..Environment::default()
        };
        let rain = Climate {
            kind: WeatherKind::Rain,
            intensity: 0.5,
            share: 0.2,
            spell_minutes: 20.0,
        };
        let mut wet = env.clone();
        rain.apply(&mut wet);
        let now = 0.6 * FAIR_COVER;
        assert!((wet.cloud_cover.0 - now).abs() < 1e-6);
        assert!((wet.sun_illuminance.0 - clear_sun * (1.0 - SUN_BY_COVER * now)).abs() < 0.5);
        assert!(
            (wet.ambient_brightness.0 - clear_ambient * (1.0 - AMBIENT_BY_COVER * now)).abs()
                < 1e-2
        );
        assert_eq!(wet.weather, Some(rain.weather()));

        for kind in [WeatherKind::Fog, WeatherKind::Dust] {
            let mut dry = env.clone();
            Climate { kind, ..rain }.apply(&mut dry);
            assert_eq!(dry.cloud_cover, env.cloud_cover, "{kind:?}");
            assert_eq!(dry.sun_illuminance, env.sun_illuminance, "{kind:?}");
        }
    }
}
