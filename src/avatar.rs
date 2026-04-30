//! Avatar fetch plugin: resolves a peer's Bluesky profile picture and
//! caches it as both a `Handle<Image>` and an `egui::TextureId`. The
//! cached image is consumed by the chat and People panels (where it
//! renders as a small icon next to each author's name).
//!
//! In-world avatar bodies do **not** carry the profile picture anymore —
//! the unified-avatar work moved that decoration to the egui side. The
//! cache is still keyed on DID so a peer who rejoins a room — or several
//! peers entering a portal at once that share DIDs with peers seen
//! earlier in the session — can skip the HTTPS round trip entirely and
//! render with the already-resident image.
//!
//! On native builds the profile blob is fetched straight from
//! `cdn.bsky.app`. On WASM that CDN lacks CORS headers, so
//! `fetch_image_bytes` instead resolves the author's PDS from their DID
//! document and downloads the raw blob via `com.atproto.sync.getBlob`.

use bevy::prelude::*;
use bevy_egui::{EguiTextureHandle, EguiUserTextures};
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde::Deserialize;

use crate::state::{AppState, LocalPlayer, RemotePeer};

pub struct AvatarPlugin;

impl Plugin for AvatarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BskyProfileCache>().add_systems(
            Update,
            (
                fetch_local_avatar,
                trigger_avatar_fetches,
                poll_avatar_tasks,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Baked result of a completed bsky profile fetch. Cached per-DID so a
/// peer who rejoins a room — or several peers entering a portal at once
/// that share DIDs with peers seen earlier in the session — can skip the
/// HTTPS round trip entirely and render with the already-resident image.
#[derive(Clone)]
pub struct CachedBskyProfile {
    /// Raw image asset handle. Kept so the egui texture remains valid
    /// across rebuilds; holders take a `Handle<Image>` clone.
    pub image: Handle<Image>,
    /// `egui::TextureId` reference to the same image, ready to drop into
    /// `egui::Image::from_texture` calls in chat / people panels.
    pub egui_texture: bevy_egui::egui::TextureId,
    pub handle: Option<String>,
}

/// DID → cached bsky profile picture. Cleared on logout (see
/// `logout::cleanup_on_logout`) so a new session can't render a previous
/// user's peer with whatever was left over in GPU asset storage.
#[derive(Resource, Default)]
pub struct BskyProfileCache {
    by_did: std::collections::HashMap<String, CachedBskyProfile>,
}

impl BskyProfileCache {
    pub fn clear(&mut self) {
        self.by_did.clear();
    }

    /// Look up a cached profile by DID. Returns `None` when the fetch is
    /// still in flight or the profile has no avatar set on bsky.
    pub fn get(&self, did: &str) -> Option<&CachedBskyProfile> {
        self.by_did.get(did)
    }
}

#[derive(Component)]
pub struct AvatarFetchPending {
    pub did: String,
}

/// Result of an ATProto profile fetch: the image blob (if any) and the
/// authoritative handle published alongside the DID's profile record.
/// Peer-supplied handles on the wire are untrusted — only the handle
/// returned by `app.bsky.actor.getProfile` for the authenticated DID is
/// authoritative.
#[derive(Default)]
pub struct AvatarFetchResult {
    pub bytes: Option<Vec<u8>>,
    pub handle: Option<String>,
}

#[derive(Component)]
pub struct AvatarFetchTask {
    pub did: String,
    pub task: bevy::tasks::Task<AvatarFetchResult>,
}

fn fetch_local_avatar(
    mut commands: Commands,
    session: Option<Res<AtprotoSession>>,
    player: Query<Entity, Added<LocalPlayer>>,
) {
    let Some(sess) = session else { return };
    let Ok(entity) = player.single() else { return };
    let did = sess.did.clone();
    spawn_avatar_task(&mut commands, entity, did);
}

fn trigger_avatar_fetches(
    mut commands: Commands,
    pending: Query<(Entity, &AvatarFetchPending)>,
    cache: Res<BskyProfileCache>,
    mut peers: Query<&mut RemotePeer>,
) {
    for (entity, pending) in pending.iter() {
        let did = pending.did.clone();
        commands.entity(entity).remove::<AvatarFetchPending>();

        // Cache hit — install the verified handle directly. The bsky CDN
        // charges us a round trip per DID per session otherwise, and a
        // portal clustering 20 familiar peers at once would stall every
        // chassis on the IoTaskPool until those fetches unwind.
        if let Some(cached) = cache.by_did.get(&did) {
            if let Some(handle) = cached.handle.clone()
                && let Ok(mut peer) = peers.get_mut(entity)
            {
                peer.handle = Some(handle);
            }
            continue;
        }

        spawn_avatar_task(&mut commands, entity, did);
    }
}

fn spawn_avatar_task(commands: &mut Commands, entity: Entity, did: String) {
    // Blocking ATProto profile fetches belong on `IoTaskPool`; mixing them
    // onto `AsyncComputeTaskPool` would pin its core-count-sized workers on
    // socket reads and stall GLTF/asset work for every other system.
    let pool = bevy::tasks::IoTaskPool::get();
    let did_for_fetch = did.clone();
    let task = pool.spawn(async move {
        let fut = fetch_avatar_bytes(did_for_fetch);
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
    commands
        .entity(entity)
        .insert(AvatarFetchTask { did, task });
}

#[allow(clippy::too_many_arguments)]
fn poll_avatar_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut AvatarFetchTask)>,
    mut peers: Query<&mut RemotePeer>,
    mut images: ResMut<Assets<Image>>,
    mut egui_textures: ResMut<EguiUserTextures>,
    mut cache: ResMut<BskyProfileCache>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };

        let did = task.did.clone();
        commands.entity(entity).remove::<AvatarFetchTask>();

        // Promote the profile-verified handle to the authoritative one on
        // the peer entity. The handle field on `OverlandsMessage::Identity`
        // is peer-supplied and cannot be trusted — a malicious peer could
        // claim any string they like to impersonate another user in the
        // chat HUD or disconnect log. Only a handle resolved from the
        // authenticated DID's profile record is safe to display.
        let verified_handle = result.handle.clone();
        if let Some(handle) = verified_handle.clone()
            && let Ok(mut peer) = peers.get_mut(entity)
        {
            peer.handle = Some(handle);
        }

        let Some(bytes) = result.bytes else { continue };

        let Ok(dyn_img) = image::load_from_memory(&bytes) else {
            bevy::log::warn!("Failed to decode avatar image");
            continue;
        };

        let img = Image::from_dynamic(
            dyn_img,
            true,
            bevy::asset::RenderAssetUsages::MAIN_WORLD
                | bevy::asset::RenderAssetUsages::RENDER_WORLD,
        );
        let image_handle = images.add(img);
        // `add_image` takes an `EguiTextureHandle`; wrap the strong
        // handle so egui shares ownership and the texture survives any
        // later release on our side. The cloned handle still lives in
        // `CachedBskyProfile.image` so the asset stays GC-anchored even
        // if egui drops its half.
        let egui_texture = egui_textures.add_image(EguiTextureHandle::Strong(image_handle.clone()));

        cache.by_did.insert(
            did.clone(),
            CachedBskyProfile {
                image: image_handle,
                egui_texture,
                handle: verified_handle,
            },
        );
    }
}

#[derive(Deserialize)]
struct BskyProfile {
    avatar: Option<String>,
    handle: Option<String>,
}

pub(crate) async fn fetch_avatar_bytes(did: String) -> AvatarFetchResult {
    let mut out = AvatarFetchResult::default();
    let client = crate::config::http::default_client();

    let url = format!(
        "https://public.api.bsky.app/xrpc/app.bsky.actor.getProfile?actor={}",
        did
    );

    let Ok(resp) = client.get(&url).send().await else {
        return out;
    };
    if !resp.status().is_success() {
        bevy::log::warn!("Failed to fetch profile for {}: {}", did, resp.status());
        return out;
    }

    let Ok(profile) = resp.json::<BskyProfile>().await else {
        return out;
    };
    out.handle = profile.handle;

    if let Some(avatar_url) = profile.avatar {
        out.bytes = fetch_image_bytes(&client, &did, &avatar_url).await;
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_image_bytes(
    client: &reqwest::Client,
    did: &str,
    avatar_url: &str,
) -> Option<Vec<u8>> {
    // `crate::pds::fetch_blob_bytes_capped` streams chunks and aborts
    // past `MAX_FETCH_BODY_BYTES`. Without this, a hostile bsky CDN /
    // PDS hosting an attacker-controlled DID could return an
    // infinitely-streaming body (`/dev/zero` over HTTP) and `reqwest`
    // would buffer the whole stream into memory until the client OOMs.
    let bytes = crate::pds::xrpc::fetch_blob_bytes_capped(client, avatar_url).await;
    if bytes.is_none() {
        bevy::log::warn!("Failed to fetch avatar image for {}", did);
    }
    bytes
}

/// WASM: cdn.bsky.app lacks CORS headers, so resolve the user's PDS from
/// their DID document and fetch the raw blob via `com.atproto.sync.getBlob`.
#[cfg(target_arch = "wasm32")]
async fn fetch_image_bytes(
    client: &reqwest::Client,
    did: &str,
    avatar_url: &str,
) -> Option<Vec<u8>> {
    let cid = avatar_url.rsplit('/').next()?.split('@').next()?;
    let pds = resolve_pds(client, did).await?;
    let blob_url = format!(
        "{}/xrpc/com.atproto.sync.getBlob?did={}&cid={}",
        pds, did, cid
    );
    // Same size-cap rationale as the native path — a hostile PDS
    // serving `com.atproto.sync.getBlob` can otherwise stream a
    // multi-gigabyte body and OOM the WASM client.
    let bytes = crate::pds::xrpc::fetch_blob_bytes_capped(client, &blob_url).await;
    if bytes.is_none() {
        bevy::log::warn!("Failed to fetch avatar blob for {}", did);
    }
    bytes
}

#[cfg(target_arch = "wasm32")]
use crate::pds::resolve_pds;

/// Render a small profile-picture icon for `did` next to a chat row or
/// a People-panel entry. When the cache holds an `egui::TextureId` for
/// this DID, draws a `bevy_egui::egui::Image` sized at `size` px square. When
/// the cache misses (load still in flight, no profile picture, or
/// `did` is `None`), allocates the same square as a transparent spacer
/// so the parent row layout doesn't shift between frames as the load
/// resolves.
pub fn draw_avatar_icon(
    ui: &mut bevy_egui::egui::Ui,
    did: Option<&str>,
    cache: &BskyProfileCache,
    size: f32,
) {
    use bevy_egui::egui;

    let texture_id = did.and_then(|d| cache.get(d)).map(|p| p.egui_texture);
    match texture_id {
        Some(texture_id) => {
            ui.add(egui::Image::from_texture((
                texture_id,
                egui::vec2(size, size),
            )));
        }
        None => {
            ui.allocate_space(egui::vec2(size, size));
        }
    }
}
