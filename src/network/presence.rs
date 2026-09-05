//! How well a peer has resolved, said once (#1217/#1218).
//!
//! A peer becomes a person in stages — a relay session binds their DID, a
//! `getProfile` call resolves their name and picture, a PDS fetch returns
//! their avatar record, a wardrobe resolve fetches what they wear, and an
//! async build turns all of it into a body. Every one of those stages could
//! fail silently, and each failure had its own (absent) surface: a failed
//! avatar fetch rendered as a plausible stranger, a failed profile fetch
//! left a permanent "identifying…", a partial wardrobe resolve dropped worn
//! items per-viewer, and a peer that never identified was counted in the
//! room while being invisible in it.
//!
//! This module is the one answer to "how is this person loading":
//!
//! * [`PeerLabel`] — the ONE naming ladder (handle → DID head → traveler).
//!   The departure line had it; the arrival line, the chat author and the
//!   gift modal each had their own worse version. Four surfaces, one name.
//! * [`PeerResolve`] — the per-peer component every stage writes its
//!   outcome into, and [`peer_status`] — the single derived
//!   [`PeerStatus`] the roster renders as ONE chip beside the existing
//!   `⚠ build` chip. Six failures, one indicator.
//! * [`RetryBackoff`] — the doubling retry the avatar-record fetch, the
//!   profile fetch and the relationship query all wait on, sharing the
//!   arithmetic ([`next_wait_secs`]) with
//!   [`super::peer_cache::PeerRigResolveBackoff`], which had it first.
//! * The presence placeholder: a translucent stand-in body drawn at the
//!   peer's playout pose so an arrival is visible from its first transform
//!   packet rather than from the end of the whole chain — and, on the other
//!   side of the same flag, the reason a peer is no longer drawn at the map
//!   centre ten metres up before any pose has played out.
//!
//! [`super::peer_cache::PeerRigResolveBackoff`]: super::peer_cache

use bevy::prelude::*;
use bevy_symbios_multiuser::prelude::*;

use crate::config;
use crate::state::{MutedDids, RemotePeer};

// ---------------------------------------------------------------------------
// The naming ladder
// ---------------------------------------------------------------------------

/// The best name we can print for a peer right now (#1218 f338/f299/f300).
///
/// Built by [`peer_label`] and nothing else. The ladder existed once, inline
/// in the departure line, and every other surface that needed a name either
/// re-derived a worse one (`"identifying…"`, a raw `PeerId` UUID) or printed
/// a `did:plc:` string inside an `@`-prefixed sentence. Making it a type is
/// what stops the four surfaces drifting apart again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeerLabel {
    /// A handle resolved from the authenticated DID's own profile record.
    /// The only tier that is a *name*; the peer-supplied handle on the wire
    /// never reaches here (see the `Identity` arm in [`super::inbound`]).
    Handle(String),
    /// No handle yet, but the relay authenticated a DID: its head, already
    /// elided by [`elide_did`] — ellipsis included, and included only when
    /// characters were really dropped.
    DidHead(String),
    /// Neither — a peer that connected and never identified.
    Anonymous,
}

/// How many leading characters of a DID stand in for a name. `did:plc:` is
/// eight of them, so this leaves eight characters of the identifier — enough
/// to tell two strangers apart in a chat log without pretending to be a name.
const DID_HEAD_CHARS: usize = 16;

/// The DID standing in for a name: its first [`DID_HEAD_CHARS`]
/// characters, with an ellipsis **only when characters were really
/// dropped**.
///
/// The unconditional `format!("{head}…")` this replaces was harmless for
/// every real `did:plc:` (32 characters) but wrong for anything shorter,
/// and an ellipsis that elides nothing invites the reader to go looking
/// for the rest of an identifier they already have in full. Folded in here
/// (#1231 f27) as `ui::travel`'s fifth copy of the ladder was retired onto
/// this one.
fn elide_did(did: &str) -> String {
    let head: String = did.chars().take(DID_HEAD_CHARS).collect();
    if head.chars().count() == did.chars().count() {
        head
    } else {
        format!("{head}…")
    }
}

impl PeerLabel {
    /// The ladder. `handle` is the profile-verified handle, `did` the
    /// relay-authenticated DID; both `None` is a peer that never identified.
    pub fn new(handle: Option<&str>, did: Option<&str>) -> Self {
        match (handle, did) {
            (Some(handle), _) => Self::Handle(handle.to_owned()),
            (None, Some(did)) => Self::DidHead(elide_did(did)),
            (None, None) => Self::Anonymous,
        }
    }

    /// The form used when the name is the subject of a sentence — a roster
    /// row, a presence line, a gift salutation.
    ///
    /// The `@` sigil is attached to the [`Handle`](Self::Handle) tier ONLY.
    /// `@did:plc:z72i7hdynmk6…` was the exact defect in the gift modal
    /// (#1218 f299): the sigil promises a name and the string underneath it
    /// is an identifier.
    pub fn addressed(&self) -> String {
        match self {
            Self::Handle(handle) => format!("@{handle}"),
            Self::DidHead(head) => head.clone(),
            Self::Anonymous => String::from("A traveler"),
        }
    }

    /// The bare form, for surfaces that supply their own decoration — the
    /// chat author tag renders `[{name}]`, so it must not be handed an `@`.
    pub fn name(&self) -> String {
        match self {
            Self::Handle(handle) => handle.clone(),
            Self::DidHead(head) => head.clone(),
            Self::Anonymous => String::from("a traveler"),
        }
    }

    /// The stable ordering key for a list of peers (#1226 f325).
    ///
    /// The roster sorted on `handle.unwrap_or("~")`, which put every
    /// handle-less peer under one key: two strangers who had not resolved
    /// were adjacent, interchangeable and — because a bare query iteration
    /// re-orders whenever a component lands — liable to swap places between
    /// frames, under a pointer aiming a durable mute. The label ladder is
    /// already what the row *renders*, so ordering on the same ladder is
    /// what makes the list agree with itself: named people first in
    /// alphabetical order, then identified strangers by DID head, then
    /// whoever never identified.
    ///
    /// The tiers are separated rather than folded into one string because a
    /// DID head begins with `did:plc:` for everyone: sorted together with
    /// handles, every stranger would bunch under `d`.
    pub fn sort_key(&self) -> (u8, String) {
        match self {
            Self::Handle(handle) => (0, handle.to_lowercase()),
            Self::DidHead(head) => (1, head.to_lowercase()),
            Self::Anonymous => (2, String::new()),
        }
    }

    /// Whether this is a real, profile-verified name. Surfaces that ask the
    /// user to *judge* the peer — the gift modal above all — must say "we
    /// don't know who this is" rather than print an identifier that reads
    /// like one.
    pub fn is_named(&self) -> bool {
        matches!(self, Self::Handle(_))
    }
}

/// [`PeerLabel::new`] for a live peer component.
pub fn peer_label(peer: &RemotePeer) -> PeerLabel {
    PeerLabel::new(peer.handle.as_deref(), peer.did.as_deref())
}

/// Who a chat message is attributed to, or `None` to drop it (#1218 f290).
///
/// `session_did` is the relay-signed DID from `PeerSessionMapRes`, and it is
/// the whole authentication: the `Chat` arm was, uniquely among the inbound
/// arms, unauthenticated, and fell back to `msg.sender.to_string()` — a raw
/// `PeerId` UUID — as the author name. That gave a peer who never identified
/// an author name, a chat channel, and a mute that could not be made
/// durable, because a mute is remembered against an account. Saying nothing
/// was the cheapest griefing posture in the product.
///
/// Returns the DID to stamp on the entry (which is what makes the row's
/// profile icon and mutual-★ work) and the name to print.
pub fn chat_attribution(
    session_did: Option<&str>,
    peer_handle: Option<&str>,
) -> Option<(String, String)> {
    let did = session_did?;
    Some((
        did.to_owned(),
        PeerLabel::new(peer_handle, Some(did)).name(),
    ))
}

/// Whether a peer's departure should be narrated in chat (#1218 f338,
/// #1219 f289).
///
/// The presence log has to balance. Three facts gate it, and none is about
/// the peer's name:
///
/// * `announced` — a farewell to somebody the room was never told had
///   arrived is worse than silence, and it was the one user-visible trace a
///   nameless peer ever left. The arrival side is
///   [`crate::avatar`]'s `announce_arrival`.
/// * `link_is_up` — a departure observed while OUR link is down is not
///   attributable to the peer (#1213 f402); `link::narrate_link_state`
///   replaces the whole run of them with one line about the real actor.
/// * `!muted` — presence lines are chat rows carrying a name, and they were
///   the one channel a blocked person retained to put theirs in front of the
///   user who blocked them. A reconnect loop scrolled every real message out
///   of a 500-entry history.
pub fn should_announce_departure(link_is_up: bool, announced: bool, muted: bool) -> bool {
    link_is_up && announced && !muted
}

// ---------------------------------------------------------------------------
// The mute funnel
// ---------------------------------------------------------------------------

/// Apply a mute decision everywhere it has to land (#1219).
///
/// The ONE mute write. Two controls reach it — the People roster's checkbox
/// and the offer dialog's "Mute & Decline" — and the review's own
/// requirement is that they cannot drift.
///
/// Two things are deliberate here:
///
/// * the durable, DID-keyed write is UNCONDITIONAL on the peer entity still
///   existing (#1219 f120). "Mute & Decline" used to write it from inside a
///   loop over live peers, so a stranger who spammed a gift and disconnected
///   — the hit-and-run case the durable list exists for — was never
///   recorded, and their next visit reached the user exactly as before. The
///   dialog's sender DID is relay-authenticated, so it is always safe to key
///   on.
/// * the live flag is written through a change guard, because
///   `Changed<RemotePeer>` drives
///   [`super::lifecycle::dismiss_offer_dialog_from_muted_sender`] and the
///   rigged-build kicker.
///
/// Returns whether anything actually changed.
pub fn set_peer_mute(
    peer: Option<&mut RemotePeer>,
    did: Option<&str>,
    muted: bool,
    muted_dids: &mut MutedDids,
    session_log: &mut crate::diagnostics::SessionLog,
    peer_id: Option<PeerId>,
    now: f64,
) -> bool {
    let flag_moved = peer.is_some_and(|peer| {
        let moved = peer.muted != muted;
        if moved {
            peer.muted = muted;
        }
        moved
    });
    let list_moved = did.is_some_and(|did| {
        let moved = muted_dids.0.contains(did) != muted;
        if moved {
            muted_dids.set(did, muted);
        }
        moved
    });
    if flag_moved || list_moved {
        // The most identifying thing we have, and the DID outranks the
        // PeerId: a PeerId is a session-scoped UUID, while the mute list
        // this event describes is keyed by account. The Settings list
        // (#1223 f292) unmutes people who are not in the room at all and has
        // no PeerId to offer.
        let subject = did
            .map(str::to_owned)
            .or_else(|| peer_id.map(|id| id.to_string()))
            .unwrap_or_else(|| String::from("unknown"));
        log_peer_mute_toggled(session_log, now, subject, muted);
    }
    flag_moved || list_moved
}

/// Log a `PeerMuteToggled` event (#635b). Called only from
/// [`set_peer_mute`], which is what keeps the event's shape from drifting
/// between the two mute controls — it used to be called from each of them.
fn log_peer_mute_toggled(
    session_log: &mut crate::diagnostics::SessionLog,
    now: f64,
    peer: String,
    muted: bool,
) {
    session_log.info(
        now,
        crate::diagnostics::event::EventPayload::PeerMuteToggled { peer, muted },
    );
}

// ---------------------------------------------------------------------------
// The shared doubling backoff
// ---------------------------------------------------------------------------

/// The doubling arithmetic behind every peer-side retry.
///
/// Extracted from [`super::peer_cache::PeerRigResolveBackoff`], which
/// implemented it first for the wardrobe fan-out (#1113) and is still the
/// only user that keys its wait by *reference set* rather than by peer. The
/// avatar-record fetch, the profile fetch and the relationship query all
/// wait on the same shape via [`RetryBackoff`]; keeping the sum in one
/// function is what keeps "doubling from `base`, capped at `max`" from
/// meaning three different things.
///
/// [`super::peer_cache::PeerRigResolveBackoff`]: super::peer_cache
pub fn next_wait_secs(previous: Option<f64>, base: f64, max: f64) -> f64 {
    match previous {
        None => base,
        Some(previous) => (previous * 2.0).min(max),
    }
}

/// A failed fetch and when it may be tried again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetryBackoff {
    /// How many times this fetch has failed in a row. Rendered nowhere; kept
    /// because a status chip that could not say "retrying" from "gave up"
    /// would be the same silent failure one layer up.
    pub attempts: u32,
    /// Seconds to wait from [`Self::failed_at`].
    pub wait_secs: f64,
    /// `Time::elapsed_secs_f64` at the failure.
    pub failed_at: f64,
}

impl RetryBackoff {
    /// The backoff to record after a failure, doubling whatever the previous
    /// one waited.
    pub fn after_failure(previous: Option<&Self>, now: f64) -> Self {
        Self::after_failure_with(
            previous,
            now,
            config::network::PEER_FETCH_RETRY_BASE_SECS,
            config::network::PEER_FETCH_RETRY_MAX_SECS,
        )
    }

    /// [`Self::after_failure`] against a caller's own two numbers.
    ///
    /// The peer-side fetches share one pair; the asset fetches (#1247) wait
    /// on the same doubling with theirs, because what is being protected is
    /// a stranger's host rather than a stranger's PDS. Keeping the shape
    /// here rather than copying six lines is what stops a fifth backoff
    /// from meaning a fifth thing.
    pub fn after_failure_with(previous: Option<&Self>, now: f64, base: f64, max: f64) -> Self {
        Self {
            attempts: previous.map_or(1, |b| b.attempts.saturating_add(1)),
            wait_secs: next_wait_secs(previous.map(|b| b.wait_secs), base, max),
            failed_at: now,
        }
    }

    /// Whether the wait has elapsed at `now`.
    pub fn ready(&self, now: f64) -> bool {
        now - self.failed_at >= self.wait_secs
    }
}

/// One fetch's outcome for one peer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum FetchState {
    /// Never attempted, or an attempt is in flight.
    #[default]
    Pending,
    /// Came back with a real answer.
    Landed,
    /// Failed, and is waiting out [`RetryBackoff`] before the next attempt.
    Failed(RetryBackoff),
}

impl FetchState {
    /// Whether a retry is due at `now`.
    pub fn retry_due(&self, now: f64) -> bool {
        matches!(self, Self::Failed(backoff) if backoff.ready(now))
    }

    /// The backoff this state is waiting out, if any — the input to the next
    /// [`RetryBackoff::after_failure`] so the doubling continues rather than
    /// restarting at the base on every attempt.
    pub fn backoff(&self) -> Option<&RetryBackoff> {
        match self {
            Self::Failed(backoff) => Some(backoff),
            _ => None,
        }
    }

    /// Whether this fetch has failed and not yet been recovered.
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }
}

// ---------------------------------------------------------------------------
// The per-peer resolution record
// ---------------------------------------------------------------------------

/// How far along each stage of "becoming a person" this peer is (#1217).
///
/// A component and not fields on [`RemotePeer`] on purpose. `RemotePeer` is
/// read through `Changed<RemotePeer>` by
/// [`super::lifecycle::dismiss_offer_dialog_from_muted_sender`] and by the
/// rigged-build kicker, and per-frame facts (the playout flag below) written
/// through a `Mut<RemotePeer>` would raise that flag every frame and destroy
/// both — the guarded-dirty rule (#879) applied to a component rather than a
/// resource.
#[derive(Component, Default, Debug)]
pub struct PeerResolve {
    /// The peer's `AvatarRecord`, fetched from their PDS. `Failed` means the
    /// body standing under them is a DID-seeded stand-in, not what they
    /// published — the distinction the roster used to be unable to draw
    /// against a peer who simply has not published one (#1217 f323).
    pub avatar: FetchState,
    /// The peer's `app.bsky.actor.getProfile` record: their handle, and the
    /// picture the icon draws (#1217 f326).
    pub profile: FetchState,
    /// Worn items the wardrobe resolve asked for and did not get (#1217
    /// f332). Per-viewer, which is what makes it worth saying: two people in
    /// the same room can see different outfits and nothing else would tell
    /// them.
    pub outfit_missing: u32,
    /// Whether a transform sample has ever played out for this peer.
    ///
    /// Until it has, the chassis is still sitting at its spawn pose — the
    /// map centre, ten metres up — so it is not drawn at all (#1217 f329).
    /// Written every frame by [`super::smoother::smooth_remote_transforms`],
    /// which is why the write is guarded.
    pub placed: bool,
    /// Whether a real body (a rigged root, or a spawned generator tree) is
    /// standing under this peer. Drives the placeholder's retirement and the
    /// roster's "arriving" chip.
    pub body: bool,
    /// Whether this peer has gone quiet past
    /// [`config::network::PEER_QUIET_SECS`] (#1224 f335). Derived from
    /// [`Self::last_sample_at`] by the sweep, and stored rather than
    /// recomputed so the roster does not need a clock.
    pub quiet: bool,
    /// When a transform packet was last accepted from this peer, on
    /// `Time::elapsed_secs_f64` (#1224 f335).
    ///
    /// `None` until the first one, so a peer that never speaks is swept on
    /// the same clock as one that stops. Lives HERE and not on
    /// [`RemotePeer`]: this is written per packet, and a per-packet write
    /// through a `Mut<RemotePeer>` would raise `Changed<RemotePeer>`
    /// continuously — destroying
    /// [`super::lifecycle::dismiss_offer_dialog_from_muted_sender`] and
    /// re-running the rigged-build kicker's whole-record compare every
    /// frame, which is the same defect #1224 f336 complains about one
    /// message over. Nothing filters on `Changed<PeerResolve>`.
    pub last_sample_at: Option<f64>,
    /// Whether this peer's arrival has been announced in chat (#1218 f338).
    ///
    /// The presence log has to balance: a peer gets one arrival line and, if
    /// and only if it got one, one departure line. The arrival is announced
    /// at handle resolution when that works — the best name — and at profile
    /// FAILURE otherwise, which is the case that used to produce a farewell
    /// to someone who was never there.
    pub announced: bool,
}

impl PeerResolve {
    /// Set [`Self::placed`], guarded so a per-frame caller does not raise the
    /// component's change flag on every frame.
    pub fn mark_placed(this: &mut Mut<'_, Self>) {
        if !this.placed {
            this.placed = true;
        }
    }

    /// Set [`Self::quiet`], guarded: this runs every frame for every peer.
    pub fn set_quiet(this: &mut Mut<'_, Self>, quiet: bool) {
        if this.quiet != quiet {
            this.quiet = quiet;
        }
    }

    /// Set [`Self::body`], guarded for the same reason.
    pub fn set_body(this: &mut Mut<'_, Self>, standing: bool) {
        if this.body != standing {
            this.body = standing;
        }
    }

    /// Record what a finished wardrobe resolve asked for against what it got
    /// (#1217 f332).
    ///
    /// A shortfall is the most likely failure in the whole rendering chain —
    /// attachment records became cross-owner with gifting — and it is
    /// PER-VIEWER, so two people in the same room see different outfits with
    /// nothing to tell them so. A resolve that comes back complete clears the
    /// shortfall, which is what retires the chip when the retry finally
    /// lands.
    pub fn record_outfit(this: &mut Mut<'_, Self>, requested: u32, installed: u32) {
        let missing = requested.saturating_sub(installed);
        if this.outfit_missing != missing {
            this.outfit_missing = missing;
        }
    }
}

/// How this peer is loading, worst first — or `None` when they are simply
/// here (#1217/#1218/#1224/#1225).
///
/// Exactly one of these is rendered. The review asked for one chip and not
/// six: a row that can carry four warnings at once is a row nobody reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeerStatus {
    /// Connected, but no authenticated DID yet — so no name, no body, and a
    /// mute that could only last the session (#1218 f290).
    Unidentified,
    /// Nothing has arrived from them for a while (#1224 f335). Their body
    /// is standing where their last packet put it, and a gift or a Visit
    /// aimed at them will not be answered.
    NotResponding,
    /// Their avatar record could not be fetched; a stand-in body is standing
    /// in for it (#1217 f323).
    AvatarUnavailable,
    /// Their profile could not be fetched, so they have no name here
    /// (#1217 f326).
    NameUnavailable,
    /// Some of what they are wearing could not be fetched *for us*
    /// (#1217 f332).
    OutfitIncomplete { missing: u32 },
    /// Everything is still on its way: identified, no body yet
    /// (#1217 f328).
    Arriving,
}

impl PeerStatus {
    /// The chip text, in the shape the wire-compatibility chip established:
    /// a glyph and one word, small, next to the name.
    pub fn chip(&self) -> String {
        match self {
            Self::Unidentified => String::from("… connecting"),
            Self::NotResponding => String::from("⚠ quiet"),
            Self::AvatarUnavailable => String::from("⚠ body"),
            Self::NameUnavailable => String::from("⚠ name"),
            Self::OutfitIncomplete { .. } => String::from("⚠ outfit"),
            Self::Arriving => String::from("… arriving"),
        }
    }

    /// The sentence behind the chip. Every one of these says what the user
    /// is looking at AND whether anything is still being tried, because
    /// "couldn't load" without "retrying" is an invitation to reload the app.
    pub fn hover(&self) -> String {
        match self {
            Self::Unidentified => String::from(
                "This person has connected but hasn't identified themselves yet, so they \
                 have no name and no body here. Muting is unavailable until they do — a \
                 mute is remembered against an account.",
            ),
            Self::NotResponding => String::from(
                "Nothing has arrived from them for a while — their connection may have \
                 dropped without telling us. Their body is standing where they last \
                 were, and a gift or a visit won't be answered.",
            ),
            Self::AvatarUnavailable => String::from(
                "Couldn't load the avatar they saved, so you're seeing a stand-in \
                 body. Still retrying.",
            ),
            Self::NameUnavailable => {
                String::from("Couldn't load their name from their profile. Still retrying.")
            }
            Self::OutfitIncomplete { missing } => format!(
                "{missing} item{} they're wearing couldn't be loaded for you, so you're \
                 seeing them without {}. Still retrying — other people in this world may \
                 see them differently.",
                if *missing == 1 { "" } else { "s" },
                if *missing == 1 { "it" } else { "them" },
            ),
            Self::Arriving => String::from(
                "They're here — their body is still being assembled from their profile \
                 and what they're wearing.",
            ),
        }
    }

    /// Whether the chip should be painted in the warning colour. The two
    /// `⋯` states are ordinary waits and must not read as faults.
    pub fn is_warning(&self) -> bool {
        !matches!(self, Self::Unidentified | Self::Arriving)
    }
}

/// Derive the one status chip from a peer and its resolution record.
///
/// Pure, so the priority order is testable without an app: an unidentified
/// peer subsumes every later stage (none of them has even started), a
/// missing body outranks a missing name because it is the bigger thing the
/// viewer is looking at, and "arriving" is only reached once nothing has
/// actually failed.
pub fn peer_status(identified: bool, resolve: &PeerResolve) -> Option<PeerStatus> {
    if !identified {
        return Some(PeerStatus::Unidentified);
    }
    // Ranked straight after "we don't know who this is", and above every
    // loading state, because it is the only one that says the person may
    // not be there at all — which changes whether the viewer should gift
    // them, follow them, or wait (#1224 f335).
    if resolve.quiet {
        return Some(PeerStatus::NotResponding);
    }
    if resolve.avatar.is_failed() {
        return Some(PeerStatus::AvatarUnavailable);
    }
    if resolve.profile.is_failed() {
        return Some(PeerStatus::NameUnavailable);
    }
    if resolve.outfit_missing > 0 {
        return Some(PeerStatus::OutfitIncomplete {
            missing: resolve.outfit_missing,
        });
    }
    if !resolve.body {
        return Some(PeerStatus::Arriving);
    }
    None
}

// ---------------------------------------------------------------------------
// DID adoption
// ---------------------------------------------------------------------------

/// Take `did` as this peer's identity, doing everything that first hangs off
/// knowing who someone is: clear any stale handle, apply the durable mute,
/// and start the avatar-record fetch (from cache when we have one).
///
/// Shared by the `Identity` arm of [`super::inbound::handle_incoming_messages`]
/// and by [`adopt_peer_sessions`] below, which is the point: the relay's
/// session map is the authority in both, and the `Identity` broadcast only
/// ever supplied a trigger. Returns whether anything changed.
#[allow(clippy::too_many_arguments)]
pub(super) fn adopt_peer_did(
    commands: &mut Commands,
    entity: Entity,
    peer: &mut RemotePeer,
    peer_id: PeerId,
    did: &str,
    muted_dids: &mut MutedDids,
    avatar_cache: &mut super::peer_cache::PeerAvatarCache,
    now: f64,
) -> bool {
    if peer.did.as_deref() == Some(did) {
        return false;
    }
    commands
        .entity(entity)
        .insert(crate::avatar::AvatarFetchPending {
            did: did.to_owned(),
        });
    // Clear any stale handle from a prior identity so the HUD reverts to the
    // DID until the profile fetch returns a verified value.
    peer.handle = None;
    peer.did = Some(did.to_owned());
    // Durable mute (#844), in BOTH directions (#1219 f331). The list→flag
    // half was here already: reconnecting used to be a mute-reset button.
    // The flag→list half was missing, so a mute applied in the window before
    // the DID resolved — the mute-on-sight case the durable list exists for
    // — set only the session-scoped flag and died with the entity.
    if muted_dids.0.contains(did) && !peer.muted {
        peer.muted = true;
    } else if peer.muted && !muted_dids.0.contains(did) {
        muted_dids.set(did, true);
    }
    // Install from cache synchronously when we've fetched this DID before in
    // the same session; otherwise kick the async PDS fetch. Skipping the
    // network round trip matters most for portal hops, which bring a cluster
    // of familiar peers in at once and would otherwise saturate the
    // IoTaskPool with duplicate DID-document resolves.
    if let Some(cached) = avatar_cache.get(did) {
        peer.avatar = Some(cached.clone());
    } else {
        super::peer_cache::spawn_peer_avatar_fetch(commands, peer_id, did.to_owned(), now);
    }
    true
}

/// Adopt a peer's DID from the relay-signed session map, without waiting for
/// them to broadcast an `Identity` (#1218 f290).
///
/// The session map was always the authority — the `Identity` arm's only use
/// for the DID on the wire is to compare it against this map and reject a
/// mismatch. Waiting for the broadcast anyway was what made "say nothing"
/// a viable griefing posture: a silent peer was counted in the room, drew
/// nothing, and could not be durably muted, because a mute is remembered
/// against an account and the client had refused to learn theirs.
///
/// Guarded on the `did:` prefix because a session id is not necessarily a
/// DID — upstream's `session_id_to_peer_id` accepts a bare UUID too, and a
/// relay that hands out opaque ids must not have them installed as
/// identities.
pub(super) fn adopt_peer_sessions(
    mut commands: Commands,
    mut peers: Query<(Entity, &mut RemotePeer)>,
    peer_sessions: Res<PeerSessionMapRes>,
    mut muted_dids: ResMut<MutedDids>,
    mut avatar_cache: ResMut<super::peer_cache::PeerAvatarCache>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    for (entity, mut peer) in peers.iter_mut() {
        if peer.did.is_some() {
            continue;
        }
        let Some(did) = peer_sessions.session_id(&peer.peer_id) else {
            continue;
        };
        if !did.starts_with("did:") {
            continue;
        }
        let peer_id = peer.peer_id;
        adopt_peer_did(
            &mut commands,
            entity,
            &mut peer,
            peer_id,
            &did,
            &mut muted_dids,
            &mut avatar_cache,
            now,
        );
    }
}

// ---------------------------------------------------------------------------
// Retries
// ---------------------------------------------------------------------------

/// Whether this peer's avatar record should be re-fetched right now
/// (#1217 f323).
///
/// Three conditions, and the third is the one the finding is about: a fetch
/// that FAILED is owed another attempt once its backoff elapses. A 404 is
/// not — it is a finished question, and the DID-seeded default it produced
/// IS that person's appearance, so re-asking would be a poll against a
/// settled answer. And one attempt at a time per peer, because the doubling
/// bounds how often a stranger's PDS is contacted and a spawn that ignored
/// the in-flight task would defeat it on the very next frame.
///
/// Pure, and separate from [`retry_peer_avatar_fetches`] for a reason worth
/// keeping: the system it drives spawns a real HTTPS round trip, so a test
/// that exercised the system would make a network call on the shared
/// `IoTaskPool` — which under `cargo test --lib`, where every test shares
/// one process and one pool, starves whatever else is waiting on it.
pub fn avatar_retry_due(
    resolve: &PeerResolve,
    did: Option<&str>,
    in_flight: bool,
    now: f64,
) -> bool {
    did.is_some() && !in_flight && resolve.avatar.retry_due(now)
}

/// Re-arm a failed avatar-record fetch once its backoff elapses (#1217 f323).
///
/// Before this, the fetch was spawned from exactly one site — the `Identity`
/// arm's `did_changed` branch — so a PDS that was unreachable for the one
/// second the peer joined left a DID-seeded stranger standing in their place
/// for the rest of the session. Two recoveries did exist, and both needed
/// the OTHER peer to act (touch their avatar editor, or reconnect); neither
/// is something a viewer can ask for.
pub(super) fn retry_peer_avatar_fetches(
    mut commands: Commands,
    peers: Query<(&RemotePeer, &PeerResolve)>,
    inflight: Query<&super::peer_cache::PeerAvatarFetchTask>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    for (peer, resolve) in peers.iter() {
        let in_flight = inflight.iter().any(|task| task.peer_id == peer.peer_id);
        if !avatar_retry_due(resolve, peer.did.as_deref(), in_flight, now) {
            continue;
        }
        // `avatar_retry_due` is false without a DID, so this always binds.
        let Some(did) = peer.did.clone() else {
            continue;
        };
        super::peer_cache::spawn_peer_avatar_fetch(&mut commands, peer.peer_id, did, now);
        // The state is deliberately NOT reset to `Pending`: the standing body
        // is still the stand-in, and `poll_peer_avatar_fetches` reads the
        // failed state to decide it may overwrite it. The backoff carries
        // forward so the next failure doubles from where this one left off.
    }
}

// ---------------------------------------------------------------------------
// Chat flood control
// ---------------------------------------------------------------------------

/// One peer's chat budget (#1222 f296).
///
/// A token bucket, because the shape of the problem is a burst allowance
/// plus a sustained rate: a person who pastes a thought as four lines must
/// not be throttled, and a script sending four hundred must be.
#[derive(Clone, Copy, Debug)]
struct ChatBucket {
    tokens: f64,
    last_seen: f64,
    /// Messages dropped in the CURRENT throttling episode, reset when the
    /// peer comes back under budget.
    dropped: u32,
    /// When this peer's throttling was last written to the session log, so
    /// N dropped messages do not become N records.
    reported_at: Option<f64>,
}

impl ChatBucket {
    fn new(now: f64) -> Self {
        Self {
            tokens: config::ui::chat::BURST_MESSAGES,
            last_seen: now,
            dropped: 0,
            reported_at: None,
        }
    }
}

/// What to do with one inbound chat message (#1222 f296).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatVerdict {
    /// Under budget — show it.
    Allow,
    /// Over budget, and quietly: this peer's throttling has already been
    /// reported recently.
    Drop,
    /// Over budget, and worth one log line naming how many have gone.
    DropAndReport { dropped: u32 },
}

/// Per-sender chat budgets.
///
/// Lives beside the other per-peer inbound state rather than on the peer
/// entity, because the flood it exists to stop can arrive from a `PeerId`
/// whose entity has not been spawned yet.
#[derive(Default, Debug)]
pub struct ChatBudgets {
    by_peer: std::collections::HashMap<PeerId, ChatBucket>,
}

impl ChatBudgets {
    /// Charge `peer` for one message and say what should become of it.
    ///
    /// The rolling 500-entry history cap is what makes a flood destructive —
    /// 500 messages evict the room's entire prior conversation, permanently,
    /// before the victim can reach a mute control two windows away. The
    /// limiter is what stops the cheapest attack in any chat product from
    /// being free.
    pub fn charge(&mut self, peer: PeerId, now: f64) -> ChatVerdict {
        let bucket = self
            .by_peer
            .entry(peer)
            .or_insert_with(|| ChatBucket::new(now));
        // Refill for the elapsed time, capped at the burst allowance.
        // `max(0.0)` because `Res<Time>` can be rewound by a test fixture and
        // a negative delta must never mint tokens.
        let elapsed = (now - bucket.last_seen).max(0.0);
        bucket.tokens = (bucket.tokens + elapsed * config::ui::chat::MESSAGES_PER_SEC)
            .min(config::ui::chat::BURST_MESSAGES);
        bucket.last_seen = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            // Back under budget: the episode is over, so the next one is
            // reported again rather than swallowed by the interval.
            bucket.dropped = 0;
            bucket.reported_at = None;
            return ChatVerdict::Allow;
        }
        bucket.dropped = bucket.dropped.saturating_add(1);
        let due = bucket
            .reported_at
            .is_none_or(|at| now - at >= config::ui::chat::THROTTLE_REPORT_INTERVAL_SECS);
        if due {
            bucket.reported_at = Some(now);
            ChatVerdict::DropAndReport {
                dropped: bucket.dropped,
            }
        } else {
            ChatVerdict::Drop
        }
    }

    /// Forget peers that have not spoken for a while, so a room that meets
    /// many senders across a long session does not grow a bucket per
    /// `PeerId` forever.
    pub fn prune(&mut self, now: f64) {
        let idle = config::ui::chat::BURST_MESSAGES / config::ui::chat::MESSAGES_PER_SEC;
        self.by_peer
            .retain(|_, bucket| now - bucket.last_seen < idle.max(60.0));
    }
}

// ---------------------------------------------------------------------------
// The presence placeholder
// ---------------------------------------------------------------------------

/// Whether a peer is currently wearing the stand-in body, or has stopped.
///
/// Present on every peer chassis from its first frame, so the "dress" pass
/// is a one-shot and a retired placeholder can never be re-dressed by a
/// later hot-swap.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PeerPlaceholder {
    Standing,
    Retired,
}

/// Put the stand-in body on every peer that does not have one yet
/// (#1217 f328).
///
/// The mesh goes on the chassis entity itself rather than on a child,
/// because `spawn_avatar_visuals` clears EVERY chassis child before it
/// spawns — including, for a rigged body, before spawning nothing at all
/// while the wardrobe resolves. A child placeholder would therefore be
/// destroyed at exactly the moment it is most needed.
///
/// It is deliberately not a guess at the peer's appearance: the comment in
/// `handle_peer_connections` is right that a synthesised body is
/// indistinguishable from a deliberately minimal one and misleads the room.
/// A translucent capsule cannot be mistaken for anybody.
pub(super) fn dress_peer_placeholders(
    mut commands: Commands,
    peers: Query<Entity, (With<RemotePeer>, Without<PeerPlaceholder>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cached: Local<Option<(Handle<Mesh>, Handle<StandardMaterial>)>>,
) {
    let mut peers = peers.iter().peekable();
    if peers.peek().is_none() {
        return;
    }
    // One mesh and one material for every peer in the session: the stand-in
    // is identical for all of them by design, and a room that cycles peers
    // must not leak an asset per arrival.
    let (mesh, material) = cached
        .get_or_insert_with(|| {
            (
                meshes.add(Capsule3d::new(
                    config::network::BODY_PLACEHOLDER_RADIUS,
                    config::network::BODY_PLACEHOLDER_LENGTH,
                )),
                materials.add(StandardMaterial {
                    base_color: config::network::BODY_PLACEHOLDER_COLOR,
                    alpha_mode: AlphaMode::Blend,
                    perceptual_roughness: 1.0,
                    ..default()
                }),
            )
        })
        .clone();
    for entity in peers {
        commands.entity(entity).insert((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            PeerPlaceholder::Standing,
        ));
    }
}

/// Take the stand-in off once a real body is standing, and record whether
/// one is on [`PeerResolve::body`] for the roster chip.
///
/// "A real body" is asked explicitly rather than inferred from the chassis
/// having children: contact effects parent a `TransientEmitter` to the
/// avatar entity, so a child is not proof of a body.
pub(super) fn retire_peer_placeholders(
    mut commands: Commands,
    mut peers: Query<(
        Entity,
        &RemotePeer,
        &mut PeerResolve,
        &PeerPlaceholder,
        Option<&Children>,
    )>,
    roots: Query<(), With<crate::player::RiggedRoot>>,
    applied: Query<&crate::player::AppliedAvatar>,
) {
    for (entity, peer, mut resolve, placeholder, children) in peers.iter_mut() {
        let has_rigged_root =
            children.is_some_and(|children| children.iter().any(|child| roots.contains(child)));
        // A generator body's nodes carry no marker of their own, so the
        // record that was applied is what says whether anything was spawned:
        // `spawn_avatar_visuals` walks `body.visuals()` and spawns nothing
        // when it is `None`.
        let has_generator_body = applied
            .get(entity)
            .is_ok_and(|applied| applied.0.body.visuals().is_some());
        // A record that names neither a rigged reference nor a visuals tree
        // is a bare chassis by contract (#1217): nothing is coming, so the
        // stand-in is what this peer looks like and the roster should stop
        // saying "arriving".
        let contractually_bare = peer.avatar.as_ref().is_some_and(|record| {
            record.body.visuals().is_none() && record.body.rigged_ref().is_none()
        });
        let standing = has_rigged_root || has_generator_body;
        PeerResolve::set_body(&mut resolve, standing || contractually_bare);
        if standing && *placeholder == PeerPlaceholder::Standing {
            commands
                .entity(entity)
                .remove::<(Mesh3d, MeshMaterial3d<StandardMaterial>)>()
                .insert(PeerPlaceholder::Retired);
        }
    }
}

/// Silence every sink belonging to a muted peer (#1219 f324).
///
/// `sync_mute_visibility` was the ONLY consumer of `RemotePeer::muted`
/// outside the chat and offer gates, and it wrote nothing but `Visibility` —
/// while Bevy's audio is not gated on visibility at all. A peer's generator
/// body can carry a `PlaybackMode::Loop` spatial emitter (the construct-audio
/// dispatcher is deliberately outside the `avatar_mode` guard), and so can
/// every prop they wear, so muting a harasser running a screaming avatar
/// made them strictly worse: still audible, now invisible, and impossible to
/// point at.
///
/// Writes the set rather than calling `mute()` on the sinks, because
/// `audio_mute::reconcile_sink_mute` (private) drives every sink to the master
/// toggle every frame and would undo a direct call within one frame.
pub(super) fn sync_mute_audio(
    peers: Query<(Entity, &RemotePeer)>,
    children: Query<&Children>,
    mut silenced: ResMut<crate::audio_mute::SilencedByMute>,
) {
    let any_muted = peers.iter().any(|(_, peer)| peer.muted);
    // The overwhelmingly common case: nobody is muted and nothing is
    // silenced, so the frame costs one pass over the peer list.
    if !any_muted && silenced.0.is_empty() {
        return;
    }
    let mut next = bevy::platform::collections::HashSet::new();
    for (entity, peer) in peers.iter() {
        if !peer.muted {
            continue;
        }
        collect_subtree(entity, &children, &mut next);
    }
    // Guarded (#879): a muted peer standing still must not flag the resource
    // every frame.
    if silenced.0 != next {
        silenced.0 = next;
    }
}

/// Collect `root` and every descendant into `out`.
fn collect_subtree(
    root: Entity,
    children: &Query<&Children>,
    out: &mut bevy::platform::collections::HashSet<Entity>,
) {
    out.insert(root);
    let Ok(kids) = children.get(root) else {
        return;
    };
    for child in kids.iter() {
        collect_subtree(child, children, out);
    }
}

/// Clear the per-peer silence set when the session ends, so a muted peer's
/// entities cannot keep a recycled entity id silent in the next session.
pub(super) fn reset_mute_audio(mut silenced: ResMut<crate::audio_mute::SilencedByMute>) {
    silenced.0.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ladder_climbs_handle_then_did_then_traveler() {
        let named = PeerLabel::new(Some("alice.bsky.social"), Some("did:plc:abcdefgh12345678"));
        assert_eq!(named.addressed(), "@alice.bsky.social");
        assert_eq!(named.name(), "alice.bsky.social");
        assert!(named.is_named());

        let identified = PeerLabel::new(None, Some("did:plc:abcdefgh12345678"));
        assert_eq!(identified.addressed(), "did:plc:abcdefgh…");
        assert!(!identified.is_named());

        // An identifier short enough to print whole gets no ellipsis:
        // eliding nothing sends the reader looking for the rest.
        let whole = PeerLabel::new(None, Some("did:web:x"));
        assert_eq!(whole.addressed(), "did:web:x");
        assert_eq!(whole.name(), "did:web:x");

        let stranger = PeerLabel::new(None, None);
        assert_eq!(stranger.addressed(), "A traveler");
        assert_eq!(stranger.name(), "a traveler");
        assert!(!stranger.is_named());
    }

    /// #1218 f299. The sigil is a promise that what follows is a name; the
    /// gift modal used to make that promise over a `did:plc:` string, in the
    /// app's one blocking dialog, on a countdown.
    #[test]
    fn a_did_head_is_never_prefixed_with_an_at_sign() {
        let identified = PeerLabel::new(None, Some("did:plc:z72i7hdynmk6r22z27h6tvur"));
        assert!(!identified.addressed().starts_with('@'));
        assert!(identified.addressed().starts_with("did:plc:"));
    }

    /// #1113/#1217. Both retries double from the same base to the same
    /// ceiling; the arithmetic is shared so they cannot drift.
    #[test]
    fn the_backoff_doubles_from_base_to_the_ceiling() {
        assert_eq!(next_wait_secs(None, 2.0, 60.0), 2.0);
        assert_eq!(next_wait_secs(Some(2.0), 2.0, 60.0), 4.0);
        assert_eq!(next_wait_secs(Some(32.0), 2.0, 60.0), 60.0);
        assert_eq!(next_wait_secs(Some(60.0), 2.0, 60.0), 60.0);
    }

    #[test]
    fn a_backoff_is_not_ready_until_its_wait_elapses() {
        let first = RetryBackoff::after_failure(None, 100.0);
        assert_eq!(first.attempts, 1);
        assert!(!first.ready(100.0 + first.wait_secs - 0.001));
        assert!(first.ready(100.0 + first.wait_secs));

        let second = RetryBackoff::after_failure(Some(&first), 200.0);
        assert_eq!(second.attempts, 2);
        assert_eq!(second.wait_secs, first.wait_secs * 2.0);
    }

    /// #1217/#1218. Six failures, one chip: the derivation is a priority
    /// ladder, and this pins its order so a later finding cannot quietly
    /// add a seventh chip beside it.
    #[test]
    fn one_status_is_derived_worst_first() {
        // Nothing has resolved and there is no DID: everything below is
        // subsumed by "we don't know who this is".
        let mut resolve = PeerResolve {
            avatar: FetchState::Failed(RetryBackoff::after_failure(None, 0.0)),
            ..PeerResolve::default()
        };
        assert_eq!(
            peer_status(false, &resolve),
            Some(PeerStatus::Unidentified),
            "an unidentified peer has not started any later stage"
        );

        assert_eq!(
            peer_status(true, &resolve),
            Some(PeerStatus::AvatarUnavailable)
        );

        // Silence outranks every loading state (#1224 f335): it is the only
        // one that says the person may not be there at all, which changes
        // whether the viewer should gift them, follow them, or wait.
        resolve.quiet = true;
        assert_eq!(peer_status(true, &resolve), Some(PeerStatus::NotResponding),);
        assert_eq!(
            peer_status(false, &resolve),
            Some(PeerStatus::Unidentified),
            "and is itself outranked by not knowing who they are"
        );
        resolve.quiet = false;

        resolve.avatar = FetchState::Landed;

        resolve.profile = FetchState::Failed(RetryBackoff::after_failure(None, 0.0));
        assert_eq!(
            peer_status(true, &resolve),
            Some(PeerStatus::NameUnavailable)
        );
        resolve.profile = FetchState::Landed;

        resolve.outfit_missing = 2;
        assert_eq!(
            peer_status(true, &resolve),
            Some(PeerStatus::OutfitIncomplete { missing: 2 })
        );
        assert!(
            peer_status(true, &resolve)
                .unwrap()
                .hover()
                .contains("2 items"),
            "the count is the whole point of the hover"
        );
        resolve.outfit_missing = 0;

        assert_eq!(peer_status(true, &resolve), Some(PeerStatus::Arriving));
        resolve.body = true;
        assert_eq!(
            peer_status(true, &resolve),
            None,
            "a fully resolved peer carries no chip at all"
        );
    }

    /// The two waiting states are ordinary and must not be painted as
    /// faults; the three failures must.
    #[test]
    fn waiting_is_not_a_warning() {
        assert!(!PeerStatus::Unidentified.is_warning());
        assert!(!PeerStatus::Arriving.is_warning());
        assert!(
            PeerStatus::NotResponding.is_warning(),
            "a body that may not be attached to anybody is not an ordinary wait"
        );
        assert!(PeerStatus::AvatarUnavailable.is_warning());
        assert!(PeerStatus::NameUnavailable.is_warning());
        assert!(PeerStatus::OutfitIncomplete { missing: 1 }.is_warning());
    }

    // -----------------------------------------------------------------
    // System-level regressions
    // -----------------------------------------------------------------

    use crate::state::MutedDids;

    fn peer_id(n: u8) -> PeerId {
        // `PeerId` is a newtype over a `Uuid` from a crate overlands does not
        // depend on directly and has no public constructor; its
        // `Deserialize` is the reachable one, and it is stable because the
        // relay uses the same one (the idiom `network::link`'s tests use).
        serde_json::from_str(&format!("\"00000000-0000-0000-0000-0000000000{n:02}\""))
            .expect("a well-formed uuid")
    }

    fn remote(n: u8) -> RemotePeer {
        RemotePeer {
            peer_id: peer_id(n),
            did: None,
            handle: None,
            muted: false,
            avatar: None,
            build: None,
            connected_at: 0.0,
        }
    }

    /// The minimum world `adopt_peer_sessions` reads.
    fn adoption_app() -> App {
        let mut app = App::new();
        app.add_plugins((bevy::app::TaskPoolPlugin::default(), bevy::time::TimePlugin));
        app.init_resource::<PeerSessionMapRes>();
        app.init_resource::<MutedDids>();
        app.init_resource::<super::super::peer_cache::PeerAvatarCache>();
        app.add_systems(Update, adopt_peer_sessions);
        app
    }

    fn bind_session(app: &mut App, n: u8, session_id: &str) {
        let map = app.world().resource::<PeerSessionMapRes>().0.clone();
        map.write()
            .expect("the fixture holds the only handle")
            .insert(peer_id(n), session_id.to_owned());
    }

    /// Pre-seed the DID's record so adoption takes the cache-hit branch.
    ///
    /// Not incidental: the cache MISS branch spawns a real PDS fetch on the
    /// shared `IoTaskPool`, and under `cargo test --lib` — one process, one
    /// pool — a handful of those starve every other test waiting on a task
    /// while nextest, which forks per test, stays green. No test in this file
    /// may take that branch.
    fn seed_cache(app: &mut App, did: &str) {
        app.world_mut()
            .resource_mut::<super::super::peer_cache::PeerAvatarCache>()
            .insert(
                did.to_owned(),
                crate::pds::AvatarRecord::default_for_did(did),
            );
    }

    /// #1218 f290. The sequence: a peer connects and never broadcasts an
    /// `Identity`. Before this, the client refused to learn who they were —
    /// so the headcount included them, nothing rendered for them, and a mute
    /// on their row skipped the durable DID-keyed list and died on their
    /// reconnect. The relay had signed their DID the whole time.
    #[test]
    fn a_peer_that_never_broadcasts_an_identity_is_still_identified() {
        let mut app = adoption_app();
        seed_cache(&mut app, "did:plc:silentgrieferxyz");
        let entity = app.world_mut().spawn(remote(1)).id();
        bind_session(&mut app, 1, "did:plc:silentgrieferxyz");

        app.update();

        let peer = app
            .world()
            .entity(entity)
            .get::<RemotePeer>()
            .expect("peer");
        assert_eq!(
            peer.did.as_deref(),
            Some("did:plc:silentgrieferxyz"),
            "the relay authenticated this DID; nothing was waiting on the peer to confirm it"
        );
        assert!(
            app.world()
                .entity(entity)
                .contains::<crate::avatar::AvatarFetchPending>(),
            "adoption is what arms the profile fetch"
        );
    }

    /// A relay is not obliged to use DIDs as session ids — upstream's
    /// `session_id_to_peer_id` accepts a bare UUID too. An opaque id must
    /// never be installed as somebody's identity.
    #[test]
    fn an_opaque_session_id_is_not_adopted_as_an_identity() {
        let mut app = adoption_app();
        let entity = app.world_mut().spawn(remote(2)).id();
        bind_session(&mut app, 2, "abcdef00-1234-5678-9abc-def012345678");

        app.update();

        assert!(
            app.world()
                .entity(entity)
                .get::<RemotePeer>()
                .expect("peer")
                .did
                .is_none(),
            "a session id that is not a DID is not an identity"
        );
    }

    /// #844, carried onto the new adoption site: the durable mute list is
    /// applied the moment the DID lands, whichever site learned it.
    #[test]
    fn adoption_applies_the_durable_mute() {
        let mut app = adoption_app();
        app.world_mut()
            .resource_mut::<MutedDids>()
            .0
            .insert(String::from("did:plc:mutedone"));
        seed_cache(&mut app, "did:plc:mutedone");
        let entity = app.world_mut().spawn(remote(3)).id();
        bind_session(&mut app, 3, "did:plc:mutedone");

        app.update();

        assert!(
            app.world()
                .entity(entity)
                .get::<RemotePeer>()
                .expect("peer")
                .muted,
            "reconnecting must not be a mute-reset button"
        );
    }

    /// The minimum world `sync_mute_visibility` reads, plus the placeholder
    /// retirement it runs beside.
    fn visibility_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.add_systems(Update, super::super::lifecycle::sync_mute_visibility);
        app
    }

    /// #1217 f329. The sequence: a peer connects and their avatar installs
    /// synchronously from the DID cache — a portal hop with a familiar,
    /// generator-bodied peer — before a single transform packet arrives. A
    /// stationary peer sends only every thirtieth 64 Hz tick, so the window
    /// is up to about half a second, and for all of it the body was drawn at
    /// the spawn pose: the map centre, ten metres up, then a teleport.
    #[test]
    fn a_peer_is_not_drawn_until_a_pose_has_played_out() {
        let mut app = visibility_app();
        let entity = app
            .world_mut()
            .spawn((
                remote(1),
                PeerResolve::default(),
                Transform::from_xyz(0.0, 10.0, 0.0),
                Visibility::Hidden,
            ))
            .id();

        app.update();
        assert_eq!(
            *app.world().entity(entity).get::<Visibility>().expect("vis"),
            Visibility::Hidden,
            "nothing true is known about where this peer is yet"
        );

        app.world_mut()
            .entity_mut(entity)
            .get_mut::<PeerResolve>()
            .expect("resolve")
            .placed = true;
        app.update();
        assert_eq!(
            *app.world().entity(entity).get::<Visibility>().expect("vis"),
            Visibility::Inherited,
            "the first playout is what makes a peer drawable"
        );
    }

    /// The mute gate and the not-yet-placed gate share one owner, and the
    /// mute one still wins: `sync_mute_visibility` runs AFTER the smoother
    /// in the plugin chain, so a placed peer must not become visible again
    /// just because a pose arrived.
    #[test]
    fn a_muted_peer_stays_hidden_after_it_is_placed() {
        let mut app = visibility_app();
        let mut peer = remote(1);
        peer.muted = true;
        let entity = app
            .world_mut()
            .spawn((
                peer,
                PeerResolve {
                    placed: true,
                    ..PeerResolve::default()
                },
                Transform::default(),
                Visibility::Inherited,
            ))
            .id();

        app.update();
        assert_eq!(
            *app.world().entity(entity).get::<Visibility>().expect("vis"),
            Visibility::Hidden,
        );
    }

    /// The minimum world the placeholder systems read. `AssetPlugin` rather
    /// than bare `init_asset`: `Assets::add` reaches for the `AssetServer`
    /// to mint a handle.
    fn placeholder_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            bevy::asset::AssetPlugin::default(),
            bevy::app::TaskPoolPlugin::default(),
        ))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .add_systems(Update, (dress_peer_placeholders, retire_peer_placeholders));
        app
    }

    /// #1217 f328. The sequence: a peer joins, and nothing renders for them
    /// until an Identity, a profile fetch, a PDS fetch, a wardrobe resolve
    /// and an async skinned build have all landed — every one a network
    /// round trip. "Someone is here but you cannot see them" is
    /// indistinguishable from a rendering bug, and it is the first
    /// impression of multiplayer.
    #[test]
    fn a_joining_peer_wears_a_stand_in_until_a_real_body_lands() {
        let mut app = placeholder_app();
        let chassis = app
            .world_mut()
            .spawn((remote(1), PeerResolve::default()))
            .id();

        app.update();
        assert!(
            app.world().entity(chassis).contains::<Mesh3d>(),
            "a peer with no body yet is not a hole in the world"
        );
        assert_eq!(
            app.world().entity(chassis).get::<PeerPlaceholder>(),
            Some(&PeerPlaceholder::Standing),
        );
        assert!(
            !app.world()
                .entity(chassis)
                .get::<PeerResolve>()
                .expect("resolve")
                .body,
            "nothing real is standing, so the roster says 'arriving'"
        );

        // The rigged build lands: a `RiggedRoot` appears under the chassis.
        app.world_mut()
            .spawn((crate::player::RiggedRoot, ChildOf(chassis)));
        app.update();

        assert!(
            !app.world().entity(chassis).contains::<Mesh3d>(),
            "the stand-in is taken off the moment a real body is standing"
        );
        assert_eq!(
            app.world().entity(chassis).get::<PeerPlaceholder>(),
            Some(&PeerPlaceholder::Retired),
        );
        assert!(
            app.world()
                .entity(chassis)
                .get::<PeerResolve>()
                .expect("resolve")
                .body,
        );
    }

    /// A retired stand-in must never come back: `spawn_avatar_visuals`
    /// clears every chassis child on a hot-swap, and a dress pass that
    /// keyed off "has no body right now" would put the capsule back on
    /// every peer mid-wardrobe-change.
    #[test]
    fn a_retired_stand_in_is_not_re_dressed_by_a_hot_swap() {
        let mut app = placeholder_app();
        let chassis = app
            .world_mut()
            .spawn((remote(1), PeerResolve::default()))
            .id();
        app.update();
        let root = app
            .world_mut()
            .spawn((crate::player::RiggedRoot, ChildOf(chassis)))
            .id();
        app.update();
        assert!(!app.world().entity(chassis).contains::<Mesh3d>());

        // The peer changes what they wear: the root is torn down and
        // rebuilt over the next few frames.
        app.world_mut().entity_mut(root).despawn();
        app.update();
        app.update();

        assert!(
            !app.world().entity(chassis).contains::<Mesh3d>(),
            "a wardrobe change is not an arrival"
        );
        assert_eq!(
            app.world().entity(chassis).get::<PeerPlaceholder>(),
            Some(&PeerPlaceholder::Retired),
        );
    }

    /// The mesh and material are minted once for the whole session. A room
    /// that cycles peers must not leak an asset per arrival.
    #[test]
    fn every_stand_in_shares_one_mesh_and_one_material() {
        let mut app = placeholder_app();
        let a = app
            .world_mut()
            .spawn((remote(1), PeerResolve::default()))
            .id();
        app.update();
        let b = app
            .world_mut()
            .spawn((remote(2), PeerResolve::default()))
            .id();
        app.update();

        let mesh_a = app.world().entity(a).get::<Mesh3d>().expect("a").0.clone();
        let mesh_b = app.world().entity(b).get::<Mesh3d>().expect("b").0.clone();
        assert_eq!(mesh_a, mesh_b);
        assert_eq!(
            app.world().resource::<Assets<Mesh>>().len(),
            1,
            "one capsule for the session, however many peers pass through"
        );
    }

    /// #1218 f338. The sequence: a peer joins, their `getProfile` fails, so
    /// no arrival line prints; they wander around and leave, and the room's
    /// only trace of them is "did:plc:z72i7hdyn… left the room." for someone
    /// the log says was never there. The two halves used to be triggered by
    /// different facts — departure unconditionally from the disconnect arm,
    /// arrival only from a resolved handle.
    #[test]
    fn a_departure_is_narrated_only_for_someone_the_room_was_told_about() {
        assert!(should_announce_departure(true, true, false));
        assert!(
            !should_announce_departure(true, false, false),
            "no arrival line was printed for this peer, so no farewell either"
        );
        assert!(
            !should_announce_departure(false, true, false),
            "a departure seen while OUR link is down is not about the peer (#1213 f402)"
        );
        assert!(
            !should_announce_departure(true, true, true),
            "a reconnect loop must not be able to scroll the history clean with \
             a muted person's name (#1219 f289)"
        );
    }

    /// #1219. The sequence the funnel exists for: two controls write a mute
    /// and each used to write it slightly differently. The durable, DID-keyed
    /// half must land whether or not the peer entity is still here — a
    /// stranger who spams a gift and disconnects is the hit-and-run case the
    /// list exists for, and "Mute & Decline" wrote the list from inside a
    /// loop over live peers, so it matched nothing and recorded nothing.
    #[test]
    fn a_mute_is_durable_even_when_the_peer_has_already_gone() {
        let mut muted_dids = MutedDids::default();
        let mut log = crate::diagnostics::SessionLog::default();

        assert!(set_peer_mute(
            None,
            Some("did:plc:hitandrun"),
            true,
            &mut muted_dids,
            &mut log,
            Some(peer_id(1)),
            0.0,
        ));
        assert!(
            muted_dids.0.contains("did:plc:hitandrun"),
            "their next visit must not reach the user exactly as before"
        );
        assert_eq!(
            log.iter()
                .filter(|e| matches!(
                    e.payload,
                    crate::diagnostics::event::EventPayload::PeerMuteToggled { .. }
                ))
                .count(),
            1,
        );
    }

    /// The funnel owns the change guard: a no-op write must not raise
    /// `Changed<RemotePeer>` (which drives the offer-dialog dismisser and the
    /// rigged-build kicker) and must not log a toggle that did not happen.
    #[test]
    fn re_muting_someone_already_muted_writes_nothing() {
        let mut muted_dids = MutedDids::default();
        let mut log = crate::diagnostics::SessionLog::default();
        let mut peer = remote(1);
        peer.did = Some(String::from("did:plc:already"));

        assert!(set_peer_mute(
            Some(&mut peer),
            Some("did:plc:already"),
            true,
            &mut muted_dids,
            &mut log,
            Some(peer_id(1)),
            0.0,
        ));
        assert!(!set_peer_mute(
            Some(&mut peer),
            Some("did:plc:already"),
            true,
            &mut muted_dids,
            &mut log,
            Some(peer_id(1)),
            1.0,
        ));
        assert_eq!(
            log.iter()
                .filter(|e| matches!(
                    e.payload,
                    crate::diagnostics::event::EventPayload::PeerMuteToggled { .. }
                ))
                .count(),
            1,
            "a no-op write is not a toggle"
        );
    }

    /// #1222 f296. The sequence: someone starts spamming in a busy room and
    /// within seconds every message anybody else sent has scrolled out of
    /// existence — the rolling 500-entry cap that protects the renderer is
    /// exactly what makes a flood destructive, and the remedy was two
    /// windows away. `Chat` is also not in the coalescing set that protects
    /// the three heavy variants from bursts.
    #[test]
    fn a_burst_is_allowed_and_a_flood_is_not() {
        let mut budgets = ChatBudgets::default();
        let peer = peer_id(1);
        let burst = config::ui::chat::BURST_MESSAGES as u32;

        // A person pasting a thought as several lines is not a flood.
        for n in 0..burst {
            assert_eq!(
                budgets.charge(peer, 0.0),
                ChatVerdict::Allow,
                "message {n} of the burst allowance"
            );
        }
        // The next one, in the same instant, is.
        assert_eq!(
            budgets.charge(peer, 0.0),
            ChatVerdict::DropAndReport { dropped: 1 },
            "the first drop of an episode is worth a log line"
        );
        assert_eq!(
            budgets.charge(peer, 0.0),
            ChatVerdict::Drop,
            "and the rest are not — the limiter must not move the flood into the log"
        );
    }

    /// The bucket refills, so a throttled peer is not silenced for the
    /// session: an ordinary conversation resumes at the sustained rate.
    #[test]
    fn a_throttled_peer_can_speak_again_once_the_bucket_refills() {
        let mut budgets = ChatBudgets::default();
        let peer = peer_id(1);
        for _ in 0..(config::ui::chat::BURST_MESSAGES as u32 + 1) {
            budgets.charge(peer, 0.0);
        }
        assert!(matches!(
            budgets.charge(peer, 0.5),
            ChatVerdict::Drop | ChatVerdict::DropAndReport { .. }
        ));
        let refill = 1.0 / config::ui::chat::MESSAGES_PER_SEC;
        assert_eq!(
            budgets.charge(peer, refill + 0.01),
            ChatVerdict::Allow,
            "one refilled token is one message"
        );
        // Coming back under budget ENDS the episode, so the next flood is
        // reported afresh rather than swallowed by the report interval that
        // the previous one started.
        assert_eq!(
            budgets.charge(peer, refill + 0.01),
            ChatVerdict::DropAndReport { dropped: 1 },
        );
    }

    /// One peer's flood must not cost anybody else their voice: the budget
    /// is per sender, not per room.
    #[test]
    fn one_flooder_does_not_throttle_the_room() {
        let mut budgets = ChatBudgets::default();
        for _ in 0..100 {
            budgets.charge(peer_id(1), 0.0);
        }
        assert_eq!(budgets.charge(peer_id(2), 0.0), ChatVerdict::Allow);
    }

    /// A clock that steps backwards must never mint tokens — `Res<Time>` is
    /// the virtual clock and a fixture can rewind it.
    #[test]
    fn a_backwards_clock_does_not_refill_the_bucket() {
        let mut budgets = ChatBudgets::default();
        let peer = peer_id(1);
        for _ in 0..(config::ui::chat::BURST_MESSAGES as u32) {
            budgets.charge(peer, 100.0);
        }
        assert!(matches!(
            budgets.charge(peer, 50.0),
            ChatVerdict::Drop | ChatVerdict::DropAndReport { .. }
        ));
    }

    /// #1223 f292. The sequence: you tick Mute by accident. Before the
    /// Settings list, the ONLY way back was to hope that person wandered
    /// into a room you happened to be standing in — both mute controls
    /// require a live `RemotePeer` entity. The funnel is what lets a surface
    /// with no peer in hand lift a mute, and it must log the person it
    /// lifted rather than a session-scoped UUID it does not have.
    #[test]
    fn a_mute_can_be_lifted_without_the_person_being_in_the_room() {
        let mut muted_dids = MutedDids::default();
        muted_dids.set("did:plc:mistake", true);
        let mut log = crate::diagnostics::SessionLog::default();

        assert!(set_peer_mute(
            None,
            Some("did:plc:mistake"),
            false,
            &mut muted_dids,
            &mut log,
            None,
            0.0,
        ));
        assert!(muted_dids.0.is_empty());
        assert!(
            log.iter().any(|e| matches!(
                &e.payload,
                crate::diagnostics::event::EventPayload::PeerMuteToggled { peer, muted }
                    if peer == "did:plc:mistake" && !muted
            )),
            "the log names the account, because the mute list is keyed by one"
        );
    }

    /// #1219 f331. The sequence: a spammer arrives, you mute them on sight
    /// before their DID has resolved, they reconnect, and they are audible
    /// and visible again — while the tooltip said the mute persists. The
    /// list→flag direction was handled; the flag→list one was not, and the
    /// peer entity took the flag with it on disconnect.
    #[test]
    fn a_session_mute_is_promoted_the_moment_the_did_lands() {
        let mut app = adoption_app();
        seed_cache(&mut app, "did:plc:mutedonsight");
        let mut peer = remote(4);
        peer.muted = true;
        let entity = app.world_mut().spawn(peer).id();
        bind_session(&mut app, 4, "did:plc:mutedonsight");

        app.update();

        assert!(
            app.world()
                .resource::<MutedDids>()
                .0
                .contains("did:plc:mutedonsight"),
            "the mute-on-sight case is the one the durable list exists for"
        );
        assert!(
            app.world()
                .entity(entity)
                .get::<RemotePeer>()
                .expect("peer")
                .muted
        );
    }

    /// #1219 f324. The sequence: a peer wearing a looping spatial emitter is
    /// muted, and every sink under their chassis — the body's own construct
    /// audio and each worn prop's — has to go silent, however deep it sits.
    #[test]
    fn muting_a_peer_silences_every_sink_under_their_body() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.init_resource::<crate::audio_mute::SilencedByMute>();
        app.add_systems(Update, sync_mute_audio);

        let mut peer = remote(1);
        peer.muted = true;
        let chassis = app.world_mut().spawn(peer).id();
        let root = app.world_mut().spawn(ChildOf(chassis)).id();
        let prop = app.world_mut().spawn(ChildOf(root)).id();
        let someone_else = app.world_mut().spawn(remote(2)).id();

        app.update();

        let silenced = app.world().resource::<crate::audio_mute::SilencedByMute>();
        assert!(silenced.0.contains(&chassis));
        assert!(silenced.0.contains(&root));
        assert!(
            silenced.0.contains(&prop),
            "a worn prop's emitter is as loud as the body's"
        );
        assert!(
            !silenced.0.contains(&someone_else),
            "muting one person is not muting the room"
        );

        // Unmuting restores every one of them.
        app.world_mut()
            .entity_mut(chassis)
            .get_mut::<RemotePeer>()
            .expect("peer")
            .muted = false;
        app.update();
        assert!(
            app.world()
                .resource::<crate::audio_mute::SilencedByMute>()
                .0
                .is_empty()
        );
    }

    /// #1217 f323. The sequence: a peer's PDS is briefly unreachable at the
    /// moment they join. `poll_peer_avatar_fetches` synthesises
    /// `default_for_did` for BOTH the 404 and the error, and after that
    /// `peer.avatar.is_none()` is false forever — so the fetch was never
    /// tried again and a procedurally generated stranger stood in for
    /// someone's authored body, for the whole room, for the whole session.
    ///
    /// Exercised through the predicate rather than the system: the system
    /// spawns a real HTTPS round trip, and under `cargo test --lib` — one
    /// process, one shared `IoTaskPool` — two of those starve every other
    /// test waiting on a task. (They did. `oauth::service_token`'s two
    /// refresh tests failed with "the refresh task never landed" while
    /// nextest, which forks per test, stayed green.)
    #[test]
    fn a_failed_avatar_fetch_is_tried_again_and_an_unpublished_one_is_not() {
        let did = Some("did:plc:unreachable");
        let base = config::network::PEER_FETCH_RETRY_BASE_SECS;
        let failed = PeerResolve {
            avatar: FetchState::Failed(RetryBackoff {
                attempts: 1,
                wait_secs: base,
                failed_at: 100.0,
            }),
            ..PeerResolve::default()
        };

        assert!(
            !avatar_retry_due(&failed, did, false, 100.0 + base - 0.001),
            "inside the wait: a failing PDS is not a load target"
        );
        assert!(
            avatar_retry_due(&failed, did, false, 100.0 + base),
            "past it: the viewer gets another chance at the real body"
        );

        // The 404 case: this person has simply not published an avatar, and
        // the DID-seeded default IS what they look like. Asking again would
        // be a poll against a settled question.
        let unpublished = PeerResolve {
            avatar: FetchState::Landed,
            ..PeerResolve::default()
        };
        assert!(!avatar_retry_due(&unpublished, did, false, 1.0e9));
        assert!(!avatar_retry_due(
            &PeerResolve::default(),
            did,
            false,
            1.0e9
        ));
    }

    /// One attempt at a time per peer, and none at all before the relay has
    /// named them: the doubling bounds how often a stranger's PDS is
    /// contacted, and a spawn that ignored the in-flight task would defeat it
    /// on the very next frame.
    #[test]
    fn a_retry_does_not_stack_on_an_in_flight_fetch_or_fire_without_a_did() {
        let failed = PeerResolve {
            avatar: FetchState::Failed(RetryBackoff {
                attempts: 1,
                wait_secs: 2.0,
                failed_at: 0.0,
            }),
            ..PeerResolve::default()
        };
        assert!(avatar_retry_due(&failed, Some("did:plc:x"), false, 100.0));
        assert!(
            !avatar_retry_due(&failed, Some("did:plc:x"), true, 100.0),
            "a fetch is already running for this peer"
        );
        assert!(
            !avatar_retry_due(&failed, None, false, 100.0),
            "there is nothing to fetch for a peer the relay has not named"
        );
    }

    /// #1217 f332. The sequence: someone wears a crown gifted by a third
    /// party, whose record is briefly unfetchable for ONE viewer. They see a
    /// bare head; everyone else sees the crown; the session log was the only
    /// place either of them could have found out. And when the retry finally
    /// lands, the chip has to go away again.
    #[test]
    fn an_outfit_shortfall_is_recorded_and_then_cleared() {
        let mut world = World::new();
        let entity = world.spawn(PeerResolve::default()).id();
        let mut resolve = world.get_mut::<PeerResolve>(entity).expect("resolve");

        PeerResolve::record_outfit(&mut resolve, 3, 1);
        assert_eq!(resolve.outfit_missing, 2);
        assert_eq!(
            peer_status(true, &resolve),
            Some(PeerStatus::OutfitIncomplete { missing: 2 })
        );

        PeerResolve::record_outfit(&mut resolve, 3, 3);
        assert_eq!(
            resolve.outfit_missing, 0,
            "a complete resolve retires the chip — otherwise the retry is invisible"
        );

        // A resolve that installed MORE than it asked for is not negative.
        PeerResolve::record_outfit(&mut resolve, 1, 4);
        assert_eq!(resolve.outfit_missing, 0);
    }

    /// #1217 f332: singular and plural, because "1 items" on a hover is the
    /// tell that nobody read the sentence.
    #[test]
    fn the_outfit_hover_agrees_with_itself() {
        let one = PeerStatus::OutfitIncomplete { missing: 1 }.hover();
        assert!(one.contains("1 item they're wearing couldn't"), "{one}");
        assert!(one.contains("without it"), "{one}");
        let many = PeerStatus::OutfitIncomplete { missing: 3 }.hover();
        assert!(many.contains("3 items they're wearing couldn't"), "{many}");
        assert!(many.contains("without them"), "{many}");
    }
}
