//! Local player plugin: spawns and drives the local avatar, hot-swaps
//! locomotion presets when the owner edits their PDS avatar record, and
//! paints matching visuals on remote peers.
//!
//! Avatars are now uniform: the `visuals` half of [`AvatarRecord`] is a
//! generator tree spawned by [`visuals::spawn_avatar_visuals`] (no
//! colliders, no per-prim markers — pure cosmetics), and the
//! `locomotion` half selects one of five physics presets:
//!
//! - **HoverBoat** — `RigidBody::Dynamic` cuboid chassis with four
//!   raycast-suspension corners + buoyancy + WASD drive (Hooke's-law
//!   spring, lateral grip, jump impulse).
//! - **Humanoid** — capsule rigid body with `LockedAxes` keeping it
//!   upright, velocity-driven walk controller, jump impulse, swim/wading
//!   modes triggered by water depth.
//! - **Airplane** — cuboid fuselage, continuous thrust, lift proportional
//!   to forward airspeed, pitch / roll / yaw torque from input.
//! - **Helicopter** — cuboid fuselage, auto-stabilising hover thrust,
//!   cyclic + strafe + yaw input, vertical climb/descend on Space/Shift.
//! - **Car** — cuboid chassis, four-corner raycast suspension, ground
//!   drive + steering + handbrake, no buoyancy.
//!
//! All five read their tuning from the live [`LiveAvatarRecord`], so UI
//! edits take effect the same frame the slider moves. Changing the
//! locomotion *variant* triggers the hot-swap system, which tears down
//! all preset-specific components (collider, markers, locked axes) and
//! rebuilds them in the new preset's shape without disturbing the parent
//! `Transform` or rigid-body identity.
//!
//! ## Sub-module map
//!
//! * [`visuals`] — generator-tree visual spawner (`spawn_avatar_visuals`
//!   plus the `AvatarVisualEntity` marker).
//! * [`hover_boat`] — HoverBoat preset: suspension / buoyancy / drive /
//!   uprighting systems.
//! * [`humanoid`] — Humanoid preset: walk controller (dry/wading/swim
//!   modes) and the `humanoid_water_state` classifier.
//! * [`airplane`] — Airplane preset: thrust + control-surface forces.
//! * [`helicopter`] — Helicopter preset: auto-stabilised hover + cyclic.
//! * [`car`] — Car preset: ground drive + steering + handbrake.
//! * [`portal`] — `handle_portal_interaction`,
//!   `poll_portal_travel_tasks`, and the `PortalTravelTask` async job.

mod airplane;
mod car;
mod helicopter;
mod hover_boat;
mod humanoid;
mod portal;
pub mod visuals;

pub use visuals::AvatarVisualEntity;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_egui::input::egui_wants_any_keyboard_input;

use crate::boot_params::TargetPos;
use crate::config::rover as cfg;
use crate::config::terrain as tcfg;
use crate::pds::{AvatarRecord, LocomotionConfig};
use crate::state::{AppState, LiveAvatarRecord, LocalPlayer, PendingSpawnPlacement, RemotePeer};
use crate::ui::avatar::AvatarEditorState;
use crate::world_builder::{AvatarVisualPrim, OverlandsFoliageTasks};

/// Snapshot of the last `AvatarRecord` whose visuals have been painted onto
/// a remote peer. `detect_remote_change` listens to the broad
/// `Changed<RemotePeer>` signal (which also fires on mute/handle/DID edits)
/// and compares against this snapshot so an unrelated field flip doesn't
/// re-enter the expensive visual rebuild path.
#[derive(Component)]
struct AppliedAvatar(AvatarRecord);

// Corner offsets in local space for the four suspension rays. The
// hover-boat and car presets share the same four-corner pattern; their
// chassis half-extents differ but the rig topology does not.
pub(super) const CORNER_OFFSETS_RAW: [[f32; 3]; 4] = [
    [1.0, -1.0, 1.0],
    [-1.0, -1.0, 1.0],
    [1.0, -1.0, -1.0],
    [-1.0, -1.0, -1.0],
];

/// Multiply the canonical `CORNER_OFFSETS_RAW` by the preset's chassis
/// half-extents to get the four world-local suspension-ray origins. Both
/// [`hover_boat`] and [`car`] share this helper because the suspension
/// math is identical — only the chassis size differs.
pub(super) fn chassis_corners(half_extents: Vec3) -> [Vec3; 4] {
    CORNER_OFFSETS_RAW.map(|raw| Vec3::new(raw[0], raw[1], raw[2]) * half_extents)
}

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

/// Marks the local or remote player as currently using the HoverBoat
/// preset. Inserted by [`build_preset_components`]; stripped by the
/// hot-swap system when the owner picks a different preset.
#[derive(Component)]
pub struct HoverBoatPreset;

/// Marks the player as using the Humanoid preset.
#[derive(Component)]
pub struct HumanoidPreset;

/// Marks the player as using the Airplane preset.
#[derive(Component)]
pub struct AirplanePreset;

/// Marks the player as using the Helicopter preset.
#[derive(Component)]
pub struct HelicopterPreset;

/// Marks the player as using the Car preset.
#[derive(Component)]
pub struct CarPreset;

/// Aggregate marker query target for camera follow / vehicle-yaw
/// inheritance. Covers every preset whose physics body rotates around Y
/// — i.e. anything except the upright-locked Humanoid.
#[derive(Component)]
pub struct VehicleChassis;

/// Request flag set when the local player's locomotion needs to be
/// rebuilt on the main thread. This exists because Avian components
/// cannot be added/removed from `Query`-held mutable borrows — we have
/// to defer the surgery to a commands-only system.
#[derive(Component)]
struct NeedsLocomotionRebuild;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_local_player)
            .add_systems(
                Update,
                (
                    detect_local_locomotion_change,
                    apply_local_locomotion_rebuild,
                    detect_remote_change,
                    rebuild_local_visuals,
                    lift_player_above_new_ground,
                    portal::handle_portal_interaction,
                    portal::poll_portal_travel_tasks,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    hover_boat::sync_hover_boat_physics,
                    hover_boat::apply_hover_boat_suspension,
                    hover_boat::apply_hover_boat_buoyancy,
                    // Disable keyboard-driven control systems while the
                    // owner is typing in an egui text field — otherwise
                    // WASD-heavy chat messages steer the vehicle through
                    // walls. Physics (suspension, buoyancy, gravity) and
                    // the uprighting / respawn passes still run so a
                    // vehicle left mid-air keeps obeying gravity.
                    hover_boat::apply_hover_boat_drive
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected)),
                    hover_boat::apply_hover_boat_uprighting
                        .run_if(not(avatar_visuals_row_selected)),
                    humanoid::apply_humanoid_walk
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected)),
                    airplane::apply_airplane_forces
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected)),
                    helicopter::apply_helicopter_forces
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected)),
                    car::apply_car_suspension,
                    car::apply_car_drive
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected)),
                    respawn_if_fallen,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                freeze_local_avatar_on_visuals_select.run_if(in_state(AppState::InGame)),
            );
    }
}

/// Run condition: true when the avatar editor has a visuals row
/// selected. The five locomotion drive systems gate on
/// `not(this)` so the avatar stays still while the owner is editing
/// its visuals — both for ergonomics (the gizmo can't track a moving
/// chassis precisely) and for correctness (the drag commit's
/// world→local conversion uses the parent chassis's `GlobalTransform`,
/// which is unstable while physics is integrating).
///
/// The uprighting torque on the hover-boat is also gated so the avatar
/// doesn't slowly tip itself back upright during a long edit; the user
/// can rotate the chassis with the gizmo and it stays where they put
/// it. Suspension and gravity-style passive systems remain on so a
/// floating avatar doesn't levitate during the edit.
fn avatar_visuals_row_selected(avatar_editor: Option<Res<AvatarEditorState>>) -> bool {
    avatar_editor
        .map(|e| e.has_visuals_selection())
        .unwrap_or(false)
}

/// On the rising edge of "avatar visuals row selected", zero the local
/// player's linear and angular velocity. Without this, residual momentum
/// from the moment of click drifts the chassis (and the gizmo target)
/// for a few seconds after the freeze gate engages. The locomotion
/// drive systems are already gated off, so they won't push velocity
/// back up — we just need the one-shot to clear what was there.
fn freeze_local_avatar_on_visuals_select(
    avatar_editor: Option<Res<AvatarEditorState>>,
    mut last_selected: Local<bool>,
    mut q: Query<(&mut LinearVelocity, &mut AngularVelocity), With<LocalPlayer>>,
) {
    let now_selected = avatar_editor
        .as_ref()
        .map(|e| e.has_visuals_selection())
        .unwrap_or(false);
    if now_selected && !*last_selected {
        for (mut lin, mut ang) in q.iter_mut() {
            lin.0 = Vec3::ZERO;
            ang.0 = Vec3::ZERO;
        }
    }
    *last_selected = now_selected;
}

#[allow(clippy::too_many_arguments)]
fn spawn_local_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut foliage_tasks: ResMut<OverlandsFoliageTasks>,
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
        .spawn((
            Transform::from_xyz(ox, oy, oz).with_rotation(rotation),
            Visibility::default(),
            RigidBody::Dynamic,
            CollidingEntities::default(),
            LocalPlayer,
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
        &mut foliage_tasks,
        &mut avatar_deps,
        true,
    );
}

/// Insert the physics components appropriate to the avatar's locomotion
/// preset. The caller is responsible for having stripped any prior
/// preset's components first (or for this being a fresh entity).
pub(super) fn build_preset_components(
    commands: &mut Commands,
    entity: Entity,
    locomotion: &LocomotionConfig,
) {
    match locomotion {
        LocomotionConfig::HoverBoat(p) => {
            let half = p.chassis_half_extents.0;
            commands.entity(entity).insert((
                Collider::cuboid(half[0] * 2.0, half[1] * 2.0, half[2] * 2.0),
                Mass(p.mass.0),
                LinearDamping(p.linear_damping.0),
                AngularDamping(p.angular_damping.0),
                HoverBoatPreset,
                VehicleChassis,
            ));
        }
        LocomotionConfig::Humanoid(p) => {
            commands.entity(entity).insert((
                Collider::capsule(p.capsule_radius.0.max(0.05), p.capsule_length.0.max(0.1)),
                Mass(p.mass.0),
                LinearDamping(p.linear_damping.0),
                AngularDamping(cfg::ANGULAR_DAMPING),
                // Traditional character controller: lock all three rotation
                // axes so the physics capsule slides without spinning. The
                // walk controller rotates the chassis transform itself to
                // face the movement direction.
                LockedAxes::new()
                    .lock_rotation_x()
                    .lock_rotation_y()
                    .lock_rotation_z(),
                HumanoidPreset,
            ));
        }
        LocomotionConfig::Airplane(p) => {
            let half = p.chassis_half_extents.0;
            commands.entity(entity).insert((
                Collider::cuboid(half[0] * 2.0, half[1] * 2.0, half[2] * 2.0),
                Mass(p.mass.0),
                LinearDamping(p.linear_damping.0),
                AngularDamping(p.angular_damping.0),
                AirplanePreset,
                VehicleChassis,
            ));
        }
        LocomotionConfig::Helicopter(p) => {
            let half = p.chassis_half_extents.0;
            commands.entity(entity).insert((
                Collider::cuboid(half[0] * 2.0, half[1] * 2.0, half[2] * 2.0),
                Mass(p.mass.0),
                LinearDamping(p.linear_damping.0),
                AngularDamping(p.angular_damping.0),
                HelicopterPreset,
                VehicleChassis,
            ));
        }
        LocomotionConfig::Car(p) => {
            let half = p.chassis_half_extents.0;
            commands.entity(entity).insert((
                Collider::cuboid(half[0] * 2.0, half[1] * 2.0, half[2] * 2.0),
                Mass(p.mass.0),
                LinearDamping(p.linear_damping.0),
                AngularDamping(p.angular_damping.0),
                CarPreset,
                VehicleChassis,
            ));
        }
        LocomotionConfig::Unknown => {
            // Forward-compat shipping a record whose preset we don't model:
            // give the entity a minimal collider so the simulation does not
            // explode. The owner's editor flags the unrecognised variant.
            commands
                .entity(entity)
                .insert((Collider::cuboid(0.5, 0.5, 0.5), Mass(40.0)));
        }
    }
}

// ---------------------------------------------------------------------------
// Hot-swap — local player
// ---------------------------------------------------------------------------

/// Watch the live avatar record and flag the local player for rebuild
/// whenever the locomotion *variant* changes (intra-variant tuning edits
/// are handled by the per-frame sync systems). A
/// `Local<Option<&'static str>>` memoises the last-seen kind so we don't
/// rebuild on every frame the resource is `Changed` — the kinematics
/// sliders fire `Changed` constantly and would otherwise drop a dozen
/// rebuilds per second.
fn detect_local_locomotion_change(
    mut commands: Commands,
    live: Res<LiveAvatarRecord>,
    player: Query<Entity, With<LocalPlayer>>,
    mut last_kind: Local<Option<&'static str>>,
) {
    let kind = live.0.locomotion.kind_tag();
    if Some(kind) == *last_kind {
        return;
    }
    *last_kind = Some(kind);
    if let Ok(entity) = player.single() {
        commands.entity(entity).insert(NeedsLocomotionRebuild);
    }
}

/// Apply a queued locomotion rebuild to the local player: strip the old
/// preset's components and visual children, then install the new preset's
/// components and visuals. Runs in `Update` on the main schedule so Avian
/// sees the removed/inserted components on the next physics step without
/// a race.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn apply_local_locomotion_rebuild(
    mut commands: Commands,
    players: Query<(Entity, Option<&Children>), (With<LocalPlayer>, With<NeedsLocomotionRebuild>)>,
    orphan_visuals: Query<Entity, (With<AvatarVisualPrim>, Without<ChildOf>)>,
    live: Res<LiveAvatarRecord>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut foliage_tasks: ResMut<OverlandsFoliageTasks>,
    mut avatar_deps: visuals::AvatarSpawnDeps,
) {
    for (entity, children) in players.iter() {
        strip_preset_components(&mut commands, entity);
        build_preset_components(&mut commands, entity, &live.0.locomotion);
        despawn_orphan_avatar_visuals(&mut commands, &orphan_visuals);
        visuals::spawn_avatar_visuals(
            &mut commands,
            entity,
            &live.0.visuals,
            children,
            &mut meshes,
            &mut materials,
            &mut foliage_tasks,
            &mut avatar_deps,
            true,
        );
        commands.entity(entity).remove::<NeedsLocomotionRebuild>();
    }
}

/// Despawn any avatar-visual entity that has been orphaned from the
/// chassis hierarchy — typically the entity the editor gizmo detached
/// (and stamped with a world-space `Transform`) so it could render at
/// the actual world pose during a drag. The chassis-children iteration
/// in `spawn_avatar_visuals` cleans up the live tree, but a detached
/// entity has no `ChildOf` link back to anything reachable from the
/// chassis, so it survives the despawn cascade and lingers as a phantom
/// mesh until a tag-based sweep like this finds it.
///
/// Selecting orphans by `Without<ChildOf>` keeps the sweep narrow —
/// every node spawned by the avatar pipeline is parented to either the
/// chassis or another visuals node, so a missing parent uniquely
/// identifies the gizmo-detached case (and any future error path that
/// leaves an avatar visual orphaned).
fn despawn_orphan_avatar_visuals(
    commands: &mut Commands,
    orphan_visuals: &Query<Entity, (With<AvatarVisualPrim>, Without<ChildOf>)>,
) {
    for orphan in orphan_visuals.iter() {
        commands.entity(orphan).despawn();
    }
}

/// Remove every preset-specific component + marker from `entity`.
/// Safe to call even if the entity currently carries only a subset — Bevy's
/// `remove` no-ops when the component is absent.
fn strip_preset_components(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).remove::<(
        Collider,
        Mass,
        LinearDamping,
        AngularDamping,
        LockedAxes,
        HoverBoatPreset,
        HumanoidPreset,
        AirplanePreset,
        HelicopterPreset,
        CarPreset,
        VehicleChassis,
    )>();
}

/// Non-variant changes (slider tweaks inside the *same* preset, or
/// visuals-tree edits) only need new visual children — rigid-body
/// identity stays intact.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn rebuild_local_visuals(
    mut commands: Commands,
    live: Res<LiveAvatarRecord>,
    players: Query<
        (Entity, Option<&Children>),
        (With<LocalPlayer>, Without<NeedsLocomotionRebuild>),
    >,
    orphan_visuals: Query<Entity, (With<AvatarVisualPrim>, Without<ChildOf>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut foliage_tasks: ResMut<OverlandsFoliageTasks>,
    mut avatar_deps: visuals::AvatarSpawnDeps,
) {
    if !live.is_changed() {
        return;
    }
    despawn_orphan_avatar_visuals(&mut commands, &orphan_visuals);
    for (entity, children) in players.iter() {
        visuals::spawn_avatar_visuals(
            &mut commands,
            entity,
            &live.0.visuals,
            children,
            &mut meshes,
            &mut materials,
            &mut foliage_tasks,
            &mut avatar_deps,
            true,
        );
    }
}

// ---------------------------------------------------------------------------
// Hot-swap — remote peers
// ---------------------------------------------------------------------------

/// Rebuild a remote peer's visual children whenever their avatar record
/// actually changes (initial fetch, live-preview broadcast, or visuals
/// edit). Remote peers are pure kinematic visual transforms — they never
/// carry a `RigidBody`, so installing a `Collider` / `Mass` / `LockedAxes`
/// here would register them as Static, and every per-frame `Transform`
/// update from `smooth_remote_transforms` would thrash the broadphase
/// spatial trees. We therefore only rebuild visuals and leave physics
/// alone. The `AppliedAvatar` snapshot gates this path so that muting or
/// relabelling a peer (both of which also trigger `Changed<RemotePeer>`)
/// doesn't redundantly despawn and rebuild every mesh — that expensive
/// path is reserved for genuine avatar-record changes.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn detect_remote_change(
    mut commands: Commands,
    peers: Query<
        (
            Entity,
            &RemotePeer,
            Option<&AppliedAvatar>,
            Option<&Children>,
        ),
        Changed<RemotePeer>,
    >,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut foliage_tasks: ResMut<OverlandsFoliageTasks>,
    mut avatar_deps: visuals::AvatarSpawnDeps,
) {
    for (entity, peer, applied, children) in peers.iter() {
        let Some(record) = peer.avatar.as_ref() else {
            continue;
        };
        if applied.is_some_and(|a| &a.0 == record) {
            continue;
        }
        visuals::spawn_avatar_visuals(
            &mut commands,
            entity,
            &record.visuals,
            children,
            &mut meshes,
            &mut materials,
            &mut foliage_tasks,
            &mut avatar_deps,
            false,
        );
        commands
            .entity(entity)
            .insert(AppliedAvatar(record.clone()));
    }
}

// ---------------------------------------------------------------------------
// Spawn pose recovery — used after terrain hot-load and fall-through
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
