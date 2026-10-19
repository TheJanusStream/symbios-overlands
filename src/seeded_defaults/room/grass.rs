//! Seeded grass (#1703): the grass a DID-seeded world grows, set in its
//! terrain's grass group (#1700) as an owner could set it - each biome its
//! own, each seed its own within the biome.
//!
//! * **Its colours** follow the world's own grass layer, the palette's
//!   seeded sample of the biome's grass band as it drew it
//!   ([`RoomPalette::grass_dry_lch`]): its hue, leaning warm or cool with
//!   the world - an arid world's straw, a jungle's green - its lightness,
//!   the root as light as the ground under it and the tip about twice as
//!   light, and its chroma, the tips a fifth more vivid than the ground
//!   ([`VIVID`]). Read as drawn, not back from the layer's sRGB colour:
//!   the band's dark greens leave the gamut and come back clipped greyer.
//!   Measured against the catalogue's green grass tuft, whose colours every
//!   green world's grass took before: a Lush world's blades land on them.
//! * **Its make** comes from a table of the biome's ranges ([`BiomeGrass`]),
//!   sampled per seed: how much of it is dry, how tall, how wide, how
//!   clumpy, how thick and how far it sways, and its flowers - a share of
//!   the blades and up to three of the biome's flower colours (a meadow's
//!   daisies and buttercups, an alpine slope's gentians). Every range sits
//!   inside what an owner may set, so inside what was measured affordable
//!   (#1702): the largest blades, a jungle's, are about two and a half
//!   times the unset blade's area, the most a world may set three.
//! * **Its tufts and blossoms** take its colours ([`Tint`]): the ground
//!   cover's grass tufts its root, tip and dry colours and its dry share,
//!   as the World Editor's Match the tufts sets them, and each wildflower
//!   scatter's blossoms one of its flower colours, so the cards stand in
//!   the field as part of it. The shore's marram grass and the lilies keep
//!   their own.
//!
//! How thickly it grows on each ground layer stays Auto: it already follows
//! the biome's own ground textures and the tufts scattered on them.
//!
//! Its own random stream ([`GRASS_STREAM_SALT`]), so no sibling deriver's
//! draws move for it.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use super::GroundCoverSpecies;
use super::palette::nudge_hue;
use crate::pds::{Fp, Fp3, Generator, SovereignGrassConfig, SovereignTextureConfig};
use crate::seeded_defaults::oklch::{oklch_to_srgb, wrap_hue_deg};
use crate::seeded_defaults::room::RoomPalette;
use crate::seeded_defaults::scene::{BiomeArchetype, SceneCharacter, range_f32, unit_f32};

/// Sub-stream salt for the seeded grass, distinct from every sibling
/// deriver's.
const GRASS_STREAM_SALT: u64 = 0x6A55_6A55_6A55_6A55;

/// How much more vivid a blade's tip is than the ground of its grass
/// layer, in OkLCH chroma: a ground's colour is its soil's as much as its
/// blades'. A Lush band's chroma averages 0.093, so its tips average the
/// catalogue's green tuft's 0.11; a jungle's run greener, a glacier's
/// greyer, as their bands do.
const VIVID: f32 = 1.2;

/// The flowers' colours, linear RGB as the grass and the texture configs
/// hold them.
mod flower {
    pub const WHITE: [f32; 3] = [0.87, 0.87, 0.79];
    pub const CREAM: [f32; 3] = [0.90, 0.85, 0.70];
    pub const YELLOW: [f32; 3] = [0.87, 0.58, 0.02];
    pub const ORANGE: [f32; 3] = [0.85, 0.30, 0.02];
    pub const RED: [f32; 3] = [0.75, 0.04, 0.03];
    pub const PINK: [f32; 3] = [0.64, 0.16, 0.31];
    pub const PALE_PINK: [f32; 3] = [0.85, 0.45, 0.55];
    pub const MAGENTA: [f32; 3] = [0.55, 0.06, 0.35];
    pub const VIOLET: [f32; 3] = [0.16, 0.08, 0.46];
    pub const LAVENDER: [f32; 3] = [0.40, 0.25, 0.65];
    pub const BLUE: [f32; 3] = [0.05, 0.12, 0.55];
}

/// What a biome's grass is made of, as ranges a seed samples within.
#[derive(Clone, Copy, Debug)]
pub struct BiomeGrass {
    /// The share of the blades that are dry.
    pub dry: (f32, f32),
    /// A blade's height (m) before its own variation.
    pub height: (f32, f32),
    /// A blade's width at the root (m).
    pub width: (f32, f32),
    /// How much it grows in tussocks and clumps (1 as grass grows unset).
    pub clumpiness: (f32, f32),
    /// How thick it grows on all the ground it grows on.
    pub thickness: (f32, f32),
    /// How far the wind leans it (1 as grass leans unset).
    pub sway: (f32, f32),
    /// The share of the blades that are flowers.
    pub flower_share: (f32, f32),
    /// The colours its flowers come in; a seed flowers in up to three.
    pub flowers: &'static [[f32; 3]],
}

/// Biome `b`'s grass: the make of the place - a meadow's lush turf in
/// flower, a savanna's tall dry bunchgrass, an alpine slope's short windy
/// turf with its gentians, a jungle's broad vivid sheltered blades.
pub fn biome_grass(b: BiomeArchetype) -> BiomeGrass {
    use BiomeArchetype::*;
    use flower::*;
    let grass =
        |dry, height, width, clumpiness, thickness, sway, flower_share, flowers| BiomeGrass {
            dry,
            height,
            width,
            clumpiness,
            thickness,
            sway,
            flower_share,
            flowers,
        };
    match b {
        Lush => grass(
            (0.12, 0.28),
            (0.40, 0.50),
            (0.055, 0.065),
            (0.9, 1.1),
            (0.95, 1.0),
            (0.9, 1.1),
            (0.008, 0.020),
            &[WHITE, YELLOW, PINK, VIOLET],
        ),
        Meadow => grass(
            (0.10, 0.25),
            (0.45, 0.60),
            (0.055, 0.065),
            (0.9, 1.1),
            (1.0, 1.0),
            (1.0, 1.2),
            (0.015, 0.035),
            &[WHITE, YELLOW, PINK, VIOLET, RED],
        ),
        Alpine => grass(
            (0.20, 0.35),
            (0.18, 0.28),
            (0.040, 0.050),
            (1.2, 1.4),
            (0.75, 0.9),
            (1.2, 1.4),
            (0.012, 0.030),
            &[BLUE, WHITE, PINK, YELLOW],
        ),
        Arid => grass(
            (0.60, 0.80),
            (0.40, 0.55),
            (0.040, 0.050),
            (1.6, 1.9),
            (0.6, 0.8),
            (1.0, 1.2),
            (0.003, 0.010),
            &[YELLOW, ORANGE],
        ),
        Savanna => grass(
            (0.65, 0.85),
            (0.65, 0.90),
            (0.045, 0.055),
            (1.6, 2.0),
            (0.8, 0.9),
            (1.0, 1.2),
            (0.003, 0.010),
            &[YELLOW, ORANGE, WHITE],
        ),
        Badlands => grass(
            (0.60, 0.80),
            (0.30, 0.40),
            (0.040, 0.050),
            (1.6, 1.8),
            (0.45, 0.6),
            (1.1, 1.3),
            (0.002, 0.006),
            &[YELLOW, ORANGE],
        ),
        Volcanic => grass(
            (0.50, 0.70),
            (0.25, 0.35),
            (0.040, 0.050),
            (1.4, 1.6),
            (0.5, 0.7),
            (1.0, 1.2),
            (0.0, 0.005),
            &[MAGENTA],
        ),
        Coastal => grass(
            (0.30, 0.45),
            (0.45, 0.65),
            (0.045, 0.055),
            (1.2, 1.4),
            (0.75, 0.9),
            (1.3, 1.5),
            (0.005, 0.015),
            &[PALE_PINK, LAVENDER, YELLOW],
        ),
        Tundra => grass(
            (0.40, 0.55),
            (0.15, 0.25),
            (0.035, 0.045),
            (1.5, 1.8),
            (0.6, 0.8),
            (1.2, 1.4),
            (0.005, 0.015),
            &[WHITE, MAGENTA, CREAM],
        ),
        Jungle => grass(
            (0.05, 0.12),
            (0.55, 0.80),
            (0.070, 0.080),
            (0.7, 0.9),
            (1.0, 1.0),
            (0.6, 0.8),
            (0.002, 0.008),
            &[ORANGE, RED, MAGENTA],
        ),
        TemperateForest => grass(
            (0.15, 0.25),
            (0.35, 0.45),
            (0.055, 0.065),
            (0.9, 1.1),
            (0.85, 0.95),
            (0.7, 0.9),
            (0.005, 0.015),
            &[VIOLET, WHITE, YELLOW],
        ),
        Boreal => grass(
            (0.20, 0.30),
            (0.30, 0.40),
            (0.045, 0.055),
            (1.0, 1.2),
            (0.75, 0.9),
            (0.8, 1.0),
            (0.004, 0.012),
            &[MAGENTA, CREAM],
        ),
        Wetland => grass(
            (0.10, 0.20),
            (0.55, 0.75),
            (0.050, 0.060),
            (1.1, 1.3),
            (0.95, 1.0),
            (0.9, 1.1),
            (0.008, 0.020),
            &[YELLOW, MAGENTA, WHITE],
        ),
        // Lifeless, as its ground cover is (`super::groundcover`): a few
        // hardy grey stalks, and no flower.
        Glacial => grass(
            (0.30, 0.50),
            (0.12, 0.20),
            (0.035, 0.045),
            (1.3, 1.5),
            (0.2, 0.3),
            (1.2, 1.4),
            (0.0, 0.0),
            &[],
        ),
    }
}

/// A seeded world's grass: its terrain's grass group, every setting set.
#[derive(Clone, Debug, PartialEq)]
pub struct SeededGrass {
    pub config: SovereignGrassConfig,
}

impl SeededGrass {
    /// The grass of the world `scene` describes, its ground coloured by
    /// `palette`, from seed `room_seed`.
    pub fn from_scene(scene: &SceneCharacter, palette: &RoomPalette, room_seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ GRASS_STREAM_SALT);
        let make = biome_grass(scene.biome);
        let in_range = |rng: &mut ChaCha8Rng, (lo, hi): (f32, f32)| range_f32(rng, lo, hi);

        // The blades from the grass layer under them, as the palette drew
        // it in OkLCH: the tip lighter and more vivid, the root as light
        // and under half as vivid, the dry blades straw.
        let [l, c, h] = palette.grass_dry_lch;
        let colour = |l: f32, c: f32, h: f32| {
            Fp3(oklch_to_srgb([
                l.clamp(0.0, 1.0),
                c.max(0.0),
                wrap_hue_deg(h),
            ]))
        };
        let vivid = c * VIVID;
        let tip = colour(
            l + 0.26 + jitter(&mut rng, 0.03),
            vivid + jitter(&mut rng, 0.01),
            h - 4.0 + jitter(&mut rng, 4.0),
        );
        let base = colour(l, vivid * 0.45, h + 4.0);
        let dry = colour(
            l + 0.24 + jitter(&mut rng, 0.02),
            0.07 + vivid * 0.15,
            nudge_hue(h, 92.0, 0.85),
        );

        // A warm world's grass is drier, a cold one's greener.
        let warmth = scene.temperature.clamp(-1.0, 1.0) * 0.1;
        let dry_share = (in_range(&mut rng, make.dry) + warmth).clamp(0.0, 1.0);

        // Two or three of the biome's flower colours, where it has them.
        let mut colours: Vec<[f32; 3]> = make.flowers.to_vec();
        for i in (1..colours.len()).rev() {
            let j = (unit_f32(&mut rng) * (i + 1) as f32) as usize;
            colours.swap(i, j.min(i));
        }
        let keep = if unit_f32(&mut rng) < 0.5 { 2 } else { 3 };
        colours.truncate(keep);

        let config = SovereignGrassConfig {
            color_base: Some(base),
            color_tip: Some(tip),
            color_dry: Some(dry),
            dry_share: Some(Fp(dry_share)),
            flower_share: Some(Fp(in_range(&mut rng, make.flower_share))),
            flower_colors: Some(colours.into_iter().map(Fp3).collect()),
            layer_density: None,
            thickness: Some(Fp(in_range(&mut rng, make.thickness))),
            sway: Some(Fp(in_range(&mut rng, make.sway))),
            height: Some(Fp(in_range(&mut rng, make.height))),
            width: Some(Fp(in_range(&mut rng, make.width))),
            clumpiness: Some(Fp(in_range(&mut rng, make.clumpiness))),
        };
        Self { config }
    }

    /// A [`Tint`] of the ground cover to this grass, its first wildflower
    /// scatter to take the first flower colour.
    pub fn tint(&self) -> Tint<'_> {
        Tint {
            grass: &self.config,
            blossoms: 0,
        }
    }
}

/// Tints a seeded world's ground cover to its grass, scatter by scatter
/// ([`Tint::apply`]).
pub struct Tint<'a> {
    grass: &'a SovereignGrassConfig,
    /// How many wildflower scatters it has tinted: the next takes the next
    /// flower colour.
    blossoms: usize,
}

impl Tint<'_> {
    /// Tint ground cover generator `generator`, a scatter of `species`:
    ///
    /// * a grass tuft's cards, green or dry, and a wildflower's take the
    ///   grass's root, tip and dry colours and its dry share, as the World
    ///   Editor's Match the tufts sets them
    ///   ([`crate::grass::profile::match_tuft_to`]), so a seeded world
    ///   leaves it nothing of theirs to match;
    /// * a wildflower's blossom takes one of the grass's flower colours -
    ///   both of its crossed cards, as they are one blossom - and each
    ///   wildflower scatter the next colour round;
    /// * every other card keeps its own colours: the shore's blue-green
    ///   marram grass, a lily's white blossom, the ferns, reeds and mosses.
    pub fn apply(&mut self, species: GroundCoverSpecies, generator: &mut Generator) {
        use GroundCoverSpecies as S;
        if !matches!(species, S::GrassTuft | S::DryGrassTuft | S::Wildflower) {
            return;
        }
        let grass = self.grass;
        let flowers = grass.flower_colors.as_deref().unwrap_or_default();
        let blossom = (species == S::Wildflower && !flowers.is_empty())
            .then(|| flowers[self.blossoms % flowers.len()]);
        tint_tree(generator, &mut |texture| match texture {
            SovereignTextureConfig::GrassTuft(tuft) => {
                crate::grass::profile::match_tuft_to(tuft, grass);
            }
            SovereignTextureConfig::Flower(flower) => {
                if let Some(colour) = blossom {
                    flower.petal.color_base = colour;
                    flower.petal.color_edge = Fp3(colour.0.map(|v| v * 0.85));
                }
            }
            _ => {}
        });
        if blossom.is_some() {
            self.blossoms += 1;
        }
    }
}

/// Visit every texture `generator` and its children draw with.
fn tint_tree(generator: &mut Generator, tint: &mut impl FnMut(&mut SovereignTextureConfig)) {
    if let Some(material) = generator.kind.material_mut() {
        tint(&mut material.texture);
    }
    for child in &mut generator.children {
        tint_tree(child, tint);
    }
}

/// A signed draw within `span` either way, biased toward none.
fn jitter(rng: &mut ChaCha8Rng, span: f32) -> f32 {
    let u = range_f32(rng, -1.0, 1.0);
    u * u.abs() * span
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize::Sanitize;
    use crate::pds::{Fp64, SovereignGrassTuftConfig};
    use crate::seeded_defaults::oklch::srgb_to_oklch;

    /// Every texture `generator` and its children draw with.
    fn textures(generator: &Generator) -> Vec<&SovereignTextureConfig> {
        let mut found: Vec<_> = generator
            .kind
            .material()
            .map(|material| &material.texture)
            .into_iter()
            .collect();
        for child in &generator.children {
            found.extend(textures(child));
        }
        found
    }

    /// The petal colours of `generator`'s blossoms.
    fn petals(generator: &Generator) -> Vec<Fp3> {
        textures(generator)
            .into_iter()
            .filter_map(|texture| match texture {
                SovereignTextureConfig::Flower(flower) => Some(flower.petal.color_base),
                _ => None,
            })
            .collect()
    }

    /// Whether grass tuft `tuft` holds grass `grass`'s colours and dry
    /// share, as Match the tufts sets them.
    fn matched(tuft: &SovereignGrassTuftConfig, grass: &SovereignGrassConfig) -> bool {
        Some(tuft.color_base) == grass.color_base
            && Some(tuft.color_tip) == grass.color_tip
            && Some(tuft.color_dry) == grass.color_dry
            && tuft.dry_fraction == Fp64(f64::from(grass.dry_share.unwrap().0)).on_wire()
    }

    /// The catalogue's ground cover prop for `species`.
    fn catalogued(species: GroundCoverSpecies) -> Generator {
        crate::catalogue::by_slug(species.slug())
            .expect("a catalogued species")
            .build("did:plc:test")
    }

    /// Every biome, many seeds: the seeded grass sets every setting but the
    /// layers, stays inside what an owner may set (sanitising it changes
    /// nothing a peer would read), and varies with the seed.
    #[test]
    fn every_biome_s_grass_is_set_in_range_and_its_own_per_seed() {
        for seed in 0..60u64 {
            let mut scene = SceneCharacter::for_seed(seed);
            for biome in BiomeArchetype::ALL {
                scene.biome = biome;
                let palette = RoomPalette::from_scene(&scene, seed);
                let grass = SeededGrass::from_scene(&scene, &palette, seed).config;
                assert!(grass.layer_density.is_none(), "{biome:?}: layers stay Auto");
                for set in [
                    grass.color_base.is_some(),
                    grass.color_tip.is_some(),
                    grass.color_dry.is_some(),
                    grass.dry_share.is_some(),
                    grass.flower_share.is_some(),
                    grass.flower_colors.is_some(),
                    grass.thickness.is_some(),
                    grass.sway.is_some(),
                    grass.height.is_some(),
                    grass.width.is_some(),
                    grass.clumpiness.is_some(),
                ] {
                    assert!(set, "{biome:?} seed {seed}: {grass:?}");
                }
                let wire: SovereignGrassConfig =
                    serde_json::from_str(&serde_json::to_string(&grass).unwrap()).unwrap();
                let mut sanitised = wire.clone();
                sanitised.sanitize();
                assert_eq!(sanitised, wire, "{biome:?} seed {seed} left its range");
            }
        }
        let tip = |seed: u64| {
            let scene = SceneCharacter::for_seed(seed);
            let palette = RoomPalette::from_scene(&scene, seed);
            SeededGrass::from_scene(&scene, &palette, seed)
                .config
                .color_tip
        };
        assert_eq!(tip(7), tip(7), "a seed's grass is the same every time");
        assert_ne!(tip(7), tip(8));
    }

    /// The colours land where the grass's were before (the catalogue's
    /// green tuft's) for a Lush world, measured in the palette's OkLCH:
    /// root, tip and dry within a few hundredths of lightness and of
    /// chroma, as vivid as the tuft - and a world's blades follow its own
    /// grass layer's hue, an Arid world's tips straw, a Jungle's green.
    #[test]
    fn the_blades_follow_the_grass_layer_under_them() {
        let tuft = SovereignGrassTuftConfig::default();
        let lush: Vec<_> = (0..40u64)
            .map(|seed| {
                let mut scene = SceneCharacter::for_seed(seed);
                scene.biome = BiomeArchetype::Lush;
                scene.temperature = 0.0;
                let palette = RoomPalette::from_scene(&scene, seed);
                SeededGrass::from_scene(&scene, &palette, seed).config
            })
            .collect();
        let mean = |colours: Vec<Fp3>| {
            let lch: Vec<[f32; 3]> = colours.iter().map(|c| srgb_to_oklch(c.0)).collect();
            let n = lch.len() as f32;
            (
                lch.iter().map(|c| c[0]).sum::<f32>() / n,
                lch.iter().map(|c| c[1]).sum::<f32>() / n,
            )
        };
        for (name, was, now) in [
            (
                "root",
                tuft.color_base,
                mean(lush.iter().filter_map(|g| g.color_base).collect()),
            ),
            (
                "tip",
                tuft.color_tip,
                mean(lush.iter().filter_map(|g| g.color_tip).collect()),
            ),
            (
                "dry",
                tuft.color_dry,
                mean(lush.iter().filter_map(|g| g.color_dry).collect()),
            ),
        ] {
            let [l, c, _] = srgb_to_oklch(was.0);
            assert!(
                (now.0 - l).abs() < 0.06,
                "{name}: lightness {} against the tuft's {l}",
                now.0
            );
            assert!(
                (now.1 - c).abs() < 0.02,
                "{name}: chroma {} against the tuft's {c}",
                now.1
            );
        }
        let hue_of = |biome: BiomeArchetype| {
            let mut scene = SceneCharacter::for_seed(3);
            scene.biome = biome;
            scene.temperature = 0.0;
            let palette = RoomPalette::from_scene(&scene, 3);
            let tip = SeededGrass::from_scene(&scene, &palette, 3)
                .config
                .color_tip
                .unwrap();
            srgb_to_oklch(tip.0)[2]
        };
        assert!(
            hue_of(BiomeArchetype::Arid) < 115.0,
            "{}",
            hue_of(BiomeArchetype::Arid)
        );
        assert!(
            hue_of(BiomeArchetype::Jungle) > 125.0,
            "{}",
            hue_of(BiomeArchetype::Jungle)
        );
    }

    /// A tint matches a wildflower's tuft to the grass, dry share and all,
    /// crosses one colour on both its blossom cards and gives the next
    /// wildflower scatter the next colour; the shore grass and the lilies
    /// keep their own.
    #[test]
    fn a_tint_matches_the_tufts_and_gives_each_blossom_one_colour() {
        let scene = SceneCharacter::for_seed(11);
        let palette = RoomPalette::from_scene(&scene, 11);
        let seeded = SeededGrass {
            config: SovereignGrassConfig {
                flower_colors: Some(vec![Fp3([0.9, 0.1, 0.1]), Fp3([0.1, 0.1, 0.9])]),
                ..SeededGrass::from_scene(&scene, &palette, 11).config
            },
        };
        let grass = &seeded.config;
        let flowers = grass.flower_colors.clone().unwrap();
        let mut tint = seeded.tint();
        for nth in 0..3 {
            let mut wildflower = catalogued(GroundCoverSpecies::Wildflower);
            tint.apply(GroundCoverSpecies::Wildflower, &mut wildflower);
            for texture in textures(&wildflower) {
                if let SovereignTextureConfig::GrassTuft(tuft) = texture {
                    assert!(matched(tuft, grass), "{tuft:?}");
                }
            }
            let petals = petals(&wildflower);
            assert_eq!(petals.len(), 2, "a crossed blossom");
            assert!(petals.iter().all(|p| *p == flowers[nth % 2]), "{petals:?}");
        }
        for species in [GroundCoverSpecies::ShoreGrass, GroundCoverSpecies::LilyPad] {
            let mut card = catalogued(species);
            tint.apply(species, &mut card);
            assert_eq!(card, catalogued(species), "{species:?} keeps its own");
        }
    }

    /// A seeded world as the builder makes it (#1703): its terrain sets its
    /// grass; every grass and wildflower tuft it scatters holds the grass's
    /// colours and dry share, so Match the tufts finds only the shore's
    /// marram grass to change; each wildflower scatter's blossoms take one
    /// flower colour, successive scatters successive ones; and the lilies
    /// and the rest keep their own.
    #[test]
    fn a_seeded_world_s_cards_stand_in_its_grass() {
        use crate::seeded_defaults::room::GroundCoverScatters;
        use crate::seeded_defaults::room::build::build_room_with_source;
        use GroundCoverSpecies as S;
        let kinds = [S::Wildflower, S::ShoreGrass, S::LilyPad, S::DryGrassTuft];
        let mut seen: Vec<GroundCoverSpecies> = Vec::new();
        for seed in 0..400u64 {
            let scene = SceneCharacter::for_seed(seed);
            let species: Vec<_> = GroundCoverScatters::from_scene(&scene, seed)
                .scatters
                .iter()
                .map(|scatter| scatter.species)
                .collect();
            if !species
                .iter()
                .any(|s| kinds.contains(s) && !seen.contains(s))
            {
                continue;
            }
            seen.extend(species.iter().filter(|s| kinds.contains(s)));
            let did = format!("did:seed:{seed}");
            let record = build_room_with_source(seed, &did, None);
            let palette = RoomPalette::from_scene(&scene, seed);
            // As the world is handed on, read back from the wire (#1704).
            let grass =
                crate::pds::read_back(&SeededGrass::from_scene(&scene, &palette, seed).config)
                    .expect("a grass writes");
            let terrain = crate::pds::find_terrain_config(&record).expect("a terrain");
            assert_eq!(terrain.grass, grass, "seed {seed}: the terrain sets it");
            let flowers = grass.flower_colors.clone().unwrap_or_default();
            let (mut wildflowers, mut marram) = (0, 0);
            for (idx, species) in species.iter().enumerate() {
                let generator = &record.generators[&format!("ground_cover_{idx}")];
                match species {
                    S::GrassTuft | S::DryGrassTuft | S::Wildflower => {
                        for texture in textures(generator) {
                            if let SovereignTextureConfig::GrassTuft(tuft) = texture {
                                assert!(matched(tuft, &grass), "seed {seed} {species:?}");
                            }
                        }
                        if *species == S::Wildflower && !flowers.is_empty() {
                            let want = flowers[wildflowers % flowers.len()];
                            assert!(
                                petals(generator).iter().all(|p| *p == want),
                                "seed {seed}: wildflower scatter {wildflowers}"
                            );
                            wildflowers += 1;
                        }
                    }
                    other => {
                        // Its own, as the wire holds it (#1704).
                        let own =
                            crate::pds::read_back(&catalogued(*other)).expect("a card writes");
                        assert_eq!(generator, &own, "seed {seed}: {other:?} keeps its own");
                        marram += textures(&own)
                            .iter()
                            .filter(|t| matches!(t, SovereignTextureConfig::GrassTuft(_)))
                            .count();
                    }
                }
            }
            let tufts = crate::grass::profile::tufts(&record);
            assert_eq!(tufts.unmatched, marram, "seed {seed}: {tufts:?}");
        }
        for kind in kinds {
            assert!(seen.contains(&kind), "no seed scatters {kind:?}");
        }
    }
}
