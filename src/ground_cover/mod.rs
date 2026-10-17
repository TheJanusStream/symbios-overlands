//! What the ground offers whatever grows on it (#1674): the terrain's
//! height as a GPU texture, the splat weights the ground is painted with,
//! and a cover mask of where something may grow at all. The GPU grass
//! (#1675) is meant to read all three; nothing draws them yet.
//!
//! [`GroundCover`] holds them, for the room whose heightmap is up:
//!
//! * **`height`** - one `R32Float` texel per heightmap vertex, its world
//!   height. A 32-bit float texture cannot be filtered on WebGL2: bind it
//!   unfilterable, read it with `textureLoad` and interpolate as
//!   [`mesh_height`] does, and a blade stands on the triangles the ground
//!   is drawn with.
//! * **`weights`** - the splat material's own weight map: one RGBA8 texel
//!   per heightmap vertex, each channel a layer's share, the record's four
//!   layers in order (by default grass, dirt, rock, snow). The ground's
//!   shader samples it linearly at its mesh UV, `(world_xz + half) /
//!   (2 half)` - which puts vertex k at `k / (n - 1)`, not at its texel's
//!   centre, half a texel off at the edges - and whatever grows must sample
//!   it the same way to follow the ground's look. `None` until the ground's
//!   textures have baked.
//! * **`cover`** - one `R8Unorm` texel per metre (two a 2 m cell): 1 where
//!   the ground is open, 0 under a road, a building, any solid placed item
//!   standing on the ground, and wherever the ground lies under water or
//!   within [`WATER_MARGIN`] of it. Sample it linearly, and its edges fade
//!   across a metre. [`GroundCover::mask`] is the same on the CPU.
//!
//! **What blocks the cover.** Every static collider the world stands up -
//! a placement's parts (lot buildings among them), a road's deck and curbs,
//! an instance set's copies - is outlined where it reaches the ground
//! ([`footprint`]); ground cover, trim and most plants carry no collider
//! and block nothing (an instanced tree's trunk blocks the disc it stands
//! on), and a moving body (a player, a vehicle) is not static. Water blocks
//! below the room's drawn water line. The heightmap is square, as the
//! terrain mesh and every other ground query take it.
//!
//! **When it is redrawn.** The height texture when a heightmap lands; the
//! cover when the world has held still for [`REBUILD_DEBOUNCE`] after a
//! change - a record edit, a static collider spawned or despawned, a new
//! heightmap - and no compile, road re-mesh or instance build is still
//! under way. A redraw rereads every collider and costs a few
//! milliseconds; it happens when a room settles and after an edit, never
//! while one is dragged.
//!
//! [`WATER_MARGIN`]: crate::config::terrain::ground_cover::WATER_MARGIN
//! [`REBUILD_DEBOUNCE`]: crate::config::terrain::ground_cover::REBUILD_DEBOUNCE

pub(crate) mod footprint;
pub(crate) mod height;
pub(crate) mod mask;

use avian3d::prelude::{Collider, RigidBody, Sensor};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

pub(crate) use height::mesh_height;
pub use mask::CoverMask;

use crate::config::terrain::ground_cover as cfg;
use crate::state::LiveRoomRecord;
use crate::terrain::FinishedHeightMap;

/// The room's ground, as whatever grows on it reads it (#1674). Present
/// while a heightmap is.
#[derive(Resource)]
pub struct GroundCover {
    /// How world XZ lands on the three textures.
    pub frame: GroundFrame,
    /// The heightmap's vertices' heights, `R32Float`, GPU only.
    pub height: Handle<Image>,
    /// The splat weight map the ground is painted with, once it has baked.
    pub weights: Option<Handle<Image>>,
    /// Where something may grow, `R8Unorm`, GPU only: 1 open, 0 blocked.
    pub cover: Handle<Image>,
    /// The cover on the CPU, as last uploaded.
    pub mask: CoverMask,
    /// How many times the cover has been drawn for this heightmap; 0 while
    /// it is still the all-open stand-in.
    pub revision: u32,
}

/// How world XZ lands on [`GroundCover`]'s textures. The ground is a square
/// centred on the world origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundFrame {
    /// Half the ground's width (m): it runs from `-half` to `half` on X and Z.
    pub half: f32,
    /// Metres between heightmap vertices.
    pub cell: f32,
    /// Heightmap vertices per side: the height texture's size.
    pub vertices: u32,
    /// Cover texels per side.
    pub cover_texels: u32,
}

impl GroundFrame {
    /// The frame of heightmap `hm`, which is square.
    pub fn of(hm: &bevy_symbios_ground::HeightMap) -> Self {
        debug_assert_eq!(hm.width(), hm.height(), "the ground is square");
        let cells = hm.width().saturating_sub(1).max(1);
        Self {
            half: cells as f32 * hm.scale() * 0.5,
            cell: hm.scale(),
            vertices: hm.width() as u32,
            cover_texels: (cells * cfg::TEXELS_PER_CELL) as u32,
        }
    }

    /// Metres per cover texel.
    pub fn cover_texel(&self) -> f32 {
        self.half * 2.0 / self.cover_texels.max(1) as f32
    }

    /// An open cover mask over this frame.
    fn open_mask(&self) -> CoverMask {
        CoverMask::open(
            self.cover_texels as usize,
            self.cover_texel(),
            Vec2::splat(-self.half),
        )
    }
}

/// The splat weight map the ground's material was given, published by the
/// terrain when it bakes (#1674): the handle [`GroundCover::weights`] takes.
#[derive(Resource, Clone)]
pub(crate) struct SplatWeights(pub(crate) Handle<Image>);

/// Whether the cover waits to be redrawn, and what can make it.
#[derive(Resource, Default)]
pub(crate) struct CoverDirty {
    /// Since when the world last changed (virtual seconds), or `None` when
    /// the cover is current.
    since: Option<f64>,
    /// The static colliders the last redraw outlined: a despawn among
    /// them dirties the cover, a moving body's does not.
    statics: bevy::ecs::entity::EntityHashSet,
}

impl CoverDirty {
    /// Whether a redraw is still to come (the render tool waits on it).
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn pending(&self) -> bool {
        self.since.is_some()
    }
}

/// [`GroundCover`] and the systems that keep it. Gated on resources, not on
/// the app's state, so the render tool's `--world` runs the game's own.
pub struct GroundCoverPlugin;

impl Plugin for GroundCoverPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CoverDirty>().add_systems(
            Update,
            (
                drop_ground_cover.run_if(
                    resource_exists::<GroundCover>
                        .and_then(not(resource_exists::<FinishedHeightMap>)),
                ),
                sync_ground_height.run_if(resource_exists::<FinishedHeightMap>),
                sync_weights.run_if(resource_exists::<GroundCover>),
                mark_cover_dirty,
                rebuild_cover.run_if(resource_exists::<GroundCover>),
            )
                .chain(),
        );
    }
}

/// The heightmap is gone (a logout, a re-roll's teardown): so is its ground.
fn drop_ground_cover(mut commands: Commands, mut dirty: ResMut<CoverDirty>) {
    commands.remove_resource::<GroundCover>();
    commands.remove_resource::<SplatWeights>();
    *dirty = CoverDirty::default();
}

/// A heightmap has landed: give the ground its height texture and frame,
/// and an all-open cover until the world settles enough to draw it. A
/// heightmap replaced in place drops the weights too: they were the last
/// ground's, and the new ground's come when it bakes.
fn sync_ground_height(
    mut commands: Commands,
    heightmap: Res<FinishedHeightMap>,
    cover: Option<ResMut<GroundCover>>,
    mut images: ResMut<Assets<Image>>,
) {
    if !heightmap.is_changed() {
        return;
    }
    let hm = &heightmap.0;
    let frame = GroundFrame::of(hm);
    let mask = frame.open_mask();
    match cover {
        Some(mut cover) => {
            replace(&mut images, &cover.height, height::height_image(hm));
            replace(&mut images, &cover.cover, cover_image(&mask));
            cover.frame = frame;
            cover.mask = mask;
            cover.revision = 0;
            cover.weights = None;
        }
        None => commands.insert_resource(GroundCover {
            frame,
            height: images.add(height::height_image(hm)),
            weights: None,
            cover: images.add(cover_image(&mask)),
            mask,
            revision: 0,
        }),
    }
}

/// The ground's textures have baked: take its weight map - but only one
/// published after this heightmap landed. Weights published before belong
/// to the last ground, whose heightmap this one replaced in place. Writes
/// only a change.
fn sync_weights(
    weights: Option<Res<SplatWeights>>,
    heightmap: Res<FinishedHeightMap>,
    mut cover: ResMut<GroundCover>,
    ticks: bevy::ecs::system::SystemChangeTick,
) {
    let Some(weights) = weights else {
        return;
    };
    let current = weights
        .last_changed()
        .is_newer_than(heightmap.last_changed(), ticks.this_run());
    if current && cover.weights.as_ref() != Some(&weights.0) {
        cover.weights = Some(weights.0.clone());
    }
}

/// Note when the world changed in a way that can move the cover: a new
/// heightmap, a record edit (an editor's or an agent's move lands in the
/// record when it is let go), a static collider spawned or despawned. Not
/// a moving body's collider - a player's, a vehicle's - and not an
/// anchor's transform: a spinning node rewrites its anchor every frame (a
/// banner pole streaming in the wind), and the cover would never be drawn.
/// The despawn reader is drained whatever happens, so a removal is never
/// seen late.
#[allow(clippy::too_many_arguments)] // Bevy system: each arg is a distinct resource/query.
fn mark_cover_dirty(
    time: Res<Time>,
    heightmap: Option<Res<FinishedHeightMap>>,
    record: Option<Res<LiveRoomRecord>>,
    added: Query<Entity, (Added<Collider>, Without<Sensor>)>,
    mut removed: RemovedComponents<Collider>,
    bodies: Query<&RigidBody>,
    parents: Query<&ChildOf>,
    mut dirty: ResMut<CoverDirty>,
) {
    let mut changed = heightmap.is_some_and(|h| h.is_changed())
        || record.is_some_and(|r| r.is_changed())
        || added.iter().any(|e| is_static(e, &bodies, &parents));
    for entity in removed.read() {
        changed |= dirty.statics.remove(&entity);
    }
    if changed {
        dirty.since = Some(time.elapsed_secs_f64());
    }
}

/// Whether the world has stopped building: no compile pass, road re-mesh
/// or instance build under way. A resource the app does not have builds
/// nothing.
fn world_at_rest(
    job: Option<&crate::world_builder::compile::CompileJob>,
    roads: Option<&crate::terrain::RoadPanelStats>,
    instances: Option<&crate::world_builder::instances::InstanceBuilds>,
) -> bool {
    job.is_none_or(|j| j.progress().is_none())
        && roads.is_none_or(|r| !r.pending)
        && instances.is_none_or(|i| i.is_idle())
}

/// Redraw the cover once the world has held still: water first, then every
/// static collider's outline on the ground.
#[allow(clippy::too_many_arguments)]
fn rebuild_cover(
    time: Res<Time>,
    mut dirty: ResMut<CoverDirty>,
    mut cover: ResMut<GroundCover>,
    heightmap: Option<Res<FinishedHeightMap>>,
    record: Option<Res<LiveRoomRecord>>,
    job: Option<Res<crate::world_builder::compile::CompileJob>>,
    roads: Option<Res<crate::terrain::RoadPanelStats>>,
    instances: Option<Res<crate::world_builder::instances::InstanceBuilds>>,
    colliders: Query<(Entity, &Collider, &GlobalTransform), Without<Sensor>>,
    bodies: Query<&RigidBody>,
    parents: Query<&ChildOf>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(since) = dirty.since else {
        return;
    };
    if time.elapsed_secs_f64() - since < f64::from(cfg::REBUILD_DEBOUNCE)
        || !world_at_rest(job.as_deref(), roads.as_deref(), instances.as_deref())
    {
        return;
    }
    let Some(heightmap) = heightmap else {
        return;
    };
    let started = bevy::platform::time::Instant::now();
    let hm = &heightmap.0;
    let ground = |xz: Vec2| mesh_height(hm, xz);
    let mut mask = cover.frame.open_mask();

    let water = record
        .as_ref()
        .and_then(|r| crate::world_builder::compile::drawn_water_level(&r.0, Some(&*heightmap)));
    if let Some(level) = water {
        block_water(&mut mask, hm, level + cfg::WATER_MARGIN);
    }

    let mut outlines = 0usize;
    dirty.statics.clear();
    for (entity, collider, transform) in &colliders {
        if !is_static(entity, &bodies, &parents) {
            continue;
        }
        dirty.statics.insert(entity);
        footprint::outlines(
            collider.shape(),
            transform.affine(),
            &ground,
            &mut |outline| {
                outlines += 1;
                mask.block_convex(outline);
            },
        );
    }

    info!(
        "ground cover drawn: {} outlines, {:.1}% open, {:.1} ms",
        outlines,
        mask.open_share() * 100.0,
        started.elapsed().as_secs_f64() * 1000.0
    );
    replace(&mut images, &cover.cover, cover_image(&mask));
    cover.mask = mask;
    cover.revision += 1;
    dirty.since = None;
}

/// Block the ground lower than `line`. The drawn ground is flat between a
/// cell's corners, so a cell whose four corners all stand above the line is
/// dry throughout and one whose four all lie under it is wet throughout;
/// only a cell the shore crosses is tested texel by texel.
fn block_water(mask: &mut CoverMask, hm: &bevy_symbios_ground::HeightMap, line: f32) {
    /// What a cell holds of the water.
    #[derive(Clone, Copy, PartialEq)]
    enum Cell {
        Dry,
        Shore,
        Under,
    }
    let (w, h) = (hm.width(), hm.height());
    let cells = Vec2::new((w - 1) as f32, (h - 1) as f32);
    let half = cells * hm.scale() * 0.5;
    let mut wet = vec![Cell::Dry; (w - 1) * (h - 1)];
    for z in 0..h - 1 {
        for x in 0..w - 1 {
            let corners = [
                hm.get(x, z),
                hm.get(x + 1, z),
                hm.get(x, z + 1),
                hm.get(x + 1, z + 1),
            ];
            wet[z * (w - 1) + x] = if corners.iter().all(|&c| c < line) {
                Cell::Under
            } else if corners.iter().any(|&c| c < line) {
                Cell::Shore
            } else {
                Cell::Dry
            };
        }
    }
    if wet.iter().all(|&c| c == Cell::Dry) {
        return;
    }
    mask.block_where(|xz| {
        let cell = ((xz + half) / hm.scale()).clamp(Vec2::ZERO, cells - 1.0);
        match wet[cell.y as usize * (w - 1) + cell.x as usize] {
            Cell::Dry => false,
            Cell::Under => true,
            Cell::Shore => mesh_height(hm, xz) < line,
        }
    });
}

/// Whether the collider on `entity` stands still: the nearest rigid body at
/// or above it is static, or there is none - avian holds a collider with
/// no body still.
fn is_static(entity: Entity, bodies: &Query<&RigidBody>, parents: &Query<&ChildOf>) -> bool {
    let mut at = entity;
    loop {
        if let Ok(body) = bodies.get(at) {
            return body.is_static();
        }
        match parents.get(at) {
            Ok(parent) => at = parent.parent(),
            Err(_) => return true,
        }
    }
}

/// Put `image` in `handle`'s place, so whatever binds the handle reads the
/// new one. A handle held strongly cannot outlive its slot; were it ever to,
/// the old texture would stay, and the log says so.
fn replace(images: &mut Assets<Image>, handle: &Handle<Image>, image: Image) {
    if let Err(error) = images.insert(handle, image) {
        warn!("ground cover: could not replace a texture: {error}");
    }
}

/// The cover mask as the texture the GPU reads: one `R8Unorm` texel each,
/// linear between them, clamped at the edges, GPU only - the CPU keeps its
/// own copy in [`GroundCover::mask`].
fn cover_image(mask: &CoverMask) -> Image {
    let size = mask.size() as u32;
    let mut image = Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        mask.bytes().to_vec(),
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..Default::default()
    });
    image
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{Fp3, Generator, GeneratorKind, RoomRecord, TransformData};

    /// A 33 x 33 heightmap of 2 m cells (64 m across), flat at `y`.
    fn flat_ground(y: f32) -> FinishedHeightMap {
        let mut hm = bevy_symbios_ground::HeightMap::new(33, 33, 2.0);
        for z in 0..33 {
            for x in 0..33 {
                hm.set(x, z, y);
            }
        }
        FinishedHeightMap(hm, None)
    }

    /// An app running the plugin alone, on a clock the test turns.
    fn app(heightmap: FinishedHeightMap) -> App {
        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<Assets<Image>>()
            .insert_resource(heightmap)
            .add_plugins(GroundCoverPlugin);
        app
    }

    /// Turn the clock past the redraw's debounce and run a few frames.
    fn settle(app: &mut App) {
        for _ in 0..3 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs_f32(cfg::REBUILD_DEBOUNCE));
            app.update();
        }
    }

    fn open_at(app: &App, x: f32, z: f32) -> f32 {
        app.world()
            .resource::<GroundCover>()
            .mask
            .open_at(Vec2::new(x, z))
    }

    /// The ground gets its frame and height texture with its heightmap, a
    /// static box standing on it blocks the cover under it, and a body that
    /// moves, a box held clear of the ground and a sensor block nothing.
    #[test]
    fn what_stands_on_the_ground_blocks_its_cover() {
        let mut app = app(flat_ground(1.0));
        let at = |x: f32, y: f32, z: f32| GlobalTransform::from_translation(Vec3::new(x, y, z));
        app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(6.0, 4.0, 6.0),
            at(-10.0, 3.0, -10.0),
        ));
        app.world_mut().spawn((
            RigidBody::Dynamic,
            Collider::cuboid(6.0, 4.0, 6.0),
            at(10.0, 3.0, 10.0),
        ));
        app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(6.0, 1.0, 6.0),
            at(10.0, 9.0, -10.0),
        ));
        app.world_mut().spawn((
            Sensor,
            Collider::cuboid(6.0, 4.0, 6.0),
            at(-10.0, 3.0, 10.0),
        ));
        app.update();
        let cover = app.world().resource::<GroundCover>();
        assert_eq!(cover.frame.vertices, 33);
        assert_eq!(cover.frame.half, 32.0);
        assert_eq!(cover.mask.size(), 64);
        assert_eq!(cover.revision, 0, "the cover waits for the world to settle");
        settle(&mut app);
        let cover = app.world().resource::<GroundCover>();
        assert_eq!(cover.revision, 1);
        assert_eq!(open_at(&app, -10.0, -10.0), 0.0, "under the static box");
        assert_eq!(open_at(&app, 10.0, 10.0), 1.0, "under the moving body");
        assert_eq!(open_at(&app, 10.0, -10.0), 1.0, "under the raised box");
        assert_eq!(open_at(&app, -10.0, 10.0), 1.0, "under the sensor");
        assert_eq!(open_at(&app, 0.0, 0.0), 1.0, "open ground");
    }

    /// A collider under a static body's anchor counts as the body's, and a
    /// despawned one stops blocking once the cover is redrawn.
    #[test]
    fn a_static_anchor_s_parts_block_until_despawned() {
        let mut app = app(flat_ground(0.0));
        let anchor = app
            .world_mut()
            .spawn((RigidBody::Static, GlobalTransform::default()))
            .id();
        let part = app
            .world_mut()
            .spawn((
                Collider::cuboid(4.0, 2.0, 4.0),
                GlobalTransform::from_translation(Vec3::new(5.0, 1.0, 5.0)),
                ChildOf(anchor),
            ))
            .id();
        app.update();
        settle(&mut app);
        assert_eq!(open_at(&app, 5.0, 5.0), 0.0);
        app.world_mut().entity_mut(part).despawn();
        settle(&mut app);
        assert_eq!(open_at(&app, 5.0, 5.0), 1.0);
        assert_eq!(app.world().resource::<GroundCover>().revision, 2);
    }

    /// Ground under the room's water line, or just above it, is blocked.
    #[test]
    fn ground_under_the_water_line_is_blocked() {
        // A slope from 0 m on the west edge to 8 m on the east one.
        let mut hm = bevy_symbios_ground::HeightMap::new(33, 33, 2.0);
        for z in 0..33 {
            for x in 0..33 {
                hm.set(x, z, x as f32 * 0.25);
            }
        }
        let mut app = app(FinishedHeightMap(hm, None));
        let mut record = RoomRecord::default();
        // The default record grows its own terrain and water: only ours.
        record.generators.clear();
        let mut terrain = Generator::from_kind(GeneratorKind::Terrain(Default::default()));
        let mut water = Generator::from_kind(GeneratorKind::Water {
            surface: Default::default(),
        });
        water.transform = TransformData {
            translation: Fp3([0.0, 4.0, 0.0]),
            ..Default::default()
        };
        terrain.children.push(water);
        record.generators.insert("terrain".into(), terrain);
        app.insert_resource(LiveRoomRecord(record));
        app.update();
        settle(&mut app);
        // The ground meets the water line at x = 0 (4 m up the slope).
        assert_eq!(open_at(&app, -20.0, 0.0), 0.0);
        assert_eq!(open_at(&app, 20.0, 0.0), 1.0);
    }

    /// A collider under a moving body moves with it: it blocks nothing,
    /// and neither its spawn nor its despawn redraws the cover.
    #[test]
    fn a_moving_body_s_parts_neither_block_nor_redraw() {
        let mut app = app(flat_ground(0.0));
        app.update();
        settle(&mut app);
        assert_eq!(app.world().resource::<GroundCover>().revision, 1);
        let body = app
            .world_mut()
            .spawn((RigidBody::Dynamic, GlobalTransform::default()))
            .id();
        let part = app
            .world_mut()
            .spawn((
                Collider::cuboid(4.0, 2.0, 4.0),
                GlobalTransform::from_translation(Vec3::new(5.0, 1.0, 5.0)),
                ChildOf(body),
            ))
            .id();
        settle(&mut app);
        assert_eq!(app.world().resource::<GroundCover>().revision, 1);
        assert_eq!(open_at(&app, 5.0, 5.0), 1.0);
        app.world_mut().entity_mut(part).despawn();
        settle(&mut app);
        assert_eq!(app.world().resource::<GroundCover>().revision, 1);
    }

    /// A heightmap replaced in place leaves the last ground's weights
    /// behind, and takes the new ground's when they are published; a
    /// heightmap taken away takes both resources with it.
    #[test]
    fn the_weights_follow_their_own_heightmap() {
        let mut app = app(flat_ground(0.0));
        app.update();
        let weights = |app: &App| app.world().resource::<GroundCover>().weights.clone();
        let first = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default());
        app.insert_resource(SplatWeights(first.clone()));
        app.update();
        assert_eq!(weights(&app), Some(first));
        app.insert_resource(flat_ground(1.0));
        app.update();
        app.update();
        assert_eq!(weights(&app), None, "the last ground's weights");
        let second = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default());
        app.insert_resource(SplatWeights(second.clone()));
        app.update();
        assert_eq!(weights(&app), Some(second));
        app.world_mut().remove_resource::<FinishedHeightMap>();
        app.update();
        app.update();
        assert!(!app.world().contains_resource::<GroundCover>());
        assert!(!app.world().contains_resource::<SplatWeights>());
    }

    /// Without its heightmap the ground's cover goes too.
    #[test]
    fn the_cover_leaves_with_its_heightmap() {
        let mut app = app(flat_ground(0.0));
        app.update();
        assert!(app.world().contains_resource::<GroundCover>());
        app.world_mut().remove_resource::<FinishedHeightMap>();
        app.update();
        app.update();
        assert!(!app.world().contains_resource::<GroundCover>());
    }
}
