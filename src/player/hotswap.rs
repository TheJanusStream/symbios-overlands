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

/// [`visuals::spawn_avatar_visuals`] with its synchronous main-thread wall
/// time recorded under `runtime.avatar_rebuild.ms` (#807): the histogram
/// attributes the re-roll hitch — with texture bakes offloaded on wasm, what
/// remains in here is dominated by part meshing.
///
/// The registry is reached through `deps.caches.metrics` — `Option`al, so a
/// headless / test app without the diagnostics plugin never panics — and
/// deliberately NOT as an own `ResMut<MetricsRegistry>` parameter on the
/// calling systems: `GeneratorCaches` (inside `deps`) carries that access
/// since #921, and a sibling parameter aliases it — a B0002 panic at
/// schedule build (#924).
#[allow(clippy::too_many_arguments)]
fn timed_spawn_avatar_visuals(
    commands: &mut Commands,
    chassis: Entity,
    body: &crate::pds::AvatarBody,
    existing_children: Option<&Children>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    deps: &mut visuals::AvatarSpawnDeps,
    is_local: bool,
) {
    let start = bevy::platform::time::Instant::now();
    visuals::spawn_avatar_visuals(
        commands,
        chassis,
        body,
        existing_children,
        meshes,
        materials,
        images,
        deps,
        is_local,
    );
    let elapsed = start.elapsed().as_secs_f64();
    if let Some(m) = deps.caches.metrics.as_deref_mut() {
        crate::diagnostics::samplers::avatar_rebuild_secs(m, elapsed);
    }
}

/// Snapshot of the last `AvatarRecord` whose visuals have been painted onto
/// a remote peer. `detect_remote_change` listens to the broad
/// `Changed<RemotePeer>` signal (which also fires on mute/handle/DID edits)
/// and compares against this snapshot so an unrelated field flip doesn't
/// re-enter the expensive visual rebuild path.
#[derive(Component)]
pub(super) struct AppliedAvatar(AvatarRecord);

/// The body the local chassis's visual children were last painted for,
/// with nothing worn ([`crate::pds::AvatarBody::sans_attachments`]) —
/// the local twin of [`AppliedAvatar`] (#1104).
///
/// [`rebuild_local_visuals`] used to respawn on *every* `LiveAvatarRecord`
/// change, and [`visuals::spawn_avatar_visuals`] clears every chassis child
/// — the rigged body's root included — so a worn prop's offset nudge tore
/// the whole body down; the rigged pipeline then saw no root, kicked an
/// async build, and the avatar was gone until it landed. Worn props are
/// dressed from the record by `attachments::sync_rigged_attachments`, so
/// they never needed the body respawned. Lives on the chassis (not a
/// `Local`) so a fresh chassis — room travel, respawn — carries no
/// snapshot and paints from scratch.
#[derive(Component)]
pub(super) struct AppliedLocalBody(crate::pds::AvatarBody);

impl AppliedLocalBody {
    /// The snapshot for a chassis whose children were just painted from
    /// `body` — stamped by every local paint site (first spawn, the
    /// locomotion hot-swap, and the visuals rebuild itself).
    pub(super) fn painted(body: &crate::pds::AvatarBody) -> Self {
        Self(body.sans_attachments())
    }
}

/// Whether a record change owes the chassis a visual respawn through
/// [`visuals::spawn_avatar_visuals`] (#1104): nothing painted yet, a
/// body-kind change, or a generator tree that differs from the one
/// painted. Two rigged bodies never do — that path spawns nothing for
/// them, and the rigged pipeline (`rigged::kick_rigged_builds`) already
/// compares the engine record itself and replaces the standing root only
/// once the new build has landed, so a body edit no longer shows a naked
/// capsule in between either.
fn needs_visual_respawn(
    applied: Option<&crate::pds::AvatarBody>,
    live_sans_attachments: &crate::pds::AvatarBody,
) -> bool {
    use crate::pds::AvatarBody;
    match applied {
        None => true,
        Some(AvatarBody::Rigged(_)) if matches!(live_sans_attachments, AvatarBody::Rigged(_)) => {
            false
        }
        Some(applied) => applied != live_sans_attachments,
    }
}

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
///
/// DEFERRED while the visuals-edit freeze parks the chassis (#867,
/// `Without<VisualsEditFreeze>`): stripping + reinserting the `Collider`
/// on a parked body that is touching the terrain corrupts avian 0.6's
/// contact/island bookkeeping — the same class as the #740
/// `RigidBodyDisabled` cycle the freeze itself avoids — and the broken
/// pair surfaces on freeze release as a clean fall through the world
/// followed by a runaway respawn→NaN feedback (the #867 meltdown).
/// `NeedsLocomotionRebuild` simply stays parked on the entity; the
/// freeze holds the pose so the stale body is invisible mid-edit, and
/// the marker is removed at release-time flush, so this system applies
/// the rebuild on the first frame the body is live again. Visuals-only
/// edits keep flowing through [`rebuild_local_visuals`] regardless.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn apply_local_locomotion_rebuild(
    mut commands: Commands,
    players: Query<
        (Entity, Option<&Children>),
        (
            With<LocalPlayer>,
            With<NeedsLocomotionRebuild>,
            Without<super::VisualsEditFreeze>,
        ),
    >,
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
        timed_spawn_avatar_visuals(
            &mut commands,
            entity,
            &live.0.body,
            children,
            &mut meshes,
            &mut materials,
            &mut images,
            &mut avatar_deps,
            true,
        );
        commands
            .entity(entity)
            .insert(AppliedLocalBody::painted(&live.0.body));
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
///
/// The `NeedsLocomotionRebuild` skip only applies while the body rebuild
/// can actually run this frame: since #867 defers that rebuild for the
/// whole frozen editing session, a kind-changing re-seed would otherwise
/// starve the cosmetic repaint too and the re-roll stayed invisible
/// until the editor closed (#870). While the freeze marker is present
/// the visuals repaint here on every record change — physics components
/// stay untouched, which is exactly what the deferral protects — at the
/// cost of one redundant repaint when the deferred rebuild lands at
/// release.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn rebuild_local_visuals(
    mut commands: Commands,
    live: Res<LiveAvatarRecord>,
    players: Query<
        (Entity, Option<&Children>, Option<&AppliedLocalBody>),
        (
            With<LocalPlayer>,
            Or<(
                Without<NeedsLocomotionRebuild>,
                With<super::VisualsEditFreeze>,
            )>,
        ),
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
    let live_body = live.0.body.sans_attachments();
    for (entity, children, applied) in players.iter() {
        // Attachment-only edits, and rigged-to-rigged body edits, owe no
        // respawn here (#1104): the props and the skinned body each have
        // their own diff-driven pipeline. The snapshot still advances so
        // the next comparison is against what is actually painted.
        if !needs_visual_respawn(applied.map(|a| &a.0), &live_body) {
            if applied.is_none_or(|a| a.0 != live_body) {
                commands
                    .entity(entity)
                    .insert(AppliedLocalBody::painted(&live.0.body));
            }
            continue;
        }
        despawn_orphan_avatar_visuals(&mut commands, &orphan_visuals);
        commands
            .entity(entity)
            .insert(AppliedLocalBody::painted(&live.0.body));
        timed_spawn_avatar_visuals(
            &mut commands,
            entity,
            &live.0.body,
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
        timed_spawn_avatar_visuals(
            &mut commands,
            entity,
            &record.body,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::AvatarBody;
    use crate::pds::avatar::wardrobe::engine_default_for_did;

    fn rigged(seed: &str) -> AvatarBody {
        let mut body = AvatarBody::rigged("3jzfcijpj2z2a");
        if let Some(rig) = body.rigged_mut() {
            rig.resolved = Some(crate::pds::avatar::ResolvedRig {
                body: engine_default_for_did(seed),
                attachments: Vec::new(),
            });
        }
        body
    }

    /// The respawn decision (#1104), case by case: a fresh chassis paints;
    /// two rigged bodies never respawn through the generator path, even
    /// when the engine record differs (the rigged pipeline owns that); a
    /// generator body respawns exactly when its tree changed; a kind
    /// change always respawns.
    #[test]
    fn a_visual_respawn_is_owed_only_for_generator_or_kind_changes() {
        let gen_a = AvatarBody::generator(crate::pds::Generator::default());
        let gen_b = {
            let mut g = crate::pds::Generator::default();
            g.transform.translation = crate::pds::types::Fp3([1.0, 0.0, 0.0]);
            AvatarBody::generator(g)
        };
        assert!(needs_visual_respawn(None, &rigged("did:plc:a")));
        assert!(needs_visual_respawn(None, &gen_a));
        assert!(!needs_visual_respawn(
            Some(&rigged("did:plc:a")),
            &rigged("did:plc:a")
        ));
        assert!(
            !needs_visual_respawn(Some(&rigged("did:plc:a")), &rigged("did:plc:b")),
            "a rigged body edit is the rigged pipeline's to land"
        );
        assert!(!needs_visual_respawn(Some(&gen_a), &gen_a));
        assert!(needs_visual_respawn(Some(&gen_a), &gen_b));
        assert!(needs_visual_respawn(Some(&gen_a), &rigged("did:plc:a")));
        assert!(needs_visual_respawn(Some(&rigged("did:plc:a")), &gen_a));
    }

    /// #1104 bug 2, reproduced in-app: a worn prop's offset nudge used to
    /// flip `LiveAvatarRecord`'s change tick into a whole-body respawn —
    /// `spawn_avatar_visuals` clears every chassis child, the rigged root
    /// included — so the avatar vanished until the async rebuild landed.
    /// After the edit the standing root must be the same entity.
    #[test]
    fn an_attachment_only_edit_keeps_the_rigged_body_standing() {
        use bevy::ecs::system::RunSystemOnce;
        let (mut app, _crown) = super::super::attachments::tests::dressed_app();
        let mut roots = app
            .world_mut()
            .query_filtered::<(Entity, &ChildOf), With<super::super::rigged::RiggedRoot>>();
        let (root, child_of) = roots.single(app.world()).expect("one root");
        let chassis = child_of.parent();
        // What the spawn path stamps when it paints the chassis.
        let painted = {
            let live = app.world().resource::<LiveAvatarRecord>();
            AppliedLocalBody::painted(&live.0.body)
        };
        app.world_mut().entity_mut(chassis).insert(painted);

        // The gizmo commit's write: one offset, nothing else.
        {
            let mut live = app.world_mut().resource_mut::<LiveAvatarRecord>();
            let rig = live.0.body.rigged_mut().expect("rigged");
            let resolved = rig.resolved.as_mut().expect("resolved");
            resolved.attachments[0].record.offset.translation =
                crate::pds::types::Fp3([0.0, 0.05, 0.0]);
        }
        app.world_mut()
            .run_system_once(rebuild_local_visuals)
            .expect("the rebuild system runs");

        assert!(
            app.world().get_entity(root).is_ok(),
            "the rigged root was torn down by an attachment-only edit"
        );
        let (root_after, _) = roots.single(app.world()).expect("still one root");
        assert_eq!(root_after, root, "the body kept its entity");

        // The snapshot advanced with the record, so the next comparison is
        // against what stands (a second identical run is a no-op too).
        app.world_mut()
            .run_system_once(rebuild_local_visuals)
            .expect("runs again");
        assert!(app.world().get_entity(root).is_ok());
    }
}
