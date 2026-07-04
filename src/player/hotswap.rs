//! Locomotion / visuals hot-swap: rebuild the local chassis when the
//! owner changes their locomotion *variant*, repaint visuals on
//! intra-variant edits, mirror avatar-record changes onto remote peers,
//! and lift the player above freshly hot-loaded terrain.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::config::rover as cfg;
use crate::pds::AvatarRecord;
use crate::state::{LiveAvatarRecord, LocalPlayer, RemotePeer};
use crate::world_builder::AvatarVisualPrim;

use super::preset::{build_preset_components, strip_preset_components};
use super::visuals;

/// Snapshot of the last `AvatarRecord` whose visuals have been painted onto
/// a remote peer. `detect_remote_change` listens to the broad
/// `Changed<RemotePeer>` signal (which also fires on mute/handle/DID edits)
/// and compares against this snapshot so an unrelated field flip doesn't
/// re-enter the expensive visual rebuild path.
#[derive(Component)]
pub(super) struct AppliedAvatar(AvatarRecord);

/// Request flag set when the local player's locomotion needs to be
/// rebuilt on the main thread. This exists because Avian components
/// cannot be added/removed from `Query`-held mutable borrows — we have
/// to defer the surgery to a commands-only system.
#[derive(Component)]
pub(super) struct NeedsLocomotionRebuild;

/// Watch the live avatar record and flag the local player for rebuild
/// whenever the locomotion *variant* changes (intra-variant tuning edits
/// are handled by the per-frame sync systems). A
/// `Local<Option<&'static str>>` memoises the last-seen kind so we don't
/// rebuild on every frame the resource is `Changed` — the kinematics
/// sliders fire `Changed` constantly and would otherwise drop a dozen
/// rebuilds per second.
pub(super) fn detect_local_locomotion_change(
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
pub(super) fn apply_local_locomotion_rebuild(
    mut commands: Commands,
    players: Query<(Entity, Option<&Children>), (With<LocalPlayer>, With<NeedsLocomotionRebuild>)>,
    orphan_visuals: Query<Entity, (With<AvatarVisualPrim>, Without<ChildOf>)>,
    live: Res<LiveAvatarRecord>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
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
            &mut images,
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

/// Non-variant changes (slider tweaks inside the *same* preset, or
/// visuals-tree edits) only need new visual children — rigid-body
/// identity stays intact.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn rebuild_local_visuals(
    mut commands: Commands,
    live: Res<LiveAvatarRecord>,
    players: Query<
        (Entity, Option<&Children>),
        (With<LocalPlayer>, Without<NeedsLocomotionRebuild>),
    >,
    orphan_visuals: Query<Entity, (With<AvatarVisualPrim>, Without<ChildOf>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
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
            &mut images,
            &mut avatar_deps,
            true,
        );
    }
}

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
pub(super) fn detect_remote_change(
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
    mut images: ResMut<Assets<Image>>,
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
            &mut images,
            &mut avatar_deps,
            false,
        );
        commands
            .entity(entity)
            .insert(AppliedAvatar(record.clone()));
    }
}

/// Lift the player above freshly hot-loaded terrain (a region re-seed can
/// raise the ground under their feet mid-session).
pub(super) fn lift_player_above_new_ground(
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
