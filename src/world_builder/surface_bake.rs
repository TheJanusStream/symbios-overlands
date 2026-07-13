//! Offload-routed surface-texture bakes (#807).
//!
//! On **native**, [`super::material::build_procedural_material`] dispatches
//! texture generation through the upstream `TextureConfig::spawn` path — a
//! private rayon pool plus the upstream `patch_procedural_material_textures`
//! system. On **wasm** that "pool" degrades to Bevy's single-threaded task
//! executor: every avatar / construct / primitive texture bakes **on the main
//! thread**, which is the dominant half of the avatar re-roll freeze (#807 —
//! a reseed rebuilds ~130 image handles synchronously).
//!
//! This module is the wasm replacement: the bake runs as a
//! [`GenJob::TextureBake`] on the pooled gen-worker (see [`crate::offload`]),
//! and [`poll_surface_bakes`] mirrors the upstream patch system — rebuild
//! images (mip chains already computed in the worker), write the
//! [`TextureCache`], patch every waiting material's texture slots, and apply
//! the upstream emissive-factor sentinel. In-flight bakes are **coalesced** by
//! [`TextureCacheKey`]: mirrored parts (wheels, lamps) that request the same
//! fingerprint while a bake is airborne just join its target list instead of
//! dispatching a duplicate job.
//!
//! The resource and poll system are registered on every target (they are
//! empty no-ops on native, where dispatch is cfg'd to the upstream path), so
//! only the dispatch fork itself is target-gated.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::Task;
use bevy_symbios_texture::{
    GeneratedHandles, TextureCache, TextureCacheKey, TextureConfig, TextureMap, map_to_images,
    map_to_images_card,
};

use crate::offload::{GenJob, GenResult};

/// In-flight offloaded surface bakes, keyed by the same content fingerprint
/// the [`TextureCache`] uses — the coalescing map described in the module doc.
#[derive(Resource, Default)]
pub struct PendingSurfaceBakes {
    jobs: HashMap<TextureCacheKey, PendingBake>,
}

/// One airborne bake and every material waiting on it.
struct PendingBake {
    task: Task<GenResult>,
    /// `true` → upload via [`map_to_images_card`] (clamp-to-edge, alpha-masked
    /// card); `false` → repeat-tiling [`map_to_images`].
    is_card: bool,
    /// Materials whose texture slots receive the generated images. Grows when
    /// an identical config is requested while this bake is in flight.
    targets: Vec<Handle<StandardMaterial>>,
    /// Session-relative dispatch time for the completion-latency metric.
    spawned_at: f64,
    /// Stable job name pairing the `OffloadJobStarted`/`Completed` log events.
    job_name: String,
}

/// `symbios_texture::for_each_generator!` callback: the
/// [`TextureConfig`] → [`gen_jobs::TextureBakeJob`] mapper. Both enums are
/// generated from the same registry rows (and the wrapper re-exports the core
/// config types), so this match is in lock-step with the full generator
/// catalogue automatically — a new upstream generator flows through with no
/// app edit.
macro_rules! define_surface_bake_job {
    ($(($variant:ident, $module:ident, $config_ty:ty, $generator_ty:ty, $kind:ident)),* $(,)?) => {
        /// Map a material's texture config to its offloadable bake job.
        /// `None` for [`TextureConfig::None`] — nothing to bake.
        // Only the wasm dispatch fork calls this at runtime; native builds
        // exercise it from unit tests alone.
        #[cfg_attr(all(not(target_arch = "wasm32"), not(test)), allow(dead_code))]
        pub(super) fn surface_bake_job(cfg: &TextureConfig) -> Option<gen_jobs::TextureBakeJob> {
            match cfg {
                TextureConfig::None => None,
                $(TextureConfig::$variant(c) => {
                    Some(gen_jobs::TextureBakeJob::$variant(c.clone()))
                })*
            }
        }
    };
}
gen_jobs::for_each_generator!(define_surface_bake_job);

/// Mirror of the upstream (private) `apply_emissive_map` — assign a
/// generator-produced emissive map, defaulting the emissive *factor* so the
/// glow is visible: a map arriving while the factor is the black default sets
/// it to white; a regeneration that drops the map undoes an auto-white; a
/// caller-supplied non-black, non-white factor is left untouched. Keep in
/// lock-step with `bevy_symbios_texture::material::apply_emissive_map` (the
/// parity test in [`super::material`] guards the cache-hit path).
pub(super) fn apply_emissive_map(material: &mut StandardMaterial, emissive: Option<Handle<Image>>) {
    let e = material.emissive;
    let factor_is_unset = e.red == 0.0 && e.green == 0.0 && e.blue == 0.0;
    let factor_is_auto_white = e.red == 1.0 && e.green == 1.0 && e.blue == 1.0;
    match &emissive {
        Some(_) if factor_is_unset => material.emissive = LinearRgba::WHITE,
        None if factor_is_auto_white => material.emissive = LinearRgba::BLACK,
        _ => {}
    }
    material.emissive_texture = emissive;
}

/// Dispatch (or coalesce) one offloaded surface bake. Runs inside a
/// [`Commands::queue`] closure from
/// [`super::material::build_procedural_material`], so it has `&mut World` and
/// no caller signature had to grow a resource.
// Only the wasm dispatch fork queues this; native builds carry it solely so
// the wasm path stays compile-checked from native development.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(super) fn dispatch_surface_bake(
    world: &mut World,
    key: TextureCacheKey,
    job: gen_jobs::TextureBakeJob,
    size: u32,
    target: Handle<StandardMaterial>,
    is_card: bool,
) {
    let mut pending = world.resource_mut::<PendingSurfaceBakes>();

    // Coalesce: an identical config is already baking — join its target list.
    if let Some(bake) = pending.jobs.get_mut(&key) {
        bake.targets.push(target);
        return;
    }

    let now = world
        .get_resource::<Time>()
        .map_or(0.0, |t| t.elapsed_secs_f64());
    // Low fingerprint bits are plenty to pair Started/Completed in the log.
    let job_name = format!("surface_tex_{}_{:08x}", key.kind, key.fingerprint as u32);

    let task = crate::offload::offload(GenJob::TextureBake {
        job,
        width: size,
        height: size,
    });

    if let Some(mut log) = world.get_resource_mut::<crate::diagnostics::SessionLog>() {
        log.info(
            now,
            crate::diagnostics::event::EventPayload::OffloadJobStarted {
                job: job_name.clone(),
            },
        );
    }

    world.resource_mut::<PendingSurfaceBakes>().jobs.insert(
        key,
        PendingBake {
            task,
            is_card,
            targets: vec![target],
            spawned_at: now,
            job_name,
        },
    );
}

/// Drain finished offloaded surface bakes: rebuild the images (pure buffer
/// move — the worker mip-chained them), persist + insert into the
/// [`TextureCache`], and patch every waiting material's albedo / normal / ORM
/// / emissive slots. The wasm mirror of the upstream
/// `patch_procedural_material_textures` system; a no-op wherever nothing
/// dispatches (native, headless).
#[allow(clippy::too_many_arguments)]
pub(super) fn poll_surface_bakes(
    mut pending: ResMut<PendingSurfaceBakes>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut cache: Option<ResMut<TextureCache>>,
    time: Option<Res<Time>>,
    mut metrics: Option<ResMut<crate::diagnostics::MetricsRegistry>>,
    mut session_log: Option<ResMut<crate::diagnostics::SessionLog>>,
) {
    if pending.jobs.is_empty() {
        return;
    }

    let mut done: Vec<(TextureCacheKey, GenResult)> = Vec::new();
    for (key, bake) in pending.jobs.iter_mut() {
        if let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut bake.task))
        {
            done.push((key.clone(), result));
        }
    }

    let now = time.map_or(0.0, |t| t.elapsed_secs_f64());
    for (key, result) in done {
        let bake = pending
            .jobs
            .remove(&key)
            .expect("a just-polled bake is still in the map");

        // A texture-bake job only ever yields a texture; count an unexpected
        // variant as an offload error (E-4) and leave the targets on their
        // flat fallback colour rather than panic (upstream's error arm ditto).
        let GenResult::Texture(data) = result else {
            if let Some(m) = metrics.as_deref_mut() {
                crate::diagnostics::samplers::offload_job_error(m);
            }
            if let Some(log) = session_log.as_deref_mut() {
                log.error(
                    now,
                    crate::diagnostics::event::EventPayload::OffloadJobFailed {
                        job: bake.job_name.clone(),
                        reason: "offload job yielded a non-texture result".into(),
                    },
                );
            }
            warn!(
                "surface-texture offload job yielded an unexpected result — {} materials keep \
                 their flat colour",
                bake.targets.len()
            );
            continue;
        };

        let map = TextureMap {
            albedo: data.albedo,
            normal: data.normal,
            roughness: data.roughness,
            emissive: data.emissive,
            // Mip chains are computed inside the job; older workers that sent
            // base-only data deserialise the count to 1 and the upload
            // mip-chains here instead.
            mip_level_count: data.mip_level_count,
            width: data.width,
            height: data.height,
        };

        // Persist raw pixels for disk-backed stores while the map is still
        // available (the upload below consumes it), then upload and cache —
        // the same order as the upstream patch system.
        if let Some(cache_ref) = cache.as_deref() {
            cache_ref.persist_pixels(&key, &map, bake.is_card);
        }
        let handles: GeneratedHandles = if bake.is_card {
            map_to_images_card(map, &mut images)
        } else {
            map_to_images(map, &mut images)
        };
        if let Some(cache_ref) = cache.as_deref_mut() {
            cache_ref.insert(key.clone(), Arc::new(handles.clone()));
        }

        for target in &bake.targets {
            if let Some(mat) = materials.get_mut(target) {
                mat.base_color_texture = Some(handles.albedo.clone());
                mat.normal_map_texture = Some(handles.normal.clone());
                mat.metallic_roughness_texture = Some(handles.roughness.clone());
                apply_emissive_map(mat, handles.emissive.clone());
            }
            // A despawned target (rapid re-roll / editor drag) has simply
            // dropped its material; skipping it here lets the handle die.
        }

        let latency = now - bake.spawned_at;
        if let Some(m) = metrics.as_deref_mut() {
            crate::diagnostics::samplers::texture_bake_latency_secs(m, latency);
        }
        if let Some(log) = session_log.as_deref_mut() {
            log.info(
                now,
                crate::diagnostics::event::EventPayload::OffloadJobCompleted {
                    job: bake.job_name.clone(),
                    duration_secs: latency,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gen_jobs::TextureBakeJob;

    #[test]
    fn none_config_maps_to_no_job() {
        assert!(surface_bake_job(&TextureConfig::None).is_none());
    }

    #[test]
    fn every_bakeable_config_maps_to_its_matching_job_variant() {
        // Spot-check across the registry: a tiling surface, a card, and the
        // emissive generator. The macro generates the full table from the same
        // registry rows as `TextureBakeJob` itself, so variant-name drift is a
        // compile error rather than a runtime mismatch — these assertions
        // guard the *payload* wiring (config cloned into the job).
        let bark = TextureConfig::Bark(bevy_symbios_texture::bark::BarkConfig::default());
        assert!(matches!(
            surface_bake_job(&bark),
            Some(TextureBakeJob::Bark(_))
        ));

        let leaf = TextureConfig::Leaf(bevy_symbios_texture::leaf::LeafConfig::default());
        assert!(matches!(
            surface_bake_job(&leaf),
            Some(TextureBakeJob::Leaf(_))
        ));

        let lava = TextureConfig::Lava(bevy_symbios_texture::lava::LavaConfig::default());
        assert!(matches!(
            surface_bake_job(&lava),
            Some(TextureBakeJob::Lava(_))
        ));
    }

    #[test]
    fn emissive_sentinel_matches_upstream_semantics() {
        // Map arrives while factor is the black default → auto-white.
        let mut mat = StandardMaterial {
            emissive: LinearRgba::BLACK,
            ..Default::default()
        };
        apply_emissive_map(&mut mat, Some(Handle::default()));
        assert_eq!(mat.emissive, LinearRgba::WHITE);

        // Regeneration drops the map while factor is the auto-white → black.
        apply_emissive_map(&mut mat, None);
        assert_eq!(mat.emissive, LinearRgba::BLACK);

        // A caller-supplied tinted factor is left untouched in both directions.
        let tinted = LinearRgba::new(0.5, 0.2, 0.1, 1.0);
        mat.emissive = tinted;
        apply_emissive_map(&mut mat, Some(Handle::default()));
        assert_eq!(mat.emissive, tinted);
        apply_emissive_map(&mut mat, None);
        assert_eq!(mat.emissive, tinted);
    }
}
