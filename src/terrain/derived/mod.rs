//! The derived stage (#1587, #1588, epic #1580): what a geodata region draws
//! from Berlin beyond its record - the buildings, trees and street furniture
//! of its walkable ground ([`core`]) and the buildings round it ([`ring`]).
//!
//! The record never holds any of it. A street-level core of central Berlin
//! is thousands of items, past the record's placement and size caps, so the
//! owner's decision (docs/geodata.md) is that untouched content is derived
//! again on every visit, and a save keeps only what the owner changed: an
//! item is named by where it comes from, a [`SourceId`] - an ALKIS building's
//! uuid, an inventory tree's gisid, a surveyed bench's gis_id - which every
//! entity a copy of it spawns at its top carries ([`DerivedItem`]; a copy
//! drawn whole, on its root), so a later save can suppress or adopt it
//! (#1590).
//!
//! Each part of the stage is a [`plan::Plan`]: its distinct catalogue
//! buildings and the copies of them that stand on Berlin's footprints, tree
//! points and survey points. The plans are drawn when a terrain lands
//! ([`start_derived`]) and spawned a slice a frame, the walkable ground's
//! first, nearest the landing first, under a root of their own on the
//! terrain, so they go with it ([`spawn_derived`]):
//!
//! 1. **Templates**: each distinct building - an entry at a drawn scale -
//!    is grown once and spawned whole, hidden, as a remote avatar's
//!    visuals are: no collider, no editor marker, no room entity.
//! 2. **Baking**: each template's parts are merged into its near and far
//!    forms ([`bake`]) and the template despawned. A building whose parts
//!    cannot all be merged per material keeps its tree for its near copies.
//! 3. **Copies**: each copy, near while its plan's entity budget lasts and
//!    far past it, on its collider where it stands on the walkable ground.
//!
//! A slice is [`SLICE_MS`] and a few steps of each stage at most: a step
//! runs to its end once begun, and the spawns it queues are applied after
//! the slice's clock has stopped.

pub(crate) mod bake;
pub(crate) mod core;
pub(crate) mod fit;
pub(crate) mod plan;
pub(crate) mod ring;

use std::collections::VecDeque;
use std::sync::Arc;

use bevy::platform::time::Instant;
use bevy::prelude::*;

use crate::player::visuals::AvatarSpawnDeps;
use crate::state::{CurrentRoomDid, LiveRoomRecord};
use crate::world_builder::draw_distance::DrawDistanceCuts;

use super::{FinishedHeightMap, TerrainMesh};
use plan::{Build, Stage};

/// How long the stage may spend spawning in one frame (ms).
const SLICE_MS: f64 = 4.0;

/// The most of each stage's steps one frame takes, however little time they
/// took: a template is a whole catalogue tree, a bake of a large landmark
/// the longest single step, and a copy a few spawns.
const TEMPLATES_PER_SLICE: usize = 2;
const BAKES_PER_SLICE: usize = 1;
const COPIES_PER_SLICE: usize = 256;

/// The Berlin layer a derived item is drawn from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SourceLayer {
    /// An ALKIS building, by its uuid.
    Building,
    /// An inventory tree, by its gisid.
    Tree,
    /// A surveyed item of street furniture, by its gis_id.
    Furniture,
    /// A lot of the middle ring, by its place: the ring is cut from renders,
    /// which carry no ids.
    RingLot,
}

/// Where a derived item comes from: its layer, and its key there - stable
/// across fetches, and what a save names the item by (#1590).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SourceId {
    pub layer: SourceLayer,
    pub key: Arc<str>,
}

impl SourceId {
    pub(crate) fn new(layer: SourceLayer, key: impl Into<Arc<str>>) -> Self {
        SourceId {
            layer,
            key: key.into(),
        }
    }
}

impl std::fmt::Display for SourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let layer = match self.layer {
            SourceLayer::Building => "alkis",
            SourceLayer::Tree => "tree",
            SourceLayer::Furniture => "furniture",
            SourceLayer::RingLot => "ring",
        };
        write!(f, "{layer}:{}", self.key)
    }
}

/// On every entity a derived item spawns: where it was drawn from.
#[derive(Component, Clone, Debug, PartialEq)]
pub(crate) struct DerivedItem(pub SourceId);

/// The root a plan's copies hang under, a child of the terrain, named by
/// its plan.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DerivedRoot(pub &'static str);

/// The plans being spawned onto the terrain they were drawn for, the one in
/// front first.
#[derive(Resource)]
pub(crate) struct DerivedBuilds {
    /// The terrain root the plans belong to.
    terrain: Entity,
    builds: VecDeque<Build>,
    /// The one material every far form is drawn with, its colour the form's.
    far_material: Option<Handle<StandardMaterial>>,
}

/// When a terrain lands, draw its plans - where its ground has street-level
/// content or a ring - and start spawning them. A new terrain ends the
/// plans of the one before, spawned or not: they go with their terrain.
pub(super) fn start_derived(
    mut commands: Commands,
    added: Query<Entity, Added<TerrainMesh>>,
    heightmap: Option<Res<FinishedHeightMap>>,
    did: Option<Res<CurrentRoomDid>>,
    record: Option<Res<LiveRoomRecord>>,
) {
    let Some(terrain) = added.iter().last() else {
        return;
    };
    commands.remove_resource::<DerivedBuilds>();
    let Some(heightmap) = heightmap else {
        return;
    };
    let Some(ground) = heightmap.ground() else {
        return;
    };
    let did = did.as_deref().map_or("", |did| did.0.as_str());
    let mut plans = Vec::new();
    if let Some(level) = ground.street_level() {
        plans.extend(core::draw_core(
            level,
            did,
            record.as_deref().map(|record| &record.0),
            &heightmap,
        ));
    }
    if let Some(ring) = ground.ring() {
        plans.push(ring::draw_ring(ring.lots(), did, &|x, z| {
            heightmap.view_height_at(x, z)
        }));
    }
    plans.retain(|plan| !plan.copies.is_empty());
    if plans.is_empty() {
        return;
    }
    let builds = plans
        .into_iter()
        .map(|plan| {
            let root = commands
                .spawn((
                    DerivedRoot(plan.label),
                    Transform::IDENTITY,
                    Visibility::default(),
                    ChildOf(terrain),
                ))
                .id();
            Build::new(plan, root)
        })
        .collect();
    commands.insert_resource(DerivedBuilds {
        terrain,
        builds,
        far_material: None,
    });
}

/// Work the front plan's stages (see the module docs) for at most
/// [`SLICE_MS`] a frame. A terrain going out, or gone, takes its plans with
/// it, so the work stops there too.
///
/// A stage ends a frame's slice when it is done, so the next stage reads the
/// world the last one's commands made: the bake reads the templates' parts,
/// which their spawn only queued.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_derived(
    mut commands: Commands,
    mut builds: ResMut<DerivedBuilds>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut deps: AvatarSpawnDeps,
    parts: bake::PartQuery,
    cuts: Option<Res<DrawDistanceCuts>>,
    wind_materials: Option<Res<Assets<crate::wind::VegetationWindMaterial>>>,
) {
    // `terrain_meshes` is the terrain that is neither gone nor going.
    if !deps.terrain_meshes.contains(builds.terrain) {
        commands.remove_resource::<DerivedBuilds>();
        return;
    }
    let started = Instant::now();
    let out_of_time = || started.elapsed().as_secs_f64() * 1_000.0 >= SLICE_MS;
    let builds = &mut *builds;
    let Some(build) = builds.builds.front_mut() else {
        commands.remove_resource::<DerivedBuilds>();
        return;
    };
    // This frame's steps, against the stage's cap.
    let mut steps = 0usize;
    loop {
        match build.stage {
            Stage::Templates(b) if b < build.plan.buildings.len() => {
                if out_of_time() || steps == TEMPLATES_PER_SLICE {
                    return;
                }
                steps += 1;
                let template = commands
                    .spawn((Transform::IDENTITY, Visibility::Hidden, ChildOf(build.root)))
                    .id();
                plan::spawn_whole(
                    &mut commands,
                    &mut build.plan,
                    b,
                    (template, Transform::IDENTITY),
                    (&mut meshes, &mut materials, &mut images),
                    &mut deps,
                );
                build.plan.buildings[b].template = Some(template);
                build.stage = Stage::Templates(b + 1);
            }
            Stage::Templates(_) => {
                build.stage = Stage::Merging(0);
                return;
            }
            Stage::Merging(b) if b < build.plan.buildings.len() => {
                if out_of_time() || steps == BAKES_PER_SLICE {
                    return;
                }
                steps += 1;
                build.bake(
                    b,
                    &mut commands,
                    &parts,
                    &mut meshes,
                    (&materials, wind_materials.as_deref()),
                );
                build.stage = Stage::Merging(b + 1);
            }
            Stage::Merging(_) => {
                build.stage = Stage::Copies(0);
                return;
            }
            Stage::Copies(c) if c < build.plan.copies.len() => {
                if out_of_time() || steps == COPIES_PER_SLICE {
                    return;
                }
                steps += 1;
                build.stage = Stage::Copies(c + 1);
                let far_material = builds
                    .far_material
                    .get_or_insert_with(|| materials.add(plan::far_material()))
                    .clone();
                build.spawn_copy(
                    c,
                    &mut commands,
                    (&mut meshes, &mut materials, &mut images),
                    &mut deps,
                    &far_material,
                    cuts.as_deref(),
                );
            }
            Stage::Copies(_) => break,
        }
    }
    info!(
        "derived {}: {} near entities, {} far copies, of {} copies from {} distinct buildings",
        build.plan.label,
        build.spawned,
        build.far_spawned,
        build.plan.copies.len(),
        build.plan.buildings.len()
    );
    builds.builds.pop_front();
    if builds.builds.is_empty() {
        commands.remove_resource::<DerivedBuilds>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_defaults::ThemeArchetype;
    use crate::terrain::OutgoingTerrain;
    use crate::terrain::geo::GeoGround;
    use crate::terrain::geo::ring::{Ring, RingLot};
    use crate::terrain::geo::street_level::{CoreBuilding, CoreFurniture, CoreTree, StreetLevel};
    use crate::water::{WaterMaterial, WaterSurfaces};
    use avian3d::prelude::Collider;
    use geodata::berlin::{BuildingUse, FurnitureKind};

    use fit::did_of;
    use plan::Plan;

    /// A lot at `(x, z)`, `beyond` past the walls, with the whole of its
    /// 30 m to itself, Berlin's building on it `standing` high.
    fn lot(x: f32, z: f32, standing: f32, beyond: f32) -> RingLot {
        RingLot {
            x,
            z,
            yaw: 0.0,
            room: 15.0,
            standing,
            beyond,
        }
    }

    /// A walkable ground of one terrace, one linden and one street lamp,
    /// south of the landing.
    fn street_level() -> StreetLevel {
        let outline = vec![
            (-30.0, 40.0),
            (30.0, 40.0),
            (30.0, 52.0),
            (-30.0, 52.0),
            (-30.0, 40.0),
        ];
        StreetLevel::new(
            vec![CoreBuilding {
                id: "DEBE00YY11100001".into(),
                outline,
                usage: BuildingUse::Residential,
                storeys: Some(5),
                peak_storeys: Some(5),
                area: 720.0,
                street_yaw: 0.0,
            }],
            vec![CoreTree {
                id: "00008100:0001".into(),
                x: -20.0,
                z: 30.0,
                genus: Some("Tilia".to_owned()),
                height: Some(16.0),
                crown: Some(9.0),
                girth: Some(140.0),
            }],
            vec![CoreFurniture {
                id: "40477-2210001-00".into(),
                kind: FurnitureKind::Lamp,
                x: 0.0,
                z: 34.0,
                yaw: 0.0,
                length: None,
            }],
        )
    }

    /// A DID of a modern city whose room grows street lamps.
    fn lit_city() -> String {
        (0..10_000)
            .map(|i| format!("did:plc:lit{i}"))
            .find(|did| {
                fit::room_theme(did).0 == ThemeArchetype::ModernCity
                    && !core::furniture::matches_for(did)[&FurnitureKind::Lamp].is_empty()
            })
            .expect("a lit city")
    }

    /// The resources the spawner reaches, as the compile's own test app has
    /// them, with no renderer.
    fn derived_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()));
        app.init_asset::<Mesh>();
        app.init_asset::<Image>();
        app.init_asset::<StandardMaterial>();
        app.init_asset::<WaterMaterial>();
        app.init_resource::<crate::world_builder::LSystemMaterialCache>();
        app.init_resource::<crate::world_builder::LSystemMeshCache>();
        app.init_resource::<crate::world_builder::ShapeMaterialCache>();
        app.init_resource::<crate::world_builder::ShapeMeshCache>();
        app.init_resource::<crate::world_builder::prim_cache::PrimMeshCache>();
        app.init_resource::<crate::world_builder::prim_cache::PrimMaterialCache>();
        app.init_resource::<bevy_symbios_shape::cache::ShapeMeshCache>();
        app.init_resource::<crate::world_builder::spatial_audio::BakedAudioCache>();
        app.insert_resource(crate::world_builder::fresh_texture_cache());
        app.init_resource::<crate::world_builder::compile::CompiledWorld>();
        app.init_resource::<crate::world_builder::compile::CompileJob>();
        app.init_resource::<WaterSurfaces>();
        app.init_resource::<crate::world_builder::image_cache::BlobImageCache>();
        app.init_resource::<crate::world_builder::audio_resolver::BlobAudioCache>();
        app.init_resource::<crate::diagnostics::SessionLog>();
        app.add_systems(
            Update,
            spawn_derived.run_if(resource_exists::<DerivedBuilds>),
        );
        app
    }

    /// `plans` handed to the spawner on a terrain of their own: the terrain
    /// and each plan's root.
    fn building(app: &mut App, plans: Vec<Plan>) -> (Entity, Vec<Entity>) {
        let world = app.world_mut();
        let terrain = world
            .spawn((TerrainMesh, Transform::IDENTITY, Visibility::default()))
            .id();
        let mut roots = Vec::new();
        let builds = plans
            .into_iter()
            .map(|plan| {
                let root = world
                    .spawn((
                        DerivedRoot(plan.label),
                        Transform::IDENTITY,
                        Visibility::default(),
                        ChildOf(terrain),
                    ))
                    .id();
                roots.push(root);
                Build::new(plan, root)
            })
            .collect();
        world.insert_resource(DerivedBuilds {
            terrain,
            builds,
            far_material: None,
        });
        (terrain, roots)
    }

    fn run_to_done(app: &mut App) {
        for _ in 0..400 {
            app.update();
            if !app.world().contains_resource::<DerivedBuilds>() {
                return;
            }
        }
        panic!("the plans are not done in 400 frames");
    }

    #[test]
    fn a_near_copy_is_its_merged_parts_and_a_far_one_its_far_form() {
        let mut app = derived_app();
        // Two rows: the near one at the walls, the far one well past.
        let lots: Vec<RingLot> = (0..24)
            .map(|i| {
                let beyond = if i < 12 {
                    20.0
                } else {
                    ring::NEAR_RING_M + 300.0
                };
                lot((i % 12) as f32 * 30.0, beyond, 12.0 + i as f32, beyond)
            })
            .collect();
        let plan = ring::draw_ring(&lots, &did_of(ThemeArchetype::ModernCity), &|_, _| 30.0);
        let (_, roots) = building(&mut app, vec![plan]);
        run_to_done(&mut app);
        let world = app.world_mut();
        let children: Vec<Entity> = world
            .get::<Children>(roots[0])
            .expect("the ring has buildings")
            .iter()
            .collect();
        let (mut near, mut far) = (Vec::new(), Vec::new());
        for child in &children {
            assert!(
                world.get::<Children>(*child).is_none(),
                "a part, not a template"
            );
            assert!(world.get::<Mesh3d>(*child).is_some());
            assert!(
                world.get::<Collider>(*child).is_none(),
                "no one walks there"
            );
            let item = world.get::<DerivedItem>(*child).expect("named by its lot");
            assert_eq!(item.0.layer, SourceLayer::RingLot);
            let t = world.get::<Transform>(*child).unwrap().translation;
            if world.get::<bevy::light::NotShadowCaster>(*child).is_some() {
                far.push(t);
            } else {
                near.push(t);
            }
        }
        // Every far lot is one entity; every near lot a handful.
        assert_eq!(far.len(), 12);
        assert!(far.iter().all(|t| t.z == ring::NEAR_RING_M + 300.0));
        let near_lots: std::collections::BTreeSet<i32> = near.iter().map(|t| t.x as i32).collect();
        assert_eq!(near_lots.len(), 12, "a building on every near lot");
        assert!(near.iter().all(|t| t.z == 20.0));
        assert!(
            near.len() > 12 && near.len() <= 12 * 16,
            "{} near parts",
            near.len()
        );
        // Far forms share one material, which colours each by its vertices.
        let far_materials: std::collections::BTreeSet<_> = children
            .iter()
            .filter(|c| world.get::<bevy::light::NotShadowCaster>(**c).is_some())
            .map(|c| {
                world
                    .get::<MeshMaterial3d<StandardMaterial>>(*c)
                    .unwrap()
                    .0
                    .id()
            })
            .collect();
        assert_eq!(far_materials.len(), 1);
        let meshes: std::collections::BTreeSet<_> = children
            .iter()
            .map(|c| world.get::<Mesh3d>(*c).unwrap().0.id())
            .collect();
        assert!(meshes.len() < children.len(), "copies share their meshes");
    }

    /// The walkable ground is walked: each building, tree and lamp stands
    /// on a collider, and every entity drawn from one names it.
    #[test]
    fn the_walkable_grounds_copies_stand_on_colliders_and_name_their_source() {
        let mut app = derived_app();
        let heightmap = FinishedHeightMap(bevy_symbios_ground::HeightMap::new(8, 8, 2.0), None);
        let plans = core::draw_core(&street_level(), &lit_city(), None, &heightmap);
        assert_eq!(
            plans.iter().map(|p| p.label).collect::<Vec<_>>(),
            ["core buildings", "core trees", "core furniture"]
        );
        assert!(plans.iter().all(|p| !p.copies.is_empty()));
        let (_, roots) = building(&mut app, plans);
        run_to_done(&mut app);
        let world = app.world_mut();
        let mut colliders: Vec<SourceLayer> = Vec::new();
        for root in roots {
            for child in world.get::<Children>(root).expect("drawn").iter() {
                let item = world
                    .get::<DerivedItem>(child)
                    .unwrap_or_else(|| panic!("{child} names no source"));
                if world.get::<Collider>(child).is_some() {
                    colliders.push(item.0.layer);
                }
            }
        }
        let count = |layer| colliders.iter().filter(|l| **l == layer).count();
        assert!(count(SourceLayer::Building) >= 1, "{colliders:?}");
        assert_eq!(count(SourceLayer::Tree), 1);
        assert_eq!(count(SourceLayer::Furniture), 1);
    }

    /// A terrain landing with derived content starts its plans - the
    /// walkable ground's first, then the ring's - each root on that
    /// terrain. A terrain after it ends those and starts its own, or none
    /// where its ground has nothing to derive.
    #[test]
    fn a_landed_terrain_starts_its_plans_and_the_next_replaces_them() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, start_derived);
        let land = |app: &mut App, ground: GeoGround| {
            let world = app.world_mut();
            world.insert_resource(FinishedHeightMap(
                bevy_symbios_ground::HeightMap::new(8, 8, 2.0),
                Some(ground),
            ));
            let terrain = world
                .spawn((TerrainMesh, Transform::IDENTITY, Visibility::default()))
                .id();
            app.update();
            terrain
        };
        let bare = || GeoGround::from_cover(8, 2.0, vec![None; 64], None);
        let ring = || Ring::from_lots(vec![lot(40.0, 0.0, 20.0, 30.0), lot(70.0, 0.0, 20.0, 60.0)]);
        app.insert_resource(CurrentRoomDid(lit_city()));
        let labels = |app: &App| -> Vec<&'static str> {
            let builds = app.world().resource::<DerivedBuilds>();
            builds.builds.iter().map(|b| b.plan.label).collect()
        };
        let first = land(&mut app, bare().with_ring(ring()));
        assert_eq!(labels(&app), ["ring"]);
        let builds = app.world().resource::<DerivedBuilds>();
        assert_eq!(builds.terrain, first);
        let root = builds.builds[0].root;
        assert_eq!(
            app.world().get::<DerivedRoot>(root),
            Some(&DerivedRoot("ring"))
        );
        assert_eq!(
            app.world().get::<ChildOf>(root).map(ChildOf::parent),
            Some(first)
        );
        let second = land(
            &mut app,
            bare().with_ring(ring()).with_street_level(street_level()),
        );
        assert_eq!(app.world().resource::<DerivedBuilds>().terrain, second);
        assert_eq!(
            labels(&app),
            ["core buildings", "core trees", "core furniture", "ring"]
        );
        land(&mut app, bare());
        assert!(
            !app.world().contains_resource::<DerivedBuilds>(),
            "nothing to derive, no build"
        );
    }

    #[test]
    fn a_terrain_going_out_stops_its_plans() {
        let mut app = derived_app();
        let lots = [lot(0.0, 0.0, 20.0, 0.0), lot(30.0, 0.0, 20.0, 0.0)];
        let plan = ring::draw_ring(&lots, &did_of(ThemeArchetype::ModernCity), &|_, _| 30.0);
        let (terrain, _) = building(&mut app, vec![plan]);
        app.world_mut().entity_mut(terrain).insert(OutgoingTerrain);
        app.update();
        assert!(!app.world().contains_resource::<DerivedBuilds>());
    }
}
