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
//!   - [`fill_rigged_drive`] tells every built body what its chassis is
//!     actually doing — where it is, how fast, which way it faces, whether
//!     the water it stands in is deep enough to swim in, whether an editor
//!     is holding it — and [`bevy_symbios_avatar::drive_avatar_bodies`]
//!     drives it from that: the gait on the speed axis when the chassis
//!     travels (one dimensionless speed decides stride, cadence, duty and
//!     the walk-run boundary — symbios-avatar #240, adopted under #1070),
//!     the idle when it stands (breath, sway, weight shift, fidgets and the
//!     glance they aim — engine #246), goal-space gestures over either
//!     (#1068), inertialized source switches, stance feet planted on the
//!     local ground plane through the world foothold ledger, and
//!     engine-driven blinking. **The state machine behind all of that was
//!     this module's until #1171 moved it upstream**, where the sibling
//!     viewer now runs the same one; what is left here is the half only an
//!     application can answer, and [`motion`] is that fill.
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

use crate::pds::avatar::EngineAvatarRecord;

/// Atlas side used while the record is still moving under an editor. The
/// sibling viewer's own draft rung: 68 ms a build against 277 at full size.
const DRAFT_ATLAS: u32 = 256;
/// How long the record must be still before the full-atlas build is owed.
const SETTLE_SECS: f32 = 0.8;

/// How long the owner's own body may be building before the wait is worth a
/// word (#1255).
///
/// Comfortably above a native full-atlas build (~277 ms) and above the
/// gen-worker's own documented 130 ms–1.0 s instantiation, so the ordinary
/// case stays silent; comfortably below the offload watchdog's 60 s, so a
/// worker that is never coming back is visible long before the diagnostics
/// log is the only place that knows.
const SLOW_BUILD_ANNOUNCE_SECS: f64 = 2.5;

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

/// The last build dispatched for this chassis came back with no body
/// (#1255).
///
/// The engine returns a bare `None` — one documented cause, limbs
/// overlapping at a joint, and no reason value — so this marker is the whole
/// of what the app knows about the failure, and the whole of what any
/// surface can say about it.
///
/// Read together with [`RiggedApplied`], which is stamped with the record
/// that build was for: the pair means "the record recorded there is the one
/// that failed". That is what makes the claim self-invalidating — a record
/// edit makes the comparison in [`kick_rigged_builds`] false without
/// anything having to clear the marker, so the next build is kicked
/// normally and the owner's escape route is simply to move the slider back.
/// [`land_rigged_builds`] removes it on the next build that lands a body.
///
/// Public because the avatar editor queries it beside
/// [`LocalPlayer`](crate::state::LocalPlayer) to draw the banner it
/// implies, and `avatar_ui` is itself `pub`. It carries no reason string
/// for the same reason there is no toast text here: the words belong to
/// the surface, the fact belongs to the player.
#[derive(Component)]
pub struct RiggedBuildFailed;

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
    /// The owner has already been told this build is taking a while
    /// (#1255). Lives here rather than in a `Local` so it dies with the
    /// build it describes: a retry is a new wait and gets to say so.
    announced: bool,
    task: bevy::tasks::Task<crate::offload::GenResult>,
}

/// The one child of the chassis the skinned body hangs off. Deliberately not
/// [`crate::world_builder::AvatarVisualRoot`], so the procedural gait layer
/// cannot see it.
#[derive(Component)]
pub(crate) struct RiggedRoot;

/// Where this body's chassis was on the last frame the fill saw it, and that
/// frame's delta, or `None` before the first.
///
/// **The delta is the one the NEXT displacement was travelled over** (#1323).
/// A remote peer's chassis is a bare transform the smoother writes in
/// `Update`, and the fill reads its `GlobalTransform`, which such a transform
/// only gets at `PostUpdate`'s propagation — so each frame the fill sees last
/// frame's playout, and the step from `last` to it was travelled over the
/// frame `last` was seen on, not over this one. Dividing by this frame's
/// delta read a steady walker at `v × previous / this`: twice its speed on
/// the frame after a dropped vsync frame, and over the engine's walk-run
/// transition after a 50–99 ms hitch. At a steady frame rate the two deltas
/// are the same number, so every instrument that marches a chassis at a fixed
/// step reads what it always read.
///
/// **Consumer state, and deliberately not [`bevy_symbios_avatar::Drive::at`]**
/// (#1171). A body whose motion arrives as positions rather than as a velocity
/// has its speed differenced from where it was, and
/// [`bevy_symbios_avatar::Drive::moved_to`] does exactly that off `at` — which
/// is the right shape for every frame except the first, where there is no
/// previous frame to difference against. `at` cannot say so: it always holds
/// something, and a body freshly installed would difference this frame's place
/// against the same instant's snapshot and call it a velocity. `None` is that
/// one frame, and it is also what keeps a peer arriving anywhere but the
/// origin from reading its first frame as a launch from (0, 0, 0).
///
/// Dropping it is not free even though the app cannot see it: the whole
/// instrument suite in this module's tests walks a body in from a standing
/// start, so a body that arrives already walking measures a different cycle —
/// worth 0.3 mm on the stop skid the first time this was tried.
#[derive(Component, Default)]
pub(super) struct RiggedTrail {
    last: Option<(Vec3, f32)>,
}

/// The next idle-and-blink seed for a body about to join the room.
///
/// **A process-wide counter, and it stays the consumer's rather than the
/// engine's** (#1194). These are the clocks a ROOM is judged on: seeded alike,
/// every body in it breathes, shifts its weight and blinks in unison, which
/// reads as a drill team rather than a crowd. The other failure is a seed
/// drawn from somewhere a measurement cannot see — it makes a figure a
/// function of how many bodies the process built first — which is why
/// [`bevy_symbios_avatar::AvatarDriver`] has no `Default` and why an
/// instrument seeds its own body by hand instead of calling this.
pub(super) fn next_room_seed() -> u64 {
    static SEED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(7);
    SEED.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// The idle seed every instrument stands its body on (#1194): the value
/// [`next_room_seed`] draws first in a fresh process, so the figure is the one
/// the instrument always read for its first sim when run alone.
///
/// Shared by the body instruments in this module's tests and the turning
/// harness beside the chassis controller (#1323), so the two read one body.
#[cfg(test)]
pub(super) const INSTRUMENT_SEED: u64 = 7;

/// One frame of motion: this app's fill, then the sibling crate's driver.
///
/// **Both, always.** Carrying a [`bevy_symbios_avatar::Drive`] is what opts a
/// body into [`bevy_symbios_avatar::drive_avatar_bodies`], so the two are one
/// unit — the fill advances no clock and writes no pose on its own, and the
/// driver alone would run a body off last frame's chassis. Driving through half
/// of a pair and believing the reading is the #1069 mistake this module's tests
/// record, which is why every instrument goes through this one helper.
#[cfg(test)]
pub(super) fn drive_frame(app: &mut App) {
    use bevy::ecs::system::RunSystemOnce;
    app.world_mut()
        .run_system_once(fill_rigged_drive)
        .expect("the fill runs");
    app.world_mut()
        .run_system_once(bevy_symbios_avatar::drive_avatar_bodies)
        .expect("the driver runs");
}

mod build;
mod motion;
mod placeholder;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(super) use build::install_built_body;
pub(super) use build::{kick_rigged_builds, land_rigged_builds};
pub(super) use motion::{count_motion_strain, fill_rigged_drive, start_emotes};
pub(super) use placeholder::{announce_slow_builds, sync_local_placeholder};
