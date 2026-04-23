//! P2P networking plugin: peer lifecycle, inbound dispatch, outbound
//! throttling, and the jitter-buffered kinematic smoother.
//!
//! Outbound `Transform` broadcasts are driven by `FixedUpdate` (not `Update`)
//! so the packet rate is independent of render FPS.  When the local rover is
//! nearly stationary the broadcast rate drops from ~60 Hz to ~2 Hz to save
//! bandwidth and downstream CPU — with a forced "final frame" broadcast on
//! the tick we cross into rest so remote peers land on the true parked pose.
//!
//! Inbound `Transform` samples are pushed into a per-peer ring buffer and
//! replayed `KINEMATIC_RENDER_DELAY_SECS` in the past; the playout position
//! is resolved with a cubic Hermite spline whose endpoint tangents come from
//! central differences of the buffered samples.  Identity messages are
//! authenticated against the relay-signed `PeerSessionMapRes` so a peer
//! cannot impersonate another DID over the unauthenticated data channel.
//!
//! Avatar records are sovereign: after a peer announces its DID, we spawn
//! an async `fetch_avatar_record` task against that peer's PDS. A live
//! preview nudge via `AvatarStateUpdate` lets remote peers mirror
//! mid-slider edits before the author presses "Publish".

use avian3d::prelude::{AngularVelocity, LinearVelocity};
use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::avatar::AvatarFetchPending;
use crate::config;
use crate::pds::{self, AvatarRecord, RoomRecord};
use crate::protocol::OverlandsMessage;
use crate::state::{
    AppState, ChatHistory, CurrentRoomDid, DiagnosticsLog, IncomingOfferDialog, LiveAvatarRecord,
    LocalPlayer, LocalSettings, PendingOutgoingOffers, RemotePeer, TransformBuffer,
    TransformSample,
};

pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(SymbiosMultiuserPlugin::<OverlandsMessage>::deferred())
            .add_systems(
                Update,
                (
                    handle_peer_connections,
                    handle_incoming_messages,
                    poll_peer_avatar_fetches,
                    smooth_remote_transforms,
                    sync_mute_visibility,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            // Network broadcast is tied to a fixed tick so the outbound rate
            // is independent of rendering FPS — otherwise a 144 Hz monitor
            // would blast peers with 2.4× the intended packet rate and a
            // 30 Hz machine would stutter.
            .add_systems(
                FixedUpdate,
                broadcast_local_state.run_if(in_state(AppState::InGame)),
            )
            // Live-preview avatar and room updates piggyback on `Update` so
            // they fire the frame an editor slider changes the resource,
            // rather than waiting for the next FixedUpdate tick.
            .add_systems(
                Update,
                (broadcast_avatar_state, broadcast_room_state).run_if(in_state(AppState::InGame)),
            );
    }
}

/// In-flight `fetch_avatar_record` task attached to a throwaway entity so
/// the `poll_peer_avatar_fetches` system can drain it without a dedicated
/// resource. The `peer_id` field identifies which remote peer the result
/// belongs to — the peer's ECS entity may have despawned by the time the
/// task completes (late disconnect), so the poller has to look it up.
#[derive(Component)]
struct PeerAvatarFetchTask {
    peer_id: PeerId,
    did: String,
    task: bevy::tasks::Task<Result<Option<AvatarRecord>, pds::FetchError>>,
}

fn spawn_peer_avatar_fetch(commands: &mut Commands, peer_id: PeerId, did: String) {
    // `IoTaskPool` is the correct home for blocking HTTP calls — the
    // `AsyncComputeTaskPool` is sized to the CPU-core count and must not be
    // starved by threads blocked on network sockets.
    let pool = bevy::tasks::IoTaskPool::get();
    let did_for_fetch = did.clone();
    let task = pool.spawn(async move {
        let fut = async {
            let client = config::http::default_client();
            pds::fetch_avatar_record(&client, &did_for_fetch).await
        };
        #[cfg(target_arch = "wasm32")]
        {
            fut.await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(fut)
        }
    });
    commands.spawn(PeerAvatarFetchTask { peer_id, did, task });
}

#[allow(clippy::too_many_arguments)]
fn handle_peer_connections(
    mut commands: Commands,
    mut peer_events: ResMut<PeerStateQueue<OverlandsMessage>>,
    mut diagnostics: ResMut<DiagnosticsLog>,
    peers: Query<(Entity, &RemotePeer)>,
    time: Res<Time>,
    session: Option<Res<AtprotoSession>>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
) {
    let elapsed = time.elapsed_secs_f64();
    for event in peer_events.drain() {
        match event.state {
            PeerConnectionState::Connected => {
                diagnostics.push(elapsed, format!("[+] Peer {} connected", event.peer));
                // Spawn the peer with no avatar yet — the hot-swap system in
                // `player.rs` will build visuals once the PDS fetch populates
                // `RemotePeer::avatar`. Leaving the vessel invisible until
                // then is deliberate: a guessed default would be indistinguishable
                // from a deliberately-minimal avatar and mislead the other
                // players about the peer's real appearance.
                commands.spawn((
                    Transform::from_xyz(0.0, 10.0, 0.0),
                    Visibility::default(),
                    RemotePeer {
                        peer_id: event.peer,
                        did: None,
                        handle: None,
                        muted: false,
                        avatar: None,
                    },
                    TransformBuffer::default(),
                ));

                // Proactively announce our identity to the newcomer.  Without
                // this, they only learn our DID on the next scheduled identity
                // broadcast (~1 s), during which a RoomStateUpdate from us
                // would fail the owner-DID check and be silently dropped.
                if let Some(sess) = &session {
                    writer.write(Broadcast {
                        payload: OverlandsMessage::Identity {
                            did: sess.did.clone(),
                            handle: sess.handle.clone(),
                        },
                        channel: ChannelKind::Reliable,
                    });
                }
            }
            PeerConnectionState::Disconnected => {
                for (entity, peer) in peers.iter() {
                    if peer.peer_id == event.peer {
                        let label = peer
                            .handle
                            .as_deref()
                            .or(peer.did.as_deref())
                            .unwrap_or("unknown");
                        diagnostics.push(
                            elapsed,
                            format!("[-] Peer {} ({}) disconnected", event.peer, label),
                        );
                        commands.entity(entity).despawn();
                    }
                }
            }
        }
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn handle_incoming_messages(
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
                        // Trigger the sovereign PDS fetch — the peer's
                        // AvatarRecord lives on their own server, not on the
                        // wire. `poll_peer_avatar_fetches` applies the result
                        // when it arrives.
                        spawn_peer_avatar_fetch(&mut commands, msg.sender, did.clone());
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
                    if peer.did.is_none() {
                        debug!(
                            "Deferring AvatarStateUpdate from {}: peer DID not yet known",
                            msg.sender
                        );
                        continue;
                    }
                    peer.avatar = Some(new_record);
                    break;
                }
            }
            OverlandsMessage::RoomStateUpdate { record_json } => {
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

                // Only accept from the peer whose DID matches the room owner.
                let sender_did = peers
                    .iter()
                    .find(|(_, peer, _, _)| peer.peer_id == msg.sender)
                    .and_then(|(_, peer, _, _)| peer.did.clone());

                let is_owner = match (&sender_did, &room_did) {
                    (Some(did), Some(rd)) => did == &rd.0,
                    _ => false,
                };

                // Clamp every unbounded numeric field before the world
                // compiler touches the recipe — a malicious owner could
                // otherwise ship a grid_size or L-system iteration count
                // designed to OOM every guest.
                new_record.sanitize();

                // Replace the whole recipe. `world_builder::compile_room_record`
                // observes the resource change and rebuilds every compiled
                // entity (water, sun colour, scattered shapes) in one pass.
                if is_owner && let Some(record) = room_record.as_mut() {
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
                    let author = peers
                        .iter()
                        .find(|(_, peer, _, _)| peer.peer_id == msg.sender)
                        .and_then(|(_, peer, _, _)| peer.handle.clone())
                        .unwrap_or_else(|| msg.sender.to_string());
                    let ts = crate::format_elapsed_ts(now);
                    chat.messages.push((author, clipped, ts));
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

                // Clamp the item name to the same reasonable bounds the
                // rest of the editor enforces so a hostile payload can't
                // blow up the dialog's text layout.
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

                let Some(pending) = pending_offers.by_id.remove(&offer_id) else {
                    // Either this response is for an offer we already
                    // resolved (duplicate reply), or it's a reply to an
                    // offer we never sent. Either way, drop it silently.
                    continue;
                };

                // Drop responses whose responder DID doesn't match the
                // originally-targeted peer. Without this, any peer could
                // observe the offer on the wire and race an "accepted"
                // reply back before the real target answered.
                if responder_did != pending.target_did {
                    continue;
                }

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

/// Drain completed peer-avatar fetch tasks and install the fetched record
/// onto the matching `RemotePeer`. A 404 means the peer has never published
/// an avatar, in which case we synthesise the deterministic default keyed
/// off their DID so their vessel is still distinguishable from other
/// "unpublished" peers.
fn poll_peer_avatar_fetches(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PeerAvatarFetchTask)>,
    mut peers: Query<&mut RemotePeer>,
    mut diagnostics: ResMut<DiagnosticsLog>,
    time: Res<Time>,
) {
    let elapsed = time.elapsed_secs_f64();
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        let peer_id = task.peer_id;
        let did = task.did.clone();
        commands.entity(entity).despawn();

        let mut record = match result {
            Ok(Some(r)) => r,
            Ok(None) => {
                info!(
                    "Peer {} ({}) has no avatar record — synthesising default",
                    peer_id, did
                );
                AvatarRecord::default_for_did(&did)
            }
            Err(err) => {
                diagnostics.push(
                    elapsed,
                    format!("Avatar fetch failed for {peer_id}: {err:?} — using default"),
                );
                warn!(
                    "Avatar fetch failed for {} ({}): {:?} — falling back to default",
                    peer_id, did, err
                );
                AvatarRecord::default_for_did(&did)
            }
        };
        record.sanitize();

        // Find the live peer entity; it may have despawned if the peer
        // disconnected between the fetch kick-off and its completion.
        if let Some(mut peer) = peers.iter_mut().find(|p| p.peer_id == peer_id) {
            peer.avatar = Some(record);
        }
    }
}

fn broadcast_local_state(
    query: Query<(&Transform, &LinearVelocity, &AngularVelocity), With<LocalPlayer>>,
    session: Option<Res<AtprotoSession>>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
    mut tick: Local<u32>,
    mut was_stationary: Local<bool>,
) {
    *tick = tick.wrapping_add(1);

    let Ok((tf, lin_vel, ang_vel)) = query.single() else {
        return;
    };

    // Throttle transform broadcasts when nearly stationary: drop from ~60 Hz
    // to ~2 Hz (every 30th tick) to save WebRTC bandwidth and WASM CPU.
    // Check both linear *and* angular velocity so a spinning-in-place rover
    // still streams smooth rotation updates to peers.
    let stationary = lin_vel.0.length() <= config::network::STATIONARY_SPEED_THRESHOLD
        && ang_vel.0.length() <= config::network::STATIONARY_ANGULAR_THRESHOLD;
    // Force one final broadcast on the tick we cross into rest so peers land
    // on the true parked pose instead of interpolating toward a stale sample.
    let just_came_to_rest = stationary && !*was_stationary;
    *was_stationary = stationary;
    let should_send = !stationary
        || just_came_to_rest
        || tick.is_multiple_of(config::network::STATIONARY_BROADCAST_DIVISOR);

    if should_send {
        writer.write(Broadcast {
            payload: OverlandsMessage::Transform {
                position: tf.translation.to_array(),
                rotation: tf.rotation.to_array(),
            },
            channel: ChannelKind::Unreliable,
        });
    }

    if tick.is_multiple_of(config::network::IDENTITY_BROADCAST_INTERVAL_TICKS)
        && let Some(sess) = &session
    {
        writer.write(Broadcast {
            payload: OverlandsMessage::Identity {
                did: sess.did.clone(),
                handle: sess.handle.clone(),
            },
            channel: ChannelKind::Reliable,
        });
    }
}

/// Broadcast a live-preview `AvatarStateUpdate` whenever the local avatar
/// resource changes, so peers can mirror the edit before the author commits
/// to Publish. Runs in `Update` so that UI mutations observed this frame
/// propagate the same frame, bypassing the fixed-timestep throttle used for
/// continuous transforms.
fn broadcast_avatar_state(
    live: Res<LiveAvatarRecord>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
) {
    if !live.is_changed() {
        return;
    }
    writer.write(Broadcast {
        payload: OverlandsMessage::avatar_state_update(&live.0),
        channel: ChannelKind::Reliable,
    });
}

/// Broadcast a live-preview `RoomStateUpdate` whenever the owner mutates
/// the `RoomRecord` resource so guests mirror the edit the same frame the
/// slider moves. The `session.did == room_did.0` gate ensures guests
/// (whose `RoomRecord` is also rewritten by inbound `RoomStateUpdate`
/// handling) do not echo the owner's broadcast back to the relay.
fn broadcast_room_state(
    record: Option<Res<RoomRecord>>,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
) {
    let (Some(record), Some(session), Some(room_did)) = (record, session, room_did) else {
        return;
    };
    if session.did != room_did.0 {
        return;
    }
    if !record.is_changed() {
        return;
    }
    writer.write(Broadcast {
        payload: OverlandsMessage::room_state_update(&record),
        channel: ChannelKind::Reliable,
    });
}

/// Resolve each remote peer's displayed transform from the jitter buffer.
///
/// When `smooth_kinematics` is enabled we evaluate a cubic Hermite spline at
/// `now - KINEMATIC_RENDER_DELAY_SECS`, using central-difference tangents of
/// the buffered samples for the translation and `Quat::slerp` for the
/// rotation.  When disabled, we snap straight to the most recent sample — a
/// useful debugging mode for observing raw network latency.
fn smooth_remote_transforms(
    time: Res<Time>,
    settings: Res<LocalSettings>,
    mut peers: Query<(&mut Transform, &mut TransformBuffer), With<RemotePeer>>,
) {
    let now = time.elapsed_secs_f64();
    let render_time = now - config::network::KINEMATIC_RENDER_DELAY_SECS;

    for (mut tf, mut buf) in peers.iter_mut() {
        if buf.samples.is_empty() {
            continue;
        }

        // Raw-snap mode — just follow the latest packet and keep the buffer
        // trimmed so a later mode flip doesn't jump back in time.
        if !settings.smooth_kinematics {
            if let Some(last) = buf.samples.back() {
                tf.translation = last.position;
                tf.rotation = last.rotation;
            }
            // Drop all but the most recent sample to bound memory.
            while buf.samples.len() > 1 {
                buf.samples.pop_front();
            }
            continue;
        }

        // Evict samples that are clearly older than render_time to avoid
        // unbounded growth while keeping at least one sample on either side.
        let prune_cutoff =
            render_time - 2.0 * config::network::KINEMATIC_RENDER_DELAY_SECS.max(0.05);
        while buf.samples.len() > 2
            && buf.samples.get(1).map(|s| s.timestamp).unwrap_or(f64::MAX) < prune_cutoff
        {
            buf.samples.pop_front();
        }

        // Find the segment [i, i+1] that brackets render_time.  If render_time
        // is before the first sample we simply snap to the earliest; if it's
        // past the last, we extrapolate by snapping to the latest.
        let samples = &buf.samples;
        if samples.len() == 1 || render_time <= samples.front().unwrap().timestamp {
            let s = samples.front().unwrap();
            tf.translation = s.position;
            tf.rotation = s.rotation;
            continue;
        }
        if render_time >= samples.back().unwrap().timestamp {
            let s = samples.back().unwrap();
            tf.translation = s.position;
            tf.rotation = s.rotation;
            continue;
        }

        // Walk to find the bracketing pair.
        let mut i = 0;
        while i + 1 < samples.len() && samples[i + 1].timestamp < render_time {
            i += 1;
        }
        let a = samples[i];
        let b = samples[i + 1];
        let dt = (b.timestamp - a.timestamp).max(1e-6);
        let t = ((render_time - a.timestamp) / dt).clamp(0.0, 1.0) as f32;

        // Estimate velocity tangents with a central difference.  Fall back to
        // forward/backward differences at the ends of the buffer so we always
        // have a well-defined tangent.
        let dt_f = dt as f32;
        let tangent_a = if i > 0 {
            let prev = samples[i - 1];
            let total = (b.timestamp - prev.timestamp).max(1e-6) as f32;
            (b.position - prev.position) / total * dt_f
        } else {
            b.position - a.position
        };
        let tangent_b = if i + 2 < samples.len() {
            let next = samples[i + 2];
            let total = (next.timestamp - a.timestamp).max(1e-6) as f32;
            (next.position - a.position) / total * dt_f
        } else {
            b.position - a.position
        };

        // Cubic Hermite basis.  Equivalent to bevy_math::CubicHermite over a
        // single segment but skips the Vec allocation and Result unwrapping.
        let t2 = t * t;
        let t3 = t2 * t;
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        tf.translation = a.position * h00 + tangent_a * h10 + b.position * h01 + tangent_b * h11;
        tf.rotation = a.rotation.slerp(b.rotation, t);
    }
}

/// Propagate each peer's mute flag to its `Visibility` component so that
/// muted vessels and their child meshes are hidden automatically.
fn sync_mute_visibility(mut peers: Query<(&RemotePeer, &mut Visibility)>) {
    for (peer, mut vis) in peers.iter_mut() {
        let desired = if peer.muted {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *vis != desired {
            *vis = desired;
        }
    }
}
