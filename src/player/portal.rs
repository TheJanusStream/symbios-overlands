//! Portal travel: reading the local player's collision-sensor set and
//! driving the async ATProto room-record fetch that carries them to a new
//! world.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::diagnostics::SessionLog;
use crate::diagnostics::event::EventPayload;
use crate::pds::{FetchError, RoomRecord, fetch_room_record};
use crate::state::{
    CurrentRoomDid, LiveRoomRecord, LocalPlayer, RemotePeer, RoomRecordRecovery, TravelPhase,
    TravelingTo,
};
use crate::ui::unsaved_guard::{GuardedAction, UnsavedGuard};
use crate::world_builder::PortalMarker;

/// An in-flight destination room-record fetch.
///
/// `pub` (with a private field) so `logout::clear_editor_state_on_logout`
/// can sweep these entities (#1140) and `ui::travel`'s *Cancel travel*
/// button can despawn them (#1231 f25): the task carries neither `LocalPlayer` nor
/// `RoomEntity`, so the logout despawn passes it by — and on wasm dropping
/// a `Task` does not cancel the work behind it, so an abandoned fetch
/// really does resolve inside the NEXT session.
#[derive(Component)]
pub struct PortalTravelTask {
    pub(super) task: bevy::tasks::Task<Result<Option<RoomRecord>, FetchError>>,
    /// The DID this fetch was dispatched for. [`poll_portal_travel_tasks`]
    /// refuses a result whose target does not match the travel that is
    /// actually pending, so a task outliving its own `TravelingTo` can
    /// never install a room nobody asked for.
    target_did: String,
}

/// Suppresses portal interactions for a brief window after a successful
/// teleport. Without this, a portal whose `target_pos` lands the player
/// inside any portal collider (the source itself or one at the
/// destination) would re-fire `handle_portal_interaction` every frame:
/// each invocation snaps the player back to `target_pos` and zeros their
/// velocity, preventing the player from ever moving out of the overlap
/// — a permanent soft-lock until logout.
///
/// `until_secs` is taken against `Time::elapsed_secs_f64()`; the cooldown
/// is "long enough for a humanoid to walk out of a typical portal
/// collider, short enough that deliberate re-entry feels responsive."
#[derive(Resource)]
pub struct PortalCooldown {
    pub until_secs: f64,
}

pub(crate) const PORTAL_COOLDOWN_SECS: f64 = 0.75;

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_portal_interaction(
    mut commands: Commands,
    mut players: Query<
        (
            &CollidingEntities,
            &mut Transform,
            &mut LinearVelocity,
            &mut AngularVelocity,
        ),
        With<LocalPlayer>,
    >,
    portals: Query<&PortalMarker>,
    current_room: Option<Res<CurrentRoomDid>>,
    traveling: Option<Res<TravelingTo>>,
    guard: Option<Res<UnsavedGuard>>,
    modal: Res<crate::ui::confirm::ModalOpen>,
    cooldown: Option<Res<PortalCooldown>>,
    time: Res<Time>,
) {
    // Guard against re-entry: once a travel task is in flight, the player
    // keeps coasting through the portal collider for several frames. Without
    // this check the Update system would spawn a fresh IoTaskPool fetch each
    // frame, flooding the pool and stalling every other background task.
    if traveling.is_some() {
        return;
    }
    // A modal is already up — don't stack another action behind it
    // (#852, widened by #1241 f164). This asked about the unsaved-edits
    // guard alone, so a player who kept walking under a GIFT OFFER (which
    // blocks the pointer but not the keys) could raise the guard behind
    // it: two modals, one of them invisible under the other.
    if guard.is_some() || modal.0 {
        return;
    }
    // Post-teleport cooldown: keeps a portal-overlapping arrival from
    // immediately re-firing this system and locking the player into a
    // single point. See [`PortalCooldown`].
    let now = time.elapsed_secs_f64();
    if let Some(cd) = cooldown.as_deref()
        && now < cd.until_secs
    {
        return;
    }

    let Ok((collisions, mut tf, mut lv, mut av)) = players.single_mut() else {
        return;
    };

    for entity in collisions.iter() {
        let Ok(portal) = portals.get(*entity) else {
            continue;
        };

        let same_room = current_room
            .as_deref()
            .map(|r| r.0 == portal.target_did)
            .unwrap_or(false);
        if same_room {
            tf.translation = portal.target_pos;
            lv.0 = Vec3::ZERO;
            av.0 = Vec3::ZERO;
            // Engage the cooldown so a portal whose target_pos lies
            // inside its own (or another portal's) collider can't
            // soft-lock the player by re-firing each frame.
            commands.insert_resource(PortalCooldown {
                until_secs: now + PORTAL_COOLDOWN_SECS,
            });
        } else {
            // Inter-room portal: Freeze the player and hand the travel to
            // the unsaved-edits guard. Zero momentum so the player doesn't
            // re-collide with the portal on the next frame.
            //
            // The guard owns the dirty decision: when the local user has
            // no unpublished room edits it calls [`begin_portal_travel`]
            // on the very next frame without showing anything; when they
            // do, it offers Publish / Discard / Stay first. Starting the
            // fetch directly here would bypass that choice and silently
            // overwrite the live record.
            lv.0 = Vec3::ZERO;
            av.0 = Vec3::ZERO;
            commands.insert_resource(UnsavedGuard::new(GuardedAction::PortalTravel {
                via: crate::ui::unsaved_guard::TravelVia::Portal,
                target_did: portal.target_did.clone(),
                // A portal in the world carries a DID and nothing else;
                // `travel_label` resolves what it can at render time.
                target_label: None,
                target_pos: Some(portal.target_pos),
            }));
        }
        break;
    }
}

/// Start the inter-room travel: pin [`TravelingTo`] (which suppresses
/// further portal interaction until the swap completes or fails) and
/// dispatch the async destination room-record fetch. Called by the
/// unsaved-edits guard once any dirty-record question is settled.
/// `target_pos: None` (#745) arrives at the destination record's
/// `default_landing` — see [`TravelingTo`].
pub(crate) fn begin_portal_travel(
    commands: &mut Commands,
    session_log: &mut SessionLog,
    now: f64,
    target_did: String,
    target_label: Option<String>,
    target_pos: Option<Vec3>,
) {
    // The `[Timeline]` has rendered "portal → did" since the analyzer was
    // written, and docs/diagnostics.md promises it — but nothing emitted it,
    // so a session with several hops read as a session with none (#1144).
    session_log.info(
        now,
        EventPayload::PortalTravelInitiated {
            target_did: target_did.clone(),
        },
    );
    commands.insert_resource(TravelingTo {
        target_did: target_did.clone(),
        target_pos,
        target_label,
        phase: TravelPhase::Fetching,
    });

    let pool = bevy::tasks::IoTaskPool::get();
    // `reqwest` spawns internal timer/IO futures the moment it issues
    // a request, which panics with "there is no reactor running"
    // unless the future is driven inside a tokio runtime. The
    // `IoTaskPool` is a plain async-executor, so on native we build
    // a per-task single-threaded runtime (same pattern as every
    // other HTTP-spawning site in the crate — see
    // `network::spawn_peer_avatar_fetch` /
    // `lib::spawn_avatar_record_fetch`). wasm32 has no tokio; the
    // browser's JS runtime backs `fetch`, so the bare future works.
    let fetch_did = target_did.clone();
    let task = pool.spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            fetch_room_record(&client, &fetch_did).await
        };
        crate::config::http::run_or(
            fut,
            Err(crate::pds::FetchError::Network(
                crate::config::http::timed_out("portal room fetch"),
            )),
        )
        .await
    });
    commands.spawn(PortalTravelTask { task, target_did });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn poll_portal_travel_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PortalTravelTask)>,
    mut traveling: Option<ResMut<TravelingTo>>,
    mut room_record: Option<ResMut<LiveRoomRecord>>,
    mut stored_room: Option<ResMut<crate::state::StoredRoomRecord>>,
    mut current_did: Option<ResMut<CurrentRoomDid>>,
    mut chat: ResMut<crate::state::ChatHistory>,
    mut session_log: ResMut<SessionLog>,
    relay_host: Option<Res<crate::state::RelayHost>>,
    peers: Query<Entity, With<RemotePeer>>,
    mut players: Query<
        (&mut Transform, &mut LinearVelocity, &mut AngularVelocity),
        With<LocalPlayer>,
    >,
    time: Res<Time>,
    mut toasts: ResMut<crate::notify::Toasts>,
    profile_cache: Res<crate::avatar::BskyProfileCache>,
    mut undo_signals: ResMut<crate::state::RoomWriteSignals>,
    // Cleared when the swap lands (#1204): the cache's own contract is
    // "cleared on room transitions" so a self-updating `DidPfp` source is
    // re-fetched in the next world, and logout was its only clear site.
    mut blob_image_cache: ResMut<crate::world_builder::image_cache::BlobImageCache>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) = bevy::tasks::futures_lite::future::block_on(
            bevy::tasks::futures_lite::future::poll_once(&mut task.task),
        ) else {
            continue;
        };

        commands.entity(entity).despawn();
        let Some(travel_data) = traveling.as_deref_mut() else {
            continue;
        };
        // The result has to belong to the travel that is still pending
        // (#1140). Logout despawns these tasks, but on wasm the work
        // behind a dropped `Task` keeps running and a survivor landing in
        // a later session would hot-swap the room, the socket and the
        // player's position for a portal nobody walked through. Comparing
        // targets makes the poll answer only its own question.
        if task.target_did != travel_data.target_did {
            continue;
        }

        // 1. Resolve the new record. The four arms preserve the original
        // owner's PDS contents — substituting the default on a transient
        // network failure would silently destroy the destination user's
        // real room as soon as they (or any autosave hook) clicked
        // "Save". Mirrors the loading-pipeline policy in
        // `loading::poll_room_record_task`.
        let elapsed = time.elapsed_secs_f64();
        let mut new_record = match result {
            // Owner has saved a record — install it. A clean install also
            // clears any recovery banner a previous room raised (#840):
            // carrying it into this room would offer a "Reset PDS to
            // default" against a HEALTHY record.
            Ok(Some(r)) => {
                commands.remove_resource::<RoomRecordRecovery>();
                r
            }
            // 404: the destination owner has never published an overland.
            // Synthesising the default is safe — and clean, so the
            // stale-banner sweep applies here too (#840) — but it is not a
            // silent success (#1232 f28). The visitor cannot tell
            // "@alice's overland" from "a world we invented for a DID
            // that may not even exist", and the second is exactly what a
            // mistyped link produces. It is also the one piece of social
            // information worth having here: this person has not set
            // theirs up yet.
            Ok(None) => {
                commands.remove_resource::<RoomRecordRecovery>();
                toasts.info(
                    format!(
                        "{} hasn't built a world yet — this one is generated from their identifier.",
                        crate::ui::travel::travel_label(
                            &profile_cache,
                            &travel_data.target_did,
                            travel_data.target_label.as_deref(),
                        )
                    ),
                    elapsed,
                );
                RoomRecord::default_for_did(&travel_data.target_did)
            }
            // Schema-incompatible record on the PDS: not transient, so
            // retrying won't help. Fall through to the default and raise
            // a recovery banner the destination owner can clear by
            // re-publishing.
            Err(FetchError::Decode(msg)) => {
                session_log.warn(
                    elapsed,
                    EventPayload::RoomRecoveryBannerRaised {
                        reason: msg.clone(),
                    },
                );
                warn!(
                    "Portal travel decode error ({}) — installing default + recovery marker",
                    msg
                );
                commands.insert_resource(RoomRecordRecovery {
                    // Decode by construction — this is the `FetchError::Decode`
                    // arm, the one cause that knows the stored record is
                    // unreadable rather than merely unread (#1265 f210).
                    cause: crate::state::RecoveryCause::Decode,
                    reason: msg.clone(),
                });
                RoomRecord::default_for_did(&travel_data.target_did)
            }
            // Transient failure (DNS, timeout, 5xx, DID resolution
            // hiccup): abort the travel and keep the player in the
            // current room. The destination owner's real record stays
            // safe; the user can walk into the portal again to retry.
            Err(err) => {
                session_log.warn(
                    elapsed,
                    EventPayload::PortalTravelFailed {
                        target_did: travel_data.target_did.clone(),
                        reason: format!("{err:?}"),
                    },
                );
                warn!("Portal travel fetch failed: {:?} — aborting travel", err);
                // The player just unfreezes with zero explanation without
                // this — "the portal did nothing" (#842).
                toasts.error(
                    format!(
                        "Couldn't reach {}'s world — walk into the portal again to retry.",
                        crate::ui::travel::travel_label(
                            &profile_cache,
                            &travel_data.target_did,
                            travel_data.target_label.as_deref(),
                        )
                    ),
                    elapsed,
                );
                commands.remove_resource::<TravelingTo>();
                // Brief cooldown so the player isn't immediately
                // re-pulled into the same failing fetch by their own
                // collision overlap with the source portal.
                commands.insert_resource(PortalCooldown {
                    until_secs: elapsed + PORTAL_COOLDOWN_SECS,
                });
                continue;
            }
        };
        new_record.sanitize();
        // Captured before the record moves into the resources below: a
        // travel with no baked target (gateway, #745) arrives at the
        // destination owner's configured landing pose.
        let destination_landing = new_record.default_landing;

        // 2. Hot-swap the ECS Resources (Triggers `world_builder` automatically!)
        if let Some(rec) = room_record.as_mut() {
            rec.0 = new_record.clone();
            // Wholesale replacement the editor didn't author: the undo
            // capture must reset to a fresh baseline, not record this as
            // an edit (#862). The room-DID identity key would catch a
            // cross-DID travel anyway; the signal also covers a self-loop
            // portal back into the same room.
            undo_signals.foreign = true;
        }
        if let Some(stored) = stored_room.as_mut() {
            **stored = crate::state::StoredRoomRecord(new_record);
        }
        if let Some(did) = current_did.as_mut() {
            did.0 = travel_data.target_did.clone();
        }
        // The browser's saved session follows the player (#1229 f2). It
        // used to record only where they FIRST signed in, so a reload
        // silently teleported anyone who had travelled back to their
        // login-time room — and a visitor onboarded through a friend's
        // landmark link was returned to that friend's world forever.
        crate::oauth::remember_room(&travel_data.target_did);
        // A same-owner record held for the room being left (#1203) is a
        // question about a world this session is no longer in.
        commands.remove_resource::<crate::ui::other_session::OtherSessionRoom>();
        blob_image_cache.clear();

        // 3. Hot-swap the WebRTC Socket
        commands.remove_resource::<bevy_symbios_multiuser::prelude::SymbiosMultiuserConfig<
            crate::protocol::OverlandsMessage,
        >>();
        if let Some(host) = relay_host.as_deref() {
            commands.insert_resource(bevy_symbios_multiuser::prelude::SymbiosMultiuserConfig::<
                crate::protocol::OverlandsMessage,
            > {
                room_url: format!("wss://{}/overlands/{}", host.0, travel_data.target_did),
                ice_servers: crate::config::network::ice_servers(),
                _marker: std::marker::PhantomData,
            });
        }

        // 3a. Despawn the origin-region's remote peers. Tearing down the
        // multiuser socket above *should* surface `Disconnected` events for
        // each peer, but those events fan through the plugin's own systems
        // next frame and are not guaranteed to sweep the ECS entities —
        // leaving mute, frozen chassis sitting at the origin's last
        // broadcast transform in the new region. The fresh socket's
        // `Connected` events will re-spawn each peer we still share a
        // room with. `try_despawn` tolerates the case where a parent
        // despawn (e.g. an in-flight avatar rebuild queued this frame)
        // already took a child down.
        for peer_entity in &peers {
            commands.entity(peer_entity).try_despawn();
        }

        // 4. Teleport player and clear momentum. A baked portal target is
        // used verbatim (classic portals, translation only — facing is
        // left alone, as ever). Without one, the destination's
        // `default_landing` supplies position *and* facing; without that,
        // the legacy origin scatter. Landing heights are provisional when
        // the pose is drop-pin (`y: None`) — the destination heightmap
        // doesn't exist yet at this point (the record swap above only
        // *queued* the terrain rebuild), so we park at y = 0 and let
        // `lift_player_above_new_ground` snap the chassis onto the new
        // ground the frame the heightmap lands, exactly as it already does
        // for re-seeds and stale baked targets.
        if let Ok((mut tf, mut lv, mut av)) = players.single_mut() {
            let (arrival, yaw_deg) = match travel_data.target_pos {
                Some(pos) => (pos, None),
                None => match destination_landing {
                    Some(landing) => (
                        Vec3::new(
                            landing.pos.0[0],
                            landing.y.map(|y| y.0).unwrap_or(0.0),
                            landing.pos.0[1],
                        ),
                        Some(landing.yaw_deg.0),
                    ),
                    None => {
                        let (x, z) = super::random_spawn_xz();
                        (Vec3::new(x, 0.0, z), None)
                    }
                },
            };
            tf.translation = arrival;
            if let Some(deg) = yaw_deg {
                tf.rotation = Quat::from_rotation_y(deg.to_radians());
            }
            lv.0 = Vec3::ZERO;
            av.0 = Vec3::ZERO;
        }

        // 5. Clean up state
        chat.messages.clear();
        // Arrival line (#842): the hard cut wipes the world AND the chat
        // history in the same frame — say where we landed and why the
        // scrollback vanished.
        chat.push(
            None,
            "system",
            format!(
                "Arrived in {}'s world — chat history starts fresh here.",
                crate::ui::travel::travel_label(
                    &profile_cache,
                    &travel_data.target_did,
                    travel_data.target_label.as_deref(),
                )
            ),
        );
        // NOT removed here (#1231 f20). The record has landed; the
        // destination has not been built. Releasing the freeze now dropped
        // the player at the landing pose — `y = 0` for a gateway hop, and
        // frequently under the ground still standing where they left — to
        // watch terrain regen and a time-sliced compile assemble the world
        // around them with no overlay at all, in the one journey the
        // loading screen's vocabulary already covers.
        //
        // `WorldCompiled` goes with it: the marker is still standing from
        // the world being LEFT, so without dropping it the gate would see
        // a finished compile that finished somewhere else.
        // `check_loading_complete` reads it only in `AppState::Loading`,
        // which this is not.
        travel_data.phase = TravelPhase::Building;
        commands.remove_resource::<crate::world_builder::WorldCompiled>();
        session_log.info(
            elapsed,
            EventPayload::PortalTravelCompleted {
                target_did: travel_data.target_did.clone(),
            },
        );
        // Engage the post-arrival cooldown so a portal at the
        // destination whose collider envelops the spawn position can't
        // immediately yank the player back through.
        commands.insert_resource(PortalCooldown {
            until_secs: elapsed + PORTAL_COOLDOWN_SECS,
        });
    }
}

/// Release the arrival gate once the destination actually exists (#1231
/// f20).
///
/// The two conditions are the ones `loading::check_loading_complete` waits
/// on for the same stretch of work — a finished heightmap and a finished
/// compile — because it is the same stretch of work. A travel that lands
/// on a terrain config serialising identically to the one being left never
/// drops `FinishedHeightMap` at all, so that arm is already satisfied and
/// the gate closes on the compile alone.
///
/// Releasing also snaps the chassis onto the ground it arrived above.
/// `lift_player_above_new_ground` covers the common case, but it fires on
/// `FinishedHeightMap::is_added` — precisely the case an identical terrain
/// config does not produce — and a gateway hop with a drop-pin landing
/// arrives at a literal `y = 0.0`, so the one arrival that got no lift was
/// the one most likely to need it.
pub(super) fn release_travel_on_arrival(
    mut commands: Commands,
    traveling: Option<Res<TravelingTo>>,
    heightmap: Option<Res<crate::terrain::FinishedHeightMap>>,
    compiled: Option<Res<crate::world_builder::WorldCompiled>>,
    mut players: Query<
        (&mut Position, &mut LinearVelocity, &mut AngularVelocity),
        With<LocalPlayer>,
    >,
    mut room_editor: Option<ResMut<crate::ui::room::RoomEditorState>>,
) {
    let Some(traveling) = traveling.as_deref() else {
        return;
    };
    if traveling.phase != TravelPhase::Building {
        return;
    }
    let (Some(heightmap), Some(_)) = (heightmap.as_deref(), compiled.as_deref()) else {
        return;
    };
    if let Ok((mut pos, mut lin, mut ang)) = players.single_mut() {
        super::hotswap::snap_above_ground(&heightmap.0, &mut pos, &mut lin, &mut ang);
    }
    // The selection belonged to the world we LEFT (#1237 f142). Travel
    // swaps the record, the DID, the socket, the peers, the chat and the
    // player's pose and never touched the editor state, so an index into
    // the old room's placements arrived pointing into a stranger's. The
    // ownership gates elsewhere stop it being *drawn* or *dragged*; this
    // is the state itself not surviving the journey.
    if let Some(room_editor) = room_editor.as_deref_mut()
        && room_editor.has_selection()
    {
        room_editor.clear_selection();
    }
    commands.remove_resource::<TravelingTo>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::FinishedHeightMap;
    use crate::world_builder::WorldCompiled;
    use bevy_symbios_ground::HeightMap;

    /// A world holding a travel in its Building phase, a local player at
    /// `y`, and whichever gate conditions are asked for.
    fn arriving(y: f32, heightmap: bool, compiled: bool) -> World {
        let mut world = World::new();
        world.insert_resource(TravelingTo {
            target_did: "did:plc:alice".into(),
            target_pos: None,
            target_label: Some("@alice.bsky.social".into()),
            phase: TravelPhase::Building,
        });
        if heightmap {
            // 3x3 grid, 2 m cells, ground flat at 10 m — well above the
            // `y = 0.0` a gateway hop with a drop-pin landing arrives at.
            let mut hm = HeightMap::new(3, 3, 2.0);
            for cell in hm.data_mut() {
                *cell = 10.0;
            }
            world.insert_resource(FinishedHeightMap(hm));
        }
        if compiled {
            world.insert_resource(WorldCompiled);
        }
        world.spawn((
            LocalPlayer,
            Position(Vec3::new(0.0, y, 0.0)),
            LinearVelocity(Vec3::new(0.0, -12.0, 0.0)),
            AngularVelocity(Vec3::ZERO),
        ));
        world
    }

    fn run_gate(world: &mut World) {
        world
            .run_system_cached(release_travel_on_arrival)
            .expect("the arrival gate runs");
    }

    /// THE SEQUENCE (#1231 f20): the player walks through a portal. The
    /// record lands, `TravelingTo` was removed on that frame, the freeze
    /// released and the card vanished — while terrain regen had not
    /// started and the time-sliced compile had not run. They were dropped
    /// at the landing pose, watched the destination assemble around them,
    /// and for a gateway hop that pose is a literal `y = 0.0`, frequently
    /// below the ground still standing where they left.
    ///
    /// The gate now waits on the two conditions `check_loading_complete`
    /// waits on for the same work.
    #[test]
    fn the_arrival_gate_holds_until_the_destination_exists() {
        for (heightmap, compiled) in [(false, false), (true, false), (false, true)] {
            let mut world = arriving(0.0, heightmap, compiled);
            run_gate(&mut world);
            assert!(
                world.contains_resource::<TravelingTo>(),
                "released with heightmap={heightmap} compiled={compiled}: the \
                 world the player is standing in does not exist yet"
            );
        }

        let mut world = arriving(0.0, true, true);
        run_gate(&mut world);
        assert!(
            !world.contains_resource::<TravelingTo>(),
            "both conditions met — the freeze has to release"
        );
    }

    /// And releasing puts the player on the ground rather than inside it.
    /// `lift_player_above_new_ground` fires on `FinishedHeightMap::
    /// is_added`, which a destination whose terrain config serialises
    /// identically to the origin's never produces — so the one arrival
    /// that never got a lift was the gateway hop that lands at `y = 0.0`.
    #[test]
    fn releasing_the_gate_stands_the_player_on_the_new_ground() {
        let mut world = arriving(0.0, true, true);
        run_gate(&mut world);
        let mut players = world.query::<(&Position, &LinearVelocity)>();
        let (pos, vel) = players.iter(&world).next().expect("the local player");
        assert!(pos.y >= 10.0, "left at y={} with the ground at 10 m", pos.y);
        assert_eq!(
            vel.0,
            Vec3::ZERO,
            "the fall the arrival pose implied must not be carried into the \
             new world"
        );
    }

    /// The gate is for the second half only. A fetch still in flight is
    /// the first half, and releasing it there would abandon the travel.
    #[test]
    fn a_fetch_still_in_flight_is_not_an_arrival() {
        let mut world = arriving(0.0, true, true);
        world.resource_mut::<TravelingTo>().phase = TravelPhase::Fetching;
        run_gate(&mut world);
        assert!(world.contains_resource::<TravelingTo>());
    }
}
