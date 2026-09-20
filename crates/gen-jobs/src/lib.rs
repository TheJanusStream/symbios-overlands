//! Shared, platform-agnostic generation jobs for the compute-offload layer.
//!
//! A [`GenJob`] is a self-contained, serialisable description of a CPU-heavy
//! generation task; [`GenJob::run`] executes it **purely** (no Bevy, no I/O,
//! no globals - deterministic from the job's seed alone) and returns
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
use symbios_audio::{
    AudioPatch, ClampToEnvelope, Envelope, SequenceRecipe, bake, bake_sequence,
    samples_to_wav_bytes_pcm16,
};
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

/// The heightmap generator roster - the single declaration of which base
/// terrain algorithms exist, alongside the display label each wears.
///
/// Invoke it with a callback macro that receives every row as
/// `(Variant, "Label")`. This crate builds [`GeneratorKind`] from it; the
/// app builds its own wire enum `SovereignGeneratorKind`, that enum's
/// translation into this one, and the terrain panel's combo box from the
/// same rows - so a fourth algorithm is one row here and nothing else.
///
/// The app cannot simply *use* [`GeneratorKind`]: its wire enum is an open
/// union carrying an `Unknown` arm for an algorithm a newer engine names
/// and this build cannot run (#1119), while every variant of this one must
/// have a body in `define_heightmap_generators!`. One type must tolerate
/// a variant it cannot dispatch; the other must not. Sharing the roster is
/// what they can share.
///
/// This mirrors [`for_each_generator!`] on the texture side, re-exported
/// just above for the same reason.
#[macro_export]
macro_rules! for_each_heightmap_generator {
    ($callback:ident) => {
        $callback! {
            (FbmNoise, "FBM Noise"),
            (DiamondSquare, "Diamond Square"),
            (VoronoiTerracing, "Voronoi Terracing"),
        }
    };
}

macro_rules! define_generator_kind {
    ($( ($variant:ident, $label:literal) ),* $(,)?) => {
        /// Base terrain algorithm. Built from the
        /// [`for_each_heightmap_generator!`] roster, which the app's
        /// `SovereignGeneratorKind` is built from too - the two are
        /// separate types (see the roster's docs) that can no longer drift
        /// apart in *membership*.
        #[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
        pub enum GeneratorKind {
            $( $variant, )*
        }

        impl GeneratorKind {
            /// Every algorithm on the roster, in declaration order. Tests
            /// that must cover the whole family walk this rather than
            /// re-listing it.
            pub const ALL: &'static [GeneratorKind] = &[ $( GeneratorKind::$variant ),* ];

            /// Human-readable name, for the app's picker.
            pub fn label(self) -> &'static str {
                match self { $( GeneratorKind::$variant => $label, )* }
            }
        }
    };
}
for_each_heightmap_generator!(define_generator_kind);

/// The per-kind base dispatch inside [`GenJob::run`]'s heightmap path
/// (#657). One arm per algorithm, and the match is exhaustive over the
/// [`for_each_heightmap_generator!`] roster - so a row added to the roster
/// with no body here, or a body here for a row that is not on the roster,
/// is a compile error in both directions.
macro_rules! define_heightmap_generators {
    ($( $variant:ident => |$p:ident, $hm:ident| $body:block ),* $(,)?) => {
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

/// Plain, serialisable inputs for a heightmap generation job - the distilled
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

/// Generated heightmap data - plain row-major `f32` heights plus the dimensions
/// needed to rebuild a `symbios_ground::HeightMap` on the consuming side. On
/// wasm the `data` floats are serialized element-wise across the worker
/// boundary (a per-element copy - unlike the RGBA / WAV buffers, they are
/// not sent as a compact `serde_bytes` bin blob).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HeightmapData {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    /// Sent as a compact little-endian byte blob (msgpack `bin`) rather than an
    /// element-wise float array (#641) - matching how `TextureData`'s RGBA and
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
/// transport - no new dependency.
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
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Audio bake job (symbios-audio core)
// ---------------------------------------------------------------------------

/// A procedural audio bake - a patch one-shot or a multi-track sequence -
/// producing WAV bytes (mono 16-bit PCM - half the size of 32-bit float, which
/// matters on wasm where the heap never shrinks). The inputs are serialisable
/// so the job crosses the worker boundary; the heavy synthesis runs in
/// [`GenJob::run`].
#[derive(Serialize, Deserialize, Clone)]
pub enum AudioBakeJob {
    /// One-shot patch render of `duration_secs` at `sample_rate`, after
    /// `warmup_secs` baked and thrown away, with the last `loop_fade_secs`
    /// of the loop's own continuation faded back into its head.
    ///
    /// The warm-up is for a patch the world LOOPS whole: every filter in a
    /// bake starts from rest, so the loop's first sample is a cold filter's
    /// and its last a settled one's, and a tonal patch steps at the seam even
    /// when every rate in it closes the loop (#1385). Baking on past a
    /// warm-up and keeping only what follows starts the loop settled; with
    /// every rate a whole number of cycles a loop, it then meets itself.
    ///
    /// The fade is for what a settled filter cannot close: a NOISE source is
    /// not periodic, so the seam joins two unrelated noise samples and a
    /// heavily low-passed noise layer ticks once a second (#1387 - 19 of the
    /// 266 avatar voice patches, the wagon's roll worst at 6.3 times the
    /// largest step inside its own loop). Baking `loop_fade_secs` further
    /// gives the loop's own continuation past its end, and blending that
    /// into the head makes the seam an ordinary step of a continuous signal.
    Patch {
        patch: AudioPatch,
        sample_rate: u32,
        duration_secs: f32,
        /// `serde(default)` so a job encoded without it (an older bundle's
        /// `gen-worker.js`) still decodes, as the cold bake it asked for.
        #[serde(default)]
        warmup_secs: f32,
        /// Seconds of crossfade at the loop seam; `0.0` bakes the loop
        /// unfaded, bit for bit as it was before #1387.
        ///
        /// `serde(default)` as `warmup_secs` is, and the wire codec is NAMED
        /// msgpack, so both directions are safe: a new worker decodes an old
        /// job as an unfaded bake, and an old cached `gen-worker.js` ignores
        /// a key it does not know and bakes unfaded - a tick until the bundle
        /// updates, never a crash.
        #[serde(default)]
        loop_fade_secs: f32,
    },
    /// Multi-track sequence render (its sample rate is carried in the recipe).
    Sequence { recipe: SequenceRecipe },
}

impl AudioBakeJob {
    fn run(self) -> Vec<u8> {
        match self {
            AudioBakeJob::Patch {
                mut patch,
                sample_rate,
                duration_secs,
                warmup_secs,
                loop_fade_secs,
            } => {
                // The second line, not a replacement for the first. The
                // mirror sanitiser clamps on the load path, on the `Fp` grid,
                // before `to_native` - but it only sees patches that arrived
                // as a *record*. This one sees whatever reached the worker,
                // and the worker is where an unbounded graph actually costs
                // something: `bake` is `try_bake(..).expect(..)`, so a
                // malformed graph panics it, and on wasm the compute pool
                // runs on the main thread. `pds_sanitize`'s drift guard
                // asserts these two clamps agree constant for constant.
                patch.clamp_to_envelope(&Envelope::default());
                let warmup_secs = warmup_secs.max(0.0);
                let duration_secs = duration_secs.max(0.0);
                // Clamped here for the same reason, and before it lengthens
                // the bake rather than after: half the loop is the most a
                // seam fade can mean, and `max(0.0)` first sends a NaN to
                // zero rather than through `min`.
                let loop_fade_secs = loop_fade_secs.max(0.0).min(duration_secs * 0.5);
                let mut samples = bake(
                    &patch,
                    sample_rate,
                    warmup_secs + duration_secs + loop_fade_secs,
                );
                // Count every offset as `bake` itself counts a duration, from
                // the SUM - never as `len - fade`. Rounding the sum is not
                // rounding the parts: (0.25 + 1.0) s x 22 050 Hz is 27 562.5,
                // a genuine half sample, so a loop derived from the end of the
                // longer bake would land one sample off today's and every
                // construct loop in the world would move for nothing.
                let duration_samples =
                    |secs: f32| (f64::from(secs) * f64::from(sample_rate)).round() as usize;
                let loop_end = duration_samples(warmup_secs + duration_secs).min(samples.len());
                let kept = duration_samples(duration_secs).min(loop_end);
                let head = loop_end - kept;
                // However many the longer bake actually added, never more
                // than half of what is kept.
                let fade = (samples.len() - loop_end).min(kept / 2);
                // Sum-to-one Hann, NOT equal-power. Every rate in a looped
                // patch is closed (#1385), so the tail past the loop's end is
                // the same waveform as its head: correlated, and correlated
                // signals sum in AMPLITUDE. An equal-power pair peaks at 1.41
                // and leaves the faded window 2.0-2.3 dB hot on every tonal
                // patch - a new swell once a second on all 266 avatar voices
                // to fix the 19 that tick. Gains that sum to one leave the
                // periodic part exact, because `g x + (1 - g) x` is `x`; what
                // they cost is a 1.25 dB dip in the noise layer alone, inside
                // the material's own level swing over 10 ms (#1387).
                for i in 0..fade {
                    let t = (i as f32 + 0.5) / fade as f32;
                    let g = 0.5 - 0.5 * (std::f32::consts::PI * t).cos();
                    let tail = samples[loop_end + i];
                    samples[head + i] = samples[head + i] * g + tail * (1.0 - g);
                }
                // Keep the `duration_secs` window that ends at the loop's end,
                // so a warm or faded bake is as long as a cold one and the
                // samples past the faded window are the ones it always was.
                samples.truncate(loop_end);
                samples.drain(..head);
                samples_to_wav_bytes_pcm16(&samples, sample_rate)
            }
            AudioBakeJob::Sequence { mut recipe } => {
                // Clamped before `sample_rate` is read, so the rate the WAV
                // header claims is the rate the mixdown actually baked at.
                recipe.clamp_to_envelope(&Envelope::default());
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
/// buffer move rather than a main-thread box-filter pass -
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
    /// predates in-job mip-chaining still decode - the upload path mip-chains
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
    /// Flat fallback of the requested size - used only if a generator rejects
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
/// in lock-step with the core automatically - the same table the wrapper uses
/// for its (Bevy-coupled) `TextureConfig` - without depending on the wrapper.
macro_rules! define_texture_bake {
    ($(($variant:ident, $module:ident, $config_ty:ty, $generator_ty:ty, $kind:ident)),* $(,)?) => {
        /// A texture bake - every generator the `symbios-texture` core exposes.
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
                // upstream async path's `f().map(TextureMap::with_mips)` -
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
    ///
    /// The job is boxed because `TextureBakeJob` carries the largest generator
    /// config in the roster, and several of those now nest a whole weathering
    /// block. Inlining it would push every `GenJob` - including the small
    /// heightmap and audio variants - up to its size, and these are moved
    /// through queues and channels far more often than they are baked.
    TextureBake {
        job: Box<TextureBakeJob>,
        width: u32,
        height: u32,
    },
    /// Build a parametric avatar body from its record (#1061).
    ///
    /// The one job here whose *reason* for existing is wasm rather than
    /// throughput: a wasm `AsyncComputeTaskPool` runs on the main thread, so on
    /// the browser build every body would otherwise be a dropped frame or
    /// several. Native still runs it straight on the compute pool.
    ///
    /// **The cost has grown, and not where the old figures said.** This doc
    /// carried 68 ms at a draft atlas against 277 ms at a full one from #1061;
    /// re-measured for the 0.9 take (#1358) over the 13 seeded survey bodies,
    /// native release: **811 ms at the 256 draft atlas and 1,059 ms at the
    /// 1,024 full one** - and the published 0.8.1 engine measures 839 ms and
    /// 1,115 ms on the same harness, so this is where the engine has been for
    /// a while rather than something 0.9 did. What moved is the RATIO: the
    /// draft rung now saves about a quarter rather than three quarters,
    /// because the expensive half of a body is geometry the atlas size does
    /// not touch.
    ///
    /// Boxed for the same reason `TextureBake` is: the record is the largest
    /// input in the roster.
    AvatarBuild {
        record: Box<symbios_avatar::AvatarRecord>,
        /// Side of the square skin atlas, in texels - the draft/settle rung
        /// the caller is asking for.
        atlas: u32,
        /// Grow the far hair tier beside the near one (symbios-avatar #350,
        /// taken at #1358), for a caller that draws bodies at a distance.
        ///
        /// Opt-in because the engine only builds one when asked and this crate
        /// builds its own [`symbios_avatar::AvatarConfig`]: nothing here gets a
        /// far tier unless the caller says so. Measured on the 13 seeds of the
        /// wear survey, paired per seed against the same build without it: a
        /// median 9.5 ms more per body (+1.2 % at the 256 draft atlas, +0.6 %
        /// at the 1024 full one) and 37,862 bytes more on the wire back -
        /// 1.7 % of a draft body, 0.27 % of a full one, the same absolute
        /// figure either way because it is geometry, not texture.
        ///
        /// `serde(default)` so the field is optional on the wire: the worker
        /// codec is self-describing msgpack, and a job encoded without it (an
        /// older bundle's `gen-worker.js` left in a browser cache) still
        /// decodes, as the body it was always asking for.
        #[serde(default)]
        far_hair: bool,
    },
}

/// The output of a [`GenJob`], paired by variant with the job that produced it.
///
/// **Not `Clone`, since #1061.** A built `symbios_avatar::Avatar` owns several
/// megabytes of atlas and the engine withholds `Clone` on it deliberately, so
/// that anybody who wants the copy has to say so. A result is produced once
/// and consumed once by every path here, so nothing needed the derive.
#[derive(Serialize, Deserialize)]
pub enum GenResult {
    Heightmap(HeightmapData),
    /// WAV bytes (mono 16-bit PCM).
    Audio(#[serde(with = "serde_bytes")] Vec<u8>),
    Texture(TextureData),
    /// A built body, or `None` for a record describing one that cannot be
    /// meshed - the engine's single failure mode (limbs overlapping at a
    /// joint). Boxed because a built avatar owns its atlas.
    ///
    /// What crosses is **drawable, not rebuildable**: see the
    /// `serde-avatar` feature on `symbios-avatar`.
    Avatar(Option<Box<symbios_avatar::Avatar>>),
}

// Hydraulic-erosion tuning fixed by the app (mirror of
// `config::terrain::hydraulic`). Kept here as constants so the job stays fully
// self-contained - these are engine-fixed, not per-request inputs.
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
            GenJob::AvatarBuild {
                record,
                atlas,
                far_hair,
            } => GenResult::Avatar(
                symbios_avatar::Avatar::build_with(
                    &record,
                    &symbios_avatar::AvatarConfig {
                        atlas,
                        far_hair,
                        ..symbios_avatar::AvatarConfig::default()
                    },
                )
                .map(Box::new),
            ),
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

/// Low-resolution proxy of the full `run_heightmap` pass, for synchronous
/// derive-time
/// terrain queries (#905) - cheap enough to run inline while a room
/// record is being derived, close enough in macro shape that flat-region
/// decisions made against it hold on the full-resolution map.
///
/// Macro-shape fidelity per generator:
///
/// - `FbmNoise` samples noise in normalised grid space and
///   `VoronoiTerracing` lays its seeds in normalised space, so both
///   produce the *same* macro features at any resolution - the proxy
///   generates directly at `proxy_grid`.
/// - `DiamondSquare`'s RNG stream depends on the recursion depth (and
///   thus the grid size), so a low-res run is a different terrain. For
///   it the base generates at the full grid and is box-downsampled.
///
/// Erosion runs on the proxy with cost-scaled parameters: hydraulic
/// drops scale with the cell-count ratio, thermal sweeps with the linear
/// ratio, and the talus step with the cell-size ratio (it is a
/// per-adjacent-cell height threshold, so a constant *slope* limit
/// scales linearly with cell spacing). The result approximates - not
/// reproduces - the full map, which is why consumers pair it with a
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
    /// (a remote crash - overlands #629). The value-noise output is always
    /// finite (its lattice table is bounded), so the place a non-finite value
    /// could actually originate is the *arithmetic*: the hydraulic/thermal
    /// erosion terms and the height-scale multiply. Exercise that corner - every
    /// erosion coefficient and `height_scale` at the top of its clamp range -
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
        for &kind in GeneratorKind::ALL {
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
    /// every generator kind - including DiamondSquare, whose RNG stream is
    /// grid-size-dependent and therefore goes through the full-res +
    /// box-downsample path. Erosion is disabled so the comparison isolates
    /// the base-shape agreement (eroded proxies only approximate).
    #[test]
    fn proxy_macro_shape_tracks_full_map() {
        for &kind in GeneratorKind::ALL {
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
        for &kind in GeneratorKind::ALL {
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
    /// wasm worker uses, byte-for-byte - the cross-peer determinism invariant is
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

    /// A built body survives the worker boundary drawable (#1061), through
    /// the same codec `gen-worker` actually uses.
    ///
    /// The engine's own suite proves the serde contract; what this pins is
    /// the *job* layer: that `AvatarBuild` runs, that its result decodes as
    /// an avatar rather than as some other variant, and that the geometry a
    /// renderer uploads is bit-identical on the far side. A small atlas -
    /// the payload scales with its square and this is a unit test, not a
    /// benchmark.
    #[test]
    fn an_avatar_build_round_trips_through_msgpack() {
        let mut record =
            symbios_avatar::AvatarRecord::new("Worker", symbios_avatar::Archetype::default());
        record.reroll(11);

        let GenResult::Avatar(built) = GenJob::AvatarBuild {
            record: Box::new(record),
            atlas: 64,
            far_hair: true,
        }
        .run() else {
            unreachable!("an avatar job must return an avatar result");
        };
        let built = built.expect("the default body meshes");

        let bytes =
            rmp_serde::to_vec_named(&GenResult::Avatar(Some(built))).expect("encode result");
        let GenResult::Avatar(back) = rmp_serde::from_slice(&bytes).expect("decode result") else {
            unreachable!("an avatar result must decode as an avatar");
        };
        let back = back.expect("a built body stays built");

        // What the far side draws and queries.
        assert!(!back.meshes.is_empty(), "a body arrived with no geometry");
        assert!(!back.rig.joints.is_empty(), "a body arrived with no rig");
        assert!(
            !back.skin.albedo.is_empty(),
            "a body arrived with no painted atlas"
        );
        assert!(
            back.parts.eyes.is_some(),
            "a humanoid arrived without the eyes its blink needs"
        );
        // The far tier is the one part of a body that is NOT among `meshes`
        // (the engine keeps it beside them so a consumer that knows nothing
        // of tiers draws what it always drew), so it is the one part a
        // `serde(skip)` or a field rename could drop in silence - the whole
        // reason overlands asks for it here rather than on the main thread.
        let far = back.far_hair.expect("the far tier crossed the boundary");
        assert!(
            far.mesh.face_count() > 0,
            "the far tier arrived with no geometry"
        );
    }

    /// A job asks for the far tier and gets one; a job that does not, does
    /// not - and an `AvatarBuild` encoded with no `farHair` key at all still
    /// decodes, as the near-only body it was always asking for (#1358).
    ///
    /// The last of those is what `serde(default)` buys: the worker codec is
    /// self-describing msgpack and a browser can be holding an older
    /// `gen-worker.js` from its cache, so the field has to be optional on the
    /// wire rather than merely new.
    #[test]
    fn the_far_tier_is_asked_for_by_the_job_and_absent_from_an_older_wire() {
        let mut record =
            symbios_avatar::AvatarRecord::new("Worker", symbios_avatar::Archetype::default());
        record.reroll(11);

        let built = |far_hair| {
            let GenResult::Avatar(built) = GenJob::AvatarBuild {
                record: Box::new(record.clone()),
                atlas: 64,
                far_hair,
            }
            .run() else {
                unreachable!("an avatar job must return an avatar result");
            };
            built.expect("the default body meshes")
        };
        assert!(
            built(true).far_hair.is_some(),
            "a job that asked for a far tier did not get one"
        );
        assert!(
            built(false).far_hair.is_none(),
            "a job that did not ask for a far tier grew one anyway"
        );

        // The job as an older bundle encoded it: the variant's fields without
        // `far_hair` at all. Built as its own type rather than by editing the
        // new encoding, because that is what actually sits in a stale
        // `gen-worker.js` - and it needs no value-tree crate in a manifest
        // whose whole point is having almost nothing in it.
        #[derive(Serialize)]
        enum OlderGenJob<'a> {
            AvatarBuild {
                record: &'a symbios_avatar::AvatarRecord,
                atlas: u32,
            },
        }
        let older = rmp_serde::to_vec_named(&OlderGenJob::AvatarBuild {
            record: &record,
            atlas: 64,
        })
        .expect("encode the older job");
        let GenJob::AvatarBuild { far_hair, .. } =
            rmp_serde::from_slice(&older).expect("a job with no far_hair key still decodes")
        else {
            unreachable!("the variant is unchanged by dropping one of its fields");
        };
        assert!(!far_hair, "a missing far_hair must default to near-only");
    }

    /// #1385 and #1387: a warm Patch bake, faded or not, is exactly as long
    /// as a cold one, and it is the TAIL of one bake that ran on past the
    /// warm-up - the head is thrown away, not re-synthesised. A seam fade
    /// bakes further still and touches only its own window at the loop's
    /// head; the samples past that window are the ones the unfaded bake
    /// kept, byte for byte. The control: the head it drops differs from the
    /// tail it keeps, so the warm-up did change what was kept.
    #[test]
    fn a_warm_patch_bake_keeps_the_settled_tail_at_the_cold_length() {
        use symbios_audio::{BiquadLowpass, Connection, GraphNode, NodeGraph, NodeId, NodeKind};
        let mut inputs = std::collections::BTreeMap::new();
        inputs.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
        let patch = AudioPatch {
            seed: 0,
            graph: NodeGraph {
                nodes: vec![
                    GraphNode {
                        id: NodeId(0),
                        kind: NodeKind::Sine(symbios_audio::SineOsc {
                            freq_hz: 40.0,
                            phase_offset: 0.0,
                            amplitude: 0.34,
                        }),
                        inputs: Default::default(),
                    },
                    GraphNode {
                        id: NodeId(1),
                        kind: NodeKind::BiquadLowpass(BiquadLowpass {
                            cutoff_hz: 320.0,
                            q: 0.9,
                        }),
                        inputs,
                    },
                ],
                output: NodeId(1),
            },
        };
        let job = |warmup_secs, loop_fade_secs| {
            AudioBakeJob::Patch {
                patch: patch.clone(),
                sample_rate: 22_050,
                duration_secs: 1.0,
                warmup_secs,
                loop_fade_secs,
            }
            .run()
        };
        let (cold, warm, faded) = (job(0.0, 0.0), job(0.25, 0.0), job(0.25, 0.01));
        assert_eq!(
            warm.len(),
            cold.len(),
            "a warm-up must not lengthen the loop"
        );
        assert_eq!(
            faded.len(),
            cold.len(),
            "a seam fade must not lengthen the loop either"
        );
        let long = bake(&patch, 22_050, 1.25);
        let tail = samples_to_wav_bytes_pcm16(&long[long.len() - 22_050..], 22_050);
        assert_eq!(warm, tail, "the warm bake is the tail of the longer one");
        assert_ne!(warm, cold, "the warm-up changed nothing it kept");
        // 44-byte WAV header, then the 220 faded samples (10 ms at 22.05 kHz)
        // as 440 bytes of PCM16. Past them the loop must not have moved at
        // all: this is the guard on the INDEXING, where rounding the sum of
        // warm-up and loop (27 562.5, a genuine half sample) is not the same
        // as counting back from the longer bake's end.
        const WINDOW: usize = 44 + 220 * 2;
        assert_eq!(
            faded[WINDOW..],
            warm[WINDOW..],
            "the fade moved the loop past its own window"
        );
    }

    /// #1387: an older bundle's `gen-worker.js` encodes a Patch job with no
    /// `loop_fade_secs` key at all, and a current worker must decode that as
    /// the unfaded bake it asked for rather than fail the job - a tick until
    /// the bundle updates, never a crash. The other direction is the same
    /// property read backwards: the key an old worker does not know is one
    /// named msgpack lets it ignore. `warmup_secs` landed this way in #1385;
    /// this is the second field to, so the shape is worth a guard.
    #[test]
    fn a_patch_job_with_no_seam_fade_on_the_wire_bakes_unfaded() {
        use symbios_audio::{Connection, GraphNode, NodeGraph, NodeId, NodeKind, SineOsc};
        let mut inputs = std::collections::BTreeMap::new();
        inputs.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
        let patch = AudioPatch {
            seed: 0,
            graph: NodeGraph {
                nodes: vec![GraphNode {
                    id: NodeId(0),
                    kind: NodeKind::Sine(SineOsc {
                        freq_hz: 40.0,
                        phase_offset: 0.0,
                        amplitude: 0.34,
                    }),
                    inputs: Default::default(),
                }],
                output: NodeId(0),
            },
        };
        // The job as an older bundle encoded it: the variant's fields without
        // `loop_fade_secs`, built as its own type rather than by editing the
        // new encoding, because that is what actually sits in a stale worker.
        #[derive(Serialize)]
        enum OlderAudioBakeJob<'a> {
            Patch {
                patch: &'a AudioPatch,
                sample_rate: u32,
                duration_secs: f32,
                warmup_secs: f32,
            },
        }
        let older = rmp_serde::to_vec_named(&OlderAudioBakeJob::Patch {
            patch: &patch,
            sample_rate: 22_050,
            duration_secs: 1.0,
            warmup_secs: 0.25,
        })
        .expect("encode the older job");
        let decoded: AudioBakeJob =
            rmp_serde::from_slice(&older).expect("a job with no loop_fade_secs key still decodes");
        let AudioBakeJob::Patch { loop_fade_secs, .. } = &decoded else {
            unreachable!("the variant is unchanged by dropping one of its fields");
        };
        assert_eq!(
            *loop_fade_secs, 0.0,
            "a missing seam fade must bake unfaded"
        );
        let unfaded = AudioBakeJob::Patch {
            patch: patch.clone(),
            sample_rate: 22_050,
            duration_secs: 1.0,
            warmup_secs: 0.25,
            loop_fade_secs: 0.0,
        }
        .run();
        assert_eq!(
            decoded.run(),
            unfaded,
            "an older job must bake the loop it always baked"
        );
        // And a fade set on this side survives the round trip, so a current
        // worker bakes what the app asked for.
        let bytes = rmp_serde::to_vec_named(&AudioBakeJob::Patch {
            patch,
            sample_rate: 22_050,
            duration_secs: 1.0,
            warmup_secs: 0.25,
            loop_fade_secs: 0.01,
        })
        .expect("encode");
        let back: AudioBakeJob = rmp_serde::from_slice(&bytes).expect("decode");
        let AudioBakeJob::Patch { loop_fade_secs, .. } = &back else {
            unreachable!("a patch job decodes as a patch job");
        };
        assert_eq!(*loop_fade_secs, 0.01, "the fade crossed the boundary");
        assert_ne!(back.run(), unfaded, "the fade that crossed did nothing");
    }

    /// #1387: the fade's law sums to one, so it leaves a PERIODIC loop
    /// exactly as it was - the tail past the loop's end is the same waveform
    /// as its head, and `g x + (1 - g) x` is `x`. What it changes is a NOISE
    /// layer, which has no such tail. Both halves in one test, on the same
    /// graph with and without the noise, because either alone reads as the
    /// fade doing nothing or doing everything.
    ///
    /// This is the test that fails if anyone swaps the law for the
    /// equal-power crossfade #1387 originally asked for: equal-power sums two
    /// correlated signals to 1.41 and leaves the window 2 dB hot, so the
    /// tonal half of it would stop holding.
    #[test]
    fn the_seam_fade_leaves_a_tonal_loop_alone_and_moves_a_noisy_one() {
        use symbios_audio::{
            BiquadLowpass, Connection, GraphNode, Mix, NodeGraph, NodeId, NodeKind, SineOsc,
            WhiteNoise,
        };
        // A 40 Hz sine - a whole number of cycles in the loop (#1385) - under
        // the same lowpass, optionally mixed with white noise.
        let build = |noisy: bool| {
            let mut nodes = vec![GraphNode {
                id: NodeId(0),
                kind: NodeKind::Sine(SineOsc {
                    freq_hz: 40.0,
                    phase_offset: 0.0,
                    amplitude: 0.34,
                }),
                inputs: Default::default(),
            }];
            let mut mix_inputs = std::collections::BTreeMap::new();
            mix_inputs.insert("a".to_string(), vec![Connection::from_node(NodeId(0))]);
            if noisy {
                nodes.push(GraphNode {
                    id: NodeId(2),
                    kind: NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.3 }),
                    inputs: Default::default(),
                });
                mix_inputs.insert("b".to_string(), vec![Connection::from_node(NodeId(2))]);
            }
            nodes.push(GraphNode {
                id: NodeId(3),
                kind: NodeKind::Mix(Mix::default()),
                inputs: mix_inputs,
            });
            let mut lp_inputs = std::collections::BTreeMap::new();
            lp_inputs.insert("in".to_string(), vec![Connection::from_node(NodeId(3))]);
            nodes.push(GraphNode {
                id: NodeId(1),
                kind: NodeKind::BiquadLowpass(BiquadLowpass {
                    cutoff_hz: 320.0,
                    q: 0.9,
                }),
                inputs: lp_inputs,
            });
            AudioPatch {
                seed: 7,
                graph: NodeGraph {
                    nodes,
                    output: NodeId(1),
                },
            }
        };
        const WINDOW: usize = 44 + 220 * 2;
        for noisy in [false, true] {
            let job = |loop_fade_secs| {
                AudioBakeJob::Patch {
                    patch: build(noisy),
                    sample_rate: 22_050,
                    duration_secs: 1.0,
                    warmup_secs: 0.25,
                    loop_fade_secs,
                }
                .run()
            };
            let (plain, faded) = (job(0.0), job(0.01));
            let pcm = |wav: &[u8]| -> Vec<i32> {
                wav[44..WINDOW]
                    .chunks_exact(2)
                    .map(|b| i32::from(i16::from_le_bytes([b[0], b[1]])))
                    .collect()
            };
            let (a, b) = (pcm(&plain), pcm(&faded));
            let worst = a
                .iter()
                .zip(&b)
                .map(|(x, y)| x.abs_diff(*y))
                .max()
                .expect("the window holds samples");
            let rms = |v: &[i32]| {
                (v.iter().map(|x| f64::from(*x) * f64::from(*x)).sum::<f64>() / v.len() as f64)
                    .sqrt()
            };
            let db = 20.0 * (rms(&b) / rms(&a).max(1e-9)).log10();
            if noisy {
                // The control. Without it "the level did not move" would
                // prove only that the fade never ran.
                assert!(
                    worst > 100,
                    "the fade did not move the noise layer: worst {worst} LSB"
                );
            } else {
                // The LEVEL is what discriminates the law: a sum-to-one pair
                // leaves a correlated window exactly where it was, an
                // equal-power pair leaves it 2.0-2.3 dB hot. The handful of
                // LSB that do move are the patch's own residual settling -
                // the tail is a second further from a cold filter than the
                // head - and an equal-power fade would carry that drift too,
                // on top of its 2 dB, so only the level is asked here.
                assert!(
                    db.abs() <= 0.25,
                    "a sum-to-one fade left a purely tonal window {db:+.2} dB \
                     ({worst} LSB) - has the law been swapped for an \
                     equal-power crossfade?"
                );
            }
        }
    }
}
