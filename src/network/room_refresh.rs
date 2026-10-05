//! A guest fetches the world it stands in when the world's live updates
//! cannot reach it (#1499).
//!
//! # Why
//!
//! A room's live edits reach its guests as
//! [`crate::protocol::OverlandsMessage::RoomStateUpdate`], the whole record in
//! one message, and past [`config::network::MAX_RELIABLE_PAYLOAD_BYTES`] the
//! owner's client refuses to send it (#1123). Nothing else ever told a guest
//! the world had moved: a guest saw a save only after leaving and coming
//! back. Ashmere, an agent's manor at 1.3 MiB, had every live edit refused
//! for a whole session, and its visitor heard a saved sound change only
//! after re-entering.
//!
//! So the owner says so ([`crate::protocol::OverlandsMessage::RoomRecordsPublished`]):
//! to the room when a save lands while the world's live updates are refused
//! (`ui::room::poll_publish_tasks`), and to a newcomer whose join push is
//! refused (`lifecycle::handle_peer_connections`). A guest then reads the
//! saved world from the owner's PDS - the read that brought it the world on
//! arrival - and replaces its copy where the fetched one differs.
//!
//! # The rule
//!
//! * Only the room's owner is listened to, and never by the owner's own other
//!   session: two sessions of one owner settle their copies by the
//!   same-owner split (#1203), which a fetch would go round.
//! * Nothing is sent while the live updates go out. A guest then holds the
//!   owner's live state, which can be NEWER than a save landing - an edit made
//!   while the save was in flight - and fetching the save would roll it back
//!   (the review of this module found exactly that). So a notice means "your
//!   copy is behind the saved one".
//! * A live update applied after a notice is newer than the save behind it:
//!   the waiting notice is dropped, and a fetch already running lands
//!   nowhere. The reliable channel keeps one sender's messages in order, so
//!   "arrived after" is "sent after". A room swap - a portal hop, to another
//!   room or back to this one - overtakes a fetch the same way: the copy the
//!   swap brought is the newer.
//! * One fetch at a time, their starts at least
//!   [`config::network::ROOM_REFRESH_MIN_INTERVAL_SECS`] apart. A notice is a
//!   message the owner sends at will and each fetch is the whole world, so a
//!   notice inside the window waits for its end: delayed, never dropped, and
//!   the notices inside one window are one fetch.
//! * A fetch that fails in a way that may pass is tried again on a doubling
//!   wait, up to [`config::network::ROOM_REFRESH_MAX_ATTEMPTS`] times; a
//!   record that will not decode is an answer, and is not asked again. The
//!   world standing stays meanwhile and afterwards.
//!
//! What this does not do: a world past the ceiling still sends its live edits
//! to nobody, so a guest sees each SAVE, not each edit. And a world that
//! crosses the ceiling while a save is in flight can still put a guest back
//! to that save: the edits it took live before the crossing were newer.

use bevy::prelude::*;
use bevy::tasks::Task;

use crate::config;
use crate::diagnostics::SessionLog;
use crate::diagnostics::event::EventPayload;
use crate::pds::{FetchError, RoomRecord};
use crate::state::{CurrentRoomDid, LiveRoomRecord, StoredRoomRecord};

use super::presence::RetryBackoff;

/// The owner's notice a guest has yet to fetch for.
#[derive(Clone, Debug, PartialEq)]
struct PendingRefresh {
    /// The room the notice came from.
    room_did: String,
    /// When it may be fetched for: at once, or after a failed fetch's wait.
    not_before: f64,
    /// The failed fetches behind it, for the doubling wait.
    backoff: Option<RetryBackoff>,
}

/// One guest's state for the room it stands in (#1499). Reset with the
/// session ([`reset_room_refresh`]).
#[derive(Resource, Default, Debug)]
pub struct RoomRefresh {
    /// The notice waiting for its window, if any.
    pending: Option<PendingRefresh>,
    /// Bumped by every owner update applied and every room swap: a fetch
    /// started at a lower count has been overtaken by a newer copy.
    generation: u64,
    /// When the last fetch started, for the window.
    last_started_at: Option<f64>,
}

impl RoomRefresh {
    /// The room's owner says this guest's copy is behind the saved one. A
    /// notice already waiting stands for both; one waiting out a failed
    /// fetch's wait is asked afresh.
    pub(super) fn note_save(&mut self, room_did: &str, now: f64) {
        self.pending = Some(PendingRefresh {
            room_did: room_did.to_owned(),
            not_before: now,
            backoff: None,
        });
    }

    /// A newer copy than any saved one arrived - the owner's live update, or
    /// a room swap: a waiting notice is dropped, and a fetch already running
    /// lands nowhere.
    pub(super) fn overtake(&mut self) {
        self.generation += 1;
        self.pending = None;
    }

    /// Whether a notice waits, and its wait and the window are over at `now`.
    fn due(&self, now: f64) -> bool {
        let window_over = self
            .last_started_at
            .is_none_or(|at| now - at >= config::network::ROOM_REFRESH_MIN_INTERVAL_SECS);
        window_over
            && self
                .pending
                .as_ref()
                .is_some_and(|pending| now >= pending.not_before)
    }

    /// Whether a notice waits to be fetched for - for the dispatcher's tests.
    #[cfg(test)]
    pub(crate) fn waiting(&self) -> bool {
        self.pending.is_some()
    }

    /// Take the waiting notice up, starting the window.
    fn take(&mut self, now: f64) -> Option<PendingRefresh> {
        self.last_started_at = Some(now);
        self.pending.take()
    }

    /// A fetch for `failed` failed at `now` in a way that may pass: ask again
    /// on the doubling wait, unless a newer notice came meanwhile or the
    /// attempts are spent. Whether it will be asked again.
    fn retry(&mut self, failed: PendingRefresh, now: f64) -> bool {
        if self.pending.is_some() {
            return true;
        }
        let backoff = RetryBackoff::after_failure_with(
            failed.backoff.as_ref(),
            now,
            config::network::ROOM_REFRESH_RETRY_BASE_SECS,
            config::network::ROOM_REFRESH_RETRY_MAX_SECS,
        );
        if backoff.attempts >= config::network::ROOM_REFRESH_MAX_ATTEMPTS {
            return false;
        }
        self.pending = Some(PendingRefresh {
            not_before: now + backoff.wait_secs,
            backoff: Some(backoff),
            ..failed
        });
        true
    }
}

/// A fetch of the saved room in flight, and what it was started for.
#[derive(Component)]
pub(super) struct RoomRefreshTask {
    task: Task<Result<Option<RoomRecord>, FetchError>>,
    /// The notice it answers.
    notice: PendingRefresh,
    /// [`RoomRefresh::generation`] when it started.
    generation: u64,
}

/// A room swap - a portal hop, to another room or back to this one, or the
/// first room of a session - overtakes a fetch for the copy it replaced.
pub(super) fn note_room_swaps(
    room_did: Option<Res<CurrentRoomDid>>,
    mut refresh: ResMut<RoomRefresh>,
) {
    if room_did.is_some_and(|room| room.is_changed()) {
        refresh.overtake();
    }
}

/// Fetch the saved room for a due notice.
pub(super) fn start_room_refreshes(
    mut commands: Commands,
    mut refresh: ResMut<RoomRefresh>,
    room_did: Option<Res<CurrentRoomDid>>,
    running: Query<(), With<RoomRefreshTask>>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    if !running.is_empty() || !refresh.due(now) {
        return;
    }
    let Some(notice) = refresh.take(now) else {
        return;
    };
    // A notice from a room left since is not this one's.
    if room_did.as_deref().map(|room| room.0.as_str()) != Some(notice.room_did.as_str()) {
        return;
    }
    let did = notice.room_did.clone();
    let task = bevy::tasks::IoTaskPool::get().spawn(async move {
        let fut = async {
            let client = config::http::default_client();
            crate::pds::fetch_room_record(&client, &did).await
        };
        config::http::run_or(
            fut,
            Err(FetchError::Network(config::http::timed_out(
                "saved room fetch",
            ))),
        )
        .await
    });
    commands.spawn(RoomRefreshTask {
        task,
        notice,
        generation: refresh.generation,
    });
}

/// Land a fetched saved room: it replaces the copy standing where the two
/// differ, unless a newer copy has arrived since the fetch began.
#[allow(clippy::too_many_arguments)]
pub(super) fn poll_room_refreshes(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut RoomRefreshTask)>,
    mut refresh: ResMut<RoomRefresh>,
    mut live: Option<ResMut<LiveRoomRecord>>,
    mut stored: Option<ResMut<StoredRoomRecord>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut undo_signals: ResMut<crate::state::RoomWriteSignals>,
    mut session_log: ResMut<SessionLog>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    for (entity, mut task) in &mut tasks {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        // Overtaken - by the owner's live update or a room swap - or answering
        // for a room left since.
        if refresh.generation != task.generation
            || room_did.as_deref().map(|room| room.0.as_str())
                != Some(task.notice.room_did.as_str())
        {
            continue;
        }
        let notice = task.notice.clone();
        match result {
            Ok(Some(mut record)) => {
                record.sanitize();
                let changed = live
                    .as_deref()
                    .is_some_and(|live| crate::state::records_differ(&live.0, &record));
                if changed && let Some(live) = live.as_mut() {
                    if let Some(stored) = stored.as_mut() {
                        stored.0 = record.clone();
                    }
                    live.0 = record;
                    // A wholesale write from elsewhere, as an owner's live
                    // update is (#862): no local undo steps across it.
                    undo_signals.foreign = true;
                    // A clean read of the room retires a banner its failed
                    // first read raised (#840).
                    commands.remove_resource::<crate::state::RoomRecordRecovery>();
                    info!("Room updated from its owner's saved copy");
                }
                session_log.info(now, EventPayload::RoomRefreshedAfterSave { changed });
            }
            Ok(None) => {
                // No room record at all: the owner deleted it. The world
                // standing stays, and this notice asks no more.
                warn!(
                    "The owner of {} sent a save notice, but their PDS holds no room record",
                    notice.room_did
                );
                session_log.warn(
                    now,
                    EventPayload::RoomRefreshFailed {
                        error: String::from("the PDS holds no room record"),
                        will_retry: false,
                    },
                );
            }
            Err(err) => {
                let will_retry = err.left_unanswered() && refresh.retry(notice, now);
                // `Debug`, never `Display`, in a log (#1432): a decoder's
                // message quotes the record, and the world is someone else's.
                warn!("Fetching the saved room failed: {err:?}");
                session_log.warn(
                    now,
                    EventPayload::RoomRefreshFailed {
                        error: format!("{err:?}"),
                        will_retry,
                    },
                );
            }
        }
    }
}

/// Forget the room's state and drop a fetch in flight when the session ends:
/// a fetch landing in the next session would install the last one's world.
pub(super) fn reset_room_refresh(
    mut commands: Commands,
    mut refresh: ResMut<RoomRefresh>,
    tasks: Query<Entity, With<RoomRefreshTask>>,
) {
    *refresh = RoomRefresh::default();
    for entity in &tasks {
        commands.entity(entity).despawn();
    }
}

/// Whether a save notice from `sender_did` is one this session listens to:
/// the room's owner's, and not when this session is the owner too - its
/// copies and the other session's are the same-owner split's (#1203).
pub(super) fn listens_to_save_notice(
    sender_did: Option<&str>,
    room_did: Option<&str>,
    session_did: Option<&str>,
) -> bool {
    let (Some(sender), Some(room)) = (sender_did, room_did) else {
        return false;
    };
    sender == room && session_did != Some(room)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A room no directory is asked about: `did:key` has no document, so a
    /// fetch the start system really spawns fails at once, with no network
    /// (as `peer_cache`'s tests do).
    const ROOM: &str = "did:key:z6MksavedmanorRoomRefresh";

    /// The world as a guest holds it: sanitised, as every copy it installs
    /// is - the poller sanitises what it fetches before comparing.
    fn record() -> RoomRecord {
        let mut record = RoomRecord::default_for_did(ROOM);
        record.sanitize();
        record
    }

    fn saved() -> RoomRecord {
        let mut saved = record();
        saved.environment.fog_visibility.0 = 1234.5;
        saved.sanitize();
        saved
    }

    /// A fetch already answered, handed over as a finished task (#1295's
    /// pattern, as `peer_cache`'s tests do): what is tested is what the
    /// poller does with an answer, not whether a pool produced one.
    fn finished(
        value: Result<Option<RoomRecord>, FetchError>,
    ) -> Task<Result<Option<RoomRecord>, FetchError>> {
        static POOL: std::sync::OnceLock<bevy::tasks::TaskPool> = std::sync::OnceLock::new();
        let pool = POOL.get_or_init(|| {
            bevy::tasks::TaskPoolBuilder::new()
                .num_threads(1)
                .thread_name("room-refresh-test".into())
                .build()
        });
        let task = pool.spawn(async move { value });
        let give_up_at = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !task.is_finished() {
            assert!(
                std::time::Instant::now() < give_up_at,
                "a trivial task never ran"
            );
            std::thread::yield_now();
        }
        task
    }

    fn notice() -> PendingRefresh {
        PendingRefresh {
            room_did: String::from(ROOM),
            not_before: 0.0,
            backoff: None,
        }
    }

    /// A guest standing in `standing`, settled in its room, with a fetch
    /// landing on `answer`.
    fn landing(standing: RoomRecord, answer: Result<Option<RoomRecord>, FetchError>) -> App {
        let mut app = App::new();
        app.add_plugins((bevy::app::TaskPoolPlugin::default(), bevy::time::TimePlugin));
        app.init_resource::<RoomRefresh>()
            .init_resource::<SessionLog>()
            .init_resource::<crate::state::RoomWriteSignals>()
            .insert_resource(CurrentRoomDid(String::from(ROOM)))
            .insert_resource(StoredRoomRecord(standing.clone()))
            .insert_resource(LiveRoomRecord(standing))
            .add_systems(Update, (note_room_swaps, poll_room_refreshes).chain());
        // Arriving is a swap of its own: the fetch starts after it.
        app.update();
        let generation = app.world().resource::<RoomRefresh>().generation;
        app.world_mut().spawn(RoomRefreshTask {
            task: finished(answer),
            notice: notice(),
            generation,
        });
        app
    }

    fn live_of(app: &App) -> &RoomRecord {
        &app.world().resource::<LiveRoomRecord>().0
    }

    fn differs(a: &RoomRecord, b: &RoomRecord) -> bool {
        crate::state::records_differ(a, b)
    }

    /// The case that asked for this (#1499): a world past the ceiling sends
    /// its guests nothing live, so a guest stands in the world as it was when
    /// it arrived. The owner saves; the fetched save replaces the live record
    /// and the stored one.
    #[test]
    fn a_fetched_save_replaces_the_world_standing() {
        let mut app = landing(record(), Ok(Some(saved())));
        app.update();

        assert!(!differs(live_of(&app), &saved()));
        assert!(!differs(
            &app.world().resource::<StoredRoomRecord>().0,
            &saved()
        ));
        assert!(
            app.world()
                .resource::<crate::state::RoomWriteSignals>()
                .foreign
        );
    }

    /// A guest that holds the saved copy already - one that arrived after the
    /// save - fetches it and changes nothing: no rebuild, no undo reset.
    #[test]
    fn a_fetched_copy_like_the_one_standing_installs_nothing() {
        let mut app = landing(saved(), Ok(Some(saved())));
        app.update();

        assert!(!differs(live_of(&app), &saved()));
        assert!(
            !app.world()
                .resource::<crate::state::RoomWriteSignals>()
                .foreign
        );
    }

    /// An owner's live update that lands while the fetch runs is newer than
    /// the save the fetch was started for: the fetch lands nowhere.
    #[test]
    fn a_live_update_since_the_fetch_began_wins() {
        let mut app = landing(record(), Ok(Some(saved())));
        app.world_mut().resource_mut::<RoomRefresh>().overtake();
        app.update();

        assert!(!differs(live_of(&app), &record()));
    }

    /// The review's finding: a portal hop out and back - or a portal into
    /// this same room - while the fetch ran brought a newer copy than the one
    /// the fetch was started for. The swap overtakes it.
    #[test]
    fn a_room_swap_since_the_fetch_began_wins() {
        let mut app = landing(record(), Ok(Some(saved())));
        // The portal writes the same room back, which is still a swap.
        app.world_mut().resource_mut::<CurrentRoomDid>().0 = String::from(ROOM);
        app.update();

        assert!(!differs(live_of(&app), &record()));
    }

    /// A portal hop to another room while the fetch ran: the answer is the
    /// room left behind's.
    #[test]
    fn a_fetch_for_a_room_left_since_lands_nowhere() {
        let mut app = landing(record(), Ok(Some(saved())));
        app.insert_resource(CurrentRoomDid(String::from("did:plc:elsewhere")));
        app.update();

        assert!(!differs(live_of(&app), &record()));
    }

    /// A fetch that fails in a way that may pass keeps the world standing
    /// and asks again on the doubling wait, until the attempts are spent.
    #[test]
    fn a_failed_fetch_is_asked_again_until_the_attempts_are_spent() {
        let mut app = landing(record(), Err(FetchError::Network(String::from("offline"))));
        app.update();

        assert!(!differs(live_of(&app), &record()));
        let again = app
            .world()
            .resource::<RoomRefresh>()
            .pending
            .clone()
            .expect("asked again");
        assert!(again.not_before > 0.0, "after a wait");

        let mut refresh = RoomRefresh::default();
        let mut failed = notice();
        let mut retried = 0;
        while refresh.retry(failed.clone(), 0.0) {
            failed = refresh.pending.take().expect("pending again");
            retried += 1;
            assert!(retried < 100, "the attempts must run out");
        }
        assert_eq!(
            retried + 1,
            config::network::ROOM_REFRESH_MAX_ATTEMPTS,
            "every attempt but the last is followed by another"
        );
    }

    /// A record that will not decode is an answer, and is not asked again.
    #[test]
    fn a_saved_room_that_will_not_decode_is_not_asked_again() {
        let mut app = landing(record(), Err(FetchError::Decode(String::from("bad"))));
        app.update();

        assert!(app.world().resource::<RoomRefresh>().pending.is_none());
        assert!(!differs(live_of(&app), &record()));
    }

    /// A start app: a guest standing in [`ROOM`], with the start system only.
    fn starting() -> App {
        let mut app = App::new();
        app.add_plugins((bevy::app::TaskPoolPlugin::default(), bevy::time::TimePlugin));
        app.init_resource::<RoomRefresh>()
            .insert_resource(CurrentRoomDid(String::from(ROOM)))
            .add_systems(Update, start_room_refreshes);
        app
    }

    fn fetches(app: &mut App) -> usize {
        let mut tasks = app.world_mut().query::<&RoomRefreshTask>();
        tasks.iter(app.world()).count()
    }

    /// The start system fetches for the owner's notice, once a window: the
    /// notices inside a window wait for its end, and one from a room left
    /// since fetches nothing.
    #[test]
    fn a_notice_is_fetched_for_once_a_window() {
        let mut app = starting();
        app.world_mut()
            .resource_mut::<RoomRefresh>()
            .note_save(ROOM, 0.0);
        app.update();
        assert_eq!(fetches(&mut app), 1, "the owner's notice is fetched for");

        let mut refresh = RoomRefresh::default();
        refresh.note_save(ROOM, 0.0);
        assert!(refresh.due(0.0));
        assert!(refresh.take(0.0).is_some());
        refresh.note_save(ROOM, 0.5);
        assert!(!refresh.due(0.5), "the next waits for the window");
        assert!(
            refresh.due(config::network::ROOM_REFRESH_MIN_INTERVAL_SECS),
            "and is not dropped"
        );

        let mut elsewhere = starting();
        elsewhere
            .world_mut()
            .resource_mut::<RoomRefresh>()
            .note_save("did:plc:elsewhere", 0.0);
        elsewhere.update();
        assert_eq!(
            fetches(&mut elsewhere),
            0,
            "a room left since is not this one"
        );
    }

    /// A live update after a notice is newer than the save behind it: the
    /// waiting notice is dropped before anything is fetched for it.
    #[test]
    fn a_live_update_drops_a_waiting_notice() {
        let mut app = starting();
        app.world_mut()
            .resource_mut::<RoomRefresh>()
            .note_save(ROOM, 0.0);
        app.world_mut().resource_mut::<RoomRefresh>().overtake();
        app.update();

        assert_eq!(fetches(&mut app), 0);
    }

    /// Only the room's owner is listened to, and not by the owner's own
    /// other session.
    #[test]
    fn only_the_owner_is_listened_to_and_not_by_the_owner() {
        let guest = Some("did:plc:visiting");
        assert!(listens_to_save_notice(Some(ROOM), Some(ROOM), guest));
        assert!(!listens_to_save_notice(
            Some("did:plc:someoneelse"),
            Some(ROOM),
            guest
        ));
        assert!(!listens_to_save_notice(None, Some(ROOM), guest));
        assert!(!listens_to_save_notice(Some(ROOM), None, guest));
        assert!(
            !listens_to_save_notice(Some(ROOM), Some(ROOM), Some(ROOM)),
            "the owner's other session is the same-owner split's"
        );
    }
}
