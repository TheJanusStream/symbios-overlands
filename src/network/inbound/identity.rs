//! Who a peer is, and what they are running (#1161).
//!
//! Both messages here are **identity claims made by a peer about itself**,
//! which is why they share a file: `Identity` carries a DID and a handle,
//! and every use of it downstream — the nametag, the mute list, the avatar
//! fetch — trusts that DID. The check that makes it trustworthy is the
//! relay's `session_id` binding, applied here and nowhere else.

use bevy::prelude::*;
use bevy_symbios_multiuser::prelude::*;

use super::{InboundBuffers, PeerParts};
use crate::diagnostics::SessionLog;
use crate::diagnostics::event::EventPayload;
use crate::network::peer_cache::PeerAvatarCache;
use crate::network::presence::adopt_peer_did;
/// Adopt a peer's claimed DID and handle, if the relay agrees it is theirs.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle(
    sender: PeerId,
    did: String,
    handle: String,
    commands: &mut Commands,
    peers: &mut Query<PeerParts>,
    peer_sessions: &PeerSessionMapRes,
    session_log: &mut SessionLog,
    avatar_cache: &mut PeerAvatarCache,
    metrics: &mut crate::diagnostics::MetricsRegistry,
    bufs: &mut InboundBuffers,
    now: f64,
) {
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
    match peer_sessions.session_id(&sender) {
        Some(authenticated_did) if authenticated_did == did => {}
        Some(authenticated_did) => {
            crate::diagnostics::samplers::identity_spoof_rejected(metrics);
            warn!(
                "Rejecting spoofed Identity from {}: claimed did={}, authenticated did={}",
                sender, did, authenticated_did
            );
            session_log.warn(
                now,
                EventPayload::PeerIdentitySpoofRejected {
                    peer: sender.to_string(),
                    claimed_did: did,
                    authenticated_did: authenticated_did.to_string(),
                },
            );
            return;
        }
        None => {
            debug!("Deferring Identity from {}: session not yet known", sender);
            return;
        }
    }

    for (entity, mut peer, _, _) in peers.iter_mut() {
        if peer.peer_id != sender {
            continue;
        }

        // The `handle` field on the wire is peer-supplied and
        // therefore untrusted — a malicious peer could claim any
        // handle string to impersonate another actor in the chat
        // HUD and disconnect log. The authoritative handle is
        // resolved asynchronously by the avatar/profile fetch
        // pipeline (kicked via `AvatarFetchPending`), which hits
        // `app.bsky.actor.getProfile` against the DID the relay
        // already authenticated. Do NOT write `peer.handle` from
        // this message.
        //
        // Normally a no-op now (#1218 f290):
        // `presence::adopt_peer_sessions` runs before this
        // dispatcher and takes the DID straight off the same
        // relay-signed map this arm authenticates against, so a
        // peer is identified whether or not they ever broadcast.
        // The call stays because the two must not be able to
        // adopt an identity two different ways, and this is where
        // a spoof is caught.
        let peer_id = peer.peer_id;
        if adopt_peer_did(
            commands,
            entity,
            &mut peer,
            peer_id,
            &did,
            &mut bufs.muted_dids,
            avatar_cache,
            now,
        ) {
            info!(
                "Peer {} identified as did={} (claimed handle @{} — unverified, will resolve via getProfile)",
                sender, did, handle
            );
        }
    }
}

/// Record a peer's protocol/build announcement, and warn on a mismatch.
pub(super) fn handle_hello(
    sender: PeerId,
    protocol: u16,
    build: String,
    peers: &mut Query<PeerParts>,
    session_log: &mut SessionLog,
    now: f64,
) {
    // The peer named its wire layout (#1121). Not authenticated
    // and not authoritative — a peer can claim any number — but
    // it does not need to be either: nothing is refused on the
    // strength of it, so the worst a liar achieves is a wrong
    // chip on its own row in the People window.
    //
    // Recorded on the peer rather than acted on, because the
    // failure this describes has already happened by the time we
    // could act: a message from an incompatible build does not
    // arrive as a wrong message, it fails to decode inside
    // `bevy_symbios_multiuser` and never reaches this dispatcher
    // at all. All that is left to do is say which two builds
    // could not talk.
    let announced = crate::state::PeerBuild {
        protocol,
        build: build.clone(),
    };
    for (_, mut peer, _, _) in peers.iter_mut() {
        if peer.peer_id != sender {
            continue;
        }
        // Re-announcements arrive every second; only a CHANGE is
        // news, so the log records one line per disagreement and
        // not one per second of it.
        if peer.build.as_ref() == Some(&announced) {
            break;
        }
        let ours = crate::protocol::PROTOCOL_VERSION;
        peer.build = Some(announced);
        if protocol != ours {
            warn!(
                "Peer {} speaks protocol {} ({}), we speak {} — messages between us may not decode",
                sender, protocol, build, ours
            );
            session_log.error(
                now,
                EventPayload::PeerProtocolMismatch {
                    peer: sender.to_string(),
                    ours,
                    theirs: Some(protocol),
                    build,
                },
            );
        }
        break;
    }
}
