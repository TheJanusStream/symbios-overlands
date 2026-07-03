//! Generic record-fetch state machine shared by the room / avatar /
//! inventory loaders.
//!
//! Each PDS record type used to carry its own copy of the same pipeline
//! (task component, exponential-backoff retry marker, poll system).
//! [`LoadedRecord`] now captures the per-record policy — how to fetch,
//! what default to synthesise, how many retries a transient failure is
//! worth, and what to do when recovery is impossible — while the
//! machinery below ([`RecordFetchTask`], [`PendingRecordRetry`],
//! [`poll_record_task`], [`fire_pending_record_retries`]) is written
//! once and instantiated per record type in `crate::run`.
//!
//! The task result preserves the distinction between *no record* (404)
//! and *couldn't reach the PDS*: only the former falls through to the
//! DID-seeded default immediately. Substituting the default on a
//! transient network failure would be catastrophic for room and avatar —
//! the owner would silently be staged on the blank default, and a
//! "Save to PDS" click would overwrite their real record.

use std::marker::PhantomData;

use bevy::prelude::*;
use bevy::tasks::Task;

use crate::config;
use crate::diagnostics::SessionLog;
use crate::diagnostics::event::{EventPayload, FetchStatus, RecordKind, Severity};
use crate::pds::FetchError;

/// Per-record policy for the shared fetch pipeline. Implemented in
/// [`super::records`] for `RoomRecord`, `AvatarRecord` and
/// `InventoryRecord`.
///
/// `pub` (not `pub(crate)`) because [`PendingRecordRetry`] appears in
/// the public signature of `ui::loading::loading_ui` and carries this
/// trait as its generic bound.
pub trait LoadedRecord: Sized + Send + Sync + 'static {
    /// Capitalised human label for logs and diagnostics ("Room", …).
    const LABEL: &'static str;

    /// Which [`RecordKind`] this fetch is for, so the shared poll system can
    /// emit typed [`RecordFetchCompleted`](EventPayload::RecordFetchCompleted)
    /// / [`RecordFetchRetrying`](EventPayload::RecordFetchRetrying) events into
    /// the session log without a stringly-typed label.
    const RECORD_KIND: RecordKind;

    /// Transient-failure retry budget. The backoff saturates at 60 s
    /// after six attempts, so the room/avatar budget of 12 buys roughly
    /// ten minutes of retrying against a flaky PDS. `0` means
    /// best-effort: the first transient failure already falls through
    /// to [`Self::default_for`] (inventory).
    const MAX_ATTEMPTS: u32;

    /// Kick off the async PDS fetch for `did` on the `IoTaskPool`.
    /// Implementations route through [`dispatch`] so the
    /// tokio-vs-browser runtime split lives in one place.
    fn dispatch_fetch(did: String) -> Task<Result<Option<Self>, FetchError>>;

    /// DID-seeded default used for 404, decode failure and exhausted
    /// retries.
    fn default_for(did: &str) -> Self;

    /// Sanitize and install the record's live + stored resources. The
    /// two start identical — any later divergence is authored by the
    /// owner (and diffed by the editors / the unsaved-edits guard).
    fn install(self, commands: &mut Commands);

    /// Hook fired when the stored record can never be fetched intact —
    /// decode failure (schema drift is permanent, retrying can't help)
    /// or an exhausted retry budget. The room impl raises the
    /// [`crate::state::RoomRecordRecovery`] banner here; the others
    /// don't need anything beyond the default fallback.
    fn on_unrecoverable(_commands: &mut Commands, _reason: String) {}
}

/// Exponential backoff for transient fetch failures. Without a delay, a
/// DNS error or immediate `ConnRefused` returns so fast that the retry
/// runs in the same or next frame, producing a busy loop that burns a
/// full CPU core and floods the log with warnings. Doubling from 2 s up
/// to a 60 s ceiling (2, 4, 8, 16, 32, 60) yields ~two minutes of
/// retries over six attempts while still converging quickly when the
/// PDS recovers.
pub(crate) fn record_backoff_secs(attempt: u32) -> u64 {
    if attempt == 0 {
        0
    } else {
        (1u64 << attempt.min(6)).min(60)
    }
}

/// The [`FetchStatus`] recorded when a fetch falls through to the DID-seeded
/// default after its retry budget is spent. A `max_attempts` of `0` means the
/// record is best-effort (inventory), so the first transient failure is an
/// expected [`FetchStatus::BestEffortFallback`] rather than an
/// [`FetchStatus::Exhausted`] failure of a gameplay-critical fetch.
pub(crate) fn terminal_fallback_status(max_attempts: u32) -> FetchStatus {
    if max_attempts == 0 {
        FetchStatus::BestEffortFallback
    } else {
        FetchStatus::Exhausted
    }
}

/// Spawn the shared IoTaskPool wrapper around a record-fetch future.
///
/// `IoTaskPool` is the correct home for blocking HTTP calls — the
/// `AsyncComputeTaskPool` is sized to the CPU-core count and must not be
/// starved by threads blocked on network sockets.
///
/// `reqwest` spawns internal timer/IO futures the moment it issues a
/// request, which panics with "there is no reactor running" unless the
/// future is driven inside a tokio runtime. The `IoTaskPool` is a plain
/// async-executor, so on native the future is driven by the
/// process-shared runtime via `config::http::block_on` (same pattern as
/// every other HTTP-spawning site in the crate). wasm32 has no tokio;
/// the browser's JS runtime backs `fetch`, so the bare future works —
/// and reqwest's wasm futures are `!Send`, which is why the two variants
/// carry different bounds.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn dispatch<R, F, Fut>(make: F) -> Task<Result<Option<R>, FetchError>>
where
    R: Send + 'static,
    F: FnOnce(reqwest::Client) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Option<R>, FetchError>> + Send + 'static,
{
    let pool = bevy::tasks::IoTaskPool::get();
    pool.spawn(async move {
        let fut = async move {
            let client = config::http::default_client();
            make(client).await
        };
        crate::config::http::block_on(fut)
    })
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn dispatch<R, F, Fut>(make: F) -> Task<Result<Option<R>, FetchError>>
where
    R: 'static,
    F: FnOnce(reqwest::Client) -> Fut + 'static,
    Fut: Future<Output = Result<Option<R>, FetchError>> + 'static,
{
    let pool = bevy::tasks::IoTaskPool::get();
    pool.spawn(async move {
        let client = config::http::default_client();
        make(client).await
    })
}

/// In-flight fetch for record type `R`, attached to a throwaway entity
/// so the `Loading` poll system can drain it without a dedicated
/// resource.
#[derive(Component)]
pub(crate) struct RecordFetchTask<R: LoadedRecord> {
    did: String,
    task: Task<Result<Option<R>, FetchError>>,
    /// Zero for the initial fetch; incremented on each transient-failure
    /// respawn so the retry marker can pick a backoff delay.
    attempt: u32,
    /// Session-relative seconds when this attempt was dispatched, so the poller
    /// can record its spawn→resolve latency (E-4).
    spawned_at: f64,
}

/// In-flight retry timer for a record fetch. The backoff sleep is
/// deferred on Bevy's frame loop instead of inside the spawned task:
/// `tokio::time::sleep` awaited inside `block_on(fut)` would hold the
/// underlying OS thread idle for the duration, and several flaky fetches
/// in retry simultaneously would saturate the small `IoTaskPool` and
/// stall every other I/O job in the engine. The sleeping period occupies
/// a tiny ECS entity rather than a precious worker thread; see
/// [`fire_pending_record_retries`].
#[derive(Component)]
pub struct PendingRecordRetry<R: LoadedRecord> {
    did: String,
    attempt: u32,
    fire_at_secs: f64,
    _marker: PhantomData<R>,
}

impl<R: LoadedRecord> PendingRecordRetry<R> {
    /// Which retry this marker will fire (1-based), for progress UI.
    pub(crate) fn attempt(&self) -> u32 {
        self.attempt
    }

    /// Absolute `Time::elapsed_secs_f64` deadline, for progress UI.
    pub(crate) fn fire_at_secs(&self) -> f64 {
        self.fire_at_secs
    }
}

/// Kick off (or retry) the fetch for record type `R`.
pub(crate) fn spawn_record_fetch<R: LoadedRecord>(
    commands: &mut Commands,
    did: String,
    attempt: u32,
    spawned_at: f64,
) {
    let task = R::dispatch_fetch(did.clone());
    commands.spawn(RecordFetchTask::<R> {
        did,
        task,
        attempt,
        spawned_at,
    });
}

/// Drain a finished [`RecordFetchTask<R>`] and install the result.
///
/// - 404 is not an error: it means the owner has never published this
///   record, so the DID-seeded default is installed directly.
/// - A decode failure is *not* transient: the stored record exists but
///   is incompatible with the current schema (lexicon drift,
///   partially-migrated field). Retrying will never recover — the
///   loading screen would hang forever — so the default is installed and
///   [`LoadedRecord::on_unrecoverable`] gets to surface the situation.
/// - Any other failure (DNS timeout, 5xx, DID-resolution hiccup) is
///   retried with exponential backoff up to [`LoadedRecord::MAX_ATTEMPTS`],
///   after which it is treated as unrecoverable.
pub(crate) fn poll_record_task<R: LoadedRecord>(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut RecordFetchTask<R>)>,
    mut session_log: ResMut<SessionLog>,
    time: Res<Time>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        let prev_attempt = task.attempt;
        let did = task.did.clone();
        let spawned_at = task.spawned_at;
        commands.entity(entity).despawn();

        // Set on the successful arms so the terminal-resolve tail emits a typed
        // `RecordFetchCompleted`; the failure arms below emit their own.
        let mut fetch_status: Option<FetchStatus> = None;
        let record = match result {
            Ok(Some(r)) => {
                fetch_status = Some(FetchStatus::Ok);
                r
            }
            Ok(None) => {
                fetch_status = Some(FetchStatus::NotFound);
                info!("No {} record on PDS — using DID-seeded default", R::LABEL);
                R::default_for(&did)
            }
            Err(FetchError::Decode(msg)) => {
                let elapsed = time.elapsed_secs_f64();
                session_log.warn(
                    elapsed,
                    EventPayload::RecordFetchCompleted {
                        record: R::RECORD_KIND,
                        did: did.clone(),
                        status: FetchStatus::DecodeError,
                        duration_secs: elapsed - spawned_at,
                    },
                );
                warn!(
                    "Stored {} record could not be decoded ({}) — using DID-seeded default",
                    R::LABEL,
                    msg
                );
                R::on_unrecoverable(&mut commands, msg);
                R::default_for(&did)
            }
            Err(err) => {
                let next_attempt = prev_attempt.saturating_add(1);
                let elapsed = time.elapsed_secs_f64();
                if next_attempt > R::MAX_ATTEMPTS {
                    let status = terminal_fallback_status(R::MAX_ATTEMPTS);
                    // Best-effort records (inventory) fall through by design →
                    // Info; a gameplay-critical fetch giving up entirely → Error.
                    let severity = match status {
                        FetchStatus::BestEffortFallback => Severity::Info,
                        _ => Severity::Error,
                    };
                    session_log.record(
                        elapsed,
                        severity,
                        EventPayload::RecordFetchCompleted {
                            record: R::RECORD_KIND,
                            did: did.clone(),
                            status,
                            duration_secs: elapsed - spawned_at,
                        },
                    );
                    warn!(
                        "{} record fetch exhausted {} attempts: {:?} — falling back to default",
                        R::LABEL,
                        R::MAX_ATTEMPTS,
                        err
                    );
                    R::on_unrecoverable(&mut commands, format!("PDS unreachable: {err:?}"));
                    R::default_for(&did)
                } else {
                    let backoff = record_backoff_secs(next_attempt);
                    session_log.warn(
                        elapsed,
                        EventPayload::RecordFetchRetrying {
                            record: R::RECORD_KIND,
                            did: did.clone(),
                            attempt: next_attempt,
                            backoff_secs: backoff,
                            reason: format!("{err:?}"),
                        },
                    );
                    warn!(
                        "{} record fetch failed: {:?} — retrying in {}s (attempt {})",
                        R::LABEL,
                        err,
                        backoff,
                        next_attempt
                    );
                    commands.spawn(PendingRecordRetry::<R> {
                        did,
                        attempt: next_attempt,
                        fire_at_secs: elapsed + backoff as f64,
                        _marker: PhantomData,
                    });
                    crate::diagnostics::samplers::record_fetch_retry(&mut metrics, elapsed);
                    continue;
                }
            }
        };
        // A terminal outcome resolved the fetch (success / decode-fallback /
        // exhausted-default) — record the resolving attempt's latency (E-4). The
        // transient-retry path `continue`s above, so a retry cycle's intermediate
        // (often timeout-length) attempt latencies never pollute this histogram.
        let now = time.elapsed_secs_f64();
        // Typed completion for the *successful* resolutions (a record, or a
        // 404-default) — feeds the analyzer's record-fetch stage distro (B-2).
        // The decode / exhausted / best-effort arms already logged their own.
        if let Some(status) = fetch_status {
            session_log.info(
                now,
                EventPayload::RecordFetchCompleted {
                    record: R::RECORD_KIND,
                    did: did.clone(),
                    status,
                    duration_secs: now - spawned_at,
                },
            );
        }
        crate::diagnostics::samplers::record_fetch_latency_secs(&mut metrics, now - spawned_at);
        record.install(&mut commands);
    }
}

/// Fire any retry markers whose backoff has elapsed. Runs on Bevy's main
/// frame loop, so an idle 60 s exponential-backoff window costs only the
/// `Time` resource read per frame instead of a permanently-sleeping
/// worker thread. See [`PendingRecordRetry`] for the rationale.
pub(crate) fn fire_pending_record_retries<R: LoadedRecord>(
    mut commands: Commands,
    pending: Query<(Entity, &PendingRecordRetry<R>)>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    for (entity, marker) in pending.iter() {
        if now >= marker.fire_at_secs {
            let did = marker.did.clone();
            let attempt = marker.attempt;
            commands.entity(entity).despawn();
            spawn_record_fetch::<R>(&mut commands, did, attempt, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{record_backoff_secs, terminal_fallback_status};
    use crate::diagnostics::event::FetchStatus;

    #[test]
    fn backoff_doubles_and_saturates() {
        assert_eq!(record_backoff_secs(0), 0);
        assert_eq!(record_backoff_secs(1), 2);
        assert_eq!(record_backoff_secs(2), 4);
        assert_eq!(record_backoff_secs(5), 32);
        // 2^6 = 64 caps at the 60 s ceiling, and the shift itself
        // saturates at 6 so huge attempt numbers can't overflow.
        assert_eq!(record_backoff_secs(6), 60);
        assert_eq!(record_backoff_secs(7), 60);
        assert_eq!(record_backoff_secs(u32::MAX), 60);
    }

    #[test]
    fn best_effort_records_fall_through_without_an_exhausted_failure() {
        // A best-effort record (MAX_ATTEMPTS == 0, i.e. inventory) fell
        // through by design — not a gameplay-critical fetch giving up.
        assert_eq!(terminal_fallback_status(0), FetchStatus::BestEffortFallback);
        // Room / avatar spent a real retry budget before giving up.
        assert_eq!(terminal_fallback_status(12), FetchStatus::Exhausted);
    }
}
