//! Sanitiser for [`SovereignMaterialSettings`] and the embedded
//! [`SovereignTextureConfig`] open union. Color channels go to `[0,1]`,
//! roughness/metallic to `[0,1]`, and emission strength is capped here; the
//! texture config's own fields are clamped into the envelope the upstream
//! per-field registry gives them, so a hostile record cannot tell the
//! texture pipeline to iterate billions of times per pixel.

use super::Sanitize;
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
        // #957 uv_transform knobs: offset shares the Sign envelope (the
        // sampler wraps regardless, the bound is sanity), rotation keeps its
        // authored sign across a full turn either way.
        for c in self.uv_offset.0.iter_mut() {
            *c = if c.is_finite() {
                c.clamp(-1_000.0, 1_000.0)
            } else {
                0.0
            };
        }
        self.uv_rotation = Fp(if self.uv_rotation.0.is_finite() {
            self.uv_rotation.0.clamp(-360.0, 360.0)
        } else {
            0.0
        });
        self.texture.sanitize();
    }
}

/// Every procedural texture field is clamped into the envelope the upstream
/// per-field registry gives it (#1304).
///
/// The envelope replaces a hand-written clamp list that covered sixty-two
/// fields of the several hundred, and was the *loosest* of the three tables
/// bounding those fields on twenty of them: the tuned ranges lived upstream
/// in the genetic operators and the inspector sliders, while the one table a
/// hostile record actually met carried round numbers picked to bound a loop.
/// Twenty-eight integer fields — `warp_octaves` among them — had no clamp
/// here at all, and nothing but `noise`'s own internal ceiling stood between
/// a hostile record and the pixel loop.
///
/// The match is exhaustive, so a new variant cannot be added without
/// deciding what happens to it, and every arm is the same decision.
impl Sanitize for SovereignTextureConfig {
    fn sanitize(&mut self) {
        match self {
            // Nothing to bound: `None` carries no config, and `Unknown` is
            // the forward-compatibility arm whose payload this build cannot
            // interpret — and cannot re-serialise either.
            Self::None | Self::Unknown => {}
            // Not a generator config at all. Forwards to the asset-reference
            // sanitiser, which caps URL / DID / CID lengths so a hostile peer
            // cannot smuggle a megabyte URL through a texture slot.
            Self::Referenced { source } => source.sanitize(),
            Self::Leaf(c) => c.clamp_to_envelope(),
            Self::Twig(c) => c.clamp_to_envelope(),
            Self::Bark(c) => c.clamp_to_envelope(),
            Self::Window(c) => c.clamp_to_envelope(),
            Self::StainedGlass(c) => c.clamp_to_envelope(),
            Self::IronGrille(c) => c.clamp_to_envelope(),
            Self::Ground(c) => c.clamp_to_envelope(),
            Self::Rock(c) => c.clamp_to_envelope(),
            Self::Brick(c) => c.clamp_to_envelope(),
            Self::Plank(c) => c.clamp_to_envelope(),
            Self::Shingle(c) => c.clamp_to_envelope(),
            Self::Stucco(c) => c.clamp_to_envelope(),
            Self::Concrete(c) => c.clamp_to_envelope(),
            Self::Metal(c) => c.clamp_to_envelope(),
            Self::Pavers(c) => c.clamp_to_envelope(),
            Self::Ashlar(c) => c.clamp_to_envelope(),
            Self::Cobblestone(c) => c.clamp_to_envelope(),
            Self::Thatch(c) => c.clamp_to_envelope(),
            Self::Marble(c) => c.clamp_to_envelope(),
            Self::Corrugated(c) => c.clamp_to_envelope(),
            Self::Asphalt(c) => c.clamp_to_envelope(),
            Self::Wainscoting(c) => c.clamp_to_envelope(),
            Self::Encaustic(c) => c.clamp_to_envelope(),
            Self::SoftDisc(c) => c.clamp_to_envelope(),
            Self::Spark(c) => c.clamp_to_envelope(),
            Self::Snowflake(c) => c.clamp_to_envelope(),
            Self::Puff(c) => c.clamp_to_envelope(),
            Self::Ring(c) => c.clamp_to_envelope(),
            Self::Petal(c) => c.clamp_to_envelope(),
            Self::Shard(c) => c.clamp_to_envelope(),
            Self::LeafSprite(c) => c.clamp_to_envelope(),
            Self::Flame(c) => c.clamp_to_envelope(),
            Self::Flower(c) => c.clamp_to_envelope(),
            Self::GrassTuft(c) => c.clamp_to_envelope(),
            Self::Frond(c) => c.clamp_to_envelope(),
            Self::Reed(c) => c.clamp_to_envelope(),
            Self::Needle(c) => c.clamp_to_envelope(),
            Self::Broadleaf(c) => c.clamp_to_envelope(),
            Self::Moss(c) => c.clamp_to_envelope(),
            Self::Lichen(c) => c.clamp_to_envelope(),
            Self::Fabric(c) => c.clamp_to_envelope(),
            Self::Sand(c) => c.clamp_to_envelope(),
            Self::Snow(c) => c.clamp_to_envelope(),
            Self::Ice(c) => c.clamp_to_envelope(),
            Self::Lava(c) => c.clamp_to_envelope(),
            Self::CactusSkin(c) => c.clamp_to_envelope(),
            Self::CrackedEarth(c) => c.clamp_to_envelope(),
            Self::Gravel(c) => c.clamp_to_envelope(),
            Self::ForestFloor(c) => c.clamp_to_envelope(),
            Self::Enamel(c) => c.clamp_to_envelope(),
            Self::Obsidian(c) => c.clamp_to_envelope(),
            Self::Chitin(c) => c.clamp_to_envelope(),
            Self::SolarPanel(c) => c.clamp_to_envelope(),
            Self::Parquet(c) => c.clamp_to_envelope(),
            Self::Truchet(c) => c.clamp_to_envelope(),
            Self::ChainLink(c) => c.clamp_to_envelope(),
            Self::LogEnd(c) => c.clamp_to_envelope(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::types::Fp2;
    use crate::pds::{SovereignFlowerConfig, SovereignSnowflakeConfig};

    /// #957: hostile uv_transform knobs must come back finite and bounded —
    /// NaN would otherwise ride straight into the material's `Affine2`.
    #[test]
    fn hostile_uv_transform_knobs_are_clamped() {
        let mut m = SovereignMaterialSettings {
            uv_offset: Fp2([f32::NAN, 5_000.0]),
            uv_rotation: Fp(f32::INFINITY),
            ..Default::default()
        };
        m.sanitize();
        assert_eq!(m.uv_offset.0[0], 0.0, "NaN offset must reset");
        assert_eq!(m.uv_offset.0[1], 1_000.0, "oversized offset must clamp");
        assert_eq!(m.uv_rotation.0, 0.0, "non-finite rotation must reset");

        let mut m = SovereignMaterialSettings {
            uv_rotation: Fp(-4_000.0),
            ..Default::default()
        };
        m.sanitize();
        assert_eq!(m.uv_rotation.0, -360.0, "rotation keeps its sign");
    }

    /// A hostile record can set count-shaped sprite fields to `u32::MAX`;
    /// the envelope must bring them back inside the per-feature loop budget
    /// so the texture task cannot be told to iterate billions of times per
    /// pixel. The exhaustive version of this — every numeric field of every
    /// variant — is `tests/texture_wire.rs`; these two are here because the
    /// atlas dimensions were the original reason this sanitiser existed.
    #[test]
    fn hostile_sprite_counts_are_clamped() {
        let snow = SovereignSnowflakeConfig {
            variant_rows: u32::MAX,
            variant_cols: u32::MAX,
            arms: u32::MAX,
            branch_pairs: u32::MAX,
            ..Default::default()
        };
        let mut cfg = SovereignTextureConfig::Snowflake(snow);
        cfg.sanitize();
        let SovereignTextureConfig::Snowflake(s) = cfg else {
            panic!("variant changed under sanitize");
        };
        assert!(s.variant_rows <= 16, "atlas rows: {}", s.variant_rows);
        assert!(s.variant_cols <= 16, "atlas cols: {}", s.variant_cols);
        assert!(s.arms <= 8, "arms: {}", s.arms);
        assert!(s.branch_pairs <= 5, "branch pairs: {}", s.branch_pairs);

        let flower = SovereignFlowerConfig {
            petal_count: u32::MAX,
            variant_rows: 0, // below the floor
            ..Default::default()
        };
        let mut cfg = SovereignTextureConfig::Flower(flower);
        cfg.sanitize();
        let SovereignTextureConfig::Flower(f) = cfg else {
            panic!("variant changed under sanitize");
        };
        assert!(f.petal_count <= 12, "petals: {}", f.petal_count);
        assert!(f.variant_rows >= 1, "atlas dim floored to at least 1");
    }
}
