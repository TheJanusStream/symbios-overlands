//! The resource the rest of the app asks for geodata.
//!
//! Requests go in with [`GeoFetcher::submit`] and come out by id with
//! [`GeoFetcher::take`]. In between, the fetcher - driven once a frame by
//! [`super::drive_geo_fetches`] - merges identical requests into one fetch,
//! keeps at most [`MAX_IN_FLIGHT`] fetches running (GDI Berlin is a public
//! service, and a region's first visit asks it for a handful of renders and
//! pages at once), retries a transient failure after a backoff, and counts
//! what it has done for a loading screen ([`GeoProgress`]).
//!
//! Each answer is settled with where it came from and its content hash
//! ([`Answer`], #1590), so a consumer holding the hash a record was saved
//! with can tell a stale stored answer from a fresh one, and fetch it again
//! past the store ([`GeoFetcher::refetch`]) - keeping the stored answer
//! should the network fail it.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};

use crate::world_builder::asset_failure::AssetFetchError;

use super::fetch::{GetResult, Source, fetch_fresh, fetch_once};
use super::{GeoFetchError, GeoRequest, GeoStore};

/// The most fetches running at once.
pub const MAX_IN_FLIGHT: usize = 4;

/// The most attempts at one request before its failure is the answer.
pub const MAX_ATTEMPTS: u32 = 3;

/// Seconds to wait before the second and the third attempt.
pub const RETRY_BACKOFF_SECS: [f64; 2] = [2.0, 8.0];

/// What the network half of a fetch returns: the platform's HTTP future,
/// `Send` where the task pool is threaded.
#[cfg(not(target_arch = "wasm32"))]
pub type GetFuture = Pin<Box<dyn Future<Output = GetResult> + Send>>;
/// What the network half of a fetch returns (the web's futures are
/// single-threaded).
#[cfg(target_arch = "wasm32")]
pub type GetFuture = Pin<Box<dyn Future<Output = GetResult>>>;

/// The network half of a fetch: a capped GET. The seam tests script.
pub trait GeoTransport: Send + Sync + 'static {
    /// GET `url`, refusing a body over `cap` bytes, answering the body and
    /// the URL it finally came from.
    fn get(&self, url: String, cap: usize) -> GetFuture;
}

/// The real network: the app's configured client, capped streaming, and a
/// time bound on every request.
pub struct HttpTransport;

impl GeoTransport for HttpTransport {
    fn get(&self, url: String, cap: usize) -> GetFuture {
        // The client is built inside the future, as every fetch site does:
        // natively that future runs on the shared Tokio runtime, which
        // reqwest's connection pool needs.
        let fetch = async move {
            let client = crate::config::http::default_client();
            crate::world_builder::blob_fetch::fetch_url_bytes_from(&client, &url, cap, "Geodata")
                .await
        };
        // Natively on the shared runtime itself, awaited without holding an
        // I/O pool thread for the whole request - several geodata fetches
        // run at once, and the pool may have a single thread. The client's
        // own timeout bounds it.
        #[cfg(not(target_arch = "wasm32"))]
        {
            Box::pin(async move {
                crate::config::http::spawn(fetch)
                    .await
                    .unwrap_or(Err(AssetFetchError::Unreachable))
            })
        }
        // On the web, raced against the browser timer every fetch site uses.
        #[cfg(target_arch = "wasm32")]
        {
            Box::pin(crate::config::http::run_or(
                fetch,
                Err(AssetFetchError::TimedOut),
            ))
        }
    }
}

/// Names one submitted request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GeoRequestId(u64);

/// What the fetcher has done since it was last cleared.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GeoProgress {
    /// Fetches started (identical requests merged count once).
    pub requested: u32,
    /// Fetches settled, answered or failed.
    pub finished: u32,
    /// Of those, the ones that failed.
    pub failed: u32,
    /// Of those, the ones the store answered.
    pub from_store: u32,
    /// Bytes fetched from the network.
    pub bytes_fetched: u64,
}

impl GeoProgress {
    /// Whether everything requested has settled.
    pub fn is_done(&self) -> bool {
        self.finished == self.requested
    }
}

/// A settled answer: its body, where it came from, and its content hash
/// ([`super::content_hash`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    pub body: Arc<[u8]>,
    pub source: Source,
    pub hash: u64,
}

/// What one attempt answers.
type Attempted = Result<Answer, GeoFetchError>;

enum State {
    /// Waiting to start, not before this many seconds of real time.
    Queued {
        not_before: f64,
    },
    InFlight(Task<Attempted>),
}

struct Pending {
    request: GeoRequest,
    /// Whether it is fetched past the store ([`GeoFetcher::refetch`]).
    fresh: bool,
    /// What a fetch past the store settles with if it fails for good: the
    /// stored answer it was to replace, older but usable.
    fallback: Option<Answer>,
    /// Every id submitted for this request while it was pending.
    waiting: Vec<GeoRequestId>,
    /// Attempts started so far.
    attempts: u32,
    state: State,
}

/// An id's answer, waiting to be taken, with the request it answers.
struct Settled {
    request: GeoRequest,
    answer: Attempted,
}

/// Geodata requests in, answers out. See the module docs.
#[derive(Resource)]
pub struct GeoFetcher {
    store: GeoStore,
    transport: Arc<dyn GeoTransport>,
    /// In submission order, which is the order they start in.
    pending: Vec<Pending>,
    results: HashMap<GeoRequestId, Settled>,
    next_id: u64,
    progress: GeoProgress,
}

impl GeoFetcher {
    /// A fetcher keeping answers in `store` and fetching through `transport`.
    pub fn new(store: GeoStore, transport: Arc<dyn GeoTransport>) -> Self {
        GeoFetcher {
            store,
            transport,
            pending: Vec::new(),
            results: HashMap::new(),
            next_id: 0,
            progress: GeoProgress::default(),
        }
    }

    /// Ask for `request`. Its answer is [`Self::take`]n by the returned id
    /// once it has settled; an identical request already pending is not
    /// fetched twice.
    pub fn submit(&mut self, request: GeoRequest) -> GeoRequestId {
        self.submit_as(request, false, None)
    }

    /// [`Self::submit`], past the store where `fresh`, settling with
    /// `fallback` should that fail. A fresh request is merged only with
    /// another fresh one: a plain one pending may yet be answered from the
    /// store.
    fn submit_as(
        &mut self,
        request: GeoRequest,
        fresh: bool,
        fallback: Option<Answer>,
    ) -> GeoRequestId {
        let id = GeoRequestId(self.next_id);
        self.next_id += 1;
        match self
            .pending
            .iter_mut()
            .find(|p| p.request == request && p.fresh == fresh)
        {
            Some(pending) => {
                pending.waiting.push(id);
                pending.fallback = pending.fallback.take().or(fallback);
            }
            None => {
                self.progress.requested += 1;
                self.pending.push(Pending {
                    request,
                    fresh,
                    fallback,
                    waiting: vec![id],
                    attempts: 0,
                    state: State::Queued { not_before: 0.0 },
                });
            }
        }
        id
    }

    /// Ask again for what `id` asked, past the store - whose answer is
    /// known to be older than one a record was saved with (#1590) - and give
    /// up `id`'s answer: the new request's id, or `None` where `id` has no
    /// answer to give up (unsettled, taken or forgotten). Should the fetch
    /// fail for good, the new id settles with the answer `id` had, where
    /// it had one: an older answer is better than none.
    pub fn refetch(&mut self, id: GeoRequestId) -> Option<GeoRequestId> {
        let settled = self.results.remove(&id)?;
        Some(self.submit_as(settled.request, true, settled.answer.ok()))
    }

    /// The answer for `id`, once settled - handed over once.
    pub fn take(&mut self, id: GeoRequestId) -> Option<Result<Arc<[u8]>, GeoFetchError>> {
        self.take_answer(id)
            .map(|answer| answer.map(|answer| answer.body))
    }

    /// The whole answer for `id`, once settled - handed over once.
    pub fn take_answer(&mut self, id: GeoRequestId) -> Option<Result<Answer, GeoFetchError>> {
        self.results.remove(&id).map(|settled| settled.answer)
    }

    /// The answer for `id`, once settled, left in place. Read-only.
    pub fn answer(&self, id: GeoRequestId) -> Option<&Result<Answer, GeoFetchError>> {
        self.results.get(&id).map(|settled| &settled.answer)
    }

    /// Whether `id` has an answer waiting to be [`Self::take`]n. Read-only,
    /// so a consumer can ask every frame without marking the resource
    /// changed.
    pub fn is_settled(&self, id: GeoRequestId) -> bool {
        self.results.contains_key(&id)
    }

    /// Give up on `id`: its answer, settled or not, is never wanted. A
    /// request nobody else is waiting for is dropped - natively its running
    /// fetch is cancelled with its task; on the web it runs to its end,
    /// unheard - and one another id shares carries on for that id. The
    /// progress counts keep what was started.
    pub fn forget(&mut self, id: GeoRequestId) {
        self.results.remove(&id);
        for pending in &mut self.pending {
            pending.waiting.retain(|&waiting| waiting != id);
        }
        let before = self.pending.len();
        self.pending.retain(|p| !p.waiting.is_empty());
        // Dropped requests will never settle: count them as finished, so a
        // loading screen reading `is_done` is not left waiting on them.
        let dropped = (before - self.pending.len()) as u32;
        self.progress.finished += dropped;
    }

    /// What has been done since the last [`Self::clear`].
    pub fn progress(&self) -> GeoProgress {
        self.progress
    }

    /// Whether nothing is queued or running.
    pub fn is_idle(&self) -> bool {
        self.pending.is_empty()
    }

    /// Whether [`Self::drive`] at `now` would change anything: a fetch has
    /// finished, or a queued one may start. Read-only, so the frame system
    /// can ask without marking the resource changed.
    pub fn needs_drive(&self, now: f64) -> bool {
        let running = self.running();
        self.pending.iter().any(|p| match &p.state {
            State::InFlight(task) => task.is_finished(),
            State::Queued { not_before } => *not_before <= now && running < MAX_IN_FLIGHT,
        })
    }

    fn running(&self) -> usize {
        self.pending
            .iter()
            .filter(|p| matches!(p.state, State::InFlight(_)))
            .count()
    }

    /// Forget everything: queued requests (they never start), untaken
    /// answers and the progress counts. For leaving the region they were
    /// asked for. A fetch already running is not heard from again; natively
    /// it may still finish and keep its answer in the store, which is
    /// harmless - every write there is whole, to its own temporary file.
    pub fn clear(&mut self) {
        self.pending.clear();
        self.results.clear();
        self.progress = GeoProgress::default();
    }

    /// Settle what has finished and start what may, at `now` seconds of
    /// real time.
    pub fn drive(&mut self, now: f64) {
        let mut index = 0;
        while index < self.pending.len() {
            let pending = &mut self.pending[index];
            let State::InFlight(task) = &mut pending.state else {
                index += 1;
                continue;
            };
            let Some(result) =
                futures_lite::future::block_on(futures_lite::future::poll_once(task))
            else {
                index += 1;
                continue;
            };
            match result {
                Err(error) if error.retryable() && pending.attempts < MAX_ATTEMPTS => {
                    let wait = RETRY_BACKOFF_SECS
                        [(pending.attempts as usize - 1).min(RETRY_BACKOFF_SECS.len() - 1)];
                    pending.state = State::Queued {
                        not_before: now + wait,
                    };
                    index += 1;
                }
                result => {
                    let pending = self.pending.remove(index);
                    self.settle(pending, result);
                }
            }
        }

        let mut running = self.running();
        for pending in &mut self.pending {
            if running >= MAX_IN_FLIGHT {
                break;
            }
            if let State::Queued { not_before } = pending.state
                && not_before <= now
            {
                pending.attempts += 1;
                pending.state = State::InFlight(spawn_fetch(
                    pending.request.clone(),
                    pending.fresh,
                    self.store.clone(),
                    self.transport.clone(),
                ));
                running += 1;
            }
        }
    }

    fn settle(&mut self, pending: Pending, result: Attempted) {
        self.progress.finished += 1;
        let result = match (result, pending.fallback.clone()) {
            (Err(error), Some(fallback)) => {
                warn!(
                    "geodata fetch past the store failed after {} attempt(s): {error} ({}) - \
                     keeping the stored answer",
                    pending.attempts,
                    pending.request.url()
                );
                Ok(fallback)
            }
            (result, _) => result,
        };
        let answer = match result {
            Ok(answer) => {
                match answer.source {
                    Source::Store { .. } => self.progress.from_store += 1,
                    Source::Network => self.progress.bytes_fetched += answer.body.len() as u64,
                }
                Ok(answer)
            }
            Err(error) => {
                self.progress.failed += 1;
                warn!(
                    "geodata fetch failed after {} attempt(s): {error} ({})",
                    pending.attempts,
                    pending.request.url()
                );
                Err(error)
            }
        };
        for id in pending.waiting {
            self.results.insert(
                id,
                Settled {
                    request: pending.request.clone(),
                    answer: answer.clone(),
                },
            );
        }
    }
}

/// One attempt, on the I/O pool - past the store where `fresh` - its
/// answer hashed there, off the frame.
fn spawn_fetch(
    request: GeoRequest,
    fresh: bool,
    store: GeoStore,
    transport: Arc<dyn GeoTransport>,
) -> Task<Attempted> {
    IoTaskPool::get().spawn(async move {
        let now = chrono::Utc::now().timestamp();
        let get = |url, cap| transport.get(url, cap);
        let fetched = if fresh {
            fetch_fresh(&request, &store, now, get).await
        } else {
            fetch_once(&request, &store, now, get).await
        };
        fetched.map(|(body, source)| Answer {
            hash: super::content_hash(request.kind(), &body),
            body,
            source,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use geodata::berlin;
    use geodata::request::Bbox;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

    const LEGEND: &[u8] = br#"{"Legend":[]}"#;

    /// One scripted attempt: a body, or a failure.
    type Attempt = Result<Vec<u8>, AssetFetchError>;

    /// Answers each URL from a script, one entry per attempt (the last one
    /// repeats), from the URL it was asked, counting the calls.
    struct Scripted {
        answers: Mutex<HashMap<String, Vec<Attempt>>>,
        calls: AtomicU32,
    }

    impl Scripted {
        fn new(script: Vec<(String, Vec<Attempt>)>) -> Arc<Self> {
            Arc::new(Scripted {
                answers: Mutex::new(script.into_iter().collect()),
                calls: AtomicU32::new(0),
            })
        }
    }

    impl GeoTransport for Scripted {
        fn get(&self, url: String, _cap: usize) -> GetFuture {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let mut answers = self.answers.lock().unwrap();
            let queue = answers
                .get_mut(&url)
                .expect("an unscripted URL was fetched");
            let answer = if queue.len() > 1 {
                queue.remove(0)
            } else {
                queue[0].clone()
            };
            Box::pin(async move { answer.map(|body| (body, url)) })
        }
    }

    /// A network that never answers.
    struct Silent;

    impl GeoTransport for Silent {
        fn get(&self, _url: String, _cap: usize) -> GetFuture {
            Box::pin(std::future::pending())
        }
    }

    fn memory() -> GeoStore {
        GeoStore::Memory(Arc::new(Mutex::new(HashMap::new())))
    }

    fn legend(index: usize) -> GeoRequest {
        GeoRequest::legend(&berlin::STOREYS, index)
    }

    /// Drive at `now` until nothing is in flight. Tasks run on the real I/O
    /// pool, so their results land a moment later - and under a one-process
    /// `cargo test` at opt-level 0 that pool may have a single thread that
    /// other tests share, hence the generous wall-clock patience.
    fn settle(fetcher: &mut GeoFetcher, now: f64) {
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(30) {
            fetcher.drive(now);
            if fetcher.running() == 0 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("fetches did not settle");
    }

    #[test]
    fn identical_requests_are_fetched_once_and_answered_to_each_id() {
        let transport = Scripted::new(vec![(legend(0).url().into(), vec![Ok(LEGEND.to_vec())])]);
        let mut fetcher = GeoFetcher::new(memory(), transport.clone());
        let a = fetcher.submit(legend(0));
        let b = fetcher.submit(legend(0));
        assert_ne!(a, b);
        settle(&mut fetcher, 0.0);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
        assert_eq!(fetcher.take(a).unwrap().unwrap().as_ref(), LEGEND);
        assert_eq!(fetcher.take(b).unwrap().unwrap().as_ref(), LEGEND);
        assert!(fetcher.take(a).is_none(), "handed over once");
        let progress = fetcher.progress();
        assert_eq!((progress.requested, progress.finished), (1, 1));
        assert_eq!(progress.bytes_fetched, LEGEND.len() as u64);
        assert!(progress.is_done() && fetcher.is_idle());
    }

    #[test]
    fn a_transient_failure_is_retried_after_its_backoff() {
        let transport = Scripted::new(vec![(
            legend(0).url().into(),
            vec![Err(AssetFetchError::HttpStatus(503)), Ok(LEGEND.to_vec())],
        )]);
        let mut fetcher = GeoFetcher::new(memory(), transport.clone());
        let id = fetcher.submit(legend(0));
        settle(&mut fetcher, 0.0);
        assert!(
            fetcher.take(id).is_none(),
            "the first failure is not the answer"
        );
        // Before the backoff has passed nothing starts.
        settle(&mut fetcher, RETRY_BACKOFF_SECS[0] - 0.1);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
        settle(&mut fetcher, RETRY_BACKOFF_SECS[0]);
        settle(&mut fetcher, RETRY_BACKOFF_SECS[0]);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
        assert_eq!(fetcher.take(id).unwrap().unwrap().as_ref(), LEGEND);
        assert_eq!(fetcher.progress().failed, 0);
    }

    #[test]
    fn a_failure_that_cannot_heal_is_the_answer_at_once() {
        let transport = Scripted::new(vec![(
            legend(0).url().into(),
            vec![Err(AssetFetchError::HttpStatus(404))],
        )]);
        let mut fetcher = GeoFetcher::new(memory(), transport.clone());
        let id = fetcher.submit(legend(0));
        settle(&mut fetcher, 0.0);
        assert_eq!(
            fetcher.take(id),
            Some(Err(GeoFetchError::Fetch(AssetFetchError::HttpStatus(404))))
        );
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
        assert_eq!(fetcher.progress().failed, 1);
    }

    #[test]
    fn retries_stop_at_the_attempt_cap() {
        let transport = Scripted::new(vec![(
            legend(0).url().into(),
            vec![Err(AssetFetchError::Unreachable)],
        )]);
        let mut fetcher = GeoFetcher::new(memory(), transport.clone());
        let id = fetcher.submit(legend(0));
        let mut now = 0.0;
        for _ in 0..MAX_ATTEMPTS + 2 {
            settle(&mut fetcher, now);
            settle(&mut fetcher, now);
            now += 100.0;
        }
        assert_eq!(transport.calls.load(Ordering::SeqCst), MAX_ATTEMPTS);
        assert_eq!(
            fetcher.take(id),
            Some(Err(GeoFetchError::Fetch(AssetFetchError::Unreachable)))
        );
    }

    #[test]
    fn no_more_than_the_cap_run_at_once_and_the_rest_follow() {
        let requests: Vec<GeoRequest> = (0..6).map(legend).collect();
        let script = requests
            .iter()
            .map(|r| (r.url().to_owned(), vec![Ok(LEGEND.to_vec())]))
            .collect();
        let transport = Scripted::new(script);
        let mut fetcher = GeoFetcher::new(memory(), transport.clone());
        let ids: Vec<GeoRequestId> = requests.into_iter().map(|r| fetcher.submit(r)).collect();
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        fetcher.drive(0.0);
        let running = fetcher
            .pending
            .iter()
            .filter(|p| matches!(p.state, State::InFlight(_)))
            .count();
        assert_eq!(running, MAX_IN_FLIGHT);
        for _ in 0..4 {
            settle(&mut fetcher, 0.0);
        }
        for id in ids {
            assert_eq!(fetcher.take(id).unwrap().unwrap().as_ref(), LEGEND);
        }
        assert_eq!(fetcher.progress().finished, 6);
    }

    #[test]
    fn a_second_visit_is_answered_from_the_store() {
        let store = memory();
        let transport = Scripted::new(vec![(legend(0).url().into(), vec![Ok(LEGEND.to_vec())])]);
        let mut first = GeoFetcher::new(store.clone(), transport.clone());
        let id = first.submit(legend(0));
        settle(&mut first, 0.0);
        assert!(first.take(id).unwrap().is_ok());

        let mut second = GeoFetcher::new(store, transport.clone());
        let id = second.submit(legend(0));
        settle(&mut second, 0.0);
        assert_eq!(second.take(id).unwrap().unwrap().as_ref(), LEGEND);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
        assert_eq!(second.progress().from_store, 1);
        assert_eq!(second.progress().bytes_fetched, 0);
    }

    /// An answer known stale (#1590) is asked for again past the store: the
    /// network answers it, its hash is the new answer's, and the store keeps
    /// that from then on.
    #[test]
    fn a_refetch_goes_past_the_store_and_hashes_what_it_got() {
        let store = memory();
        let newer = br#"{"Legend":[{"rules":[]}]}"#;
        let transport = Scripted::new(vec![(
            legend(0).url().into(),
            vec![Ok(LEGEND.to_vec()), Ok(newer.to_vec())],
        )]);
        let mut first = GeoFetcher::new(store.clone(), transport.clone());
        let id = first.submit(legend(0));
        settle(&mut first, 0.0);
        assert!(first.take(id).unwrap().is_ok());

        let mut second = GeoFetcher::new(store, transport.clone());
        let id = second.submit(legend(0));
        settle(&mut second, 0.0);
        let stored = second.answer(id).unwrap().clone().unwrap();
        assert!(matches!(stored.source, Source::Store { .. }));
        assert_eq!(
            stored.hash,
            super::super::content_hash(super::super::GeoKind::Legend, LEGEND)
        );
        let again = second.refetch(id).expect("a settled answer");
        assert!(second.answer(id).is_none(), "the stale answer is gone");
        assert!(second.refetch(id).is_none());
        settle(&mut second, 0.0);
        let fresh = second.take_answer(again).unwrap().unwrap();
        assert_eq!((&*fresh.body, fresh.source), (&newer[..], Source::Network));
        assert_ne!(fresh.hash, stored.hash);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
        // A plain request merges with no fresh one, and the store now
        // answers the newer body.
        let plain = second.submit(legend(0));
        settle(&mut second, 0.0);
        assert_eq!(second.take(plain).unwrap().unwrap().as_ref(), newer);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
    }

    /// A fetch past the store that fails for good settles with the stored
    /// answer it was to replace: an older answer, not none.
    #[test]
    fn a_failed_refetch_keeps_the_stored_answer() {
        let store = memory();
        let transport = Scripted::new(vec![(
            legend(0).url().into(),
            vec![Ok(LEGEND.to_vec()), Err(AssetFetchError::HttpStatus(404))],
        )]);
        let mut first = GeoFetcher::new(store.clone(), transport.clone());
        let id = first.submit(legend(0));
        settle(&mut first, 0.0);
        assert!(first.take(id).unwrap().is_ok());
        let mut second = GeoFetcher::new(store, transport.clone());
        let id = second.submit(legend(0));
        settle(&mut second, 0.0);
        let stored = second.answer(id).unwrap().clone().unwrap();
        let again = second.refetch(id).unwrap();
        settle(&mut second, 0.0);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 2, "it was asked");
        assert_eq!(second.take_answer(again), Some(Ok(stored)));
        assert_eq!(second.progress().failed, 0);
    }

    #[test]
    fn a_refused_request_settles_without_a_fetch() {
        let transport = Scripted::new(vec![]);
        let mut fetcher = GeoFetcher::new(memory(), transport.clone());
        let bbox = Bbox {
            min_e: 0,
            min_n: 0,
            max_e: 1,
            max_n: 1,
        };
        let id = fetcher.submit(GeoRequest::render(&berlin::TERRAIN, bbox, 0, 0));
        settle(&mut fetcher, 0.0);
        assert_eq!(fetcher.take(id), Some(Err(GeoFetchError::Refused)));
        assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
    }

    /// The live path, end to end: the app's HTTP client against GDI Berlin,
    /// the disk store, and the decoders on what comes back; then a second
    /// visit answered from the store alone. Needs the network, so no gate
    /// runs it - fire it by hand when the live path is in question:
    /// `cargo test --profile test-release --lib live_gdi_berlin -- --ignored`.
    #[test]
    #[ignore = "needs the network: GDI Berlin live"]
    fn live_gdi_berlin_round_trip_decodes_and_is_kept() {
        let root =
            std::env::temp_dir().join(format!("overlands-geodata-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = GeoStore::Disk(Arc::new(super::super::store::disk::DiskStore::new(
            root.clone(),
            super::super::store::MAX_DISK_BYTES,
        )));
        let dom = Bbox {
            min_e: 391_200,
            min_n: 5_819_700,
            max_e: 391_800,
            max_n: 5_820_300,
        };
        let legend = GeoRequest::legend(&berlin::TERRAIN, 0);
        let render = GeoRequest::render(&berlin::TERRAIN, dom, 150, 150);
        let hits = GeoRequest::hits(&berlin::BUILDINGS, dom);

        let mut fetcher = GeoFetcher::new(store.clone(), Arc::new(HttpTransport));
        let ids = [
            fetcher.submit(legend.clone()),
            fetcher.submit(render.clone()),
            fetcher.submit(hits.clone()),
        ];
        let start = std::time::Instant::now();
        while !fetcher.progress().is_done() {
            assert!(
                start.elapsed().as_secs() < 90,
                "the live service did not answer"
            );
            settle(&mut fetcher, start.elapsed().as_secs_f64());
        }
        let legend_body = fetcher.take(ids[0]).unwrap().unwrap();
        let render_body = fetcher.take(ids[1]).unwrap().unwrap();
        let hits_body = fetcher.take(ids[2]).unwrap().unwrap();
        let first = fetcher.progress();
        assert_eq!((first.failed, first.from_store), (0, 0));

        let legend = geodata::legend::parse_value_legend(&legend_body).unwrap();
        let image = geodata::raster::decode_png(&render_body, 150, 150).unwrap();
        let grid = geodata::raster::decode_terrain(&image, &legend).unwrap();
        let (low, high) = grid
            .heights
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, h), &v| (l.min(v), h.max(v)));
        // The Museumsinsel: the Spree at ~32 m, the island a few metres up.
        assert!(
            (25.0..45.0).contains(&low) && (30.0..60.0).contains(&high),
            "{low}..{high}"
        );
        let buildings = geodata::request::parse_hits(&hits_body).unwrap();
        assert!(buildings > 50, "{buildings} footprints at the Dom");

        let mut second = GeoFetcher::new(store, Arc::new(HttpTransport));
        let again = second.submit(render);
        settle(&mut second, 0.0);
        assert_eq!(second.take(again).unwrap().unwrap(), render_body);
        assert_eq!(
            (
                second.progress().from_store,
                second.progress().bytes_fetched
            ),
            (1, 0)
        );
        eprintln!(
            "live: {} bytes fetched, terrain {low:.1}..{high:.1} m, {buildings} footprints",
            first.bytes_fetched
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// The frame system takes the resource mutably only when there is work,
    /// so a frame spent waiting - on the network or out a backoff - does not
    /// mark it changed.
    #[test]
    fn waiting_frames_do_not_mark_the_fetcher_changed() {
        fn changed_over_frames(app: &mut App, frames: usize) -> bool {
            let before = app.world().resource_ref::<GeoFetcher>().last_changed();
            for _ in 0..frames {
                app.update();
            }
            app.world().resource_ref::<GeoFetcher>().last_changed() != before
        }
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);

        // Waiting on a network that never answers.
        let mut app = App::new();
        app.init_resource::<Time<Real>>()
            .insert_resource(GeoFetcher::new(memory(), Arc::new(Silent)))
            .add_systems(Update, super::super::drive_geo_fetches);
        app.world_mut()
            .resource_mut::<GeoFetcher>()
            .submit(legend(0));
        app.update();
        assert_eq!(app.world().resource::<GeoFetcher>().running(), 1);
        assert!(
            !changed_over_frames(&mut app, 5),
            "a frame waiting on the network"
        );

        // Waiting out a backoff: the real clock in this app stays at 0.
        let transport = Scripted::new(vec![(
            legend(0).url().into(),
            vec![Err(AssetFetchError::HttpStatus(503))],
        )]);
        let mut app = App::new();
        app.init_resource::<Time<Real>>()
            .insert_resource(GeoFetcher::new(memory(), transport))
            .add_systems(Update, super::super::drive_geo_fetches);
        app.world_mut()
            .resource_mut::<GeoFetcher>()
            .submit(legend(0));
        let start = std::time::Instant::now();
        while app.world().resource::<GeoFetcher>().running() > 0
            || !app
                .world()
                .resource::<GeoFetcher>()
                .pending
                .iter()
                .any(|p| p.attempts == 1)
        {
            assert!(
                start.elapsed().as_secs() < 30,
                "the first attempt never settled"
            );
            app.update();
        }
        assert!(
            !changed_over_frames(&mut app, 5),
            "a frame waiting out a backoff"
        );
    }

    #[test]
    fn clear_forgets_everything() {
        let transport = Scripted::new(vec![(legend(0).url().into(), vec![Ok(LEGEND.to_vec())])]);
        let mut fetcher = GeoFetcher::new(memory(), transport);
        let id = fetcher.submit(legend(0));
        settle(&mut fetcher, 0.0);
        fetcher.submit(legend(0));
        fetcher.clear();
        assert!(fetcher.is_idle());
        assert_eq!(fetcher.take(id), None);
        assert_eq!(fetcher.progress(), GeoProgress::default());
    }

    #[test]
    fn a_forgotten_request_is_dropped_unless_another_id_shares_it() {
        let transport = Scripted::new(vec![
            (legend(0).url().into(), vec![Ok(LEGEND.to_vec())]),
            (legend(1).url().into(), vec![Ok(LEGEND.to_vec())]),
        ]);
        let mut fetcher = GeoFetcher::new(memory(), transport.clone());
        let alone = fetcher.submit(legend(0));
        let shared_a = fetcher.submit(legend(1));
        let shared_b = fetcher.submit(legend(1));
        fetcher.forget(alone);
        fetcher.forget(shared_a);
        // The lone request is gone and counts as settled; the shared one
        // still runs for the id that wants it.
        assert_eq!(fetcher.pending.len(), 1);
        assert_eq!(fetcher.progress().finished, 1);
        settle(&mut fetcher, 0.0);
        assert_eq!(fetcher.take(shared_b).unwrap().unwrap().as_ref(), LEGEND);
        assert_eq!(fetcher.take(shared_a), None);
        assert_eq!(fetcher.take(alone), None);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
        assert!(fetcher.progress().is_done());
        // A settled answer forgotten before it is taken is dropped too.
        let late = fetcher.submit(legend(0));
        settle(&mut fetcher, 0.0);
        fetcher.forget(late);
        assert_eq!(fetcher.take(late), None);
    }
}
