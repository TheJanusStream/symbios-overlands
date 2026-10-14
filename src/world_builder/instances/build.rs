//! One instance set being drawn (#1643): its copies stood where the record
//! puts them and ordered nearest the landing first, its variants' templates
//! grown and baked, and its copies spawned (see the module docs).
//!
//! A drawn set follows its record's copies without being drawn again
//! (#1644): when they change, each copy is matched to one of the same
//! numbers it drew before, which keeps its entities - renumbered where a
//! copy before it went - and the rest are taken away or queued to be
//! drawn, so moving or deleting one copy redraws that copy alone.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use avian3d::prelude::Collider;
use bevy::camera::visibility::VisibilityRange;
use bevy::ecs::system::EntityCommands;
use bevy::platform::time::Instant;
use bevy::prelude::*;

use crate::pds::{Generator, InstanceCopy, InstanceSolid, InstanceVariant, Placement, RoomRecord};
use crate::player::visuals::AvatarSpawnDeps;
use crate::world_builder::avatar_spawn::{detached_record, spawn_detached_tree};
use crate::world_builder::draw_distance::{DrawDistanceCuts, SizeClass};

use super::bake::PartQuery;
use super::forms::{Form, Forms, form_key};
use super::{InstanceCopyOf, ROOM_FAR_COPIES, ROOM_NEAR_ENTITIES, strip_for_distance};

/// The tallest a trunk collider stands (m): the trunk a walker meets, not
/// the crown.
const TRUNK_M: f32 = 3.0;

/// The narrowest and widest a trunk collider is (m): a street lamp's post
/// to an old oak's trunk.
const TRUNK_RANGE_M: (f32, f32) = (0.08, 0.8);

/// The key the caches file a template's nodes under: no record generator's
/// name holds a `/`, so none is ever filed there.
fn template_key(form: u64) -> String {
    format!("instance/{form:016x}")
}

/// Drop what growing the template keyed `root` left in the generator
/// caches, once its forms hold what it drew: otherwise every variant's own
/// terminals would stay pinned until the room's next full compile.
fn forget_template(caches: &mut crate::world_builder::compile::GeneratorCaches, root: &str) {
    let under = |node: &str| {
        node.strip_prefix(root)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    };
    caches.shape_mesh.forget_under(root);
    caches.shape_material.forget_where(|(node, _)| under(node));
    caches.lsystem_mesh.forget_where(|node| under(node));
    caches
        .lsystem_material
        .forget_where(|(node, _)| under(node));
}

/// This frame's allowance, shared by every build: a wall-clock budget and
/// the most steps of each stage.
pub(super) struct Slice {
    pub started: Instant,
    pub budget_ms: f64,
    pub templates: usize,
    pub bakes: usize,
    pub copies: usize,
}

impl Slice {
    /// Whether the frame's time is spent.
    pub(super) fn out_of_time(&self) -> bool {
        self.started.elapsed().as_secs_f64() * 1_000.0 >= self.budget_ms
    }
}

/// The asset stores a build draws into.
pub(super) struct Stores<'a> {
    pub meshes: &'a mut Assets<Mesh>,
    pub materials: &'a mut Assets<StandardMaterial>,
    pub images: &'a mut Assets<Image>,
    pub wind_materials: Option<&'a Assets<crate::wind::VegetationWindMaterial>>,
}

/// Where a build has got to, each stage with the next index it works on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Templates(usize),
    Baking(usize),
    Copies(usize),
    Done,
}

/// One variant of the set: what its grammars draw with, the key its forms
/// are kept under, its template while it waits to be baked, and its forms.
struct Slot {
    variant: InstanceVariant,
    key: u64,
    /// Whether any copy draws it: one no copy draws is never grown.
    drawn: bool,
    template: Option<Entity>,
    form: Option<Arc<Form>>,
    /// What one copy of it drawn whole costs, once one has been.
    whole_cost: Option<u32>,
}

/// One copy as it is drawn: its index among the record's copies, its pose
/// under the anchor, and its variant.
#[derive(Clone, Copy)]
struct Stood {
    index: usize,
    pose: Transform,
    variant: usize,
}

/// What one drawn copy cost its set's budgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cost {
    /// Near, this many entities.
    Near(u32),
    /// Far: one of the set's far copies.
    Far,
}

/// One copy as drawn: where it stands under the anchor, its variant, every
/// entity it spawned and what it cost.
pub(super) struct DrawnCopy {
    pub pose: Transform,
    pub variant: usize,
    pub entities: Vec<Entity>,
    cost: Cost,
}

/// What a set's copies changing did to its build.
pub(super) enum Followed {
    /// The copies are the ones it draws, or it now draws them.
    Done,
    /// It cannot follow them in place - it is still being drawn, or a copy
    /// draws a variant it never grew - and is to be built again.
    Rebuild,
}

/// A copy stood where the record puts it: its pose under the anchor, and
/// where it stands in the world. On the ground under it, where the set
/// snaps, raised by its own `y`.
fn stand(
    anchor: &Transform,
    snap: bool,
    ground: Option<&bevy_symbios_ground::HeightMap>,
    copy: &InstanceCopy,
) -> (Transform, Vec3) {
    let [x, y, z] = copy.position();
    let on_plan = anchor.translation + anchor.rotation * Vec3::new(x, 0.0, z);
    let world_y = match (snap, ground) {
        (true, Some(ground)) => {
            crate::world_builder::snapped_ground_y(ground, on_plan.x, on_plan.z, None) + y
        }
        (true, None) => y,
        (false, _) => (anchor.translation + anchor.rotation * Vec3::new(x, y, z)).y,
    };
    let world = Vec3::new(on_plan.x, world_y, on_plan.z);
    let pose = Transform {
        translation: anchor.rotation.inverse() * (world - anchor.translation),
        rotation: copy.rotation(),
        scale: Vec3::splat(copy.scale()),
    };
    (pose, world)
}

/// Why a variant of `tree` draws nothing (#1644): the first of its Shape
/// nodes whose grammar, with the variant's seed and values, refuses or
/// derives nothing - or, with none, that it draws nothing.
fn why_nothing(tree: &Generator, variant: &InstanceVariant) -> String {
    let mut nodes = vec![tree];
    while let Some(node) = nodes.pop() {
        nodes.extend(node.children.iter().rev());
        if let Some(def) = node.kind.shape_def()
            && let Err(underived) = def.variant(variant.seed, Some(&variant.params)).derive()
        {
            return underived.message().to_owned();
        }
    }
    "it draws nothing".to_owned()
}

/// What a set is drawn from, but its copies: a change here is a change the
/// compile builds the set's unit anew for (its fingerprint), not one the
/// build follows in place.
fn unit_key(record: &RoomRecord, placement: &Placement) -> u64 {
    let unit = match placement {
        Placement::Instances {
            generator_ref,
            transform,
            snap_to_terrain,
            solid,
            variants,
            ..
        } => serde_json::json!({
            "generator": record.generators.get(generator_ref),
            "generator_ref": generator_ref,
            "transform": transform,
            "snap": snap_to_terrain,
            "solid": solid,
            "variants": variants,
        }),
        _ => serde_json::Value::Null,
    };
    crate::seeded_defaults::fnv1a_64(&unit.to_string())
}

/// A set's share of the room's budgets, by its `copies` among
/// `copies_in_room`: its near entities and its far copies.
fn share_of(copies: usize, copies_in_room: usize) -> (u32, usize) {
    let share = copies as f64 / copies_in_room.max(copies).max(1) as f64;
    (
        (f64::from(ROOM_NEAR_ENTITIES) * share) as u32,
        (ROOM_FAR_COPIES as f64 * share) as usize,
    )
}

/// One set being drawn, or drawn.
pub(super) struct Build {
    /// The anchor the compile spawned, which every copy hangs under.
    root: Entity,
    /// Its placement's index, which every copy's entities carry.
    placement: usize,
    generator_ref: String,
    /// The set's generator, stripped for drawing merged.
    tree: Generator,
    slots: Vec<Slot>,
    /// The copies still to be drawn, nearest the landing first.
    copies: Vec<Stood>,
    /// The record's copies as drawn, and each one's entities by its index.
    record_copies: Vec<InstanceCopy>,
    drawn: Vec<Option<DrawnCopy>>,
    /// Where the compile stood the anchor, and whether the set snaps: what
    /// a copy it is given later is stood by.
    anchor: Transform,
    snap: bool,
    /// What the set is drawn from but its copies ([`unit_key`]).
    unit: u64,
    solid: InstanceSolid,
    /// Its share of the room's budgets.
    near_budget: u32,
    far_budget: usize,
    near_spent: u32,
    far_spent: usize,
    stage: Stage,
    /// Whether its first drawing has been told to the log.
    announced: bool,
    /// Whether its variants' failures have been reported.
    reported: bool,
}

impl Build {
    /// The build of the set `placement`, the `index`th of `record`'s, under
    /// the anchor `root` the compile stood at `anchor`: `None` where it is
    /// not a set, or its generator is missing or cannot be written. Each
    /// copy is stood on `ground` where the set snaps, and the set's budgets
    /// are its share of the room's by its copies among `copies_in_room`.
    pub(super) fn new(
        record: &RoomRecord,
        (index, placement): (usize, &Placement),
        (root, anchor): (Entity, Transform),
        ground: Option<&bevy_symbios_ground::HeightMap>,
        copies_in_room: usize,
    ) -> Option<Self> {
        let Placement::Instances {
            generator_ref,
            snap_to_terrain,
            solid,
            variants,
            copies,
            ..
        } = placement
        else {
            return None;
        };
        let mut tree = record.generators.get(generator_ref)?.clone();
        strip_for_distance(&mut tree);
        let tree_json = serde_json::to_string(&tree).ok()?;
        let variants: Vec<InstanceVariant> = if variants.is_empty() {
            vec![InstanceVariant::default()]
        } else {
            variants.clone()
        };
        let mut slots: Vec<Slot> = variants
            .into_iter()
            .map(|variant| Slot {
                key: form_key(&tree_json, &variant),
                variant,
                drawn: false,
                template: None,
                form: None,
                whole_cost: None,
            })
            .collect();
        let landing = record
            .default_landing
            .map_or(Vec2::ZERO, |landing| Vec2::from_array(landing.pos.0));
        let mut stood: Vec<(f32, Stood)> = copies
            .iter()
            .enumerate()
            .filter_map(|(index, copy)| {
                let variant = usize::try_from(copy.variant).ok()?;
                let slot = slots.get_mut(variant)?;
                slot.drawn = true;
                let (pose, world) = stand(&anchor, *snap_to_terrain, ground, copy);
                let distance = (Vec2::new(world.x, world.z) - landing).length_squared();
                Some((
                    distance,
                    Stood {
                        index,
                        pose,
                        variant,
                    },
                ))
            })
            .collect();
        // Stable: copies as far from the landing keep the record's order,
        // so every peer draws the same ones near.
        stood.sort_by(|(a, _), (b, _)| a.total_cmp(b));
        let (near_budget, far_budget) = share_of(copies.len(), copies_in_room);
        Some(Build {
            root,
            placement: index,
            generator_ref: generator_ref.clone(),
            tree,
            slots,
            copies: stood.into_iter().map(|(_, stood)| stood).collect(),
            record_copies: copies.clone(),
            drawn: copies.iter().map(|_| None).collect(),
            anchor,
            snap: *snap_to_terrain,
            unit: unit_key(record, placement),
            solid: solid.clone(),
            near_budget,
            far_budget,
            near_spent: 0,
            far_spent: 0,
            stage: if copies.is_empty() {
                Stage::Done
            } else {
                Stage::Templates(0)
            },
            announced: false,
            reported: false,
        })
    }

    /// The anchor it hangs under.
    pub(super) fn root(&self) -> Entity {
        self.root
    }

    /// Set its budgets, for a test of what lies past them.
    #[cfg(test)]
    pub(super) fn set_budgets(&mut self, near_entities: u32, far_copies: usize) {
        self.near_budget = near_entities;
        self.far_budget = far_copies;
    }

    /// Its budgets: near entities and far copies.
    #[cfg(test)]
    pub(super) fn budgets(&self) -> (u32, usize) {
        (self.near_budget, self.far_budget)
    }

    /// Whether every copy it draws is drawn.
    pub(super) fn is_done(&self) -> bool {
        self.stage == Stage::Done
    }

    /// Take this frame's steps, as far as `slice` allows.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn advance(
        &mut self,
        slice: &mut Slice,
        commands: &mut Commands,
        forms: &mut Forms,
        stores: Stores,
        deps: &mut AvatarSpawnDeps,
        parts: &PartQuery,
        (far_material, cuts): (&Handle<StandardMaterial>, Option<&DrawDistanceCuts>),
    ) {
        let Stores {
            meshes,
            materials,
            images,
            wind_materials,
        } = stores;
        loop {
            match self.stage {
                Stage::Templates(v) if v < self.slots.len() => {
                    let slot = &mut self.slots[v];
                    if !slot.drawn || slot.form.is_some() {
                        self.stage = Stage::Templates(v + 1);
                        continue;
                    }
                    if let Some(form) = forms.get(slot.key) {
                        slot.form = Some(form);
                        self.stage = Stage::Templates(v + 1);
                        continue;
                    }
                    if slice.out_of_time() || slice.templates == 0 {
                        return;
                    }
                    slice.templates -= 1;
                    let template = commands
                        .spawn((Transform::IDENTITY, Visibility::Hidden, ChildOf(self.root)))
                        .id();
                    spawn_detached_tree(
                        commands,
                        template,
                        &self.tree,
                        &template_key(slot.key),
                        Transform::from(&self.tree.transform),
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
                        Some(&slot.variant),
                    );
                    slot.template = Some(template);
                    self.stage = Stage::Templates(v + 1);
                }
                Stage::Templates(_) => {
                    self.stage = Stage::Baking(0);
                    // A template spawned is baked from the next frame on,
                    // once its parts are in the world.
                    if self.slots.iter().any(|slot| slot.template.is_some()) {
                        return;
                    }
                }
                Stage::Baking(v) if v < self.slots.len() => {
                    let Some(template) = self.slots[v].template else {
                        self.stage = Stage::Baking(v + 1);
                        continue;
                    };
                    if slice.out_of_time() || slice.bakes == 0 {
                        return;
                    }
                    slice.bakes -= 1;
                    let baked = super::bake::merge_template(
                        template,
                        parts,
                        meshes,
                        materials,
                        wind_materials,
                    );
                    let mut form = Form::from_baked(baked, meshes);
                    if !form.draws() {
                        form.failure = Some(why_nothing(&self.tree, &self.slots[v].variant));
                    }
                    let slot = &mut self.slots[v];
                    // Merged, its forms hold all it draws; drawn whole, its
                    // copies are grown from the caches each time.
                    if form.near.is_some() {
                        forget_template(&mut deps.caches, &template_key(slot.key));
                    }
                    if let Some(failure) = &form.failure {
                        warn!(
                            "instances #{} ({}): variant {v} draws nothing: {failure}",
                            self.placement, self.generator_ref
                        );
                    }
                    slot.form = Some(forms.insert(slot.key, form));
                    slot.template = None;
                    commands.entity(template).despawn();
                    self.stage = Stage::Baking(v + 1);
                }
                Stage::Baking(_) => self.stage = Stage::Copies(0),
                Stage::Copies(c) if c < self.copies.len() => {
                    if slice.out_of_time() || slice.copies == 0 {
                        return;
                    }
                    slice.copies -= 1;
                    let stood = self.copies[c];
                    let drawn = self.spawn_copy(
                        stood,
                        commands,
                        (&mut *meshes, &mut *materials, &mut *images),
                        deps,
                        far_material,
                        cuts,
                    );
                    if let Some(slot) = self.drawn.get_mut(stood.index) {
                        *slot = drawn;
                    }
                    self.stage = Stage::Copies(c + 1);
                }
                Stage::Copies(_) => {
                    if !self.announced {
                        info!(
                            "instances #{} ({}): {} near entities, {} far copies, of {} copies from {} variants",
                            self.placement,
                            self.generator_ref,
                            self.near_spent,
                            self.far_spent,
                            self.record_copies.len(),
                            self.slots.iter().filter(|slot| slot.drawn).count()
                        );
                        self.announced = true;
                    }
                    self.copies.clear();
                    self.stage = Stage::Done;
                }
                Stage::Done => return,
            }
        }
    }

    /// Spawn the copy `stood`: near while the set's budget lasts, else far
    /// where it is big enough to be seen far and the far budget lasts, and
    /// its collider where it drew anything. What it drew, or `None`.
    fn spawn_copy(
        &mut self,
        stood: Stood,
        commands: &mut Commands,
        (meshes, materials, images): (
            &mut Assets<Mesh>,
            &mut Assets<StandardMaterial>,
            &mut Assets<Image>,
        ),
        deps: &mut AvatarSpawnDeps,
        far_material: &Handle<StandardMaterial>,
        cuts: Option<&DrawDistanceCuts>,
    ) -> Option<DrawnCopy> {
        let Stood {
            index,
            pose,
            variant,
        } = stood;
        let slot = &mut self.slots[variant];
        let form = slot.form.clone().filter(|form| form.draws())?;
        let mut entities = Vec::new();
        let tag = InstanceCopyOf {
            placement: self.placement,
            copy: index,
        };
        // Small copies are cut at the draw distance, and drawn near or not
        // at all: a few pixels far off are not worth a far form.
        let class = form
            .bounds
            .and_then(|(lo, hi)| SizeClass::of((hi - lo).max_element() * pose.scale.x));
        let cut = class.map(|class| (class, cuts.and_then(|cuts| cuts.range(class))));
        // A copy drawn whole costs what its first did; before one has been,
        // at least a node a part.
        let near_cost = match &form.near {
            Some(parts) => parts.len() as u32,
            None => slot.whole_cost.unwrap_or_else(|| {
                u32::try_from(crate::pds::room::generator_entity_count(&self.tree))
                    .unwrap_or(u32::MAX)
            }),
        };
        let root = self.root;
        let cost = if self.near_spent.saturating_add(near_cost) <= self.near_budget {
            let spent = match &form.near {
                Some(parts) => {
                    for (mesh, material, sway) in parts {
                        let mut part = commands.spawn((
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(material.clone()),
                            pose,
                            tag,
                            ChildOf(root),
                        ));
                        if let Some(sway) = sway {
                            part.insert(*sway);
                        }
                        stamp(&mut part, cut.clone());
                        entities.push(part.id());
                    }
                    near_cost
                }
                // Drawn whole, where a part is drawn with a material the
                // bake cannot merge: not cut at the draw distance, which
                // stamps merged parts only. No kind a set keeps draws so.
                None => {
                    let (whole, spent) = spawn_detached_tree(
                        commands,
                        root,
                        &self.tree,
                        &template_key(slot.key),
                        pose * Transform::from(&self.tree.transform),
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
                        Some(&slot.variant),
                    );
                    if let Some(whole) = whole {
                        commands.entity(whole).insert(tag);
                        entities.push(whole);
                    }
                    slot.whole_cost = Some(spent);
                    spent
                }
            };
            self.near_spent += spent;
            Cost::Near(spent)
        } else if class.is_none()
            && self.far_spent < self.far_budget
            && let Some(far) = form.far.clone()
        {
            entities.push(
                commands
                    .spawn((
                        Mesh3d(far),
                        MeshMaterial3d(far_material.clone()),
                        pose,
                        bevy::light::NotShadowCaster,
                        tag,
                        ChildOf(root),
                    ))
                    .id(),
            );
            self.far_spent += 1;
            Cost::Far
        } else {
            return None;
        };
        if let Some((collider, at)) = collider(&self.solid, &form, pose) {
            entities.push(commands.spawn((collider, at, tag, ChildOf(root))).id());
        }
        Some(DrawnCopy {
            pose,
            variant,
            entities,
            cost,
        })
    }

    /// Despawn a drawn copy's entities and give back what it cost.
    fn take_back(&mut self, drawn: DrawnCopy, commands: &mut Commands) {
        for entity in drawn.entities {
            commands.entity(entity).try_despawn();
        }
        match drawn.cost {
            Cost::Near(spent) => self.near_spent = self.near_spent.saturating_sub(spent),
            Cost::Far => self.far_spent = self.far_spent.saturating_sub(1),
        }
    }

    /// Whether the set `placement` of `record` is still the one this build
    /// draws, but for its copies: else the compile builds its unit anew.
    pub(super) fn draws_unit_of(&self, record: &RoomRecord, placement: &Placement) -> bool {
        self.unit == unit_key(record, placement)
    }

    /// Follow the record's copies `now` (see the module docs): each kept
    /// where its numbers are unchanged, renumbered where it moved up the
    /// list, taken away where it is gone, and queued to be drawn where it
    /// is new or changed - each stood on `ground` as a copy is - with the
    /// set's budgets its share again, by its copies among `copies_in_room`.
    pub(super) fn follow(
        &mut self,
        now: &[InstanceCopy],
        commands: &mut Commands,
        ground: Option<&bevy_symbios_ground::HeightMap>,
        copies_in_room: usize,
    ) -> Followed {
        if self.record_copies.as_slice() == now {
            return Followed::Done;
        }
        if self.stage != Stage::Done {
            return Followed::Rebuild;
        }
        // Each copy as drawn, by its numbers, earliest first: a copy keeps
        // the entities of the first one of its numbers not yet taken.
        let mut before: HashMap<InstanceCopy, VecDeque<usize>> = HashMap::new();
        for (i, copy) in self.record_copies.iter().enumerate() {
            before.entry(*copy).or_default().push_back(i);
        }
        let matched: Vec<Option<usize>> = now
            .iter()
            .map(|copy| before.get_mut(copy).and_then(VecDeque::pop_front))
            .collect();
        // A new copy of a variant never grown cannot be drawn in place.
        let ungrown = now.iter().zip(&matched).any(|(copy, kept)| {
            kept.is_none()
                && usize::try_from(copy.variant)
                    .ok()
                    .and_then(|v| self.slots.get(v))
                    .is_some_and(|slot| slot.form.is_none())
        });
        if ungrown {
            return Followed::Rebuild;
        }
        let mut old = std::mem::take(&mut self.drawn);
        let mut queue = Vec::new();
        let drawn: Vec<Option<DrawnCopy>> = now
            .iter()
            .zip(matched)
            .enumerate()
            .map(|(j, (copy, kept))| match kept {
                Some(i) => {
                    let kept = old[i].take();
                    if i != j
                        && let Some(kept) = &kept
                    {
                        let tag = InstanceCopyOf {
                            placement: self.placement,
                            copy: j,
                        };
                        for entity in &kept.entities {
                            commands.entity(*entity).try_insert(tag);
                        }
                    }
                    kept
                }
                None => {
                    if let Some(variant) = usize::try_from(copy.variant)
                        .ok()
                        .filter(|v| *v < self.slots.len())
                    {
                        let (pose, _) = stand(&self.anchor, self.snap, ground, copy);
                        queue.push(Stood {
                            index: j,
                            pose,
                            variant,
                        });
                    }
                    None
                }
            })
            .collect();
        for gone in old.iter_mut().filter_map(Option::take) {
            self.take_back(gone, commands);
        }
        self.drawn = drawn;
        self.record_copies = now.to_vec();
        (self.near_budget, self.far_budget) = share_of(now.len(), copies_in_room);
        if !queue.is_empty() {
            self.copies = queue;
            self.stage = Stage::Copies(0);
        }
        Followed::Done
    }

    /// Despawn everything it drew and every template it grew, and drop what
    /// a template not yet baked left in `caches`: before it is built anew
    /// under the same anchor.
    pub(super) fn take_down(
        &mut self,
        commands: &mut Commands,
        caches: &mut crate::world_builder::compile::GeneratorCaches,
    ) {
        for drawn in self.drawn.iter_mut().filter_map(Option::take) {
            for entity in drawn.entities {
                commands.entity(entity).try_despawn();
            }
        }
        for slot in &mut self.slots {
            if let Some(template) = slot.template.take() {
                commands.entity(template).try_despawn();
                forget_template(caches, &template_key(slot.key));
            }
        }
    }

    /// Its placement's index.
    pub(super) fn placement(&self) -> usize {
        self.placement
    }

    /// Each grown variant with why it draws nothing, or `None` where it
    /// draws - once, the first time its build is done (#1644): what the
    /// set's section says of its variants.
    pub(super) fn report_failures(&mut self) -> Option<Vec<(usize, Option<String>)>> {
        if self.reported || self.stage != Stage::Done {
            return None;
        }
        self.reported = true;
        Some(
            self.slots
                .iter()
                .enumerate()
                .filter_map(|(v, slot)| Some((v, slot.form.as_ref()?.failure.clone())))
                .collect(),
        )
    }

    /// Where the anchor stands, as the compile stood it.
    pub(super) fn anchor(&self) -> Transform {
        self.anchor
    }

    /// Copy `copy` as drawn, if it is.
    pub(super) fn drawn(&self, copy: usize) -> Option<&DrawnCopy> {
        self.drawn.get(copy)?.as_ref()
    }

    /// Where `copy` stands in the world as this set would draw it.
    pub(super) fn stand_world(
        &self,
        copy: &InstanceCopy,
        ground: Option<&bevy_symbios_ground::HeightMap>,
    ) -> Transform {
        let (pose, _) = stand(&self.anchor, self.snap, ground, copy);
        self.anchor * pose
    }

    /// The box variant `variant`'s parts fill, in a copy's frame, once it
    /// is baked.
    pub(super) fn variant_bounds(&self, variant: usize) -> Option<(Vec3, Vec3)> {
        self.slots.get(variant)?.form.as_ref()?.bounds
    }

    /// The nearest drawn copy whose baked box `ray` enters, and how far
    /// along it (#1644): what a click in the World Editor may pick. A box
    /// the ray starts in is not one it enters: a camera under a crown picks
    /// through it.
    pub(super) fn pick(&self, ray: Ray3d, keep: &dyn Fn(usize) -> bool) -> Option<(usize, f32)> {
        let mut nearest: Option<(usize, f32)> = None;
        for (c, drawn) in self.drawn.iter().enumerate() {
            let Some(drawn) = drawn.as_ref().filter(|_| keep(c)) else {
                continue;
            };
            let Some((lo, hi)) = self.slots[drawn.variant]
                .form
                .as_ref()
                .and_then(|form| form.bounds)
            else {
                continue;
            };
            // The ray in the copy's frame, where its box is: the same
            // distance along it, the map being affine.
            let to_copy = (self.anchor * drawn.pose).compute_affine().inverse();
            let hit = super::ray_box_distance(
                to_copy.transform_point3(ray.origin),
                to_copy.transform_vector3(*ray.direction),
                (lo, hi),
            );
            if let Some(distance) = hit
                && nearest.is_none_or(|(_, d)| distance < d)
            {
                nearest = Some((c, distance));
            }
        }
        nearest
    }
}

/// The collider a copy at `pose` stands on, and where: what its set's
/// `solid` says a walker meets.
fn collider(solid: &InstanceSolid, form: &Form, pose: Transform) -> Option<(Collider, Transform)> {
    match solid {
        InstanceSolid::Shell | InstanceSolid::Other(_) => {
            form.shell.clone().map(|shell| (shell, pose))
        }
        InstanceSolid::Box => {
            let (lo, hi) = form.bounds?;
            let size = (hi - lo).max(Vec3::splat(0.05));
            let centre = pose.transform_point((lo + hi) / 2.0);
            Some((
                Collider::cuboid(size.x, size.y, size.z),
                Transform::from_translation(centre)
                    .with_rotation(pose.rotation)
                    .with_scale(pose.scale),
            ))
        }
        InstanceSolid::Trunk => {
            let (lo, hi) = form.bounds?;
            let radius = (form.base? * pose.scale.x).clamp(TRUNK_RANGE_M.0, TRUNK_RANGE_M.1);
            let tall = ((hi.y - lo.y) * pose.scale.y).min(TRUNK_M);
            Some((
                Collider::cylinder(radius, tall),
                Transform::from_translation(pose.translation + Vec3::Y * (tall / 2.0)),
            ))
        }
        InstanceSolid::None => None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{DefaultLanding, Fp, Fp2, InstanceCopy, TransformData};

    /// A room holding the set `copies` of a cuboid, and nothing else.
    fn room_of(copies: Vec<InstanceCopy>, variants: Vec<InstanceVariant>) -> RoomRecord {
        let mut record = RoomRecord::default();
        record.generators.clear();
        record.placements.clear();
        record.traits.clear();
        record.default_landing = None;
        record
            .generators
            .insert("post".into(), Generator::default_cuboid());
        record.placements.push(Placement::Instances {
            generator_ref: "post".into(),
            transform: TransformData::default(),
            snap_to_terrain: true,
            solid: InstanceSolid::Shell,
            variants,
            copies,
        });
        record
    }

    fn built(
        record: &RoomRecord,
        anchor: Transform,
        ground: Option<&bevy_symbios_ground::HeightMap>,
    ) -> Build {
        Build::new(
            record,
            (0, &record.placements[0]),
            (Entity::PLACEHOLDER, anchor),
            ground,
            record
                .placements
                .iter()
                .map(|p| match p {
                    Placement::Instances { copies, .. } => copies.len(),
                    _ => 0,
                })
                .sum(),
        )
        .expect("a set")
    }

    /// A snapped copy stands on the ground under it, raised by its own y,
    /// turned clockwise and sized as the record says, in the anchor's frame.
    #[test]
    fn a_snapped_copy_stands_on_the_ground_under_it() {
        let mut ground = bevy_symbios_ground::HeightMap::new(33, 33, 1.0);
        ground.data_mut().fill(7.0);
        let copy = InstanceCopy {
            y_cm: -35,
            scale_pm: 2_000,
            ..InstanceCopy::at(4.0, -2.0, 90.0)
        };
        let record = room_of(vec![copy], Vec::new());
        let anchor = Transform::from_xyz(3.0, 7.0, 1.0);
        let build = built(&record, anchor, Some(&ground));
        let pose = build.copies[0].pose;
        assert!(
            (anchor * pose)
                .translation
                .abs_diff_eq(Vec3::new(7.0, 6.65, -1.0), 1e-4),
            "{pose:?}"
        );
        assert!(
            pose.rotation
                .abs_diff_eq(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2), 1e-6)
        );
        assert_eq!(pose.scale, Vec3::splat(2.0));
        // Turned clockwise: its front (-Z) faces +X.
        assert!((pose.rotation * Vec3::NEG_Z).abs_diff_eq(Vec3::X, 1e-6));
    }

    /// Copies are drawn nearest the landing first, ties in the record's
    /// order; a copy of a variant the set does not hold is not drawn.
    #[test]
    fn copies_are_drawn_nearest_the_landing_first() {
        let mut record = room_of(
            vec![
                InstanceCopy::at(30.0, 0.0, 0.0),
                InstanceCopy::at(10.0, 0.0, 0.0),
                InstanceCopy {
                    variant: 4,
                    ..InstanceCopy::at(0.0, 0.0, 0.0)
                },
                InstanceCopy::at(-10.0, 0.0, 0.0),
            ],
            Vec::new(),
        );
        record.default_landing = Some(DefaultLanding {
            pos: Fp2([10.0, 0.0]),
            y: None,
            yaw_deg: Fp(0.0),
        });
        let build = built(&record, Transform::IDENTITY, None);
        let order: Vec<usize> = build.copies.iter().map(|copy| copy.index).collect();
        assert_eq!(order, [1, 0, 3]);
        assert_eq!(build.slots.len(), 1);
    }

    /// A set's budgets are its share of the room's by its copies.
    #[test]
    fn a_sets_budget_is_its_share_of_the_rooms_by_its_copies() {
        let mut record = room_of(vec![InstanceCopy::at(0.0, 0.0, 0.0); 25], Vec::new());
        let other = record.placements[0].clone();
        record.placements.push(other.clone());
        record.placements.push(other.clone());
        record.placements.push(other);
        let build = built(&record, Transform::IDENTITY, None);
        assert_eq!(build.near_budget, ROOM_NEAR_ENTITIES / 4);
        assert_eq!(build.far_budget, ROOM_FAR_COPIES / 4);
    }

    fn form(bounds: (Vec3, Vec3), base: Option<f32>) -> Form {
        Form {
            near: None,
            far: None,
            bounds: Some(bounds),
            base,
            shell: None,
            failure: None,
        }
    }

    /// A box collider is the box the parts fill, turned and sized with the
    /// copy; a trunk is as wide as the base, at most 3 m tall, upright.
    #[test]
    fn a_copy_stands_on_the_solid_its_set_names() {
        let pose = Transform::from_xyz(4.0, 1.0, -2.0)
            .with_rotation(Quat::from_rotation_y(0.5))
            .with_scale(Vec3::splat(2.0));
        let boxed = form((Vec3::new(-1.0, 0.0, -0.5), Vec3::new(1.0, 2.0, 0.5)), None);
        let (_, at) = collider(&InstanceSolid::Box, &boxed, pose).expect("a box");
        assert!(
            at.translation
                .abs_diff_eq(pose.transform_point(Vec3::Y), 1e-5)
        );
        assert_eq!(at.rotation, pose.rotation);
        assert_eq!(at.scale, pose.scale);

        let tree = form(
            (Vec3::new(-3.0, 0.0, -3.0), Vec3::new(3.0, 9.0, 3.0)),
            Some(0.2),
        );
        let (trunk, at) = collider(&InstanceSolid::Trunk, &tree, pose).expect("a trunk");
        assert!(
            at.translation
                .abs_diff_eq(Vec3::new(4.0, 1.0 + TRUNK_M / 2.0, -2.0), 1e-5)
        );
        assert_eq!(at.rotation, Quat::IDENTITY, "a trunk stands upright");
        let cylinder = trunk.shape().as_cylinder().expect("a cylinder");
        assert!((cylinder.radius - 0.4).abs() < 1e-6, "the base, scaled");
        assert!((cylinder.half_height - TRUNK_M / 2.0).abs() < 1e-6);

        assert!(collider(&InstanceSolid::None, &tree, pose).is_none());
        assert!(
            collider(&InstanceSolid::Shell, &tree, pose).is_none(),
            "no shell baked, nothing to stand on"
        );
    }

    /// No variants is one: the generator as it is.
    #[test]
    fn a_set_with_no_variants_draws_one() {
        let record = room_of(vec![InstanceCopy::at(0.0, 0.0, 0.0)], Vec::new());
        let build = built(&record, Transform::IDENTITY, None);
        assert_eq!(build.slots.len(), 1);
        assert_eq!(build.slots[0].variant, InstanceVariant::default());
        assert!(build.slots[0].drawn);
    }
}
