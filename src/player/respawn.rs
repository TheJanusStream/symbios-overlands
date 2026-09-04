//! Recovery: put the local player back on solid ground — automatically
//! when they fall through the terrain, go non-finite, or leave the world,
//! and on demand when they ask (#1240 f159), with the respawn metric +
//! typed timeline event.
//!
//! The two halves share one pose ([`recovery_pose`]) and one reason
//! vocabulary ([`RecoveryReason`]), and differ in exactly one way: an
//! automatic recovery may escalate to a full physics-body rebuild, while
//! a user-requested reset is a plain position write. Escalating on a
//! request would let three deliberate resets in a minute tear down and
//! rebuild the chassis of somebody who was merely lost.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::config::rover as cfg;
use crate::state::{LiveRoomRecord, LocalPlayer, TravelingTo};

use super::random_spawn_xz;

/// Windowed respawn count at which the teleport escalates to a full
/// physics-body rebuild (#867). One or two catches are ordinary falls;
/// three inside [`RESPAWN_WINDOW_SECS`](crate::diagnostics::anomaly) is
/// the thrash signature — the teleport is not sticking, so position
/// writes alone won't recover.
const BODY_REBUILD_AFTER_RESPAWNS: u32 = 3;

/// Why the player is being put back on solid ground (#1240).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RecoveryReason {
    /// The physics body reached NaN/Inf (#867).
    NonFinite,
    /// Dropped more than [`cfg::FALL_BELOW_GROUND`] below local ground.
    FellThrough,
    /// Left the heightmap's extent by more than
    /// [`cfg::WORLD_EDGE_MARGIN`] (#1240 f169). The heightmap sample is
    /// CLAMPED into the extent, so out past the edge the ground reference
    /// is the boundary height and an aircraft cruising above it satisfies
    /// no fall test — the world simply ended, and the only documented way
    /// back was to dive into the void until the fall test fired.
    LeftTheWorld,
    /// The owner asked (#1240 f159).
    Requested,
}

impl RecoveryReason {
    /// What the player is told. The teleport used to be silent (#842);
    /// each reason names the thing that happened, because "you fell out of
    /// the world" is a confusing thing to read after flying level.
    pub fn toast(self) -> &'static str {
        match self {
            Self::NonFinite | Self::FellThrough => "Returned to spawn — you fell out of the world.",
            Self::LeftTheWorld => "Returned to spawn — you left the world behind you.",
            Self::Requested => "Returned to spawn.",
        }
    }

    /// Whether this recovery may escalate to a physics-body rebuild.
    /// Only the automatic ones: a rebuild is the answer to a body the
    /// solver has broken, not to a player who walked into a crevasse.
    fn may_escalate(self) -> bool {
        !matches!(self, Self::Requested)
    }
}

/// Which automatic recovery, if any, this body needs — pure, so the three
/// boundaries are testable without a physics world.
///
/// Order matters: the non-finite check comes FIRST (#867) because once a
/// body reaches NaN every comparison below misbehaves (NaN fails the
/// fall test, so the old code "respawned" every frame without healing)
/// and the heightmap sample runs on garbage coordinates.
pub fn automatic_recovery(
    finite: bool,
    pos: Vec3,
    local_ground: f32,
    half_extent: f32,
) -> Option<RecoveryReason> {
    if !finite {
        return Some(RecoveryReason::NonFinite);
    }
    if pos.y <= local_ground - cfg::FALL_BELOW_GROUND {
        return Some(RecoveryReason::FellThrough);
    }
    // `>=`, not `>`: `edge_buoyancy_falloff` reaches zero at exactly this
    // distance, and a hull with no lift and no recovery is the silent sink
    // this fixes.
    let edge = half_extent + cfg::WORLD_EDGE_MARGIN;
    if pos.x.abs() >= edge || pos.z.abs() >= edge {
        return Some(RecoveryReason::LeftTheWorld);
    }
    None
}

/// How much of the water's lift survives at this distance from the world
/// edge (#1240 f169): 1 inside the extent, ramping to 0 across
/// [`cfg::WORLD_EDGE_MARGIN`].
///
/// `apply_hover_boat_buoyancy` used to cut lift to zero the instant the
/// hull crossed the extent, so a boat driven over the boundary sank with
/// no cue at all and read as a physics bug. The band is deliberately the
/// same width as the margin [`automatic_recovery`] returns the player at,
/// so a hull that keeps going is recovered before the lift runs out.
pub fn edge_buoyancy_falloff(pos: Vec3, half_extent: f32) -> f32 {
    let past = (pos.x.abs() - half_extent).max(pos.z.abs() - half_extent);
    if past <= 0.0 {
        return 1.0;
    }
    (1.0 - past / cfg::WORLD_EDGE_MARGIN).clamp(0.0, 1.0)
}

/// Where a recovery puts the player, and facing which way.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RecoveryPose {
    pub pos: Vec3,
    pub rot: Quat,
}

/// The recovery pose (#745), shared by the fall path and the owner's
/// "Return to spawn" (#1240 f159) so the two cannot land in different
/// places: the room's default landing when configured, otherwise the
/// legacy random scatter.
///
/// Not pure in the scatter case — `random_spawn_xz` walks a
/// process-global counter, which is the historical behaviour and is what
/// stops two players stacking on one point. With a landing configured it
/// is a pure function of the record and the heightmap.
///
/// The (x, z) is clamped into the terrain extent so a landing aimed
/// outside the heightmap (possible in a hand-edited record — sanitize
/// only bounds magnitude) can't strand the player on an endless
/// fall-respawn-fall loop over the void. An explicit landing height is
/// honoured (sky-platform landings) but floored at ground level, because
/// respawning *below* the terrain re-triggers the fall path every frame.
pub fn recovery_pose(
    hm: &bevy_symbios_ground::HeightMap,
    landing: Option<crate::pds::DefaultLanding>,
) -> RecoveryPose {
    let extent = (hm.width() - 1) as f32 * hm.scale();
    let half = extent * 0.5;
    let centre = extent * 0.5;
    let (ox, oz, explicit_y, yaw_deg) = match landing {
        Some(l) => (
            l.pos.0[0].clamp(-half, half),
            l.pos.0[1].clamp(-half, half),
            l.y.map(|y| y.0),
            Some(l.yaw_deg.0),
        ),
        None => {
            let (x, z) = random_spawn_xz();
            (x, z, None, None)
        }
    };
    let hm_x = (centre + ox).clamp(0.0, extent);
    let hm_z = (centre + oz).clamp(0.0, extent);
    let ground_y = hm.get_height_at(hm_x, hm_z);
    let surface_normal = hm.get_normal_at(hm_x, hm_z);
    let tilt = Quat::from_rotation_arc(Vec3::Y, Vec3::from_array(surface_normal));
    let yaw = yaw_deg
        .map(|deg| Quat::from_rotation_y(deg.to_radians()))
        .unwrap_or(Quat::IDENTITY);
    let floor_y = ground_y + cfg::SPAWN_HEIGHT_OFFSET;
    RecoveryPose {
        pos: Vec3::new(ox, explicit_y.map_or(floor_y, |y| y.max(floor_y)), oz),
        rot: tilt * yaw,
    }
}

/// Where the owner has asked to be moved, on purpose (#1240 f159, #1244
/// f148).
///
/// Two requests, one resource and one system, because they are the same
/// write with different destinations — and neither may escalate the way
/// the fall path does.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PlayerMove {
    /// Put me back on solid ground (#1240 f159). Being unable to move is
    /// the most total failure a 3D world has, and before this the only
    /// recovery was `respawn_if_fallen` — which fires only 20 m BELOW
    /// local ground, so geometry that traps you *above* the terrain (a
    /// construct collider, a crevasse, a settlement wall) never satisfied
    /// it, and the exit was logging out.
    ReturnToSpawn,
    /// Take me to the thing I have selected in the editor (#1244 f148).
    /// A tree-row click attaches the gizmo to whichever live instance is
    /// nearest the CAMERA, which for a distant or behind-the-camera asset
    /// is still arbitrarily far away — so half the time selecting from
    /// the tree produced no visible result, and the owner could not tell
    /// "nothing was selected" from "the thing is 200 m behind me". The
    /// camera cannot be aimed independently — `follow_local_player` pins
    /// `target_focus` to the chassis every frame — so GOING there is what
    /// "bring it into view" means in this world.
    GoTo(Vec3),
}

/// The pending [`PlayerMove`], if any.
#[derive(Resource, Default)]
pub struct PlayerMoveRequest(Option<PlayerMove>);

impl PlayerMoveRequest {
    /// Ask for a move on the next `Update`.
    pub fn request(&mut self, what: PlayerMove) {
        self.0 = Some(what);
    }
}

/// Why "Return to spawn" is unavailable right now, or `None` when it is
/// (#1240 f159, in the shape of `travel::home_travel_blocked`).
///
/// Ordered most-fundamental-first, so the reason names the thing the user
/// has to resolve rather than whichever check happened to run first.
pub fn return_to_spawn_blocked(terrain_ready: bool, traveling: bool) -> Option<&'static str> {
    if !terrain_ready {
        return Some("The ground is still being built.");
    }
    if traveling {
        return Some("You're already travelling somewhere.");
    }
    None
}

/// How far from a framed object the player is put down (#1244 f148), on
/// top of the object's own radius. Close enough that the thing fills the
/// view, far enough not to land inside it.
const GO_TO_STANDOFF_M: f32 = 4.0;

/// Where to stand to look at something of `radius` centred on `centre`,
/// approaching from `from` (#1244 f148). Pure.
///
/// Approaching from the player's current side means the shortest trip and
/// no surprise about which way they end up facing. A degenerate direction
/// (already exactly on the object) falls back to +Z, so the result is
/// never the object's own centre.
pub fn go_to_pose(centre: Vec3, radius: f32, from: Vec3) -> Vec3 {
    let mut away = from - centre;
    away.y = 0.0;
    let dir = away.normalize_or_zero();
    let dir = if dir == Vec3::ZERO { Vec3::Z } else { dir };
    centre + dir * (radius + GO_TO_STANDOFF_M)
}

/// Perform a requested move. A plain position write: no
/// `NeedsLocomotionRebuild`, and it does not feed the respawn-thrashing
/// window — a player who resets three times in a minute has been lost
/// three times, not corrupted.
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_player_move(
    mut request: ResMut<PlayerMoveRequest>,
    mut query: Query<
        (
            &mut Position,
            &mut Rotation,
            &mut LinearVelocity,
            &mut AngularVelocity,
        ),
        With<LocalPlayer>,
    >,
    hm_res: Option<Res<crate::terrain::FinishedHeightMap>>,
    room: Option<Res<LiveRoomRecord>>,
    traveling: Option<Res<TravelingTo>>,
    time: Res<Time>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
    mut toasts: ResMut<crate::ui::toast::Toasts>,
) {
    let Some(what) = request.0.take() else {
        return;
    };
    // Taken whatever happens: a request the world cannot serve must not
    // fire the instant it can, minutes later and somewhere else.
    let now = time.elapsed_secs_f64();
    if let Some(reason) = return_to_spawn_blocked(hm_res.is_some(), traveling.is_some()) {
        toasts.warn(reason, now);
        return;
    }
    let (Some(hm_res), Ok((mut pos, mut rot, mut lin_vel, mut ang_vel))) =
        (hm_res, query.single_mut())
    else {
        return;
    };
    let from_y = pos.y;
    let (destination, told) = match what {
        PlayerMove::ReturnToSpawn => {
            let landing = room.as_deref().and_then(|r| r.0.default_landing);
            let pose = recovery_pose(&hm_res.0, landing);
            rot.0 = pose.rot;
            (pose.pos, RecoveryReason::Requested.toast())
        }
        PlayerMove::GoTo(target) => {
            // Put down ON the ground beside it, whatever height the object
            // itself sits at — an editor selection can be a sky platform.
            let mut at = target;
            at.y = hm_res.world_height_at(at.x, at.z) + cfg::SPAWN_HEIGHT_OFFSET;
            (at, "Moved to your selection.")
        }
    };
    pos.0 = destination;
    lin_vel.0 = Vec3::ZERO;
    ang_vel.0 = Vec3::ZERO;
    toasts.info(told, now);
    session_log.info(
        now,
        crate::diagnostics::event::EventPayload::RespawnTriggered {
            fell_to_y: crate::diagnostics::event::finite_or_sentinel(from_y),
            ground_y: crate::diagnostics::event::finite_or_sentinel(destination.y),
        },
    );
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn respawn_if_fallen(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut Position,
            &mut Rotation,
            &mut LinearVelocity,
            &mut AngularVelocity,
        ),
        With<LocalPlayer>,
    >,
    hm_res: Option<Res<crate::terrain::FinishedHeightMap>>,
    room: Option<Res<LiveRoomRecord>>,
    time: Res<Time>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
    mut recent_respawns: ResMut<crate::diagnostics::anomaly::RecentRespawns>,
    mut toasts: ResMut<crate::ui::toast::Toasts>,
) {
    let Ok((entity, mut pos, mut rot, mut lin_vel, mut ang_vel)) = query.single_mut() else {
        return;
    };
    let Some(hm_res) = hm_res else {
        return;
    };
    // Non-finite state check FIRST (#867): once a body reaches NaN/Inf
    // every comparison below misbehaves (NaN fails the early-return
    // test, so the old code "respawned" every frame without healing)
    // and the heightmap sample runs on garbage coordinates.
    let finite =
        pos.0.is_finite() && rot.0.is_finite() && lin_vel.0.is_finite() && ang_vel.0.is_finite();
    let hm = &hm_res.0;
    let extent = (hm.width() - 1) as f32 * hm.scale();
    let half = extent * 0.5;
    let local_ground = if finite {
        let hm_x = (pos.x + half).clamp(0.0, extent);
        let hm_z = (pos.z + half).clamp(0.0, extent);
        hm.get_height_at(hm_x, hm_z)
    } else {
        // No meaningful sample under a non-finite body; log-only value.
        0.0
    };
    let Some(reason) = automatic_recovery(finite, pos.0, local_ground, half) else {
        return;
    };
    let non_finite = reason == RecoveryReason::NonFinite;
    // Depth the player fell to, before the respawn overwrites their position.
    let fell_to_y = pos.y;
    let landing = room.as_deref().and_then(|r| r.0.default_landing);
    let pose = recovery_pose(hm, landing);
    pos.0 = pose.pos;
    rot.0 = pose.rot;
    lin_vel.0 = Vec3::ZERO;
    ang_vel.0 = Vec3::ZERO;
    let now = time.elapsed_secs_f64();
    // The teleport used to be silent (#842) — one instant the player is
    // falling, the next they are somewhere else with no explanation.
    toasts.warn(reason.toast(), now);
    crate::diagnostics::samplers::player_respawned(&mut metrics);
    // Feed the respawn-thrashing window (#672) alongside the monotonic metric.
    recent_respawns.note(now);
    // Typed event (#635d) — the metric counts respawns, this records each one's
    // fall depth vs. the terrain height it dropped through, for the timeline.
    // Sentinel-clamped (#868): during the #867 meltdown these fields went
    // NaN and serde_json wrote them as `null`, breaking the NDJSON schema
    // for the offline analyzer.
    session_log.warn(
        now,
        crate::diagnostics::event::EventPayload::RespawnTriggered {
            fell_to_y: crate::diagnostics::event::finite_or_sentinel(fell_to_y),
            ground_y: crate::diagnostics::event::finite_or_sentinel(local_ground),
        },
    );
    // Escalation (#867): a corrupted solver re-launches the body no
    // matter how many times Position is rewritten (the meltdown fell
    // ~10× deeper per frame across 1,489 respawns), and a non-finite
    // body never integrates back to sanity. Strip + rebuild the whole
    // physics body via the locomotion hot-swap machinery — fresh
    // collider, fresh contact pairs — so the world is recoverable
    // without a restart. Deferred automatically while the visuals-edit
    // freeze parks the chassis (the rebuild system's
    // `Without<VisualsEditFreeze>` gate), though a parked body cannot
    // fall here in the first place.
    let respawns_recent = recent_respawns.count_recent(now);
    if reason.may_escalate() && (non_finite || respawns_recent >= BODY_REBUILD_AFTER_RESPAWNS) {
        commands
            .entity(entity)
            .insert(super::hotswap::NeedsLocomotionRebuild);
        session_log.warn(
            now,
            crate::diagnostics::event::EventPayload::PhysicsBodyRebuilt {
                respawns_recent,
                non_finite,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1240 f169. Sequence: fly the airship out over the sea until the
    /// ground ends. The fall test cannot fire out there — the heightmap
    /// sample is CLAMPED into the extent, so the ground reference past the
    /// edge is the boundary height and an aircraft cruising above it
    /// satisfies nothing. Recovery required descending into the void until
    /// the fall test finally caught, which nothing told the user.
    #[test]
    fn leaving_the_world_recovers_without_falling_first() {
        let half = 512.0;
        let cruising = Vec3::new(half + 300.0, 200.0, 0.0);
        assert_eq!(
            automatic_recovery(true, cruising, 0.0, half),
            Some(RecoveryReason::LeftTheWorld)
        );
        // Well inside the edge, level with the ground: nothing happens.
        assert_eq!(
            automatic_recovery(true, Vec3::new(10.0, 5.0, -10.0), 4.0, half),
            None
        );
        // The margin is a band, not a hard wall at the extent — a hull
        // nosing over the boundary is still in the world.
        assert_eq!(
            automatic_recovery(true, Vec3::new(half + 1.0, 5.0, 0.0), 4.0, half),
            None
        );
    }

    /// #867's ordering, kept: the non-finite check comes FIRST, because
    /// every comparison below it misbehaves on NaN (NaN fails the fall
    /// test, so the pre-#867 code "respawned" every frame without healing).
    #[test]
    fn a_non_finite_body_outranks_every_other_reason() {
        let nonsense = Vec3::new(f32::NAN, f32::NEG_INFINITY, f32::NAN);
        assert_eq!(
            automatic_recovery(false, nonsense, 0.0, 512.0),
            Some(RecoveryReason::NonFinite)
        );
        assert_eq!(
            automatic_recovery(true, Vec3::new(0.0, -100.0, 0.0), 4.0, 512.0),
            Some(RecoveryReason::FellThrough)
        );
    }

    /// #1240 f159. A user-requested reset must NOT escalate to a physics
    /// body rebuild: three deliberate resets in a minute mean somebody got
    /// lost three times, not that the solver broke. The automatic reasons
    /// keep #867's escalation.
    #[test]
    fn only_automatic_recoveries_may_rebuild_the_body() {
        assert!(!RecoveryReason::Requested.may_escalate());
        for reason in [
            RecoveryReason::NonFinite,
            RecoveryReason::FellThrough,
            RecoveryReason::LeftTheWorld,
        ] {
            assert!(reason.may_escalate(), "{reason:?}");
        }
    }

    /// #1240 f169. Sequence: drive the hover-boat across the map edge.
    /// Buoyancy used to return outright the instant the hull passed the
    /// extent — all lift gone, no cue, the boat sinks. The band must reach
    /// zero no sooner than the margin at which the player is recovered, or
    /// the silent sinking is merely postponed.
    #[test]
    fn buoyancy_fades_across_the_same_band_the_recovery_uses() {
        let half = 512.0;
        assert_eq!(edge_buoyancy_falloff(Vec3::new(0.0, 0.0, 0.0), half), 1.0);
        assert_eq!(edge_buoyancy_falloff(Vec3::new(half, 0.0, 0.0), half), 1.0);
        let mid = edge_buoyancy_falloff(
            Vec3::new(half + cfg::WORLD_EDGE_MARGIN * 0.5, 0.0, 0.0),
            half,
        );
        assert!((mid - 0.5).abs() < 1e-5, "{mid}");
        let at_recovery = Vec3::new(half + cfg::WORLD_EDGE_MARGIN, 0.0, 0.0);
        assert_eq!(edge_buoyancy_falloff(at_recovery, half), 0.0);
        assert!(
            automatic_recovery(true, at_recovery, 0.0, half).is_some(),
            "lift must not run out before the recovery fires"
        );
        // Either axis, and the deeper crossing wins.
        assert_eq!(
            edge_buoyancy_falloff(Vec3::new(0.0, 0.0, half + 500.0), half),
            0.0
        );
    }

    /// #1244 f148. Sequence: click a region asset in the tree; the panel
    /// shows its properties and the gizmo attaches — to whichever live
    /// instance is nearest the CAMERA, which for a distant or
    /// behind-the-camera asset is still arbitrarily far away. Half the
    /// time selecting from the tree produced no visible result, and the
    /// user could not tell "nothing was selected" from "the thing is
    /// 200 m behind me".
    #[test]
    fn going_to_a_selection_stands_beside_it_on_the_players_own_side() {
        let centre = Vec3::new(100.0, 12.0, 0.0);
        // Approaching from the east puts the player east of it.
        let from_east = go_to_pose(centre, 3.0, Vec3::new(400.0, 0.0, 0.0));
        assert!(from_east.x > centre.x, "{from_east:?}");
        assert!((from_east.x - centre.x - (3.0 + GO_TO_STANDOFF_M)).abs() < 1e-4);
        assert_eq!(from_east.z, centre.z, "no sideways drift");

        // …and from the west, west of it: the shortest trip either way.
        let from_west = go_to_pose(centre, 3.0, Vec3::new(-400.0, 0.0, 0.0));
        assert!(from_west.x < centre.x, "{from_west:?}");

        // A bigger object is stood further off, so it fills the view
        // rather than swallowing the camera.
        let near = go_to_pose(centre, 1.0, Vec3::new(400.0, 0.0, 0.0));
        let far = go_to_pose(centre, 40.0, Vec3::new(400.0, 0.0, 0.0));
        assert!(far.x > near.x);

        // Standing exactly ON it is degenerate, and must never resolve to
        // the object's own centre.
        let coincident = go_to_pose(centre, 2.0, centre);
        assert_ne!(coincident, centre);
        assert!((coincident - centre).length() > 2.0);
    }

    /// #1240 f159, in the shape of `travel::home_travel_blocked`: ordered
    /// most-fundamental-first, so the reason names the thing the user has
    /// to resolve rather than whichever check ran first.
    #[test]
    fn return_to_spawn_names_the_most_fundamental_blocker() {
        assert_eq!(return_to_spawn_blocked(true, false), None);
        assert_eq!(
            return_to_spawn_blocked(false, true),
            Some("The ground is still being built.")
        );
        assert_eq!(
            return_to_spawn_blocked(true, true),
            Some("You're already travelling somewhere.")
        );
    }

    /// Each reason says what happened. "You fell out of the world" after
    /// flying level for a minute is a confusing thing to read, and the
    /// requested reset is not an accident to be warned about.
    #[test]
    fn every_recovery_reason_has_its_own_sentence() {
        let lines = [
            RecoveryReason::NonFinite.toast(),
            RecoveryReason::FellThrough.toast(),
            RecoveryReason::LeftTheWorld.toast(),
            RecoveryReason::Requested.toast(),
        ];
        assert_ne!(lines[1], lines[2]);
        assert_ne!(lines[1], lines[3]);
        for line in lines {
            assert!(line.contains("Returned to spawn"), "{line}");
        }
    }
}
