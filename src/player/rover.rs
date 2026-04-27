//! HoverRover archetype — airship-style visual rig and the physics systems
//! (suspension, buoyancy, drive, uprighting) that move the chassis.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::config::airship as ac;
use crate::config::rover as cfg;
use crate::pds::{AvatarBody, RoverPhenotype};
use crate::protocol::PontoonShape;
use crate::state::{LiveAvatarRecord, LocalPlayer, TravelingTo};
use crate::world_builder::{OverlandsFoliageTasks, build_procedural_material};

use super::{CORNER_OFFSETS, HoverRoverArchetype, MastTip, RoverSail};

#[allow(clippy::too_many_arguments)]
pub fn rebuild_airship_children(
    commands: &mut Commands,
    entity: Entity,
    phen: &RoverPhenotype,
    existing_children: Option<&Children>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    foliage_tasks: &mut OverlandsFoliageTasks,
    avatar_override: Option<&Handle<StandardMaterial>>,
) {
    if let Some(children) = existing_children {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }

    let hull_l = phen.hull_length.0;
    let hull_w = phen.hull_width.0;
    let hull_d = phen.hull_depth.0;
    let mast_h = phen.mast_height.0;
    let [mx, mz] = phen.mast_offset.0;
    let mast_top_y = mast_h;
    let drop_y = -phen.strut_drop.0 * hull_d;

    // Hull, pontoons, and sail want `double_sided=true` regardless of what
    // `render_properties` infers from the texture: the hull/pontoon meshes
    // are open-bottomed shells and the sail is a flat quad, so back-face
    // culling would leave holes when the camera sees them from inside.
    let mut build_slot = |m: &crate::pds::SovereignMaterialSettings,
                          double_sided: bool|
     -> Handle<StandardMaterial> {
        let h = build_procedural_material(materials, foliage_tasks, m);
        if double_sided && let Some(mat) = materials.get_mut(&h) {
            mat.double_sided = true;
            mat.cull_mode = None;
        }
        h
    };

    let hull_mat = build_slot(&phen.hull_material, true);
    let pontoon_mat = build_slot(&phen.pontoon_material, true);
    let mast_mat = build_slot(&phen.mast_material, false);
    let strut_mat = build_slot(&phen.strut_material, false);
    let sail_mat = avatar_override
        .cloned()
        .unwrap_or_else(|| build_slot(&phen.sail_material, true));

    let pontoon_shape = phen.pontoon_shape;
    let pontoon_spread = phen.pontoon_spread.0;
    let pontoon_length = phen.pontoon_length.0;
    let pontoon_width = phen.pontoon_width.0;
    let pontoon_height = phen.pontoon_height.0;
    let mast_r = phen.mast_radius.0;
    let sail_size = phen.sail_size.0;

    let hull_mesh = meshes.add(with_tangents(build_v_hull_mesh(hull_l, hull_w, hull_d)));
    let pontoon_mesh = match pontoon_shape {
        PontoonShape::Capsule => meshes.add(with_tangents(
            Capsule3d::new(pontoon_width / 2.0, pontoon_length)
                .mesh()
                .build(),
        )),
        PontoonShape::VHull => meshes.add(with_tangents(build_v_hull_mesh(
            pontoon_length,
            pontoon_width,
            pontoon_height,
        ))),
    };
    let pontoon_rot = match pontoon_shape {
        PontoonShape::Capsule => Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        PontoonShape::VHull => Quat::IDENTITY,
    };
    let strut_mesh = meshes.add(with_tangents(
        Capsule3d::new(ac::STRUT_THICKNESS * 0.5, pontoon_spread * 2.0)
            .mesh()
            .build(),
    ));
    let mast_mesh = meshes.add(with_tangents(Cylinder::new(mast_r, mast_h).mesh().build()));
    let mast_tip_mesh = meshes.add(with_tangents(Sphere::new(mast_r).mesh().uv(16, 8)));
    let sail_mesh = meshes.add(with_tangents(
        Rectangle::new(sail_size, sail_size).mesh().build(),
    ));

    commands.entity(entity).with_children(|parent| {
        parent.spawn((
            Mesh3d(hull_mesh),
            MeshMaterial3d(hull_mat.clone()),
            Transform::IDENTITY,
        ));

        parent.spawn((
            Mesh3d(pontoon_mesh.clone()),
            MeshMaterial3d(pontoon_mat.clone()),
            Transform::from_xyz(-pontoon_spread, drop_y, 0.0).with_rotation(pontoon_rot),
        ));
        parent.spawn((
            Mesh3d(pontoon_mesh),
            MeshMaterial3d(pontoon_mat),
            Transform::from_xyz(pontoon_spread, drop_y, 0.0).with_rotation(pontoon_rot),
        ));

        parent.spawn((
            Mesh3d(strut_mesh.clone()),
            MeshMaterial3d(strut_mat.clone()),
            Transform::from_xyz(0.0, drop_y, hull_l * 0.3)
                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
        ));
        parent.spawn((
            Mesh3d(strut_mesh),
            MeshMaterial3d(strut_mat),
            Transform::from_xyz(0.0, drop_y, -hull_l * 0.3)
                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
        ));

        parent.spawn((
            Mesh3d(mast_mesh),
            MeshMaterial3d(mast_mat.clone()),
            Transform::from_xyz(mx, mast_h * 0.5, mz),
        ));
        parent.spawn((
            Mesh3d(mast_tip_mesh),
            MeshMaterial3d(mast_mat),
            Transform::from_xyz(mx, mast_h, mz),
            MastTip,
        ));

        parent.spawn((
            Mesh3d(sail_mesh),
            MeshMaterial3d(sail_mat),
            Transform::from_xyz(mx, mast_top_y - sail_size * 0.5, mz + sail_size * 0.5)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
            RoverSail,
        ));
    });
}

/// Generate tangent space on a mesh before storing it. Tangents are required
/// by PBR normal maps produced by `bevy_symbios_texture` — without them the
/// shader samples garbage in the TBN matrix and lighting goes haywire.
pub(super) fn with_tangents(mut mesh: Mesh) -> Mesh {
    let _ = mesh.generate_tangents();
    mesh
}

fn build_v_hull_mesh(hull_length: f32, hull_width: f32, hull_depth: f32) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::Indices;
    use bevy::render::render_resource::PrimitiveTopology;

    const SEGMENTS: usize = 20;

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity((SEGMENTS + 1) * 3);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity((SEGMENTS + 1) * 3);
    let mut indices: Vec<u32> = Vec::with_capacity(SEGMENTS * 12);

    for i in 0..=SEGMENTS {
        let t = i as f32 / SEGMENTS as f32;
        let z = -hull_length * 0.5 + t * hull_length;
        let scale = (t * std::f32::consts::PI).sin();
        let r = (hull_width * 0.5) * scale;
        let keel_y = -hull_depth * scale;
        positions.push([-r, 0.0, z]);
        positions.push([0.0, keel_y, z]);
        positions.push([r, 0.0, z]);
        uvs.push([0.0, t]);
        uvs.push([0.5, t]);
        uvs.push([1.0, t]);
    }

    for i in 0..SEGMENTS {
        let l0 = (i * 3) as u32;
        let k0 = l0 + 1;
        let r0 = l0 + 2;
        let l1 = ((i + 1) * 3) as u32;
        let k1 = l1 + 1;
        let r1 = l1 + 2;

        indices.extend_from_slice(&[l0, k0, k1]);
        indices.extend_from_slice(&[l0, k1, l1]);
        indices.extend_from_slice(&[k0, r0, r1]);
        indices.extend_from_slice(&[k0, r1, k1]);
        indices.extend_from_slice(&[l0, l1, r1]);
        indices.extend_from_slice(&[l0, r1, r0]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh.duplicate_vertices();
    mesh.compute_flat_normals();
    let _ = mesh.generate_tangents();
    mesh
}

// ---------------------------------------------------------------------------
// Physics systems
// ---------------------------------------------------------------------------

/// Push kinematics changes from the live record onto the chassis's Mass
/// and Damping components every fixed step, so slider tweaks take effect
/// immediately without requiring an archetype rebuild.
pub(super) fn sync_local_chassis_physics(
    live: Res<LiveAvatarRecord>,
    mut query: Query<(&mut Mass, &mut LinearDamping, &mut AngularDamping), With<LocalPlayer>>,
) {
    let Ok((mut mass, mut lin_damp, mut ang_damp)) = query.single_mut() else {
        return;
    };
    let (m, ld, ad) = match &live.0.body {
        AvatarBody::HoverRover { kinematics, .. } => (
            kinematics.mass.0,
            kinematics.linear_damping.0,
            kinematics.angular_damping.0,
        ),
        AvatarBody::Humanoid { kinematics, .. } => (
            kinematics.mass.0,
            kinematics.linear_damping.0,
            cfg::ANGULAR_DAMPING,
        ),
        AvatarBody::Unknown => return,
    };
    if mass.0 != m {
        mass.0 = m;
    }
    if lin_damp.0 != ld {
        lin_damp.0 = ld;
    }
    if ang_damp.0 != ad {
        ang_damp.0 = ad;
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn apply_suspension_forces(
    live: Res<LiveAvatarRecord>,
    mut query: Query<
        (Entity, Forces, &GlobalTransform),
        (With<LocalPlayer>, With<HoverRoverArchetype>),
    >,
    spatial_query: SpatialQuery,
) {
    let AvatarBody::HoverRover { kinematics, .. } = &live.0.body else {
        return;
    };
    let Ok((chassis_entity, mut forces, global_tf)) = query.single_mut() else {
        return;
    };

    let ray_max = kinematics.suspension_rest_length.0 + 1.5;
    let chassis_tf = global_tf.compute_transform();
    let filter = SpatialQueryFilter::default().with_excluded_entities([chassis_entity]);
    let lin_vel = forces.linear_velocity();
    let ang_vel = forces.angular_velocity();
    let center_of_mass = global_tf.translation();

    for offset in CORNER_OFFSETS {
        let local_offset = Vec3::from_array(offset);
        let world_origin = chassis_tf.transform_point(local_offset);

        let Some(hit) = spatial_query.cast_ray(world_origin, Dir3::NEG_Y, ray_max, true, &filter)
        else {
            continue;
        };

        let compression = kinematics.suspension_rest_length.0 - hit.distance;
        if compression > 0.0 {
            let r = world_origin - center_of_mass;
            let point_vel = lin_vel + ang_vel.cross(r);
            let closing_speed = -point_vel.dot(hit.normal);
            let spring_force = kinematics.suspension_stiffness.0 * compression;
            let damping_force = kinematics.suspension_damping.0 * closing_speed;
            let total_force = (spring_force + damping_force).max(0.0);
            forces.apply_force_at_point(Vec3::Y * total_force, world_origin);
        }
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn apply_rover_drive_forces(
    live: Res<LiveAvatarRecord>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(Forces, &GlobalTransform), (With<LocalPlayer>, With<HoverRoverArchetype>)>,
    traveling: Option<Res<TravelingTo>>,
) {
    if traveling.is_some() {
        return;
    }
    let AvatarBody::HoverRover { kinematics, .. } = &live.0.body else {
        return;
    };
    let Ok((mut forces, global_tf)) = query.single_mut() else {
        return;
    };

    let lin_vel = forces.linear_velocity();
    let forward = global_tf.forward().as_vec3();
    let flat_forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let local_up = global_tf.up().as_vec3();
    let right = global_tf.right().as_vec3();

    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        forces.apply_force(flat_forward * kinematics.drive_force.0);
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        forces.apply_force(-flat_forward * kinematics.drive_force.0);
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        forces.apply_torque(local_up * kinematics.turn_torque.0);
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        forces.apply_torque(-local_up * kinematics.turn_torque.0);
    }

    let lateral_vel = right.dot(lin_vel);
    forces.apply_force(-right * lateral_vel * kinematics.lateral_grip.0);

    if keyboard.pressed(KeyCode::Space) {
        forces.apply_force(Vec3::Y * kinematics.jump_force.0);
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn apply_rover_uprighting_force(
    live: Res<LiveAvatarRecord>,
    mut query: Query<(Forces, &GlobalTransform), (With<LocalPlayer>, With<HoverRoverArchetype>)>,
) {
    let AvatarBody::HoverRover { kinematics, .. } = &live.0.body else {
        return;
    };
    let Ok((mut forces, global_tf)) = query.single_mut() else {
        return;
    };
    let vehicle_up = global_tf.up().as_vec3();
    forces.apply_torque(vehicle_up.cross(Vec3::Y) * kinematics.uprighting_torque.0);
}

/// Sample the runtime water-surface registry at each chassis corner and
/// apply per-corner buoyancy + flow current. The previous flat-only
/// implementation used a single Y threshold and pushed every corner along
/// world-Y; this version uses [`WaterSurfaces::query`] so the lift acts
/// along the surface normal — meaning a tilted pond pushes an immersed
/// rover perpendicular to its slope rather than straight up — and a
/// non-zero `flow_strength` adds a tangent push along the surface
/// gradient that turns a tilted pond into a flowing river.
///
/// Force application uses `apply_force_at_point` per corner — matching the
/// per-corner suspension pattern in [`apply_suspension_forces`] — so
/// asymmetric submersion produces emergent pitch and roll torques without
/// any explicit torque computation. Per-corner buoyancy is divided by the
/// corner count so a fully-submerged chassis sees the same integrated lift
/// as a chassis-centre single-point version. Flow force is applied in
/// full per corner; multiple submerged corners pushing in the same
/// downhill direction add up to the expected total.
#[allow(clippy::type_complexity)]
pub(super) fn apply_buoyancy_forces(
    live: Res<LiveAvatarRecord>,
    water_surfaces: Res<crate::water::WaterSurfaces>,
    hm_res: Option<Res<crate::terrain::FinishedHeightMap>>,
    mut query: Query<(Forces, &GlobalTransform), (With<LocalPlayer>, With<HoverRoverArchetype>)>,
) {
    let AvatarBody::HoverRover { kinematics, .. } = &live.0.body else {
        return;
    };
    let Ok((mut forces, global_tf)) = query.single_mut() else {
        return;
    };
    if let Some(hm_res) = hm_res.as_deref() {
        let hm = &hm_res.0;
        let half = (hm.width() - 1) as f32 * hm.scale() * 0.5;
        let p = global_tf.translation();
        if p.x.abs() > half || p.z.abs() > half {
            return;
        }
    }

    let chassis_tf = global_tf.compute_transform();
    let lin_vel = forces.linear_velocity();
    let ang_vel = forces.angular_velocity();
    let center_of_mass = global_tf.translation();
    let buoyancy_scale = 1.0 / super::CORNER_OFFSETS.len() as f32;

    for offset in super::CORNER_OFFSETS {
        let local_offset = Vec3::from_array(offset);
        let world_origin = chassis_tf.transform_point(local_offset);

        // `water_rest_length` shifts the buoyancy plane upward relative to
        // the visible surface so a partially-submerged chassis sits with
        // some hull above water. The signed-distance query returns depth
        // below the visible surface; we add `water_rest_length` so depth
        // is taken against the buoyancy plane along the same normal.
        let Some(q) = water_surfaces.query(world_origin) else {
            continue;
        };
        let depth =
            (q.depth + kinematics.water_rest_length.0).clamp(0.0, kinematics.buoyancy_max_depth.0);
        if depth <= 0.0 {
            continue;
        }
        let r = world_origin - center_of_mass;
        let point_vel = lin_vel + ang_vel.cross(r);
        // Drag opposes the velocity component along the surface normal —
        // movement *parallel* to the surface should not be damped by
        // buoyancy itself (water's lateral resistance is captured by the
        // body's `linear_damping`).
        let normal_vel = point_vel.dot(q.normal);
        let lift = kinematics.buoyancy_strength.0 * depth;
        let drag = -kinematics.buoyancy_damping.0 * normal_vel;
        forces.apply_force_at_point(q.normal * ((lift + drag) * buoyancy_scale), world_origin);

        // Flow current — projected gravity tangent to the surface, scaled by
        // submerged depth so a corner barely under water feels less push
        // than one fully immersed. Skip zero `flow_strength` and zero
        // `flow_dir` (flat water) so the common case takes no extra work.
        if q.flow_strength > 0.0 && q.flow_dir != Vec3::ZERO {
            let flow_force = q.flow_dir * q.flow_strength * depth;
            forces.apply_force_at_point(flow_force, world_origin);
        }
    }
}
