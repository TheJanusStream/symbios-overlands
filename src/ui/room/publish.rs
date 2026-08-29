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
/// from `PublishRoomTask` only for logging clarity — the two share the same
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
/// pipeline for its "Publish & continue" path — the shared
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
        crate::config::http::run_or(fut, Err(crate::config::http::timed_out("room publish"))).await
    });
    commands.spawn(PublishRoomTask {
        task,
        did,
        spawned_at: now,
        record_bytes,
        published,
    });
}

/// Spawn the hard-reset publish task — wipe the stored manifest + child
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
        crate::config::http::run_or(fut, Err(crate::config::http::timed_out("room reset"))).await
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
    mut toasts: ResMut<crate::ui::toast::Toasts>,
) {
    for (entity, mut task) in publish_tasks.iter_mut() {
        let spawned_at = task.spawned_at;
        let Some(result) = poll_or_expire(
            &mut task.task,
            spawned_at,
            time.elapsed_secs_f64(),
            "room publish",
        ) else {
            continue;
        };

        commands.entity(entity).despawn();
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
                publish_feedback.status = PublishStatus::Success { at_secs: now };
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
            "room reset",
        ) else {
            continue;
        };

        commands.entity(entity).despawn();
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
                publish_feedback.status = PublishStatus::Success { at_secs: now };
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
