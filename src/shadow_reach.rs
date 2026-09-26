//! The sun's shadow cascades follow the camera's zoom (#1475).
//!
//! Bevy measures every cascade bound from the camera, along its view axis,
//! and the sun's [`CascadeShadowConfig`] used to be built once: the first
//! cascade out to 15 m, the last to 200 m. That is right for the chase camera
//! at rest, 12 m from the player, and wrong for the 200 m the wheel zooms out
//! to. There the shadows ended at the player, so the far half of the view had
//! none and a hard line crossed the ground where they stopped - and three of
//! the four cascades (ending at 15, 36 and 84 m) were spent on the air
//! between the camera and the ground.
//!
//! [`reach`] is the rule, a pure function of the camera's distance to its
//! focus and the room's fog. [`follow`] writes it to the sun when a bound
//! really moves, and never otherwise. [`follow_orbit_zoom`] feeds it the
//! world camera every frame; the render tool's rig camera, which has no orbit
//! controller, feeds it through `render_tool::headless::follow_rig_zoom`, so
//! a far `--world` shot is shaded the way the game shades it.
//!
//! There is no fade at the far edge, and there must not be one made from a
//! `VisibilityRange` margin: any non-zero margin compiles Bevy's dither
//! shader, which fails WebGL2 pipeline validation and quits every web client
//! (#1358). The fog is the fade - see [`reach`].

use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::ScheduleSystem;
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder, SimulationLightSystems};
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_panorbit_camera::PanOrbitCamera;

use crate::camera::IsWorldCamera;
use crate::config::{camera, lighting};
use crate::state::LiveRoomRecord;

/// Where a cascade set is cut, in metres from the camera along its view axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Reach {
    /// Far bound of the first - the sharpest - cascade.
    pub(crate) first_far: f32,
    /// Far bound of the last cascade: nothing further gets a shadow.
    pub(crate) max: f32,
}

/// The chase camera at rest: the bounds the sun was always built with, and
/// the ones it spawns with.
pub(crate) const REST: Reach = Reach {
    first_far: lighting::CASCADE_FIRST_FAR,
    max: lighting::CASCADE_MAX_DIST,
};

/// First-cascade far bound per metre of camera-to-focus distance: the
/// 15 / 12 = 1.25 the rest zoom has, so the first cascade ends a quarter of
/// the distance past the player - until
/// [`lighting::CASCADE_FIRST_SHARE_MAX`] caps it.
const FIRST_PER_METRE: f32 = lighting::CASCADE_FIRST_FAR / camera::ORBIT_RADIUS;

/// How far past the focus the shadows reach at rest: 200 - 12 = 188 m.
const REACH_PAST_FOCUS: f32 = lighting::CASCADE_MAX_DIST - camera::ORBIT_RADIUS;

/// A bound that moved by less than this share of itself is not a change.
///
/// While the player walks, the orbit's focus moves every frame and the
/// camera's distance to it comes back from float arithmetic a few units in
/// the last place either side of the radius; comparing exactly would restamp
/// the sun every frame of every walk. A thousandth is under 40 cm on any
/// bound at the 200 m zoom, against cascades some hundreds of metres wide
/// whose width Bevy rounds up to a whole metre anyway.
const RETUNE_SHARE: f32 = 1.0e-3;

/// The cascade cuts for a camera `view_dist` metres from its focus, under fog
/// that leaves `fog_visibility` metres visible.
///
/// - **At or inside the rest zoom** ([`camera::ORBIT_RADIUS`], 12 m) it is
///   [`REST`], 15 m and 200 m, whatever the fog: what the player sees at the
///   zoom the game opens on does not change.
/// - **The first cascade ends a quarter of the distance past the focus**, as
///   it does at rest ([`FIRST_PER_METRE`]). Bevy sizes each cascade's
///   square map by the diagonal of the view frustum's slice at the
///   cascade's far bound, so at a 45 deg lens, 16:9 and the default
///   2048-texel map, one shadow texel is about one pixel of a 1080-line
///   window at the far end of EVERY cascade, and about 1.35 pixels at 0.8 of
///   it. Holding the first bound in proportion to the distance holds the
///   player's own shadow at the density it has at rest (1.34 pixels a
///   texel), however far out the camera is.
/// - **The shadows reach as far past the focus as they do at rest**,
///   [`REACH_PAST_FOCUS`] (188 m)...
/// - **...but never into the fog.** Past `fog_visibility` nothing keeps even
///   5% of its contrast, so a cascade spent there is spent on haze - and
///   where the fog is nearer than that reach, the fog is what hides the
///   edge. The fog only caps what the zoom adds: it never pulls the reach
///   under the rest zoom's 200 m.
/// - **The first cascade takes at most
///   [`lighting::CASCADE_FIRST_SHARE_MAX`] of the reach**, so the three
///   cascades beyond it keep room to split, and the first bound always ends
///   inside the reach. Nothing downstream would catch it if it did not:
///   Bevy's builder takes a first bound past the maximum without complaint
///   and splits it backwards, so the first cascade would shade every depth
///   out to its own bound, past the fog, and the rest would go all but
///   unused. The cap keeps the split rising; no panic is being averted.
///
/// At the 200 m zoom under the default 350 m fog this cuts at 175, 220, 278
/// and 350 m, where it used to cut at 15, 36, 84 and 200 m: the player sits
/// in the second cascade, about 1.2 pixels a texel, and the ground runs
/// shaded 150 m past them into the fog. WebGL2 splits no cascades at all -
/// Bevy builds one there, out to `max` - so the web client gets the longer
/// reach, with one texel for 1.9 pixels at the player at full zoom, never
/// coarser than the 18 it has at rest.
///
/// A distance that is not a finite number is a fault upstream; it gets the
/// rest cuts rather than bounds Bevy cannot split.
pub(crate) fn reach(view_dist: f32, fog_visibility: f32) -> Reach {
    let d = if view_dist.is_finite() {
        view_dist.max(0.0)
    } else {
        0.0
    };
    // `min` then `max`: a NaN fog is ignored by `min`, and the rest reach
    // is a floor whatever the fog says.
    let max = (d + REACH_PAST_FOCUS).min(fog_visibility).max(REST.max);
    let first_far = (FIRST_PER_METRE * d)
        .max(REST.first_far)
        .min(max * lighting::CASCADE_FIRST_SHARE_MAX);
    Reach { first_far, max }
}

/// The sun's cascade config for `reach`, on Bevy's defaults for everything
/// else - four cascades natively, one on WebGL2, 20% overlap, 0.1 m near.
pub(crate) fn cascades(reach: Reach) -> CascadeShadowConfig {
    CascadeShadowConfigBuilder {
        first_cascade_far_bound: reach.first_far,
        maximum_distance: reach.max,
        ..default()
    }
    .build()
}

/// Whether `wanted` differs from `current` by more than [`RETUNE_SHARE`] in
/// any bound, or at all in anything else.
fn moved(current: &CascadeShadowConfig, wanted: &CascadeShadowConfig) -> bool {
    current.bounds.len() != wanted.bounds.len()
        || current.minimum_distance != wanted.minimum_distance
        || current.overlap_proportion != wanted.overlap_proportion
        || current
            .bounds
            .iter()
            .zip(&wanted.bounds)
            .any(|(c, w)| (c - w).abs() > RETUNE_SHARE * w)
}

/// Every sun's cascade config.
pub(crate) type SunCascades<'w, 's> =
    Query<'w, 's, &'static mut CascadeShadowConfig, With<DirectionalLight>>;

/// Cut the sun's cascades for a camera `view_dist` metres from its focus,
/// under the active room's fog (the config default before a room is
/// entered). Writes only a config that [`moved`]: reading through the `Mut`
/// stamps nothing, and a write stamps whether or not the value changed.
pub(crate) fn follow(view_dist: f32, record: Option<&LiveRoomRecord>, suns: &mut SunCascades) {
    let fog = record.map_or(camera::fog::VISIBILITY, |r| {
        r.0.environment.fog_visibility.0
    });
    let wanted = cascades(reach(view_dist, fog));
    for mut config in suns.iter_mut() {
        if moved(&config, &wanted) {
            *config = wanted.clone();
        }
    }
}

/// The world camera's distance to its orbit focus, where it is DRAWN this
/// frame: the eased orbit, after `camera::clamp_camera_to_terrain` has
/// pulled it in along its ray, which the orbit's own `radius` does not know
/// about.
pub(crate) fn follow_orbit_zoom(
    cameras: Query<(&Transform, &PanOrbitCamera), IsWorldCamera>,
    record: Option<Res<LiveRoomRecord>>,
    mut suns: SunCascades,
) {
    let Ok((transform, orbit)) = cameras.single() else {
        return;
    };
    follow(
        transform.translation.distance(orbit.focus),
        record.as_deref(),
        &mut suns,
    );
}

/// Schedule a feeder of [`follow`] where it has to run: in `PostUpdate`,
/// after the camera's `Transform` is final - the orbit crate and the terrain
/// clamp both write it before propagation - and before Bevy cuts this
/// frame's cascades from the config, so a zoom is shaded on the frame it is
/// drawn.
pub(crate) fn register<M>(app: &mut App, feeder: impl IntoScheduleConfigs<ScheduleSystem, M>) {
    app.add_systems(
        PostUpdate,
        feeder
            .after(TransformSystems::Propagate)
            .before(SimulationLightSystems::UpdateDirectionalLightCascades),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::WorldCamera;
    use crate::config::camera::fog::VISIBILITY;
    use crate::pds::{Fp, RoomRecord};

    /// Fogs from the sanitizer's floor to its ceiling, through the seeded
    /// biomes' range (180 to 600 m) and the config default.
    const FOGS: [f32; 8] = [
        10.0, 150.0, 180.0, 300.0, VISIBILITY, 600.0, 1_000.0, 10_000.0,
    ];

    /// Equal but for float rounding: Bevy computes each bound as a power of
    /// the split ratio, so even the last one lands a few units in the last
    /// place off `maximum_distance`.
    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1.0e-5 * b.abs().max(1.0)
    }

    fn same(a: &[f32], b: &[f32]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(a, b)| near(*a, *b))
    }

    #[test]
    fn the_rest_zoom_and_closer_keep_the_cuts_the_game_always_had() {
        for fog in FOGS {
            for d in [0.0, camera::ZOOM_LOWER_LIMIT, 8.0, camera::ORBIT_RADIUS] {
                assert_eq!(reach(d, fog), REST, "{d} m under {fog} m fog");
            }
        }
        assert_eq!(REST.first_far, 15.0);
        assert_eq!(REST.max, 200.0);
    }

    /// The camera at full zoom-out sits 200 m from the player. The reach has
    /// to carry on well past them, up to the fog - the owner's report was
    /// that the shadows stopped at the player.
    #[test]
    fn full_zoom_out_shades_well_past_the_player() {
        let d = camera::ZOOM_UPPER_LIMIT;
        let r = reach(d, VISIBILITY);
        assert_eq!(r.max, VISIBILITY, "the default fog is the edge: {r:?}");
        assert!(r.max >= d + 100.0, "{r:?}");
        // A clear room: the zoom adds the rest zoom's reach past the focus.
        let clear = reach(d, 10_000.0);
        assert_eq!(clear.max, d + REACH_PAST_FOCUS, "{clear:?}");
        assert_eq!(clear.max, 388.0);
    }

    /// Depth along the view axis of the nearest ground the camera can see:
    /// the bottom edge of the frame meeting flat ground at the focus's
    /// height, for a camera `d` from the focus at `pitch`, 45 deg lens.
    fn nearest_ground_depth(d: f32, pitch: f32) -> f32 {
        let half_fov = std::f32::consts::FRAC_PI_8;
        let height = d * pitch.sin();
        height / (pitch + half_fov).sin() * half_fov.cos()
    }

    /// Zoomed right out, the old first cascade ended 15 m from the camera,
    /// in the air 78 m above the ground. The new one has to reach the ground
    /// the camera actually sees - at the game's own pitch, nothing is nearer
    /// than half the distance.
    #[test]
    fn full_zoom_out_ends_the_first_cascade_on_the_ground_not_in_the_air() {
        let d = camera::ZOOM_UPPER_LIMIT;
        let nearest = nearest_ground_depth(d, camera::ORBIT_PITCH);
        assert!((nearest / d - 0.505).abs() < 1e-3, "{nearest}");
        for fog in [VISIBILITY, 600.0, 10_000.0] {
            let r = reach(d, fog);
            assert!(r.first_far > nearest, "{r:?} under {fog} m fog: {nearest}");
        }
        // Where the fog allows, the first cascade holds the rest zoom's
        // ratio: it ends a quarter of the distance past the focus.
        assert_eq!(reach(100.0, 1_000.0).first_far, 125.0);
        assert!(REST.first_far < nearest, "the old cut was in the air");
    }

    #[test]
    fn the_cuts_never_pull_in_as_the_camera_backs_off() {
        for fog in FOGS {
            let mut last = reach(0.0, fog);
            for step in 1..=1_000 {
                let d = step as f32 * 0.5;
                let r = reach(d, fog);
                assert!(
                    r.first_far >= last.first_far && r.max >= last.max,
                    "{d} m under {fog} m fog: {last:?} -> {r:?}"
                );
                last = r;
            }
        }
    }

    /// First bound inside the reach, the split Bevy builds from it rising,
    /// past Bevy's 0.1 m near bound, every value finite - for any distance
    /// and any fog, including the ones a fault upstream could hand over.
    ///
    /// Nothing downstream guards the ordering. Bevy's builder does not check
    /// it (`an_inverted_pair_is_split_backwards_not_refused`): an inverted
    /// pair would draw, badly, with no panic to show for it. What
    /// the builder does refuse - a panic, which aborts the client - is a
    /// first bound at or under its near bound, NaN included.
    #[test]
    fn the_first_cascade_always_ends_inside_the_reach() {
        let odd = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -5.0, 0.0];
        let distances = (0..=100).map(|i| i as f32 * 5.0).chain([1.0e6]).chain(odd);
        for d in distances {
            for fog in FOGS.into_iter().chain(odd) {
                let r = reach(d, fog);
                assert!(
                    r.first_far.is_finite() && r.max.is_finite(),
                    "{d}/{fog}: {r:?}"
                );
                let built = cascades(r);
                assert!(
                    built.bounds.windows(2).all(|w| w[0] < w[1]),
                    "{d}/{fog}: the split runs backwards: {built:?}"
                );
                assert!(r.first_far < r.max, "{d}/{fog}: {r:?}");
                assert!(
                    r.first_far <= r.max * lighting::CASCADE_FIRST_SHARE_MAX,
                    "{d}/{fog}: {r:?}"
                );
                assert!(built.minimum_distance < r.first_far, "{d}/{fog}: {built:?}");
                assert!(built.bounds.iter().all(|b| b.is_finite()), "{built:?}");
                assert!(
                    near(*built.bounds.last().unwrap(), r.max),
                    "{built:?} vs {r:?}"
                );
            }
        }
    }

    /// `shadows.wgsl`'s `get_cascade_index`: a fragment is shaded from the
    /// first cascade whose far bound lies beyond its depth, and from none
    /// past them all.
    fn cascade_at(bounds: &[f32], depth: f32) -> Option<usize> {
        bounds.iter().position(|far| depth < *far)
    }

    /// What the share cap is and is not for, read off Bevy 0.19 rather than
    /// assumed. Its builder does not check that the first bound lies inside
    /// the maximum: an inverted pair builds, and splits backwards, so the
    /// first cascade takes every depth out to its own bound - past the
    /// maximum - and the last two are never looked up. What the builder
    /// does refuse, with more than one cascade, is a first bound at or under
    /// its 0.1 m near bound, NaN included; `reach`'s floors keep that out.
    #[test]
    fn an_inverted_pair_is_split_backwards_not_refused() {
        let inverted = CascadeShadowConfigBuilder {
            num_cascades: 4,
            first_cascade_far_bound: 250.0,
            maximum_distance: 200.0,
            ..default()
        };
        let built = std::panic::catch_unwind(|| inverted.build())
            .expect("Bevy builds an inverted pair without a word");
        let b = &built.bounds;
        assert!(b.windows(2).all(|w| w[0] > w[1]), "falling: {b:?}");
        assert!(near(b[0], 250.0) && near(b[3], 200.0), "{b:?}");
        for depth in [1.0, 100.0, 199.0, 220.0, 249.0] {
            assert_eq!(cascade_at(b, depth), Some(0), "{depth} m: {b:?}");
        }
        assert_eq!(cascade_at(b, 251.0), None, "the shadows end at 250");

        for first in [f32::NAN, 0.0, 0.1] {
            let refused = std::panic::catch_unwind(|| {
                CascadeShadowConfigBuilder {
                    num_cascades: 4,
                    first_cascade_far_bound: first,
                    maximum_distance: 200.0,
                    ..default()
                }
                .build()
            });
            assert!(refused.is_err(), "a first bound of {first} is refused");
        }
        // One cascade (WebGL2) skips that check: the maximum is all it uses.
        let one = CascadeShadowConfigBuilder {
            num_cascades: 1,
            first_cascade_far_bound: f32::NAN,
            maximum_distance: 200.0,
            ..default()
        }
        .build();
        assert_eq!(one.bounds, vec![200.0]);
    }

    #[test]
    fn a_sub_share_wobble_is_not_a_move_and_anything_more_is() {
        let at = cascades(reach(150.0, VISIBILITY));
        assert!(!moved(&at, &at.clone()));
        assert!(!moved(&at, &cascades(reach(150.01, VISIBILITY))));
        assert!(moved(&at, &cascades(reach(151.0, VISIBILITY))));
        let mut fewer = at.clone();
        fewer.bounds.pop();
        assert!(moved(&fewer, &at), "a different cascade count is a move");
    }

    // ------------------------------------------------------------------
    // The system, in an app
    // ------------------------------------------------------------------

    const FOCUS: Vec3 = Vec3::new(40.0, 12.0, -30.0);

    /// The game's direction from the focus to the camera at rest.
    fn away() -> Vec3 {
        Vec3::new(0.0, camera::ORBIT_PITCH.sin(), camera::ORBIT_PITCH.cos())
    }

    struct Scene {
        app: App,
        camera: Entity,
        sun: Entity,
    }

    /// The sun as `setup_lighting` spawns it, and the world camera at rest.
    fn scene() -> Scene {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        register(&mut app, follow_orbit_zoom);
        let sun = app
            .world_mut()
            .spawn((DirectionalLight::default(), cascades(REST)))
            .id();
        let camera = app
            .world_mut()
            .spawn((
                Camera3d::default(),
                WorldCamera,
                PanOrbitCamera {
                    focus: FOCUS,
                    radius: Some(camera::ORBIT_RADIUS),
                    ..default()
                },
                Transform::from_translation(FOCUS + away() * camera::ORBIT_RADIUS),
            ))
            .id();
        Scene { app, camera, sun }
    }

    impl Scene {
        /// Move the camera as the orbit crate does: the eased radius, and
        /// the transform `radius` out from the focus.
        fn zoom(&mut self, radius: f32) {
            self.drawn_at(radius, radius);
        }

        /// Set the eased radius, and draw the camera `drawn` from the focus.
        fn drawn_at(&mut self, radius: f32, drawn: f32) {
            let world = self.app.world_mut();
            world.get_mut::<PanOrbitCamera>(self.camera).unwrap().radius = Some(radius);
            world.get_mut::<Transform>(self.camera).unwrap().translation = FOCUS + away() * drawn;
        }

        fn config(&self) -> CascadeShadowConfig {
            self.app
                .world()
                .get::<CascadeShadowConfig>(self.sun)
                .unwrap()
                .clone()
        }

        fn tick(&self) -> bevy::ecs::change_detection::Tick {
            self.app
                .world()
                .entity(self.sun)
                .get_ref::<CascadeShadowConfig>()
                .unwrap()
                .last_changed()
        }
    }

    #[test]
    fn zooming_out_recuts_the_sun_and_a_still_camera_stamps_nothing() {
        let mut s = scene();
        let spawned = s.tick();
        s.app.update();
        s.app.update();
        assert_eq!(
            s.tick(),
            spawned,
            "at rest the spawn config is already right"
        );
        assert_eq!(s.config().bounds, cascades(REST).bounds);
        assert_eq!(s.config().bounds.len(), 4, "native: four cascades");

        s.zoom(camera::ZOOM_UPPER_LIMIT);
        s.app.update();
        let zoomed = s.tick();
        assert_ne!(zoomed, spawned, "the zoom re-cut the sun");
        let want = cascades(reach(camera::ZOOM_UPPER_LIMIT, VISIBILITY));
        assert!(same(&s.config().bounds, &want.bounds), "{:?}", s.config());
        assert!(near(s.config().bounds[0], 175.0), "{:?}", s.config());
        assert!(near(*s.config().bounds.last().unwrap(), VISIBILITY));

        for _ in 0..3 {
            s.app.update();
        }
        assert_eq!(s.tick(), zoomed, "an unchanged radius stamps no change");

        s.zoom(camera::ORBIT_RADIUS);
        s.app.update();
        assert!(same(&s.config().bounds, &cascades(REST).bounds), "and back");
    }

    /// The camera a hair nearer - the float noise of a walk - is not a
    /// change; a real step is.
    #[test]
    fn a_wobble_in_the_last_bits_does_not_restamp_the_sun() {
        let mut s = scene();
        s.zoom(150.0);
        s.app.update();
        let settled = s.tick();
        s.drawn_at(150.0, 150.0 - 1.0e-4);
        s.app.update();
        assert_eq!(s.tick(), settled, "a tenth of a millimetre is not a change");
        s.drawn_at(150.0, 140.0);
        s.app.update();
        assert_ne!(s.tick(), settled, "ten metres is");
    }

    /// The terrain clamp pulls the camera in along its ray without touching
    /// the orbit's radius; the cascades follow the camera that is drawn.
    #[test]
    fn a_terrain_clamped_camera_is_measured_where_it_is_drawn() {
        let mut s = scene();
        s.drawn_at(camera::ZOOM_UPPER_LIMIT, 60.0);
        s.app.update();
        assert!(
            same(
                &s.config().bounds,
                &cascades(reach(60.0, VISIBILITY)).bounds
            ),
            "60 m drawn, whatever the radius says: {:?}",
            s.config()
        );
    }

    /// The terrain clamp moves the camera in `PostUpdate`, before
    /// propagation. The cut has to read the camera after that, in the same
    /// frame, or a clamped camera is shaded for where it was a frame ago.
    #[test]
    fn the_cut_reads_the_camera_after_the_clamp_has_moved_it() {
        let mut s = scene();
        let clamp = |mut cameras: Query<&mut Transform, IsWorldCamera>| {
            for mut transform in &mut cameras {
                transform.translation = FOCUS + away() * 80.0;
            }
        };
        s.app
            .add_systems(PostUpdate, clamp.before(TransformSystems::Propagate));
        s.app.update();
        assert!(
            same(
                &s.config().bounds,
                &cascades(reach(80.0, VISIBILITY)).bounds
            ),
            "{:?}",
            s.config()
        );
    }

    #[test]
    fn the_room_s_fog_caps_the_reach() {
        let mut s = scene();
        let mut record = RoomRecord::default();
        record.environment.fog_visibility = Fp(260.0);
        s.app.insert_resource(LiveRoomRecord(record));
        s.zoom(camera::ZOOM_UPPER_LIMIT);
        s.app.update();
        assert!(
            near(*s.config().bounds.last().unwrap(), 260.0),
            "{:?}",
            s.config()
        );
    }
}
