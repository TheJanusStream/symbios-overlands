//! Local player plugin: spawns and drives the local avatar, hot-swaps
//! archetypes when the owner edits their PDS avatar record, and paints
//! matching visuals on remote peers.
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
//!
//! ## Sub-module map
//!
//! * [`rover`] — HoverRover rig (`rebuild_airship_children`), the V-hull
//!   mesh builder, the per-tick physics systems (suspension / buoyancy /
//!   drive / uprighting), and `sync_local_chassis_physics`.
//! * [`humanoid`] — Humanoid rig (`rebuild_humanoid_children`), joint
//!   factory, walk controller, and limb animator.
//! * [`portal`] — `handle_portal_interaction`, `poll_portal_travel_tasks`,
//!   and the `PortalTravelTask` async job.

mod humanoid;
mod portal;
mod rover;

pub use rover::rebuild_airship_children;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_egui::input::egui_wants_any_keyboard_input;

use crate::avatar::AvatarMaterial;
use crate::config::rover as cfg;
use crate::config::terrain as tcfg;
use crate::pds::{AvatarBody, AvatarRecord, HumanoidPhenotype};
use crate::state::{AppState, LiveAvatarRecord, LocalPlayer, RemotePeer};
use crate::world_builder::OverlandsFoliageTasks;

/// Snapshot of the last `AvatarRecord` whose visuals have been painted onto
/// a remote peer. `detect_remote_archetype_change` listens to the broad
/// `Changed<RemotePeer>` signal (which also fires on mute/handle/DID edits)
/// and compares against this snapshot so an unrelated field flip doesn't
/// re-enter the expensive visual rebuild path. The cheaper
/// `sync_mute_visibility` handles the mute toggle on its own.
#[derive(Component)]
struct AppliedAvatar(AvatarRecord);

// Corner offsets in local space for the four suspension rays.
pub(super) const CORNER_OFFSETS: [[f32; 3]; 4] = [
    [cfg::CHASSIS_X, -cfg::CHASSIS_Y, cfg::CHASSIS_Z],
    [-cfg::CHASSIS_X, -cfg::CHASSIS_Y, cfg::CHASSIS_Z],
    [cfg::CHASSIS_X, -cfg::CHASSIS_Y, -cfg::CHASSIS_Z],
    [-cfg::CHASSIS_X, -cfg::CHASSIS_Y, -cfg::CHASSIS_Z],
];

/// Draw an (x, z) pair uniformly distributed inside a square of
/// `SPAWN_SCATTER_SIZE` metres per side, centred on the origin.
pub(super) fn random_spawn_xz() -> (f32, f32) {
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
/// Inserted by `build_hover_rover_archetype` and stripped by the
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
                    humanoid::animate_humanoid_limbs,
                    portal::handle_portal_interaction,
                    portal::poll_portal_travel_tasks,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    rover::sync_local_chassis_physics,
                    rover::apply_suspension_forces,
                    rover::apply_buoyancy_forces,
                    // Disable keyboard-driven drive/walk systems while the
                    // owner is typing in an egui text field — otherwise
                    // WASD-heavy chat messages steer the rover or the
                    // humanoid through walls. Physics (suspension, buoyancy)
                    // and the uprighting / respawn passes still run so a
                    // vehicle left mid-air keeps obeying gravity.
                    rover::apply_rover_drive_forces.run_if(not(egui_wants_any_keyboard_input)),
                    rover::apply_rover_uprighting_force,
                    humanoid::apply_humanoid_walk.run_if(not(egui_wants_any_keyboard_input)),
                    respawn_if_fallen,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn spawn_local_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut foliage_tasks: ResMut<OverlandsFoliageTasks>,
    hm_res: Res<crate::terrain::FinishedHeightMap>,
    live: Res<LiveAvatarRecord>,
) {
    let hm = &hm_res.0;
    let extent = (hm.width() - 1) as f32 * hm.scale();
    let centre = extent * 0.5;

    let (rx, rz) = random_spawn_xz();
    let hm_x = (centre + rx).clamp(0.0, extent);
    let hm_z = (centre + rz).clamp(0.0, extent);
    let ground_y = hm.get_height_at(hm_x, hm_z);
    let surface_normal = hm.get_normal_at(hm_x, hm_z);
    let tilt = Quat::from_rotation_arc(Vec3::Y, Vec3::from_array(surface_normal));
    let (ox, oy, oz) = (rx, ground_y + cfg::SPAWN_HEIGHT_OFFSET, rz);

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
        &mut foliage_tasks,
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
    mut foliage_tasks: ResMut<OverlandsFoliageTasks>,
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
            &mut foliage_tasks,
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
    mut foliage_tasks: ResMut<OverlandsFoliageTasks>,
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
            &mut foliage_tasks,
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
    mut foliage_tasks: ResMut<OverlandsFoliageTasks>,
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
            &mut foliage_tasks,
        );
        commands
            .entity(entity)
            .insert(AppliedAvatar(record.clone()));
    }
}

// ---------------------------------------------------------------------------
// Visuals
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn build_archetype_visuals(
    commands: &mut Commands,
    entity: Entity,
    record: &AvatarRecord,
    existing_children: Option<&Children>,
    avatar_override: Option<&Handle<StandardMaterial>>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    foliage_tasks: &mut OverlandsFoliageTasks,
) {
    match &record.body {
        AvatarBody::HoverRover { phenotype, .. } => {
            rebuild_airship_children(
                commands,
                entity,
                phenotype,
                existing_children,
                meshes,
                materials,
                foliage_tasks,
                avatar_override,
            );
        }
        AvatarBody::Humanoid { phenotype, .. } => {
            humanoid::rebuild_humanoid_children(
                commands,
                entity,
                phenotype,
                existing_children,
                meshes,
                materials,
                foliage_tasks,
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

/// Build the steampunk-airship visual children of `entity` directly from a
/// [`RoverPhenotype`] — no intermediate struct.

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
/// portal stages a `TravelingTo` resource and spawns an async `RoomRecord`
/// fetch so the destination can be hot-swapped without leaving `InGame`.
#[allow(clippy::type_complexity)]

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
