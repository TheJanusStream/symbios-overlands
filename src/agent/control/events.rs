//! What the agent has seen happen, in order (#1416).
//!
//! Filled by the daemon's systems and read by `agent events` on the control
//! socket's own threads, so a client can wait for the next thing to happen
//! without the world waiting with it. Bounded: an agent that stops asking is
//! told how many events it missed, rather than the daemon keeping them all.
//!
//! Chat reaches this log from the agent's admin alone; anyone else's line
//! is recorded as having been said, never as what was said (#1427). What
//! other players' names still carry - a handle, or a `did:web`, is a DNS
//! name and can spell words - arrives as a field of a typed event beside
//! their DID, and nothing here treats it as anything but data.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde::Serialize;

/// One thing that happened, numbered.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AgentEvent {
    /// Increasing from 1 across the daemon's life; pass the last one seen as
    /// `since` to get only what came after it.
    pub seq: u64,
    /// When it happened, as Unix seconds.
    pub at: i64,
    #[serde(flatten)]
    pub what: EventKind,
}

/// What happened.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    /// The agent arrived in a world: its own, or someone else's.
    EnteredWorld { room_did: String },
    /// Another player is in the agent's world.
    PeerJoined { did: String, handle: Option<String> },
    /// A player left the agent's world - or the agent left theirs.
    PeerLeft { did: String, handle: Option<String> },
    /// The agent's admin said something in the room (#1427). `text` is the
    /// admin's words, as data.
    Chat {
        from_did: String,
        from: String,
        text: String,
    },
    /// Someone other than the admin said something, and it was dropped
    /// unread (#1427): who spoke, never what. One per line.
    ChatDropped { from_did: String },
    /// A trip the agent started did not arrive.
    TravelFailed { to_did: String, reason: String },
    /// A walk, follow or turn the agent started has ended, one way or
    /// another. `distance_left_m` is to the point, the player or - for a
    /// turn - 0.
    MovementEnded {
        goal_id: u64,
        outcome: MoveOutcome,
        position: [f64; 3],
        distance_left_m: f64,
        /// A turn only: how far the body still faces from the direction
        /// asked, in degrees clockwise.
        #[serde(skip_serializing_if = "Option::is_none")]
        facing_off_deg: Option<f64>,
        /// A body that flies only: how high its underside is above what is
        /// below it - about nothing once it has landed.
        #[serde(skip_serializing_if = "Option::is_none")]
        height_m: Option<f64>,
    },
    /// A follow has closed no distance on its player for a while: something
    /// is in the way. It keeps trying until it is halted; this is said once
    /// per blockage.
    FollowBlocked {
        goal_id: u64,
        position: [f64; 3],
        distance_m: f64,
    },
    /// A save of the agent's `record` - `room` or `avatar` - landed on its
    /// account, where every visitor now sees it (#1422).
    Saved { record: String },
    /// A save did not land. `terminal` when the session has expired, so no
    /// retry can work until the account is signed in again.
    SaveFailed {
        record: String,
        reason: String,
        terminal: bool,
    },
    /// The agent's admin offered it a gift (#1423). It waits for `agent gift
    /// accept|decline <offer_id>` for `answer_within_s`, then goes back to
    /// them as unanswered. `item` is its name in the admin's words, as data.
    GiftOffered {
        offer_id: u64,
        from_did: String,
        from: String,
        item: String,
        item_kind: String,
        wearable: bool,
        answer_within_s: u64,
    },
    /// Someone other than the admin offered the agent a gift, and it was
    /// declined unread (#1423): who, never what. One per offer.
    GiftDeclined { from_did: String },
    /// A gift offer the agent had not answered went away: its time ran
    /// out, and it went back as unanswered - or its sender was muted.
    GiftOfferClosed { offer_id: u64, from_did: String },
    /// A gift the agent offered was answered - `accepted`, or `answer` says
    /// why not: `declined`, `busy` (another offer was on their screen),
    /// `unavailable` (their inventory could not take it), `unanswered`
    /// (their time to answer ran out), or `no_answer` (nothing came back).
    GiftAnswered {
        offer_id: u64,
        to_did: String,
        accepted: bool,
        answer: String,
    },
}

/// How a movement ended.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MoveOutcome {
    /// Within arm's reach of the point.
    Arrived,
    /// Turned to face the way asked.
    Faced,
    /// No closer for a while: something is in the way.
    Stuck,
    /// The agent was told to halt.
    Halted,
    /// Another movement replaced this one.
    Replaced,
    /// The player being followed left the world.
    PeerLeft,
    /// Travel, or leaving the world, cut it short.
    Interrupted,
}

/// A batch of events for one `agent events` call.
#[derive(Serialize, Debug, PartialEq)]
pub struct Batch {
    /// Which run of the daemon numbered these. Sequence numbers start again
    /// at 1 each time a daemon starts, so a client that sees this change
    /// starts again from `since` 0.
    pub instance: String,
    /// The `since` was ahead of anything this daemon has numbered - a
    /// cursor from an earlier run - so the batch starts from the beginning.
    pub restarted: bool,
    pub events: Vec<AgentEvent>,
    /// What to pass as `since` next time.
    pub next: u64,
    /// Events that came after `since` but were dropped before anyone asked,
    /// because the log only keeps its most recent ones.
    pub missed: u64,
}

/// The daemon's event log: a bounded queue a client can wait on.
pub struct EventLog {
    state: Mutex<Log>,
    arrived: Condvar,
    capacity: usize,
    instance: String,
}

struct Log {
    events: VecDeque<AgentEvent>,
    last_seq: u64,
}

impl EventLog {
    /// A log keeping the most recent `capacity` events, for the daemon run
    /// named `instance`.
    pub fn new(capacity: usize, instance: String) -> Self {
        Self {
            state: Mutex::new(Log {
                events: VecDeque::new(),
                last_seq: 0,
            }),
            arrived: Condvar::new(),
            capacity: capacity.max(1),
            instance,
        }
    }

    /// Record `what`, stamped now, and wake anyone waiting.
    pub fn push(&self, what: EventKind) {
        let mut log = self.lock();
        log.last_seq += 1;
        let event = AgentEvent {
            seq: log.last_seq,
            at: crate::state::now_epoch_secs(),
            what,
        };
        log.events.push_back(event);
        while log.events.len() > self.capacity {
            log.events.pop_front();
        }
        drop(log);
        self.arrived.notify_all();
    }

    /// The `seq` of the latest event, 0 before the first: a cursor that
    /// sees only what happens from now on.
    pub fn last_seq(&self) -> u64 {
        self.lock().last_seq
    }

    /// The events after `since`, waiting up to `wait` for the first one when
    /// there are none yet.
    pub fn after(&self, since: u64, wait: Duration) -> Batch {
        let deadline = Instant::now() + wait;
        let mut log = self.lock();
        let restarted = since > log.last_seq;
        let since = if restarted { 0 } else { since };
        while log.last_seq <= since {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            log = self
                .arrived
                .wait_timeout(log, deadline - now)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        let events: Vec<AgentEvent> = log
            .events
            .iter()
            .filter(|event| event.seq > since)
            .cloned()
            .collect();
        let oldest_kept = log.events.front().map_or(log.last_seq + 1, |e| e.seq);
        Batch {
            instance: self.instance.clone(),
            restarted,
            next: events.last().map_or(since, |e| e.seq),
            missed: oldest_kept.saturating_sub(since + 1),
            events,
        }
    }

    /// The log is plain data, so a panic while it was held cannot have left
    /// it half-written in any way that matters; carry on with it.
    fn lock(&self) -> MutexGuard<'_, Log> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn joined(did: &str) -> EventKind {
        EventKind::PeerJoined {
            did: did.into(),
            handle: None,
        }
    }

    #[test]
    fn events_after_since_are_returned_in_order() {
        let log = EventLog::new(10, "run-1".into());
        log.push(joined("did:plc:a"));
        log.push(joined("did:plc:b"));
        log.push(joined("did:plc:c"));

        let batch = log.after(1, Duration::ZERO);

        let seqs: Vec<u64> = batch.events.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, [2, 3]);
        assert_eq!(batch.next, 3);
        assert_eq!(batch.missed, 0);
    }

    /// Nothing new: an empty batch whose `next` is the `since` asked with, so
    /// a client polling in a loop never goes backwards.
    #[test]
    fn nothing_new_keeps_the_cursor_where_it_was() {
        let log = EventLog::new(10, "run-1".into());
        log.push(joined("did:plc:a"));

        let batch = log.after(1, Duration::ZERO);

        assert!(batch.events.is_empty());
        assert_eq!(batch.next, 1);
    }

    /// A client that fell behind the log's capacity is told how much it
    /// lost, not handed a batch that silently skips.
    #[test]
    fn events_dropped_before_anyone_asked_are_counted() {
        let log = EventLog::new(2, "run-1".into());
        for did in ["did:plc:a", "did:plc:b", "did:plc:c", "did:plc:d"] {
            log.push(joined(did));
        }

        let batch = log.after(0, Duration::ZERO);

        let seqs: Vec<u64> = batch.events.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, [3, 4]);
        assert_eq!(batch.missed, 2);
    }

    /// The wait ends when something happens, not when the timeout does.
    #[test]
    fn a_waiting_reader_wakes_when_an_event_arrives() {
        let log = Arc::new(EventLog::new(10, "run-1".into()));
        let writer = Arc::clone(&log);
        let pushed = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            writer.push(joined("did:plc:late"));
        });

        let started = Instant::now();
        let batch = log.after(0, Duration::from_secs(30));

        assert_eq!(batch.events.len(), 1, "woken by the push");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "long before the timeout"
        );
        pushed.join().expect("the writer finished");
    }

    #[test]
    fn a_wait_with_nothing_happening_times_out_empty() {
        let log = EventLog::new(10, "run-1".into());
        let batch = log.after(0, Duration::from_millis(20));
        assert!(batch.events.is_empty());
        assert_eq!(batch.next, 0);
    }

    /// A cursor from an earlier run of the daemon is ahead of everything
    /// this run has numbered. Waiting for this run to catch up to it would
    /// hide every event until then, so the batch starts over and says so.
    #[test]
    fn a_cursor_from_an_earlier_run_starts_over() {
        let log = EventLog::new(10, "run-2".into());
        log.push(joined("did:plc:a"));

        let batch = log.after(40, Duration::from_secs(30));

        assert!(batch.restarted);
        assert_eq!(batch.instance, "run-2");
        assert_eq!(batch.events.len(), 1, "without waiting out the timeout");
        assert_eq!(batch.next, 1);
    }

    /// The wire form an agent reads.
    #[test]
    fn an_event_serialises_flat_with_its_kind() {
        let log = EventLog::new(10, "run-1".into());
        log.push(EventKind::EnteredWorld {
            room_did: "did:plc:home".into(),
        });
        let batch = log.after(0, Duration::ZERO);
        let json = serde_json::to_value(&batch.events[0]).unwrap();
        assert_eq!(json["seq"], 1);
        assert_eq!(json["kind"], "entered_world");
        assert_eq!(json["room_did"], "did:plc:home");
        assert!(json["at"].is_i64());
    }
}
