//! Sanitiser for [`SovereignMaterialSettings`] and the embedded
//! [`SovereignTextureConfig`] open union. Color channels go to `[0,1]`,
//! roughness/metallic to `[0,1]`, emission strength is capped, and each
//! procedural-texture variant has its octave / cell / grid loop counts
//! clamped so a hostile record can't tell the texture pipeline to
//! iterate billions of times per pixel.

use super::Sanitize;
use super::limits;
use crate::pds::texture::{SovereignMaterialSettings, SovereignTextureConfig};
use crate::pds::types::{Fp, Fp3};

impl Sanitize for SovereignMaterialSettings {
    fn sanitize(&mut self) {
        let clamp_unit = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let clamp3 = |c: Fp3| Fp3([clamp_unit(c.0[0]), clamp_unit(c.0[1]), clamp_unit(c.0[2])]);
        self.base_color = clamp3(self.base_color);
        self.emission_color = clamp3(self.emission_color);
        self.emission_strength = Fp(if self.emission_strength.0.is_finite() {
            self.emission_strength.0.clamp(0.0, 1_000.0)
        } else {
            0.0
        });
        self.roughness = Fp(clamp_unit(self.roughness.0));
        self.metallic = Fp(clamp_unit(self.metallic.0));
        self.uv_scale = Fp(if self.uv_scale.0.is_finite() {
            self.uv_scale.0.clamp(0.001, 1_000.0)
        } else {
            1.0
        });
        self.texture.sanitize();
    }
}

impl Sanitize for SovereignTextureConfig {
    fn sanitize(&mut self) {
        let axis = limits::MAX_TEXTURE_GRID_AXIS;
        match self {
            SovereignTextureConfig::Ground(g) => {
                g.macro_octaves = g.macro_octaves.clamp(1, limits::MAX_GROUND_OCTAVES);
                g.micro_octaves = g.micro_octaves.clamp(1, limits::MAX_GROUND_OCTAVES);
            }
            SovereignTextureConfig::Rock(r) => {
                r.octaves = r.octaves.clamp(1, limits::MAX_ROCK_OCTAVES);
            }
            SovereignTextureConfig::Bark(b) => {
                b.octaves = b.octaves.clamp(1, limits::MAX_ROCK_OCTAVES);
            }
            SovereignTextureConfig::Stucco(s) => {
                s.octaves = s.octaves.clamp(1, limits::MAX_ROCK_OCTAVES);
            }
            SovereignTextureConfig::Concrete(c) => {
                c.octaves = c.octaves.clamp(1, limits::MAX_ROCK_OCTAVES);
            }
            SovereignTextureConfig::Marble(m) => {
                m.octaves = m.octaves.clamp(1, limits::MAX_ROCK_OCTAVES);
            }
            SovereignTextureConfig::Snow(s) => {
                s.drift_octaves = s.drift_octaves.clamp(1, limits::MAX_ROCK_OCTAVES);
            }
            // Variants with explicit cell / grid loop counts — without these
            // a peer can ship a `cell_count: 4_000_000_000` (or
            // `bars_x: u32::MAX`) and pin every guest's procedural texture
            // task on a per-pixel inner loop billions of iterations long.
            SovereignTextureConfig::Twig(t) => {
                t.leaf_pairs = t.leaf_pairs.clamp(1, limits::MAX_TEXTURE_LEAF_PAIRS);
            }
            SovereignTextureConfig::Window(w) => {
                w.panes_x = w.panes_x.clamp(1, axis);
                w.panes_y = w.panes_y.clamp(1, axis);
            }
            SovereignTextureConfig::StainedGlass(s) => {
                s.cell_count = s.cell_count.clamp(1, limits::MAX_TEXTURE_VORONOI_CELLS);
            }
            SovereignTextureConfig::IronGrille(i) => {
                i.bars_x = i.bars_x.clamp(1, axis);
                i.bars_y = i.bars_y.clamp(1, axis);
            }
            SovereignTextureConfig::Ashlar(a) => {
                a.rows = a.rows.clamp(1, axis);
                a.cols = a.cols.clamp(1, axis);
            }
            SovereignTextureConfig::Wainscoting(w) => {
                w.panels_x = w.panels_x.clamp(1, axis);
                w.panels_y = w.panels_y.clamp(1, axis);
            }
            // Particle sprite cards. Each atlas dimension drives a cell
            // count (rows × cols cell constructions) and each count-shaped
            // field drives a per-pixel inner loop; clamp both at the record
            // boundary so a hostile record can't depend on the upstream
            // generator's internal clamps still being present.
            SovereignTextureConfig::SoftDisc(s) => {
                clamp_atlas(&mut s.variant_rows, &mut s.variant_cols);
            }
            SovereignTextureConfig::Spark(s) => {
                clamp_atlas(&mut s.variant_rows, &mut s.variant_cols);
                s.points = s.points.clamp(2, limits::MAX_SPRITE_SPARK_POINTS);
            }
            SovereignTextureConfig::Snowflake(s) => {
                clamp_atlas(&mut s.variant_rows, &mut s.variant_cols);
                s.arms = s.arms.clamp(3, limits::MAX_SPRITE_SNOWFLAKE_ARMS);
                s.branch_pairs = s
                    .branch_pairs
                    .min(limits::MAX_SPRITE_SNOWFLAKE_BRANCH_PAIRS);
            }
            SovereignTextureConfig::Puff(s) => {
                clamp_atlas(&mut s.variant_rows, &mut s.variant_cols);
                s.octaves = s.octaves.clamp(1, limits::MAX_SPRITE_PUFF_OCTAVES);
            }
            SovereignTextureConfig::Ring(s) => {
                clamp_atlas(&mut s.variant_rows, &mut s.variant_cols);
                s.wave_count = s.wave_count.clamp(2, limits::MAX_SPRITE_RING_WAVES);
            }
            SovereignTextureConfig::Shard(s) => {
                clamp_atlas(&mut s.variant_rows, &mut s.variant_cols);
                s.sides = s.sides.clamp(3, limits::MAX_SPRITE_SHARD_SIDES);
            }
            SovereignTextureConfig::Petal(s) => {
                clamp_atlas(&mut s.variant_rows, &mut s.variant_cols);
            }
            SovereignTextureConfig::Flame(s) => {
                clamp_atlas(&mut s.variant_rows, &mut s.variant_cols);
            }
            SovereignTextureConfig::LeafSprite(s) => {
                clamp_atlas(&mut s.variant_rows, &mut s.variant_cols);
            }
            SovereignTextureConfig::Flower(s) => {
                clamp_atlas(&mut s.variant_rows, &mut s.variant_cols);
                s.petal_count = s.petal_count.clamp(4, limits::MAX_SPRITE_FLOWER_PETALS);
            }
            // Forward to the asset-reference sanitiser — caps URL / DID /
            // CID lengths so a hostile peer can't smuggle a megabyte URL
            // through a referenced texture slot.
            SovereignTextureConfig::Referenced { source } => source.sanitize(),
            // Variants whose only count-shaped fields are `fp64` scale
            // factors (Brick, Plank, Shingle, Metal, Pavers, Cobblestone,
            // Thatch, Corrugated, Asphalt, Encaustic, Leaf; and the Fabric /
            // Sand / Ice / Lava surfaces, whose thread / ripple / crack
            // counts are likewise `fp64` frequencies): per-pixel cost is
            // bounded by `MAX_TEXTURE_SIZE`, so no extra clamp is needed.
            SovereignTextureConfig::None
            | SovereignTextureConfig::Leaf(_)
            | SovereignTextureConfig::Brick(_)
            | SovereignTextureConfig::Plank(_)
            | SovereignTextureConfig::Shingle(_)
            | SovereignTextureConfig::Metal(_)
            | SovereignTextureConfig::Pavers(_)
            | SovereignTextureConfig::Cobblestone(_)
            | SovereignTextureConfig::Thatch(_)
            | SovereignTextureConfig::Corrugated(_)
            | SovereignTextureConfig::Asphalt(_)
            | SovereignTextureConfig::Encaustic(_)
            | SovereignTextureConfig::Fabric(_)
            | SovereignTextureConfig::Sand(_)
            | SovereignTextureConfig::Ice(_)
            | SovereignTextureConfig::Lava(_)
            | SovereignTextureConfig::ChainLink(_)
            | SovereignTextureConfig::LogEnd(_)
            | SovereignTextureConfig::Unknown => {}
        }
    }
}

/// Clamp a sprite atlas's `(rows, cols)` into `1..=MAX_PARTICLE_ATLAS_DIM`.
///
/// The upstream `generate_atlas` clamps these before allocating cells, but
/// clamping at the record boundary keeps the cost bound independent of the
/// installed `bevy_symbios_texture` version.
fn clamp_atlas(rows: &mut u32, cols: &mut u32) {
    *rows = (*rows).clamp(1, limits::MAX_PARTICLE_ATLAS_DIM);
    *cols = (*cols).clamp(1, limits::MAX_PARTICLE_ATLAS_DIM);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{SovereignFlowerConfig, SovereignSnowflakeConfig};

    /// A hostile record can set count-shaped sprite fields to `u32::MAX`; the
    /// sanitiser must bring them back inside the per-feature loop budget so the
    /// texture task can't be told to iterate billions of times per pixel.
    #[test]
    fn hostile_sprite_counts_are_clamped() {
        let mut snow = SovereignSnowflakeConfig::default();
        snow.variant_rows = u32::MAX;
        snow.variant_cols = u32::MAX;
        snow.arms = u32::MAX;
        snow.branch_pairs = u32::MAX;
        let mut cfg = SovereignTextureConfig::Snowflake(snow);
        cfg.sanitize();
        let SovereignTextureConfig::Snowflake(s) = cfg else {
            panic!("variant changed under sanitize");
        };
        assert!(s.variant_rows <= limits::MAX_PARTICLE_ATLAS_DIM);
        assert!(s.variant_cols <= limits::MAX_PARTICLE_ATLAS_DIM);
        assert!(s.arms <= limits::MAX_SPRITE_SNOWFLAKE_ARMS);
        assert!(s.branch_pairs <= limits::MAX_SPRITE_SNOWFLAKE_BRANCH_PAIRS);

        let mut flower = SovereignFlowerConfig::default();
        flower.petal_count = u32::MAX;
        flower.variant_rows = 0; // below the floor
        let mut cfg = SovereignTextureConfig::Flower(flower);
        cfg.sanitize();
        let SovereignTextureConfig::Flower(f) = cfg else {
            panic!("variant changed under sanitize");
        };
        assert!(f.petal_count <= limits::MAX_SPRITE_FLOWER_PETALS);
        assert!(f.variant_rows >= 1, "atlas dim floored to at least 1");
    }
}
