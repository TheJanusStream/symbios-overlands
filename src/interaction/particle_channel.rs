//! Particle consumer channel (Phase 2, #244).
//!
//! [`particle_dispatcher`] walks this frame's [`AvatarContacts`] against
//! the [`ContactRecipeRegistry`] and spawns a short-lived, burst-only
//! [`ParticleEmitter`] for every matched `(sample, recipe)` pair. Each
//! such emitter is:
//!
//! - parented to the avatar via [`ChildOf`] so
//!   `update_emitter_motion`'s parent-chain walk resolves the avatar's
//!   `LinearVelocity` - that drives `inherit_velocity` so droplets fly
//!   with the avatar's momentum - while its `SimulationSpace::World`
//!   particles are still shed unparented and left behind;
//! - tagged [`TransientEmitter`] so [`retire_transient_emitters`]
//!   despawns the (otherwise idle-forever) emitter entity once its
//!   one-shot burst has fully aged out - without this, every water
//!   entry would leak a dead emitter for the rest of the session.
//!
//! Two guards bound emission: a global per-frame particle ceiling
//! ([`ContactRecipeRegistry::max_particles_per_frame`]) that absorbs a
//! stutter-frame / many-avatar spike, and a per-(avatar, recipe)
//! cooldown that throttles continuous `Dwell` recipes to a trickle
//! instead of an every-frame emitter storm.

use bevy::prelude::*;

use crate::pds::{EmitterShape, Fp, Fp3};
use crate::world_builder::particles::{EmitterState, ParticleEmitter, spawn_particle_emitter};

use super::contact::{AvatarContacts, SurfaceContact, dominant_layer};
use super::cooldown::CooldownTable;
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
/// of the surface - raw grass albedo (≈`[0.07, 0.12, 0.03]`) is near-black
/// and would render as soot; the lift lands it on a green-grey haze while
/// near-white snow stays white.
const DUST_ALBEDO_LIFT: f32 = 0.4;
/// End-of-life RGB as a fraction of the start tint - mirrors the default
/// tan ramp's slight darkening as a particle fades out.
const DUST_END_DARKEN: f32 = 0.9;

/// A representative albedo for a terrain splat layer, for tinting the
/// dust kicked off it. Only the procedural ground-family variants carry an
/// obvious colour pair; anything else (`Referenced`, bricks, planks, …)
/// returns `None` and the burst keeps its template colours.
fn layer_albedo(layer: &crate::pds::SovereignTextureConfig) -> Option<Vec3> {
    use crate::pds::SovereignTextureConfig;
    // Midpoint of the variant's two authored colours - representative of
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

/// Per-`(avatar, recipe index)` cooldown state - a shared
/// [`CooldownTable`] behind this channel's own `Resource` type (mirrors
/// the audio / decal channels).
#[derive(Resource)]
pub struct ParticleDispatchState {
    cooldowns: CooldownTable,
}

/// Drop cooldown entries older than this (s) - far longer than any
/// recipe cooldown, so pruning never resets a live throttle.
const COOLDOWN_ENTRY_TTL: f32 = 5.0;

impl ParticleDispatchState {
    /// Forget every live throttle - the registry whose indices they key on
    /// has been replaced (#1254 f322).
    pub fn clear_cooldowns(&mut self) {
        self.cooldowns.clear();
    }
}

impl Default for ParticleDispatchState {
    fn default() -> Self {
        Self {
            cooldowns: CooldownTable::new(COOLDOWN_ENTRY_TTL),
        }
    }
}

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

/// How much bigger a terrain burst's puffs are for a body whose footprint is
/// `footprint` than for a walker's (#1549): in proportion to a walker's
/// default footprint, from 1 - a walker, or anything smaller - up to 3. The
/// recipe's puffs are sized for a running walker's feet, and the same 18 of
/// them spread over a car's footprint read as nothing at all.
fn dust_scale(footprint: f32) -> f32 {
    use super::locomotion::LocomotionFootprint;
    let walker = crate::pds::HumanoidParams::default().footprint_radius();
    (footprint / walker.max(0.01)).clamp(1.0, 3.0)
}

/// Where a burst rides its body, in the body's own frame: at its origin for
/// water - a hull at the waterline - and on the ground under it for terrain
/// (#1549), so dust rises from the ground rather than from a capsule's middle,
/// or out of a car's box above its wheels. Turned into the body's frame
/// because the burst is parented to it (velocity inheritance, despawn).
fn burst_offset(surface: &SurfaceContact, world_pos: Vec3, body: Option<&GlobalTransform>) -> Vec3 {
    let (SurfaceContact::Terrain { ground_y, .. }, Some(body)) = (surface, body) else {
        return Vec3::ZERO;
    };
    body.affine()
        .inverse()
        .transform_vector3(Vec3::new(0.0, ground_y - world_pos.y, 0.0))
}

/// Phase 2 consumer: `AvatarContacts × recipes` → transient particle
/// bursts. Ordered `.after(ContactProducerSet)` so it reads the
/// freshly-built contacts for this frame.
#[allow(clippy::too_many_arguments)]
pub fn particle_dispatcher(
    time: Res<Time>,
    contacts: Res<AvatarContacts>,
    registry: Res<ContactRecipeRegistry>,
    room_record: Option<Res<crate::state::LiveRoomRecord>>,
    mut state: ResMut<ParticleDispatchState>,
    mut commands: Commands,
    settings: Res<crate::state::LocalSettings>,
    bodies: Query<&GlobalTransform>,
) {
    // The viewer's own ceiling on somebody else's room (#1221 f308).
    // Bursts are the flashing-and-motion half of the same control.
    let intensity = settings.effects_intensity;
    if !intensity.plays() {
        return;
    }
    let now = time.elapsed_secs();
    let mut spawned_this_frame: u32 = 0;

    // Terrain splat layers, for tinting ground dust by the material the
    // avatar is running on. Resolved once per frame - `None` outside a
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

            // Cooldown throttle (continuous Dwell recipes), with the
            // viewer's floor under it (#1221 f308) - an authored zero means
            // once per frame per avatar.
            let cooldown = recipe.spawn.cooldown.max(intensity.cooldown_floor());
            if cooldown > 0.0 && state.cooldowns.active((sample.avatar, idx), now, cooldown) {
                continue;
            }

            let want = recipe.spawn.count.eval(sample);
            if want == 0 {
                continue;
            }

            // Global per-frame ceiling - drop the overflow, never queue.
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
            // A body bigger than a walker throws bigger dust (#1549).
            if matches!(sample.surface, SurfaceContact::Terrain { .. }) {
                let k = dust_scale(sample.footprint_radius);
                emitter.start_size *= k;
                emitter.end_size *= k;
            }
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
            // (#661) - green-grey on grass, brown on dirt, grey on rock,
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
            // `tag_room_entity = false` - retirement / the avatar owns
            // its lifetime, not the room cleanup sweep.
            let at = burst_offset(
                &sample.surface,
                sample.world_pos,
                bodies.get(sample.avatar).ok(),
            );
            let e = spawn_particle_emitter(
                &mut commands,
                emitter,
                seed,
                Transform::from_translation(at),
                false,
                crate::world_builder::PlacementUnit::NONE,
            );
            commands
                .entity(e)
                .insert((TransientEmitter, ChildOf(sample.avatar)));

            spawned_this_frame += count;
            if cooldown > 0.0 {
                state.cooldowns.mark((sample.avatar, idx), now);
            }
        }
    }

    // Prune stale cooldown entries (despawned avatars, long-idle).
    state.cooldowns.prune(now);
}

/// Reclaim transient dispatcher emitters once their one-shot burst has
/// finished AND every particle it shed has aged out (`alive_count`
/// back to 0). Without this the burst-only emitter entity would idle
/// forever after firing - one leaked entity per water entry.
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
    fn a_terrain_burst_rises_from_the_ground_under_its_body() {
        let ground = SurfaceContact::Terrain {
            material_blend: [1.0, 0.0, 0.0, 0.0],
            normal: Vec3::Y,
            ground_y: 2.0,
        };
        let at = Vec3::new(5.0, 2.95, -3.0);
        // Level, and pitched nose-up on a ramp: either way the burst lands on
        // the ground straight under the body, once its frame is undone.
        for rotation in [
            Quat::IDENTITY,
            Quat::from_rotation_x(0.4) * Quat::from_rotation_y(1.0),
        ] {
            let body =
                GlobalTransform::from(Transform::from_translation(at).with_rotation(rotation));
            let offset = burst_offset(&ground, at, Some(&body));
            let world = body.transform_point(offset);
            assert!(
                world.abs_diff_eq(Vec3::new(5.0, 2.0, -3.0), 1e-5),
                "{world}"
            );
        }
        let water = SurfaceContact::Water {
            plane_idx: 0,
            depth: 0.3,
            flow_dir: Vec2::ZERO,
            surface_y: 2.0,
        };
        let body = GlobalTransform::from(Transform::from_translation(at));
        assert_eq!(
            burst_offset(&water, at, Some(&body)),
            Vec3::ZERO,
            "a splash keeps to the hull"
        );
        assert_eq!(
            burst_offset(&ground, at, None),
            Vec3::ZERO,
            "no body, no offset"
        );
    }

    /// A walker's dust is the recipe's own size; a car's, a few times
    /// bigger in proportion to its footprint, and never more than three
    /// times (#1549).
    #[test]
    fn a_bigger_body_throws_bigger_dust() {
        use crate::interaction::locomotion::locomotion_footprint;
        use crate::pds::LocomotionConfig;
        let walker = locomotion_footprint(&LocomotionConfig::Humanoid(Box::default()));
        let car = locomotion_footprint(&LocomotionConfig::Car(Box::default()));
        assert_eq!(dust_scale(walker), 1.0);
        assert_eq!(dust_scale(walker * 0.5), 1.0, "a small body is not shrunk");
        assert!(
            (dust_scale(car) - (car / walker).min(3.0)).abs() < 1e-5 && dust_scale(car) > 1.5,
            "a car's dust grows with its footprint: {}",
            dust_scale(car)
        );
        assert_eq!(dust_scale(walker * 10.0), 3.0);
    }

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
