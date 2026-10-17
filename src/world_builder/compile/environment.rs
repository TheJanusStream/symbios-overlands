//! Atmospheric `Environment` projection: sun, ambient, sky, fog, and the
//! cloud-deck shader uniforms. Reads the active
//! [`RoomRecord::environment`](crate::pds::RoomRecord::environment)
//! and re-paints every renderer-side resource the editor sliders touch.

use bevy::light::GlobalAmbientLight;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::view::ColorGrading;

use crate::clouds::{CloudLayer, CloudMaterial};
use crate::pds::{Fp3, Fp4};
use crate::state::LiveRoomRecord;

/// A request to re-paint the atmosphere from the LIVE record right now,
/// without waiting for the editor's debounce (#1249 f59).
///
/// **The lane this opens.** Every widget edit re-arms the 0.25 s flush, and
/// `set_changed()` fires only when that timer drains - so during a
/// continuous drag the record is never marked changed, and colour and
/// atmosphere sliders, which are tuned by eye, showed nothing at all until
/// the hand stopped. The debounce is right for the two expensive consumers
/// (the peer broadcast and the world compile / terrain rebuild) and wrong
/// for the cheap ones: this system, which patches light, fog, sky and cloud
/// uniforms, and `sky_light::update_sky_light`, which rebuilds the sky
/// light's maps when the sky changed (#1680) - both safe at frame rate.
///
/// A resource rather than a flag on the record because the record's change
/// tick IS the debounce signal; a second signal is the only way to say
/// "cheap consumers only".
#[derive(Resource, Default)]
pub struct EnvironmentPreview;

/// Apply the active `RoomRecord`'s `Environment` to every atmospheric
/// resource in the scene - sun, ambient, the sky's shader, clear colour,
/// and distance fog. Runs on every `RoomRecord` change so an editor slider
/// (or peer broadcast) retints the world without restarting the session.
///
/// Kept separate from `compile_room_record` because the combined
/// signature would exceed Bevy's 16-param `IntoSystem` limit; splitting
/// it out also lets Bevy schedule the two passes in parallel when their
/// resource borrows don't conflict.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_environment_state(
    record: Option<Res<LiveRoomRecord>>,
    // `Without<CloudLayer>` keeps this query disjoint from the
    // `cloud_layer` query below (which holds `&mut Transform`). Bevy's
    // borrow checker conservatively assumes any pair of queries that
    // touch `Transform` could match the same entity unless we tell it
    // otherwise - and a directional light entity never carries the
    // `CloudLayer` marker, so the filter has no runtime cost.
    mut lights: Query<(&mut DirectionalLight, &mut Transform), Without<CloudLayer>>,
    mut clear_color: ResMut<ClearColor>,
    mut ambient_light: ResMut<GlobalAmbientLight>,
    mut fog: Query<&mut DistanceFog>,
    // Every camera that carries the room's fog carries its grade too
    // (#1669): the game's, the render tool's world camera and the agent's
    // look camera - not an item preview's, which has no fog.
    mut grading: Query<&mut ColorGrading, With<DistanceFog>>,
    skybox: Query<&MeshMaterial3d<crate::sky::SkyMaterial>, With<crate::SkyBox>>,
    mut sky_materials: ResMut<Assets<crate::sky::SkyMaterial>>,
    mut cloud_layer: Query<(&MeshMaterial3d<CloudMaterial>, &mut Transform), With<CloudLayer>>,
    mut cloud_materials: ResMut<Assets<CloudMaterial>>,
    mut water_materials: ResMut<Assets<crate::water::WaterMaterial>>,
    vegetation_wind: Option<ResMut<crate::wind::VegetationWind>>,
    // The cheap lane (#1249 f59): the editor stamps this every frame a
    // widget changes, so a drag repaints continuously while the broadcast
    // and the recompile still wait for the pause.
    preview: Option<Res<EnvironmentPreview>>,
) {
    let Some(record) = record else {
        return;
    };
    let previewing = preview.is_some_and(|p| p.is_changed());
    if !record.is_changed() && !previewing {
        return;
    }
    let record = &record.0;
    let env = &record.environment;

    let Fp3(sun_c) = env.sun_color;
    // Snapshot the runtime sun direction (unit vector *toward* the sun) for
    // the water's sun glitter. The directional light's forward axis points
    // from the light toward its target, so the unit toward-sun vector is
    // `-forward()`. Falls back to world Y when the light's transform is
    // degenerate. (The cloud deck reads the light itself since #1673.)
    let mut sun_dir = Vec3::Y;
    let Fp3(sp) = env.sun_position;
    let sun_pos = Vec3::new(sp[0], sp[1], sp[2]);
    for (mut light, mut transform) in lights.iter_mut() {
        light.color = Color::srgb(sun_c[0], sun_c[1], sun_c[2]);
        light.illuminance = env.sun_illuminance.0;
        // Re-orient the directional light so its forward points toward
        // the origin from `sun_position`. Sanitise has already rejected
        // a zero-length vector, but `look_at` still requires the
        // target ≠ eye AND a non-collinear up vector - a `sun_position`
        // sitting on the world Y axis would make forward ‖ Vec3::Y and
        // panic the cross-product inside `look_at`. Swap to Vec3::Z as
        // the up reference when that happens (any non-Y axis works
        // because the resulting roll is invisible for a directional
        // light - only the forward direction is observed).
        if sun_pos.length_squared() > 1.0e-6 {
            transform.translation = sun_pos;
            let forward = -sun_pos.normalize();
            let up = if forward.x.abs() < 1.0e-4 && forward.z.abs() < 1.0e-4 {
                Vec3::Z
            } else {
                Vec3::Y
            };
            transform.look_at(Vec3::ZERO, up);
        }
        sun_dir = (-transform.forward().as_vec3()).normalize_or(Vec3::Y);
    }

    // The global ambient light: what lights a camera the sky does not - the
    // item previews. A sky-lit camera turns its own off and takes the
    // room's sky in its place, at this same level (`sky_light`, #1680).
    ambient_light.brightness = env.ambient_brightness.0;

    let Fp3(sky_c) = env.sky_color;
    let Fp4(fog_c) = env.fog_color;
    ambient_light.color = sky_ambient_color(sky_c, [fog_c[0], fog_c[1], fog_c[2]]);
    for mut grade in grading.iter_mut() {
        apply_grade(env, &mut grade);
    }

    clear_color.0 = Color::srgb(sky_c[0], sky_c[1], sky_c[2]);
    // The sky (#1671): its colour overhead, the haze's depth and whether
    // the night shows its moon and stars, all from this environment.
    let sky = crate::sky::sky_uniforms(env);
    for material_handle in skybox.iter() {
        if let Some(mut mat) = sky_materials.get_mut(&material_handle.0) {
            mat.uniforms = sky;
        }
    }

    let Fp4(fog_sun_c) = env.fog_sun_color;
    let Fp3(ext_c) = env.fog_extinction;
    let Fp3(in_c) = env.fog_inscattering;
    for mut dfog in fog.iter_mut() {
        dfog.color = Color::srgba(fog_c[0], fog_c[1], fog_c[2], fog_c[3]);
        dfog.directional_light_color =
            Color::srgba(fog_sun_c[0], fog_sun_c[1], fog_sun_c[2], fog_sun_c[3]);
        dfog.directional_light_exponent = env.fog_sun_exponent.0;
        dfog.falloff = FogFalloff::from_visibility_colors(
            env.fog_visibility.0,
            Color::srgb(ext_c[0], ext_c[1], ext_c[2]),
            Color::srgb(in_c[0], in_c[1], in_c[2]),
        );
    }

    // Cloud-deck. Both the plane's altitude and the shader uniforms are
    // patched together so a slider drag in the editor's "Clouds" tab
    // re-positions and re-lights the deck in the same change tick. The
    // shade's reach is the sun's cascades', which the record does not hold
    // (`clouds::follow_shadow_reach` keeps it), so it is carried over.
    let Fp3(cloud_c) = env.cloud_color;
    let crate::pds::Fp2(wind) = env.cloud_wind_dir;
    let clouds = crate::clouds::cloud_uniforms(env, &sky);
    for (material_handle, mut transform) in cloud_layer.iter_mut() {
        transform.translation.y = env.cloud_height.0;
        if let Some(mut mat) = cloud_materials.get_mut(&material_handle.0) {
            mat.extension.uniforms = crate::clouds::CloudUniforms {
                shadow_reach: mat.extension.uniforms.shadow_reach,
                ..clouds
            };
            // Mirror the underlying StandardMaterial's base colour to the
            // sunlit tint so any non-shader fallback path (e.g. an asset
            // inspector) still shows a recognisable cloud colour.
            mat.base.base_color = Color::srgb(cloud_c[0], cloud_c[1], cloud_c[2]);
        }
    }

    // Vegetation sways on the same wind that drives the cloud deck (#916),
    // so a wind-direction drag in the editor turns the clouds and the
    // foliage together instead of leaving them visibly disagreeing. The
    // resource is `Option` because the headless spawn path (the render tool,
    // minimal test apps) runs the world compiler without
    // `VegetationWindPlugin`; writing it only when the values actually differ
    // keeps `Res::is_changed` meaningful for `wind::apply_wind_state`, which
    // would otherwise re-upload every foliage material on every record edit.
    if let Some(mut veg_wind) = vegetation_wind {
        let dir = Vec2::new(wind[0], wind[1]);
        let speed = env.cloud_speed.0;
        if veg_wind.dir != dir || veg_wind.speed != speed {
            veg_wind.dir = dir;
            veg_wind.speed = speed;
        }
    }

    // Water sun-glitter tracks the same runtime sun as the cloud deck
    // (#662), and its reflection the same sky (#1672). Patched here rather
    // than at water spawn only, because a sun-position slider drag or a sky
    // colour pick retints the world without recompiling the water volumes -
    // every live water material shares the one global sun and sky.
    for (_, mat) in water_materials.iter_mut() {
        mat.extension.uniforms.sun_dir = Vec4::new(sun_dir.x, sun_dir.y, sun_dir.z, 0.0);
        mat.extension.uniforms.sky_zenith = sky.zenith;
        mat.extension.uniforms.sky_look = sky.look;
    }
}

/// The colour of the room's ambient light (#1669): the sky's own hue, so
/// shade under a blue sky reads cool and under a dusty one warm, where a
/// white ambient left every shadow the same neutral grey.
///
/// The hue is the mean of the sky's zenith (`sky_color`) and its horizon
/// (`fog_color`) in linear light, at unit luminance, and the light leans
/// [`AMBIENT_SKY_TINT`] of the way from white toward it - less where that
/// would carry a channel outside [`AMBIENT_CHANNEL_RANGE`] times the
/// luminance, so a saturated sky tints the shade rather than flooding it
/// with one primary. Leaning between two colours of unit luminance keeps
/// unit luminance, so `ambient_brightness` keeps meaning the light's
/// strength and only its colour moves. A sky with no colour to give, black
/// or grey, leaves it white.
///
/// [`AMBIENT_SKY_TINT`]: crate::config::lighting::AMBIENT_SKY_TINT
/// [`AMBIENT_CHANNEL_RANGE`]: crate::config::lighting::AMBIENT_CHANNEL_RANGE
pub(crate) fn sky_ambient_color(sky: [f32; 3], horizon: [f32; 3]) -> Color {
    use crate::config::lighting::{AMBIENT_CHANNEL_RANGE, AMBIENT_SKY_TINT};
    let linear = |c: [f32; 3]| {
        let l = Color::srgb(c[0], c[1], c[2]).to_linear();
        [l.red, l.green, l.blue]
    };
    let luminance = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    let (zenith, edge) = (linear(sky), linear(horizon));
    let mean = [
        (zenith[0] + edge[0]) * 0.5,
        (zenith[1] + edge[1]) * 0.5,
        (zenith[2] + edge[2]) * 0.5,
    ];
    let lum = luminance(mean);
    if lum <= 1.0e-5 {
        return Color::WHITE;
    }
    let c = lean_toward(
        mean.map(|c| c / lum),
        AMBIENT_SKY_TINT,
        AMBIENT_CHANNEL_RANGE,
    );
    Color::linear_rgb(c[0], c[1], c[2])
}

/// White leant `lean` of the way toward `hue`, a colour of unit luminance.
/// Leaning between two colours of unit luminance keeps unit luminance, so
/// the lean is cut short only where it would carry a channel outside
/// `(lo, hi)` - and the result is never renormalised, which is what let a
/// clamped channel overshoot the range again (#1669).
fn lean_toward(hue: [f32; 3], lean: f32, (lo, hi): (f32, f32)) -> [f32; 3] {
    let lean = hue.iter().fold(lean, |lean, &h| {
        if h > 1.0 {
            lean.min((hi - 1.0) / (h - 1.0))
        } else if h < 1.0 {
            lean.min((1.0 - lo) / (1.0 - h))
        } else {
            lean
        }
    });
    hue.map(|h| 1.0 + lean * (h - 1.0))
}

/// The world camera's grade from the record's three `grade_*` fields
/// (#1669); everything else on the camera's grade is left as Bevy's
/// neutral (see `config::lighting::grading` for why there is no contrast).
pub(crate) fn apply_grade(env: &crate::pds::Environment, grade: &mut ColorGrading) {
    grade.global.temperature = env.grade_temperature.0;
    grade.global.tint = env.grade_tint.0;
    grade.global.post_saturation = env.grade_saturation.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear(c: Color) -> [f32; 3] {
        let l = c.to_linear();
        [l.red, l.green, l.blue]
    }

    fn luminance(c: [f32; 3]) -> f32 {
        0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
    }

    /// The ambient light takes the sky's hue and keeps unit luminance, so
    /// `ambient_brightness` still means its strength (#1669).
    #[test]
    fn the_ambient_light_takes_the_skys_hue_at_unit_luminance() {
        let blue = linear(sky_ambient_color([0.35, 0.48, 0.66], [0.55, 0.62, 0.72]));
        assert!(blue[2] > blue[1] && blue[1] > blue[0], "{blue:?}");
        assert!((luminance(blue) - 1.0).abs() < 1e-4, "{blue:?}");
        let dusk = linear(sky_ambient_color([0.75, 0.5, 0.3], [0.8, 0.6, 0.4]));
        assert!(dusk[0] > dusk[2], "{dusk:?}");
        assert!((luminance(dusk) - 1.0).abs() < 1e-4, "{dusk:?}");
    }

    /// A grey sky has no hue to give and a black one no light: both leave
    /// the ambient white, as it was before #1669.
    #[test]
    fn a_grey_or_black_sky_leaves_the_ambient_white() {
        for (sky, horizon) in [([0.533; 3], [0.533; 3]), ([0.0; 3], [0.0; 3])] {
            let c = linear(sky_ambient_color(sky, horizon));
            for channel in c {
                assert!((channel - 1.0).abs() < 1e-3, "{sky:?}: {c:?}");
            }
        }
    }

    /// A saturated sky tints the shade without flooding it with one
    /// primary: every channel stays within the configured multiples of the
    /// luminance.
    #[test]
    fn a_saturated_sky_cannot_flood_the_shade() {
        let (lo, hi) = crate::config::lighting::AMBIENT_CHANNEL_RANGE;
        for sky in [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            let c = linear(sky_ambient_color(sky, sky));
            let l = luminance(c);
            for channel in c {
                assert!(
                    channel >= lo * l * 0.999 && channel <= hi * l * 1.001,
                    "{sky:?}: {c:?}"
                );
            }
        }
    }

    /// The lean holds every channel inside its range at either end, at unit
    /// luminance - the low end too, which the shipped tint never reaches.
    #[test]
    fn the_lean_holds_both_ends_of_the_range_at_unit_luminance() {
        let range = (0.35, 2.0);
        // Pure blue and pure green at unit luminance. Blue's one channel
        // sits far above 1, so its lean stops at the high end; green's sits
        // a little above 1 with the other two at 0, so its stops at the low.
        let blue = [0.0, 0.0, 1.0 / 0.0722];
        let green = [0.0, 1.0 / 0.7152, 0.0];
        for (hue, binds) in [(blue, 2usize), (green, 0)] {
            for lean in [0.4, 1.0] {
                let c = lean_toward(hue, lean, range);
                assert!((luminance(c) - 1.0).abs() < 1e-4, "{lean}: {c:?}");
                for channel in c {
                    assert!(
                        (range.0 - 1e-4..=range.1 + 1e-4).contains(&channel),
                        "{lean}: {c:?}"
                    );
                }
            }
            let c = lean_toward(hue, 1.0, range);
            let end = if binds == 0 { range.0 } else { range.1 };
            assert!(
                (c[binds] - end).abs() < 1e-4,
                "{hue:?} at a full lean: {c:?}"
            );
        }
    }

    /// The camera's grade follows the record's three fields (#1669), and
    /// leaves the sections' contrast neutral.
    #[test]
    fn the_cameras_grade_follows_the_record() {
        let env = crate::pds::Environment {
            grade_temperature: crate::pds::Fp(0.02),
            grade_tint: crate::pds::Fp(-0.004),
            grade_saturation: crate::pds::Fp(0.7),
            ..default()
        };
        let mut grade = ColorGrading::default();
        apply_grade(&env, &mut grade);
        assert_eq!(grade.global.temperature, 0.02);
        assert_eq!(grade.global.tint, -0.004);
        assert_eq!(grade.global.post_saturation, 0.7);
        for section in [grade.shadows, grade.midtones, grade.highlights] {
            assert_eq!(section.contrast, 1.0);
        }
    }
}
