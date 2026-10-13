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
//!    grows nothing again.
//! 3. **Copies**: each copy near, as its variant's merged parts (a few
//!    entities), while the set's share of the room's near budget lasts;
//!    past it far, as one entity, its shell, where it is big enough to be
//!    seen far; and on its collider ([`crate::pds::InstanceSolid`]) where
//!    it is drawn. A small copy is cut at the player's draw distance, as a
//!    scattered one is (#1480).
//!
//! A set draws its generator's geometry and nothing else: its sounds,
//! spins, particles, signs, portals and gateways are left out
//! ([`strip_for_distance`]), as a merged part has no transform of its own
//! to turn or emit from. Those are placed as placements of their own.
//!
//! The room's budget is the derived stage's summed - what a Kiez of
//! Berlin's buildings, trees and street furniture was drawn with - and is
//! shared among the room's sets by their copies, each set's share set when
//! it is built: a set built before another grew keeps the share it had
//! until it is built again, so for a while the room's sets together may
//! draw past the budget, and a peer that loads the room fresh shares it
//! anew.

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
/// derived stage's buildings, trees and furniture budgets summed (12,000,
/// 6,000 and 4,000).
pub(crate) const ROOM_NEAR_ENTITIES: u32 = 22_000;

/// The most far copies, one entity each, all a room's sets may draw: the
/// derived stage's buildings and trees far budgets summed (3,000 and 2,000;
/// its furniture drew none far).
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

/// The sets being drawn, and the forms their variants were baked into.
#[derive(Resource, Default)]
pub(crate) struct InstanceBuilds {
    builds: Vec<build::Build>,
    forms: forms::Forms,
    far_material: Option<Handle<StandardMaterial>>,
}

impl InstanceBuilds {
    /// Whether every set the room has spawned is drawn: what a renderer
    /// waits for before it takes its picture.
    pub(crate) fn is_idle(&self) -> bool {
        self.builds.iter().all(build::Build::is_done)
    }
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
    app.init_resource::<InstanceBuilds>().add_systems(
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
    builds.builds.retain(|build| roots.contains(build.root()));
    if added.is_empty() {
        return;
    }
    let record = &record.0;
    let copies_in_room: usize = record
        .placements
        .iter()
        .map(|placement| match placement {
            Placement::Instances { copies, .. } => copies.len(),
            _ => 0,
        })
        .sum();
    for (root, unit, anchor) in &added {
        let Some(placement) = record.placements.get(unit.0) else {
            continue;
        };
        let Some(build) = build::Build::new(
            record,
            (unit.0, placement),
            (root, *anchor),
            heightmap.as_deref().map(|heightmap| &heightmap.0),
            copies_in_room,
        ) else {
            continue;
        };
        builds.builds.push(build);
    }
}

/// Advance the builds, a slice a frame (see the module docs).
#[allow(clippy::too_many_arguments)]
fn spawn_instances(
    mut commands: Commands,
    mut builds: ResMut<InstanceBuilds>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut deps: AvatarSpawnDeps,
    parts: bake::PartQuery,
    cuts: Option<Res<DrawDistanceCuts>>,
    wind_materials: Option<Res<Assets<crate::wind::VegetationWindMaterial>>>,
) {
    let started = Instant::now();
    if builds.is_idle() {
        return;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{InstanceCopy, InstanceSolid, RoomRecord, TransformData};
    use crate::water::{WaterMaterial, WaterSurfaces};
    use avian3d::prelude::Collider;

    /// The resources the spawner reaches, as the compile's own test app has
    /// them, with no renderer; the record holding `sets` of a cuboid.
    fn instances_app(sets: Vec<Vec<InstanceCopy>>) -> App {
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
    fn anchor(app: &mut App, index: usize) -> Entity {
        app.world_mut()
            .spawn((
                InstanceSetRoot,
                PlacementUnit(index),
                Transform::IDENTITY,
                Visibility::default(),
            ))
            .id()
    }

    fn run_to_idle(app: &mut App) {
        for _ in 0..200 {
            app.update();
            if app.world().resource::<InstanceBuilds>().is_idle() {
                return;
            }
        }
        panic!("the sets are not drawn in 200 frames");
    }

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

    /// Past its near budget a copy big enough to be seen far is its shell,
    /// one entity drawn in the far material and casting no shadow, while
    /// the far budget lasts, and past that nothing; a small copy is stamped
    /// with its draw distance.
    #[test]
    fn past_its_near_budget_a_big_copy_is_its_shell_and_then_nothing() {
        let big = |x: f32| InstanceCopy {
            scale_pm: 20_000,
            ..InstanceCopy::at(x, 0.0, 0.0)
        };
        let mut app = instances_app(vec![vec![big(0.0), big(30.0), big(60.0)]]);
        let root = anchor(&mut app, 0);
        app.update();
        app.world_mut()
            .resource_mut::<InstanceBuilds>()
            .builds
            .iter_mut()
            .for_each(|build| build.set_budgets(1, 1));
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
