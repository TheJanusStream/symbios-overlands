//! Shared, platform-agnostic generation jobs for the compute-offload layer.
//!
//! A [`GenJob`] is a self-contained, serialisable description of a CPU-heavy
//! generation task; [`GenJob::run`] executes it **purely** (no Bevy, no I/O,
//! no globals — deterministic from the job's seed alone) and returns
//! serialisable [`GenResult`] data. The same `run()` is invoked by the app's
//! native `AsyncComputeTaskPool` backend and inside the wasm Web Worker, so
//! native and worker execution are byte-identical (the determinism invariant
//! the terrain pipeline already relies on).
//!
//! This crate deliberately depends only on the **Bevy-free** `symbios-*` cores
//! (`symbios-ground` / `symbios-texture` / `symbios-audio`, which the
//! `bevy_symbios_*` crates merely re-export + wrap), never the engine, so the
//! worker `.wasm` that links it stays slim instead of pulling Bevy.

use serde::{Deserialize, Serialize};
use symbios_audio::{bake, bake_sequence, samples_to_wav_bytes_pcm16, AudioPatch, SequenceRecipe};
use symbios_ground::{
    DiamondSquare, FbmNoise, HeightMap, HydraulicErosion, TerrainGenerator, ThermalErosion,
    VoronoiTerracing,
};
use symbios_texture::generator::{TextureGenerator, TextureMap};

/// Re-export of the texture core's registry macro so the app can generate its
/// own per-generator tables (e.g. the `TextureConfig` → [`TextureBakeJob`]
/// mapper) in lock-step with [`TextureBakeJob`] itself, without taking a
/// direct `symbios-texture` dependency.
pub use symbios_texture::for_each_generator;

/// One table drives both the [`GeneratorKind`] enum and the per-kind base
/// dispatch inside [`GenJob::run`]'s heightmap path (#657) — the texture
/// path in this file is already table-generated (`define_texture_bake!`),
/// and this closes the same hand-sync gap on the terrain side: adding a
/// generator is one entry here (plus the app's `SovereignGeneratorKind`
/// mirror), and the enum and its dispatch can no longer drift apart.
macro_rules! define_heightmap_generators {
    ($( $variant:ident => |$p:ident, $hm:ident| $body:block ),* $(,)?) => {
        /// Base terrain algorithm. Mirrors the app's `SovereignGeneratorKind`
        /// (kept independent so this crate stays free of the app and Bevy).
        #[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
        pub enum GeneratorKind {
            $( $variant, )*
        }

        /// Apply the selected base generator — generated from the same
        /// table as the enum, so the two stay in lock-step.
        fn apply_base_generator(params: &HeightmapParams, heightmap: &mut HeightMap) {
            match params.generator_kind {
                $( GeneratorKind::$variant => {
                    let $p = params;
                    let $hm = &mut *heightmap;
                    $body
                } )*
            }
        }
    };
}

define_heightmap_generators! {
    FbmNoise => |p, hm| {
        FbmNoise {
            seed: p.seed,
            octaves: p.octaves.clamp(1, 32),
            persistence: p.persistence,
            lacunarity: p.lacunarity,
            base_frequency: p.base_frequency,
        }
        .generate(hm);
        hm.normalize();
    },
    DiamondSquare => |p, hm| {
        DiamondSquare::new(p.seed, p.ds_roughness).generate(hm);
        hm.normalize();
    },
    VoronoiTerracing => |p, hm| {
        VoronoiTerracing::new(
            p.seed,
            p.voronoi_num_seeds.max(1) as usize,
            p.voronoi_num_terraces.max(1) as usize,
        )
        .generate(hm);
        // Voronoi already emits bounded [0, 1] output.
    },
}

/// Plain, serialisable inputs for a heightmap generation job — the distilled
/// generation-relevant subset of the app's terrain config (no material/splat
/// fields, which are a separate generation concern).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HeightmapParams {
    pub grid_size: u32,
    pub cell_scale: f32,
    pub height_scale: f32,
    pub generator_kind: GeneratorKind,
    pub seed: u64,
    pub octaves: u32,
    pub persistence: f32,
    pub lacunarity: f32,
    pub base_frequency: f32,
    pub ds_roughness: f32,
    pub voronoi_num_seeds: u32,
    pub voronoi_num_terraces: u32,
    pub erosion_enabled: bool,
    pub erosion_drops: u32,
    pub inertia: f32,
    pub erosion_rate: f32,
    pub deposition_rate: f32,
    pub evaporation_rate: f32,
    pub capacity_factor: f32,
    pub thermal_enabled: bool,
    pub thermal_iterations: u32,
    pub thermal_talus_angle: f32,
}

/// Generated heightmap data — plain row-major `f32` heights plus the dimensions
/// needed to rebuild a `symbios_ground::HeightMap` on the consuming side. On
/// wasm the `data` floats are serialized element-wise across the worker
/// boundary (a per-element copy — unlike the RGBA / WAV buffers, they are
/// not sent as a compact `serde_bytes` bin blob).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HeightmapData {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    /// Sent as a compact little-endian byte blob (msgpack `bin`) rather than an
    /// element-wise float array (#641) — matching how `TextureData`'s RGBA and
    /// `GenResult::Audio`'s WAV already cross the worker boundary. One bulk copy
    /// per side instead of ~262k tagged `serialize_f32`/`deserialize_f32` visitor
    /// calls at the default 512² grid, ~20% smaller wire. Both wasm and native
    /// are little-endian, and the raw IEEE-754 bytes round-trip `f32` bit-exactly,
    /// so the cross-peer determinism invariant is preserved.
    #[serde(with = "f32_blob")]
    pub data: Vec<f32>,
}

/// serde `with`-module: serialize a `Vec<f32>` as a contiguous little-endian
/// byte blob (via `serialize_bytes`, which msgpack encodes as a `bin` payload)
/// and reconstruct it. Reuses the already-present `serde_bytes` for byte
/// transport — no new dependency.
mod f32_blob {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[f32], s: S) -> Result<S::Ok, S::Error> {
        let mut bytes = Vec::with_capacity(v.len() * 4);
        for f in v {
            bytes.extend_from_slice(&f.to_le_bytes());
        }
        s.serialize_bytes(&bytes)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<f32>, D::Error> {
        let bytes = serde_bytes::ByteBuf::deserialize(d)?;
        if bytes.len() % 4 != 0 {
            return Err(serde::de::Error::custom(
                "heightmap byte length is not a multiple of 4",
            ));
        }
        Ok(bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Audio bake job (symbios-audio core)
// ---------------------------------------------------------------------------

/// A procedural audio bake — a patch one-shot or a multi-track sequence —
/// producing WAV bytes (mono 16-bit PCM — half the size of 32-bit float, which
/// matters on wasm where the heap never shrinks). The inputs are serialisable
/// so the job crosses the worker boundary; the heavy synthesis runs in [`run`].
#[derive(Serialize, Deserialize, Clone)]
pub enum AudioBakeJob {
    /// One-shot patch render of `duration_secs` at `sample_rate`.
    Patch {
        patch: AudioPatch,
        sample_rate: u32,
        duration_secs: f32,
    },
    /// Multi-track sequence render (its sample rate is carried in the recipe).
    Sequence { recipe: SequenceRecipe },
}

impl AudioBakeJob {
    fn run(self) -> Vec<u8> {
        match self {
            AudioBakeJob::Patch {
                patch,
                sample_rate,
                duration_secs,
            } => {
                let samples = bake(&patch, sample_rate, duration_secs);
                samples_to_wav_bytes_pcm16(&samples, sample_rate)
            }
            AudioBakeJob::Sequence { recipe } => {
                let sample_rate = recipe.sample_rate;
                let samples = bake_sequence(&recipe);
                samples_to_wav_bytes_pcm16(&samples, sample_rate)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Texture bake job (symbios-texture core)
// ---------------------------------------------------------------------------

/// Plain, serialisable pixel buffers extracted from a `symbios_texture`
/// [`TextureMap`] (which is not itself `Serialize`). RGBA8, row-major. `albedo`
/// is the large payload transferred back from the worker; the app rebuilds
/// Bevy `Image`s from these.
///
/// Buffers carry the **full mip chain** ([`TextureMap::with_mips`] runs inside
/// the job, mirroring the upstream async path) so the app's upload is a pure
/// buffer move rather than a main-thread box-filter pass —
/// [`mip_level_count`](Self::mip_level_count) says how many levels each buffer
/// holds (base level first).
#[derive(Serialize, Deserialize, Clone)]
pub struct TextureData {
    #[serde(with = "serde_bytes")]
    pub albedo: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub normal: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub roughness: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub emissive: Option<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    /// Mip levels contained in each pixel buffer, including the base level.
    /// Defaults to `1` (base only) so payloads from an older peer/worker that
    /// predates in-job mip-chaining still decode — the upload path mip-chains
    /// base-only data itself.
    #[serde(default = "default_mip_level_count")]
    pub mip_level_count: u32,
}

fn default_mip_level_count() -> u32 {
    1
}

impl From<TextureMap> for TextureData {
    fn from(m: TextureMap) -> Self {
        Self {
            albedo: m.albedo,
            normal: m.normal,
            roughness: m.roughness,
            emissive: m.emissive,
            width: m.width,
            height: m.height,
            mip_level_count: m.mip_level_count,
        }
    }
}

impl TextureData {
    /// Flat fallback of the requested size — used only if a generator rejects
    /// the dimensions (zero / over-`MAX_DIMENSION`), which the app's size clamps
    /// prevent, so the worker never panics on a stray config.
    fn flat(width: u32, height: u32) -> Self {
        let px = (width as usize) * (height as usize);
        Self {
            albedo: [0, 0, 0, 255].repeat(px),
            normal: [128, 128, 255, 255].repeat(px),
            roughness: [255, 255, 255, 255].repeat(px),
            emissive: None,
            width,
            height,
            mip_level_count: 1,
        }
    }
}

/// `symbios_texture::for_each_generator!` callback: build a unified,
/// serialisable [`TextureBakeJob`] enum (one variant per texture kind, carrying
/// that kind's config) plus a `generate()` that constructs the matching
/// generator and renders a `TextureMap`. This keeps the full texture catalogue
/// in lock-step with the core automatically — the same table the wrapper uses
/// for its (Bevy-coupled) `TextureConfig` — without depending on the wrapper.
macro_rules! define_texture_bake {
    ($(($variant:ident, $module:ident, $config_ty:ty, $generator_ty:ty, $kind:ident)),* $(,)?) => {
        /// A texture bake — every generator the `symbios-texture` core exposes.
        #[derive(Serialize, Deserialize, Clone)]
        pub enum TextureBakeJob {
            $( $variant($config_ty), )*
        }

        impl TextureBakeJob {
            fn generate(self, width: u32, height: u32) -> TextureData {
                let map = match self {
                    $(
                        TextureBakeJob::$variant(config) => {
                            <$generator_ty>::new(config).generate(width, height)
                        }
                    )*
                };
                // Mip-chain inside the job (worker thread), mirroring the
                // upstream async path's `f().map(TextureMap::with_mips)` —
                // the app-side upload then moves buffers instead of running
                // a box-filter pass on the main thread.
                map.map(TextureMap::with_mips)
                    .map(TextureData::from)
                    .unwrap_or_else(|_| TextureData::flat(width.max(1), height.max(1)))
            }
        }
    };
}

symbios_texture::for_each_generator!(define_texture_bake);

// ---------------------------------------------------------------------------
// Job / result
// ---------------------------------------------------------------------------

/// A self-contained generation job. New offloadable hotspots get added as
/// further variants; `run()` and the worker pick them up automatically.
#[derive(Serialize, Deserialize, Clone)]
pub enum GenJob {
    Heightmap(HeightmapParams),
    /// Procedural audio bake (patch or sequence) → WAV bytes.
    AudioBake(AudioBakeJob),
    /// Procedural texture bake at `width`×`height` → RGBA pixel buffers.
    TextureBake {
        job: TextureBakeJob,
        width: u32,
        height: u32,
    },
}

/// The output of a [`GenJob`], paired by variant with the job that produced it.
#[derive(Serialize, Deserialize, Clone)]
pub enum GenResult {
    Heightmap(HeightmapData),
    /// WAV bytes (mono 16-bit PCM).
    Audio(#[serde(with = "serde_bytes")] Vec<u8>),
    Texture(TextureData),
}

// Hydraulic-erosion tuning fixed by the app (mirror of
// `config::terrain::hydraulic`). Kept here as constants so the job stays fully
// self-contained — these are engine-fixed, not per-request inputs.
const HYDRAULIC_MAX_STEPS: u32 = 64;
const HYDRAULIC_MIN_SLOPE: f32 = 0.01;
const HYDRAULIC_WATER_LEVEL: f32 = 0.0;

impl GenJob {
    /// Execute the job purely on the current thread.
    pub fn run(self) -> GenResult {
        match self {
            GenJob::Heightmap(p) => GenResult::Heightmap(run_heightmap(p)),
            GenJob::AudioBake(j) => GenResult::Audio(j.run()),
            GenJob::TextureBake { job, width, height } => {
                GenResult::Texture(job.generate(width, height))
            }
        }
    }
}

/// Faithful port of the app's `terrain::heightmap::generate_terrain`, operating
/// on plain params and returning plain data. Reproducible from `seed` alone.
fn run_heightmap(p: HeightmapParams) -> HeightmapData {
    let grid = (p.grid_size as usize).max(2);
    let mut hm = HeightMap::new(grid, grid, p.cell_scale.max(0.01));

    apply_base_generator(&p, &mut hm);

    for v in hm.data_mut() {
        *v *= p.height_scale;
    }

    if p.erosion_enabled {
        HydraulicErosion {
            seed: p.seed,
            num_drops: p.erosion_drops,
            max_steps: HYDRAULIC_MAX_STEPS,
            inertia: p.inertia,
            erosion_rate: p.erosion_rate,
            deposition_rate: p.deposition_rate,
            evaporation_rate: p.evaporation_rate,
            capacity_factor: p.capacity_factor,
            min_slope: HYDRAULIC_MIN_SLOPE,
            water_level: HYDRAULIC_WATER_LEVEL,
            ..HydraulicErosion::new(p.seed)
        }
        .erode(&mut hm);
    }

    if p.thermal_enabled {
        ThermalErosion::new()
            .with_iterations(p.thermal_iterations)
            .with_talus_angle(p.thermal_talus_angle)
            .erode(&mut hm);
    }

    HeightmapData {
        width: hm.width() as u32,
        height: hm.height() as u32,
        scale: hm.scale(),
        data: hm.data().to_vec(),
    }
}

/// Floor on the hydraulic drop count of a proxy run, so tiny proxies
/// still carve *some* macro drainage instead of skipping erosion in all
/// but name.
const PROXY_MIN_DROPS: u32 = 500;

/// Floor on the thermal sweep count of a proxy run.
const PROXY_MIN_THERMAL: u32 = 4;

/// Low-resolution proxy of [`run_heightmap`] for synchronous derive-time
/// terrain queries (#905) — cheap enough to run inline while a room
/// record is being derived, close enough in macro shape that flat-region
/// decisions made against it hold on the full-resolution map.
///
/// Macro-shape fidelity per generator:
///
/// - `FbmNoise` samples noise in normalised grid space and
///   `VoronoiTerracing` lays its seeds in normalised space, so both
///   produce the *same* macro features at any resolution — the proxy
///   generates directly at `proxy_grid`.
/// - `DiamondSquare`'s RNG stream depends on the recursion depth (and
///   thus the grid size), so a low-res run is a different terrain. For
///   it the base generates at the full grid and is box-downsampled.
///
/// Erosion runs on the proxy with cost-scaled parameters: hydraulic
/// drops scale with the cell-count ratio, thermal sweeps with the linear
/// ratio, and the talus step with the cell-size ratio (it is a
/// per-adjacent-cell height threshold, so a constant *slope* limit
/// scales linearly with cell spacing). The result approximates — not
/// reproduces — the full map, which is why consumers pair it with a
/// conservative threshold and a compile-time safety net.
///
/// Deterministic from `p` + `proxy_grid` alone, like every job here.
pub fn run_heightmap_proxy(p: &HeightmapParams, proxy_grid: u32) -> HeightmapData {
    let full_grid = (p.grid_size as usize).max(2);
    let proxy_grid = (proxy_grid as usize).clamp(2, full_grid);
    let extent = (full_grid - 1) as f32 * p.cell_scale.max(0.01);
    let proxy_cell = extent / (proxy_grid - 1) as f32;

    let mut hm = match p.generator_kind {
        GeneratorKind::DiamondSquare => {
            let mut full = HeightMap::new(full_grid, full_grid, p.cell_scale.max(0.01));
            apply_base_generator(p, &mut full);
            box_downsample(&full, proxy_grid, proxy_cell)
        }
        GeneratorKind::FbmNoise | GeneratorKind::VoronoiTerracing => {
            let mut proxy = HeightMap::new(proxy_grid, proxy_grid, proxy_cell);
            apply_base_generator(p, &mut proxy);
            proxy
        }
    };

    for v in hm.data_mut() {
        *v *= p.height_scale;
    }

    let cell_ratio = ((full_grid * full_grid) as f64 / (proxy_grid * proxy_grid) as f64) as f32;
    let linear_ratio = (proxy_grid - 1) as f32 / (full_grid - 1) as f32;

    if p.erosion_enabled {
        HydraulicErosion {
            seed: p.seed,
            num_drops: ((p.erosion_drops as f32 / cell_ratio) as u32).max(PROXY_MIN_DROPS),
            max_steps: HYDRAULIC_MAX_STEPS,
            inertia: p.inertia,
            erosion_rate: p.erosion_rate,
            deposition_rate: p.deposition_rate,
            evaporation_rate: p.evaporation_rate,
            capacity_factor: p.capacity_factor,
            min_slope: HYDRAULIC_MIN_SLOPE,
            water_level: HYDRAULIC_WATER_LEVEL,
            ..HydraulicErosion::new(p.seed)
        }
        .erode(&mut hm);
    }

    if p.thermal_enabled {
        ThermalErosion::new()
            .with_iterations(
                ((p.thermal_iterations as f32 * linear_ratio) as u32).max(PROXY_MIN_THERMAL),
            )
            .with_talus_angle(p.thermal_talus_angle / linear_ratio)
            .erode(&mut hm);
    }

    HeightmapData {
        width: hm.width() as u32,
        height: hm.height() as u32,
        scale: hm.scale(),
        data: hm.data().to_vec(),
    }
}

/// Box-downsample `full` to a `proxy_grid`² map with cell size
/// `proxy_cell`: each proxy cell averages the full-resolution cells in
/// the window it covers, so macro shape survives and single-cell spikes
/// don't alias through.
fn box_downsample(full: &HeightMap, proxy_grid: usize, proxy_cell: f32) -> HeightMap {
    let full_grid = full.width();
    let mut proxy = HeightMap::new(proxy_grid, proxy_grid, proxy_cell);
    let ratio = (full_grid - 1) as f32 / (proxy_grid - 1) as f32;
    let half = (ratio * 0.5).max(0.5);

    for pz in 0..proxy_grid {
        for px in 0..proxy_grid {
            let cx = px as f32 * ratio;
            let cz = pz as f32 * ratio;
            let x0 = ((cx - half).ceil() as i32).max(0) as usize;
            let x1 = ((cx + half).floor() as i32).min(full_grid as i32 - 1) as usize;
            let z0 = ((cz - half).ceil() as i32).max(0) as usize;
            let z1 = ((cz + half).floor() as i32).min(full_grid as i32 - 1) as usize;
            let mut sum = 0.0_f64;
            let mut n = 0u32;
            for z in z0..=z1 {
                for x in x0..=x1 {
                    sum += full.get(x, z) as f64;
                    n += 1;
                }
            }
            proxy.set(px, pz, (sum / n.max(1) as f64) as f32);
        }
    }
    proxy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(seed: u64) -> HeightmapParams {
        HeightmapParams {
            grid_size: 16,
            cell_scale: 1.0,
            height_scale: 10.0,
            generator_kind: GeneratorKind::FbmNoise,
            seed,
            octaves: 4,
            persistence: 0.5,
            lacunarity: 2.0,
            base_frequency: 0.05,
            ds_roughness: 0.5,
            voronoi_num_seeds: 8,
            voronoi_num_terraces: 2,
            erosion_enabled: true,
            erosion_drops: 200,
            inertia: 0.05,
            erosion_rate: 0.3,
            deposition_rate: 0.3,
            evaporation_rate: 0.02,
            capacity_factor: 4.0,
            thermal_enabled: true,
            thermal_iterations: 5,
            thermal_talus_angle: 0.7,
        }
    }

    fn run(p: HeightmapParams) -> HeightmapData {
        match GenJob::Heightmap(p).run() {
            GenResult::Heightmap(d) => d,
            _ => unreachable!("a heightmap job must yield a heightmap result"),
        }
    }

    /// The whole offload design relies on native and the wasm worker producing
    /// byte-identical output from the same seed (cross-peer determinism).
    #[test]
    fn heightmap_is_deterministic_from_seed() {
        let a = run(params(1337));
        let b = run(params(1337));
        assert_eq!(a, b);
        assert_eq!(a.data.len(), (a.width * a.height) as usize);
        assert_eq!((a.width, a.height), (16, 16));
    }

    #[test]
    fn distinct_seeds_differ() {
        assert_ne!(run(params(1)).data, run(params(2)).data);
    }

    /// The app's `SovereignTerrainConfig::sanitize` clamps every coefficient
    /// into a finite envelope so the generated heightmap can never feed a
    /// non-finite value into `build_heightfield_collider`'s `assert!(is_finite)`
    /// (a remote crash — overlands #629). The value-noise output is always
    /// finite (its lattice table is bounded), so the place a non-finite value
    /// could actually originate is the *arithmetic*: the hydraulic/thermal
    /// erosion terms and the height-scale multiply. Exercise that corner — every
    /// erosion coefficient and `height_scale` at the top of its clamp range —
    /// for all three generators and assert the output stays finite.
    ///
    /// Octaves / lacunarity / base-frequency are held at moderate values rather
    /// than their clamp ceilings on purpose: the upstream value-noise lattice
    /// indexes with `coord as i32` and, at a huge `base_frequency ·
    /// lacunarity^octaves` product, hits a *debug-only* integer overflow (native
    /// and wasm release both wrap it harmlessly via `rem_euclid`). That is a
    /// separate upstream concern from the release-mode non-finite panic #629 is
    /// about, and it is reachable with editor-legal params independent of this
    /// clamp, so it is out of scope here.
    fn erosion_corner(kind: GeneratorKind) -> HeightmapParams {
        HeightmapParams {
            grid_size: 64,
            cell_scale: 0.01,       // MIN_CELL_SCALE
            height_scale: 10_000.0, // MAX_HEIGHT_SCALE
            generator_kind: kind,
            seed: 7,
            octaves: 8,
            persistence: 1.0,
            lacunarity: 4.0,      // MAX_LACUNARITY
            base_frequency: 32.0, // MAX_BASE_FREQUENCY
            ds_roughness: 1.0,
            voronoi_num_seeds: 256,
            voronoi_num_terraces: 8,
            erosion_enabled: true,
            erosion_drops: 4_000,
            inertia: 1.0,
            erosion_rate: 1.0,
            deposition_rate: 1.0,
            evaporation_rate: 1.0,
            capacity_factor: 256.0, // MAX_CAPACITY_FACTOR
            thermal_enabled: true,
            thermal_iterations: 40,
            thermal_talus_angle: 1.0,
        }
    }

    #[test]
    fn erosion_corner_output_is_finite() {
        for kind in [
            GeneratorKind::FbmNoise,
            GeneratorKind::DiamondSquare,
            GeneratorKind::VoronoiTerracing,
        ] {
            let d = run(erosion_corner(kind));
            assert!(
                d.data.iter().all(|v| v.is_finite()),
                "{kind:?} produced a non-finite height at the erosion/height clamp corner",
            );
        }
    }

    fn heightmap_of(d: &HeightmapData) -> HeightMap {
        let mut hm = HeightMap::new(d.width as usize, d.height as usize, d.scale);
        hm.data_mut().copy_from_slice(&d.data);
        hm
    }

    #[test]
    fn proxy_is_deterministic_and_spans_the_full_extent() {
        let p = params(99);
        let a = run_heightmap_proxy(&p, 8);
        let b = run_heightmap_proxy(&p, 8);
        assert_eq!(a, b);
        assert_eq!((a.width, a.height), (8, 8));
        // Same world extent as the full map, just sparser cells.
        let full_extent = (p.grid_size - 1) as f32 * p.cell_scale;
        let proxy_extent = (a.width - 1) as f32 * a.scale;
        assert!((full_extent - proxy_extent).abs() < 1e-3);
        assert!(a.data.iter().all(|v| v.is_finite()));
    }

    /// The proxy's whole purpose: macro shape must track the full map for
    /// every generator kind — including DiamondSquare, whose RNG stream is
    /// grid-size-dependent and therefore goes through the full-res +
    /// box-downsample path. Erosion is disabled so the comparison isolates
    /// the base-shape agreement (eroded proxies only approximate).
    #[test]
    fn proxy_macro_shape_tracks_full_map() {
        for kind in [
            GeneratorKind::FbmNoise,
            GeneratorKind::DiamondSquare,
            GeneratorKind::VoronoiTerracing,
        ] {
            let p = HeightmapParams {
                grid_size: 129,
                cell_scale: 2.0,
                base_frequency: 3.0,
                generator_kind: kind,
                erosion_enabled: false,
                thermal_enabled: false,
                ..params(4242)
            };
            let full = heightmap_of(&run(p.clone()));
            let proxy = heightmap_of(&run_heightmap_proxy(&p, 33));

            let mut sum_abs = 0.0_f64;
            let mut n = 0u32;
            for pz in 0..proxy.height() {
                for px in 0..proxy.width() {
                    let wx = px as f32 * proxy.scale();
                    let wz = pz as f32 * proxy.scale();
                    sum_abs += (proxy.get(px, pz) - full.get_height_at(wx, wz)).abs() as f64;
                    n += 1;
                }
            }
            let mean_abs = sum_abs / n as f64;
            // Tolerance is relative to the height scale; Voronoi's hard
            // terrace edges make point-vs-average differ locally, so the
            // assertion is on the mean.
            assert!(
                mean_abs < 0.12 * p.height_scale as f64,
                "{kind:?}: proxy diverges from full map (mean abs {mean_abs})"
            );
        }
    }

    #[test]
    fn proxy_with_erosion_is_finite_and_still_tracks_roughly() {
        for kind in [
            GeneratorKind::FbmNoise,
            GeneratorKind::DiamondSquare,
            GeneratorKind::VoronoiTerracing,
        ] {
            let p = HeightmapParams {
                grid_size: 129,
                cell_scale: 2.0,
                base_frequency: 3.0,
                generator_kind: kind,
                ..params(777)
            };
            let full = heightmap_of(&run(p.clone()));
            let proxy = heightmap_of(&run_heightmap_proxy(&p, 33));
            assert!(proxy.data().iter().all(|v| v.is_finite()));

            let mut sum_abs = 0.0_f64;
            let mut n = 0u32;
            for pz in 0..proxy.height() {
                for px in 0..proxy.width() {
                    let wx = px as f32 * proxy.scale();
                    let wz = pz as f32 * proxy.scale();
                    sum_abs += (proxy.get(px, pz) - full.get_height_at(wx, wz)).abs() as f64;
                    n += 1;
                }
            }
            let mean_abs = sum_abs / n as f64;
            // Looser than the erosion-free bound: the proxy's erosion is an
            // approximation by design.
            assert!(
                mean_abs < 0.2 * p.height_scale as f64,
                "{kind:?}: eroded proxy far from full map (mean abs {mean_abs})"
            );
        }
    }

    /// The heightmap `data` blob (#641) must survive the exact msgpack codec the
    /// wasm worker uses, byte-for-byte — the cross-peer determinism invariant is
    /// that the worker's returned heightmap equals native's direct `run()`.
    #[test]
    fn heightmap_data_round_trips_through_msgpack() {
        let original = run(params(2026));
        // Same codec as gen-worker's MsgpackCodec (to_vec_named / from_slice).
        let bytes = rmp_serde::to_vec_named(&original).expect("encode");
        let back: HeightmapData = rmp_serde::from_slice(&bytes).expect("decode");
        assert_eq!(
            original, back,
            "heightmap must round-trip bit-exactly through the worker codec"
        );
        // And via the actual boundary type the worker returns.
        let res = GenResult::Heightmap(original.clone());
        let res_bytes = rmp_serde::to_vec_named(&res).expect("encode result");
        let GenResult::Heightmap(res_back) =
            rmp_serde::from_slice(&res_bytes).expect("decode result")
        else {
            unreachable!("a heightmap result must decode as a heightmap");
        };
        assert_eq!(original, res_back);
    }
}
