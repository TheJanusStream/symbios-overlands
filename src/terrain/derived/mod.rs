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
//!    far past it, on its collider - the ring's too, since P4.1 (#1596)
//!    walks the ground round the core.
//!
//! A slice is [`SLICE_MS`] and a few steps of each stage at most: a step
//! runs to its end once begun, and the spawns it queues are applied after
//! the slice's clock has stopped.
//!
//! The owner's edits (#1590, [`edit`]) name the walkable ground's items the
//! record suppresses - removed, or made the world's own - and none of them
//! is drawn. They apply as the record changes, with no rebuild: a
//! suppressed item's entities are despawned, and a restored one drawn again
//! from its plan, which stays resident for its terrain's life for that.

pub(crate) mod bake;
pub(crate) mod core;
pub(crate) mod edit;
pub(crate) mod fit;
pub(crate) mod plan;
pub(crate) mod ring;
pub(crate) mod streets;

use std::collections::HashSet;
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

    /// The item a record's edit names (`alkis:<uuid>`, `tree:<gisid>`,
    /// `furniture:<id>`): the walkable ground's layers only, the ring's lots
    /// having no stable ids to edit them by.
    pub(crate) fn parse(id: &str) -> Option<Self> {
        let (layer, key) = id.split_once(':')?;
        let layer = match layer {
            "alkis" => SourceLayer::Building,
            "tree" => SourceLayer::Tree,
            "furniture" => SourceLayer::Furniture,
            _ => return None,
        };
        (!key.is_empty()).then(|| SourceId::new(layer, key))
    }

    /// Whether the owner may edit the item: one of the walkable ground's.
    pub(crate) fn is_editable(&self) -> bool {
        self.layer != SourceLayer::RingLot
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

/// The plans of the terrain they were drawn for: spawned in order, then
/// kept for that terrain's life, so the record's edits apply to them as it
/// changes.
#[derive(Resource)]
pub(crate) struct DerivedBuilds {
    /// The terrain root the plans belong to.
    terrain: Entity,
    builds: Vec<Build>,
    /// The plan being spawned: the first not yet done.
    next: usize,
    /// The one material every far form is drawn with, its colour the form's.
    far_material: Option<Handle<StandardMaterial>>,
    /// The items the record's edits keep from being drawn (#1590).
    suppressed: HashSet<SourceId>,
    /// The ring's items a detail patch stands over (P4.2, #1597): its own
    /// street level is drawn there, so they are not, while it stands.
    covered: HashSet<SourceId>,
    /// Where the detail patch's plans begin: the plans before are the
    /// walkable core's and the ring's, the rest the patch's.
    patch_from: usize,
    /// Copies to draw again, `(plan, copy)`: restored since the spawn
    /// passed them suppressed.
    restores: Vec<(usize, usize)>,
}

impl DerivedBuilds {
    /// `builds` on `terrain`, none of them begun, `suppressed` kept from
    /// being drawn.
    fn new(terrain: Entity, builds: Vec<Build>, suppressed: HashSet<SourceId>) -> Self {
        DerivedBuilds {
            terrain,
            patch_from: builds.len(),
            builds,
            next: 0,
            far_material: None,
            suppressed,
            covered: HashSet::new(),
            restores: Vec::new(),
        }
    }

    /// No plans on `terrain` yet, `suppressed` kept from being drawn: where
    /// a detail patch lands on a terrain whose core drew none (P4.2).
    pub(crate) fn empty(terrain: Entity, suppressed: HashSet<SourceId>) -> Self {
        Self::new(terrain, Vec::new(), suppressed)
    }

    /// The terrain root the plans belong to.
    pub(crate) fn terrain(&self) -> Entity {
        self.terrain
    }

    /// Whether `id` is kept from being drawn: suppressed by the record, or
    /// covered by a detail patch.
    fn hidden(&self, id: &SourceId) -> bool {
        self.suppressed.contains(id) || self.covered.contains(id)
    }

    /// Put a detail patch's plans (P4.2, #1597), each with the root its
    /// copies hang under, in place of the last patch's - whose roots go
    /// with that patch - and spawn them after the core's and the ring's.
    pub(crate) fn set_patch(&mut self, plans: Vec<(plan::Plan, Entity)>) {
        let from = self.patch_from;
        self.builds.truncate(from);
        self.restores.retain(|&(b, _)| b < from);
        self.next = self.next.min(from);
        self.builds.extend(
            plans
                .into_iter()
                .filter(|(plan, _)| !plan.copies.is_empty())
                .map(|(plan, root)| Build::new(plan, root)),
        );
    }

    /// The ring's items standing within `rect` - world `(x, z)` - grown by
    /// `margin` (m): what a detail patch there covers.
    pub(crate) fn ring_items_within(
        &self,
        rect: bevy::math::Rect,
        margin: f32,
    ) -> HashSet<SourceId> {
        let rect = rect.inflate(margin);
        self.builds[..self.patch_from]
            .iter()
            .flat_map(|build| &build.plan.copies)
            .filter(|copy| {
                copy.source.layer == SourceLayer::RingLot
                    && rect.contains(copy.pose.translation.xz())
            })
            .map(|copy| copy.source.clone())
            .collect()
    }

    /// Cover `covered` of the ring's items (P4.2, #1597) in place of those
    /// covered before: the newly covered have their entities despawned and
    /// what they cost given back, the uncovered are drawn again - each
    /// unless the record suppresses it.
    pub(crate) fn cover(
        &mut self,
        covered: HashSet<SourceId>,
        commands: &mut Commands,
        items: &Query<(Entity, &DerivedItem)>,
    ) {
        if covered == self.covered {
            return;
        }
        let gone: HashSet<SourceId> = covered
            .difference(&self.covered)
            .filter(|id| !self.suppressed.contains(*id))
            .cloned()
            .collect();
        let back: HashSet<SourceId> = self
            .covered
            .difference(&covered)
            .filter(|id| !self.suppressed.contains(*id))
            .cloned()
            .collect();
        self.covered = covered;
        self.hide_and_restore(&gone, &back, commands, items);
    }

    /// Despawn the entities of the items in `gone`, giving back what their
    /// copies cost their plans; draw again the copies of those in `back`,
    /// now where the spawn has passed them, else when it reaches them.
    fn hide_and_restore(
        &mut self,
        gone: &HashSet<SourceId>,
        back: &HashSet<SourceId>,
        commands: &mut Commands,
        items: &Query<(Entity, &DerivedItem)>,
    ) {
        if !gone.is_empty() {
            for (entity, item) in items {
                if gone.contains(&item.0) {
                    commands.entity(entity).despawn();
                }
            }
        }
        for (b, build) in self.builds.iter_mut().enumerate() {
            for c in 0..build.plan.copies.len() {
                let source = &build.plan.copies[c].source;
                if gone.contains(source) {
                    build.take_back(c);
                } else if back.contains(source) && build.reached(c) {
                    self.restores.push((b, c));
                }
            }
        }
    }

    /// Whether every plan is spawned and no copy waits to be drawn again.
    pub(crate) fn is_idle(&self) -> bool {
        self.next >= self.builds.len() && self.restores.is_empty()
    }

    /// The plans, spawned or being spawned.
    pub(crate) fn builds(&self) -> &[Build] {
        &self.builds
    }

    /// The nearest drawn item of the walkable ground, of those `keep` keeps,
    /// whose copy's box `ray` enters, and how far along it: what a click in
    /// the World Editor may pick (#1590). A box, not the mesh: a copy's
    /// merged meshes keep no copy on the CPU for a mesh ray to read, and a
    /// box takes a tree's crown as well as its trunk. A box the ray starts
    /// in is not one it enters: a camera under a crown picks through it.
    pub(crate) fn pick(
        &self,
        ray: Ray3d,
        keep: impl Fn(&SourceId) -> bool,
    ) -> Option<(SourceId, f32)> {
        let mut nearest: Option<(SourceId, f32)> = None;
        for build in &self.builds {
            for (c, copy) in build.plan.copies.iter().enumerate() {
                if build.drawn[c] == plan::Drawn::No
                    || !copy.source.is_editable()
                    || !keep(&copy.source)
                {
                    continue;
                }
                let Some((lo, hi)) = build.plan.bounds(copy.building) else {
                    continue;
                };
                // The ray in the copy's frame, where its box is: the same
                // distance along it, the map being affine.
                let to_copy = build.copy_pose(c).compute_affine().inverse();
                let hit = ray_box_distance(
                    to_copy.transform_point3(ray.origin),
                    to_copy.transform_vector3(*ray.direction),
                    (lo, hi),
                );
                if let Some(distance) = hit
                    && nearest.as_ref().is_none_or(|(_, d)| distance < *d)
                {
                    nearest = Some((copy.source.clone(), distance));
                }
            }
        }
        nearest
    }

    /// Every copy of `id`, as `(plan, copy)`.
    pub(crate) fn copies_of<'a>(
        &'a self,
        id: &'a SourceId,
    ) -> impl Iterator<Item = (usize, usize)> + 'a {
        self.builds.iter().enumerate().flat_map(move |(b, build)| {
            build
                .plan
                .copies
                .iter()
                .enumerate()
                .filter(move |(_, copy)| copy.source == *id)
                .map(move |(c, _)| (b, c))
        })
    }
}

/// How far along the ray from `origin` along `direction` it enters the box
/// `(lo, hi)`, in steps of `direction`: `None` where it misses, the box lies
/// behind it, or it starts inside the box and so enters it nowhere.
fn ray_box_distance(origin: Vec3, direction: Vec3, (lo, hi): (Vec3, Vec3)) -> Option<f32> {
    let step = direction.recip();
    let (t0, t1) = ((lo - origin) * step, (hi - origin) * step);
    let enter = t0.min(t1).max_element();
    let leave = t0.max(t1).min_element();
    (enter >= 0.0 && leave >= enter).then_some(enter)
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
    let record = record.as_deref().map(|record| &record.0);
    // The room as its record was rolled: its theme dresses Berlin (#1589).
    let room = fit::RoomScene::of(did, record);
    // What the record keeps for its own: nothing derived stands on it, on
    // the walkable ground or - walked since P4.1 (#1596) - in the ring.
    let kept = core::kept_for(record, &heightmap);
    let mut plans = Vec::new();
    if let Some(level) = ground.street_level() {
        plans.extend(core::draw_core(level, &room, &kept, &|x, z| {
            heightmap.world_height_at(x, z)
        }));
    }
    if let Some(ring) = ground.ring() {
        plans.push(ring::draw_ring(ring.lots(), &room, &kept, &|x, z| {
            heightmap.world_height_at(x, z)
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
    commands.insert_resource(DerivedBuilds::new(
        terrain,
        builds,
        edit::suppressed_by(record),
    ));
}

/// Whether there is spawning to do: plans not yet spawned, or copies to
/// draw again. [`spawn_derived`]'s run condition, so that once its plans
/// are spawned it no longer holds the asset stores every frame; a terrain
/// going out takes them with it through the teardown and the next
/// terrain's [`start_derived`].
pub(super) fn derived_spawning(builds: Option<Res<DerivedBuilds>>) -> bool {
    builds.is_some_and(|builds| !builds.is_idle())
}

/// Draw again the copies restored since the spawn passed them, then work
/// the front plan's stages (see the module docs) for at most [`SLICE_MS`] a
/// frame, a copy the record's edits suppress passed over. A terrain going
/// out, or gone, takes its plans with it, so the work stops there too. Runs
/// only while there is work ([`derived_spawning`]).
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
    for (b, c) in std::mem::take(&mut builds.restores) {
        let hidden = builds.hidden(&builds.builds[b].plan.copies[c].source);
        let build = &mut builds.builds[b];
        if hidden || build.drawn[c] != plan::Drawn::No {
            continue;
        }
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
    let Some(build) = builds.builds.get_mut(builds.next) else {
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
                build.stage = Stage::Copies(c + 1);
                let source = &build.plan.copies[c].source;
                if builds.suppressed.contains(source) || builds.covered.contains(source) {
                    continue;
                }
                steps += 1;
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
    builds.next += 1;
}

/// Bring what is drawn into line with the record's edits as it changes
/// (#1590): an item newly suppressed - removed, or made the world's own -
/// has its entities despawned and what they cost its plan given back; one
/// newly restored is drawn again, now where the spawn has passed it, else
/// when the spawn reaches it. An id no plan draws is passed over: an item
/// the data no longer holds, or one outside the walkable ground.
pub(super) fn apply_derived_edits(
    mut commands: Commands,
    mut builds: ResMut<DerivedBuilds>,
    record: Option<Res<LiveRoomRecord>>,
    items: Query<(Entity, &DerivedItem)>,
) {
    let wanted = edit::suppressed_by(record.as_deref().map(|record| &record.0));
    // Read through `Deref`: a record change that edits nothing here stamps
    // no change.
    if wanted == builds.suppressed {
        return;
    }
    let builds = &mut *builds;
    // An item a detail patch covers is not drawn either way (P4.2).
    let gone: HashSet<SourceId> = wanted
        .difference(&builds.suppressed)
        .filter(|id| !builds.covered.contains(*id))
        .cloned()
        .collect();
    let back: HashSet<SourceId> = builds
        .suppressed
        .difference(&wanted)
        .filter(|id| !builds.covered.contains(*id))
        .cloned()
        .collect();
    builds.suppressed = wanted;
    builds.hide_and_restore(&gone, &back, &mut commands, &items);
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
                fit::RoomScene::for_did(did).theme().0 == ThemeArchetype::ModernCity
                    && !core::furniture::matches_for(&fit::RoomScene::for_did(did))
                        [&FurnitureKind::Lamp]
                        .is_empty()
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
            (
                apply_derived_edits.run_if(
                    resource_exists::<DerivedBuilds>
                        .and_then(resource_exists_and_changed::<LiveRoomRecord>),
                ),
                spawn_derived.run_if(derived_spawning),
            )
                .chain(),
        );
        app
    }

    /// `plans` handed to the spawner on a terrain of their own: the terrain
    /// and each plan's root.
    fn building(app: &mut App, plans: Vec<Plan>) -> (Entity, Vec<Entity>) {
        building_with(app, plans, HashSet::new())
    }

    /// [`building`], `suppressed` kept from being drawn.
    fn building_with(
        app: &mut App,
        plans: Vec<Plan>,
        suppressed: HashSet<SourceId>,
    ) -> (Entity, Vec<Entity>) {
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
        world.insert_resource(DerivedBuilds::new(terrain, builds, suppressed));
        (terrain, roots)
    }

    fn run_to_done(app: &mut App) {
        for _ in 0..400 {
            app.update();
            if app
                .world()
                .get_resource::<DerivedBuilds>()
                .is_none_or(DerivedBuilds::is_idle)
            {
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
        let plan = ring::draw_ring(
            &lots,
            &fit::RoomScene::for_did(&did_of(ThemeArchetype::ModernCity)),
            &core::Kept::nothing(),
            &|_, _| 30.0,
        );
        let (_, roots) = building(&mut app, vec![plan]);
        run_to_done(&mut app);
        let world = app.world_mut();
        // Every copy stands on its shell (P4.1, #1596): one collider each,
        // apart from the parts drawn.
        let (shells, children): (Vec<Entity>, Vec<Entity>) = world
            .get::<Children>(roots[0])
            .expect("the ring has buildings")
            .iter()
            .partition(|child| world.get::<Collider>(*child).is_some());
        assert_eq!(shells.len(), 24, "a shell under every copy drawn");
        assert!(
            shells
                .iter()
                .all(|shell| world.get::<Mesh3d>(*shell).is_none())
        );
        let (mut near, mut far) = (Vec::new(), Vec::new());
        for child in &children {
            assert!(
                world.get::<Children>(*child).is_none(),
                "a part, not a template"
            );
            assert!(world.get::<Mesh3d>(*child).is_some());
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
        let plans = core::draw_core(
            &street_level(),
            &fit::RoomScene::for_did(&lit_city()),
            &core::kept_for(None, &heightmap),
            &|x, z| heightmap.world_height_at(x, z),
        );
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
        let plan = ring::draw_ring(
            &lots,
            &fit::RoomScene::for_did(&did_of(ThemeArchetype::ModernCity)),
            &core::Kept::nothing(),
            &|_, _| 30.0,
        );
        let (terrain, _) = building(&mut app, vec![plan]);
        app.world_mut().entity_mut(terrain).insert(OutgoingTerrain);
        app.update();
        assert!(!app.world().contains_resource::<DerivedBuilds>());
    }

    /// The walkable ground's three items, by their source ids.
    fn ids() -> [SourceId; 3] {
        [
            SourceId::new(SourceLayer::Building, "DEBE00YY11100001"),
            SourceId::new(SourceLayer::Tree, "00008100:0001"),
            SourceId::new(SourceLayer::Furniture, "40477-2210001-00"),
        ]
    }

    /// A record of the lit city's DID on a Berlin square, with `edits`
    /// made of the walkable ground's items.
    fn edited_record(
        edits: &[(&SourceId, crate::pds::geo_source::Edit)],
    ) -> crate::pds::RoomRecord {
        let mut record = crate::pds::RoomRecord::default_for_did("did:plc:edited");
        let mut source = crate::pds::GeoSource::berlin(geodata::GeoSquare {
            min_e: 391_000,
            min_n: 5_819_500,
            size_m: 1_000,
        });
        for (id, edit) in edits {
            source.set_edit(&id.to_string(), *edit).unwrap();
        }
        record.geo_source = Some(source);
        record
    }

    /// The layers of the items drawn under `roots`.
    fn drawn_items(app: &mut App) -> Vec<SourceId> {
        let world = app.world_mut();
        let mut query = world.query::<&DerivedItem>();
        let mut items: Vec<SourceId> = query.iter(world).map(|item| item.0.clone()).collect();
        items.sort_by_key(|id| id.to_string());
        items.dedup();
        items
    }

    fn core_plans() -> Vec<Plan> {
        let heightmap = FinishedHeightMap(bevy_symbios_ground::HeightMap::new(8, 8, 2.0), None);
        core::draw_core(
            &street_level(),
            &fit::RoomScene::for_did(&lit_city()),
            &core::kept_for(None, &heightmap),
            &|x, z| heightmap.world_height_at(x, z),
        )
    }

    /// An item the record removes is never drawn; restored, it is drawn
    /// from its plan with no rebuild; removed again, its entities go - each
    /// the frame the record changes (#1590).
    #[test]
    fn a_removed_item_is_not_drawn_and_a_restored_one_is_drawn_again() {
        use crate::pds::geo_source::Edit;
        let [building, tree, lamp] = ids();
        let mut app = derived_app();
        let record = edited_record(&[(&tree, Edit::Removed)]);
        app.insert_resource(LiveRoomRecord(record.clone()));
        let suppressed = edit::suppressed_by(Some(&record));
        assert_eq!(suppressed, HashSet::from([tree.clone()]));
        building_with(&mut app, core_plans(), suppressed);
        run_to_done(&mut app);
        let mut items = drawn_items(&mut app);
        assert!(!items.contains(&tree), "the removed tree is not drawn");
        assert!(items.contains(&building) && items.contains(&lamp));
        // Restored: drawn again from its plan, the same frame.
        let mut restored = record.clone();
        edit::restore(&mut restored, &tree.to_string()).unwrap();
        app.world_mut().resource_mut::<LiveRoomRecord>().0 = restored.clone();
        app.update();
        items = drawn_items(&mut app);
        assert!(items.contains(&tree), "the restored tree stands again");
        // Removed once more, and the lamp with it: both go.
        let mut removed = restored;
        edit::remove(&mut removed, &tree).unwrap();
        edit::remove(&mut removed, &lamp).unwrap();
        app.world_mut().resource_mut::<LiveRoomRecord>().0 = removed;
        app.update();
        app.update();
        items = drawn_items(&mut app);
        assert_eq!(items, [building]);
        // What the removed items cost their plans is given back.
        let builds = app.world().resource::<DerivedBuilds>();
        for build in &builds.builds[1..] {
            assert_eq!(
                (build.spawned, build.far_spawned),
                (0, 0),
                "{}",
                build.plan.label
            );
            assert!(build.drawn.iter().all(|d| *d == plan::Drawn::No));
        }
        // A record change that edits nothing here leaves the plans alone.
        let before = app.world().resource_ref::<DerivedBuilds>().last_changed();
        app.world_mut()
            .resource_mut::<LiveRoomRecord>()
            .0
            .environment
            .sun_illuminance
            .0 += 1.0;
        app.update();
        assert_eq!(
            app.world().resource_ref::<DerivedBuilds>().last_changed(),
            before
        );
    }

    /// Made the world's own, an item's copy is ordinary record content as
    /// drawn: its generators, one to each catalogue item and size, placed
    /// where its copies stood, turned and sized as they were; and the
    /// original is drawn no more. Restored, the copy goes (#1590).
    #[test]
    fn an_adopted_item_is_copied_as_drawn_and_restored_without_its_copy() {
        use crate::pds::geo_source::Edit;
        let [building, tree, _] = ids();
        let mut app = derived_app();
        // Not grown yet: refused, and the record as it was.
        building_with(&mut app, core_plans(), HashSet::new());
        let mut record = edited_record(&[]);
        let ground = |_: f32, _: f32| 0.0;
        {
            let builds = app.world().resource::<DerivedBuilds>();
            let refused = edit::adopt(&mut record, &tree, builds, &ground).unwrap_err();
            assert!(refused.contains("still being drawn"), "{refused}");
            assert!(record.geo_source.as_ref().unwrap().adopted.is_empty());
        }
        run_to_done(&mut app);
        let builds = app.world().resource::<DerivedBuilds>();
        // The tree: one generator, its copy's size in its root.
        let placed = edit::adopt(&mut record, &tree, builds, &ground).expect("copied");
        assert_eq!(placed, 1);
        let name = format!("{tree}#1");
        let generator = &record.generators[&name];
        let (b, c) = builds.copies_of(&tree).next().unwrap();
        let pose = builds.builds[b].copy_pose(c);
        assert!(pose.scale.x != 1.0, "the tree is drawn to its height");
        let root = Transform::from(&generator.transform);
        let grown = builds.builds[b]
            .plan
            .grown(builds.builds[b].plan.copies[c].building)
            .unwrap();
        let expected =
            Transform::from_scale(Vec3::splat(pose.scale.x)) * Transform::from(&grown.transform);
        assert!((root.scale - expected.scale).abs().max_element() < 1e-6);
        let Some(crate::pds::Placement::Absolute {
            generator_ref,
            transform,
            snap_to_terrain: true,
            avoid_water: false,
            ..
        }) = record.placements.last()
        else {
            panic!("a snapped absolute placement");
        };
        assert_eq!(generator_ref, &name);
        let at = Transform::from(transform);
        assert_eq!(at.translation, pose.translation, "on the ground, as drawn");
        assert!(at.rotation.angle_between(pose.rotation) < 1e-6);
        assert_eq!(at.scale, Vec3::ONE);
        let source = record.geo_source.as_ref().unwrap();
        assert_eq!(source.edit_of(&tree.to_string()), Edit::Adopted);
        assert!(edit::suppressed_by(Some(&record)).contains(&tree));
        // Twice is refused.
        assert!(edit::adopt(&mut record, &tree, builds, &ground).is_err());
        // The building: a row of copies sharing their generators - those
        // drawn.
        let before = record.placements.len();
        let copies = builds
            .copies_of(&building)
            .filter(|&(b, c)| builds.builds[b].drawn[c] != plan::Drawn::No)
            .count();
        assert!(copies >= 1);
        assert_eq!(
            edit::adopt(&mut record, &building, builds, &ground),
            Ok(copies)
        );
        assert_eq!(record.placements.len(), before + copies);
        let generators = record
            .generators
            .keys()
            .filter(|name| name.starts_with(&building.to_string()))
            .count();
        assert!(
            (1..=copies).contains(&generators),
            "{generators} for {copies} copies"
        );
        // Restored: the copy goes, generators and placements both.
        let placements = record.placements.len();
        edit::restore(&mut record, &tree.to_string()).unwrap();
        assert!(!record.generators.contains_key(&name));
        assert_eq!(record.placements.len(), placements - 1);
        assert_eq!(
            record
                .geo_source
                .as_ref()
                .unwrap()
                .edit_of(&tree.to_string()),
            Edit::Drawn
        );
    }

    /// Past the record's counts, a copy is refused and says why, and the
    /// record is as it was.
    #[test]
    fn a_copy_past_the_records_counts_is_refused() {
        let [_, tree, _] = ids();
        let mut app = derived_app();
        building_with(&mut app, core_plans(), HashSet::new());
        run_to_done(&mut app);
        let builds = app.world().resource::<DerivedBuilds>();
        let mut record = edited_record(&[]);
        for i in record.generators.len()..crate::pds::sanitize::limits::MAX_GENERATORS {
            record.generators.insert(
                format!("filler_{i:03}"),
                crate::pds::Generator::default_cuboid(),
            );
        }
        let generators = record.generators.len();
        let refused = edit::adopt(&mut record, &tree, builds, &|_, _| 0.0).unwrap_err();
        assert!(refused.contains("256 items"), "{refused}");
        assert_eq!(record.generators.len(), generators);
        assert!(record.geo_source.as_ref().unwrap().adopted.is_empty());
    }

    /// The panel names an item by what Berlin records of it, and an item
    /// the walkable ground does not hold by its kind.
    #[test]
    fn an_item_is_named_by_what_berlin_records() {
        let [building, tree, lamp] = ids();
        let level = street_level();
        assert_eq!(
            edit::describe(Some(&level), &building),
            "A residential building, of 5 storeys, 720 m\u{b2}"
        );
        assert_eq!(
            edit::describe(Some(&level), &tree),
            "A linden (Tilia), 16 m tall"
        );
        assert_eq!(edit::describe(Some(&level), &lamp), "A street lamp");
        assert_eq!(
            edit::describe(None, &tree),
            "A tree, not on this world's walkable ground"
        );
        // A record's ids parse back to the items, and the ring's to none.
        for id in ids() {
            assert_eq!(SourceId::parse(&id.to_string()), Some(id));
        }
        assert_eq!(SourceId::parse("ring:10,20"), None);
        assert_eq!(SourceId::parse("tree:"), None);
    }

    /// A click picks the nearest drawn item whose box the ray passes
    /// through - a tree by its crown as well as its trunk - and nothing
    /// removed, or of the ring.
    #[test]
    fn a_ray_picks_the_nearest_drawn_item_by_its_box() {
        let [_, tree, lamp] = ids();
        let mut app = derived_app();
        building_with(&mut app, core_plans(), HashSet::new());
        run_to_done(&mut app);
        let builds = app.world().resource::<DerivedBuilds>();
        let (b, c) = builds.copies_of(&tree).next().unwrap();
        let pose = builds.builds[b].copy_pose(c);
        let (lo, hi) = builds.builds[b]
            .plan
            .bounds(builds.builds[b].plan.copies[c].building)
            .unwrap();
        let crown = pose.transform_point((lo + hi) / 2.0 + Vec3::Y * (hi.y - lo.y) * 0.3);
        // From the west, level with the crown.
        let from = crown - Vec3::X * 20.0;
        let ray = Ray3d::new(from, Dir3::X);
        let (picked, distance) = builds.pick(ray, |_| true).expect("the crown is hit");
        assert_eq!(picked, tree);
        assert!(distance > 0.0 && distance < 20.0, "{distance}");
        // Straight up from far below the lamp: the lamp, nearest.
        let (bl, cl) = builds.copies_of(&lamp).next().unwrap();
        let at = builds.builds[bl].copy_pose(cl).translation;
        let up = Ray3d::new(at - Vec3::Y * 5.0, Dir3::Y);
        assert_eq!(
            builds.pick(up, |_| true).map(|(id, _)| id),
            Some(lamp.clone())
        );
        // Pointing away: nothing; nor from inside the crown's box, nor an
        // item the caller does not keep (one not on screen).
        assert!(
            builds
                .pick(Ray3d::new(from, Dir3::NEG_X), |_| true)
                .is_none()
        );
        assert_ne!(
            builds
                .pick(Ray3d::new(crown, Dir3::X), |_| true)
                .map(|(id, _)| id),
            Some(tree.clone())
        );
        assert_ne!(
            builds.pick(ray, |id| *id != tree).map(|(id, _)| id),
            Some(tree.clone())
        );
        // Removed, it is no longer there to pick.
        let record = edited_record(&[(&tree, crate::pds::geo_source::Edit::Removed)]);
        app.insert_resource(LiveRoomRecord(record));
        app.update();
        let builds = app.world().resource::<DerivedBuilds>();
        assert_ne!(builds.pick(ray, |_| true).map(|(id, _)| id), Some(tree));
    }

    /// Stand a detail patch over `rect` with `plans` (P4.2), as its landing
    /// does: its plans in place of the last patch's, the ring's lots under
    /// it covered.
    fn stand_patch(app: &mut App, plans: Vec<Plan>, rect: bevy::math::Rect) {
        use bevy::ecs::system::RunSystemOnce;
        let world = app.world_mut();
        let terrain = world.resource::<DerivedBuilds>().terrain();
        let rooted: Vec<(Plan, Entity)> = plans
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
                (plan, root)
            })
            .collect();
        let mut rooted = Some(rooted);
        world
            .run_system_once(
                move |mut commands: Commands,
                      mut builds: ResMut<DerivedBuilds>,
                      items: Query<(Entity, &DerivedItem)>| {
                    builds.set_patch(rooted.take().unwrap_or_default());
                    let covered = builds.ring_items_within(rect, 15.0);
                    builds.cover(covered, &mut commands, &items);
                },
            )
            .expect("stands");
    }

    /// P4.2 (#1597): a detail patch's plans spawn after the ring's and
    /// replace the last patch's alone, and the ring's lots under a patch
    /// are not drawn while it stands - drawn again once it moves on.
    #[test]
    fn a_patch_covers_the_ring_under_it_and_replaces_the_last_patch() {
        let mut app = derived_app();
        let lots: Vec<RingLot> = (0..12)
            .map(|i| lot(i as f32 * 30.0, 0.0, 12.0, 20.0))
            .collect();
        let ring = ring::draw_ring(
            &lots,
            &fit::RoomScene::for_did(&did_of(ThemeArchetype::ModernCity)),
            &core::Kept::nothing(),
            &|_, _| 30.0,
        );
        building(&mut app, vec![ring]);
        run_to_done(&mut app);
        let drawn = |app: &mut App, layer: SourceLayer| {
            drawn_items(app)
                .into_iter()
                .filter(|id| id.layer == layer)
                .count()
        };
        assert_eq!(drawn(&mut app, SourceLayer::RingLot), 12);

        // Over the lots from x = 180 on (165 with half a lot): six go, and
        // the patch's own street level is drawn after the ring.
        let over = |from: f32| bevy::math::Rect::new(from, -100.0, from + 300.0, 100.0);
        stand_patch(&mut app, core_plans(), over(180.0));
        run_to_done(&mut app);
        assert_eq!(drawn(&mut app, SourceLayer::RingLot), 6);
        assert_eq!(drawn(&mut app, SourceLayer::Building), 1);
        assert_eq!(drawn(&mut app, SourceLayer::Tree), 1);
        let labels: Vec<&str> = app
            .world()
            .resource::<DerivedBuilds>()
            .builds()
            .iter()
            .map(|b| b.plan().label)
            .collect();
        assert_eq!(
            labels,
            ["ring", "core buildings", "core trees", "core furniture"]
        );

        // The next patch, further on, with nothing of its own drawn: the
        // last patch's plans go, and the lots it alone covered come back.
        stand_patch(&mut app, Vec::new(), over(270.0));
        run_to_done(&mut app);
        let builds = app.world().resource::<DerivedBuilds>();
        assert_eq!(builds.builds().len(), 1, "the ring's plan alone");
        assert_eq!(drawn(&mut app, SourceLayer::RingLot), 9);
    }

    #[test]
    fn a_ray_enters_a_box_where_it_crosses_it() {
        let unit = (Vec3::ZERO, Vec3::ONE);
        let at = |o: Vec3, d: Vec3| ray_box_distance(o, d, unit);
        assert_eq!(at(Vec3::new(-2.0, 0.5, 0.5), Vec3::X), Some(2.0));
        assert_eq!(at(Vec3::splat(0.5), Vec3::X), None, "from inside");
        assert_eq!(at(Vec3::new(-2.0, 0.5, 0.5), Vec3::NEG_X), None, "behind");
        assert_eq!(at(Vec3::new(-2.0, 3.0, 0.5), Vec3::X), None, "above");
        // Along an axis the ray runs parallel to.
        assert_eq!(at(Vec3::new(0.5, -1.0, 0.5), Vec3::Y), Some(1.0));
    }
}
