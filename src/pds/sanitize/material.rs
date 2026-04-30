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
            // Variants whose only count-shaped fields are `fp64` scale
            // factors (Brick, Plank, Shingle, Metal, Pavers, Cobblestone,
            // Thatch, Corrugated, Asphalt, Encaustic, Leaf): per-pixel cost
            // is bounded by `MAX_TEXTURE_SIZE`, so no extra clamp is needed.
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
            | SovereignTextureConfig::Unknown => {}
        }
    }
}
