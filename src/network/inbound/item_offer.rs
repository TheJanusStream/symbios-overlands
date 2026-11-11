//! Peer-to-peer gifting: the offer and the answer (#1161).
//!
//! The largest pair of arms in the protocol and the one with the most gates,
//! because an `ItemOffer` asks this client to put a stranger's item on
//! screen and, if accepted, into the owner's stash. Between the wire and the
//! dialog sit: the relay's DID binding, the mute list, the busy-gate (one
//! dialog at a time, #843/#1220 f288) - all three before anything the payload
//! holds is decoded - and then what the item is (#1844): a thing passes
//! `pds::inventory::is_drop_placeable`, the rule that decides whether a
//! stranger's generator is safe to place at all; a whole avatar passes the
//! stash's own sanitiser and must be readable by this build; a whole world's
//! head passes `pds::inventory::world::check_head` and its summary's
//! sanitiser, and nothing of its contents is read here - they are fetched
//! from the sender's data server only after Accept. Each refusal answers
//! `Unavailable`, writes one session-log line, and raises no dialog.

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use super::{InboundBuffers, PeerParts};
use crate::diagnostics::SessionLog;
use crate::diagnostics::event::EventPayload;
use crate::network::presence::PeerLabel;
use crate::protocol::{DeclineReason, GiftItem, OverlandsMessage};
use crate::state::{BusyAutoDeclines, IncomingOfferDialog, PendingOutgoingOffers};

/// Decide what to do with a stranger's gift: refuse it, auto-decline it, or
/// raise the dialog that asks the owner.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle(
    sender: PeerId,
    offer_id: u64,
    target_did: String,
    payload_json: Vec<u8>,
    commands: &mut Commands,
    peers: &mut Query<PeerParts>,
    peer_sessions: &PeerSessionMapRes,
    session: &Option<Res<AtprotoSession>>,
    session_log: &mut SessionLog,
    out: &mut SendMessage<OverlandsMessage>,
    metrics: &mut crate::diagnostics::MetricsRegistry,
    // Counted for the dialog's closing note (#843). The `ResMut` itself, so
    // its tick moves only on a busy decline.
    busy_declines: &mut ResMut<BusyAutoDeclines>,
    now: f64,
    // Whether a dialog is already up (or staged) this frame. `Commands` do
    // not apply until end-of-system, so re-reading the resource would report
    // the stale pre-frame state and let a peer pack many offers into one
    // frame, bypassing the busy-gate.
    dialog_open: &mut bool,
) {
    // Broadcast-with-address: only the peer whose DID matches
    // `target_did` should act on the offer. Everyone else
    // silently drops it because `bevy_symbios_multiuser` has no
    // directed-send primitive.
    let Some(sess) = session.as_deref() else {
        return;
    };
    if sess.did != target_did {
        return;
    }

    // Authenticate the sender's DID against the relay-signed
    // PeerSessionMap - same defence the Identity handler uses.
    // A `None` lookup means the peer connected before its
    // session bound; defer by dropping the message (the sender
    // can retry).
    let Some(sender_did) = peer_sessions.session_id(&sender) else {
        debug!(
            "Deferring ItemOffer from {}: peer session not yet known",
            sender
        );
        return;
    };

    // Silent auto-decline for muted senders. The sender still
    // gets a response so their UI clears the pending state, but
    // no dialog is shown and no diagnostics entry is written -
    // muted senders should be invisible by design.
    let peer_lookup = peers.iter().find(|(_, peer, _, _)| peer.peer_id == sender);
    let sender_muted = peer_lookup
        .as_ref()
        .map(|(_, peer, _, _)| peer.muted)
        .unwrap_or(false);
    // The ONE ladder (#1218 f299): a handle if the profile
    // fetch has landed, the DID's head otherwise - and never the
    // DID dressed up as a name.
    let sender_label = PeerLabel::new(
        peer_lookup
            .as_ref()
            .and_then(|(_, peer, _, _)| peer.handle.as_deref()),
        Some(sender_did.as_str()),
    );
    let sender_name = sender_label.addressed();

    if sender_muted {
        out.to(
            sender,
            // Reported as a plain decline on purpose (#1220
            // f127): telling somebody they have been muted is a
            // privacy leak, and a muted sender should be unable
            // to tell a mute from a refusal.
            OverlandsMessage::item_offer_response(
                offer_id,
                sender_did.clone(),
                false,
                DeclineReason::Declined,
            ),
            ChannelKind::Reliable,
        );
        return;
    }

    // Busy-gate: a dialog is already up (or was staged earlier
    // this same frame), so an attacker can't queue-flood the
    // recipient with nested prompts. Decline and log so the
    // user knows someone tried. Before the payload is read
    // (#1184, #1844): every peer in the room receives every
    // gift, and decoding a whole avatar the recipient is about
    // to turn away is work nobody asked for - the envelope
    // carries everything this gate needs.
    if *dialog_open {
        out.to(
            sender,
            // The finding's headline case (#1220 f127): a
            // mechanical throttle reaching the sender as "they
            // declined" misattributes it to a person AND teaches
            // them not to retry, in the one case where retrying
            // in ten seconds works.
            OverlandsMessage::item_offer_response(
                offer_id,
                sender_did.clone(),
                false,
                DeclineReason::Busy,
            ),
            ChannelKind::Reliable,
        );
        session_log.info(now, EventPayload::ItemOfferAutoDeclinedBusy { offer_id });
        crate::diagnostics::samplers::offer_auto_declined_busy(metrics);
        // Counted for the dialog's closing note (#843) - the
        // decline itself stays invisible until then, preserving
        // the single-dialog anti-spam invariant.
        busy_declines.0 = busy_declines.0.saturating_add(1);
        return;
    }

    // Decode the payload. A malformed one is a protocol error:
    // auto-decline and log.
    let Some(payload) = OverlandsMessage::decode_item_offer(&payload_json) else {
        out.to(
            sender,
            // Not the recipient's choice: this client could not
            // read the gift, which most often means the two
            // builds disagree about the wire.
            OverlandsMessage::item_offer_response(
                offer_id,
                sender_did.clone(),
                false,
                DeclineReason::Unavailable,
            ),
            ChannelKind::Reliable,
        );
        session_log.warn(
            now,
            EventPayload::ItemOfferDecodeFailed {
                reason: format!("from {sender_name}: failed to decode"),
            },
        );
        return;
    };

    // Clamp the wire-supplied item name *before* any
    // diagnostics or dialog state references it. The
    // protocol field is an unbounded `String`, so a hostile
    // sender can ship a 10 MiB blob; clamping up front
    // guarantees the rest of this handler only sees a
    // bounded value.
    let item_name = {
        let mut n: String = payload
            .item_name
            .chars()
            .filter(|c| !c.is_control())
            .take(64)
            .collect();
        if n.is_empty() {
            n.push_str("(unnamed)");
        }
        n
    };

    // A refusal of what the payload holds: not the recipient's
    // choice, so `Unavailable`, one log line, and no dialog.
    let mut refuse = |reason: String| {
        out.to(
            sender,
            OverlandsMessage::item_offer_response(
                offer_id,
                sender_did.clone(),
                false,
                DeclineReason::Unavailable,
            ),
            ChannelKind::Reliable,
        );
        session_log.warn(
            now,
            EventPayload::ItemOfferRejected {
                offer_id,
                reason: format!("from {sender_name}: {reason}"),
            },
        );
    };

    let item = match payload.into_gift() {
        Ok(item) => item,
        Err(why) => {
            refuse(String::from(why));
            return;
        }
    };
    let item = match item {
        GiftItem::Generator {
            mut generator,
            wear,
        } => {
            crate::pds::sanitize_generator(&mut generator);
            // Wear metadata (#1108) through the same clamp the inventory
            // and attachment records share. A `wear` the payload could
            // not read is already `None` by the time it gets here - the
            // payload's own lenient decoder degrades a bad one to decor
            // rather than refusing the gift (#1184).
            let wear = wear.map(|mut meta| {
                meta.sanitize();
                meta
            });
            // Non-placeable kinds (terrain / water / Unknown) never
            // make sense as a gift - the sender UI already filters
            // these, but reject here too so a hand-crafted payload
            // can't stuff an unplaceable item into the recipient's
            // stash via the accept path.
            if !crate::pds::inventory::is_drop_placeable(&generator) {
                refuse(String::from("item kind not giftable"));
                return;
            }
            GiftItem::Generator { generator, wear }
        }
        GiftItem::Avatar(mut avatar) => {
            // The stash's own sanitiser for an avatar item (#1838), and
            // the stash's own rule for what this build can hold: an
            // avatar it cannot read could only be deleted.
            avatar.sanitize();
            if !avatar.is_readable() {
                refuse(String::from("avatar this build cannot read"));
                return;
            }
            GiftItem::Avatar(avatar)
        }
        GiftItem::World(mut head) => {
            // Only the head is read here; its contents are fetched from
            // the sender's data server after Accept, and the load's own
            // checks and room sanitiser stand there.
            head.summary.sanitize();
            if let Err(why) = crate::pds::inventory::world::check_head(&head) {
                refuse(format!("world head refused - {why}"));
                return;
            }
            GiftItem::World(head)
        }
    };

    // The name's size, never the name (#1432): it is the sender's words,
    // and this line is written before anything - the agent daemon's
    // `sort_offers`, a person's dialog - has chosen whether to read them.
    session_log.info(
        now,
        EventPayload::ItemOfferReceived {
            offer_id,
            sender_did: sender_did.clone(),
            name_bytes: u32::try_from(item_name.len()).unwrap_or(u32::MAX),
        },
    );
    commands.insert_resource(IncomingOfferDialog {
        offer_id,
        sender_peer_id: sender,
        sender_did,
        sender_label,
        item_name,
        item,
        arrived_at_secs: now,
        // The TTL and the countdown run on wall clock (#1216);
        // the virtual stamp above stays for the session log.
        arrived_at_epoch: crate::state::now_epoch_secs(),
    });
    // Slam the gate shut for the rest of this frame so any
    // further offers in the same drain auto-decline instead of
    // racing the deferred `insert_resource` above.
    *dialog_open = true;
}

/// Land the answer to a gift THIS client offered: toast the outcome and
/// retire the pending record.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_response(
    sender: PeerId,
    offer_id: u64,
    target_did: String,
    payload_json: Vec<u8>,
    peer_sessions: &PeerSessionMapRes,
    session: &Option<Res<AtprotoSession>>,
    session_log: &mut SessionLog,
    pending_offers: &mut PendingOutgoingOffers,
    bufs: &mut InboundBuffers,
    now: f64,
) {
    // Gate on the local DID first: a response broadcast is
    // carrying our own sender-side offer_id only when
    // `target_did` equals our DID. Other peers drop it.
    let Some(sess) = session.as_deref() else {
        return;
    };
    if sess.did != target_did {
        return;
    }

    // Authenticate the responder's DID against the relay map
    // so a third-party peer can't impersonate the real
    // recipient and spoof an "accepted" reply to steal
    // visibility into what we gifted.
    let Some(responder_did) = peer_sessions.session_id(&sender) else {
        return;
    };

    // Authenticate the responder's DID against the pending
    // offer's target BEFORE consuming the pending entry. A
    // prior implementation removed unconditionally, letting any
    // peer in the room race a spoofed "accepted" reply onto the
    // wire and silently delete the genuine target's pending
    // offer - permanently breaking gifting for the sender.
    match pending_offers.by_id.get(&offer_id) {
        Some(pending) if pending.target_did != responder_did => return,
        None => return,
        _ => {}
    }

    // Consume the pending entry now that the responder is
    // authenticated - its handle + item name feed the sender's
    // outcome toast (#843).
    let Some(pending) = pending_offers.by_id.remove(&offer_id) else {
        return;
    };

    // A payload this build cannot read is NOT an acceptance:
    // `ItemOfferResponsePayload::accepted` defaults to false
    // and an outright decode failure declines too (#1184), so
    // the worst a skewed peer can do is make a gift that
    // arrived look declined - never the reverse.
    let decoded = OverlandsMessage::decode_item_offer_response(&payload_json);
    let accepted = decoded.as_ref().is_some_and(|payload| payload.accepted);
    let reason = decoded.map(|payload| payload.reason).unwrap_or_default();

    session_log.info(
        now,
        EventPayload::ItemOfferResponseReceived { offer_id, accepted },
    );
    bufs.answered.write(crate::state::OfferAnswered {
        offer_id,
        target_did: pending.target_did.clone(),
        outcome: if accepted {
            crate::state::OfferOutcome::Accepted
        } else {
            crate::state::OfferOutcome::Declined(reason)
        },
    });
    // The sender finally learns the outcome somewhere visible
    // (#843), and now learns WHICH outcome (#1220 f127): the
    // boolean used to render one sentence for a refusal, a
    // throttle, a mute and a timeout alike.
    if accepted {
        bufs.toasts.success(
            format!(
                "{} accepted \"{}\".",
                pending.target_label, pending.item_name
            ),
            now,
        );
    } else {
        bufs.toasts.info(
            crate::protocol::offer_refusal_line(reason, &pending.target_label, &pending.item_name),
            now,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::inventory::avatar_item_tests::rigged_record;
    use crate::pds::inventory::{AvatarItem, WorldHead, WorldItem};
    use crate::protocol::WHOLE_GIFT_MARKER;
    use crate::state::RemotePeer;
    use bevy::ecs::system::{RunSystemOnce, SystemState};

    const ME: &str = "did:plc:recipientofgifts222222";
    const GIVER: &str = "did:plc:giverofgifts22222222222";
    const ELSEWHERE: &str = "did:plc:someoneelseentirely222";

    fn id(n: u8) -> PeerId {
        serde_json::from_str(&format!("\"00000000-0000-0000-0000-0000000000{n:02}\""))
            .expect("a well-formed uuid")
    }

    /// The recipient signed in as [`ME`], with the giver - peer 1, whom the
    /// relay knows as [`GIVER`] - in the room, muted or not.
    fn app_with(giver_muted: bool) -> App {
        let mut app = App::new();
        let sessions = PeerSessionMapRes::default();
        sessions
            .0
            .write()
            .expect("unpoisoned")
            .insert(id(1), GIVER.to_owned());
        app.insert_resource(sessions)
            .insert_resource(
                crate::oauth::stand_in::stand_in_session(ME, "me.test").expect("a session"),
            )
            .init_resource::<SessionLog>()
            .init_resource::<crate::diagnostics::MetricsRegistry>()
            .init_resource::<BusyAutoDeclines>()
            .add_message::<SendTo<OverlandsMessage>>()
            .add_message::<Broadcast<OverlandsMessage>>();
        app.world_mut().spawn((
            RemotePeer {
                peer_id: id(1),
                did: Some(GIVER.to_owned()),
                handle: Some(String::from("giver.test")),
                muted: giver_muted,
                avatar: None,
                build: None,
                connected_at: 0.0,
            },
            Transform::default(),
            TransformBuffer::default(),
        ));
        app
    }

    /// Hand `payload` to the offer handler as peer 1's offer 9, with a
    /// dialog already up when `busy`.
    fn offer(app: &mut App, payload: Vec<u8>, busy: bool) {
        app.world_mut()
            .run_system_once(
                move |mut commands: Commands,
                      mut peers: Query<PeerParts>,
                      sessions: Res<PeerSessionMapRes>,
                      session: Option<Res<AtprotoSession>>,
                      mut log: ResMut<SessionLog>,
                      mut out: SendMessage<OverlandsMessage>,
                      mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
                      mut busy_declines: ResMut<BusyAutoDeclines>| {
                    let mut open = busy;
                    handle(
                        id(1),
                        9,
                        ME.to_owned(),
                        payload.clone(),
                        &mut commands,
                        &mut peers,
                        &sessions,
                        &session,
                        &mut log,
                        &mut out,
                        &mut metrics,
                        &mut busy_declines,
                        1.0,
                        &mut open,
                    );
                },
            )
            .expect("the offer is handled");
    }

    /// The answers sent back: (offer id, to whom, accepted, reason).
    fn answers(app: &App) -> Vec<(u64, String, bool, DeclineReason)> {
        let sent = app.world().resource::<Messages<SendTo<OverlandsMessage>>>();
        sent.get_cursor()
            .read(sent)
            .filter_map(|message| match &message.payload {
                OverlandsMessage::ItemOfferResponse {
                    offer_id,
                    target_did,
                    payload_json,
                } => {
                    let payload = OverlandsMessage::decode_item_offer_response(payload_json)?;
                    Some((
                        *offer_id,
                        target_did.clone(),
                        payload.accepted,
                        payload.reason,
                    ))
                }
                _ => None,
            })
            .collect()
    }

    fn rejected(app: &App) -> usize {
        app.world()
            .resource::<SessionLog>()
            .iter()
            .filter(|event| matches!(event.payload, EventPayload::ItemOfferRejected { .. }))
            .count()
    }

    fn avatar() -> AvatarItem {
        AvatarItem::from_record(&rigged_record()).expect("copies")
    }

    fn head() -> WorldHead {
        WorldItem::snapshot(&crate::pds::inventory::world::tests::seeded_world(71))
            .expect("copies")
            .head()
            .clone()
    }

    /// An offer's payload bytes as the sending build writes them.
    fn wire(gift: &GiftItem) -> Vec<u8> {
        let OverlandsMessage::ItemOffer { payload_json, .. } =
            OverlandsMessage::item_offer(9, ME.to_owned(), String::from("gift"), gift)
        else {
            panic!("not an offer");
        };
        payload_json
    }

    /// A whole gift's payload, hand-written so it can say what no build
    /// sends.
    fn whole(avatar: Option<serde_json::Value>, world: Option<serde_json::Value>) -> Vec<u8> {
        let mut payload = serde_json::json!({
            "item_name": "gift",
            "generator": {"$type": WHOLE_GIFT_MARKER},
        });
        if let Some(avatar) = avatar {
            payload["avatar"] = avatar;
        }
        if let Some(world) = world {
            payload["world"] = world;
        }
        serde_json::to_vec(&payload).expect("encodes")
    }

    /// #1844. A valid whole avatar and world each raise the dialog,
    /// carrying what they give.
    #[test]
    fn a_whole_avatar_and_a_world_head_raise_the_dialog() {
        for gift in [
            GiftItem::Avatar(Box::new(avatar())),
            GiftItem::World(head()),
        ] {
            let mut app = app_with(false);
            offer(&mut app, wire(&gift), false);
            let dialog = app
                .world()
                .get_resource::<IncomingOfferDialog>()
                .expect("a dialog");
            assert_eq!(dialog.item.kind(), gift.kind());
            assert_eq!(dialog.sender_did, GIVER);
            assert!(answers(&app).is_empty(), "nothing answered yet");
        }
    }

    /// #1844. An avatar this build cannot read, and a world head a load
    /// would refuse, are turned away at the seam: `Unavailable`, one log
    /// line, and no dialog for a gift that could not land. So is a payload
    /// offering both at once.
    #[test]
    fn an_unreadable_avatar_or_a_bad_head_auto_declines_with_no_dialog() {
        let mut unreadable = serde_json::to_value(avatar()).expect("encodes");
        unreadable["body"]["$type"] =
            serde_json::json!("network.symbios.overlands.inventory.item#future");
        let mut damaged = head();
        damaged.parts.clear();
        for payload in [
            whole(Some(unreadable), None),
            whole(None, Some(serde_json::to_value(&damaged).expect("encodes"))),
            whole(
                Some(serde_json::to_value(avatar()).expect("encodes")),
                Some(serde_json::to_value(head()).expect("encodes")),
            ),
        ] {
            let mut app = app_with(false);
            offer(&mut app, payload, false);
            assert!(!app.world().contains_resource::<IncomingOfferDialog>());
            assert_eq!(
                answers(&app),
                [(9, GIVER.to_owned(), false, DeclineReason::Unavailable)]
            );
            assert_eq!(rejected(&app), 1);
        }
    }

    /// #1844. The muted and busy gates stand before anything a whole gift
    /// holds is read, exactly as for a thing: a muted giver hears a plain
    /// decline with nothing logged, a busy recipient answers busy and counts
    /// it for the dialog's closing note.
    #[test]
    fn the_muted_and_busy_gates_are_unchanged_for_whole_gifts() {
        for gift in [
            GiftItem::Avatar(Box::new(avatar())),
            GiftItem::World(head()),
        ] {
            let mut app = app_with(true);
            offer(&mut app, wire(&gift), false);
            assert!(!app.world().contains_resource::<IncomingOfferDialog>());
            assert_eq!(
                answers(&app),
                [(9, GIVER.to_owned(), false, DeclineReason::Declined)]
            );
            assert_eq!(app.world().resource::<SessionLog>().iter().count(), 0);

            let mut app = app_with(false);
            offer(&mut app, wire(&gift), true);
            assert!(!app.world().contains_resource::<IncomingOfferDialog>());
            assert_eq!(
                answers(&app),
                [(9, GIVER.to_owned(), false, DeclineReason::Busy)]
            );
            assert_eq!(app.world().resource::<BusyAutoDeclines>().0, 1);
        }
    }

    /// **A world is fetched from the giver the relay authenticated**
    /// (#1844). The payload names another DID - at the top and inside the
    /// head - and it changes nothing: Accept aims the fetch at the sender the
    /// offer arrived under, and answers nothing until the parts land.
    #[test]
    fn a_world_fetch_is_aimed_at_the_authenticated_sender() {
        use crate::ui::inventory::gift_world::tests::recording_fetch;
        use crate::ui::people::{OfferAnswer, OfferAnswering};
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let mut world = serde_json::to_value(head()).expect("encodes");
        world["did"] = serde_json::json!(ELSEWHERE);
        world["owner"] = serde_json::json!(ELSEWHERE);
        let mut payload: serde_json::Value =
            serde_json::from_slice(&whole(None, Some(world))).expect("JSON");
        payload["did"] = serde_json::json!(ELSEWHERE);
        payload["from_did"] = serde_json::json!(ELSEWHERE);

        let mut app = app_with(false);
        offer(
            &mut app,
            serde_json::to_vec(&payload).expect("encodes"),
            false,
        );
        let dialog = app
            .world()
            .get_resource::<IncomingOfferDialog>()
            .cloned()
            .expect("a dialog");

        let (fetch, asked) = recording_fetch(Err(String::from("not needed")));
        app.insert_resource(fetch)
            .insert_resource(Time::<()>::default())
            .insert_resource(crate::state::LiveInventoryRecord(Default::default()))
            .insert_resource(crate::state::StoredInventoryRecord(Default::default()))
            .init_resource::<crate::state::PublishFeedback<crate::pds::InventoryRecord>>()
            .init_resource::<crate::notify::Toasts>()
            .init_resource::<crate::state::MutedDids>();
        let mut state = SystemState::<OfferAnswering>::new(app.world_mut());
        let landed = state
            .get_mut(app.world_mut())
            .expect("answerable")
            .answer(&dialog, OfferAnswer::Accept { publish: true });
        state.apply(app.world_mut());

        assert_eq!(landed, None, "it lands when its parts do");
        assert_eq!(*asked.lock().expect("unpoisoned"), vec![GIVER.to_owned()]);
        let sent = app
            .world()
            .resource::<Messages<Broadcast<OverlandsMessage>>>();
        assert_eq!(sent.get_cursor().read(sent).count(), 0, "no answer yet");
        assert_eq!(
            app.world_mut()
                .query::<&crate::ui::inventory::GiftWorldTask>()
                .single(app.world())
                .expect("a fetch in flight")
                .giver_did,
            GIVER
        );
        assert!(!app.world().contains_resource::<IncomingOfferDialog>());
    }

    /// #1845 (G1 review). A world accepted with nobody signed in starts no
    /// fetch - there is no stash to land it in - so it is answered at once,
    /// `Unavailable`, rather than left for the giver to wait out the three
    /// minutes their offer lapses after. Unreachable in practice (the offer
    /// handler needs a session, and logout scrubs the dialog); the answer
    /// is the honest one if it ever is.
    #[test]
    fn a_world_accepted_with_nobody_signed_in_is_answered_at_once() {
        use crate::ui::inventory::gift_world::tests::recording_fetch;
        use crate::ui::people::{OfferAnswer, OfferAnswering};
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let mut app = app_with(false);
        offer(&mut app, wire(&GiftItem::World(head())), false);
        let dialog = app
            .world()
            .get_resource::<IncomingOfferDialog>()
            .cloned()
            .expect("a dialog");
        app.world_mut()
            .remove_resource::<bevy_symbios_multiuser::auth::AtprotoSession>();

        let (fetch, asked) = recording_fetch(Err(String::from("not needed")));
        app.insert_resource(fetch)
            .insert_resource(Time::<()>::default())
            .insert_resource(crate::state::LiveInventoryRecord(Default::default()))
            .insert_resource(crate::state::StoredInventoryRecord(Default::default()))
            .init_resource::<crate::state::PublishFeedback<crate::pds::InventoryRecord>>()
            .init_resource::<crate::notify::Toasts>()
            .init_resource::<crate::state::MutedDids>();
        let mut state = SystemState::<OfferAnswering>::new(app.world_mut());
        let landed = state
            .get_mut(app.world_mut())
            .expect("answerable")
            .answer(&dialog, OfferAnswer::Accept { publish: true });
        state.apply(app.world_mut());

        assert_eq!(landed, None);
        assert!(
            asked.lock().expect("unpoisoned").is_empty(),
            "nothing fetched"
        );
        assert_eq!(
            app.world_mut()
                .query::<&crate::ui::inventory::GiftWorldTask>()
                .iter(app.world())
                .count(),
            0
        );
        let sent = app
            .world()
            .resource::<Messages<Broadcast<OverlandsMessage>>>();
        let answered: Vec<(u64, String, bool, DeclineReason)> = sent
            .get_cursor()
            .read(sent)
            .filter_map(|broadcast| match &broadcast.payload {
                OverlandsMessage::ItemOfferResponse {
                    offer_id,
                    target_did,
                    payload_json,
                } => {
                    let payload = OverlandsMessage::decode_item_offer_response(payload_json)?;
                    Some((
                        *offer_id,
                        target_did.clone(),
                        payload.accepted,
                        payload.reason,
                    ))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            answered,
            [(9, GIVER.to_owned(), false, DeclineReason::Unavailable)],
            "answered now, as a gift that cannot land"
        );
        assert!(!app.world().contains_resource::<IncomingOfferDialog>());
    }
}
