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
//!   around the camera out to the quality setting's reach, each drawing
//!   one of three meshes - every tile the same blades, the far ones fewer
//!   ([`tiles`]). The vertex shader does the rest ([`material`]).
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

pub mod material;
pub mod profile;
pub mod tiles;

use std::collections::HashMap;

use avian3d::prelude::ColliderAabb;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{NoAutoAabb, VisibilitySystems};
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
use tiles::Detail;

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
    /// The three meshes, near to far, and the blades a near tile has.
    meshes: Vec<Handle<Mesh>>,
    blades: usize,
    /// The tiles standing, by grid coordinate.
    tiles: HashMap<IVec2, Entity>,
    /// Each tile's lowest and highest ground, read from the heightmap the
    /// tiles stand on.
    heights: HashMap<IVec2, (f32, f32)>,
}

/// A body the grass parts around, besides the players: what the render
/// tool's walkers carry.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PartsGrass;

/// A tile of the field, at `coord` on the grid, drawing its mesh at
/// `detail`.
#[derive(Component, Debug)]
pub struct GrassTile {
    pub coord: IVec2,
    pub detail: Detail,
}

pub struct GrassPlugin;

impl Plugin for GrassPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<GrassMaterial>::default())
            .init_resource::<GrassField>()
            .add_systems(
                PostUpdate,
                (sync_grass_material, lay_grass_tiles)
                    .chain()
                    .after(TransformSystems::Propagate)
                    .before(VisibilitySystems::CheckVisibility),
            );
    }
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

/// Lay the field's tiles around the camera: every grid square within the
/// quality setting's reach that lies on the ground, each at the detail its
/// distance needs, and none while the field has no material.
#[allow(clippy::too_many_arguments)]
fn lay_grass_tiles(
    mut commands: Commands,
    mut field: ResMut<GrassField>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut tiles: Query<(&mut GrassTile, &mut Mesh3d)>,
    heightmap: Option<Res<FinishedHeightMap>>,
    ground: Option<Res<GroundCover>>,
    settings: Res<LocalSettings>,
    camera: Query<&GlobalTransform, With<WorldCamera>>,
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
    if blades != field.blades || field.meshes.is_empty() {
        clear(&mut commands, field);
        field.blades = blades;
        let roots = tiles::roots(blades);
        field.meshes = Detail::ALL
            .iter()
            .map(|&detail| meshes.add(tiles::tile_mesh(&roots, detail)))
            .collect();
    }
    // A new heightmap: the tiles' bounds and the heights cached for them
    // were the last ground's.
    if heightmap.is_changed() {
        clear(&mut commands, field);
    }

    let hm = &heightmap.0;
    let eye = camera.translation();
    let span = (grown.reach / cfg::TILE_M).ceil() as i32 + 1;
    let centre = tiles::tile_of(Vec2::new(eye.x, eye.z));
    let edge = ground.frame.half;
    let mut wanted: HashMap<IVec2, Detail> = HashMap::new();
    for dz in -span..=span {
        for dx in -span..=span {
            let coord = centre + IVec2::new(dx, dz);
            let lo = coord.as_vec2() * cfg::TILE_M;
            if lo.x >= edge
                || lo.y >= edge
                || lo.x + cfg::TILE_M <= -edge
                || lo.y + cfg::TILE_M <= -edge
            {
                continue;
            }
            let heights = *field
                .heights
                .entry(coord)
                .or_insert_with(|| tiles::tile_heights(hm, coord));
            let nearest = tiles::nearest(eye, coord, heights.1);
            if nearest < grown.reach {
                wanted.insert(coord, Detail::at(nearest, &grown));
            }
        }
    }

    field.tiles.retain(|coord, entity| {
        let keep = wanted.contains_key(coord);
        if !keep {
            commands.entity(*entity).try_despawn();
        }
        keep
    });
    for (coord, detail) in wanted {
        if let Some(&entity) = field.tiles.get(&coord) {
            if let Ok((mut tile, mut mesh)) = tiles.get_mut(entity)
                && tile.detail != detail
            {
                tile.detail = detail;
                mesh.0 = field.meshes[detail.index()].clone();
            }
            continue;
        }
        let heights = field.heights[&coord];
        let corner = Transform::from_translation(Vec3::new(
            coord.x as f32 * cfg::TILE_M,
            0.0,
            coord.y as f32 * cfg::TILE_M,
        ));
        // Its `GlobalTransform` too: this runs after transforms propagate,
        // and a tile left at the default would draw at the origin for its
        // first frame.
        let entity = commands
            .spawn((
                Name::new("Grass tile"),
                GrassTile { coord, detail },
                Mesh3d(field.meshes[detail.index()].clone()),
                MeshMaterial3d(material.clone()),
                corner,
                GlobalTransform::from(corner),
                tiles::tile_aabb(heights),
                NoAutoAabb,
            ))
            .id();
        field.tiles.insert(coord, entity);
    }
}

/// Take every tile down, and forget the ground under them.
fn clear(commands: &mut Commands, field: &mut GrassField) {
    for (_, entity) in field.tiles.drain() {
        commands.entity(entity).try_despawn();
    }
    field.heights.clear();
}

#[cfg(test)]
mod tests;
