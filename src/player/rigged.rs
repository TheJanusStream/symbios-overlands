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
use symbios_avatar::anim::{contacts_during, plant_feet_of};
use symbios_avatar::{
    Avatar, Blink, Expression, FootingConfig, Gait, Ground, Inertializer, Limb, Pose, PoseClip,
    Stride, Walk,
};

use crate::interaction::locomotion::locomotion_total_height;
use crate::pds::avatar::EngineAvatarRecord;
use crate::player::emote::{Emote, EmoteRequest};
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
/// The shortest gap between two emotes on one body, in seconds (#1068).
///
/// Measured from one gesture's START rather than its end, so the limit is a
/// rate and not a gap: a peer pasting "hi hi hi hi" waves once and then stands
/// there, which is what a room full of people needs it to do. Two seconds is
/// longer than the longest emote in the archive, so a well-behaved sender is
/// never throttled by it.
const EMOTE_COOLDOWN: f32 = 2.0;

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
    /// The emote playing over the locomotion, if any (#1068).
    gesture: Option<ActiveGesture>,
    /// When the last emote *started*, in seconds since app start, for the
    /// per-body rate limit. Kept across the gesture ending, which is the whole
    /// point: the cooldown outlives what it is limiting.
    gestured_at: Option<f32>,
}

/// An emote in progress on one body.
#[derive(Clone, Copy)]
struct ActiveGesture {
    /// Index into [`Clips`].
    clip: usize,
    /// How far into the clip, in seconds. A gesture plays **once**: this counts
    /// up to the clip's duration and then the gesture clears, where a
    /// locomotion cycle wraps.
    elapsed: f32,
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
            gesture: None,
            gestured_at: None,
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
    /// A one-shot emote clip riding over whatever else the body is doing
    /// (#1068), by index into [`Clips`].
    Gesture(usize),
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
    /// One slot per [`Emote`], in [`Emote::ALL`] order (#1068).
    emotes: [Option<usize>; Emote::ALL.len()],
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
    for (slot, emote) in roles.emotes.iter_mut().zip(Emote::ALL) {
        *slot = find(emote.clip_name());
    }
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
        let (locomotion, cadence) = if planar < IDLE_BELOW {
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

        // Advance the emote and retire it at the end of its one play. Done
        // before the pose is built so a gesture that finished this frame does
        // not get one last frame of overlay.
        let gesture = motion.gesture.and_then(|mut active| {
            let duration = clips.0.clips.get(active.clip)?.duration();
            active.elapsed += delta;
            (active.elapsed < duration).then_some(active)
        });
        motion.gesture = gesture;
        // A gesture is its own motion source, so ending one blends back into
        // the walk it was riding rather than snapping.
        let source = match gesture {
            Some(active) => MotionSource::Gesture(active.clip),
            None => locomotion,
        };

        // Build this frame's target pose.
        let mut pose = Pose::rest(rig);
        let mut stance: Vec<Limb> = Vec::new();
        // The floor under this body, in the rigged root's own frame: the root
        // is offset so `y = 0` is the chassis collider's bottom, which on a
        // standing body is the ground the physics chassis rests on. Slopes are
        // carried by the chassis pose, exactly as the collider itself handles
        // them. **One closure, given to both the stride and the plant** — the
        // gait seats its contacts on the same surface the footing solve settles
        // them onto, and handing the two different floors is what put the swing
        // arc through the hill upstream (#1069, symbios-avatar #221).
        let floor = |point: Vec3| Some(Ground::level(Vec3::new(point.x, 0.0, point.z)));
        // Kept past the match so the ankles can roll AFTER the plant: the plant
        // lays every sole flat and a roll applied before it is simply levelled
        // away, which is the order `examples/walkaudit` establishes upstream.
        let mut walking = None;
        // Always the LOCOMOTION pose, never the gesture: an emote is laid over
        // what the body is already doing rather than replacing it, so the legs
        // keep their walk and their contacts (#1068).
        match locomotion {
            MotionSource::Rest => {}
            MotionSource::Gait => {
                let gait = Gait::natural(rig);
                let stride = Stride::for_body(rig, 1.0);
                // The head of the engine's own drive sequence — step, arms,
                // lean — with the footing OFF, because an emote is laid over
                // this pose below and the feet have to be settled after that
                // rather than before (symbios-avatar #253). This file used to
                // spell the stages out and was one of the three consumers that
                // had forgotten the ankles entirely (#1069).
                //
                // The lean is scaled by the stride this body is taking, which
                // here is a CONSTANT: the stride is pinned at pace 1.0 and
                // speed is expressed by bending the cadence instead, so a
                // sprinting avatar leans exactly as far as a strolling one.
                // That belongs to the speed axis rather than to the lean —
                // symbios-avatar #240 is where stride, cadence and gait choice
                // all start coming from one dimensionless speed, and the lean
                // responds for free the moment they do.
                let walked = Walk {
                    footing: None,
                    ..Walk::at(motion.cycle)
                }
                .drive(rig, &mut pose, &gait, &stride, floor);
                stance = walked.steps.stance;
                walking = Some(gait);
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
            // `locomotion` is never a gesture by construction — the gesture is
            // chosen below, from this.
            MotionSource::Gesture(_) => {}
        }
        if let Some(active) = gesture
            && let Some(clip) = clips.0.clips.get(active.clip)
        {
            overlay_gesture(rig, &mut pose, clip, active.elapsed);
        }
        if airborne {
            stance.clear();
        }
        // The tail of the drive: settle the contacts, then roll the ankles, in
        // that order — the engine owns both so this file cannot get the order
        // wrong again (symbios-avatar #253). Gait only, because a clip carries
        // its own ankle motion and rolling on top of authored feet would fight
        // it; a clip's contacts still get planted, they just do not roll.
        match &walking {
            Some(gait) => {
                Walk::at(motion.cycle).settle(rig, &mut pose, gait, &stance, floor);
            }
            None if !stance.is_empty() => {
                plant_feet_of(rig, &mut pose, &stance, floor, &FootingConfig::default());
            }
            None => {}
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

/// Start emotes on the bodies their requests name (#1068).
///
/// Runs in the Animate set ahead of [`drive_rigged_motion`], so a gesture
/// requested this frame is posed this frame rather than a frame late.
///
/// **A request names a chassis and this finds the body under it**, because the
/// chat and network layers hold chassis entities and know nothing of rigged
/// roots. Three ways a request is dropped, all silent and all intended: the
/// chassis has no rigged body (a boat has nothing to wave with), the archive
/// has no clip for that emote, or the body is inside its cooldown.
pub(super) fn start_emotes(
    mut requests: MessageReader<EmoteRequest>,
    time: Res<Time>,
    clips: Res<Clips>,
    roles: Res<ClipRoles>,
    mut bodies: Query<(&ChildOf, &mut RiggedMotion), With<RiggedRoot>>,
) {
    let now = time.elapsed_secs();
    for request in requests.read() {
        let slot = Emote::ALL
            .iter()
            .position(|emote| *emote == request.emote)
            .and_then(|index| roles.emotes[index]);
        let Some(slot) = slot else {
            continue;
        };
        // A zero-length clip would divide the overlay by nothing and hold the
        // gesture forever; the locomotion roles filter the same way.
        if clips
            .0
            .clips
            .get(slot)
            .is_none_or(|clip| clip.duration() <= 0.0)
        {
            continue;
        }
        for (child_of, mut motion) in &mut bodies {
            if child_of.parent() != request.chassis {
                continue;
            }
            if motion
                .gestured_at
                .is_some_and(|last| now - last < EMOTE_COOLDOWN)
            {
                continue;
            }
            motion.gesture = Some(ActiveGesture {
                clip: slot,
                elapsed: 0.0,
            });
            motion.gestured_at = Some(now);
        }
    }
}

/// Lay an emote clip over a pose the locomotion layer already built (#1068).
///
/// **The legs and the pelvis are left exactly as they were, and that is the
/// whole design.** A gesture has to be playable while the body is walking —
/// people wave as they arrive, not only once they have stopped — and a clip
/// applied wholesale would replace the walk with a standing wave and slide the
/// feet through the ground for its duration. So the gesture owns the spine, the
/// arms and the head, and the locomotion layer keeps whatever carries the body.
///
/// The pelvis is on the locomotion side too, which costs a bow some depth and
/// buys correctness: rotating the root swings both feet through the floor, and
/// on an idle body no contact is planted, so nothing downstream would put them
/// back. A bow bends at the spine here. When symbios-avatar #248 re-authors
/// these as goal-space clips the constraint can be revisited, because a goal is
/// solved against the ground rather than replayed at it.
///
/// The root translation is dropped for the same reason
/// [`drive_rigged_motion`] drops a locomotion clip's: the chassis owns where
/// the body is.
fn overlay_gesture(rig: &symbios_avatar::Rig, pose: &mut Pose, clip: &PoseClip, at: f32) {
    let mut gesture = Pose::rest(rig);
    clip.apply(rig, &mut gesture, at);
    if !gesture.fits(rig) || !pose.fits(rig) {
        return;
    }
    for joint in 0..pose.rotations.len() {
        if !carries_the_body(rig, joint) {
            pose.rotations[joint] = gesture.rotations[joint];
        }
    }
}

/// Whether a joint is part of what holds the body up, and so off limits to an
/// emote overlay.
///
/// Asked of the rig rather than assumed from anatomy — `ground_contacts` is the
/// same question `gait::swing_arms` asks to decide which limbs are legs, so a
/// body plan nobody has written yet answers it correctly too.
fn carries_the_body(rig: &symbios_avatar::Rig, joint: usize) -> bool {
    let zone = rig.joints[joint].zone;
    if zone == symbios_avatar::Zone::Pelvis {
        return true;
    }
    let carries = rig.ground_contacts();
    match zone {
        symbios_avatar::Zone::UpperLimb(limb)
        | symbios_avatar::Zone::LowerLimb(limb)
        | symbios_avatar::Zone::Extremity(limb) => carries.contains(&limb),
        _ => false,
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
    use symbios_avatar::anim::gait;

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
        app.add_message::<crate::player::emote::EmoteRequest>();
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

    /// Build the app with the SHIPPED archive loaded and its roles indexed,
    /// which is what an emote needs to resolve a clip at all.
    fn app_with_clips() -> App {
        let mut app = test_app();
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/avatar.clips"))
            .expect("assets/avatar.clips ships with the app");
        let library = symbios_avatar::ClipLibrary::read(&bytes).expect("parses");
        app.insert_resource(Clips(library));
        app.world_mut()
            .run_system_once(index_clip_roles)
            .expect("the role indexer runs");
        app
    }

    /// A chassis with a rigged root under it, carrying motion state.
    fn chassis_with_body(app: &mut App) -> (Entity, Entity) {
        let chassis = app
            .world_mut()
            .spawn((Transform::default(), GlobalTransform::default()))
            .id();
        let root = app
            .world_mut()
            .spawn((RiggedRoot, RiggedMotion::default(), ChildOf(chassis)))
            .id();
        (chassis, root)
    }

    /// How far a foot's sole is pitched from its own rest attitude, in degrees,
    /// positive toe-up.
    ///
    /// Mirrors the engine's own `sole_pitch`: measured on the POSED body
    /// between the rearmost and foremost joints past the ankle, and referenced
    /// to the rest attitude rather than to level, because this body's foot
    /// nodes run a few degrees uphill at rest and 0 has to mean "carried as it
    /// stands".
    fn sole_pitch(rig: &symbios_avatar::Rig, pose: &Pose, limb: Limb) -> f32 {
        let joints = rig.extremity_joints(limb);
        let sole = &joints[1..];
        let along = |&joint: &usize| rig.joints[joint].position.z;
        let rear = *sole
            .iter()
            .min_by(|a, b| along(a).total_cmp(&along(b)))
            .expect("a foot");
        let fore = *sole
            .iter()
            .max_by(|a, b| along(a).total_cmp(&along(b)))
            .expect("a foot");
        let angle = |run: Vec3| {
            run.y
                .atan2((run.x * run.x + run.z * run.z).sqrt())
                .to_degrees()
        };
        let posed = pose.forward(rig);
        angle(posed.positions[fore] - posed.positions[rear])
            - angle(rig.joints[fore].position - rig.joints[rear].position)
    }

    #[test]
    fn the_procedural_walk_lands_toe_up_and_leaves_toe_down() {
        // **#1069.** This drove `gait::step` and `gait::swing_arms` and stopped,
        // never `gait::roll_feet` — so every procedurally-driven body walked
        // with its soles held at their rest attitude: no heel-strike, no
        // toe-off, the whole foot tilting with the shin at full stride. The
        // engine treats the three as one drive sequence and `examples/walkaudit`
        // has always called all three; this had two of them.
        //
        // **Driven through `drive_rigged_motion` and read off the `AvatarPose`
        // the body is actually drawn in.** Written first as a loop that called
        // step/swing_arms/plant/roll itself and asserted on that — which proves
        // nothing about this file, because deleting the roll from the system
        // under test leaves such a test passing on its own copy of the
        // sequence. A test that reimplements its subject measures its own
        // arithmetic.
        //
        // The defect was invisible in the app for a separate reason worth
        // recording: the gait only drives a body when the clip library is
        // empty, which on native never happens. It would have appeared the
        // moment #1067 removed the clips, looking like a regression the removal
        // caused. `test_app` ships an empty library, which is exactly the wasm
        // pre-fetch case and the post-#1067 case.
        let mut app = test_app();
        let chassis = app
            .world_mut()
            .spawn((Transform::default(), GlobalTransform::default()))
            .id();
        let avatar = symbios_avatar::Avatar::build_with(
            &engine_default_for_did("did:plc:ankle-test"),
            &symbios_avatar::AvatarConfig {
                atlas: 64,
                ..Default::default()
            },
        )
        .expect("the seeded default engine body builds");
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
                        0.9,
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
        let mut roots = app.world_mut().query_filtered::<Entity, With<RiggedRoot>>();
        let root = roots.single(app.world()).expect("one rigged root");
        let rig = app
            .world()
            .get::<BuiltBody>(root)
            .expect("the body landed")
            .avatar
            .rig
            .clone();

        // Walk the chassis forward. No `LinearVelocity` and no transform
        // propagation here, so the speed the driver reads is the one this moves
        // the `GlobalTransform` by — which is the remote-peer path, and enough
        // to select the gait.
        const STEP_SECS: f32 = 1.0 / 60.0;
        const PACE: f32 = 1.3;
        let (mut lowest, mut highest) = (f32::MAX, f32::MIN);
        for frame in 0..150 {
            let at = Vec3::Z * (PACE * STEP_SECS * frame as f32);
            let mut chassis_mut = app.world_mut().entity_mut(chassis);
            *chassis_mut.get_mut::<Transform>().unwrap() = Transform::from_translation(at);
            *chassis_mut.get_mut::<GlobalTransform>().unwrap() =
                GlobalTransform::from_translation(at);
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs_f32(STEP_SECS));
            app.world_mut()
                .run_system_once(drive_rigged_motion)
                .expect("runs");

            let motion = app.world().get::<RiggedMotion>(root).expect("motion state");
            // Only once the gait is actually the source: the first frame has no
            // previous position, so it reads as standing.
            if motion.source != MotionSource::Gait {
                continue;
            }
            let pose = &app.world().get::<AvatarPose>(root).expect("a pose").0;
            for limb in [Limb::HindLeft, Limb::HindRight] {
                let pitch = sole_pitch(&rig, pose, limb);
                lowest = lowest.min(pitch);
                highest = highest.max(pitch);
            }
        }
        assert!(
            lowest < f32::MAX,
            "the gait never drove the body — the test never measured anything"
        );

        // The literature's bands, which the engine's constants are set against:
        // heel-strike ~15-25 degrees toe-up, push-off ~15-20 toe-down. Asserted
        // loosely, because what is guarded here is that the stage RUNS — a sole
        // held flat all cycle reads -0.0 to 0.0 and is a shuffle. What actually
        // arrives through this path is -17.2 to 20.1 degrees, which is
        // `examples/walkaudit`'s own reading upstream: the app is now driving
        // the gait the engine's instrument measures.
        assert!(
            highest > 10.0,
            "the foot never landed toe-up: peak pitch {highest:.1} deg — roll_feet is not running"
        );
        assert!(
            lowest < -10.0,
            "the foot never left toe-down: lowest pitch {lowest:.1} deg — roll_feet is not running"
        );
    }

    #[test]
    fn a_chat_keyword_gestures_the_sender_and_nobody_else() {
        // #1068. The request names a chassis; every OTHER body in the room has
        // to stay still, which is the property that makes this readable as
        // "that person waved" rather than as the room twitching.
        let mut app = app_with_clips();
        let (sender, sender_body) = chassis_with_body(&mut app);
        let (_bystander, bystander_body) = chassis_with_body(&mut app);

        let request = crate::player::emote::request_for(sender, "hello everyone")
            .expect("\"hello\" asks for a greeting");
        app.world_mut().write_message(request);
        app.world_mut()
            .run_system_once(start_emotes)
            .expect("start_emotes runs");

        let gestured = |app: &App, body: Entity| {
            app.world()
                .entity(body)
                .get::<RiggedMotion>()
                .expect("the body keeps its motion state")
                .gesture
                .is_some()
        };
        assert!(gestured(&app, sender_body), "the sender should gesture");
        assert!(
            !gestured(&app, bystander_body),
            "a body the request did not name must not gesture"
        );
    }

    #[test]
    fn a_flood_of_keywords_gestures_once() {
        // The rate limit, and the reason it is measured from the START of a
        // gesture: a peer pasting "hi hi hi" must wave once and then stand
        // there. Both requests are delivered in one run, so this also covers
        // two keywords arriving in the same frame.
        let mut app = app_with_clips();
        let (chassis, body) = chassis_with_body(&mut app);

        for text in ["hi", "hello again", "hey"] {
            let request = crate::player::emote::request_for(chassis, text).expect("asks");
            app.world_mut().write_message(request);
        }
        app.world_mut()
            .run_system_once(start_emotes)
            .expect("start_emotes runs");

        // Advance the gesture as playback would, and assert the flood does not
        // rewind it. **Asserted on `elapsed` rather than on `gestured_at`,
        // because the clock does not move under `run_system_once`** — the
        // stamp is identical whether the cooldown ran or not, so the first
        // version of this test passed with the guard deleted. Playback
        // progress is the one thing a restart cannot fake.
        {
            let mut motion = app.world_mut().entity_mut(body);
            let mut motion = motion.get_mut::<RiggedMotion>().unwrap();
            assert!(motion.gesture.is_some(), "the first keyword should gesture");
            motion.gesture.as_mut().unwrap().elapsed = 0.4;
        }

        for text in ["hi", "hey"] {
            let request = crate::player::emote::request_for(chassis, text).expect("asks");
            app.world_mut().write_message(request);
        }
        app.world_mut()
            .run_system_once(start_emotes)
            .expect("start_emotes runs");

        let motion = app.world().entity(body).get::<RiggedMotion>().unwrap();
        let gesture = motion.gesture.expect("the gesture is still running");
        assert_eq!(
            gesture.elapsed, 0.4,
            "a keyword inside the cooldown restarted the gesture from the top"
        );
    }

    #[test]
    fn a_gesture_leaves_the_legs_to_the_locomotion_layer() {
        // **The load-bearing claim of the overlay** (#1068): an emote plays
        // over a walk rather than replacing it, so the joints that carry the
        // body must come out of `overlay_gesture` untouched while the upper
        // body takes the clip. Without this a wave while walking would stand
        // the body still and slide its feet along the ground.
        let avatar = symbios_avatar::Avatar::build_with(
            &engine_default_for_did("did:plc:emote-test"),
            &symbios_avatar::AvatarConfig {
                atlas: 64,
                ..Default::default()
            },
        )
        .expect("the default body builds");
        let rig = &avatar.rig;

        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/avatar.clips"))
            .expect("assets/avatar.clips ships with the app");
        let library = symbios_avatar::ClipLibrary::read(&bytes).expect("parses");
        let clip = library
            .clips
            .iter()
            .find(|clip| clip.name == Emote::Greeting.clip_name())
            .expect("the archive carries a Greeting");

        // A walk pose, so the legs hold something a gesture could destroy.
        let mut walking = Pose::rest(rig);
        let gait = Gait::natural(rig);
        let stride = Stride::for_body(rig, 1.0);
        gait::step(rig, &mut walking, &gait, &stride, 0.25, |_| None);
        gait::swing_arms(rig, &mut walking, &gait, 0.25);

        let mut posed = walking.clone();
        overlay_gesture(rig, &mut posed, clip, clip.duration() * 0.5);

        // Compared by DOT rather than by `Quat::angle_between`, which is
        // `acos` of the dot and so loses all its precision exactly where this
        // test looks: acos(0.9999999) is already 3.4e-4, so two bit-identical
        // rotations read as a third of a milliradian apart and every carrying
        // joint failed.
        let apart = |a: Quat, b: Quat| 1.0 - a.dot(b).abs();
        // **Judged against the zones directly, NOT through `carries_the_body`.**
        // Written the other way first, and reintroducing the bug proved it
        // worthless: the test asked the function under test which joints to
        // check, so inverting that function's pelvis arm moved the pelvis AND
        // excused itself from noticing. An instrument that takes its
        // expectations from its subject measures nothing.
        let carried: Vec<symbios_avatar::Zone> = rig
            .ground_contacts()
            .into_iter()
            .flat_map(|limb| {
                [
                    symbios_avatar::Zone::UpperLimb(limb),
                    symbios_avatar::Zone::LowerLimb(limb),
                    symbios_avatar::Zone::Extremity(limb),
                ]
            })
            .chain([symbios_avatar::Zone::Pelvis])
            .collect();
        assert!(
            carried.len() > 3,
            "a biped should carry itself on more than {} zones",
            carried.len()
        );
        let mut upper_moved = 0;
        for joint in 0..posed.rotations.len() {
            if carried.contains(&rig.joints[joint].zone) {
                assert_eq!(
                    posed.rotations[joint], walking.rotations[joint],
                    "joint {joint} ({:?}) carries the body and the gesture moved it",
                    rig.joints[joint].zone
                );
            } else if apart(posed.rotations[joint], walking.rotations[joint]) > 1e-6 {
                upper_moved += 1;
            }
        }
        assert!(
            upper_moved > 0,
            "the gesture changed nothing at all — the overlay is not applying"
        );
    }

    #[test]
    fn the_shipped_clip_archive_carries_every_emote() {
        // The same guard the locomotion roles carry, from the playback side:
        // `index_clip_roles` must resolve a slot for every emote, or a keyword
        // is silently inert at runtime.
        let app = app_with_clips();
        let roles = app.world().resource::<ClipRoles>();
        for (slot, emote) in roles.emotes.iter().zip(Emote::ALL) {
            assert!(
                slot.is_some(),
                "{emote:?} resolved no clip from the shipped archive"
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

    #[test]
    fn a_chat_keyword_changes_the_pose_the_body_is_actually_drawn_in() {
        // **End to end through the real systems** (#1068), because every other
        // test here proves a piece: the keyword scan, the targeting, the
        // cooldown and the overlay arithmetic each pass on their own while the
        // wiring between them could still be wrong. This is the one that fails
        // if `drive_rigged_motion` never reaches the gesture branch — the exact
        // defect that would otherwise only show up in the running app.
        let mut app = app_with_clips();
        let chassis = app
            .world_mut()
            .spawn((Transform::default(), GlobalTransform::default()))
            .id();
        let avatar = symbios_avatar::Avatar::build_with(
            &engine_default_for_did("did:plc:emote-drive-test"),
            &symbios_avatar::AvatarConfig {
                atlas: 128,
                ..Default::default()
            },
        )
        .expect("the seeded default engine body builds");
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
                        0.9,
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
        let mut roots = app.world_mut().query_filtered::<Entity, With<RiggedRoot>>();
        let root = roots.single(app.world()).expect("one rigged root");

        // A frame with no keyword: the idle baseline this is measured against.
        let frame = |app: &mut App| {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_millis(16));
            app.world_mut().run_system_once(start_emotes).expect("runs");
            app.world_mut()
                .run_system_once(drive_rigged_motion)
                .expect("runs");
            app.world()
                .get::<AvatarPose>(root)
                .expect("a pose")
                .0
                .clone()
        };
        let idle = frame(&mut app);

        // Now say hello, through the same helper the chat and network layers
        // call — not by poking `RiggedMotion` directly, which would skip the
        // half of the path most likely to be miswired.
        let request = crate::player::emote::request_for(chassis, "hello!")
            .expect("\"hello\" asks for a greeting");
        app.world_mut().write_message(request);
        let waving = frame(&mut app);

        assert!(
            app.world()
                .get::<RiggedMotion>(root)
                .expect("motion state")
                .gesture
                .is_some(),
            "the greeting never started"
        );
        let apart = |a: Quat, b: Quat| 1.0 - a.dot(b).abs();
        let moved = (0..idle.rotations.len())
            .filter(|joint| apart(idle.rotations[*joint], waving.rotations[*joint]) > 1e-6)
            .count();
        assert!(
            moved > 0,
            "the body was drawn in the same pose with and without the greeting"
        );
    }
}
