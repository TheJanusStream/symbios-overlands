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
        app.add_systems(
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
pub struct AvatarFetchTask(pub bevy::tasks::Task<AvatarFetchResult>);

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

fn trigger_avatar_fetches(mut commands: Commands, pending: Query<(Entity, &AvatarFetchPending)>) {
    for (entity, pending) in pending.iter() {
        let did = pending.did.clone();
        commands.entity(entity).remove::<AvatarFetchPending>();
        spawn_avatar_task(&mut commands, entity, did);
    }
}

fn spawn_avatar_task(commands: &mut Commands, entity: Entity, did: String) {
    // Blocking ATProto profile fetches belong on `IoTaskPool`; mixing them
    // onto `AsyncComputeTaskPool` would pin its core-count-sized workers on
    // socket reads and stall GLTF/asset work for every other system.
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = fetch_avatar_bytes(did);
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
    commands.entity(entity).insert(AvatarFetchTask(task));
}

#[allow(clippy::too_many_arguments)]
fn poll_avatar_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut AvatarFetchTask)>,
    mut peers: Query<&mut RemotePeer>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    children_query: Query<&Children>,
    sail_query: Query<Entity, With<RoverSail>>,
    badge_query: Query<Entity, With<ChestBadge>>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };

        commands.entity(entity).remove::<AvatarFetchTask>();

        // Promote the profile-verified handle to the authoritative one on
        // the peer entity. The handle field on `OverlandsMessage::Identity`
        // is peer-supplied and cannot be trusted — a malicious peer could
        // claim any string they like to impersonate another user in the
        // chat HUD or disconnect log. Only a handle resolved from the
        // authenticated DID's profile record is safe to display.
        if let Some(handle) = result.handle.clone()
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
