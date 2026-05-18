//! Phase-1 (revised) consumer: packs the live [`PerturbationPool`]
//! into each water material's `wake_samples_*` uniform arrays so the
//! fragment shader can render per-kind, age-enveloped displacement.
//!
//! Mirrors the per-frame material-patching pattern established by
//! [`crate::world_builder::compile::environment::apply_environment_state`]
//! for clouds: query the entity, fetch the asset mutably, write into
//! the extension's uniform block in place.
//!
//! ## Routing
//!
//! [`crate::water::WaterPlaneIndex`] (attached to each water-volume
//! entity by `spawn_water_volume`) maps the spawned entity back to its
//! index in [`crate::water::WaterSurfaces::planes`]. Each
//! [`crate::interaction::perturbation::Perturbation`] carries the same
//! `plane_idx`, so routing is a direct filter — no XZ search.
//!
//! ## Sample budget
//!
//! [`crate::water::WAKE_SAMPLES_MAX`] caps the per-plane slot count at
//! 32. When more perturbations share a plane,
//! [`crate::interaction::perturbation::pack_plane`] keeps the newest 32
//! (a fresh disturbance reads as more salient than one fading out).
//!
//! ## Update elision
//!
//! Calling `Assets::<WaterMaterial>::get_mut` marks the asset dirty
//! and triggers a uniform upload. A [`PrevWakeCount`] [`Local`] map of
//! `plane_idx → last frame's active_count` lets the system skip the
//! `get_mut` on planes that were empty last frame and remain empty
//! this frame, so an idle pond costs nothing past its last
//! perturbation aging out.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::terrain::WaterVolume;
use crate::water::{WAKE_SAMPLES_MAX, WaterMaterial, WaterPlaneIndex};

use super::perturbation::{PerturbationPool, pack_plane};

/// Per-system `Local` carrying the previous frame's `wake_active_count`
/// per water plane. Drives the idle-plane `get_mut` elision.
#[derive(Default)]
pub struct PrevWakeCount(HashMap<usize, u32>);

/// Per-frame consumer system. Runs in [`Update`] after
/// [`super::perturbation::spawn_perturbations`] (so it observes this
/// frame's freshly spawned perturbations) which itself runs after
/// [`super::plugin::ContactProducerSet`].
pub fn feed_water_wakes(
    pool: Res<PerturbationPool>,
    water_volumes: Query<(&WaterPlaneIndex, &MeshMaterial3d<WaterMaterial>), With<WaterVolume>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    mut prev_counts: Local<PrevWakeCount>,
) {
    let mut next_counts: HashMap<usize, u32> = HashMap::new();

    for (plane, mat_ref) in water_volumes.iter() {
        let idx = plane.0;
        let (a, b) = pack_plane(idx, &pool.live, WAKE_SAMPLES_MAX);
        let count = a.len() as u32;
        let prev = prev_counts.0.get(&idx).copied().unwrap_or(0);

        if count == 0 && prev == 0 {
            // Neither this frame nor the last touched this plane's
            // wake arrays — skip the get_mut so idle ponds pay nothing.
            continue;
        }

        let Some(mat) = water_materials.get_mut(&mat_ref.0) else {
            continue;
        };
        let uniforms = &mut mat.extension.uniforms;

        for (slot, (av, bv)) in a.iter().zip(b.iter()).enumerate() {
            uniforms.wake_samples_a[slot] = *av;
            uniforms.wake_samples_b[slot] = *bv;
        }
        // Zero the tail that was written last frame so a future caller
        // reading the arrays directly (the shader already stops at
        // `wake_active_count`) sees clean data.
        for slot in (count as usize)..(prev as usize).min(WAKE_SAMPLES_MAX) {
            uniforms.wake_samples_a[slot] = Vec4::ZERO;
            uniforms.wake_samples_b[slot] = Vec4::ZERO;
        }
        uniforms.wake_active_count = count;

        if count > 0 {
            next_counts.insert(idx, count);
        }
    }

    prev_counts.0 = next_counts;
}
