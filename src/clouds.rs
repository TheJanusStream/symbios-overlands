//! The cloud deck (#1673): a sheet of procedural cloud at the room's
//! `cloud_height`, lit by the scene's own sun and sky, and casting its shade
//! on everything below it.
//!
//! The deck is one horizontal plane drawn through [`CloudMaterial`]. Its
//! shape is a domain-warped noise field over world XZ, drifting on the
//! room's wind, cut at the room's cover and feathered by its softness -
//! `assets/shaders/cloud_common.wgsl`, read by both shaders below, so the
//! shade on the ground is the cloud overhead.
//!
//! * **The clouds** (`cloud.wgsl`) take the scene's directional light: a
//!   cloud lit through takes the room's cloud colour, one in the shade of
//!   the cloud between it and the sun takes its shadow colour, and thin
//!   cloud toward the sun glows. They haze into the horizon as the sky
//!   behind them does (`sky_common::sky_toward`), so a warm sun warms them,
//!   a low one shades their undersides, and a night leaves them dark.
//! * **The shade** (`cloud_shadow.wgsl`) is drawn into the sun's shadow
//!   cascades, so the ground, the water, the buildings and the player all
//!   pass in and out of it as the clouds drift.
//!
//! A lightning flash (#1684) lights the deck from within, carried in the
//! sky's own uniforms (`sky_zenith.w`).
//!
//! The room's drawn environment (`crate::drawn_environment`: its record's
//! at its hour and under its weather) becomes [`CloudUniforms`] in
//! [`cloud_uniforms`]. One of its fields is read differently from before:
//! `cloud_cover` is now the share of the sky the clouds cover
//! ([`threshold`]), as the field always said, where it used to cut the noise
//! at `1 - cover` - which covered 6% of the sky at 0.3 and all of it from
//! about 0.75.
//!
//! Designed to run on WebGL2: fragment work and the shadow pass Bevy runs
//! anyway, no compute, no textures.

use bevy::{
    light::{CascadeShadowConfig, NotShadowCaster},
    math::{Vec2, Vec4},
    mesh::MeshTag,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

use crate::config::lighting::clouds as cfg;
use crate::pds::Environment;
use crate::sky::SkyUniforms;

const CLOUD_SHADER_PATH: &str = "shaders/cloud.wgsl";
const CLOUD_SHADOW_SHADER_PATH: &str = "shaders/cloud_shadow.wgsl";

/// What `cloud.wgsl` and `cloud_shadow.wgsl` are given, through
/// `cloud_common.wgsl`'s copy of the block
/// (`the_wgsl_block_mirrors_the_rust_one`). The `Vec4`s lead, for WGSL's
/// 16-byte alignment, and the block is a multiple of 16 bytes, which WebGL2
/// requires (`the_uniform_block_is_16_byte_aligned`).
#[derive(Debug, Clone, Copy, Default, PartialEq, ShaderType)]
pub struct CloudUniforms {
    /// The sunlit cloud colour, linear RGB; `w` unused.
    pub color: Vec4,
    /// The colour of a cloud's thick, shaded base, linear RGB; `w` unused.
    pub shadow_color: Vec4,
    /// The sky the deck hazes into: [`SkyUniforms::zenith`].
    pub sky_zenith: Vec4,
    /// And its [`SkyUniforms::look`].
    pub sky_look: Vec4,
    /// 2D drift direction in world XZ (need not be unit length; the shader
    /// normalises an epsilon-padded copy).
    pub wind_dir: Vec2,
    /// The noise value a cloud's edge is centred on: [`threshold`] of the
    /// room's cover.
    pub threshold: f32,
    /// Opacity of the clouds that survive the threshold.
    pub density: f32,
    /// Edge-softness band around the threshold.
    pub softness: f32,
    /// Drift speed (m/s) along `wind_dir`.
    pub speed: f32,
    /// World metres per noise unit: the clouds' feature size.
    pub scale: f32,
    /// The share of the direct sun the thickest cloud takes off the ground;
    /// `0` casts no shadow.
    pub shadow_strength: f32,
    /// How far from the camera the sun's shadow cascades reach, metres; the
    /// shade fades out inside it. Kept by [`follow_shadow_reach`].
    pub shadow_reach: f32,
}

/// [`MaterialExtension`] that drives `cloud.wgsl`, and `cloud_shadow.wgsl`
/// in the sun's shadow pass.
///
/// Bind-group slots (group `MATERIAL_BIND_GROUP`, 100 +):
/// - 100 [`CloudUniforms`] uniform
///
/// The deck stays out of the camera's depth prepass: it is transparent, and
/// a depth written there would hide everything under it from the water's
/// shore foam and anything else that reads that depth.
#[derive(Asset, TypePath, AsBindGroup, Clone, Default, Debug)]
pub struct CloudExtension {
    #[uniform(100)]
    pub uniforms: CloudUniforms,
}

impl MaterialExtension for CloudExtension {
    fn fragment_shader() -> ShaderRef {
        CLOUD_SHADER_PATH.into()
    }

    /// The shadow pass runs the prepass pipeline, and the deck's vertices
    /// go through `cloud_shadow.wgsl` there: Bevy's own pancaking, made to
    /// hold a kilometres-wide triangle whole where depth must be clipped by
    /// hand (WebGL2).
    fn prepass_vertex_shader() -> ShaderRef {
        CLOUD_SHADOW_SHADER_PATH.into()
    }

    /// And its fragments: the deck's shade, dithered into the shadow map.
    fn prepass_fragment_shader() -> ShaderRef {
        CLOUD_SHADOW_SHADER_PATH.into()
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        true
    }
}

/// Convenience alias for the full extended-material type used by the cloud
/// deck plane.
pub type CloudMaterial = ExtendedMaterial<StandardMaterial, CloudExtension>;

/// Marker component on the single cloud-deck plane spawned in
/// `setup_lighting`. The world compiler's `apply_environment_state` system
/// uses this to find the plane each change tick and update its uniform
/// block plus altitude in place, mirroring how the `SkyBox` marker is used
/// to repaint the sky's uniforms.
#[derive(Component)]
pub struct CloudLayer;

/// The deck's uniforms for a room's environment, under the sky `sky` the
/// same environment gives ([`crate::sky::sky_uniforms`]). Pure, so what each
/// room gets is testable without a GPU. The shadow's reach is the
/// cascades' at rest until [`follow_shadow_reach`] says otherwise.
pub(crate) fn cloud_uniforms(env: &Environment, sky: &SkyUniforms) -> CloudUniforms {
    let linear = |[r, g, b]: [f32; 3]| {
        let c = Color::srgb(r, g, b).to_linear();
        Vec4::new(c.red, c.green, c.blue, 1.0)
    };
    CloudUniforms {
        color: linear(env.cloud_color.0),
        shadow_color: linear(env.cloud_shadow_color.0),
        sky_zenith: sky.zenith,
        sky_look: sky.look,
        wind_dir: Vec2::from_array(env.cloud_wind_dir.0),
        threshold: threshold(env.cloud_cover.0, env.cloud_softness.0),
        density: env.cloud_density.0,
        softness: env.cloud_softness.0,
        speed: env.cloud_speed.0,
        scale: env.cloud_scale.0,
        shadow_strength: shadow_strength(env.cloud_cover.0),
        shadow_reach: crate::shadow_reach::REST.max,
    }
}

/// How much of the sun a full-density cloud takes off the ground under a
/// room's `cover`: [`cfg::SHADOW_STRENGTH`] under broken cloud, easing to
/// nothing from [`cfg::OVERCAST_FROM`] to a whole overcast. A cover that is
/// not a number reads as clear.
pub(crate) fn shadow_strength(cover: f32) -> f32 {
    let cover = if cover.is_nan() {
        0.0
    } else {
        cover.clamp(0.0, 1.0)
    };
    let t = ((cover - cfg::OVERCAST_FROM) / (1.0 - cfg::OVERCAST_FROM)).clamp(0.0, 1.0);
    cfg::SHADOW_STRENGTH * (1.0 - t * t * (3.0 - 2.0 * t))
}

/// The noise value at which a cloud's edge sits for a room's `cover`, so
/// that the clouds cover that share of the sky: the field's
/// `(1 - cover)` quantile, read off [`cfg::FIELD_THRESHOLDS`] between 0.05
/// and 0.95. A cloud's edge is feathered `softness` either side of it, so
/// the two ends stand that much clear of the field's range
/// ([`cfg::FIELD_MAX`], [`cfg::FIELD_MIN`]): no cover leaves not a wisp, a
/// whole one not a thin spot. A cover that is not a number reads as clear,
/// and a softness that is not one as the shader's floor; the sanitiser
/// passes neither.
pub(crate) fn threshold(cover: f32, softness: f32) -> f32 {
    let table = &cfg::FIELD_THRESHOLDS;
    // The table's points, with the two ends at 0 and at `steps`.
    let steps = table.len() + 1;
    let cover = if cover.is_nan() {
        0.0
    } else {
        cover.clamp(0.0, 1.0)
    };
    let soft = if softness.is_finite() {
        softness.max(SOFTNESS_FLOOR)
    } else {
        SOFTNESS_FLOOR
    };
    let at = |i: usize| match i {
        0 => cfg::FIELD_MAX + soft,
        i if i == steps => cfg::FIELD_MIN - soft,
        i => table[i - 1],
    };
    let x = cover * steps as f32;
    let i = (x.floor() as usize).min(steps - 1);
    let t = x - i as f32;
    at(i) + (at(i + 1) - at(i)) * t
}

/// The softness `cloud_common.wgsl` feathers an edge across at the least.
const SOFTNESS_FLOOR: f32 = 1.0e-3;

/// How far below its threshold a cloud's shade begins, in softness widths:
/// `cloud_shadow.wgsl` casts from a mass of 0.25 (`SHADE_EDGE_FROM`), which
/// the edge's smoothstep reaches at `threshold - 0.347 * softness`.
const SHADE_EDGE_REACH: f32 = 0.347;

/// Keep the cloud deck around the active camera every frame: centred on
/// it, wide enough that its edge stays down at the horizon, and tagged with
/// the ground's height under it.
///
/// The noise is sampled in world XZ, so moving the plane moves only the
/// window the clouds are seen through, never the clouds. Its half-width
/// grows with the deck's height over the camera
/// ([`cfg::EDGE_ZENITH_TAN`]), so the fade at its edge (`cloud.wgsl`) runs
/// low toward the horizon, deep in the haze, however high the room sets
/// its clouds - and a camera above the deck sees it reach as far. `.y` is
/// the room's `cloud_height`, owned by `apply_environment_state`.
///
/// The [`MeshTag`] is the height of the ground under the camera, which the
/// shadow pass (`cloud_shadow.wgsl`) takes for the ground a cloud's shade
/// lands on when it fades the shade out before the cascades end. A tag is
/// per-instance data, so the material is never touched for it. Over no
/// heightmap the camera's own height stands in. Every write is guarded by
/// a comparison, so a still camera changes nothing.
pub fn track_cloud_layer_to_camera(
    camera: Query<&GlobalTransform, (crate::camera::IsWorldCamera, Without<CloudLayer>)>,
    heightmap: Option<Res<crate::terrain::FinishedHeightMap>>,
    mut cloud_layer: Query<(&mut Transform, &mut MeshTag), With<CloudLayer>>,
) {
    let Ok(cam_tx) = camera.single() else {
        return;
    };
    let cam = cam_tx.translation();
    let ground = heightmap.map_or(cam.y, |h| h.world_height_at(cam.x, cam.z));
    for (mut transform, mut tag) in cloud_layer.iter_mut() {
        let spread = deck_spread(transform.translation.y - cam.y);
        let wanted = Vec3::new(cam.x, transform.translation.y, cam.z);
        if transform.translation != wanted {
            transform.translation = wanted;
        }
        let scale = Vec3::new(spread, 1.0, spread);
        if transform.scale != scale {
            transform.scale = scale;
        }
        let bits = ground.to_bits();
        if tag.0 != bits {
            tag.0 = bits;
        }
    }
}

/// The deck plane's scale for a deck `rise` metres above the camera (below
/// it if negative): 1 until the plane's half-width would bring its edge up
/// off the horizon, and wider from there.
pub(crate) fn deck_spread(rise: f32) -> f32 {
    let wanted = rise.abs() * cfg::EDGE_ZENITH_TAN / cfg::PLANE_HALF_EXTENT;
    if wanted.is_finite() {
        wanted.max(1.0)
    } else {
        1.0
    }
}

/// Keep the deck's shade inside the sun's shadow cascades: write the
/// cascades' reach into [`CloudUniforms::shadow_reach`] when the sun's
/// [`CascadeShadowConfig`] changes (#1475 re-cuts it as the camera zooms).
/// A reach within [`cfg::REACH_RETUNE_SHARE`] of the one written is left,
/// so a zoom does not rebuild the material every frame: the shade's fade is
/// broad enough that the lag never shows.
pub fn follow_shadow_reach(
    suns: Query<&CascadeShadowConfig, (With<DirectionalLight>, Changed<CascadeShadowConfig>)>,
    decks: Query<&MeshMaterial3d<CloudMaterial>, With<CloudLayer>>,
    mut materials: ResMut<Assets<CloudMaterial>>,
) {
    let Some(reach) = suns.iter().find_map(|config| config.bounds.last().copied()) else {
        return;
    };
    if !reach.is_finite() || reach <= 0.0 {
        return;
    }
    for handle in decks.iter() {
        let Some(current) = materials
            .get(&handle.0)
            .map(|m| m.extension.uniforms.shadow_reach)
        else {
            continue;
        };
        if (reach - current).abs() <= cfg::REACH_RETUNE_SHARE * current.abs() {
            continue;
        }
        if let Some(mut material) = materials.get_mut(&handle.0) {
            material.extension.uniforms.shadow_reach = reach;
        }
    }
}

/// A cloud deck, its material, and whether it is out of the shadow pass.
type DeckCasting<'a> = (
    Entity,
    &'a MeshMaterial3d<CloudMaterial>,
    Has<NotShadowCaster>,
);

/// Take the deck out of the shadow pass when it would cast nothing - no
/// shadow strength, or no clouds - and put it back when it would: the
/// shadow pass runs the deck's shader over every texel of every cascade,
/// which a clear sky need not pay. Writes only a change.
pub fn cast_cloud_shadows(
    mut commands: Commands,
    decks: Query<DeckCasting, With<CloudLayer>>,
    materials: Res<Assets<CloudMaterial>>,
) {
    for (deck, handle, excluded) in decks.iter() {
        let Some(material) = materials.get(&handle.0) else {
            continue;
        };
        let casts = casts_shadow(&material.extension.uniforms);
        if casts && excluded {
            commands.entity(deck).remove::<NotShadowCaster>();
        } else if !casts && !excluded {
            commands.entity(deck).insert(NotShadowCaster);
        }
    }
}

/// Whether a deck with these uniforms shades anything: some strength,
/// some density, and cloud thick enough to cast where the field can reach.
pub(crate) fn casts_shadow(u: &CloudUniforms) -> bool {
    u.shadow_strength > 0.0
        && u.density > 0.0
        && u.threshold - SHADE_EDGE_REACH * u.softness.max(SOFTNESS_FLOOR) < cfg::FIELD_MAX
}

/// Hide the cloud deck under an orthographic world camera (#1603), and show
/// it again under a perspective one. Looking down, as an orthographic camera
/// always does, the view never shows the sky the deck hangs in; and that
/// view is a box reaching back behind the camera, tall enough at a wide zoom
/// to hold the deck between the camera and the ground - a sheet of cloud
/// over the frame. A hidden deck casts no shadow either, so the clouds'
/// shade leaves the ground with it. Writes only a change.
pub fn hide_cloud_deck(
    camera: Query<&Projection, crate::camera::IsWorldCamera>,
    mut decks: Query<&mut Visibility, With<CloudLayer>>,
) {
    let Ok(projection) = camera.single() else {
        return;
    };
    let wanted = match projection {
        Projection::Orthographic(_) => Visibility::Hidden,
        _ => Visibility::Inherited,
    };
    for mut visibility in decks.iter_mut() {
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The deck is hidden while the world camera is orthographic and shown
    /// again when it is not (#1603).
    #[test]
    fn the_deck_hides_under_an_orthographic_camera() {
        let mut app = App::new();
        app.add_systems(Update, hide_cloud_deck);
        let camera = app
            .world_mut()
            .spawn((
                Camera3d::default(),
                crate::camera::WorldCamera,
                Projection::Orthographic(OrthographicProjection::default_3d()),
            ))
            .id();
        let deck = app
            .world_mut()
            .spawn((CloudLayer, Visibility::Inherited))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(deck),
            Some(&Visibility::Hidden)
        );
        *app.world_mut()
            .get_mut::<Projection>(camera)
            .expect("a lens") = Projection::Perspective(default());
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(deck),
            Some(&Visibility::Inherited)
        );
    }

    /// The block's size is what the WebGL2 uniform binding is validated
    /// against, and only a browser would otherwise say it broke.
    #[test]
    fn the_uniform_block_is_16_byte_aligned() {
        let size = CloudUniforms::min_size().get();
        assert_eq!(
            size % 16,
            0,
            "CloudUniforms is {size} bytes - WebGL2 needs a multiple of 16"
        );
    }

    /// The Rust and WGSL declarations of the block are two hand-written
    /// copies of one layout, and nothing in the build compiles the shader:
    /// a field added to one and not the other reads every later field at
    /// the wrong offset, and the deck draws wrongly or not at all with no
    /// error anywhere. The list is [`CloudUniforms`]'s fields in order.
    #[test]
    fn the_wgsl_block_mirrors_the_rust_one() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/shaders/cloud_common.wgsl"
        ))
        .expect("cloud_common.wgsl is a tracked asset");
        let head = "struct CloudUniforms {";
        let start = src.find(head).expect("struct not found") + head.len();
        let body = &src[start..start + src[start..].find("};").expect("unterminated struct")];
        let fields: Vec<(String, String)> = body
            .lines()
            .map(|l| l.split("//").next().unwrap_or("").trim())
            .filter(|l| l.contains(':'))
            .map(|l| {
                let (name, ty) = l.split_once(':').expect("filtered on ':'");
                (
                    name.trim().to_string(),
                    ty.trim().trim_end_matches(',').to_string(),
                )
            })
            .collect();
        // Naming every field, with no `..`, makes a field added to the Rust
        // struct alone a compile error here: the list below must grow too.
        let CloudUniforms {
            color: _,
            shadow_color: _,
            sky_zenith: _,
            sky_look: _,
            wind_dir: _,
            threshold: _,
            density: _,
            softness: _,
            speed: _,
            scale: _,
            shadow_strength: _,
            shadow_reach: _,
        } = CloudUniforms::default();
        let expected = [
            ("color", "vec4<f32>"),
            ("shadow_color", "vec4<f32>"),
            ("sky_zenith", "vec4<f32>"),
            ("sky_look", "vec4<f32>"),
            ("wind_dir", "vec2<f32>"),
            ("threshold", "f32"),
            ("density", "f32"),
            ("softness", "f32"),
            ("speed", "f32"),
            ("scale", "f32"),
            ("shadow_strength", "f32"),
            ("shadow_reach", "f32"),
        ];
        let expected: Vec<(String, String)> = expected
            .iter()
            .map(|(n, t)| (n.to_string(), t.to_string()))
            .collect();
        assert_eq!(fields, expected);
    }

    /// The colours reach the shader linear: the record stores sRGB, and
    /// the shader lights them in linear light (they were read as linear
    /// before #1673, which washed every shadow colour out).
    #[test]
    fn the_colours_reach_the_shader_linear() {
        let env = Environment {
            cloud_color: crate::pds::Fp3([1.0, 0.5, 0.0]),
            cloud_shadow_color: crate::pds::Fp3([0.55, 0.62, 0.72]),
            ..default()
        };
        let u = cloud_uniforms(&env, &crate::sky::sky_uniforms(&env));
        assert!((u.color.x - 1.0).abs() < 1e-5, "{:?}", u.color);
        assert!((u.color.y - 0.214).abs() < 1e-3, "{:?}", u.color);
        assert!(u.color.z.abs() < 1e-5, "{:?}", u.color);
        assert!(
            (u.shadow_color.x - 0.263).abs() < 1e-3,
            "{:?}",
            u.shadow_color
        );
        assert!(
            (u.shadow_color.z - 0.477).abs() < 1e-3,
            "{:?}",
            u.shadow_color
        );
    }

    /// The deck hazes into the sky the same environment draws.
    #[test]
    fn the_deck_hazes_into_its_rooms_sky() {
        let env = Environment::default();
        let sky = crate::sky::sky_uniforms(&env);
        let u = cloud_uniforms(&env, &sky);
        assert_eq!(u.sky_zenith, sky.zenith);
        assert_eq!(u.sky_look, sky.look);
    }

    /// The threshold falls as the cover rises, from a softness clear of
    /// anything the field reaches at no cover to a softness under it at full
    /// cover, and halves the sky at the field's median.
    #[test]
    fn the_threshold_runs_from_clear_to_overcast() {
        let soft = 0.18;
        assert_eq!(threshold(0.0, soft), cfg::FIELD_MAX + soft);
        assert_eq!(threshold(1.0, soft), cfg::FIELD_MIN - soft);
        assert!((threshold(0.5, soft) - 0.499).abs() < 1e-3);
        let mut last = f32::INFINITY;
        for i in 0..=100 {
            let t = threshold(i as f32 / 100.0, soft);
            assert!(t < last, "{i}: {t} after {last}");
            last = t;
        }
        assert_eq!(threshold(f32::NAN, soft), threshold(0.0, soft));
        assert_eq!(threshold(-1.0, soft), threshold(0.0, soft));
        assert_eq!(threshold(2.0, soft), threshold(1.0, soft));
        assert_eq!(threshold(0.0, f32::NAN), cfg::FIELD_MAX + SOFTNESS_FLOOR);
        // Between the ends, the measured table, a twentieth of cover apart.
        assert_eq!(threshold(0.05, soft), cfg::FIELD_THRESHOLDS[0]);
        assert_eq!(threshold(0.95, soft), cfg::FIELD_THRESHOLDS[18]);
    }

    /// A Rust copy of `cloud_common.wgsl`'s field, to measure what share of
    /// the sky a threshold covers. The table was measured on this field's
    /// numpy port; this holds the two to each other.
    mod field {
        fn fract(x: f32) -> f32 {
            x - x.floor()
        }

        fn hash21(x: f32, y: f32) -> f32 {
            let (mut a, mut b, mut c) = (fract(x * 0.1031), fract(y * 0.1031), fract(x * 0.1031));
            let d = a * (b + 33.33) + b * (c + 33.33) + c * (a + 33.33);
            a += d;
            b += d;
            c += d;
            fract((a + b) * c)
        }

        fn value_noise(x: f32, y: f32) -> f32 {
            let (ix, iy) = (x.floor(), y.floor());
            let (fx, fy) = (x - ix, y - iy);
            let ux = fx * fx * (3.0 - 2.0 * fx);
            let uy = fy * fy * (3.0 - 2.0 * fy);
            let a = hash21(ix, iy);
            let b = hash21(ix + 1.0, iy);
            let c = hash21(ix, iy + 1.0);
            let d = hash21(ix + 1.0, iy + 1.0);
            (a + (b - a) * ux) * (1.0 - uy) + (c + (d - c) * ux) * uy
        }

        fn fbm(mut x: f32, mut y: f32) -> f32 {
            let (mut amp, mut sum) = (0.5, 0.0);
            for _ in 0..5 {
                sum += amp * value_noise(x, y);
                let (qx, qy) = (x * 2.03, y * 2.03);
                x = 0.866025 * qx + 0.5 * qy + 13.7;
                y = -0.5 * qx + 0.866025 * qy + 7.3;
                amp *= 0.5;
            }
            sum / 0.96875
        }

        /// The field at world `(x, z)` on a 320 m scale, still air.
        pub(super) fn at(x: f32, z: f32) -> f32 {
            let (u, v) = (x / 320.0, z / 320.0);
            let wx = fbm(u * 0.6, v * 0.6);
            let wy = fbm(u * 0.6 + 31.0, v * 0.6 + 17.0);
            fbm(u + wx * 0.7, v + wy * 0.7)
        }
    }

    /// A cover covers that share of the sky: the share of the field above
    /// each cover's threshold, over a deck 64 km across, is the cover
    /// within two points.
    #[test]
    fn a_cover_covers_its_share_of_the_sky() {
        let mut samples = Vec::with_capacity(160 * 160);
        for i in 0..160 {
            for j in 0..160 {
                // An irrational stride, so no sample shares a noise cell's
                // phase with its neighbours.
                let x = -32_000.0 + i as f32 * 400.0 + j as f32 * 0.618_034 * 3.0;
                let z = -32_000.0 + j as f32 * 400.0 + i as f32 * 0.414_214 * 5.0;
                samples.push(field::at(x, z));
            }
        }
        for cover in [0.1, 0.25, 0.45, 0.6, 0.75, 0.9] {
            let t = threshold(cover, cfg::SOFTNESS);
            let share = samples.iter().filter(|&&n| n > t).count() as f32 / samples.len() as f32;
            assert!(
                (share - cover).abs() < 0.02,
                "cover {cover}: threshold {t} covers {share}"
            );
        }
        let max = samples.iter().copied().fold(f32::MIN, f32::max);
        assert!(max < cfg::FIELD_MAX, "the field reached {max}");
        let min = samples.iter().copied().fold(f32::MAX, f32::min);
        assert!(min > cfg::FIELD_MIN, "the field reached {min}");
    }

    /// No cover draws not a wisp and a whole cover not a gap, however soft
    /// the edges - the editor runs the softness up to 1 (#1673's critic: at
    /// the default softness a cover of 0 still drew wisps, and the slant law
    /// thickened them into a band along the horizon).
    #[test]
    fn the_ends_of_the_cover_are_clear_and_whole() {
        let smoothstep = |e0: f32, e1: f32, x: f32| {
            let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        let samples: Vec<f32> = (0..4_000)
            .map(|k| {
                let k = k as f32;
                field::at(-20_000.0 + k * 9.7, 13_000.0 - k * 6.1)
            })
            .collect();
        for soft in [0.001, 0.1, 0.18, 0.28, 0.6, 1.0] {
            let clear = threshold(0.0, soft);
            let whole = threshold(1.0, soft);
            for &n in &samples {
                assert_eq!(
                    smoothstep(clear - soft, clear + soft, n),
                    0.0,
                    "{soft}: {n}"
                );
                assert_eq!(
                    smoothstep(whole - soft, whole + soft, n),
                    1.0,
                    "{soft}: {n}"
                );
            }
        }
        let env = Environment {
            cloud_cover: crate::pds::Fp(0.0),
            ..default()
        };
        assert!(!casts_shadow(&cloud_uniforms(
            &env,
            &crate::sky::sky_uniforms(&env)
        )));
    }

    /// Broken cloud casts its full shade; an overcast's eases away, to none
    /// under a whole one.
    #[test]
    fn the_shade_eases_away_under_an_overcast() {
        assert_eq!(shadow_strength(0.3), cfg::SHADOW_STRENGTH);
        assert_eq!(shadow_strength(cfg::OVERCAST_FROM), cfg::SHADOW_STRENGTH);
        let mut last = cfg::SHADOW_STRENGTH;
        for i in 1..=20 {
            let cover = cfg::OVERCAST_FROM + (1.0 - cfg::OVERCAST_FROM) * i as f32 / 20.0;
            let s = shadow_strength(cover);
            assert!(s < last, "{cover}: {s} after {last}");
            last = s;
        }
        assert_eq!(shadow_strength(1.0), 0.0);
        assert_eq!(shadow_strength(f32::NAN), cfg::SHADOW_STRENGTH);
    }

    /// The plane widens only once its edge would rise off the horizon, and
    /// as much for a deck below the camera as above it.
    #[test]
    fn the_plane_widens_with_the_decks_height() {
        assert_eq!(deck_spread(0.0), 1.0);
        assert_eq!(deck_spread(250.0), 1.0);
        let high = deck_spread(2_000.0);
        assert!(
            (high * cfg::PLANE_HALF_EXTENT - 2_000.0 * cfg::EDGE_ZENITH_TAN).abs() < 1.0,
            "{high}"
        );
        assert_eq!(deck_spread(-2_000.0), high);
        assert_eq!(deck_spread(f32::NAN), 1.0);
    }

    /// The deck follows the camera, keeps the room's height, and tags itself
    /// with the ground under the camera - which, over no heightmap, is the
    /// camera's own height.
    #[test]
    fn the_deck_follows_the_camera_and_tags_the_ground() {
        let mut app = App::new();
        app.add_systems(Update, track_cloud_layer_to_camera);
        app.world_mut().spawn((
            Camera3d::default(),
            crate::camera::WorldCamera,
            GlobalTransform::from_translation(Vec3::new(30.0, 12.0, -40.0)),
        ));
        let deck = app
            .world_mut()
            .spawn((
                CloudLayer,
                Transform::from_xyz(0.0, 3_000.0, 0.0),
                MeshTag(0),
            ))
            .id();
        app.update();
        let transform = app.world().get::<Transform>(deck).expect("a transform");
        assert_eq!(transform.translation, Vec3::new(30.0, 3_000.0, -40.0));
        assert_eq!(transform.scale.x, deck_spread(2_988.0));
        assert_eq!(transform.scale.y, 1.0);
        let tag = app.world().get::<MeshTag>(deck).expect("a tag");
        assert_eq!(f32::from_bits(tag.0), 12.0);
    }

    /// A deck with no strength, no density or no cloud the field can reach
    /// casts nothing, and leaves the shadow pass.
    #[test]
    fn a_deck_that_shades_nothing_leaves_the_shadow_pass() {
        let env = Environment::default();
        let base = cloud_uniforms(&env, &crate::sky::sky_uniforms(&env));
        assert!(casts_shadow(&base));
        for u in [
            CloudUniforms {
                shadow_strength: 0.0,
                ..base
            },
            CloudUniforms {
                density: 0.0,
                ..base
            },
            CloudUniforms {
                threshold: 1.0,
                softness: 0.05,
                ..base
            },
        ] {
            assert!(!casts_shadow(&u), "{u:?}");
        }

        let mut app = App::new();
        app.init_resource::<Assets<CloudMaterial>>();
        app.add_systems(Update, cast_cloud_shadows);
        let handle = app
            .world_mut()
            .resource_mut::<Assets<CloudMaterial>>()
            .add(CloudMaterial {
                base: StandardMaterial::default(),
                extension: CloudExtension { uniforms: base },
            });
        let deck = app
            .world_mut()
            .spawn((CloudLayer, MeshMaterial3d(handle.clone()), NotShadowCaster))
            .id();
        app.update();
        assert!(app.world().get::<NotShadowCaster>(deck).is_none());
        app.world_mut()
            .resource_mut::<Assets<CloudMaterial>>()
            .get_mut(&handle)
            .expect("the material")
            .extension
            .uniforms
            .shadow_strength = 0.0;
        app.update();
        assert!(app.world().get::<NotShadowCaster>(deck).is_some());
    }

    /// The shade's reach follows the sun's cascades, and a change too small
    /// to see leaves the material alone.
    #[test]
    fn the_shadow_reach_follows_the_cascades() {
        let mut app = App::new();
        app.init_resource::<Assets<CloudMaterial>>();
        app.add_systems(Update, follow_shadow_reach);
        let env = Environment::default();
        let handle = app
            .world_mut()
            .resource_mut::<Assets<CloudMaterial>>()
            .add(CloudMaterial {
                base: StandardMaterial::default(),
                extension: CloudExtension {
                    uniforms: cloud_uniforms(&env, &crate::sky::sky_uniforms(&env)),
                },
            });
        app.world_mut()
            .spawn((CloudLayer, MeshMaterial3d(handle.clone())));
        let sun = app
            .world_mut()
            .spawn((
                DirectionalLight::default(),
                crate::shadow_reach::cascades(crate::shadow_reach::Reach {
                    first_far: 40.0,
                    max: 350.0,
                }),
            ))
            .id();
        let reach = |app: &App| {
            app.world()
                .resource::<Assets<CloudMaterial>>()
                .get(&handle)
                .expect("the material")
                .extension
                .uniforms
                .shadow_reach
        };
        app.update();
        let first = reach(&app);
        assert!((first - 350.0).abs() < 0.01, "{first}");
        *app.world_mut()
            .get_mut::<CascadeShadowConfig>(sun)
            .expect("cascades") = crate::shadow_reach::cascades(crate::shadow_reach::Reach {
            first_far: 40.0,
            max: 351.0,
        });
        app.update();
        assert_eq!(reach(&app), first, "a 0.3% change is not one");
    }
}
