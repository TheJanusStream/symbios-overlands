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
            (fetch_local_avatar, trigger_avatar_fetches, poll_avatar_tasks)
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
            unlit: true, // Better visibility for avatars
            ..default()
        });

        if let Ok(children) = sails.get(entity) {
            for child in children.iter() {
                // Correctly check if the child is a sail before injecting the material
                if sail_query.get(child).is_ok() {
                    commands
                        .entity(child)
                        .insert(MeshMaterial3d(new_mat.clone()));
                }
            }
        }
    }
}

#[derive(Deserialize)]
struct BskyProfile {
    avatar: Option<String>,
}

async fn fetch_avatar_bytes(did: String) -> Option<Vec<u8>> {
    // A proper User-Agent prevents silent blocks by the ATProto API
    let client = reqwest::Client::builder()
        .user_agent("SymbiosOverlands/1.0")
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
