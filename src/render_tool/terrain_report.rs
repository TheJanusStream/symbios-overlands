//! `--terrain-report` (#1449): a world's ground as numbers, and a plan view
//! of it, without standing up a render app.
//!
//! Shaping a region means choosing a terrain recipe, a water level and where
//! things stand. `--world` shows how a world looks; this says what its ground
//! is: the heights the game stands things on - the same heightmap job, the
//! same world-to-cell mapping and the same footprint rule a snapped placement
//! rests on - how much of the map the water floods, the slope, the downhill
//! way and the contour at any point, and where every placement stands. It
//! prints one JSON object; `--plan` also draws the ground from above with the
//! landing, the placements and the points asked about on it.
//!
//! The plan is seen from above with +X to the right and +Z down the image.

use std::path::Path;

use bevy_symbios_ground::HeightMap;
use serde_json::{Value, json};

use crate::pds::{GeneratorKind, Placement, RoomRecord, ScatterBounds};
use crate::world_builder::compile::pad;
use crate::world_builder::compile::water::room_water_level;

/// What `--terrain-report` was asked.
pub(super) struct Request<'a> {
    pub record: RoomRecord,
    /// Points to read, world `(x, z)`.
    pub at: Vec<(f32, f32)>,
    /// A footprint radius: each point also says what a thing that wide rests
    /// on there, and how far the ground falls away under it.
    pub footprint: Option<f32>,
    /// Where to write the plan view, if anywhere.
    pub plan: Option<&'a Path>,
    /// The plan's centre, world `(x, z)` (default the map's centre).
    pub focus: Option<(f32, f32)>,
    /// How many metres the plan spans (default the whole map).
    pub span: Option<f32>,
    /// Scan these seeds of the record's terrain recipe instead (#1449).
    pub seed_scan: Option<std::ops::Range<u64>>,
}

/// The most seeds one scan reads - a few seconds' work, and a sheet that
/// still reads at a glance.
pub(super) const MAX_SCAN_SEEDS: u64 = 64;

/// Parse `--seed-scan`: `N` for seeds 0..N, or `A..B` (B not included).
pub(super) fn parse_seed_range(raw: &str) -> Result<std::ops::Range<u64>, String> {
    let range = match raw.trim().split_once("..") {
        Some((a, b)) => {
            let a: u64 = a
                .trim()
                .parse()
                .map_err(|_| format!("{raw:?}: {a:?} is not a seed"))?;
            let b: u64 = b
                .trim()
                .parse()
                .map_err(|_| format!("{raw:?}: {b:?} is not a seed"))?;
            a..b
        }
        None => {
            0..raw
                .trim()
                .parse()
                .map_err(|_| format!("{raw:?} is not a count or A..B"))?
        }
    };
    if range.is_empty() || range.end - range.start > MAX_SCAN_SEEDS {
        return Err(format!("{raw:?}: scan 1 to {MAX_SCAN_SEEDS} seeds"));
    }
    Ok(range)
}

/// Parse an `X,Z` pair as `--at` and `--focus` take it.
pub(super) fn parse_xz(raw: &str) -> Result<(f32, f32), String> {
    let mut parts = raw.split(',').map(str::trim);
    match (parts.next(), parts.next(), parts.next()) {
        (Some(x), Some(z), None) => match (x.parse(), z.parse()) {
            (Ok(x), Ok(z)) => Ok((x, z)),
            _ => Err(format!("{raw:?} is not two numbers X,Z")),
        },
        _ => Err(format!("{raw:?} is not X,Z")),
    }
}

/// The ground as the game reads it: world `(x, z)` shifted onto the map,
/// whose centre is the world's origin, and clamped at its edges.
struct Ground<'a> {
    map: &'a HeightMap,
    extent: f32,
}

impl<'a> Ground<'a> {
    fn new(map: &'a HeightMap) -> Self {
        let extent = (map.width().saturating_sub(1)) as f32 * map.scale();
        Self { map, extent }
    }

    fn cell(&self, x: f32, z: f32) -> (f32, f32) {
        let half = self.extent * 0.5;
        (
            (x + half).clamp(0.0, self.extent),
            (z + half).clamp(0.0, self.extent),
        )
    }

    fn height(&self, x: f32, z: f32) -> f32 {
        let (cx, cz) = self.cell(x, z);
        self.map.get_height_at(cx, cz)
    }

    fn normal(&self, x: f32, z: f32) -> [f32; 3] {
        let (cx, cz) = self.cell(x, z);
        self.map.get_normal_at(cx, cz)
    }
}

/// One point read: its height, how steep it is and which way is downhill,
/// and the `place --yaw` that lays a long thing's local +X along the contour
/// there, so neither end floats. With a footprint, what a thing that wide
/// rests on (the highest ground under it, the rule a snapped building uses)
/// and how far the ground falls away beneath it; under water, how deep.
fn point(ground: &Ground<'_>, x: f32, z: f32, footprint: Option<f32>, water: Option<f32>) -> Value {
    let height = ground.height(x, z);
    let [nx, ny, nz] = ground.normal(x, z);
    let slope_deg = ny.clamp(-1.0, 1.0).acos().to_degrees();
    // A heightfield's normal leans away from the uphill side, so its level
    // part points downhill; the gradient is the other way.
    let level = nx.hypot(nz);
    let flat = level < 1e-4;
    let downhill = (!flat).then(|| [round2(nx / level), round2(nz / level)]);
    // `place --yaw` turns local +X to world (cos yaw, sin yaw); the contour
    // runs square to the gradient (-nx, -nz).
    let contour_yaw = (!flat).then(|| {
        let gradient = (-nz).atan2(-nx).to_degrees();
        round2((gradient + 90.0).rem_euclid(360.0))
    });
    let mut out = json!({
        "x": round2(x),
        "z": round2(z),
        "ground_m": round2(height),
        "slope_deg": round2(slope_deg),
        "downhill": downhill,
        "contour_yaw_deg": contour_yaw,
    });
    if let Some(radius) = footprint.filter(|r| *r > 0.0) {
        let rests_on = pad::snapped_ground_y(ground.map, x, z, Some(radius));
        let lowest = lowest_under(ground, x, z, radius);
        out["footprint"] = json!({
            "radius_m": round2(radius),
            "rests_on_m": round2(rests_on),
            "lowest_m": round2(lowest),
            "drop_m": round2(rests_on - lowest),
        });
    }
    if let Some(level) = water.filter(|level| *level > height) {
        out["under_water_m"] = json!(round2(level - height));
    }
    out
}

/// The lowest ground under a disc, sampled a cell apart and round its rim.
/// The ground textures the game blends at a point (#1461): each of the four
/// splat layers' share of the blend - from the same mapper, rules and
/// heightmap the ground's weight map is generated from - and the layer a
/// scatter's `biome_filter` reads there, the largest share. Where no rule
/// matches, the mapper paints the third layer, and so does this.
struct Splat {
    mapper: bevy_symbios_ground::SplatMapper,
    textures: [&'static str; 4],
}

impl Splat {
    fn new(record: &RoomRecord) -> Self {
        let textures = crate::pds::find_terrain_config(record)
            .map(|c| c.material.layers.each_ref().map(|l| l.label()))
            .unwrap_or_else(|| {
                crate::pds::SovereignTerrainConfig::default()
                    .material
                    .layers
                    .each_ref()
                    .map(|l| l.label())
            });
        Self {
            mapper: crate::terrain::record_splat_mapper(Some(record)),
            textures,
        }
    }

    /// `layers` (the shares over 0.5 %, by layer) and `biome` for a point.
    fn read(&self, ground: &Ground<'_>, x: f32, z: f32, out: &mut Value) {
        let (cx, cz) = ground.cell(x, z);
        let shares = self.mapper.sample_weights_at(ground.map, cx, cz);
        out["layers"] = shares
            .iter()
            .enumerate()
            .filter(|(_, share)| **share >= 0.005)
            .map(|(layer, share)| {
                json!({
                    "layer": layer,
                    "texture": self.textures[layer],
                    "share": round2(*share),
                })
            })
            .collect();
        out["biome"] = json!(self.mapper.sample_biome_at(ground.map, cx, cz));
    }
}

fn lowest_under(ground: &Ground<'_>, x: f32, z: f32, radius: f32) -> f32 {
    let step = ground.map.scale().max(0.25);
    let n = (radius / step).ceil() as i32;
    let mut lowest = ground.height(x, z);
    for i in -n..=n {
        for j in -n..=n {
            let (dx, dz) = (i as f32 * step, j as f32 * step);
            if dx.hypot(dz) <= radius {
                lowest = lowest.min(ground.height(x + dx, z + dz));
            }
        }
    }
    for k in 0..24 {
        let a = k as f32 * std::f32::consts::TAU / 24.0;
        lowest = lowest.min(ground.height(x + radius * a.cos(), z + radius * a.sin()));
    }
    lowest
}

/// Heights over the whole map, and the share of it under `water`.
fn heights(map: &HeightMap, water: Option<f32>) -> Value {
    let mut all: Vec<f32> = map.data().to_vec();
    all.sort_by(f32::total_cmp);
    let at = |q: f32| round2(all[((all.len() - 1) as f32 * q).round() as usize]);
    let flooded = water.map(|level| {
        let under = all.partition_point(|h| *h < level);
        round2(under as f32 / all.len() as f32)
    });
    json!({
        "ground_m": {
            "min": at(0.0), "p10": at(0.1), "p25": at(0.25), "p50": at(0.5),
            "p75": at(0.75), "p90": at(0.9), "max": at(1.0),
        },
        "water": water.map(|level| json!({ "level_m": round2(level), "flooded_share": flooded })),
    })
}

/// Where each placement stands - an absolute one where the game stands it
/// (walked off water and steep ground when it avoids water, on the ground it
/// rests on), a scatter or a grid by its bounds. The ground itself is left
/// out.
fn placements(record: &RoomRecord, map: &HeightMap, water: Option<f32>) -> Vec<Value> {
    let is_ground = |name: &str| {
        record
            .generators
            .get(name)
            .is_some_and(|g| matches!(g.kind, GeneratorKind::Terrain(_)))
    };
    record
        .placements
        .iter()
        .enumerate()
        .filter_map(|(index, placement)| match placement {
            Placement::Absolute {
                generator_ref,
                transform,
                snap_to_terrain,
                avoid_water,
                avoid_water_clearance,
            } if !is_ground(generator_ref) => {
                let [rx, ry, rz] = transform.translation.0;
                let stands = if *snap_to_terrain {
                    pad::snapped_absolute_anchor(
                        map,
                        transform,
                        *avoid_water,
                        avoid_water_clearance.0,
                        water,
                    )
                    .to_array()
                } else {
                    [rx, ry, rz]
                };
                let mut entry = json!({
                    "index": index,
                    "name": generator_ref,
                    "x": round2(stands[0]),
                    "z": round2(stands[2]),
                    "stands_y": round2(stands[1]),
                });
                if (stands[0] - rx).hypot(stands[2] - rz) > 0.01 {
                    entry["moved_from"] = json!([round2(rx), round2(rz)]);
                }
                Some(entry)
            }
            Placement::Scatter {
                generator_ref,
                bounds,
                count,
                ..
            } => Some(json!({
                "index": index,
                "name": generator_ref,
                "scatter": scatter_bounds(bounds),
                "count": count,
            })),
            Placement::Grid {
                generator_ref,
                transform,
                counts,
                ..
            } => Some(json!({
                "index": index,
                "name": generator_ref,
                "grid_at": [round2(transform.translation.0[0]), round2(transform.translation.0[2])],
                "counts": counts,
            })),
            _ => None,
        })
        .collect()
}

fn scatter_bounds(bounds: &ScatterBounds) -> Value {
    match bounds {
        ScatterBounds::Circle { center, radius } => json!({
            "center": [round2(center.0[0]), round2(center.0[1])],
            "radius_m": round2(radius.0),
        }),
        ScatterBounds::Rect {
            center,
            extents,
            rotation,
        } => json!({
            "center": [round2(center.0[0]), round2(center.0[1])],
            "extents_m": [round2(extents.0[0]), round2(extents.0[1])],
            "rotation_rad": round2(rotation.0),
        }),
    }
}

/// Print the report - and draw the plan, if asked - for `request`.
pub(super) fn terrain_report(request: &Request<'_>) -> Value {
    if let Some(seeds) = request.seed_scan.clone() {
        return seed_scan(request, seeds);
    }
    let record = &request.record;
    let map = crate::terrain::rebuild_heightmap_for_record(record);
    let ground = Ground::new(&map);
    let splat = Splat::new(record);
    let water = room_water_level(record);
    let landing_json = record.default_landing.map(|landing| {
        let (lx, lz) = (landing.pos.0[0], landing.pos.0[1]);
        // Facing as the arrival turns: counter-clockwise from -Z (session 873).
        let yaw = landing.yaw_deg.0.to_radians();
        let mut at = point(&ground, lx, lz, None, water);
        at["facing"] = json!([round2(-yaw.sin()), round2(-yaw.cos())]);
        splat.read(&ground, lx, lz, &mut at);
        at
    });
    let mut report = json!({
        "map": {
            "cells": map.width(),
            "cell_m": round2(map.scale()),
            "extent_m": round2(ground.extent),
            "centre": [0.0, 0.0],
        },
        "landing": landing_json,
        "points": request
            .at
            .iter()
            .map(|&(x, z)| {
                let mut at = point(&ground, x, z, request.footprint, water);
                splat.read(&ground, x, z, &mut at);
                at
            })
            .collect::<Vec<_>>(),
        "placements": placements(record, &map, water),
    });
    if let (Some(obj), Value::Object(h)) = (report.as_object_mut(), heights(&map, water)) {
        obj.extend(h);
    }
    if let Some(path) = request.plan {
        let window = plan::Window::new(request.focus, request.span, ground.extent, plan::SIZE);
        report["plan"] = match plan::draw(&ground, record, water, &request.at, &window, path) {
            Ok(()) => json!({
                "path": path.display().to_string(),
                "centre": [round2(window.centre.0), round2(window.centre.1)],
                "span_m": round2(window.span),
                "px": plan::SIZE,
                "grid_m": window.grid_step(),
                "axes": "+X right, +Z down",
            }),
            Err(e) => json!({ "error": e }),
        };
    }
    report
}

/// `--seed-scan` (#1449): the record's terrain recipe under each of `seeds`,
/// read with the same heightmap job, water line and landing, as numbers -
/// and with `--plan` as a contact sheet of small plans, seeds left to right
/// and top to bottom, each marked with its seed. Session 873 found the
/// Understory's pool beside its landing this way, on the eleventh seed of a
/// scratch program's sheet.
fn seed_scan(request: &Request<'_>, seeds: std::ops::Range<u64>) -> Value {
    const TILE: u32 = 256;
    let seeds: Vec<u64> = seeds.collect();
    let columns = (seeds.len() as f32).sqrt().ceil().max(1.0) as u32;
    let rows = (seeds.len() as u32).div_ceil(columns);
    let (width, height) = (columns * TILE, rows * TILE);
    let mut sheet = request
        .plan
        .map(|_| vec![0u8; (width * height * 3) as usize]);
    let water = room_water_level(&request.record);
    let mut table = Vec::new();
    for (i, &seed) in seeds.iter().enumerate() {
        let record = reseeded(&request.record, seed);
        let map = crate::terrain::rebuild_heightmap_for_record(&record);
        let ground = Ground::new(&map);
        let mut row = json!({ "seed": seed });
        if let (Some(obj), Value::Object(h)) = (row.as_object_mut(), heights(&map, water)) {
            obj.extend(h);
        }
        if let Some(landing) = record.default_landing {
            row["landing"] = point(&ground, landing.pos.0[0], landing.pos.0[1], None, water);
        }
        if let Some(sheet) = sheet.as_mut() {
            let window = plan::Window::new(request.focus, request.span, ground.extent, TILE);
            let mut tile = plan::render(&ground, &record, water, &request.at, &window, false);
            tile.number(6, 6, seed as usize, [255, 255, 255]);
            let (left, top) = ((i as u32 % columns) * TILE, (i as u32 / columns) * TILE);
            for y in 0..TILE {
                let from = (y * TILE * 3) as usize;
                let to = (((top + y) * width + left) * 3) as usize;
                sheet[to..to + (TILE * 3) as usize]
                    .copy_from_slice(&tile.buf[from..from + (TILE * 3) as usize]);
            }
        }
        table.push(row);
    }
    let mut report = json!({ "seeds": table });
    if let (Some(path), Some(sheet)) = (request.plan, sheet) {
        report["sheet"] =
            match image::save_buffer(path, &sheet, width, height, image::ExtendedColorType::Rgb8) {
                Ok(()) => json!({
                    "path": path.display().to_string(),
                    "columns": columns,
                    "tile_px": TILE,
                    "order": "seeds left to right, top to bottom",
                }),
                Err(e) => json!({ "error": format!("cannot write {}: {e}", path.display()) }),
            };
    }
    report
}

/// `record` with its terrain recipe - the generator the game reads it from
/// (the first terrain by name, as `find_terrain_config` picks) - re-seeded.
fn reseeded(record: &RoomRecord, seed: u64) -> RoomRecord {
    let mut record = record.clone();
    let mut names: Vec<String> = record.generators.keys().cloned().collect();
    names.sort();
    for name in names {
        if let Some(GeneratorKind::Terrain(cfg)) =
            record.generators.get_mut(&name).map(|g| &mut g.kind)
        {
            cfg.seed = seed;
            break;
        }
    }
    record
}

fn round2(v: f32) -> f64 {
    (f64::from(v) * 100.0).round() / 100.0
}

/// The plan view: the ground from above, shaded by its slope and tinted by
/// its height, the water blue, a grid every so many metres, and on it the
/// landing (white, with the way arrivals face), each absolute placement
/// (amber, a way out magenta) by its index, each scatter's bounds (faint
/// rings), and the points asked about (red, numbered from 1).
mod plan {
    use super::*;

    /// The plan's side, px.
    pub const SIZE: u32 = 1024;

    pub struct Window {
        pub centre: (f32, f32),
        pub span: f32,
        /// The image's side, px.
        pub size: u32,
    }

    impl Window {
        pub fn new(focus: Option<(f32, f32)>, span: Option<f32>, extent: f32, size: u32) -> Self {
            Self {
                centre: focus.unwrap_or((0.0, 0.0)),
                span: span.filter(|s| *s > 1.0).unwrap_or(extent),
                size,
            }
        }

        /// Metres between grid lines: about eight to twenty across.
        pub fn grid_step(&self) -> f32 {
            [5.0, 10.0, 20.0, 50.0, 100.0, 200.0]
                .into_iter()
                .find(|step| self.span / step <= 20.0)
                .unwrap_or(500.0)
        }

        fn to_world(&self, px: f32, py: f32) -> (f32, f32) {
            let m = self.span / self.size as f32;
            (
                self.centre.0 - self.span * 0.5 + px * m,
                self.centre.1 - self.span * 0.5 + py * m,
            )
        }

        fn to_px(&self, x: f32, z: f32) -> (i64, i64) {
            let m = self.size as f32 / self.span;
            (
                ((x - self.centre.0 + self.span * 0.5) * m).floor() as i64,
                ((z - self.centre.1 + self.span * 0.5) * m).floor() as i64,
            )
        }
    }

    /// An RGB image being drawn, `size` px a side.
    pub struct Canvas {
        pub buf: Vec<u8>,
        pub size: u32,
    }

    impl Canvas {
        pub fn new(size: u32) -> Self {
            Self {
                buf: vec![0; (size * size * 3) as usize],
                size,
            }
        }

        fn put(&mut self, x: i64, y: i64, rgb: [u8; 3]) {
            let size = i64::from(self.size);
            if x < 0 || y < 0 || x >= size || y >= size {
                return;
            }
            let i = ((y as u32 * self.size + x as u32) * 3) as usize;
            self.buf[i..i + 3].copy_from_slice(&rgb);
        }

        fn dot(&mut self, x: i64, y: i64, r: i64, rgb: [u8; 3]) {
            for dy in -r..=r {
                for dx in -r..=r {
                    self.put(x + dx, y + dy, rgb);
                }
            }
        }

        fn ring(&mut self, x: i64, y: i64, r: f32, rgb: [u8; 3]) {
            let steps = (r * 8.0).clamp(64.0, 4096.0) as i32;
            for k in 0..steps {
                let a = k as f32 * std::f32::consts::TAU / steps as f32;
                self.put(
                    x + (r * a.cos()).round() as i64,
                    y + (r * a.sin()).round() as i64,
                    rgb,
                );
            }
        }

        fn line(&mut self, from: (i64, i64), to: (i64, i64), rgb: [u8; 3]) {
            let n = (to.0 - from.0).abs().max((to.1 - from.1).abs()).max(1);
            for k in 0..=n {
                let t = k as f32 / n as f32;
                self.put(
                    from.0 + ((to.0 - from.0) as f32 * t).round() as i64,
                    from.1 + ((to.1 - from.1) as f32 * t).round() as i64,
                    rgb,
                );
            }
        }

        /// A number in a 3x5 digit font, drawn `scale` px a dot, dark-edged
        /// so it reads over any ground.
        pub fn number(&mut self, x: i64, y: i64, n: usize, rgb: [u8; 3]) {
            const DIGITS: [[u8; 5]; 10] = [
                [7, 5, 5, 5, 7],
                [2, 6, 2, 2, 7],
                [7, 1, 7, 4, 7],
                [7, 1, 7, 1, 7],
                [5, 5, 7, 1, 1],
                [7, 4, 7, 1, 7],
                [7, 4, 7, 5, 7],
                [7, 1, 1, 1, 1],
                [7, 5, 7, 5, 7],
                [7, 5, 7, 1, 7],
            ];
            const SCALE: i64 = 2;
            let text = n.to_string();
            for pass in [[16, 16, 16], rgb] {
                let grow = i64::from(pass != rgb);
                for (i, ch) in text.bytes().enumerate() {
                    let glyph = DIGITS[usize::from(ch - b'0')];
                    for (row, bits) in glyph.iter().enumerate() {
                        for col in 0..3 {
                            if bits & (4 >> col) != 0 {
                                let gx = x + (i as i64 * 4 + col) * SCALE;
                                let gy = y + row as i64 * SCALE;
                                for dy in -grow..SCALE + grow {
                                    for dx in -grow..SCALE + grow {
                                        self.put(gx + dx, gy + dy, pass);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn draw(
        ground: &Ground<'_>,
        record: &RoomRecord,
        water: Option<f32>,
        at: &[(f32, f32)],
        window: &Window,
        path: &Path,
    ) -> Result<(), String> {
        let canvas = render(ground, record, water, at, window, true);
        image::save_buffer(
            path,
            &canvas.buf,
            canvas.size,
            canvas.size,
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
    }

    /// The plan drawn into a canvas: the ground, the water, the grid, the
    /// landing, the points - and the placements, when `marks` (a seed
    /// scan's small tiles leave them out: they stand where the record says
    /// whatever the ground).
    pub fn render(
        ground: &Ground<'_>,
        record: &RoomRecord,
        water: Option<f32>,
        at: &[(f32, f32)],
        window: &Window,
        marks: bool,
    ) -> Canvas {
        let size = window.size;
        let mut canvas = Canvas::new(size);
        let data = ground.map.data();
        let (low, high) = data
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), h| {
                (lo.min(*h), hi.max(*h))
            });
        let light = {
            let l = [-1.0f32, 2.0, -1.0];
            let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
            [l[0] / n, l[1] / n, l[2] / n]
        };
        let step = window.grid_step();
        for py in 0..size {
            for px in 0..size {
                let (x, z) = window.to_world(px as f32 + 0.5, py as f32 + 0.5);
                let h = ground.height(x, z);
                let n = ground.normal(x, z);
                let shade = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).clamp(0.0, 1.0);
                let t = ((h - low) / (high - low).max(1e-3)).clamp(0.0, 1.0);
                let base = [40.0 + 150.0 * t, 70.0 + 110.0 * t, 40.0 + 100.0 * t];
                let lit = 0.3 + 0.7 * shade;
                let mut rgb = base.map(|c| (c * lit).clamp(0.0, 255.0) as u8);
                if let Some(level) = water.filter(|level| h < *level) {
                    let depth = ((level - h) / 6.0).clamp(0.0, 1.0);
                    rgb = [
                        (60.0 - 30.0 * depth) as u8,
                        (95.0 - 45.0 * depth) as u8,
                        (140.0 - 50.0 * depth) as u8,
                    ];
                }
                canvas.put(px as i64, py as i64, rgb);
            }
        }
        // The grid, and the world's axes brighter.
        let m_per_px = window.span / size as f32;
        for p in 0..size {
            let (x, z) = window.to_world(p as f32 + 0.5, p as f32 + 0.5);
            let on = |v: f32| (v / step).round() * step;
            for (coord, vertical) in [(x, true), (z, false)] {
                let line = on(coord);
                if (coord - line).abs() < m_per_px * 0.5 {
                    let rgb: [u8; 3] = if line == 0.0 {
                        [230, 230, 230]
                    } else {
                        [150, 150, 150]
                    };
                    for q in 0..i64::from(size) {
                        let (gx, gy) = if vertical {
                            (p as i64, q)
                        } else {
                            (q, p as i64)
                        };
                        let i = ((gy as u32 * size + gx as u32) * 3) as usize;
                        let old = [canvas.buf[i], canvas.buf[i + 1], canvas.buf[i + 2]];
                        let mix = old
                            .iter()
                            .zip(rgb)
                            .map(|(o, g)| ((u16::from(*o) + u16::from(g)) / 2) as u8)
                            .collect::<Vec<_>>();
                        canvas.put(gx, gy, [mix[0], mix[1], mix[2]]);
                    }
                }
            }
        }
        let px_per_m = size as f32 / window.span;
        for (index, placement) in record.placements.iter().enumerate().filter(|_| marks) {
            match placement {
                Placement::Scatter {
                    bounds: ScatterBounds::Circle { center, radius },
                    ..
                } => {
                    let (cx, cy) = window.to_px(center.0[0], center.0[1]);
                    canvas.ring(cx, cy, radius.0 * px_per_m, [120, 200, 200]);
                }
                Placement::Absolute {
                    generator_ref,
                    transform,
                    snap_to_terrain,
                    avoid_water,
                    avoid_water_clearance,
                } => {
                    let Some(generator) = record.generators.get(generator_ref) else {
                        continue;
                    };
                    if matches!(generator.kind, GeneratorKind::Terrain(_)) {
                        continue;
                    }
                    let stands = if *snap_to_terrain {
                        pad::snapped_absolute_anchor(
                            ground.map,
                            transform,
                            *avoid_water,
                            avoid_water_clearance.0,
                            water,
                        )
                    } else {
                        bevy::math::Vec3::from_array(transform.translation.0)
                    };
                    let way_out = holds_way_out(generator);
                    let rgb = if way_out {
                        [230, 80, 230]
                    } else {
                        [250, 190, 60]
                    };
                    let (x, y) = window.to_px(stands.x, stands.z);
                    canvas.dot(x, y, 3, [16, 16, 16]);
                    canvas.dot(x, y, 2, rgb);
                    canvas.number(x + 6, y - 12, index, rgb);
                }
                _ => {}
            }
        }
        if let Some(landing) = record.default_landing {
            let (lx, lz) = (landing.pos.0[0], landing.pos.0[1]);
            let yaw = landing.yaw_deg.0.to_radians();
            let (x, y) = window.to_px(lx, lz);
            let reach = 20.0 / px_per_m;
            let tip = window.to_px(lx - yaw.sin() * reach, lz - yaw.cos() * reach);
            canvas.line((x, y), tip, [255, 255, 255]);
            canvas.dot(x, y, 4, [16, 16, 16]);
            canvas.dot(x, y, 3, [255, 255, 255]);
        }
        for (k, &(ax, az)) in at.iter().enumerate() {
            let (x, y) = window.to_px(ax, az);
            for d in -6..=6 {
                canvas.put(x + d, y + d, [230, 40, 40]);
                canvas.put(x + d, y - d, [230, 40, 40]);
            }
            canvas.number(x + 8, y + 4, k + 1, [255, 90, 90]);
        }
        canvas
    }

    fn holds_way_out(generator: &crate::pds::Generator) -> bool {
        matches!(
            generator.kind,
            GeneratorKind::Gateway { .. } | GeneratorKind::Portal { .. }
        ) || generator.children.iter().any(holds_way_out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 33 x 33 map, 2 m a cell, that rises half a metre per metre toward
    /// +X: ground at world x is `0.5 * (x + 32)`.
    fn ramp() -> HeightMap {
        let mut map = HeightMap::new(33, 33, 2.0);
        for z in 0..33 {
            for x in 0..33 {
                map.data_mut()[z * 33 + x] = 0.5 * (x as f32 * 2.0);
            }
        }
        map
    }

    /// The numbers a long thing is laid by: on ground rising toward +X,
    /// downhill is -X, the slope is atan(0.5), and the contour runs along Z -
    /// which `place --yaw 90` turns local +X to.
    #[test]
    fn a_point_on_a_slope_says_its_height_slope_downhill_and_contour() {
        let map = ramp();
        let ground = Ground::new(&map);

        let at = point(&ground, 0.0, 5.0, None, None);

        assert_eq!(at["ground_m"], 16.0, "{at}");
        assert_eq!(at["slope_deg"], 26.57, "{at}");
        assert_eq!(at["downhill"], json!([-1.0, 0.0]), "{at}");
        assert_eq!(at["contour_yaw_deg"], 90.0, "{at}");
        assert!(at.get("footprint").is_none() && at.get("under_water_m").is_none());
    }

    /// A 4 m wide thing on that slope rests on the high side (2 m above the
    /// centre) and the ground falls 4 m beneath it; under a water line, the
    /// point says how deep.
    #[test]
    fn a_footprint_rests_on_its_highest_ground_and_says_the_drop() {
        let map = ramp();
        let ground = Ground::new(&map);

        let at = point(&ground, 0.0, 0.0, Some(4.0), Some(20.0));

        assert_eq!(at["footprint"]["rests_on_m"], 18.0, "{at}");
        assert_eq!(at["footprint"]["lowest_m"], 14.0, "{at}");
        assert_eq!(at["footprint"]["drop_m"], 4.0, "{at}");
        assert_eq!(at["under_water_m"], 4.0, "{at}");
    }

    /// Flat ground has no downhill and no contour to lay along.
    #[test]
    fn flat_ground_has_no_downhill() {
        let map = HeightMap::new(9, 9, 1.0);
        let ground = Ground::new(&map);

        let at = point(&ground, 1.0, 1.0, None, None);

        assert_eq!(at["slope_deg"], 0.0);
        assert!(
            at["downhill"].is_null() && at["contour_yaw_deg"].is_null(),
            "{at}"
        );
    }

    /// The share flooded is the share of the map below the water line.
    #[test]
    fn the_flooded_share_is_the_share_below_the_water() {
        let map = ramp();

        let report = heights(&map, Some(8.0));

        // Columns 0..=7 of 33 (heights 0..7) are below 8 m.
        assert_eq!(
            report["water"]["flooded_share"],
            round2(8.0 / 33.0),
            "{report}"
        );
        assert_eq!(report["ground_m"]["min"], 0.0);
        assert_eq!(report["ground_m"]["max"], 32.0);
    }

    /// The report stands each placement where the game stands it and reads
    /// the landing's ground as the game does - one heightmap, one mapping.
    #[test]
    fn a_seeded_worlds_report_reads_the_ground_the_game_stands_things_on() {
        let record = RoomRecord::default_for_seed(3, "did:render:3");
        let map = crate::terrain::rebuild_heightmap_for_record(&record);
        let finished = crate::terrain::FinishedHeightMap(map);
        let landing = record
            .default_landing
            .expect("a seeded world has a landing");

        let report = terrain_report(&Request {
            record: record.clone(),
            at: vec![(10.0, -20.0)],
            footprint: Some(3.5),
            plan: None,
            focus: None,
            span: None,
            seed_scan: None,
        });

        assert_eq!(
            report["landing"]["ground_m"],
            round2(finished.world_height_at(landing.pos.0[0], landing.pos.0[1])),
        );
        assert_eq!(
            report["points"][0]["ground_m"],
            round2(finished.world_height_at(10.0, -20.0)),
        );
        let gateway = report["placements"]
            .as_array()
            .expect("placements")
            .iter()
            .find(|p| p["name"] == "social_gateway")
            .expect("the seeded gateway");
        let Placement::Absolute {
            transform,
            avoid_water,
            avoid_water_clearance,
            ..
        } = &record.placements[gateway["index"].as_u64().expect("an index") as usize]
        else {
            panic!("the seeded gateway is placed absolutely");
        };
        let stands = pad::snapped_absolute_anchor(
            &finished.0,
            transform,
            *avoid_water,
            avoid_water_clearance.0,
            room_water_level(&record),
        );
        assert_eq!(gateway["stands_y"], round2(stands.y));
        assert!(
            report["placements"]
                .as_array()
                .expect("placements")
                .iter()
                .all(|p| p["name"] != "base_terrain"),
            "the ground is not a placement to report"
        );
    }

    /// #1449: a scan re-seeds the terrain the game reads, and each seed's
    /// row reads that seed's ground - two seeds, two different grounds, each
    /// equal to its own heightmap's.
    #[test]
    fn a_seed_scan_reads_each_seeds_own_ground() {
        let record = RoomRecord::default_for_seed(3, "did:render:3");
        let request = Request {
            record: record.clone(),
            at: vec![],
            footprint: None,
            plan: None,
            focus: None,
            span: None,
            seed_scan: Some(5..7),
        };

        let report = terrain_report(&request);

        let rows = report["seeds"].as_array().expect("rows");
        assert_eq!(rows.len(), 2);
        for row in rows {
            let seed = row["seed"].as_u64().expect("a seed");
            let map = crate::terrain::rebuild_heightmap_for_record(&reseeded(&record, seed));
            assert_eq!(
                row["ground_m"],
                heights(&map, room_water_level(&record))["ground_m"]
            );
            assert_eq!(
                crate::pds::find_terrain_config(&reseeded(&record, seed)).map(|c| c.seed),
                Some(seed),
                "the terrain the game reads is the one re-seeded"
            );
        }
        assert_ne!(
            rows[0]["ground_m"], rows[1]["ground_m"],
            "two seeds, two grounds"
        );
    }

    #[test]
    fn a_seed_range_is_a_count_or_a_span() {
        assert_eq!(parse_seed_range("18"), Ok(0..18));
        assert_eq!(parse_seed_range("3..9"), Ok(3..9));
        assert!(parse_seed_range("0").is_err() && parse_seed_range("9..3").is_err());
        assert!(
            parse_seed_range("0..65").is_err(),
            "more than a sheet reads"
        );
        assert!(parse_seed_range("a..b").is_err());
    }

    #[test]
    fn a_point_is_two_numbers() {
        assert_eq!(parse_xz("-18.6, 22"), Ok((-18.6, 22.0)));
        assert!(parse_xz("12").is_err() && parse_xz("1,2,3").is_err() && parse_xz("a,b").is_err());
    }

    /// #1461: a point's layer shares are the ground's own weight map there -
    /// the texel the GPU blends by, from the record's rules - and its biome
    /// is that texel's largest channel. Read at grid nodes across a seeded
    /// world, where a point and a texel are the same sample.
    #[test]
    fn a_points_layers_are_the_weight_map_the_ground_is_drawn_with() {
        let record = RoomRecord::default_for_seed(3, "did:render:3");
        let map = crate::terrain::rebuild_heightmap_for_record(&record);
        let texels = crate::terrain::record_splat_mapper(Some(&record)).generate(&map);
        let ground = Ground::new(&map);
        let splat = Splat::new(&record);
        let half = ground.extent * 0.5;
        // One interior node for each layer that is largest somewhere, so
        // every layer the world draws is read at least once.
        let largest_at = |i: usize, j: usize| {
            let texel = texels.data[j * texels.width + i];
            (0..4)
                .max_by_key(|&l| (texel[l], std::cmp::Reverse(l)))
                .expect("four")
        };
        let mut nodes = std::collections::BTreeMap::new();
        for j in (9..texels.height - 9).step_by(13) {
            for i in (9..texels.width - 9).step_by(13) {
                nodes.entry(largest_at(i, j)).or_insert((i, j));
            }
        }
        let mut biomes = std::collections::BTreeSet::new();
        let mut blends = 0;
        for (i, j) in nodes.into_values() {
            let (x, z) = (i as f32 * map.scale() - half, j as f32 * map.scale() - half);
            let mut at = json!({});
            splat.read(&ground, x, z, &mut at);
            let texel = texels.data[j * texels.width + i];
            let layers = at["layers"].as_array().expect("layers");
            for (layer, &byte) in texel.iter().enumerate() {
                let share = layers
                    .iter()
                    .find(|l| l["layer"] == layer)
                    .map_or(0.0, |l| l["share"].as_f64().expect("a share"));
                assert!(
                    (share - f64::from(byte) / 255.0).abs() <= 0.0075,
                    "layer {layer} at ({x}, {z}): {share} against the texel's {byte}/255 - {at}"
                );
            }
            let largest = largest_at(i, j);
            assert_eq!(at["biome"], largest, "the biome is the largest share: {at}");
            biomes.insert(largest);
            blends += usize::from(layers.len() > 1);
        }
        assert!(
            biomes.len() > 1 && blends > 0,
            "the points must reach more than one layer and a blend, or they test little: {biomes:?}, {blends}"
        );
    }

    /// Why #1461 exists. A rule's weight fades over a skirt a third of its
    /// half-range wide outside its band, so a rock rule written for "any slope
    /// past 0.22" as `0.22..10` - the Understory's - reaches all the way down
    /// to level ground and takes a share of it (there the rippled "sand"
    /// texture of session 876). Capped at 1.0, a vertical face, it takes
    /// none. The report reads the record's own rules, so it tells the two
    /// apart.
    #[test]
    fn a_rock_band_open_to_ten_takes_a_share_of_level_ground() {
        let mut flat = HeightMap::new(33, 33, 2.0);
        flat.data_mut().fill(1.0);
        let rules = |rock_slope_max: f32| {
            let mut record = RoomRecord::default_for_seed(3, "did:render:3");
            let cfg = record
                .generators
                .values_mut()
                .find_map(|g| match &mut g.kind {
                    GeneratorKind::Terrain(cfg) => Some(cfg),
                    _ => None,
                })
                .expect("a seeded world has its terrain");
            let rule = |h: (f32, f32), s: (f32, f32)| crate::pds::SovereignSplatRule {
                height_min: crate::pds::Fp(h.0),
                height_max: crate::pds::Fp(h.1),
                slope_min: crate::pds::Fp(s.0),
                slope_max: crate::pds::Fp(s.1),
                sharpness: crate::pds::Fp(2.0),
            };
            let never = rule((2.0, 3.0), (0.0, 1.0));
            cfg.material.rules = [
                rule((0.0, 1.0), (0.0, 0.2)),
                never,
                rule((0.0, 1.0), (0.22, rock_slope_max)),
                never,
            ];
            record
        };
        let share_of_rock = |record: &RoomRecord| {
            let mut at = json!({});
            Splat::new(record).read(&Ground::new(&flat), 0.0, 0.0, &mut at);
            at["layers"]
                .as_array()
                .expect("layers")
                .iter()
                .find(|l| l["layer"] == 2)
                .map_or(0.0, |l| l["share"].as_f64().expect("a share"))
        };

        let open = share_of_rock(&rules(10.0));
        assert!(open > 0.4, "the open band takes {open} of level ground");
        assert_eq!(share_of_rock(&rules(1.0)), 0.0, "capped at a vertical face");
    }
}
