//! The detail patch in the world (P4.2, #1597): following the body, fetched
//! and decoded as a core is, landed, and let go.
//!
//! [`follow_body`] asks [`super::want`] each frame where the body is, and
//! starts fetching the patch it asks for; one patch is on its way at a
//! time, and it lands before the next is asked for, so a body driving on
//! is never left waiting on patches it has already passed. [`drive_patch`]
//! drives the fetch, decodes the answers on the compute pool - the ground,
//! its mesh and collider, the far field's colliders with the patch's hole
//! cut, and the street level's plans - and lands the result in one frame:
//!
//! - the far field's slot holds the patch, so every reader of the ground as
//!   drawn reads it ([`super::super::far::FarField::drawn_height_at`]);
//! - the patch stands under the terrain's root, on its own heightfield and
//!   its streets' colliders, marked as ground for the rays that ask for it,
//!   drawn with the core's splat layers and its own weight map, its streets
//!   in the theme's road look;
//! - the far field's colliders are swapped for ones with the patch's hole
//!   cut, and its material cuts the same hole in the shader;
//! - the patch's buildings, trees and street furniture join the derived
//!   stage's plans, and the ring's lots under the patch are covered;
//! - [`PatchStamp`] names the patch, so the compile sets down again what of
//!   the record stands on it, or stood on the last one.
//!
//! The patch before goes in the same frame. Letting a patch go undoes all
//! of it. A new terrain forgets the patch of the one before, which went
//! with it.

use std::collections::HashSet;
use std::sync::Arc;

use avian3d::prelude::Collider;
use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::math::Rect;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use bevy_symbios_ground::{HeightMap, HeightMapMeshBuilder, NormalMethod};

use super::{Lattice, Pace, PatchGround, PatchInputs, PatchPlan, Want, decode_patch, want};
use crate::geodata::GeoFetcher;
use crate::splat::SplatTerrainMaterial;
use crate::state::{CurrentRoomDid, LiveRoomRecord, LocalPlayer};
use crate::terrain::derived::core::{KeptPlaces, draw_core, kept_places_for};
use crate::terrain::derived::fit::RoomScene;
use crate::terrain::derived::plan::Plan;
use crate::terrain::derived::{DerivedBuilds, DerivedItem, DerivedRoot, edit};
use crate::terrain::geo::far::{FarCollider, FarField, FarGround};
use crate::terrain::geo::{CoreRequests, core_bbox, core_grid, take_walkable};
use crate::terrain::{FinishedHeightMap, OutgoingTerrain, SplatMaterialHandle, TerrainMesh};

/// How far past a patch's edge a ring lot still stands under it (m): about
/// half a lot, so no ring building straddles the patch's edge into its
/// street level.
const COVER_MARGIN_M: f32 = 15.0;

/// What the compile reads of the detail patch: where it stands and which it
/// is, so a placement on it - or on the patch before - is set down again
/// when it changes. Changed only when a patch lands or goes.
#[derive(Resource, Default, Clone, Debug, PartialEq)]
pub(crate) struct PatchStamp(pub Option<(Rect, u64)>);

/// The detail patch round the body: the one standing, the one on its way,
/// and the terrain both belong to.
#[derive(Resource, Default)]
pub(crate) struct RoamingPatch {
    /// The terrain root the patch belongs to.
    terrain: Option<Entity>,
    standing: Option<Standing>,
    job: Option<PatchJob>,
    /// The plan last refused - its terrain or land use could not be had -
    /// not asked for again until the body has asked for another or gone
    /// deep into the core: a body standing still does not fetch a failing
    /// patch every frame, and one that comes back later tries it again.
    refused: Option<PatchPlan>,
    /// Whether the standing patch is to go: the body is deep in the core.
    letting_go: bool,
    /// How many patches have landed: the stamp's name for each.
    landed: u64,
    /// The far field's own colliders, uncut, kept from the first landing:
    /// what letting the patch go puts back, rather than build them again.
    whole_far: Option<Vec<Collider>>,
}

/// A patch standing in the world.
struct Standing {
    plan: PatchPlan,
    /// Where the body asked for it.
    asked: Vec2,
    /// Its root, under the terrain's.
    root: Entity,
}

/// A patch on its way.
enum PatchJob {
    /// Its answers being fetched; the places the record keeps, read as the
    /// fetch began.
    Fetching {
        plan: PatchPlan,
        asked: Vec2,
        requests: Box<CoreRequests>,
        places: KeptPlaces,
    },
    /// Its answers being decoded on the compute pool, paced by `pace`.
    Decoding {
        plan: PatchPlan,
        asked: Vec2,
        task: Task<Result<PatchBuild, String>>,
        pace: Pace,
    },
}

impl PatchJob {
    /// The patch on its way, and where it was asked for.
    fn asked(&self) -> (PatchPlan, Vec2) {
        match self {
            PatchJob::Fetching { plan, asked, .. } | PatchJob::Decoding { plan, asked, .. } => {
                (*plan, *asked)
            }
        }
    }
}

impl RoamingPatch {
    /// Whether no patch is on its way, nor one to go: what the render
    /// tool's readiness waits for (`--patch-at`).
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn is_settled(&self) -> bool {
        self.job.is_none() && !self.letting_go
    }

    /// Forget the patch on its way, its fetches with it (a decode running
    /// is dropped with its task).
    fn forget_job(&mut self, fetcher: Option<&mut GeoFetcher>) {
        if let (Some(PatchJob::Fetching { requests, .. }), Some(fetcher)) =
            (self.job.take(), fetcher)
        {
            for id in requests.ids() {
                fetcher.forget(id);
            }
        }
    }

    /// Forget the patch on its way and the one standing - which goes with
    /// its terrain - starting afresh on `terrain`.
    pub(crate) fn abandon(&mut self, fetcher: Option<&mut GeoFetcher>, terrain: Option<Entity>) {
        self.forget_job(fetcher);
        *self = RoamingPatch {
            terrain,
            landed: self.landed,
            ..Default::default()
        };
    }
}

/// A decoded patch, ready to land: built on the compute pool.
pub(crate) struct PatchBuild {
    ground: Arc<PatchGround>,
    mesh: Mesh,
    collider: Collider,
    /// The far field's colliders with the patch's hole cut.
    far_colliders: Vec<Collider>,
    /// Its splat weight map's side and bytes, one texel a point.
    weights: (u32, Vec<u8>),
    /// Its streets' surfaces, built: added to the stores on landing.
    streets: Vec<crate::terrain::roads::BuiltSurface>,
    plans: Vec<Plan>,
    /// Why each part that could not be had was not.
    lost: Vec<String>,
    /// How long it took to build (s).
    took_s: f64,
}

/// Decode a patch's answers and build all that lands with it (see the
/// module docs): on the compute pool, paced between its stages ([`Pace`]).
async fn build_patch(
    bodies: crate::terrain::geo::CoreBodies,
    inputs: PatchInputs,
    places: KeptPlaces,
    room: RoomScene,
    pace: Pace,
) -> Result<PatchBuild, String> {
    let started = bevy::platform::time::Instant::now();
    let decoded = decode_patch(&bodies, &inputs, &pace).await?;
    drop(bodies);
    pace.next().await;
    let ground = Arc::new(decoded.ground);
    let mesh = patch_mesh(&ground, &inputs.core);
    let collider = bevy_symbios_ground::build_heightfield_collider(ground.heights());
    let far_colliders = inputs.far.colliders(&inputs.core, Some(ground.rect()));
    // Without its hole cut the far field could not stand round the patch:
    // no patch, rather than ground no body can walk.
    if far_colliders.is_empty() {
        return Err("the far field's colliders could not be cut for the patch".to_owned());
    }
    let weight_map = ground.weight_map();
    let weights = (
        weight_map.width as u32,
        weight_map.data.iter().flatten().copied().collect(),
    );
    pace.next().await;
    let plans = decoded.street_level.map_or_else(Vec::new, |level| {
        let kept = places.for_level(Some(&level));
        let on = ground.clone();
        let mut plans = draw_core(&level, &room, &kept, &move |x, z| on.height_at(x, z));
        for plan in &mut plans {
            plan.label = match plan.label {
                "core buildings" => "patch buildings",
                "core trees" => "patch trees",
                "core furniture" => "patch furniture",
                other => other,
            };
        }
        plans
    });
    pace.next().await;
    let streets = decoded
        .streets
        .as_ref()
        .map(crate::terrain::roads::build_road_surfaces)
        .unwrap_or_default();
    Ok(PatchBuild {
        ground,
        mesh,
        collider,
        far_colliders,
        weights,
        streets,
        plans,
        lost: decoded.lost,
        took_s: started.elapsed().as_secs_f64(),
    })
}

/// The patch's mesh, as the core's is built ([`crate::terrain`]'s
/// `build_terrain_mesh`): heightfield triangles, area-weighted normals,
/// tangents, no CPU copy - its UVs the core's mapping run on past the
/// core's edges, as the far field's are, so the layers tile across both
/// seams, and on the boundary it shares with the core the core's own
/// normals, so the light runs on across it.
pub(crate) fn patch_mesh(ground: &PatchGround, core: &HeightMap) -> Mesh {
    let lattice = Lattice::of(core);
    let core_m = lattice.cells as f32 * lattice.cell;
    let mut mesh = HeightMapMeshBuilder::new()
        .with_normal_method(NormalMethod::AreaWeighted)
        .with_uv_tile_size(core_m)
        .build(ground.heights());
    let shift = (ground.rect().min + Vec2::splat(lattice.half())) / core_m;
    if let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
        for uv in uvs {
            uv[0] += shift.x;
            uv[1] += shift.y;
        }
    }
    let plan = ground.plan();
    if let Some((edge, x_line, (from, to))) = plan.shared_edge(lattice)
        && let Some(VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
    {
        let grid = plan.grid() as usize;
        for along in from..=to {
            let (cx, cz) = if x_line { (edge, along) } else { (along, edge) };
            let (i, j) = ((cx - plan.x0) as usize, (cz - plan.z0) as usize);
            normals[j * grid + i] =
                crate::terrain::geo::far::core_normal(core, cx as usize, cz as usize).into();
        }
    }
    let tangents = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(normals)) => grid_tangents(ground.heights(), normals),
        _ => Vec::new(),
    };
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
    mesh.asset_usage = RenderAssetUsages::RENDER_WORLD;
    mesh
}

/// Tangents for a heightfield mesh whose UVs run with world X and Z, as
/// `HeightMapMeshBuilder` lays them: at each point the surface's direction
/// along +X - its slope there by central differences - made square to the
/// point's `normals`. What Bevy's mikktspace
/// generation gives such a mesh, to within a hair, in a few milliseconds
/// where mikktspace takes a second for a patch's 180,000 points.
pub(crate) fn grid_tangents(heights: &HeightMap, normals: &[[f32; 3]]) -> Vec<[f32; 4]> {
    let (width, depth, scale) = (heights.width(), heights.height(), heights.scale());
    let mut tangents = Vec::with_capacity(width * depth);
    for z in 0..depth {
        for x in 0..width {
            let (west, east) = (x.saturating_sub(1), (x + 1).min(width - 1));
            let run = (east - west).max(1) as f32 * scale;
            let along = Vec3::new(
                1.0,
                (heights.get(east, z) - heights.get(west, z)) / run,
                0.0,
            );
            let normal = Vec3::from(normals[z * width + x]);
            let tangent = (along - normal * normal.dot(along)).normalize_or(Vec3::X);
            // The handedness mikktspace gives this UV mapping: the test
            // below holds the two together.
            tangents.push([tangent.x, tangent.y, tangent.z, 1.0]);
        }
    }
    tangents
}

/// Follow the body (see the module docs): start fetching the patch it asks
/// for, or mark the standing one to go. A new terrain starts afresh.
#[allow(clippy::too_many_arguments)] // Bevy system: each arg is a distinct resource/query.
pub(in crate::terrain) fn follow_body(
    mut roaming: ResMut<RoamingPatch>,
    heightmap: Option<Res<FinishedHeightMap>>,
    terrain: Query<Entity, (With<TerrainMesh>, Without<OutgoingTerrain>)>,
    far_colliders: Query<(), With<FarCollider>>,
    body: Query<&GlobalTransform, With<LocalPlayer>>,
    mut fetcher: Option<ResMut<GeoFetcher>>,
    record: Option<Res<LiveRoomRecord>>,
    mut stamp: ResMut<PatchStamp>,
) {
    // A terrain going out keeps its patch until the next one stands: the
    // body stands on it meanwhile.
    let Ok(root) = terrain.single() else {
        return;
    };
    if roaming.terrain != Some(root) {
        roaming.abandon(fetcher.as_deref_mut(), Some(root));
        if stamp.0.is_some() {
            stamp.0 = None;
        }
    }
    let (Some(heightmap), Some(record), Ok(body)) = (heightmap, record, body.single()) else {
        return;
    };
    let Some(far) = heightmap.far() else {
        return;
    };
    // A far field drawn and not walked (P4.1: parry refused it) ends the
    // world at the core's edge: no patch past it.
    if far_colliders.is_empty() {
        return;
    }
    let at = body.translation().xz();
    let lattice = Lattice::of(&heightmap.0);
    let current = roaming
        .job
        .as_ref()
        .map(PatchJob::asked)
        .or(roaming.standing.as_ref().map(|s| (s.plan, s.asked)));
    match want(at, current, lattice, far.span_m() / 2.0) {
        Want::Stay => {}
        Want::Drop => {
            if roaming.job.is_some() {
                roaming.forget_job(fetcher.as_deref_mut());
            }
            if roaming.standing.is_some() && !roaming.letting_go {
                roaming.letting_go = true;
            }
            // Back in the core: a plan refused out there is tried again
            // when the body next asks for it.
            if roaming.refused.is_some() {
                roaming.refused = None;
            }
        }
        Want::Load { plan, asked } => {
            // One on its way at a time; and a refused one only once.
            if roaming.job.is_some() || roaming.refused == Some(plan) {
                return;
            }
            let Some(fetcher) = fetcher.as_deref_mut() else {
                return;
            };
            let Some(square) = record
                .0
                .geo_source
                .as_ref()
                .and_then(crate::pds::GeoSource::berlin_square)
            else {
                return;
            };
            let cfg = crate::pds::find_terrain_config(&record.0)
                .cloned()
                .unwrap_or_default();
            let (grid, cell) = core_grid(square.size_m, &cfg);
            // The ground in hand is the record's: a square or a grid edited
            // since is still being fetched, and no patch is asked for it.
            if grid as usize != heightmap.0.width() || cell != heightmap.0.scale() {
                return;
            }
            let bbox = plan.bbox(core_bbox(square, grid, cell), grid);
            let requests = CoreRequests::submit(fetcher, bbox, plan.grid(), None);
            let places = kept_places_for(Some(&record.0), &heightmap);
            roaming.letting_go = false;
            roaming.refused = None;
            roaming.job = Some(PatchJob::Fetching {
                plan,
                asked,
                requests: Box::new(requests),
                places,
            });
        }
    }
}

/// What landing a patch, and letting one go, reach.
#[derive(SystemParam)]
pub(in crate::terrain) struct PatchWorld<'w, 's> {
    commands: Commands<'w, 's>,
    meshes: ResMut<'w, Assets<Mesh>>,
    images: ResMut<'w, Assets<Image>>,
    materials: ResMut<'w, Assets<SplatTerrainMaterial>>,
    road_materials: ResMut<'w, Assets<StandardMaterial>>,
    splat: Option<ResMut<'w, SplatMaterialHandle>>,
    builds: Option<ResMut<'w, DerivedBuilds>>,
    far_colliders: Query<'w, 's, (Entity, &'static Collider), With<FarCollider>>,
    items: Query<'w, 's, (Entity, &'static DerivedItem)>,
    stamp: ResMut<'w, PatchStamp>,
    /// The body the patch follows: lifted where the ground rises under it.
    bodies: Query<'w, 's, &'static mut avian3d::prelude::Position, With<LocalPlayer>>,
}

/// Drive the patch on its way (see the module docs): once its answers are
/// in, decode them on the compute pool; once decoded, land it. And let the
/// standing patch go once the body is deep in the core.
#[allow(clippy::too_many_arguments)] // Bevy system: each arg is a distinct resource/query.
pub(in crate::terrain) fn drive_patch(
    mut roaming: ResMut<RoamingPatch>,
    mut fetcher: Option<ResMut<GeoFetcher>>,
    heightmap: Option<Res<FinishedHeightMap>>,
    live: Query<Entity, (With<TerrainMesh>, Without<OutgoingTerrain>)>,
    record: Option<Res<LiveRoomRecord>>,
    did: Option<Res<CurrentRoomDid>>,
    mut world: PatchWorld,
) {
    let Some(heightmap) = heightmap else {
        return;
    };
    let Some(far) = heightmap.ground().and_then(|ground| ground.far()).cloned() else {
        return;
    };
    // Only on the terrain the patch was asked for, and while it is the
    // live one: the heightmap in hand is then its own. While it goes out,
    // the next terrain's heightmap may already be in hand, and a patch
    // landed then would be set into a far field it was never cut for.
    let Some(terrain) = roaming
        .terrain
        .filter(|&terrain| live.single().ok() == Some(terrain))
    else {
        return;
    };
    if roaming.letting_go {
        roaming.letting_go = false;
        if let Some(standing) = roaming.standing.take() {
            let whole = roaming.whole_far.clone();
            let_go(&mut world, (&far, &heightmap.0), terrain, standing, whole);
        }
        return;
    }
    match roaming.job.take() {
        None => {}
        Some(PatchJob::Fetching {
            plan,
            asked,
            requests,
            places,
        }) => {
            let in_hand = fetcher.as_deref().is_some_and(|fetcher| {
                requests.core_ids().iter().all(|&id| fetcher.is_settled(id))
            });
            let Some(fetcher) = fetcher.as_deref_mut().filter(|_| in_hand) else {
                roaming.job = Some(PatchJob::Fetching {
                    plan,
                    asked,
                    requests,
                    places,
                });
                return;
            };
            let bodies = take_walkable(fetcher, &requests);
            for id in requests.ids() {
                fetcher.forget(id);
            }
            let bodies = match bodies {
                Ok(bodies) => bodies,
                Err(reason) => {
                    warn!("geodata patch: {reason} - the far field stays");
                    roaming.refused = Some(plan);
                    return;
                }
            };
            let inputs = PatchInputs {
                plan,
                core: heightmap.0.clone(),
                level: heightmap
                    .ground()
                    .and_then(|ground| ground.water_level())
                    .filter(|_| far.wet()),
                far: far.clone(),
            };
            let room = RoomScene::of(
                did.as_deref().map_or("", |did| did.0.as_str()),
                record.as_deref().map(|record| &record.0),
            );
            let pace = Pace::default();
            let task = AsyncComputeTaskPool::get().spawn(build_patch(
                bodies,
                inputs,
                places,
                room,
                pace.clone(),
            ));
            roaming.job = Some(PatchJob::Decoding {
                plan,
                asked,
                task,
                pace,
            });
        }
        Some(PatchJob::Decoding {
            plan,
            asked,
            mut task,
            pace,
        }) => {
            let Some(result) =
                futures_lite::future::block_on(futures_lite::future::poll_once(&mut task))
            else {
                // A frame has passed: on the web, the build's next stage.
                pace.tick();
                roaming.job = Some(PatchJob::Decoding {
                    plan,
                    asked,
                    task,
                    pace,
                });
                return;
            };
            match result {
                Ok(build) => {
                    for reason in &build.lost {
                        warn!("geodata patch: {reason} - leaving that out");
                    }
                    let old = roaming.standing.take();
                    roaming.landed += 1;
                    // The far field's own colliders, before the first patch
                    // cuts them: what letting go puts back.
                    if roaming.whole_far.is_none() {
                        roaming.whole_far = Some(
                            world
                                .far_colliders
                                .iter()
                                .map(|(_, collider)| collider.clone())
                                .collect(),
                        );
                    }
                    let root = land(
                        &mut world,
                        (&far, &heightmap.0),
                        terrain,
                        build,
                        (old, roaming.landed),
                        (record.as_deref(), did.as_deref()),
                    );
                    roaming.standing = Some(Standing { plan, asked, root });
                }
                Err(reason) => {
                    warn!("geodata patch: {reason} - the far field stays");
                    roaming.refused = Some(plan);
                }
            }
        }
    }
}

/// Land `build` (see the module docs) on the terrain `terrain` - its far
/// field and core - in place of the patch `old`, named `landed`; answers
/// the new patch's root.
fn land(
    world: &mut PatchWorld,
    (far, core): (&Arc<FarField>, &HeightMap),
    terrain: Entity,
    build: PatchBuild,
    (old, landed): (Option<Standing>, u64),
    (record, did): (Option<&LiveRoomRecord>, Option<&CurrentRoomDid>),
) -> Entity {
    let started = bevy::platform::time::Instant::now();
    let ground = build.ground;
    let rect = ground.rect();
    // A body standing on the ground the patch replaces - the far field's,
    // where the patch landed after the body got there - is lifted by as
    // much as the ground rises under it, so it is not left inside the new
    // ground; where the ground falls, it settles down onto it.
    for mut position in &mut world.bodies {
        let (x, z) = (position.0.x, position.0.z);
        if rect.contains(Vec2::new(x, z)) {
            let rise = ground.height_at(x, z) - far.drawn_height_at(core, x, z);
            if rise > 0.0 {
                position.0.y += rise;
            }
        }
    }
    far.patch().set(Some(ground.clone()));
    if let Some(old) = old {
        world.commands.entity(old.root).try_despawn();
    }
    swap_far_colliders(world, terrain, build.far_colliders);

    // Its material: the core's splat layers, its own weight map, the
    // weight map looked up from the core's UV mapping run on.
    let lattice = Lattice::of(core);
    let (core_m, patch_m) = (lattice.cells as f32 * lattice.cell, ground.extent());
    let (side, bytes) = build.weights;
    let weight_map = world
        .images
        .add(crate::terrain::splat::weight_image(side, side, bytes));
    let material = world
        .splat
        .as_ref()
        .and_then(|splat| world.materials.get(&splat.0).cloned())
        .map(|mut material| {
            let u = &mut material.extension.uniforms;
            material.extension.weight_map = weight_map;
            u.weight_uv_scale = core_m / patch_m;
            u.weight_uv_offset_u = (-lattice.half() - rect.min.x) / patch_m;
            u.weight_uv_offset_v = (-lattice.half() - rect.min.y) / patch_m;
            (u.hole_min_x, u.hole_min_z, u.hole_max_x, u.hole_max_z) = (0.0, 0.0, 0.0, 0.0);
            world.materials.add(material)
        });
    // The far field cuts the patch's hole: its material is drawn as a mask
    // from its spawn ([`crate::splat::HOLEABLE_ALPHA`]), so nothing compiles.
    if let Some(far_material) = world.splat.as_ref().and_then(|splat| splat.1.clone())
        && let Some(mut far_material) = world.materials.get_mut(&far_material)
    {
        let u = &mut far_material.extension.uniforms;
        (u.hole_min_x, u.hole_min_z, u.hole_max_x, u.hole_max_z) =
            (rect.min.x, rect.min.y, rect.max.x, rect.max.y);
    }
    if let Some(splat) = world.splat.as_mut() {
        splat.2 = material.clone();
    }

    // Its streets, in the theme's road look.
    let streets = crate::terrain::roads::add_road_surfaces(
        build.streets,
        crate::terrain::roads::road_theme(did),
        &Default::default(),
        &mut world.meshes,
        &mut world.road_materials,
    );
    let half = patch_m / 2.0;
    let centre = rect.center();
    let mesh = world.meshes.add(build.mesh);
    let root = world
        .commands
        .spawn((
            Transform::from_xyz(centre.x, 0.0, centre.y),
            Visibility::default(),
            build.collider,
            FarGround,
            ChildOf(terrain),
        ))
        .id();
    let mut drawn = world.commands.spawn((
        Mesh3d(mesh),
        Transform::from_xyz(-half, 0.0, -half),
        ChildOf(root),
    ));
    if let Some(material) = material {
        drawn.insert(MeshMaterial3d(material));
    }
    for surface in streets {
        let mut street = world.commands.spawn((
            Mesh3d(surface.mesh),
            MeshMaterial3d(surface.material),
            Transform::from_xyz(-half, 0.0, -half),
            surface.kind,
            ChildOf(root),
        ));
        if let Some(collider) = surface.collider {
            street.insert(collider);
        }
    }

    // Its street level, after the core's and the ring's plans; the ring's
    // lots under it covered. A plan's copies stand in the world frame, so
    // their roots undo the patch root's place: they go with the patch.
    let plans: Vec<(Plan, Entity)> = build
        .plans
        .into_iter()
        .map(|plan| {
            let at = world
                .commands
                .spawn((
                    DerivedRoot(plan.label),
                    Transform::from_xyz(-centre.x, 0.0, -centre.y),
                    Visibility::default(),
                    ChildOf(root),
                ))
                .id();
            (plan, at)
        })
        .collect();
    match world.builds.as_deref_mut() {
        Some(builds) if builds.terrain() == terrain => {
            builds.set_patch(plans);
            let covered = builds.ring_items_within(rect, COVER_MARGIN_M);
            builds.cover(covered, &mut world.commands, &world.items);
        }
        // A terrain whose core drew nothing: the patch's plans alone.
        None => {
            let mut builds =
                DerivedBuilds::empty(terrain, edit::suppressed_by(record.map(|record| &record.0)));
            builds.set_patch(plans);
            world.commands.insert_resource(builds);
        }
        // Plans of a terrain gone: the next terrain's replace them.
        Some(_) => {}
    }
    info!(
        "geodata patch: {:.0} m square at ({:.0}, {:.0}) landed - built in {:.2} s on the \
         compute pool, landed in {:.1} ms",
        patch_m,
        centre.x,
        centre.y,
        build.took_s,
        started.elapsed().as_secs_f64() * 1_000.0
    );
    world.stamp.0 = Some((rect, landed));
    root
}

/// Let the standing patch go: all [`land`] did, undone - the far field's
/// own colliders put back from `whole`, kept from the first landing, or
/// built again where there are none.
fn let_go(
    world: &mut PatchWorld,
    (far, core): (&Arc<FarField>, &HeightMap),
    terrain: Entity,
    standing: Standing,
    whole: Option<Vec<Collider>>,
) {
    far.patch().set(None);
    world.commands.entity(standing.root).try_despawn();
    let colliders = whole.unwrap_or_else(|| far.colliders(core, None));
    swap_far_colliders(world, terrain, colliders);
    if let Some(far_material) = world.splat.as_ref().and_then(|splat| splat.1.clone())
        && let Some(mut far_material) = world.materials.get_mut(&far_material)
    {
        let u = &mut far_material.extension.uniforms;
        (u.hole_min_x, u.hole_min_z, u.hole_max_x, u.hole_max_z) = (0.0, 0.0, 0.0, 0.0);
    }
    if let Some(splat) = world.splat.as_mut() {
        splat.2 = None;
    }
    if let Some(builds) = world.builds.as_deref_mut()
        && builds.terrain() == terrain
    {
        builds.set_patch(Vec::new());
        builds.cover(HashSet::new(), &mut world.commands, &world.items);
    }
    world.stamp.0 = None;
    info!("geodata patch: let go, the body deep in the core");
}

/// The far field's colliders, in place of those it stands on now: each an
/// entity of its own under the terrain's root, as the terrain's spawn puts
/// them.
fn swap_far_colliders(world: &mut PatchWorld, terrain: Entity, colliders: Vec<Collider>) {
    for (entity, _) in &world.far_colliders {
        world.commands.entity(entity).try_despawn();
    }
    for collider in colliders {
        world.commands.spawn((
            collider,
            Transform::IDENTITY,
            FarGround,
            FarCollider,
            ChildOf(terrain),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::mesh::VertexAttributeValues;

    /// A core of 101 points 2 m apart on a slope, and a rolling far field
    /// round it, as the pure tests' scene.
    fn scene() -> (HeightMap, Arc<FarField>) {
        let mut core = HeightMap::new(101, 101, 2.0);
        for z in 0..101 {
            for x in 0..101 {
                core.set(x, z, 34.0 + 0.02 * x as f32 - 0.01 * z as f32);
            }
        }
        let far = FarField::from_fn(48, 10.0, |x, z| {
            34.0 + 0.002 * x + 1.5 * (x * 0.031).sin() * (z * 0.027).cos()
        });
        (core, Arc::new(far))
    }

    /// East of the core, against its edge, 60 cells a side.
    const EAST: PatchPlan = PatchPlan {
        x0: 100,
        z0: -20,
        cells: 60,
    };

    /// The patch `EAST` on the far field's own ground, all of it park.
    fn ground(core: &HeightMap, far: &FarField) -> PatchGround {
        let lattice = Lattice::of(core);
        let rect = EAST.rect(lattice);
        let grid = EAST.grid() as usize;
        let mut heights = HeightMap::new(grid, grid, lattice.cell);
        for j in 0..grid {
            for i in 0..grid {
                let (x, z) = (rect.min.x + i as f32 * 2.0, rect.min.y + j as f32 * 2.0);
                heights.set(i, j, far.mesh_height_at(core, x, z) + 0.5);
            }
        }
        PatchGround {
            plan: EAST,
            min: rect.min,
            heights,
            cover: vec![Some(geodata::berlin::LandUse::Park); grid * grid],
        }
    }

    /// The patch's tangents are mikktspace's, as Bevy generates them for
    /// the core's mesh, on rolling ground and on rough.
    #[test]
    fn grid_tangents_are_the_mikktspace_tangents_of_a_heightfield() {
        for rough in [0.0_f32, 1.0] {
            let mut heights = HeightMap::new(33, 29, 2.0);
            for z in 0..29 {
                for x in 0..33 {
                    let (fx, fz) = (x as f32, z as f32);
                    let noise = rough * (((x * 7 + z * 13) % 5) as f32 - 2.0) * 0.2;
                    heights.set(
                        x,
                        z,
                        3.0 * (fx * 0.3).sin() + 2.0 * (fz * 0.2).cos() + noise,
                    );
                }
            }
            let mut mesh = HeightMapMeshBuilder::new()
                .with_normal_method(NormalMethod::AreaWeighted)
                .with_uv_tile_size(64.0)
                .build(&heights);
            let Some(VertexAttributeValues::Float32x3(normals)) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!("normals");
            };
            let ours = grid_tangents(&heights, normals);
            mesh.generate_tangents().expect("mikktspace");
            let Some(VertexAttributeValues::Float32x4(theirs)) =
                mesh.attribute(Mesh::ATTRIBUTE_TANGENT)
            else {
                panic!("tangents");
            };
            let mut worst = 1.0_f32;
            for (a, b) in ours.iter().zip(theirs) {
                assert_eq!(a[3], b[3], "the bitangent's sign");
                worst = worst.min(Vec3::from_slice(a).dot(Vec3::from_slice(b)));
            }
            assert!(worst > 0.98, "rough {rough}: cos {worst}");
        }
    }

    #[test]
    fn a_patch_mesh_tiles_on_from_the_core_and_takes_its_normals_at_the_seam() {
        let (core, far) = scene();
        let ground = ground(&core, &far);
        let mesh = patch_mesh(&ground, &core);
        let lattice = Lattice::of(&core);
        let core_m = lattice.cells as f32 * lattice.cell;
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("UVs");
        };
        let grid = EAST.grid() as usize;
        let rect = ground.rect();
        for (i, j) in [(0, 0), (7, 3), (60, 60)] {
            let world = rect.min + Vec2::new(i as f32, j as f32) * lattice.cell;
            let want = (world + Vec2::splat(lattice.half())) / core_m;
            let uv = uvs[j * grid + i];
            assert!((uv[0] - want.x).abs() < 1e-5 && (uv[1] - want.y).abs() < 1e-5);
        }
        let Some(VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("normals");
        };
        // The shared edge: the patch's column 0, the core's lattice z 0..=40.
        for lz in 0..=40 {
            let j = (lz - EAST.z0) as usize;
            let core_normal = crate::terrain::geo::far::core_normal(&core, 100, lz as usize);
            assert_eq!(normals[j * grid], core_normal.to_array(), "row {j}");
        }
        assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
    }

    /// An app with the asset stores a landing reaches, no renderer.
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()));
        app.init_asset::<Mesh>();
        app.init_asset::<Image>();
        app.init_asset::<StandardMaterial>();
        app.init_asset::<SplatTerrainMaterial>();
        app.init_resource::<PatchStamp>();
        app
    }

    /// The critic's finding (#1597): a patch built while its terrain goes
    /// out - a regeneration, the next terrain's heightmap already in hand -
    /// waits; it lands only while its terrain is the live one.
    #[test]
    fn a_patch_lands_only_on_the_live_terrain_it_was_asked_for() {
        let (core, _) = scene();
        let mut app = app();
        app.add_systems(Update, drive_patch);
        let far = FarField::from_fn(48, 10.0, |x, z| {
            34.0 + 0.002 * x + 1.5 * (x * 0.031).sin() * (z * 0.027).cos()
        });
        let grid = 101_usize;
        let geo = crate::terrain::geo::GeoGround::from_cover(
            grid as u32,
            2.0,
            vec![None; grid * grid],
            None,
        )
        .with_far(far);
        let world = app.world_mut();
        world.insert_resource(FinishedHeightMap(core.clone(), Some(geo)));
        let far = Arc::new(world.resource::<FinishedHeightMap>().far().unwrap().clone());
        let terrain = world
            .spawn((Transform::IDENTITY, TerrainMesh, OutgoingTerrain))
            .id();
        for collider in far.colliders(&core, None) {
            world.spawn((collider, FarGround, FarCollider, ChildOf(terrain)));
        }
        let mut materials = world.resource_mut::<Assets<SplatTerrainMaterial>>();
        let core_material = materials.add(SplatTerrainMaterial::default());
        world.insert_resource(SplatMaterialHandle(core_material, None, None));
        let patch = Arc::new(ground(&core, &far));
        let rect = patch.rect();
        let build = PatchBuild {
            mesh: patch_mesh(&patch, &core),
            collider: bevy_symbios_ground::build_heightfield_collider(patch.heights()),
            far_colliders: far.colliders(&core, Some(rect)),
            weights: (
                61,
                patch.weight_map().data.iter().flatten().copied().collect(),
            ),
            ground: patch,
            streets: Vec::new(),
            plans: Vec::new(),
            lost: Vec::new(),
            took_s: 0.0,
        };
        let task = bevy::tasks::AsyncComputeTaskPool::get_or_init(Default::default)
            .spawn(async move { Ok(build) });
        world.insert_resource(RoamingPatch {
            terrain: Some(terrain),
            job: Some(PatchJob::Decoding {
                plan: EAST,
                asked: Vec2::ZERO,
                task,
                pace: Pace::default(),
            }),
            ..Default::default()
        });
        let slot = || far.patch().get().is_some();
        for _ in 0..20 {
            app.update();
        }
        assert!(!slot(), "not while its terrain goes out");
        assert!(app.world().resource::<RoamingPatch>().job.is_some());
        app.world_mut()
            .entity_mut(terrain)
            .remove::<OutgoingTerrain>();
        for _ in 0..200 {
            app.update();
            if slot() {
                break;
            }
        }
        assert!(slot(), "on the live terrain, it lands");
        let roaming = app.world().resource::<RoamingPatch>();
        assert!(roaming.job.is_none() && roaming.standing.is_some());
    }

    #[test]
    fn a_patch_lands_in_one_frame_and_lets_go_in_one() {
        let (core, far) = scene();
        let mut app = app();
        let world = app.world_mut();
        let terrain = world.spawn((Transform::IDENTITY, TerrainMesh)).id();
        let own = far.colliders(&core, None);
        for collider in own.clone() {
            world.spawn((collider, FarGround, FarCollider, ChildOf(terrain)));
        }
        let mut materials = world.resource_mut::<Assets<SplatTerrainMaterial>>();
        let core_material = materials.add(SplatTerrainMaterial::default());
        // The far field's material, a mask from its spawn, as the terrain
        // spawns it.
        let mut holeable = SplatTerrainMaterial::default();
        holeable.base.alpha_mode = crate::splat::HOLEABLE_ALPHA;
        let far_material = materials.add(holeable);
        world.insert_resource(SplatMaterialHandle(
            core_material,
            Some(far_material.clone()),
            None,
        ));

        let ground = Arc::new(ground(&core, &far));
        let rect = ground.rect();
        // A body on the far field where the patch lands, 0.5 m higher, and
        // one beside it.
        let on = far.mesh_height_at(&core, 160.0, -80.0) + 0.3;
        let body = world
            .spawn((
                avian3d::prelude::Position::new(Vec3::new(160.0, on, -80.0)),
                LocalPlayer,
            ))
            .id();
        let beside = world
            .spawn((
                avian3d::prelude::Position::new(Vec3::new(60.0, 40.0, 0.0)),
                LocalPlayer,
            ))
            .id();
        let build = PatchBuild {
            mesh: patch_mesh(&ground, &core),
            collider: bevy_symbios_ground::build_heightfield_collider(ground.heights()),
            far_colliders: far.colliders(&core, Some(rect)),
            weights: (
                61,
                ground.weight_map().data.iter().flatten().copied().collect(),
            ),
            ground,
            streets: Vec::new(),
            plans: Vec::new(),
            lost: Vec::new(),
            took_s: 0.0,
        };
        let holed = build.far_colliders.len();
        let (landed_far, landed_core) = (far.clone(), core.clone());
        let mut build = Some(build);
        let root = world
            .run_system_once(move |mut world: PatchWorld| {
                land(
                    &mut world,
                    (&landed_far, &landed_core),
                    terrain,
                    build.take().expect("one landing"),
                    (None, 1),
                    (None, None),
                )
            })
            .expect("lands");
        let world = app.world_mut();
        // Lifted as the ground rose under it; beside it, left.
        let y = |world: &World, body| world.get::<avian3d::prelude::Position>(body).unwrap().0.y;
        assert!(
            (y(world, body) - (on + 0.5)).abs() < 1e-4,
            "{}",
            y(world, body)
        );
        assert_eq!(y(world, beside), 40.0);
        // Read in the far field's place, standing on its own collider under
        // the terrain, marked as ground.
        assert!(far.patch().get().is_some());
        let patch_root = world.entity(root);
        assert!(patch_root.contains::<FarGround>() && patch_root.contains::<Collider>());
        assert_eq!(
            patch_root.get::<ChildOf>().map(ChildOf::parent),
            Some(terrain)
        );
        assert_eq!(
            patch_root.get::<Transform>().map(|t| t.translation.xz()),
            Some(rect.center())
        );
        let mut far_colliders = world.query_filtered::<(), With<FarCollider>>();
        assert_eq!(far_colliders.iter(world).count(), holed);
        // The far field cuts its hole; the patch is drawn with its own map.
        let splat = world.resource::<SplatMaterialHandle>();
        let patch_material = splat.2.clone().expect("the patch's material");
        let materials = world.resource::<Assets<SplatTerrainMaterial>>();
        let cut = materials.get(&far_material).expect("the far material");
        assert_eq!(
            cut.base.alpha_mode,
            crate::splat::HOLEABLE_ALPHA,
            "its pipelines unchanged: nothing compiles as a patch lands"
        );
        let u = &cut.extension.uniforms;
        assert_eq!(
            (u.hole_min_x, u.hole_min_z, u.hole_max_x, u.hole_max_z),
            (rect.min.x, rect.min.y, rect.max.x, rect.max.y)
        );
        let drawn = materials.get(&patch_material).expect("the patch material");
        assert_eq!(drawn.base.alpha_mode, AlphaMode::Opaque);
        assert_eq!(drawn.extension.uniforms.weight_uv_scale, 200.0 / 120.0);
        assert_eq!(
            world.resource::<PatchStamp>().0,
            Some((rect, 1)),
            "the compile is told"
        );

        // And let go: all of it undone, the far field's own colliders put
        // back as they were kept.
        let (gone_far, gone_core, kept) = (far.clone(), core.clone(), own.clone());
        app.world_mut()
            .run_system_once(move |mut world: PatchWorld| {
                let standing = Standing {
                    plan: EAST,
                    asked: Vec2::ZERO,
                    root,
                };
                let_go(
                    &mut world,
                    (&gone_far, &gone_core),
                    terrain,
                    standing,
                    Some(kept.clone()),
                );
            })
            .expect("lets go");
        let world = app.world_mut();
        assert!(far.patch().get().is_none());
        assert!(world.get_entity(root).is_err(), "the patch is gone");
        let mut far_colliders = world.query_filtered::<(), With<FarCollider>>();
        assert_eq!(far_colliders.iter(world).count(), own.len());
        let splat = world.resource::<SplatMaterialHandle>();
        assert!(splat.2.is_none());
        let materials = world.resource::<Assets<SplatTerrainMaterial>>();
        let whole = materials.get(&far_material).expect("the far material");
        assert_eq!(whole.base.alpha_mode, crate::splat::HOLEABLE_ALPHA);
        let u = &whole.extension.uniforms;
        assert_eq!(
            (u.hole_min_x, u.hole_min_z, u.hole_max_x, u.hole_max_z),
            (0.0, 0.0, 0.0, 0.0),
            "no hole"
        );
        assert_eq!(world.resource::<PatchStamp>().0, None);
    }
}
