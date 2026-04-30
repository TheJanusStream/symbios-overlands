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

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::avatar::AvatarFetchPending;
use crate::config;
use crate::pds::RoomRecord;
use crate::protocol::OverlandsMessage;
use crate::state::{
    ChatHistory, CurrentRoomDid, DiagnosticsLog, IncomingOfferDialog, PendingOutgoingOffers,
    RemotePeer, TransformBuffer, TransformSample,
};

use super::peer_cache::{PeerAvatarCache, spawn_peer_avatar_fetch};

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn handle_incoming_messages(
    mut commands: Commands,
    mut queue: ResMut<NetworkQueue<OverlandsMessage>>,
    mut chat: ResMut<ChatHistory>,
    mut peers: Query<(
        Entity,
        &mut RemotePeer,
        &mut Transform,
        &mut TransformBuffer,
    )>,
    time: Res<Time>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut room_record: Option<ResMut<RoomRecord>>,
    peer_sessions: Res<PeerSessionMapRes>,
    session: Option<Res<AtprotoSession>>,
    mut diagnostics: ResMut<DiagnosticsLog>,
    incoming_dialog: Option<Res<IncomingOfferDialog>>,
    mut pending_offers: ResMut<PendingOutgoingOffers>,
    mut offer_writer: MessageWriter<Broadcast<OverlandsMessage>>,
    mut avatar_cache: ResMut<PeerAvatarCache>,
) {
    let now = time.elapsed_secs_f64();
    // Drain the whole queue into a buffer so we can dedupe `Identity`
    // messages per sender. A burst of Identity messages would otherwise
    // fire N redundant avatar fetches against the peer's PDS.
    let messages: Vec<_> = queue.drain().collect();
    let mut last_identity_idx: std::collections::HashMap<PeerId, usize> =
        std::collections::HashMap::new();
    for (i, msg) in messages.iter().enumerate() {
        if matches!(msg.payload, OverlandsMessage::Identity { .. }) {
            last_identity_idx.insert(msg.sender, i);
        }
    }
    for (i, msg) in messages.into_iter().enumerate() {
        if matches!(msg.payload, OverlandsMessage::Identity { .. })
            && last_identity_idx.get(&msg.sender) != Some(&i)
        {
            continue;
        }
        match msg.payload {
            OverlandsMessage::Transform { position, rotation } => {
                for (_, peer, _tf, mut buf) in peers.iter_mut() {
                    if peer.peer_id == msg.sender {
                        // Assign a *playout* timestamp rather than the raw
                        // arrival time.  WebRTC data channels frequently
                        // deliver bursts of 2–3 packets in the same frame;
                        // stamping them with identical `now` values collapses
                        // the Hermite spline's dt to ~0 and launches the
                        // remote mesh to infinity via a divide-by-near-zero
                        // velocity tangent.  Instead, advance the stamp by the
                        // expected send interval, anchored to `now` so bursts
                        // can't drift arbitrarily into the future.
                        //
                        // The `(last + expected).max(now)` form alone would
                        // wind up permanently if the sender's clock runs
                        // faster than ours — every packet pushes `next`
                        // slightly ahead of `now`, and after a few minutes
                        // `render_time = now - delay` falls behind the
                        // oldest sample and the spline degenerates into a
                        // snap to the earliest buffered sample. Clamp the
                        // forward drift so the assigned timestamp can never
                        // sit more than `MAX_JITTER_DRIFT_SECS` ahead of
                        // `now`, rebasing to live wall-clock on overrun.
                        let expected = config::network::EXPECTED_BROADCAST_INTERVAL_SECS;
                        let max_drift = config::network::MAX_JITTER_DRIFT_SECS;
                        let raw_next = match buf.samples.back() {
                            Some(last) => (last.timestamp + expected).max(now),
                            None => now,
                        };
                        let ceiling = now + max_drift;
                        let next = if raw_next > ceiling {
                            ceiling
                        } else {
                            raw_next
                        };
                        // Reject non-finite positions and normalise the
                        // quaternion before it reaches `Quat::slerp` in the
                        // kinematic smoother — `slerp` on an unnormalised or
                        // NaN quat propagates NaN into every peer's
                        // Transform, which then NaN-poisons the avian3d
                        // broadphase for the *local* rigid body. Drop
                        // garbage packets silently; the peer broadcasts
                        // Transform at 60 Hz so the next well-formed one
                        // overrides within ~16 ms.
                        let pos_vec = Vec3::from_array(position);
                        if !pos_vec.is_finite() {
                            continue;
                        }
                        // `is_finite` accepts `f32::MAX`, but subtracting two
                        // such values inside the Hermite tangent computation
                        // below overflows to `+Inf`, producing NaN/Inf in
                        // `Transform.translation` and poisoning the avian3d
                        // broadphase. Reject any component whose magnitude
                        // exceeds the play-space bound so arithmetic stays
                        // well clear of the f32 overflow threshold.
                        if pos_vec.abs().max_element() > config::network::MAX_REMOTE_COORD_ABS {
                            continue;
                        }
                        let raw_rot = Quat::from_array(rotation);
                        let rot = if raw_rot.is_finite() && raw_rot.length_squared() > 1e-6 {
                            raw_rot.normalize()
                        } else {
                            Quat::IDENTITY
                        };
                        buf.samples.push_back(TransformSample {
                            position: pos_vec,
                            rotation: rot,
                            timestamp: next,
                        });
                        while buf.samples.len() > config::network::KINEMATIC_BUFFER_CAPACITY {
                            buf.samples.pop_front();
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
                        warn!(
                            "Rejecting spoofed Identity from {}: claimed did={}, authenticated did={}",
                            msg.sender, did, authenticated_did
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
                            spawn_peer_avatar_fetch(&mut commands, msg.sender, did.clone());
                        }
                    }
                }
            }
            OverlandsMessage::AvatarStateUpdate { record_json } => {
                // Live preview nudge from a peer who is mid-edit. Decode,
                // authenticate against the already-validated DID, sanitize,
                // then overwrite `peer.avatar` so the hot-swap system in
                // `player.rs` rebuilds the visual next frame.
                let Some(mut new_record) = OverlandsMessage::decode_avatar_state(&record_json)
                else {
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
                    // Only accept live updates from peers whose DID we have
                    // already authenticated via Identity — otherwise a peer
                    // that connected before its session bound could smuggle
                    // an avatar under an empty DID and never be swept.
                    let Some(peer_did) = peer.did.clone() else {
                        debug!(
                            "Deferring AvatarStateUpdate from {}: peer DID not yet known",
                            msg.sender
                        );
                        continue;
                    };
                    // Refresh the cache so a future Identity from this DID
                    // (e.g. reconnect within the session) restores the
                    // live-preview state instead of the stale PDS record.
                    avatar_cache.insert(peer_did, new_record.clone());
                    peer.avatar = Some(new_record);
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
                    **record = new_record;
                    info!("Room state updated from owner broadcast");
                }
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
                    let ts = crate::format_elapsed_ts(now);
                    chat.messages.push(crate::state::ChatEntry {
                        did,
                        author,
                        text: clipped,
                        timestamp: ts,
                    });
                    // Bound the rolling history so a chatty peer can't grow
                    // the scroll area unbounded — each entry re-wraps every
                    // frame once it's in egui's text layout cache.
                    let cap = crate::config::ui::chat::MAX_HISTORY_ENTRIES;
                    if chat.messages.len() > cap {
                        let drop = chat.messages.len() - cap;
                        chat.messages.drain(..drop);
                    }
                }
            }
            OverlandsMessage::ItemOffer {
                offer_id,
                target_did,
                item_name,
                generator_json,
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
                    offer_writer.write(Broadcast {
                        payload: OverlandsMessage::ItemOfferResponse {
                            offer_id,
                            target_did: sender_did.clone(),
                            accepted: false,
                        },
                        channel: ChannelKind::Reliable,
                    });
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

                // Busy-gate: a dialog is already up, so an attacker can't
                // queue-flood the recipient with nested prompts. Decline
                // and log so the user knows someone tried.
                if incoming_dialog.is_some() {
                    offer_writer.write(Broadcast {
                        payload: OverlandsMessage::ItemOfferResponse {
                            offer_id,
                            target_did: sender_did.clone(),
                            accepted: false,
                        },
                        channel: ChannelKind::Reliable,
                    });
                    diagnostics.push(
                        now,
                        format!(
                            "Auto-declined offer \"{item_name}\" from @{sender_handle} (already handling an offer)"
                        ),
                    );
                    continue;
                }

                // Decode + sanitise the inbound generator. A malformed
                // payload or an Unknown variant is treated as a protocol
                // error — auto-decline and log.
                let Some(mut generator) = OverlandsMessage::decode_item_offer(&generator_json)
                else {
                    offer_writer.write(Broadcast {
                        payload: OverlandsMessage::ItemOfferResponse {
                            offer_id,
                            target_did: sender_did.clone(),
                            accepted: false,
                        },
                        channel: ChannelKind::Reliable,
                    });
                    diagnostics.push(
                        now,
                        format!(
                            "Dropped malformed item offer from @{sender_handle}: failed to decode"
                        ),
                    );
                    continue;
                };
                crate::pds::sanitize_generator(&mut generator);

                // Non-placeable kinds (terrain / water / Unknown) never
                // make sense as a gift — the sender UI already filters
                // these, but reject here too so a hand-crafted payload
                // can't stuff an unplaceable item into the recipient's
                // stash via the accept path.
                if !crate::ui::inventory::is_drop_placeable(&generator) {
                    offer_writer.write(Broadcast {
                        payload: OverlandsMessage::ItemOfferResponse {
                            offer_id,
                            target_did: sender_did.clone(),
                            accepted: false,
                        },
                        channel: ChannelKind::Reliable,
                    });
                    diagnostics.push(
                        now,
                        format!(
                            "Rejected offer \"{item_name}\" from @{sender_handle}: item kind not giftable"
                        ),
                    );
                    continue;
                }

                diagnostics.push(
                    now,
                    format!("Received offer \"{item_name}\" from @{sender_handle}"),
                );
                commands.insert_resource(IncomingOfferDialog {
                    offer_id,
                    sender_peer_id: msg.sender,
                    sender_did,
                    sender_handle,
                    item_name,
                    generator,
                    arrived_at_secs: now,
                });
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

                let Some(pending) = pending_offers.by_id.remove(&offer_id) else {
                    continue;
                };

                let outcome = if accepted { "accepted" } else { "declined" };
                diagnostics.push(
                    now,
                    format!(
                        "@{} {} offer \"{}\"",
                        pending.target_handle, outcome, pending.item_name
                    ),
                );
            }
        }
    }
}
