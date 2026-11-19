//! Avatar-side generator-tree spawner. Walks an
//! [`crate::pds::AvatarRecord`]'s `visuals: Generator` tree and spawns
//! one Bevy entity per node,
//! parented into the chassis hierarchy. Routes through
//! [`crate::world_builder::avatar_spawn::spawn_avatar_visuals_subtree`]
//! so the avatar dispatch arms reuse the same primitive / LSystem /
//! Shape machinery as room generators, with the room-only behaviours
//! (RoomEntity tag, PrimMarker, per-prim colliders) suppressed by
//! `SpawnCtx::avatar_mode = true`.
//!
//! Hot-swap discipline: callers despawn the previous chassis children
//! before re-running this spawner. The sub-tree spawner does not tag
//! children with `RoomEntity`, so the room compiler's cleanup query
//! cannot reach them - only the chassis's `with_children` despawn
//! does.

use bevy::prelude::*;
use bevy_symbios::materials::MaterialPalette;

use crate::pds::AvatarBody;
use crate::state::CurrentRoomDid;
use crate::terrain::{FinishedHeightMap, OutgoingTerrain, TerrainMesh};
use crate::water::{WaterMaterial, WaterSurfaces};
use crate::world_builder::avatar_spawn::spawn_avatar_visuals_subtree;
use crate::world_builder::compile::GeneratorCaches;
use crate::world_builder::image_cache::BlobImageCache;
use crate::world_builder::light::LightOwner;

/// Bundle of every shared `world_builder` resource the avatar spawn
/// path needs to reach. Bundled because Bevy 0.18 caps `IntoSystem`
/// at 16 parameters and the host systems already carry a handful of
/// `ResMut`s of their own - folding the world-builder fan-out into one
/// `SystemParam` keeps callers under the budget.
#[derive(bevy::ecs::system::SystemParam)]
pub struct AvatarSpawnDeps<'w, 's> {
    /// The parts' materials (#1686): an avatar's draw dry, an instance
    /// set's (which spawns through here too) under the weather.
    pub materials: ResMut<'w, Assets<crate::weathered::WeatheredMaterial>>,
    pub water_materials: ResMut<'w, Assets<WaterMaterial>>,
    pub palette: Option<Res<'w, MaterialPalette>>,
    pub heightmap: Option<Res<'w, FinishedHeightMap>>,
    pub terrain_meshes: Query<'w, 's, Entity, (With<TerrainMesh>, Without<OutgoingTerrain>)>,
    pub caches: GeneratorCaches<'w>,
    pub blob_image_cache: ResMut<'w, BlobImageCache>,
    pub blob_audio_cache: ResMut<'w, crate::world_builder::audio_resolver::BlobAudioCache>,
    pub water_surfaces: ResMut<'w, WaterSurfaces>,
    pub current_room: Option<Res<'w, CurrentRoomDid>>,
}

/// Spawn the record's body under `chassis`. A generator body walks its
/// `Generator` tree and spawns one entity per node; the visuals root's
/// transform composes with each node's local transform, and the chassis (a
/// parent rigid body) provides the world-space anchor.
///
/// A **rigged** body ([`AvatarBody::Rigged`]) spawns nothing here yet - its
/// skinned build is #1057's slice and slots in exactly at this seam - and
/// unknown/absent bodies are bare chassis by contract. Every variant still
/// clears the previous children, because a hot-swap from a generator body
/// to any other kind owes the despawn regardless of what replaces it.
///
/// `existing_children` is the chassis's current child list, despawned
/// before the new tree spawns to avoid double-instantiation on hot-swap.
/// Pass `None` on first spawn (no prior visuals to clear).
///
/// `owner` is the DID of whose body it is, where known: the body's lights
/// are seeded by it (#1924, [`LightOwner`]), so two players' craft built
/// alike flicker apart and every peer agrees on each.
#[allow(clippy::too_many_arguments)]
pub fn spawn_avatar_visuals(
    commands: &mut Commands,
    chassis: Entity,
    body: &AvatarBody,
    existing_children: Option<&Children>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    deps: &mut AvatarSpawnDeps,
    is_local: bool,
    owner: Option<&str>,
) {
    if let Some(children) = existing_children {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }
    let Some(visuals) = body.visuals() else {
        return;
    };
    let visuals = with_still_root(visuals);
    // A body's lights shine (#1924), the local player's and every peer's.
    spawn_visual_tree(
        commands,
        chassis,
        &visuals,
        meshes,
        materials,
        images,
        deps,
        is_local,
        Some(LightOwner::body(owner)),
    );
}

/// A body's visuals as they are spawned: the root never turns (#1604),
/// slides or swells (#1935). The gait layer writes the visuals root's pose
/// every frame, so a spin there - one a live edit carried past the
/// sanitiser, which takes it off a saved body - could only fight it. A node
/// below the root may move, and so may a worn prop's root, which no gait
/// moves. Borrowed unless the root does carry one, so the editor's
/// per-frame rebuild clones nothing.
fn with_still_root(visuals: &crate::pds::Generator) -> std::borrow::Cow<'_, crate::pds::Generator> {
    if visuals.spin.is_none() && visuals.slide.is_none() && visuals.swell.is_none() {
        return std::borrow::Cow::Borrowed(visuals);
    }
    let mut still = visuals.clone();
    still.spin = None;
    still.slide = None;
    still.swell = None;
    std::borrow::Cow::Owned(still)
}

/// The generator-tree walk itself, body-kind agnostic. Split from
/// [`spawn_avatar_visuals`] because the render tool feeds bare `Generator`
/// trees (catalogue items, avatar parts) through the avatar spawn path
/// without any record around them.
///
/// `lights`: whether the tree's lit nodes spawn their lights (#1924), and
/// whose they are ([`LightOwner`], which seeds them) - a body's do, and so
/// does a tree the render tool stands in the world, but an item preview's
/// never (`None`): Bevy lights the world camera's view with every
/// light it does not keep to another layer, and a light does not take its
/// layer from its node, so a preview's lamp would light the ground round
/// the player as well as its picture.
#[allow(clippy::too_many_arguments)]
pub fn spawn_visual_tree(
    commands: &mut Commands,
    chassis: Entity,
    visuals: &crate::pds::Generator,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    deps: &mut AvatarSpawnDeps,
    is_local: bool,
    lights: Option<LightOwner<'_>>,
) {
    spawn_attachment_tree(
        commands, chassis, visuals, meshes, materials, images, deps, is_local, None, lights,
    );
}

/// [`spawn_visual_tree`] for a worn prop (#1098): `attachment` names the
/// attachment record the tree belongs to, and every spawned node then
/// carries an [`AttachmentPrim`](crate::world_builder::AttachmentPrim)
/// marker so the part can be tree-selected, scene-picked and gizmo-dragged
/// like a region asset's. Pass `None` for a peer's outfit - their records
/// are not editable here - which makes this exactly [`spawn_visual_tree`].
/// `lights` as [`spawn_visual_tree`] takes it: a worn prop's lamp shines,
/// its owner naming the prop (`LightOwner::prop`) for a peer's outfit too.
#[allow(clippy::too_many_arguments)]
pub fn spawn_attachment_tree(
    commands: &mut Commands,
    chassis: Entity,
    visuals: &crate::pds::Generator,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    deps: &mut AvatarSpawnDeps,
    is_local: bool,
    attachment: Option<&str>,
    lights: Option<LightOwner<'_>>,
) {
    // The avatar spawner's `record` parameter is unused on every reachable
    // dispatch arm, so it is the shared sentinel (#638).
    let empty_record = crate::world_builder::avatar_spawn::detached_record();

    spawn_avatar_visuals_subtree(
        commands,
        chassis,
        visuals,
        meshes,
        materials,
        deps.materials.as_mut(),
        deps.water_materials.as_mut(),
        images,
        deps.palette.as_deref(),
        deps.heightmap.as_deref(),
        &deps.terrain_meshes,
        &mut deps.caches,
        deps.blob_image_cache.as_mut(),
        deps.blob_audio_cache.as_mut(),
        deps.water_surfaces.as_mut(),
        empty_record,
        deps.current_room.as_deref(),
        is_local,
        attachment,
        lights,
    );
}

// --- The rigged half (#1482) ----------------------------------------------
//
// A rigged body is not spawned by this module - `player::rigged` builds it
// off the main thread and `player::attachments` dresses it - but the three
// questions it is built and hung by live here, beside the generator walk,
// because this module is the avatar spawn path the render tool reaches: its
// `--rigged` sheet and `--walker-avatar` walker ask them exactly as
// `rigged::build::kick_rigged_builds` does, so the body a sheet shows is the
// body the game stands under a chassis. The item preview's stage (#1842) asks
// them too, and stands its picture in the same still pose a sheet does.

/// The skin atlas a rigged body is built at once its record has settled:
/// the engine's full size, the rung above the editor's draft
/// (`rigged`'s settle ladder, #1059).
pub(crate) fn settled_atlas() -> u32 {
    symbios_avatar::AvatarConfig::default().atlas
}

/// The skin atlas a rigged body is built at while its record is still moving:
/// the rung below [`settled_atlas`] (`rigged`'s settle ladder, #1059). The item
/// preview builds its picture of a person or a beast at it too (#1842): the
/// picture is at most 180 points of a 256-pixel target, so a body fills some
/// 200 pixels of it and its face a few dozen, and a 256 atlas already gives
/// that face more texels than it covers pixels - the full 1,024 would cost a
/// quarter more build and sixteen times the texture memory for a picture that
/// cannot show the difference.
pub(crate) fn draft_atlas() -> u32 {
    super::rigged::DRAFT_ATLAS
}

/// The job that builds a rigged body's engine record at `atlas`.
///
/// With the far hair tier. Every body the game installs is drawn by the
/// chase camera, which orbits from 2 m to 200 m (`cfg::camera`), so every
/// one of them crosses the hair switch - the owner's own included, whose
/// resting 12 m orbit already sits 12.38 m from its root. Asking for the far
/// tier is therefore not a peer-only optimisation: it is what keeps a
/// zoomed-out owner and the peers beside them drawing the same thing (#1358).
///
/// **On the web, with the head measured where the body is built** (#1815): the
/// worker keeps the closed surface a fitted circlet is sized from, so it
/// measures a person's head before the body leaves and the answer lands
/// beside it ([`super::attachments::CarriedHatLine`]). A native build holds
/// the whole body and is measured when something worn asks, as it always was.
pub(crate) fn rigged_build_job(
    body: &crate::pds::avatar::EngineAvatarRecord,
    atlas: u32,
) -> crate::offload::GenJob {
    crate::offload::GenJob::AvatarBuild {
        record: Box::new(body.clone()),
        atlas,
        far_hair: true,
        measure_head: cfg!(target_arch = "wasm32"),
    }
}

/// How far below its chassis's centre a rigged body's ground plane (its
/// feet, `y = 0` in the engine) hangs: half the collider the record's
/// locomotion gives it, so the feet meet the capsule's bottom.
pub(crate) fn rigged_root_offset(record: &crate::pds::AvatarRecord) -> f32 {
    crate::interaction::locomotion::locomotion_total_height(&record.locomotion) / 2.0
}

/// Where the skinned body hangs relative to its chassis (#1066).
///
/// Two corrections, both of them convention mismatches rather than tuning:
///
/// * **Height** - the engine's ground plane is `y = 0`, so the body drops by
///   half the collider so its feet meet the chassis capsule's bottom.
/// * **Facing** - a half turn about Y. `symbios_avatar::rig::landmark::FORWARD`
///   is `+Z`, the glTF/VRM convention the engine shares; Bevy's forward is
///   `-Z`, and the chassis is steered by
///   `Transform::looking_to(movement_direction, Y)`, which aims *its* `-Z`
///   down the direction of travel. Hanging the body off that with no rotation
///   pointed the engine's `+Z` face directly away from where the avatar was
///   going - walking correctly, moonwalking visibly. The half turn is applied
///   here, on the one entity that bridges the two conventions, rather than by
///   re-aiming the chassis (which the camera, the vehicles and the locomotion
///   drive all share) or by rotating the clips (which are authored in the
///   engine's frame and are consistent with the body).
///
/// Everything below this entity inherits the turn together - geometry, rig,
/// clips, and the socket anchors that
/// [`crate::player::attachments::LocalAttachment::rest_frame`] reconstructs an
/// offset against - so worn props stay put relative to the body they are on.
pub(crate) fn rigged_root_transform(offset: f32) -> Transform {
    Transform::from_xyz(0.0, -offset, 0.0)
        .with_rotation(Quat::from_rotation_y(std::f32::consts::PI))
}

/// The pose a body the game shows standing still is built on: the rest pose
/// with the arms hung at its sides, the engine's own carriage
/// ([`symbios_avatar::anim::gait::hang_arms`]) and the first thing the
/// game's idle does every frame. The idle's breath, sway, weight shift and
/// fidgets move over it with time, and are left out where a still picture is
/// wanted: the render tool's `--rigged` sheet, so two sheets of one record are
/// the same picture, and the item preview's stage (#1842). (The bind pose is
/// the A-pose a body is modelled in, arms 50 degrees out; nothing standing in
/// the game shows it. A beast's fore legs carry it, so they are left alone.)
pub(crate) fn standing_pose(rig: &symbios_avatar::Rig) -> symbios_avatar::Pose {
    let mut pose = symbios_avatar::Pose::rest(rig);
    symbios_avatar::anim::gait::hang_arms(rig, &mut pose);
    pose
}

/// The box `avatar` fills in its [`standing_pose`], in its own space: every
/// drawn mesh posed as the engine poses it (the far hair tier is not drawn
/// beside the near one, so it is not here either).
///
/// What frames a standing body, because Bevy bounds a skinned mesh by its
/// bind pose - the A-pose, arms out - so the drawn meshes' own boxes say a
/// person is half again as wide as they stand.
pub(crate) fn standing_box(avatar: &symbios_avatar::Avatar) -> (Vec3, Vec3) {
    let pose = standing_pose(&avatar.rig);
    avatar
        .posed(&pose, 0.0)
        .iter()
        .flat_map(|drawn| drawn.mesh.positions.iter().copied())
        .fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(min, max), p| (min.min(p), max.max(p)),
        )
}

/// An app holding every asset store and cache [`AvatarSpawnDeps`] fans out
/// to, and nothing else - the stage a test needs to run the real spawn path.
///
/// Shared by `player::attachments`' dressed-body tests and
/// `ui::catalogue`'s #1301 preview-work count, so the list of what the spawn
/// path reaches lives beside the bundle that reaches it. A new field on
/// [`AvatarSpawnDeps`] fails both at once, and is fixed once, here.
#[cfg(test)]
pub(crate) fn spawn_path_app() -> bevy::app::App {
    let mut app = bevy::app::App::new();
    app.add_plugins((
        bevy::app::TaskPoolPlugin::default(),
        bevy::asset::AssetPlugin::default(),
    ));
    add_spawn_path(&mut app);
    app
}

/// A signed-in account of `did` (#1924): the spawn path reads only its
/// DID, which seeds a body's lights; the rest is the live signing session
/// the type insists on (the fixture `prefs`' tests use).
#[cfg(test)]
pub(crate) fn signed_in(did: &str) -> bevy_symbios_multiuser::auth::AtprotoSession {
    use std::sync::Arc;
    bevy_symbios_multiuser::auth::AtprotoSession {
        did: String::from(did),
        handle: String::from("someone"),
        pds_url: String::from("https://pds.example"),
        session: Arc::new(proto_blue_oauth::session::OAuthSession::with_fetch_handler(
            proto_blue_oauth::types::TokenSet {
                issuer: String::from("https://as.example"),
                sub: String::from(did),
                scope: String::from("atproto"),
                access_token: String::from("access"),
                refresh_token: Some(String::from("refresh")),
                token_type: String::from("DPoP"),
                expires_at: Some(String::from("2099-01-01T00:00:00Z")),
                aud: Some(String::from("https://as.example")),
            },
            proto_blue_oauth::DpopKey::generate().expect("DPoP key"),
            proto_blue_oauth::DpopNonceCache::new(),
            Arc::new(crate::oauth::capped_fetch::CappedFetcher::new()),
        )),
    }
}

/// What the spawn path reaches, added to an `app` that already has its asset
/// plugin - [`spawn_path_app`]'s list, for an app standing on other plugins
/// (the re-roll bench in `player::reroll_tests`, which runs avian too).
#[cfg(test)]
pub(crate) fn add_spawn_path(app: &mut bevy::app::App) {
    app.init_asset::<Mesh>();
    app.init_asset::<StandardMaterial>();
    app.init_asset::<crate::weathered::WeatheredMaterial>();
    app.init_asset::<Image>();
    app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>();
    app.init_asset::<WaterMaterial>();
    app.init_resource::<BlobImageCache>();
    app.init_resource::<crate::world_builder::audio_resolver::BlobAudioCache>();
    app.init_resource::<WaterSurfaces>();
    app.init_resource::<crate::world_builder::lsystem::LSystemMaterialCache>();
    app.init_resource::<crate::world_builder::lsystem::LSystemMeshCache>();
    app.init_resource::<crate::world_builder::ShapeMaterialCache>();
    app.init_resource::<crate::world_builder::ShapeMeshCache>();
    app.init_resource::<crate::world_builder::prim_cache::PrimMeshCache>();
    app.init_resource::<crate::world_builder::prim_cache::PrimMaterialCache>();
    app.init_resource::<bevy_symbios_shape::cache::ShapeMeshCache>();
    app.init_resource::<crate::world_builder::spatial_audio::BakedAudioCache>();
    app.insert_resource(crate::world_builder::fresh_texture_cache());
    // The compile bookkeeping `GeneratorCaches` also borrows.
    app.init_resource::<crate::world_builder::compile::CompiledWorld>();
    app.init_resource::<crate::world_builder::compile::CompileJob>();
    app.init_resource::<crate::diagnostics::SessionLog>();
    app.init_resource::<Time>();
}

/// #1924: a body's lights and a worn prop's are drawn in the world, the
/// local player's and a peer's alike, through the real spawn path and the
/// real drawing.
#[cfg(test)]
mod light_tests {
    use super::*;
    use crate::pds::avatar::AvatarBody;
    use crate::pds::{Generator, LightSource};
    use crate::world_builder::light::{NodeLight, follow_node_lights};
    use bevy::camera::visibility::RenderLayers;
    use bevy::ecs::system::RunSystemOnce;

    /// A hull with one lamp on it.
    fn lit_tree() -> Generator {
        let mut tree = Generator::default_cuboid();
        let mut lamp = Generator::default_cuboid();
        lamp.light = Some(LightSource::default());
        tree.children.push(lamp);
        tree
    }

    /// Every entity under `entity`, itself included.
    fn tree_of(world: &World, entity: Entity) -> Vec<Entity> {
        let mut out = vec![entity];
        let mut at = 0;
        while at < out.len() {
            if let Some(children) = world.get::<Children>(out[at]) {
                out.extend(children.iter());
            }
            at += 1;
        }
        out
    }

    /// By night the local player's body, a peer's body and a worn prop
    /// each spawn their lamp's light, and the drawing fades all three in
    /// and lights them - each kept off any layer its body is moved to, so
    /// a body the first-person view hides still lights the road.
    #[test]
    fn a_bodys_and_a_worn_props_lights_shine_by_night() {
        let tree = lit_tree();
        let body = AvatarBody::generator(tree.clone());
        let mut app = spawn_path_app();
        app.add_plugins(bevy::time::TimePlugin)
            .insert_resource(crate::weathered::WorldWeather::default())
            .add_systems(Update, follow_node_lights);
        app.world_mut()
            .resource_mut::<crate::weathered::WorldWeather>()
            .uniforms
            .lights = 1.0;
        app.world_mut().spawn((
            Camera::default(),
            Projection::Perspective(PerspectiveProjection::default()),
            crate::camera::WorldCamera,
            GlobalTransform::from(
                Transform::from_xyz(0.0, 2.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
            ),
        ));
        let local = app.world_mut().spawn(Transform::default()).id();
        let peer = app.world_mut().spawn(Transform::default()).id();
        let socket = app.world_mut().spawn(Transform::default()).id();
        app.world_mut()
            .run_system_once(
                move |mut commands: Commands,
                      mut meshes: ResMut<Assets<Mesh>>,
                      mut materials: ResMut<Assets<StandardMaterial>>,
                      mut images: ResMut<Assets<Image>>,
                      mut deps: AvatarSpawnDeps| {
                    for (chassis, is_local) in [(local, true), (peer, false)] {
                        spawn_avatar_visuals(
                            &mut commands,
                            chassis,
                            &body,
                            None,
                            &mut meshes,
                            &mut materials,
                            &mut images,
                            &mut deps,
                            is_local,
                            None,
                        );
                    }
                    spawn_attachment_tree(
                        &mut commands,
                        socket,
                        &tree,
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        &mut deps,
                        false,
                        None,
                        Some(LightOwner::default()),
                    );
                },
            )
            .expect("spawns");
        // What Bevy's visibility propagation gives every node of a body on
        // show (this app runs none).
        let world = app.world_mut();
        for root in [local, peer, socket] {
            for entity in tree_of(world, root) {
                if !world.entity(entity).contains::<NodeLight>() {
                    world
                        .entity_mut(entity)
                        .insert(InheritedVisibility::VISIBLE);
                }
            }
        }
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(100),
        ));
        for _ in 0..12 {
            app.update();
        }
        let world = app.world();
        for (root, what) in [
            (local, "the local body"),
            (peer, "a peer's body"),
            (socket, "a worn prop"),
        ] {
            let lights: Vec<Entity> = tree_of(world, root)
                .into_iter()
                .filter(|&e| world.entity(e).contains::<NodeLight>())
                .collect();
            assert_eq!(lights.len(), 1, "{what}: its lamp's light");
            let light = world.entity(lights[0]);
            let state = light.get::<NodeLight>().expect("a light");
            assert!(
                state.held && state.level == 1.0,
                "{what}: not drawn, {state:?}"
            );
            assert!(
                light.get::<PointLight>().is_some_and(|p| p.intensity > 0.0),
                "{what}: dark"
            );
            assert_eq!(
                light.get::<Visibility>(),
                Some(&Visibility::Inherited),
                "{what}"
            );
            assert!(
                light.contains::<bevy::app::PropagateOver<RenderLayers>>(),
                "{what}: its light would follow its body onto a hidden layer"
            );
        }
    }

    /// #1924: a body's light and a worn prop's at the same place in one
    /// tree are seeded apart, and so are the lights of two props an owner
    /// wears, through the real spawn path; a body of no known owner is
    /// seeded by its tree alone, as every body was before.
    #[test]
    fn a_bodys_and_a_worn_props_lights_at_one_path_are_seeded_apart() {
        const ALICE: &str = "did:plc:alice";
        let tree = lit_tree();
        let body = AvatarBody::generator(tree.clone());
        let mut app = spawn_path_app();
        let roots: Vec<Entity> = (0..4)
            .map(|_| app.world_mut().spawn(Transform::default()).id())
            .collect();
        let at = roots.clone();
        app.world_mut()
            .run_system_once(
                move |mut commands: Commands,
                      mut meshes: ResMut<Assets<Mesh>>,
                      mut materials: ResMut<Assets<StandardMaterial>>,
                      mut images: ResMut<Assets<Image>>,
                      mut deps: AvatarSpawnDeps| {
                    for (chassis, owner) in [(at[0], Some(ALICE)), (at[1], None)] {
                        spawn_avatar_visuals(
                            &mut commands,
                            chassis,
                            &body,
                            None,
                            &mut meshes,
                            &mut materials,
                            &mut images,
                            &mut deps,
                            false,
                            owner,
                        );
                    }
                    for (socket, prop) in [(at[2], "3lantern"), (at[3], "3torch")] {
                        spawn_attachment_tree(
                            &mut commands,
                            socket,
                            &tree,
                            &mut meshes,
                            &mut materials,
                            &mut images,
                            &mut deps,
                            false,
                            None,
                            Some(LightOwner {
                                did: Some(ALICE),
                                prop: Some(prop),
                            }),
                        );
                    }
                },
            )
            .expect("spawns");
        let world = app.world();
        let seed = |root: Entity| {
            let seeds: Vec<u64> = tree_of(world, root)
                .into_iter()
                .filter_map(|e| world.get::<NodeLight>(e).map(|l| l.seed))
                .collect();
            assert_eq!(seeds.len(), 1, "its lamp's light");
            seeds[0]
        };
        let [body, guest, lantern, torch] = [roots[0], roots[1], roots[2], roots[3]].map(seed);
        assert_ne!(
            body, lantern,
            "a body's and a worn prop's light at one path"
        );
        assert_ne!(lantern, torch, "two worn props' lights at one path");
        assert_eq!(
            guest,
            crate::world_builder::light::light_seed("avatar", &[0], 0.0, 0),
            "with no owner, the tree's own seed"
        );
        assert_ne!(body, guest, "an owner's body and no one's");
    }
}

#[cfg(test)]
mod spin_tests {
    use super::*;
    use crate::pds::avatar::AvatarBody;
    use crate::pds::{Fp, Generator, Spin, SpinTerm};
    use crate::world_builder::spin::Spinner;
    use bevy::ecs::system::RunSystemOnce;

    /// #1604: a body's visuals root never turns - the gait layer writes its
    /// pose every frame - while a part below it does, and so does a worn
    /// prop's root, which no gait moves. Through the real spawn path, since
    /// the two meet in one spawner that cannot tell them apart.
    #[test]
    fn a_bodys_root_stands_still_and_a_worn_props_root_turns() {
        let turning = || {
            Some(Spin::about(
                [0.0, 1.0, 0.0],
                SpinTerm::Constant { rate: Fp(30.0) },
            ))
        };
        let mut tree = Generator::default_cuboid();
        tree.spin = turning();
        let mut wheel = Generator::default_cuboid();
        wheel.spin = turning();
        tree.children.push(wheel);
        let body = AvatarBody::generator(tree.clone());

        let mut app = spawn_path_app();
        let chassis = app.world_mut().spawn(Transform::default()).id();
        let socket = app.world_mut().spawn(Transform::default()).id();
        app.world_mut()
            .run_system_once(
                move |mut commands: Commands,
                      mut meshes: ResMut<Assets<Mesh>>,
                      mut materials: ResMut<Assets<StandardMaterial>>,
                      mut images: ResMut<Assets<Image>>,
                      mut deps: AvatarSpawnDeps| {
                    spawn_avatar_visuals(
                        &mut commands,
                        chassis,
                        &body,
                        None,
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        &mut deps,
                        false,
                        None,
                    );
                    spawn_attachment_tree(
                        &mut commands,
                        socket,
                        &tree,
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        &mut deps,
                        false,
                        None,
                        Some(LightOwner::default()),
                    );
                },
            )
            .expect("spawns");
        let world = app.world_mut();
        let root_of = |world: &mut World, parent: Entity| {
            let children = world.get::<Children>(parent).expect("a root hangs here");
            children[0]
        };
        let body_root = root_of(world, chassis);
        let prop_root = root_of(world, socket);
        assert!(
            world.get::<Spinner>(body_root).is_none(),
            "the gait moves it"
        );
        let wheel = root_of(world, body_root);
        assert!(world.get::<Spinner>(wheel).is_some(), "a body's part turns");
        assert!(
            world.get::<Spinner>(prop_root).is_some(),
            "a prop's root turns"
        );
    }

    /// #1686: an avatar and what it wears draw dry - their parts are the
    /// world's material type, so they share every spawn path and cache, but
    /// none of them weathers.
    #[test]
    fn an_avatar_and_what_it_wears_take_no_weather() {
        let tree = Generator::default_cuboid();
        let body = AvatarBody::generator(tree.clone());
        let mut app = spawn_path_app();
        let chassis = app.world_mut().spawn(Transform::default()).id();
        let socket = app.world_mut().spawn(Transform::default()).id();
        app.world_mut()
            .run_system_once(
                move |mut commands: Commands,
                      mut meshes: ResMut<Assets<Mesh>>,
                      mut materials: ResMut<Assets<StandardMaterial>>,
                      mut images: ResMut<Assets<Image>>,
                      mut deps: AvatarSpawnDeps| {
                    spawn_avatar_visuals(
                        &mut commands,
                        chassis,
                        &body,
                        None,
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        &mut deps,
                        false,
                        None,
                    );
                    spawn_attachment_tree(
                        &mut commands,
                        socket,
                        &tree,
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        &mut deps,
                        false,
                        None,
                        Some(LightOwner::default()),
                    );
                },
            )
            .expect("spawns");
        let world = app.world_mut();
        let mut parts = world.query::<&MeshMaterial3d<crate::weathered::WeatheredMaterial>>();
        let handles: Vec<_> = parts.iter(world).map(|m| m.0.clone()).collect();
        assert_eq!(handles.len(), 2, "a body's part and a worn prop's");
        let materials = world.resource::<Assets<crate::weathered::WeatheredMaterial>>();
        for handle in &handles {
            assert!(!materials.get(handle).expect("live").extension.weathers);
        }
    }
}

#[cfg(test)]
mod water_tests {
    use super::*;
    use crate::pds::{Fp3, Generator, GeneratorKind};
    use bevy::ecs::system::RunSystemOnce;

    /// #1949: a tree spawned with no world frame - the item preview's,
    /// through this very path - draws the water it holds and registers none:
    /// a plane registered in its parent's frame is phantom water round the
    /// world's origin, one more at every restage.
    #[test]
    fn a_detached_tree_draws_its_water_and_registers_none() {
        let mut basin = Generator::default_cuboid();
        let mut water = Generator::from_kind(GeneratorKind::Water {
            surface: Default::default(),
        });
        water.transform.translation = Fp3([0.0, 0.885, 0.0]);
        water.transform.scale = Fp3([0.01, 1.0, 0.01]);
        basin.children.push(water);
        let mut app = spawn_path_app();
        let stage = app.world_mut().spawn(Transform::default()).id();
        for _ in 0..2 {
            let basin = basin.clone();
            app.world_mut()
                .run_system_once(
                    move |mut commands: Commands,
                          mut meshes: ResMut<Assets<Mesh>>,
                          mut materials: ResMut<Assets<StandardMaterial>>,
                          mut images: ResMut<Assets<Image>>,
                          mut deps: AvatarSpawnDeps| {
                        spawn_visual_tree(
                            &mut commands,
                            stage,
                            &basin,
                            &mut meshes,
                            &mut materials,
                            &mut images,
                            &mut deps,
                            false,
                            None,
                        );
                    },
                )
                .expect("spawns");
        }
        let world = app.world_mut();
        let drawn = world
            .query_filtered::<(), With<crate::terrain::WaterVolume>>()
            .iter(world)
            .count();
        assert_eq!(drawn, 2, "its water is drawn, once a spawn");
        let indexed = world
            .query::<&crate::water::WaterPlaneIndex>()
            .iter(world)
            .count();
        assert_eq!(indexed, 0, "and routes no wake to a plane");
        assert!(
            world.resource::<WaterSurfaces>().planes.is_empty(),
            "and no water is registered"
        );
    }
}
