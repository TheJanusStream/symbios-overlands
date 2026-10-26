//! What the ground offers whatever grows on it (#1674): the terrain's
//! height as a GPU texture, the splat weights the ground is painted with,
//! and a cover mask of where something may grow at all. The GPU grass
//! ([`crate::grass`], #1675) reads all three.
//!
//! [`GroundCover`] holds them, for the room whose heightmap is up:
//!
//! * **`height`** - one `Rg32Uint` texel per heightmap vertex: its world
//!   height and the normal the terrain's mesh gives it (#1695). An integer
//!   texture cannot be filtered: read it with `textureLoad` and interpolate
//!   as [`mesh_height`] does, and a blade stands on the triangles the ground
//!   is drawn with, lit as the ground there is.
//! * **`weights`** - the splat material's own weight map, a two-layer
//!   array of one RGBA8 texel per heightmap vertex. Layer 0 holds each
//!   ground layer's share, the record's four layers in order (by default
//!   grass, dirt, rock, snow); layer 1's red holds how sheltered the vertex
//!   is from what falls (#1685): the share of the four shelter texels
//!   round it with something standing at least [`SHELTER_CLEARANCE`] over
//!   its ground, so the ground and the grass stay dry under a roof. The
//!   ground's shader samples it linearly at its mesh UV, `(world_xz + half)
//!   / (2 half)` - which puts vertex k at `k / (n - 1)`, not at its texel's
//!   centre, half a texel off at the edges - and whatever grows must sample
//!   it the same way to follow the ground's look. `None` until the ground's
//!   textures have baked.
//! * **`cover`** - one `R8Unorm` texel per metre (two a 2 m cell): 1 where
//!   the ground is open, 0 under a road, a building, any solid placed item
//!   standing on the ground, and wherever the ground lies under water - a
//!   lake's at its own level (#1727) - or within [`WATER_MARGIN`] of it.
//!   Sample it linearly, and its edges fade
//!   across a metre. [`GroundCover::mask`] is the same on the CPU.
//! * **`shelter`** - on the cover's grid, one `Rg16Float` texel each: red
//!   the top of the highest static shape over it, minus infinity under the
//!   open sky ([`shelter`], #1685), which what falls folds away below
//!   (`crate::weather::precipitation`); green the same but for instance
//!   sets' voxel shells, which the world's parts stand dry under (#1686,
//!   `crate::weathered`). [`GroundCover::shelter_map`] is the same on the
//!   CPU.
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
//! **What shelters the ground.** The same static colliders, every piece of
//! them at whatever height it stands - a roof on posts as much as a wall -
//! each raising the shelter under its outline to its top
//! ([`footprint::tops`]).
//!
//! **When it is redrawn.** The height texture when a heightmap lands; the
//! cover and the shelter when the world has held still for
//! [`REBUILD_DEBOUNCE`] after a change - a record edit, a static collider
//! spawned or despawned, a new heightmap - and no compile, road re-mesh or
//! instance build is still under way, the weights' shelter layer with them
//! and whenever the ground's weights bake anew. A redraw rereads every
//! collider and costs a few milliseconds; it happens when a room settles
//! and after an edit, never while one is dragged.
//!
//! [`WATER_MARGIN`]: crate::config::terrain::ground_cover::WATER_MARGIN
//! [`REBUILD_DEBOUNCE`]: crate::config::terrain::ground_cover::REBUILD_DEBOUNCE
//! [`SHELTER_CLEARANCE`]: crate::config::terrain::ground_cover::SHELTER_CLEARANCE

pub(crate) mod footprint;
pub(crate) mod height;
pub(crate) mod mask;
pub(crate) mod shelter;
pub(crate) mod weight_maxima;

use std::sync::Arc;

use avian3d::prelude::{Collider, RigidBody, Sensor};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

pub(crate) use height::mesh_height;
pub use mask::CoverMask;
pub use shelter::ShelterMap;
pub use weight_maxima::WeightMaxima;

use crate::config::terrain::ground_cover as cfg;
use crate::state::LiveRoomRecord;
use crate::terrain::FinishedHeightMap;

/// The room's ground, as whatever grows on it reads it (#1674). Present
/// while a heightmap is.
#[derive(Resource)]
pub struct GroundCover {
    /// How world XZ lands on the three textures.
    pub frame: GroundFrame,
    /// The heightmap's vertices' heights and normals, `Rg32Uint`, GPU only
    /// (`height::height_image`).
    pub height: Handle<Image>,
    /// The splat weight map the ground is painted with, once it has baked.
    pub weights: Option<Handle<Image>>,
    /// The weight map's most, grass tile by grass tile (#1693), with it.
    pub maxima: Option<Arc<WeightMaxima>>,
    /// Where something may grow, `R8Unorm`, GPU only: 1 open, 0 blocked.
    pub cover: Handle<Image>,
    /// The cover on the CPU, as last uploaded.
    pub mask: CoverMask,
    /// What shelters the ground from what falls (#1685), `Rg16Float`, GPU
    /// only: the highest static shape's top over each cover texel, and the
    /// highest but a voxel shell's (#1686).
    pub shelter: Handle<Image>,
    /// The shelter on the CPU, as last uploaded.
    pub shelter_map: ShelterMap,
    /// The weight map's own layer, kept on the CPU to write the shelter
    /// beside it ([`weight_image`]), with the weights.
    pub(crate) weight_layer: Option<Arc<WeightLayer>>,
    /// The lakes written beside the shelter ([`lake_layer`]), as last drawn:
    /// `None` in a room without them.
    pub(crate) lakes: Option<Vec<[u8; 2]>>,
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

    /// An open shelter map over this frame: the cover's grid.
    fn open_shelter(&self) -> ShelterMap {
        ShelterMap::open(
            self.cover_texels as usize,
            self.cover_texel(),
            Vec2::splat(-self.half),
        )
    }
}

/// The splat weight map the ground's material was given, published by the
/// terrain when it bakes (#1674): the handle [`GroundCover::weights`] takes,
/// its most per grass tile, which [`GroundCover::maxima`] takes with it
/// (#1693), and its own layer's bytes, which the shelter is written beside
/// (#1685).
#[derive(Resource, Clone)]
pub(crate) struct SplatWeights(
    pub(crate) Handle<Image>,
    pub(crate) Arc<WeightMaxima>,
    pub(crate) Arc<WeightLayer>,
);

/// A splat weight map's own layer on the CPU (#1685): RGBA8, `width` by
/// `height` texels, one per heightmap vertex.
#[derive(Debug, PartialEq)]
pub(crate) struct WeightLayer {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) bytes: Vec<u8>,
}

/// The splat weight map as the image the ground and the grass sample: a
/// two-layer array, `layer` the weights and the second layer's red
/// `sheltered` (one byte a texel, 0 open to 255 sheltered; open everywhere
/// when `None`), its green and blue the `lakes` ([`lake_layer`]; none when
/// `None`) - linear between texels, clamped at the edges, GPU only. The same
/// size each time, so a rewrite is copied into the texture already bound
/// ([`replace`]).
pub(crate) fn weight_image(
    layer: &WeightLayer,
    sheltered: Option<&[u8]>,
    lakes: Option<&[[u8; 2]]>,
) -> Image {
    use bevy::render::render_resource::{TextureViewDescriptor, TextureViewDimension};
    let texels = (layer.width * layer.height) as usize;
    let mut bytes = Vec::with_capacity(texels * 8);
    bytes.extend_from_slice(&layer.bytes);
    let sheltered = |i: usize| sheltered.map_or(0, |s| s[i]);
    let lakes = lakes.filter(|lakes| lakes.len() == texels);
    let lake = |i: usize| lakes.map_or([0, 0], |l| l[i]);
    bytes.extend((0..texels).flat_map(|i| {
        let [level, shore] = lake(i);
        [sheltered(i), level, shore, 0]
    }));
    let mut image = Image::new(
        Extent3d {
            width: layer.width,
            height: layer.height,
            depth_or_array_layers: 2,
        },
        TextureDimension::D2,
        bytes,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..Default::default()
    });
    image
}

/// How sheltered each heightmap vertex of `hm` is from what falls (#1685),
/// one byte each in the weight map's order: the share of the four
/// `shelter` texels round the vertex whose top stands at least
/// [`cfg::SHELTER_CLEARANCE`] over the ground under that texel - not the
/// vertex's, which on a steep slope lies a hand's breadth off. A roof's
/// edge comes out half way, and the GPU's linear sampling eases the dry
/// ground in under it. The ground samples this layer at its mesh UV, which
/// puts vertex k half a texel off its texel's centre at the map's edges
/// (see the module's `weights`): there the dry ground shifts by up to half
/// a cell toward the middle.
///
/// Read from the covered texels alone, as most of a world is open sky: at
/// two texels a cell each texel's centre is one of exactly one vertex's
/// four, half a texel from it on both axes, so it counts for the vertex
/// nearest it.
fn sheltered_layer(
    frame: &GroundFrame,
    hm: &bevy_symbios_ground::HeightMap,
    shelter: &ShelterMap,
) -> Vec<u8> {
    const _: () = assert!(
        cfg::TEXELS_PER_CELL == 2,
        "a shelter texel's centre must be one vertex's sample"
    );
    let n = frame.vertices as usize;
    let mut counts = vec![0u8; n * n];
    for (centre, top) in shelter.covered() {
        if top < mesh_height(hm, centre) + cfg::SHELTER_CLEARANCE {
            continue;
        }
        let vertex = ((centre + Vec2::splat(frame.half)) / frame.cell).round();
        let (col, row) = (vertex.x as usize, vertex.y as usize);
        if vertex.min_element() >= 0.0 && col < n && row < n {
            counts[row * n + col] += 1;
        }
    }
    counts
        .into_iter()
        .map(|c| (usize::from(c) * 255 / 4) as u8)
        .collect()
}

/// The lakes over each heightmap vertex of `hm` (#1727), two bytes each in
/// the weight map's order, for its second layer's green and blue: how far
/// the lake's level stands over the vertex, `LAKE_SPAN` about the middle
/// (`config::terrain::splat`); and 255 where a lake's reach takes the vertex
/// in (`water::bodies::BodyCells::reach`), else 0 - its water, and the bank
/// reached from it standing under the damp margin over its level - the
/// highest lake's where two reach one vertex. There the ground is damp to
/// the lake's level and its snow and grass stop at its water, as they do at
/// the sea's (`splat.wgsl`). `None` in a room without lakes: the ground
/// reads the sea's alone.
pub(crate) fn lake_layer(
    hm: &bevy_symbios_ground::HeightMap,
    water: &crate::water::bodies::RoomWater,
) -> Option<Vec<[u8; 2]>> {
    use crate::config::terrain::splat::LAKE_SPAN;
    let grid = crate::water::bodies::CellGrid::of(hm);
    if !water.bodies.iter().any(|body| body.cells.grid == grid) {
        return None;
    }
    let w = hm.width();
    let mut levels = vec![f32::NEG_INFINITY; w * hm.height()];
    let mut any = false;
    for body in water.bodies.iter().filter(|body| body.cells.grid == grid) {
        for (cz, first, past) in body.cells.reach.runs() {
            for cx in first..past {
                let i = cz as usize * w + cx as usize;
                levels[i] = levels[i].max(body.level);
                any = true;
            }
        }
    }
    any.then(|| {
        levels
            .iter()
            .zip(hm.data())
            .map(|(&level, &ground)| {
                if level.is_finite() {
                    let over = (0.5 + (level - ground) / LAKE_SPAN).clamp(0.0, 1.0);
                    [(over * 255.0).round() as u8, 255]
                } else {
                    [0, 0]
                }
            })
            .collect()
    })
}

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
    let shelter_map = frame.open_shelter();
    match cover {
        Some(mut cover) => {
            replace(&mut images, &cover.height, height::height_image(hm));
            replace(&mut images, &cover.cover, cover_image(&mask));
            replace(&mut images, &cover.shelter, shelter_map.image());
            cover.frame = frame;
            cover.mask = mask;
            cover.shelter_map = shelter_map;
            cover.revision = 0;
            cover.weights = None;
            cover.maxima = None;
            cover.weight_layer = None;
            cover.lakes = None;
        }
        None => commands.insert_resource(GroundCover {
            frame,
            height: images.add(height::height_image(hm)),
            weights: None,
            maxima: None,
            cover: images.add(cover_image(&mask)),
            mask,
            shelter: images.add(shelter_map.image()),
            shelter_map,
            weight_layer: None,
            lakes: None,
            revision: 0,
        }),
    }
}

/// The ground's textures have baked: take its weight map - but only one
/// published after this heightmap landed. Weights published before belong
/// to the last ground, whose heightmap this one replaced in place. Writes
/// only a change. A bake after the shelter was drawn has it written beside
/// the new weights at once (#1685). The heightmap can go at a sync point
/// inside this chain - the terrain regenerating under a live edit - after
/// [`drop_ground_cover`] has passed on the frame; this one then waits, and
/// the next frame drops the cover (#1738: a hard `Res` there panicked the
/// client on a live splat-layer edit).
fn sync_weights(
    weights: Option<Res<SplatWeights>>,
    heightmap: Option<Res<FinishedHeightMap>>,
    mut cover: ResMut<GroundCover>,
    mut images: ResMut<Assets<Image>>,
    ticks: bevy::ecs::system::SystemChangeTick,
) {
    let (Some(weights), Some(heightmap)) = (weights, heightmap) else {
        return;
    };
    let current = weights
        .last_changed()
        .is_newer_than(heightmap.last_changed(), ticks.this_run());
    if current && cover.weights.as_ref() != Some(&weights.0) {
        cover.weights = Some(weights.0.clone());
        cover.maxima = Some(weights.1.clone());
        cover.weight_layer = Some(weights.2.clone());
        if cover.revision > 0 {
            write_shelter_layer(&mut images, &cover, &heightmap.0);
        }
    }
}

/// Write the shelter beside the weights (#1685): the weight map rebuilt in
/// place with its second layer from [`GroundCover::shelter_map`]. Nothing
/// without weights yet, or with weights of another heightmap's size.
fn write_shelter_layer(
    images: &mut Assets<Image>,
    cover: &GroundCover,
    hm: &bevy_symbios_ground::HeightMap,
) {
    let (Some(handle), Some(layer)) = (&cover.weights, &cover.weight_layer) else {
        return;
    };
    if layer.width != cover.frame.vertices || layer.height != cover.frame.vertices {
        return;
    }
    let sheltered = sheltered_layer(&cover.frame, hm, &cover.shelter_map);
    replace(
        images,
        handle,
        weight_image(layer, Some(&sheltered), cover.lakes.as_deref()),
    );
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
    colliders: Query<
        (
            Entity,
            &Collider,
            &GlobalTransform,
            Has<crate::world_builder::instances::VoxelShell>,
        ),
        Without<Sensor>,
    >,
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
        .map(|r| crate::world_builder::compile::drawn_water(&r.0, Some(&*heightmap)))
        .unwrap_or_default();
    if !water.is_dry() {
        block_water(&mut mask, hm, &water, cfg::WATER_MARGIN);
    }
    // A world's paths and tracks grow no grass (#1779, #1782), nor its
    // fields (#1791).
    if let Some(paths) = record
        .as_ref()
        .and_then(|r| crate::pds::find_terrain_config(&r.0))
        .map(|cfg| &cfg.paths)
    {
        for (line, width) in crate::terrain::path_ways(paths, Some((hm, &water))) {
            mask.block_path(&line, width);
        }
        for field in crate::terrain::path_fields(paths, Some((hm, &water))) {
            mask.block_convex(&field.corners().map(Vec2::from_array));
        }
    }

    let mut outlines = 0usize;
    let mut shelter = cover.frame.open_shelter();
    let mut sheltering = std::time::Duration::ZERO;
    dirty.statics.clear();
    for (entity, collider, transform, shell) in &colliders {
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
        let began = bevy::platform::time::Instant::now();
        footprint::tops(
            collider.shape(),
            transform.affine(),
            &ground,
            &mut |outline, top| shelter.raise(outline, top, shell),
        );
        sheltering += began.elapsed();
    }

    replace(&mut images, &cover.cover, cover_image(&mask));
    let (open, covered) = (mask.open_share(), shelter.covered_share());
    cover.mask = mask;
    // Most redraws - an edit that moves no static shape and no lake - leave
    // the shelter and the lakes as they were: their uploads and the weights'
    // rewrite are skipped then.
    let began = bevy::platform::time::Instant::now();
    let lakes = lake_layer(hm, &water);
    let lakes_moved = lakes != cover.lakes;
    cover.lakes = lakes;
    let sheltered_moved = shelter != cover.shelter_map;
    if sheltered_moved {
        replace(&mut images, &cover.shelter, shelter.image());
        cover.shelter_map = shelter;
    }
    if sheltered_moved || lakes_moved {
        write_shelter_layer(&mut images, &cover, hm);
    }
    sheltering += began.elapsed();
    info!(
        "ground cover drawn: {} outlines, {:.1}% open, {:.1}% covered, {:.1} ms ({:.1} ms of it the shelter)",
        outlines,
        open * 100.0,
        covered * 100.0,
        started.elapsed().as_secs_f64() * 1000.0,
        sheltering.as_secs_f64() * 1000.0
    );
    cover.revision += 1;
    dirty.since = None;
}

/// Block the ground lower than the water's line: its level at each place,
/// a lake's in its cells and the sea's elsewhere (#1727), and `margin` over
/// it. The drawn ground is flat between a cell's corners, so a cell whose
/// four corners share a line and all stand above it is dry throughout and
/// one whose four all lie under it is wet throughout; only a cell the shore
/// crosses, or a lake's edge, is tested texel by texel, each texel against
/// the line at its own place.
fn block_water(
    mask: &mut CoverMask,
    hm: &bevy_symbios_ground::HeightMap,
    water: &crate::water::bodies::RoomWater,
    margin: f32,
) {
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
    let line_at = |xz: Vec2| water.level_at(xz.x, xz.y).map(|level| level + margin);
    let grid = crate::water::bodies::CellGrid::of(hm);
    let line_of = |(x, z): (usize, usize)| {
        water
            .level_at_cell(grid, (x as u32, z as u32))
            .map(|level| level + margin)
    };
    let mut wet = vec![Cell::Dry; (w - 1) * (h - 1)];
    for z in 0..h - 1 {
        for x in 0..w - 1 {
            let corners = [(x, z), (x + 1, z), (x, z + 1), (x + 1, z + 1)];
            let line = line_of((x, z));
            wet[z * (w - 1) + x] = if corners[1..].iter().any(|&corner| line_of(corner) != line) {
                Cell::Shore
            } else if let Some(line) = line {
                if corners.iter().all(|&(cx, cz)| hm.get(cx, cz) < line) {
                    Cell::Under
                } else if corners.iter().any(|&(cx, cz)| hm.get(cx, cz) < line) {
                    Cell::Shore
                } else {
                    Cell::Dry
                }
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
            Cell::Shore => line_at(xz).is_some_and(|line| mesh_height(hm, xz) < line),
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
/// new one - and one of the same size and format is copied into the GPU
/// texture already bound, so a material binding it is not rebuilt (the
/// grass's bodies, #1675, rely on it). A handle held strongly cannot
/// outlive its slot; were it ever to, the old texture would stay, and the
/// log says so.
pub(crate) fn replace(images: &mut Assets<Image>, handle: &Handle<Image>, image: Image) {
    if let Err(error) = images.insert(handle, image) {
        warn!("could not replace a texture in place: {error}");
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

    /// The shelter (#1685): a roof held clear of the ground blocks no cover
    /// but shelters the ground under it up to its top, and a moving body
    /// and a sensor shelter nothing. With the ground's weights in, the
    /// weight map's second layer marks the vertices under the roof
    /// sheltered, those on its edge half, and the open ones not, its first
    /// layer untouched; and what falls is given the shelter once it is drawn.
    #[test]
    fn a_roof_shelters_the_ground_under_it() {
        let mut app = app(flat_ground(1.0));
        let at = |x: f32, y: f32, z: f32| GlobalTransform::from_translation(Vec3::new(x, y, z));
        // Longer along X than along Z (x -16..-4, z -13..-7), so a layer
        // written with X and Z swapped reads wrong below.
        app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(12.0, 0.4, 6.0),
            at(-10.0, 5.0, -10.0),
        ));
        app.world_mut().spawn((
            RigidBody::Dynamic,
            Collider::cuboid(6.0, 4.0, 6.0),
            at(10.0, 3.0, 10.0),
        ));
        app.world_mut().spawn((
            Sensor,
            Collider::cuboid(6.0, 4.0, 6.0),
            at(-10.0, 3.0, 10.0),
        ));
        app.update();
        let layer = Arc::new(WeightLayer {
            width: 33,
            height: 33,
            bytes: [255, 0, 0, 0].repeat(33 * 33),
        });
        let weights = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(weight_image(&layer, None, None));
        let maxima = Arc::new(WeightMaxima::of(&[[255, 0, 0, 0]; 4], 2, 2, 8.0));
        app.insert_resource(SplatWeights(weights.clone(), maxima, layer.clone()));
        app.update();
        let (_, frame) =
            crate::weather::precipitation::shelter_of(app.world().get_resource::<GroundCover>());
        assert_eq!(frame, Vec4::ZERO, "no shelter before it is drawn");
        settle(&mut app);

        let cover = app.world().resource::<GroundCover>();
        let top = |x: f32, z: f32| cover.shelter_map.top_at(Vec2::new(x, z));
        assert!((top(-10.0, -10.0) - 5.2).abs() < 1e-4, "under the roof");
        assert_eq!(top(-10.0, -14.0), f32::NEG_INFINITY, "past its short side");
        assert_eq!(top(10.0, 10.0), f32::NEG_INFINITY, "under the moving body");
        assert_eq!(top(-10.0, 10.0), f32::NEG_INFINITY, "under the sensor");
        assert_eq!(open_at(&app, -10.0, -10.0), 1.0, "the roof blocks no cover");
        let (shelter, frame) = crate::weather::precipitation::shelter_of(Some(cover));
        assert_eq!(shelter, cover.shelter);
        assert_eq!(frame, Vec4::new(-32.0, -32.0, 1.0, 64.0));

        let images = app.world().resource::<Assets<Image>>();
        let data = images
            .get(&weights)
            .and_then(|i| i.data.as_ref())
            .expect("the weight map's texels");
        assert_eq!(data.len(), 33 * 33 * 8, "two layers");
        assert_eq!(
            &data[..33 * 33 * 4],
            &layer.bytes[..],
            "the weights as they were"
        );
        // Vertex (k, l) stands at (-32 + 2k, -32 + 2l).
        let sheltered = |x: f32, z: f32| {
            let (k, l) = (((x + 32.0) / 2.0) as usize, ((z + 32.0) / 2.0) as usize);
            data[33 * 33 * 4 + (l * 33 + k) * 4]
        };
        assert_eq!(sheltered(-10.0, -10.0), 255);
        assert_eq!(sheltered(-14.0, -10.0), 255, "along its long side");
        assert_eq!(sheltered(-10.0, -14.0), 0, "past its short side");
        assert_eq!(sheltered(-4.0, -10.0), 127, "half on the roof's edge");
        assert_eq!(sheltered(0.0, 0.0), 0);
        assert_eq!(sheltered(10.0, 10.0), 0, "under the moving body");
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

    /// A valley at 5 m in the west, a ridge at 10 m, and in the east a round
    /// dip 20 m across, its floor at 6 m round (16, 0): on 33 x 33 vertices
    /// 2 m apart.
    fn dip_and_valley() -> bevy_symbios_ground::HeightMap {
        let mut hm = bevy_symbios_ground::HeightMap::new(33, 33, 2.0);
        for z in 0..33 {
            for x in 0..33 {
                let (wx, wz) = (x as f32 * 2.0 - 32.0, z as f32 * 2.0 - 32.0);
                let dip = ((wx - 16.0).powi(2) + wz.powi(2)).sqrt();
                let y = if wx < -12.0 {
                    5.0
                } else if dip < 10.0 {
                    6.0 + dip * 0.4
                } else {
                    10.0
                };
                hm.set(x, z, y);
            }
        }
        hm
    }

    /// A room whose terrain has its sea at 0 m and a lake at 8 m in the dip.
    fn dip_lake_record() -> RoomRecord {
        let mut record = RoomRecord::default();
        record.generators.clear();
        let mut terrain = Generator::from_kind(GeneratorKind::Terrain(Default::default()));
        for (x, level) in [(0.0, 0.0), (16.0, 8.0)] {
            let mut water = Generator::from_kind(GeneratorKind::Water {
                surface: Default::default(),
            });
            water.transform = TransformData {
                translation: Fp3([x, level, 0.0]),
                ..Default::default()
            };
            terrain.children.push(water);
        }
        record.generators.insert("terrain".into(), terrain);
        record
    }

    /// #1727: a lake blocks the ground in its dip up to its own level, and
    /// not the lower valley beyond its reach, which the sea under it leaves
    /// open - and its shore is written beside the shelter, where the ground
    /// reads it.
    #[test]
    fn a_lake_blocks_its_dip_and_not_the_valley_beyond_it() {
        let mut app = app(FinishedHeightMap(dip_and_valley(), None));
        app.insert_resource(LiveRoomRecord(dip_lake_record()));
        app.update();
        let layer = Arc::new(WeightLayer {
            width: 33,
            height: 33,
            bytes: [255, 0, 0, 0].repeat(33 * 33),
        });
        let weights = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(weight_image(&layer, None, None));
        let maxima = Arc::new(WeightMaxima::of(&[[255, 0, 0, 0]; 4], 2, 2, 8.0));
        app.insert_resource(SplatWeights(weights.clone(), maxima, layer));
        app.update();
        settle(&mut app);
        assert_eq!(open_at(&app, 16.0, 0.0), 0.0, "the lake's floor");
        assert_eq!(open_at(&app, 16.0, 3.0), 0.0, "in the lake");
        assert_eq!(
            open_at(&app, 16.0, 8.0),
            1.0,
            "the dip's bank, over the lake"
        );
        assert_eq!(
            open_at(&app, -24.0, 0.0),
            1.0,
            "the valley, below the lake but out of its reach"
        );
        assert_eq!(open_at(&app, 0.0, 0.0), 1.0, "the ridge");

        let images = app.world().resource::<Assets<Image>>();
        let data = images
            .get(&weights)
            .and_then(|i| i.data.as_ref())
            .expect("the weight map's texels");
        // Vertex (k, l) stands at (-32 + 2k, -32 + 2l): its second layer's
        // green and blue.
        let lake = |x: f32, z: f32| {
            let (k, l) = (((x + 32.0) / 2.0) as usize, ((z + 32.0) / 2.0) as usize);
            let i = 33 * 33 * 4 + (l * 33 + k) * 4;
            [data[i + 1], data[i + 2]]
        };
        assert_eq!(lake(16.0, 0.0), [159, 255], "2 m under the lake");
        assert_eq!(lake(-24.0, 0.0), [0, 0], "the valley");
    }

    /// #1727: a lake's shore is its cells and the ground over it its damp
    /// margin reaches, each with how far the lake's level stands over it;
    /// the valley below it, out of its reach, is the sea's. A room without
    /// lakes has no lake layer at all.
    #[test]
    fn a_lakes_shore_is_its_water_and_the_bank_its_margin_reaches() {
        let terrain = FinishedHeightMap(dip_and_valley(), None);
        let water = crate::world_builder::compile::drawn_water(&dip_lake_record(), Some(&terrain));
        let layer = lake_layer(&terrain.0, &water).expect("a lake");
        let at = |x: f32, z: f32| {
            let (k, l) = (((x + 32.0) / 2.0) as usize, ((z + 32.0) / 2.0) as usize);
            layer[l * 33 + k]
        };
        // (0.5 + over / LAKE_SPAN) of 255: 2 m under, and 1.2 m over.
        assert_eq!(at(16.0, 0.0), [159, 255], "the lake's floor");
        assert_eq!(at(16.0, 8.0), [108, 255], "the bank");
        assert_eq!(at(0.0, 0.0)[1], 255, "the ridge, 2 m over the lake");
        assert_eq!(
            at(-24.0, 0.0),
            [0, 0],
            "the valley, below the lake but out of its reach"
        );
        let sea = crate::water::bodies::RoomWater::sea_alone(Some(0.0));
        assert_eq!(lake_layer(&terrain.0, &sea), None, "no lake, no layer");
    }

    /// #1724: in a region built from real Berlin, whose own bodies are its
    /// water and no sea spans it, each body blocks the ground under it, and
    /// the hollow beside it that is lower but no water stays open.
    #[test]
    fn berlins_own_water_blocks_its_cover() {
        // Ground at 10 m, a pool round (-10, 0) whose bed lies at 5 m, and
        // an underpass at 4 m round (14, 0) that is no water.
        let mut hm = bevy_symbios_ground::HeightMap::new(33, 33, 2.0);
        let mut cover = Vec::new();
        for z in 0..33 {
            for x in 0..33 {
                let (wx, wz) = (x as f32 * 2.0 - 32.0, z as f32 * 2.0 - 32.0);
                let pool = (wx + 10.0).abs() < 6.0 && wz.abs() < 6.0;
                let underpass = (wx - 14.0).abs() < 4.0 && wz.abs() < 4.0;
                hm.set(
                    x,
                    z,
                    if pool {
                        5.0
                    } else if underpass {
                        4.0
                    } else {
                        10.0
                    },
                );
                cover.push(Some(if pool {
                    geodata::berlin::LandUse::Water
                } else {
                    geodata::berlin::LandUse::Park
                }));
            }
        }
        let water = crate::terrain::geo::GeoGround::water_of_cover(&hm, &cover, 8.0);
        let berlin = crate::terrain::geo::GeoGround::from_cover(33, 2.0, cover, water);
        let mut app = app(FinishedHeightMap(hm, Some(berlin)));
        let mut record = dip_lake_record();
        record
            .generators
            .values_mut()
            .for_each(|terrain| terrain.children.truncate(1));
        app.insert_resource(LiveRoomRecord(record));
        app.update();
        settle(&mut app);
        assert_eq!(open_at(&app, -10.0, 0.0), 0.0, "the pool");
        assert_eq!(
            open_at(&app, 14.0, 0.0),
            1.0,
            "the underpass, lower, but dry"
        );
        assert_eq!(open_at(&app, 0.0, 20.0), 1.0, "the ground");
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
        let maxima = |app: &App| app.world().resource::<GroundCover>().maxima.clone();
        // Each ground's maxima tell it apart: one tile of grass, or of dirt.
        let of = |texel: [u8; 4]| Arc::new(WeightMaxima::of(&[texel; 4], 2, 2, 8.0));
        let layer = || {
            Arc::new(WeightLayer {
                width: 33,
                height: 33,
                bytes: vec![0; 33 * 33 * 4],
            })
        };
        let first = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default());
        app.insert_resource(SplatWeights(first.clone(), of([255, 0, 0, 0]), layer()));
        app.update();
        assert_eq!(weights(&app), Some(first));
        assert_eq!(maxima(&app), Some(of([255, 0, 0, 0])));
        app.insert_resource(flat_ground(1.0));
        app.update();
        app.update();
        assert_eq!(weights(&app), None, "the last ground's weights");
        assert_eq!(maxima(&app), None, "the last ground's maxima");
        let second = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default());
        app.insert_resource(SplatWeights(second.clone(), of([0, 255, 0, 0]), layer()));
        app.update();
        assert_eq!(weights(&app), Some(second));
        assert_eq!(maxima(&app), Some(of([0, 255, 0, 0])));
        app.world_mut().remove_resource::<FinishedHeightMap>();
        app.update();
        app.update();
        assert!(!app.world().contains_resource::<GroundCover>());
        assert!(!app.world().contains_resource::<SplatWeights>());
    }

    /// #1738: a heightmap that goes between the chain's first step and the
    /// weights' sync - as the terrain regenerating under a live edit takes
    /// it at a sync point inside the chain - leaves the cover alone for that
    /// frame and drops it the next. A hard `Res` there panicked the client.
    #[test]
    fn a_heightmap_gone_inside_the_chain_waits_a_frame() {
        let mut app = app(flat_ground(0.0));
        app.update();
        assert!(app.world().contains_resource::<GroundCover>());
        let weights = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default());
        let maxima = Arc::new(WeightMaxima::of(&[[255, 0, 0, 0]; 4], 2, 2, 8.0));
        let layer = Arc::new(WeightLayer {
            width: 33,
            height: 33,
            bytes: vec![0; 33 * 33 * 4],
        });
        app.insert_resource(SplatWeights(weights, maxima, layer));
        app.add_systems(
            Update,
            (|world: &mut World| {
                world.remove_resource::<FinishedHeightMap>();
            })
            .after(drop_ground_cover)
            .before(sync_weights),
        );
        app.update();
        assert!(app.world().contains_resource::<GroundCover>());
        app.update();
        assert!(!app.world().contains_resource::<GroundCover>());
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
