//! Inbound message dispatch — drains the `NetworkQueue<OverlandsMessage>`
//! and routes each variant through the appropriate side-effect: jitter
//! buffer push for `Transform`, identity authentication + avatar fetch
//! kick-off for `Identity`, owner-DID-gated room-state replacement for
//! `RoomStateUpdate`, busy-gated incoming-offer dialog for `ItemOffer`,
//! and so on.
//!
//! [`handle_incoming_messages`] is now only the drain, the coalesce and the
//! dispatch; each variant is handled by a function in a sibling module
//! ([`transform`], [`identity`], [`record_updates`], [`chat`],
//! [`item_offer`]). This file's header used to argue the opposite — that
//! per-variant functions "would just push 12+ parameters around without
//! improving readability" — and that was wrong in the way that matters
//! (#1161): **the parameter list is the point.** A `match` with eleven arms
//! over a peer-supplied enum is the hostile-peer surface, and a handler
//! whose signature names exactly the state its message kind may touch is
//! what lets the owner/DID checks be read, and fuzzed, one kind at a time.
//! Writing them out immediately showed that four handlers could not reach
//! state they appeared to have.
//!
//! The bundle the finding proposed — one `InboundCtx` of `&mut` references
//! to the system's parameters — does not compile: Bevy gives each parameter
//! its own `'w`/`'s`, a struct forces them to unify, and `&mut T<'w>` is
//! invariant in `'w`. `reborrow()` rescues `Commands` and `Query` and does
//! not exist for derived [`SystemParam`]s. [`InboundBuffers`] stays as the
//! one genuine bundle, because it is a `SystemParam` in its own right.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::diagnostics::SessionLog;
use crate::pds::AvatarRecord;
use crate::protocol::OverlandsMessage;
use crate::state::{
    ChatHistory, CurrentRoomDid, IncomingOfferDialog, LiveRoomRecord, PendingOutgoingOffers,
    RemotePeer,
};

mod chat;
mod identity;
mod item_offer;
mod record_updates;
mod transform;

use super::SmootherConfigRes;
use super::peer_cache::PeerAvatarCache;
use super::presence::PeerResolve;

/// Message kinds that can be coalesced to the latest-per-sender within a
/// single drain. Each fully supersedes any earlier instance from the same
/// peer — `Identity` re-kicks one avatar fetch per DID change,
/// `AvatarStateUpdate` overwrites `peer.avatar` wholesale, and
/// `RoomStateUpdate` wholesale-replaces the live room record — so decoding and
/// sanitising the stale ones is pure wasted work a flooding peer could
/// weaponise into a main-thread DoS (the sanitize pass on a deeply-nested
/// generator tree is not cheap). `RoomStateUpdate` is the heaviest of the three
/// (~1 MiB JSON per broadcast, emitted every `record.is_changed()` frame during
/// an owner's slider drag), so a guest whose frame is slower than the owner's
/// send rate would otherwise decode several full snapshots per drain when only
/// the last feeds the rebuild. Dropping all but the last is behaviour-preserving
/// because only the final value ever survives. (`RoomStateUpdate`'s authority
/// check already runs before decode, so the guest-spam DoS is separately
/// mitigated; this deduplicates legitimate owner-snapshot pile-up.)
#[derive(PartialEq, Eq, Hash, Clone, Copy)]
enum CoalesceKey {
    Identity,
    AvatarState,
    RoomState,
    /// `Hello` rides the identity cadence and every instance carries the same
    /// two values, so all but the last in a drain are literally redundant.
    Hello,
    /// `AvatarRecordsPublished` carries no payload and is idempotent — it
    /// forgets a resolution, and forgetting it twice is forgetting it once
    /// (#1224 f336). It was the one heavyweight arm that neither coalesced
    /// nor checked authority, so a modified client could hold the message
    /// down and impose steady per-frame cost on every guest: each instance
    /// took `peer.avatar.as_mut()`, which raises the change tick
    /// unconditionally, and two `Changed<RemotePeer>` systems then re-ran —
    /// a whole-record deep compare in `detect_remote_change` and a
    /// whole-outfit diff in `sync_rigged_attachments`, which is exactly the
    /// work #1135's latch exists to avoid.
    AvatarPublished,
}

fn coalesce_key(msg: &OverlandsMessage) -> Option<CoalesceKey> {
    match msg {
        OverlandsMessage::Identity { .. } => Some(CoalesceKey::Identity),
        OverlandsMessage::AvatarStateUpdate { .. } => Some(CoalesceKey::AvatarState),
        OverlandsMessage::RoomStateUpdate { .. } => Some(CoalesceKey::RoomState),
        OverlandsMessage::Hello { .. } => Some(CoalesceKey::Hello),
        OverlandsMessage::AvatarRecordsPublished => Some(CoalesceKey::AvatarPublished),
        _ => None,
    }
}

/// A drained message decoupled from the multiuser `NetworkReceived` wrapper so
/// the dispatch loop can process both directly-received messages *and*
/// messages reassembled from [`OverlandsMessage::ChunkedPayload`] fragments
/// (#716) uniformly — a reassembled message carries its originating peer's
/// `sender` so every downstream authority check still applies. Field names
/// mirror `NetworkReceived`, so the dispatch body reads `msg.sender` /
/// `msg.payload` unchanged.
struct Incoming {
    sender: PeerId,
    payload: OverlandsMessage,
}

/// Inbound transport state consulted while draining the P2P queue: the
/// jitter-buffer [`SmootherConfigRes`] applied to each remote `Transform`, and
/// the [`super::chunk::ChunkReassembly`] buffer that stitches
/// [`OverlandsMessage::ChunkedPayload`] fragments back into whole messages.
/// Grouped into one [`SystemParam`] so [`handle_incoming_messages`] stays
/// within Bevy's 16-parameter-per-system ceiling.
#[derive(SystemParam)]
pub(super) struct InboundBuffers<'w, 's> {
    smoother_cfg: Res<'w, SmootherConfigRes>,
    /// The stored mirror, for the same-owner split (#1203): whether this
    /// session has unpublished edits decides whether another session of
    /// the owner may replace the live record or must ask first.
    stored_room: Option<Res<'w, crate::state::StoredRoomRecord>>,
    /// A held same-owner record already awaiting the owner's answer; a
    /// newer one replaces it without a second toast.
    held_room: Option<Res<'w, crate::state::OtherSessionRoom>>,
    /// When the "updated from another session" toast last fired, so a
    /// session editing continuously does not toast every broadcast.
    other_session_toast_at: Local<'s, Option<f64>>,
    reassembly: ResMut<'w, super::chunk::ChunkReassembly>,
    /// Gift-lifecycle feedback (#843): accepted/declined responses toast
    /// to the sender the moment they land.
    toasts: ResMut<'w, crate::notify::Toasts>,
    /// Busy-gate auto-declines counted while an offer dialog is up
    /// (#843); the dialog reports them when it closes.
    busy_declines: ResMut<'w, crate::state::BusyAutoDeclines>,
    /// An offer the user set aside to make room for (#1220 f288) counts as
    /// busy: it is coming back the moment a slot frees, and a second dialog
    /// opening under it would be answered on top of the one already owed.
    held_offer: Option<Res<'w, crate::state::HeldOffer>>,
    /// Per-sender chat budgets (#1222 f296). A `Local`, because they are
    /// this system's own bookkeeping and nothing else reads them — and
    /// because a resource would have to be torn down at logout to avoid
    /// carrying one session's flooder into the next.
    chat_budgets: Local<'s, super::presence::ChatBudgets>,
    /// Durable mute list (#844): applied the moment a peer's DID
    /// resolves, so a muted harasser stays muted across reconnects — and
    /// written in the other direction too (#1219 f331), so a mute applied
    /// before the DID landed is promoted rather than lost with the entity.
    muted_dids: ResMut<'w, crate::state::MutedDids>,
    /// Undo-capture classification (#862): an inbound owner
    /// `RoomStateUpdate` wholesale-replaces `LiveRoomRecord`, and the
    /// history must reset instead of recording it as a local edit.
    undo_signals: ResMut<'w, crate::state::RoomWriteSignals>,
    /// Chat-keyword emotes (#1068): an arriving message plays a gesture on
    /// its sender's own body.
    emotes: MessageWriter<'w, crate::player::emote::EmoteRequest>,
    /// The local peer's world digest (#1146), for comparing against the
    /// `WorldDigest` announcements peers broadcast. Bundled here rather than
    /// taken as its own parameter because `handle_incoming_messages` is at
    /// Bevy's 16-parameter `IntoSystem` ceiling.
    world_digest: Res<'w, crate::world_digest::WorldDigest>,
    /// How far each peer has resolved (#1217/#1218). A separate query rather
    /// than a fifth element of the `peers` tuple: the two touch disjoint
    /// components, so Bevy's per-component access check lets them coexist,
    /// and `handle_incoming_messages` is at the 16-parameter ceiling so a
    /// bare parameter is not available.
    resolve: Query<'w, 's, &'static mut PeerResolve>,
}

/// Move a peer's existing rig resolution onto an incoming record when the
/// two name the same references (#1113).
///
/// A resolution is a fetch of the records the reference list names, so it
/// stays correct for exactly as long as that list does. Requiring an exact
/// match — wardrobe rkey *and* the ordered attachment rkeys — is what keeps
/// this from carrying a stale outfit across a real change: any edit to what
/// the peer wears alters the list and forces a fresh resolve.
fn carry_resolution(existing: Option<&AvatarRecord>, incoming: &mut AvatarRecord) {
    let Some(previous) = existing.and_then(|record| record.body.rigged_ref()) else {
        return;
    };
    let Some(rig) = incoming.body.rigged_mut() else {
        return;
    };
    if rig.resolved.is_none()
        && rig.avatar == previous.avatar
        && rig.attachments == previous.attachments
    {
        rig.resolved = previous.resolved.clone();
    }
}

/// Forget the rig resolution a peer's record is carrying (#1122).
///
/// The counterpart to [`carry_resolution`], for the one event that changes
/// the records behind an unchanged reference set: their owner saved. Only
/// the resolution goes — the references stay, because they still name the
/// right records; it is the bytes at those rkeys that moved.
fn forget_rig_resolution(record: &mut AvatarRecord) {
    if let Some(rig) = record.body.rigged_mut() {
        rig.resolved = None;
    }
}

/// The four components every peer entity carries that this module writes:
/// its identity, its interpolated pose and the jitter buffer feeding it.
pub(super) type PeerParts = (
    Entity,
    &'static mut RemotePeer,
    &'static mut Transform,
    &'static mut TransformBuffer,
);

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn handle_incoming_messages(
    mut commands: Commands,
    mut messages_received: MessagesReceived<OverlandsMessage>,
    mut chat: ResMut<ChatHistory>,
    mut peers: Query<PeerParts>,
    time: Res<Time>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut room_record: Option<ResMut<LiveRoomRecord>>,
    peer_sessions: Res<PeerSessionMapRes>,
    session: Option<Res<AtprotoSession>>,
    mut session_log: ResMut<SessionLog>,
    incoming_dialog: Option<Res<IncomingOfferDialog>>,
    mut pending_offers: ResMut<PendingOutgoingOffers>,
    mut sender: SendMessage<OverlandsMessage>,
    mut avatar_cache: ResMut<PeerAvatarCache>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
    mut bufs: InboundBuffers,
) {
    let now = time.elapsed_secs_f64();
    // Drain the whole queue into a buffer so we can coalesce per
    // (sender, kind): a burst of `Identity` or `AvatarStateUpdate`
    // messages from one peer would otherwise fire N redundant avatar
    // fetches / run the heavy decode+sanitize pass N times, letting a
    // flooding peer pin the main thread. Only the last of each kind per
    // sender survives — see [`CoalesceKey`].
    // Pre-pass (#716): peel `ChunkedPayload` fragments off into the
    // reassembly buffer and splice any completed message back into the work
    // list as a normal `Incoming`. A fragment that does not complete its
    // message contributes nothing this drain; the buffer carries it forward.
    let raw: Vec<_> = messages_received.drain().collect();
    let mut messages: Vec<Incoming> = Vec::with_capacity(raw.len());
    for m in raw {
        match m.payload {
            OverlandsMessage::ChunkedPayload {
                msg_id,
                seq,
                total,
                data,
            } => {
                if let Some(reassembled) = bufs
                    .reassembly
                    .ingest(m.sender, msg_id, seq, total, data, now)
                {
                    messages.push(Incoming {
                        sender: m.sender,
                        payload: reassembled,
                    });
                }
            }
            payload => messages.push(Incoming {
                sender: m.sender,
                payload,
            }),
        }
    }

    let mut last_coalesced_idx: std::collections::HashMap<(PeerId, CoalesceKey), usize> =
        std::collections::HashMap::new();
    for (i, msg) in messages.iter().enumerate() {
        if let Some(key) = coalesce_key(&msg.payload) {
            last_coalesced_idx.insert((msg.sender, key), i);
        }
    }
    // Tracks whether an incoming-offer dialog is (or will be) up this
    // frame. Seeded from the resource and flipped to `true` the moment we
    // stage one via `commands.insert_resource` — `Commands` don't apply
    // until end-of-system, so reading the resource again would report the
    // stale pre-frame state and let a peer pack many `ItemOffer`s into one
    // frame, bypassing the busy-gate.
    // A held offer counts as busy (#1220 f288) — it is owed the screen the
    // moment a slot frees, and a second dialog answered on top of it would
    // shuffle the two.
    let mut dialog_open = incoming_dialog.is_some() || bufs.held_offer.is_some();
    // Forget senders who have gone quiet (#1222 f296), so a long session in
    // a busy hub does not grow one bucket per `PeerId` ever seen.
    bufs.chat_budgets.prune(now);

    for (i, msg) in messages.into_iter().enumerate() {
        if let Some(key) = coalesce_key(&msg.payload)
            && last_coalesced_idx.get(&(msg.sender, key)) != Some(&i)
        {
            continue;
        }
        match msg.payload {
            OverlandsMessage::Transform { position, rotation } => transform::handle(
                msg.sender,
                position,
                rotation,
                &mut peers,
                &mut bufs,
                &mut metrics,
                now,
            ),
            OverlandsMessage::Identity { did, handle } => identity::handle(
                msg.sender,
                did,
                handle,
                &mut commands,
                &mut peers,
                &peer_sessions,
                &mut session_log,
                &mut avatar_cache,
                &mut metrics,
                &mut bufs,
                now,
            ),
            OverlandsMessage::AvatarStateUpdate { record_json } => {
                record_updates::handle_avatar_state(
                    msg.sender,
                    record_json,
                    &mut peers,
                    &mut session_log,
                    &mut avatar_cache,
                    &mut bufs,
                    now,
                )
            }
            OverlandsMessage::Hello { protocol, build } => identity::handle_hello(
                msg.sender,
                protocol,
                build,
                &mut peers,
                &mut session_log,
                now,
            ),
            OverlandsMessage::WorldDigest {
                record_fp,
                digest: theirs,
            } => record_updates::handle_world_digest(
                msg.sender,
                record_fp,
                theirs,
                &mut session_log,
                &mut bufs,
                now,
            ),
            OverlandsMessage::AvatarRecordsPublished => {
                record_updates::handle_records_published(msg.sender, &mut commands, &mut peers)
            }
            OverlandsMessage::RoomStateUpdate { record_json } => record_updates::handle_room_state(
                msg.sender,
                record_json,
                &mut commands,
                &mut peers,
                &room_did,
                &mut room_record,
                &session,
                &mut session_log,
                &mut bufs,
                now,
            ),
            OverlandsMessage::ChunkedPayload { .. } => {
                // Fragments are consumed by the reassembly pre-pass above and
                // never reach dispatch. One arriving here means a peer nested
                // a `ChunkedPayload` inside a reassembled message (malformed or
                // hostile) — ignore it rather than recurse.
                debug!(
                    "Ignoring nested/unexpected ChunkedPayload from {:?}",
                    msg.sender
                );
            }
            OverlandsMessage::Chat { text } => chat::handle(
                msg.sender,
                text,
                &mut chat,
                &mut peers,
                &peer_sessions,
                &mut session_log,
                &mut bufs,
                now,
            ),
            OverlandsMessage::ItemOffer {
                offer_id,
                target_did,
                payload_json,
            } => item_offer::handle(
                msg.sender,
                offer_id,
                target_did,
                payload_json,
                &mut commands,
                &mut peers,
                &peer_sessions,
                &session,
                &mut session_log,
                &mut sender,
                &mut metrics,
                &mut bufs,
                now,
                &mut dialog_open,
            ),
            OverlandsMessage::ItemOfferResponse {
                offer_id,
                target_did,
                payload_json,
            } => item_offer::handle_response(
                msg.sender,
                offer_id,
                target_did,
                payload_json,
                &peer_sessions,
                &session,
                &mut session_log,
                &mut pending_offers,
                &mut bufs,
                now,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::avatar::{ResolvedRig, wardrobe::engine_default_for_did};

    fn resolved_record(avatar: &str, attachments: &[&str]) -> AvatarRecord {
        let mut record = AvatarRecord::wearing(avatar);
        if let Some(rig) = record.body.rigged_mut() {
            rig.attachments = attachments.iter().map(|a| (*a).to_string()).collect();
            rig.resolved = Some(ResolvedRig {
                body: engine_default_for_did("did:plc:carry"),
                attachments: Vec::new(),
            });
        }
        record
    }

    /// #1113: `resolved` is `#[serde(skip)]`, so every live-preview
    /// broadcast decodes unresolved. Overwriting the peer's record wholesale
    /// discarded a resolution that was still correct and made each debounced
    /// keystroke cost a DID lookup, a wardrobe fetch and up to sixteen
    /// attachment fetches on every client in the room.
    #[test]
    fn an_unchanged_reference_set_keeps_its_resolution_across_a_preview() {
        let standing = resolved_record("3jzfcijpj2z2a", &["att-1"]);
        let mut off_the_wire = AvatarRecord::wearing("3jzfcijpj2z2a");
        if let Some(rig) = off_the_wire.body.rigged_mut() {
            rig.attachments = vec![String::from("att-1")];
        }

        carry_resolution(Some(&standing), &mut off_the_wire);

        assert!(
            off_the_wire
                .body
                .rigged_ref()
                .is_some_and(|rig| rig.resolved.is_some()),
            "same references: the fetched records are still the right ones"
        );
    }

    /// #1122. Sequence: a peer wears a circlet, sculpts their face, then
    /// presses Save. The records their references name now hold new
    /// bytes at the SAME rkeys — so the obvious fix, re-broadcasting the
    /// record on publish success, changes nothing: the rule above correctly
    /// carries the pre-save resolution forward and the peer keeps the old
    /// body until its owner happens to change what they wear. That is why
    /// the publish sends its own notice, and why the notice's whole job is
    /// to drop the resolution the reference set would otherwise preserve.
    #[test]
    fn only_a_publish_notice_can_dislodge_a_resolution_the_references_still_match() {
        let standing = resolved_record("3jzfcijpj2z2a", &["att-1"]);

        // What a re-broadcast on publish success would do, by itself:
        let mut re_broadcast = AvatarRecord::wearing("3jzfcijpj2z2a");
        if let Some(rig) = re_broadcast.body.rigged_mut() {
            rig.attachments = vec![String::from("att-1")];
        }
        carry_resolution(Some(&standing), &mut re_broadcast);
        assert!(
            re_broadcast
                .body
                .rigged_ref()
                .is_some_and(|rig| rig.resolved.is_some()),
            "the references are unchanged, so the stale body survives the nudge"
        );

        // What `AvatarRecordsPublished` does:
        let mut notified = standing.clone();
        forget_rig_resolution(&mut notified);
        assert!(
            notified
                .body
                .rigged_ref()
                .is_some_and(|rig| rig.resolved.is_none()),
            "the peer must re-fetch, and the references still name what to fetch"
        );
        assert!(
            notified
                .body
                .rigged_ref()
                .is_some_and(|rig| rig.attachments == vec![String::from("att-1")]),
            "the outfit is not forgotten — only the copy of the records"
        );
    }

    /// The other half of the rule — carrying it when the peer actually
    /// changed what they wear would show everyone a stale outfit.
    #[test]
    fn a_changed_reference_set_is_re_resolved() {
        let standing = resolved_record("3jzfcijpj2z2a", &["att-1"]);

        for changed in [
            ("3jzfcijpj2z2b", vec!["att-1"]),
            ("3jzfcijpj2z2a", vec!["att-1", "att-2"]),
            ("3jzfcijpj2z2a", vec![]),
        ] {
            let mut incoming = AvatarRecord::wearing(changed.0);
            if let Some(rig) = incoming.body.rigged_mut() {
                rig.attachments = changed.1.iter().map(|a| (*a).to_string()).collect();
            }
            carry_resolution(Some(&standing), &mut incoming);
            assert!(
                incoming
                    .body
                    .rigged_ref()
                    .is_some_and(|rig| rig.resolved.is_none()),
                "{changed:?} names different records and must be fetched afresh"
            );
        }
    }
}
