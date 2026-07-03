//! Sanitiser for [`SovereignTerrainConfig`]. Clamps grid size,
//! cell/height scales, voronoi/erosion budgets, and walks the per-layer
//! material configs through their own [`Sanitize`] impl.

use super::Sanitize;
use super::common::clamp_finite;
use super::limits;
use crate::pds::terrain::SovereignTerrainConfig;
use crate::pds::types::Fp;

impl Sanitize for SovereignTerrainConfig {
    fn sanitize(&mut self) {
        self.grid_size = self.grid_size.clamp(2, limits::MAX_GRID_SIZE);
        // Every `f32` coefficient below feeds the heightmap noise / erosion
        // math, whose output lands in `build_heightfield_collider` — an
        // `assert!(is_finite)` that panics the physics step on a single NaN or
        // infinity. Two subtleties make a plain range clamp insufficient:
        // `HeightMap::normalize` does NOT scrub non-finite values (its min/max
        // fold ignores NaN, so a poisoned cell is rescaled to NaN rather than
        // dropped), and `f32::clamp` *propagates* NaN (`NaN.clamp(lo, hi)` is
        // NaN). `clamp_finite` replaces any non-finite value with the field
        // default *before* clamping, closing the path a hostile record uses to
        // crash a peer that loads or receives it. Ranges mirror the terrain
        // editor's sliders — magnitude multipliers keep the usual forward-compat
        // headroom, while the frequency-exponent fields (`lacunarity`,
        // `base_frequency`) get none, since values past the editor range only
        // alias the noise lattice (see `limits::MAX_LACUNARITY`).
        self.cell_scale = Fp(clamp_finite(
            self.cell_scale.0,
            limits::MIN_CELL_SCALE,
            limits::MAX_CELL_SCALE,
            2.0,
        ));
        self.height_scale = Fp(clamp_finite(
            self.height_scale.0,
            limits::MIN_HEIGHT_SCALE,
            limits::MAX_HEIGHT_SCALE,
            50.0,
        ));
        self.octaves = self.octaves.clamp(1, limits::MAX_OCTAVES);

        // Noise / erosion coefficients. `unit` bounds the fields whose semantics
        // are a `[0, 1]` fraction; the scale-like fields carry documented
        // `limits` ceilings. Every hydraulic term is a multiplier / blend factor
        // (never a divisor — see `HydraulicErosion::erode`), so a `0.0` floor is
        // safe. Defaults match `SovereignTerrainConfig::default`.
        let unit = |v: f32, default: f32| clamp_finite(v, 0.0, 1.0, default);
        self.persistence = Fp(unit(self.persistence.0, 0.5));
        self.lacunarity = Fp(clamp_finite(
            self.lacunarity.0,
            1.0,
            limits::MAX_LACUNARITY,
            2.0,
        ));
        self.base_frequency = Fp(clamp_finite(
            self.base_frequency.0,
            0.0,
            limits::MAX_BASE_FREQUENCY,
            4.0,
        ));
        self.ds_roughness = Fp(unit(self.ds_roughness.0, 0.5));
        self.inertia = Fp(unit(self.inertia.0, 0.05));
        self.erosion_rate = Fp(unit(self.erosion_rate.0, 0.3));
        self.deposition_rate = Fp(unit(self.deposition_rate.0, 0.3));
        self.evaporation_rate = Fp(unit(self.evaporation_rate.0, 0.02));
        self.capacity_factor = Fp(clamp_finite(
            self.capacity_factor.0,
            0.0,
            limits::MAX_CAPACITY_FACTOR,
            8.0,
        ));
        self.thermal_talus_angle = Fp(unit(self.thermal_talus_angle.0, 0.05));

        self.voronoi_num_seeds = self.voronoi_num_seeds.clamp(1, limits::MAX_VORONOI_SEEDS);
        self.voronoi_num_terraces = self
            .voronoi_num_terraces
            .clamp(1, limits::MAX_VORONOI_TERRACES);
        self.erosion_drops = self.erosion_drops.min(limits::MAX_EROSION_DROPS);
        self.thermal_iterations = self.thermal_iterations.min(limits::MAX_THERMAL_ITERATIONS);
        self.material.texture_size = self
            .material
            .texture_size
            .clamp(16, limits::MAX_TEXTURE_SIZE);
        // Cap per-variant octave-like fields so a forward-compat peer cannot
        // weaponise texture-size × octave blowups.
        for layer in self.material.layers.iter_mut() {
            layer.sanitize();
        }
    }
}
