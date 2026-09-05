use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use bevy_symbios_avatar::spawn_avatar;
use symbios_avatar::Avatar;

use crate::interaction::locomotion::locomotion_total_height;
use crate::state::{LiveAvatarRecord, LocalPlayer, RemotePeer};

use super::{
    DRAFT_ATLAS, RiggedApplied, RiggedBuild, RiggedBuildFailed, RiggedMotion, RiggedRoot,
    RiggedSettle, RiggedSteady, SETTLE_SECS,
};

/// What the owner is told when their own body cannot be built (#1255).
///
/// Names the one documented cause in the user's own vocabulary and points
/// at both escape routes, because the failure leaves nothing on screen to
/// click: with no body ever installed there is no geometry to select, and
/// with one standing the edit is a silent no-op on stale geometry.
pub(in crate::player) const BUILD_FAILED_LINE: &str =
    "This body can't be built at these proportions — press Ctrl+Z, or move the shape sliders back.";

/// The part of an engine record the BUILT BODY depends on (#1257 f110).
///
/// Three of `EngineAvatarRecord`'s fields cannot change a mesh or a texel:
/// the wardrobe display `name`, the `seed` of the last re-roll, and the
/// `locks` a re-roll must respect. The engine's own editor already knows
/// this and says so — its sections return `(changed, noted)`, where `noted`
/// means "the record changed and the body did not", and its doc warns that a
/// host ignoring the distinction "would pay a draft build per letter". The
/// host collapsed the two flags, and this comparison is where that landed:
/// `built.record == resolved.body` over the WHOLE record, so typing a name
/// re-armed `RiggedSettle` and dispatched a fresh draft-atlas build every
/// quarter-second of typing — visibly re-popping the body through the low
/// atlas, and on wasm paying a worker round trip per keystroke burst.
///
/// Fixed HERE rather than by routing the flags, because the record still has
/// to reach `live.set_changed()`: the undo ring captures on that tick
/// (`capture_avatar_history`), and so does the peer preview broadcast. Only
/// the REBUILD was wrong, so only the rebuild's question is narrowed.
///
/// Built by clearing the three inert fields on a clone rather than by
/// listing the ones that matter: a field added upstream then stays in the
/// comparison by default, so a dependency bump can cost a redundant rebuild
/// but can never silently skip a needed one.
fn build_identity(
    record: &crate::pds::avatar::EngineAvatarRecord,
) -> crate::pds::avatar::EngineAvatarRecord {
    let mut identity = record.clone();
    identity.name = String::new();
    identity.seed = 0;
    identity.locks = Default::default();
    identity
}

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
    // #1255: the last build for this chassis produced no body. Only
    // meaningful alongside `RiggedApplied.record` — see the component.
    failed: Query<(), With<RiggedBuildFailed>>,
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
        //
        // A FAILED build is reconciled too (#1255). The stamp below always
        // claimed this — "re-kicking the same doomed record every frame
        // would burn a core" — but only delivered it for a chassis that
        // already had a body standing, because both this gate and the latch
        // further down also require a root. A build that fails installs no
        // root, so the one case the comment names, a doomed record with
        // nothing standing, re-dispatched the same build every frame for
        // the rest of the session.
        //
        // `RiggedBuildFailed` is deliberately NOT cleared here. It is only
        // ever read beside `RiggedApplied.record`, so a record that really
        // changed invalidates it through the value compare below — while a
        // `source_changed` that turns out to touch nothing (the resource is
        // shared by every local surface) leaves the chassis reconciled
        // instead of re-dispatching the doomed build one more time.
        if source_changed {
            commands.entity(chassis).remove::<RiggedSteady>();
        } else if steady.contains(chassis)
            && !building.contains(chassis)
            && (failed.contains(chassis)
                || (chassis_with_root.contains(&chassis)
                    && applied.get(chassis).is_ok_and(|b| b.atlas >= full_atlas)))
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
                // Compared on the build identity, not the whole record
                // (#1257 f110): a name, a seed number or a lock toggle
                // changes the record and not the body.
                let same_record = built.is_some_and(|built| {
                    build_identity(&built.record) == build_identity(&resolved.body)
                });
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
                // A record whose last build FAILED is reconciled: there is
                // nothing left to try (#1255). The atlas ladder is skipped
                // for it deliberately — a draft failure is not a texture
                // problem, so re-running it at the full atlas only spends a
                // second build to fail identically.
                let doomed = same_record && failed.contains(chassis);
                if doomed || (same_record && has_root && !atlas_owed) {
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
                    announced: false,
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
    // `Has<LocalPlayer>` because only the owner's own body is worth a toast
    // (#1255): a peer's failed build is their editor's problem, and the
    // metrics and session-log lines below already cover both. `Has<..Failed>`
    // is the rising edge — a run of consecutive doomed builds says it once
    // and lets the editor's banner carry the standing state.
    mut builds: Query<(
        Entity,
        &mut RiggedBuild,
        Has<LocalPlayer>,
        Has<RiggedBuildFailed>,
    )>,
    roots: Query<(Entity, &ChildOf), With<RiggedRoot>>,
    // All optional, because headless embedders (the render tool, minimal
    // test worlds) run this without the diagnostics or UI plugins.
    mut metrics: Option<ResMut<crate::diagnostics::MetricsRegistry>>,
    mut session_log: Option<ResMut<crate::diagnostics::SessionLog>>,
    mut toasts: Option<ResMut<crate::ui::toast::Toasts>>,
) {
    use bevy::tasks::{block_on, futures_lite::future};
    for (chassis, mut build, is_local, was_failing) in &mut builds {
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
            // #1255. Before this the failure was silent to the user: a warn
            // on a console they cannot see, a Diagnostics counter, and a
            // session-log line. What they actually saw was their own body
            // missing from their own camera (no rigged root was ever
            // installed and nothing draws a placeholder), or — with a body
            // already standing — an edit that did nothing at all.
            commands.entity(chassis).insert(RiggedBuildFailed);
            if is_local
                && !was_failing
                && let Some(toasts) = toasts.as_deref_mut()
            {
                toasts.warn(BUILD_FAILED_LINE, time.elapsed_secs_f64());
            }
            continue;
        };
        // Landed a body, so whatever the last attempt did is history.
        commands.entity(chassis).remove::<RiggedBuildFailed>();
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
