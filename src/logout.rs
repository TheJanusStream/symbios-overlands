//! Logout cleanup: despawn game entities and remove session/game resources
//! when transitioning from [`crate::state::AppState::InGame`] back to
//! [`crate::state::AppState::Login`].
//!
//! Runs on `OnExit(AppState::InGame)`. Removing the
//! [`bevy_symbios_multiuser::prelude::SymbiosMultiuserConfig`] resource
//! tears down the existing matchbox socket on the next frame (see
//! `bevy_symbios_multiuser` docs).

use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use bevy_symbios_multiuser::auth::{AtprotoSession, logout as revoke_oauth_tokens};
use bevy_symbios_multiuser::prelude::SymbiosMultiuserConfig;
use bevy_symbios_multiuser::signaller::TokenSourceRes;

use crate::avatar::BskyProfileCache;
use crate::network::PeerAvatarCache;
use crate::oauth::OauthRefreshCtx;
use crate::pds::{AvatarRecord, InventoryRecord, RoomRecord};
use crate::protocol::OverlandsMessage;
use crate::state::{
    AppState, ChatHistory, CurrentRoomDid, LiveAvatarRecord, LiveInventoryRecord, LiveRoomRecord,
    LocalPlayer, PendingOutgoingOffers, PublishFeedback, RelayHost, RemotePeer, RoomRecordRecovery,
    StoredAvatarRecord, StoredInventoryRecord, StoredRoomRecord,
};
use crate::world_builder::RoomEntity;
use crate::world_builder::image_cache::BlobImageCache;

pub struct LogoutPlugin;

impl Plugin for LogoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnExit(AppState::InGame),
            (cleanup_on_logout, clear_editor_state_on_logout),
        );
    }
}

/// The resources one logged-in session owns, and the teardown that drops
/// them — declared **once** so the two can never disagree (#1140, finding
/// 118 of #1152).
///
/// The bug this exists to prevent: `CurrentRoomDid` was installed by
/// `ui::login::complete::install_completed_session` and had no remove site
/// anywhere in the crate, so the previous session's room DID sat in the
/// world across a logout. It was harmless only because the next login
/// happened to overwrite it — `TravelingTo` next door was not harmless at
/// all. Every entry below is a resource whose value is a claim about ONE
/// session (who is logged in, which room, what is in flight); an
/// app-lifetime resource that merely needs *resetting* is not listed here,
/// because removing it would panic the next frame that reads it — those
/// live in `cleanup_on_logout` as `insert_resource(Default)` instead.
///
/// The list generates both the teardown and the name set the drift test
/// diffs the login install set against, so adding a type here is the whole
/// change; forgetting to add one fails
/// `every_resource_login_installs_is_torn_down_at_logout`.
macro_rules! session_scoped_resources {
    ($($ty:ty),* $(,)?) => {
        /// Drop every session-scoped resource. Split out of
        /// [`cleanup_on_logout`] so the list is one declaration rather
        /// than a run of hand-written lines a new resource can miss.
        fn remove_session_scoped_resources(commands: &mut Commands) {
            $(commands.remove_resource::<$ty>();)*
        }

        /// The same list as type names, for the drift test.
        #[cfg(test)]
        fn session_scoped_resource_names() -> Vec<&'static str> {
            vec![$(std::any::type_name::<$ty>()),*]
        }
    };
}

session_scoped_resources! {
    // Identity. Removing `SymbiosMultiuserConfig` tears down the matchbox
    // socket on the next frame (see the module docs above).
    AtprotoSession,
    crate::oauth::OauthRefreshCtx,
    TokenSourceRes,
    SymbiosMultiuserConfig<OverlandsMessage>,
    RelayHost,
    // Which overland this session was visiting, and the arrival pose the
    // login handed forward. `PendingSpawnPlacement` is normally consumed by
    // `player::spawn`, but a logout from the loading screen (#849) never
    // reaches it.
    CurrentRoomDid,
    crate::state::PendingSpawnPlacement,
    // In-flight portal travel (#1140). `TravelingTo` is otherwise removed
    // only inside `poll_portal_travel_tasks`, so logging out mid-fetch left
    // it pinned: every drive system early-returns while it is present, so
    // the next login started frozen under a travel overlay that never
    // cleared. The entity carrying the fetch is swept in
    // `clear_editor_state_on_logout`.
    crate::state::TravelingTo,
    crate::player::PortalCooldown,
    // The three records and their stored mirrors: the previous user's
    // world, body and stash.
    LiveRoomRecord,
    StoredRoomRecord,
    LiveAvatarRecord,
    StoredAvatarRecord,
    LiveInventoryRecord,
    StoredInventoryRecord,
    // Recovery markers (#840): a fresh login must not open with the
    // previous session's "incompatible record" banner offering to reset a
    // repo it has never read.
    RoomRecordRecovery,
    crate::state::AvatarRecordRecovery,
    crate::state::InventoryRecordRecovery,
    // Defensive: the unsaved-edits guard removes itself when it proceeds,
    // but if anything else ever drives the InGame->Login edge while a
    // dialog is open, a stale guard must not greet the next login.
    crate::ui::unsaved_guard::UnsavedGuard,
    // The gateway picker pair (#748): logging out while standing in a
    // gateway zone must not leave the picker (or its dismissal latch)
    // armed for the next session.
    crate::ui::gateway::GatewayPicker,
    crate::ui::gateway::GatewayDismissed,
    // The world this session compiled is despawned in `cleanup_on_logout`,
    // so the next login's loading gate must wait for a fresh compile pass —
    // and the per-unit fingerprints must not short-circuit it into skipping
    // the rebuild of a now-empty scene. Any in-flight sliced job is dropped
    // with them (its queue indexes the old record). `WorldCompileArmed`
    // re-arms the one-frame compile delay for the next Loading pass (#849).
    crate::world_builder::WorldCompiled,
    crate::world_builder::WorldCompileArmed,
}

/// Reset the two editor windows' cross-frame state and sweep any in-flight
/// portal fetch (#1140).
///
/// Kept out of [`cleanup_on_logout`], which already sits at Bevy's 16-param
/// ceiling — the same reason `ui::undo::clear_history_on_logout` is its own
/// system.
///
/// `AvatarEditorState` and `RoomEditorState` are app-lifetime
/// `init_resource`s, so they are *reset*, never removed. The avatar half is
/// the one that bit: a worn-prop selection made before logging out keeps
/// `holds_avatar_still()` true on the first `InGame` frame of the NEXT
/// login, which parks the new chassis with ALL_LOCKED and pins the rig to
/// rest — while `sync_gizmo_selection` finds no entity carrying that rkey,
/// so there is no gizmo on screen to explain it or to click away. The
/// release path (`release_hidden_selections`) only runs under
/// `in_state(InGame)`, so nothing between the two sessions could ever have
/// cleared it. The same resource also carried the previous account's
/// wardrobe listing across the boundary, where one click on a row would
/// have made the new user wear — and republish under the old user's rkey —
/// a body from a repo they do not own.
///
/// `RoomEditorState` is reset alongside it. Its stale selection names a
/// generator in a room that no longer exists, so nothing freezes and no
/// other identity's data is exposed — but it is the same resource with the
/// same lifetime and the same absence of any other teardown, and leaving
/// exactly one of the pair behind is how the next reader concludes that
/// editor state is meant to survive a logout.
fn clear_editor_state_on_logout(
    mut commands: Commands,
    travel_tasks: Query<Entity, With<crate::player::PortalTravelTask>>,
    mut avatar_editor: ResMut<crate::ui::avatar::AvatarEditorState>,
    mut room_editor: ResMut<crate::ui::room::RoomEditorState>,
) {
    *avatar_editor = crate::ui::avatar::AvatarEditorState::default();
    *room_editor = crate::ui::room::RoomEditorState::default();
    // Neither `LocalPlayer` nor `RoomEntity`, so `cleanup_on_logout`'s
    // despawn sweep never reaches these. Dropping the `Task` is enough on
    // native; on wasm the work behind it keeps running (project memory,
    // #560-563), which is why `poll_portal_travel_tasks` also refuses a
    // result whose target does not match the pending travel.
    for entity in &travel_tasks {
        commands.entity(entity).try_despawn();
    }
}

// Grab-bag teardown system at Bevy's 16-param ceiling: the session-scoped caches
// ride in a tuple param (their combined type trips `type_complexity`) to stay
// under it, so both lints are expected here.
//
// `pub(crate)`: besides the `OnExit(InGame)` registration above, the
// loading screen's "Back to login" abort path (#849) runs this on demand
// via `commands.run_system_cached` — aborting a stuck load is a real
// logout (session, sockets, caches), it just never reached `InGame`.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn cleanup_on_logout(
    mut commands: Commands,
    players: Query<Entity, With<LocalPlayer>>,
    peers: Query<Entity, With<RemotePeer>>,
    room_entities: Query<Entity, With<RoomEntity>>,
    session: Option<Res<AtprotoSession>>,
    refresh_ctx: Option<Res<OauthRefreshCtx>>,
    mut chat: ResMut<ChatHistory>,
    // Grouped into one tuple param to stay within Bevy's 16-param system arity;
    // the per-generator and primitive caches ride along here for the same reason (#625).
    (
        mut session_log,
        mut metrics,
        time,
        mut shape_mesh,
        mut shape_material,
        mut lsystem_mesh,
        mut lsystem_material,
        mut prim_mesh,
        mut prim_material,
        mut egui_textures,
    ): (
        ResMut<crate::diagnostics::SessionLog>,
        ResMut<crate::diagnostics::MetricsRegistry>,
        Res<Time>,
        ResMut<crate::world_builder::ShapeMeshCache>,
        ResMut<crate::world_builder::ShapeMaterialCache>,
        ResMut<crate::world_builder::LSystemMeshCache>,
        ResMut<crate::world_builder::LSystemMaterialCache>,
        ResMut<crate::world_builder::prim_cache::PrimMeshCache>,
        ResMut<crate::world_builder::prim_cache::PrimMaterialCache>,
        // Rides in the tuple for the arity reason above; needed to release
        // egui's strong handles on the profile images (#1125).
        ResMut<bevy_egui::EguiUserTextures>,
    ),
    mut avatar_cache: ResMut<PeerAvatarCache>,
    mut bsky_cache: ResMut<BskyProfileCache>,
    mut blob_image_cache: ResMut<BlobImageCache>,
    mut pending_offers: ResMut<PendingOutgoingOffers>,
    mut baked_audio_cache: ResMut<crate::world_builder::spatial_audio::BakedAudioCache>,
    mut upstream_shape_mesh: ResMut<bevy_symbios_shape::cache::ShapeMeshCache>,
    ambient_players: Query<Entity, With<crate::loading::AmbientPlayer>>,
    mut playing_ambient: ResMut<crate::loading::PlayingAmbient>,
) {
    // Best-effort: revoke the OAuth tokens at the user's PDS (RFC 7009)
    // before we drop the session. Fire-and-forget on IoTaskPool because
    // the network round-trip mustn't block the OnExit transition or
    // delay the local-state cleanup below — local state is wiped
    // regardless of the network outcome.
    //
    // See `bevy_symbios_multiuser::auth::logout` for the refresh-then-access
    // ordering rationale and the documented best-effort semantics.
    if let (Some(session), Some(ctx)) = (session.as_deref(), refresh_ctx.as_deref()) {
        let session = session.clone();
        let client = ctx.client.clone();
        let metadata = ctx.server_metadata.clone();
        IoTaskPool::get()
            .spawn(async move {
                // `run_or` rather than a hand-rolled cfg fork (#1129): it
                // reuses the process-shared Tokio runtime on native and
                // bounds the browser fetch on wasm. Revocation is
                // best-effort by design, so a timeout is logged and the
                // local state is cleared regardless — but an unbounded
                // wait would leave this detached task alive for the rest
                // of the page's life.
                let fut = revoke_oauth_tokens(&session, &client, &metadata);
                if let Err(e) = crate::config::http::run_or(
                    fut,
                    Err(bevy_symbios_multiuser::error::SymbiosError::AuthFailed(
                        crate::config::http::timed_out("token revocation"),
                    )),
                )
                .await
                {
                    warn!("OAuth token revocation failed; clearing local state anyway: {e}");
                }
            })
            .detach();
    }

    // Despawn game-world entities (recursive by default in Bevy 0.18).
    //
    // `try_despawn` swallows the `EntityMutableFetchError` that fires
    // when an entity has already been despawned this frame — which can
    // happen when a parent's recursive despawn reaches a child before
    // the child's own queue entry runs, or when a deferred closure
    // queued by a gameplay system (e.g. `commands.queue(...)` in the
    // avatar paint pipeline) lands in the same apply pass. The warnings
    // are harmless but noisy; using `try_despawn` keeps the log clean
    // without masking genuine lifecycle bugs elsewhere.
    for e in &players {
        commands.entity(e).try_despawn();
    }
    for e in &peers {
        commands.entity(e).try_despawn();
    }
    // Also drop every world-compiler output (L-systems, scatter props,
    // water volumes). `terrain.rs` despawns the heightfield on its own
    // `OnExit(InGame)` hook, but the world builder does not — without
    // this loop, trees and shapes from the previous room would sit
    // orphaned in the ECS until the next room loaded.
    for e in &room_entities {
        commands.entity(e).try_despawn();
    }
    // The ambient bed plays on its own `AmbientPlayer` entity, which is not
    // a `RoomEntity` / player / peer, so nothing above reaches it. Without
    // this it would keep looping after logout and — because the next
    // login's `spawn` path can't see a survivor it didn't track — leave two
    // overlapping loops playing. Despawn it and forget the handle.
    for e in &ambient_players {
        commands.entity(e).try_despawn();
    }
    playing_ambient.clear();

    // Every resource that belongs to the departing session, in one
    // declaration shared with the login-install drift test — see
    // [`session_scoped_resources`]. Hand-written runs of `remove_resource`
    // are what let `CurrentRoomDid` and `TravelingTo` survive a logout
    // (#1140).
    remove_session_scoped_resources(&mut commands);
    commands.insert_resource(crate::world_builder::compile::CompiledWorld::default());
    commands.insert_resource(crate::world_builder::compile::CompileJob::default());

    // Reset (don't remove — these are app-lifetime `init_resource`s, so
    // a missing one would panic the next editor frame) every per-record
    // publish-status line back to `Idle`, so re-logging in as a
    // different user never shows the previous session's stale
    // "✓ Saved (Ns ago)".
    commands.insert_resource(PublishFeedback::<RoomRecord>::default());
    // UiPanels is deliberately NOT reset here (#820): panel layout is a
    // machine-local preference persisted by `crate::prefs`, so logging
    // out and back in reopens the same windows — and the dismissed
    // first-run Controls hint stays dismissed instead of greeting the
    // user every session.
    commands.insert_resource(PublishFeedback::<AvatarRecord>::default());
    commands.insert_resource(PublishFeedback::<InventoryRecord>::default());
    // Toasts are session-scoped feedback: a "Copied: …" from the old
    // session must not greet the next login's first InGame frames.
    commands.insert_resource(crate::ui::toast::Toasts::default());
    // Grammar compile statuses (#829) describe the OLD session's world;
    // the next login's arrival compile rewrites its own set.
    commands.insert_resource(crate::world_builder::grammar_diag::GrammarDiagnostics::default());

    // Drop the persisted session blob so the next page load lands back
    // on the login screen instead of silently restoring the stale
    // identity. WASM-only: native sessions aren't persisted today.
    #[cfg(target_arch = "wasm32")]
    crate::oauth::wasm::clear_persisted();

    // Reset in-memory buffers so the next session starts fresh. Whole
    // resource, not just `messages`: `unread` drove the toolbar badge into
    // the next login, and since #1140 the half-typed input line lives here
    // too — a draft is one keystroke from being sent under a new identity.
    *chat = ChatHistory::default();
    // Roll the diagnostic stream into a fresh segment: flush the departing
    // session to disk, then clear the in-memory tail so the next user's HUD
    // starts blank. The on-disk NDJSON file keeps the full history (the segment
    // boundary is marked in it), so no post-mortem data is lost while the GUI
    // shows nothing cross-session.
    session_log.reset_segment(time.elapsed_secs_f64(), "logout");
    session_log.flush();
    // Wipe the metrics registry too, so one session's counters/gauges/histograms
    // never bleed into the next login (parallels the session-log reset above).
    metrics.clear();
    // Drop the peer avatar cache so a new login can't see the previous
    // user's peers; the cache lives by DID, so a stale entry would install
    // a stranger's vessel the moment a new session's peer Identity claim
    // happened to match a DID from the old room.
    avatar_cache.clear();
    // Likewise for the bsky profile material cache — if the previous user
    // lingered on a peer's pfp we don't want to render it on someone else
    // after a DID collision.
    // Releasing egui's strong handles is what actually frees the profile
    // images (#1125): `add_image` was given `EguiTextureHandle::Strong`, so
    // clearing the map alone left every icon this session ever decoded
    // resident for the life of the page.
    for evicted in bsky_cache.clear() {
        egui_textures.remove_image(&evicted.image);
    }
    // The shared blob image cache holds `Handle<Image>` keyed by source
    // (URL / atproto blob / DID-pfp) across compile passes for both Sign
    // generators and Portal top-face pfps; same DID-collision argument
    // applies, and any pending tasks that complete after logout would
    // otherwise paint the previous session's image into a fresh
    // generator pointing at the same source.
    blob_image_cache.clear();
    // Pending outgoing offers are session-scoped — a new login must not
    // inherit the previous user's outstanding gifts (different DID, the
    // recipient could never authenticate a response back into the map).
    pending_offers.by_id.clear();
    pending_offers.next_id = 0;
    // Baked-audio buffers are content-keyed (not session-keyed), but
    // dropping them on logout releases the pinned AudioSource bytes and
    // any in-flight Pending waiter lists that point at entities the
    // teardown above just despawned.
    baked_audio_cache.clear();

    // The upstream shape-mesh cache is keyed by float-exact terminal footprint
    // and never evicts, so an editing session's slider drags pin a growing set
    // of `Handle<Mesh>` that otherwise survives logout into the next session.
    // Drop them here (the full-rebuild GC bounds it within a session; this
    // bounds it across login cycles). See `world_builder::compile`.
    upstream_shape_mesh.clear();

    // The four per-generator geometry/material caches (Shape + L-system) are
    // bounded within a session by the full-rebuild GC, but nothing else clears
    // them at logout — so the last room's `Handle<Mesh>` / `Handle<StandardMaterial>`
    // survive into the next login. Drop them here (#625).
    shape_mesh.clear();
    shape_material.clear();
    lsystem_mesh.clear();
    lsystem_material.clear();
    // The content-addressed primitive caches (#918) have no generator ref to
    // GC against, so within a session they are bounded only by capacity —
    // making them the same retention hazard, and clearing them here the same
    // fix (#625).
    prim_mesh.clear();
    prim_material.clear();

    // The procedural `TextureCache` (FIFO-64, ~192 MiB worst case) is content-
    // keyed and deliberately survives room *changes*, but nothing clears it at
    // logout — its retained `Handle<Image>`s are the dominant texture memory the
    // next session would inherit. Re-insert a fresh one to release them (#625).
    commands.insert_resource(crate::world_builder::fresh_texture_cache());
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::ecs::world::CommandQueue;

    /// Apply `f` against a scratch world through a real `Commands`, so the
    /// tests exercise the same deferred path the systems do.
    fn with_commands(world: &mut World, f: impl FnOnce(&mut Commands)) {
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, world);
        f(&mut commands);
        queue.apply(world);
    }

    fn resource_names(world: &World) -> Vec<String> {
        world
            .iter_resources()
            .map(|(info, _)| info.name().to_string())
            .collect()
    }

    /// A minimal but real [`crate::ui::login::CompletedSession`]. Nothing
    /// here talks to the network — the point is only that
    /// `install_completed_session` runs its full body and inserts exactly
    /// the resources it inserts in production.
    fn completed_session() -> crate::ui::login::CompletedSession {
        use proto_blue_oauth::types::TokenSet;
        use proto_blue_oauth::{DpopKey, DpopNonceCache, OAuthSession};

        let token_set = TokenSet {
            issuer: "https://example.invalid".into(),
            sub: "did:plc:alice".into(),
            scope: "atproto".into(),
            access_token: "access".into(),
            refresh_token: Some("refresh".into()),
            token_type: "DPoP".into(),
            expires_at: Some("2099-01-01T00:00:00Z".into()),
            aud: None,
        };
        let session = std::sync::Arc::new(OAuthSession::new(
            token_set,
            DpopKey::generate().expect("dpop keygen"),
            DpopNonceCache::new(),
        ));
        // Deserialised rather than hand-built: `OAuthServerMetadata` has no
        // `Default` and its optional half is irrelevant here.
        let server_metadata = serde_json::from_str(
            r#"{"issuer":"https://example.invalid",
                "authorization_endpoint":"https://example.invalid/authorize",
                "token_endpoint":"https://example.invalid/token"}"#,
        )
        .expect("server metadata");

        crate::ui::login::CompletedSession {
            session: AtprotoSession {
                did: "did:plc:alice".into(),
                handle: "alice.test".into(),
                pds_url: "https://example.invalid".into(),
                session,
            },
            refresh_ctx: OauthRefreshCtx {
                client: crate::oauth::OauthClientRes::default().0,
                server_metadata,
            },
            service_token: "service-token".into(),
            room_did: "did:plc:bob".into(),
            spawn_pos: Some(crate::boot_params::TargetPos {
                x: 1.0,
                y: None,
                z: 2.0,
            }),
            spawn_yaw_deg: Some(90.0),
        }
    }

    /// The guard for the `CurrentRoomDid` class of leak (#1140): a login
    /// installs a resource, nobody ever removes it, and it sits in the
    /// world across the logout into the next user's session. Diffing the
    /// two sets means the omission fails here rather than surfacing as
    /// "the game is broken" three sessions later.
    ///
    /// It fails against the pre-#1140 teardown: `CurrentRoomDid` and
    /// `PendingSpawnPlacement` were installed at `complete.rs:73`/`:88` and
    /// had zero remove sites in the crate.
    #[test]
    fn every_resource_login_installs_is_torn_down_at_logout() {
        let mut world = World::new();
        // `World::new()` seeds its own bookkeeping resources (Bevy's
        // `DefaultQueryFilters`, for one), so the install set is the
        // difference, not the whole world.
        let before: Vec<String> = resource_names(&world);

        let mut next_state = NextState::<AppState>::default();
        with_commands(&mut world, |commands| {
            crate::ui::login::complete::install_completed_session(
                commands,
                &mut next_state,
                completed_session(),
                Some(&RelayHost("relay.test".into())),
            );
        });

        let installed: Vec<String> = resource_names(&world)
            .into_iter()
            .filter(|name| !before.contains(name))
            .collect();
        assert!(
            !installed.is_empty(),
            "the install path inserted nothing — the test is measuring the wrong thing"
        );

        let torn_down = session_scoped_resource_names();
        for name in installed {
            assert!(
                torn_down.contains(&name.as_str()),
                "login installs {name} but logout never removes it; add it to \
                 `session_scoped_resources!` or explain in that list why it outlives a session"
            );
        }
    }

    /// The travel half of #1140. Sequence: walk into a portal, let the
    /// destination fetch start, and log out from the toolbar before it
    /// lands. `TravelingTo` was removed only inside
    /// `poll_portal_travel_tasks`, so it survived — and every drive system
    /// early-returns while it is present, which is a frozen avatar under a
    /// "Traveling to …" overlay on the next login, followed by an
    /// unrequested room swap when the old fetch finally resolves.
    #[test]
    fn logging_out_mid_travel_leaves_no_travel_state_behind() {
        let mut world = World::new();
        world.insert_resource(CurrentRoomDid("did:plc:alice".into()));
        world.insert_resource(crate::state::TravelingTo {
            target_did: "did:plc:bob".into(),
            target_pos: None,
        });
        world.insert_resource(crate::player::PortalCooldown { until_secs: 12.0 });

        with_commands(&mut world, remove_session_scoped_resources);

        assert!(!world.contains_resource::<crate::state::TravelingTo>());
        assert!(!world.contains_resource::<crate::player::PortalCooldown>());
        assert!(!world.contains_resource::<CurrentRoomDid>());
    }

    /// The avatar-editor half of #1140. Sequence: select a worn prop so the
    /// offset gizmo comes up, log out with the Avatar window still open
    /// (`UiPanels` is deliberately persisted, #820), log back in — as
    /// anyone. `AvatarEditorState` is an app-lifetime `init_resource` that
    /// no teardown touched, so `holds_avatar_still()` was already true on
    /// the new session's first frame and parked the fresh chassis with no
    /// gizmo on screen to release.
    #[test]
    fn a_worn_prop_selection_does_not_survive_logout() {
        let mut world = World::new();
        let mut editor = crate::ui::avatar::AvatarEditorState::default();
        editor.select_attachment_from_scene_pick("3lkabcxyz".into());
        assert!(
            editor.holds_avatar_still(),
            "precondition: the freeze holds"
        );
        world.insert_resource(editor);
        world.insert_resource(crate::ui::room::RoomEditorState::default());

        world
            .run_system_once(clear_editor_state_on_logout)
            .expect("teardown system");

        let editor = world.resource::<crate::ui::avatar::AvatarEditorState>();
        assert!(
            !editor.holds_avatar_still(),
            "a selection made in the previous session still freezes the new one's body"
        );
        assert_eq!(editor.selected_attachment(), None);
    }
}
