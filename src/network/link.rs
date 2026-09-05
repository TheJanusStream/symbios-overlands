//! The one place the client knows — and says — whether it is connected.
//!
//! Before #1213 the app had no answer to "am I connected?". A socket that
//! never opened, a relay that never welcomed us and a Wi-Fi drop mid-session
//! were all indistinguishable from an empty world: People asserted
//! "In room (1)" and "(no other peers)" about a room the client could not
//! see, chat pushed a message that reached nobody into the local HUD exactly
//! as it pushes a delivered one, remote peers stayed spawned as frozen
//! ghosts, and a gift that never left the machine expired with a toast
//! blaming the recipient for not answering.
//!
//! Every one of those is the same missing fact, so this module supplies it
//! once. [`track_link_state`] writes [`LinkState`] from signals the upstream
//! plugin already publishes and overlands ignored:
//!
//! * `resource_exists::<MatchboxSocket>` — a socket *object* exists. The
//!   plugin removes the resource when the message loop dies, which is the
//!   only observable trace a local outage leaves (`open_socket`'s
//!   `any_channel_closed()` teardown branch).
//! * `MatchboxSocket::id()` — the relay welcomed us and assigned our
//!   `PeerId`. This, not the socket's existence, is what "connected" means.
//! * [`LocalSocketReopened`] — a fresh socket was just spawned; the relay
//!   has not answered yet.
//!
//! ## A message is not a state (#1279)
//!
//! The welcome was originally read from the `WelcomeHandshakeComplete`
//! *message*, and that was the bug. Bevy messages are double-buffered and
//! readable for about two frames; upstream writes that one **exactly once
//! per socket** (`detect_welcome_handshake` early-returns forever after,
//! guarded by its own `welcomed_peer_id`); and this module's reader was
//! registered `run_if(in_state(AppState::InGame))`. But
//! `ui::login::complete::install_completed_session` inserts
//! `SymbiosMultiuserConfig` and only *then* sets `AppState::Loading`, so the
//! socket opens and the relay answers during `Loading` — 0.65 s and 1.9 s
//! before `InGame` in the two login segments of the session log that
//! reported this. The message expired unread, no second one was ever
//! written, and [`LinkPhase::Connected`] became unreachable for the life of
//! the socket. Six surfaces then told the same wrong story, because #1213
//! deliberately made this the one connection fact.
//!
//! So the phase is now derived from a **durable** fact instead. `socket.id()`
//! answers from a `OnceCell` that upstream's own detector fills from the same
//! welcome, so it is true on every frame of the socket's life after the relay
//! answers, and false again the moment a fresh socket replaces it. Nothing
//! here can miss an edge any more, because there is no edge to miss:
//! [`next_phase`] is a pure function of what is true *now*, with no history
//! parameter at all. Widening the run condition alone would have fixed the
//! reported symptom and left the mechanism — any later ordering change,
//! state gate or two-frame stall would have silently dropped the fact again.
//!
//! [`LocalSocketReopened`] survives as an edge, and only an edge: it covers
//! the single frame in which `open_socket` has queued a new socket but the
//! resource has not been inserted yet.
//!
//! Everything else in the tranche reads [`LinkState`]; nothing else reads
//! the three signals. The user-facing wording lives here too — the chip
//! label, the roster header, the chat composer note, the delivery suffix,
//! the presence lines and the offer-expiry sentence are all functions on
//! [`LinkPhase`] / [`LinkState`] / [`ChatDelivery`], so the four surfaces
//! cannot drift into four different stories about one state.

use bevy::prelude::*;
use bevy_symbios_multiuser::prelude::*;

use crate::diagnostics::SessionLog;
use crate::diagnostics::event::EventPayload;
use crate::protocol::OverlandsMessage;
use crate::state::{AppState, ChatHistory, RemotePeer};

/// What the client currently knows about its own relay link.
///
/// Deliberately three states, not two: "a socket object exists" and "the
/// relay has welcomed us" are different facts, and the gap between them is
/// exactly the window in which a message would be dropped while the UI had
/// no reason to say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinkPhase {
    /// No socket object exists — never opened, or torn down after its
    /// message loop died. Nothing sent now leaves this machine.
    #[default]
    Down,
    /// A socket exists but the relay's welcome handshake has not landed.
    /// Sends are still dropped; the difference from [`LinkPhase::Down`] is
    /// that the client has something in flight to be hopeful about.
    Connecting,
    /// The relay welcomed us. Peers can reach us and we can reach them.
    Connected,
}

impl LinkPhase {
    /// Is the link actually usable? The one predicate every caller should
    /// ask, so "usable" cannot come to mean different things on different
    /// surfaces.
    pub fn is_up(self) -> bool {
        matches!(self, LinkPhase::Connected)
    }

    /// The toolbar chip's label. Short enough for a reserved-width slot
    /// (see `ui::toolbar::LINK_CHIP_WIDTH`) and never colour-only — the
    /// word carries the state on its own for a greyscale viewer.
    pub fn chip_label(self) -> &'static str {
        match self {
            LinkPhase::Down => "Offline",
            LinkPhase::Connecting => "Connecting…",
            LinkPhase::Connected => "Connected",
        }
    }

    /// The one sentence naming this state. The chip's hover text, and the
    /// fallback wording anywhere else that needs a full sentence.
    pub fn sentence(self) -> &'static str {
        match self {
            LinkPhase::Down => {
                "Not connected — nothing you send is reaching anyone. The client is retrying."
            }
            LinkPhase::Connecting => {
                "Connecting — waiting for the relay to answer. Nothing you send arrives yet."
            }
            LinkPhase::Connected => "Connected — you can see and be seen by everyone here.",
        }
    }

    /// The People window's header line. When the link is down the client is
    /// in no position to make a claim about the ROOM, so it makes a claim
    /// about itself instead (#1213 f395).
    pub fn roster_header(self, total: usize) -> String {
        match self {
            LinkPhase::Connected => format!("In room ({total})"),
            LinkPhase::Connecting => "Connecting — can't see who's here yet".to_owned(),
            LinkPhase::Down => "Not connected — can't see who's here".to_owned(),
        }
    }

    /// The note under an empty roster. "(no other peers)" is a fact about
    /// the room and may only be printed when the client can observe it.
    pub fn roster_empty_note(self) -> &'static str {
        match self {
            LinkPhase::Connected => "(no other peers)",
            LinkPhase::Connecting => "(the roster fills in when the relay answers)",
            LinkPhase::Down => "(this list is not who is here — you are offline)",
        }
    }
}

/// The fate of one chat line the local user sent (#1213 f396).
///
/// Before this, a message that reached nobody was pushed into `ChatHistory`
/// byte-identically to a delivered one, and the sender's own HUD was the
/// lie: upstream's `transmit_messages` drains the broadcast reader and
/// returns without sending when `connected_peers()` is empty, and the whole
/// transmit chain is `run_if(resource_exists::<MatchboxSocket>)` so a torn
/// down socket never even reaches that check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatDelivery {
    /// Not one of our outgoing messages — inbound chat, presence lines and
    /// system notices. Renders no suffix.
    #[default]
    NotApplicable,
    /// Went out to this many connected peers.
    Reached(usize),
    /// Reached nobody, and the link was up: we really are alone here.
    NobodyHere,
    /// Reached nobody because our own link was down — it never left this
    /// machine, and saying "nobody answered" would blame the room for it.
    NotConnected,
}

impl ChatDelivery {
    /// The weak-coloured suffix rendered after the message text, or `None`
    /// for a line that needs no annotation.
    pub fn suffix(self) -> Option<&'static str> {
        match self {
            ChatDelivery::NotApplicable | ChatDelivery::Reached(_) => None,
            ChatDelivery::NobodyHere => Some("· nobody here to hear it"),
            ChatDelivery::NotConnected => Some("· not sent — you're offline"),
        }
    }

    /// Did this line actually reach anyone?
    pub fn reached_anyone(self) -> bool {
        matches!(self, ChatDelivery::Reached(n) if n > 0)
    }
}

/// The client's own view of its relay link, and the only thing the UI reads.
///
/// Written by [`track_link_state`] on an edge only — never every frame — so
/// a `Changed<LinkState>` filter (and anything downstream of the guarded
/// dirty rule, #879) stays meaningful.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct LinkState {
    phase: LinkPhase,
    /// `Time::elapsed_secs_f64()` at which `phase` was entered. What makes
    /// "was the link up for the whole life of this offer?" answerable.
    since_secs: f64,
}

impl LinkState {
    /// The current phase.
    pub fn phase(&self) -> LinkPhase {
        self.phase
    }

    /// Shorthand for [`LinkPhase::is_up`].
    pub fn is_up(&self) -> bool {
        self.phase.is_up()
    }

    /// Seconds (on the monotonic clock) since the current phase began.
    pub fn since_secs(&self) -> f64 {
        self.since_secs
    }

    /// Has the link been continuously up since `t`? False when it is down
    /// now, and false when it came up *after* `t` — which is the case that
    /// matters: an offer sent into a dead socket, answered by a relay we
    /// only reached afterwards, never reached its recipient either.
    pub fn up_continuously_since(&self, t: f64) -> bool {
        self.is_up() && self.since_secs <= t
    }

    /// Classify a chat line the local user is about to send.
    pub fn delivery(&self, peer_count: usize) -> ChatDelivery {
        if !self.is_up() {
            ChatDelivery::NotConnected
        } else if peer_count == 0 {
            ChatDelivery::NobodyHere
        } else {
            ChatDelivery::Reached(peer_count)
        }
    }

    /// The persistent note above the chat input, or `None` when the message
    /// will actually go somewhere.
    pub fn composer_note(&self, peer_count: usize) -> Option<&'static str> {
        match self.delivery(peer_count) {
            ChatDelivery::Reached(_) | ChatDelivery::NotApplicable => None,
            ChatDelivery::NobodyHere => {
                Some("No one else is here — messages you send now go nowhere.")
            }
            ChatDelivery::NotConnected => {
                Some("You're not connected — messages you send now go nowhere.")
            }
        }
    }

    /// The expiry sentence for an outgoing gift offer (#1213 f404).
    ///
    /// The old wording asserted a fact about the recipient
    /// ("expired without an answer") on a sweep that checked nothing about
    /// connectivity. When our own link was down for any part of the offer's
    /// life the offer never left the machine, and blaming the recipient for
    /// not answering is a social signal the user will act on.
    pub fn offer_expiry_line(&self, sent_at_secs: f64, item: &str, who: &str) -> String {
        // `who` arrives already addressed off the shared ladder (#1218
        // f299) — this must not glue an `@` onto a DID head.
        if self.up_continuously_since(sent_at_secs) {
            format!("Offer of \"{item}\" to {who} expired without an answer.")
        } else {
            format!("Offer of \"{item}\" to {who} never reached them — your connection dropped.")
        }
    }
}

/// The chat line and toast raised when the link drops. One honest sentence
/// about the local client, in place of the per-peer "left the room." lines
/// that told the user everybody walked out on them (#1213 f398 + f402).
pub const LINK_LOST_LINE: &str = "Connection lost — rejoining…";

/// The matching line when the relay welcomes us again. Only ever pushed
/// after [`LINK_LOST_LINE`] was pushed, so a first connect and a portal hop
/// stay quiet.
pub const LINK_RESTORED_LINE: &str = "Reconnected.";

/// Narration bookkeeping. A separate resource from [`LinkState`] so that
/// [`reset_link_state`] can clear the edge and the state together — an edge
/// kept in a `Local` would survive a logout and narrate the next session's
/// first frame as an outage.
#[derive(Resource, Debug, Clone, Default)]
pub struct LinkNarration {
    /// The phase the last narration ran against.
    last: Option<LinkPhase>,
    /// We told the user the link dropped, so the matching "Reconnected."
    /// is owed. Without this a portal hop's reconnect would announce itself.
    loss_announced: bool,
    /// The room URL the socket is pointed at. A change means *we* re-pointed
    /// it, which is what arms `expected_teardown`.
    room_url: Option<String>,
    /// The drop that follows is one we asked for (a portal hop re-pointing
    /// the socket), so it is swept but not narrated.
    expected_teardown: bool,
}

/// May the link be tracked in this app state?
///
/// A named function rather than an inline `in_state(..).or(in_state(..))` so
/// the window is one testable fact instead of a run condition nobody can
/// assert on — the gate being wrong by one state is exactly what #1279 was.
///
/// `Loading` is in the window because that is when the relay actually
/// answers: `install_completed_session` inserts `SymbiosMultiuserConfig`
/// before it sets `Loading`, so the socket opens, the welcome lands, and the
/// world is still compiling. `Login` is out — no session exists there, and
/// leaving it out is what keeps [`reset_link_state`]'s clean slate clean.
pub(crate) fn link_is_tracked(state: Res<State<AppState>>) -> bool {
    matches!(state.get(), AppState::Loading | AppState::InGame)
}

/// The phase, as a pure function of what is true *this frame*.
///
/// Split out from [`track_link_state`] because `MatchboxSocket` wraps a live
/// `WebRtcSocket` and cannot be constructed in a test — the transition table
/// is the part worth pinning, and this is the shape that lets a test pin it.
///
/// There is deliberately no `current` parameter (#1279). The phase used to be
/// latched forward from the previous frame because the welcome arrived as a
/// message that had to be caught in a two-frame window; `peer_id_assigned` is
/// durable, so the answer can be recomputed from scratch every frame and a
/// system that does not run for a while cannot come back holding a stale one.
///
/// Order matters. `open_socket` writes [`LocalSocketReopened`] in the same
/// frame it queues `commands.open_socket(..)`, so the resource is still
/// absent on the frame the reopen fires: checking `socket_present` first
/// would read that frame as `Down` and flicker the chip on every reconnect.
/// `reopened` outranks `peer_id_assigned` for the same reason it outranks
/// everything — it means *this socket is not the one you were connected to*.
/// Upstream cannot in fact raise both at once (every teardown branch of
/// `manage_socket` removes `MatchboxSocket` and returns, so the fresh-open
/// path that writes the reopen only runs on a later frame, with no socket
/// resource present to carry an id), but the ordering states the intent.
pub(crate) fn next_phase(
    socket_present: bool,
    reopened: bool,
    peer_id_assigned: bool,
) -> LinkPhase {
    if reopened {
        // A brand-new socket: whatever we were, we are waiting again.
        LinkPhase::Connecting
    } else if peer_id_assigned {
        // The relay answered *this* socket and named us.
        LinkPhase::Connected
    } else if socket_present {
        // A socket object exists and no welcome has landed on it yet.
        LinkPhase::Connecting
    } else {
        LinkPhase::Down
    }
}

/// Write [`LinkState`] from the upstream signals. The single writer.
///
/// Runs under [`link_is_tracked`], not `in_state(InGame)`: the welcome lands
/// during `Loading` (#1279).
pub(super) fn track_link_state(
    socket: Option<ResMut<MatchboxSocket>>,
    mut reopened: MessageReader<LocalSocketReopened>,
    time: Res<Time>,
    mut link: ResMut<LinkState>,
) {
    // Drained unconditionally: an un-read message would re-fire on the frame
    // after something else moved us.
    let reopened_now = reopened.read().count() > 0;
    // `WebRtcSocket::id` needs `&mut` because it lazily drains the id channel
    // into a `OnceCell` — but only until the cell is filled, after which it
    // answers from the cell. So reading it here cannot steal the signal from
    // upstream's `detect_welcome_handshake`: whichever of us drains the
    // channel first initialises the same cell, and the other still sees
    // `Some`. Nothing in the app or the plugin filters on
    // `Changed<MatchboxSocket>`, so the `ResMut` costs only the flag.
    let (socket_present, peer_id_assigned) = match socket {
        Some(mut socket) => (true, socket.id().is_some()),
        None => (false, false),
    };
    let next = next_phase(socket_present, reopened_now, peer_id_assigned);
    // Guarded-dirty (#879): read through `Res`-style deref (which does not
    // flag), write only on a real transition.
    if link.phase != next {
        link.phase = next;
        link.since_secs = time.elapsed_secs_f64();
    }
}

/// Reset the link state and its narration edge when the session ends.
///
/// Registered on `OnExit(AppState::InGame)`, which covers both logout and
/// the login-screen return. Without it the narration edge would carry
/// `Connected` into the next session and report its first frame — where no
/// socket exists yet — as an outage.
pub(super) fn reset_link_state(mut link: ResMut<LinkState>, mut narration: ResMut<LinkNarration>) {
    *link = LinkState::default();
    *narration = LinkNarration::default();
}

/// Sweep ghost peers and narrate the link's transitions (#1213 f398+f402).
///
/// The sweep is the load-bearing half. Overlands despawned a `RemotePeer` in
/// exactly two places — a `PeerConnectionState::Disconnected` event and
/// logout — and neither covers a local socket teardown: upstream's
/// `poll_peers` takes its early-return branch when the message loop is dead
/// and emits nothing, `open_socket` then removes the socket resource, and
/// the whole peer-polling chain is gated off behind
/// `resource_exists::<MatchboxSocket>`. What was left behind was frozen
/// avatars, an inflated headcount, and live drag-to-gift targets for peers
/// whose channel was long gone. `player::portal` already performs exactly
/// this sweep by hand for a portal hop, with a comment explaining why; this
/// is that sweep, on the edge that actually needs it.
#[allow(clippy::too_many_arguments)]
pub(super) fn narrate_link_state(
    mut commands: Commands,
    link: Res<LinkState>,
    config: Option<Res<SymbiosMultiuserConfig<OverlandsMessage>>>,
    peers: Query<Entity, With<RemotePeer>>,
    mut narration: ResMut<LinkNarration>,
    mut chat: ResMut<ChatHistory>,
    mut toasts: ResMut<crate::ui::toast::Toasts>,
    mut session_log: ResMut<SessionLog>,
    time: Res<Time>,
) {
    // A room change re-points the socket at a new relay URL; the teardown
    // that follows a frame or two later is ours, not an outage. Arm only on
    // a change between two live URLs — the `None -> Some` of the first login
    // would otherwise swallow the session's first genuine drop.
    let current_url = config.as_deref().map(|c| c.room_url.clone());
    if narration.room_url.is_some() && current_url != narration.room_url {
        narration.expected_teardown = true;
    }
    if narration.room_url != current_url {
        narration.room_url = current_url;
    }

    let phase = link.phase();
    let Some(previous) = narration.last else {
        // First run of the session: baseline, never narrate.
        narration.last = Some(phase);
        return;
    };
    if previous == phase {
        return;
    }
    narration.last = Some(phase);
    let now = time.elapsed_secs_f64();
    let expected = std::mem::take(&mut narration.expected_teardown);

    if previous.is_up() && !phase.is_up() {
        // Falling edge. Sweep unconditionally — a ghost is a ghost whether
        // or not we asked for the teardown — and narrate only when the drop
        // is news to the user.
        for entity in &peers {
            // `try_despawn` for the reason portal.rs gives: a parent
            // despawn queued this frame may already have taken a child.
            commands.entity(entity).try_despawn();
        }
        if !expected {
            chat.push(None, "system", LINK_LOST_LINE);
            toasts.warn(LINK_LOST_LINE, now);
            session_log.warn(now, EventPayload::LinkLost);
            narration.loss_announced = true;
        }
    } else if phase.is_up() && narration.loss_announced {
        narration.loss_announced = false;
        chat.push(None, "system", LINK_RESTORED_LINE);
        toasts.success(LINK_RESTORED_LINE, now);
        session_log.info(
            now,
            EventPayload::LinkRestored {
                down_secs: (now - link.since_secs()).max(0.0),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RemotePeer;
    use crate::ui::toast::ToastKind;
    use bevy::ecs::system::RunSystemOnce;

    /// A `PeerId` for a fixture peer. `PeerId` is a newtype over a `Uuid`
    /// from a crate overlands does not depend on directly, and it has no
    /// `Default` — its `Deserialize` is the only constructor reachable from
    /// here, and it is stable because the relay uses the same one.
    fn peer(n: u8) -> PeerId {
        serde_json::from_str(&format!("\"00000000-0000-0000-0000-0000000000{n:02}\""))
            .expect("a well-formed uuid")
    }

    fn remote(n: u8) -> RemotePeer {
        RemotePeer {
            peer_id: peer(n),
            did: Some(format!("did:plc:fixture{n}")),
            handle: Some(format!("peer{n}")),
            muted: false,
            avatar: None,
            build: None,
            connected_at: 0.0,
        }
    }

    /// An app with just the resources the narration reads, and the real
    /// system under test.
    fn narration_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<LinkState>();
        app.init_resource::<LinkNarration>();
        app.init_resource::<ChatHistory>();
        app.init_resource::<crate::ui::toast::Toasts>();
        app.init_resource::<SessionLog>();
        app.add_systems(Update, narrate_link_state);
        app
    }

    fn set_phase(app: &mut App, phase: LinkPhase, at: f64) {
        let mut link = app.world_mut().resource_mut::<LinkState>();
        link.phase = phase;
        link.since_secs = at;
    }

    fn peers_alive(app: &mut App) -> usize {
        let mut q = app.world_mut().query::<&RemotePeer>();
        q.iter(app.world()).count()
    }

    fn chat_lines(app: &mut App) -> Vec<String> {
        app.world()
            .resource::<ChatHistory>()
            .messages
            .iter()
            .map(|e| e.text.clone())
            .collect()
    }

    fn config_for(url: &str) -> SymbiosMultiuserConfig<OverlandsMessage> {
        SymbiosMultiuserConfig {
            room_url: url.to_owned(),
            ice_servers: None,
            _marker: std::marker::PhantomData,
        }
    }

    /// The reopen edge outranks the socket's absence — `open_socket` writes
    /// `LocalSocketReopened` in the frame it *queues* the socket, so the
    /// resource is still gone when the message arrives. Reading presence
    /// first would render that frame "Offline" and flicker the chip on
    /// every reconnect.
    #[test]
    fn a_reopen_outranks_the_socket_not_being_there_yet() {
        assert_eq!(
            next_phase(false, true, false),
            LinkPhase::Connecting,
            "the frame a socket is queued is Connecting, not Down"
        );
        // …and it outranks an assigned id too: that id would belong to the
        // socket being replaced, not to the one just queued.
        assert_eq!(next_phase(true, true, true), LinkPhase::Connecting);
    }

    /// An assigned `PeerId` is what "connected" means, and it is the only
    /// thing that means it.
    #[test]
    fn an_assigned_peer_id_is_what_connected_means() {
        assert_eq!(next_phase(true, false, true), LinkPhase::Connected);
        // The two ways to not be connected stay distinguishable.
        assert_eq!(next_phase(true, false, false), LinkPhase::Connecting);
        assert_eq!(next_phase(false, false, false), LinkPhase::Down);
    }

    /// The whole point of the resource: the socket resource vanishing is
    /// the ONLY trace a dead message loop leaves, and it must read as Down.
    #[test]
    fn a_removed_socket_is_down() {
        assert_eq!(next_phase(false, false, false), LinkPhase::Down);
    }

    /// A live socket with no welcome yet is not "connected". This is the
    /// gap the old code had no name for: the socket object exists, the UI
    /// showed a normal room, and every send was dropped.
    #[test]
    fn a_socket_without_a_welcome_is_only_connecting() {
        assert_eq!(next_phase(true, false, false), LinkPhase::Connecting);
    }

    /// THE MECHANISM behind #1279: the phase carries no history, so a
    /// tracker that was not allowed to run for a hundred frames comes back
    /// with the right answer rather than a stale one.
    ///
    /// The old table took the previous phase and latched `Connected`
    /// forward, because the welcome arrived as a message that had to be
    /// caught inside a two-frame window. That is what made a single missed
    /// read permanent. `next_phase` now has no parameter that could carry a
    /// missed edge: for one set of live facts there is exactly one answer,
    /// whatever happened before.
    #[test]
    fn the_phase_is_derived_not_latched() {
        // A socket that has been welcomed reads Connected on the FIRST
        // frame it is ever asked about — no prior Connected required.
        assert_eq!(next_phase(true, false, true), LinkPhase::Connected);
        // And a socket that has not been reads Connecting even if the
        // caller has been Connected all session: nothing latches.
        assert_eq!(next_phase(true, false, false), LinkPhase::Connecting);
    }

    /// `up_continuously_since` is the predicate the gift-expiry wording
    /// turns on, and the case it exists for is the reconnect: the link is
    /// up *now*, but it came up after the offer was sent, so the offer
    /// still never left the machine.
    #[test]
    fn a_link_that_came_back_after_the_offer_did_not_carry_it() {
        let up_all_along = LinkState {
            phase: LinkPhase::Connected,
            since_secs: 10.0,
        };
        let up_again_since = LinkState {
            phase: LinkPhase::Connected,
            since_secs: 40.0,
        };
        let down = LinkState {
            phase: LinkPhase::Down,
            since_secs: 40.0,
        };
        assert!(up_all_along.up_continuously_since(20.0));
        assert!(!up_again_since.up_continuously_since(20.0));
        assert!(!down.up_continuously_since(20.0));

        assert_eq!(
            up_all_along.offer_expiry_line(20.0, "Lamp", "@alice"),
            "Offer of \"Lamp\" to @alice expired without an answer.",
        );
        assert_eq!(
            up_again_since.offer_expiry_line(20.0, "Lamp", "@alice"),
            "Offer of \"Lamp\" to @alice never reached them — your connection dropped.",
        );
    }

    /// The three chat outcomes are distinguishable, and only the delivered
    /// one is silent. "Alone in the room" and "not connected" are different
    /// facts and must not share a sentence.
    #[test]
    fn the_three_chat_outcomes_read_differently() {
        let up = LinkState {
            phase: LinkPhase::Connected,
            since_secs: 0.0,
        };
        let down = LinkState {
            phase: LinkPhase::Down,
            since_secs: 0.0,
        };
        assert_eq!(up.delivery(2), ChatDelivery::Reached(2));
        assert_eq!(up.delivery(0), ChatDelivery::NobodyHere);
        assert_eq!(down.delivery(0), ChatDelivery::NotConnected);
        // A stale peer entity must not make an offline send look delivered.
        assert_eq!(down.delivery(3), ChatDelivery::NotConnected);

        assert!(up.composer_note(2).is_none());
        assert_ne!(up.composer_note(0), down.composer_note(0));
        assert!(up.composer_note(0).is_some());
        assert!(down.composer_note(0).is_some());

        assert!(ChatDelivery::Reached(1).suffix().is_none());
        assert!(ChatDelivery::NotApplicable.suffix().is_none());
        assert!(ChatDelivery::NobodyHere.suffix().is_some());
        assert!(ChatDelivery::NotConnected.suffix().is_some());
        assert!(ChatDelivery::Reached(1).reached_anyone());
        assert!(!ChatDelivery::Reached(0).reached_anyone());
        assert!(!ChatDelivery::NotConnected.reached_anyone());
    }

    /// The People window may only claim "In room (N)" / "(no other peers)"
    /// when the client can actually observe the room. Both lines were
    /// assertions about the ROOM made from a client that could see nothing.
    #[test]
    fn the_roster_only_claims_a_headcount_when_it_can_see_one() {
        assert_eq!(LinkPhase::Connected.roster_header(4), "In room (4)");
        for phase in [LinkPhase::Down, LinkPhase::Connecting] {
            let header = phase.roster_header(4);
            assert!(
                !header.contains('4'),
                "a client that cannot see the room must not print a headcount: {header}"
            );
        }
        assert_eq!(LinkPhase::Connected.roster_empty_note(), "(no other peers)");
        assert_ne!(
            LinkPhase::Down.roster_empty_note(),
            LinkPhase::Connected.roster_empty_note(),
        );
    }

    /// Every phase has a distinct chip label and a distinct sentence, and
    /// only `Connected` reads as up — so the chip can never be the thing
    /// that says "fine" during an outage.
    #[test]
    fn every_phase_names_itself() {
        let all = [LinkPhase::Down, LinkPhase::Connecting, LinkPhase::Connected];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a.chip_label(), b.chip_label());
                assert_ne!(a.sentence(), b.sentence());
            }
        }
        assert!(LinkPhase::Connected.is_up());
        assert!(!LinkPhase::Connecting.is_up());
        assert!(!LinkPhase::Down.is_up());
        assert_eq!(LinkPhase::default(), LinkPhase::Down);
    }
    /// THE SEQUENCE: welcomed, three peers in the room, the message loop
    /// dies and the plugin removes the socket. Before #1213 nothing
    /// despawned a `RemotePeer` on a LOCAL teardown — upstream's
    /// `poll_peers` early-returns without emitting `Disconnected` when the
    /// loop is dead — so three frozen ghosts stayed in the room, People
    /// kept counting them, and each was still a live drag-to-gift target.
    #[test]
    fn a_local_teardown_sweeps_the_ghost_peers_and_says_so_once() {
        let mut app = narration_app();
        for n in 1..=3 {
            app.world_mut().spawn(remote(n));
        }
        set_phase(&mut app, LinkPhase::Connected, 1.0);
        app.update(); // baseline the narration edge on the connected state
        assert_eq!(peers_alive(&mut app), 3);
        assert!(chat_lines(&mut app).is_empty());

        set_phase(&mut app, LinkPhase::Down, 2.0);
        app.update();

        assert_eq!(peers_alive(&mut app), 0, "ghost peers must be swept");
        assert_eq!(
            chat_lines(&mut app),
            vec![LINK_LOST_LINE.to_owned()],
            "one honest line about US, not three about the peers"
        );
        let toasts = app.world().resource::<crate::ui::toast::Toasts>().shown();
        assert_eq!(toasts.len(), 1);
        assert_eq!(toasts[0].0, ToastKind::Warn);
        assert!(
            app.world()
                .resource::<SessionLog>()
                .iter()
                .any(|e| matches!(e.payload, EventPayload::LinkLost)),
            "a local outage used to leave no trace in the session log at all"
        );
    }

    /// THE SEQUENCE: the link drops, is announced, and comes back. The
    /// recovery half was entirely mute before #1213 — upstream reopens the
    /// socket and is welcomed again on every reconnect, and overlands
    /// observed neither — so even a user who worked out what had happened
    /// was never told the client had fixed it.
    #[test]
    fn a_reconnect_is_announced_after_a_loss_was() {
        let mut app = narration_app();
        set_phase(&mut app, LinkPhase::Connected, 1.0);
        app.update();
        set_phase(&mut app, LinkPhase::Down, 2.0);
        app.update();
        set_phase(&mut app, LinkPhase::Connecting, 3.0);
        app.update();
        set_phase(&mut app, LinkPhase::Connected, 4.0);
        app.update();

        assert_eq!(
            chat_lines(&mut app),
            vec![LINK_LOST_LINE.to_owned(), LINK_RESTORED_LINE.to_owned()],
        );
        assert!(
            app.world()
                .resource::<SessionLog>()
                .iter()
                .any(|e| matches!(e.payload, EventPayload::LinkRestored { .. })),
        );
        // A second flap gets its own matched pair, and the narration
        // strictly alternates — a "Reconnected." can only ever follow a
        // "Connection lost", never another "Reconnected.".
        set_phase(&mut app, LinkPhase::Connecting, 5.0);
        app.update();
        set_phase(&mut app, LinkPhase::Connected, 6.0);
        app.update();
        assert_eq!(
            chat_lines(&mut app),
            vec![
                LINK_LOST_LINE.to_owned(),
                LINK_RESTORED_LINE.to_owned(),
                LINK_LOST_LINE.to_owned(),
                LINK_RESTORED_LINE.to_owned(),
            ],
        );
    }

    /// THE SEQUENCE: the session's first connect. `Down -> Connecting ->
    /// Connected` is what every login does, and it must be silent — a
    /// "Reconnected." on the first frame in a world would be a lie about a
    /// drop that never happened.
    #[test]
    fn the_first_connect_of_a_session_is_silent() {
        let mut app = narration_app();
        app.update();
        set_phase(&mut app, LinkPhase::Connecting, 1.0);
        app.update();
        set_phase(&mut app, LinkPhase::Connected, 2.0);
        app.update();
        assert!(chat_lines(&mut app).is_empty());
        assert!(
            app.world()
                .resource::<crate::ui::toast::Toasts>()
                .shown()
                .is_empty()
        );
    }

    /// THE SEQUENCE: a portal hop. `player::portal` re-points the socket at
    /// the destination's room URL and sweeps the peers itself; the teardown
    /// that follows is one we asked for, so it is swept but never narrated
    /// — the user who just walked through a portal must not be told their
    /// connection dropped.
    #[test]
    fn a_room_change_is_swept_but_not_narrated() {
        let mut app = narration_app();
        app.insert_resource(config_for("wss://relay/overlands/did:plc:home"));
        app.world_mut().spawn(remote(1));
        set_phase(&mut app, LinkPhase::Connected, 1.0);
        app.update();

        app.insert_resource(config_for("wss://relay/overlands/did:plc:elsewhere"));
        app.update();
        set_phase(&mut app, LinkPhase::Down, 2.0);
        app.update();

        assert_eq!(peers_alive(&mut app), 0, "a ghost is a ghost either way");
        assert!(
            chat_lines(&mut app).is_empty(),
            "travel must not read as an outage: {:?}",
            chat_lines(&mut app)
        );
        // …and the reconnect that follows it is equally quiet, because no
        // loss was announced to pair with.
        set_phase(&mut app, LinkPhase::Connected, 3.0);
        app.update();
        assert!(chat_lines(&mut app).is_empty());
    }

    /// THE SEQUENCE: log in, connect, log out, log back in. The narration
    /// edge has to die with the session — carried in a `Local` it would
    /// survive, and the next login's first frame (no socket yet) would
    /// narrate itself as an outage.
    #[test]
    fn a_logout_clears_the_state_and_the_edge_together() {
        let mut app = narration_app();
        set_phase(&mut app, LinkPhase::Connected, 1.0);
        app.update();

        app.world_mut()
            .run_system_once(reset_link_state)
            .expect("reset runs");
        assert_eq!(app.world().resource::<LinkState>().phase(), LinkPhase::Down);

        // The next session starts at Down with no baseline, so its first
        // observed phase is a baseline and not a transition.
        app.update();
        set_phase(&mut app, LinkPhase::Connecting, 2.0);
        app.update();
        assert!(chat_lines(&mut app).is_empty());
    }

    /// The resource is written on an edge only (#879). A `LinkState` that
    /// re-wrote itself every frame would mark itself changed every frame and
    /// starve anything watching it — the exact shape of the prefs-debounce
    /// bug the guarded-dirty rule exists for.
    #[test]
    fn the_link_state_is_written_only_on_an_edge() {
        #[derive(Resource, Default)]
        struct Changes(usize);
        fn count(link: Res<LinkState>, mut changes: ResMut<Changes>) {
            if link.is_changed() {
                changes.0 += 1;
            }
        }

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<LinkState>();
        app.init_resource::<Changes>();
        app.add_message::<LocalSocketReopened>();
        app.add_systems(Update, (track_link_state, count).chain());

        // Steady Down: no socket, no messages, nothing to say.
        for _ in 0..5 {
            app.update();
        }
        let baseline = app.world().resource::<Changes>().0;
        assert!(
            baseline <= 1,
            "a steady phase must not re-write the resource, saw {baseline} writes"
        );

        // One edge, then steady again. `LocalSocketReopened` is the only
        // signal a socketless test app can raise — and it is the only one
        // left that is a message at all.
        app.world_mut()
            .resource_mut::<Messages<LocalSocketReopened>>()
            .write(LocalSocketReopened);
        app.update();
        assert_eq!(
            app.world().resource::<LinkState>().phase(),
            LinkPhase::Connecting
        );
        let after_edge = app.world().resource::<Changes>().0;
        assert_eq!(after_edge, baseline + 1, "the edge is exactly one write");

        // With the socket resource absent the very next frame reads Down —
        // which is the real teardown signal, and the one write it earns.
        app.update();
        assert_eq!(app.world().resource::<LinkState>().phase(), LinkPhase::Down);
        app.update();
        app.update();
        assert_eq!(
            app.world().resource::<Changes>().0,
            after_edge + 1,
            "settling back to Down is one write, not one per frame"
        );
    }

    /// An app with the real tracker under its real run condition and a real
    /// `AppState`. `MatchboxSocket` cannot be built here, so the socket half
    /// of the signals is exercised by [`next_phase`] in the sequence test
    /// below; what THIS app can prove is the half #1279 actually got wrong
    /// — which states the tracker is allowed to observe at all.
    fn gated_tracker_app() -> App {
        let mut app = App::new();
        // `MinimalPlugins` carries no `StatesPlugin`, and `init_state`
        // panics without the `StateTransition` schedule it installs.
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin));
        app.init_state::<AppState>();
        app.init_resource::<LinkState>();
        app.add_message::<LocalSocketReopened>();
        app.add_systems(Update, track_link_state.run_if(link_is_tracked));
        app
    }

    fn enter(app: &mut App, state: AppState) {
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(state);
        // One update to apply the transition, one to run in the new state.
        app.update();
        app.update();
    }

    /// THE SEQUENCE, first half (#1279): the relay answers while the world
    /// is still compiling, and the tracker has to be watching by then.
    ///
    /// `install_completed_session` inserts `SymbiosMultiuserConfig` and only
    /// THEN sets `Loading`, so the socket opens and the welcome lands during
    /// `Loading` — 0.65 s and 1.9 s before `InGame` in the two login
    /// segments of the session log that reported this. Registered
    /// `run_if(in_state(InGame))`, the tracker was not running yet and the
    /// phase stayed `Down`; because the welcome was a message written once
    /// per socket, it never got another chance.
    #[test]
    fn the_tracker_is_watching_from_loading_not_from_in_game() {
        assert!(!link_is_tracked_in(AppState::Login));
        assert!(link_is_tracked_in(AppState::Loading));
        assert!(link_is_tracked_in(AppState::InGame));

        let mut app = gated_tracker_app();

        // Login: no session, nothing to track — the tracker must not run.
        app.world_mut()
            .resource_mut::<Messages<LocalSocketReopened>>()
            .write(LocalSocketReopened);
        app.update();
        assert_eq!(
            app.world().resource::<LinkState>().phase(),
            LinkPhase::Down,
            "the login screen has no link to report on"
        );

        // Loading: the socket is opening. THIS is the window the old gate
        // missed, and a signal raised here must move the phase.
        enter(&mut app, AppState::Loading);
        app.world_mut()
            .resource_mut::<Messages<LocalSocketReopened>>()
            .write(LocalSocketReopened);
        app.update();
        assert_eq!(
            app.world().resource::<LinkState>().phase(),
            LinkPhase::Connecting,
            "a relay signal during Loading must be observed, not dropped"
        );
    }

    /// `link_is_tracked` as a plain predicate — the run condition is a
    /// system, so this is the shape a test can assert on.
    fn link_is_tracked_in(state: AppState) -> bool {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin));
        app.init_state::<AppState>();
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(state);
        app.update();
        app.world_mut()
            .run_system_once(link_is_tracked)
            .expect("the condition runs")
    }

    /// THE SEQUENCE, in full (#1279): the relay welcomes us during
    /// `Loading`, and the phase is `Connected` by the time the player is in
    /// the world — and stays that way, for as many frames as the socket
    /// lives.
    ///
    /// This is the whole bug in one replay. Each step is the live facts of
    /// one frame — the app state, whether the socket resource exists,
    /// whether a reopen fired, and whether the relay has assigned our
    /// `PeerId` — and the assertion is what a user reading the toolbar chip
    /// would see. The old code failed the `InGame` steps: the welcome was a
    /// message, `next_phase` could only reach `Connected` through it, and
    /// the frames that could have read it were frames the tracker was gated
    /// out of.
    #[test]
    fn the_relay_welcomes_us_during_loading_and_we_are_connected_in_the_world() {
        // (state, socket_present, reopened, peer_id_assigned, expected)
        let login = [
            // Sign-in complete: the config is inserted, `Loading` begins,
            // and upstream queues the socket in the same frame it writes
            // the reopen — the resource is not there yet.
            (AppState::Loading, false, true, false, LinkPhase::Connecting),
            // The socket resource lands. Still no answer from the relay.
            (AppState::Loading, true, false, false, LinkPhase::Connecting),
            // ~1.5 s of world compile with the handshake in flight.
            (AppState::Loading, true, false, false, LinkPhase::Connecting),
            // The relay welcomes us and names us. STILL IN `Loading` — this
            // is the frame the old tracker was not running for, 0.65 s
            // before the loading gate opened.
            (AppState::Loading, true, false, true, LinkPhase::Connected),
            (AppState::Loading, true, false, true, LinkPhase::Connected),
            // The loading gate opens. Nothing about the link changed here,
            // and nothing needs to have been remembered across the edge.
            (AppState::InGame, true, false, true, LinkPhase::Connected),
            (AppState::InGame, true, false, true, LinkPhase::Connected),
        ];
        for (i, (state, present, reopened, assigned, want)) in login.iter().enumerate() {
            assert!(
                link_is_tracked_in(state.clone()),
                "step {i}: the tracker must be running in {state:?}"
            );
            assert_eq!(
                next_phase(*present, *reopened, *assigned),
                *want,
                "step {i} of the login sequence"
            );
        }

        // A thousand frames later — long past any message's two-frame life
        // — the same live facts still read Connected. The chip that was
        // stuck on "Connecting…" for a whole session is the assertion this
        // one replaces.
        assert_eq!(next_phase(true, false, true), LinkPhase::Connected);
        assert_eq!(LinkPhase::Connected.chip_label(), "Connected");
    }

    /// THE SEQUENCE, repeated (#1279): log out and log in again.
    ///
    /// `reset_link_state` runs `OnExit(InGame)`, so the second login starts
    /// from a cleared `LinkState` exactly as the first did — which is why
    /// the reported bug happened TWICE in one session log rather than once.
    /// A fix that only worked on the first login would leave the second
    /// stuck, so the relogin is asserted end to end.
    #[test]
    fn a_relogin_reconnects_and_says_so() {
        let mut app = gated_tracker_app();
        app.init_resource::<LinkNarration>();

        // First session: reach Connecting under the real gate, then the
        // welcome (which needs a socket) via the table.
        enter(&mut app, AppState::Loading);
        app.world_mut()
            .resource_mut::<Messages<LocalSocketReopened>>()
            .write(LocalSocketReopened);
        app.update();
        assert_eq!(
            app.world().resource::<LinkState>().phase(),
            LinkPhase::Connecting
        );
        enter(&mut app, AppState::InGame);

        // Log out: the reset clears the state and the narration edge.
        app.world_mut()
            .run_system_once(reset_link_state)
            .expect("reset runs");
        assert_eq!(app.world().resource::<LinkState>().phase(), LinkPhase::Down);
        enter(&mut app, AppState::Login);

        // Second login. The gate must open again — a fix that latched
        // anything into the first session would leave this one at Down.
        enter(&mut app, AppState::Loading);
        app.world_mut()
            .resource_mut::<Messages<LocalSocketReopened>>()
            .write(LocalSocketReopened);
        app.update();
        assert_eq!(
            app.world().resource::<LinkState>().phase(),
            LinkPhase::Connecting,
            "the second login must be tracked exactly like the first"
        );
        // And the durable welcome is just as reachable the second time:
        // nothing about it is consumed by the first session.
        assert_eq!(next_phase(true, false, true), LinkPhase::Connected);
    }
}
