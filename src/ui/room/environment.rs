//! Environment tab - directional sun, ambient, sky, fog, and room-wide water
//! widgets. Per-volume water appearance (colour, choppiness) lives on the
//! Water generator detail editor instead; the fields here are the ones that
//! should match the room's mood rather than varying between water bodies.
//!
//! It also hosts the owner-configurable **arrival point** (#773): the
//! [`crate::pds::DefaultLanding`] pose visitors come to rest at when they
//! enter without an explicit destination link (including through another
//! room's social gateway). Unset means the legacy random scatter near the
//! world origin.
//!
//! And the **region source** (#1583): whether the region is built from a
//! square of real Berlin ([`crate::pds::GeoSource`]) or from its seed alone.

use bevy::prelude::*;
use bevy_egui::egui;
use geodata::GeoSquare;
use geodata::berlin::{Coverage, Keep};
use geodata::square::{SIZE_MAX_M, SIZE_MIN_M, SIZE_STEP_M};

use crate::pds::geo_source::BERLIN;
use crate::pds::{DefaultLanding, Environment, Fp, Fp2, GeoSource};

use super::widgets::{color_picker, color_picker_rgba, fp_slider};

/// The local player's ground pose, captured once per frame from the
/// `LocalPlayer` transform so the "Set to my position & facing" button can
/// stamp it into the arrival point without the tab body touching the ECS.
#[derive(Clone, Copy)]
pub(super) struct PlayerPose {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Facing in the spawn convention (degrees, `Quat::from_rotation_y`),
    /// normalised into `[0, 360)`.
    pub yaw_deg: f32,
}

impl PlayerPose {
    /// Extract the pose from a world-space transform. The yaw is recovered
    /// from the transform's forward vector projected onto XZ so it inverts
    /// the spawn path's `Quat::from_rotation_y(yaw_deg)` exactly - and stays
    /// correct even when the chassis is tilted to a slope (the projection
    /// discards the pitch/roll the surface-normal alignment adds).
    pub(super) fn from_transform(tf: &Transform) -> Self {
        let fwd = tf.forward();
        let yaw_deg = (-fwd.x).atan2(-fwd.z).to_degrees().rem_euclid(360.0);
        Self {
            x: tf.translation.x,
            y: tf.translation.y,
            z: tf.translation.z,
            yaw_deg,
        }
    }
}

/// What the world as built makes of a Berlin square, for the Region source
/// section to show (#1586).
#[derive(Clone, Copy)]
pub(super) struct BuiltGround<'a> {
    /// Berlin's ground under the world, once it has landed.
    pub berlin: Option<&'a crate::terrain::geo::GeoGround>,
    /// Whether the world has water to draw at Berlin's level.
    pub has_water: bool,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_environment_tab(
    ui: &mut egui::Ui,
    env: &mut Environment,
    landing: &mut Option<DefaultLanding>,
    geo_source: &mut Option<GeoSource>,
    ground: BuiltGround<'_>,
    player_pose: Option<PlayerPose>,
    dirty: &mut bool,
    audio_editor: &mut super::audio::AudioEditorState,
    assets: &mut super::assets::AssetPanel<'_>,
) {
    ui.heading("Environment");
    ui.add_space(4.0);

    egui::CollapsingHeader::new("Region source")
        .default_open(false)
        .show(ui, |ui| draw_region_source(ui, geo_source, ground, dirty));
    draw_arrival_point(ui, landing, player_pose, dirty);

    egui::CollapsingHeader::new("Lighting & sky")
        .default_open(true)
        .show(ui, |ui| {
            color_picker(ui, "Sun colour", &mut env.sun_color, dirty);
            color_picker(ui, "Sky colour", &mut env.sky_color, dirty);
            fp_slider(
                ui,
                "Sun illuminance",
                &mut env.sun_illuminance,
                0.0,
                50_000.0,
                dirty,
            );
            fp_slider(
                ui,
                "Ambient brightness",
                &mut env.ambient_brightness,
                0.0,
                2_000.0,
                dirty,
            );
        });

    egui::CollapsingHeader::new("Clouds")
        .default_open(false)
        .show(ui, |ui| {
            fp_slider(ui, "Cover", &mut env.cloud_cover, 0.0, 1.0, dirty);
            fp_slider(ui, "Density", &mut env.cloud_density, 0.0, 1.0, dirty);
            fp_slider(
                ui,
                "Edge softness",
                &mut env.cloud_softness,
                0.001,
                1.0,
                dirty,
            );
            fp_slider(
                ui,
                "Drift speed (m/s)",
                &mut env.cloud_speed,
                0.0,
                50.0,
                dirty,
            );
            fp_slider(
                ui,
                "Feature scale (m)",
                &mut env.cloud_scale,
                10.0,
                2_000.0,
                dirty,
            );
            fp_slider(
                ui,
                "Altitude (m)",
                &mut env.cloud_height,
                10.0,
                2_000.0,
                dirty,
            );
            color_picker(ui, "Sunlit colour", &mut env.cloud_color, dirty);
            color_picker(ui, "Shadow colour", &mut env.cloud_shadow_color, dirty);

            ui.label(
                egui::RichText::new("Wind direction (XZ)")
                    .small()
                    .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
            let mut wind = env.cloud_wind_dir.0;
            ui.horizontal(|ui| {
                if ui
                    .add(
                        crate::ui::num::drag(&mut wind[0])
                            .speed(0.05)
                            .range(-10.0..=10.0),
                    )
                    .changed()
                {
                    *dirty = true;
                }
                if ui
                    .add(
                        crate::ui::num::drag(&mut wind[1])
                            .speed(0.05)
                            .range(-10.0..=10.0),
                    )
                    .changed()
                {
                    *dirty = true;
                }
            });
            env.cloud_wind_dir = Fp2(wind);

            ui.label(
                egui::RichText::new(
                    "Cloud-deck dissolves into the distance-fog colour at the horizon, \
                     so adjust Distance Fog › Visibility for a tighter or wider band.",
                )
                .small()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
        });

    egui::CollapsingHeader::new("Distance fog")
        .default_open(false)
        .show(ui, |ui| {
            // #1268 f66. These six were the app's most jargon-dense
            // labels and the only surface an owner has for learning what
            // they control - and until `color_picker` returned a
            // `Response` (this issue) four of them COULD not carry a
            // hint at all. Extinction and inscattering are the two halves
            // of the same physical model and neither name says so.
            fp_slider(
                ui,
                "Visibility (m)",
                &mut env.fog_visibility,
                50.0,
                2_000.0,
                dirty,
            )
            .on_hover_text(
                "Roughly how far you can see before the fog closes in. Lower is \
                 hazier; the cloud deck dissolves at this distance too.",
            );
            color_picker_rgba(ui, "Fog colour", &mut env.fog_color, dirty).on_hover_text(
                "The colour distant things fade towards. Alpha is how strongly the \
                 fog takes over at full distance.",
            );
            color_picker(ui, "Extinction", &mut env.fog_extinction, dirty).on_hover_text(
                "Which colours the air SWALLOWS with distance, per channel. Lower a \
                 channel and that colour survives further - a low blue gives warm, \
                 dusty air.",
            );
            color_picker(ui, "Inscattering", &mut env.fog_inscattering, dirty).on_hover_text(
                "Which colours the air ADDS back with distance, per channel - the \
                 light bouncing around in it. Raise blue for the usual hazy-blue \
                 horizon.",
            );
            color_picker_rgba(ui, "Sun glow", &mut env.fog_sun_color, dirty).on_hover_text(
                "The colour of the halo the fog picks up when you look towards the \
                 sun.",
            );
            fp_slider(
                ui,
                "Sun glow exponent",
                &mut env.fog_sun_exponent,
                0.0,
                200.0,
                dirty,
            )
            .on_hover_text(
                "How tightly that halo hugs the sun. Low spreads it across the whole \
                 sky; high keeps it to a small disc.",
            );
        });

    egui::CollapsingHeader::new("Water (room-wide)")
        .default_open(false)
        .show(ui, |ui| {
            fp_slider(
                ui,
                "Detail normal - near tile",
                &mut env.water_normal_scale_near,
                0.0,
                4.0,
                dirty,
            );
            fp_slider(
                ui,
                "Detail normal - far tile",
                &mut env.water_normal_scale_far,
                0.0,
                1.0,
                dirty,
            );
            ui.label(
                egui::RichText::new(
                    "Near + far tiles blend by distance so the repeating-grid look \
                     disappears on long sightlines.",
                )
                .small()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
            ui.add_space(4.0);
            fp_slider(
                ui,
                "Sun glitter",
                &mut env.water_sun_glitter,
                0.0,
                8.0,
                dirty,
            );
            color_picker(
                ui,
                "Crest scatter tint",
                &mut env.water_scatter_color,
                dirty,
            );
            ui.add_space(4.0);
            fp_slider(
                ui,
                "Shoreline foam width (m)",
                &mut env.water_shore_foam_width,
                0.0,
                8.0,
                dirty,
            );
            ui.label(
                egui::RichText::new(
                    "Shoreline foam fades in where water meets terrain, over \
                     this many metres of water depth. 0 disables it.",
                )
                .small()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
        });

    egui::CollapsingHeader::new("Ambient audio")
        .default_open(false)
        .show(ui, |ui| {
            super::audio::draw_audio_bridge(
                ui,
                &mut env.ambient_audio,
                super::audio_slots::ENVIRONMENT_SALT,
                "World ambient",
                super::audio::AudioSlotKind::WorldAmbient,
                dirty,
                audio_editor,
                assets,
            );
        });
}

/// The arrival-point editor (#773). Toggling it off clears the pose back
/// to `None` (legacy origin scatter); toggling on seeds it from the
/// player's current pose when available so the common "land people where
/// I'm standing" flow is one click. Height defaults to drop-pin (follows
/// the terrain at X/Z) so the pose survives later terrain edits.
fn draw_arrival_point(
    ui: &mut egui::Ui,
    landing: &mut Option<DefaultLanding>,
    player_pose: Option<PlayerPose>,
    dirty: &mut bool,
) {
    egui::CollapsingHeader::new("Arrival point")
        .default_open(false)
        .show(ui, |ui| {
            let mut enabled = landing.is_some();
            if ui
                .checkbox(&mut enabled, "Set a custom arrival point")
                .on_hover_text(
                    "Where visitors come to rest when they enter without a specific \
                     destination link - including through another world's gateway. \
                     Off: they scatter near the world origin.",
                )
                .changed()
            {
                *landing = enabled.then(|| match player_pose {
                    Some(p) => DefaultLanding {
                        pos: Fp2([p.x, p.z]),
                        y: None,
                        yaw_deg: Fp(p.yaw_deg),
                    },
                    None => DefaultLanding::default(),
                });
                *dirty = true;
            }

            let Some(l) = landing.as_mut() else {
                return;
            };

            if let Some(p) = player_pose
                && ui
                    .button("⟲ Set to my position & facing")
                    .on_hover_text("Stamp your avatar's current spot and heading.")
                    .clicked()
            {
                l.pos = Fp2([p.x, p.z]);
                l.yaw_deg = Fp(p.yaw_deg);
                // Only overwrite the height when the owner is pinning one;
                // a drop-pin pose keeps following the terrain.
                if l.y.is_some() {
                    l.y = Some(Fp(p.y));
                }
                *dirty = true;
            }

            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label("X");
                if ui
                    .add(crate::ui::num::drag(&mut l.pos.0[0]).speed(0.25))
                    .changed()
                {
                    *dirty = true;
                }
                ui.label("Z");
                if ui
                    .add(crate::ui::num::drag(&mut l.pos.0[1]).speed(0.25))
                    .changed()
                {
                    *dirty = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Facing (°)");
                if ui
                    .add(
                        crate::ui::num::drag(&mut l.yaw_deg.0)
                            .speed(1.0)
                            .range(0.0..=360.0),
                    )
                    .on_hover_text(
                        "0° faces −Z, 90° faces −X: the facing turns counter-clockwise seen \
                         from above.",
                    )
                    .changed()
                {
                    *dirty = true;
                }
            });

            let mut pin = l.y.is_some();
            if ui
                .checkbox(&mut pin, "Pin exact height")
                .on_hover_text(
                    "Off: height follows the terrain at (X, Z) - best for ground-level \
                     spots. On: use a fixed Y, for a platform or rooftop.",
                )
                .changed()
            {
                l.y = pin.then(|| Fp(player_pose.map(|p| p.y).unwrap_or(0.0)));
                *dirty = true;
            }
            if let Some(y) = l.y.as_mut() {
                ui.horizontal(|ui| {
                    ui.label("Y");
                    if ui.add(crate::ui::num::drag(&mut y.0).speed(0.25)).changed() {
                        *dirty = true;
                    }
                });
            }
        });
}

/// The body of the Region source section: build this region from a square
/// of real Berlin, at real scale, or from its seed as before.
///
/// Every square it writes lies wholly inside Berlin: a drawn one by
/// construction, an edited one moved by [`Coverage::nearest`] to the
/// closest place it fits - the same rule the record sanitiser applies, so
/// an edit is never rewritten under the owner on the next round trip.
fn draw_region_source(
    ui: &mut egui::Ui,
    source: &mut Option<GeoSource>,
    ground: BuiltGround<'_>,
    dirty: &mut bool,
) {
    let weak = crate::ui::theme::current(ui.ctx()).text_weak;
    // A dataset a newer version wrote: shown, kept, and never edited here.
    if let Some(other) = source.as_ref().filter(|s| s.dataset != BERLIN) {
        ui.label(format!(
            "Built from the \"{}\" dataset, which this version cannot draw: \
             visitors on it see the world drawn from its seed.",
            other.dataset
        ));
        if ui
            .button("Use the world drawn from its seed")
            .on_hover_text("Forget the other dataset's square.")
            .clicked()
        {
            *source = None;
            *dirty = true;
        }
        return;
    }

    let mut berlin = source.is_some();
    if ui
        .checkbox(&mut berlin, "Build from real Berlin")
        .on_hover_text(
            "Build the region's ground from a square of real Berlin, at real \
             scale and real altitude: the city's own terrain, land use and \
             water, centred on this square, under everything else in the \
             world. Off: the ground is drawn from the world's terrain \
             settings.",
        )
        .changed()
    {
        *source = berlin.then(|| GeoSource::berlin(drawn_square(fresh_seed())));
        *dirty = true;
    }
    let Some(current) = source.as_mut() else {
        return;
    };
    let square = current.square();

    let (centre_e, centre_n) = square.centre();
    let borough = Coverage::berlin()
        .borough_at(centre_e, centre_n)
        .map_or("Berlin", |b| b.name());
    ui.label(format!(
        "A {} square in {borough}",
        side_text(square.size_m)
    ));
    ui.label(
        egui::RichText::new(format!(
            "E {} to {}, N {} to {} (ETRS89 / UTM 33N)",
            square.min_e,
            square.max_e(),
            square.min_n,
            square.max_n()
        ))
        .small()
        .color(weak),
    );

    if ui
        .button("Draw another square")
        .on_hover_text("A new square of a new size, anywhere it fits wholly inside Berlin.")
        .clicked()
    {
        *current = GeoSource::berlin(drawn_square(fresh_seed()));
        *dirty = true;
    }

    // Typed values apply when typing ends (Enter, or leaving the field):
    // applied per keystroke, the first digit of a new easting is far off
    // the map, and every keystroke after it would start from where that one
    // was put. Dragging still applies as it goes.
    let mut side = square.size_m;
    ui.horizontal(|ui| {
        ui.label("Side");
        if ui
            .add(
                crate::ui::num::drag(&mut side)
                    .range(SIZE_MIN_M..=SIZE_MAX_M)
                    .speed(f64::from(SIZE_STEP_M))
                    .suffix(" m")
                    .update_while_editing(false),
            )
            .on_hover_text(
                "The region's extent, in steps of 10 m. It keeps its centre where the new \
                 size fits.",
            )
            .changed()
            && let Some(resized) = resized_keeping_centre(square, side)
            && resized != square
        {
            *current = GeoSource::berlin(resized);
            *dirty = true;
        }
    });

    let (mut west, mut south) = (square.min_e, square.min_n);
    ui.horizontal(|ui| {
        ui.label("West edge");
        let moved_e = ui
            .add(
                crate::ui::num::drag(&mut west)
                    .speed(10.0)
                    .update_while_editing(false),
            )
            .on_hover_text("Easting of the west edge, metres. The south edge stays where it is.")
            .changed();
        ui.label("South edge");
        let moved_n = ui
            .add(
                crate::ui::num::drag(&mut south)
                    .speed(10.0)
                    .update_while_editing(false),
            )
            .on_hover_text("Northing of the south edge, metres. The west edge stays where it is.")
            .changed();
        let moved = if moved_e {
            moved_west(square, west)
        } else if moved_n {
            moved_south(square, south)
        } else {
            None
        };
        if let Some(moved) = moved.filter(|m| *m != square) {
            *current = GeoSource::berlin(moved);
            *dirty = true;
        }
    });

    // What Berlin makes of the ground (#1586): the land use paints it and
    // zones the scatters, and the water sets the world's.
    ui.label(
        egui::RichText::new(
            "Berlin's land use paints the ground with the world's own layers: parks and \
             woods on the first, built-up blocks and bare earth on the second, streets and \
             squares on the third. Seeded trees and rocks keep to its open, natural ground. \
             Its rivers and lakes set the water: the world's water is drawn at their level.",
        )
        .small()
        .color(weak),
    );
    // The far field (#1585).
    ui.label(
        egui::RichText::new(
            "A square wider than the walkable ground is drawn on to its edge as the \
             horizon: coarser, not walkable - invisible walls stand at the walkable ground's \
             edge - and the world's fog opens at least far enough to show it.",
        )
        .small()
        .color(weak),
    );
    if let Some(berlin) = ground.berlin {
        ui.label(match (berlin.water_level(), ground.has_water) {
            (Some(level), true) => format!("Water at {level:.1} m above sea level, from Berlin."),
            (Some(level), false) => format!(
                "Berlin's water lies at {level:.1} m, but this world has no water to draw \
                 there: its beds lie dry."
            ),
            (None, _) => "No water is mapped in this square.".to_owned(),
        });
    }

    ui.label(
        egui::RichText::new("Map data: Geoportal Berlin, dl-de/zero-2.0")
            .small()
            .color(weak),
    );
}

/// "250 m", "1.25 km".
fn side_text(size_m: u32) -> String {
    if size_m < 1_000 {
        format!("{size_m} m")
    } else {
        format!("{:.2} km", f64::from(size_m) / 1_000.0)
    }
}

/// A square drawn as a seeded region draws one, from `seed` (#1589).
fn drawn_square(seed: u64) -> GeoSquare {
    crate::seeded_defaults::RegionSource::for_seed(seed).square()
}

/// A seed for an owner's "draw another": the clock and a counter, so two
/// clicks in one millisecond still differ. Not a seeded region's draw - the
/// square it makes is stored in the record.
fn fresh_seed() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CLICKS: AtomicU64 = AtomicU64::new(0);
    let micros = chrono::Utc::now().timestamp_micros() as u64;
    micros
        ^ CLICKS
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

/// `square` at side `side` (made drawable: a whole 10 m in range), centred
/// where it was if it fits there, else as near as it fits.
fn resized_keeping_centre(square: GeoSquare, side: u32) -> Option<GeoSquare> {
    let side = geodata::square::snap_size(side);
    let (centre_e, centre_n) = square.centre();
    let half = f64::from(side) / 2.0;
    Coverage::berlin().nearest(
        side,
        (centre_e - half).round() as i64,
        (centre_n - half).round() as i64,
    )
}

/// `square` with its west edge at `west` - the south edge held where it
/// is, the west edge as near `west` as the square fits along that row. Only
/// if the square fits nowhere on its row does the south edge move too.
fn moved_west(square: GeoSquare, west: i32) -> Option<GeoSquare> {
    let (e, n) = (i64::from(west), i64::from(square.min_n));
    let coverage = Coverage::berlin();
    coverage
        .nearest_keeping(square.size_m, e, n, Keep::Northing)
        .or_else(|| coverage.nearest(square.size_m, e, n))
}

/// `square` with its south edge at `south`, the west edge held - as
/// [`moved_west`], the other way.
fn moved_south(square: GeoSquare, south: i32) -> Option<GeoSquare> {
    let (e, n) = (i64::from(square.min_e), i64::from(south));
    let coverage = Coverage::berlin();
    coverage
        .nearest_keeping(square.size_m, e, n, Keep::Easting)
        .or_else(|| coverage.nearest(square.size_m, e, n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_1_SQRT_2;

    #[test]
    fn region_squares_drawn_resized_or_moved_lie_inside_berlin() {
        let coverage = Coverage::berlin();
        for seed in 0..32 {
            let square = drawn_square(seed);
            assert!(coverage.contains(&square), "seed {seed}: {square:?}");
            for side in [SIZE_MIN_M, 1_000, SIZE_MAX_M] {
                let resized = resized_keeping_centre(square, side).unwrap();
                assert!(coverage.contains(&resized) && resized.size_m == side);
            }
        }
        // Resizing in place keeps the centre when the new size fits there.
        let dom = GeoSquare {
            min_e: 391_000,
            min_n: 5_819_500,
            size_m: 1_000,
        };
        let smaller = resized_keeping_centre(dom, 500).unwrap();
        assert_eq!(smaller.centre(), dom.centre());
        // A side off the 10 m step is snapped onto it.
        assert_eq!(resized_keeping_centre(dom, 1_003).unwrap().size_m, 1_000);
        // An edge typed off the map lands on it, the other edge held.
        let moved = moved_west(dom, 0).unwrap();
        assert!(coverage.contains(&moved));
        assert_eq!(moved.min_n, dom.min_n);
        assert_eq!(moved_west(dom, 391_000), Some(dom));
        assert_eq!(moved_south(dom, 5_819_500), Some(dom));
    }

    /// The edits the owner makes one edge at a time leave the other edge
    /// where it was: a west edge dragged 30 km east (off the map) and back
    /// returns the square to where it started, and so does a south edge.
    #[test]
    fn an_edge_dragged_off_the_map_and_back_leaves_the_square_where_it_was() {
        let dom = GeoSquare {
            min_e: 391_000,
            min_n: 5_819_500,
            size_m: 1_000,
        };
        // Out 30 km in kilometre steps, and back the same way.
        let there_and_back =
            |from: i32| (0..=30).chain((0..30).rev()).map(move |k| from + 1_000 * k);
        let mut square = dom;
        for west in there_and_back(dom.min_e) {
            square = moved_west(square, west).unwrap();
            assert_eq!(
                square.min_n, dom.min_n,
                "the south edge held at west {west}"
            );
        }
        assert_eq!(square, dom);
        for south in there_and_back(dom.min_n) {
            square = moved_south(square, south).unwrap();
            assert_eq!(
                square.min_e, dom.min_e,
                "the west edge held at south {south}"
            );
        }
        assert_eq!(square, dom);
    }

    #[test]
    fn side_text_reads_metres_then_kilometres() {
        assert_eq!(side_text(250), "250 m");
        assert_eq!(side_text(1_000), "1.00 km");
        assert_eq!(side_text(19_000), "19.00 km");
    }

    /// Drawn with no input, the section writes nothing - not the source,
    /// not the dirty flag - whatever the source is (#1390's rule for every
    /// editor widget: a value the owner did not touch is not rewritten).
    #[test]
    fn an_untouched_region_source_writes_nothing() {
        let berlin = Some(GeoSource::berlin(GeoSquare {
            min_e: 391_000,
            min_n: 5_819_500,
            size_m: 1_000,
        }));
        let other = Some(GeoSource {
            dataset: "hamburg".into(),
            min_e: 1,
            min_n: 2,
            size_m: 3,
        });
        // Berlin's ground as built (#1586) is read, never written.
        let wet = crate::terrain::geo::GeoGround::from_cover(2, 2.0, vec![None; 4], Some(30.5));
        let dry = crate::terrain::geo::GeoGround::from_cover(2, 2.0, vec![None; 4], None);
        let grounds = [None, Some(&wet), Some(&dry)]
            .into_iter()
            .flat_map(|berlin| [true, false].map(|has_water| BuiltGround { berlin, has_water }));
        for start in [None, berlin, other] {
            for ground in grounds.clone() {
                let ctx = egui::Context::default();
                let mut source = start.clone();
                let mut dirty = false;
                let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                    draw_region_source(ui, &mut source, ground, &mut dirty);
                });
                assert_eq!(source, start);
                assert!(!dirty);
            }
        }
    }

    /// The pose yaw must invert the spawn path's
    /// `Quat::from_rotation_y(yaw_deg.to_radians())` for every heading, so a
    /// captured landing faces exactly where the owner was looking.
    #[test]
    fn yaw_extraction_inverts_spawn_rotation() {
        for deg in [0.0_f32, 30.0, 45.0, 90.0, 135.0, 180.0, 225.0, 359.0] {
            let tf = Transform::from_rotation(Quat::from_rotation_y(deg.to_radians()));
            let pose = PlayerPose::from_transform(&tf);
            let mut diff = (pose.yaw_deg - deg).rem_euclid(360.0);
            diff = diff.min(360.0 - diff);
            assert!(diff < 0.01, "deg {deg} round-tripped to {}", pose.yaw_deg);
        }
    }

    /// A surface tilt (chassis rested on a slope) must not corrupt the yaw:
    /// the XZ projection discards the pitch the normal-alignment adds.
    #[test]
    fn yaw_survives_surface_tilt() {
        let tilt = Quat::from_rotation_arc(Vec3::Y, Vec3::new(0.3, 1.0, 0.0).normalize());
        let yaw = Quat::from_rotation_y(90.0_f32.to_radians());
        let tf = Transform::from_rotation(tilt * yaw);
        let pose = PlayerPose::from_transform(&tf);
        let mut diff = (pose.yaw_deg - 90.0).rem_euclid(360.0);
        diff = diff.min(360.0 - diff);
        assert!(diff < 1.0, "tilted yaw drifted to {}", pose.yaw_deg);
    }

    #[test]
    fn position_is_copied_verbatim() {
        let tf =
            Transform::from_xyz(3.0, 5.0, -7.0).with_rotation(Quat::from_rotation_y(FRAC_1_SQRT_2));
        let pose = PlayerPose::from_transform(&tf);
        assert_eq!((pose.x, pose.y, pose.z), (3.0, 5.0, -7.0));
    }
}
