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
    AppState, ChatHistory, DiagnosticsLog, LiveAvatarRecord, LiveInventoryRecord, LiveRoomRecord,
    LocalPlayer, PendingOutgoingOffers, PublishFeedback, RelayHost, RemotePeer, RoomRecordRecovery,
    StoredAvatarRecord, StoredInventoryRecord, StoredRoomRecord,
};
use crate::world_builder::RoomEntity;
use crate::world_builder::image_cache::BlobImageCache;

pub struct LogoutPlugin;

impl Plugin for LogoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnExit(AppState::InGame), cleanup_on_logout);
    }
}

#[allow(clippy::too_many_arguments)]
fn cleanup_on_logout(
    mut commands: Commands,
    players: Query<Entity, With<LocalPlayer>>,
    peers: Query<Entity, With<RemotePeer>>,
    room_entities: Query<Entity, With<RoomEntity>>,
    session: Option<Res<AtprotoSession>>,
    refresh_ctx: Option<Res<OauthRefreshCtx>>,
    mut chat: ResMut<ChatHistory>,
    mut diagnostics: ResMut<DiagnosticsLog>,
    mut avatar_cache: ResMut<PeerAvatarCache>,
    mut bsky_cache: ResMut<BskyProfileCache>,
    mut blob_image_cache: ResMut<BlobImageCache>,
    mut pending_offers: ResMut<PendingOutgoingOffers>,
    mut baked_audio_cache: ResMut<crate::world_builder::spatial_audio::BakedAudioCache>,
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
                let fut = revoke_oauth_tokens(&session, &client, &metadata);
                #[cfg(not(target_arch = "wasm32"))]
                {
                    // Reuses the process-shared Tokio runtime — see
                    // `crate::config::http::block_on` — so logout's
                    // best-effort token revocation no longer constructs
                    // a one-shot runtime just to drop it again.
                    if let Err(e) = crate::config::http::block_on(fut) {
                        warn!("OAuth token revocation failed; clearing local state anyway: {e}");
                    }
                }
                #[cfg(target_arch = "wasm32")]
                {
                    if let Err(e) = fut.await {
                        warn!("OAuth token revocation failed; clearing local state anyway: {e}");
                    }
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

    // Drop the active recipe so a later login does not compile the old
    // room's contents into the new session's scene graph.
    commands.remove_resource::<LiveRoomRecord>();
    commands.remove_resource::<StoredRoomRecord>();
    commands.remove_resource::<LiveAvatarRecord>();
    commands.remove_resource::<StoredAvatarRecord>();
    commands.remove_resource::<LiveInventoryRecord>();
    commands.remove_resource::<StoredInventoryRecord>();
    // Clear any recovery marker from this session so a fresh login does
    // not start with the "incompatible record" banner still showing.
    commands.remove_resource::<RoomRecordRecovery>();
    // Defensive: the unsaved-edits guard removes itself when it proceeds,
    // but if anything else ever drives the InGame→Login edge while a
    // dialog is open, a stale guard must not greet the next login.
    commands.remove_resource::<crate::ui::unsaved_guard::UnsavedGuard>();
    // The world this session compiled is being despawned just below, so
    // the next login's loading gate must wait for a fresh compile pass —
    // and the compile fingerprint must not short-circuit it into
    // skipping the rebuild of a now-empty scene.
    commands.remove_resource::<crate::world_builder::WorldCompiled>();
    commands.insert_resource(crate::world_builder::compile::CompiledWorldFingerprint::default());

    // Reset (don't remove — these are app-lifetime `init_resource`s, so
    // a missing one would panic the next editor frame) every per-record
    // publish-status line back to `Idle`, so re-logging in as a
    // different user never shows the previous session's stale
    // "✓ Saved (Ns ago)".
    commands.insert_resource(PublishFeedback::<RoomRecord>::default());
    // Same reset-don't-remove treatment for the toolbar's panel flags:
    // the next session starts from the defaults, including a fresh
    // first-run controls hint.
    commands.insert_resource(crate::ui::toolbar::UiPanels::default());
    commands.insert_resource(PublishFeedback::<AvatarRecord>::default());
    commands.insert_resource(PublishFeedback::<InventoryRecord>::default());

    // Hard logout path: tear down every session + networking resource.
    commands.remove_resource::<AtprotoSession>();
    commands.remove_resource::<crate::oauth::OauthRefreshCtx>();
    commands.remove_resource::<TokenSourceRes>();
    commands.remove_resource::<SymbiosMultiuserConfig<OverlandsMessage>>();
    commands.remove_resource::<RelayHost>();

    // Drop the persisted session blob so the next page load lands back
    // on the login screen instead of silently restoring the stale
    // identity. WASM-only: native sessions aren't persisted today.
    #[cfg(target_arch = "wasm32")]
    crate::oauth::wasm::clear_persisted();

    // Reset in-memory buffers so the next session starts fresh.
    chat.messages.clear();
    *diagnostics = DiagnosticsLog::default();
    // Drop the peer avatar cache so a new login can't see the previous
    // user's peers; the cache lives by DID, so a stale entry would install
    // a stranger's vessel the moment a new session's peer Identity claim
    // happened to match a DID from the old room.
    avatar_cache.clear();
    // Likewise for the bsky profile material cache — if the previous user
    // lingered on a peer's pfp we don't want to render it on someone else
    // after a DID collision.
    bsky_cache.clear();
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
}
