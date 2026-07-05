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
use crate::state::{LiveRoomRecord, PublishFeedback, PublishStatus, StoredRoomRecord};

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
}

/// Async task for the hard-reset publish path (delete-then-put). Separate
/// from `PublishRoomTask` only for logging clarity — the two share the same
/// result type and poll system.
#[derive(Component)]
pub struct ResetRoomTask {
    pub task: bevy::tasks::Task<Result<(), String>>,
    pub did: String,
    pub spawned_at: f64,
    /// See [`PublishRoomTask::record_bytes`].
    pub record_bytes: Option<usize>,
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
    let record_bytes = pds::record_size::serialized_record_bytes(&record);
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            pds::publish_room_record(&client, &session_clone, &refresh_clone, &record).await
        };
        #[cfg(target_arch = "wasm32")]
        {
            fut.await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::config::http::block_on(fut)
        }
    });
    commands.spawn(PublishRoomTask {
        task,
        did,
        spawned_at: now,
        record_bytes,
    });
}

/// Spawn the hard-reset publish task — delete the stored record first, then
/// create a fresh one. Used by the recovery banner's "Reset PDS to default"
/// button, which has to work around PDS implementations that return 500 on
/// `putRecord` when the prior blob is schema-incompatible.
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
    let record_bytes = pds::record_size::serialized_record_bytes(&record);
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            pds::reset_room_record(&client, &session_clone, &refresh_clone, &record).await
        };
        #[cfg(target_arch = "wasm32")]
        {
            fut.await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::config::http::block_on(fut)
        }
    });
    commands.spawn(ResetRoomTask {
        task,
        did,
        spawned_at: now,
        record_bytes,
    });
}

/// Poll outstanding publish and reset tasks and log results. On success,
/// pin `StoredRoomRecord` to the live `RoomRecord` so subsequent "Load from
/// PDS" presses restore the now-committed state and the dirty indicator
/// resets.
#[allow(clippy::too_many_arguments)]
pub fn poll_publish_tasks(
    mut commands: Commands,
    mut publish_tasks: Query<(Entity, &mut PublishRoomTask)>,
    mut reset_tasks: Query<(Entity, &mut ResetRoomTask)>,
    live: Option<Res<LiveRoomRecord>>,
    mut stored: Option<ResMut<StoredRoomRecord>>,
    mut publish_feedback: ResMut<PublishFeedback<RoomRecord>>,
    mut session_log: ResMut<SessionLog>,
    mut metrics: ResMut<MetricsRegistry>,
    time: Res<Time>,
) {
    for (entity, mut task) in publish_tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
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
                if let (Some(live), Some(stored)) = (live.as_ref(), stored.as_mut()) {
                    stored.0 = live.0.clone();
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
            Err(e) => {
                warn!("Failed to save room record: {}", e);
                session_log.error(
                    now,
                    EventPayload::RecordWriteFailed {
                        record: RecordKind::Room,
                        did,
                        reason: e.clone(),
                    },
                );
                publish_feedback.status = PublishStatus::Failed {
                    at_secs: now,
                    message: e,
                };
            }
        }
    }
    for (entity, mut task) in reset_tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
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
                if let (Some(live), Some(stored)) = (live.as_ref(), stored.as_mut()) {
                    stored.0 = live.0.clone();
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
            Err(e) => {
                warn!("Failed to reset room record: {}", e);
                session_log.error(
                    now,
                    EventPayload::RecordWriteFailed {
                        record: RecordKind::Room,
                        did,
                        reason: e.clone(),
                    },
                );
                publish_feedback.status = PublishStatus::Failed {
                    at_secs: now,
                    message: e,
                };
            }
        }
    }
}
