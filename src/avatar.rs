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
///
/// FIFO-bounded at [`crate::config::network::MAX_BSKY_PROFILE_CACHE_ENTRIES`]
/// (#1125), mirroring `PeerAvatarCache`. Logout was previously the only
/// thing that ever removed an entry, so a session that met many DIDs grew
/// until it ended.
///
/// **Dropping an entry is not enough to free its image.** `add_image` is
/// given an `EguiTextureHandle::Strong`, so egui holds a strong handle of
/// its own and the asset outlives anything this map does — which is why
/// both [`insert`](Self::insert) and [`clear`](Self::clear) hand the
/// removed entries back rather than dropping them: the caller has
/// `EguiUserTextures` and must call `remove_image` on each.
#[derive(Resource, Default)]
pub struct BskyProfileCache {
    by_did: std::collections::HashMap<String, CachedBskyProfile>,
    order: std::collections::VecDeque<String>,
}

impl BskyProfileCache {
    /// Empty the cache, returning every entry so the caller can release
    /// its egui texture. Dropping the returned entries alone frees
    /// nothing — see the type docs.
    #[must_use = "the returned entries still hold egui's strong image handles"]
    pub fn clear(&mut self) -> Vec<CachedBskyProfile> {
        self.order.clear();
        self.by_did.drain().map(|(_, entry)| entry).collect()
    }

    /// Cache `profile` under `did`, returning any entries evicted to stay
    /// within the bound. Re-caching a DID refreshes its position rather
    /// than adding a second queue entry, so one peer reconnecting
    /// repeatedly cannot evict everyone else.
    #[must_use = "evicted entries still hold egui's strong image handles"]
    pub fn insert(&mut self, did: String, profile: CachedBskyProfile) -> Vec<CachedBskyProfile> {
        let mut evicted = Vec::new();
        if let Some(previous) = self.by_did.remove(&did) {
            self.order.retain(|d| d != &did);
            evicted.push(previous);
        }
        while self.order.len() >= crate::config::network::MAX_BSKY_PROFILE_CACHE_ENTRIES {
            match self.order.pop_front() {
                Some(oldest) => evicted.extend(self.by_did.remove(&oldest)),
                None => break,
            }
        }
        self.order.push_back(did.clone());
        self.by_did.insert(did, profile);
        evicted
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

/// Presence line for a peer whose verified handle just resolved (#844):
/// "joined" is announced at identification, not at raw socket connect —
/// that's the first moment there is a trustworthy name to print. Styled
/// as the same system authorship the portal arrival line uses; does NOT
/// bump the unread badge (presence is ambience, not a message).
fn push_joined_line(chat: &mut crate::state::ChatHistory, handle: &str) {
    chat.push(None, "system", format!("@{handle} joined the room."));
}

fn trigger_avatar_fetches(
    mut commands: Commands,
    pending: Query<(Entity, &AvatarFetchPending)>,
    cache: Res<BskyProfileCache>,
    mut peers: Query<&mut RemotePeer>,
    mut chat: ResMut<crate::state::ChatHistory>,
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
                if peer.handle.is_none() {
                    push_joined_line(&mut chat, &handle);
                }
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
        crate::config::http::run_or(fut, AvatarFetchResult::default()).await
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
    mut chat: ResMut<crate::state::ChatHistory>,
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
            if peer.handle.is_none() {
                push_joined_line(&mut chat, &handle);
            }
            peer.handle = Some(handle);
        }

        let Some(bytes) = result.bytes else { continue };

        let Some(dyn_img) =
            crate::world_builder::blob_fetch::decode_image_capped(&bytes, "Avatar image")
        else {
            continue;
        };

        // Downscale BEFORE the image reaches `Assets<Image>` (#1125): what
        // is retained is what is stored, and this is only ever drawn at
        // `AVATAR_ICON_PX`. On wasm the source comes from the DID's own
        // PDS rather than the resizing bsky CDN, so without this an entry
        // is whatever the owner uploaded — up to 64 MiB of RGBA each.
        let icon_px = crate::config::network::BSKY_PROFILE_ICON_PX;
        let dyn_img = if dyn_img.width() > icon_px || dyn_img.height() > icon_px {
            dyn_img.resize(icon_px, icon_px, image::imageops::FilterType::Triangle)
        } else {
            dyn_img
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

        for evicted in cache.insert(
            did.clone(),
            CachedBskyProfile {
                image: image_handle,
                egui_texture,
                handle: verified_handle,
            },
        ) {
            // Releases egui's strong handle; the asset is freed once ours
            // goes out of scope with `evicted`.
            egui_textures.remove_image(&evicted.image);
        }
    }
}

#[derive(Deserialize)]
struct BskyProfile {
    avatar: Option<String>,
    handle: Option<String>,
}

pub(crate) async fn fetch_avatar_bytes(did: String) -> AvatarFetchResult {
    let mut out = AvatarFetchResult::default();
    // Owner monuments and portals ask for their room owner's PFP, and the
    // login backdrop's demo world owns itself under a synthetic
    // `did:attract:…`. The AppView answers 400 for any DID it cannot
    // resolve, so a profile-less identity would log a failure warning on
    // every attract scene. Skip the round-trip instead.
    if !crate::pds::xrpc::is_resolvable_did(&did) {
        return out;
    }
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

#[cfg(test)]
mod profile_cache_tests {
    use super::*;
    use bevy_egui::egui;

    /// An entry with a distinguishable texture id. `Handle::default()` is
    /// enough here: what is under test is the eviction bookkeeping, not the
    /// asset it points at.
    fn entry(tag: u64) -> CachedBskyProfile {
        CachedBskyProfile {
            image: Handle::default(),
            egui_texture: egui::TextureId::User(tag),
            handle: None,
        }
    }

    /// #1125: the cache was populated by every peer Identity and emptied
    /// only at logout, so a relay or peer set churning DIDs grew a guest's
    /// heap for the whole session — and wasm never gives heap back.
    #[test]
    fn the_cache_evicts_oldest_first_past_its_bound() {
        let mut cache = BskyProfileCache::default();
        let bound = crate::config::network::MAX_BSKY_PROFILE_CACHE_ENTRIES;
        for i in 0..bound {
            assert!(
                cache
                    .insert(format!("did:plc:{i}"), entry(i as u64))
                    .is_empty(),
                "nothing is evicted while there is room"
            );
        }
        let evicted = cache.insert(String::from("did:plc:one-too-many"), entry(9999));
        assert_eq!(evicted.len(), 1, "exactly one entry makes way");
        assert_eq!(
            evicted[0].egui_texture,
            egui::TextureId::User(0),
            "and it is the oldest"
        );
        assert!(cache.get("did:plc:0").is_none());
        assert!(cache.get("did:plc:one-too-many").is_some());
    }

    /// One peer reconnecting repeatedly must not evict everyone else: a
    /// re-cache refreshes the DID's slot rather than queueing a second one.
    /// The stale entry still comes back, because its egui handle needs
    /// releasing just as an evicted one does.
    #[test]
    fn re_caching_a_did_replaces_its_entry_without_consuming_a_slot() {
        let mut cache = BskyProfileCache::default();
        assert!(cache.insert(String::from("did:plc:a"), entry(1)).is_empty());
        assert!(cache.insert(String::from("did:plc:b"), entry(2)).is_empty());

        let replaced = cache.insert(String::from("did:plc:a"), entry(3));
        assert_eq!(replaced.len(), 1, "the superseded entry is handed back");
        assert_eq!(replaced[0].egui_texture, egui::TextureId::User(1));
        assert_eq!(
            cache.get("did:plc:a").map(|p| p.egui_texture),
            Some(egui::TextureId::User(3))
        );
        assert!(
            cache.get("did:plc:b").is_some(),
            "nobody else was disturbed"
        );
    }

    /// Clearing hands every entry back for the same reason eviction does:
    /// `add_image` was given a STRONG handle, so egui outlives this map and
    /// dropping entries frees nothing on its own. Before #1125 logout
    /// cleared the map and left every icon the session had decoded resident
    /// for the life of the page.
    #[test]
    fn clearing_returns_every_entry_so_its_texture_can_be_released() {
        let mut cache = BskyProfileCache::default();
        for i in 0..8 {
            let _ = cache.insert(format!("did:plc:{i}"), entry(i));
        }
        let released = cache.clear();
        assert_eq!(released.len(), 8, "every entry comes back to be released");
        assert!(cache.get("did:plc:0").is_none());
        // And the queue is emptied with the map, so the next insert starts
        // from a clean bound rather than an inherited backlog.
        assert!(
            cache
                .insert(String::from("did:plc:fresh"), entry(99))
                .is_empty(),
            "a cleared cache has room again"
        );
    }
}
