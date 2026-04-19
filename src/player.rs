//! Local player plugin (formerly `rover.rs`): spawns and drives the local
//! avatar, hot-swaps archetypes when the owner edits their PDS avatar
//! record, and paints matching visuals on remote peers.
//!
//! Two archetypes are supported:
//!
//! - **HoverRover** — a `RigidBody::Dynamic` cuboid chassis with four
//!   raycast-suspension corners, buoyancy, airship-style visual children,
//!   and WASD drive (Hooke's-law spring, lateral grip, jump impulse).
//! - **Humanoid** — a capsule rigid body with `LockedAxes` holding it
//!   upright, a velocity-driven walk controller, and a jump impulse.
//!
//! Both archetypes read their kinematics from the live
//! [`LiveAvatarRecord`], so UI edits take effect the same frame the slider
//! moves. Changing `body` *variant* triggers the hot-swap system, which
//! tears down all archetype-specific components (colliders, markers,
//! child meshes) and rebuilds them in the new archetype's shape without
//! disturbing the parent `Transform` or rigid-body identity.

use crate::avatar::AvatarMaterial;
use crate::config::airship as ac;
use crate::config::rover as cfg;
use crate::config::terrain as tcfg;
use crate::pds::{AvatarBody, AvatarRecord, HumanoidPhenotype};
use crate::protocol::{AirshipParams, PontoonShape};
use crate::state::{
    AppState, CurrentRoomDid, LiveAvatarRecord, LocalPlayer, RemotePeer, TravelRequest,
};
use crate::world_builder::PortalMarker;

/// Snapshot of the last `AvatarRecord` whose visuals have been painted onto
/// a remote peer. `detect_remote_archetype_change` listens to the broad
/// `Changed<RemotePeer>` signal (which also fires on mute/handle/DID edits)
/// and compares against this snapshot so an unrelated field flip doesn't
/// re-enter the expensive visual rebuild path. The cheaper
/// `sync_mute_visibility` handles the mute toggle on its own.
#[derive(Component)]
struct AppliedAvatar(AvatarRecord);
use avian3d::prelude::*;
use bevy::prelude::*;

// Corner offsets in local space for the four suspension rays.
const CORNER_OFFSETS: [[f32; 3]; 4] = [
    [cfg::CHASSIS_X, -cfg::CHASSIS_Y, cfg::CHASSIS_Z],
    [-cfg::CHASSIS_X, -cfg::CHASSIS_Y, cfg::CHASSIS_Z],
    [cfg::CHASSIS_X, -cfg::CHASSIS_Y, -cfg::CHASSIS_Z],
    [-cfg::CHASSIS_X, -cfg::CHASSIS_Y, -cfg::CHASSIS_Z],
];

/// Draw an (x, z) pair uniformly distributed inside a square of
/// `SPAWN_SCATTER_SIZE` metres per side, centred on the origin.
fn random_spawn_xz() -> (f32, f32) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEED: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);
    let s = SEED.fetch_add(0xDA94_2042_E4DD_58B5, Ordering::Relaxed);
    let mut z = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    let u = (z as u32 as f32) / (u32::MAX as f32);
    let v = ((z >> 32) as u32 as f32) / (u32::MAX as f32);
    let side = cfg::SPAWN_SCATTER_SIZE;
    ((u - 0.5) * side, (v - 0.5) * side)
}

/// Visual water-plane altitude used by both terrain rendering and the
/// swimming/buoyancy system so the two stay in perfect agreement.
#[inline]
pub fn water_level_y() -> f32 {
    (tcfg::water::LEVEL_FACTOR * tcfg::HEIGHT_SCALE).max(0.001)
}

/// Marker placed on the solar-sail mesh child so the avatar system can find it.
#[derive(Component)]
pub struct RoverSail;

/// Marker placed on the mast-tip hemisphere child so the social-resonance
/// system can light it up when a peer is a mutual follow.
#[derive(Component)]
pub struct MastTip;

/// Marks an entity as currently using the HoverRover archetype.
/// Inserted by [`build_hover_rover_archetype`] and stripped by the
/// hot-swap system when the owner picks a different body variant.
#[derive(Component)]
pub struct HoverRoverArchetype;

/// Marks an entity as currently using the Humanoid archetype.
#[derive(Component)]
pub struct HumanoidArchetype;

/// Intermediate visual parent on a Humanoid. The rigid body never rotates —
/// the walk controller yaws this root to face the movement direction.
#[derive(Component)]
pub struct HumanoidVisualRoot;

/// Shoulder / hip joint pivots. The limb cylinder is a child offset downward,
/// so rotating this entity swings the limb from its top (not its middle).
#[derive(Component, Clone, Copy)]
pub struct HumanoidJoint {
    /// +1 for left (or forward-phase) limbs, -1 for right. Used to
    /// counter-rotate the animation pairs.
    pub phase_sign: f32,
    /// Additional phase offset in radians — legs are 180° out of phase with
    /// their paired arm so the gait alternates naturally.
    pub phase_offset: f32,
}

/// Chest-mounted profile badge quad. The avatar system paints the owner's
/// ATProto profile picture onto this material when one is available.
#[derive(Component)]
pub struct ChestBadge;

/// Request flag set when the local player's archetype needs to be
/// rebuilt on the main thread. This exists because Avian components
/// cannot be added/removed from `Query`-held mutable borrows — we have
/// to defer the surgery to a commands-only system.
#[derive(Component)]
struct NeedsArchetypeRebuild;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_local_player)
            .add_systems(
                Update,
                (
                    detect_local_archetype_change,
                    apply_local_archetype_rebuild,
                    detect_remote_archetype_change,
                    rebuild_local_visuals,
                    lift_player_above_new_ground,
                    animate_humanoid_limbs,
                    handle_portal_interaction,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    sync_local_chassis_physics,
                    apply_suspension_forces,
                    apply_buoyancy_forces,
                    apply_rover_drive_forces,
                    apply_rover_uprighting_force,
                    apply_humanoid_walk,
                    respawn_if_fallen,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

// ---------------------------------------------------------------------------
// Spawn / rebuild — local player
// ---------------------------------------------------------------------------

fn spawn_local_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    hm_res: Res<crate::terrain::FinishedHeightMap>,
    live: Res<LiveAvatarRecord>,
    travel_req: Option<Res<TravelRequest>>,
) {
    let hm = &hm_res.0;
    let extent = (hm.width() - 1) as f32 * hm.scale();
    let centre = extent * 0.5;

    // A pending `TravelRequest` (inserted by the portal-interaction system
    // immediately before a state transition) hijacks the spawn location so the
    // avatar materialises at the portal's exit coordinate in the new room.
    // Consume it so the next natural spawn/respawn falls back to the random
    // homeworld scatter.
    let (ox, oy, oz, tilt) = if let Some(req) = travel_req.as_deref() {
        let p = req.target_pos;
        (p.x, p.y, p.z, Quat::IDENTITY)
    } else {
        let (rx, rz) = random_spawn_xz();
        let hm_x = (centre + rx).clamp(0.0, extent);
        let hm_z = (centre + rz).clamp(0.0, extent);
        let ground_y = hm.get_height_at(hm_x, hm_z);
        let surface_normal = hm.get_normal_at(hm_x, hm_z);
        let tilt = Quat::from_rotation_arc(Vec3::Y, Vec3::from_array(surface_normal));
        (rx, ground_y + cfg::SPAWN_HEIGHT_OFFSET, rz, tilt)
    };

    if travel_req.is_some() {
        commands.remove_resource::<TravelRequest>();
    }

    let entity = commands
        .spawn((
            Transform::from_xyz(ox, oy, oz).with_rotation(tilt),
            Visibility::default(),
            RigidBody::Dynamic,
            CollidingEntities::default(),
            LocalPlayer,
        ))
        .id();

    build_archetype_components(&mut commands, entity, &live.0);
    build_archetype_visuals(
        &mut commands,
        entity,
        &live.0,
        None,
        None,
        &mut meshes,
        &mut materials,
    );
}

/// Insert the physics components appropriate to the avatar's body variant.
/// The caller is responsible for having removed any prior archetype's
/// components first (or for this being a fresh entity).
fn build_archetype_components(commands: &mut Commands, entity: Entity, record: &AvatarRecord) {
    match &record.body {
        AvatarBody::HoverRover { kinematics, .. } => {
            commands.entity(entity).insert((
                Collider::cuboid(
                    cfg::CHASSIS_X * 2.0,
                    cfg::CHASSIS_Y * 2.0,
                    cfg::CHASSIS_Z * 2.0,
                ),
                Mass(kinematics.mass.0),
                LinearDamping(kinematics.linear_damping.0),
                AngularDamping(kinematics.angular_damping.0),
                HoverRoverArchetype,
            ));
        }
        AvatarBody::Humanoid {
            phenotype,
            kinematics,
        } => {
            let (radius, length) = humanoid_capsule_dimensions(phenotype);
            commands.entity(entity).insert((
                Collider::capsule(radius, length),
                Mass(kinematics.mass.0),
                LinearDamping(kinematics.linear_damping.0),
                AngularDamping(cfg::ANGULAR_DAMPING),
                // Traditional character controller: lock all three rotation
                // axes so the physics capsule slides without spinning. The
                // walk controller rotates a child visual root to face the
                // movement direction, keeping the rigid body stable.
                LockedAxes::new()
                    .lock_rotation_x()
                    .lock_rotation_y()
                    .lock_rotation_z(),
                HumanoidArchetype,
            ));
        }
        AvatarBody::Unknown => {
            // Forward-compat shipping a record whose body type we don't
            // model: give the entity a minimal collider so the simulation
            // does not explode. The owner's editor should show an
            // "unrecognised avatar" warning in this state.
            commands
                .entity(entity)
                .insert((Collider::cuboid(0.5, 0.5, 0.5), Mass(40.0)));
        }
    }
}

fn humanoid_capsule_dimensions(phen: &HumanoidPhenotype) -> (f32, f32) {
    // Capsule: (radius, cylindrical length). Total height ~= length + 2·radius.
    // Clamp so a malicious/corrupt record can't panic `Capsule3d::new`.
    let radius = phen.torso_half_width.0.max(0.05);
    let cylinder_len = (phen.height.0 - 2.0 * radius).max(0.1);
    (radius, cylinder_len)
}

// ---------------------------------------------------------------------------
// Hot-swap — local player
// ---------------------------------------------------------------------------

/// Watch the live avatar record and flag the local player for rebuild
/// whenever the body *variant* changes (kinematics-only edits are handled
/// by the per-frame sync systems). A tiny `Local<Option<&'static str>>`
/// memoises the last-seen kind so we don't rebuild on every frame the
/// resource is `Changed` — the kinematics sliders fire `Changed` constantly
/// and would otherwise drop a dozen rebuilds per second.
fn detect_local_archetype_change(
    mut commands: Commands,
    live: Res<LiveAvatarRecord>,
    player: Query<Entity, With<LocalPlayer>>,
    mut last_kind: Local<Option<&'static str>>,
) {
    let kind = live.0.body.kind_tag();
    if Some(kind) == *last_kind {
        return;
    }
    *last_kind = Some(kind);
    if let Ok(entity) = player.single() {
        commands.entity(entity).insert(NeedsArchetypeRebuild);
    }
}

/// Apply a queued archetype rebuild to the local player: strip the old
/// archetype's components and visual children, then install the new
/// archetype's components and visuals. Runs in `Update` on the main
/// schedule so Avian sees the removed/inserted components on the next
/// physics step without a race.
#[allow(clippy::type_complexity)]
fn apply_local_archetype_rebuild(
    mut commands: Commands,
    players: Query<
        (Entity, Option<&Children>, Option<&AvatarMaterial>),
        (With<LocalPlayer>, With<NeedsArchetypeRebuild>),
    >,
    live: Res<LiveAvatarRecord>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, children, avatar_mat) in players.iter() {
        strip_archetype_components(&mut commands, entity);
        build_archetype_components(&mut commands, entity, &live.0);
        build_archetype_visuals(
            &mut commands,
            entity,
            &live.0,
            children,
            avatar_mat.map(|m| &m.0),
            &mut meshes,
            &mut materials,
        );
        commands.entity(entity).remove::<NeedsArchetypeRebuild>();
    }
}

/// Remove every archetype-specific component + marker from `entity`.
/// Safe to call even if the entity currently carries only a subset — Bevy's
/// `remove` no-ops when the component is absent.
fn strip_archetype_components(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).remove::<(
        Collider,
        Mass,
        LinearDamping,
        AngularDamping,
        LockedAxes,
        HoverRoverArchetype,
        HumanoidArchetype,
    )>();
}

/// Non-variant changes (slider tweaks inside the *same* body type) only
/// need new visual children — rigid-body identity stays intact.
#[allow(clippy::type_complexity)]
fn rebuild_local_visuals(
    mut commands: Commands,
    live: Res<LiveAvatarRecord>,
    players: Query<
        (Entity, Option<&Children>, Option<&AvatarMaterial>),
        (With<LocalPlayer>, Without<NeedsArchetypeRebuild>),
    >,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !live.is_changed() {
        return;
    }
    for (entity, children, avatar_mat) in players.iter() {
        build_archetype_visuals(
            &mut commands,
            entity,
            &live.0,
            children,
            avatar_mat.map(|m| &m.0),
            &mut meshes,
            &mut materials,
        );
    }
}

// ---------------------------------------------------------------------------
// Hot-swap — remote peers
// ---------------------------------------------------------------------------

/// Rebuild a remote peer's visual children whenever their avatar record
/// actually changes (initial fetch, live-preview broadcast, or archetype
/// swap). Remote peers are pure kinematic visual transforms — they never
/// carry a `RigidBody`, so installing a `Collider` / `Mass` / `LockedAxes`
/// here would register them as Static, and every per-frame `Transform`
/// update from `smooth_remote_transforms` would thrash the broadphase
/// spatial trees. We therefore only rebuild visuals and leave physics
/// alone. The `AppliedAvatar` snapshot gates this path so that muting or
/// relabelling a peer (both of which also trigger `Changed<RemotePeer>`)
/// doesn't redundantly despawn and rebuild every mesh — that expensive
/// path is reserved for genuine avatar-record changes.
#[allow(clippy::type_complexity)]
fn detect_remote_archetype_change(
    mut commands: Commands,
    peers: Query<
        (
            Entity,
            &RemotePeer,
            Option<&AppliedAvatar>,
            Option<&Children>,
            Option<&AvatarMaterial>,
        ),
        Changed<RemotePeer>,
    >,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, peer, applied, children, avatar_mat) in peers.iter() {
        let Some(record) = peer.avatar.as_ref() else {
            continue;
        };
        if applied.is_some_and(|a| &a.0 == record) {
            continue;
        }
        build_archetype_visuals(
            &mut commands,
            entity,
            record,
            children,
            avatar_mat.map(|m| &m.0),
            &mut meshes,
            &mut materials,
        );
        commands
            .entity(entity)
            .insert(AppliedAvatar(record.clone()));
    }
}

// ---------------------------------------------------------------------------
// Visuals
// ---------------------------------------------------------------------------

fn build_archetype_visuals(
    commands: &mut Commands,
    entity: Entity,
    record: &AvatarRecord,
    existing_children: Option<&Children>,
    avatar_override: Option<&Handle<StandardMaterial>>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    match &record.body {
        AvatarBody::HoverRover { phenotype, .. } => {
            let airship = phenotype.to_airship_params();
            rebuild_airship_children(
                commands,
                entity,
                &airship,
                existing_children,
                meshes,
                materials,
                avatar_override,
            );
        }
        AvatarBody::Humanoid { phenotype, .. } => {
            rebuild_humanoid_children(
                commands,
                entity,
                phenotype,
                existing_children,
                meshes,
                materials,
                avatar_override,
            );
        }
        AvatarBody::Unknown => {
            // Despawn any leftover children and leave the entity bare —
            // the owner's client will flag the unrecognised variant in its
            // editor.
            if let Some(children) = existing_children {
                for child in children.iter() {
                    commands.entity(child).despawn();
                }
            }
        }
    }
}

/// Build the steampunk-airship visual children of `entity`. Unchanged
/// semantics from the pre-rename version; only the module path moved.
pub fn rebuild_airship_children(
    commands: &mut Commands,
    entity: Entity,
    params: &AirshipParams,
    existing_children: Option<&Children>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    avatar_override: Option<&Handle<StandardMaterial>>,
) {
    if let Some(children) = existing_children {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }

    let hull_l = params.hull_length;
    let hull_w = params.hull_width;
    let mast_h = params.mast_height;
    let [mx, mz] = params.mast_offset;
    let mast_top_y = mast_h;
    let drop_y = -params.strut_drop * params.hull_depth;

    let [hr, hg, hb] = params.hull_color;
    let [pr, pg, pb] = params.pontoon_color;
    let [mr, mg, mb] = params.mast_color;
    let [sr, sg, sb] = params.strut_color;

    let hull_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(hr, hg, hb),
        metallic: params.metallic,
        perceptual_roughness: params.roughness,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let pontoon_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(pr, pg, pb),
        metallic: params.metallic * 0.5,
        perceptual_roughness: (params.roughness + 0.15).min(1.0),
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let mast_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(mr, mg, mb),
        metallic: 0.75,
        perceptual_roughness: 0.35,
        ..default()
    });
    let strut_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(sr, sg, sb),
        metallic: params.metallic * 0.7,
        perceptual_roughness: (params.roughness + 0.1).min(1.0),
        ..default()
    });
    let sail_mat = avatar_override.cloned().unwrap_or_else(|| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(0.82, 0.82, 0.92),
            double_sided: true,
            cull_mode: None,
            ..default()
        })
    });

    commands.entity(entity).with_children(|parent| {
        parent.spawn((
            Mesh3d(meshes.add(build_v_hull_mesh(hull_l, hull_w, params.hull_depth))),
            MeshMaterial3d(hull_mat.clone()),
            Transform::IDENTITY,
        ));

        let pontoon_mesh = match params.pontoon_shape {
            PontoonShape::Capsule => meshes.add(Capsule3d::new(
                params.pontoon_width / 2.0,
                params.pontoon_length,
            )),
            PontoonShape::VHull => meshes.add(build_v_hull_mesh(
                params.pontoon_length,
                params.pontoon_width,
                params.pontoon_height,
            )),
        };
        let pontoon_rot = match params.pontoon_shape {
            PontoonShape::Capsule => Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            PontoonShape::VHull => Quat::IDENTITY,
        };

        parent.spawn((
            Mesh3d(pontoon_mesh.clone()),
            MeshMaterial3d(pontoon_mat.clone()),
            Transform::from_xyz(-params.pontoon_spread, drop_y, 0.0).with_rotation(pontoon_rot),
        ));

        parent.spawn((
            Mesh3d(pontoon_mesh),
            MeshMaterial3d(pontoon_mat),
            Transform::from_xyz(params.pontoon_spread, drop_y, 0.0).with_rotation(pontoon_rot),
        ));

        let strut_mesh = meshes.add(Capsule3d::new(
            ac::STRUT_THICKNESS * 0.5,
            params.pontoon_spread * 2.0,
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

        let mast_r = params.mast_radius;
        parent.spawn((
            Mesh3d(meshes.add(Cylinder::new(mast_r, mast_h))),
            MeshMaterial3d(mast_mat.clone()),
            Transform::from_xyz(mx, mast_h * 0.5, mz),
        ));
        parent.spawn((
            Mesh3d(meshes.add(Sphere::new(mast_r).mesh().uv(16, 8))),
            MeshMaterial3d(mast_mat),
            Transform::from_xyz(mx, mast_h, mz),
            MastTip,
        ));

        parent.spawn((
            Mesh3d(meshes.add(Rectangle::new(params.sail_size, params.sail_size))),
            MeshMaterial3d(sail_mat),
            Transform::from_xyz(
                mx,
                mast_top_y - params.sail_size * 0.5,
                mz + params.sail_size * 0.5,
            )
            .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
            RoverSail,
        ));
    });
}

/// Build the humanoid visual rig. Instead of attaching meshes directly to
/// the physics capsule, we spawn an intermediate `HumanoidVisualRoot` that
/// the walk controller rotates to face the movement direction, plus
/// shoulder/hip joint pivots so the procedural animation system can swing
/// the limbs from their tops.
fn rebuild_humanoid_children(
    commands: &mut Commands,
    entity: Entity,
    phen: &HumanoidPhenotype,
    existing_children: Option<&Children>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    avatar_override: Option<&Handle<StandardMaterial>>,
) {
    if let Some(children) = existing_children {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }

    // Clamp every dimension so a malicious record cannot panic mesh
    // constructors. Bevy primitive ctors reject zero/negative/NaN inputs.
    let height = phen.height.0.clamp(0.4, 4.0);
    let tw = phen.torso_half_width.0.clamp(0.05, 1.0);
    let td = phen.torso_half_depth.0.clamp(0.05, 1.0);
    let head = phen.head_size.0.clamp(0.05, 1.0);
    let limb = phen.limb_thickness.0.clamp(0.03, 0.4);
    let arm_ratio = phen.arm_length_ratio.0.clamp(0.5, 1.5);
    let leg_ratio = phen.leg_length_ratio.0.clamp(0.3, 0.6);

    let [br, bg, bb] = phen.body_color.0;
    let [hr, hg, hb] = phen.head_color.0;
    let [lr, lg, lb] = phen.limb_color.0;
    let body_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(br, bg, bb),
        metallic: phen.metallic.0,
        perceptual_roughness: phen.roughness.0,
        ..default()
    });
    let head_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(hr, hg, hb),
        metallic: phen.metallic.0 * 0.3,
        perceptual_roughness: (phen.roughness.0 + 0.1).min(1.0),
        ..default()
    });
    let limb_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(lr, lg, lb),
        metallic: phen.metallic.0 * 0.5,
        perceptual_roughness: phen.roughness.0,
        ..default()
    });
    let badge_mat = avatar_override.cloned().unwrap_or_else(|| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.9, 0.95),
            unlit: true,
            double_sided: true,
            cull_mode: None,
            ..default()
        })
    });

    let head_h = head;
    let torso_h = (height * 0.45).max(0.2);
    let leg_len = (height * leg_ratio).max(0.2);
    let arm_len = (torso_h * arm_ratio).max(0.15);
    // Capsule body's origin sits at the rigid-body centre; torso centre
    // sits at y = 0 in local space.
    let torso_y = 0.0;
    let head_y = torso_h * 0.5 + head_h * 0.5;
    let shoulder_y = torso_h * 0.45;
    let hip_y = -torso_h * 0.5;
    let shoulder_x = tw + limb * 0.5;
    let hip_x = tw * 0.6;

    let arm_mesh = meshes.add(Capsule3d::new(limb * 0.5, arm_len));
    let leg_mesh = meshes.add(Capsule3d::new(limb * 0.6, leg_len));
    let show_badge = phen.show_badge;

    commands.entity(entity).with_children(|root| {
        root.spawn((
            Transform::IDENTITY,
            Visibility::default(),
            HumanoidVisualRoot,
        ))
        .with_children(|parent| {
            parent.spawn((
                Mesh3d(meshes.add(Cuboid::new(tw * 2.0, torso_h, td * 2.0))),
                MeshMaterial3d(body_mat),
                Transform::from_xyz(0.0, torso_y, 0.0),
            ));
            parent.spawn((
                Mesh3d(meshes.add(Cuboid::new(head_h, head_h, head_h))),
                MeshMaterial3d(head_mat),
                Transform::from_xyz(0.0, head_y, 0.0),
            ));

            if show_badge {
                let badge_w = (tw * 1.6).min(tw * 2.0 - 0.02).max(0.05);
                let badge_h = (torso_h * 0.55).max(0.05);
                parent.spawn((
                    Mesh3d(meshes.add(Rectangle::new(badge_w, badge_h))),
                    MeshMaterial3d(badge_mat),
                    Transform::from_xyz(0.0, torso_y, td + 0.01),
                    ChestBadge,
                ));
            }

            // Shoulders — rotating the joint swings the limb from its top
            // because the cylinder is offset downward by half its length.
            spawn_joint(
                parent,
                Vec3::new(-shoulder_x, shoulder_y, 0.0),
                HumanoidJoint {
                    phase_sign: 1.0,
                    phase_offset: 0.0,
                },
                arm_mesh.clone(),
                limb_mat.clone(),
                arm_len,
            );
            spawn_joint(
                parent,
                Vec3::new(shoulder_x, shoulder_y, 0.0),
                HumanoidJoint {
                    phase_sign: -1.0,
                    phase_offset: 0.0,
                },
                arm_mesh,
                limb_mat.clone(),
                arm_len,
            );

            // Hips — 180° out of phase with the arm on the same side so the
            // gait alternates (left arm forward ↔ left leg back).
            spawn_joint(
                parent,
                Vec3::new(-hip_x, hip_y, 0.0),
                HumanoidJoint {
                    phase_sign: -1.0,
                    phase_offset: 0.0,
                },
                leg_mesh.clone(),
                limb_mat.clone(),
                leg_len,
            );
            spawn_joint(
                parent,
                Vec3::new(hip_x, hip_y, 0.0),
                HumanoidJoint {
                    phase_sign: 1.0,
                    phase_offset: 0.0,
                },
                leg_mesh,
                limb_mat,
                leg_len,
            );
        });
    });
}

fn spawn_joint(
    parent: &mut ChildSpawnerCommands,
    position: Vec3,
    joint: HumanoidJoint,
    limb_mesh: Handle<Mesh>,
    limb_mat: Handle<StandardMaterial>,
    limb_length: f32,
) {
    parent
        .spawn((
            Transform::from_translation(position),
            Visibility::default(),
            joint,
        ))
        .with_children(|pivot| {
            pivot.spawn((
                Mesh3d(limb_mesh),
                MeshMaterial3d(limb_mat),
                Transform::from_xyz(0.0, -limb_length * 0.5, 0.0),
            ));
        });
}

fn build_v_hull_mesh(hull_length: f32, hull_width: f32, hull_depth: f32) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::Indices;
    use bevy::render::render_resource::PrimitiveTopology;

    const SEGMENTS: usize = 20;

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity((SEGMENTS + 1) * 3);
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
    mesh.insert_indices(Indices::U32(indices));
    mesh.duplicate_vertices();
    mesh.compute_flat_normals();
    mesh
}

// ---------------------------------------------------------------------------
// Physics systems
// ---------------------------------------------------------------------------

/// Push kinematics changes from the live record onto the chassis's Mass
/// and Damping components every fixed step, so slider tweaks take effect
/// immediately without requiring an archetype rebuild.
fn sync_local_chassis_physics(
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
fn apply_suspension_forces(
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
fn apply_rover_drive_forces(
    live: Res<LiveAvatarRecord>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(Forces, &GlobalTransform), (With<LocalPlayer>, With<HoverRoverArchetype>)>,
) {
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

    if keyboard.pressed(KeyCode::KeyW) {
        forces.apply_force(flat_forward * kinematics.drive_force.0);
    }
    if keyboard.pressed(KeyCode::KeyS) {
        forces.apply_force(-flat_forward * kinematics.drive_force.0);
    }
    if keyboard.pressed(KeyCode::KeyA) {
        forces.apply_torque(local_up * kinematics.turn_torque.0);
    }
    if keyboard.pressed(KeyCode::KeyD) {
        forces.apply_torque(-local_up * kinematics.turn_torque.0);
    }

    let lateral_vel = right.dot(lin_vel);
    forces.apply_force(-right * lateral_vel * kinematics.lateral_grip.0);

    if keyboard.pressed(KeyCode::Space) {
        forces.apply_force(Vec3::Y * kinematics.jump_force.0);
    }
}

#[allow(clippy::type_complexity)]
fn apply_rover_uprighting_force(
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

#[allow(clippy::type_complexity)]
fn apply_buoyancy_forces(
    live: Res<LiveAvatarRecord>,
    room_record: Option<Res<crate::pds::RoomRecord>>,
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
    let water_offset: f32 = room_record
        .as_ref()
        .and_then(|r| {
            let mut best: Option<(&String, f32)> = None;
            for (k, g) in r.generators.iter() {
                if let crate::pds::Generator::Water { level_offset } = g
                    && best.is_none_or(|(bk, _)| k < bk)
                {
                    best = Some((k, level_offset.0));
                }
            }
            best.map(|(_, off)| off)
        })
        .unwrap_or(0.0);
    let wl = water_level_y() + water_offset + kinematics.water_rest_length.0;
    let y = global_tf.translation().y;
    let depth = (wl - y).clamp(0.0, kinematics.buoyancy_max_depth.0);
    if depth <= 0.0 {
        return;
    }
    let lin_vel = forces.linear_velocity();
    let lift = kinematics.buoyancy_strength.0 * depth;
    let drag = -kinematics.buoyancy_damping.0 * lin_vel.y;
    forces.apply_force(Vec3::Y * (lift + drag));
}

/// Velocity-driven walk controller for the Humanoid archetype.
/// WASD nudges the target horizontal velocity toward `walk_speed` in the
/// input direction; when no key is held we aggressively damp the horizontal
/// velocity so the avatar doesn't ice-skate. The `HumanoidVisualRoot` child
/// is slerped to face the movement direction each step.
/// `Space` adds a one-shot vertical impulse whenever a short downward
/// raycast confirms the capsule is on the ground.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn apply_humanoid_walk(
    live: Res<LiveAvatarRecord>,
    time: Res<Time<Fixed>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut query: Query<
        (Entity, &mut LinearVelocity, &GlobalTransform, &Children),
        (With<LocalPlayer>, With<HumanoidArchetype>),
    >,
    mut visual_roots: Query<&mut Transform, With<HumanoidVisualRoot>>,
    spatial_query: SpatialQuery,
) {
    let AvatarBody::Humanoid {
        kinematics,
        phenotype,
    } = &live.0.body
    else {
        return;
    };
    let Ok((entity, mut lin_vel, global_tf, children)) = query.single_mut() else {
        return;
    };

    let cam_forward = camera
        .single()
        .ok()
        .map(|t| t.forward().as_vec3())
        .unwrap_or(Vec3::NEG_Z);
    let forward = Vec3::new(cam_forward.x, 0.0, cam_forward.z).normalize_or_zero();
    let right = Vec3::new(-forward.z, 0.0, forward.x);

    let mut desired = Vec3::ZERO;
    let mut any_input = false;
    if keyboard.pressed(KeyCode::KeyW) {
        desired += forward;
        any_input = true;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        desired -= forward;
        any_input = true;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        desired += right;
        any_input = true;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        desired -= right;
        any_input = true;
    }
    desired = desired.normalize_or_zero() * kinematics.walk_speed.0;

    let dt = time.delta_secs().max(1e-4);
    let current_h = Vec3::new(lin_vel.0.x, 0.0, lin_vel.0.z);
    let new_h = if any_input {
        let alpha = (kinematics.acceleration.0 * dt).clamp(0.0, 1.0);
        current_h.lerp(desired, alpha)
    } else {
        // Snappy friction: collapse horizontal velocity to zero fast so the
        // avatar stops on a dime instead of coasting.
        let decay = (-20.0 * dt).exp();
        current_h * decay
    };
    lin_vel.0.x = new_h.x;
    lin_vel.0.z = new_h.z;

    // Rotate the visual root to face movement. The physics body has all
    // rotation axes locked, so this is purely cosmetic — and exactly what
    // a traditional character controller does.
    if new_h.length_squared() > 0.01 {
        let facing = new_h.normalize();
        let target = Transform::IDENTITY.looking_to(facing, Vec3::Y).rotation;
        let turn_alpha = (12.0 * dt).clamp(0.0, 1.0);
        for child in children.iter() {
            if let Ok(mut tf) = visual_roots.get_mut(child) {
                tf.rotation = tf.rotation.slerp(target, turn_alpha);
            }
        }
    }

    if keyboard.just_pressed(KeyCode::Space) {
        let origin = global_tf.translation() + Vec3::Y * 0.05;
        let feet_distance = phenotype.height.0 * 0.5 + 0.1;
        let filter = SpatialQueryFilter::default().with_excluded_entities([entity]);
        let grounded = spatial_query
            .cast_ray(origin, Dir3::NEG_Y, feet_distance, true, &filter)
            .is_some();
        if grounded {
            let delta_v = kinematics.jump_impulse.0 / kinematics.mass.0.max(1.0);
            lin_vel.0.y += delta_v;
        }
    }
}

/// Procedural gait animation: swing each shoulder/hip joint in a sine
/// counter-rotation, scaled by horizontal speed. Legs are 180° out of
/// phase with their paired arm so the walk alternates naturally. When
/// stopped the joints smoothly slerp back to the idle (identity) pose.
#[allow(clippy::type_complexity)]
fn animate_humanoid_limbs(
    time: Res<Time>,
    players: Query<(&LinearVelocity, &Children), With<HumanoidArchetype>>,
    visual_roots: Query<&Children, With<HumanoidVisualRoot>>,
    mut joints: Query<(&HumanoidJoint, &mut Transform)>,
) {
    const SWING_AMPLITUDE: f32 = 0.9;
    const PHASE_SPEED: f32 = 2.2;
    const IDLE_SLERP_RATE: f32 = 10.0;

    let dt = time.delta_secs().max(1e-4);
    let t = time.elapsed_secs();

    for (lin_vel, children) in players.iter() {
        let horiz = Vec3::new(lin_vel.0.x, 0.0, lin_vel.0.z);
        let speed = horiz.length();
        let amplitude = SWING_AMPLITUDE * (speed / 4.0).clamp(0.0, 1.0);
        let phase = t * PHASE_SPEED * speed.max(0.0);

        for chassis_child in children.iter() {
            let Ok(root_children) = visual_roots.get(chassis_child) else {
                continue;
            };
            for joint_entity in root_children.iter() {
                let Ok((joint, mut tf)) = joints.get_mut(joint_entity) else {
                    continue;
                };
                if amplitude < 1e-3 {
                    let idle_alpha = (IDLE_SLERP_RATE * dt).clamp(0.0, 1.0);
                    tf.rotation = tf.rotation.slerp(Quat::IDENTITY, idle_alpha);
                } else {
                    let angle = (phase + joint.phase_offset).sin() * amplitude * joint.phase_sign;
                    tf.rotation = Quat::from_rotation_x(angle);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shared respawn / recovery
// ---------------------------------------------------------------------------

fn lift_player_above_new_ground(
    hm_res: Option<Res<crate::terrain::FinishedHeightMap>>,
    mut query: Query<(&mut Position, &mut LinearVelocity, &mut AngularVelocity), With<LocalPlayer>>,
) {
    let Some(hm_res) = hm_res else {
        return;
    };
    if !hm_res.is_added() {
        return;
    }
    let Ok((mut pos, mut lin_vel, mut ang_vel)) = query.single_mut() else {
        return;
    };
    let hm = &hm_res.0;
    let extent = (hm.width() - 1) as f32 * hm.scale();
    let half = extent * 0.5;
    let hm_x = (pos.x + half).clamp(0.0, extent);
    let hm_z = (pos.z + half).clamp(0.0, extent);
    let ground_y = hm.get_height_at(hm_x, hm_z);
    let min_y = ground_y + cfg::SPAWN_HEIGHT_OFFSET;
    if pos.y < min_y {
        pos.y = min_y;
        lin_vel.0 = Vec3::ZERO;
        ang_vel.0 = Vec3::ZERO;
    }
}

/// Per-frame sweep that fires the portal jump the instant the local player's
/// sensor-collision set contains a `PortalMarker`. An intra-room portal snaps
/// the chassis to the exit pose and zeros its velocities; an inter-room
/// portal stages a `TravelRequest` + `AppState::Loading` transition so the
/// logout cleanup can swap the matchbox socket without dropping the session.
#[allow(clippy::type_complexity)]
fn handle_portal_interaction(
    mut commands: Commands,
    mut players: Query<
        (
            &CollidingEntities,
            &mut Transform,
            &mut LinearVelocity,
            &mut AngularVelocity,
        ),
        With<LocalPlayer>,
    >,
    portals: Query<&PortalMarker>,
    current_room: Option<Res<CurrentRoomDid>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let Ok((collisions, mut tf, mut lv, mut av)) = players.single_mut() else {
        return;
    };

    for entity in collisions.iter() {
        let Ok(portal) = portals.get(*entity) else {
            continue;
        };

        let same_room = current_room
            .as_deref()
            .map(|r| r.0 == portal.target_did)
            .unwrap_or(false);
        if same_room {
            tf.translation = portal.target_pos;
            lv.0 = Vec3::ZERO;
            av.0 = Vec3::ZERO;
        } else {
            commands.insert_resource(TravelRequest {
                target_did: portal.target_did.clone(),
                target_pos: portal.target_pos,
            });
            commands.insert_resource(CurrentRoomDid(portal.target_did.clone()));
            next_state.set(AppState::Loading);
        }
        break;
    }
}

fn respawn_if_fallen(
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
}
