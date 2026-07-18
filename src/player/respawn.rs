//! Fall-through recovery: teleport the local player back to a fresh
//! spawn-scatter point when they drop below the terrain, with the
//! respawn metric + typed timeline event.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::config::rover as cfg;
use crate::state::{LiveRoomRecord, LocalPlayer};

use super::random_spawn_xz;

/// Windowed respawn count at which the teleport escalates to a full
/// physics-body rebuild (#867). One or two catches are ordinary falls;
/// three inside [`RESPAWN_WINDOW_SECS`](crate::diagnostics::anomaly) is
/// the thrash signature — the teleport is not sticking, so position
/// writes alone won't recover.
const BODY_REBUILD_AFTER_RESPAWNS: u32 = 3;

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
    let non_finite =
        !(pos.0.is_finite() && rot.0.is_finite() && lin_vel.0.is_finite() && ang_vel.0.is_finite());
    let hm = &hm_res.0;
    let extent = (hm.width() - 1) as f32 * hm.scale();
    let half = extent * 0.5;
    let local_ground = if non_finite {
        // No meaningful sample under a non-finite body; log-only value.
        0.0
    } else {
        let hm_x = (pos.x + half).clamp(0.0, extent);
        let hm_z = (pos.z + half).clamp(0.0, extent);
        hm.get_height_at(hm_x, hm_z)
    };
    if !non_finite && pos.y > local_ground - cfg::FALL_BELOW_GROUND {
        return;
    }
    // Depth the player fell to, before the respawn overwrites their position.
    let fell_to_y = pos.y;
    let centre = extent * 0.5;
    // Recovery pose (#745): the room's default landing when configured,
    // otherwise the legacy random scatter. The (x, z) is clamped into the
    // terrain extent so a landing aimed outside the heightmap (possible in
    // a hand-edited record — sanitize only bounds magnitude) can't strand
    // the player on an endless fall-respawn-fall loop over the void.
    let landing = room.as_deref().and_then(|r| r.0.default_landing);
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
    // An explicit landing height is honoured (sky-platform landings) but
    // floored at ground level here — respawning *below* the terrain would
    // re-trigger this system every frame.
    let floor_y = ground_y + cfg::SPAWN_HEIGHT_OFFSET;
    pos.0 = Vec3::new(ox, explicit_y.map_or(floor_y, |y| y.max(floor_y)), oz);
    rot.0 = tilt * yaw;
    lin_vel.0 = Vec3::ZERO;
    ang_vel.0 = Vec3::ZERO;
    let now = time.elapsed_secs_f64();
    // The teleport used to be silent (#842) — one instant the player is
    // falling, the next they are somewhere else with no explanation.
    toasts.warn("Returned to spawn — you fell out of the world.", now);
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
    if non_finite || respawns_recent >= BODY_REBUILD_AFTER_RESPAWNS {
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
