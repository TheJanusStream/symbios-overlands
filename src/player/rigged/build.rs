use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use bevy_symbios_avatar::spawn_avatar;
use symbios_avatar::Avatar;

use crate::interaction::locomotion::locomotion_total_height;
use crate::state::{LiveAvatarRecord, LocalPlayer, RemotePeer};

use super::{
    DRAFT_ATLAS, RiggedApplied, RiggedBuild, RiggedMotion, RiggedRoot, RiggedSettle, RiggedSteady,
    SETTLE_SECS,
};

/// Start a build for every chassis whose resolved rigged record is not the
/// one standing under it, and tear down rigged state on a chassis whose
/// body stopped being rigged.
#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub(in crate::player) fn kick_rigged_builds(
    mut commands: Commands,
    time: Res<Time>,
    live: Option<Res<LiveAvatarRecord>>,
    locals: Query<Entity, With<LocalPlayer>>,
    peers: Query<(Entity, Ref<RemotePeer>)>,
    applied: Query<&RiggedApplied>,
    building: Query<&RiggedBuild>,
    steady: Query<(), With<RiggedSteady>>,
    settle: Query<&RiggedSettle>,
    roots: Query<(Entity, &ChildOf), With<RiggedRoot>>,
) {
    let now = time.elapsed_secs();
    // One pass over the roots instead of one per body (#1135). The inner scan
    // was `O(bodies × roots)` every frame, and both terms are the peer count
    // — so the cost of standing in a room grew with its square.
    let chassis_with_root: bevy::platform::collections::HashSet<Entity> = roots
        .iter()
        .map(|(_, child_of)| child_of.parent())
        .collect();
    let full_atlas = symbios_avatar::AvatarConfig::default().atlas;

    let mut visit = |chassis: Entity,
                     record: Option<&crate::pds::AvatarRecord>,
                     source_changed: bool| {
        // The gate that makes standing still free (#1135).
        //
        // Everything below this — a full `AvatarRecord` deep-equality against
        // the body that is standing, per body, every frame — used to run for
        // thousands of consecutive frames to conclude "unchanged". It can be
        // skipped only when nothing that feeds it can have moved, and that is
        // three conditions, not one:
        //
        //   * the record this chassis draws from has not changed since the
        //     last look. A bare `Changed<>` gate would stop here and be
        //     WRONG, because the record can change while a build is in flight
        //     — the change is noticed, no build is kicked (one at a time per
        //     chassis), and it is the NEXT frame's mismatch that kicks the
        //     newer one. `RiggedSteady` is therefore a latch, not a tick: set
        //     only once the chassis is genuinely reconciled, and cleared by
        //     any change, so a change noticed mid-flight stays noticed.
        //   * the standing body was built at the FULL atlas. This is what
        //     keeps the settle ladder (#1059) working: a draft-atlas body is
        //     owed a full-atlas rebuild on a TIMER with no record change
        //     behind it, so while one is owed the answer really can change
        //     with nothing but the clock, and the ladder has to keep being
        //     re-evaluated. At the full atlas there is no rung above.
        //   * a root is actually standing, and no build is in flight.
        if source_changed {
            commands.entity(chassis).remove::<RiggedSteady>();
        } else if steady.contains(chassis)
            && chassis_with_root.contains(&chassis)
            && !building.contains(chassis)
            && applied.get(chassis).is_ok_and(|b| b.atlas >= full_atlas)
        {
            return;
        }

        let rigged = record.and_then(|r| r.body.rigged_ref());
        // Rigged but unresolved is a WAIT, not a teardown: a live-preview
        // broadcast arrives with its references unresolved (`resolved` never
        // rides the wire), and tearing the standing body down while
        // `network::peer_cache` re-resolves would blink every rigged peer
        // out on every preview. The body that is up stays up.
        if rigged.is_some_and(|rig| rig.resolved.is_none()) {
            return;
        }
        let resolved = rigged.and_then(|rig| rig.resolved.as_ref());
        match resolved {
            Some(resolved) => {
                let has_root = chassis_with_root.contains(&chassis);
                let built = applied.get(chassis).ok();
                let same_record = built.is_some_and(|built| built.record == resolved.body);
                // The draft/settle ladder (#1059): while a record is moving —
                // an editor slider mid-drag, a stream of peer previews — a
                // build is only worth the draft atlas, because the next edit
                // obsoletes it; once it has been still for SETTLE_SECS the
                // full-atlas build is owed, even though nothing changed.
                if !same_record {
                    commands
                        .entity(chassis)
                        .insert(RiggedSettle { changed_at: now });
                }
                let settled = settle
                    .get(chassis)
                    .ok()
                    .is_none_or(|s| now - s.changed_at >= SETTLE_SECS);
                let atlas = if settled { full_atlas } else { DRAFT_ATLAS };
                let atlas_owed = built.is_some_and(|built| built.atlas < atlas);
                if same_record && has_root && !atlas_owed {
                    // Reconciled: latch it so the compare above is skipped
                    // until something clears the latch.
                    commands.entity(chassis).insert(RiggedSteady);
                    return;
                }
                // One in flight per chassis: a stale target lands, and the
                // next frame's mismatch kicks the newer one.
                if building.contains(chassis) {
                    return;
                }
                let target = resolved.body.clone();
                let offset = record.map_or(0.0, |r| locomotion_total_height(&r.locomotion) / 2.0);
                // Through the platform-routed offload (#1061), not the compute
                // pool directly: on wasm that pool runs on the main thread, so
                // every body would be a dropped frame or several. Native still
                // lands on `AsyncComputeTaskPool` inside `offload`.
                let task = crate::offload::offload(crate::offload::GenJob::AvatarBuild {
                    record: Box::new(target.clone()),
                    atlas,
                });
                commands.entity(chassis).insert(RiggedBuild {
                    target,
                    atlas,
                    offset,
                    kicked_at: now as f64,
                    task,
                });
            }
            None => {
                // Not rigged (or not resolved): the generator path owns this
                // chassis. Drop any rigged residue so switching back later
                // rebuilds from scratch.
                if applied.contains(chassis) || building.contains(chassis) {
                    commands.entity(chassis).remove::<(
                        RiggedApplied,
                        RiggedBuild,
                        RiggedSteady,
                        RiggedSettle,
                    )>();
                    for (root, child_of) in &roots {
                        if child_of.parent() == chassis {
                            commands.entity(root).despawn();
                        }
                    }
                }
            }
        }
    };

    if let Some(live) = live.as_ref() {
        let changed = live.is_changed();
        for chassis in &locals {
            visit(chassis, Some(&live.0), changed);
        }
    }
    for (chassis, peer) in &peers {
        let changed = peer.is_changed();
        visit(chassis, peer.avatar.as_ref(), changed);
    }
}

/// Land finished builds: swap the skinned body in under its offset root.
#[allow(clippy::too_many_arguments)]
pub(in crate::player) fn land_rigged_builds(
    mut commands: Commands,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    mut builds: Query<(Entity, &mut RiggedBuild)>,
    roots: Query<(Entity, &ChildOf), With<RiggedRoot>>,
    // Both optional, because headless embedders (the render tool, minimal
    // test worlds) run this without the diagnostics plugin.
    mut metrics: Option<ResMut<crate::diagnostics::MetricsRegistry>>,
    mut session_log: Option<ResMut<crate::diagnostics::SessionLog>>,
) {
    use bevy::tasks::{block_on, futures_lite::future};
    for (chassis, mut build) in &mut builds {
        let Some(result) = block_on(future::poll_once(&mut build.task)) else {
            continue;
        };
        // The job roster is shared, so the variant is matched rather than
        // assumed: anything else here is a dispatch bug, not a bad body.
        let result = match result {
            crate::offload::GenResult::Avatar(avatar) => avatar.map(|boxed| *boxed),
            _ => {
                error!("an avatar build returned some other job's result");
                None
            }
        };
        commands.entity(chassis).remove::<RiggedBuild>();
        // Stamped even on failure: the engine returns None for exactly one
        // reason (limbs overlapping at a joint), and re-kicking the same
        // doomed record every frame would burn a core proving it. A changed
        // record re-triggers through the value comparison.
        commands.entity(chassis).insert(RiggedApplied {
            record: build.target.clone(),
            atlas: build.atlas,
        });
        // How long the chassis stood bodiless, and whether it got one (#1078).
        // Reported before the failure branch so a doomed record is visible in
        // the timeline rather than only in the warn.
        let waited = (time.elapsed_secs_f64() - build.kicked_at).max(0.0);
        if let Some(metrics) = metrics.as_deref_mut() {
            crate::diagnostics::samplers::rigged_build_secs(metrics, waited);
            if result.is_none() {
                crate::diagnostics::samplers::rigged_build_failed(metrics);
            }
        }
        if let Some(log) = session_log.as_deref_mut() {
            log.info(
                time.elapsed_secs_f64(),
                crate::diagnostics::event::EventPayload::RiggedBuildCompleted {
                    atlas: build.atlas,
                    duration_secs: waited,
                    ok: result.is_some(),
                },
            );
        }
        let Some(avatar) = result else {
            warn!("a rigged avatar record described a body that could not be built");
            continue;
        };
        let stale: Vec<Entity> = roots
            .iter()
            .filter(|(_, child_of)| child_of.parent() == chassis)
            .map(|(root, _)| root)
            .collect();
        install_built_body(
            &mut commands,
            chassis,
            build.offset,
            avatar,
            &stale,
            &mut meshes,
            &mut materials,
            &mut images,
            &mut bindposes,
        );
    }
}

/// Replace whatever rigged body stands under `chassis` with `avatar`, hung
/// off a fresh [`RiggedRoot`] whose offset puts the engine's ground plane at
/// the chassis collider's bottom. Split from [`land_rigged_builds`] so a
/// test can land a body it built itself, at whatever atlas it can afford.
#[allow(clippy::too_many_arguments)]
pub(in crate::player) fn install_built_body(
    commands: &mut Commands,
    chassis: Entity,
    offset: f32,
    avatar: Avatar,
    stale_roots: &[Entity],
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
) {
    for &root in stale_roots {
        commands.entity(root).despawn();
    }
    let root = commands
        .spawn((
            RiggedRoot,
            RiggedMotion::default(),
            rigged_root_transform(offset),
            Visibility::default(),
            ChildOf(chassis),
        ))
        .id();
    spawn_avatar(
        commands, root, avatar, 0.0, meshes, materials, images, bindposes,
    );
}

/// Where the skinned body hangs relative to its chassis (#1066).
///
/// Two corrections, both of them convention mismatches rather than tuning:
///
/// * **Height** — the engine's ground plane is `y = 0`, so the body drops by
///   half the collider so its feet meet the chassis capsule's bottom.
/// * **Facing** — a half turn about Y. `symbios_avatar::rig::landmark::FORWARD`
///   is `+Z`, the glTF/VRM convention the engine shares; Bevy's forward is
///   `-Z`, and the chassis is steered by
///   `Transform::looking_to(movement_direction, Y)`, which aims *its* `-Z`
///   down the direction of travel. Hanging the body off that with no rotation
///   pointed the engine's `+Z` face directly away from where the avatar was
///   going — walking correctly, moonwalking visibly. The half turn is applied
///   here, on the one entity that bridges the two conventions, rather than by
///   re-aiming the chassis (which the camera, the vehicles and the locomotion
///   drive all share) or by rotating the clips (which are authored in the
///   engine's frame and are consistent with the body).
///
/// Everything below this entity inherits the turn together — geometry, rig,
/// clips, and the socket anchors that
/// [`crate::player::attachments::LocalAttachment::rest_frame`] reconstructs an
/// offset against — so worn props stay put relative to the body they are on.
pub(super) fn rigged_root_transform(offset: f32) -> Transform {
    Transform::from_xyz(0.0, -offset, 0.0)
        .with_rotation(Quat::from_rotation_y(std::f32::consts::PI))
}
