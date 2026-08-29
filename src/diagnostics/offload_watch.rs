//! Live watchdog over the offload census (#1143).
//!
//! Turns the process-global ledger [`crate::offload::census`] keeps into
//! session events, and holds the one scalar the `offload.task_never_resolves`
//! rule needs to fire live rather than only in replay.
//!
//! The failure this exists for is a wasm deploy that ships the app bundle
//! beside a stale or missing `gen-worker.js`. The worker future never
//! resolves, the loading gate never opens or every body stays a bare chassis,
//! and — before this — the log said HEALTHY with at most a generic
//! `loading.gate_stall` to show for it. `OffloadJobStarted`,
//! `OffloadJobCompleted` and `OffloadTaskTimeout` had no emit site anywhere in
//! the crate, so the replay rule written for exactly this had nothing to fold
//! and the live rule had no body at all.

use bevy::prelude::*;
use std::collections::HashSet;

use crate::diagnostics::SessionLog;
use crate::diagnostics::event::{EventPayload, Severity};
use crate::offload::census::{self, Transition};

/// How long an offloaded job may be in flight before the watchdog calls it
/// stuck.
///
/// The same number the replay rule uses, and deliberately generous: a full
/// atlas avatar build is ~277 ms and the heaviest texture bake is seconds, so
/// a minute of silence is not slowness, it is a job that is never coming back.
pub const TASK_TIMEOUT_SECS: f64 = 60.0;

/// What the offload census looked like at the last sample.
///
/// Read by the anomaly tick into `LiveCtx` so the rule stays pure over its
/// inputs — the rules must be unit-testable without a process-global.
#[derive(Resource, Default)]
pub struct OffloadWatch {
    /// The longest-waiting in-flight job and its age in seconds.
    pub oldest_pending: Option<(&'static str, f64)>,
}

/// The pending jobs that have just crossed [`TASK_TIMEOUT_SECS`] and have not
/// been reported yet, marking them as reported.
///
/// Pure over its inputs so the latch is testable without waiting a real
/// minute. The latch matters in both directions: a stuck job stays in the
/// census until it answers or is cancelled, so without it every frame would
/// emit another `OffloadTaskTimeout` for the same job — and clearing the entry
/// when the job retires is what lets a later job reusing nothing but the same
/// *kind* report on its own merits.
fn newly_timed_out<'a>(
    pending: &[(u64, &'a str, f64)],
    reported: &mut HashSet<u64>,
) -> Vec<(&'a str, f64)> {
    pending
        .iter()
        .filter(|(id, _, age)| *age > TASK_TIMEOUT_SECS && reported.insert(*id))
        .map(|(_, job, age)| (*job, *age))
        .collect()
}

/// Drain the census into the session log and refresh [`OffloadWatch`].
///
/// Runs every frame rather than on the 1 Hz scrape: a `Started`/`Finished`
/// pair carries the job's real duration from the census's own monotonic
/// clock, but the *event* is stamped when it is observed, and a job that
/// finishes in 80 ms should not be reported a second late in the timeline.
/// The cost when nothing has happened is one uncontended lock and an empty
/// `Vec`.
pub fn sample_offload_census(
    mut log: ResMut<SessionLog>,
    mut watch: ResMut<OffloadWatch>,
    time: Res<Time>,
    // Jobs already reported stuck. A stuck job stays in the census until it
    // answers or is cancelled, so without the latch this would emit one
    // `OffloadTaskTimeout` per frame for the rest of the session.
    mut reported: Local<HashSet<u64>>,
) {
    let now = time.elapsed_secs_f64();

    for transition in census::drain_transitions() {
        match transition {
            Transition::Started { job, .. } => {
                log.info(now, EventPayload::OffloadJobStarted { job: job.into() });
            }
            Transition::Finished {
                id,
                job,
                elapsed_secs,
            } => {
                reported.remove(&id);
                log.info(
                    now,
                    EventPayload::OffloadJobCompleted {
                        job: job.into(),
                        duration_secs: elapsed_secs,
                    },
                );
            }
        }
    }

    let pending = census::pending_snapshot();
    for (job, age) in newly_timed_out(&pending, &mut reported) {
        log.record(
            now,
            Severity::Error,
            EventPayload::OffloadTaskTimeout {
                job: job.into(),
                elapsed_secs: age,
            },
        );
    }

    // Every in-flight job, not just the first over the line: a broken worker
    // fails all of them, and the guarded write keeps an idle frame from
    // flagging the resource changed.
    let oldest = pending
        .into_iter()
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(_, job, age)| (job, age));
    if watch.oldest_pending != oldest {
        watch.oldest_pending = oldest;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    /// The census is process-global (it has to be — `offload` has no `World`),
    /// so a test starts by taking whatever an earlier test left behind.
    fn drained_census() {
        let _ = census::drain_transitions();
    }

    /// #1143. Sequence: a wasm deploy ships the app bundle beside a stale
    /// `gen-worker.js`; `run_on_worker` never resolves, and the job sits in
    /// flight for the rest of the session. Before the census nothing observed
    /// that at all — `OffloadJobStarted` had no emit site in the crate, so the
    /// rule written for it could not fire live OR in replay.
    #[test]
    fn a_dispatched_job_is_announced_and_a_finished_one_is_closed_out() {
        drained_census();
        let ticket = census::start("avatar_build");

        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin);
        app.insert_resource(SessionLog::with_capacity(64));
        app.init_resource::<OffloadWatch>();

        app.world_mut()
            .run_system_once(sample_offload_census)
            .expect("sampler");
        assert!(
            app.world()
                .resource::<SessionLog>()
                .iter()
                .any(|e| matches!(&e.payload, EventPayload::OffloadJobStarted { job } if job == "avatar_build")),
            "the dispatch must reach the log"
        );
        assert_eq!(
            app.world()
                .resource::<OffloadWatch>()
                .oldest_pending
                .map(|(j, _)| j),
            Some("avatar_build"),
            "and the watchdog must see it in flight"
        );

        drop(ticket);
        app.world_mut()
            .run_system_once(sample_offload_census)
            .expect("sampler");
        assert!(
            app.world()
                .resource::<SessionLog>()
                .iter()
                .any(|e| matches!(&e.payload, EventPayload::OffloadJobCompleted { job, .. } if job == "avatar_build")),
            "and its completion must close the span the replay rule folds"
        );
        assert_eq!(
            app.world().resource::<OffloadWatch>().oldest_pending,
            None,
            "nothing is in flight once the ticket retires"
        );
    }

    /// A job the caller cancelled — the `Task` dropped mid-flight — must leave
    /// the census, or the watchdog would report a stall nobody is waiting on.
    #[test]
    fn a_cancelled_job_does_not_look_stuck() {
        drained_census();
        let ticket = census::start("texture_bake");
        assert_eq!(census::pending_snapshot().len(), 1);
        drop(ticket);
        assert!(
            census::pending_snapshot().is_empty(),
            "the ticket retires the entry however the future ended"
        );
    }

    /// The timeout is reported once per job, not once per frame — a stuck job
    /// stays in the census for the rest of the session.
    #[test]
    fn a_stuck_job_is_reported_once_and_a_later_one_on_its_own_merits() {
        let mut reported = HashSet::new();
        let stuck = [(7u64, "heightmap", TASK_TIMEOUT_SECS + 1.0)];

        assert_eq!(
            newly_timed_out(&stuck, &mut reported),
            vec![("heightmap", TASK_TIMEOUT_SECS + 1.0)]
        );
        assert!(
            newly_timed_out(&stuck, &mut reported).is_empty(),
            "the same job must not re-report every frame"
        );

        // A job still inside its budget says nothing at all.
        let young = [(8u64, "audio_bake", TASK_TIMEOUT_SECS - 1.0)];
        assert!(newly_timed_out(&young, &mut reported).is_empty());

        // A different job of the same kind is its own story.
        let other = [(9u64, "heightmap", TASK_TIMEOUT_SECS + 5.0)];
        assert_eq!(
            newly_timed_out(&other, &mut reported),
            vec![("heightmap", TASK_TIMEOUT_SECS + 5.0)]
        );
    }
}
