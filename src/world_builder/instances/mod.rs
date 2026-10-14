//! Instance sets drawn (#1643, P2.1 of epic #1636): every copy of a set
//! ([`crate::pds::Placement::Instances`]) drawn from forms baked once per
//! variant, over the frames after the compile spawned the set's anchor.
//!
//! The compile spawns a set's anchor, marked [`InstanceSetRoot`], and is
//! done with it. Here each new anchor becomes a [`build::Build`]: its
//! copies stood where the record puts them - on the ground under each, in a
//! set that snaps - and ordered nearest the landing first. Then, a slice a
//! frame:
//!
//! 1. **Templates**: each variant whose forms are not held yet
//!    ([`forms::Forms`]) is grown once from the set's generator, its Shape
//!    nodes drawn with the variant's seed and values, and spawned whole and
//!    hidden under the anchor, as a remote avatar's visuals are: no
//!    collider, no editor marker.
//! 2. **Baking**: the frame after, each template's parts are merged into a
//!    near form, one mesh per material, and a far form, a 1 m voxel shell
//!    ([`bake`]), and the template despawned. The forms are kept by the
//!    generator's content and the variant, so an edit that respawns a set
//!    grows nothing again; once the room's sets are all drawn, and the
//!    compile is done, every form none of them draws from is let go
//!    (#1649), so a re-roll's new Kiez does not keep the last one's.
//! 3. **Copies**: once every set the room is drawing is baked, the room's
//!    budgets are shared among them by what their copies need, each copy
//!    tiered (#1648, below); then each copy as its tier says - near, as its
//!    variant's merged parts (a few entities), or far, as one entity, its
//!    shell, where it is big enough to be seen far - and on its collider
//!    ([`crate::pds::InstanceSolid`]) where it is drawn. A small copy is cut
//!    at the player's draw distance, as a scattered one is (#1480).
//!
//! A drawn set follows its record's copies in place (#1644): an edit of the
//! copies alone leaves the compile's unit as it is, and the build redraws
//! the copies that changed and no other ([`build`]); an edit of anything
//! else about the set builds its unit anew. Each drawn copy's entities
//! carry [`InstanceCopyOf`], and its pose and baked box are kept, so the
//! World Editor can pick one ([`InstanceBuilds::pick`]) and move it.
//!
//! A set draws its generator's geometry and nothing else: its sounds,
//! spins, particles, signs, portals and gateways are left out
//! ([`strip_for_distance`]), as a merged part has no transform of its own
//! to turn or emit from. Those are placed as placements of their own.
//!
//! The room's budget is what a Kiez of Berlin's buildings, trees and street
//! furniture was drawn with before it was seeded into the record (#1588,
//! #1647), and is shared by need ([`build::share_by_need`], #1648): every
//! copy of every set, nearest the landing first, is tiered near while the
//! near budget lasts, whatever it costs and whatever set it is in, then far
//! while the far budget does - one radius of detail round the landing, the
//! same on every client. A set baked waits until the room's last is, and
//! until the compile has spawned every set, so the first baked does not
//! take it all; the price is that a room shows none of its copies until
//! all its variants are baked. Copies a follow queues are tiered by the
//! room shared anew, with the sets baked so far, and drawn as tiered: a
//! copy added by the landing is drawn near however full the room is, while
//! a copy drawn before keeps what it drew - so for a while the room's sets
//! together may draw past the budget, and a peer that loads the room fresh
//! shares it anew.

pub(crate) mod bake;
mod build;
mod forms;

use bevy::platform::time::Instant;
use bevy::prelude::*;

use crate::pds::audio::SovereignAudioConfig;
use crate::pds::{Generator, GeneratorKind, Placement};
use crate::player::visuals::AvatarSpawnDeps;
use crate::state::{CurrentRoomDid, LiveRoomRecord};
use crate::world_builder::draw_distance::DrawDistanceCuts;

use super::PlacementUnit;

/// The most entities the near copies of all a room's sets may be: the
/// budgets Berlin's buildings, trees and furniture were drawn with before
/// #1647 seeded them, summed (12,000, 6,000 and 4,000).
pub(crate) const ROOM_NEAR_ENTITIES: u32 = 22_000;

/// The most far copies, one entity each, all a room's sets may draw: the
/// far budgets Berlin's buildings and trees were drawn with before #1647,
/// summed (3,000 and 2,000; its furniture drew none far).
pub(crate) const ROOM_FAR_COPIES: usize = 5_000;

/// How long the spawner may spend in one frame (ms).
const SLICE_MS: f64 = 4.0;

/// The most of each stage's steps one frame takes, however little time
/// they took: a template is a whole generator tree, a bake the longest
/// single step, and a copy a few spawns.
const TEMPLATES_PER_SLICE: usize = 2;
const BAKES_PER_SLICE: usize = 1;
const COPIES_PER_SLICE: usize = 256;

/// The anchor an instance set's copies hang under: spawned by the compile,
/// filled here.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct InstanceSetRoot;

/// On every entity a copy of a set spawns - each merged part, a far form,
/// a collider, the root of a copy drawn whole - which copy of which
/// placement it is (#1643), so a pick can name it.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InstanceCopyOf {
    pub placement: usize,
    pub copy: usize,
}

/// A room's drawing budgets (#1648): the near entities and far copies its
/// sets share by need.
#[derive(Clone, Copy, Debug)]
struct RoomBudget {
    near: u32,
    far: usize,
}

impl Default for RoomBudget {
    fn default() -> Self {
        RoomBudget {
            near: ROOM_NEAR_ENTITIES,
            far: ROOM_FAR_COPIES,
        }
    }
}

/// When the forms no set draws from are let go (#1649): due once the
/// room's sets change, and done once the room is drawn, no compile runs, and
/// both have stayed so a frame - the compile spawns a set's anchor a frame
/// before its build sees it, and a form let go then would be baked again.
#[derive(Clone, Copy, Debug, Default)]
struct Sweep {
    due: bool,
    armed: bool,
}

impl Sweep {
    /// The room's sets changed: their forms are to be looked over.
    fn owe(&mut self) {
        *self = Sweep {
            due: true,
            armed: false,
        };
    }
}

/// The sets being drawn, and the forms their variants were baked into.
#[derive(Resource, Default)]
pub(crate) struct InstanceBuilds {
    builds: Vec<build::Build>,
    forms: forms::Forms,
    far_material: Option<Handle<StandardMaterial>>,
    budget: RoomBudget,
    sweep: Sweep,
}

impl InstanceBuilds {
    /// Set the room's budgets, for a test of what lies past them.
    #[cfg(test)]
    pub(crate) fn set_room_budget(&mut self, near: u32, far: usize) {
        self.budget = RoomBudget { near, far };
    }

    /// Share the room's budgets among its sets by what they need (#1648,
    /// see the module docs), `landing` the landing on the plan: every set
    /// baked so far, as its copies stand now - a set still growing asks
    /// nothing yet. What the sets were given in all, and how far from the
    /// landing the near budget reached, where it did not reach every copy.
    fn share(&mut self, landing: Vec2) -> (u32, usize, Option<f32>) {
        let baked: Vec<usize> = (0..self.builds.len())
            .filter(|&i| !self.builds[i].is_growing())
            .collect();
        let demands: Vec<Vec<build::Demand>> = baked
            .iter()
            .map(|&i| self.builds[i].demands(landing))
            .collect();
        let shared = build::share_by_need(&demands, (self.budget.near, self.budget.far));
        for ((&i, asks), tiers) in baked.iter().zip(&demands).zip(&shared.tiers) {
            self.builds[i].set_share(asks, tiers);
        }
        (shared.given.0, shared.given.1, shared.cut)
    }

    /// Whether every set the room has spawned is drawn: what a renderer
    /// waits for before it takes its picture.
    pub(crate) fn is_idle(&self) -> bool {
        self.builds.iter().all(build::Build::is_done)
    }

    /// Whether every set is drawn and the forms none of them draws from are
    /// let go (#1649): what a rebuild's asset marks wait for.
    pub(crate) fn is_settled(&self) -> bool {
        self.is_idle() && !self.sweep.due
    }

    /// The nearest drawn copy of any set, of those `keep` keeps, whose
    /// baked box `ray` enters, as `(placement, copy)`, and how far along the
    /// ray (#1644).
    pub(crate) fn pick(
        &self,
        ray: Ray3d,
        keep: &dyn Fn((usize, usize)) -> bool,
    ) -> Option<((usize, usize), f32)> {
        self.builds
            .iter()
            .filter_map(|build| {
                let placement = build.placement();
                build
                    .pick(ray, &|copy| keep((placement, copy)))
                    .map(|(copy, distance)| ((placement, copy), distance))
            })
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
    }

    /// Where copy `copy` of the set `placement` stands as drawn, in the
    /// world: for a test to read what was drawn.
    #[cfg(test)]
    pub(crate) fn copy_pose(&self, placement: usize, copy: usize) -> Option<Transform> {
        let build = self.build_of(placement)?;
        Some(build.anchor() * build.drawn(copy)?.pose)
    }

    /// Every entity copy `copy` of the set `placement` spawned.
    pub(crate) fn copy_entities(&self, placement: usize, copy: usize) -> Option<&[Entity]> {
        Some(&self.build_of(placement)?.drawn(copy)?.entities)
    }

    /// Where `copy`, a copy of the set `placement`, stands in the world as
    /// the set would draw it on `ground`: drawn or not, as the record has
    /// it now.
    pub(crate) fn stand(
        &self,
        placement: usize,
        copy: &crate::pds::InstanceCopy,
        ground: Option<&bevy_symbios_ground::HeightMap>,
    ) -> Option<Transform> {
        Some(self.build_of(placement)?.stand_world(copy, ground))
    }

    /// The box variant `variant` of the set `placement` fills, in a copy's
    /// frame, once it is baked.
    pub(crate) fn variant_bounds(&self, placement: usize, variant: usize) -> Option<(Vec3, Vec3)> {
        self.build_of(placement)?.variant_bounds(variant)
    }

    /// Where the set `placement`'s anchor stands, as the compile stood it.
    pub(crate) fn anchor_of(&self, placement: usize) -> Option<Transform> {
        Some(self.build_of(placement)?.anchor())
    }

    fn build_of(&self, placement: usize) -> Option<&build::Build> {
        self.builds
            .iter()
            .find(|build| build.placement() == placement)
    }
}

/// How far along the ray from `origin` along `direction` it enters the box
/// `(lo, hi)`, in steps of `direction`: `None` where it misses, the box lies
/// behind it, or it starts inside the box and so enters it nowhere. How a
/// click finds a copy drawn merged (#1590, #1644): its meshes keep no copy
/// on the CPU for a mesh ray to read, and a box takes a tree's crown as
/// well as its trunk.
pub(crate) fn ray_box_distance(
    origin: Vec3,
    direction: Vec3,
    (lo, hi): (Vec3, Vec3),
) -> Option<f32> {
    let step = direction.recip();
    let (t0, t1) = ((lo - origin) * step, (hi - origin) * step);
    let enter = t0.min(t1).max_element();
    let leave = t0.max(t1).min_element();
    (enter >= 0.0 && leave >= enter).then_some(enter)
}

/// Strip from a tree drawn merged what it only costs: every sound, every
/// spin (#1604 - a merged part has no transform of its own to turn, so a
/// copy drawn whole would turn where its neighbours stand still), and
/// every node that is no geometry to see - particles, signs, portals,
/// gateways, and any water, terrain or road a tree should not carry.
pub(crate) fn strip_for_distance(tree: &mut Generator) {
    tree.audio = SovereignAudioConfig::None;
    tree.spin = None;
    tree.children.retain(|child| kept_afar(&child.kind));
    for child in &mut tree.children {
        strip_for_distance(child);
    }
}

/// Whether a node of this kind stays on a tree drawn merged.
fn kept_afar(kind: &GeneratorKind) -> bool {
    !matches!(
        kind,
        GeneratorKind::Terrain(_)
            | GeneratorKind::Water { .. }
            | GeneratorKind::RoadNetwork(_)
            | GeneratorKind::Portal { .. }
            | GeneratorKind::Gateway { .. }
            | GeneratorKind::ParticleSystem(_)
            | GeneratorKind::Sign { .. }
            | GeneratorKind::Unknown
    )
}

/// Register the spawner: the game's world builder and the headless render
/// tool's compile both draw sets.
pub(crate) fn register(app: &mut App) {
    app.init_resource::<InstanceBuilds>()
        // Where a variant that draws nothing says why (#1644): the game's
        // own, and the render tool's, which keeps none otherwise.
        .init_resource::<crate::world_builder::grammar_diag::GrammarDiagnostics>()
        .add_systems(
            Update,
            (start_instance_builds, spawn_instances)
                .chain()
                .run_if(resource_exists::<LiveRoomRecord>),
        );
}

/// Turn each anchor the compile spawned since the last frame into a build,
/// and drop the builds whose anchors are gone.
fn start_instance_builds(
    mut builds: ResMut<InstanceBuilds>,
    mut diagnostics: ResMut<crate::world_builder::grammar_diag::GrammarDiagnostics>,
    record: Res<LiveRoomRecord>,
    added: Query<(Entity, &PlacementUnit, &Transform), Added<InstanceSetRoot>>,
    roots: Query<(), With<InstanceSetRoot>>,
    heightmap: Option<Res<crate::terrain::FinishedHeightMap>>,
    room: Option<Res<CurrentRoomDid>>,
) {
    let builds = &mut *builds;
    builds
        .forms
        .keep_for_room(room.as_deref().map(|room| room.0.as_str()));
    let before = builds.builds.len();
    builds.builds.retain(|build| roots.contains(build.root()));
    if builds.builds.len() != before || !added.is_empty() {
        builds.sweep.owe();
    }
    if added.is_empty() {
        return;
    }
    let record = &record.0;
    for (root, unit, anchor) in &added {
        let Some(placement) = record.placements.get(unit.0) else {
            continue;
        };
        let Some(build) = build::Build::new(
            record,
            (unit.0, placement),
            (root, *anchor),
            heightmap.as_deref().map(|heightmap| &heightmap.0),
        ) else {
            continue;
        };
        diagnostics.forget_variants_of(unit.0);
        builds.builds.push(build);
    }
}

/// Follow each drawn set's copies as the record changes them (#1644): in
/// place where it can, else built anew under the same anchor. A set whose
/// unit changed is left alone - the compile builds it anew, anchor and all.
/// Whether any set queued copies to draw, which the room's share is then to
/// tier; a set built anew is shared when it is baked.
fn follow_copies(
    builds: &mut [build::Build],
    record: &crate::pds::RoomRecord,
    commands: &mut Commands,
    ground: Option<&bevy_symbios_ground::HeightMap>,
    caches: &mut crate::world_builder::compile::GeneratorCaches,
) -> bool {
    let mut queued = false;
    for build in builds.iter_mut() {
        let Some(placement) = record.placements.get(build.placement()) else {
            continue;
        };
        let Placement::Instances { copies, .. } = placement else {
            continue;
        };
        if !build.draws_unit_of(record, placement) {
            continue;
        }
        match build.follow(copies, commands, ground) {
            build::Followed::Unchanged | build::Followed::Done => {}
            build::Followed::Queued => queued = true,
            build::Followed::Rebuild => {
                build.take_down(commands, caches);
                if let Some(fresh) = build::Build::new(
                    record,
                    (build.placement(), placement),
                    (build.root(), build.anchor()),
                    ground,
                ) {
                    *build = fresh;
                }
            }
        }
    }
    queued
}

/// Advance the builds, a slice a frame (see the module docs), after
/// following what the record's copies did.
#[allow(clippy::too_many_arguments)]
fn spawn_instances(
    mut commands: Commands,
    mut builds: ResMut<InstanceBuilds>,
    mut diagnostics: ResMut<crate::world_builder::grammar_diag::GrammarDiagnostics>,
    record: Res<LiveRoomRecord>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut deps: AvatarSpawnDeps,
    parts: bake::PartQuery,
    cuts: Option<Res<DrawDistanceCuts>>,
    wind_materials: Option<Res<Assets<crate::wind::VegetationWindMaterial>>>,
) {
    let started = Instant::now();
    let landing = record
        .0
        .default_landing
        .map_or(Vec2::ZERO, |landing| Vec2::from_array(landing.pos.0));
    if record.is_changed() {
        let ground = deps.heightmap.as_deref().map(|heightmap| &heightmap.0);
        // Copies a set queued are tiered before they are drawn: the room
        // shared anew, with the sets baked so far.
        if follow_copies(
            &mut builds.builds,
            &record.0,
            &mut commands,
            ground,
            &mut deps.caches,
        ) {
            builds.share(landing);
        }
    }
    // Each build done says once what its variants drew - or why not.
    for build in &mut builds.builds {
        if let Some(failures) = build.report_failures() {
            for (variant, failure) in failures {
                diagnostics.record_variant((build.placement(), variant), failure);
            }
        }
    }
    let compiling = deps.caches.job.progress().is_some();
    if builds.is_idle() {
        // Drawn, and drawn still a frame later: the forms no set draws
        // from are let go (#1649).
        if !compiling && builds.sweep.due {
            if builds.sweep.armed {
                let gone = builds.forms.forget_unused();
                builds.sweep = Sweep::default();
                if gone > 0 {
                    info!("instances: let go of {gone} forms no set draws from");
                }
            } else {
                builds.sweep.armed = true;
            }
        }
        return;
    }
    builds.sweep.armed = false;
    // The sets baked wait until the room's last is - and the compile has
    // spawned every set's anchor, so no set is still to come: then the
    // room is shared by what they all need, and every waiting set draws
    // (#1648).
    if !compiling
        && builds.builds.iter().any(build::Build::is_ready)
        && !builds.builds.iter().any(build::Build::is_growing)
    {
        let (near, far, cut) = builds.share(landing);
        let mut released = 0;
        for build in builds.builds.iter_mut().filter(|build| build.is_ready()) {
            build.release();
            released += 1;
        }
        let reach = cut.map_or_else(
            || "every copy near".to_owned(),
            |cut| format!("near within {cut:.0} m of the landing"),
        );
        info!(
            "instances: the room's budgets shared by need among {} sets ({released} new): \
             {near} of {} near entities, {far} of {} far copies, {reach}",
            builds.builds.len(),
            builds.budget.near,
            builds.budget.far
        );
    }
    let builds = &mut *builds;
    let far_material = builds
        .far_material
        .get_or_insert_with(|| materials.add(far_material()))
        .clone();
    let mut slice = build::Slice {
        started,
        budget_ms: SLICE_MS,
        templates: TEMPLATES_PER_SLICE,
        bakes: BAKES_PER_SLICE,
        copies: COPIES_PER_SLICE,
    };
    for build in &mut builds.builds {
        if slice.out_of_time() {
            return;
        }
        build.advance(
            &mut slice,
            &mut commands,
            &mut builds.forms,
            build::Stores {
                meshes: &mut meshes,
                materials: &mut materials,
                images: &mut images,
                wind_materials: wind_materials.as_deref(),
            },
            &mut deps,
            &parts,
            (&far_material, cuts.as_deref()),
        );
    }
}

/// The material every far form is drawn with: white, so each part shows
/// its vertex colour, and matte, as a building a kilometre off is.
pub(crate) fn far_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.9,
        reflectance: 0.3,
        ..default()
    }
}

/// The app the spawner's tests draw sets in, shared with the editor's
/// copy tests (#1644).
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::pds::{InstanceCopy, InstanceSolid, RoomRecord, TransformData};
    use crate::water::{WaterMaterial, WaterSurfaces};

    /// The resources the spawner reaches, as the compile's own test app has
    /// them, with no renderer; the record holding `sets` of a cuboid.
    pub(crate) fn instances_app(sets: Vec<Vec<InstanceCopy>>) -> App {
        let mut record = RoomRecord::default();
        record.generators.clear();
        record.placements.clear();
        record.traits.clear();
        record.default_landing = None;
        record
            .generators
            .insert("post".into(), Generator::default_cuboid());
        for copies in sets {
            record.placements.push(Placement::Instances {
                generator_ref: "post".into(),
                transform: TransformData::default(),
                snap_to_terrain: true,
                solid: InstanceSolid::Shell,
                variants: Vec::new(),
                copies,
            });
        }
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
        app.insert_resource(LiveRoomRecord(record));
        app.insert_resource(CurrentRoomDid("did:plc:instances".into()));
        register(&mut app);
        app
    }

    /// The anchor the compile spawns for the set `index`.
    pub(crate) fn anchor(app: &mut App, index: usize) -> Entity {
        app.world_mut()
            .spawn((
                InstanceSetRoot,
                PlacementUnit(index),
                Transform::IDENTITY,
                Visibility::default(),
            ))
            .id()
    }

    pub(crate) fn run_to_idle(app: &mut App) {
        for _ in 0..200 {
            app.update();
            if app.world().resource::<InstanceBuilds>().is_idle() {
                return;
            }
        }
        panic!("the sets are not drawn in 200 frames");
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{anchor, instances_app, run_to_idle};
    use super::*;
    use crate::pds::{InstanceCopy, InstanceVariant};
    use avian3d::prelude::Collider;

    /// Each copy of a set is drawn from its variant's merged parts where the
    /// record stands it, on its shell, every entity naming the copy; the
    /// template it was baked from is gone.
    #[test]
    fn a_set_draws_each_copy_merged_on_its_collider() {
        let copies = vec![
            InstanceCopy::at(0.0, 0.0, 0.0),
            InstanceCopy::at(5.0, 0.0, 90.0),
            InstanceCopy::at(10.0, 0.0, 180.0),
        ];
        let mut app = instances_app(vec![copies.clone()]);
        let root = anchor(&mut app, 0);
        run_to_idle(&mut app);
        let world = app.world_mut();
        let children: Vec<Entity> = world
            .get::<Children>(root)
            .expect("copies under the anchor")
            .iter()
            .collect();
        let mut drawn = Vec::new();
        let mut colliders = Vec::new();
        for child in children {
            let copy = world
                .get::<InstanceCopyOf>(child)
                .unwrap_or_else(|| panic!("{child} names no copy - a template left behind?"));
            assert_eq!(copy.placement, 0);
            if world.get::<Mesh3d>(child).is_some() {
                drawn.push((copy.copy, *world.get::<Transform>(child).expect("a pose")));
            }
            if world.get::<Collider>(child).is_some() {
                colliders.push(copy.copy);
            }
        }
        drawn.sort_by_key(|(copy, _)| *copy);
        colliders.sort();
        assert_eq!(colliders, [0, 1, 2]);
        assert_eq!(drawn.len(), 3, "one merged part each: {drawn:?}");
        for ((index, pose), copy) in drawn.iter().zip(&copies) {
            let [x, y, z] = copy.position();
            assert!(
                pose.translation.abs_diff_eq(Vec3::new(x, y, z), 1e-5),
                "copy {index}: {pose:?}"
            );
            assert!(pose.rotation.abs_diff_eq(copy.rotation(), 1e-6));
        }
        assert_eq!(world.resource::<InstanceBuilds>().forms.len(), 1);
    }

    /// Past the room's near budget a copy big enough to be seen far is its
    /// shell, one entity drawn in the far material and casting no shadow,
    /// while the far budget lasts, and past that nothing; a small copy is
    /// stamped with its draw distance.
    #[test]
    fn past_its_near_budget_a_big_copy_is_its_shell_and_then_nothing() {
        let big = |x: f32| InstanceCopy {
            scale_pm: 20_000,
            ..InstanceCopy::at(x, 0.0, 0.0)
        };
        let mut app = instances_app(vec![vec![big(0.0), big(30.0), big(60.0)]]);
        app.world_mut()
            .resource_mut::<InstanceBuilds>()
            .set_room_budget(1, 1);
        let root = anchor(&mut app, 0);
        run_to_idle(&mut app);
        let world = app.world_mut();
        let mut near = Vec::new();
        let mut far = Vec::new();
        for child in world.get::<Children>(root).expect("copies").iter() {
            let Some(copy) = world.get::<InstanceCopyOf>(child) else {
                continue;
            };
            if world.get::<Mesh3d>(child).is_none() {
                continue;
            }
            if world.get::<bevy::light::NotShadowCaster>(child).is_some() {
                far.push(copy.copy);
            } else {
                near.push(copy.copy);
            }
        }
        assert_eq!((near, far), (vec![0], vec![1]));

        let mut app = instances_app(vec![vec![InstanceCopy::at(0.0, 0.0, 0.0)]]);
        let root = anchor(&mut app, 0);
        run_to_idle(&mut app);
        let world = app.world_mut();
        let stamped = world
            .get::<Children>(root)
            .expect("a copy")
            .iter()
            .filter(|child| world.get::<Mesh3d>(*child).is_some())
            .filter_map(|child| world.get::<crate::world_builder::draw_distance::SizeClass>(child))
            .count();
        assert_eq!(stamped, 1, "a metre post is ground cover's size");
    }

    /// The entities each copy of the set under `root` drew, by its number.
    fn copies_drawn(app: &mut App, root: Entity) -> std::collections::BTreeMap<usize, Vec<Entity>> {
        let world = app.world_mut();
        let mut drawn: std::collections::BTreeMap<usize, Vec<Entity>> = Default::default();
        for child in world.get::<Children>(root).expect("copies").iter() {
            if let Some(copy) = world.get::<InstanceCopyOf>(child) {
                drawn.entry(copy.copy).or_default().push(child);
            }
        }
        drawn
    }

    fn set_copies(app: &mut App, index: usize, now: Vec<InstanceCopy>) {
        let mut record = app.world_mut().resource_mut::<LiveRoomRecord>();
        let Placement::Instances { copies, .. } = &mut record.0.placements[index] else {
            panic!("a set");
        };
        *copies = now;
    }

    /// #1644: a drawn set follows its copies in place. Deleting the first
    /// copy and moving the third keeps the second's entities - renumbered
    /// first - takes the first's away, and draws the third anew where it
    /// was moved to, nothing else.
    #[test]
    fn a_copy_moved_or_deleted_redraws_that_copy_alone() {
        let copies = vec![
            InstanceCopy::at(0.0, 0.0, 0.0),
            InstanceCopy::at(5.0, 0.0, 0.0),
            InstanceCopy::at(10.0, 0.0, 0.0),
        ];
        let mut app = instances_app(vec![copies.clone()]);
        let root = anchor(&mut app, 0);
        run_to_idle(&mut app);
        let before = copies_drawn(&mut app, root);
        assert_eq!(before.len(), 3);
        set_copies(
            &mut app,
            0,
            vec![copies[1], InstanceCopy::at(10.0, 0.0, 40.0)],
        );
        run_to_idle(&mut app);
        let after = copies_drawn(&mut app, root);
        assert_eq!(after.len(), 2, "{after:?}");
        assert_eq!(after[&0], before[&1], "the second copy kept its entities");
        assert!(
            after[&1].iter().all(|e| !before[&2].contains(e)),
            "the moved copy is drawn anew"
        );
        let world = app.world();
        for gone in before[&0].iter().chain(&before[&2]) {
            assert!(world.get_entity(*gone).is_err(), "{gone} was taken away");
        }
        let moved = after[&1]
            .iter()
            .find_map(|e| world.get::<Mesh3d>(*e).and(world.get::<Transform>(*e)))
            .expect("a drawn part");
        assert!(
            moved
                .translation
                .abs_diff_eq(Vec3::new(10.0, 0.0, 0.0), 1e-5),
            "{moved:?}"
        );
        let builds = app.world().resource::<InstanceBuilds>();
        assert_eq!(
            builds.copy_pose(0, 1).map(|p| p
                .rotation
                .abs_diff_eq(InstanceCopy::at(0.0, 0.0, 40.0).rotation(), 1e-6)),
            Some(true)
        );
    }

    /// The set `placement` of `app`'s room.
    fn build_of(app: &App, placement: usize) -> &build::Build {
        app.world()
            .resource::<InstanceBuilds>()
            .builds
            .iter()
            .find(|build| build.placement() == placement)
            .expect("the set")
    }

    /// What the room's share gave the set `placement`'s copies: near
    /// entities and far copies.
    fn budgets_of(app: &App, placement: usize) -> (u32, usize) {
        build_of(app, placement).given()
    }

    /// #1648: a set grown in place is given its share anew, by what its
    /// copies need: one copy beside a set of fifty, grown to fifty, is given
    /// what its fifty one-part copies cost, and every copy it gained is
    /// drawn; the set beside it keeps its own.
    #[test]
    fn a_set_grown_in_place_is_given_its_share_anew_by_need() {
        let mut app = instances_app(vec![
            vec![InstanceCopy::at(0.0, 0.0, 0.0)],
            vec![InstanceCopy::at(0.0, 20.0, 0.0); 50],
        ]);
        let root = anchor(&mut app, 0);
        anchor(&mut app, 1);
        run_to_idle(&mut app);
        assert_eq!(budgets_of(&app, 0), (1, 0));
        assert_eq!(budgets_of(&app, 1), (50, 0));
        let grown = (0..50)
            .map(|i| InstanceCopy::at(i as f32, 0.0, 0.0))
            .collect();
        set_copies(&mut app, 0, grown);
        run_to_idle(&mut app);
        assert_eq!(budgets_of(&app, 0), (50, 0));
        assert_eq!(budgets_of(&app, 1), (50, 0));
        assert_eq!(copies_drawn(&mut app, root).len(), 50);
    }

    /// #1648: a set baked first waits for the room's others before it draws.
    /// A set far from the landing, its one variant baked at once, and a set
    /// by the landing still growing three: shared once both are baked, the
    /// room's near budget goes to the copies by the landing, and none to the
    /// far set, whose small copies are never drawn far, however much of the
    /// far budget is left. Each set draws just what it was given.
    #[test]
    fn a_set_baked_first_waits_for_the_rooms_others() {
        let mut app = instances_app(vec![
            (0..3)
                .map(|i| InstanceCopy::at(100.0 + i as f32, 0.0, 0.0))
                .collect(),
            (0..3u32)
                .map(|v| InstanceCopy {
                    variant: v,
                    ..InstanceCopy::at(2.0 + v as f32, 0.0, 0.0)
                })
                .collect(),
        ]);
        if let Placement::Instances { variants, .. } = &mut app
            .world_mut()
            .resource_mut::<LiveRoomRecord>()
            .0
            .placements[1]
        {
            *variants = (1..=3)
                .map(|seed| InstanceVariant {
                    seed: Some(seed),
                    params: Default::default(),
                })
                .collect();
        }
        app.world_mut()
            .resource_mut::<InstanceBuilds>()
            .set_room_budget(3, 3);
        let far = anchor(&mut app, 0);
        let near = anchor(&mut app, 1);
        // The far set is baked before the near one is.
        let mut waited = false;
        for _ in 0..200 {
            app.update();
            let builds = &app.world().resource::<InstanceBuilds>().builds;
            if builds.iter().any(|build| build.is_ready())
                && builds.iter().any(|build| build.is_growing())
            {
                waited = true;
                assert!(
                    copies_drawn_or_none(&mut app, far).is_empty(),
                    "a waiting set draws nothing"
                );
            }
            if app.world().resource::<InstanceBuilds>().is_idle() {
                break;
            }
        }
        assert!(waited, "the far set was baked first, and waited");
        assert_eq!(budgets_of(&app, 1), (3, 0));
        assert_eq!(budgets_of(&app, 0), (0, 0));
        for placement in [0, 1] {
            let set = build_of(&app, placement);
            assert_eq!(set.spent(), set.given(), "set {placement} drew its share");
        }
        assert_eq!(copies_drawn_or_none(&mut app, near).len(), 3);
        assert!(copies_drawn_or_none(&mut app, far).is_empty());
    }

    /// #1648: while the compile is still spawning the room's sets, a set
    /// baked waits too - a set still to come may stand nearer the landing.
    #[test]
    fn a_set_baked_waits_while_the_compile_runs() {
        let mut app = instances_app(vec![vec![InstanceCopy::at(0.0, 0.0, 0.0)]]);
        app.insert_resource(crate::world_builder::compile::CompileJob::running());
        let root = anchor(&mut app, 0);
        for _ in 0..20 {
            app.update();
        }
        assert!(build_of(&app, 0).is_ready(), "baked, and waiting");
        assert!(copies_drawn_or_none(&mut app, root).is_empty());
        app.insert_resource(crate::world_builder::compile::CompileJob::default());
        run_to_idle(&mut app);
        assert_eq!(copies_drawn_or_none(&mut app, root).len(), 1);
    }

    /// #1648: in a room whose near budget is spent, a copy added by the
    /// landing is drawn near, as the share tiers it, though the copies
    /// drawn before keep what they drew and the room draws past its budget
    /// until it is loaded again.
    #[test]
    fn a_copy_added_by_the_landing_in_a_full_room_is_drawn_near() {
        let mut app = instances_app(vec![
            (1..=3)
                .map(|i| InstanceCopy::at(10.0 * i as f32, 0.0, 0.0))
                .collect(),
        ]);
        app.world_mut()
            .resource_mut::<InstanceBuilds>()
            .set_room_budget(3, 0);
        let root = anchor(&mut app, 0);
        run_to_idle(&mut app);
        assert_eq!(copies_drawn(&mut app, root).len(), 3, "the room is full");
        let mut grown: Vec<InstanceCopy> = (1..=3)
            .map(|i| InstanceCopy::at(10.0 * i as f32, 0.0, 0.0))
            .collect();
        grown.push(InstanceCopy::at(1.0, 0.0, 0.0));
        set_copies(&mut app, 0, grown);
        run_to_idle(&mut app);
        let drawn = copies_drawn(&mut app, root);
        assert!(drawn.contains_key(&3), "the copy by the landing: {drawn:?}");
        let world = app.world();
        assert!(
            drawn[&3].iter().any(|e| world.get::<Mesh3d>(*e).is_some()
                && world.get::<bevy::light::NotShadowCaster>(*e).is_none()),
            "drawn near"
        );
        assert_eq!(drawn.len(), 4, "and the three before keep what they drew");
    }

    /// The entities each copy of the set under `root` drew, by its number:
    /// none where it has drawn nothing yet.
    fn copies_drawn_or_none(
        app: &mut App,
        root: Entity,
    ) -> std::collections::BTreeMap<usize, Vec<Entity>> {
        if app.world().get::<Children>(root).is_none() {
            return Default::default();
        }
        copies_drawn(app, root)
    }

    /// A copy of a variant no copy drew before cannot be drawn in place -
    /// its variant was never grown - so the set is built anew, all of it.
    #[test]
    fn a_new_copy_of_a_variant_never_grown_builds_the_set_anew() {
        let mut app = instances_app(vec![vec![InstanceCopy::at(0.0, 0.0, 0.0)]]);
        if let Placement::Instances { variants, .. } = &mut app
            .world_mut()
            .resource_mut::<LiveRoomRecord>()
            .0
            .placements[0]
        {
            *variants = vec![
                InstanceVariant::default(),
                InstanceVariant {
                    seed: Some(9),
                    params: Default::default(),
                },
            ];
        }
        let root = anchor(&mut app, 0);
        run_to_idle(&mut app);
        assert_eq!(app.world().resource::<InstanceBuilds>().forms.len(), 1);
        set_copies(
            &mut app,
            0,
            vec![
                InstanceCopy::at(0.0, 0.0, 0.0),
                InstanceCopy {
                    variant: 1,
                    ..InstanceCopy::at(4.0, 0.0, 0.0)
                },
            ],
        );
        run_to_idle(&mut app);
        assert_eq!(copies_drawn(&mut app, root).len(), 2);
        assert_eq!(app.world().resource::<InstanceBuilds>().forms.len(), 2);
    }

    /// #1644: a ray from above picks the copy whose baked box it enters
    /// first, of those kept; one starting inside a box picks through it.
    #[test]
    fn a_ray_picks_the_copy_whose_box_it_enters() {
        let mut app = instances_app(vec![vec![
            InstanceCopy::at(0.0, 0.0, 0.0),
            InstanceCopy::at(5.0, 0.0, 0.0),
        ]]);
        anchor(&mut app, 0);
        run_to_idle(&mut app);
        let builds = app.world().resource::<InstanceBuilds>();
        let down_at = |x: f32, y: f32| Ray3d::new(Vec3::new(x, y, 0.0), Dir3::NEG_Y);
        let all = |_: (usize, usize)| true;
        let (picked, distance) = builds.pick(down_at(5.0, 10.0), &all).expect("a pick");
        assert_eq!(picked, (0, 1));
        assert!(distance > 8.0 && distance < 10.0, "{distance}");
        assert!(
            builds
                .pick(down_at(5.0, 10.0), &|copy| copy != (0, 1))
                .is_none()
        );
        assert!(
            builds.pick(down_at(2.5, 10.0), &all).is_none(),
            "between them"
        );
        assert!(
            builds.pick(down_at(0.0, 0.0), &all).is_none(),
            "a ray starting inside the box"
        );
    }

    /// #1644: a variant whose values break its grammar draws nothing, and
    /// says why where the set's section reads it; the variant beside it
    /// draws.
    #[test]
    fn a_variant_that_draws_nothing_says_why() {
        let mut app = instances_app(vec![vec![
            InstanceCopy::at(0.0, 0.0, 0.0),
            InstanceCopy {
                variant: 1,
                ..InstanceCopy::at(4.0, 0.0, 0.0)
            },
        ]]);
        let wide: crate::pds::Generator = serde_json::from_value(serde_json::json!({
            "$type": "network.symbios.gen.shape",
            "grammar_source": "attr W = 1\nLot --> Size(W, 1, 1) I(\"Block\")",
            "root_rule": "Lot",
            "footprint": [10_000, 10_000, 10_000],
            "seed": "5",
        }))
        .expect("a Shape node");
        {
            let mut record = app.world_mut().resource_mut::<LiveRoomRecord>();
            record.0.generators.insert("post".into(), wide);
            if let Placement::Instances { variants, .. } = &mut record.0.placements[0] {
                *variants = vec![
                    InstanceVariant::default(),
                    InstanceVariant {
                        seed: None,
                        params: crate::pds::GrammarParams(std::collections::BTreeMap::from([(
                            "W".to_string(),
                            crate::pds::Fp(-1.0),
                        )])),
                    },
                ];
            }
        }
        anchor(&mut app, 0);
        run_to_idle(&mut app);
        app.update();
        let diagnostics = app
            .world()
            .resource::<crate::world_builder::grammar_diag::GrammarDiagnostics>();
        assert_eq!(diagnostics.variant_failure(0, 0), None);
        let failure = diagnostics.variant_failure(0, 1).expect("it says why");
        assert!(failure.contains("derivation"), "{failure}");
    }

    /// #1649: a re-roll leaves nothing of the sets it replaced. Each re-roll
    /// gives the set an item drawn otherwise (as a new Kiez's every item
    /// is) and the compile a new anchor; once the room is drawn again, the
    /// forms of the item before - its merged meshes and their materials -
    /// are let go, so the room's meshes and materials stay where one item's
    /// put them, however many re-rolls.
    #[test]
    fn a_re_roll_leaves_no_forms_of_the_sets_it_replaced() {
        let mut app = instances_app(vec![vec![
            InstanceCopy::at(0.0, 0.0, 0.0),
            InstanceCopy::at(5.0, 0.0, 0.0),
        ]]);
        let mut root = anchor(&mut app, 0);
        run_to_idle(&mut app);
        let held = |app: &mut App| {
            for _ in 0..3 {
                app.update();
            }
            let world = app.world();
            (
                world.resource::<InstanceBuilds>().forms.len(),
                world.resource::<Assets<Mesh>>().len(),
                world.resource::<Assets<StandardMaterial>>().len(),
            )
        };
        let first = held(&mut app);
        assert_eq!(first.0, 1, "one item, one form");
        for roll in 1..=3 {
            {
                let mut record = app.world_mut().resource_mut::<LiveRoomRecord>();
                let post = record.0.generators.get_mut("post").expect("the item");
                post.transform.translation = crate::pds::Fp3([0.0, roll as f32, 0.0]);
            }
            app.world_mut().entity_mut(root).despawn();
            root = anchor(&mut app, 0);
            run_to_idle(&mut app);
            assert_eq!(held(&mut app), first, "after re-roll {roll}");
        }
    }

    /// #1649: a set the compile takes down and spawns again, as an edit to
    /// it does, draws from the forms it was drawn from: none is let go while
    /// the compile runs, though no set is drawn in between, so no template
    /// is grown again.
    #[test]
    fn a_set_respawned_while_the_compile_runs_bakes_nothing_again() {
        let mut app = instances_app(vec![vec![InstanceCopy::at(0.0, 0.0, 0.0)]]);
        let first = anchor(&mut app, 0);
        run_to_idle(&mut app);
        app.insert_resource(crate::world_builder::compile::CompileJob::running());
        app.world_mut().entity_mut(first).despawn();
        for _ in 0..5 {
            app.update();
        }
        assert_eq!(
            app.world().resource::<InstanceBuilds>().forms.len(),
            1,
            "kept while the compile runs"
        );
        // The compile done the frame before the new anchor shows: the room
        // idle and nothing compiling for one frame, which only arms the
        // sweep.
        app.insert_resource(crate::world_builder::compile::CompileJob::default());
        app.update();
        let second = anchor(&mut app, 0);
        app.update();
        let world = app.world_mut();
        let hidden = world.get::<Children>(second).map_or(0, |children| {
            children
                .iter()
                .filter(|child| world.get::<Visibility>(*child) == Some(&Visibility::Hidden))
                .count()
        });
        assert_eq!(hidden, 0, "no template grown again");
        run_to_idle(&mut app);
        assert_eq!(app.world().resource::<InstanceBuilds>().forms.len(), 1);
    }

    /// A second set of the same generator draws from the forms the first
    /// baked: no template is grown again, and a set whose anchor goes is
    /// forgotten.
    #[test]
    fn a_second_set_of_one_generator_bakes_nothing_again() {
        let mut app = instances_app(vec![
            vec![InstanceCopy::at(0.0, 0.0, 0.0)],
            vec![InstanceCopy::at(3.0, 0.0, 0.0); 2],
        ]);
        let first = anchor(&mut app, 0);
        run_to_idle(&mut app);
        let second = anchor(&mut app, 1);
        // The frame the second set is built, it finds its forms held and
        // spawns no hidden template under its anchor.
        app.update();
        let hidden = |app: &mut App, root: Entity| {
            let world = app.world_mut();
            world.get::<Children>(root).map_or(0, |children| {
                children
                    .iter()
                    .filter(|child| world.get::<Visibility>(*child) == Some(&Visibility::Hidden))
                    .count()
            })
        };
        assert_eq!(hidden(&mut app, second), 0);
        run_to_idle(&mut app);
        let builds = app.world().resource::<InstanceBuilds>();
        assert_eq!(builds.forms.len(), 1);
        assert_eq!(builds.builds.len(), 2);
        app.world_mut().entity_mut(first).despawn();
        app.update();
        assert_eq!(app.world().resource::<InstanceBuilds>().builds.len(), 1);
    }

    #[test]
    fn a_tree_drawn_merged_loses_its_sounds_spins_and_what_is_not_seen() {
        let hum = || SovereignAudioConfig::Patch {
            patch: Default::default(),
        };
        let turning = || {
            Some(crate::pds::Spin::about(
                [0.0, 0.0, 1.0],
                crate::pds::SpinTerm::Constant {
                    rate: crate::pds::Fp(30.0),
                },
            ))
        };
        let mut tree = Generator::default_cuboid();
        tree.audio = hum();
        tree.spin = turning();
        let mut wall = Generator::default_cuboid();
        wall.children
            .push(Generator::from_kind(GeneratorKind::default_particles()));
        wall.audio = hum();
        wall.spin = turning();
        tree.children.push(wall);
        tree.children
            .push(Generator::from_kind(GeneratorKind::default_particles()));
        strip_for_distance(&mut tree);
        assert!(tree.audio.is_none() && tree.spin.is_none());
        assert_eq!(tree.children.len(), 1, "the wall stays, the sparks go");
        assert!(tree.children[0].children.is_empty() && tree.children[0].audio.is_none());
        assert!(
            tree.children[0].spin.is_none(),
            "#1604: no part of a copy turns"
        );
    }
}
