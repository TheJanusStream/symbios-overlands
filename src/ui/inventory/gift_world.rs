//! A world given to this client (#1844): accepted, its contents are fetched
//! from the giver's saved copy, and it lands in the Inventory when they
//! arrive - or the giver hears why not.
//!
//! An offer carries a world's head alone. The offer dialog's Accept starts a
//! [`GiftWorldTask`]: the parts the head names are fetched from the data
//! server of the GIVER - the relay-authenticated sender DID the offer
//! arrived under, never any DID the payload names - through the load's own
//! [`crate::pds::inventory::world::fetch_parts`], joined, checked and passed
//! through the room sanitiser ([`crate::pds::inventory::world::join_parts`]),
//! and re-cut into this client's OWN snapshot
//! ([`crate::pds::inventory::WorldItem::snapshot`]): what the stash keeps is
//! the world as this build read it, never the giver's text. Nothing in the
//! recipient's own world changes.
//!
//! [`poll_gift_world_tasks`] lands it by the one rule every gift lands by
//! ([`super::accept_gift`]: the key free in live and stored, live gets the
//! item, the save is stored plus the gift) and answers the giver: accepted,
//! `Unavailable` when the stash filled while it was fetched, `NotFetched`
//! when it could not be fetched or read. A session whose account changed
//! meanwhile drops it unlanded, and logout scrubs the task
//! (`ui::logout::SessionTasks`).
//!
//! The fetch is a [`GiftWorldFetch`] resource, so a test can stand one in for
//! the network and see whom it was aimed at.
//!
//! How each landing ended is also left in [`GiftWorldsEnded`], a mailbox
//! only the agent client inserts (#1847): its player has no toast to read.

use std::sync::Arc;

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::diagnostics::SessionLog;
use crate::diagnostics::event::EventPayload;
use crate::pds::InventoryRecord;
use crate::pds::inventory::{WorldHead, WorldItem};
use crate::protocol::{DeclineReason, OverlandsMessage};
use crate::state::{LiveInventoryRecord, PublishFeedback, StoredInventoryRecord};

/// How a given world's parts are fetched: from `did`'s repo, the parts
/// `head` names, in its order. The network's
/// [`crate::pds::inventory::world::fetch_parts`] unless a test stands one in.
type FetchParts = dyn Fn(String, WorldHead) -> bevy::tasks::BoxedFuture<'static, Result<Vec<String>, String>>
    + Send
    + Sync;

/// The fetch a given world's parts go through (#1844). Absent, the network's
/// own; a test inserts one to see the DID it is aimed at.
#[derive(Resource, Clone)]
pub struct GiftWorldFetch(pub Arc<FetchParts>);

impl Default for GiftWorldFetch {
    fn default() -> Self {
        Self(Arc::new(|did, head| {
            Box::pin(async move {
                let client = crate::config::http::default_client();
                crate::pds::inventory::world::fetch_parts(&client, &did, &head).await
            })
        }))
    }
}

/// How each given world's landing ended (#1847), for a listener with no
/// screen to read the toast on: the agent client inserts this to tell its
/// player how a world it accepted went, as [`super::WorldLoadsEnded`] tells
/// it how a load went. [`poll_gift_world_tasks`] pushes one entry per
/// landing that answered the giver (landed, or not with the toast's reason)
/// only while it is there; a person's client never inserts it, and nothing
/// changes for them. A landing dropped because the session's account changed
/// answers nobody and leaves nothing here.
#[derive(Resource, Default, Debug)]
pub struct GiftWorldsEnded(pub Vec<GiftWorldEnded>);

/// One given world's landing, as [`GiftWorldsEnded`] keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GiftWorldEnded {
    /// The offer it answered.
    pub offer_id: u64,
    /// The giver, as the relay authenticated them.
    pub giver_did: String,
    /// The name it was offered under.
    pub name: String,
    /// Where it landed - its key, and whether a save was started - or why
    /// it did not, in the toast's words.
    pub outcome: Result<crate::ui::people::AcceptedGift, String>,
}

/// A given world being fetched (#1844): this client's own snapshot of it,
/// or the sentence saying why not - with everything its landing needs.
#[derive(Component)]
pub struct GiftWorldTask {
    pub task: bevy::tasks::Task<Result<WorldItem, String>>,
    /// The offer being answered.
    pub offer_id: u64,
    /// The giver, as the relay authenticated them: whose data server the
    /// parts come from, and who the answer is addressed to.
    pub giver_did: String,
    /// The giver's name for the toasts, off the one ladder.
    pub giver_label: String,
    /// The account that accepted: a session signed in as another by the
    /// time the parts land drops them.
    pub recipient_did: String,
    /// The name it was offered under; it lands under a free one.
    pub name: String,
    /// Whether the stash is saved at once when it lands, as Accept saves a
    /// thing (the agent client saves only when it may).
    pub publish: bool,
    pub spawned_at: f64,
    /// How long the fetch may run before the poll declares it dead: a
    /// request's bound per window of parts beyond the publish deadline,
    /// sized as a load's is (`LoadWorldTask`).
    pub deadline_secs: f64,
    /// Wall-clock seconds the offer arrived (#1216). However long the load's
    /// bound allows, the fetch gives up in time to answer before the giver's
    /// own offer lapses ([`answer_due`]).
    pub offer_arrived_epoch: i64,
}

/// Whether a given world's answer is due now, fetched or not (#1844): past
/// this the giver's own offer may lapse before the answer reaches them -
/// [`crate::config::network::GIFT_WORLD_ANSWER_MARGIN_SECS`] short of
/// [`crate::config::network::PENDING_OFFER_TIMEOUT_SECS`], on the wall clock
/// both machines count it on.
fn answer_due(offer_arrived_epoch: i64) -> bool {
    crate::state::real_secs_since(offer_arrived_epoch)
        >= crate::config::network::PENDING_OFFER_TIMEOUT_SECS
            - crate::config::network::GIFT_WORLD_ANSWER_MARGIN_SECS
}

/// What a given world's fetch needs to know about the offer.
pub(crate) struct GiftWorldOffer<'a> {
    pub offer_id: u64,
    pub giver_did: &'a str,
    pub giver_label: String,
    pub recipient_did: &'a str,
    pub name: &'a str,
    pub head: &'a WorldHead,
    pub publish: bool,
    /// Wall-clock seconds the offer arrived (`IncomingOfferDialog`'s).
    pub arrived_at_epoch: i64,
}

/// Start fetching the world `offer` gives, through `fetch`.
pub(crate) fn spawn_gift_world_task(
    commands: &mut Commands,
    fetch: &GiftWorldFetch,
    offer: GiftWorldOffer<'_>,
    now: f64,
) {
    // Sizes only: the session log never carries a world's contents.
    info!(
        "Fetching a given world: {} parts, {} bytes",
        offer.head.parts.len(),
        offer.head.bytes
    );
    let windows = offer
        .head
        .parts
        .len()
        .div_ceil(crate::pds::inventory::world::PART_FETCH_CONCURRENCY)
        .max(1);
    let deadline_secs = crate::config::http::PUBLISH_TASK_DEADLINE.as_secs_f64()
        + crate::config::http::REQUEST_TIMEOUT.as_secs_f64() * windows as f64;
    let head = offer.head.clone();
    let parts = (fetch.0)(offer.giver_did.to_owned(), head.clone());
    let task = bevy::tasks::IoTaskPool::get().spawn(async move {
        let parts = parts.await?;
        let world = crate::pds::inventory::world::join_parts(&head, &parts)?;
        WorldItem::snapshot(&world)
    });
    commands.spawn(GiftWorldTask {
        task,
        offer_id: offer.offer_id,
        giver_did: offer.giver_did.to_owned(),
        giver_label: offer.giver_label,
        recipient_did: offer.recipient_did.to_owned(),
        name: offer.name.to_owned(),
        publish: offer.publish,
        spawned_at: now,
        deadline_secs,
        offer_arrived_epoch: offer.arrived_at_epoch,
    });
}

/// Poll the given worlds in flight (#1844), and land and answer each one.
#[allow(clippy::too_many_arguments)]
pub fn poll_gift_world_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut GiftWorldTask)>,
    session: Option<Res<AtprotoSession>>,
    refresh_ctx: Option<Res<crate::oauth::OauthRefreshCtx>>,
    mut live: Option<ResMut<LiveInventoryRecord>>,
    stored: Option<Res<StoredInventoryRecord>>,
    recovery: Option<Res<crate::state::InventoryRecordRecovery>>,
    mut feedback: ResMut<PublishFeedback<InventoryRecord>>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
    mut session_log: ResMut<SessionLog>,
    mut toasts: ResMut<crate::notify::Toasts>,
    mut ended: Option<ResMut<GiftWorldsEnded>>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    for (entity, mut task) in tasks.iter_mut() {
        let result =
            match futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task)) {
                Some(result) => result,
                None if now - task.spawned_at > task.deadline_secs
                    || answer_due(task.offer_arrived_epoch) =>
                {
                    Err(crate::config::http::timed_out_after(
                        "Fetching the world",
                        std::time::Duration::from_secs_f64((now - task.spawned_at).max(0.0)),
                    ))
                }
                None => continue,
            };
        commands.entity(entity).despawn();
        // Another account by now: the gift was accepted into a stash this
        // session no longer holds, and an answer from this identity would
        // not be the recipient's. Dropped unlanded; the giver's own offer
        // lapses on its clock.
        let current = session.as_deref().map(|s| s.did.as_str());
        if current.is_none()
            || crate::ui::room::stale_result("Fetching a given world", &task.recipient_did, current)
        {
            continue;
        }
        let answer = |writer: &mut MessageWriter<Broadcast<OverlandsMessage>>,
                      accepted: bool,
                      reason: DeclineReason| {
            writer.write(Broadcast {
                payload: OverlandsMessage::item_offer_response(
                    task.offer_id,
                    task.giver_did.clone(),
                    accepted,
                    reason,
                ),
                channel: ChannelKind::Reliable,
            });
        };
        let mut tell = |outcome: Result<crate::ui::people::AcceptedGift, String>| {
            if let Some(ended) = ended.as_mut() {
                ended.0.push(GiftWorldEnded {
                    offer_id: task.offer_id,
                    giver_did: task.giver_did.clone(),
                    name: task.name.clone(),
                    outcome,
                });
            }
        };
        let world = match result {
            Ok(world) => world,
            Err(reason) => {
                tell(Err(format!(
                    "it could not be fetched from {}'s saved copy - {reason}",
                    task.giver_label
                )));
                answer(&mut writer, false, DeclineReason::NotFetched);
                session_log.warn(
                    now,
                    EventPayload::ItemOfferRejected {
                        offer_id: task.offer_id,
                        reason: format!("a given world was not fetched: {reason}"),
                    },
                );
                toasts.warn(
                    format!(
                        "Couldn't fetch \"{}\" from {}'s saved copy - {reason}",
                        task.name, task.giver_label
                    ),
                    now,
                );
                continue;
            }
        };
        // Asked through `Deref`: the stash is written only once the gift
        // is sure to land.
        let has_room = live.as_deref().is_some_and(|live| !live.0.is_full());
        let Some(live) = live.as_mut().filter(|_| has_room) else {
            tell(Err(String::from(
                "the inventory filled up while it was fetched",
            )));
            answer(&mut writer, false, DeclineReason::Unavailable);
            session_log.warn(
                now,
                EventPayload::ItemOfferRejected {
                    offer_id: task.offer_id,
                    reason: String::from("a given world arrived to a full inventory"),
                },
            );
            toasts.warn(
                format!(
                    "Couldn't take \"{}\" - your inventory filled up while it was fetched.",
                    task.name
                ),
                now,
            );
            continue;
        };
        let stored_stash = stored.as_deref().map(|s| s.0.clone()).unwrap_or_default();
        let (key, payload) = super::accept_gift(
            &mut live.0,
            &stored_stash,
            &task.name,
            super::GiftLanding::World(world),
        );
        answer(&mut writer, true, DeclineReason::Declined);
        session_log.info(
            now,
            EventPayload::ItemOfferUserResponded {
                offer_id: task.offer_id,
                accepted: true,
            },
        );
        let saving = super::after_gift_landed(
            super::Landed {
                key: &key,
                offered: &task.name,
                payload,
                stored: stored_stash,
                publish: task.publish,
            },
            &mut super::LandingSinks {
                commands: &mut commands,
                session: session.as_deref(),
                refresh: refresh_ctx.as_deref(),
                recovery: recovery.is_some(),
                feedback: feedback.reborrow(),
                toasts: &mut toasts,
            },
            now,
        );
        tell(Ok(crate::ui::people::AcceptedGift { key, saving }));
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::pds::inventory::world::tests::seeded_world;
    use crate::ui::inventory::PublishInventoryTask;
    use bevy_symbios_multiuser::prelude::SendTo;
    use std::sync::Mutex;

    pub(crate) const GIVER: &str = "did:plc:giverofworlds2222222222";
    pub(crate) const ME: &str = "did:plc:takerofworlds2222222222";

    /// A fetch that answers from `parts` (or fails with `fail`) and writes
    /// down the DID every request was aimed at.
    pub(crate) fn recording_fetch(
        parts: Result<Vec<String>, String>,
    ) -> (GiftWorldFetch, Arc<Mutex<Vec<String>>>) {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&asked);
        let fetch = GiftWorldFetch(Arc::new(move |did, _head| {
            seen.lock().expect("unpoisoned").push(did);
            let parts = parts.clone();
            Box::pin(async move { parts })
        }));
        (fetch, asked)
    }

    /// The recipient signed in as `me`, with the stash `live` over `stored`
    /// and everything a landing writes to.
    pub(crate) fn app_for(me: &str, live: InventoryRecord, stored: InventoryRecord) -> App {
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(
                crate::oauth::stand_in::stand_in_session(me, "me.test").expect("a session"),
            )
            .insert_resource(crate::oauth::stand_in::stand_in_refresh_ctx().expect("a context"))
            .insert_resource(LiveInventoryRecord(live))
            .insert_resource(StoredInventoryRecord(stored))
            .init_resource::<PublishFeedback<InventoryRecord>>()
            .init_resource::<SessionLog>()
            .init_resource::<crate::notify::Toasts>()
            .add_message::<Broadcast<OverlandsMessage>>()
            .add_message::<SendTo<OverlandsMessage>>()
            .add_systems(Update, poll_gift_world_tasks);
        app
    }

    /// Run the app until no given world is in flight.
    ///
    /// Bounded generously against the clock: the fetch's join and re-cut run
    /// on the process-wide `IoTaskPool`, which under a one-process `cargo
    /// test` run (CI's) is shared with every other test and may be slow to
    /// get round to it - the reason `network::peer_cache`'s tests give up
    /// only after 30 s.
    pub(crate) fn settle(app: &mut App) {
        let give_up_at = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while std::time::Instant::now() < give_up_at {
            app.update();
            let left = app
                .world_mut()
                .query::<&GiftWorldTask>()
                .iter(app.world())
                .count();
            if left == 0 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the given world never landed");
    }

    /// The answers broadcast so far: (offer id, to whom, accepted, reason).
    pub(crate) fn answers(app: &App) -> Vec<(u64, String, bool, DeclineReason)> {
        let sent = app
            .world()
            .resource::<Messages<Broadcast<OverlandsMessage>>>();
        sent.get_cursor()
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
            .collect()
    }

    /// The toasts shown so far.
    pub(crate) fn toast_lines(app: &App) -> Vec<String> {
        app.world()
            .resource::<crate::notify::Toasts>()
            .shown()
            .into_iter()
            .map(|(_, line)| line.to_owned())
            .collect()
    }

    /// Start fetching the world `head` names, offered as "harbour" just now.
    fn start(app: &mut App, fetch: &GiftWorldFetch, head: &WorldHead, recipient: &str) {
        start_arrived(app, fetch, head, recipient, crate::state::now_epoch_secs());
    }

    /// Start fetching the world `head` names, offered as "harbour" at the
    /// wall-clock second `arrived_at_epoch`.
    fn start_arrived(
        app: &mut App,
        fetch: &GiftWorldFetch,
        head: &WorldHead,
        recipient: &str,
        arrived_at_epoch: i64,
    ) {
        let fetch = fetch.clone();
        let head = head.clone();
        let recipient = recipient.to_owned();
        app.world_mut()
            .run_system_once(move |mut commands: Commands| {
                spawn_gift_world_task(
                    &mut commands,
                    &fetch,
                    GiftWorldOffer {
                        offer_id: 4,
                        giver_did: GIVER,
                        giver_label: String::from("@giver.test"),
                        recipient_did: &recipient,
                        name: "harbour",
                        head: &head,
                        publish: true,
                        arrived_at_epoch,
                    },
                    0.0,
                );
            })
            .expect("the fetch starts");
    }

    /// #1844 (the G1 critic's MEDIUM): a fetch still running when the
    /// giver's offer is about to lapse gives up and answers `NotFetched`
    /// while the giver still listens, however long the load's own bound
    /// would allow it - else the giver hears "didn't answer in time" about a
    /// world the recipient goes on to keep. One just short of that runs on.
    #[test]
    fn a_fetch_outlasting_the_givers_offer_answers_not_fetched_in_time() {
        let (item, _) = given_world(55);
        let never = GiftWorldFetch(Arc::new(|_did, _head| {
            Box::pin(std::future::pending::<Result<Vec<String>, String>>())
        }));
        let due = (crate::config::network::PENDING_OFFER_TIMEOUT_SECS
            - crate::config::network::GIFT_WORLD_ANSWER_MARGIN_SECS) as i64;

        let mut app = app_for(ME, InventoryRecord::default(), InventoryRecord::default());
        start_arrived(
            &mut app,
            &never,
            item.head(),
            ME,
            crate::state::now_epoch_secs() - due + 20,
        );
        app.update();
        app.update();
        assert!(answers(&app).is_empty(), "still inside the giver's time");
        assert_eq!(
            app.world_mut()
                .query::<&GiftWorldTask>()
                .iter(app.world())
                .count(),
            1
        );

        let mut app = app_for(ME, InventoryRecord::default(), InventoryRecord::default());
        start_arrived(
            &mut app,
            &never,
            item.head(),
            ME,
            crate::state::now_epoch_secs() - due,
        );
        app.update();
        assert_eq!(
            answers(&app),
            [(4, GIVER.to_owned(), false, DeclineReason::NotFetched)]
        );
        assert_eq!(
            app.world_mut()
                .query::<&GiftWorldTask>()
                .iter(app.world())
                .count(),
            0,
            "given up"
        );
        assert!(
            app.world()
                .resource::<LiveInventoryRecord>()
                .0
                .worlds
                .is_empty()
        );
    }

    use bevy::ecs::system::RunSystemOnce;

    /// The world's parts, as the giver's data server would answer them.
    pub(crate) fn given_world(seed: u64) -> (WorldItem, Vec<String>) {
        let item = WorldItem::snapshot(&seeded_world(seed)).expect("copies");
        let parts = item.slices().expect("held").to_vec();
        (item, parts)
    }

    /// #1844: the world lands as this client's own snapshot of the
    /// sanitised record, under a name free in live and stored, the save is
    /// the stored stash plus the gift and nothing else, and the giver hears
    /// accepted.
    #[test]
    fn a_fetched_world_lands_as_its_own_snapshot_and_answers_accepted() {
        let (item, parts) = given_world(51);
        let mut stored = InventoryRecord::default();
        stored.put_item(
            String::from("harbour"),
            crate::pds::Generator::default_cuboid(),
            None,
        );
        let mut live = stored.clone();
        live.put_item(
            String::from("unsaved"),
            crate::pds::Generator::default_cuboid(),
            None,
        );
        let mut app = app_for(ME, live, stored);
        let (fetch, asked) = recording_fetch(Ok(parts.clone()));

        start(&mut app, &fetch, item.head(), ME);
        settle(&mut app);

        assert_eq!(*asked.lock().expect("unpoisoned"), vec![GIVER.to_owned()]);
        let live = &app.world().resource::<LiveInventoryRecord>().0;
        let landed = live.worlds.get("harbour_2").expect("under a free name");
        let joined = crate::pds::inventory::world::join_parts(item.head(), &parts).expect("joins");
        let own = WorldItem::snapshot(&joined).expect("copies");
        assert_eq!(landed, &own, "the snapshot of the sanitised record");
        assert!(landed.slices().is_some(), "its parts held until the save");
        let task = app
            .world_mut()
            .query::<&PublishInventoryTask>()
            .single(app.world())
            .expect("a save was started");
        assert!(task.published.worlds.contains_key("harbour_2"));
        assert!(task.published.generators.contains_key("harbour"));
        assert!(
            !task.published.generators.contains_key("unsaved"),
            "no unsaved edit rides along"
        );
        assert_eq!(
            answers(&app),
            [(4, GIVER.to_owned(), true, DeclineReason::Declined)]
        );
        assert!(
            toast_lines(&app)
                .iter()
                .any(|line| line.contains("harbour_2")),
            "{:?}",
            toast_lines(&app)
        );
    }

    /// #1847: a person's client, which never inserts [`GiftWorldsEnded`],
    /// lands a given world as ever and is left without one; where the agent
    /// client has inserted it, each landing that answered leaves one entry -
    /// where it landed, or the toast's reason - and a dropped one none.
    #[test]
    fn only_a_client_that_inserted_the_mailbox_hears_how_a_landing_ended() {
        let (item, parts) = given_world(56);
        let mut app = app_for(ME, InventoryRecord::default(), InventoryRecord::default());
        let (fetch, _) = recording_fetch(Ok(parts.clone()));
        start(&mut app, &fetch, item.head(), ME);
        settle(&mut app);
        assert!(
            app.world()
                .resource::<LiveInventoryRecord>()
                .0
                .worlds
                .contains_key("harbour")
        );
        assert!(!app.world().contains_resource::<GiftWorldsEnded>());

        let ended = |app: &App| app.world().resource::<GiftWorldsEnded>().0.clone();
        let mut app = app_for(ME, InventoryRecord::default(), InventoryRecord::default());
        app.init_resource::<GiftWorldsEnded>();
        start(&mut app, &fetch, item.head(), ME);
        settle(&mut app);
        assert_eq!(
            ended(&app),
            [GiftWorldEnded {
                offer_id: 4,
                giver_did: GIVER.to_owned(),
                name: String::from("harbour"),
                outcome: Ok(crate::ui::people::AcceptedGift {
                    key: String::from("harbour"),
                    saving: true,
                }),
            }]
        );

        let mut app = app_for(ME, InventoryRecord::default(), InventoryRecord::default());
        app.init_resource::<GiftWorldsEnded>();
        let (failing, _) = recording_fetch(Err(String::from("part 2 is missing")));
        start(&mut app, &failing, item.head(), ME);
        settle(&mut app);
        let failed = ended(&app);
        let [
            GiftWorldEnded {
                outcome: Err(reason),
                ..
            },
        ] = failed.as_slice()
        else {
            panic!("{failed:?}");
        };
        assert!(
            reason.contains("@giver.test's saved copy") && reason.contains("part 2 is missing"),
            "{reason}"
        );

        let mut app = app_for(
            "did:plc:someoneelsenow222222222",
            InventoryRecord::default(),
            InventoryRecord::default(),
        );
        app.init_resource::<GiftWorldsEnded>();
        start(&mut app, &fetch, item.head(), ME);
        settle(&mut app);
        assert!(ended(&app).is_empty(), "dropped, answering nobody");
    }

    /// #1844: a fetch that fails lands nothing, answers `NotFetched`, and
    /// says why in a toast.
    #[test]
    fn a_failed_fetch_answers_not_fetched_and_toasts() {
        let (item, _) = given_world(52);
        let mut app = app_for(ME, InventoryRecord::default(), InventoryRecord::default());
        let (fetch, _) = recording_fetch(Err(String::from("part 2 is missing")));

        start(&mut app, &fetch, item.head(), ME);
        settle(&mut app);

        assert!(
            app.world()
                .resource::<LiveInventoryRecord>()
                .0
                .worlds
                .is_empty()
        );
        assert_eq!(
            answers(&app),
            [(4, GIVER.to_owned(), false, DeclineReason::NotFetched)]
        );
        assert!(
            toast_lines(&app)
                .iter()
                .any(|line| line.contains("saved copy") && line.contains("part 2 is missing")),
            "{:?}",
            toast_lines(&app)
        );
        // Parts that do not add up to the head fail the same way.
        let mut app = app_for(ME, InventoryRecord::default(), InventoryRecord::default());
        let (fetch, _) = recording_fetch(Ok(vec![String::from("{}")]));
        start(&mut app, &fetch, item.head(), ME);
        settle(&mut app);
        assert_eq!(answers(&app)[0].3, DeclineReason::NotFetched);
    }

    /// #1844: a stash that filled while the world was fetched takes
    /// nothing, and the giver hears `Unavailable`.
    #[test]
    fn a_full_stash_at_landing_answers_unavailable() {
        let (item, parts) = given_world(53);
        let mut full = InventoryRecord::default();
        for n in 0..crate::config::state::MAX_INVENTORY_ITEMS {
            full.put_item(
                format!("item {n}"),
                crate::pds::Generator::default_cuboid(),
                None,
            );
        }
        let mut app = app_for(ME, full.clone(), full);
        let (fetch, _) = recording_fetch(Ok(parts));

        start(&mut app, &fetch, item.head(), ME);
        settle(&mut app);

        assert!(
            app.world()
                .resource::<LiveInventoryRecord>()
                .0
                .worlds
                .is_empty()
        );
        assert_eq!(
            answers(&app),
            [(4, GIVER.to_owned(), false, DeclineReason::Unavailable)]
        );
    }

    /// #1844: a session signed in as another account by the time the parts
    /// land drops them - nothing lands, and nothing is answered in a
    /// stranger's name.
    #[test]
    fn a_did_change_mid_fetch_lands_nothing() {
        let (item, parts) = given_world(54);
        let mut app = app_for(
            "did:plc:someoneelsenow222222222",
            InventoryRecord::default(),
            InventoryRecord::default(),
        );
        let (fetch, _) = recording_fetch(Ok(parts));

        start(&mut app, &fetch, item.head(), ME);
        settle(&mut app);

        assert!(
            app.world()
                .resource::<LiveInventoryRecord>()
                .0
                .worlds
                .is_empty()
        );
        assert!(answers(&app).is_empty());
        assert_eq!(
            app.world_mut()
                .query::<&PublishInventoryTask>()
                .iter(app.world())
                .count(),
            0
        );
    }
}
