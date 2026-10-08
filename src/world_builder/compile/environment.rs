//! Atmospheric `Environment` projection: sun, ambient, sky, fog, and the
//! cloud-deck shader uniforms. Reads the active
//! [`RoomRecord::environment`](crate::pds::RoomRecord::environment)
//! and re-paints every renderer-side resource the editor sliders touch.

use bevy::light::GlobalAmbientLight;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;

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
/// for this one, which patches light, fog, sky and cloud uniforms and is
/// safe at frame rate.
///
/// A resource rather than a flag on the record because the record's change
/// tick IS the debounce signal; a second signal is the only way to say
/// "cheap consumers only".
#[derive(Resource, Default)]
pub struct EnvironmentPreview;

/// The sky cuboid's material and transform: no sun is the sky, and the sky
/// is not the cloud deck, so the queries stay disjoint.
type SkyQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static MeshMaterial3d<StandardMaterial>,
        &'static mut Transform,
    ),
    (
        With<crate::SkyBox>,
        Without<CloudLayer>,
        Without<DirectionalLight>,
    ),
>;

/// How far the room's air lets a visitor see (m): its own fog visibility,
/// opened round a Berlin region's far field (#1585) so its horizon reads
/// ([`crate::terrain::geo::far::FarField::horizon_m`]). Only a far field
/// that landed opens it: a region whose horizon could not be had keeps its
/// own fog, which hides the end of its walkable ground. Every reader of the
/// fog asks this, so ground cover and shadows reach as far as the air lets
/// the eye.
pub(crate) fn fog_visibility(
    record: &crate::pds::RoomRecord,
    heightmap: Option<&crate::terrain::FinishedHeightMap>,
) -> f32 {
    let own = record.environment.fog_visibility.0;
    far_field(heightmap).map_or(own, |far| own.max(far.horizon_m()))
}

/// The sky cuboid's scale (its half-width, m): the default backdrop, or,
/// round a Berlin region's far field, wide enough to stand beyond its
/// horizon from anywhere on the walkable ground (#1585).
pub(crate) fn sky_scale(heightmap: Option<&crate::terrain::FinishedHeightMap>) -> f32 {
    let default = crate::config::lighting::SKY_SCALE;
    match (heightmap, far_field(heightmap)) {
        (Some(map), Some(far)) => {
            let core_m = (map.0.width() - 1) as f32 * map.0.scale();
            default.max(far.sky_half_m(core_m))
        }
        _ => default,
    }
}

/// The far field the ground landed with, if any.
fn far_field(
    heightmap: Option<&crate::terrain::FinishedHeightMap>,
) -> Option<&crate::terrain::geo::far::FarField> {
    Some(heightmap?.ground()?.far()?.as_ref())
}

/// Apply the active `RoomRecord`'s `Environment` to every atmospheric
/// resource in the scene - sun, ambient, sky cuboid, clear colour, and
/// distance fog. Runs on every `RoomRecord` change so an editor slider
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
    mut skybox: SkyQuery,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
    mut cloud_layer: Query<(&MeshMaterial3d<CloudMaterial>, &mut Transform), With<CloudLayer>>,
    mut cloud_materials: ResMut<Assets<CloudMaterial>>,
    mut water_materials: ResMut<Assets<crate::water::WaterMaterial>>,
    vegetation_wind: Option<ResMut<crate::wind::VegetationWind>>,
    // The cheap lane (#1249 f59): the editor stamps this every frame a
    // widget changes, so a drag repaints continuously while the broadcast
    // and the recompile still wait for the pause.
    preview: Option<Res<EnvironmentPreview>>,
    // A landing far field opens the haze and widens the sky (#1585).
    heightmap: Option<Res<crate::terrain::FinishedHeightMap>>,
) {
    let Some(record) = record else {
        return;
    };
    let previewing = preview.is_some_and(|p| p.is_changed());
    let landed = heightmap.as_ref().is_some_and(|h| h.is_changed());
    if !record.is_changed() && !previewing && !landed {
        return;
    }
    let heightmap = heightmap.as_deref();
    let record = &record.0;
    let env = &record.environment;

    let Fp3(sun_c) = env.sun_color;
    // Snapshot the runtime sun direction (unit vector *toward* the sun) so
    // the cloud shader can shade the underside without a real lighting
    // pass. The directional light's forward axis points from the light
    // toward its target, so the unit toward-sun vector is `-forward()`.
    // Falls back to world Y when the light's transform is degenerate.
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

    ambient_light.brightness = env.ambient_brightness.0;

    let Fp3(sky_c) = env.sky_color;
    clear_color.0 = Color::srgb(sky_c[0], sky_c[1], sky_c[2]);
    // Sized past a Berlin region's horizon (#1585); the follow system keeps
    // it centred on the camera.
    let sky_scale = Vec3::splat(sky_scale(heightmap));
    for (material_handle, mut transform) in skybox.iter_mut() {
        if let Some(mut mat) = std_materials.get_mut(&material_handle.0) {
            mat.base_color = Color::srgb(sky_c[0], sky_c[1], sky_c[2]);
        }
        if transform.scale != sky_scale {
            transform.scale = sky_scale;
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
            fog_visibility(record, heightmap),
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
        if let Some(mut mat) = cloud_materials.get_mut(&material_handle.0) {
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
    // (#662). Patched here rather than at water spawn only, because a
    // sun-position slider drag retints the world without recompiling the
    // water volumes - every live water material shares the one global sun.
    for (_, mat) in water_materials.iter_mut() {
        mat.extension.uniforms.sun_dir = Vec4::new(sun_dir.x, sun_dir.y, sun_dir.z, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{Fp, RoomRecord};
    use crate::terrain::FinishedHeightMap;
    use crate::terrain::geo::GeoGround;
    use crate::terrain::geo::far::FarField;

    fn record(fog_m: f32) -> RoomRecord {
        let mut record = RoomRecord::default_for_did("did:test:horizon");
        record.environment.fog_visibility = Fp(fog_m);
        record
    }

    /// A 200 m core of Berlin's ground, round `far` where one landed.
    fn ground(far: Option<FarField>) -> FinishedHeightMap {
        let mut ground = GeoGround::from_cover(101, 2.0, vec![None; 101 * 101], None);
        if let Some(far) = far {
            ground = ground.with_far(far);
        }
        FinishedHeightMap(
            bevy_symbios_ground::HeightMap::new(101, 101, 2.0),
            Some(ground),
        )
    }

    /// A far field 10 km across.
    fn far() -> FarField {
        FarField::from_fn(250, 40.0, |_, _| 40.0)
    }

    /// #1585: the haze opens round a far field that landed, and is the
    /// world's own everywhere else - a region whose horizon could not be had
    /// keeps the fog that hides the end of its walkable ground.
    #[test]
    fn the_fog_opens_round_a_far_field_that_landed() {
        assert_eq!(fog_visibility(&record(300.0), None), 300.0, "no ground yet");
        assert_eq!(
            fog_visibility(&record(300.0), Some(&ground(None))),
            300.0,
            "Berlin's ground with no horizon"
        );
        assert_eq!(
            fog_visibility(&record(300.0), Some(&ground(Some(far())))),
            10_000.0
        );
        assert_eq!(
            fog_visibility(&record(15_000.0), Some(&ground(Some(far())))),
            15_000.0,
            "a clearer fog is kept"
        );
    }

    #[test]
    fn the_sky_stands_past_the_horizon() {
        let default = crate::config::lighting::SKY_SCALE;
        assert_eq!(sky_scale(None), default);
        assert_eq!(sky_scale(Some(&ground(None))), default);
        // Past the far edge of 10 km seen from the near edge of the 200 m
        // core.
        let half = sky_scale(Some(&ground(Some(far()))));
        assert!(half > 10_000.0 / 2.0 + 200.0 / 2.0, "{half}");
    }
}
