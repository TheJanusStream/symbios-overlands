//! Rigged-body spawn and clip-driven locomotion (#1057, epic #1054).
//!
//! The generator half of an avatar spawns through the room compiler
//! ([`super::visuals`]); this module is the other half: a
//! [`crate::pds::AvatarBody::Rigged`] body whose references resolved becomes
//! a skinned `symbios-avatar` build under the same physics chassis. The
//! physics root is untouched — locomotion presets, colliders and controllers
//! stay exactly what the record's `locomotion` half says — and the skinned
//! body hangs off one offset child the way generator visuals do.
//!
//! Three systems, in the sibling crate's own frame order
//! ([`bevy_symbios_avatar::AvatarSystems`]):
//!
//!   - [`kick_rigged_builds`] notices a chassis whose resolved engine record
//!     differs from what is built and starts an [`Avatar::build`] on the
//!     compute pool. One build in flight per chassis, compared **by value**:
//!     a task whose target is stale lands, is stamped, and the next frame's
//!     comparison kicks the newer record — the same latest-wins discipline
//!     the record editor uses, and the guard #1061 needs on wasm, where a
//!     dropped task does not cancel. (Until #1061 routes this through
//!     gen-worker, the wasm "pool" is the main thread and a build stalls the
//!     frame; builds only fire on record changes, so the stall is per-edit,
//!     not per-frame.)
//!   - [`land_rigged_builds`] swaps the finished body in under a
//!     [`RiggedRoot`] child offset so the engine's ground plane (y = 0, feet)
//!     sits at the chassis collider's bottom
//!     ([`locomotion_total_height`] / 2 below its centre) — the same
//!     convention generator visuals are authored to.
//!   - [`drive_rigged_motion`] poses every built body from what its chassis
//!     is actually doing: clip locomotion (Idle_A / Walk / Jog / Sprint)
//!     picked by horizontal speed, play rate scaled by speed against each
//!     clip's reference so feet do not skate (the #873-877 anti-slide
//!     lesson), inertialized transitions, stance feet planted on the local
//!     ground plane, and engine-driven blinking. With an empty [`Clips`]
//!     library — a wasm session before `avatar.clips` arrives — the engine's
//!     procedural gait carries the body instead, so no build is ever a
//!     T-pose.
//!
//! The procedural [`super::gait`] layer never touches these bodies: it
//! animates [`crate::world_builder::AvatarVisualRoot`], which only the
//! generator spawn path inserts.

use avian3d::prelude::LinearVelocity;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use bevy_symbios_avatar::{
    AvatarBody as BuiltBody, AvatarClosure, AvatarPose, Clips, spawn_avatar,
};
use symbios_avatar::anim::{contacts_during, gait, plant_feet_of};
use symbios_avatar::{
    Avatar, Blink, Expression, FootingConfig, Gait, Ground, Inertializer, Limb, Pose, Stride,
};

use crate::interaction::locomotion::locomotion_total_height;
use crate::pds::avatar::EngineAvatarRecord;
use crate::state::{LiveAvatarRecord, LocalPlayer, RemotePeer};

/// Below this horizontal speed the body idles.
const IDLE_BELOW: f32 = 0.3;
/// Walk hands over to jog here, jog to sprint at the next one (m/s).
const WALK_TO_JOG: f32 = 2.2;
const JOG_TO_SPRINT: f32 = 4.4;
/// The travel speed each clip was authored at, used to scale its play rate
/// so foot speed tracks ground speed. Provenance: the mesh2motion source
/// clips walk ≈ 1.4 m/s, jog ≈ 3.2, sprint ≈ 5.8; exact figures matter less
/// than the ratio staying near 1, because the clamp below keeps a mismatch
/// from becoming either a moonwalk or a blur.
const WALK_REF: f32 = 1.4;
const JOG_REF: f32 = 3.2;
const SPRINT_REF: f32 = 5.8;
/// How far a clip's play rate may be bent from natural before it reads as
/// wrong; outside this the feet slide instead, which is the lesser evil at
/// the extremes of the speed range.
const RATE_CLAMP: (f32, f32) = (0.5, 2.0);
/// Vertical speed past which a body is treated as airborne: the cycle holds
/// and no foot is planted, so a fall does not march in mid-air.
const AIRBORNE_VERTICAL: f32 = 3.5;
/// How long a source switch blends, in seconds — the sibling viewer's own
/// default transition.
const BLEND_SECS: f32 = 0.15;

/// Atlas side used while the record is still moving under an editor. The
/// sibling viewer's own draft rung: 68 ms a build against 277 at full size.
const DRAFT_ATLAS: u32 = 256;
/// How long the record must be still before the full-atlas build is owed.
const SETTLE_SECS: f32 = 0.8;

/// The engine record whose build is currently standing under this chassis,
/// and the atlas it was built at. Compared by value against the resolved
/// reference to decide rebuilds; a draft-atlas build owes a full one once
/// the record settles (#1059's editor ladder).
#[derive(Component)]
pub(super) struct RiggedApplied {
    pub(super) record: EngineAvatarRecord,
    atlas: u32,
}

/// A build in flight for this chassis. At most one exists at a time.
#[derive(Component)]
pub(super) struct RiggedBuild {
    target: EngineAvatarRecord,
    /// The atlas this build runs at, stamped onto [`RiggedApplied`] so the
    /// settle pass knows a draft still owes the full build.
    atlas: u32,
    /// Vertical drop from chassis centre to the engine's ground plane,
    /// captured at kick time from the record's locomotion half.
    offset: f32,
    task: bevy::tasks::Task<crate::offload::GenResult>,
}

/// The one child of the chassis the skinned body hangs off. Deliberately not
/// [`crate::world_builder::AvatarVisualRoot`], so the procedural gait layer
/// cannot see it.
#[derive(Component)]
pub(super) struct RiggedRoot;

/// What the body was doing last frame, for cycle continuity and blends.
#[derive(Component)]
pub(super) struct RiggedMotion {
    cycle: f32,
    source: MotionSource,
    last_position: Option<Vec3>,
    previous: Option<Pose>,
    current: Option<Pose>,
    transition: Option<Inertializer>,
    blink: Blink,
}

impl Default for RiggedMotion {
    fn default() -> Self {
        Self {
            cycle: 0.0,
            source: MotionSource::Rest,
            last_position: None,
            previous: None,
            current: None,
            transition: None,
            blink: Blink::seeded(7),
        }
    }
}

/// What is carrying the body this frame. A change starts a blend.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MotionSource {
    /// Standing, no clips loaded: rest pose plus blinking.
    Rest,
    /// A baked clip, by index into [`Clips`].
    Clip(usize),
    /// The engine's procedural gait — the no-clips fallback.
    Gait,
}

/// Which library index each locomotion role resolved to. Rebuilt whenever
/// the [`Clips`] resource changes (the wasm fetch replacing the empty
/// library is exactly such a change).
#[derive(Resource, Default)]
pub(super) struct ClipRoles {
    idle: Option<usize>,
    walk: Option<usize>,
    jog: Option<usize>,
    sprint: Option<usize>,
}

/// Re-index the roles when the library changes. Names are the baked
/// artifact's own (`docs/clips.md` in the engine): a library without one of
/// them simply leaves that role procedural.
pub(super) fn index_clip_roles(clips: Res<Clips>, mut roles: ResMut<ClipRoles>) {
    if !clips.is_changed() {
        return;
    }
    let find = |name: &str| clips.0.clips.iter().position(|clip| clip.name == name);
    roles.idle = find("Idle_A");
    roles.walk = find("Walk");
    roles.jog = find("Jog");
    roles.sprint = find("Sprint");
}

/// Start a build for every chassis whose resolved rigged record is not the
/// one standing under it, and tear down rigged state on a chassis whose
/// body stopped being rigged.
#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub(super) fn kick_rigged_builds(
    mut commands: Commands,
    time: Res<Time>,
    live: Option<Res<LiveAvatarRecord>>,
    locals: Query<Entity, With<LocalPlayer>>,
    peers: Query<(Entity, &RemotePeer)>,
    applied: Query<&RiggedApplied>,
    building: Query<&RiggedBuild>,
    roots: Query<(Entity, &ChildOf), With<RiggedRoot>>,
    mut last_change: Local<bevy::platform::collections::HashMap<Entity, f32>>,
) {
    let now = time.elapsed_secs();
    let mut visit = |chassis: Entity, record: Option<&crate::pds::AvatarRecord>| {
        let rigged = record.and_then(|r| r.body.rigged_ref());
        // Rigged but unresolved is a WAIT, not a teardown: a live-preview
        // broadcast arrives with its references unresolved (`resolved` never
        // rides the wire), and tearing the standing body down while
        // `network::peer_cache` re-resolves would blink every rigged peer
        // out on every preview. The body that is up stays up.
        if rigged.is_some_and(|rig| rig.resolved.is_none()) {
            return;
        }
        let resolved = rigged.and_then(|rig| rig.resolved.as_ref());
        match resolved {
            Some(resolved) => {
                let has_root = roots
                    .iter()
                    .any(|(_, child_of)| child_of.parent() == chassis);
                let built = applied.get(chassis).ok();
                let same_record = built.is_some_and(|built| built.record == resolved.body);
                // The draft/settle ladder (#1059): while a record is moving —
                // an editor slider mid-drag, a stream of peer previews — a
                // build is only worth the draft atlas, because the next edit
                // obsoletes it; once it has been still for SETTLE_SECS the
                // full-atlas build is owed, even though nothing changed.
                if !same_record {
                    last_change.insert(chassis, now);
                }
                let settled = last_change
                    .get(&chassis)
                    .is_none_or(|&at| now - at >= SETTLE_SECS);
                let atlas = if settled {
                    symbios_avatar::AvatarConfig::default().atlas
                } else {
                    DRAFT_ATLAS
                };
                let atlas_owed = built.is_some_and(|built| built.atlas < atlas);
                if same_record && has_root && !atlas_owed {
                    return;
                }
                // One in flight per chassis: a stale target lands, and the
                // next frame's mismatch kicks the newer one.
                if building.contains(chassis) {
                    return;
                }
                let target = resolved.body.clone();
                let offset = record.map_or(0.0, |r| locomotion_total_height(&r.locomotion) / 2.0);
                // Through the platform-routed offload (#1061), not the compute
                // pool directly: on wasm that pool runs on the main thread, so
                // every body would be a dropped frame or several. Native still
                // lands on `AsyncComputeTaskPool` inside `offload`.
                let task = crate::offload::offload(crate::offload::GenJob::AvatarBuild {
                    record: Box::new(target.clone()),
                    atlas,
                });
                commands.entity(chassis).insert(RiggedBuild {
                    target,
                    atlas,
                    offset,
                    task,
                });
            }
            None => {
                // Not rigged (or not resolved): the generator path owns this
                // chassis. Drop any rigged residue so switching back later
                // rebuilds from scratch.
                if applied.contains(chassis) || building.contains(chassis) {
                    commands
                        .entity(chassis)
                        .remove::<(RiggedApplied, RiggedBuild)>();
                    for (root, child_of) in &roots {
                        if child_of.parent() == chassis {
                            commands.entity(root).despawn();
                        }
                    }
                }
            }
        }
    };

    if let Some(live) = live.as_ref() {
        for chassis in &locals {
            visit(chassis, Some(&live.0));
        }
    }
    for (chassis, peer) in &peers {
        visit(chassis, peer.avatar.as_ref());
    }
}

/// Land finished builds: swap the skinned body in under its offset root.
pub(super) fn land_rigged_builds(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    mut builds: Query<(Entity, &mut RiggedBuild)>,
    roots: Query<(Entity, &ChildOf), With<RiggedRoot>>,
) {
    use bevy::tasks::{block_on, futures_lite::future};
    for (chassis, mut build) in &mut builds {
        let Some(result) = block_on(future::poll_once(&mut build.task)) else {
            continue;
        };
        // The job roster is shared, so the variant is matched rather than
        // assumed: anything else here is a dispatch bug, not a bad body.
        let result = match result {
            crate::offload::GenResult::Avatar(avatar) => avatar.map(|boxed| *boxed),
            _ => {
                error!("an avatar build returned some other job's result");
                None
            }
        };
        commands.entity(chassis).remove::<RiggedBuild>();
        // Stamped even on failure: the engine returns None for exactly one
        // reason (limbs overlapping at a joint), and re-kicking the same
        // doomed record every frame would burn a core proving it. A changed
        // record re-triggers through the value comparison.
        commands.entity(chassis).insert(RiggedApplied {
            record: build.target.clone(),
            atlas: build.atlas,
        });
        let Some(avatar) = result else {
            warn!("a rigged avatar record described a body that could not be built");
            continue;
        };
        let stale: Vec<Entity> = roots
            .iter()
            .filter(|(_, child_of)| child_of.parent() == chassis)
            .map(|(root, _)| root)
            .collect();
        install_built_body(
            &mut commands,
            chassis,
            build.offset,
            avatar,
            &stale,
            &mut meshes,
            &mut materials,
            &mut images,
            &mut bindposes,
        );
    }
}

/// Replace whatever rigged body stands under `chassis` with `avatar`, hung
/// off a fresh [`RiggedRoot`] whose offset puts the engine's ground plane at
/// the chassis collider's bottom. Split from [`land_rigged_builds`] so a
/// test can land a body it built itself, at whatever atlas it can afford.
#[allow(clippy::too_many_arguments)]
fn install_built_body(
    commands: &mut Commands,
    chassis: Entity,
    offset: f32,
    avatar: Avatar,
    stale_roots: &[Entity],
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
) {
    for &root in stale_roots {
        commands.entity(root).despawn();
    }
    let root = commands
        .spawn((
            RiggedRoot,
            RiggedMotion::default(),
            rigged_root_transform(offset),
            Visibility::default(),
            ChildOf(chassis),
        ))
        .id();
    spawn_avatar(
        commands, root, avatar, 0.0, meshes, materials, images, bindposes,
    );
}

/// Where the skinned body hangs relative to its chassis (#1066).
///
/// Two corrections, both of them convention mismatches rather than tuning:
///
/// * **Height** — the engine's ground plane is `y = 0`, so the body drops by
///   half the collider so its feet meet the chassis capsule's bottom.
/// * **Facing** — a half turn about Y. `symbios_avatar::rig::landmark::FORWARD`
///   is `+Z`, the glTF/VRM convention the engine shares; Bevy's forward is
///   `-Z`, and the chassis is steered by
///   `Transform::looking_to(movement_direction, Y)`, which aims *its* `-Z`
///   down the direction of travel. Hanging the body off that with no rotation
///   pointed the engine's `+Z` face directly away from where the avatar was
///   going — walking correctly, moonwalking visibly. The half turn is applied
///   here, on the one entity that bridges the two conventions, rather than by
///   re-aiming the chassis (which the camera, the vehicles and the locomotion
///   drive all share) or by rotating the clips (which are authored in the
///   engine's frame and are consistent with the body).
///
/// Everything below this entity inherits the turn together — geometry, rig,
/// clips, and the socket anchors that
/// [`crate::player::attachments::LocalAttachment::rest_frame`] reconstructs an
/// offset against — so worn props stay put relative to the body they are on.
fn rigged_root_transform(offset: f32) -> Transform {
    Transform::from_xyz(0.0, -offset, 0.0)
        .with_rotation(Quat::from_rotation_y(std::f32::consts::PI))
}

/// Pose every built body from what its chassis is doing.
///
/// **The attachment-editing hold (#1062).** An attachment offset is stored
/// in its carrying joint's *rest* frame, so while the owner is authoring one
/// — numerically or with the in-world gizmo — their own body is pinned to
/// the bind pose and the whole editing session happens in the frame the
/// record actually keeps. The pin is a hard snap, not an inertial blend: a
/// body still settling would let a gizmo release land against a pose that is
/// already gone. Peers are never held; neither is the local body outside
/// that editor state.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn drive_rigged_motion(
    mut commands: Commands,
    time: Res<Time>,
    clips: Res<Clips>,
    roles: Res<ClipRoles>,
    mut bodies: Query<(Entity, &ChildOf, &BuiltBody, &mut RiggedMotion), With<RiggedRoot>>,
    chassis: Query<(&GlobalTransform, Option<&LinearVelocity>)>,
    locals: Query<(), With<LocalPlayer>>,
    avatar_editor: Option<Res<crate::ui::avatar::AvatarEditorState>>,
) {
    let delta = time.delta_secs();
    if delta <= 0.0 {
        return;
    }
    let editing_offsets = avatar_editor.is_some_and(|state| state.holds_rig_at_rest());
    for (entity, child_of, body, mut motion) in &mut bodies {
        if editing_offsets && locals.contains(child_of.parent()) {
            hold_at_rest(&mut commands, entity, body, &mut motion, delta);
            continue;
        }
        let Ok((transform, velocity)) = chassis.get(child_of.parent()) else {
            continue;
        };
        // Local chassis carry an avian velocity; remote peers are kinematic
        // playout, so their speed is read off the smoothed transform itself.
        let position = transform.translation();
        let (planar, vertical) = match velocity {
            Some(v) => (Vec2::new(v.0.x, v.0.z).length(), v.0.y.abs()),
            None => {
                let moved = motion
                    .last_position
                    .map_or(Vec3::ZERO, |last| (position - last) / delta);
                (Vec2::new(moved.x, moved.z).length(), moved.y.abs())
            }
        };
        motion.last_position = Some(position);
        let airborne = vertical > AIRBORNE_VERTICAL;

        // Pick the source and its cycle rate.
        let rig = &body.avatar.rig;
        let clip_for = |slot: Option<usize>| {
            slot.and_then(|index| clips.0.clips.get(index).map(|clip| (index, clip)))
                .filter(|(_, clip)| clip.duration() > 0.0)
        };
        let (source, cadence) = if planar < IDLE_BELOW {
            match clip_for(roles.idle) {
                Some((index, clip)) => (MotionSource::Clip(index), 1.0 / clip.duration()),
                None => (MotionSource::Rest, 0.0),
            }
        } else {
            let (slot, reference) = if planar < WALK_TO_JOG {
                (roles.walk, WALK_REF)
            } else if planar < JOG_TO_SPRINT {
                (roles.jog, JOG_REF)
            } else {
                (roles.sprint, SPRINT_REF)
            };
            match clip_for(slot) {
                Some((index, clip)) => {
                    let rate = (planar / reference).clamp(RATE_CLAMP.0, RATE_CLAMP.1);
                    (MotionSource::Clip(index), rate / clip.duration())
                }
                // No library (or a library missing the role): the engine's
                // procedural gait, cadence scaled the same way.
                None => (
                    MotionSource::Gait,
                    1.1 * (planar / WALK_REF).clamp(RATE_CLAMP.0, RATE_CLAMP.1),
                ),
            }
        };
        if !airborne {
            motion.cycle = (motion.cycle + delta * cadence).fract();
        }

        // Build this frame's target pose.
        let mut pose = Pose::rest(rig);
        let mut stance: Vec<Limb> = Vec::new();
        match source {
            MotionSource::Rest => {}
            MotionSource::Gait => {
                let gait = Gait::natural(rig);
                let stride = Stride::for_body(rig, 1.0);
                let steps = gait::step(rig, &mut pose, &gait, &stride, motion.cycle);
                gait::swing_arms(rig, &mut pose, &gait, motion.cycle);
                stance = steps.stance;
            }
            MotionSource::Clip(index) => {
                if let Some(clip) = clips.0.clips.get(index) {
                    let at = motion.cycle * clip.duration();
                    clip.apply(rig, &mut pose, at);
                    // In place: the chassis carries the travel; a clip that
                    // also travelled would walk out of its own collider and
                    // snap back once a cycle. The vertical bob is kept.
                    pose.translation.x = 0.0;
                    pose.translation.z = 0.0;
                    stance = contacts_during(rig, clip, at);
                }
            }
        }
        if airborne {
            stance.clear();
        }
        if !stance.is_empty() {
            // The engine's ground plane: the rigged root is offset so y = 0
            // is the chassis collider's bottom, which on a standing body is
            // the ground the physics chassis is resting on. Slopes are
            // carried by the chassis pose, exactly as the collider itself
            // handles them.
            plant_feet_of(
                rig,
                &mut pose,
                &stance,
                |point| Some(Ground::level(Vec3::new(point.x, 0.0, point.z))),
                &FootingConfig::default(),
            );
        }

        // Inertialize source switches so a jog does not snap into a stand.
        if motion.source != source
            && let (Some(previous), Some(current)) = (&motion.previous, &motion.current)
        {
            motion.transition = Some(Inertializer::start(
                previous, current, &pose, delta, BLEND_SECS,
            ));
        }
        motion.source = source;
        let mut posed = match &mut motion.transition {
            Some(transition) if !transition.finished() => {
                transition.advance(delta);
                transition.apply(&pose)
            }
            _ => {
                motion.transition = None;
                pose.clone()
            }
        };
        motion.previous = motion.current.take();
        motion.current = Some(posed.clone());

        // The blink rides after the blend, like the sibling viewer: smoothed
        // by a gait transition it reads as falling asleep.
        let closure = Expression::NEUTRAL.closure_at(motion.blink.advance(delta));
        if let Some(eyes) = body.avatar.parts.eyes.as_ref() {
            eyes.blink(&mut posed, closure);
        }
        commands
            .entity(entity)
            .insert((AvatarPose(posed), AvatarClosure(closure)));
    }
}

/// Pin one body to the bind pose for this frame — the attachment-editing
/// hold documented on [`drive_rigged_motion`].
///
/// Deliberately bypasses the [`Inertializer`]: the point is that the joint
/// entities sit *exactly* where `symbios_avatar::Pose::rest` puts them, which
/// is the frame [`crate::player::attachments::LocalAttachment::rest_frame`]
/// reconstructs a released gizmo pose against. `previous`/`current` are still
/// kept up to date, so releasing the hold blends back out of rest normally.
/// The blink rides along — an eyelid is not a socket, and a body that stops
/// blinking reads as broken rather than as held.
fn hold_at_rest(
    commands: &mut Commands,
    entity: Entity,
    body: &BuiltBody,
    motion: &mut RiggedMotion,
    delta: f32,
) {
    let mut posed = Pose::rest(&body.avatar.rig);
    let closure = Expression::NEUTRAL.closure_at(motion.blink.advance(delta));
    if let Some(eyes) = body.avatar.parts.eyes.as_ref() {
        eyes.blink(&mut posed, closure);
    }
    motion.source = MotionSource::Rest;
    motion.transition = None;
    motion.previous = motion.current.take();
    motion.current = Some(posed.clone());
    commands
        .entity(entity)
        .insert((AvatarPose(posed), AvatarClosure(closure)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::AvatarRecord;
    use crate::pds::avatar::ResolvedRig;
    use crate::pds::avatar::wardrobe::engine_default_for_did;
    use bevy::ecs::system::RunSystemOnce;

    /// A minimal world carrying every store the spawn path touches — the
    /// same skeleton `tests/freeze_rigid_body.rs` builds.
    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()));
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.init_asset::<Image>();
        app.init_asset::<SkinnedMeshInverseBindposes>();
        app.insert_resource(Clips::default());
        app.init_resource::<ClipRoles>();
        app
    }

    fn rigged_record(resolved: ResolvedRig) -> AvatarRecord {
        let mut record = AvatarRecord::wearing("3jzfcijpj2z2a");
        if let Some(rig) = record.body.rigged_mut() {
            rig.resolved = Some(resolved);
        }
        record
    }

    /// #1066: the body must face where the chassis is going.
    ///
    /// The engine and Bevy disagree about forward — `landmark::FORWARD` is
    /// `+Z` (the glTF/VRM convention), Bevy's is `-Z`, and the chassis is
    /// aimed with `Transform::looking_to`, which points *its* `-Z` down the
    /// direction of travel. Without the half turn on the rigged root the
    /// avatar ran backwards.
    ///
    /// Asserted against the engine's OWN forward landmark rather than
    /// against a re-derivation of it: `Socket::Chest`'s anchor is defined to
    /// face `+FORWARD`, so if upstream ever changes which way that points,
    /// this fails instead of silently agreeing with a stale constant.
    #[test]
    fn a_rigged_body_faces_the_way_its_chassis_travels() {
        let avatar = symbios_avatar::Avatar::build_with(
            &engine_default_for_did("did:plc:facing-test"),
            &symbios_avatar::AvatarConfig {
                atlas: 64,
                ..Default::default()
            },
        )
        .expect("the seeded default engine body builds");

        let chest = symbios_avatar::Socket::Chest
            .anchor(&avatar.rig)
            .expect("a humanoid rig has a chest");
        // Sanity: the engine really does put the chest on +Z. If this trips,
        // the bug is upstream and the correction below is aimed wrong.
        assert!(
            chest.direction.dot(Vec3::Z) > 0.9,
            "engine chest anchor is no longer +Z forward: {}",
            chest.direction
        );

        // In chassis space, after the root's correction.
        let facing = rigged_root_transform(0.9).rotation * chest.direction;
        assert!(
            facing.dot(Vec3::NEG_Z) > 0.9,
            "the body's chest must point down Bevy's forward (-Z), the axis \
             `looking_to` aims at the direction of travel; got {facing}"
        );
        // And the turn must be a pure yaw — a body tipped or rolled here
        // would plant its feet through the floor.
        let up = rigged_root_transform(0.9).rotation * Vec3::Y;
        assert!(
            up.dot(Vec3::Y) > 0.999,
            "the correction tilted the body: {up}"
        );
    }

    #[test]
    fn the_shipped_clip_archive_carries_every_locomotion_role() {
        // The artifact overlands actually deploys, read by the loader's own
        // path: a renamed or re-baked archive that loses a role would
        // otherwise degrade every body to the procedural gait silently.
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/avatar.clips"))
            .expect("assets/avatar.clips ships with the app");
        let library = symbios_avatar::ClipLibrary::read(&bytes).expect("parses");
        for role in ["Idle_A", "Walk", "Jog", "Sprint"] {
            assert!(
                library.clips.iter().any(|clip| clip.name == role),
                "the shipped archive is missing {role}"
            );
        }
    }

    #[test]
    fn kick_starts_one_build_and_tears_down_when_the_body_stops_being_rigged() {
        bevy::tasks::AsyncComputeTaskPool::get_or_init(Default::default);
        let mut app = test_app();
        let resolved = ResolvedRig {
            body: engine_default_for_did("did:plc:rigged-test"),
            attachments: Vec::new(),
        };
        app.insert_resource(LiveAvatarRecord(rigged_record(resolved)));
        let chassis = app
            .world_mut()
            .spawn((
                LocalPlayer,
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();

        app.world_mut()
            .run_system_once(kick_rigged_builds)
            .expect("runs");
        let build = app.world().get::<RiggedBuild>(chassis);
        assert!(build.is_some(), "a resolved rigged body kicks a build");

        // A second pass must not stack a second task.
        app.world_mut()
            .run_system_once(kick_rigged_builds)
            .expect("runs");
        assert!(app.world().get::<RiggedBuild>(chassis).is_some());

        // Switching the record back to a generator body drops the build and
        // every rigged residue. (The dropped task cancels with it.)
        app.insert_resource(LiveAvatarRecord(AvatarRecord::default_for_seed(7)));
        app.world_mut()
            .run_system_once(kick_rigged_builds)
            .expect("runs");
        assert!(app.world().get::<RiggedBuild>(chassis).is_none());
        assert!(app.world().get::<RiggedApplied>(chassis).is_none());
    }

    #[test]
    fn a_landed_body_hangs_off_an_offset_root_and_drives_to_a_pose() {
        let mut app = test_app();
        let chassis = app
            .world_mut()
            .spawn((Transform::default(), GlobalTransform::default()))
            .id();

        // Built directly at a draft atlas: the test owns the build so it can
        // afford one, and `install_built_body` is exactly what landing runs.
        let engine_record = engine_default_for_did("did:plc:rigged-test");
        let avatar = symbios_avatar::Avatar::build_with(
            &engine_record,
            &symbios_avatar::AvatarConfig {
                atlas: 128,
                ..Default::default()
            },
        )
        .expect("the seeded default engine body builds");
        let offset = 0.9;
        // `Avatar` withholds `Clone` on purpose (megabytes of texture), and a
        // closure system must be `FnMut` — so the one build is taken out of an
        // `Option` on the single run.
        let mut built = Some(avatar);
        app.world_mut()
            .run_system_once(
                move |mut commands: Commands,
                      mut meshes: ResMut<Assets<Mesh>>,
                      mut materials: ResMut<Assets<StandardMaterial>>,
                      mut images: ResMut<Assets<Image>>,
                      mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>| {
                    let Some(avatar) = built.take() else {
                        return;
                    };
                    install_built_body(
                        &mut commands,
                        chassis,
                        offset,
                        avatar,
                        &[],
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        &mut bindposes,
                    );
                },
            )
            .expect("runs");

        let mut roots = app
            .world_mut()
            .query_filtered::<(Entity, &Transform, &ChildOf), With<RiggedRoot>>();
        let (root, transform, child_of) = roots.single(app.world()).expect("one rigged root");
        assert_eq!(child_of.parent(), chassis);
        assert!(
            (transform.translation.y + offset).abs() < 1e-6,
            "the ground plane sits half the collider below the chassis centre"
        );
        assert!(
            !app.world()
                .get::<bevy_symbios_avatar::AvatarJoints>(root)
                .expect("joints spawned")
                .0
                .is_empty()
        );

        // Drive one frame: an empty clip library and zero speed is the Rest
        // source, which must still write a pose (the blink is alive).
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(16));
        app.world_mut()
            .run_system_once(drive_rigged_motion)
            .expect("runs");
        assert!(app.world().get::<AvatarPose>(root).is_some());
        assert!(app.world().get::<AvatarClosure>(root).is_some());
    }
}
