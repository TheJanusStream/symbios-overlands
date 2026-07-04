//! Particle consumer channel (Phase 2, #244).
//!
//! [`particle_dispatcher`] walks this frame's [`AvatarContacts`] against
//! the [`ContactRecipeRegistry`] and spawns a short-lived, burst-only
//! [`ParticleEmitter`] for every matched `(sample, recipe)` pair. Each
//! such emitter is:
//!
//! - parented to the avatar via [`ChildOf`] so
//!   `update_emitter_motion`'s parent-chain walk resolves the avatar's
//!   `LinearVelocity` — that drives `inherit_velocity` so droplets fly
//!   with the avatar's momentum — while its `SimulationSpace::World`
//!   particles are still shed unparented and left behind;
//! - tagged [`TransientEmitter`] so [`retire_transient_emitters`]
//!   despawns the (otherwise idle-forever) emitter entity once its
//!   one-shot burst has fully aged out — without this, every water
//!   entry would leak a dead emitter for the rest of the session.
//!
//! Two guards bound emission: a global per-frame particle ceiling
//! ([`ContactRecipeRegistry::max_particles_per_frame`]) that absorbs a
//! stutter-frame / many-avatar spike, and a per-(avatar, recipe)
//! cooldown that throttles continuous `Dwell` recipes to a trickle
//! instead of an every-frame emitter storm.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::pds::{EmitterShape, Fp, Fp3};
use crate::world_builder::particles::{EmitterState, ParticleEmitter, spawn_particle_emitter};

use super::contact::{AvatarContacts, SurfaceContact, dominant_layer};
use super::recipes::{ContactRecipeRegistry, DUST_END_COLOR, DUST_START_COLOR};

/// World-space drift acceleration (m/s² per unit of `flow_dir`) biasing
/// water-contact bursts downstream. Gentle relative to gravity so a
/// splash still reads as a splash, just carried by the current.
const FLOW_DRIFT_ACCEL: f32 = 1.2;
/// Cap on the total drift bias so a pathological flow tangent can't
/// fling particles horizontally.
const FLOW_DRIFT_ACCEL_MAX: f32 = 3.0;

/// How far a terrain layer's albedo is lifted toward white before it
/// becomes the dust tint. Kicked-up dust reads as a dry, powdered version
/// of the surface — raw grass albedo (≈`[0.07, 0.12, 0.03]`) is near-black
/// and would render as soot; the lift lands it on a green-grey haze while
/// near-white snow stays white.
const DUST_ALBEDO_LIFT: f32 = 0.4;
/// End-of-life RGB as a fraction of the start tint — mirrors the default
/// tan ramp's slight darkening as a particle fades out.
const DUST_END_DARKEN: f32 = 0.9;

/// A representative albedo for a terrain splat layer, for tinting the
/// dust kicked off it. Only the procedural ground-family variants carry an
/// obvious colour pair; anything else (`Referenced`, bricks, planks, …)
/// returns `None` and the burst keeps its template colours.
fn layer_albedo(layer: &crate::pds::SovereignTextureConfig) -> Option<Vec3> {
    use crate::pds::SovereignTextureConfig;
    // Midpoint of the variant's two authored colours — representative of
    // the visible surface whichever of the pair dominates locally.
    match layer {
        SovereignTextureConfig::Ground(g) => {
            Some((Vec3::from_array(g.color_dry.0) + Vec3::from_array(g.color_moist.0)) * 0.5)
        }
        SovereignTextureConfig::Rock(r) => {
            Some((Vec3::from_array(r.color_light.0) + Vec3::from_array(r.color_dark.0)) * 0.5)
        }
        _ => None,
    }
}

/// Dust colour ramp derived from a terrain layer albedo: RGB comes from
/// the (white-lifted) albedo, the alpha ramp stays the default template's
/// so authored fade behaviour is untouched.
fn dust_colors_for_albedo(albedo: Vec3) -> (LinearRgba, LinearRgba) {
    let start = albedo.lerp(Vec3::ONE, DUST_ALBEDO_LIFT);
    let end = start * DUST_END_DARKEN;
    (
        LinearRgba::new(start.x, start.y, start.z, DUST_START_COLOR.alpha),
        LinearRgba::new(end.x, end.y, end.z, DUST_END_COLOR.alpha),
    )
}

/// Marks an emitter spawned by [`particle_dispatcher`] so
/// [`retire_transient_emitters`] can reclaim it after its one-shot
/// burst finishes. Never added to room/avatar PDS emitters, so the
/// existing particle use cases are untouched.
#[derive(Component, Debug)]
pub struct TransientEmitter;

/// Per-(avatar, recipe-index) time of last emission, for the cooldown
/// throttle on continuous (`Dwell`) recipes. Keyed by the recipe's
/// index in the registry (stable for the registry's lifetime; a room
/// recompile rebuilds both, and stale entries are TTL-pruned anyway),
/// so renaming a recipe in the editor never resets a live cooldown.
#[derive(Resource, Default)]
pub struct ParticleDispatchState {
    last_emit: HashMap<(Entity, usize), f32>,
}

/// Drop cooldown entries older than this (s) — far longer than any
/// recipe cooldown, so pruning never resets a live throttle.
const COOLDOWN_ENTRY_TTL: f32 = 5.0;

/// Scale an emitter's spawn shape so its extent tracks the avatar's
/// footprint (issue #244: "footprint radius from sample drives the
/// emitter spawn area radius"). The cone's `half_angle` is preserved so
/// the upward-fan character is size-independent; only the linear extent
/// scales. Clamped to a sane band so a degenerate footprint can't
/// collapse or blow up the shape. `Point`/`Unknown` have no extent.
fn scaled_shape(shape: &EmitterShape, extent: f32) -> EmitterShape {
    let e = extent.clamp(0.05, 8.0);
    match shape {
        EmitterShape::Point => EmitterShape::Point,
        EmitterShape::Sphere { .. } => EmitterShape::Sphere { radius: Fp(e) },
        EmitterShape::Box { .. } => EmitterShape::Box {
            half_extents: Fp3([e, e, e]),
        },
        EmitterShape::Cone { half_angle, .. } => EmitterShape::Cone {
            half_angle: *half_angle,
            height: Fp(e),
        },
        EmitterShape::Unknown => EmitterShape::Unknown,
    }
}

/// Phase 2 consumer: `AvatarContacts × recipes` → transient particle
/// bursts. Ordered `.after(ContactProducerSet)` so it reads the
/// freshly-built contacts for this frame.
pub fn particle_dispatcher(
    time: Res<Time>,
    contacts: Res<AvatarContacts>,
    registry: Res<ContactRecipeRegistry>,
    room_record: Option<Res<crate::state::LiveRoomRecord>>,
    mut state: ResMut<ParticleDispatchState>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs();
    let mut spawned_this_frame: u32 = 0;

    // Terrain splat layers, for tinting ground dust by the material the
    // avatar is running on. Resolved once per frame — `None` outside a
    // loaded room, where no terrain contact can fire anyway.
    let terrain_layers = room_record
        .as_ref()
        .and_then(|r| crate::pds::find_terrain_config(&r.0))
        .map(|cfg| &cfg.material.layers);

    'samples: for sample in &contacts.samples {
        for (idx, recipe) in registry.recipes.iter().enumerate() {
            if !recipe.enabled || !recipe.trigger.matches(sample) {
                continue;
            }

            // Cooldown throttle (continuous Dwell recipes).
            if recipe.spawn.cooldown > 0.0 {
                let key = (sample.avatar, idx);
                if let Some(&last) = state.last_emit.get(&key)
                    && now - last < recipe.spawn.cooldown
                {
                    continue;
                }
            }

            let want = recipe.spawn.count.eval(sample);
            if want == 0 {
                continue;
            }

            // Global per-frame ceiling — drop the overflow, never queue.
            let remaining = registry
                .max_particles_per_frame
                .saturating_sub(spawned_this_frame);
            if remaining == 0 {
                break 'samples;
            }
            let count = want.min(remaining);

            let mut emitter: ParticleEmitter = recipe.spawn.template.clone();
            emitter.burst_count = count;
            emitter.max_particles = emitter.max_particles.max(count);
            emitter.inherit_velocity = recipe.spawn.velocity_inherit;
            emitter.shape = scaled_shape(
                &emitter.shape,
                sample.footprint_radius * recipe.spawn.radius_scale,
            );
            // Flowing-water contacts drift their burst downstream: bias
            // the emitter's world-space acceleration along the surface's
            // downhill tangent (#659) so splash droplets ride the current
            // instead of hanging over the entry point. Flat water
            // (`flow_dir == 0`) is untouched.
            if let SurfaceContact::Water { flow_dir, .. } = sample.surface
                && flow_dir != Vec2::ZERO
            {
                let drift = (flow_dir * FLOW_DRIFT_ACCEL).clamp_length_max(FLOW_DRIFT_ACCEL_MAX);
                emitter.acceleration += Vec3::new(drift.x, 0.0, drift.y);
            }
            // Terrain bursts still carrying the default tan dust ramp get
            // their RGB re-derived from the dominant splat layer's albedo
            // (#661) — green-grey on grass, brown on dirt, grey on rock,
            // white on snow. A record-authored custom colour differs from
            // the sentinel and is left untouched; alpha ramps are kept
            // either way. Pure CPU colour pick at spawn, so native and
            // wasm behave identically.
            if let SurfaceContact::Terrain { material_blend, .. } = sample.surface
                && emitter.start_color == DUST_START_COLOR
                && emitter.end_color == DUST_END_COLOR
                && let Some(layers) = terrain_layers
                && let Some(albedo) = layer_albedo(&layers[dominant_layer(material_blend)])
            {
                (emitter.start_color, emitter.end_color) = dust_colors_for_albedo(albedo);
            }

            // Determinism is not required for cosmetic particles (same
            // policy as the perturbation pool); mix avatar + recipe +
            // time so concurrent bursts don't share an RNG stream.
            let seed = sample.avatar.to_bits().wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ (idx as u64)
                ^ ((now * 1000.0) as u64);

            // Parent to the avatar: `update_emitter_motion` walks the
            // `ChildOf` chain to the avatar's `LinearVelocity` (so
            // velocity inheritance works from frame 1), the World-space
            // particles are still shed unparented and left behind, and
            // the emitter rides the avatar's despawn if it leaves.
            // `tag_room_entity = false` — retirement / the avatar owns
            // its lifetime, not the room cleanup sweep.
            let e = spawn_particle_emitter(
                &mut commands,
                emitter,
                seed,
                Transform::from_translation(Vec3::ZERO),
                false,
                crate::world_builder::PlacementUnit::NONE,
            );
            commands
                .entity(e)
                .insert((TransientEmitter, ChildOf(sample.avatar)));

            spawned_this_frame += count;
            if recipe.spawn.cooldown > 0.0 {
                state.last_emit.insert((sample.avatar, idx), now);
            }
        }
    }

    // Prune stale cooldown entries (despawned avatars, long-idle).
    state
        .last_emit
        .retain(|_, &mut last| now - last < COOLDOWN_ENTRY_TTL);
}

/// Reclaim transient dispatcher emitters once their one-shot burst has
/// finished AND every particle it shed has aged out (`alive_count`
/// back to 0). Without this the burst-only emitter entity would idle
/// forever after firing — one leaked entity per water entry.
pub fn retire_transient_emitters(
    mut commands: Commands,
    emitters: Query<(Entity, &ParticleEmitter, &EmitterState), With<TransientEmitter>>,
) {
    for (entity, emitter, state) in emitters.iter() {
        if !emitter.looping && state.age > emitter.duration && state.alive_count == 0 {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaled_shape_tracks_extent_and_preserves_kind() {
        // Sphere radius follows the extent.
        let s = scaled_shape(&EmitterShape::Sphere { radius: Fp(1.0) }, 2.0);
        match s {
            EmitterShape::Sphere { radius } => assert!((radius.0 - 2.0).abs() < 1e-6),
            _ => panic!("kind changed"),
        }
        // Cone keeps its half-angle, height follows the extent.
        let c = scaled_shape(
            &EmitterShape::Cone {
                half_angle: Fp(0.7),
                height: Fp(0.4),
            },
            3.0,
        );
        match c {
            EmitterShape::Cone { half_angle, height } => {
                assert!((half_angle.0 - 0.7).abs() < 1e-6);
                assert!((height.0 - 3.0).abs() < 1e-6);
            }
            _ => panic!("kind changed"),
        }
        // Point has no extent.
        assert!(matches!(
            scaled_shape(&EmitterShape::Point, 5.0),
            EmitterShape::Point
        ));
    }

    #[test]
    fn layer_albedo_covers_ground_family_only() {
        use crate::pds::{SovereignGroundConfig, SovereignRockConfig, SovereignTextureConfig};
        // Ground / Rock average their colour pair.
        let ground = SovereignGroundConfig {
            color_dry: crate::pds::Fp3([1.0, 0.0, 0.0]),
            color_moist: crate::pds::Fp3([0.0, 1.0, 0.0]),
            ..Default::default()
        };
        let a = layer_albedo(&SovereignTextureConfig::Ground(ground)).expect("ground has albedo");
        assert!((a - Vec3::new(0.5, 0.5, 0.0)).length() < 1e-6);
        assert!(
            layer_albedo(&SovereignTextureConfig::Rock(SovereignRockConfig::default())).is_some()
        );
        // Non-ground variants keep the template colours.
        assert!(layer_albedo(&SovereignTextureConfig::None).is_none());
    }

    #[test]
    fn dust_ramp_lifts_albedo_and_keeps_alpha() {
        // Near-black grass albedo lands on a readable green-grey, not soot.
        let (start, end) = dust_colors_for_albedo(Vec3::new(0.07, 0.12, 0.03));
        assert!(
            start.green > start.red && start.red > start.blue,
            "hue order preserved"
        );
        assert!(start.green > 0.3, "lifted out of the near-black band");
        // Alpha ramp is the default template's, untouched by the tint.
        assert_eq!(start.alpha, DUST_START_COLOR.alpha);
        assert_eq!(end.alpha, DUST_END_COLOR.alpha);
        // Fade-out darkens slightly, mirroring the tan default.
        assert!(end.red < start.red && end.green < start.green);
    }

    #[test]
    fn scaled_shape_clamps_degenerate_extent() {
        // Zero footprint can't collapse the shape.
        let s = scaled_shape(&EmitterShape::Sphere { radius: Fp(1.0) }, 0.0);
        match s {
            EmitterShape::Sphere { radius } => assert!(radius.0 >= 0.05),
            _ => panic!("kind changed"),
        }
        // Absurd footprint is capped.
        let s = scaled_shape(&EmitterShape::Sphere { radius: Fp(1.0) }, 1000.0);
        match s {
            EmitterShape::Sphere { radius } => assert!(radius.0 <= 8.0),
            _ => panic!("kind changed"),
        }
    }
}
