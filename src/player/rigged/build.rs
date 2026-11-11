use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use bevy_symbios_avatar::{AvatarBody, AvatarDriver, Drive, spawn_avatar};
use symbios_avatar::Avatar;

use crate::player::attachments::{CarriedHatLine, HatLine};
// The three questions a rigged body is built and hung by live beside the
// avatar spawn path (#1482), so the render tool's `--rigged` asks them the
// same way; `rigged_root_transform` is re-exported for this module's tests.
pub(super) use crate::player::visuals::rigged_root_transform;
use crate::player::visuals::{rigged_build_job, rigged_root_offset, settled_atlas};
use crate::state::{LiveAvatarRecord, LocalPlayer, RemotePeer};

use super::{
    DRAFT_ATLAS, RiggedApplied, RiggedBuild, RiggedBuildFailed, RiggedRoot, RiggedSettle,
    RiggedSteady, RiggedTrail, SETTLE_SECS, next_room_seed,
};
use crate::pds::avatar::RiggedKind;

/// What the owner is told when their own body cannot be built (#1255).
///
/// Names the one documented cause in the user's own vocabulary and points
/// at both escape routes, because the failure leaves nothing on screen to
/// click: with no body ever installed there is no geometry to select, and
/// with one standing the edit is a silent no-op on stale geometry.
pub(in crate::player) const BUILD_FAILED_LINE: &str =
    "This body can't be built at these proportions - press Ctrl+Z, or move the shape sliders back.";

/// The part of an engine record the BUILT BODY depends on (#1257 f110).
///
/// Three of `EngineAvatarRecord`'s fields cannot change a mesh or a texel:
/// the wardrobe display `name`, the `seed` of the last re-roll, and the
/// `locks` a re-roll must respect. The engine's own editor already knows
/// this and says so - its sections return `(changed, noted)`, where `noted`
/// means "the record changed and the body did not", and its doc warns that a
/// host ignoring the distinction "would pay a draft build per letter". The
/// host collapsed the two flags, and this comparison is where that landed:
/// `built.record == resolved.body` over the WHOLE record, so typing a name
/// re-armed `RiggedSettle` and dispatched a fresh draft-atlas build every
/// quarter-second of typing - visibly re-popping the body through the low
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
    // meaningful alongside `RiggedApplied.record` - see the component.
    failed: Query<(), With<RiggedBuildFailed>>,
) {
    let now = time.elapsed_secs();
    // One pass over the roots instead of one per body (#1135). The inner scan
    // was `O(bodies × roots)` every frame, and both terms are the peer count
    // - so the cost of standing in a room grew with its square.
    let chassis_with_root: bevy::platform::collections::HashSet<Entity> = roots
        .iter()
        .map(|(_, child_of)| child_of.parent())
        .collect();
    let full_atlas = settled_atlas();

    let mut visit =
        |chassis: Entity, record: Option<&crate::pds::AvatarRecord>, source_changed: bool| {
            // The gate that makes standing still free (#1135).
            //
            // Everything below this - a full `AvatarRecord` deep-equality against
            // the body that is standing, per body, every frame - used to run for
            // thousands of consecutive frames to conclude "unchanged". It can be
            // skipped only when nothing that feeds it can have moved, and that is
            // three conditions, not one:
            //
            //   * the record this chassis draws from has not changed since the
            //     last look. A bare `Changed<>` gate would stop here and be
            //     WRONG, because the record can change while a build is in flight
            //     - the change is noticed, no build is kicked (one at a time per
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
            // claimed this - "re-kicking the same doomed record every frame
            // would burn a core" - but only delivered it for a chassis that
            // already had a body standing, because both this gate and the latch
            // further down also require a root. A build that fails installs no
            // root, so the one case the comment names, a doomed record with
            // nothing standing, re-dispatched the same build every frame for
            // the rest of the session.
            //
            // `RiggedBuildFailed` is deliberately NOT cleared here. It is only
            // ever read beside `RiggedApplied.record`, so a record that really
            // changed invalidates it through the value compare below - while a
            // `source_changed` that turns out to touch nothing (the resource is
            // shared by every local surface) leaves the chassis reconciled
            // instead of re-dispatching the doomed build one more time.
            if source_changed {
                // `try_*` at every command in this system that addresses a
                // chassis (#1411): the query gathers peers as well as the local
                // player, and `network::lifecycle` despawns a peer the frame
                // its transport drops - unordered against this one, so an
                // ordinary insert can land on an entity that is already gone
                // and abort the client (#1410).
                commands.entity(chassis).try_remove::<RiggedSteady>();
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
                    // The draft/settle ladder (#1059): while a record is moving -
                    // an editor slider mid-drag, a stream of peer previews - a
                    // build is only worth the draft atlas, because the next edit
                    // obsoletes it; once it has been still for SETTLE_SECS the
                    // full-atlas build is owed, even though nothing changed.
                    if !same_record {
                        commands
                            .entity(chassis)
                            .try_insert(RiggedSettle { changed_at: now });
                    }
                    let settled = settle
                        .get(chassis)
                        .ok()
                        .is_none_or(|s| now - s.changed_at >= SETTLE_SECS);
                    let atlas = if settled { full_atlas } else { DRAFT_ATLAS };
                    let atlas_owed = built.is_some_and(|built| built.atlas < atlas);
                    // A record whose last build FAILED is reconciled: there is
                    // nothing left to try (#1255). The atlas ladder is skipped
                    // for it deliberately - a draft failure is not a texture
                    // problem, so re-running it at the full atlas only spends a
                    // second build to fail identically.
                    let doomed = same_record && failed.contains(chassis);
                    if doomed || (same_record && has_root && !atlas_owed) {
                        // Reconciled: latch it so the compare above is skipped
                        // until something clears the latch.
                        commands.entity(chassis).try_insert(RiggedSteady);
                        return;
                    }
                    // One in flight per chassis: a stale target lands, and the
                    // next frame's mismatch kicks the newer one.
                    if building.contains(chassis) {
                        return;
                    }
                    let target = resolved.body.clone();
                    let offset = record.map_or(0.0, rigged_root_offset);
                    // Through the platform-routed offload (#1061), not the compute
                    // pool directly: on wasm that pool runs on the main thread, so
                    // every body would be a dropped frame or several. Native still
                    // lands on `AsyncComputeTaskPool` inside `offload`. The job
                    // itself - the far hair tier included - is
                    // `rigged_build_job`'s, which says why.
                    let task = crate::offload::offload(rigged_build_job(&target, atlas));
                    commands.entity(chassis).try_insert(RiggedBuild {
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
                        commands.entity(chassis).try_remove::<(
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
    // is the rising edge - a run of consecutive doomed builds says it once
    // and lets the editor's banner carry the standing state.
    mut builds: Query<(
        Entity,
        &mut RiggedBuild,
        Has<LocalPlayer>,
        Has<RiggedBuildFailed>,
    )>,
    roots: Query<(Entity, &ChildOf), With<RiggedRoot>>,
    standing: Query<(&AvatarBody, &AvatarDriver, &Drive, &RiggedTrail), With<RiggedRoot>>,
    live: Option<Res<LiveAvatarRecord>>,
    peers: Query<&RemotePeer>,
    // All optional, because headless embedders (the render tool, minimal
    // test worlds) run this without the diagnostics or UI plugins.
    mut metrics: Option<ResMut<crate::diagnostics::MetricsRegistry>>,
    mut session_log: Option<ResMut<crate::diagnostics::SessionLog>>,
    mut toasts: Option<ResMut<crate::notify::Toasts>>,
) {
    use bevy::tasks::{block_on, futures_lite::future};
    for (chassis, mut build, is_local, was_failing) in &mut builds {
        let Some(result) = block_on(future::poll_once(&mut build.task)) else {
            continue;
        };
        // The job roster is shared, so the variant is matched rather than
        // assumed: anything else here is a dispatch bug, not a bad body.
        // A web build brings its head's measure with it (#1815); a native
        // one, or one from an older worker, brings none.
        let (result, hat_line) = match result {
            crate::offload::GenResult::Avatar(avatar) => (avatar.map(|boxed| *boxed), None),
            crate::offload::GenResult::MeasuredAvatar { body, hat_line } => {
                (body.map(|boxed| *boxed), hat_line)
            }
            _ => {
                error!("an avatar build returned some other job's result");
                (None, None)
            }
        };
        // `try_*` here for the same reason as the kick above (#1411), and
        // more sharply: a build spans frames, so the peer it was kicked for
        // has had that long to leave.
        commands.entity(chassis).try_remove::<RiggedBuild>();
        // Stamped even on failure: the engine returns None for exactly one
        // reason (limbs overlapping at a joint), and re-kicking the same
        // doomed record every frame would burn a core proving it. A changed
        // record re-triggers through the value comparison.
        commands.entity(chassis).try_insert(RiggedApplied {
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
            // installed and nothing draws a placeholder), or - with a body
            // already standing - an edit that did nothing at all.
            commands.entity(chassis).try_insert(RiggedBuildFailed);
            if is_local
                && !was_failing
                && let Some(toasts) = toasts.as_deref_mut()
            {
                toasts.warn(BUILD_FAILED_LINE, time.elapsed_secs_f64());
            }
            continue;
        };
        // Landed a body, so whatever the last attempt did is history.
        commands.entity(chassis).try_remove::<RiggedBuildFailed>();
        let stale: Vec<Entity> = roots
            .iter()
            .filter(|(_, child_of)| child_of.parent() == chassis)
            .map(|(root, _)| root)
            .collect();
        let carried = stale
            .iter()
            .find_map(|&root| standing.get(root).ok())
            .and_then(|(body, driver, drive, trail)| {
                Carried::across(&body.avatar.rig, &avatar.rig, driver, drive, trail)
            });
        // Hung where its record says now (#1805): a build spans frames, and one
        // kicked under a lying record can land after its owner has stood up
        // again, with nothing then owing a rebuild.
        let record = if is_local {
            live.as_deref().map(|live| &live.0)
        } else {
            peers
                .get(chassis)
                .ok()
                .and_then(|peer| peer.avatar.as_ref())
        };
        install_carrying(
            &mut commands,
            chassis,
            record.map_or(build.offset, rigged_root_offset),
            avatar,
            hat_line,
            &stale,
            carried,
            &mut meshes,
            &mut materials,
            &mut images,
            &mut bindposes,
        );
    }
}

/// What a beast's new body takes over from the one it replaces (#1805, owner
/// decision D14), so a Sit or a Lie Down outlives a rebuild - which the
/// draft-to-full ladder runs about [`SETTLE_SECS`] after any change to the
/// record, on the owner's client and on every peer's. Kept until engine #383
/// gives a [`symbios_avatar::anim::Driver`] a seat of its own. **A winged
/// body's too** (#1875), so a resting or a flying one is not stood up or
/// snapped shut by the same ladder: across any change of winged body, a bird
/// made a bat among them, on the same reading of the driver. **Across a change
/// of winged family a rest still going down or held goes on as the new
/// family's** (#1878, [`rest_onto`]): each family's rest is its own roster's,
/// and a driver asked to hold a name its roster lacks lets it go - which is
/// what stands up a body that was getting up.
///
/// **Carried whole across any change of beast, because the driver is safe
/// across any**, read from symbios-avatar 0.14 rather than assumed. What it
/// keeps of the old rig is its rest cache, keyed on the rig's joints and its
/// outline's length and so solved again for a new one; its springs, found
/// again when they stop fitting; its footholds, which are world points by
/// limb; and its last two poses and any blend under way, which a change of
/// joint count only shortens: a blend stops at the shorter pose, a forward
/// solve reads what both have, and the idle's arrival refuses a pose that does
/// not fit. Every beast the engine rolls has the same 38 joints today, so a
/// change of count is read here and not yet driven by a test.
///
/// **With its [`Drive`] and its [`RiggedTrail`]**, the driver's other memories:
/// the swimming it read last frame (the thresholds have two sides), and where
/// the chassis was, whose absence reads a walking peer as a stop for a frame.
pub(in crate::player) struct Carried {
    driver: AvatarDriver,
    drive: Drive,
    trail: RiggedTrail,
}

impl Carried {
    /// What a body on `new` keeps of one standing on `old`: everything, when
    /// both are beasts or both winged ([`RiggedKind::same_body`], read off the
    /// rigs as drawn), and nothing when either is a person, who keeps the
    /// fresh driver a rebuild has always given them, nor across a change of
    /// kind.
    fn across(
        old: &symbios_avatar::Rig,
        new: &symbios_avatar::Rig,
        driver: &AvatarDriver,
        drive: &Drive,
        trail: &RiggedTrail,
    ) -> Option<Self> {
        let (from, to) = (RiggedKind::of_rig(old), RiggedKind::of_rig(new));
        let carries = matches!(from, RiggedKind::Beast | RiggedKind::Winged(_));
        (carries && from.same_body(to)).then(|| {
            let mut driver = driver.0.clone();
            if let RiggedKind::Winged(family) = to
                && from != to
            {
                rest_onto(&mut driver, drive, old, new, family);
            }
            Self {
                driver: AvatarDriver(driver),
                drive: drive.clone(),
                trail: trail.clone(),
            }
        })
    }
}

/// A winged rest carried onto a body of another family, `family` leading it
/// (#1878): a perched bird made a bat rests as a bat rests, and a curled glider
/// made a bird perches. The new family's own rest
/// ([`crate::player::emote::WingedEmote::Rest`]), as far down as the old one
/// had gone, held if it was all the way down - a bird's roost too, which only
/// a bird has. **Only a rest still going down or held, untouched**
/// ([`still_resting`]): a body getting up - told to, got up by another gesture
/// or by moving - is let go, and stands up on the new body, whose roster lacks
/// the old rest's name. Nothing else changes: an emote, a flight and a rest
/// the new roster also has go on as they were.
///
/// **A rest asked for again while the body was getting up is seated afresh**
/// (#1886): the engine gets the body up and then starts the rest again, from
/// standing ([`StillResting::Afresh`]), so it is seated at the first instant
/// of the new family's rest, and goes down from standing on the new body.
/// Seated by how long the old rest had run - its clock runs on through the
/// getting up, past its whole way down - a body nearly standing was put
/// fully down in a frame.
fn rest_onto(
    driver: &mut symbios_avatar::anim::Driver,
    drive: &Drive,
    old: &symbios_avatar::Rig,
    new: &symbios_avatar::Rig,
    family: symbios_avatar::WingedFamily,
) {
    use symbios_avatar::anim::emote;
    let Some((name, elapsed)) = driver.gesture() else {
        return;
    };
    let Some(down) = emote::seconds(old, name) else {
        return;
    };
    if emote::seconds(new, name).is_some() {
        return;
    }
    let resting = still_resting(driver, drive, old, name, down);
    if resting == StillResting::No {
        return;
    }
    let Some(rest) = crate::player::emote::WingedEmote::Rest.gesture_name(family) else {
        return;
    };
    let through = if down <= 0.0 {
        1.0
    } else if resting == StillResting::Afresh {
        // Its first instant: never nothing, which `seat` reads as standing
        // and out of it.
        f32::EPSILON
    } else {
        (elapsed / down).min(1.0)
    };
    driver.seat(new, rest, through);
}

/// Whether the rest `name` the driver plays on `rig`, which takes `down`
/// seconds to go down, is still going down or held and nothing has got it up
/// ([`StillResting::On`]): not told to get up, nor to play another gesture,
/// nor moved, since it began - nor about to be, by what `drive` asks of the
/// next frame. Anything else is [`StillResting::No`], but for the rest asked
/// for again below.
///
/// **Asked of the driver itself**, as nothing else knows. symbios-avatar 0.23
/// says only the rest's name and how long it has run, and that clock runs on
/// through a getting up, so a body rising or coming back up part way reads as
/// further down (#1878). Where the rest has got to is the driver's own state,
/// and what it does next shows it: a copy is run on, once, for longer than the
/// rest takes to go down and to get up, with the drive's speed, water, flight
/// and request and nothing else. A rest going down or held is still held on
/// the copy, and one getting up is over - by the engine's own rules for what
/// gets a body up, not a copy of them here. The copy is dropped, and costs one
/// frame of the motion, only on a change of family under a resting body.
///
/// A rest asked for again while the body was getting up is held on the copy
/// too, started afresh (#1886, [`StillResting::Afresh`]): the engine gets
/// the body up and then starts the rest again, and in symbios-avatar 0.23.1
/// starts its clock after that frame's advance, so the copy's rest has run
/// nothing at all, where one that went on has run the copy's whole step
/// longer than before.
fn still_resting(
    driver: &symbios_avatar::anim::Driver,
    drive: &Drive,
    rig: &symbios_avatar::Rig,
    name: &str,
    down: f32,
) -> StillResting {
    use symbios_avatar::anim::{Inputs, emote, level_ground};
    let Some((_, up)) = emote::getting_up(rig, name) else {
        return StillResting::No;
    };
    let mut copy = driver.clone();
    let step = down + up + 1.0;
    let inputs = Inputs {
        delta: step,
        velocity: drive.velocity,
        at: drive.at,
        facing: drive.facing,
        swimming: drive.swimming,
        gesture: drive.gesture.as_deref(),
        showing: drive.showing,
        flight: drive.flight,
        ..Default::default()
    };
    let _ = copy.drive(rig, &inputs, level_ground);
    match copy.gesture().filter(|(held, _)| *held == name) {
        None => StillResting::No,
        Some((_, elapsed)) if elapsed < step => StillResting::Afresh,
        Some(_) => StillResting::On,
    }
}

/// What [`still_resting`] finds of a winged body's rest across a change of
/// family (#1878, #1886).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StillResting {
    /// Got up, or about to be: let go.
    No,
    /// Going down or held, untouched: seated as far down as it had gone.
    On,
    /// Asked for again while getting up: once up, it starts again from
    /// standing, and is seated there.
    Afresh,
}

/// [`install_carrying`] with nothing carried: a fresh driver, as a test lands
/// a body it built itself, at whatever atlas it can afford.
#[cfg(test)]
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
    install_carrying(
        commands,
        chassis,
        offset,
        avatar,
        None,
        stale_roots,
        None,
        meshes,
        materials,
        images,
        bindposes,
    );
}

/// Replace whatever rigged body stands under `chassis` with `avatar`, hung
/// off a fresh [`RiggedRoot`] whose offset puts the engine's ground plane at
/// the chassis collider's bottom. Split from [`land_rigged_builds`] so a
/// test can land a body it built itself, at whatever atlas it can afford.
///
/// **The three components the motion needs go on at spawn** (#1171): the
/// driver that holds every clock outliving a frame, the [`Drive`] whose
/// presence is what opts the body into
/// [`bevy_symbios_avatar::drive_avatar_bodies`] at all, and the
/// [`RiggedTrail`] that says this body's chassis has not been seen to move
/// yet. [`Drive::default`] stands at the origin, and nothing ever reads that:
/// [`super::fill_rigged_drive`] writes a real place before the driver first
/// sees the body.
///
/// **A beast takes over what `carried` says of the old body** ([`Carried`]).
/// A carried trail's surface lift goes onto the new root as well, which the
/// fill reads the hang back out from under.
///
/// **A head measure the build took goes onto the root beside the body**
/// (#1815, [`CarriedHatLine`]), for the dressing to fit a circlet to.
#[allow(clippy::too_many_arguments)]
fn install_carrying(
    commands: &mut Commands,
    chassis: Entity,
    offset: f32,
    avatar: Avatar,
    hat_line: Option<HatLine>,
    stale_roots: &[Entity],
    carried: Option<Carried>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
) {
    for &root in stale_roots {
        commands.entity(root).despawn();
    }
    let mut hung = rigged_root_transform(offset);
    let (driver, drive, trail) = match carried {
        Some(Carried {
            driver,
            drive,
            trail,
        }) => {
            hung.translation.y += trail.lift;
            (driver, drive, trail)
        }
        None => (
            AvatarDriver::seeded(next_room_seed()),
            Drive::default(),
            RiggedTrail::default(),
        ),
    };
    let root = commands
        .spawn((
            RiggedRoot,
            driver,
            drive,
            trail,
            hung,
            Visibility::default(),
            ChildOf(chassis),
        ))
        .id();
    if let Some(hat_line) = hat_line {
        commands.entity(root).insert(CarriedHatLine(hat_line));
    }
    spawn_avatar(
        commands, root, avatar, 0.0, meshes, materials, images, bindposes,
    );
}
