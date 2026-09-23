//! What the daemon writes into its event log (#1416): the things that happen
//! to the agent without its asking.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;

use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::network::ChatDelivery;
use crate::state::{ChatHistory, CurrentRoomDid, RemotePeer};

use super::super::control::events::{EventKind, EventLog};

/// The daemon's handle on its event log, shared with the control socket.
#[derive(Resource, Clone)]
pub(super) struct EventSink(pub Arc<EventLog>);

/// `OnEnter(InGame)`: the agent is in a world - at sign-in, and again after
/// every trip.
pub(super) fn record_arrival(room: Option<Res<CurrentRoomDid>>, sink: Res<EventSink>) {
    if let Some(room) = room {
        sink.0.push(EventKind::EnteredWorld {
            room_did: room.0.clone(),
        });
    }
}

/// Who a peer was, kept so that their leaving can name them: a despawned
/// entity takes its [`RemotePeer`] with it.
#[derive(Default)]
pub(super) struct KnownPeers(HashMap<Entity, (String, Option<String>)>);

/// A peer joins once their identity is known - the relay vouches for a DID,
/// which is what a peer is announced by - and leaves when their entity goes,
/// whether they walked out or the agent did.
pub(super) fn record_peers(
    peers: Query<(Entity, &RemotePeer), Changed<RemotePeer>>,
    mut gone: RemovedComponents<RemotePeer>,
    mut known: Local<KnownPeers>,
    sink: Res<EventSink>,
) {
    for entity in gone.read() {
        if let Some((did, handle)) = known.0.remove(&entity) {
            sink.0.push(EventKind::PeerLeft { did, handle });
        }
    }
    for (entity, peer) in &peers {
        let Some(did) = peer.did.clone() else {
            continue;
        };
        match known.0.get_mut(&entity) {
            // The handle can land after the DID; later events carry it.
            Some(seen) => seen.1.clone_from(&peer.handle),
            None => {
                known.0.insert(entity, (did.clone(), peer.handle.clone()));
                sink.0.push(EventKind::PeerJoined {
                    did,
                    handle: peer.handle.clone(),
                });
            }
        }
    }
}

/// Chat from other players becomes `chat` events.
///
/// Read off [`ChatHistory`] by its push count, which only grows, rather than
/// by index into its capped, travel-cleared list. The agent's own lines and
/// the system's presence lines are left out: the agent knows what it said,
/// and joins, departures and arrivals are events of their own.
pub(super) fn record_chat(
    chat: Res<ChatHistory>,
    session: Option<Res<AtprotoSession>>,
    mut seen: Local<u64>,
    sink: Res<EventSink>,
) {
    // Logout replaces the history, count and all.
    if chat.pushed < *seen {
        *seen = 0;
    }
    let new = usize::try_from(chat.pushed - *seen).unwrap_or(usize::MAX);
    *seen = chat.pushed;
    let own = session.as_deref().map(|s| s.did.as_str());
    let arrived = &chat.messages[chat.messages.len().saturating_sub(new)..];
    for entry in arrived {
        let Some(did) = entry.did.as_deref() else {
            continue;
        };
        if entry.delivery != ChatDelivery::NotApplicable || Some(did) == own {
            continue;
        }
        sink.0.push(EventKind::Chat {
            from_did: did.to_owned(),
            from: entry.author.clone(),
            text: entry.text.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn peer(did: Option<&str>, handle: Option<&str>) -> RemotePeer {
        RemotePeer {
            peer_id: serde_json::from_str("\"00000000-0000-0000-0000-000000000002\"")
                .expect("a uuid"),
            did: did.map(str::to_owned),
            handle: handle.map(str::to_owned),
            muted: false,
            avatar: None,
            build: None,
            connected_at: 0.0,
        }
    }

    fn app() -> (App, Arc<EventLog>) {
        let log = Arc::new(EventLog::new(16, "test".into()));
        let mut app = App::new();
        app.insert_resource(EventSink(Arc::clone(&log)))
            .add_systems(Update, record_peers);
        (app, log)
    }

    fn kinds(log: &EventLog) -> Vec<EventKind> {
        log.after(0, Duration::ZERO)
            .events
            .into_iter()
            .map(|e| e.what)
            .collect()
    }

    /// THE SEQUENCE: a peer's entity appears before its identity does, the
    /// DID lands, then the handle, then the peer goes. One join - when there
    /// is someone to name - and one leave, naming them with what was
    /// learned since.
    #[test]
    fn a_peer_joins_when_named_and_leaves_with_its_name() {
        let (mut app, log) = app();
        let entity = app.world_mut().spawn(peer(None, None)).id();
        app.update();
        assert!(kinds(&log).is_empty(), "nobody to name yet");

        app.world_mut()
            .entity_mut(entity)
            .insert(peer(Some("did:plc:bob"), None));
        app.update();
        app.world_mut()
            .entity_mut(entity)
            .insert(peer(Some("did:plc:bob"), Some("bob.test")));
        app.update();
        app.world_mut().entity_mut(entity).despawn();
        app.update();

        assert_eq!(
            kinds(&log),
            [
                EventKind::PeerJoined {
                    did: "did:plc:bob".into(),
                    handle: None
                },
                EventKind::PeerLeft {
                    did: "did:plc:bob".into(),
                    handle: Some("bob.test".into())
                },
            ]
        );
    }

    /// THE SEQUENCE: a peer speaks, the agent speaks, the system announces a
    /// departure, the history is cleared by travel and another peer speaks.
    /// Only the two peers' lines become events, each once.
    #[test]
    fn only_other_players_lines_become_chat_events() {
        let log = Arc::new(EventLog::new(16, "test".into()));
        let mut app = App::new();
        app.insert_resource(EventSink(Arc::clone(&log)))
            .init_resource::<ChatHistory>()
            .add_systems(Update, record_chat);

        let push = |app: &mut App, did: Option<&str>, author: &str, text: &str| {
            app.world_mut().resource_mut::<ChatHistory>().push(
                did.map(str::to_owned),
                author,
                text,
            );
        };
        push(&mut app, Some("did:plc:bob"), "bob.test", "hi agent");
        app.world_mut().resource_mut::<ChatHistory>().push_sent(
            Some("did:plc:agent".into()),
            "agent.test",
            "hello bob",
            ChatDelivery::Reached(1),
        );
        push(&mut app, None, "system", "bob.test left the room.");
        app.update();
        app.world_mut()
            .resource_mut::<ChatHistory>()
            .messages
            .clear();
        push(&mut app, Some("did:plc:carol"), "carol.test", "welcome");
        app.update();
        app.update();

        let chat: Vec<(String, String)> = log
            .after(0, Duration::ZERO)
            .events
            .into_iter()
            .filter_map(|e| match e.what {
                EventKind::Chat { from_did, text, .. } => Some((from_did, text)),
                _ => None,
            })
            .collect();
        assert_eq!(
            chat,
            [
                ("did:plc:bob".to_owned(), "hi agent".to_owned()),
                ("did:plc:carol".to_owned(), "welcome".to_owned()),
            ]
        );
    }
}
