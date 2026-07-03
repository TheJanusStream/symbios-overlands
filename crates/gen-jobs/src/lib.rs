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

/// Base terrain algorithm. Mirrors the app's `SovereignGeneratorKind` (kept
/// independent so this crate stays free of the app and Bevy).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratorKind {
    FbmNoise,
    DiamondSquare,
    VoronoiTerracing,
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
                map.map(TextureData::from)
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

    match p.generator_kind {
        GeneratorKind::FbmNoise => {
            FbmNoise {
                seed: p.seed,
                octaves: p.octaves.clamp(1, 32),
                persistence: p.persistence,
                lacunarity: p.lacunarity,
                base_frequency: p.base_frequency,
            }
            .generate(&mut hm);
            hm.normalize();
        }
        GeneratorKind::DiamondSquare => {
            DiamondSquare::new(p.seed, p.ds_roughness).generate(&mut hm);
            hm.normalize();
        }
        GeneratorKind::VoronoiTerracing => {
            VoronoiTerracing::new(
                p.seed,
                p.voronoi_num_seeds.max(1) as usize,
                p.voronoi_num_terraces.max(1) as usize,
            )
            .generate(&mut hm);
            // Voronoi already emits bounded [0, 1] output.
        }
    }

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
