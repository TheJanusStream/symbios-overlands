//! Fall-through recovery: teleport the local player back to a fresh
//! spawn-scatter point when they drop below the terrain, with the
//! respawn metric + typed timeline event.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::config::rover as cfg;
use crate::state::LocalPlayer;

use super::random_spawn_xz;

#[allow(clippy::type_complexity)]
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
    time: Res<Time>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
    mut recent_respawns: ResMut<crate::diagnostics::anomaly::RecentRespawns>,
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
    let (ox, oz) = random_spawn_xz();
    let hm_x = (centre + ox).clamp(0.0, extent);
    let hm_z = (centre + oz).clamp(0.0, extent);
    let ground_y = hm.get_height_at(hm_x, hm_z);
    let surface_normal = hm.get_normal_at(hm_x, hm_z);
    let tilt = Quat::from_rotation_arc(Vec3::Y, Vec3::from_array(surface_normal));
    pos.0 = Vec3::new(ox, ground_y + cfg::SPAWN_HEIGHT_OFFSET, oz);
    rot.0 = tilt;
    lin_vel.0 = Vec3::ZERO;
    ang_vel.0 = Vec3::ZERO;
    let now = time.elapsed_secs_f64();
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
