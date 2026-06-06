//! Splat-stains overlay — the terrain consumer of the interaction
//! framework (Phase 3, #245).
//!
//! A low-res RGBA texture is CPU-stamped from terrain
//! [`ContactSample`](crate::interaction::ContactSample)s
//! and re-uploaded so `splat.wgsl` can darken/wet/dust the ground where
//! avatars have been:
//!
//! - **R — wetness**: deposited only while the avatar is still carrying
//!   water (a terrain contact within [`scfg::WET_CARRY_SECS`] of its
//!   last water contact). Decays over ~30 s. Shader: lower roughness +
//!   slightly darker albedo (a damp patch).
//! - **G — dust**: deposited proportional to ground speed; decays fast
//!   (~2 s). Shader: briefly lightens + desaturates albedo (a haze).
//! - **B — footprint**: deposited on every terrain contact; decays very
//!   slowly (~5 min). Shader: darkens albedo + flattens the normal
//!   (a trodden indent).
//! - **A — reserved**.
//!
//! ## Addressing — toroidal, no camera recenter
//!
//! World XZ maps to UV by `fract(xz / WORLD_PERIOD)` (CPU here, and in
//! the shader), sampled with a Repeat sampler. There is **no**
//! camera-recentred ring buffer and therefore **no origin pop** (the
//! "follows camera without re-centering pop" acceptance criterion is
//! met by construction). The trade-off is that stains repeat every
//! [`scfg::WORLD_PERIOD`] metres — invisible in practice for ephemeral
//! marks at a 64 m period, and the only thing within a period of the
//! camera at a time is the local avatar's own fresh trail.
//!
//! ## Cost
//!
//! - f32 shadow buffer: `DIM² × 16` ≈ 1 MiB (`DIM` = 256). Kept so a
//!   5-minute footprint half-life doesn't quantise to a fixed `u8` and
//!   freeze.
//! - RGBA8 GPU image: `DIM² × 4` ≈ 256 KiB.
//!
//! Decay + re-upload are skipped entirely once every texel has faded to
//! zero and nothing is being stamped (`dirty == false`), so a
//! grass-only scene with no avatars active costs nothing per frame.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::config::terrain::stains as scfg;

use super::contact::{AvatarContacts, SurfaceContact};

/// CPU-side stains accumulator + its GPU image handle. Allocated once at
/// startup; the terrain material binds [`Self::handle`] and the shader
/// gates on the stains uniform so an un-bound / all-zero texture is a
/// no-op.
#[derive(Resource)]
pub struct StainsImage {
    /// Handle to the RGBA8 `DIM×DIM` GPU texture.
    pub handle: Handle<Image>,
    /// f32 `[R,G,B,A]` accumulator, row-major `DIM×DIM`. The source of
    /// truth; the GPU image is a quantised mirror of this.
    shadow: Vec<[f32; 4]>,
    /// Elapsed-seconds stamp of the last decay pass (decay is computed
    /// from real elapsed time, so the cadence only bounds cost).
    last_decay: f32,
    /// `true` when the shadow has non-zero content (something was
    /// stamped, or residual stain is still decaying). While `false` the
    /// per-frame system early-returns — zero idle cost.
    dirty: bool,
}

impl StainsImage {
    fn new(handle: Handle<Image>) -> Self {
        Self {
            handle,
            shadow: vec![[0.0; 4]; scfg::TEXEL_DIM * scfg::TEXEL_DIM],
            last_decay: 0.0,
            dirty: false,
        }
    }

    /// World XZ → wrapped texel `(tx, ty)`. Pure; the shader performs
    /// the equivalent `fract(world / WORLD_PERIOD)`.
    fn texel(world_x: f32, world_z: f32) -> (usize, usize) {
        let dim = scfg::TEXEL_DIM;
        let u = (world_x / scfg::WORLD_PERIOD).rem_euclid(1.0);
        let v = (world_z / scfg::WORLD_PERIOD).rem_euclid(1.0);
        // `rem_euclid(1.0)` is in [0, 1); `* dim` is < dim, but guard
        // the float-edge case where it rounds to exactly `dim`.
        let tx = ((u * dim as f32) as usize).min(dim - 1);
        let ty = ((v * dim as f32) as usize).min(dim - 1);
        (tx, ty)
    }

    fn meters_per_texel() -> f32 {
        scfg::WORLD_PERIOD / scfg::TEXEL_DIM as f32
    }

    /// Additively stamp a toroidally-wrapped Gaussian disc into the
    /// given channels around a world XZ. `deposits` is `[R,G,B,A]`
    /// peak deposit; each is clamped into `[0, 1]` after accumulation.
    fn stamp(&mut self, world_x: f32, world_z: f32, radius_world: f32, deposits: [f32; 4]) {
        let dim = scfg::TEXEL_DIM;
        let mpt = Self::meters_per_texel();
        let radius_texels = (radius_world / mpt).max(0.75);
        // σ chosen so the disc edge (≈2σ) lands near `radius_texels`.
        let sigma = (radius_texels * 0.5).max(0.5);
        let reach = radius_texels.ceil() as i32;
        let (cx, cy) = Self::texel(world_x, world_z);
        let (cx, cy) = (cx as i32, cy as i32);
        let two_sigma_sq = 2.0 * sigma * sigma;

        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let d2 = (dx * dx + dy * dy) as f32;
                let w = (-d2 / two_sigma_sq).exp();
                if w < 0.01 {
                    continue;
                }
                // Toroidal wrap (matches the shader's Repeat sampler).
                let tx = (cx + dx).rem_euclid(dim as i32) as usize;
                let ty = (cy + dy).rem_euclid(dim as i32) as usize;
                let cell = &mut self.shadow[ty * dim + tx];
                for c in 0..4 {
                    if deposits[c] != 0.0 {
                        cell[c] = (cell[c] + deposits[c] * w).clamp(0.0, 1.0);
                    }
                }
            }
        }
        self.dirty = true;
    }

    /// Multiplicative per-channel decay over `dt` seconds using each
    /// channel's half-life. Returns the largest residual value (so the
    /// caller can flip `dirty` off once everything has faded).
    fn decay(&mut self, dt: f32) -> f32 {
        let f = |halflife: f32| 0.5_f32.powf(dt / halflife);
        let fr = f(scfg::WET_HALFLIFE);
        let fg = f(scfg::DUST_HALFLIFE);
        let fb = f(scfg::FOOTPRINT_HALFLIFE);
        let mut max_residual = 0.0_f32;
        for cell in &mut self.shadow {
            cell[0] *= fr;
            cell[1] *= fg;
            cell[2] *= fb;
            // A is reserved/unused — keep it pinned at zero.
            cell[3] = 0.0;
            max_residual = max_residual.max(cell[0]).max(cell[1]).max(cell[2]);
        }
        max_residual
    }

    /// Quantise the f32 shadow into the RGBA8 GPU image (the "upload":
    /// mutating the `Image` through `Assets<Image>` re-extracts it).
    fn write_to(&self, image: &mut Image) {
        let Some(data) = image.data.as_mut() else {
            return;
        };
        for (px, cell) in data.chunks_exact_mut(4).zip(&self.shadow) {
            px[0] = (cell[0] * 255.0) as u8;
            px[1] = (cell[1] * 255.0) as u8;
            px[2] = (cell[2] * 255.0) as u8;
            px[3] = 0;
        }
    }
}

/// Per-avatar "feet still wet" timer. An avatar that was in water within
/// [`scfg::WET_CARRY_SECS`] deposits wetness onto terrain it then walks.
/// Pruned every frame so it can't grow across a long session.
#[derive(Resource, Default)]
pub struct WetCarry {
    last_water: HashMap<Entity, f32>,
}

/// Startup: allocate the zeroed stains image + sampler and insert
/// [`StainsImage`]. RGBA8, `MAIN_WORLD | RENDER_WORLD` usage so CPU
/// mutations re-upload (same mechanism as the splat weight-map sync).
pub fn setup_stains(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let dim = scfg::TEXEL_DIM as u32;
    let mut image = Image::new(
        Extent3d {
            width: dim,
            height: dim,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0u8; scfg::TEXEL_DIM * scfg::TEXEL_DIM * 4],
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::default(),
    );
    // Repeat: the toroidal world→uv mapping wraps, and bilinear taps at
    // the wrap seam must wrap too (else a thin discontinuity line).
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..Default::default()
    });
    let handle = images.add(image);
    commands.insert_resource(StainsImage::new(handle));
}

/// Per-frame stains update — folds the issue's three conceptual systems
/// (stamp / decay / upload) into one to avoid double `get_mut` of the
/// image and a stamp→upload latency gap:
///
/// 1. Stamp every terrain [`ContactSample`](crate::interaction::ContactSample) into the f32 shadow.
/// 2. On the [`scfg::DECAY_INTERVAL`] cadence, multiplicatively decay
///    the shadow (real elapsed `dt`, so the fade curve is cadence-
///    independent).
/// 3. If anything changed this frame (stamped or decayed), quantise the
///    shadow into the GPU image — the re-upload.
///
/// Early-returns with zero work when nothing has ever been stamped and
/// no residual remains (idle grass scene → no regression).
pub fn update_stains(
    time: Res<Time>,
    contacts: Res<AvatarContacts>,
    mut stains: ResMut<StainsImage>,
    mut wet: ResMut<WetCarry>,
    mut images: ResMut<Assets<Image>>,
) {
    let now = time.elapsed_secs();

    // Refresh the water-carry timer from this frame's water contacts,
    // then prune anything older than the carry window.
    for s in &contacts.samples {
        if matches!(s.surface, SurfaceContact::Water { .. }) {
            wet.last_water.insert(s.avatar, now);
        }
    }
    wet.last_water
        .retain(|_, t| now - *t <= scfg::WET_CARRY_SECS);

    // --- Stamp ----------------------------------------------------------
    let mut stamped = false;
    for s in &contacts.samples {
        if !matches!(s.surface, SurfaceContact::Terrain { .. }) {
            continue;
        }
        let radius_world = (s.footprint_radius * scfg::STAMP_RADIUS_SCALE).max(0.05);

        // Footprint: every contact, scaled by engagement so a hard
        // landing presses deeper than a glancing step.
        let footprint = scfg::FOOTPRINT_DEPOSIT * s.intensity.clamp(0.0, 1.0);

        // Dust: driven by horizontal ground speed (a run kicks it up;
        // standing still does not). Ramps in above ~1 m/s.
        let ground_speed = Vec2::new(s.world_vel.x, s.world_vel.z).length();
        let dust_factor = ((ground_speed - 1.0) / 6.0).clamp(0.0, 1.0);
        let dust = scfg::DUST_DEPOSIT * dust_factor;

        // Wetness: only while the avatar's feet are still carrying water.
        let carrying = wet
            .last_water
            .get(&s.avatar)
            .is_some_and(|t| now - *t <= scfg::WET_CARRY_SECS);
        let wetness = if carrying { scfg::WET_DEPOSIT } else { 0.0 };

        stains.stamp(
            s.world_pos.x,
            s.world_pos.z,
            radius_world,
            [wetness, dust, footprint, 0.0],
        );
        stamped = true;
    }

    // --- Decay (cadenced) ----------------------------------------------
    let mut decayed = false;
    if stains.dirty && now - stains.last_decay >= scfg::DECAY_INTERVAL {
        let dt = (now - stains.last_decay).max(0.0);
        stains.last_decay = now;
        let residual = stains.decay(dt);
        decayed = true;
        if residual < 1.0 / 255.0 {
            // Everything has faded below one quantisation step — zero
            // the shadow and go clean so the next idle frames are free.
            for cell in &mut stains.shadow {
                *cell = [0.0; 4];
            }
            stains.dirty = false;
        }
    }
    if stains.last_decay == 0.0 {
        // First-ever pass: anchor the decay clock so the initial `dt`
        // isn't "all of session elapsed time".
        stains.last_decay = now;
    }

    // --- Upload (only when something changed) ---------------------------
    if !(stamped || decayed) {
        return;
    }
    if let Some(image) = images.get_mut(&stains.handle) {
        stains.write_to(image);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img() -> StainsImage {
        StainsImage::new(Handle::default())
    }

    #[test]
    fn texel_mapping_wraps_toroidally() {
        let dim = scfg::TEXEL_DIM;
        // Origin maps to texel 0.
        assert_eq!(StainsImage::texel(0.0, 0.0), (0, 0));
        // Exactly one period away wraps back to the same texel — the
        // defining property that makes "no recenter pop" hold.
        let p = scfg::WORLD_PERIOD;
        assert_eq!(StainsImage::texel(p, p), StainsImage::texel(0.0, 0.0));
        // Negative coords wrap (rem_euclid), not clamp.
        assert_eq!(StainsImage::texel(-p, -p), (0, 0));
        // Half a period → middle of the texture, and never out of range.
        let (mx, my) = StainsImage::texel(p * 0.5, p * 0.5);
        assert!(mx < dim && my < dim);
        assert!((mx as i32 - dim as i32 / 2).abs() <= 1);
    }

    #[test]
    fn stamp_writes_a_disc_and_marks_dirty() {
        let mut s = img();
        assert!(!s.dirty);
        s.stamp(0.0, 0.0, 1.0, [0.0, 0.0, 0.8, 0.0]);
        assert!(s.dirty);
        let dim = scfg::TEXEL_DIM;
        let (cx, cy) = StainsImage::texel(0.0, 0.0);
        // Peak at the centre, present in channel B only.
        let centre = s.shadow[cy * dim + cx];
        assert!(centre[2] > 0.0, "footprint deposited at centre");
        assert_eq!(centre[0], 0.0);
        assert_eq!(centre[1], 0.0);
    }

    #[test]
    fn stamp_saturates_at_one() {
        let mut s = img();
        for _ in 0..50 {
            s.stamp(0.0, 0.0, 1.0, [0.0, 0.0, 0.9, 0.0]);
        }
        let dim = scfg::TEXEL_DIM;
        let (cx, cy) = StainsImage::texel(0.0, 0.0);
        assert!(s.shadow[cy * dim + cx][2] <= 1.0 + 1e-6);
        assert!(s.shadow[cy * dim + cx][2] > 0.99);
    }

    #[test]
    fn decay_channels_at_their_configured_half_lives() {
        let mut s = img();
        let dim = scfg::TEXEL_DIM;
        let (cx, cy) = StainsImage::texel(0.0, 0.0);
        let i = cy * dim + cx;
        s.shadow[i] = [1.0, 1.0, 1.0, 0.0];
        s.dirty = true;

        // Decay each channel by exactly one of its half-lives.
        s.decay(scfg::WET_HALFLIFE);
        assert!(
            (s.shadow[i][0] - 0.5).abs() < 1e-4,
            "R halves in WET_HALFLIFE"
        );

        let mut s2 = img();
        s2.shadow[i] = [0.0, 1.0, 0.0, 0.0];
        s2.decay(scfg::DUST_HALFLIFE);
        assert!(
            (s2.shadow[i][1] - 0.5).abs() < 1e-4,
            "G halves in DUST_HALFLIFE"
        );

        let mut s3 = img();
        s3.shadow[i] = [0.0, 0.0, 1.0, 0.0];
        s3.decay(scfg::FOOTPRINT_HALFLIFE);
        assert!(
            (s3.shadow[i][2] - 0.5).abs() < 1e-4,
            "B halves in FOOTPRINT_HALFLIFE"
        );

        // Dust is the fastest-fading channel by design; footprints the
        // slowest. These are config invariants, so check at compile time.
        const _: () = assert!(scfg::DUST_HALFLIFE < scfg::WET_HALFLIFE);
        const _: () = assert!(scfg::WET_HALFLIFE < scfg::FOOTPRINT_HALFLIFE);
    }

    #[test]
    fn write_to_quantises_shadow_into_rgba8() {
        let mut s = img();
        let dim = scfg::TEXEL_DIM;
        let (cx, cy) = StainsImage::texel(0.0, 0.0);
        s.shadow[cy * dim + cx] = [1.0, 0.5, 0.25, 0.9];
        let mut image = Image::new(
            Extent3d {
                width: dim as u32,
                height: dim as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            vec![0u8; dim * dim * 4],
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::default(),
        );
        s.write_to(&mut image);
        let data = image.data.as_ref().unwrap();
        let base = (cy * dim + cx) * 4;
        assert_eq!(data[base], 255);
        assert_eq!(data[base + 1], 127);
        assert_eq!(data[base + 2], 63);
        // A is reserved → always written as 0.
        assert_eq!(data[base + 3], 0);
    }
}
