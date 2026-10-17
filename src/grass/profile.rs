//! What a room's grass looks like (#1675), read from what its record
//! already stores - there is no grass field to read, and every room, the
//! ones made before the grass came included, gets its grass from what it
//! has.
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
//!
//! The texture configs hold linear colours (the texture crate encodes them
//! to sRGB when it bakes), which is what the grass's vertex colours are.

use crate::config::grass as cfg;
use crate::pds::{
    Generator, Placement, RoomRecord, SovereignFlowerConfig, SovereignGrassTuftConfig,
    SovereignTextureConfig,
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
}

/// What one ground-cover texture is worth: its config, and how many
/// instances the room's scatters place of it.
struct Weighed<'a, T> {
    config: &'a T,
    count: f32,
}

/// The grass of room `record`.
pub fn grass_profile(record: &RoomRecord) -> GrassProfile {
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
        height: cfg::BLADE_HEIGHT * (1.0 + 0.4 * dry_share.clamp(0.0, 1.0)),
    }
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
        let mut record = RoomRecord::default();
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
        assert!(profile.flowers.is_empty());
    }

    /// The colours are the most-placed tuft's, the dry share the tufts'
    /// weighted mean, and a room of tufts grows grass on its soil layers
    /// whatever their texture.
    #[test]
    fn the_room_s_own_tufts_colour_its_grass() {
        let mut record = RoomRecord::default();
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

    /// Soil colours: green grows, brown a little, white not at all.
    #[test]
    fn soil_colour_decides_how_much_grows() {
        assert!(soil_density([0.07, 0.12, 0.03], [0.03, 0.07, 0.01]) > 0.95);
        let brown = soil_density([0.30, 0.22, 0.15], [0.20, 0.14, 0.09]);
        assert!((brown - 0.2).abs() < 1e-6, "{brown}");
        assert_eq!(soil_density([0.95, 0.95, 0.98], [0.80, 0.82, 0.88]), 0.0);
    }
}
