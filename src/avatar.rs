//! Avatar fetch plugin: resolves a peer's Bluesky profile picture and paints
//! it onto every [`RoverSail`] or [`ChestBadge`] descendant of their
//! chassis — so a HoverRover flies it as a double-sided sail panel and a
//! Humanoid wears it on the chest.
//!
//! On native builds the profile blob is fetched straight from `cdn.bsky.app`.
//! On WASM that CDN lacks CORS headers, so `fetch_image_bytes` instead
//! resolves the author's PDS from their DID document and downloads the raw
//! blob via `com.atproto.sync.getBlob`.  Successful materials are cached on
//! the chassis entity ([`AvatarMaterial`]) so archetype rebuilds (hot-swap
//! HoverRover ↔ Humanoid, or a phenotype re-spawn) can re-apply the texture
//! to the freshly spawned child without another network round trip.

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde::Deserialize;

use crate::player::{ChestBadge, RoverSail};
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

/// Baked result of a completed bsky profile fetch. Cached per-DID so a peer
/// who rejoins a room — or several peers entering a portal at once that
/// share DIDs with peers seen earlier in the session — can skip the HTTPS
/// round trip entirely and render with the already-GPU-resident material.
#[derive(Clone)]
pub struct CachedBskyProfile {
    pub material: Handle<StandardMaterial>,
    pub handle: Option<String>,
}

/// DID → cached bsky profile material. Cleared on logout
/// (see `logout::cleanup_on_logout`) so a new session can't render a
/// previous user's peer with whatever was left over in GPU asset storage.
#[derive(Resource, Default)]
pub struct BskyProfileCache {
    by_did: std::collections::HashMap<String, CachedBskyProfile>,
}

impl BskyProfileCache {
    pub fn clear(&mut self) {
        self.by_did.clear();
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

/// Stores the last successfully applied avatar material on a chassis entity.
/// Used to re-apply the material to a new sail child after an airship rebuild
/// without triggering a redundant network fetch.
#[derive(Component, Clone)]
pub struct AvatarMaterial(pub Handle<StandardMaterial>);

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

#[allow(clippy::too_many_arguments)]
fn trigger_avatar_fetches(
    mut commands: Commands,
    pending: Query<(Entity, &AvatarFetchPending)>,
    cache: Res<BskyProfileCache>,
    mut peers: Query<&mut RemotePeer>,
    children_query: Query<&Children>,
    sail_query: Query<Entity, With<RoverSail>>,
    badge_query: Query<Entity, With<ChestBadge>>,
) {
    for (entity, pending) in pending.iter() {
        let did = pending.did.clone();
        commands.entity(entity).remove::<AvatarFetchPending>();

        // Cache hit — install the verified handle and paint the already-
        // GPU-resident material onto the chassis descendants. The bsky
        // CDN charges us a round trip per DID per session otherwise, and
        // a portal clustering 20 familiar peers at once would stall every
        // chassis on the IoTaskPool until those fetches unwind.
        if let Some(cached) = cache.by_did.get(&did) {
            if let Some(handle) = cached.handle.clone()
                && let Ok(mut peer) = peers.get_mut(entity)
            {
                peer.handle = Some(handle);
            }
            commands
                .entity(entity)
                .insert(AvatarMaterial(cached.material.clone()));
            apply_avatar_material_to_descendants(
                &mut commands,
                entity,
                cached.material.clone(),
                &children_query,
                &sail_query,
                &badge_query,
            );
            continue;
        }

        spawn_avatar_task(&mut commands, entity, did);
    }
}

/// Walk the chassis subtree and queue deferred `MeshMaterial3d` inserts on
/// every `RoverSail` / `ChestBadge` descendant. The deferred closure guards
/// against the target despawning between system execution and command
/// flush (happens when a rover rebuild lands in the same frame).
fn apply_avatar_material_to_descendants(
    commands: &mut Commands,
    root: Entity,
    mat: Handle<StandardMaterial>,
    children_query: &Query<&Children>,
    sail_query: &Query<Entity, With<RoverSail>>,
    badge_query: &Query<Entity, With<ChestBadge>>,
) {
    let mut targets: Vec<Entity> = Vec::new();
    let mut stack: Vec<Entity> = vec![root];
    while let Some(node) = stack.pop() {
        if sail_query.get(node).is_ok() || badge_query.get(node).is_ok() {
            targets.push(node);
        }
        if let Ok(children) = children_query.get(node) {
            stack.extend(children.iter());
        }
    }
    for target in targets {
        let mat = mat.clone();
        commands.queue(move |world: &mut World| {
            if let Ok(mut eref) = world.get_entity_mut(target) {
                eref.insert(MeshMaterial3d(mat));
            }
        });
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
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: ResMut<BskyProfileCache>,
    children_query: Query<&Children>,
    sail_query: Query<Entity, With<RoverSail>>,
    badge_query: Query<Entity, With<ChestBadge>>,
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
        let tex_handle = images.add(img);
        let new_mat = materials.add(StandardMaterial {
            base_color_texture: Some(tex_handle),
            base_color: Color::WHITE,
            unlit: true,
            double_sided: true,
            cull_mode: None,
            ..default()
        });

        // Cache the material on the chassis so rebuilds can re-apply it without
        // a new network fetch.
        commands
            .entity(entity)
            .insert(AvatarMaterial(new_mat.clone()));

        // Cache by DID so a future peer identifying with the same DID
        // (reconnect, portal hop with shared visitors) paints immediately
        // without re-hitting bsky / PDS. The material handle is cheap to
        // clone and keeps the underlying GPU texture alive as long as any
        // peer references it.
        cache.by_did.insert(
            did.clone(),
            CachedBskyProfile {
                material: new_mat.clone(),
                handle: verified_handle,
            },
        );

        // Apply the material to every RoverSail or ChestBadge descendant.
        // Walk the full child tree because the humanoid badge sits below an
        // intermediate `HumanoidVisualRoot`, not directly under the chassis.
        // The deferred world closure guards against a rebuild despawning the
        // target between system execution and command flush.
        let mut targets: Vec<Entity> = Vec::new();
        let mut stack: Vec<Entity> = vec![entity];
        while let Some(node) = stack.pop() {
            if sail_query.get(node).is_ok() || badge_query.get(node).is_ok() {
                targets.push(node);
            }
            if let Ok(children) = children_query.get(node) {
                stack.extend(children.iter());
            }
        }

        for target in targets {
            let mat = new_mat.clone();
            commands.queue(move |world: &mut World| {
                if let Ok(mut eref) = world.get_entity_mut(target) {
                    eref.insert(MeshMaterial3d(mat));
                }
            });
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
    let resp = client.get(avatar_url).send().await.ok()?;
    if !resp.status().is_success() {
        bevy::log::warn!(
            "Failed to fetch avatar image for {}: {}",
            did,
            resp.status()
        );
        return None;
    }
    Some(resp.bytes().await.ok()?.to_vec())
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
    let resp = client.get(&blob_url).send().await.ok()?;
    if !resp.status().is_success() {
        bevy::log::warn!("Failed to fetch avatar blob for {}: {}", did, resp.status());
        return None;
    }
    Some(resp.bytes().await.ok()?.to_vec())
}

#[cfg(target_arch = "wasm32")]
use crate::pds::resolve_pds;
