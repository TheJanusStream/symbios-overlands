//! Projected-decal stamper — consumer channel C of the interaction
//! framework (#246 remainder; authored per-room since #261).
//!
//! Where Phase 3's stains texture covers the splat terrain, this
//! channel drops short-lived, surface-aligned quads for contact marks
//! the stains texture can't carry. As of #261 it is **PDS-authored**:
//! the channel consumes the [`ContactEffectKind::DecalStamp`] recipes
//! that [`ContactRecipeRegistry::from_effects`] routes into
//! [`ContactRecipeRegistry::decals`] — same trigger + per-recipe
//! cooldown machinery as the particle dispatcher. No decal is seeded by
//! default, so the channel is inert until a room authors one (zero cost
//! when `registry.decals` is empty).
//!
//! Lifecycle: [`stamp_decals`] spawns a fading quad on a matched
//! contact (cooldown-throttled per `(avatar, recipe)`, ground-anchored
//! via [`TerrainSurfaceQuery`] for terrain so it lies flat);
//! [`update_decals`] ages every decal — growing + fading it — GCs the
//! expired and enforces a global live cap; [`cleanup_decals`] drops the
//! lot (and their one-off materials) on room exit so logout never
//! leaks.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::config::interaction::decal as dcfg;
use crate::state::AppState;

use super::classifier::TerrainSurfaceQuery;
use super::contact::{AvatarContacts, SurfaceContact};
use super::recipes::{ContactRecipeRegistry, DecalRuntimeParams};

/// Shared 1 m × 1 m XZ quad (normal +Y) every decal instances. Created
/// once at startup so stamping never allocates a mesh.
#[derive(Resource)]
pub struct DecalAssets {
    quad: Handle<Mesh>,
}

/// Per-`(avatar, decal-recipe index)` time of last stamp, for the
/// per-recipe cooldown throttle (mirrors
/// [`super::particle_channel::ParticleDispatchState`]; keyed by recipe
/// *index* so renaming a recipe never resets a live cooldown). Pruned
/// on a TTL far longer than any cooldown.
#[derive(Resource, Default)]
pub struct DecalStampState {
    last_stamp: HashMap<(Entity, usize), f32>,
}

/// Drop cooldown entries older than this (s) — far longer than any sane
/// per-recipe cooldown, so pruning never resets a live throttle.
const COOLDOWN_ENTRY_TTL: f32 = 30.0;

/// A live contact decal: a flat quad that grows + fades over [`Self::ttl`]
/// then is GC'd. Carries its own one-off [`StandardMaterial`] handle so
/// the fade can be per-instance; the handle is freed when the decal is
/// despawned.
#[derive(Component)]
pub struct Decal {
    age: f32,
    ttl: f32,
    start_size: f32,
    end_size: f32,
    start_alpha: f32,
    end_alpha: f32,
    material: Handle<StandardMaterial>,
}

impl Decal {
    fn from_params(p: &DecalRuntimeParams, material: Handle<StandardMaterial>) -> Self {
        Self {
            age: 0.0,
            // `ttl` is sanitised ≥ MIN_CONTACT_DECAL_TTL upstream, but
            // guard the `age / ttl` divide anyway (a registry built from
            // an un-sanitised record in a test could pass 0).
            ttl: p.ttl.max(1e-3),
            start_size: p.start_size,
            end_size: p.end_size,
            start_alpha: p.start_alpha,
            end_alpha: p.end_alpha,
            material,
        }
    }

    /// Interpolant in `[0, 1]` across the decal's life.
    fn t(&self) -> f32 {
        (self.age / self.ttl).clamp(0.0, 1.0)
    }
    fn size(&self) -> f32 {
        self.start_size + (self.end_size - self.start_size) * self.t()
    }
    fn alpha(&self) -> f32 {
        self.start_alpha + (self.end_alpha - self.start_alpha) * self.t()
    }
    fn expired(&self) -> bool {
        self.age >= self.ttl
    }
}

/// Startup: build the shared unit quad.
pub fn setup_decal_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let quad = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(0.5)));
    commands.insert_resource(DecalAssets { quad });
}

/// Spawn a fading quad for each matched authored decal recipe,
/// cooldown-throttled per `(avatar, recipe)` and ground-anchored so it
/// lies flat. Zero cost (early return) when no decal recipe is authored.
#[allow(clippy::too_many_arguments)]
pub fn stamp_decals(
    time: Res<Time>,
    contacts: Res<AvatarContacts>,
    registry: Res<ContactRecipeRegistry>,
    assets: Option<Res<DecalAssets>>,
    terrain: Option<Res<TerrainSurfaceQuery>>,
    mut state: ResMut<DecalStampState>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    if registry.decals.is_empty() {
        return;
    }
    let Some(assets) = assets else {
        return;
    };
    let now = time.elapsed_secs();
    let terrain = terrain.as_deref();

    for sample in &contacts.samples {
        for (idx, recipe) in registry.decals.iter().enumerate() {
            if !recipe.enabled || !recipe.trigger.matches(sample) {
                continue;
            }
            // Per-(avatar, recipe) cooldown.
            if recipe.cooldown > 0.0 {
                let key = (sample.avatar, idx);
                if let Some(&last) = state.last_stamp.get(&key)
                    && now - last < recipe.cooldown
                {
                    continue;
                }
            }

            // Anchor: terrain contacts get the exact ground point +
            // surface normal (lies flat); any other surface falls back
            // to the contact position, upright.
            let (anchor, normal) = match sample.surface {
                SurfaceContact::Terrain { .. } => {
                    if let Some(t) = terrain {
                        let (gy, n) = t.ground_at(sample.world_pos.x, sample.world_pos.z);
                        (Vec3::new(sample.world_pos.x, gy, sample.world_pos.z), n)
                    } else {
                        (sample.world_pos, Vec3::Y)
                    }
                }
                _ => (sample.world_pos, Vec3::Y),
            };
            let p = &recipe.params;
            let pos = anchor + normal * p.normal_offset;
            let rotation = Quat::from_rotation_arc(Vec3::Y, normal);

            let material = materials.add(StandardMaterial {
                base_color: Color::srgba(p.color[0], p.color[1], p.color[2], p.start_alpha),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                double_sided: true,
                cull_mode: None,
                ..default()
            });

            commands.spawn((
                Mesh3d(assets.quad.clone()),
                MeshMaterial3d(material.clone()),
                Transform {
                    translation: pos,
                    rotation,
                    scale: Vec3::splat(p.start_size),
                },
                Decal::from_params(p, material),
            ));

            if recipe.cooldown > 0.0 {
                state.last_stamp.insert((sample.avatar, idx), now);
            }
        }
    }

    // Prune stale cooldown entries (despawned avatars, long-idle).
    state
        .last_stamp
        .retain(|_, &mut t| now - t < COOLDOWN_ENTRY_TTL);
}

/// Age every decal (grow + fade via its own material), GC the expired,
/// and enforce the global live cap (oldest culled first). Sharing one
/// system keeps a single `Assets<StandardMaterial>` borrow and one
/// pass over the decals.
pub fn update_decals(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut decals: Query<(Entity, &mut Decal, &mut Transform)>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();

    // --- Age + fade -----------------------------------------------------
    for (_e, mut decal, mut xf) in decals.iter_mut() {
        decal.age += dt;
        let size = decal.size();
        xf.scale = Vec3::splat(size);
        if let Some(mat) = materials.get_mut(&decal.material) {
            mat.base_color.set_alpha(decal.alpha());
        }
    }

    // --- GC expired -----------------------------------------------------
    let mut live: Vec<(Entity, f32)> = Vec::new();
    for (e, decal, _) in decals.iter() {
        if decal.expired() {
            materials.remove(&decal.material);
            commands.entity(e).despawn();
        } else {
            live.push((e, decal.age));
        }
    }

    // --- Enforce the live cap (despawn the oldest over the limit) -------
    if live.len() > dcfg::MAX_LIVE {
        // Oldest first.
        live.sort_by(|a, b| b.1.total_cmp(&a.1));
        for &(e, _) in &live[dcfg::MAX_LIVE..] {
            if let Ok((_, decal, _)) = decals.get(e) {
                materials.remove(&decal.material);
            }
            commands.entity(e).despawn();
        }
    }
}

/// Room exit: despawn every decal and free its material so a logout /
/// room change never leaks transient quads.
pub fn cleanup_decals(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    decals: Query<(Entity, &Decal)>,
) {
    for (e, decal) in decals.iter() {
        materials.remove(&decal.material);
        commands.entity(e).despawn();
    }
}

/// Register the decal channel. Inert until a room authors a
/// [`crate::pds::ContactEffectKind::DecalStamp`] recipe (the registry's
/// `decals` list is empty by default).
pub fn build(app: &mut App) {
    app.init_resource::<DecalStampState>()
        .add_systems(Startup, setup_decal_assets)
        .add_systems(
            Update,
            (stamp_decals, update_decals).run_if(in_state(AppState::InGame)),
        )
        .add_systems(OnExit(AppState::InGame), cleanup_decals);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> DecalRuntimeParams {
        // Mirrors `pds::DecalParams::default()`.
        DecalRuntimeParams {
            ttl: 6.0,
            start_size: 0.45,
            end_size: 0.85,
            start_alpha: 0.55,
            end_alpha: 0.0,
            color: [0.14, 0.11, 0.09],
            normal_offset: 0.02,
        }
    }

    fn decal_at(age: f32) -> Decal {
        let mut d = Decal::from_params(&params(), Handle::default());
        d.age = age;
        d
    }

    #[test]
    fn fade_and_growth_track_age() {
        let p = params();
        let young = decal_at(0.0);
        assert!((young.alpha() - p.start_alpha).abs() < 1e-6);
        assert!((young.size() - p.start_size).abs() < 1e-6);
        assert!(!young.expired());

        let mid = decal_at(p.ttl * 0.5);
        let mid_alpha = p.start_alpha + (p.end_alpha - p.start_alpha) * 0.5;
        assert!((mid.alpha() - mid_alpha).abs() < 1e-5);
        // Grows monotonically toward end_size.
        assert!(mid.size() > young.size());

        let old = decal_at(p.ttl + 1.0);
        assert!(old.expired());
        // Interpolant is clamped, so an over-age decal reads as fully
        // faded, not extrapolated past end_alpha.
        assert!((old.alpha() - p.end_alpha).abs() < 1e-6);
        assert!((old.size() - p.end_size).abs() < 1e-6);
    }

    #[test]
    fn default_decal_outlives_the_acceptance_minimum() {
        // #246/#261 require a decal to be visible for 5 s+; the seeded
        // DecalParams::default ttl must clear that bar.
        let p = params();
        assert!(p.ttl >= 5.0);
        let at_five = decal_at(5.0);
        assert!(at_five.alpha() > 0.0);
        assert!(!at_five.expired());
    }

    #[test]
    fn zero_ttl_cannot_divide_by_zero() {
        let mut p = params();
        p.ttl = 0.0;
        let mut d = Decal::from_params(&p, Handle::default());
        // Guarded to a tiny positive ttl → t() is finite, no NaN/inf.
        assert!(d.t().is_finite());
        assert!(d.ttl > 0.0);
        // Fresh (age 0) it isn't expired yet; once aged past the
        // guarded tiny ttl it is, and t() stays clamped/finite.
        assert!(!d.expired());
        d.age = 1.0;
        assert!(d.expired());
        assert!(d.t().is_finite() && (d.t() - 1.0).abs() < 1e-6);
    }
}
