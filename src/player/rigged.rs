//! Rigged-body spawn and procedural locomotion (#1057, epic #1054; clips
//! removed under #1067, the overlands half of symbios-avatar epic #237).
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
//!     is actually doing, through the engine's procedural layer and nothing
//!     else: the gait on the speed axis when the chassis travels (one
//!     dimensionless speed decides stride, cadence, duty and the walk-run
//!     boundary — symbios-avatar #240, adopted under #1070), the idle when
//!     it stands (breath, sway, weight shift and fidgets — engine #246),
//!     goal-space gestures over either (#1068), inertialized source
//!     switches, stance feet planted on the local ground plane, and
//!     engine-driven blinking.
//!
//! There is no clip library, no clip fetch and no play-rate arithmetic left
//! anywhere in this path (#1067): a generator needs no reference speed to
//! apologise to, so the anti-slide clamps went with the clips. The engine
//! keeps its baked archive as a dev-only comparison target (symbios-avatar
//! #249); overlands never ships or downloads it.
//!
//! The procedural [`super::gait`] layer never touches these bodies: it
//! animates [`crate::world_builder::AvatarVisualRoot`], which only the
//! generator spawn path inserts.

use avian3d::prelude::LinearVelocity;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use bevy_symbios_avatar::{AvatarBody as BuiltBody, AvatarClosure, AvatarPose, spawn_avatar};
use symbios_avatar::anim::{Stage, gesture, transition};
use symbios_avatar::{
    Avatar, Blink, Expression, Ground, Idle, Inertializer, Leap, Limb, Pose, Speed, Swim, Walk,
};

use crate::interaction::locomotion::locomotion_total_height;
use crate::pds::avatar::EngineAvatarRecord;
use crate::player::emote::{Emote, EmoteRequest};
use crate::player::humanoid::{WaterState, humanoid_water_state};
use crate::state::{LiveAvatarRecord, LocalPlayer, RemotePeer};
use crate::water::WaterSurfaces;

/// Below this horizontal speed the body idles.
const IDLE_BELOW: f32 = 0.3;
/// Upward speed that can only be a launch, in m/s.
///
/// Nothing a body does on the ground pushes it up this fast — the humanoid
/// preset's own jump is 450 N·s on 80 kg, which is 5.6 — and a walk on rough
/// ground never approaches it. Deliberately well clear of both, because the
/// cost of missing a launch is one frame of walk cycle and the cost of a false
/// one is a body that tucks its legs while standing on a kerb.
const LAUNCH_SPEED: f32 = 2.0;

/// Downward speed past which a body has left the ground rather than walked
/// down something, in m/s.
///
/// The other way into the air: a body that steps off a ledge never launches,
/// and reaches this within about a fifth of a second of having nothing under
/// it.
const FALL_SPEED: f32 = 2.0;

/// How far one stroke of a crawl carries the body, in its own lengths.
///
/// **One, and both ends of the stroke's clock are derived from it and from
/// the engine.** `Swim` leaves the cadence to its caller on purpose — a tread
/// and a crawl run the same loops and differ in how fast the caller advances
/// them — so this is where the seconds are decided. A body covering its own
/// length per cycle at the engine's own full effort ([`symbios_avatar::anim::swim::PRONE_AT`],
/// 0.7 lengths a second) is stroking 0.7 times a second, which sits inside the
/// 0.5 to 0.83 that competitive swimmers hold.
const LENGTHS_PER_STROKE: f32 = 1.0;

/// Vertical speed below which a body counts as no longer falling, in m/s.
///
/// **The landing edge, and the reason airborne has to be a state.** No
/// instantaneous test can find it: at the apex of a jump the vertical speed is
/// zero, which is the most airborne a body ever is. What cannot be confused
/// with the apex is a body that HAS been falling and has stopped, because at
/// the apex it is still on its way down.
const SETTLE_SPEED: f32 = 0.5;
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
/// How long an emote takes, in seconds.
///
/// The engine's gestures are written in normalised time — a goal-space clip
/// runs `0..1` and says nothing about seconds — so this is the only place the
/// real duration is decided, and it is the sibling viewer's own figure: a
/// second and a half is a greeting, long enough for three waves to read as
/// waves and short enough that a body is not still doing it when the
/// conversation has moved on.
const EMOTE_SECS: f32 = 1.5;

/// Atlas side used while the record is still moving under an editor. The
/// sibling viewer's own draft rung: 68 ms a build against 277 at full size.
const DRAFT_ATLAS: u32 = 256;
/// How long the record must be still before the full-atlas build is owed.
const SETTLE_SECS: f32 = 0.8;

/// When the resolved record under this chassis last differed from the body
/// standing on it — the settle ladder's clock (#1059).
///
/// A component rather than the `Local<HashMap<Entity, f32>>` it used to be
/// (#1135). That map was insert-only: nothing pruned it when a chassis
/// despawned, so a session that met peers accumulated an entry per peer for
/// the life of the process. Hanging the timestamp on the chassis makes the
/// despawn the cleanup, which is the property the map could never have.
#[derive(Component)]
pub(super) struct RiggedSettle {
    changed_at: f32,
}

/// Latch marking a chassis as reconciled: the right record is standing, at
/// the full atlas, under a live root (#1135).
///
/// Its presence is what lets [`kick_rigged_builds`] skip the per-frame
/// `AvatarRecord` deep compare. Deliberately a latch and not a change tick —
/// see the gate's own comment for why a `Changed<>` gate would drop a record
/// edit that arrives while a build is in flight.
#[derive(Component)]
pub(super) struct RiggedSteady;

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
    /// When the build was kicked, in seconds since app start (#1078): the
    /// land reports kick-to-land wall time, which is how long this chassis
    /// stands as a naked capsule.
    kicked_at: f64,
    task: bevy::tasks::Task<crate::offload::GenResult>,
}

/// The one child of the chassis the skinned body hangs off. Deliberately not
/// [`crate::world_builder::AvatarVisualRoot`], so the procedural gait layer
/// cannot see it.
#[derive(Component)]
pub(crate) struct RiggedRoot;

/// What the body was doing last frame, for cycle continuity and blends.
#[derive(Component)]
pub(super) struct RiggedMotion {
    cycle: f32,
    source: MotionSource,
    last_position: Option<Vec3>,
    /// The speed the gait was built from last frame, or `None` when the body
    /// was not travelling.
    ///
    /// Kept so [`transition::carry_cycle`] has a `from` gait to map out of
    /// (#1071). A [`Speed`] is one dimensionless number, so remembering the
    /// speed rather than the gait keeps this component a value and rebuilds
    /// the gait only on the frames a change of duty actually needs one.
    gaiting: Option<Speed>,
    /// The leap in progress, or `None` while the body is on the ground
    /// (#1072).
    airborne: Option<Airborne>,

    previous: Option<Pose>,
    current: Option<Pose>,
    transition: Option<Inertializer>,
    blink: Blink,
    /// The engine's idle driver: breath, sway, weight shift and fidget
    /// scheduling for a body that is standing (engine #246). Stateful — it
    /// owns its own clocks and its fidget schedule — and paused rather than
    /// advanced while something else carries the body, so a body that stops
    /// walking resumes its own idle rather than a reset one.
    idler: Idle,
    /// The emote playing over the locomotion, if any (#1068).
    gesture: Option<ActiveGesture>,
    /// When the last emote *started*, in seconds since app start, for the
    /// per-body rate limit. Kept across the gesture ending, which is the whole
    /// point: the cooldown outlives what it is limiting.
    gestured_at: Option<f32>,
}

/// A body that is off the ground, and the leap describing it (#1072).
///
/// **Built twice, and that is the shape of the problem rather than a
/// hesitation.** The engine's [`Leap`] wants to know at the start how far it
/// will fall, because the landing's depth and the flight's duration both come
/// off it; here the physics chassis decides that after the fact. So this
/// carries the leap the takeoff implied while the body is rising, and is
/// replaced at touchdown by one built from the impact that actually arrived —
/// which is the number the landing needs and the only one it needs.
#[derive(Clone, Copy)]
struct Airborne {
    /// The leap being driven.
    leap: Leap,
    /// Seconds into it, on [`Leap`]'s own clock.
    elapsed: f32,
    /// Whether the body has begun falling, which is what makes the landing
    /// edge findable — see [`SETTLE_SPEED`].
    falling: bool,
    /// The fastest the body has been travelling downward, in m/s, which at
    /// touchdown is the impact the legs have to absorb.
    impact: f32,
    /// Whether the feet are back on the ground and this is playing out the
    /// landing.
    landed: bool,
}

/// An emote in progress on one body.
#[derive(Clone, Copy)]
struct ActiveGesture {
    /// Which emote is playing. The clip itself is rebuilt from
    /// [`gesture::by_name`] each frame — a goal-space clip is a few keyed
    /// goals, so rebuilding costs less than the solve that follows it and
    /// keeps this state a value rather than a cache.
    emote: Emote,
    /// How far into the gesture, in seconds. A gesture plays **once**: this
    /// counts up to [`EMOTE_SECS`] and then the gesture clears, where a
    /// locomotion cycle wraps.
    elapsed: f32,
}

impl Default for RiggedMotion {
    fn default() -> Self {
        // Each body seeds its own idle and blink off a process-wide counter,
        // because these are the clocks a ROOM is judged on: seeded alike,
        // every body in it breathes, shifts its weight and blinks in unison,
        // which reads as a drill team rather than a crowd.
        static SEED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(7);
        let seed = SEED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self {
            cycle: 0.0,
            source: MotionSource::Rest,
            last_position: None,
            gaiting: None,
            airborne: None,
            previous: None,
            current: None,
            transition: None,
            blink: Blink::seeded(seed),
            idler: Idle::seeded(seed),
            gesture: None,
            gestured_at: None,
        }
    }
}

/// What is carrying the body this frame. A change starts a blend.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MotionSource {
    /// Pinned to the bind pose: the attachment-editing hold, and the state a
    /// body is born in. Kept distinct from [`Self::Idle`] so releasing the
    /// hold reads as a source change and blends back out instead of snapping.
    Rest,
    /// Standing: the engine's idle — breath, sway, weight shift, fidgets.
    Idle,
    /// Travelling: the engine's procedural gait on the speed axis.
    Gait,
    /// Off the ground on purpose or otherwise, and landing again (#1072).
    Leap,
    /// In deep water: treading, or crawling if travelling (#1074).
    Swim,
    /// A one-shot emote riding over whatever else the body is doing (#1068).
    Gesture(Emote),
}

impl MotionSource {
    /// Which of the engine's motion families this is, or `None` for the hold.
    ///
    /// **The blend question, asked the engine's way** (#1071, engine #247). A
    /// blend joins two activities that share no clock; it is *not* for a body
    /// moving along one axis, where the pose is already continuous and
    /// inertializing it against itself only adds a decaying error to a motion
    /// that had none. [`transition::Family::needs_blend`] is where that rule
    /// lives.
    ///
    /// Every speed is one family, which is the whole point of the speed axis:
    /// a walk becoming a run is a change of duty inside `Locomotion`, and the
    /// discontinuity that really is there at that moment is the CLOCK's, fixed
    /// by [`transition::carry_cycle`] rather than smeared by a blend.
    ///
    /// [`Self::Rest`] has no family and that is not an omission. It is a held
    /// pose rather than a generator — the attachment-editing pin — so a change
    /// into or out of it is a pose swap with no clock to be continuous with,
    /// and it always blends. Answering `Family::Idle` instead would silently
    /// stop the release from blending, and the idle it releases into is up to
    /// a sway's amplitude away from the bind pose it was pinned at.
    fn family(self) -> Option<transition::Family> {
        match self {
            MotionSource::Rest => None,
            MotionSource::Idle => Some(transition::Family::Idle),
            MotionSource::Gait => Some(transition::Family::Locomotion),
            MotionSource::Leap => Some(transition::Family::Jump),
            MotionSource::Swim => Some(transition::Family::Swim),
            MotionSource::Gesture(_) => Some(transition::Family::Expressive),
        }
    }

    /// Whether moving from `self` to `into` needs a blend at all.
    fn needs_blend(self, into: Self) -> bool {
        match (self.family(), into.family()) {
            (Some(from), Some(into)) => from.needs_blend(into),
            // A hold at either end: always, for the reason [`Self::family`]
            // gives.
            _ => self != into,
        }
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
    peers: Query<(Entity, Ref<RemotePeer>)>,
    applied: Query<&RiggedApplied>,
    building: Query<&RiggedBuild>,
    steady: Query<(), With<RiggedSteady>>,
    settle: Query<&RiggedSettle>,
    roots: Query<(Entity, &ChildOf), With<RiggedRoot>>,
) {
    let now = time.elapsed_secs();
    // One pass over the roots instead of one per body (#1135). The inner scan
    // was `O(bodies × roots)` every frame, and both terms are the peer count
    // — so the cost of standing in a room grew with its square.
    let chassis_with_root: bevy::platform::collections::HashSet<Entity> = roots
        .iter()
        .map(|(_, child_of)| child_of.parent())
        .collect();
    let full_atlas = symbios_avatar::AvatarConfig::default().atlas;

    let mut visit = |chassis: Entity,
                     record: Option<&crate::pds::AvatarRecord>,
                     source_changed: bool| {
        // The gate that makes standing still free (#1135).
        //
        // Everything below this — a full `AvatarRecord` deep-equality against
        // the body that is standing, per body, every frame — used to run for
        // thousands of consecutive frames to conclude "unchanged". It can be
        // skipped only when nothing that feeds it can have moved, and that is
        // three conditions, not one:
        //
        //   * the record this chassis draws from has not changed since the
        //     last look. A bare `Changed<>` gate would stop here and be
        //     WRONG, because the record can change while a build is in flight
        //     — the change is noticed, no build is kicked (one at a time per
        //     chassis), and it is the NEXT frame's mismatch that kicks the
        //     newer one. `RiggedSteady` is therefore a latch, not a tick: set
        //     only once the chassis is genuinely reconciled, and cleared by
        //     any change, so a change noticed mid-flight stays noticed.
        //   * the standing body was built at the FULL atlas. This is what
        //     keeps the settle ladder (#1059) working: a draft-atlas body is
        //     owed a full-atlas rebuild on a TIMER with no record change
        //     behind it, so while one is owed the answer really can change
        //     with nothing but the clock, and the ladder has to keep being
        //     re-evaluated. At the full atlas there is no rung above.
        //   * a root is actually standing, and no build is in flight.
        if source_changed {
            commands.entity(chassis).remove::<RiggedSteady>();
        } else if steady.contains(chassis)
            && chassis_with_root.contains(&chassis)
            && !building.contains(chassis)
            && applied.get(chassis).is_ok_and(|b| b.atlas >= full_atlas)
        {
            return;
        }

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
                let has_root = chassis_with_root.contains(&chassis);
                let built = applied.get(chassis).ok();
                let same_record = built.is_some_and(|built| built.record == resolved.body);
                // The draft/settle ladder (#1059): while a record is moving —
                // an editor slider mid-drag, a stream of peer previews — a
                // build is only worth the draft atlas, because the next edit
                // obsoletes it; once it has been still for SETTLE_SECS the
                // full-atlas build is owed, even though nothing changed.
                if !same_record {
                    commands
                        .entity(chassis)
                        .insert(RiggedSettle { changed_at: now });
                }
                let settled = settle
                    .get(chassis)
                    .ok()
                    .is_none_or(|s| now - s.changed_at >= SETTLE_SECS);
                let atlas = if settled { full_atlas } else { DRAFT_ATLAS };
                let atlas_owed = built.is_some_and(|built| built.atlas < atlas);
                if same_record && has_root && !atlas_owed {
                    // Reconciled: latch it so the compare above is skipped
                    // until something clears the latch.
                    commands.entity(chassis).insert(RiggedSteady);
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
                    kicked_at: now as f64,
                    task,
                });
            }
            None => {
                // Not rigged (or not resolved): the generator path owns this
                // chassis. Drop any rigged residue so switching back later
                // rebuilds from scratch.
                if applied.contains(chassis) || building.contains(chassis) {
                    commands.entity(chassis).remove::<(
                        RiggedApplied,
                        RiggedBuild,
                        RiggedSteady,
                        RiggedSettle,
                    )>();
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
        let changed = live.is_changed();
        for chassis in &locals {
            visit(chassis, Some(&live.0), changed);
        }
    }
    for (chassis, peer) in &peers {
        let changed = peer.is_changed();
        visit(chassis, peer.avatar.as_ref(), changed);
    }
}

/// Land finished builds: swap the skinned body in under its offset root.
#[allow(clippy::too_many_arguments)]
pub(super) fn land_rigged_builds(
    mut commands: Commands,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    mut builds: Query<(Entity, &mut RiggedBuild)>,
    roots: Query<(Entity, &ChildOf), With<RiggedRoot>>,
    // Both optional, because headless embedders (the render tool, minimal
    // test worlds) run this without the diagnostics plugin.
    mut metrics: Option<ResMut<crate::diagnostics::MetricsRegistry>>,
    mut session_log: Option<ResMut<crate::diagnostics::SessionLog>>,
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
        // How long the chassis stood bodiless, and whether it got one (#1078).
        // Reported before the failure branch so a doomed record is visible in
        // the timeline rather than only in the warn.
        let waited = (time.elapsed_secs_f64() - build.kicked_at).max(0.0);
        if let Some(metrics) = metrics.as_deref_mut() {
            crate::diagnostics::samplers::rigged_build_secs(metrics, waited);
            if result.is_none() {
                crate::diagnostics::samplers::rigged_build_failed(metrics);
            }
        }
        if let Some(log) = session_log.as_deref_mut() {
            log.info(
                time.elapsed_secs_f64(),
                crate::diagnostics::event::EventPayload::RiggedBuildCompleted {
                    atlas: build.atlas,
                    duration_secs: waited,
                    ok: result.is_some(),
                },
            );
        }
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
pub(super) fn install_built_body(
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
/// **The attachment-editing holds (#1062, #1106).** An attachment offset is
/// stored in its carrying joint's *rest* frame, so while the owner has the
/// in-world gizmo on a **whole worn prop** their own body is pinned to the
/// bind pose and the drag happens in the frame the record actually keeps.
/// The pin is a hard snap, not an inertial blend: a body still settling
/// would let a gizmo release land against a pose that is already gone.
///
/// A gizmo on a **part** of a worn item holds the body **where it stands**
/// instead: the part is detached at its current world pose and committed
/// back against its parent's pose, which may be any pose so long as it does
/// not move. Re-posing to rest here moved the parent out from under the
/// freshly detached part — selecting visibly shifted it (#1106). The pose
/// hold is a plain skip: nothing is inserted, so the last `AvatarPose`
/// stays applied (the joint writer runs on `Changed<AvatarPose>` only) and
/// the motion state resumes from exactly where it paused. Peers are never
/// held; neither is the local body outside those editor states.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn drive_rigged_motion(
    mut commands: Commands,
    time: Res<Time>,
    water: Option<Res<WaterSurfaces>>,
    mut bodies: Query<
        (Entity, &ChildOf, &BuiltBody, &Transform, &mut RiggedMotion),
        With<RiggedRoot>,
    >,
    chassis: Query<(&GlobalTransform, Option<&LinearVelocity>)>,
    locals: Query<(), With<LocalPlayer>>,
    avatar_editor: Option<Res<crate::ui::avatar::AvatarEditorState>>,
    mut metrics: Option<ResMut<crate::diagnostics::MetricsRegistry>>,
) {
    let delta = time.delta_secs();
    if delta <= 0.0 {
        return;
    }
    let editing_offsets = avatar_editor
        .as_deref()
        .is_some_and(|state| state.holds_rig_at_rest());
    let editing_parts = avatar_editor
        .as_deref()
        .is_some_and(|state| state.holds_rig_pose());
    // Whether ANY body strained a contact this frame (#1078) — a goal its
    // solver could not reach. Counted per frame rather than per body so a
    // crowd cannot inflate one defect, and read at the end of the loop.
    let mut strained = false;
    for (entity, child_of, body, root, mut motion) in &mut bodies {
        if editing_offsets && locals.contains(child_of.parent()) {
            hold_at_rest(&mut commands, entity, body, &mut motion, delta);
            continue;
        }
        if editing_parts && locals.contains(child_of.parent()) {
            // Held as it stands (#1106): no pose written, no state advanced.
            continue;
        }
        let Ok((transform, velocity)) = chassis.get(child_of.parent()) else {
            continue;
        };
        // Local chassis carry an avian velocity; remote peers are kinematic
        // playout, so their speed is read off the smoothed transform itself.
        let position = transform.translation();
        // Signed, not a magnitude: which WAY a body is going vertically is the
        // whole of the airborne state machine below, and `abs` threw it away.
        let (planar, vertical) = match velocity {
            Some(v) => (Vec2::new(v.0.x, v.0.z).length(), v.0.y),
            None => {
                let moved = motion
                    .last_position
                    .map_or(Vec3::ZERO, |last| (position - last) / delta);
                (Vec2::new(moved.x, moved.z).length(), moved.y)
            }
        };
        motion.last_position = Some(position);

        // **Airborne is a state, and it has to be** (#1072). Every
        // instantaneous test fails at the apex of a jump, where the vertical
        // speed is zero and the body is as airborne as it ever gets — which is
        // exactly what the old `|v_y| > 3.5` did, resuming the walk cycle and
        // planting the feet at the top of every jump.
        //
        // Two ways in and one way out. A body launches when something pushes
        // it up faster than the ground can, or falls when nothing is holding
        // it up; it lands when a body that HAS been falling stops falling,
        // which the apex cannot imitate because at the apex it is still on its
        // way down.
        let rig = &body.avatar.rig;

        // **In deep water before anything else** (#1074). The rigged root is
        // offset so `y = 0` is the chassis collider's bottom, which makes its
        // own transform the body's half-height — the same figure the
        // controller classifies with, so the animation and the physics cannot
        // disagree about whether this body is swimming.
        //
        // Only `Swimming` animates as one: a wading body has its feet on the
        // bottom and is walking, which is what the controller does with it too.
        let half_height = -root.translation.y;
        let swimming = water.as_ref().is_some_and(|water| {
            matches!(
                humanoid_water_state(
                    position.y,
                    Vec2::new(position.x, position.z),
                    half_height * 2.0,
                    water,
                ),
                WaterState::Swimming { .. }
            )
        });
        // A body in the water is not falling, whatever its vertical speed says
        // — and the preset swims upward at 1.8 m/s against a launch threshold
        // of 2.0, which is close enough that a peer's smoothed transform would
        // otherwise trip the leap.
        if swimming {
            motion.airborne = None;
        }

        let launched = vertical > LAUNCH_SPEED;
        let dropped = vertical < -FALL_SPEED;
        match &mut motion.airborne {
            None if swimming => {}
            None if launched || dropped => {
                // **Always a symmetric leap, and never a drop** (#1073). The
                // engine's `drop` says the floor arrived at is lower than the
                // one left, and carries that difference in the pose for the
                // whole landing — which is right for a body that owns its own
                // root and catastrophic here, where the chassis has already
                // carried it down: the two add, and the root goes underground
                // by the entire fall height.
                //
                // `Leap::new(speed)` is the same leap with the trajectory left
                // to the chassis. Built from the SPEED rather than the
                // direction, so a body that walks off a ledge gets a flight of
                // `2v/g` instead of the zero-length one `Leap::new(0.0)` has —
                // whose stage machine divides by an epsilon and reports a
                // landing from the first airborne frame, planting the feet in
                // mid-air all the way down.
                let leap = Leap::new(vertical.abs());
                motion.airborne = Some(Airborne {
                    leap,
                    // Past the wind-up: this app has no anticipation to spend
                    // on one — the impulse fires the fixed step after the key
                    // — so the leap is joined at the instant its feet leave.
                    elapsed: leap.wind_up(rig),
                    falling: vertical < -SETTLE_SPEED,
                    impact: vertical.min(0.0),
                    landed: false,
                });
            }
            Some(air) if !air.landed => {
                // Held inside the flight, because physics decides how long a
                // body is in the air and the leap only predicted it. A fall
                // that outlasts its prediction holds at the end of the arc,
                // where the tuck has returned to nothing and the legs are
                // straight — which is what a body reaching for the ground
                // does anyway.
                let flight_ends = air.leap.wind_up(rig) + air.leap.flight();
                air.elapsed = (air.elapsed + delta).min(flight_ends - f32::EPSILON);
                air.falling |= vertical < -SETTLE_SPEED;
                air.impact = air.impact.min(vertical);
                if air.falling && vertical >= -SETTLE_SPEED {
                    // Touchdown. The leap is rebuilt from the arrival that
                    // actually happened rather than the one predicted at
                    // takeoff, and its clock is set to the head of the
                    // landing — the only stage left to play. Symmetric again,
                    // so the depth is the impact's and the pose carries no
                    // trajectory (#1073).
                    let leap = Leap::new(air.impact.abs());
                    air.leap = leap;
                    air.elapsed = leap.wind_up(rig) + leap.flight();
                    air.landed = true;
                }
            }
            Some(air) => {
                air.elapsed += delta;
                // Leaving the ground again mid-landing is a new leap, not a
                // continuation of this one.
                if launched {
                    motion.airborne = None;
                }
            }
            None => {}
        }
        // A landing that has played out is over; the body stands up out of it.
        if motion
            .airborne
            .is_some_and(|air| air.leap.stage_at(rig, air.elapsed) == Stage::Standing)
        {
            motion.airborne = None;
        }
        let airborne = motion.airborne.is_some();

        // Pick the source and its cycle rate. Standing is the engine's idle,
        // travelling is the gait — and the gait's cadence is **derived from
        // the speed rather than bent toward it** (symbios-avatar #240): a
        // clip had one stride baked into it and could only be played faster
        // or slower; a generator asks the speed axis for the cadence the
        // stride it is about to take actually implies.
        // **The change of source is taken on the frame the world changes, and
        // #1071's handoff wait is deliberately NOT here.** The governor's rule
        // is that a transition should not begin while a contact is bearing
        // weight, and should hold for the next handoff — where support is
        // transferring anyway. Implemented and measured on a stop, it made
        // this worse, and the sweep says why: the skid a stop costs is how far
        // the blend must drag the PLANTED foot to reach the standing stance,
        // and that is smallest at midstance, where the stance foot is already
        // under the hip, and largest at a handoff, where the legs are at full
        // split. Waiting for the handoff steers the body from the best moment
        // to the worst.
        //
        // Measured, stopping from 1.4 m/s at every phase of the cycle, as the
        // furthest a foot that was bearing weight then slid: 15.4 mm stopping
        // near midstance and 203.0 at a handoff, worst 297.9 across the sweep;
        // with the wait, the phases it engaged on went 131.2 mm to 351.3 and
        // the worst rose to 351.3. See `a_stop_does_not_skate_further_than_it_
        // already_does`, which ratchets the figure, and #1071 for the write-up.
        let travelling = (planar >= IDLE_BELOW).then(|| Speed::new(rig, planar));
        // A leap owns the legs for as long as it lasts: a body cannot be
        // mid-stride and mid-air at once, and pretending otherwise is how a
        // jump ends up with a walk cycle running underneath it (#1072).
        // **The stroke's clock, which the engine leaves to its caller** — a
        // tread and a crawl are the same loops at different rates, so the rate
        // is where the difference actually lives (#1074). Both ends are
        // derived: at speed one cycle carries the body [`LENGTHS_PER_STROKE`]
        // of its own length, and at rest a tread sculls at the body's own
        // pendulum frequency, `sqrt(g/L)/2pi`, which is the one rate a body of
        // this size has without anybody choosing it. A giant sculls slower for
        // the same reason it walks slower.
        let stroke_rate = || {
            let length = rig.extent().max(f32::EPSILON);
            let tread = (9.81 / length).sqrt() / std::f32::consts::TAU;
            (planar / (length * LENGTHS_PER_STROKE)).max(tread)
        };
        let (locomotion, cadence) = match (swimming, motion.airborne, travelling) {
            (true, _, _) => (MotionSource::Swim, stroke_rate()),
            (false, Some(_), _) => (MotionSource::Leap, 0.0),
            (false, None, Some(speed)) => (MotionSource::Gait, speed.cadence(rig)),
            (false, None, None) => (MotionSource::Idle, 0.0),
        };

        // **Hand the idle the stance the body arrived in** (#1071, engine
        // #276), before anything below moves the clock or forgets the speed.
        //
        // Left to itself the idle solves every contact back to its REST
        // position each frame, so a body that stops mid-stride drags the foot
        // that was bearing its weight up to a third of a metre across the
        // ground to close its stance. That is the skid this file has ratcheted
        // since #1071, and it is not a transition-timing problem: waiting for a
        // handoff and waiting for a midstance were both implemented here and
        // both measured WORSE than stopping immediately, because a body that
        // holds keeps striding while the chassis has already stopped.
        //
        // So the idle is told which contacts were carrying the body and where
        // they were. It pins those and recovers them through its own weight
        // shifts, which is the only way a foot moves without sliding.
        //
        // Read before the assignments below: `motion.gaiting` still holds the
        // speed the body was travelling at, `motion.cycle` still reads the
        // moment the last pose was DRAWN at, and `motion.source` still says
        // what the body was doing. All three are overwritten within twenty
        // lines.
        if motion.source == MotionSource::Gait
            && locomotion == MotionSource::Idle
            && let Some(before) = motion.gaiting
            && let Some(arrival) = motion.current.clone()
        {
            let gait = before.gait(rig);
            let bearing: Vec<Limb> = gait
                .limbs
                .iter()
                .enumerate()
                .filter(|(index, _)| gait.phase(*index, motion.cycle).is_stance())
                .map(|(_, &limb)| limb)
                .collect();
            motion.idler.arrive(rig, &arrival, &bearing);
        }

        // **Carry the CYCLE across a change of gait, not the number** (#1071,
        // engine #247). A cycle fraction means a different part of the step at
        // a different duty: the duty falls all the way along the speed axis
        // and STEPS at the walk-run transition, so a body that crosses it
        // mid-stride hands 0.5 to a gait where 0.5 is a different moment, and
        // a foot in mid-swing arrives planted. `phase_matched` maps it exactly
        // — the leading contact keeps the part of the step it was actually in.
        //
        // Asked every frame rather than at a detected boundary, because there
        // is no boundary to detect: the duty moves continuously as well as
        // stepping, and where it has not moved the map is the identity to the
        // bit. One `Gait` is rebuilt on the frames it has moved, which is a
        // pair of two-element vectors beside a solve that allocates poses.
        if let (Some(speed), Some(before)) = (travelling, motion.gaiting)
            && before != speed
        {
            let into = speed.gait(rig);
            let from = symbios_avatar::Gait {
                duty: before.duty(),
                ..into.clone()
            };
            motion.cycle = transition::carry_cycle(&from, &into, motion.cycle);
        }
        motion.gaiting = travelling;

        if !airborne || swimming {
            motion.cycle = (motion.cycle + delta * cadence).fract();
        }

        // Advance the emote and retire it at the end of its one play. Done
        // before the pose is built so a gesture that finished this frame does
        // not get one last frame of overlay.
        let gesture = motion.gesture.and_then(|mut active| {
            active.elapsed += delta;
            (active.elapsed < EMOTE_SECS).then_some(active)
        });
        motion.gesture = gesture;
        // A gesture is its own motion source, so ending one blends back into
        // the walk it was riding rather than snapping.
        let source = match gesture {
            Some(active) => MotionSource::Gesture(active.emote),
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
            MotionSource::Idle => {
                // **A body with nothing else to do stands and breathes**
                // (engine #246): breath, sway, a weight shift onto one foot,
                // and scheduled fidgets, all through `Idle::drive`, which
                // advances the schedule and poses every layer in one call —
                // a stage a caller has to remember is a stage a caller
                // forgets, and this file has the scar (#1069). The idle
                // solves and plants its own feet against the same level
                // floor the gait uses, so no settle tail runs for it below.
                motion.idler.drive(rig, &mut pose, delta);
            }
            MotionSource::Gait => {
                // **One number in, everything out** (symbios-avatar #240). The
                // gait pattern, its duty, the stride length, the foot lift and
                // the cadence above all come from how fast this body is
                // actually travelling, expressed as the Froude number so the
                // same relation fits a child, a giant and a quadruped.
                //
                // Three things follow that this file used to get wrong. The
                // body now takes a LONGER step when it moves faster instead of
                // the same step more often. It CHANGES GAIT on its own at the
                // Froude number where walking stops working, so a fast avatar
                // runs rather than speed-walking. And the trunk lean stops
                // being a constant: the lean is scaled by the stride against
                // the legs taking it, and that ratio was pinned at pace 1.0 —
                // it responds for free now, which is the whole of #239's
                // complaint against this file.
                let speed = travelling.unwrap_or(Speed::STILL);
                let gait = speed.gait(rig);
                let stride = speed.stride(rig);
                // The head of the engine's own drive sequence — step, arms,
                // lean — with the footing OFF, because an emote is laid over
                // this pose below and the feet have to be settled after that
                // rather than before (symbios-avatar #253). This file used to
                // spell the stages out and was one of the three consumers that
                // had forgotten the ankles entirely (#1069).
                let walked = Walk {
                    footing: None,
                    ..Walk::at(motion.cycle)
                }
                .drive(rig, &mut pose, &gait, &stride, floor);
                strained |= !walked.steps.straining.is_empty();
                stance = walked.steps.stance;
                // The stride travels with the gait, because the tail of the
                // sequence needs it too: a turning body's ankles are turned to
                // face where they were planted, and `roll_feet` is where that
                // lands (engine #241).
                walking = Some((gait, stride));
            }
            MotionSource::Swim => {
                // **Nothing is added to `stance`**: a swimming body has
                // nothing on the ground, and handing the footing tail a
                // contact is what would drag its feet back down to a floor it
                // is nowhere near.
                //
                // No vertical correction either, and that is worth saying
                // beside the leap's, which needs one. `Swim` pitches the body
                // by rotating the ROOT joint — the pelvis — and the pelvis
                // sits about half a body height above the rigged root, which
                // is where the chassis capsule's centre is. So a body going
                // prone lies along the capsule rather than pivoting about its
                // own feet.
                Swim {
                    cycle: motion.cycle,
                    ..Swim::at(motion.cycle).toward(planar)
                }
                .drive(rig, &mut pose);
            }
            MotionSource::Leap => {
                // **The chassis owns the trajectory, so the flight's height is
                // given back** (#1072). `Leap::drive` carries the root through
                // the parabola itself, which is right for a body that owns its
                // own root and wrong here: avian is already moving the physics
                // capsule this body hangs off, and applying both flies it
                // twice. The wind-up's and the landing's heights are KEPT —
                // those are the legs compressing, which a capsule does not do.
                //
                // Taken off after the drive rather than by not asking for it,
                // because the drive plants the contacts at the height it
                // applied: in flight there is nothing to plant, so subtracting
                // afterward is exact, and on the ground there is nothing to
                // subtract.
                if let Some(air) = motion.airborne {
                    let leapt = air.leap.drive(rig, &mut pose, air.elapsed, floor);
                    strained |= !leapt.straining.is_empty();
                    if leapt.stage.is_grounded() {
                        stance = rig.ground_contacts();
                    } else {
                        pose.translation.y -= leapt.height;
                    }
                }
            }
            // `locomotion` is never a gesture by construction — the gesture
            // is chosen below, from this.
            MotionSource::Gesture(_) => {}
        }
        // The emote rides over whatever the body is already doing. A
        // goal-space clip writes only the parts its own tracks address —
        // a wave is a hand, a nod is a gaze, a bow is a trunk and a gaze
        // (engine #248) — so the legs and the pelvis keep the walk or the
        // idle that carries them, and there is no joint mask here to
        // maintain: the clip's own vocabulary is the mask. Normalised time,
        // so the elapsed seconds are scaled by the one duration decision
        // ([`EMOTE_SECS`]).
        if let Some(active) = gesture
            && let Some(clip) = gesture::by_name(active.emote.gesture_name())
        {
            clip.apply(rig, &mut pose, active.elapsed / EMOTE_SECS);
        }
        // The leap keeps its own contacts: `Leap::drive` plants them itself
        // during a wind-up or a landing, and in flight there are none.
        if airborne && locomotion != MotionSource::Leap {
            stance.clear();
        }
        // The tail of the drive: settle the contacts, then roll the ankles, in
        // that order — the engine owns both so this file cannot get the order
        // wrong again (symbios-avatar #253). Gait only: the idle plants its
        // own feet inside `Idle::drive`, and a resting body has nothing down.
        if let Some((gait, stride)) = &walking {
            let walked =
                Walk::at(motion.cycle).settle(rig, &mut pose, gait, stride, &stance, floor);
            strained |= walked.straining() > 0;
        }

        // Inertialize source switches so a walk does not snap into a stand —
        // but only where the two are different activities (#1071). Within
        // locomotion the answer is no: see [`MotionSource::family`].
        //
        // **Measured, because the obvious simplification here is wrong.** A
        // gesture looks like it should need no blend: every emote in the
        // roster is a goal-space clip that begins and ends at the body's own
        // rest offsets, so its POSITION is continuous at both ends. Its
        // velocity is not. Sampled one frame in at 60 fps, a greeting has
        // already moved a joint 74.5 mm and a refusal 101.9 — a seventh of
        // the whole gesture's travel, in one frame — and the same at the far
        // end. Dropping the blend there would put a visible snap on both ends
        // of every emote.
        if motion.source.needs_blend(source)
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
    if strained && let Some(metrics) = metrics.as_deref_mut() {
        crate::diagnostics::samplers::motion_strain_frame(metrics);
    }
}

/// Start emotes on the bodies their requests name (#1068).
///
/// Runs in the Animate set ahead of [`drive_rigged_motion`], so a gesture
/// requested this frame is posed this frame rather than a frame late.
///
/// **A request names a chassis and this finds the body under it**, because the
/// chat and network layers hold chassis entities and know nothing of rigged
/// roots. Two ways a request is dropped, both silent and both intended: the
/// chassis has no rigged body (a boat has nothing to wave with), or the body
/// is inside its cooldown. There is no missing-clip case any more — every
/// [`Emote`] names an engine gesture, and the pairing is guarded by test
/// rather than checked per request.
pub(super) fn start_emotes(
    mut requests: MessageReader<EmoteRequest>,
    time: Res<Time>,
    mut bodies: Query<(&ChildOf, &mut RiggedMotion), With<RiggedRoot>>,
) {
    let now = time.elapsed_secs();
    for request in requests.read() {
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
                emote: request.emote,
                elapsed: 0.0,
            });
            motion.gestured_at = Some(now);
        }
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

    /// Drives one body along `+Z` at a fixed ground speed and reports what the
    /// drawn pose did: the widest fore-and-aft split between the feet, and the
    /// highest the pelvis rode above its standing height.
    ///
    /// Both are read off the `AvatarPose` the body is actually drawn in rather
    /// than recomputed here — a test that reimplements its subject measures its
    /// own arithmetic, which is the lesson
    /// `the_procedural_walk_lands_toe_up_and_leaves_toe_down` records above.
    fn walked_at(metres_per_second: f32) -> (f32, f32) {
        let mut app = test_app();
        let chassis = app
            .world_mut()
            .spawn((Transform::default(), GlobalTransform::default()))
            .id();
        let avatar = symbios_avatar::Avatar::build_with(
            &engine_default_for_did("did:plc:speed-test"),
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
        let pelvis = rig
            .joints
            .iter()
            .position(|joint| joint.parent.is_none())
            .expect("a root joint");
        let standing = rig.joints[pelvis].position.y;
        let feet: Vec<usize> = [Limb::HindLeft, Limb::HindRight]
            .into_iter()
            .map(|limb| rig.in_zone(symbios_avatar::Zone::Extremity(limb))[0])
            .collect();

        const STEP_SECS: f32 = 1.0 / 60.0;
        let (mut split, mut crest) = (0.0f32, f32::MIN);
        for frame in 0..240 {
            let at = Vec3::Z * (metres_per_second * STEP_SECS * frame as f32);
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
            if motion.source != MotionSource::Gait {
                continue;
            }
            let pose = &app.world().get::<AvatarPose>(root).expect("a pose").0;
            let posed = pose.forward(&rig);
            split = split.max((posed.positions[feet[0]].z - posed.positions[feet[1]].z).abs());
            crest = crest.max(posed.positions[pelvis].y - standing);
        }
        assert!(crest > f32::MIN, "the gait never drove the body");
        (split, crest)
    }

    /// Accelerates one body across the walk-run transition and reports the
    /// largest single-frame step the leading contact took **through its own
    /// step**, as a fraction of one.
    ///
    /// **Not a distance, and that is the point** (#1071). A foot legitimately
    /// travels every frame, and at a run it travels a long way; millimetres
    /// cannot separate a body moving fast from a body whose clock was
    /// relabelled under it. What a change of gait must not do is move the
    /// contact to a different part of its STEP, so the reading is taken on an
    /// axis that does not move when the duty does — half for the stance, half
    /// for the swing — which is exactly the quantity `phase_matched` preserves
    /// and the quantity that jumps when nothing preserves it.
    ///
    /// Taken off `RiggedMotion::cycle` as the drive actually left it, under
    /// the gait the drive actually built, so it measures this file rather than
    /// a re-derivation of it.
    fn worst_phase_step(from: f32, to: f32, seconds: f32, fps: f32) -> f32 {
        let mut app = test_app();
        let chassis = app
            .world_mut()
            .spawn((Transform::default(), GlobalTransform::default()))
            .id();
        let avatar = symbios_avatar::Avatar::build_with(
            &engine_default_for_did("did:plc:transition-test"),
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
        let feet: Vec<usize> = [Limb::HindLeft, Limb::HindRight]
            .into_iter()
            .map(|limb| rig.in_zone(symbios_avatar::Zone::Extremity(limb))[0])
            .collect();

        let step_secs = 1.0 / fps;
        let frames = (seconds / step_secs) as usize;
        let mut moves: Vec<f32> = Vec::new();
        let mut worst_detail = (0.0f32, 0.0f32, Vec3::ZERO, 0.0f32, false, 0.0f32);
        let mut last_phase: Option<f32> = None;
        let mut phase_jump = (0.0f32, 0.0f32, 0.0f32, false);
        let mut before: Option<Vec<Vec3>> = None;
        let mut at = Vec3::ZERO;
        let mut crossed = (false, false);
        for frame in 0..frames {
            // A ramp in speed, integrated into a position the driver reads its
            // own speed back off — the remote-peer path, and the one that
            // exercises the transition without a velocity component to fake.
            let pace = from + (to - from) * (frame as f32 / frames as f32);
            at += Vec3::Z * (pace * step_secs);
            let mut chassis_mut = app.world_mut().entity_mut(chassis);
            *chassis_mut.get_mut::<Transform>().unwrap() = Transform::from_translation(at);
            *chassis_mut.get_mut::<GlobalTransform>().unwrap() =
                GlobalTransform::from_translation(at);
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs_f32(step_secs));
            app.world_mut()
                .run_system_once(drive_rigged_motion)
                .expect("runs");
            let motion = app.world().get::<RiggedMotion>(root).expect("motion state");
            if motion.source != MotionSource::Gait {
                continue;
            }
            let speed = Speed::new(&rig, pace);
            crossed = (
                crossed.0 || !speed.is_running(),
                crossed.1 || speed.is_running(),
            );
            let pose = &app.world().get::<AvatarPose>(root).expect("a pose").0;
            let posed = pose.forward(&rig);
            let now: Vec<Vec3> = feet.iter().map(|&joint| posed.positions[joint]).collect();
            // Where the leading contact is in its OWN step, on an axis that
            // does not move when the duty does: half for the stance, half for
            // the swing. This is what a change of gait must not relabel.
            let phase = match speed.gait(&rig).phase(0, motion.cycle) {
                symbios_avatar::anim::gait::Phase::Stance(t) => t * 0.5,
                symbios_avatar::anim::gait::Phase::Swing(t) => 0.5 + t * 0.5,
            };
            if let Some(was) = last_phase {
                let ahead: f32 = (phase - was).rem_euclid(1.0);
                if ahead > phase_jump.0 {
                    phase_jump = (ahead, pace, speed.duty(), speed.is_running());
                }
            }
            last_phase = Some(phase);
            if let Some(prev) = &before {
                let step = now
                    .iter()
                    .zip(prev)
                    .map(|(now, before)| now.distance(*before))
                    .fold(0.0f32, f32::max);
                if step > worst_detail.0 {
                    let which = now
                        .iter()
                        .zip(prev)
                        .max_by(|a, b| a.0.distance(*a.1).total_cmp(&b.0.distance(*b.1)))
                        .expect("two feet");
                    worst_detail = (
                        step,
                        pace,
                        (*which.0 - *which.1),
                        speed.duty(),
                        speed.is_running(),
                        motion.cycle,
                    );
                }
                moves.push(step);
            }
            before = Some(now);
        }
        assert!(
            crossed.0 && crossed.1,
            "the sweep never crossed the walk-run transition — it measured one gait"
        );
        assert!(moves.len() > 30, "too few samples to have a median");
        let worst = moves.iter().copied().fold(0.0f32, f32::max);
        let _ = (worst, worst_detail);
        moves.sort_by(f32::total_cmp);
        phase_jump.0
    }

    /// Walks a body up to speed, stops the chassis dead, and reports the
    /// furthest a foot that was BEARING WEIGHT at that moment then slid
    /// through the world, in metres.
    ///
    /// A planted contact is pinned to the ground: everything it does out here
    /// is a skate. The chassis holds still after the stop, so the world and
    /// the body's own frame differ by a constant and a foot's motion in one is
    /// its motion in the other.
    fn skid_through_a_stop(walk_frames: usize) -> (f32, f32, usize) {
        skid_through_a_decelerating_stop(walk_frames, 0)
    }

    /// How far a foot standing on the ground slides while the body CHANGES
    /// SPEED without ever stopping, in metres.
    ///
    /// **The control is the whole instrument** (#277). A walking body slides
    /// its feet a little at the best of times, so a figure from a decelerating
    /// walk means nothing until the same body walking at a CONSTANT speed
    /// through the same window has been read the same way. Pass `from == to`
    /// for that control.
    ///
    /// The body never drops below `IDLE_BELOW`, so the idle never engages and
    /// none of #276 is in the path: whatever this measures belongs to the gait.
    fn skate_through_a_speed_change(
        walk_frames: usize,
        from: f32,
        to: f32,
        ramp_frames: usize,
    ) -> f32 {
        let mut app = test_app();
        let chassis = app
            .world_mut()
            .spawn((Transform::default(), GlobalTransform::default()))
            .id();
        let avatar = symbios_avatar::Avatar::build_with(
            &engine_default_for_did("did:plc:stop-test"),
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
        // **The SOLE joints, not the ankle** (#277). `extremity_joints` puts the
        // joint the foot hangs from first and the sole points after it, and the
        // ankle is the wrong point to ask: a planted foot rolls heel to toe
        // through its stance, which translates the ankle horizontally while the
        // sole under it has not moved at all. Asking the ankle read 53.5 mm on
        // a body walking at a DEAD CONSTANT speed, which is the roll and not a
        // skate — and it read the same 53.5 at every phase and under every
        // speed change, which is what a measurement of the wrong thing looks
        // like when the wrong thing is deterministic.
        // **And the SOLE POINTS under them, not the joints themselves**
        // (#1082). A sole joint sits above the sole it belongs to, so a foot
        // pitching about a sole point still translates every joint over it:
        // reading the joints billed that roll as a skate and left a flat 10 to
        // 13 mm on every speed, against 3 mm for the points beneath them. This
        // is the same trap the ankle correction above records, one level down.
        // The sole point is the joint's rest position dropped to the ground
        // plane the body was built standing on, carried into the pose by the
        // ankle it hangs from — which is how `roll_feet` itself models a sole.
        let feet: Vec<(usize, Vec<usize>)> = [Limb::HindLeft, Limb::HindRight]
            .into_iter()
            .filter_map(|limb| {
                let joints = rig.extremity_joints(limb);
                let (&ankle, sole) = (joints.first()?, joints.get(1..)?);
                Some((ankle, sole.to_vec()))
            })
            .collect();

        const STEP_SECS: f32 = 1.0 / 60.0;
        let mut at = Vec3::ZERO;
        let frame = |app: &mut App, at: Vec3| {
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
        };
        for _ in 0..walk_frames {
            at += Vec3::Z * (from * STEP_SECS);
            frame(&mut app, at);
        }
        let mut track: Vec<Vec<Vec<Vec3>>> = Vec::with_capacity(120);
        let mut down: Vec<Vec<bool>> = Vec::with_capacity(120);
        for step in 0..120 {
            let speed = if step < ramp_frames {
                let done = (step as f32 + 0.5) / ramp_frames as f32;
                from + (to - from) * done
            } else {
                to
            };
            at += Vec3::Z * (speed * STEP_SECS);
            frame(&mut app, at);
            let motion = app.world().get::<RiggedMotion>(root).expect("motion");
            let stance: Vec<bool> = {
                let gait = motion.gaiting.map(|speed| speed.gait(&rig));
                let cycle = motion.cycle;
                [Limb::HindLeft, Limb::HindRight]
                    .iter()
                    .map(|limb| {
                        gait.as_ref().is_some_and(|gait| {
                            gait.limbs
                                .iter()
                                .position(|which| which == limb)
                                .is_some_and(|index| gait.phase(index, cycle).is_stance())
                        })
                    })
                    .collect()
            };
            let pose = &app.world().get::<AvatarPose>(root).expect("a pose").0;
            let posed = pose.forward(&rig);
            track.push(
                feet.iter()
                    .map(|(ankle, sole)| {
                        sole.iter()
                            .map(|&joint| {
                                let rest = rig.joints[joint].position;
                                at + posed.positions[*ankle]
                                    + posed.rotations[*ankle]
                                        * (Vec3::new(rest.x, 0.0, rest.z)
                                            - rig.joints[*ankle].position)
                            })
                            .collect()
                    })
                    .collect(),
            );
            down.push(stance);
        }
        assert_eq!(
            app.world()
                .get::<RiggedMotion>(root)
                .expect("motion")
                .source,
            MotionSource::Gait,
            "the body must still be walking, or this is measuring a stop"
        );

        // **Each sole point judged while IT is on the ground, against itself.**
        // Two gates, because a stance foot is not one rigid thing: the gait
        // says the FOOT is bearing, and each sole point's own height says
        // whether that point is the one in contact right now. Contact transfers
        // heel to toe through a stance, and a heel that lifts has moved without
        // sliding — so a point is only asked to hold still while it is down.
        //
        // Per point against itself, never the lowest point of the moment: an
        // argmin whose identity moves compares a heel against a toe, which is
        // the reading `Walk::settle` records as a quarter of a metre of
        // correction on flat ground where there were four millimetres.
        const CLEARANCE: f32 = 0.005;
        let mut skate = 0.0f32;
        for which in 0..feet.len() {
            for point in 0..feet[which].1.len() {
                let floor = track
                    .iter()
                    .map(|frame| frame[which][point].y)
                    .fold(f32::MAX, f32::min);
                let mut anchor = None;
                for (frame, stance) in track.iter().zip(&down) {
                    let world = frame[which][point];
                    if stance[which] && world.y - floor <= CLEARANCE {
                        let from = *anchor.get_or_insert(world);
                        skate =
                            skate.max(Vec3::new(world.x - from.x, 0.0, world.z - from.z).length());
                    } else {
                        anchor = None;
                    }
                }
            }
        }
        skate
    }

    /// How far a planted sole may slide at a constant speed, in metres.
    ///
    /// Eight millimetres. The swept figure is 1.4 to 4.1 and the defect this
    /// guards against read 16.6 to 17.7 on the same instrument, so this sits
    /// between the two rather than just above the passing number.
    const STEADY_SKATE_CEILING: f32 = 0.008;

    #[test]
    fn a_walking_body_holds_its_planted_sole_at_every_pace() {
        // **#278, and the acceptance is the CURVE rather than a figure.** The
        // issue was filed on two speeds — 26.3 mm at 1.4 m/s against 0.8 at
        // 0.7, thirty-three times the skate for twice the pace — and a pair
        // cannot show a shape. Swept, the shape is a THRESHOLD between 1.0 and
        // 1.2 m/s, which is neither the square nor the crouch the issue offered
        // as candidates.
        //
        // Two things were wrong and they have to be told apart, because each
        // accounts for about half of the filed number:
        //
        // ```text
        //   m/s                       0.4   0.7   1.0   1.2   1.4   1.6   1.8
        //   sole JOINTS, unfixed      1.4   0.9  12.7  28.3  25.3  29.4  25.8
        //   sole POINTS, unfixed      1.5   0.9   2.0  16.6  17.5  17.1  17.7
        //   sole POINTS, fixed        3.0   2.8   4.1   3.9   3.6   3.4   1.4
        // ```
        //
        // The first row is what this file used to measure: a sole JOINT sits
        // above the sole it belongs to, so a foot pitching about a point still
        // translates every joint over it, and that added about ten millimetres
        // at every speed (#1082). The second row is the defect itself. The
        // third is with engine #278 landed — `Walk::settle` no longer rolls the
        // ankles when it has not planted, which is what this file was asking
        // for by driving the head with `footing: None` and settling separately.
        //
        // Swept rather than pinned at one pace, which is what the issue asked
        // for: a guard at 1.4 alone would have passed throughout the defect's
        // life at 0.7.
        for metres in [0.4f32, 0.7, 1.0, 1.2, 1.4, 1.6, 1.8] {
            let skate = skate_through_a_speed_change(120, metres, metres, 1);
            assert!(
                skate < STEADY_SKATE_CEILING,
                "at a constant {metres} m/s a planted sole slid {:.1} mm, against a ceiling \
                 of {:.1}",
                skate * 1000.0,
                STEADY_SKATE_CEILING * 1000.0
            );
        }
    }

    #[test]
    #[ignore = "probe for #277: does changing speed slide a planted foot"]
    fn probe_whether_changing_speed_slides_a_planted_foot() {
        // **Two controls, one per end speed** (#277). A ramp case ENDS at a
        // different speed than it started, and the first run read a
        // decelerating body as skating LESS than the steady one — which says
        // nothing about changing speed and everything about ending up at
        // 0.7 m/s. Without both controls the ramp columns are uninterpretable.
        for walk in [100usize, 110, 120] {
            let fast = skate_through_a_speed_change(walk, 1.4, 1.4, 1);
            let slow = skate_through_a_speed_change(walk, 0.7, 0.7, 1);
            let eased = skate_through_a_speed_change(walk, 1.4, 0.7, 15);
            let quick = skate_through_a_speed_change(walk, 1.4, 0.7, 3);
            let faster = skate_through_a_speed_change(walk, 0.7, 1.4, 3);
            println!(
                "walk {walk}: STEADY 1.4 {:.1} mm, 0.7 {:.1} | 1.4->0.7 slow {:.1} quick {:.1} \
                 | 0.7->1.4 quick {:.1}",
                fast * 1000.0,
                slow * 1000.0,
                eased * 1000.0,
                quick * 1000.0,
                faster * 1000.0
            );
        }
    }

    /// The same, with the chassis brought to rest over `ramp_frames` instead of
    /// stopped dead.
    ///
    /// **Because a dead stop is a worst case no player produces.** The chassis
    /// is physics-driven and decelerates, so `planar` falls through
    /// `IDLE_BELOW` gradually and the gait's own stride shrinks with it on the
    /// way down. Whether a hold is worth taking depends entirely on how fast
    /// the body is still nominally travelling while it holds, so a governor
    /// judged only against an instantaneous stop is judged against the one
    /// profile that makes waiting maximally expensive.
    fn skid_through_a_decelerating_stop(
        walk_frames: usize,
        ramp_frames: usize,
    ) -> (f32, f32, usize) {
        let mut app = test_app();
        let chassis = app
            .world_mut()
            .spawn((Transform::default(), GlobalTransform::default()))
            .id();
        let avatar = symbios_avatar::Avatar::build_with(
            &engine_default_for_did("did:plc:stop-test"),
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
        let feet: Vec<usize> = [Limb::HindLeft, Limb::HindRight]
            .into_iter()
            .map(|limb| rig.in_zone(symbios_avatar::Zone::Extremity(limb))[0])
            .collect();

        const STEP_SECS: f32 = 1.0 / 60.0;
        const PACE: f32 = 1.4;
        let mut at = Vec3::ZERO;
        let frame = |app: &mut App, at: Vec3| {
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
        };
        for _ in 0..walk_frames {
            at += Vec3::Z * (PACE * STEP_SECS);
            frame(&mut app, at);
        }
        // Which feet are carrying the body at the instant it stops, and where
        // they are. Asked of the gait the drive is actually running.
        let motion = app.world().get::<RiggedMotion>(root).expect("motion");
        assert_eq!(
            motion.source,
            MotionSource::Gait,
            "the body must be walking"
        );
        let gait = Speed::new(&rig, PACE).gait(&rig);
        let planted: Vec<usize> = gait
            .limbs
            .iter()
            .enumerate()
            .filter(|(index, _)| gait.phase(*index, motion.cycle).is_stance())
            .filter_map(|(_, limb)| {
                [Limb::HindLeft, Limb::HindRight]
                    .iter()
                    .position(|which| which == limb)
            })
            .collect();
        assert!(!planted.is_empty(), "a walking body has a foot down");
        let ahead = gait.until_handoff(motion.cycle);
        let mut held = 0usize;
        // **World positions, not body-local ones** — `at` is added back in.
        // With a dead stop the two differ by a constant and the distinction
        // does not matter, which is why the original reading could omit it.
        // The moment the chassis is still MOVING during the measurement (the
        // deceleration ramp below), a foot correctly planted in the world
        // travels backward in the body's frame at walking pace, and a
        // body-local reading calls that a skate. It is not one.
        let start: Vec<Vec3> = {
            let pose = &app.world().get::<AvatarPose>(root).expect("a pose").0;
            let posed = pose.forward(&rig);
            planted
                .iter()
                .map(|&foot| at + posed.positions[feet[foot]])
                .collect()
        };

        // Now the chassis stops. The foot that was down must stay where the
        // ground is while the body works out that it has stopped.
        //
        // **Measured within a STANCE EPISODE, not from the stop instant**, and
        // the distinction is the difference between a skate and a step. A
        // planted foot is pinned, so everything it does is a skate — but a foot
        // is only planted while the gait says it is. Under the deceleration
        // ramp the body is still genuinely walking, so the foot this follows
        // completes its stance and then SWINGS, and a reading taken from the
        // stop instant counts that swing: the first version of the ramp read
        // 753.6 mm at a half-second ramp, which is one stride length (731 mm)
        // and not a defect at all.
        //
        // So displacement is reset every time the foot leaves the ground and
        // measured afresh from where it next lands. At a dead stop there is
        // exactly one episode and it begins at the stop, which is why the
        // ratcheted figure is unchanged by this: the harness got stricter
        // somewhere it was never exercised.
        // **Collected, then judged**, because whether a foot was DOWN on a
        // given frame is only knowable against the lowest that foot gets — and
        // that is not known until the window is over. See the verdict below.
        let mut track: Vec<Vec<Vec3>> = Vec::with_capacity(60);
        for held_frame in 0..60 {
            // The deceleration ramp, linear in speed down to rest. Zero
            // `ramp_frames` is the dead stop the ratchet is measured at.
            if held_frame < ramp_frames {
                let left = 1.0 - (held_frame as f32 + 0.5) / ramp_frames as f32;
                at += Vec3::Z * (PACE * STEP_SECS * left);
            }
            frame(&mut app, at);
            let motion = app.world().get::<RiggedMotion>(root).expect("motion");
            if motion.source == MotionSource::Gait {
                held += 1;
            }
            // Whether each followed foot is bearing weight THIS frame, asked of
            // the gait the drive is actually running rather than inferred from
            // the foot's height. A body with no gait left is standing, and a
            // standing foot is as pinned as a stance one — that is the interval
            // the blend drags it through, and it is the whole reading.
            let pose = &app.world().get::<AvatarPose>(root).expect("a pose").0;
            let posed = pose.forward(&rig);
            track.push(
                planted
                    .iter()
                    .map(|&foot| at + posed.positions[feet[foot]])
                    .collect(),
            );
        }

        // **A foot ON THE GROUND may not move; a foot in the air may.** That is
        // the whole definition of a skate, it is what a viewer actually sees,
        // and it needs no bookkeeping from the drive — which is the point,
        // because the drive's own answer changed underneath this harness. It
        // used to be enough to ask the gait, treating a body with no gait left
        // as standing on both feet. Since engine #276 an idle deliberately
        // LIFTS an unweighted foot to step it home, and the old rule counted
        // that step as a skate: it read 127 mm and rising across one recovery
        // on a body doing exactly the right thing.
        //
        // Grounded is judged per foot against the lowest that foot reaches in
        // the window, which is where it stands, rather than against a floor
        // height this harness would have to be told.
        const CLEARANCE: f32 = 0.005;
        let mut skid = 0.0f32;
        for which in 0..planted.len() {
            let floor = track
                .iter()
                .map(|frame| frame[which].y)
                .fold(f32::MAX, f32::min);
            let mut anchor = Some(start[which]);
            for frame in &track {
                let world = frame[which];
                if world.y - floor <= CLEARANCE {
                    let from = *anchor.get_or_insert(world);
                    skid = skid.max(world.distance(from));
                } else {
                    anchor = None;
                }
            }
        }
        (skid, ahead, held)
    }

    #[test]
    #[ignore = "probe for engine #266: prints the stop-cost matrix, asserts nothing"]
    fn probe_the_stop_cost_against_how_fast_the_body_stopped() {
        for ramp in [0usize, 6, 15, 30] {
            let worst = (100..=130)
                .map(|frames| skid_through_a_decelerating_stop(frames, ramp).0)
                .fold(0.0f32, f32::max);
            let best = (100..=130)
                .map(|frames| skid_through_a_decelerating_stop(frames, ramp).0)
                .fold(f32::MAX, f32::min);
            println!(
                "ramp {:>2} frames ({:.2}s): worst {:.1} mm, best {:.1} mm",
                ramp,
                ramp as f32 / 60.0,
                worst * 1000.0,
                best * 1000.0
            );
        }
    }

    #[test]
    fn a_stop_does_not_skate() {
        // **This was a ratchet on a known defect and is now a guard on a fixed
        // one** (#1071, engine #266 and #276). A body that stopped walking used
        // to blend from mid-stride into a stand, dragging the foot that was
        // bearing its weight — pinned to the ground, so every millimetre of it
        // a skate. It depended entirely on WHEN in the step the body stopped:
        // 15.4 mm near midstance and 297.9 mm at the worst phase.
        //
        // TWO FIXES WERE TRIED ON THE TRANSITION'S TIMING AND BOTH LOST. Hold
        // the change until the next handoff: 351.3 mm. Hold it until the next
        // midstance, which engine #266 proved is the moment the drag is
        // identically zero: 322.2 mm. Neither can win, because a body that
        // holds keeps striding while whatever stopped it has stopped — the WAIT
        // IS ITSELF A SKATE, at about 1.23 m per cycle held on this body, and
        // the best moment only saves 366 mm.
        //
        // What fixed it was not a moment but a mechanism (engine #276): the
        // idle is handed the stance the body arrived in, pins the contacts that
        // were bearing weight, and steps them home one at a time on its own
        // weight shifts — a foot only ever moving while it is unloaded and off
        // the ground.
        //
        // Swept over a whole cycle's worth of stopping phases, because a single
        // phase measures one point of a curve that varies by twenty to one —
        // and the first version of this test did exactly that and read the same
        // 203.2 mm with a wait in and with it out.
        //
        // Measured like for like under the current metric: 225.3 mm before the
        // fix, 35.2 after.
        //
        // **THE THRESHOLD IS SET FOR CROSS-ENVIRONMENT SPREAD, NOT FOR THE
        // MEASUREMENT** (#1182). This simulation is deterministic — a fixed
        // seed, a fixed 1/60 s step, no wall clock — and it still reads
        // differently depending on where it is COMPILED:
        //
        //     25.4 mm   this repo's dev box (Gentoo-packaged rustc 1.96.1)
        //     35.2 mm   whatever box recorded the figure above
        //     50.1 mm   a GitHub ubuntu-latest runner (official rustup 1.96.1)
        //
        // Same source, same lockfile, same profile; a 2x spread. Rust
        // documents `f32` transcendentals as platform-dependent, and the gait
        // is built out of them, so the rig's arithmetic is only reproducible
        // against a fixed toolchain BUILD, not merely a fixed version. That is
        // #1132's finding — recorded there against terrain and scatter —
        // reaching locomotion.
        //
        // Ruled out while chasing it, so nobody re-runs these: it is not the
        // test profile (dev and test-release agree here), not the floating
        // lockfile (a fresh CI-style resolve reads the same 25.4 mm, and the
        // glam that moved is a different major from the 0.32.1 bevy_math and
        // symbios-avatar actually link), and not in-process interference (the
        // full threaded suite agrees with the test run alone).
        //
        // So the guard is at 100 mm: comfortably above the worst reading any
        // environment has produced, and still four-fold below the 225.3 mm
        // regression it exists to catch. Tightening it back toward the
        // measurement needs the transcendentals in the locomotion path routed
        // through `libm` first (#1183) — until then a tighter number is not a
        // stricter test, just one that fails on some machines and not others.
        let worst = (100..=130)
            .map(|frames| skid_through_a_stop(frames).0)
            .fold(0.0f32, f32::max);
        assert!(
            worst < 0.1,
            "a foot standing on the ground slid {:.1} mm through a stop, against 25.4-50.1 mm \
             across build environments after engine #276 and 225.3 mm before it — this is \
             the pre-fix regime returning, not the cross-environment spread of #1182",
            worst * 1000.0
        );
    }

    #[test]
    fn crossing_the_walk_run_boundary_does_not_relabel_the_clock() {
        // **#1071's item 3, and the defect it removes is invisible in a
        // number and obvious in a body.** The duty falls all the way along
        // the speed axis and STEPS at the transition — about 0.55 to 0.35 —
        // so a cycle fraction handed across unchanged means a different part
        // of the step on the other side, and a foot in mid-swing arrives
        // planted. `transition::carry_cycle` maps it exactly.
        //
        // Asked the only way a discontinuity can be told from a fast motion:
        // sample twice as finely and see whether the largest step halves.
        // Measured on this body, accelerating 1.4 -> 3.2 m/s over three
        // seconds, as the largest step the leading contact takes through its
        // own step between two frames:
        //
        //   without the carry   0.121, 0.112, 0.108  at 60, 120, 240 fps
        //   with it             0.050, 0.025, 0.013
        //
        // The first converges on a tenth of a step that no amount of sampling
        // removes, at pace 1.72 m/s and duty 0.350 — the transition frame
        // itself. The second is exactly the cadence and halves with it.
        let steps: Vec<f32> = [60.0, 120.0, 240.0]
            .into_iter()
            .map(|fps| worst_phase_step(1.4, 3.2, 3.0, fps))
            .collect();
        // Six tenths rather than a half: halving is what a smooth motion does
        // exactly, and the slack is for where two samplings straddle the peak
        // differently.
        for pair in steps.windows(2) {
            assert!(
                pair[1] <= pair[0] * 0.6,
                "the leading contact's phase stepped {:.4} of a step and then {:.4} at twice \
                 the frame rate — a step that does not halve when the sampling doubles is a \
                 cliff, and the clock was relabelled under the body",
                pair[0],
                pair[1],
            );
        }
    }

    #[test]
    fn a_faster_body_takes_a_longer_step_and_eventually_leaves_the_ground() {
        // **#1070, and the defect it removes is a one-liner with a long shadow.**
        // This file pinned the stride at `Stride::for_body(rig, 1.0)` and
        // expressed speed by bending the CADENCE alone, so a sprinting avatar
        // took a stroller's step very quickly. Everything scaled by the stride
        // inherited that: the trunk lean is scaled by the stride against the
        // legs taking it, and with the stride pinned that ratio never moved, so
        // #239's new lean was a constant in the app.
        //
        // Now the speed axis answers all of it (symbios-avatar #240): a faster
        // body takes a LONGER step, and past the Froude number where the
        // inverted pendulum stops working it runs rather than speed-walking.
        // Both are read off the drawn pose.
        let (slow_split, slow_crest) = walked_at(1.0);
        let (brisk_split, _) = walked_at(1.8);
        let (_, fast_crest) = walked_at(3.0);

        // **Both samples are walks, and that is deliberate.** A foot's split is
        // its EXCURSION — how far it slides back under the body across one
        // stance — and that legitimately FALLS when the body starts running,
        // because a running foot is down for a third of the cycle instead of
        // two thirds. Comparing a walk against a run here reads the gait change
        // as a shorter stride and asserts the opposite of the truth.
        assert!(
            brisk_split > slow_split * 1.15,
            "a body at 1.8 m/s split its feet {brisk_split:.3} m against {slow_split:.3} at \
             1.0 — the stride is still pinned"
        );
        // A walking body never rises above its standing height; a running one
        // is a projectile between steps and does. That is the cleanest sign in
        // the drawn pose that the gait changed on its own.
        assert!(
            slow_crest <= 1e-3,
            "a walking body rode {:.1} mm above standing height",
            slow_crest * 1000.0
        );
        assert!(
            fast_crest > 0.005,
            "a body at 3 m/s never left the ground: crest {:.1} mm — it is still walking",
            fast_crest * 1000.0
        );
    }

    #[test]
    fn a_chat_keyword_gestures_the_sender_and_nobody_else() {
        // #1068. The request names a chassis; every OTHER body in the room has
        // to stay still, which is the property that makes this readable as
        // "that person waved" rather than as the room twitching.
        let mut app = test_app();
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
        let mut app = test_app();
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
        // **The load-bearing claim of the emote layer** (#1068): a gesture
        // plays over a walk rather than replacing it, so the joints that
        // carry the body must come out of the application untouched while the
        // upper body takes it. Without this a wave while walking would stand
        // the body still and slide its feet along the ground.
        //
        // Under the clips this was `overlay_gesture`'s rule, enforced with a
        // joint mask; since #1067 it is a property of the goal-space format —
        // a clip writes only the parts its tracks address — and THIS is the
        // test that keeps it one: the engine is free to add tracks to a
        // gesture, and one that grows a leg or root track fails here before
        // a walking body ever slides a foot.
        let avatar = symbios_avatar::Avatar::build_with(
            &engine_default_for_did("did:plc:emote-test"),
            &symbios_avatar::AvatarConfig {
                atlas: 64,
                ..Default::default()
            },
        )
        .expect("the default body builds");
        let rig = &avatar.rig;

        // A walk pose, so the legs hold something a gesture could destroy.
        let mut walking = Pose::rest(rig);
        let speed = Speed::new(rig, 1.4);
        let gait = speed.gait(rig);
        let stride = speed.stride(rig);
        gait::step(rig, &mut walking, &gait, &stride, 0.25, |_| None);
        gait::swing_arms(rig, &mut walking, &gait, &stride, 0.25);

        // Compared by DOT rather than by `Quat::angle_between`, which is
        // `acos` of the dot and so loses all its precision exactly where this
        // test looks: acos(0.9999999) is already 3.4e-4, so two bit-identical
        // rotations read as a third of a milliradian apart and every carrying
        // joint failed.
        let apart = |a: Quat, b: Quat| 1.0 - a.dot(b).abs();
        // Judged against the rig's own zones — the same question
        // `gait::swing_arms` asks to decide which limbs are legs, so a body
        // plan nobody has written yet answers it correctly too.
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

        // Every emote in the roster, over the same walk, at three points of
        // its play — the claim is about the set, not about the wave, and a
        // track added to any one of them is exactly what this exists to
        // catch. Mid-gesture alone would miss a key that returns to zero by
        // the middle.
        for emote in Emote::ALL {
            let clip = gesture::by_name(emote.gesture_name()).expect("the roster covers it");
            for through in [0.25, 0.5, 0.9] {
                let mut posed = walking.clone();
                clip.apply(rig, &mut posed, through);
                assert_eq!(
                    posed.translation, walking.translation,
                    "{emote:?} moved the root at {through} — a Root track has no \
                     business in an emote"
                );
                let mut upper_moved = 0;
                for joint in 0..posed.rotations.len() {
                    if carried.contains(&rig.joints[joint].zone) {
                        assert_eq!(
                            posed.rotations[joint], walking.rotations[joint],
                            "joint {joint} ({:?}) carries the body and {emote:?} moved it \
                             at {through}",
                            rig.joints[joint].zone
                        );
                    } else if apart(posed.rotations[joint], walking.rotations[joint]) > 1e-6 {
                        upper_moved += 1;
                    }
                }
                if through == 0.5 {
                    assert!(
                        upper_moved > 0,
                        "{emote:?} changed nothing at all mid-play — the gesture is not \
                         applying"
                    );
                }
            }
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

    /// #1135: once a body is standing at the full atlas under the record it
    /// was built from, the per-frame `AvatarRecord` deep compare stops.
    ///
    /// Sequence: a chassis with a rigged record, its full-atlas body already
    /// installed, and no editing going on — a peer standing in a room, which
    /// is the state a session spends nearly all of its frames in.
    #[test]
    fn a_standing_full_atlas_body_latches_out_of_the_per_frame_compare() {
        bevy::tasks::AsyncComputeTaskPool::get_or_init(Default::default);
        let mut app = test_app();
        let body = engine_default_for_did("did:plc:rigged-test");
        let resolved = ResolvedRig {
            body: body.clone(),
            attachments: Vec::new(),
        };
        app.insert_resource(LiveAvatarRecord(rigged_record(resolved)));
        let chassis = app
            .world_mut()
            .spawn((
                LocalPlayer,
                Transform::default(),
                GlobalTransform::default(),
                // What a landed full-atlas build leaves behind.
                RiggedApplied {
                    record: body,
                    atlas: symbios_avatar::AvatarConfig::default().atlas,
                },
            ))
            .id();
        // …under a root, which is the other half of "already standing".
        app.world_mut()
            .spawn((RiggedRoot, Transform::default(), ChildOf(chassis)));

        // First pass reconciles and latches; it must not kick a build.
        app.world_mut()
            .run_system_once(kick_rigged_builds)
            .expect("runs");
        assert!(
            app.world().get::<RiggedBuild>(chassis).is_none(),
            "a matching full-atlas body must not be rebuilt"
        );
        assert!(
            app.world().get::<RiggedSteady>(chassis).is_some(),
            "a reconciled chassis did not latch — the deep compare runs every frame"
        );

        // Standing still keeps the latch.
        app.world_mut()
            .run_system_once(kick_rigged_builds)
            .expect("runs");
        assert!(app.world().get::<RiggedSteady>(chassis).is_some());
        assert!(app.world().get::<RiggedBuild>(chassis).is_none());
    }

    /// And the latch releases on a record edit, which is the failure a
    /// cheaper gate would ship: an avatar that stops following its own
    /// editor.
    #[test]
    fn editing_the_record_releases_the_latch_and_kicks_a_rebuild() {
        bevy::tasks::AsyncComputeTaskPool::get_or_init(Default::default);
        let mut app = test_app();
        let body = engine_default_for_did("did:plc:rigged-test");
        app.insert_resource(LiveAvatarRecord(rigged_record(ResolvedRig {
            body: body.clone(),
            attachments: Vec::new(),
        })));
        let chassis = app
            .world_mut()
            .spawn((
                LocalPlayer,
                Transform::default(),
                GlobalTransform::default(),
                RiggedApplied {
                    record: body,
                    atlas: symbios_avatar::AvatarConfig::default().atlas,
                },
            ))
            .id();
        app.world_mut()
            .spawn((RiggedRoot, Transform::default(), ChildOf(chassis)));
        app.world_mut()
            .run_system_once(kick_rigged_builds)
            .expect("latching pass");
        assert!(app.world().get::<RiggedSteady>(chassis).is_some());

        // A different body under the same chassis — an editor slider, or a
        // peer's next broadcast.
        app.insert_resource(LiveAvatarRecord(rigged_record(ResolvedRig {
            body: engine_default_for_did("did:plc:someone-else"),
            attachments: Vec::new(),
        })));
        app.world_mut()
            .run_system_once(kick_rigged_builds)
            .expect("runs");
        assert!(
            app.world().get::<RiggedBuild>(chassis).is_some(),
            "the record changed and no rebuild was kicked — the latch swallowed the edit"
        );
    }

    /// The settle ladder (#1059) is time-driven with no record change behind
    /// it, so a DRAFT-atlas body must keep being re-evaluated. This is the
    /// case the issue warns a pure `Changed<>` gate would break, and the
    /// reason the latch requires the full atlas rather than merely a match.
    #[test]
    fn a_draft_atlas_body_does_not_latch_so_the_settle_rung_still_arrives() {
        bevy::tasks::AsyncComputeTaskPool::get_or_init(Default::default);
        let mut app = test_app();
        let body = engine_default_for_did("did:plc:rigged-test");
        app.insert_resource(LiveAvatarRecord(rigged_record(ResolvedRig {
            body: body.clone(),
            attachments: Vec::new(),
        })));
        let chassis = app
            .world_mut()
            .spawn((
                LocalPlayer,
                Transform::default(),
                GlobalTransform::default(),
                RiggedApplied {
                    record: body,
                    atlas: DRAFT_ATLAS,
                },
            ))
            .id();
        app.world_mut()
            .spawn((RiggedRoot, Transform::default(), ChildOf(chassis)));

        app.world_mut()
            .run_system_once(kick_rigged_builds)
            .expect("runs");
        assert!(
            app.world().get::<RiggedSteady>(chassis).is_none(),
            "a draft-atlas body latched — its full-atlas rung is owed on a TIMER, \
             and a latched chassis would never look at the clock again"
        );
        // With no `RiggedSettle` stamped, `settled` is true immediately, so
        // the full-atlas rung is owed right now and gets kicked.
        assert!(
            app.world().get::<RiggedBuild>(chassis).is_some(),
            "the settle ladder's full-atlas rung was never claimed"
        );
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

        // Drive one frame: zero speed is the Idle source (#1067), which must
        // still write a pose — the body breathes and the blink is alive.
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(16));
        app.world_mut()
            .run_system_once(drive_rigged_motion)
            .expect("runs");
        assert!(app.world().get::<AvatarPose>(root).is_some());
        assert!(app.world().get::<AvatarClosure>(root).is_some());
    }

    /// What one driven jump did, read off the drawn body.
    struct Jumped {
        /// Whether the walk cycle ever advanced while the body was in flight.
        marched: bool,
        /// Whether a foot was ever at the floor while the body was in flight.
        planted: bool,
        /// How high the lowest foot got at the apex, in metres.
        highest: f32,
        /// Whether the landing was ever reached.
        landed: bool,
        /// How far the root sank below its standing height during the landing.
        sank: f32,
        /// How far the lowest foot went under the floor during the landing.
        buried: f32,
    }

    /// Drives one body through a whole jump on a chassis that moves the way
    /// avian moves one — an impulse, then gravity, caught by a floor `ledge`
    /// metres below the one it left — and reports what the drawn body did: whether the walk cycle ever advanced in the air,
    /// whether a foot was ever planted in the air, how far the lowest foot
    /// rose at the apex, and whether the landing was ever reached.
    fn jumped(launch: f32, ledge: f32) -> Jumped {
        let mut app = test_app();
        let chassis = app
            .world_mut()
            .spawn((Transform::default(), GlobalTransform::default()))
            .id();
        let avatar = symbios_avatar::Avatar::build_with(
            &engine_default_for_did("did:plc:leap-test"),
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
        let feet: Vec<usize> = [Limb::HindLeft, Limb::HindRight]
            .into_iter()
            .map(|limb| rig.in_zone(symbios_avatar::Zone::Extremity(limb))[0])
            .collect();

        const STEP_SECS: f32 = 1.0 / 60.0;
        const GRAVITY: f32 = 9.81;
        // Walk in first, so the body is in a gait when it leaves the ground
        // and the walk cycle has something to advance.
        let mut at = Vec3::ZERO;
        let frame = |app: &mut App, at: Vec3| {
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
        };
        for _ in 0..90 {
            at += Vec3::Z * (1.4 * STEP_SECS);
            frame(&mut app, at);
        }

        // The jump: the chassis rises and falls under gravity, travelling
        // forward all the while, and is caught at the height it left.
        // The floor the body arrives on, which for a step off a ledge is
        // below the one it left.
        let landing_y = at.y - ledge;
        let mut vertical = launch;
        let (mut marched, mut planted, mut highest, mut landed) = (false, false, f32::MIN, false);
        // How far the drawn ROOT sank below where it stands, and how far the
        // lowest foot went under the floor, once the body was back on it.
        let (mut sank, mut buried) = (0.0f32, 0.0f32);
        for _ in 0..600 {
            vertical -= GRAVITY * STEP_SECS;
            at += Vec3::new(0.0, vertical * STEP_SECS, 1.4 * STEP_SECS);
            let caught = at.y <= landing_y;
            if caught {
                at.y = landing_y;
                vertical = 0.0;
            }
            let before = app.world().get::<RiggedMotion>(root).expect("motion").cycle;
            frame(&mut app, at);
            let motion = app.world().get::<RiggedMotion>(root).expect("motion");
            let source = motion.source;
            let in_flight = motion
                .airborne
                .is_some_and(|air| !air.leap.stage_at(&rig, air.elapsed).is_grounded());
            landed |= motion.airborne.is_some_and(|air| air.landed);
            if in_flight {
                marched |= (motion.cycle - before).abs() > 1e-6;
                let pose = &app.world().get::<AvatarPose>(root).expect("a pose").0;
                let posed = pose.forward(&rig);
                let lowest = feet
                    .iter()
                    .map(|&joint| posed.positions[joint].y)
                    .fold(f32::MAX, f32::min);
                highest = highest.max(lowest);
                // A foot at the floor while the body is in the air is a foot
                // the plant grabbed.
                planted |= lowest.abs() < 1e-4;
            }
            if motion.airborne.is_some_and(|air| air.landed) {
                let pose = &app.world().get::<AvatarPose>(root).expect("a pose").0;
                sank = sank.max(-pose.translation.y);
                let posed = pose.forward(&rig);
                let lowest = feet
                    .iter()
                    .map(|&joint| posed.positions[joint].y)
                    .fold(f32::MAX, f32::min);
                buried = buried.max(-lowest);
            }
            // Once the body has landed and stood up, the jump is over.
            if caught && source != MotionSource::Leap && landed {
                break;
            }
        }
        assert!(highest > f32::MIN, "the body never went airborne at all");
        Jumped {
            marched,
            planted,
            highest,
            landed,
            sank,
            buried,
        }
    }

    #[test]
    fn a_body_in_the_air_does_not_march_or_plant_a_foot() {
        // **#1072, and the apex is the whole of it.** This file called a body
        // airborne when |v_y| exceeded 3.5 m/s, which is TRUE at launch, FALSE
        // through the middle of the jump — at the apex the vertical speed is
        // zero, which is the most airborne a body ever is — and true again on
        // the way down. So the walk cycle resumed and the feet were planted at
        // the top of every jump. Airborne had to become a state.
        //
        // Driven at the preset's own launch speed: 450 N·s of impulse on an
        // 80 kg humanoid is 5.6 m/s, which is a 1.6 m apex and 1.15 s of
        // flight — a long time to be marching.
        let jump = jumped(5.6, 0.0);
        let Jumped {
            marched,
            planted,
            highest,
            landed,
            ..
        } = jump;
        assert!(
            !marched,
            "the walk cycle advanced while the body was in the air"
        );
        assert!(
            !planted,
            "a foot was planted on the floor while the body was in the air"
        );
        // The tuck: the engine draws the feet up a fifth of the leg's reach at
        // mid-flight, so a foot at the apex sits well above where it stands.
        assert!(
            highest > 0.05,
            "the feet never drew up under the body: the highest the lowest \
             foot reached was {:.1} mm above the floor",
            highest * 1000.0
        );
        assert!(landed, "the leap never reached its landing");
    }

    #[test]
    fn a_landing_bends_the_legs_and_does_not_bury_the_body() {
        // **#1073, reported as 'the landings end up underground' and it was.**
        // A landing was built as `Leap::falling(drop)`, whose height carries
        // `-drop` for the whole stage — because in the engine's model the body
        // really is that much lower, having landed on a floor below the one it
        // left. Here the chassis has already carried it down, so the two added
        // and the root went under by the entire fall height: 1.6 m on the
        // preset's own jump. The legs then could not reach up to the floor and
        // the body stayed buried through the landing.
        //
        // Two readings, both of the symptom rather than of the arithmetic
        // behind it: how far the ROOT sank, which must be a leg compressing
        // and no more, and how far the lowest FOOT went under the floor, which
        // must be nothing.
        let jump = jumped(5.6, 0.0);
        assert!(jump.landed, "the leap never reached its landing");
        // The engine clamps a leg to `MAX_SQUASH` = 0.45 of its own reach, and
        // this body's legs are about 0.9 m, so half a metre is past anything a
        // landing can legitimately ask for and nowhere near the 1.6 m the bug
        // produced.
        assert!(
            jump.sank < 0.5,
            "the root sank {:.0} mm into the floor on landing — a leg compressing \
             cannot account for that",
            jump.sank * 1000.0,
        );
        assert!(
            jump.buried < 0.01,
            "a foot ended {:.0} mm under the floor on landing",
            jump.buried * 1000.0,
        );
        // And the landing must actually be a landing: a body that absorbs
        // nothing has not bent its legs at all.
        assert!(
            jump.sank > 0.02,
            "the root sank {:.0} mm — the landing is not absorbing anything",
            jump.sank * 1000.0,
        );
    }

    #[test]
    fn stepping_off_a_ledge_flies_before_it_lands() {
        // The second defect in the same line (#1073). A body that walks off an
        // edge never launches, and `Leap::new(0.0)` has a flight of
        // `(0 + sqrt(0)) / g` — zero — so `stage_at` divided by an epsilon and
        // reported a LANDING from the first airborne frame: feet planted in
        // mid-air, all the way down. Built from the speed instead, a fall gets
        // a real arc.
        //
        // Driven with no launch at all, which is the case the jump test cannot
        // see because its launch is 5.6 m/s.
        // Two metres down, and no push off the edge at all.
        let fall = jumped(0.0, 2.0);
        assert!(
            !fall.planted,
            "a foot was planted in mid-air while the body was falling"
        );
        assert!(
            !fall.marched,
            "the walk cycle advanced while the body was falling"
        );
        assert!(fall.landed, "the fall never reached its landing");
        assert!(
            fall.buried < 0.01,
            "a foot ended {:.0} mm under the floor after a fall",
            fall.buried * 1000.0,
        );
    }

    /// Drives one body through deep water at `pace` and reports what the drawn
    /// body did: whether a foot was ever planted, and how far the trunk was
    /// pitched onto its front at the end.
    ///
    /// **No walk-cycle reading here, deliberately.** `RiggedMotion::cycle` is
    /// the stroke's clock while a body is swimming, so it advancing proves
    /// nothing either way; what says no gait ran is the source assertion
    /// inside the loop and the planted foot below it.
    ///
    /// The pitch is read as the angle between the body's own long axis — root
    /// to head on the posed skeleton — and the world's vertical, so it is a
    /// property of the drawn body rather than a number handed back by the
    /// thing under test.
    fn swam(pace: f32) -> (bool, f32) {
        let mut app = test_app();
        // A pond whose surface is well above the body: a chassis at y = 0 with
        // a 1.8 m capsule has its head at 0.9, so a surface at 5 m is
        // unambiguously deep water.
        app.insert_resource(crate::water::WaterSurfaces {
            planes: vec![crate::water::WaterPlane {
                world_from_local: Transform::from_xyz(0.0, 5.0, 0.0),
                local_half_extents: Vec2::splat(200.0),
                flow_strength: 0.0,
                owner: crate::water::WaterPlane::NO_OWNER,
            }],
        });
        let chassis = app
            .world_mut()
            .spawn((Transform::default(), GlobalTransform::default()))
            .id();
        let avatar = symbios_avatar::Avatar::build_with(
            &engine_default_for_did("did:plc:swim-test"),
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
        let feet: Vec<usize> = [Limb::HindLeft, Limb::HindRight]
            .into_iter()
            .map(|limb| rig.in_zone(symbios_avatar::Zone::Extremity(limb))[0])
            .collect();
        let pelvis = rig
            .joints
            .iter()
            .position(|joint| joint.parent.is_none())
            .expect("a root joint");
        let head = *rig
            .in_zone(symbios_avatar::Zone::Head)
            .first()
            .expect("a head");

        const STEP_SECS: f32 = 1.0 / 60.0;
        let mut at = Vec3::ZERO;
        let mut planted = false;
        let mut pitch = 0.0f32;
        for _ in 0..180 {
            at += Vec3::Z * (pace * STEP_SECS);
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
            let motion = app.world().get::<RiggedMotion>(root).expect("motion");
            assert_eq!(
                motion.source,
                MotionSource::Swim,
                "a body under the surface must be swimming"
            );
            let pose = &app.world().get::<AvatarPose>(root).expect("a pose").0;
            let posed = pose.forward(&rig);
            planted |= feet
                .iter()
                .any(|&joint| posed.positions[joint].y.abs() < 1e-4);
            let along = posed.positions[head] - posed.positions[pelvis];
            pitch = along
                .normalize_or(Vec3::Y)
                .dot(Vec3::Y)
                .clamp(-1.0, 1.0)
                .acos();
        }
        (planted, pitch.to_degrees())
    }

    #[test]
    fn a_swimming_body_treads_at_rest_and_lies_down_to_travel() {
        // **#1074.** The controller has had three water modes since the
        // humanoid locomotion work and the animation layer knew about none of
        // them: a body crossing a pond walked through it, striding at whatever
        // its horizontal speed implied.
        //
        // The engine's swim is one axis from a tread to a crawl, so the claim
        // worth guarding is that BOTH ends arrive: a body holding station
        // hangs upright and sculls, and a travelling one lies along the water.
        // A caller that pinned the effort would get one of the two and pass any
        // test that only looked at the other.
        let (planted, upright) = swam(0.0);
        assert!(!planted, "a treading body planted a foot on the bottom");
        // Full effort on this body is 0.7 of its length a second — about 1.2
        // m/s — so 1.4 is comfortably prone.
        let (_, prone) = swam(1.4);
        assert!(
            upright < 25.0,
            "a body treading water hung {upright:.0} deg off vertical — a tread \
             is upright"
        );
        assert!(
            prone > 60.0,
            "a body swimming at 1.4 m/s lay {prone:.0} deg off vertical — a \
             crawl lies along the water"
        );
    }

    #[test]
    fn a_standing_body_breathes_instead_of_freezing() {
        // **The statue regression, which is what #1067 risked.** Under the
        // clips a standing body played Idle_A; with them gone the replacement
        // is the engine's idle (#246), and the failure mode of forgetting to
        // wire it is silent: `Rest` writes a perfectly valid pose every frame
        // — the same one, forever. So the guard is change over time, read off
        // the drawn pose: a breathing body's joints move between frames a
        // second apart, a statue's do not. Blinking cannot satisfy it — the
        // eyes are geometry closure, not pose rotations.
        let mut app = test_app();
        let chassis = app
            .world_mut()
            .spawn((Transform::default(), GlobalTransform::default()))
            .id();
        let avatar = symbios_avatar::Avatar::build_with(
            &engine_default_for_did("did:plc:idle-test"),
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

        // The chassis never moves; the body is standing. Sampled once a
        // second for several seconds, because a breath is slow — adjacent
        // 16 ms frames of a breathing body are nearly identical too.
        let mut samples: Vec<Pose> = Vec::new();
        for frame in 0..240 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs_f32(1.0 / 60.0));
            app.world_mut()
                .run_system_once(drive_rigged_motion)
                .expect("runs");
            if frame % 60 == 0 {
                samples.push(
                    app.world()
                        .get::<AvatarPose>(root)
                        .expect("a pose")
                        .0
                        .clone(),
                );
            }
        }
        let motion = app.world().get::<RiggedMotion>(root).expect("motion state");
        assert_eq!(
            motion.source,
            MotionSource::Idle,
            "a standing body's source must be the idle"
        );
        let apart = |a: Quat, b: Quat| 1.0 - a.dot(b).abs();
        let moved = samples
            .windows(2)
            .map(|pair| {
                (0..pair[0].rotations.len())
                    .map(|joint| apart(pair[0].rotations[joint], pair[1].rotations[joint]))
                    .fold(0.0f32, f32::max)
            })
            .fold(0.0f32, f32::max);
        assert!(
            moved > 1e-6,
            "four seconds of standing drew a bit-identical skeleton — the idle is not \
             running and every standing body in the room is a statue"
        );
    }

    #[test]
    fn a_chat_keyword_changes_the_pose_the_body_is_actually_drawn_in() {
        // **End to end through the real systems** (#1068), because every other
        // test here proves a piece: the keyword scan, the targeting, the
        // cooldown and the overlay arithmetic each pass on their own while the
        // wiring between them could still be wrong. This is the one that fails
        // if `drive_rigged_motion` never reaches the gesture branch — the exact
        // defect that would otherwise only show up in the running app.
        let mut app = test_app();
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

    /// **#1106, reproduced.** Selecting a PART of a worn item snapped the
    /// body to the bind pose: the part had just been detached to world
    /// space at its animated pose, so its parent moved out from under it
    /// and the selection itself visibly shifted the part. A part selection
    /// must hold the body exactly where it stands — the pose the driver
    /// wrote last frame is the pose it leaves on the root, bit for bit —
    /// while a whole-prop selection still pins the bind pose (#1062).
    #[test]
    fn selecting_a_part_holds_the_pose_as_it_stands() {
        let mut app = test_app();
        let chassis = app
            .world_mut()
            .spawn((
                crate::state::LocalPlayer,
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();
        let avatar = symbios_avatar::Avatar::build_with(
            &engine_default_for_did("did:plc:part-hold-test"),
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

        // Walk into a mid-stride pose, as the #1069 test does.
        const STEP_SECS: f32 = 1.0 / 60.0;
        const PACE: f32 = 1.3;
        for frame in 0..90 {
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
        }
        let stride = app
            .world()
            .get::<AvatarPose>(root)
            .expect("a pose")
            .0
            .clone();
        let rest = Pose::rest(&rig);
        let drift = |a: &Pose, b: &Pose| {
            let (a, b) = (a.forward(&rig), b.forward(&rig));
            a.positions
                .iter()
                .zip(&b.positions)
                .map(|(p, q)| p.distance(*q))
                .fold(0.0_f32, f32::max)
        };
        assert!(
            drift(&stride, &rest) > 0.02,
            "the walk must have taken the body away from rest for the hold to mean anything"
        );

        // Select a PART: the pose must not move by a single bit.
        let mut editor = crate::ui::avatar::AvatarEditorState::default();
        editor.select_attachment_part_from_scene_pick(String::from("3jzfcijpj2z2a"), vec![0]);
        app.insert_resource(editor);
        for _ in 0..30 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs_f32(STEP_SECS));
            app.world_mut()
                .run_system_once(drive_rigged_motion)
                .expect("runs");
        }
        let held = &app.world().get::<AvatarPose>(root).expect("a pose").0;
        assert_eq!(
            drift(held, &stride),
            0.0,
            "a part selection re-posed the body (max joint travel {} m) — the detached part \
             was left behind by its own parent",
            drift(held, &stride)
        );

        // A WHOLE-prop selection still pins the bind pose (#1062).
        let mut editor = crate::ui::avatar::AvatarEditorState::default();
        editor.select_attachment_from_scene_pick(String::from("3jzfcijpj2z2a"));
        app.insert_resource(editor);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(STEP_SECS));
        app.world_mut()
            .run_system_once(drive_rigged_motion)
            .expect("runs");
        let pinned = &app.world().get::<AvatarPose>(root).expect("a pose").0;
        assert_eq!(
            drift(pinned, &rest),
            0.0,
            "the whole-prop hold is the bind pose"
        );
    }
}
