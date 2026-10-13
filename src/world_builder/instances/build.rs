//! One instance set being drawn (#1643): its copies stood where the record
//! puts them and ordered nearest the landing first, its variants' templates
//! grown and baked, and its copies spawned (see the module docs).

use std::sync::Arc;

use avian3d::prelude::Collider;
use bevy::camera::visibility::VisibilityRange;
use bevy::ecs::system::EntityCommands;
use bevy::platform::time::Instant;
use bevy::prelude::*;

use crate::pds::{Generator, InstanceSolid, InstanceVariant, Placement, RoomRecord};
use crate::player::visuals::AvatarSpawnDeps;
use crate::world_builder::avatar_spawn::{detached_record, spawn_detached_tree};
use crate::world_builder::draw_distance::{DrawDistanceCuts, SizeClass};

use super::bake::PartQuery;
use super::forms::{Form, Forms, form_key};
use super::{InstanceCopyOf, ROOM_FAR_COPIES, ROOM_NEAR_ENTITIES, strip_for_distance};

/// The tallest a trunk collider stands (m): the trunk a walker meets, not
/// the crown.
const TRUNK_M: f32 = 3.0;

/// The narrowest and widest a trunk collider is (m), as the derived stage
/// bounds a tree's.
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
    /// Nearest the landing first.
    copies: Vec<Stood>,
    solid: InstanceSolid,
    /// Its share of the room's budgets.
    near_budget: u32,
    far_budget: usize,
    near_spent: u32,
    far_spent: usize,
    stage: Stage,
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
                let [x, y, z] = copy.position();
                let on_plan = anchor.translation + anchor.rotation * Vec3::new(x, 0.0, z);
                let world_y = match (snap_to_terrain, ground) {
                    (true, Some(ground)) => {
                        crate::world_builder::snapped_ground_y(ground, on_plan.x, on_plan.z, None)
                            + y
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
        let share = copies.len() as f64 / copies_in_room.max(copies.len()).max(1) as f64;
        Some(Build {
            root,
            placement: index,
            generator_ref: generator_ref.clone(),
            tree,
            slots,
            copies: stood.into_iter().map(|(_, stood)| stood).collect(),
            solid: solid.clone(),
            near_budget: (f64::from(ROOM_NEAR_ENTITIES) * share) as u32,
            far_budget: (ROOM_FAR_COPIES as f64 * share) as usize,
            near_spent: 0,
            far_spent: 0,
            stage: if copies.is_empty() {
                Stage::Done
            } else {
                Stage::Templates(0)
            },
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
                    let form = Form::from_baked(baked, meshes);
                    let slot = &mut self.slots[v];
                    // Merged, its forms hold all it draws; drawn whole, its
                    // copies are grown from the caches each time.
                    if form.near.is_some() {
                        forget_template(&mut deps.caches, &template_key(slot.key));
                    }
                    if !form.draws() {
                        warn!(
                            "instances #{} ({}): variant {v} draws nothing",
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
                    self.spawn_copy(
                        c,
                        commands,
                        (&mut *meshes, &mut *materials, &mut *images),
                        deps,
                        far_material,
                        cuts,
                    );
                    self.stage = Stage::Copies(c + 1);
                }
                Stage::Copies(_) => {
                    info!(
                        "instances #{} ({}): {} near entities, {} far copies, of {} copies from {} variants",
                        self.placement,
                        self.generator_ref,
                        self.near_spent,
                        self.far_spent,
                        self.copies.len(),
                        self.slots.iter().filter(|slot| slot.drawn).count()
                    );
                    self.stage = Stage::Done;
                }
                Stage::Done => return,
            }
        }
    }

    /// Spawn copy `c`: near while the set's budget lasts, else far where it
    /// is big enough to be seen far and the far budget lasts, and its
    /// collider where it drew anything.
    fn spawn_copy(
        &mut self,
        c: usize,
        commands: &mut Commands,
        (meshes, materials, images): (
            &mut Assets<Mesh>,
            &mut Assets<StandardMaterial>,
            &mut Assets<Image>,
        ),
        deps: &mut AvatarSpawnDeps,
        far_material: &Handle<StandardMaterial>,
        cuts: Option<&DrawDistanceCuts>,
    ) {
        let Stood {
            index,
            pose,
            variant,
        } = self.copies[c];
        let slot = &mut self.slots[variant];
        let Some(form) = slot.form.clone().filter(|form| form.draws()) else {
            return;
        };
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
        let drawn = if self.near_spent.saturating_add(near_cost) <= self.near_budget {
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
                    }
                    slot.whole_cost = Some(spent);
                    spent
                }
            };
            self.near_spent += spent;
            true
        } else if class.is_none()
            && self.far_spent < self.far_budget
            && let Some(far) = form.far.clone()
        {
            commands.spawn((
                Mesh3d(far),
                MeshMaterial3d(far_material.clone()),
                pose,
                bevy::light::NotShadowCaster,
                tag,
                ChildOf(root),
            ));
            self.far_spent += 1;
            true
        } else {
            false
        };
        if drawn && let Some((collider, at)) = collider(&self.solid, &form, pose) {
            commands.spawn((collider, at, tag, ChildOf(root)));
        }
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
