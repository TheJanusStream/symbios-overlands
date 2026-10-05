//! Room-record publish pipeline: the async Save-to-PDS / hard-reset
//! tasks and the shared poll system that lands their results. Split out
//! of the editor orchestration in `mod.rs` (#650); the unsaved-edits
//! guard ([`crate::ui::unsaved_guard`]) drives the same pipeline for its
//! "Publish & continue" path.

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::diagnostics::event::{EventPayload, RecordKind};
use crate::diagnostics::{MetricsRegistry, SessionLog};
use crate::pds::{self, RoomRecord};
use crate::state::{PublishFeedback, PublishStatus, StoredRoomRecord};
use crate::ui::editable::poll_or_expire;

/// Async task for publishing the room record to the owner's PDS. Carries the
/// target `did` and the dispatch time so [`poll_publish_tasks`] can emit a typed
/// `RecordWrite*` session event (with the write's duration) when it resolves.
#[derive(Component)]
pub struct PublishRoomTask {
    pub task: bevy::tasks::Task<Result<(), String>>,
    pub did: String,
    pub spawned_at: f64,
    /// Serialized size of the record being written, measured at dispatch so
    /// the poll system can gauge + log it (#694). `None` only on a
    /// serialization failure, which the publish itself will also report.
    pub record_bytes: Option<usize>,
    /// The exact record this task handed to the PDS. On success `stored`
    /// is pinned to THIS, never to whatever `live` holds when the task
    /// lands (#1116): an edit made while a save is in flight would
    /// otherwise be marked clean without ever having been written, and
    /// the dirty flag is derived from `records_differ(live, stored)`, so
    /// there is nothing left to notice it. Since #1110 `stored` is also
    /// the baseline the attachment delete set is derived from, which
    /// makes a wrong snapshot here a wrong *delete* on the next save.
    pub published: RoomRecord,
}

/// Async task for the hard-reset publish path (wipe-then-republish). Separate
/// from `PublishRoomTask` only for logging clarity - the two share the same
/// result type and poll system.
#[derive(Component)]
pub struct ResetRoomTask {
    pub task: bevy::tasks::Task<Result<(), String>>,
    pub did: String,
    pub spawned_at: f64,
    /// See [`PublishRoomTask::record_bytes`].
    pub record_bytes: Option<usize>,
    /// See [`PublishRoomTask::published`].
    pub published: RoomRecord,
}

/// Spawn the async room-record publish. `pub(crate)` because the
/// unsaved-edits guard ([`crate::ui::unsaved_guard`]) drives the same
/// pipeline for its "Publish & continue" path - the shared
/// [`poll_publish_tasks`] system lands the result either way.
pub(crate) fn spawn_room_publish_task(
    commands: &mut Commands,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: RoomRecord,
    did: String,
    now: f64,
) {
    let session_clone = session.clone();
    let refresh_clone = refresh.clone();
    // Split wire format (#697): the budget gauge tracks the largest single
    // record the publish writes (manifest or biggest child), not the
    // in-memory monolith.
    let record_bytes = pds::room::max_publish_record_bytes(&record);
    let published = record.clone();
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            pds::publish_room_record(&client, &session_clone, &refresh_clone, &record).await
        };
        crate::config::http::run_or(
            fut,
            Err(crate::config::http::timed_out("Saving your world")),
        )
        .await
    });
    commands.spawn(PublishRoomTask {
        task,
        did,
        spawned_at: now,
        record_bytes,
        published,
    });
}

/// Spawn the hard-reset publish task - wipe the stored manifest + child
/// records first, then republish fresh (all via `applyWrites`). Used by the
/// recovery banner's "Reset PDS to default" button, which must work even
/// when the stored record is schema-incompatible and cannot be decoded.
pub(super) fn spawn_reset_task(
    commands: &mut Commands,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: RoomRecord,
    did: String,
    now: f64,
) {
    let session_clone = session.clone();
    let refresh_clone = refresh.clone();
    // Split wire format (#697): the budget gauge tracks the largest single
    // record the publish writes (manifest or biggest child), not the
    // in-memory monolith.
    let record_bytes = pds::room::max_publish_record_bytes(&record);
    let published = record.clone();
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            pds::reset_room_record(&client, &session_clone, &refresh_clone, &record).await
        };
        crate::config::http::run_or(
            fut,
            Err(crate::config::http::timed_out("Resetting your world")),
        )
        .await
    });
    commands.spawn(ResetRoomTask {
        task,
        did,
        spawned_at: now,
        record_bytes,
        published,
    });
}

/// Poll outstanding publish and reset tasks and log results. On success,
/// pin `StoredRoomRecord` to the record the task actually published so
/// subsequent "Load from PDS" presses restore the now-committed state and
/// the dirty indicator resets.
///
/// Deliberately does not read `LiveRoomRecord` (#1116): `stored` is a claim
/// about what the PDS holds, and the live resource is a claim about what
/// the user has since typed. Pinning one to the other at landing time made
/// every edit dispatched-but-not-yet-landed read as saved.
#[allow(clippy::too_many_arguments)]
pub fn poll_publish_tasks(
    mut commands: Commands,
    mut publish_tasks: Query<(Entity, &mut PublishRoomTask)>,
    mut reset_tasks: Query<(Entity, &mut ResetRoomTask)>,
    mut stored: Option<ResMut<StoredRoomRecord>>,
    mut publish_feedback: ResMut<PublishFeedback<RoomRecord>>,
    mut session_log: ResMut<SessionLog>,
    mut metrics: ResMut<MetricsRegistry>,
    time: Res<Time>,
    // A failed write is reported OUTSIDE this window (#1137): the toast and
    // the auto-open are what make it visible when the editor is closed.
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    mut toasts: ResMut<crate::notify::Toasts>,
    // The room this session is in now (#1204): a result for another room
    // - a save let run in the background across a portal hop, or a task
    // that outlived its session - must not pin `stored`.
    current_room: Option<Res<crate::state::CurrentRoomDid>>,
    // The save notice (#1499), sent while the world's live updates are
    // refused - which this latch records.
    mut network: bevy_symbios_multiuser::prelude::SendMessage<crate::protocol::OverlandsMessage>,
    live_sync: Res<crate::network::chunk::OversizeNotices>,
) {
    for (entity, mut task) in publish_tasks.iter_mut() {
        let spawned_at = task.spawned_at;
        let Some(result) = poll_or_expire(
            &mut task.task,
            spawned_at,
            time.elapsed_secs_f64(),
            "Saving your world",
        ) else {
            continue;
        };

        commands.entity(entity).despawn();
        if stale_result(
            "Saving your world",
            &task.did,
            current_room.as_deref().map(|r| r.0.as_str()),
        ) {
            continue;
        }
        let now = time.elapsed_secs_f64();
        let did = task.did.clone();
        let duration_secs = now - task.spawned_at;
        crate::ui::editable::log_record_size(
            &mut session_log,
            &mut metrics,
            now,
            RecordKind::Room,
            task.record_bytes,
        );
        match result {
            Ok(()) => {
                info!("Room record saved to PDS");
                if let Some(stored) = stored.as_mut() {
                    stored.0 = task.published.clone();
                }
                // The PDS now holds what `stored` says it holds, so the
                // recovery marker - "the stored copy was never read" - is
                // retired HERE, where success is known (#1199).
                commands.remove_resource::<crate::state::RoomRecordRecovery>();
                announce_save(&mut network, &live_sync);
                publish_feedback.status = PublishStatus::Success { at_secs: now };
                crate::ui::editable::report_publish_success(
                    RecordKind::Room,
                    &panels,
                    &mut toasts,
                    now,
                );
                session_log.info(
                    now,
                    EventPayload::RecordWriteCompleted {
                        record: RecordKind::Room,
                        did,
                        duration_secs,
                    },
                );
            }
            Err(e) => crate::ui::editable::report_publish_failure(
                RecordKind::Room,
                crate::ui::editable::WriteOp::Save,
                did,
                e,
                now,
                crate::ui::editable::FailureSinks {
                    session_log: &mut session_log,
                    feedback: &mut publish_feedback,
                    toasts: &mut toasts,
                    panels: &mut panels,
                },
            ),
        }
    }
    for (entity, mut task) in reset_tasks.iter_mut() {
        let spawned_at = task.spawned_at;
        let Some(result) = poll_or_expire(
            &mut task.task,
            spawned_at,
            time.elapsed_secs_f64(),
            "Resetting your world",
        ) else {
            continue;
        };

        commands.entity(entity).despawn();
        if stale_result(
            "Resetting your world",
            &task.did,
            current_room.as_deref().map(|r| r.0.as_str()),
        ) {
            continue;
        }
        let now = time.elapsed_secs_f64();
        let did = task.did.clone();
        let duration_secs = now - task.spawned_at;
        crate::ui::editable::log_record_size(
            &mut session_log,
            &mut metrics,
            now,
            RecordKind::Room,
            task.record_bytes,
        );
        match result {
            Ok(()) => {
                info!("Room record reset on PDS (delete + put)");
                if let Some(stored) = stored.as_mut() {
                    stored.0 = task.published.clone();
                }
                // See the publish arm: retired on the landed write, not on
                // the click that asked for it (#1199). A failed reset keeps
                // its banner and its button.
                commands.remove_resource::<crate::state::RoomRecordRecovery>();
                announce_save(&mut network, &live_sync);
                publish_feedback.status = PublishStatus::Success { at_secs: now };
                crate::ui::editable::report_publish_success(
                    RecordKind::Room,
                    &panels,
                    &mut toasts,
                    now,
                );
                session_log.info(
                    now,
                    EventPayload::RecordWriteCompleted {
                        record: RecordKind::Room,
                        did,
                        duration_secs,
                    },
                );
            }
            Err(e) => crate::ui::editable::report_publish_failure(
                RecordKind::Room,
                crate::ui::editable::WriteOp::Reset,
                did,
                e,
                now,
                crate::ui::editable::FailureSinks {
                    session_log: &mut session_log,
                    feedback: &mut publish_feedback,
                    toasts: &mut toasts,
                    panels: &mut panels,
                },
            ),
        }
    }
}

/// Tell the room its world was saved (#1499) - while the world's live
/// updates are refused, and only then.
///
/// Past the live ceiling a guest's copy is the one it arrived to, or the last
/// save it fetched, since the owner's edits are refused at the wire
/// (`network::chunk`); the notice is how it learns to fetch the save. While
/// the live updates go out, a guest holds the owner's live state, which can
/// be NEWER than the save landing now - an edit made while the save was in
/// flight - so nothing is sent: fetching the save would roll that edit back.
fn announce_save(
    network: &mut bevy_symbios_multiuser::prelude::SendMessage<crate::protocol::OverlandsMessage>,
    live_sync: &crate::network::chunk::OversizeNotices,
) {
    if live_sync.paused(crate::network::chunk::LIVE_WORLD) {
        network.broadcast(
            crate::protocol::OverlandsMessage::RoomRecordsPublished,
            bevy_symbios_multiuser::prelude::ChannelKind::Reliable,
        );
    }
}

/// Whether a landed write belongs to a session or room this client is no
/// longer in (#1204). `expected` is the DID the poll answers for now -
/// the current room for a room write, the signed-in session for an avatar
/// or inventory write; `None` (no such resource, i.e. not in a session)
/// lets the result through so the harnesses that drive the polls without
/// a session keep working, and is unreachable in the app because the
/// polls run only `InGame`.
///
/// A stale result is dropped whole: `stored` is a claim about what the PDS
/// holds for THIS identity and room, and the status line describes THIS
/// editor. Logout sweeps the task entities; this covers a task that
/// outlived a portal hop under "Stay here (save continues)", and the wasm case
/// where a dropped task's fetch keeps running.
pub(crate) fn stale_result(label: &str, task_did: &str, expected: Option<&str>) -> bool {
    match expected {
        Some(expected) if expected != task_did => {
            warn!(
                "{label} for {task_did} landed after this client moved on to {expected}; \
                 ignoring the result"
            );
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::chunk::{LIVE_WORLD, OversizeNotices, SendOutcome, warn_once_on_refusal};
    use crate::protocol::OverlandsMessage;
    use bevy_symbios_multiuser::prelude::{Broadcast, SendTo};

    #[test]
    fn a_result_is_stale_only_when_the_expected_did_differs() {
        assert!(!stale_result("t", "did:plc:a", None));
        assert!(!stale_result("t", "did:plc:a", Some("did:plc:a")));
        assert!(stale_result("t", "did:plc:a", Some("did:plc:b")));
    }

    /// Whether a save landing tells the room, with the world's live updates
    /// refused (`paused`) or going out.
    fn a_save_lands(paused: bool) -> bool {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<SessionLog>()
            .init_resource::<MetricsRegistry>()
            .init_resource::<crate::ui::toolbar::UiPanels>()
            .init_resource::<crate::notify::Toasts>()
            .init_resource::<PublishFeedback<RoomRecord>>()
            .add_message::<Broadcast<OverlandsMessage>>()
            .add_message::<SendTo<OverlandsMessage>>()
            .add_systems(Update, poll_publish_tasks);
        let mut live_sync = OversizeNotices::default();
        if paused {
            warn_once_on_refusal(
                SendOutcome::Refused { bytes: 1_000_000 },
                &mut live_sync,
                &mut crate::notify::Toasts::default(),
                LIVE_WORLD,
                0.0,
            );
        }
        let record = RoomRecord::default_for_did("did:plc:savedworld");
        app.insert_resource(live_sync)
            .insert_resource(crate::state::LiveRoomRecord(record.clone()))
            .insert_resource(StoredRoomRecord(record.clone()));
        app.world_mut().spawn(PublishRoomTask {
            task: bevy::tasks::IoTaskPool::get().spawn(async { Ok(()) }),
            did: String::from("did:plc:savedworld"),
            spawned_at: 0.0,
            record_bytes: Some(1),
            published: record,
        });
        let mut told = false;
        for _ in 0..2_000 {
            app.update();
            told |= app
                .world_mut()
                .resource_mut::<Messages<Broadcast<OverlandsMessage>>>()
                .drain()
                .any(|sent| matches!(sent.payload, OverlandsMessage::RoomRecordsPublished));
            let mut tasks = app.world_mut().query::<&PublishRoomTask>();
            if tasks.iter(app.world()).next().is_none() {
                return told;
            }
        }
        panic!("the save never landed");
    }

    /// #1499, as its review reshaped it: a save landing while the world's
    /// live updates are refused tells the room - a guest's copy is behind the
    /// save - and one landing while they go out tells nobody: a guest then
    /// holds the owner's live state, which can be newer than the save (an
    /// edit made while it was in flight), and fetching the save would roll
    /// that edit back.
    #[test]
    fn a_save_tells_the_room_only_while_its_live_updates_are_refused() {
        assert!(a_save_lands(true), "past the ceiling the save is news");
        assert!(
            !a_save_lands(false),
            "while the live updates go out, the guests already hold it or newer"
        );
    }
}
