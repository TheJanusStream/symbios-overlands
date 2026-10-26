//! The water a room draws on its ground, at more than one level (#1594).
//!
//! A room's water is set by its terrain's own Water children, its layers;
//! water under any other piece keeps the rectangle its plane is drawn as,
//! the walls built round it holding it. The lowest layer is the room's sea:
//! all ground below its level, drawn as the world plane it always was. Each
//! higher layer is a lake (#1760, the owner's decision of 2026-10-23): the
//! ground below its level round its point - the cells of the finished
//! heightmap below it that its point's cell reaches through others below
//! it, inside its rectangle - so it fills its dip and stops where the
//! ground rises over its level, never hanging over lower ground. A level
//! over its dip's rim spills, and the lake then covers what lies below it
//! as far as it reaches. A lake whose point stands at or over its level is
//! no lake, and draws nothing.
//!
//! [`RoomWater`] holds them: the sea's level and the bodies, each a level
//! and its cells ([`WaterBody`]), with one lookup, the level at a point
//! ([`RoomWater::level_at`]): the highest body that holds it, else the sea.
//! A room with its sea alone has no bodies, and is drawn, answered and read
//! as it always was.
//!
//! A body's cells take in the ring of dry ones round its water too
//! ([`body_round`]): its mesh is drawn over them at its level and the
//! ground hides what lies under it there, so its shore is the line along
//! which the ground crosses its level, not the cells' steps. Its level is
//! the water's along its bank as well, the ground reached from it that
//! stands less than [`REACH_M`] over it: a reed band or a damp margin up
//! the bank measures from the lake beside it, not from the sea below.

use std::sync::Arc;

use bevy::prelude::*;
use bevy_symbios_ground::HeightMap;

use crate::pds::{GeneratorKind, RoomRecord};

/// The cells of a heightmap, centred on the world's origin as the terrain
/// is: cell `(cx, cz)` stands at its column and row times the scale, less
/// half the grid's extent, and its square reaches half a scale round that.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellGrid {
    pub width: u32,
    pub height: u32,
    /// Metres between cells.
    pub scale: f32,
}

impl CellGrid {
    /// The grid of `hm`'s cells.
    pub fn of(hm: &HeightMap) -> Self {
        Self {
            width: hm.width() as u32,
            height: hm.height() as u32,
            scale: hm.scale(),
        }
    }

    /// Half the grid's extent along x and along z (m).
    fn half(&self) -> (f32, f32) {
        (
            self.width.saturating_sub(1) as f32 * self.scale * 0.5,
            self.height.saturating_sub(1) as f32 * self.scale * 0.5,
        )
    }

    /// Where cell `(cx, cz)` stands in the world.
    pub fn world_of(&self, (cx, cz): (u32, u32)) -> (f32, f32) {
        let (hx, hz) = self.half();
        (cx as f32 * self.scale - hx, cz as f32 * self.scale - hz)
    }

    /// The cell whose square holds world `(x, z)`, where one does.
    pub fn cell_at(&self, x: f32, z: f32) -> Option<(u32, u32)> {
        let (hx, hz) = self.half();
        let (fx, fz) = (
            ((x + hx) / self.scale).round(),
            ((z + hz) / self.scale).round(),
        );
        (fx >= 0.0 && fz >= 0.0 && fx < self.width as f32 && fz < self.height as f32)
            .then_some((fx as u32, fz as u32))
    }
}

/// A set of a grid's cells: a rectangle of the grid, each cell in or out.
#[derive(Clone, Debug, PartialEq)]
pub struct CellMask {
    /// The rectangle's first column and row, and its columns and rows.
    x0: u32,
    z0: u32,
    w: u32,
    h: u32,
    /// One bit a cell, row-major.
    bits: Vec<u64>,
}

impl CellMask {
    /// No cell of the rectangle of `w x h` cells from `(x0, z0)`.
    fn empty((x0, z0): (u32, u32), (w, h): (u32, u32)) -> Self {
        Self {
            x0,
            z0,
            w,
            h,
            bits: vec![0; (w as usize * h as usize).div_ceil(64)],
        }
    }

    /// The bit of cell `(cx, cz)`, where the rectangle holds it.
    fn bit(&self, (cx, cz): (u32, u32)) -> Option<usize> {
        let (dx, dz) = (cx.checked_sub(self.x0)?, cz.checked_sub(self.z0)?);
        (dx < self.w && dz < self.h).then(|| dz as usize * self.w as usize + dx as usize)
    }

    /// The cells `cells` (of which there is one at least), in the smallest
    /// rectangle that holds them.
    fn of(cells: impl Iterator<Item = (u32, u32)> + Clone) -> Self {
        let (mut x0, mut x1, mut z0, mut z1) = (u32::MAX, 0, u32::MAX, 0);
        for (cx, cz) in cells.clone() {
            (x0, x1, z0, z1) = (x0.min(cx), x1.max(cx), z0.min(cz), z1.max(cz));
        }
        let mut mask = Self::empty((x0, z0), (x1 - x0 + 1, z1 - z0 + 1));
        for cell in cells {
            mask.insert(cell);
        }
        mask
    }

    fn insert(&mut self, cell: (u32, u32)) {
        if let Some(i) = self.bit(cell) {
            self.bits[i / 64] |= 1 << (i % 64);
        }
    }

    /// Whether cell `(cx, cz)` is in it.
    pub fn contains(&self, cell: (u32, u32)) -> bool {
        self.bit(cell)
            .is_some_and(|i| self.bits[i / 64] & (1 << (i % 64)) != 0)
    }

    /// How many cells are in it.
    pub fn len(&self) -> usize {
        self.bits.iter().map(|b| b.count_ones() as usize).sum()
    }

    /// Whether no cell is in it.
    pub fn is_empty(&self) -> bool {
        self.bits.iter().all(|&b| b == 0)
    }

    /// Whether a cell in it lies in the rectangle of `w x h` cells from
    /// `(x0, z0)`.
    pub fn meets(&self, (x0, z0): (u32, u32), (w, h): (u32, u32)) -> bool {
        let (xa, za) = (x0.max(self.x0), z0.max(self.z0));
        let (xb, zb) = (
            x0.saturating_add(w).min(self.x0 + self.w),
            z0.saturating_add(h).min(self.z0 + self.h),
        );
        (za..zb).any(|cz| (xa..xb).any(|cx| self.contains((cx, cz))))
    }

    /// Its rectangle and a hash of the cells in it: what a fingerprint keys
    /// it by. FNV-1a over its words, the same on every machine.
    pub fn signature(&self) -> [u64; 5] {
        let hash = self.bits.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, word| {
            word.to_le_bytes()
                .iter()
                .fold(h, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
        });
        [
            u64::from(self.x0),
            u64::from(self.z0),
            u64::from(self.w),
            u64::from(self.h),
            hash,
        ]
    }

    /// Each row's runs of cells in it, west to east and north to south: the
    /// run's row, its first column and the column one past its last.
    pub fn runs(&self) -> Vec<(u32, u32, u32)> {
        let mut runs = Vec::new();
        for dz in 0..self.h {
            let cz = self.z0 + dz;
            let mut start = None;
            for dx in 0..=self.w {
                let cx = self.x0 + dx;
                let inside = dx < self.w && self.contains((cx, cz));
                match (inside, start) {
                    (true, None) => start = Some(cx),
                    (false, Some(first)) => {
                        runs.push((cz, first, cx));
                        start = None;
                    }
                    _ => {}
                }
            }
        }
        runs
    }
}

/// How far over a body's level its bank is the body's own (m): the damp
/// margin's depth (`config::terrain::splat::MOISTURE_DEPTH`), which the
/// riparian bands the seeded reeds grow in lie inside.
pub const REACH_M: f32 = crate::config::terrain::splat::MOISTURE_DEPTH;

/// Where a body lies: its cells, on the grid they are cells of, and how far
/// its level reaches up its bank.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyCells {
    pub grid: CellGrid,
    /// Where it is water: drawn, swum in, and the shore its streets stop at.
    pub mask: CellMask,
    /// Where its level is the water's ([`RoomWater::level_at`]): its cells,
    /// and the bank reached from them standing under [`REACH_M`] over it.
    pub reach: CellMask,
}

impl BodyCells {
    /// Whether world `(x, z)` lies in one of its cells.
    pub fn holds(&self, x: f32, z: f32) -> bool {
        self.grid
            .cell_at(x, z)
            .is_some_and(|cell| self.mask.contains(cell))
    }

    /// Whether world `(x, z)` lies in its reach.
    pub fn reaches(&self, x: f32, z: f32) -> bool {
        self.grid
            .cell_at(x, z)
            .is_some_and(|cell| self.reach.contains(cell))
    }
}

/// The mesh a body is drawn as (#1725), in its own frame - its level at
/// local Y 0, the world's origin under it: each row's run of its cells one
/// quad, facing up, covering their squares. Flat, so a run ending where the
/// next row's does not leaves no gap; the water's shader takes its waves and
/// its light from the world's place, so no more is needed of the mesh.
pub fn body_mesh(cells: &BodyCells) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    let grid = cells.grid;
    let s = grid.scale * 0.5;
    let (hx, hz) = grid.half();
    let runs = cells.mask.runs();
    let mut positions = Vec::with_capacity(runs.len() * 4);
    let mut uvs = Vec::with_capacity(runs.len() * 4);
    let mut indices = Vec::with_capacity(runs.len() * 6);
    for (cz, first, past) in runs {
        let (x0, z) = grid.world_of((first, cz));
        let (x1, _) = grid.world_of((past - 1, cz));
        let (x0, x1, z0, z1) = (x0 - s, x1 + s, z - s, z + s);
        let base = positions.len() as u32;
        for (x, z) in [(x0, z0), (x0, z1), (x1, z1), (x1, z0)] {
            positions.push([x, 0.0, z]);
            uvs.push([
                (x + hx) / (2.0 * hx).max(f32::EPSILON),
                (z + hz) / (2.0 * hz).max(f32::EPSILON),
            ]);
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// A body of water at its own level: a lake (#1760), or one of Berlin's
/// (#1724). Its cells are shared, so the plane physics answers from and
/// the mesh drawn over them hold one copy.
#[derive(Clone, Debug, PartialEq)]
pub struct WaterBody {
    /// Its level, world Y (m).
    pub level: f32,
    pub cells: Arc<BodyCells>,
}

/// The cells of the lake a layer at `level` fills round world `point` on
/// `hm` (see the module docs): its point's cell and every cell below its
/// level that this one reaches through others below it, 4-connected, where
/// `inside` holds them - round them, its ring and its reach
/// ([`body_round`]), as far as `inside` holds them. `None` where its point's
/// cell lies off the grid, at or over its level, or where `inside` does not
/// hold it.
///
/// A cell is below the level where the heightmap's height there is: the
/// same comparisons on the same heights, so every peer finds one lake.
pub fn lake_cells(
    hm: &HeightMap,
    level: f32,
    point: (f32, f32),
    inside: impl Fn(f32, f32) -> bool,
) -> Option<BodyCells> {
    /// What a cell has been found to be.
    const UNSEEN: u8 = 0;
    const WATER: u8 = 1;
    /// Inside the rectangle, at or over the level.
    const DRY: u8 = 2;
    const OUT: u8 = 3;
    let grid = CellGrid::of(hm);
    let (w, h) = (hm.width(), hm.height());
    let heights = hm.data();
    let holds = |i: usize| {
        let (x, z) = grid.world_of(((i % w) as u32, (i / w) as u32));
        inside(x, z)
    };
    let (sx, sz) = grid.cell_at(point.0, point.1)?;
    let start = sz as usize * w + sx as usize;
    if !(holds(start) && heights[start] < level) {
        return None;
    }
    let mut found = vec![UNSEEN; w * h];
    found[start] = WATER;
    let (mut stack, mut water) = (vec![start], Vec::new());
    while let Some(i) = stack.pop() {
        water.push(i);
        let (cx, cz) = (i % w, i / w);
        let sides = [
            (cx > 0, i.wrapping_sub(1)),
            (cx + 1 < w, i + 1),
            (cz > 0, i.wrapping_sub(w)),
            (cz + 1 < h, i + w),
        ];
        for (on_grid, j) in sides {
            if on_grid && found[j] == UNSEEN {
                found[j] = if !holds(j) {
                    OUT
                } else if heights[j] < level {
                    stack.push(j);
                    WATER
                } else {
                    DRY
                };
            }
        }
    }
    Some(body_round(hm, level, &water, holds))
}

/// A body's cells round its `water` - `hm`'s cells it covers at `level`,
/// row-major indices - as far as `inside` holds them: the water and the
/// ring of cells round it, corners too, where it is drawn; and its reach,
/// those and the bank reached from them, 4-connected, through cells
/// standing at or over the level by less than [`REACH_M`]. The same
/// comparisons on the same heights, in one order, so every peer finds one
/// body.
///
/// # Panics
///
/// If `water` is empty.
pub fn body_round(
    hm: &HeightMap,
    level: f32,
    water: &[usize],
    inside: impl Fn(usize) -> bool,
) -> BodyCells {
    body_round_on(hm.data(), CellGrid::of(hm), level, water, inside)
}

/// [`body_round`] over `heights`, the cells of `grid` row-major: for ground
/// not yet a heightmap, Berlin's as it is settled (#1724).
pub fn body_round_on(
    heights: &[f32],
    grid: CellGrid,
    level: f32,
    water: &[usize],
    inside: impl Fn(usize) -> bool,
) -> BodyCells {
    /// What a cell has been found to be.
    const UNSEEN: u8 = 0;
    const WATER: u8 = 1;
    const RING: u8 = 2;
    const BANK: u8 = 3;
    const OUT: u8 = 4;
    let (w, h) = (grid.width as usize, grid.height as usize);
    let mut found = vec![UNSEEN; w * h];
    for &i in water {
        found[i] = WATER;
    }
    let mut ring = Vec::new();
    for &i in water {
        let (cx, cz) = (i % w, i / w);
        for nz in cz.saturating_sub(1)..(cz + 2).min(h) {
            for nx in cx.saturating_sub(1)..(cx + 2).min(w) {
                let j = nz * w + nx;
                if found[j] == UNSEEN {
                    found[j] = if inside(j) { RING } else { OUT };
                    if found[j] == RING {
                        ring.push(j);
                    }
                }
            }
        }
    }
    let mut bank = Vec::new();
    let mut stack: Vec<usize> = water.iter().chain(&ring).copied().collect();
    while let Some(i) = stack.pop() {
        let (cx, cz) = (i % w, i / w);
        let sides = [
            (cx > 0, i.wrapping_sub(1)),
            (cx + 1 < w, i + 1),
            (cz > 0, i.wrapping_sub(w)),
            (cz + 1 < h, i + w),
        ];
        for (on_grid, j) in sides {
            if on_grid && found[j] == UNSEEN && (level..level + REACH_M).contains(&heights[j]) {
                found[j] = if inside(j) { BANK } else { OUT };
                if found[j] == BANK {
                    bank.push(j);
                    stack.push(j);
                }
            }
        }
    }
    let cell = |&i: &usize| ((i % w) as u32, (i / w) as u32);
    BodyCells {
        grid,
        mask: CellMask::of(water.iter().chain(&ring).map(cell)),
        reach: CellMask::of(water.iter().chain(&ring).chain(&bank).map(cell)),
    }
}

/// A room's terrain water layers (see the module docs): the Water children
/// of its Terrain generators, each as the transform its plane is drawn
/// with - its height the level, its place the point - the terrain standing
/// at the origin, as `room_water_level` reads them. In the order of the
/// generators' names and of the children, so every peer lists them alike.
pub fn terrain_layers(record: &RoomRecord) -> Vec<Transform> {
    let mut terrains: Vec<_> = record
        .generators
        .iter()
        .filter(|(_, g)| matches!(g.kind, GeneratorKind::Terrain(_)))
        .collect();
    terrains.sort_by(|a, b| a.0.cmp(b.0));
    terrains
        .into_iter()
        .flat_map(|(_, g)| g.children.iter())
        .filter(|c| matches!(c.kind, GeneratorKind::Water { .. }))
        .map(|c| Transform::from(&c.transform))
        .collect()
}

/// The lowest of `layers`' levels: the sea's, where there is water at all.
pub fn sea_level(layers: &[Transform]) -> Option<f32> {
    layers
        .iter()
        .map(|t| t.translation.y)
        .fold(None, |low: Option<f32>, y| {
            Some(low.map_or(y, |l| l.min(y)))
        })
}

/// The rectangle a layer drawn with `layer` holds on `hm`: the plane's -
/// `hm`'s extent about its own frame, as the spawner draws it - as a test of
/// world `(x, z)` at the layer's level. Worked by one fixed formula in f64 -
/// the point's offset from the layer turned back by its rotation and divided
/// by its scale, with no matrix inverse, whose SIMD and scalar paths may part
/// in the last bit - so a cell on the edge falls on the same side on every
/// peer, native or wasm, and every peer finds the same lake.
fn layer_rectangle(hm: &HeightMap, layer: &Transform) -> impl Fn(f32, f32) -> bool {
    let half = f64::from(hm.width().saturating_sub(1) as f32 * hm.scale() * 0.5);
    let q = layer.rotation;
    let (qx, qy, qz, qw) = (
        f64::from(q.x),
        f64::from(q.y),
        f64::from(q.z),
        f64::from(q.w),
    );
    let norm = (qx * qx + qy * qy + qz * qz + qw * qw).sqrt();
    // The inverse turn: the conjugate of the unit rotation.
    let (ux, uy, uz, w) = (-qx / norm, -qy / norm, -qz / norm, qw / norm);
    let (tx, tz) = (
        f64::from(layer.translation.x),
        f64::from(layer.translation.z),
    );
    let (sx, sz) = (f64::from(layer.scale.x), f64::from(layer.scale.z));
    move |x: f32, z: f32| {
        // At the layer's own level, the offset has no height.
        let (dx, dz) = (f64::from(x) - tx, f64::from(z) - tz);
        // v + 2w (u x v) + 2 u x (u x v), with v = (dx, 0, dz).
        let (cx, cy, cz) = (uy * dz, uz * dx - ux * dz, -uy * dx);
        let (ex, ez) = (uy * cz - uz * cy, ux * cy - uy * cx);
        let (lx, lz) = (dx + 2.0 * w * cx + 2.0 * ex, dz + 2.0 * w * cz + 2.0 * ez);
        (lx / sx).abs() <= half && (lz / sz).abs() <= half
    }
}

/// The lake the layer drawn with `layer` fills on `hm`, inside its
/// rectangle ([`layer_rectangle`]), or `None` where it is no lake (see
/// [`lake_cells`]).
pub fn lake(hm: &HeightMap, layer: &Transform) -> Option<WaterBody> {
    let level = layer.translation.y;
    let point = (layer.translation.x, layer.translation.z);
    lake_cells(hm, level, point, layer_rectangle(hm, layer)).map(|cells| WaterBody {
        level,
        cells: Arc::new(cells),
    })
}

/// Whether the layer drawn with `layer` is a lake on `hm` at all - its
/// point's cell on the grid, in its rectangle and below its level - as
/// [`lake`] finds it, at the cost of that one cell: for a reader that asks
/// every frame, the editor saying why a layer draws nothing.
pub fn holds_a_lake(hm: &HeightMap, layer: &Transform) -> bool {
    let grid = CellGrid::of(hm);
    let inside = layer_rectangle(hm, layer);
    grid.cell_at(layer.translation.x, layer.translation.z)
        .is_some_and(|cell| {
            let (x, z) = grid.world_of(cell);
            inside(x, z) && hm.get(cell.0 as usize, cell.1 as usize) < layer.translation.y
        })
}

/// The room's water on its ground, by level (see the module docs).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoomWater {
    /// The sea's level: its lowest terrain water layer's, or `None` where
    /// the room has no water - or where a geodata region's core has water,
    /// whose bodies are the room's in its place (#1724).
    pub sea: Option<f32>,
    /// Its bodies: Berlin's, in such a region, then the lakes, in the order
    /// of their layers.
    pub bodies: Vec<WaterBody>,
}

/// What a reader asks of water: its level at a point (#1727), `None` where
/// there is none. The room's water as drawn ([`RoomWater`]), the planes the
/// compile has spawned (`crate::water::WaterSurfaces`), what a scatter reads
/// of them (`crate::water::ScatterWater`), or one level for the whole room,
/// as the room's single water line was read.
pub trait WaterAt {
    fn water_at(&self, x: f32, z: f32) -> Option<f32>;
}

impl WaterAt for RoomWater {
    fn water_at(&self, x: f32, z: f32) -> Option<f32> {
        self.level_at(x, z)
    }
}

impl WaterAt for Option<f32> {
    fn water_at(&self, _: f32, _: f32) -> Option<f32> {
        *self
    }
}

impl WaterAt for crate::water::WaterSurfaces {
    fn water_at(&self, x: f32, z: f32) -> Option<f32> {
        self.level_near(Vec2::new(x, z)).map(|(_, y)| y)
    }
}

impl RoomWater {
    /// Whether the room has no water at all: no sea, and no body.
    pub fn is_dry(&self) -> bool {
        self.sea.is_none() && self.bodies.is_empty()
    }

    /// Water that is a sea alone at `level`, or none: a room with no lakes,
    /// as a reader of one level reads it.
    pub fn sea_alone(level: Option<f32>) -> Self {
        Self {
            sea: level,
            bodies: Vec::new(),
        }
    }

    /// What a fingerprint of what stands on the room's water is keyed by
    /// (#1727): the sea's level alone where it has no bodies, as the room's
    /// one water line was keyed, so nothing keyed so far moves; and each
    /// body's level and cells besides where it has some.
    pub fn key(&self) -> serde_json::Value {
        if self.bodies.is_empty() {
            return serde_json::to_value(self.sea).unwrap_or_default();
        }
        let bodies: Vec<(f32, [u64; 5])> = self
            .bodies
            .iter()
            .map(|body| (body.level, body.cells.mask.signature()))
            .collect();
        serde_json::json!({ "sea": self.sea, "bodies": bodies })
    }

    /// `record`'s water on `heightmap`. With no heightmap yet, its sea
    /// alone: a lake is found on the ground it fills.
    pub fn of(record: &RoomRecord, heightmap: Option<&crate::terrain::FinishedHeightMap>) -> Self {
        Self::from_layers(&terrain_layers(record), heightmap)
    }

    /// The water of a room whose terrain water layers are `layers`
    /// ([`terrain_layers`]) on `heightmap`: what [`Self::of`] reads from a
    /// record, for a reader that holds the layers and not the record.
    pub fn from_layers(
        layers: &[Transform],
        heightmap: Option<&crate::terrain::FinishedHeightMap>,
    ) -> Self {
        let Some(lowest) = sea_level(layers) else {
            return Self::default();
        };
        // Berlin's water, where its core has any, is its own bodies, each at
        // its own level (#1724): no sea spans such a region.
        let (sea, mut bodies) = match crate::world_builder::compile::water::geo_water(heightmap) {
            Some(berlin) => (None, berlin.to_vec()),
            None => (Some(lowest), Vec::new()),
        };
        if let Some(heightmap) = heightmap {
            bodies.extend(
                layers
                    .iter()
                    .filter(|layer| layer.translation.y > lowest)
                    .filter_map(|layer| lake(&heightmap.0, layer)),
            );
        }
        Self { sea, bodies }
    }

    /// The level of the water at world `(x, z)`: the highest body whose
    /// reach holds it - its water, or its bank - else the sea; `None` where
    /// the room has no water there.
    pub fn level_at(&self, x: f32, z: f32) -> Option<f32> {
        self.bodies
            .iter()
            .filter(|body| body.cells.reaches(x, z))
            .map(|body| body.level)
            .fold(self.sea, |high, level| {
                Some(high.map_or(level, |h| h.max(level)))
            })
    }

    /// The level of the water over cell `cell` of `grid`, the finished
    /// heightmap's: [`Self::level_at`] where the cell stands, asked of the
    /// cell itself - a body on another grid holds none of it.
    pub fn level_at_cell(&self, grid: CellGrid, cell: (u32, u32)) -> Option<f32> {
        self.bodies
            .iter()
            .filter(|body| body.cells.grid == grid && body.cells.reach.contains(cell))
            .map(|body| body.level)
            .fold(self.sea, |high, level| {
                Some(high.map_or(level, |h| h.max(level)))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{Fp3, Generator, TransformData};
    use crate::terrain::FinishedHeightMap;

    /// The cells next to `(cx, cz)` on a `w x h` grid: the four it shares a
    /// side with, and with `corners` the four it shares a corner with too.
    fn neighbours(
        (cx, cz): (u32, u32),
        (w, h): (u32, u32),
        corners: bool,
    ) -> impl Iterator<Item = (u32, u32)> {
        let sides = [(-1, 0), (1, 0), (0, -1), (0, 1)];
        let diagonals = [(-1, -1), (1, -1), (-1, 1), (1, 1)];
        sides
            .into_iter()
            .chain(diagonals.into_iter().filter(move |_| corners))
            .filter_map(move |(dx, dz): (i64, i64)| {
                let (x, z) = (i64::from(cx) + dx, i64::from(cz) + dz);
                (x >= 0 && z >= 0 && x < i64::from(w) && z < i64::from(h))
                    .then_some((x as u32, z as u32))
            })
    }

    /// A heightmap of `w x h` cells `scale` apart, each as tall as `height`
    /// says at its place in the world.
    fn ground(w: usize, h: usize, scale: f32, height: impl Fn(f32, f32) -> f32) -> HeightMap {
        let mut hm = HeightMap::new(w, h, scale);
        let grid = CellGrid::of(&hm);
        for cz in 0..h {
            for cx in 0..w {
                let (x, z) = grid.world_of((cx as u32, cz as u32));
                *hm.get_mut(cx, cz) = height(x, z);
            }
        }
        hm
    }

    /// A record whose terrain has a Water child at each of `layers`: its
    /// place `(x, z)` and its level, with `scale` on x and z.
    fn record(layers: &[((f32, f32), f32, f32)]) -> RoomRecord {
        let mut terrain = Generator::from_kind(GeneratorKind::Terrain(Default::default()));
        for &((x, z), level, scale) in layers {
            let mut water = Generator::from_kind(GeneratorKind::Water {
                surface: Default::default(),
            });
            water.transform = TransformData {
                translation: Fp3([x, level, z]),
                scale: Fp3([scale, 1.0, scale]),
                ..TransformData::default()
            };
            terrain.children.push(water);
        }
        RoomRecord {
            lex_type: "network.symbios.room".to_owned(),
            environment: Default::default(),
            generators: [("land".to_owned(), terrain)].into(),
            placements: Vec::new(),
            traits: Default::default(),
            contact_effects: Default::default(),
            default_landing: None,
            geo_source: None,
            identity: None,
            opaque_refs: Default::default(),
        }
    }

    /// A valley on the west at 4 m, rising past x = 0 to a plateau at 30 m
    /// from x = 20, with a round dip in it 40 m across, its floor at 22 m,
    /// round (40, 0): on 121 x 121 cells a metre apart.
    fn plateau(x: f32, z: f32) -> f32 {
        let land = if x < 0.0 {
            4.0
        } else {
            (4.0 + x * 1.3).min(30.0)
        };
        let dip = ((x - 40.0).powi(2) + z.powi(2)).sqrt();
        if dip < 20.0 { 22.0 + dip * 0.3 } else { land }
    }

    #[test]
    fn a_cell_is_found_where_it_stands_and_nowhere_off_the_grid() {
        let grid = CellGrid {
            width: 11,
            height: 7,
            scale: 2.0,
        };
        for cell in [(0, 0), (10, 6), (3, 4)] {
            let (x, z) = grid.world_of(cell);
            assert_eq!(grid.cell_at(x, z), Some(cell));
            assert_eq!(
                grid.cell_at(x + 0.9, z - 0.9),
                Some(cell),
                "within its square"
            );
        }
        assert_eq!(grid.world_of((0, 0)), (-10.0, -6.0));
        for off in [(-11.5, 0.0), (11.5, 0.0), (0.0, 7.5), (f32::NAN, 0.0)] {
            assert_eq!(grid.cell_at(off.0, off.1), None, "{off:?}");
        }
    }

    /// **A world with its sea alone has no bodies** (#1760): the sea is the
    /// level everywhere, as the room's one water line always was.
    #[test]
    fn a_world_with_its_sea_alone_has_no_bodies() {
        let hm = FinishedHeightMap(ground(121, 121, 1.0, plateau), None);
        let water = RoomWater::of(&record(&[((0.0, 0.0), 2.0, 1.0)]), Some(&hm));
        assert_eq!(water.sea, Some(2.0));
        assert!(water.bodies.is_empty());
        for (x, z) in [(-50.0, 0.0), (40.0, 0.0), (55.0, 50.0)] {
            assert_eq!(water.level_at(x, z), Some(2.0));
        }
        assert_eq!(RoomWater::of(&record(&[]), Some(&hm)), RoomWater::default());
        assert_eq!(RoomWater::default().level_at(0.0, 0.0), None, "no water");
    }

    /// **A plateau lake fills its dip and leaves the valley dry** (#1760,
    /// the owner's decision): a lake at 26 m with its point in the dip
    /// holds the dip and its shore, and not the valley, which is below its
    /// level too but out of its reach - where the sea is the level still.
    #[test]
    fn a_plateau_lake_fills_its_dip_and_leaves_the_valley_dry() {
        let hm = FinishedHeightMap(ground(121, 121, 1.0, plateau), None);
        let water = RoomWater::of(
            &record(&[((0.0, 0.0), 2.0, 1.0), ((40.0, 0.0), 26.0, 1.0)]),
            Some(&hm),
        );
        assert_eq!(water.bodies.len(), 1);
        let lake = &water.bodies[0];
        assert_eq!(lake.level, 26.0);
        for (x, z) in [(40.0, 0.0), (40.0, 12.0), (28.0, 0.0)] {
            assert!(lake.cells.holds(x, z), "the dip at ({x}, {z})");
            assert_eq!(water.level_at(x, z), Some(26.0));
        }
        for (x, z) in [(-30.0, 0.0), (5.0, 0.0), (40.0, 30.0)] {
            assert!(!lake.cells.holds(x, z), "out of its reach at ({x}, {z})");
            assert_eq!(water.level_at(x, z), Some(2.0), "the sea's");
        }
        // Every cell it holds is below its level or next to one that is.
        let grid = lake.cells.grid;
        for (cz, a, b) in lake.cells.mask.runs() {
            for cx in a..b {
                let height = hm.0.get(cx as usize, cz as usize);
                let near_water = neighbours((cx, cz), (grid.width, grid.height), true)
                    .chain([(cx, cz)])
                    .any(|c| hm.0.get(c.0 as usize, c.1 as usize) < 26.0);
                assert!(near_water, "({cx}, {cz}) at {height} m");
            }
        }
    }

    /// **A lake's level reaches up its bank** (#1727): as far as the ground
    /// is reached from it standing under [`REACH_M`] over it - its bank at
    /// 26.5 m reads the lake's level, the plateau 4 m over it the sea's, and
    /// so does the slope behind the 30 m rim, near as it is to the lake's
    /// level, which no water of the lake's reaches over.
    #[test]
    fn a_lakes_level_reaches_up_its_bank_and_no_further() {
        let hm = FinishedHeightMap(ground(121, 121, 1.0, plateau), None);
        let water = RoomWater::of(
            &record(&[((0.0, 0.0), 2.0, 1.0), ((40.0, 0.0), 26.0, 1.0)]),
            Some(&hm),
        );
        let lake = &water.bodies[0];
        assert!(!lake.cells.holds(40.0, 15.0), "the bank is no water");
        assert!(lake.cells.reaches(40.0, 15.0));
        assert_eq!(water.level_at(40.0, 15.0), Some(26.0), "the bank");
        assert_eq!(water.level_at(40.0, 30.0), Some(2.0), "the plateau");
        assert_eq!(water.level_at(18.0, 0.0), Some(2.0), "behind the rim");
        for (cz, a, b) in lake.cells.mask.runs() {
            assert!((a..b).all(|cx| lake.cells.reach.contains((cx, cz))));
        }
    }

    /// **A lake over its rim spills** (#1760): at 32 m, over the plateau's
    /// 30, it reaches the valley and covers it, as water at that level
    /// would - inside its rectangle, the world's extent centred on its point
    /// as its plane is, which ends 20 m west of the valley's edge here.
    #[test]
    fn a_lake_over_its_rim_spills() {
        let hm = FinishedHeightMap(ground(121, 121, 1.0, plateau), None);
        let water = RoomWater::of(
            &record(&[((0.0, 0.0), 2.0, 1.0), ((40.0, 0.0), 32.0, 1.0)]),
            Some(&hm),
        );
        assert_eq!(water.level_at(-15.0, 0.0), Some(32.0));
        assert_eq!(water.level_at(55.0, 50.0), Some(32.0));
        assert_eq!(water.level_at(-30.0, 0.0), Some(2.0), "past its rectangle");
    }

    /// **A lake whose point stands dry has no body** (#1760), and the
    /// ground there reads the sea's level.
    #[test]
    fn a_lake_whose_point_stands_dry_has_no_body() {
        let hm = FinishedHeightMap(ground(121, 121, 1.0, plateau), None);
        let water = RoomWater::of(
            &record(&[((0.0, 0.0), 2.0, 1.0), ((55.0, 50.0), 26.0, 1.0)]),
            Some(&hm),
        );
        assert!(water.bodies.is_empty());
        assert_eq!(water.level_at(40.0, 0.0), Some(2.0));
        // And off the grid altogether.
        let off = RoomWater::of(
            &record(&[((0.0, 0.0), 2.0, 1.0), ((500.0, 0.0), 26.0, 1.0)]),
            Some(&hm),
        );
        assert!(off.bodies.is_empty());
    }

    /// The editor's one-cell question - is there a lake at all - answers as
    /// the flood does, wherever the layer stands: in the dip, on the
    /// plateau, at the dip's edge, off the grid, and with its point past a
    /// turned, shrunk rectangle's reach.
    #[test]
    fn holds_a_lake_answers_as_the_flood_does() {
        let hm = ground(121, 121, 1.0, plateau);
        let turned = Quat::from_rotation_y(0.6);
        for (x, z, level, scale, rotation) in [
            (40.0, 0.0, 26.0, 1.0, Quat::IDENTITY),
            (55.0, 50.0, 26.0, 1.0, Quat::IDENTITY),
            (40.0, 19.5, 26.0, 1.0, Quat::IDENTITY),
            (40.0, 0.0, 21.0, 1.0, Quat::IDENTITY),
            (500.0, 0.0, 26.0, 1.0, Quat::IDENTITY),
            (40.0, 0.0, 26.0, 0.01, turned),
            (40.0, 0.0, 26.0, 0.1, turned),
        ] {
            let layer = Transform::from_xyz(x, level, z)
                .with_rotation(rotation)
                .with_scale(Vec3::new(scale, 1.0, scale));
            assert_eq!(
                holds_a_lake(&hm, &layer),
                lake(&hm, &layer).is_some(),
                "({x}, {z}) at {level} m, scaled {scale}"
            );
        }
    }

    /// A layer's rectangle, worked by its own fixed formula so every peer
    /// finds the same edge cells, is the plane's: it agrees with the plane's
    /// own frame - glam's inverse of the layer's transform - wherever a point
    /// is not within a hair of the edge, for a turned, scaled layer.
    #[test]
    fn a_layers_rectangle_is_its_planes() {
        let hm = HeightMap::new(41, 41, 1.0);
        let half = 20.0;
        let layer = Transform::from_xyz(3.0, 7.5, -2.0)
            .with_rotation(Quat::from_rotation_y(0.7))
            .with_scale(Vec3::new(0.6, 1.0, 0.3));
        let inside = layer_rectangle(&hm, &layer);
        let from_world = layer.compute_affine().inverse();
        let mut checked = 0;
        for i in 0..2_000 {
            let (x, z) = ((i % 50) as f32 * 0.9 - 22.0, (i / 50) as f32 * 1.1 - 22.0);
            let local = from_world.transform_point3(Vec3::new(x, 7.5, z));
            let (ex, ez) = (local.x.abs() - half, local.z.abs() - half);
            if ex.abs() > 1e-3 && ez.abs() > 1e-3 {
                assert_eq!(inside(x, z), ex < 0.0 && ez < 0.0, "({x}, {z}): {local}");
                checked += 1;
            }
        }
        assert!(checked > 1_900);
    }

    /// **Two lakes at two levels** (#1760): each fills its own dip at its
    /// own level, and each place reads its own.
    #[test]
    fn two_lakes_keep_two_levels() {
        let two_dips = |x: f32, z: f32| {
            let a = ((x + 30.0).powi(2) + z.powi(2)).sqrt();
            let b = ((x - 30.0).powi(2) + z.powi(2)).sqrt();
            if a < 15.0 {
                10.0 + a * 0.2
            } else if b < 15.0 {
                20.0 + b * 0.2
            } else {
                40.0
            }
        };
        let hm = FinishedHeightMap(ground(101, 101, 1.0, two_dips), None);
        let water = RoomWater::of(
            &record(&[
                ((0.0, 0.0), 0.0, 1.0),
                ((-30.0, 0.0), 12.0, 1.0),
                ((30.0, 0.0), 22.0, 1.0),
            ]),
            Some(&hm),
        );
        assert_eq!(water.bodies.len(), 2);
        assert_eq!(water.level_at(-30.0, 0.0), Some(12.0));
        assert_eq!(water.level_at(30.0, 0.0), Some(22.0));
        assert_eq!(water.level_at(0.0, 40.0), Some(0.0), "neither: the sea");
    }

    /// **A lake stops at its rectangle** (#1760): its plane's extent, scaled
    /// as the spawner scales it, cuts the dip where its owner set it, its
    /// shore cells inside it too.
    #[test]
    fn a_lake_stops_at_its_rectangle() {
        let hm = ground(121, 121, 1.0, plateau);
        // 120 m across, scaled to a tenth: 12 m round its point.
        let layer = Transform::from_xyz(40.0, 26.0, 0.0).with_scale(Vec3::new(0.1, 1.0, 0.1));
        let lake = lake(&hm, &layer).expect("its point is in the dip");
        assert!(lake.cells.holds(40.0, 0.0) && lake.cells.holds(45.0, 5.0));
        assert!(!lake.cells.holds(40.0, 9.0), "past its rectangle's edge");
        assert!(!lake.cells.holds(28.0, 0.0));
        for (cz, a, b) in lake.cells.mask.runs() {
            for cx in a..b {
                let (x, z) = lake.cells.grid.world_of((cx, cz));
                assert!((x - 40.0).abs() <= 6.0 && z.abs() <= 6.0, "({x}, {z})");
            }
        }
    }

    /// **A body is drawn over its cells, facing up** (#1725): a quad a
    /// run, each over its cells' squares, every triangle turned to the sky
    /// (the water's material culls its back), at the body's level in its
    /// own frame - and nothing outside its cells.
    #[test]
    fn a_body_is_drawn_over_its_cells_facing_up() {
        let hm = ground(121, 121, 1.0, plateau);
        let lake = lake(&hm, &Transform::from_xyz(40.0, 26.0, 0.0)).expect("a lake");
        let mesh = body_mesh(&lake.cells);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        let indices: Vec<usize> = mesh.indices().expect("indexed").iter().collect();
        assert_eq!(indices.len(), lake.cells.mask.runs().len() * 6);
        let mut area = 0.0;
        for tri in indices.chunks(3) {
            let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| Vec3::from_array(positions[i]));
            let normal = (b - a).cross(c - a);
            assert!(normal.y > 0.0, "a triangle turned down: {a} {b} {c}");
            area += normal.length() * 0.5;
            for p in [a, b, c] {
                assert_eq!(p.y, 0.0);
            }
            let middle = (a + b + c) / 3.0;
            assert!(
                lake.cells.holds(middle.x, middle.z),
                "drawn outside its cells at {middle}"
            );
        }
        assert!(
            (area - lake.cells.mask.len() as f32).abs() < 1e-2,
            "{area} m2 over {} cells",
            lake.cells.mask.len()
        );
    }

    /// The mask's runs cover what it holds and nothing else.
    #[test]
    fn a_masks_runs_are_its_cells() {
        let hm = ground(121, 121, 1.0, plateau);
        let lake = lake(&hm, &Transform::from_xyz(40.0, 26.0, 0.0)).expect("a lake");
        let mask = &lake.cells.mask;
        let in_runs: usize = mask.runs().iter().map(|&(_, a, b)| (b - a) as usize).sum();
        assert_eq!(in_runs, mask.len());
        for (cz, a, b) in mask.runs() {
            assert!((a..b).all(|cx| mask.contains((cx, cz))));
            assert!(a == 0 || !mask.contains((a - 1, cz)));
            assert!(!mask.contains((b, cz)));
        }
    }
}
