//! Inbound message dispatch — drains the `NetworkQueue<OverlandsMessage>`
//! and routes each variant through the appropriate side-effect: jitter
//! buffer push for `Transform`, identity authentication + avatar fetch
//! kick-off for `Identity`, owner-DID-gated room-state replacement for
//! `RoomStateUpdate`, busy-gated incoming-offer dialog for `ItemOffer`,
//! and so on.
//!
//! The handler is one big `match` on [`OverlandsMessage`]; per-variant
//! arms read the same `peers` query, the same `peer_sessions` map, and
//! the same outbound `MessageWriter`, so factoring each arm into its own
//! function would just push 12+ parameters around without improving
//! readability. Kept as a single-file dispatcher.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::avatar::AvatarFetchPending;
use crate::diagnostics::SessionLog;
use crate::diagnostics::event::EventPayload;
use crate::pds::AvatarRecord;
use crate::protocol::OverlandsMessage;
use crate::state::{
    ChatHistory, CurrentRoomDid, IncomingOfferDialog, LiveRoomRecord, PendingOutgoingOffers,
    RemotePeer,
};

use super::SmootherConfigRes;
use super::peer_cache::{PeerAvatarCache, spawn_peer_avatar_fetch};

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
}

fn coalesce_key(msg: &OverlandsMessage) -> Option<CoalesceKey> {
    match msg {
        OverlandsMessage::Identity { .. } => Some(CoalesceKey::Identity),
        OverlandsMessage::AvatarStateUpdate { .. } => Some(CoalesceKey::AvatarState),
        OverlandsMessage::RoomStateUpdate { .. } => Some(CoalesceKey::RoomState),
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
pub(super) struct InboundBuffers<'w> {
    smoother_cfg: Res<'w, SmootherConfigRes>,
    reassembly: ResMut<'w, super::chunk::ChunkReassembly>,
    /// Read-only peek at which panels are open: a chat message landing
    /// while the Chat window is closed bumps the unread badge (#835).
    panels: Res<'w, crate::ui::toolbar::UiPanels>,
    /// Gift-lifecycle feedback (#843): accepted/declined responses toast
    /// to the sender the moment they land.
    toasts: ResMut<'w, crate::ui::toast::Toasts>,
    /// Busy-gate auto-declines counted while an offer dialog is up
    /// (#843); the dialog reports them when it closes.
    busy_declines: ResMut<'w, crate::state::BusyAutoDeclines>,
    /// Durable mute list (#844): applied the moment a peer's DID
    /// resolves, so a muted harasser stays muted across reconnects.
    muted_dids: Res<'w, crate::state::MutedDids>,
    /// Undo-capture classification (#862): an inbound owner
    /// `RoomStateUpdate` wholesale-replaces `LiveRoomRecord`, and the
    /// history must reset instead of recording it as a local edit.
    undo_signals: ResMut<'w, crate::ui::undo::RoomWriteSignals>,
    /// Chat-keyword emotes (#1068): an arriving message plays a gesture on
    /// its sender's own body.
    emotes: MessageWriter<'w, crate::player::emote::EmoteRequest>,
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

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn handle_incoming_messages(
    mut commands: Commands,
    mut messages_received: MessagesReceived<OverlandsMessage>,
    mut chat: ResMut<ChatHistory>,
    mut peers: Query<(
        Entity,
        &mut RemotePeer,
        &mut Transform,
        &mut TransformBuffer,
    )>,
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
    let mut dialog_open = incoming_dialog.is_some();
    for (i, msg) in messages.into_iter().enumerate() {
        if let Some(key) = coalesce_key(&msg.payload)
            && last_coalesced_idx.get(&(msg.sender, key)) != Some(&i)
        {
            continue;
        }
        match msg.payload {
            OverlandsMessage::Transform { position, rotation } => {
                // Hand the wire-supplied pose to the upstream jitter buffer.
                // `push_sample` performs every guard the local code used to
                // do inline (NaN / Inf rejection, magnitude clamp via
                // `max_coord_abs`, quaternion normalisation, playout-timestamp
                // anchoring against same-frame bursts and clock-skew drift)
                // so the worst a malicious peer can do is have their packet
                // silently discarded.
                for (_, peer, _tf, mut buf) in peers.iter_mut() {
                    if peer.peer_id == msg.sender {
                        let accepted = buf.push_sample(
                            Vec3::from_array(position),
                            Quat::from_array(rotation),
                            now,
                            &bufs.smoother_cfg.0,
                        );
                        // A rejected sample (NaN/Inf or out-of-bounds) is silently
                        // discarded by the smoother; count it (E-4).
                        if !accepted {
                            crate::diagnostics::samplers::transform_rejected(&mut metrics);
                        }
                    }
                }
            }
            OverlandsMessage::Identity { did, handle } => {
                // Reject identity claims whose DID does not match the
                // session_id the relay bound to the sender's PeerId. The
                // signaller publishes (PeerId → authenticated DID) entries to
                // `PeerSessionMapRes` as peers join, so any mismatch means the
                // peer is impersonating another user over the unauthenticated
                // data channel.
                //
                // A `None` lookup means matchbox surfaced the peer before the
                // signaller recorded its session_id (or the peer disconnected
                // mid-frame). Treat this as "not yet verified" and drop the
                // message — the peer broadcasts Identity on a timer, so a
                // subsequent attempt will succeed once the map catches up.
                match peer_sessions.session_id(&msg.sender) {
                    Some(authenticated_did) if authenticated_did == did => {}
                    Some(authenticated_did) => {
                        crate::diagnostics::samplers::identity_spoof_rejected(&mut metrics);
                        warn!(
                            "Rejecting spoofed Identity from {}: claimed did={}, authenticated did={}",
                            msg.sender, did, authenticated_did
                        );
                        session_log.warn(
                            now,
                            EventPayload::PeerIdentitySpoofRejected {
                                peer: msg.sender.to_string(),
                                claimed_did: did,
                                authenticated_did: authenticated_did.to_string(),
                            },
                        );
                        continue;
                    }
                    None => {
                        debug!(
                            "Deferring Identity from {}: session not yet known",
                            msg.sender
                        );
                        continue;
                    }
                }

                for (entity, mut peer, _, _) in peers.iter_mut() {
                    if peer.peer_id != msg.sender {
                        continue;
                    }

                    let did_changed = peer.did.as_deref() != Some(did.as_str());

                    // The `handle` field on the wire is peer-supplied and
                    // therefore untrusted — a malicious peer could claim any
                    // handle string to impersonate another actor in the chat
                    // HUD and disconnect log. The authoritative handle is
                    // resolved asynchronously by the avatar/profile fetch
                    // pipeline (kicked below via `AvatarFetchPending`), which
                    // hits `app.bsky.actor.getProfile` against the DID the
                    // relay already authenticated. Do NOT write `peer.handle`
                    // from this message.

                    if did_changed {
                        info!(
                            "Peer {} identified as did={} (claimed handle @{} — unverified, will resolve via getProfile)",
                            msg.sender, did, handle
                        );
                        commands
                            .entity(entity)
                            .insert(AvatarFetchPending { did: did.clone() });
                        // Clear any stale handle from a prior identity so the
                        // HUD reverts to the DID until the profile fetch
                        // returns a verified value.
                        peer.handle = None;
                        peer.did = Some(did.clone());
                        // Durable mute (#844): the flag used to live only on
                        // this session-scoped entity, so disconnect/rejoin
                        // reset it — reconnecting was a mute-reset button.
                        if bufs.muted_dids.0.contains(&did) && !peer.muted {
                            peer.muted = true;
                        }
                        // Install from cache synchronously when we've fetched
                        // this DID before in the same session; otherwise
                        // kick the async PDS fetch. Skipping the network
                        // round trip matters most for portal hops, which
                        // bring a cluster of familiar peers in at once and
                        // would otherwise saturate the IoTaskPool with
                        // duplicate DID-document resolves.
                        if let Some(cached) = avatar_cache.get(&did) {
                            peer.avatar = Some(cached.clone());
                        } else {
                            spawn_peer_avatar_fetch(&mut commands, msg.sender, did.clone(), now);
                        }
                    }
                }
            }
            OverlandsMessage::AvatarStateUpdate { record_json } => {
                // Live preview nudge from a peer who is mid-edit.
                //
                // AUTHORITY FIRST, matching the `RoomStateUpdate` arm below
                // (#1126). This used to decode and sanitize up to ~900 KiB
                // of JSON and only then check whether the sender was a peer
                // we had authenticated — so a peer that never sent a valid
                // Identity could make every guest walk a maximal generator
                // tree once per frame and throw the result away. The
                // heaviest message in the protocol already resolves the
                // sender before touching the payload; the second-heaviest
                // now does too.
                let sender_did = peers
                    .iter()
                    .find(|(_, peer, _, _)| peer.peer_id == msg.sender)
                    .and_then(|(_, peer, _, _)| peer.did.clone());
                let Some(sender_did) = sender_did else {
                    debug!(
                        "Deferring AvatarStateUpdate from {}: peer DID not yet known",
                        msg.sender
                    );
                    continue;
                };
                // Symmetric with `from_chunk_bytes`, which refuses to
                // reassemble past this ceiling: a payload that could never
                // have been legitimately broadcast is not worth decoding.
                if record_json.len() > crate::config::network::MAX_RELIABLE_PAYLOAD_BYTES {
                    warn!(
                        "Dropping AvatarStateUpdate from {:?}: {} bytes exceeds the wire ceiling",
                        msg.sender,
                        record_json.len()
                    );
                    continue;
                }
                let Some(mut new_record) = OverlandsMessage::decode_avatar_state(&record_json)
                else {
                    // Emit the typed decode-failure event (#634) so the
                    // `net.silent_decode_failure` rule sees this arm too — it
                    // already matches all three, but only ItemOffer was emitting.
                    session_log.warn(
                        now,
                        EventPayload::AvatarStateDecodeFailed {
                            peer: msg.sender.to_string(),
                            reason: "payload failed to decode".into(),
                        },
                    );
                    warn!(
                        "Dropping AvatarStateUpdate from {:?}: payload failed to decode",
                        msg.sender
                    );
                    continue;
                };
                new_record.sanitize();

                for (_, mut peer, _, _) in peers.iter_mut() {
                    if peer.peer_id != msg.sender {
                        continue;
                    }
                    // The DID was resolved above, before the decode — a peer
                    // without one never reaches here.
                    let peer_did = sender_did.clone();
                    // Carry a still-valid resolution across the update
                    // (#1113). `resolved` is `#[serde(skip)]`, so every
                    // preview arrives unresolved; overwriting wholesale threw
                    // away a resolution that was still correct and made each
                    // debounced keystroke cost a DID-document lookup plus a
                    // wardrobe record plus up to sixteen attachment records,
                    // on every client in the room. The references name what
                    // the resolution is *of*, so an unchanged reference set
                    // means the fetched records are unchanged too.
                    carry_resolution(peer.avatar.as_ref(), &mut new_record);
                    // Refresh the cache so a future Identity from this DID
                    // (e.g. reconnect within the session) restores the
                    // live-preview state instead of the stale PDS record.
                    avatar_cache.insert(peer_did, new_record.clone());
                    peer.avatar = Some(new_record);
                    break;
                }
            }
            OverlandsMessage::AvatarRecordsPublished => {
                // The sender saved their rigged body (#1122). Same rkeys,
                // new bytes behind them — so drop the resolution we are
                // carrying and let `spawn_peer_rig_resolutions` fetch the
                // published records. Without this the owner's Save reached
                // nobody: the re-broadcast preview names the same
                // references, `carry_resolution` keeps the pre-save body,
                // and peers stayed on it until the owner happened to edit
                // something that changed a reference.
                //
                // Deliberately does NOT clear `PeerRigResolveFloor` (#1126):
                // the floor is the per-peer cap on how often one identity can
                // make every guest fan out to hosts of its choosing, and a
                // message a peer sends at will must not lift it. The
                // reference-set backoff does go, because it records that
                // THESE references failed to resolve — which a publish is
                // precisely the news that invalidates.
                for (peer_entity, mut peer, _, _) in peers.iter_mut() {
                    if peer.peer_id != msg.sender {
                        continue;
                    }
                    if let Some(record) = peer.avatar.as_mut() {
                        forget_rig_resolution(record);
                    }
                    commands
                        .entity(peer_entity)
                        .remove::<super::peer_cache::PeerRigResolveBackoff>();
                    break;
                }
            }
            OverlandsMessage::RoomStateUpdate { record_json } => {
                // Authority check FIRST: decoding and sanitising up to ~1 MiB
                // of JSON per broadcast is expensive enough that a guest
                // spamming forged updates at 60 Hz would burn main-thread
                // cycles even though the result is ultimately discarded. By
                // resolving the sender's DID and comparing against the room
                // owner before touching `record_json`, a non-owner broadcast
                // short-circuits before the parse runs.
                let sender_did = peers
                    .iter()
                    .find(|(_, peer, _, _)| peer.peer_id == msg.sender)
                    .and_then(|(_, peer, _, _)| peer.did.clone());

                let is_owner = match (&sender_did, &room_did) {
                    (Some(did), Some(rd)) => did == &rd.0,
                    _ => false,
                };

                if !is_owner {
                    continue;
                }

                // Decode the JSON payload shipped by the owner. The wire
                // format is JSON-in-bincode because `RoomRecord`'s tagged
                // enums are incompatible with bincode's streaming decoder —
                // see `OverlandsMessage::RoomStateUpdate` docs.
                let Some(mut new_record) = OverlandsMessage::decode_room_state(&record_json) else {
                    // Typed decode-failure event (#634). `sender_did` is
                    // guaranteed `Some` here — the `is_owner` gate above required
                    // it — but default defensively rather than unwrap.
                    session_log.warn(
                        now,
                        EventPayload::RoomStateDecodeFailed {
                            sender_did: sender_did.clone().unwrap_or_default(),
                            error: "failed to decode as RoomRecord".into(),
                        },
                    );
                    warn!(
                        "Dropping RoomStateUpdate from {:?}: payload failed to decode as RoomRecord",
                        msg.sender
                    );
                    continue;
                };

                // Clamp every unbounded numeric field before the world
                // compiler touches the recipe — a malicious owner could
                // otherwise ship a grid_size or L-system iteration count
                // designed to OOM every guest.
                new_record.sanitize();

                // Replace the whole recipe. `world_builder::compile_room_record`
                // observes the resource change and rebuilds every compiled
                // entity (water, sun colour, scattered shapes) in one pass.
                if let Some(record) = room_record.as_mut() {
                    record.0 = new_record;
                    // Foreign wholesale write (#862): mostly guests (whose
                    // history is empty anyway), but a second session of
                    // the SAME owner DID passes the is_owner gate too —
                    // the local ring must reset rather than offer undos
                    // across the other session's replacement.
                    bufs.undo_signals.foreign = true;
                    info!("Room state updated from owner broadcast");
                }
            }
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
            OverlandsMessage::Chat { text } => {
                // Ignore messages from muted peers.
                let sender_muted = peers
                    .iter()
                    .find(|(_, peer, _, _)| peer.peer_id == msg.sender)
                    .map(|(_, peer, _, _)| peer.muted)
                    .unwrap_or(false);

                if !sender_muted {
                    // Defend against over-long chat payloads from a malicious
                    // peer: the local sender throttles via the chat UI, but a
                    // hand-crafted packet could still ship an 800 KiB string
                    // and lock every guest's renderer trying to word-wrap it.
                    let max = crate::config::ui::chat::MAX_MESSAGE_LEN;
                    let clipped = if text.len() <= max {
                        text
                    } else {
                        let mut end = max;
                        while end > 0 && !text.is_char_boundary(end) {
                            end -= 1;
                        }
                        text[..end].to_string()
                    };
                    // Strip ASCII control bytes (newlines, carriage returns,
                    // form feeds, etc.) so a peer cannot inject multi-line
                    // payloads that impersonate another author's rows in the
                    // HUD log.
                    let clipped: String = clipped
                        .chars()
                        .map(|c| if c.is_control() && c != '\t' { ' ' } else { c })
                        .collect();
                    let sender_peer = peers
                        .iter()
                        .find(|(_, peer, _, _)| peer.peer_id == msg.sender);
                    let did = sender_peer.and_then(|(_, peer, _, _)| peer.did.clone());
                    let author = sender_peer
                        .and_then(|(_, peer, _, _)| peer.handle.clone())
                        .unwrap_or_else(|| msg.sender.to_string());
                    // Chat-keyword emotes (#1068): the sender's own body plays
                    // the gesture their words asked for. Read off the CLIPPED,
                    // control-stripped text — the same string the room is shown
                    // — so a hostile peer cannot smuggle a trigger past the
                    // sanitiser, and driven from the sender's chassis entity so
                    // a muted peer stays silent in gesture as well as in text.
                    if let Some((chassis, ..)) = sender_peer
                        && let Some(request) = crate::player::emote::request_for(chassis, &clipped)
                    {
                        bufs.emotes.write(request);
                    }
                    // Capped + wall-clock-stamped (#846).
                    chat.push(did, author, clipped);
                    // With the window closed this message would be
                    // invisible — count it for the toolbar badge (#835).
                    if !bufs.panels.chat {
                        chat.unread += 1;
                    }
                }
            }
            OverlandsMessage::ItemOffer {
                offer_id,
                target_did,
                item_name,
                generator_json,
                wear_json,
            } => {
                // Broadcast-with-address: only the peer whose DID matches
                // `target_did` should act on the offer. Everyone else
                // silently drops it because `bevy_symbios_multiuser` has no
                // directed-send primitive.
                let Some(sess) = session.as_deref() else {
                    continue;
                };
                if sess.did != target_did {
                    continue;
                }

                // Authenticate the sender's DID against the relay-signed
                // PeerSessionMap — same defence the Identity handler uses.
                // A `None` lookup means the peer connected before its
                // session bound; defer by dropping the message (the sender
                // can retry).
                let Some(sender_did) = peer_sessions.session_id(&msg.sender) else {
                    debug!(
                        "Deferring ItemOffer from {}: peer session not yet known",
                        msg.sender
                    );
                    continue;
                };

                // Silent auto-decline for muted senders. The sender still
                // gets a response so their UI clears the pending state, but
                // no dialog is shown and no diagnostics entry is written —
                // muted senders should be invisible by design.
                let peer_lookup = peers
                    .iter()
                    .find(|(_, peer, _, _)| peer.peer_id == msg.sender);
                let sender_muted = peer_lookup
                    .as_ref()
                    .map(|(_, peer, _, _)| peer.muted)
                    .unwrap_or(false);
                let sender_handle = peer_lookup
                    .as_ref()
                    .and_then(|(_, peer, _, _)| peer.handle.clone())
                    .unwrap_or_else(|| sender_did.clone());

                if sender_muted {
                    sender.to(
                        msg.sender,
                        OverlandsMessage::ItemOfferResponse {
                            offer_id,
                            target_did: sender_did.clone(),
                            accepted: false,
                        },
                        ChannelKind::Reliable,
                    );
                    continue;
                }

                // Clamp the wire-supplied item name *before* any
                // diagnostics or dialog state references it. The
                // protocol field is an unbounded `String`, so a hostile
                // sender can ship a 10 MiB blob; spamming such offers
                // at a busy victim would otherwise force the main
                // thread to allocate that string into every busy-gate
                // / rejection log line. Clamping up front guarantees
                // the rest of this handler only sees a bounded value.
                let item_name = {
                    let mut n: String = item_name
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(64)
                        .collect();
                    if n.is_empty() {
                        n.push_str("(unnamed)");
                    }
                    n
                };

                // Busy-gate: a dialog is already up (or was staged earlier
                // this same frame), so an attacker can't queue-flood the
                // recipient with nested prompts. Decline and log so the
                // user knows someone tried.
                if dialog_open {
                    sender.to(
                        msg.sender,
                        OverlandsMessage::ItemOfferResponse {
                            offer_id,
                            target_did: sender_did.clone(),
                            accepted: false,
                        },
                        ChannelKind::Reliable,
                    );
                    session_log.info(now, EventPayload::ItemOfferAutoDeclinedBusy { offer_id });
                    crate::diagnostics::samplers::offer_auto_declined_busy(&mut metrics);
                    // Counted for the dialog's closing note (#843) — the
                    // decline itself stays invisible until then, preserving
                    // the single-dialog anti-spam invariant.
                    bufs.busy_declines.0 = bufs.busy_declines.0.saturating_add(1);
                    continue;
                }

                // Decode + sanitise the inbound generator. A malformed
                // payload or an Unknown variant is treated as a protocol
                // error — auto-decline and log.
                let Some(mut generator) = OverlandsMessage::decode_item_offer(&generator_json)
                else {
                    sender.to(
                        msg.sender,
                        OverlandsMessage::ItemOfferResponse {
                            offer_id,
                            target_did: sender_did.clone(),
                            accepted: false,
                        },
                        ChannelKind::Reliable,
                    );
                    session_log.warn(
                        now,
                        EventPayload::ItemOfferDecodeFailed {
                            reason: format!("from @{sender_handle}: failed to decode"),
                        },
                    );
                    continue;
                };
                crate::pds::sanitize_generator(&mut generator);
                // Wear metadata (#1108) through the same clamp the inventory
                // and attachment records share; a bad payload is decor.
                let wear = OverlandsMessage::decode_item_offer_wear(&wear_json).map(|mut meta| {
                    meta.sanitize();
                    meta
                });

                // Non-placeable kinds (terrain / water / Unknown) never
                // make sense as a gift — the sender UI already filters
                // these, but reject here too so a hand-crafted payload
                // can't stuff an unplaceable item into the recipient's
                // stash via the accept path.
                if !crate::ui::inventory::is_drop_placeable(&generator) {
                    sender.to(
                        msg.sender,
                        OverlandsMessage::ItemOfferResponse {
                            offer_id,
                            target_did: sender_did.clone(),
                            accepted: false,
                        },
                        ChannelKind::Reliable,
                    );
                    session_log.warn(
                        now,
                        EventPayload::ItemOfferRejected {
                            offer_id,
                            reason: format!("from @{sender_handle}: item kind not giftable"),
                        },
                    );
                    continue;
                }

                session_log.info(
                    now,
                    EventPayload::ItemOfferReceived {
                        offer_id,
                        sender_did: sender_did.clone(),
                        item_name: item_name.clone(),
                    },
                );
                commands.insert_resource(IncomingOfferDialog {
                    offer_id,
                    sender_peer_id: msg.sender,
                    sender_did,
                    sender_handle,
                    item_name,
                    generator,
                    wear,
                    arrived_at_secs: now,
                });
                // Slam the gate shut for the rest of this frame so any
                // further offers in the same drain auto-decline instead of
                // racing the deferred `insert_resource` above.
                dialog_open = true;
            }
            OverlandsMessage::ItemOfferResponse {
                offer_id,
                target_did,
                accepted,
            } => {
                // Gate on the local DID first: a response broadcast is
                // carrying our own sender-side offer_id only when
                // `target_did` equals our DID. Other peers drop it.
                let Some(sess) = session.as_deref() else {
                    continue;
                };
                if sess.did != target_did {
                    continue;
                }

                // Authenticate the responder's DID against the relay map
                // so a third-party peer can't impersonate the real
                // recipient and spoof an "accepted" reply to steal
                // visibility into what we gifted.
                let Some(responder_did) = peer_sessions.session_id(&msg.sender) else {
                    continue;
                };

                // Authenticate the responder's DID against the pending
                // offer's target BEFORE consuming the pending entry. A
                // prior implementation removed unconditionally, letting any
                // peer in the room race a spoofed "accepted" reply onto the
                // wire and silently delete the genuine target's pending
                // offer — permanently breaking gifting for the sender.
                match pending_offers.by_id.get(&offer_id) {
                    Some(pending) if pending.target_did != responder_did => continue,
                    None => continue,
                    _ => {}
                }

                // Consume the pending entry now that the responder is
                // authenticated — its handle + item name feed the sender's
                // outcome toast (#843).
                let Some(pending) = pending_offers.by_id.remove(&offer_id) else {
                    continue;
                };

                session_log.info(
                    now,
                    EventPayload::ItemOfferResponseReceived { offer_id, accepted },
                );
                // The sender finally learns the outcome somewhere visible
                // (#843). `accepted:false` covers declined / busy / muted
                // undifferentiated — the protocol carries no reason code.
                if accepted {
                    bufs.toasts.success(
                        format!(
                            "@{} accepted \"{}\".",
                            pending.target_handle, pending.item_name
                        ),
                        now,
                    );
                } else {
                    bufs.toasts.info(
                        format!(
                            "@{} declined \"{}\".",
                            pending.target_handle, pending.item_name
                        ),
                        now,
                    );
                }
            }
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
    /// presses Save to PDS. The records their references name now hold new
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
