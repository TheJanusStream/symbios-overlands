//! The GPU grass (#1675): a field of blades around the camera, grown on
//! whatever ground grows grass, in every room, from what its record already
//! stores.
//!
//! * **What it looks like** is the room's own ground cover: its grass
//!   tufts' colours, its flowers, how its layers' ground grows grass
//!   ([`profile`]). No record carries a grass field; a room made before the
//!   grass came has grass all the same. It varies as the ground under it
//!   does (#1676): drier where the ground's tone drifts lighter, taller
//!   where it is darker patch by patch - one pair of fields, in
//!   `assets/shaders/ground_common.wgsl`, read by both shaders.
//! * **Where it grows** is the ground under it, read on the GPU from
//!   [`GroundCover`]: blades stand on the drawn ground, grow as thickly as
//!   the splat layers there allow, and not at all under a building, a road
//!   or water.
//! * **How it is drawn**: square tiles of blades on a world grid, laid
//!   around the camera out to the quality setting's reach, each drawing a
//!   prefix of the same blades - the fewest that hold what can grow in it,
//!   so a far tile or one on thin ground draws few, and none is laid where
//!   nothing can grow (#1693) - with fewer segments a blade the further it
//!   is ([`tiles`]). Past a few tens of metres the field thins as the
//!   square of the distance, as many blades a patch of screen however far
//!   it lies, which is what lets it reach 128 m; out there it is laid in
//!   16 and 32 m tiles, and each blade takes the shape of fewer segments
//!   before its tile draws them (#1694). A tile the ground hides from the
//!   camera, behind a hill, is laid hidden ([`horizon`]). The vertex shader
//!   does the rest ([`material`]), and a blade it does not grow leaves it at
//!   once.
//! * **What moves it**: the room's wind (the vegetation wind the cards
//!   sway in) and a storm's gust fronts over it (#1684,
//!   `storm_common.wgsl`), and the bodies nearest the camera, which part
//!   it, each over its own collider's footprint - so a parked car stands
//!   on flattened grass rather than in blades poking through its floor.
//! * **The weather** on the ground weathers it too (#1683), from the
//!   terrain's [`GroundWeatherNow`] and the same shared fields: snow buries
//!   most of a blade, a puddle drowns it, wet blades darken and shine.
//!
//! The field waits for the ground's cover, so it never grows through a
//! building for the half second before the cover is drawn, and is gone
//! when [`GrassQuality::Off`] is picked or the room has no ground. Its
//! systems run in `PostUpdate`, after transforms propagate: after the
//! ground cover's (a heightmap replaced this frame drops the field this
//! frame, before it can draw on the new ground's all-open cover), and with
//! the camera where this frame draws it, which is where the shader
//! measures each blade's distance from.

pub mod horizon;
pub mod material;
pub mod profile;
pub mod tiles;

use std::collections::HashMap;

use avian3d::prelude::ColliderAabb;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{NoAutoAabb, VisibilitySystems};
use bevy::ecs::change_detection::Tick;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::transform::TransformSystems;

use crate::camera::WorldCamera;
use crate::config::grass as cfg;
use crate::ground_cover::GroundCover;
use crate::state::{LiveRoomRecord, LocalPlayer, LocalSettings, RemotePeer};
use crate::terrain::FinishedHeightMap;
use crate::terrain::{GroundWeather, GroundWeatherNow};
use crate::wind::VegetationWind;

use material::{GrassExtension, GrassMaterial, GrassUniforms};
use profile::{GrassProfile, grass_profile};
use tiles::{Detail, TileKey};

/// How much grass is drawn (#1675): a per-account setting,
/// [`LocalSettings::grass_quality`], which holds one only once it is
/// picked. Until then each build draws its own default, so a WebGPU
/// session's prefs do not carry its Medium into a WebGL2 one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GrassQuality {
    /// None.
    Off,
    /// A short reach and a thin field: the WebGL2 build's default.
    Low,
    /// The WebGPU build's default.
    Medium,
    /// The desktop default.
    High,
}

impl Default for GrassQuality {
    fn default() -> Self {
        if cfg!(webgl2) {
            Self::Low
        } else if cfg!(target_arch = "wasm32") {
            Self::Medium
        } else {
            Self::High
        }
    }
}

impl GrassQuality {
    /// Every setting, in the settings window's order.
    pub const ALL: [Self; 4] = [Self::Off, Self::Low, Self::Medium, Self::High];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        }
    }

    /// The field this setting draws; `None` when off.
    pub fn field(self) -> Option<cfg::Field> {
        match self {
            Self::Off => None,
            Self::Low => Some(cfg::LOW),
            Self::Medium => Some(cfg::MEDIUM),
            Self::High => Some(cfg::HIGH),
        }
    }

    /// The setting `settings` draws at: the one picked, or the build's
    /// default.
    pub fn of(settings: &LocalSettings) -> Self {
        settings.grass_quality.unwrap_or_default()
    }
}

/// The field: the material its tiles draw with, their meshes, and the tiles
/// standing.
#[derive(Resource, Default)]
pub struct GrassField {
    /// The material, while there is ground to grow on.
    material: Option<Handle<GrassMaterial>>,
    /// The room's grass, as last read from its record.
    profile: Option<GrassProfile>,
    /// The bodies the grass parts around, as the texture the shader reads
    /// ([`bender_image`]), and what it was last written with.
    benders: Option<Handle<Image>>,
    bodies: [Vec4; cfg::MAX_BENDERS],
    /// Where a near tile's blades stand, in rank order ([`tiles::roots`]),
    /// which every mesh is cut from, and how many.
    roots: Vec<Vec2>,
    blades: usize,
    /// The meshes made so far, by detail, step of [`tiles::SHARES`] and
    /// tile size, and the vertices each holds (#1693, #1694).
    meshes: HashMap<(Detail, usize, u32), (Handle<Mesh>, usize)>,
    /// The tiles standing, by where they lie.
    tiles: HashMap<TileKey, Entity>,
    /// Each tile's lowest and highest ground, read from the heightmap the
    /// tiles stand on.
    heights: HashMap<TileKey, (f32, f32)>,
    /// The most the shader can grow in each tile ([`tiles::most_density`]),
    /// for the ground and grass `densities_of` names (#1693).
    densities: HashMap<TileKey, f32>,
    densities_of: Option<DensitiesOf>,
    /// The ground's lowest heights, for the horizon test, and the change of
    /// the heightmap they were read from (#1693): kept while the field is
    /// down, as they depend on nothing else.
    floor: Option<(Tick, horizon::GroundFloor)>,
    /// Each tile last found seen from the world camera, and where the camera
    /// stood then: a seen tile is looked at again only once the camera has
    /// moved [`cfg::HORIZON_RETEST`] (a tile left drawn a moment longer than
    /// it is seen costs only that), a hidden one every frame (#1693).
    seen_from: HashMap<TileKey, Vec3>,
}

/// What a tile's most density was read from: the cover's redraw, the
/// weights' maxima and the room's grass layers. Any change reads them anew.
#[derive(Clone, Debug)]
struct DensitiesOf {
    cover: u32,
    maxima: Option<std::sync::Arc<crate::ground_cover::WeightMaxima>>,
    layers: [u32; 4],
}

impl DensitiesOf {
    fn of(ground: &GroundCover, profile: Option<&GrassProfile>) -> Self {
        Self {
            cover: ground.revision,
            maxima: ground.maxima.clone(),
            layers: profile
                .map_or([1.0; 4], |p| p.layer_density)
                .map(f32::to_bits),
        }
    }

    /// Whether `self` and `other` name the same ground and grass: the same
    /// redraw, the very same maxima, the same layers.
    fn same(&self, other: &Self) -> bool {
        let maxima = match (&self.maxima, &other.maxima) {
            (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        self.cover == other.cover && maxima && self.layers == other.layers
    }
}

/// A camera besides the world camera that draws the world for someone -
/// the agent's look (`agent::daemon::look`) - which the field lays its
/// tiles for too (#1693): each tile holds what the nearest such camera
/// needs, and is hidden only from them all.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct GrassViewer;

/// A body the grass parts around, besides the players: what the render
/// tool's walkers carry.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PartsGrass;

/// A tile of the field, where `key` lays it, drawing its mesh at `detail`
/// holding the blades ranked below `tiles::SHARES[share]` in each of its
/// 8 m cells.
#[derive(Component, Debug)]
pub struct GrassTile {
    pub key: TileKey,
    pub detail: Detail,
    pub share: usize,
}

/// What the field drew this frame (#1692): the tiles in view at each
/// detail and the vertices they submit, each of which the vertex shader
/// runs at least once whether its blade grows or not - the measure the
/// field's cost is taken by, as the `runtime.grass.*` gauges and in
/// `render --bench`.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GrassDrawn {
    /// Tiles in view, near to far ([`Detail::ALL`]'s order).
    pub tiles: [u32; 3],
    /// The vertices those tiles submit.
    pub vertices: u64,
    /// Tiles laid hidden behind the ground (#1693): drawn by no view.
    pub hidden: u32,
}

pub struct GrassPlugin;

impl Plugin for GrassPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<GrassMaterial>::default())
            .init_resource::<GrassField>()
            .init_resource::<GrassDrawn>()
            .add_systems(
                PostUpdate,
                (
                    // Before visibility propagates, too: a tile laid hidden
                    // or shown takes effect this frame, not the next.
                    (sync_grass_material, lay_grass_tiles)
                        .chain()
                        .after(TransformSystems::Propagate)
                        .before(VisibilitySystems::VisibilityPropagate)
                        .before(VisibilitySystems::CheckVisibility),
                    // After the lights' visibility too, which also writes
                    // `ViewVisibility`: the frame's final value.
                    count_grass_drawn.after(VisibilitySystems::MarkNewlyHiddenEntitiesInvisible),
                ),
            );
    }
}

/// Count what the field draws this frame ([`GrassDrawn`]): once visibility
/// is decided, the tiles any view sees, each once however many views see
/// it. The tiles cast no shadow (`NotShadowCaster`), so no light's view
/// marks one seen.
fn count_grass_drawn(
    field: Res<GrassField>,
    tiles: Query<(&GrassTile, &ViewVisibility, &Visibility)>,
    mut drawn: ResMut<GrassDrawn>,
) {
    let mut now = GrassDrawn::default();
    for (tile, seen, laid) in &tiles {
        if *laid == Visibility::Hidden {
            now.hidden += 1;
        }
        if seen.get() {
            now.tiles[tile.detail.index()] += 1;
            now.vertices += field
                .meshes
                .get(&(tile.detail, tile.share, tile.key.level))
                .map_or(0, |&(_, vertices)| vertices as u64);
        }
    }
    drawn.set_if_neq(now);
}

/// The bodies the grass parts around, with their collider bounds if they
/// have any.
type Bodies<'w, 's> = Query<
    'w,
    's,
    (&'static GlobalTransform, Option<&'static ColliderAabb>),
    Or<(With<LocalPlayer>, With<RemotePeer>, With<PartsGrass>)>,
>;

/// Keep the field's material in step with the room: its record's grass,
/// its ground's textures and weather, the wind and the quality setting;
/// and the bodies standing in the grass in their texture. Writes the
/// material only when something in it changed, since a write rebuilds its
/// bind group and re-specializes every tile.
#[allow(clippy::too_many_arguments)]
fn sync_grass_material(
    mut field: ResMut<GrassField>,
    mut materials: ResMut<Assets<GrassMaterial>>,
    mut images: ResMut<Assets<Image>>,
    ground: Option<Res<GroundCover>>,
    record: Option<Res<LiveRoomRecord>>,
    settings: Res<LocalSettings>,
    wind: Res<VegetationWind>,
    weather: Option<Res<GroundWeatherNow>>,
    camera: Query<&GlobalTransform, With<WorldCamera>>,
    bodies: Bodies,
) {
    let field = &mut *field;
    // The room's grass, read whenever its record changes - before any gate
    // below, which would otherwise swallow the change for good: a record
    // that lands before its ground (every room after the login backdrop's)
    // would keep the last room's grass.
    let Some(record) = record else {
        field.material = None;
        field.profile = None;
        return;
    };
    if record.is_changed() || field.profile.is_none() {
        let profile = grass_profile(&record.0);
        if field.profile.as_ref() != Some(&profile) {
            field.profile = Some(profile);
        }
    }
    let grown = GrassQuality::of(&settings).field();
    let weights = ground
        .as_ref()
        .and_then(|g| g.weights.clone().filter(|_| g.revision > 0));
    let (Some(grown), Some(ground), Some(weights), Some(profile)) =
        (grown, ground, weights, field.profile.as_ref())
    else {
        field.material = None;
        return;
    };
    let weather = weather.map_or_else(GroundWeather::default, |w| w.0);
    let uniforms = uniforms(profile, &ground, &wind, &grown, &weather);
    let shine = roughness(weather.wetness);

    // The bodies, in a texture written in place: a write of the same size
    // goes into the texture the material already binds, where a material
    // written every frame a body moves would be rebuilt every frame.
    let eye = camera
        .single()
        .map_or(Vec3::ZERO, GlobalTransform::translation);
    let near = benders(eye, bodies.iter().map(|(at, bounds)| footprint(at, bounds)));
    let bender_texture = match &field.benders {
        Some(handle) => {
            if field.bodies != near {
                crate::ground_cover::replace(&mut images, handle, bender_image(&near));
            }
            handle.clone()
        }
        None => images.add(bender_image(&near)),
    };
    field.benders = Some(bender_texture.clone());
    field.bodies = near;

    if let Some(handle) = field.material.as_ref()
        && let Some(current) = materials.get(handle)
    {
        let extension = &current.extension;
        if extension.uniforms == uniforms
            && extension.height == ground.height
            && extension.weights == weights
            && extension.cover == ground.cover
            && extension.benders == bender_texture
            && current.base.perceptual_roughness == shine
        {
            return;
        }
        if let Some(mut material) = materials.get_mut(handle) {
            material.base.perceptual_roughness = shine;
            let extension = &mut material.extension;
            extension.uniforms = uniforms;
            extension.height = ground.height.clone();
            extension.weights = weights;
            extension.cover = ground.cover.clone();
            extension.benders = bender_texture;
        }
        return;
    }
    field.material = Some(materials.add(GrassMaterial {
        base: StandardMaterial {
            perceptual_roughness: shine,
            ..material::grass_base()
        },
        extension: GrassExtension {
            uniforms,
            height: ground.height.clone(),
            weights,
            cover: ground.cover.clone(),
            benders: bender_texture,
        },
    }));
}

/// The uniform block for room grass `profile` on ground `ground`, in wind
/// `wind`, for field `field`, under the ground's weather `weather`.
fn uniforms(
    profile: &GrassProfile,
    ground: &GroundCover,
    wind: &VegetationWind,
    field: &cfg::Field,
    weather: &GroundWeather,
) -> GrassUniforms {
    let mut flowers = [Vec4::ZERO; 4];
    for (slot, colour) in flowers.iter_mut().zip(&profile.flowers) {
        *slot = Vec3::from_array(*colour).extend(1.0);
    }
    GrassUniforms {
        base: Vec3::from_array(profile.base).extend(profile.height),
        tip: Vec3::from_array(profile.tip).extend(cfg::BLADE_WIDTH),
        dry: Vec3::from_array(profile.dry).extend(profile.dry_share),
        flowers,
        layer_density: Vec4::from_array(profile.layer_density),
        ground: profile.ground.map(|c| Vec3::from_array(c).extend(1.0)),
        frame: Vec4::new(
            ground.frame.half,
            ground.frame.cell,
            ground.frame.vertices as f32,
            profile.flower_share,
        ),
        wind: Vec4::new(wind.dir.x, wind.dir.y, wind.speed, cfg::WIND_LEAN),
        reach: Vec4::new(
            field.full,
            cfg::MID_AT * field.full,
            cfg::FAR_AT * field.full,
            field.reach,
        ),
        shares: Vec4::new(
            cfg::MID_SHARE,
            cfg::FAR_SHARE,
            cfg::FADE_FROM * field.reach,
            wind.storm,
        ),
        weather: Vec4::new(
            weather.wetness,
            weather.snow,
            weather.snow_line,
            weather.snow_feather,
        ),
    }
}

/// The blades' roughness on ground as wet as `wetness` (#1683): wet grass
/// shines.
fn roughness(wetness: f32) -> f32 {
    let dry = material::grass_base().perceptual_roughness;
    dry + (cfg::WET_ROUGHNESS - dry) * wetness.clamp(0.0, 1.0)
}

/// Where a body stands, as the grass parts around it: the middle of its
/// footprint, the height of its underside, and how far from that middle
/// (m) the blades lean away. A body with collider bounds - a player's or a
/// peer's chassis - is its bounds, so a car parts the grass under all of
/// it; one without (a peer not yet built, a render-tool walker) stands at
/// most [`cfg::BODY_DROP`] above its feet, and parts [`cfg::BEND_REACH`].
fn footprint(at: &GlobalTransform, bounds: Option<&ColliderAabb>) -> (Vec3, f32, f32) {
    match bounds.filter(|b| b.min.cmple(b.max).all()) {
        Some(b) => {
            let half = (b.max - b.min) * 0.5;
            let reach = (half.x.max(half.z) + cfg::BEND_MARGIN).max(cfg::BEND_REACH);
            ((b.min + b.max) * 0.5, b.min.y, reach)
        }
        None => {
            let middle = at.translation();
            (middle, middle.y - cfg::BODY_DROP, cfg::BEND_REACH)
        }
    }
}

/// The bodies of `bodies` ([`footprint`]s) nearest camera `eye`, at most
/// [`cfg::MAX_BENDERS`], as the shader reads them: each one's middle on XZ
/// and underside on Y, and its reach; the nearest first and the unused
/// ones 0, which the shader stops at. Kept to the centimetre, so a body
/// standing still writes nothing.
fn benders(eye: Vec3, bodies: impl Iterator<Item = (Vec3, f32, f32)>) -> [Vec4; cfg::MAX_BENDERS] {
    let mut near: Vec<(Vec3, f32, f32)> = bodies.collect();
    near.sort_by(|a, b| {
        a.0.distance_squared(eye)
            .total_cmp(&b.0.distance_squared(eye))
    });
    let mut texels = [Vec4::ZERO; cfg::MAX_BENDERS];
    for (texel, (middle, underside, reach)) in texels.iter_mut().zip(near) {
        let at = Vec4::new(middle.x, underside, middle.z, reach);
        *texel = (at * 100.0).round() / 100.0;
    }
    texels
}

/// The bodies as the texture the shader reads: one `Rgba32Float` texel a
/// body, a row of [`cfg::MAX_BENDERS`], GPU only. A 32-bit float texture is
/// read by `textureLoad`, as the height texture is.
fn bender_image(texels: &[Vec4; cfg::MAX_BENDERS]) -> Image {
    let bytes = texels
        .iter()
        .flat_map(|t| t.to_array())
        .flat_map(f32::to_le_bytes)
        .collect();
    Image::new(
        Extent3d {
            width: cfg::MAX_BENDERS as u32,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        bytes,
        TextureFormat::Rgba32Float,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// What a tile is laid to draw (#1693): its detail, the step of
/// [`tiles::SHARES`] its mesh holds, and whether the ground hides it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Laid {
    detail: Detail,
    share: usize,
    hidden: bool,
}

/// The cameras besides the world camera the field is laid for
/// ([`GrassViewer`]), with their lenses.
type Viewers<'w, 's> = Query<
    'w,
    's,
    (&'static GlobalTransform, Option<&'static Projection>),
    (With<GrassViewer>, Without<WorldCamera>),
>;

/// Whether a camera looking through `projection` looks through a
/// perspective lens, whose sight lines meet at its eye: the only kind the
/// horizon test can hide grass from. A camera without one is taken as the
/// game's own perspective one.
fn through_perspective(projection: Option<&Projection>) -> bool {
    matches!(projection, None | Some(Projection::Perspective(_)))
}

/// Lay the field's tiles around the camera: every stretch of ground within
/// the quality setting's reach that can grow grass, each at the detail its
/// distance needs, holding the fewest blades that hold what the shader can
/// grow in it, and hidden while the ground stands between it and the camera
/// (#1693) - and none while the field has no material. Far off, in larger
/// tiles (#1694): a 32 m block wholly on the ground and wholly past
/// [`cfg::LARGE_AT`]'s second distance is one tile, a 16 m quarter past
/// its first another, and the rest 8 m ones, so each 8 m cell is laid
/// once. A [`GrassViewer`] is laid for too: each tile holds what the
/// nearest camera needs, and is hidden only from them all.
#[allow(clippy::too_many_arguments)]
fn lay_grass_tiles(
    mut commands: Commands,
    mut field: ResMut<GrassField>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut tiles: Query<(&mut GrassTile, &mut Mesh3d, &mut Visibility)>,
    heightmap: Option<Res<FinishedHeightMap>>,
    ground: Option<Res<GroundCover>>,
    settings: Res<LocalSettings>,
    camera: Query<(&GlobalTransform, Option<&Projection>), With<WorldCamera>>,
    viewers: Viewers,
) {
    let field = &mut *field;
    let growing = GrassQuality::of(&settings).field();
    let (Some(material), Some(grown), Some(heightmap), Some(ground), Ok(camera)) = (
        field.material.clone(),
        growing,
        heightmap,
        ground,
        camera.single(),
    ) else {
        clear(&mut commands, field);
        return;
    };
    let blades = tiles::blade_count(grown.density);
    if blades != field.blades || field.roots.is_empty() {
        clear(&mut commands, field);
        field.blades = blades;
        field.roots = tiles::roots(blades);
        field.meshes.clear();
    }
    // A new heightmap: the tiles' bounds, and the heights, densities and
    // sight cached for them, were the last ground's.
    if heightmap.is_changed() {
        clear(&mut commands, field);
    }
    let hm = &heightmap.0;
    // The ground's floor follows the heightmap alone, whenever it changed.
    let changed = heightmap.last_changed();
    if field.floor.as_ref().is_none_or(|(of, _)| *of != changed) {
        field.floor = Some((changed, horizon::GroundFloor::of(hm)));
    }
    let Some((_, floor)) = field.floor.as_ref() else {
        return;
    };
    // The densities follow the cover, the weights and the room's grass.
    let densities_of = DensitiesOf::of(&ground, field.profile.as_ref());
    if !field
        .densities_of
        .as_ref()
        .is_some_and(|of| of.same(&densities_of))
    {
        field.densities.clear();
        field.densities_of = Some(densities_of);
    }

    // The eyes the field is laid for, the world camera's first, each with
    // whether the horizon test can hide grass from it.
    let eye = camera.0.translation();
    let mut eyes = vec![(eye, through_perspective(camera.1))];
    eyes.extend(
        viewers
            .iter()
            .map(|(at, projection)| (at.translation(), through_perspective(projection))),
    );
    // Where the larger tiles begin, by size.
    let far = cfg::FAR_AT * grown.full;
    let large_from = cfg::LARGE_AT.map(|at| at * far);
    let block = cfg::TILE_M * tiles::CELLS_A_ROW as f32;
    let span = (grown.reach / block).ceil() as i32 + 1;
    let centre = (Vec2::new(eye.x, eye.z) / block).floor().as_ivec2();
    let edge = ground.frame.half;
    let mut wanted: HashMap<TileKey, Laid> = HashMap::new();
    let mut quarters: Vec<TileKey> = Vec::new();
    for dz in -span..=span {
        for dx in -span..=span {
            quarters.push(TileKey {
                level: tiles::MAX_LEVEL,
                coord: centre + IVec2::new(dx, dz),
            });
            while let Some(key) = quarters.pop() {
                let (lo, side) = (key.lo(), key.side());
                if lo.x >= edge || lo.y >= edge || lo.x + side <= -edge || lo.y + side <= -edge {
                    continue;
                }
                let heights = *field
                    .heights
                    .entry(key)
                    .or_insert_with(|| tiles::tile_heights(hm, key));
                let nearest = eyes
                    .iter()
                    .map(|&(at, _)| tiles::nearest(at, key, heights.1))
                    .fold(f32::INFINITY, f32::min);
                if nearest >= grown.reach {
                    continue;
                }
                // A larger tile only wholly on the ground and wholly past
                // where its size begins; otherwise its quarters.
                if key.level > 0 {
                    let on_ground = lo.x >= -edge
                        && lo.y >= -edge
                        && lo.x + side <= edge
                        && lo.y + side <= edge;
                    if !on_ground || nearest < large_from[key.level as usize - 1] {
                        quarters.extend(key.quarters());
                        continue;
                    }
                }
                let most = *field
                    .densities
                    .entry(key)
                    .or_insert_with(|| tile_density(&ground, field.profile.as_ref(), key));
                // Never more than the detail's whole share, as its mesh held
                // before: thin ground's clumps can lift the most a hair over
                // it, by blades that would grow to nothing.
                let detail = Detail::at(nearest, &grown);
                let need = (most * tiles::thinning(nearest, &grown)).min(detail.share());
                let Some(fewest) = tiles::share_for(need) else {
                    continue;
                };
                // A tile whose share still holds what it needs keeps it until
                // it needs well under the next one down: one standing on the
                // line does not swap its mesh every frame. One that needs more
                // takes it at once.
                let held = field
                    .tiles
                    .get(&key)
                    .and_then(|&entity| tiles.get(entity).ok())
                    .map(|(tile, _, _)| (tile.detail, tile.share));
                let share = match held {
                    Some((held_detail, held))
                        if held_detail == detail
                            && held < fewest
                            && need > cfg::SHARE_HYSTERESIS * tiles::SHARES[held + 1] =>
                    {
                        held
                    }
                    _ => fewest,
                };
                // Hidden only from every eye, and only from one the test can
                // hide grass from. With the world camera alone, a tile found
                // seen is looked at again once it has moved.
                let hidden = if let &[(at, perspective)] = eyes.as_slice() {
                    if !perspective
                        || field
                            .seen_from
                            .get(&key)
                            .is_some_and(|from| from.distance(at) < cfg::HORIZON_RETEST)
                    {
                        false
                    } else if floor.hides(hm, at, key, heights) {
                        field.seen_from.remove(&key);
                        true
                    } else {
                        field.seen_from.insert(key, at);
                        false
                    }
                } else {
                    eyes.iter()
                        .all(|&(at, perspective)| perspective && floor.hides(hm, at, key, heights))
                };
                wanted.insert(
                    key,
                    Laid {
                        detail,
                        share,
                        hidden,
                    },
                );
            }
        }
    }

    field.seen_from.retain(|key, _| wanted.contains_key(key));
    field.tiles.retain(|key, entity| {
        let keep = wanted.contains_key(key);
        if !keep {
            commands.entity(*entity).try_despawn();
        }
        keep
    });
    for (key, laid) in wanted {
        let mesh = field
            .meshes
            .entry((laid.detail, laid.share, key.level))
            .or_insert_with(|| {
                let mesh = tiles::tile_mesh(
                    &field.roots,
                    laid.detail,
                    tiles::SHARES[laid.share],
                    key.level,
                );
                let vertices = mesh.count_vertices();
                (meshes.add(mesh), vertices)
            })
            .0
            .clone();
        let visibility = if laid.hidden {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if let Some(&entity) = field.tiles.get(&key) {
            if let Ok((mut tile, mut drawn, mut seen)) = tiles.get_mut(entity) {
                if tile.detail != laid.detail || tile.share != laid.share {
                    tile.detail = laid.detail;
                    tile.share = laid.share;
                    drawn.0 = mesh;
                }
                seen.set_if_neq(visibility);
            }
            continue;
        }
        let heights = field.heights[&key];
        let corner = Transform::from_translation(key.lo().extend(0.0).xzy());
        // Its `GlobalTransform` too: this runs after transforms propagate,
        // and a tile left at the default would draw at the origin for its
        // first frame.
        let entity = commands
            .spawn((
                Name::new("Grass tile"),
                GrassTile {
                    key,
                    detail: laid.detail,
                    share: laid.share,
                },
                Mesh3d(mesh),
                MeshMaterial3d(material.clone()),
                corner,
                GlobalTransform::from(corner),
                visibility,
                tiles::tile_aabb(heights, key.side()),
                NoAutoAabb,
                // The material draws into no shadow map (`material.rs`):
                // this keeps the lights' culling from testing the tiles too.
                NotShadowCaster,
            ))
            .id();
        field.tiles.insert(key, entity);
    }
}

/// The most the shader can grow anywhere in tile `key` (#1693): the most
/// share of each layer its ground's weights reach in any of its 8 m cells,
/// by how thickly the room's grass grows on each, under its most open
/// cover. Before the ground's weights have baked, as though every layer
/// covered it all.
fn tile_density(ground: &GroundCover, profile: Option<&GrassProfile>, key: TileKey) -> f32 {
    let lo = key.lo();
    let cover = ground.mask.most_open_in(lo, lo + Vec2::splat(key.side()));
    let shares = ground.maxima.as_ref().map_or([1.0; 4], |maxima| {
        key.cells().fold([0.0f32; 4], |most, cell| {
            let shares = maxima.in_tile(cell);
            std::array::from_fn(|layer| most[layer].max(shares[layer]))
        })
    });
    let layers = profile.map_or([1.0; 4], |p| p.layer_density);
    tiles::most_density(shares, layers, cover)
}

/// Take every tile down, and forget the ground under them - all but the
/// ground's floor, which follows the heightmap alone.
fn clear(commands: &mut Commands, field: &mut GrassField) {
    for (_, entity) in field.tiles.drain() {
        commands.entity(entity).try_despawn();
    }
    field.heights.clear();
    field.densities.clear();
    field.densities_of = None;
    field.seen_from.clear();
}

#[cfg(test)]
mod tests;
