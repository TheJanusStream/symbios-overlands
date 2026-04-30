//! Sanitiser for [`SovereignTerrainConfig`]. Clamps grid size,
//! cell/height scales, voronoi/erosion budgets, and walks the per-layer
//! material configs through their own [`Sanitize`] impl.

use super::Sanitize;
use super::limits;
use crate::pds::terrain::SovereignTerrainConfig;
use crate::pds::types::Fp;

impl Sanitize for SovereignTerrainConfig {
    fn sanitize(&mut self) {
        self.grid_size = self.grid_size.clamp(2, limits::MAX_GRID_SIZE);
        // `cell_scale` and `height_scale` feed straight into the heightmap
        // mesh/collider builders. A NaN or infinity smuggled in via a malicious
        // record propagates to `avian3d` collider construction and panics the
        // physics step, so clamp both to finite positive ranges.
        self.cell_scale = Fp(self
            .cell_scale
            .0
            .clamp(limits::MIN_CELL_SCALE, limits::MAX_CELL_SCALE));
        self.height_scale = Fp(self
            .height_scale
            .0
            .clamp(limits::MIN_HEIGHT_SCALE, limits::MAX_HEIGHT_SCALE));
        self.octaves = self.octaves.clamp(1, limits::MAX_OCTAVES);
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
