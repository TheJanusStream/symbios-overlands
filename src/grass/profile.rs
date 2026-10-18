//! What a world's grass looks like (#1675): what its owner sets in its
//! terrain's grass group (#1700, `pds::grass`), and for every setting left
//! unset - Auto - what its record already stores, as every world's grass was
//! read before the group existed ([`derived_profile`]). A world that sets
//! nothing, the ones made before the grass came included, gets its grass
//! from what it has:
//!
//! * **Its colours** are the room's own ground-cover grass: the grass tuft
//!   texture its ground-cover scatters place most of (a meadow's green
//!   tufts, a savanna's sun-bleached ones), base, tip and dry colour, and
//!   the share of its tufts that are dry. A room with no grass tufts takes
//!   the catalogue's default tuft.
//! * **Its flowers** are its scatters' flower textures' petal colours: a
//!   flower card stands among a dozen blades or so, so a small share of
//!   the blades as the flower cards are of the room's grass cards, and
//!   never more than one blade in fifty.
//! * **Where it grows** is each splat layer's ground: green soil grows the
//!   most, brown soil a little, moss and forest litter some, sand and
//!   gravel a few stalks, rock, snow, ice, lava and paving none. Where a
//!   room's scatters grow grass tufts, grass grows on the layers their
//!   biome allow-lists name (an open list: the soil pair, where seeded
//!   cover grows) as thickly as the room grows tufts, whatever the layer's
//!   texture - a savanna's sand layer is grass country. The first layer
//!   takes the full thickness, the others half, so a dirt band under the
//!   same tufts still reads as worn ground.
//! * **It sways** as far as the wind leans any grass ([`cfg::WIND_LEAN`]),
//!   is [`cfg::BLADE_WIDTH`] wide at the root and clumps as grass does
//!   unset (#1702), and the ground under it hides
//!   [`cfg::CARPET_DEPTH`].
//!
//! A world with no flower cards holds [`cfg::DEFAULT_FLOWERS`] at a share
//! of none, so its owner can set a flower share alone. And the owner can
//! recolour its tufts to the grass they set ([`match_tufts`]).
//!
//! The texture configs hold linear colours (the texture crate encodes them
//! to sRGB when it bakes), which is what the grass's vertex colours are.

use crate::config::grass as cfg;
use crate::pds::{
    Fp64, Generator, Placement, RoomRecord, SovereignFlowerConfig, SovereignGrassConfig,
    SovereignGrassTuftConfig, SovereignTextureConfig,
};

/// A room's grass: its palette, its flowers and where it grows.
#[derive(Clone, Debug, PartialEq)]
pub struct GrassProfile {
    /// A blade's colour at its root, linear RGB.
    pub base: [f32; 3],
    /// Its colour at its tip, linear RGB.
    pub tip: [f32; 3],
    /// A dry blade's colour, linear RGB.
    pub dry: [f32; 3],
    /// The share of the blades that are dry.
    pub dry_share: f32,
    /// Flower colours, linear RGB, at most four.
    pub flowers: Vec<[f32; 3]>,
    /// The share of the blades that are flowers.
    pub flower_share: f32,
    /// How thickly grass grows on each of the four splat layers, 0 to 1.
    pub layer_density: [f32; 4],
    /// Each splat layer's colour as it reads from a distance, linear RGB:
    /// what the field's far blades turn to, so its edge is not a line.
    pub ground: [[f32; 3]; 4],
    /// A blade's height (m) before its own variation.
    pub height: f32,
    /// How far the wind leans it, as a share of [`cfg::WIND_LEAN`].
    pub sway: f32,
    /// A blade's width at its root (m), before the quality widens a thinner
    /// field's (`config::grass::Field::blade_width`).
    pub width: f32,
    /// How much it grows in tussocks and clumps: 1 as grass grows unset.
    pub clumpiness: f32,
    /// How much of the ground it hides seen 45 degrees from straight above,
    /// as `1 - exp(-carpet_depth)` (`ground_common.wgsl`'s `carpet_cover`):
    /// [`cfg::CARPET_DEPTH`] for grass of the height its dry share makes it
    /// and [`cfg::BLADE_WIDTH`] wide - every world's grass on Auto - and as
    /// much more as its blades are taller or wider than that. Dry grass
    /// stands taller but hides no more for it: its blades are as much
    /// thinner, so a drier field of one height hides less of the ground.
    pub carpet_depth: f32,
}

impl GrassProfile {
    /// This grass with world settings `own` over it (#1700): each one set
    /// in place of what the world's ground cover gives, and each unset one
    /// left as it is.
    pub fn with_settings(mut self, own: &SovereignGrassConfig) -> Self {
        if let Some(colour) = own.color_base {
            self.base = colour.0;
        }
        if let Some(colour) = own.color_tip {
            self.tip = colour.0;
        }
        if let Some(colour) = own.color_dry {
            self.dry = colour.0;
        }
        if let Some(share) = own.dry_share {
            self.dry_share = share.0.clamp(0.0, 1.0);
            self.height = height_for(self.dry_share);
        }
        if let Some(colours) = &own.flower_colors {
            self.flowers = colours
                .iter()
                .take(crate::pds::sanitize::limits::MAX_GRASS_FLOWER_COLORS)
                .map(|colour| colour.0)
                .collect();
        }
        if let Some(share) = own.flower_share {
            self.flower_share = share.0.clamp(0.0, 1.0);
        }
        if let Some(layers) = own.layer_density {
            self.layer_density = layers.map(|density| density.0.clamp(0.0, 1.0));
        }
        if let Some(thickness) = own.thickness {
            let thickness = thickness.0.clamp(0.0, 1.0);
            for density in &mut self.layer_density {
                *density *= thickness;
            }
        }
        if let Some(sway) = own.sway {
            self.sway = sway
                .0
                .clamp(0.0, crate::pds::sanitize::limits::MAX_GRASS_SWAY);
        }
        use crate::pds::sanitize::limits;
        if let Some(height) = own.height {
            self.height = height
                .0
                .clamp(limits::MIN_GRASS_HEIGHT, limits::MAX_GRASS_HEIGHT);
        }
        if let Some(width) = own.width {
            self.width = width
                .0
                .clamp(limits::MIN_GRASS_WIDTH, limits::MAX_GRASS_WIDTH);
        }
        if let Some(clumpiness) = own.clumpiness {
            self.clumpiness = clumpiness.0.clamp(0.0, limits::MAX_GRASS_CLUMPINESS);
        }
        // The ground hides as much more as the blades on it are taller or
        // wider than the grass would grow unset - none at all on Auto.
        self.carpet_depth = cfg::CARPET_DEPTH
            * (self.height / height_for(self.dry_share))
            * (self.width / cfg::BLADE_WIDTH);
        self
    }
}

/// What one ground-cover texture is worth: its config, and how many
/// instances the room's scatters place of it.
struct Weighed<'a, T> {
    config: &'a T,
    count: f32,
}

/// The grass of world `record`: what it grows with its terrain's grass
/// settings over it.
pub fn grass_profile(record: &RoomRecord) -> GrassProfile {
    let derived = derived_profile(record);
    match crate::pds::find_terrain_config(record) {
        Some(terrain) => derived.with_settings(&terrain.grass),
        None => derived,
    }
}

/// The grass world `record` grows with every grass setting Auto: what its
/// record stores of ground cover and ground, read as the module header
/// says. What an Auto row of the World Editor shows, and where a setting
/// starts from once its owner sets it.
pub fn derived_profile(record: &RoomRecord) -> GrassProfile {
    let mut tufts: Vec<Weighed<SovereignGrassTuftConfig>> = Vec::new();
    let mut flowers: Vec<Weighed<SovereignFlowerConfig>> = Vec::new();
    // Tufts per splat layer, by the scatters' biome allow-lists.
    let mut tufts_on = [0.0f32; 4];
    for placement in &record.placements {
        let Placement::Scatter {
            generator_ref,
            count,
            biome_filter,
            ..
        } = placement
        else {
            continue;
        };
        let Some(generator) = record.generators.get(generator_ref) else {
            continue;
        };
        let count = *count as f32;
        // Each texture once a scatter: a crossed card draws its texture on
        // two or three planes, and is one tuft.
        let mut drawn: Vec<&SovereignTextureConfig> = Vec::new();
        visit(generator, &mut |texture| {
            if !drawn.contains(&texture) {
                drawn.push(texture);
            }
        });
        let mut has_tufts = false;
        for texture in drawn {
            match texture {
                SovereignTextureConfig::GrassTuft(config) => {
                    has_tufts = true;
                    tufts.push(Weighed { config, count });
                }
                SovereignTextureConfig::Flower(config) => flowers.push(Weighed { config, count }),
                _ => {}
            }
        }
        if has_tufts {
            let layers: &[u8] = if biome_filter.biomes.is_empty() {
                &[0, 1]
            } else {
                &biome_filter.biomes
            };
            for &layer in layers {
                if let Some(on) = tufts_on.get_mut(layer as usize) {
                    *on += count;
                }
            }
        }
    }

    let tuft_count: f32 = tufts.iter().map(|t| t.count).sum();
    let flower_count: f32 = flowers.iter().map(|f| f.count).sum();
    let default_tuft = SovereignGrassTuftConfig::default();
    let dominant = tufts
        .iter()
        .max_by(|a, b| a.count.total_cmp(&b.count))
        .map_or(&default_tuft, |t| t.config);
    let dry_share = if tuft_count > 0.0 {
        tufts
            .iter()
            .map(|t| t.count * t.config.dry_fraction.0 as f32)
            .sum::<f32>()
            / tuft_count
    } else {
        default_tuft.dry_fraction.0 as f32
    };

    let mut ordered: Vec<&Weighed<SovereignFlowerConfig>> = flowers.iter().collect();
    ordered.sort_by(|a, b| b.count.total_cmp(&a.count));
    let mut flower_colours: Vec<[f32; 3]> = Vec::new();
    for flower in ordered {
        let colour = flower.config.petal.color_base.0;
        if flower_colours.len() < 4 && !flower_colours.contains(&colour) {
            flower_colours.push(colour);
        }
    }
    let flower_share = if flower_colours.is_empty() {
        // No flower cards: none flower, in the colours a share would show.
        flower_colours = cfg::DEFAULT_FLOWERS.to_vec();
        0.0
    } else {
        (cfg::FLOWERS_PER_CARD * flower_count / (tuft_count + flower_count))
            .min(cfg::MAX_FLOWER_SHARE)
    };

    let layers = crate::pds::find_terrain_config(record).map(|t| &t.material.layers);
    let mut layer_density =
        layers.map_or([1.0, 0.2, 0.0, 0.0], |l| l.each_ref().map(ground_density));
    // A layer whose colour cannot be read (an uploaded image) takes the
    // blades' own, so its far blades keep theirs rather than turning grey.
    let blade = [0, 1, 2].map(|i| (dominant.color_base.0[i] + dominant.color_tip.0[i]) * 0.5);
    let ground = layers.map_or([blade; 4], |l| {
        l.each_ref()
            .map(|texture| ground_colour(texture).unwrap_or(blade))
    });
    for (layer, density) in layer_density.iter_mut().enumerate() {
        let share = if layer == 0 { 1.0 } else { 0.5 };
        let lush = (tufts_on[layer] / cfg::LUSH_TUFTS).min(1.0) * share;
        *density = density.max(lush);
    }

    GrassProfile {
        base: dominant.color_base.0,
        tip: dominant.color_tip.0,
        dry: dominant.color_dry.0,
        dry_share: dry_share.clamp(0.0, 1.0),
        flowers: flower_colours,
        flower_share,
        layer_density,
        ground,
        height: height_for(dry_share.clamp(0.0, 1.0)),
        sway: 1.0,
        width: cfg::BLADE_WIDTH,
        clumpiness: 1.0,
        carpet_depth: cfg::CARPET_DEPTH,
    }
}

/// A blade's height (m) in grass `dry_share` of whose blades are dry: dry
/// grass stands taller.
fn height_for(dry_share: f32) -> f32 {
    cfg::BLADE_HEIGHT * (1.0 + 0.4 * dry_share)
}

/// The height (m) a world's blades stand at with its Height on Auto
/// (#1702), its ground cover giving grass `derived` and its own settings
/// `own`: as tall as its dry share makes them - the world's own Dry blades
/// where set, its tufts' otherwise - as [`GrassProfile::with_settings`]
/// grows them. What the World Editor's Height row shows on Auto, and starts
/// from once unticked.
pub fn unset_height(derived: &GrassProfile, own: &SovereignGrassConfig) -> f32 {
    height_for(
        own.dry_share
            .map_or(derived.dry_share, |share| share.0.clamp(0.0, 1.0)),
    )
}

/// What a grass tuft holds once it is matched to grass `grass` (#1700): its
/// root, tip and dry colours, and its dry share as the record holds a
/// tuft's - on the wire's grid, so a tuft matched before a save is still
/// matched after it.
struct TuftMatch {
    base: [f32; 3],
    tip: [f32; 3],
    dry: [f32; 3],
    dry_fraction: Fp64,
}

impl TuftMatch {
    fn of(grass: &GrassProfile) -> Self {
        Self {
            base: grass.base,
            tip: grass.tip,
            dry: grass.dry,
            dry_fraction: Fp64(f64::from(grass.dry_share)).on_wire(),
        }
    }

    fn holds(&self, tuft: &SovereignGrassTuftConfig) -> bool {
        tuft.color_base.0 == self.base
            && tuft.color_tip.0 == self.tip
            && tuft.color_dry.0 == self.dry
            && tuft.dry_fraction == self.dry_fraction
    }

    /// Give tuft `tuft` this match; whether it changed.
    fn apply(&self, tuft: &mut SovereignGrassTuftConfig) -> bool {
        if self.holds(tuft) {
            return false;
        }
        tuft.color_base.0 = self.base;
        tuft.color_tip.0 = self.tip;
        tuft.color_dry.0 = self.dry;
        tuft.dry_fraction = self.dry_fraction;
        true
    }
}

/// Match grass tuft `tuft` to grass settings `own` as [`match_tufts`]
/// matches a world's tufts to its grass, where `own` sets all a match
/// holds - the root, tip and dry colours and the dry share - so [`tufts`]
/// counts it matched: a seeded world's tint (#1703). Left as it is where
/// `own` leaves any of them Auto.
pub fn match_tuft_to(tuft: &mut SovereignGrassTuftConfig, own: &SovereignGrassConfig) {
    let (Some(base), Some(tip), Some(dry), Some(share)) =
        (own.color_base, own.color_tip, own.color_dry, own.dry_share)
    else {
        return;
    };
    // As `GrassProfile::with_settings` takes them, so the match is the one
    // `TuftMatch::of` makes of the world's grass.
    let grass = TuftMatch {
        base: base.0,
        tip: tip.0,
        dry: dry.0,
        dry_fraction: Fp64(f64::from(share.0.clamp(0.0, 1.0))).on_wire(),
    };
    grass.apply(tuft);
}

/// The scatter generators of world `record`: what its tufts are found in.
fn scattered(record: &RoomRecord) -> Vec<String> {
    record
        .placements
        .iter()
        .filter_map(|placement| match placement {
            Placement::Scatter { generator_ref, .. } => Some(generator_ref.clone()),
            _ => None,
        })
        .collect()
}

/// Recolour the grass tufts world `record`'s grass follows (#1700, the
/// World Editor's Match the tufts): every grass-tuft texture its scatters
/// place takes the grass's root, tip and dry colours and its dry share, so
/// the tufts stand among the blades as part of the field. How many tuft
/// textures changed (see [`Tufts`]); a second match changes none.
pub fn match_tufts(record: &mut RoomRecord) -> usize {
    let target = TuftMatch::of(&grass_profile(record));
    let mut changed = 0;
    for name in scattered(record) {
        let Some(generator) = record.generators.get_mut(&name) else {
            continue;
        };
        visit_mut(generator, &mut |texture| {
            if let SovereignTextureConfig::GrassTuft(tuft) = texture
                && target.apply(tuft)
            {
                changed += 1;
            }
        });
    }
    changed
}

/// The grass tufts a world's scatters place, counted by texture - each
/// textured part of a card, and a generator once for each scatter of it -
/// as [`match_tufts`] counts what it changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tufts {
    /// How many there are.
    pub scattered: usize,
    /// How many of them do not yet hold the world's grass colours: what
    /// [`match_tufts`] would change.
    pub unmatched: usize,
}

/// The grass tufts world `record`'s scatters place (see [`Tufts`]).
pub fn tufts(record: &RoomRecord) -> Tufts {
    let target = TuftMatch::of(&grass_profile(record));
    let mut tufts = Tufts::default();
    for name in scattered(record) {
        if let Some(generator) = record.generators.get(&name) {
            visit(generator, &mut |texture| {
                if let SovereignTextureConfig::GrassTuft(tuft) = texture {
                    tufts.scattered += 1;
                    if !target.holds(tuft) {
                        tufts.unmatched += 1;
                    }
                }
            });
        }
    }
    tufts
}

/// Visit every texture `generator` and its children draw with.
fn visit<'a>(generator: &'a Generator, found: &mut impl FnMut(&'a SovereignTextureConfig)) {
    if let Some(material) = generator.kind.material() {
        found(&material.texture);
    }
    for child in &generator.children {
        visit(child, found);
    }
}

/// [`visit`], to change what it finds.
fn visit_mut(generator: &mut Generator, found: &mut impl FnMut(&mut SovereignTextureConfig)) {
    if let Some(material) = generator.kind.material_mut() {
        found(&mut material.texture);
    }
    for child in &mut generator.children {
        visit_mut(child, found);
    }
}

/// How thickly grass grows on ground textured `texture`, 0 to 1.
fn ground_density(texture: &SovereignTextureConfig) -> f32 {
    use SovereignTextureConfig as T;
    match texture {
        T::Ground(g) => soil_density(g.color_dry.0, g.color_moist.0),
        T::Moss(_) => 0.5,
        T::ForestFloor(_) => 0.4,
        T::Gravel(_) => 0.12,
        T::Sand(_) => 0.08,
        T::Lichen(_) | T::CrackedEarth(_) => 0.05,
        _ => 0.0,
    }
}

/// The colour ground textured `texture` reads as from a distance, linear
/// RGB: its config's main colours, in about the share it paints them;
/// `None` for a texture with no colours to read (an uploaded image).
fn ground_colour(texture: &SovereignTextureConfig) -> Option<[f32; 3]> {
    use SovereignTextureConfig as T;
    let mix = |parts: &[([f32; 3], f32)]| -> [f32; 3] {
        let total: f32 = parts.iter().map(|p| p.1).sum();
        [0, 1, 2].map(|i| parts.iter().map(|(c, w)| c[i] * w).sum::<f32>() / total)
    };
    Some(match texture {
        T::Ground(g) => mix(&[(g.color_dry.0, 1.0), (g.color_moist.0, 1.0)]),
        T::Moss(m) => mix(&[(m.color_deep.0, 1.0), (m.color_tip.0, 1.0)]),
        T::ForestFloor(f) => mix(&[
            (f.color_humus.0, 2.0),
            (f.color_leaf.0, 1.0),
            (f.color_leaf_old.0, 1.0),
        ]),
        T::Sand(s) => mix(&[(s.color_crest.0, 1.0), (s.color_trough.0, 1.0)]),
        T::Gravel(g) => mix(&[
            (g.color_stone.0, 1.0),
            (g.color_dark.0, 1.0),
            (g.color_fines.0, 1.0),
        ]),
        T::Lichen(l) => mix(&[
            (l.color_rock.0, 3.0),
            (l.color_lichen_a.0, 1.0),
            (l.color_lichen_b.0, 1.0),
        ]),
        T::CrackedEarth(c) => mix(&[(c.color_plate.0, 6.0), (c.color_crack.0, 1.0)]),
        T::Rock(r) => mix(&[(r.color_light.0, 1.0), (r.color_dark.0, 1.0)]),
        T::Snow(s) => s.color_snow.0,
        T::Ice(i) => i.color_ice.0,
        T::Lava(l) => l.color_crust.0,
        _ => return None,
    })
}

/// How thickly grass grows on soil of these two colours (linear RGB): green
/// soil the most, brown soil a little, white soil - snow painted as soil -
/// none.
fn soil_density(dry: [f32; 3], moist: [f32; 3]) -> f32 {
    let c = [0, 1, 2].map(|i| (dry[i] + moist[i]) * 0.5);
    let luminance = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    if luminance > 0.5 {
        return 0.0;
    }
    let green = ((c[1] - c[0].max(c[2])) / c[1].max(1.0e-3)).clamp(0.0, 1.0);
    let t = (green / 0.35).clamp(0.0, 1.0);
    0.2 + 0.8 * t * t * (3.0 - 2.0 * t)
}

/// The default world with every grass setting Auto, so what it grows is
/// what its ground cover gives - the seeded worlds set theirs (#1703).
#[cfg(test)]
pub(crate) fn auto_world() -> RoomRecord {
    let mut record = RoomRecord::default();
    for generator in record.generators.values_mut() {
        if let crate::pds::GeneratorKind::Terrain(terrain) = &mut generator.kind {
            terrain.grass = SovereignGrassConfig::default();
        }
    }
    record
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{Fp3, Fp64, GeneratorKind};

    /// A grass tuft scatter of `count` with these colours and dry share.
    fn tuft_scatter(
        record: &mut RoomRecord,
        name: &str,
        count: u32,
        tuft: SovereignGrassTuftConfig,
    ) {
        let mut kind = GeneratorKind::default_cuboid();
        if let Some(material) = kind.material_mut() {
            material.texture = SovereignTextureConfig::GrassTuft(tuft);
        }
        let mut generator = Generator::from_kind(GeneratorKind::default_cuboid());
        generator.children.push(Generator::from_kind(kind));
        record.generators.insert(name.into(), generator);
        record.placements.push(Placement::Scatter {
            generator_ref: name.into(),
            bounds: Default::default(),
            count,
            local_seed: 0,
            biome_filter: crate::pds::BiomeFilter {
                biomes: vec![0, 1],
                water: Default::default(),
            },
            snap_to_terrain: true,
            random_yaw: true,
            avoid_urban: false,
            float_on_water: false,
            naturalness: Default::default(),
        });
    }

    /// A room on the default splat layers - green soil, brown dirt, rock,
    /// snow painted as soil - grows its fullest on the soil, a little on
    /// the dirt, none on the rock or the snow.
    #[test]
    fn green_soil_grows_grass_and_snow_none() {
        let mut record = auto_world();
        record.placements.clear();
        for generator in record.generators.values_mut() {
            if let GeneratorKind::Terrain(terrain) = &mut generator.kind {
                terrain.material = Default::default();
            }
        }
        let profile = grass_profile(&record);
        assert!(
            profile.layer_density[0] > 0.9,
            "{:?}",
            profile.layer_density
        );
        assert!(
            profile.layer_density[1] < 0.5,
            "{:?}",
            profile.layer_density
        );
        assert_eq!(profile.layer_density[2], 0.0);
        assert_eq!(profile.layer_density[3], 0.0);
        // No flower cards: none flower, though the default colours wait for
        // a share (#1700).
        assert_eq!(profile.flower_share, 0.0);
        assert_eq!(profile.flowers, cfg::DEFAULT_FLOWERS.to_vec());
    }

    /// The colours are the most-placed tuft's, the dry share the tufts'
    /// weighted mean, and a room of tufts grows grass on its soil layers
    /// whatever their texture.
    #[test]
    fn the_room_s_own_tufts_colour_its_grass() {
        let mut record = auto_world();
        record.placements.clear();
        let green = SovereignGrassTuftConfig {
            color_tip: Fp3([0.2, 0.6, 0.1]),
            dry_fraction: Fp64(0.0),
            ..Default::default()
        };
        let dry = SovereignGrassTuftConfig {
            color_tip: Fp3([0.6, 0.5, 0.2]),
            dry_fraction: Fp64(1.0),
            ..Default::default()
        };
        tuft_scatter(&mut record, "gc_green", 900, green);
        tuft_scatter(&mut record, "gc_dry", 300, dry);
        let profile = grass_profile(&record);
        assert_eq!(profile.tip, [0.2, 0.6, 0.1]);
        assert!(
            (profile.dry_share - 0.25).abs() < 1e-6,
            "{}",
            profile.dry_share
        );
        assert!(profile.layer_density[0] >= 0.8 - 1e-6);
        assert!(profile.layer_density[1] >= 0.4 - 1e-6);
    }

    /// The terrain's grass group of world `record`.
    fn settings(record: &mut RoomRecord) -> &mut SovereignGrassConfig {
        record
            .generators
            .values_mut()
            .find_map(|generator| match &mut generator.kind {
                GeneratorKind::Terrain(terrain) => Some(&mut terrain.grass),
                _ => None,
            })
            .expect("the default world has a terrain")
    }

    /// A world that sets nothing grows the grass its ground cover gives
    /// (#1700): the group is a lever only once its owner pulls it.
    #[test]
    fn a_world_that_sets_nothing_grows_what_its_ground_cover_gives() {
        let record = auto_world();
        assert_eq!(grass_profile(&record), derived_profile(&record));
        assert_eq!(grass_profile(&record).sway, 1.0);
    }

    /// Each setting a world sets takes the place of what its ground cover
    /// gives, and the rest stay as they were: a dry share raises the blades
    /// with it, and the thickness thins every layer it grows on.
    #[test]
    fn each_set_setting_takes_the_place_of_the_derived_one() {
        use crate::pds::Fp;
        let mut record = auto_world();
        let derived = derived_profile(&record);
        *settings(&mut record) = SovereignGrassConfig {
            color_tip: Some(Fp3([0.6, 0.45, 0.1])),
            dry_share: Some(Fp(1.0)),
            flower_share: Some(Fp(0.3)),
            flower_colors: Some(vec![Fp3([0.3, 0.1, 0.6])]),
            layer_density: Some([Fp(1.0), Fp(0.8), Fp(0.4), Fp(0.0)]),
            thickness: Some(Fp(0.5)),
            sway: Some(Fp(1.5)),
            ..SovereignGrassConfig::default()
        };
        let grass = grass_profile(&record);
        assert_eq!(grass.tip, [0.6, 0.45, 0.1]);
        assert_eq!(
            grass.base, derived.base,
            "an unset colour follows the tufts"
        );
        assert_eq!(grass.dry, derived.dry);
        assert_eq!(grass.dry_share, 1.0);
        assert_eq!(grass.height, height_for(1.0));
        assert!(grass.height > derived.height, "dry grass stands taller");
        assert_eq!(grass.flowers, vec![[0.3, 0.1, 0.6]]);
        assert_eq!(grass.flower_share, 0.3);
        assert_eq!(grass.layer_density, [0.5, 0.4, 0.2, 0.0]);
        assert_eq!(grass.sway, 1.5);
        assert_eq!(grass.ground, derived.ground, "the ground is the ground's");
    }

    /// The grass's shape (#1702): a height, a width and a clumpiness set
    /// take the place of what the world gives, and the ground hides as much
    /// more as the blades are taller and wider than the grass would grow
    /// unset - on Auto exactly [`cfg::CARPET_DEPTH`].
    #[test]
    fn the_shape_settings_reach_the_grass_and_the_ground() {
        use crate::pds::Fp;
        let mut record = auto_world();
        let derived = derived_profile(&record);
        assert_eq!(grass_profile(&record).carpet_depth, cfg::CARPET_DEPTH);
        assert_eq!(derived.width, cfg::BLADE_WIDTH);
        assert_eq!(derived.clumpiness, 1.0);
        *settings(&mut record) = SovereignGrassConfig {
            height: Some(Fp(0.8)),
            width: Some(Fp(0.07)),
            clumpiness: Some(Fp(1.5)),
            ..SovereignGrassConfig::default()
        };
        let grass = grass_profile(&record);
        assert_eq!(grass.height, 0.8);
        assert_eq!(grass.width, 0.07);
        assert_eq!(grass.clumpiness, 1.5);
        let deeper = (0.8 / height_for(derived.dry_share)) * (0.07 / cfg::BLADE_WIDTH);
        assert!((grass.carpet_depth - cfg::CARPET_DEPTH * deeper).abs() < 1e-5);
    }

    /// A flower share set alone flowers in the default colours of a world
    /// with no flower cards, and a thickness alone thins what the ground
    /// grows; a thickness of none grows no grass.
    #[test]
    fn a_share_or_a_thickness_set_alone_works_on_what_is_derived() {
        use crate::pds::Fp;
        let mut record = auto_world();
        record.placements.clear();
        let derived = derived_profile(&record);
        *settings(&mut record) = SovereignGrassConfig {
            flower_share: Some(Fp(0.5)),
            thickness: Some(Fp(0.0)),
            ..SovereignGrassConfig::default()
        };
        let grass = grass_profile(&record);
        assert_eq!(grass.flowers, cfg::DEFAULT_FLOWERS.to_vec());
        assert_eq!(grass.flower_share, 0.5);
        assert_eq!(grass.layer_density, [0.0; 4], "{:?}", derived.layer_density);
    }

    /// Match the tufts recolours every tuft the world scatters to the grass
    /// it grows - set colours and derived ones alike - so afterwards the
    /// tufts and the grass agree with every colour Auto; a second match
    /// changes nothing.
    #[test]
    fn matching_the_tufts_gives_them_the_grass_colours() {
        let mut record = auto_world();
        record.placements.clear();
        let green = SovereignGrassTuftConfig {
            color_tip: Fp3([0.2, 0.6, 0.1]),
            dry_fraction: Fp64(0.0),
            ..Default::default()
        };
        let dry = SovereignGrassTuftConfig {
            color_tip: Fp3([0.6, 0.5, 0.2]),
            dry_fraction: Fp64(1.0),
            ..Default::default()
        };
        tuft_scatter(&mut record, "gc_green", 900, green);
        tuft_scatter(&mut record, "gc_dry", 300, dry);
        assert_eq!(
            tufts(&record),
            Tufts {
                scattered: 2,
                unmatched: 2
            }
        );
        settings(&mut record).color_tip = Some(Fp3([0.7, 0.3, 0.6]));
        let grass = grass_profile(&record);

        assert_eq!(match_tufts(&mut record), 2);
        let derived = derived_profile(&record);
        assert_eq!(derived.tip, [0.7, 0.3, 0.6]);
        assert_eq!(derived.base, grass.base);
        assert_eq!(derived.dry, grass.dry);
        assert!((derived.dry_share - grass.dry_share).abs() < 1e-6);
        assert_eq!(match_tufts(&mut record), 0);
        assert_eq!(tufts(&record).unmatched, 0);
    }

    /// A match outlives a save: the tufts as a peer reads them back - every
    /// value on the wire's grid - still hold the grass, whose dry share an
    /// f32 holds a hair off the grid, so a second match changes nothing.
    #[test]
    fn a_match_survives_the_wire() {
        use crate::pds::Fp;
        let mut record = auto_world();
        record.placements.clear();
        tuft_scatter(
            &mut record,
            "gc_tufts",
            600,
            SovereignGrassTuftConfig::default(),
        );
        settings(&mut record).dry_share = Some(Fp(0.3));
        settings(&mut record).color_tip = Some(Fp3([0.61, 0.47, 0.13]));
        assert!(match_tufts(&mut record) > 0);

        let wire = serde_json::to_string(&record).expect("the world writes");
        let mut read: RoomRecord = serde_json::from_str(&wire).expect("it reads back");
        assert_eq!(tufts(&read).unmatched, 0, "the tufts still match");
        assert_eq!(match_tufts(&mut read), 0);
    }

    /// Soil colours: green grows, brown a little, white not at all.
    #[test]
    fn soil_colour_decides_how_much_grows() {
        assert!(soil_density([0.07, 0.12, 0.03], [0.03, 0.07, 0.01]) > 0.95);
        let brown = soil_density([0.30, 0.22, 0.15], [0.20, 0.14, 0.09]);
        assert!((brown - 0.2).abs() < 1e-6, "{brown}");
        assert_eq!(soil_density([0.95, 0.95, 0.98], [0.80, 0.82, 0.88]), 0.0);
    }
}
