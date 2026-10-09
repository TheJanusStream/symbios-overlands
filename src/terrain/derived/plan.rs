//! A derived plan and its spawning (#1587, #1588): the distinct buildings,
//! trees and props a plan draws, each grown once, spawned hidden as a
//! template and baked ([`super::bake`]), and the copies of them its lots,
//! footprints and inventory points stand.
//!
//! A copy is drawn near - its building's parts merged per material, a few
//! entities - while its plan's near copies stay inside the plan's entity
//! budget, and far - one entity, its building's voxel shell - past that,
//! where the plan draws far copies at all. A copy on the walkable ground
//! stands on a collider: its building's shell, its box, or a tree's trunk.
//! Every entity a copy spawns at its top carries its [`DerivedItem`], the
//! stable id of the Berlin feature it was drawn from: each merged part, a
//! far form, a collider - and the root of a copy drawn whole, its parts
//! hanging under it.

use avian3d::prelude::Collider;
use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;

use crate::catalogue::CatalogueEntry;
use crate::pds::audio::SovereignAudioConfig;
use crate::pds::{Generator, GeneratorKind};
use crate::player::visuals::AvatarSpawnDeps;
use crate::seeded_defaults::fnv1a_64;
use crate::world_builder::avatar_spawn::{detached_record, spawn_detached_tree};
use crate::world_builder::draw_distance::{DrawDistanceCuts, SizeClass};

use super::bake::NearPart;
use super::{DerivedItem, SourceId};

/// The smallest and largest a copy's height may scale its template (a
/// tree's, to the inventory's height): past these the species is the wrong
/// one, not merely a big or small example of it.
const HEIGHT_SCALE_RANGE: (f32, f32) = (0.3, 3.0);

/// The tallest a tree's trunk collider stands (m): the trunk a walker meets,
/// not the crown.
const TRUNK_M: f32 = 3.0;

/// How a plan's copies are drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Policy {
    /// The most entities the near copies may be: a near copy past it takes
    /// the far form, or is left out where the plan draws no far copies.
    pub near_entities: u32,
    /// The most far copies, one entity each; zero draws none.
    pub far_copies: usize,
    /// Whether a small copy is cut at the player's draw distance, as ground
    /// cover is (#1480): street furniture.
    pub cut: bool,
}

/// How one distinct building is grown from its catalogue entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Grow {
    /// As the lot layer grows its buildings and props: finished and ruined
    /// by the room's prosperity and escalation, drawn at its scale.
    Built,
    /// A plant, re-skinned by its named variant, its growth held to the
    /// seeded stands' per-tree budget, drawn at its catalogue size.
    Plant { variant: Option<&'static str> },
}

/// What a copy stands on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Solid {
    /// Nothing: a copy no one walks to.
    None,
    /// Its building's voxel shell: a building's.
    Shell,
    /// The box its building fills: a prop's.
    Box,
    /// A trunk of this radius (m): a tree's.
    Trunk(f32),
}

/// One distinct building of a plan: a catalogue entry at a drawn scale.
pub(crate) struct PlannedBuilding {
    pub entry: &'static dyn CatalogueEntry,
    pub scale: f32,
    pub grow: Grow,
    /// The cache key its tree's caches file under.
    pub key: String,
    /// Its generator, grown for its template.
    tree: Option<Generator>,
    /// Its template, spawned hidden to be baked, until it is.
    pub(super) template: Option<Entity>,
    /// How its near copies are drawn, once its template is baked.
    form: Option<Form>,
    /// Its far form, once baked; `None` where it has none.
    far: Option<Handle<Mesh>>,
    /// The box its template filled, once baked.
    bounds: Option<(Vec3, Vec3)>,
    /// Its shell as a collider, once baked, where it has a far form.
    shell: Option<Collider>,
}

impl PlannedBuilding {
    pub(crate) fn new(
        entry: &'static dyn CatalogueEntry,
        scale: f32,
        grow: Grow,
        key: String,
    ) -> Self {
        PlannedBuilding {
            entry,
            scale,
            grow,
            key,
            tree: None,
            template: None,
            form: None,
            far: None,
            bounds: None,
            shell: None,
        }
    }
}

/// How a building's near copies are drawn.
pub(super) enum Form {
    /// One mesh per material and sway, in the copy's frame.
    Merged(Vec<NearPart<Handle<Mesh>>>),
    /// Spawned whole from its tree, each copy costing what its first did.
    Whole(Option<u32>),
}

/// One copy of a plan's building.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlannedCopy {
    /// Which of the plan's buildings.
    pub building: usize,
    /// Where it stands on the ground, and how it is turned.
    pub pose: Transform,
    /// Whether it is drawn near while the plan's budget lasts.
    pub near: bool,
    /// The Berlin feature it is drawn from.
    pub source: SourceId,
    /// The height it is drawn at (m), its building scaled to it; `None`
    /// draws it as grown.
    pub height: Option<f32>,
    pub solid: Solid,
}

/// What a plan draws, nearest first.
pub(crate) struct Plan {
    /// What it is, for the log and its root's name.
    pub label: &'static str,
    pub policy: Policy,
    pub buildings: Vec<PlannedBuilding>,
    pub copies: Vec<PlannedCopy>,
    /// What the buildings are grown for: the room's DID, its prosperity and
    /// escalation, and the plan's seed.
    pub did: String,
    pub character: (f32, f32),
    pub seed: u64,
}

impl Plan {
    /// The generator of building `b`, grown on first asking, stripped for
    /// distance.
    pub(super) fn tree(&mut self, b: usize) -> &Generator {
        let (did, character, seed) = (&self.did, self.character, self.seed);
        let building = &mut self.buildings[b];
        building.tree.get_or_insert_with(|| {
            let slug = building.entry.slug();
            let mut tree = match building.grow {
                Grow::Built => crate::terrain::lots::grow_generator(
                    building.entry,
                    did,
                    seed ^ fnv1a_64(slug),
                    character,
                    building.scale,
                ),
                Grow::Plant { variant } => grow_plant(building.entry, did, variant),
            };
            strip_for_distance(&mut tree);
            tree
        })
    }
}

/// A plant as the seeded stands grow one: its catalogue tree, re-skinned by
/// `variant`, its L-system's growth stepped down until its expansion fits
/// the stands' per-tree budget.
fn grow_plant(entry: &dyn CatalogueEntry, did: &str, variant: Option<&'static str>) -> Generator {
    let mut tree = entry.build(did);
    if let GeneratorKind::LSystem {
        materials,
        source_code,
        finalization_code,
        iterations,
        seed,
        angle,
        step,
        width,
        elasticity,
        tropism,
        ..
    } = &mut tree.kind
    {
        if let Some(variant) = variant {
            crate::catalogue::items::plants::variant::apply_named(
                entry.variants(),
                variant,
                materials,
            );
        }
        let budget = crate::seeded_defaults::room::build::TREE_ENTITY_BUDGET;
        while *iterations > 2
            && crate::world_builder::lsystem::lsystem_entity_estimate(
                source_code,
                finalization_code,
                *iterations,
                *seed,
                *angle,
                *step,
                *width,
                *elasticity,
                *tropism,
                entry.slug(),
            )
            .is_some_and(|entities| entities > budget)
        {
            *iterations -= 1;
        }
    }
    tree
}

/// Strip from a derived tree what it only costs: every sound, and every
/// node that is no geometry to see - particles, signs, portals, gateways,
/// and any water, terrain or road a tree should not carry.
pub(crate) fn strip_for_distance(tree: &mut Generator) {
    tree.audio = SovereignAudioConfig::None;
    tree.children.retain(|child| kept_afar(&child.kind));
    for child in &mut tree.children {
        strip_for_distance(child);
    }
}

/// Whether a node of this kind stays on a derived building.
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

/// Where a [`Build`] has got to, each with the next index it works on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stage {
    Templates(usize),
    Merging(usize),
    Copies(usize),
}

/// A plan being spawned.
pub(crate) struct Build {
    pub(super) plan: Plan,
    /// The root its copies hang under, a child of the terrain.
    pub(super) root: Entity,
    pub(super) stage: Stage,
    /// The entities its near copies have spawned so far.
    pub(super) spawned: u32,
    /// The far copies it has spawned so far.
    pub(super) far_spawned: usize,
}

impl Build {
    pub(super) fn new(plan: Plan, root: Entity) -> Self {
        Build {
            plan,
            root,
            stage: Stage::Templates(0),
            spawned: 0,
            far_spawned: 0,
        }
    }

    /// Bake building `b` from its template, and despawn the template.
    pub(super) fn bake(
        &mut self,
        b: usize,
        commands: &mut Commands,
        parts: &super::bake::PartQuery,
        meshes: &mut Assets<Mesh>,
        (materials, wind_materials): (
            &Assets<StandardMaterial>,
            Option<&Assets<crate::wind::VegetationWindMaterial>>,
        ),
    ) {
        let building = &mut self.plan.buildings[b];
        let template = building.template.take();
        let baked = template
            .and_then(|t| super::bake::merge_template(t, parts, meshes, materials, wind_materials));
        let (near, far, bounds) = baked.map_or((None, None, None), |baked| {
            (baked.near, baked.far, baked.bounds)
        });
        // Drawn, never read again: no CPU copy.
        let mut add = |mut mesh: Mesh| {
            mesh.asset_usage = bevy::asset::RenderAssetUsages::RENDER_WORLD;
            meshes.add(mesh)
        };
        building.form = Some(match near {
            Some(near) => Form::Merged(
                near.into_iter()
                    .map(|(mesh, material, sway)| (add(mesh), material, sway))
                    .collect(),
            ),
            None => {
                warn!(
                    "derived {}: {} is drawn whole near, its parts not merged",
                    self.plan.label, building.key
                );
                Form::Whole(None)
            }
        });
        building.shell = far.as_ref().and_then(Collider::trimesh_from_mesh);
        building.far = far.map(&mut add);
        building.bounds = bounds;
        if let Some(template) = template {
            commands.entity(template).despawn();
        }
    }

    /// Spawn copy `c`: near while the budget lasts, else far where the plan
    /// draws far copies, and its collider where it drew anything. Answers
    /// whether the copy was drawn.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn spawn_copy(
        &mut self,
        c: usize,
        commands: &mut Commands,
        assets: (
            &mut Assets<Mesh>,
            &mut Assets<StandardMaterial>,
            &mut Assets<Image>,
        ),
        deps: &mut AvatarSpawnDeps,
        far_material: &Handle<StandardMaterial>,
        cuts: Option<&DrawDistanceCuts>,
    ) -> bool {
        let copy = self.plan.copies[c].clone();
        let (root, policy) = (self.root, self.plan.policy);
        let building = &self.plan.buildings[copy.building];
        let scale = copy
            .height
            .zip(building.bounds)
            .map_or(1.0, |(height, (lo, hi))| {
                (height / (hi.y - lo.y).max(0.1)).clamp(HEIGHT_SCALE_RANGE.0, HEIGHT_SCALE_RANGE.1)
            });
        let pose = copy.pose.with_scale(Vec3::splat(scale));
        let tag = DerivedItem(copy.source.clone());
        // The draw distance a small copy is cut at, where its plan cuts.
        let cut = building
            .bounds
            .filter(|_| policy.cut)
            .and_then(|(lo, hi)| SizeClass::of((hi - lo).max_element() * scale))
            .map(|class| (class, cuts.and_then(|cuts| cuts.range(class))));
        let near_cost = match &building.form {
            Some(Form::Merged(merged)) => merged.len() as u32,
            Some(Form::Whole(cost)) => cost.unwrap_or(0),
            None => u32::MAX,
        };
        let drawn = if copy.near && self.spawned.saturating_add(near_cost) <= policy.near_entities {
            let spent = match &building.form {
                Some(Form::Merged(merged)) => {
                    for (mesh, material, sway) in merged {
                        let mut part = commands.spawn((
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(material.clone()),
                            pose,
                            tag.clone(),
                            ChildOf(root),
                        ));
                        if let Some(sway) = sway {
                            part.insert(*sway);
                        }
                        stamp(&mut part, cut.clone());
                    }
                    near_cost
                }
                _ => {
                    let (spawned_root, spent) = spawn_whole(
                        commands,
                        &mut self.plan,
                        copy.building,
                        (root, pose),
                        assets,
                        deps,
                    );
                    if let Some(spawned_root) = spawned_root {
                        commands.entity(spawned_root).insert(tag.clone());
                    }
                    self.plan.buildings[copy.building].form = Some(Form::Whole(Some(spent)));
                    spent
                }
            };
            self.spawned += spent;
            true
        } else if self.far_spawned < policy.far_copies
            && let Some(far) = self.plan.buildings[copy.building].far.clone()
        {
            let mut part = commands.spawn((
                Mesh3d(far),
                MeshMaterial3d(far_material.clone()),
                pose,
                bevy::light::NotShadowCaster,
                tag.clone(),
                ChildOf(root),
            ));
            stamp(&mut part, cut);
            self.far_spawned += 1;
            true
        } else {
            false
        };
        if drawn && let Some((collider, at)) = self.collider(&copy, pose) {
            commands.spawn((collider, at, tag, ChildOf(root)));
        }
        drawn
    }

    /// The collider `copy` stands on, and where, at `pose`.
    fn collider(&self, copy: &PlannedCopy, pose: Transform) -> Option<(Collider, Transform)> {
        let building = &self.plan.buildings[copy.building];
        match copy.solid {
            Solid::None => None,
            Solid::Shell => building.shell.clone().map(|shell| (shell, pose)),
            Solid::Box => {
                let (lo, hi) = building.bounds?;
                let size = (hi - lo).max(Vec3::splat(0.05));
                let centre = pose.transform_point((lo + hi) / 2.0);
                Some((
                    Collider::cuboid(size.x, size.y, size.z),
                    Transform::from_translation(centre)
                        .with_rotation(pose.rotation)
                        .with_scale(pose.scale),
                ))
            }
            Solid::Trunk(radius) => {
                let tall = building
                    .bounds
                    .map_or(TRUNK_M, |(lo, hi)| (hi.y - lo.y) * pose.scale.y)
                    .min(TRUNK_M);
                Some((
                    Collider::cylinder(radius, tall),
                    Transform::from_translation(pose.translation + Vec3::Y * (tall / 2.0)),
                ))
            }
        }
    }
}

/// Stamp a part with the draw distance it is cut at, where it is.
fn stamp(part: &mut EntityCommands, cut: Option<(SizeClass, Option<VisibilityRange>)>) {
    if let Some((class, range)) = cut {
        part.insert(class);
        if let Some(range) = range {
            part.insert(range);
        }
    }
}

/// The material every far form is drawn with: white, so each part shows
/// its vertex colour, and matte, as a city a kilometre off is.
pub(super) fn far_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.9,
        reflectance: 0.3,
        ..default()
    }
}

/// Spawn building `b` of `plan` whole from its tree, under `parent` at
/// `pose`: its template, or a copy that could not be merged. Answers the
/// tree's root and how many entities it spawned.
pub(super) fn spawn_whole(
    commands: &mut Commands,
    plan: &mut Plan,
    b: usize,
    (parent, pose): (Entity, Transform),
    (meshes, materials, images): (
        &mut Assets<Mesh>,
        &mut Assets<StandardMaterial>,
        &mut Assets<Image>,
    ),
    deps: &mut AvatarSpawnDeps,
) -> (Option<Entity>, u32) {
    let key = plan.buildings[b].key.clone();
    let tree = plan.tree(b);
    spawn_detached_tree(
        commands,
        parent,
        tree,
        &key,
        pose * Transform::from(&tree.transform),
        meshes,
        materials,
        deps.water_materials.as_mut(),
        images,
        deps.palette.as_deref(),
        deps.heightmap.as_deref(),
        &deps.terrain_meshes,
        &mut deps.caches,
        deps.blob_image_cache.as_mut(),
        deps.blob_audio_cache.as_mut(),
        deps.water_surfaces.as_mut(),
        detached_record(),
        deps.current_room.as_deref(),
        false,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_derived_building_loses_its_sounds_and_what_is_not_seen() {
        let hum = || SovereignAudioConfig::Patch {
            patch: Default::default(),
        };
        let mut tree = Generator::default_cuboid();
        tree.audio = hum();
        let mut wall = Generator::default_cuboid();
        wall.children
            .push(Generator::from_kind(GeneratorKind::default_particles()));
        wall.audio = hum();
        tree.children.push(wall);
        tree.children
            .push(Generator::from_kind(GeneratorKind::default_particles()));
        strip_for_distance(&mut tree);
        assert!(tree.audio.is_none());
        assert_eq!(tree.children.len(), 1, "the wall stays, the sparks go");
        assert!(tree.children[0].children.is_empty() && tree.children[0].audio.is_none());
    }

    /// A plant is grown as the seeded stands grow one: re-skinned by its
    /// variant, and its growth stepped down until it fits their budget.
    #[test]
    fn a_plant_is_grown_within_the_stands_budget() {
        let entry = crate::catalogue::by_slug("lsys_monopodial_tree").expect("registered");
        let plain = grow_plant(entry, "did:plc:plant", None);
        let pine = grow_plant(entry, "did:plc:plant", Some("pine"));
        let (
            GeneratorKind::LSystem {
                materials: plain_materials,
                ..
            },
            GeneratorKind::LSystem {
                materials,
                source_code,
                finalization_code,
                iterations,
                seed,
                angle,
                step,
                width,
                elasticity,
                tropism,
                ..
            },
        ) = (&plain.kind, &pine.kind)
        else {
            panic!("an L-system");
        };
        assert_ne!(plain_materials, materials, "the pine re-skins it");
        let entities = crate::world_builder::lsystem::lsystem_entity_estimate(
            source_code,
            finalization_code,
            *iterations,
            *seed,
            *angle,
            *step,
            *width,
            *elasticity,
            *tropism,
            entry.slug(),
        );
        assert!(
            *iterations == 2
                || entities
                    .is_some_and(|n| n <= crate::seeded_defaults::room::build::TREE_ENTITY_BUDGET),
            "{iterations} iterations, {entities:?} entities"
        );
    }
}
