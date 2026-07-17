//! Fall-through recovery: teleport the local player back to a fresh
//! spawn-scatter point when they drop below the terrain, with the
//! respawn metric + typed timeline event.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::config::rover as cfg;
use crate::state::{LiveRoomRecord, LocalPlayer};

use super::random_spawn_xz;

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn respawn_if_fallen(
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
    time: Res<Time>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
    mut recent_respawns: ResMut<crate::diagnostics::anomaly::RecentRespawns>,
    mut toasts: ResMut<crate::ui::toast::Toasts>,
) {
    let Ok((mut pos, mut rot, mut lin_vel, mut ang_vel)) = query.single_mut() else {
        return;
    };
    let Some(hm_res) = hm_res else {
        return;
    };
    let hm = &hm_res.0;
    let extent = (hm.width() - 1) as f32 * hm.scale();
    let half = extent * 0.5;
    let hm_x = (pos.x + half).clamp(0.0, extent);
    let hm_z = (pos.z + half).clamp(0.0, extent);
    let local_ground = hm.get_height_at(hm_x, hm_z);
    if pos.y > local_ground - cfg::FALL_BELOW_GROUND {
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
    session_log.warn(
        now,
        crate::diagnostics::event::EventPayload::RespawnTriggered {
            fell_to_y,
            ground_y: local_ground,
        },
    );
}
