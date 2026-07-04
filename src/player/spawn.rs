//! Local-avatar spawn: the `OnEnter(InGame)` chassis + preset + visuals
//! assembly, and the shared chassis-root bundle the #670 easing guard
//! tests against.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::boot_params::TargetPos;
use crate::config::rover as cfg;
use crate::state::{LiveAvatarRecord, LocalPlayer, PendingSpawnPlacement};

use super::preset::build_preset_components;
use super::{random_spawn_xz, visuals};

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_local_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    hm_res: Res<crate::terrain::FinishedHeightMap>,
    live: Res<LiveAvatarRecord>,
    placement: Option<Res<PendingSpawnPlacement>>,
    mut avatar_deps: visuals::AvatarSpawnDeps,
) {
    let hm = &hm_res.0;
    let extent = (hm.width() - 1) as f32 * hm.scale();
    let half = extent * 0.5;
    let centre = half;

    // Pick (rx, rz) from the URL/CLI placement when supplied, falling back to
    // the random spawn-scatter. World coordinates are centred on (0, 0); the
    // heightmap sample uses (centre + x, centre + z).
    let (rx, rz) = match placement.as_deref().and_then(|p| p.pos) {
        Some(TargetPos { x, z, .. }) => (x.clamp(-half, half), z.clamp(-half, half)),
        None => random_spawn_xz(),
    };
    let hm_x = (centre + rx).clamp(0.0, extent);
    let hm_z = (centre + rz).clamp(0.0, extent);
    let ground_y = hm.get_height_at(hm_x, hm_z);
    let surface_normal = hm.get_normal_at(hm_x, hm_z);
    let tilt = Quat::from_rotation_arc(Vec3::Y, Vec3::from_array(surface_normal));
    // Apply yaw on top of the surface tilt so a landmark "facing N" lands the
    // chassis aimed at -Z while still resting flush on the slope.
    let yaw = placement
        .as_deref()
        .and_then(|p| p.yaw_deg)
        .map(|deg| Quat::from_rotation_y(deg.to_radians()))
        .unwrap_or(Quat::IDENTITY);
    let rotation = tilt * yaw;
    // y override (`pos=x,y,z`) bypasses the heightmap sample; the drop-pin
    // form (`pos=x,z`) keeps the heightmap-resolved height.
    let oy = match placement.as_deref().and_then(|p| p.pos).and_then(|p| p.y) {
        Some(y) => y,
        None => ground_y + cfg::SPAWN_HEIGHT_OFFSET,
    };
    let (ox, oz) = (rx, rz);

    let entity = commands
        .spawn(chassis_root_bundle(
            Transform::from_xyz(ox, oy, oz).with_rotation(rotation),
        ))
        .id();

    // One-shot: remove the resource so a portal travel or fall-respawn
    // later in the session does not retroactively reapply this placement.
    if placement.is_some() {
        commands.remove_resource::<PendingSpawnPlacement>();
    }

    build_preset_components(&mut commands, entity, &live.0.locomotion);
    visuals::spawn_avatar_visuals(
        &mut commands,
        entity,
        &live.0.visuals,
        None,
        &mut meshes,
        &mut materials,
        &mut images,
        &mut avatar_deps,
        true,
    );
}

/// Preset-independent components of the local chassis root, shared by the
/// live spawn path and the #670 regression test so the two can't drift.
///
/// `TransformInterpolation` is load-bearing: Avian steps physics (and the
/// `Position` → `Transform` writeback) entirely inside `FixedPostUpdate`
/// at the 64 Hz fixed timestep, so without easing the chassis `Transform`
/// holds still on tick-less render frames and the own avatar judders at
/// the fixed-vs-refresh beat (~4 Hz on a 60 Hz display) — remote avatars
/// don't, because the network smoother repositions them every render
/// frame (#670). The easing writes the smoothed pose in
/// `RunFixedMainLoop`, before `Update`, so per-frame readers such as the
/// camera follow see it, while `FixedUpdate` systems (drive controllers,
/// transform broadcast) still read true tick poses. Transform writes
/// outside the fixed schedules — portal teleports, the terrain-hot-load
/// lift — are detected as teleports and snap for that timestep, which is
/// the wanted shape.
fn chassis_root_bundle(transform: Transform) -> impl Bundle {
    (
        transform,
        Visibility::default(),
        RigidBody::Dynamic,
        TransformInterpolation,
        CollidingEntities::default(),
        LocalPlayer,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The #670 guard: the chassis root must opt into transform easing.
    /// Avian writes `Transform` only on 64 Hz fixed ticks, so a chassis
    /// without `TransformInterpolation` visibly steps against the render
    /// rate. `#[require]` on the component chains in the per-axis easing
    /// components, so spawning the bundle in a bare `World` (no plugins)
    /// proves the whole easing state machinery lands on the entity — a
    /// regression that drops the component from `chassis_root_bundle`
    /// (the exact bundle `spawn_local_player` uses) fails here.
    #[test]
    fn chassis_root_opts_into_transform_easing() {
        let mut world = World::new();
        let entity = world.spawn(chassis_root_bundle(Transform::IDENTITY)).id();
        let e = world.entity(entity);
        assert!(
            e.contains::<TransformInterpolation>(),
            "chassis root must carry TransformInterpolation (#670)"
        );
        assert!(
            e.contains::<TranslationInterpolation>() && e.contains::<RotationInterpolation>(),
            "easing per-axis components must be required in by TransformInterpolation"
        );
        assert!(
            e.contains::<RigidBody>() && e.contains::<LocalPlayer>(),
            "bundle must still assemble the physics chassis root"
        );
    }
}
