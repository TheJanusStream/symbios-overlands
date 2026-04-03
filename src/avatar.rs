use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde::Deserialize;

use crate::rover::RoverSail;
use crate::state::{AppState, LocalPlayer};

pub struct AvatarPlugin;

impl Plugin for AvatarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                fetch_local_avatar,
                trigger_avatar_fetches,
                poll_avatar_tasks,
                reapply_avatar_after_rebuild,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component)]
pub struct AvatarFetchPending {
    pub did: String,
}

#[derive(Component)]
pub struct AvatarFetchTask(pub bevy::tasks::Task<Option<Vec<u8>>>);

/// Stores the last successfully applied avatar material on a chassis entity.
/// Used to re-apply the material to a new sail child after an airship rebuild
/// without triggering a redundant network fetch.
#[derive(Component, Clone)]
pub struct AvatarMaterial(pub Handle<StandardMaterial>);

/// Placed on a chassis entity by `rebuild_local_rover` (and equivalent network
/// rebuilds) to signal that the sail needs the cached `AvatarMaterial`
/// reapplied on the next frame, once the new children are live.
#[derive(Component)]
pub struct NeedsAvatarReapply;

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
    let pool = bevy::tasks::AsyncComputeTaskPool::get();
    let task = pool.spawn(async move {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(fetch_avatar_bytes(did))
    });
    commands.entity(entity).insert(AvatarFetchTask(task));
}

fn poll_avatar_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut AvatarFetchTask)>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    sails: Query<&Children>,
    sail_query: Query<Entity, With<RoverSail>>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };

        commands.entity(entity).remove::<AvatarFetchTask>();

        let Some(bytes) = result else { continue };

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
            ..default()
        });

        // Cache the material on the chassis so rebuilds can re-apply it without
        // a new network fetch.
        commands
            .entity(entity)
            .insert(AvatarMaterial(new_mat.clone()));

        // Apply the material to every RoverSail child.  Use a deferred world
        // closure so validity is checked at *application* time, not query time.
        // This prevents a panic when a rebuild despawns the sail between this
        // system running and the commands being flushed.
        if let Ok(children) = sails.get(entity) {
            let sail_entities: Vec<Entity> = children
                .iter()
                .filter(|&child| sail_query.get(child).is_ok())
                .collect();

            for sail in sail_entities {
                let mat = new_mat.clone();
                commands.queue(move |world: &mut World| {
                    if let Ok(mut eref) = world.get_entity_mut(sail) {
                        eref.insert(MeshMaterial3d(mat));
                    }
                });
            }
        }
    }
}

/// Runs every frame and re-applies the cached `AvatarMaterial` to any chassis
/// entity that was recently rebuilt (marked with `NeedsAvatarReapply`).
/// Running one frame after the rebuild ensures the new sail children are live.
fn reapply_avatar_after_rebuild(
    mut commands: Commands,
    query: Query<(Entity, &Children, &AvatarMaterial), With<NeedsAvatarReapply>>,
    sail_query: Query<Entity, With<RoverSail>>,
) {
    for (entity, children, avatar_mat) in query.iter() {
        commands.entity(entity).remove::<NeedsAvatarReapply>();

        let sail_entities: Vec<Entity> = children
            .iter()
            .filter(|&child| sail_query.get(child).is_ok())
            .collect();

        for sail in sail_entities {
            let mat = avatar_mat.0.clone();
            // Safe closure: sail may have been replaced by another rapid
            // rebuild; in that case we skip silently.
            commands.queue(move |world: &mut World| {
                if let Ok(mut eref) = world.get_entity_mut(sail) {
                    eref.insert(MeshMaterial3d(mat));
                }
            });
        }
    }
}

#[derive(Deserialize)]
struct BskyProfile {
    avatar: Option<String>,
}

async fn fetch_avatar_bytes(did: String) -> Option<Vec<u8>> {
    let client = reqwest::Client::builder()
        .user_agent(crate::config::avatar::USER_AGENT)
        .build()
        .ok()?;

    let url = format!(
        "https://public.api.bsky.app/xrpc/app.bsky.actor.getProfile?actor={}",
        did
    );

    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        bevy::log::warn!("Failed to fetch profile for {}: {}", did, resp.status());
        return None;
    }

    let profile = resp.json::<BskyProfile>().await.ok()?;
    let avatar_url = profile.avatar?;

    let img_resp = client.get(&avatar_url).send().await.ok()?;
    if !img_resp.status().is_success() {
        bevy::log::warn!(
            "Failed to fetch avatar image for {}: {}",
            did,
            img_resp.status()
        );
        return None;
    }

    let bytes = img_resp.bytes().await.ok()?;
    Some(bytes.to_vec())
}
