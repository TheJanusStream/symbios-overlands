//! Atmospheric `Environment` projection: sun, ambient, sky, fog, and the
//! cloud-deck shader uniforms. Reads the active [`RoomRecord::environment`]
//! and re-paints every renderer-side resource the editor sliders touch.

use bevy::light::GlobalAmbientLight;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;

use crate::clouds::{CloudLayer, CloudMaterial};
use crate::pds::{Fp3, Fp4, RoomRecord};

/// Apply the active `RoomRecord`'s `Environment` to every atmospheric
/// resource in the scene — sun, ambient, sky cuboid, clear colour, and
/// distance fog. Runs on every `RoomRecord` change so an editor slider
/// (or peer broadcast) retints the world without restarting the session.
///
/// Kept separate from `compile_room_record` because the combined
/// signature would exceed Bevy's 16-param `IntoSystem` limit; splitting
/// it out also lets Bevy schedule the two passes in parallel when their
/// resource borrows don't conflict.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_environment_state(
    record: Option<Res<RoomRecord>>,
    // `Without<CloudLayer>` keeps this query disjoint from the
    // `cloud_layer` query below (which holds `&mut Transform`). Bevy's
    // borrow checker conservatively assumes any pair of queries that
    // touch `Transform` could match the same entity unless we tell it
    // otherwise — and a directional light entity never carries the
    // `CloudLayer` marker, so the filter has no runtime cost.
    mut lights: Query<(&mut DirectionalLight, &Transform), Without<CloudLayer>>,
    mut clear_color: ResMut<ClearColor>,
    mut ambient_light: ResMut<GlobalAmbientLight>,
    mut fog: Query<&mut DistanceFog>,
    skybox: Query<&MeshMaterial3d<StandardMaterial>, With<crate::SkyBox>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
    mut cloud_layer: Query<(&MeshMaterial3d<CloudMaterial>, &mut Transform), With<CloudLayer>>,
    mut cloud_materials: ResMut<Assets<CloudMaterial>>,
) {
    let Some(record) = record else {
        return;
    };
    if !record.is_changed() {
        return;
    }
    let env = &record.environment;

    let Fp3(sun_c) = env.sun_color;
    // Snapshot the runtime sun direction (unit vector *toward* the sun) so
    // the cloud shader can shade the underside without a real lighting
    // pass. The directional light's forward axis points from the light
    // toward its target, so the unit toward-sun vector is `-forward()`.
    // Falls back to world Y when the light's transform is degenerate.
    let mut sun_dir = Vec3::Y;
    for (mut light, transform) in lights.iter_mut() {
        light.color = Color::srgb(sun_c[0], sun_c[1], sun_c[2]);
        light.illuminance = env.sun_illuminance.0;
        sun_dir = (-transform.forward().as_vec3()).normalize_or(Vec3::Y);
    }

    ambient_light.brightness = env.ambient_brightness.0;

    let Fp3(sky_c) = env.sky_color;
    clear_color.0 = Color::srgb(sky_c[0], sky_c[1], sky_c[2]);
    for material_handle in skybox.iter() {
        if let Some(mat) = std_materials.get_mut(&material_handle.0) {
            mat.base_color = Color::srgb(sky_c[0], sky_c[1], sky_c[2]);
        }
    }

    let Fp4(fog_c) = env.fog_color;
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
    // re-positions and re-lights the deck in the same change tick.
    let Fp3(cloud_c) = env.cloud_color;
    let Fp3(cloud_sh) = env.cloud_shadow_color;
    let crate::pds::Fp2(wind) = env.cloud_wind_dir;
    for (material_handle, mut transform) in cloud_layer.iter_mut() {
        transform.translation.y = env.cloud_height.0;
        if let Some(mat) = cloud_materials.get_mut(&material_handle.0) {
            mat.extension.uniforms.color = Vec4::new(cloud_c[0], cloud_c[1], cloud_c[2], 1.0);
            mat.extension.uniforms.shadow_color =
                Vec4::new(cloud_sh[0], cloud_sh[1], cloud_sh[2], 1.0);
            mat.extension.uniforms.fog_color = Vec4::new(fog_c[0], fog_c[1], fog_c[2], fog_c[3]);
            mat.extension.uniforms.sun_dir = Vec4::new(sun_dir.x, sun_dir.y, sun_dir.z, 0.0);
            mat.extension.uniforms.wind_dir = Vec2::new(wind[0], wind[1]);
            mat.extension.uniforms.cover = env.cloud_cover.0;
            mat.extension.uniforms.density = env.cloud_density.0;
            mat.extension.uniforms.softness = env.cloud_softness.0;
            mat.extension.uniforms.speed = env.cloud_speed.0;
            mat.extension.uniforms.scale = env.cloud_scale.0;
            // Mirror the underlying StandardMaterial's base colour to the
            // sunlit tint so any non-shader fallback path (e.g. an asset
            // inspector) still shows a recognisable cloud colour.
            mat.base.base_color = Color::srgb(cloud_c[0], cloud_c[1], cloud_c[2]);
        }
    }
}
