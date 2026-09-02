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
//!     differs from what is built and starts an [`symbios_avatar::Avatar::build`] on the
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
//!     ([`crate::interaction::locomotion::locomotion_total_height`] / 2 below
//!     its centre) — the same convention generator visuals are authored to.
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

use bevy::prelude::*;
use symbios_avatar::anim::transition;
use symbios_avatar::{Blink, Footholds, Idle, Inertializer, Leap, Pose, Speed};

use crate::pds::avatar::EngineAvatarRecord;
use crate::player::emote::Emote;

/// Below this horizontal speed the body idles.
const IDLE_BELOW: f32 = 0.3;
/// How long the gait takes to believe a change of speed, in seconds (#1192).
///
/// The engine's postural terms — trunk lean, stride, duty, the neck's
/// counter — are pure functions of the pace they are fed, so they are exactly
/// as continuous as this repo's chassis, which assigns velocity at a measured
/// 12 m/s² up and 39–50 m/s² on stops. Fed raw, a speed step unloads the
/// whole trunk pitch between two frames 55 ms apart (symbios-avatar #325's
/// conviction, `decel.png`); no gait looks human fed a step function. The
/// pace the speed axis reads is therefore eased through a first-order lag
/// with this time constant — the playbook's human load-response figure, of
/// the order of one step (§4: springs for every steppable input) — so a
/// change of speed reads as intent the body carries out over a step rather
/// than a snap. **The pace only, never the source**: stopping still hands
/// over to the idle on the frame the chassis stops, because holding the gait
/// while the world stands still is a measured skid (#1071), and the planted
/// feet stay honest through the eased pace by the foothold ledger (engine
/// #277). The stop itself barely meets this filter — the chassis crosses
/// `IDLE_BELOW` in under 0.1 s — its work is the changes that stay inside
/// the gait: the run key's walk↔run steps, partial slowdowns, and a remote
/// peer's corrected velocity.
const PACE_RESPONSE: f32 = 0.3;
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
    /// The eased pace the gait was last fed, in m/s (#1192): the chassis'
    /// speed through a first-order lag of [`PACE_RESPONSE`], so postural
    /// terms ride a human load-response instead of the chassis' 40 m/s²
    /// steps. `None` whenever the gait is not carrying the body — the next
    /// walk eases from its own first frame, not from a stale speed.
    paced: Option<f32>,
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
    /// The world plant-point ledger (engine #277, adopted at 0.5.0): while a
    /// foot is down its world point is held and served back, so a planted
    /// contact stands still through speed changes — the stride-derived offset
    /// slides `|t − 0.5| · dL` under a changing speed, measured upstream at
    /// 91 mm against a 2 mm steady control on this app's own speed profile.
    /// Fed the chassis transform each gait frame; reset whenever something
    /// other than the gait carries the body, so a resumed walk plants afresh
    /// instead of serving points the idle's weight shifts walked away from.
    /// The P2P asterisk is the engine's: a peer's ledger integrates its own
    /// observed travel, so holds may diverge cosmetically between peers, and
    /// nothing here feeds back into the clock.
    footholds: Footholds,
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
    /// [`symbios_avatar::anim::gesture::by_name`] each frame — a goal-space clip is a few keyed
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
        Self::seeded(seed)
    }
}

impl RiggedMotion {
    /// A fresh motion state whose idle and blink are seeded from `seed`
    /// rather than from the process-wide counter [`Default`] draws on.
    ///
    /// The counter is the right thing for a room and the wrong thing for an
    /// instrument (#1194): an idle's seed decides when its settling weight
    /// shift fires and which leg it moves first, so a foot-skid figure read
    /// off a body made through [`Default`] is a function of how many bodies
    /// the PROCESS had already made — which, under `cargo test`, is how many
    /// other tests had reached this line first. A test that measures the
    /// idle seeds its body here.
    fn seeded(seed: u64) -> Self {
        Self {
            cycle: 0.0,
            source: MotionSource::Rest,
            last_position: None,
            gaiting: None,
            paced: None,
            airborne: None,
            previous: None,
            current: None,
            transition: None,
            blink: Blink::seeded(seed),
            idler: Idle::seeded(seed),
            footholds: Footholds::new(),
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

mod build;
mod motion;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(super) use build::install_built_body;
pub(super) use build::{kick_rigged_builds, land_rigged_builds};
pub(super) use motion::{drive_rigged_motion, start_emotes};
