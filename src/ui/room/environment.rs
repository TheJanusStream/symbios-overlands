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
//! square of real Berlin ([`crate::pds::GeoSource`]) or from its seed alone,
//! and the owner's edits over Berlin's items there (#1590), each with
//! Restore.

use bevy::prelude::*;
use bevy_egui::egui;
use geodata::GeoSquare;
use geodata::berlin::Coverage;
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
    restore: &mut Option<String>,
    audio_editor: &mut super::audio::AudioEditorState,
    assets: &mut super::assets::AssetPanel<'_>,
) {
    ui.heading("Environment");
    ui.add_space(4.0);

    egui::CollapsingHeader::new("Region source")
        .default_open(false)
        .show(ui, |ui| {
            draw_region_source(ui, geo_source, ground, dirty, restore)
        });
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
/// of real Berlin, at real scale, or from its seed as before. The square is
/// set by its side and by its middle's latitude and longitude (#1599),
/// which anyone can read off a web map.
///
/// Every square it writes lies wholly inside Berlin: a drawn one by
/// construction, an edited one moved by [`Coverage::nearest`] to the
/// closest place it fits - the same rule the record sanitiser applies, so
/// an edit is never rewritten under the owner on the next round trip. A
/// moved square keeps the owner's edits over Berlin's items (#1590), which
/// name their items wherever the square lies.
///
/// The edits are listed, each with Restore, which it asks for through
/// `restore`: an adopted item's copy is record content beyond the source,
/// for the caller to take away with it.
fn draw_region_source(
    ui: &mut egui::Ui,
    source: &mut Option<GeoSource>,
    ground: BuiltGround<'_>,
    dirty: &mut bool,
    restore: &mut Option<String>,
) {
    let weak = crate::ui::theme::current(ui.ctx()).text_weak;
    // A dataset a newer version wrote: shown, kept, and never edited here.
    if let Some(other) = source.as_ref().filter(|s| s.dataset != BERLIN) {
        ui.label(format!(
            "Dataset \"{}\" needs a newer version.",
            other.dataset
        ));
        if ui
            .button("Build from the seed instead")
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
        .on_hover_text("Terrain, land use and water from a square of Berlin, at real scale.")
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
    ui.label(format!("{} square in {borough}", side_text(square.size_m)))
        .on_hover_text(format!(
            "E {} to {}, N {} to {} (ETRS89 / UTM 33N)",
            square.min_e,
            square.max_e(),
            square.min_n,
            square.max_n()
        ));
    if ui
        .button("Draw another square")
        .on_hover_text("A random size, anywhere inside Berlin.")
        .clicked()
    {
        *current = current.moved_to(drawn_square(fresh_seed()));
        *dirty = true;
    }

    // Typed values apply when typing ends (Enter, or leaving the field):
    // applied per keystroke, the first digit of a new latitude is far off
    // the map, and every keystroke after it would start from where that one
    // was put. Dragging still applies as it goes.
    let memory = ui.make_persistent_id("region-middle");
    let asked = ui.data(|d| d.get_temp::<AskedMiddle>(memory));
    let (lat, lon) = middle_shown(square, asked);
    let (mut side, mut new_lat, mut new_lon) = (square.size_m, lat, lon);
    let (mut resized, mut lat_edited, mut lon_edited) = (false, false, false);
    egui::Grid::new("region-square")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("Side");
            resized = ui
                .add(
                    crate::ui::num::drag(&mut side)
                        .range(SIZE_MIN_M..=SIZE_MAX_M)
                        .speed(f64::from(SIZE_STEP_M))
                        .suffix(" m")
                        .update_while_editing(false),
                )
                .on_hover_text("Steps of 10 m; the middle stays.")
                .changed();
            ui.end_row();
            lat_edited = degrees_field(ui, "Latitude", &mut new_lat, lat, "\u{b0} N");
            lon_edited = degrees_field(ui, "Longitude", &mut new_lon, lon, "\u{b0} E");
        });
    let edited = if resized {
        resized_keeping_centre(square, side)
    } else if lat_edited || lon_edited {
        let moved = moved_middle(square, (new_lat, new_lon), lat_edited);
        if let Some((_, asked)) = moved {
            ui.data_mut(|d| d.insert_temp(memory, asked));
        }
        moved.map(|(square, _)| square)
    } else {
        None
    };
    if let Some(edited) = edited.filter(|m| *m != square) {
        *current = current.moved_to(edited);
        *dirty = true;
    }

    // What Berlin makes of the world's water (#1586), and its data moving
    // on since the last save (#1590).
    if let Some(berlin) = ground.berlin {
        ui.label(match (berlin.water_level(), ground.has_water) {
            (Some(level), true) => format!("Water at {level:.1} m above sea level"),
            (Some(level), false) => {
                format!("Water at {level:.1} m, not drawn: this world has none")
            }
            (None, _) => "No water in this square".to_owned(),
        });
        if let Some(changed) = berlin
            .layers()
            .and_then(crate::terrain::geo::layers::DrawnLayers::changed_sentence)
        {
            ui.label(egui::RichText::new(changed).small().color(weak));
        }
    }

    draw_berlin_edits(
        ui,
        current,
        ground
            .berlin
            .and_then(crate::terrain::geo::GeoGround::street_level)
            .map(|level| &**level),
        restore,
    );

    ui.label(
        egui::RichText::new("Map data: Geoportal Berlin, dl-de/zero-2.0")
            .small()
            .color(weak),
    );
}

/// How far a drag of one pixel moves the square's middle (degrees): about
/// 11 m of latitude and 7 m of longitude, in Berlin.
const DEGREES_PER_DRAG: f64 = 0.0001;

/// The decimals a latitude or longitude is shown with: a tenth of a metre
/// or finer, so the number shown, written back, lands on the same metre.
const DEGREE_DECIMALS: usize = 6;

/// One coordinate of the square's middle in the Region source grid,
/// labelled `label`: `value`, shown as `shown`, edited in degrees, `suffix`
/// after it. Whether the owner changed it: a commit of the number it shows,
/// by a click in and out (which egui writes back) or by typing that number,
/// is no change.
fn degrees_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    shown: f64,
    suffix: &str,
) -> bool {
    ui.label(label);
    let changed = ui
        .add(
            crate::ui::num::drag(value)
                .custom_parser(degrees)
                .speed(DEGREES_PER_DRAG)
                .fixed_decimals(DEGREE_DECIMALS)
                .suffix(suffix)
                .update_while_editing(false),
        )
        .on_hover_text("Of the square's middle.")
        .changed();
    ui.end_row();
    changed && !same_shown(*value, shown)
}

/// Whether `value` is what a field showing `shown` reads as, to its
/// [`DEGREE_DECIMALS`].
fn same_shown(value: f64, shown: f64) -> bool {
    let scale = 10f64.powi(DEGREE_DECIMALS as i32);
    (value - (shown * scale).round() / scale).abs() < 1e-9
}

/// A typed latitude or longitude: degrees, a degree sign and an `N` or `E`
/// allowed after it, and a lone comma read as the decimal comma - no
/// coordinate has a thousands group, and the locale reader would take
/// `52,520` for fifty-two thousand.
fn degrees(text: &str) -> Option<f64> {
    let number = text
        .trim()
        .trim_end_matches(['N', 'n', 'E', 'e'])
        .trim_end()
        .trim_end_matches('\u{b0}')
        .trim();
    crate::ui::num::locale_number(&number.replacen(',', ".", 1))
}

/// What the owner last asked of the square's middle (#1599), and the
/// square that answer produced. While the square is still that one, the
/// fields show what was asked rather than a reading off a square rounded to
/// whole metres or moved to fit the map: a slow drag of one coordinate
/// leaves the other where the owner put it, and a latitude dragged off the
/// map and back brings the square back.
#[derive(Clone, Copy, Debug, PartialEq)]
struct AskedMiddle {
    lat: f64,
    lon: f64,
    square: GeoSquare,
}

/// The middle the fields show for `square`: the owner's, where `asked`
/// produced `square`, else `square`'s own.
fn middle_shown(square: GeoSquare, asked: Option<AskedMiddle>) -> (f64, f64) {
    match asked {
        Some(asked) if asked.square == square => (asked.lat, asked.lon),
        _ => {
            let (e, n) = square.centre();
            geodata::latlon::to_lat_lon(e, n)
        }
    }
}

/// `square` moved to the middle `(lat, lon)` (see [`centred_at`]), and what
/// to remember of the asking: the middle as asked where the square sits on
/// it, else - moved to fit the map - the edited coordinate as the square
/// took it (`lat_edited`, else the longitude) and the other as asked.
fn moved_middle(
    square: GeoSquare,
    (lat, lon): (f64, f64),
    lat_edited: bool,
) -> Option<(GeoSquare, AskedMiddle)> {
    let moved = centred_at(square, lat, lon)?;
    let (e, n) = moved.centre();
    let (asked_e, asked_n) = grid_of(lat, lon)?;
    let asked = if (asked_e - e).abs() <= 1.0 && (asked_n - n).abs() <= 1.0 {
        (lat, lon)
    } else {
        let (took_lat, took_lon) = geodata::latlon::to_lat_lon(e, n);
        if lat_edited {
            (took_lat, lon)
        } else {
            (lat, took_lon)
        }
    };
    Some((
        moved,
        AskedMiddle {
            lat: asked.0,
            lon: asked.1,
            square: moved,
        },
    ))
}

/// The owner's edits over Berlin's items (#1590): how many, then every item
/// removed or made the world's own, by what Berlin records of it on the
/// walkable ground `level`, each with Restore - asked for through
/// `restore`.
fn draw_berlin_edits(
    ui: &mut egui::Ui,
    source: &GeoSource,
    level: Option<&crate::terrain::geo::street_level::StreetLevel>,
    restore: &mut Option<String>,
) {
    let weak = crate::ui::theme::current(ui.ctx()).text_weak;
    let edits = source.removed.len() + source.adopted.len();
    ui.add_space(4.0);
    let count = if edits == 0 {
        "Changes to Berlin's items: none".to_owned()
    } else {
        format!("Changes to Berlin's items: {edits}")
    };
    ui.label(egui::RichText::new(count).small().color(weak))
        .on_hover_text(
            "Click a building, tree or street item in the world to remove it or make it \
             this world's own.",
        );
    if edits == 0 {
        return;
    }
    let rows = source
        .adopted
        .iter()
        .map(|id| (id, true))
        .chain(source.removed.iter().map(|id| (id, false)));
    egui::ScrollArea::vertical()
        .id_salt("berlin-edits")
        .max_height(180.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for (id, adopted) in rows {
                ui.horizontal_wrapped(|ui| {
                    let hover = if adopted {
                        "Draw it from Berlin again, and take this world's copy of it away."
                    } else {
                        "Draw it from Berlin again."
                    };
                    if ui.small_button("Restore").on_hover_text(hover).clicked() {
                        *restore = Some(id.clone());
                    }
                    let named = crate::terrain::derived::SourceId::parse(id).map_or_else(
                        || "An item this version does not draw".to_owned(),
                        |id| crate::terrain::derived::edit::describe(level, &id),
                    );
                    let state = if adopted {
                        "made this world's own"
                    } else {
                        "removed"
                    };
                    ui.label(format!("{named}: {state}"))
                        .on_hover_text(id.as_str());
                });
            }
        });
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

/// The grid point of `(lat, lon)` (degrees), brought within the band the
/// projection holds in: `None` for a coordinate that is no number.
fn grid_of(lat: f64, lon: f64) -> Option<(f64, f64)> {
    (lat.is_finite() && lon.is_finite())
        .then(|| geodata::latlon::to_grid(lat.clamp(-80.0, 84.0), lon.clamp(-15.0, 45.0)))
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

/// `square` with its middle at latitude `lat` and longitude `lon`
/// (degrees, ETRS89), or as near there as it fits inside Berlin. A point
/// far off the map is first brought within the band its projection holds
/// in; the square then lands where it fits nearest.
fn centred_at(square: GeoSquare, lat: f64, lon: f64) -> Option<GeoSquare> {
    let (e, n) = grid_of(lat, lon)?;
    let half = f64::from(square.size_m) / 2.0;
    Coverage::berlin().nearest(
        square.size_m,
        (e - half).round() as i64,
        (n - half).round() as i64,
    )
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
        // A middle typed off the map - the equator, the poles, nonsense -
        // lands on it, or changes nothing.
        for (lat, lon) in [(0.0, 0.0), (90.0, 13.4), (52.5, 180.0), (-90.0, -180.0)] {
            let moved = centred_at(dom, lat, lon).unwrap();
            assert!(coverage.contains(&moved), "({lat}, {lon}): {moved:?}");
        }
        assert_eq!(centred_at(dom, f64::NAN, 13.4), None);
        // Its own middle leaves it where it is.
        let (e, n) = dom.centre();
        let (lat, lon) = geodata::latlon::to_lat_lon(e, n);
        assert_eq!(centred_at(dom, lat, lon), Some(dom));
    }

    /// A middle typed as latitude and longitude (#1599) puts the square's
    /// middle within a metre of it - the square sits on whole metres -
    /// wherever in Berlin it is typed, at any size that fits there.
    #[test]
    fn a_typed_middle_lands_within_a_metre_of_it() {
        let dom = GeoSquare {
            min_e: 391_000,
            min_n: 5_819_500,
            size_m: 1_000,
        };
        // The Brandenburg Gate, Hermannplatz, Spandau's old town, the
        // Mueggelsee's north shore.
        for (lat, lon) in [
            (52.516_275, 13.377_704),
            (52.487_2, 13.424_3),
            (52.536_0, 13.205_0),
            (52.445_0, 13.640_0),
        ] {
            let moved = centred_at(dom, lat, lon).unwrap();
            let (e, n) = moved.centre();
            let (got_lat, got_lon) = geodata::latlon::to_lat_lon(e, n);
            // A metre is about 9e-6 degrees of latitude and 1.5e-5 of
            // longitude here.
            assert!(
                (got_lat - lat).abs() < 9e-6 && (got_lon - lon).abs() < 1.5e-5,
                "({lat}, {lon}) put the middle at ({got_lat}, {got_lon})"
            );
        }
    }

    /// The panel's edit of the middle as a frame makes it: one field given
    /// `lat` or `lon` - a drag's value is egui's own running total from
    /// where the drag began, which the panel's writes do not touch - the
    /// other as shown, the square moved to fit and the asking remembered.
    fn frame(
        square: GeoSquare,
        asked: Option<AskedMiddle>,
        (lat, lon): (Option<f64>, Option<f64>),
    ) -> (GeoSquare, Option<AskedMiddle>) {
        let shown = middle_shown(square, asked);
        let middle = (lat.unwrap_or(shown.0), lon.unwrap_or(shown.1));
        match moved_middle(square, middle, lat.is_some()) {
            Some((moved, asked)) => (moved, Some(asked)),
            None => (square, asked),
        }
    }

    /// A latitude dragged off the map and back - or typed off it and back -
    /// returns the square to where it started (#1599, the critic's HIGH):
    /// the longitude shown is the one the owner left, not one read back off
    /// the square while it slid along the map's edge to fit.
    #[test]
    fn a_latitude_dragged_off_the_map_and_back_leaves_the_square_where_it_was() {
        let start = GeoSquare {
            min_e: 387_500,
            min_n: 5_830_000,
            size_m: 1_000,
        };
        let (lat, lon) = middle_shown(start, None);
        let (mut square, mut asked) = (start, None);
        // A tenth of a degree north, past the map's edge, and back.
        for k in (1..=100).chain((0..100).rev()) {
            (square, asked) = frame(square, asked, (Some(lat + 0.001 * f64::from(k)), None));
            assert!(Coverage::berlin().contains(&square));
            assert_eq!(middle_shown(square, asked).1, lon, "the longitude held");
        }
        assert_eq!(square, start);
        // Typed far off the map, then typed back.
        let (far, asked) = moved_middle(start, (60.0, lon), true).unwrap();
        assert_ne!(far, start);
        let (back, _) = moved_middle(far, (lat, middle_shown(far, Some(asked)).1), true).unwrap();
        assert_eq!(back, start);
    }

    /// A slow drag of one coordinate - a pixel a frame - leaves the other
    /// where it was (the critic's MEDIUM): read back off each whole-metre
    /// square, it drifted 70 m along the grid in 300 frames.
    #[test]
    fn a_slow_drag_of_one_coordinate_leaves_the_other() {
        let start = GeoSquare {
            min_e: 391_000,
            min_n: 5_814_500,
            size_m: 500,
        };
        let (lat0, lon0) = middle_shown(start, None);
        let step = |k: u32| DEGREES_PER_DRAG * f64::from(k);
        // The latitude dragged north.
        let (mut square, mut asked) = (start, None);
        for k in 1..=300 {
            (square, asked) = frame(square, asked, (Some(lat0 + step(k)), None));
        }
        let (e, n) = square.centre();
        assert_eq!(middle_shown(square, asked), (lat0 + step(300), lon0));
        let on = geodata::latlon::to_lat_lon(e, n).1;
        assert!((on - lon0).abs() < 1.5e-5, "the square stayed on it: {on}");
        // The longitude dragged east.
        let (mut square, mut asked) = (start, None);
        for k in 1..=300 {
            (square, asked) = frame(square, asked, (None, Some(lon0 + step(k))));
        }
        let (e, n) = square.centre();
        assert_eq!(middle_shown(square, asked), (lat0, lon0 + step(300)));
        let on = geodata::latlon::to_lat_lon(e, n).0;
        assert!((on - lat0).abs() < 9e-6, "the square stayed on it: {on}");
    }

    /// The number a field shows, written back - egui writes it on a click
    /// in and out, and an owner may type it - lands on the same metre, so
    /// no square moves and the record stays clean (the critic's MEDIUM: at
    /// five decimals, one square in ten moved).
    #[test]
    fn the_number_shown_written_back_moves_nothing() {
        let scale = 10f64.powi(DEGREE_DECIMALS as i32);
        let round = |v: f64| (v * scale).round() / scale;
        for min_e in (370_000..410_000).step_by(997) {
            for min_n in (5_805_000..5_835_000).step_by(1_499) {
                let square = GeoSquare {
                    min_e,
                    min_n,
                    size_m: 500,
                };
                if !Coverage::berlin().contains(&square) {
                    continue;
                }
                let (lat, lon) = middle_shown(square, None);
                assert!(same_shown(round(lat), lat) && same_shown(round(lon), lon));
                assert_eq!(centred_at(square, round(lat), lon), Some(square));
                assert_eq!(centred_at(square, lat, round(lon)), Some(square));
            }
        }
        assert!(!same_shown(52.4872, 52.4871));
    }

    /// A latitude or longitude reads a lone comma as the decimal comma -
    /// the locale reader took `52,520` for fifty-two thousand five hundred
    /// and twenty, and put the square on the map's north edge - and takes
    /// the degree sign and the hemisphere a field shows.
    #[test]
    fn a_typed_coordinate_reads_its_comma_as_a_decimal_comma() {
        assert_eq!(degrees("52,520"), Some(52.52));
        assert_eq!(degrees("13,42480"), Some(13.4248));
        assert_eq!(degrees(" 52.487100\u{b0} N "), Some(52.4871));
        assert_eq!(degrees("13.4248 E"), Some(13.4248));
        assert_eq!(degrees("-52,5"), Some(-52.5));
        assert_eq!(degrees("Berlin"), None);
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
            ..Default::default()
        });
        // Berlin's ground as built (#1586) is read, never written.
        let wet = crate::terrain::geo::GeoGround::from_cover(2, 2.0, vec![None; 4], Some(30.5));
        let dry = crate::terrain::geo::GeoGround::from_cover(2, 2.0, vec![None; 4], None);
        // Drawn from layers Berlin has changed since the save (#1590).
        let changed = {
            use crate::terrain::geo::layers::{DrawnLayers, Layer};
            let square = GeoSquare {
                min_e: 391_000,
                min_n: 5_819_500,
                size_m: 1_000,
            };
            let hashes = std::collections::BTreeMap::from([(Layer::Trees, 2)]);
            let saved = std::collections::BTreeMap::from([(Layer::Trees, 1)]);
            dry.clone()
                .with_layers(DrawnLayers::new(square, hashes, &saved))
        };
        let grounds = [None, Some(&wet), Some(&dry), Some(&changed)]
            .into_iter()
            .flat_map(|berlin| [true, false].map(|has_water| BuiltGround { berlin, has_water }));
        let mut edited = berlin.clone();
        if let Some(source) = edited.as_mut() {
            source
                .set_edit("tree:1", crate::pds::geo_source::Edit::Removed)
                .unwrap();
            source
                .set_edit("alkis:A", crate::pds::geo_source::Edit::Adopted)
                .unwrap();
        }
        for start in [None, berlin, other, edited] {
            for ground in grounds.clone() {
                let ctx = egui::Context::default();
                let mut source = start.clone();
                let mut dirty = false;
                let mut restore = None;
                let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                    draw_region_source(ui, &mut source, ground, &mut dirty, &mut restore);
                });
                assert_eq!(source, start);
                assert!(!dirty);
                assert_eq!(restore, None);
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
