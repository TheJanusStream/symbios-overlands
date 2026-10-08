//! The far field (#1585, epic #1580): Berlin from the core's edge out to
//! the square's, drawn as the region's horizon.
//!
//! The walkable core is a kilometre or so of street-level ground, and a
//! square may be 19 km across. Beyond the core the square is drawn coarse,
//! from one more render of the terrain and of the land use over the whole
//! square at about 40 m a pixel ([`far_plan`]). It has the city's hills, its
//! land use painted on the region's own ground layers, and its water at the
//! core's level ([`decode_far`]), so the river the core stands by runs on to
//! the horizon. It has no collider: nobody walks there.
//!
//! The far field meets the core without a crack ([`build_far_mesh`]). Its
//! grid is the renders' pixel centres plus the core's four edges, and along
//! those edges every boundary vertex of the core is a vertex of the far
//! field too, at the core's own height, fanned out to the coarse grid.
//!
//! A square that the core nearly fills gets no far field. Where one lands,
//! the haze opens so the horizon reads ([`FarField::horizon_m`]), and the
//! sky stands beyond it ([`FarField::sky_half_m`]).

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy_symbios_ground::{HeightMap, WeightMap};
use geodata::GeoSquare;
use geodata::berlin::LandUse;
use geodata::request::Bbox;

use crate::pds::SovereignTerrainConfig;

/// The far field's pixel size the renders aim at (m): fine enough for the
/// city's hills and lakes on the horizon, coarse enough for a 19 km square
/// to stay one small render.
pub(crate) const FAR_CELL_M: f32 = 40.0;

/// The fewest pixels a far render has a side: a small square's ring is
/// still drawn in more than a handful of cells.
pub(crate) const FAR_GRID_MIN: u32 = 64;

/// The most pixels a far render has a side.
pub(crate) const FAR_GRID_MAX: u32 = 256;

/// How many far pixels wide the ring round the core must be for a far field
/// to be drawn at all.
const MIN_RING_CELLS: f32 = 2.0;

/// How far past the far field's farthest edge the sky stands (m).
const SKY_MARGIN_M: f32 = 500.0;

/// The boundary walls' thickness (m).
const WALL_THICKNESS_M: f32 = 2.0;

/// How far below the core's lowest ground the boundary walls reach (m).
const WALL_BELOW_M: f32 = 50.0;

/// How far above the core's highest ground the boundary walls stand (m):
/// past any jump, and above the cloud deck.
const WALL_ABOVE_M: f32 = 500.0;

/// The collision layer the boundary walls alone belong to. They interact
/// with every layer, so they stop every body; a spatial query that leaves
/// this bit out of its mask - a particle's bounce - passes them by.
pub(crate) const WALL_LAYER: u32 = 1 << 1;

/// The far field a square gets: the whole square, in renders of `grid`
/// pixels a side, round a core `core_m` metres across.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FarPlan {
    /// The square, whole: the far renders' box.
    pub square: GeoSquare,
    /// Pixels per side of the far renders.
    pub grid: u32,
    /// Metres per pixel.
    pub cell: f32,
    /// The core's extent (m): its heightmap's `(grid - 1) * cell`.
    pub core_m: f32,
}

impl FarPlan {
    /// The box the far renders cover: the square.
    pub(crate) fn bbox(&self) -> Bbox {
        let side = i64::from(self.square.size_m);
        let (min_e, min_n) = (i64::from(self.square.min_e), i64::from(self.square.min_n));
        Bbox {
            min_e,
            min_n,
            max_e: min_e + side,
            max_n: min_n + side,
        }
    }
}

/// The far field for `square` round the core `cfg` builds, or `None` where
/// the core nearly fills the square: a ring under [`MIN_RING_CELLS`] far
/// pixels wide is not drawn.
pub(crate) fn far_plan(square: GeoSquare, cfg: &SovereignTerrainConfig) -> Option<FarPlan> {
    let (core_grid, core_cell) = super::core_grid(square.size_m, cfg);
    let core_m = (core_grid - 1) as f32 * core_cell;
    let side = square.size_m as f32;
    let grid = ((side / FAR_CELL_M).ceil() as u32).clamp(FAR_GRID_MIN, FAR_GRID_MAX);
    let cell = side / grid as f32;
    ((side - core_m) / 2.0 >= MIN_RING_CELLS * cell).then_some(FarPlan {
        square,
        grid,
        cell,
        core_m,
    })
}

/// The square beyond the core, decoded: heights at real altitude, shaped to
/// the core's water where it takes it, and the land use over them, both
/// row-major from the north-west corner at the far renders' pixel centres.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FarField {
    grid: u32,
    cell: f32,
    heights: Vec<f32>,
    cover: Vec<Option<LandUse>>,
    /// Whether the far field took the core's water: its beds carved below
    /// the core's level and its ground kept above it, so the region's water
    /// plane may span it.
    wet: bool,
}

impl FarField {
    /// The square's side (m): what the far renders cover.
    pub(crate) fn side_m(&self) -> f32 {
        self.grid as f32 * self.cell
    }

    /// The far mesh's extent (m), first pixel centre to last: what a water
    /// plane spanning the far field must cover, and no more.
    pub(crate) fn span_m(&self) -> f32 {
        (self.grid - 1) as f32 * self.cell
    }

    /// Whether the far field took the core's water.
    pub(crate) fn wet(&self) -> bool {
        self.wet
    }

    /// The fog visibility the horizon needs to read (m): the square's side,
    /// twice the distance from the core's centre to its edge. Visibility is
    /// where the haze leaves 5 % of a far thing's contrast, so the edge keeps
    /// about a seventh of its contrast and the city fades into the air there
    /// rather than ending; a hill halfway out keeps more than a third. At
    /// half the side the edge vanished, and a hill 5 km out with it.
    pub(crate) fn horizon_m(&self) -> f32 {
        self.side_m()
    }

    /// The sky's half-width round a core `core_m` across: past the far
    /// field's farthest edge from anywhere on the walkable ground.
    pub(crate) fn sky_half_m(&self, core_m: f32) -> f32 {
        self.side_m() / 2.0 + core_m / 2.0 + SKY_MARGIN_M
    }

    /// `(scale, offset)` that turn the far mesh's UVs - the core's own
    /// mapping, run on past its edges so the layers' tiles do too - into
    /// the far weight map's: `weight_uv = uv * scale + offset`.
    pub(crate) fn weight_uv(&self, core_m: f32) -> (f32, f32) {
        let side = self.side_m();
        (core_m / side, (side - core_m) / (2.0 * side))
    }

    /// The splat weight map: one texel per pixel, all of it on the pixel's
    /// layer, as the core's.
    pub(crate) fn weight_map(&self) -> WeightMap {
        super::ground::one_hot_weights(&self.cover, self.grid as usize)
    }

    /// The height at world `(x, z)`, between the pixel centres round it, the
    /// world centring the square on the origin. Clamped to the outermost
    /// centres.
    pub(crate) fn height_at(&self, x: f32, z: f32) -> f32 {
        let last = (self.grid - 1) as f32;
        let half = self.side_m() / 2.0;
        let (gx, gz) = (
            ((x + half) / self.cell - 0.5).clamp(0.0, last),
            ((z + half) / self.cell - 0.5).clamp(0.0, last),
        );
        let (x0, z0) = (gx.floor() as usize, gz.floor() as usize);
        let side = self.grid as usize;
        let (x1, z1) = ((x0 + 1).min(side - 1), (z0 + 1).min(side - 1));
        let (fx, fz) = (gx - x0 as f32, gz - z0 as f32);
        let at = |x: usize, z: usize| self.heights[z * side + x];
        let north = at(x0, z0) + (at(x1, z0) - at(x0, z0)) * fx;
        let south = at(x0, z1) + (at(x1, z1) - at(x0, z1)) * fx;
        north + (south - north) * fz
    }
}

/// Decode the far renders: the terrain through its legend, the land use
/// through its, and - where the core has water at `level` - the far field's
/// water settled to that level ([`geodata::water::settle_to`]), so the
/// region's one water plane can span it. A level the far field cannot take
/// leaves its water as the terrain draws it.
pub(crate) fn decode_far(
    plan: &FarPlan,
    terrain_legend: &[u8],
    terrain: &[u8],
    land_use_legend: &[u8],
    land_use: &[u8],
    level: Option<f32>,
) -> Result<FarField, String> {
    let mut heights = super::decode_heights(terrain_legend, terrain, plan.grid, plan.cell)?.data;
    let cover = super::ground::decode_cover(land_use_legend, land_use, plan.grid)?;
    // At far pixels no street is drawn: the bridges stay as mapped.
    let water = super::ground::water_mask(&cover, plan.grid as usize, plan.cell, None);
    let wet = level
        .and_then(|level| {
            geodata::water::settle_to(&mut heights, &water, plan.grid, plan.grid, plan.cell, level)
        })
        .is_some();
    Ok(FarField {
        grid: plan.grid,
        cell: plan.cell,
        heights,
        cover,
        wet,
    })
}

/// The far field's mesh: the square minus the core, in world coordinates
/// round the origin as the core is drawn, with no CPU copy.
///
/// Its grid lines are the far renders' pixel centres and the core's four
/// edges. A cell whose edge lies along the core takes the core's boundary
/// vertices on that edge - from the one nearest each of its corners, so no
/// far vertex sits partway along a core edge - at the core's height, and
/// fans them out to its far corners: the two meshes share their boundary
/// vertex for vertex. There the far mesh also takes the core's own normals,
/// so the light runs on across the seam. All other cells are two triangles,
/// and every triangle faces up.
///
/// Its UVs are the core's mapping run on past the core's edges, so the
/// layers' tiles run on across the seam; [`FarField::weight_uv`] turns them
/// into the far weight map's.
pub(crate) fn build_far_mesh(far: &FarField, core: &HeightMap) -> Mesh {
    let (width, depth, scale) = (core.width(), core.height(), core.scale());
    let core_m = (width - 1) as f32 * scale;
    let core_half = core_m / 2.0;
    let lines = grid_lines(far, core_half);
    let n = lines.len();
    let mut mesh = FarMesh::new(n, width, depth, core_m);

    // The core's boundary vertex nearest `along` on an edge running `count`
    // vertices.
    let nearest = |along: f32, count: usize| {
        (((along + core_half) / scale).round().max(0.0) as usize).min(count - 1)
    };
    for b in 0..n - 1 {
        for a in 0..n - 1 {
            let (x0, x1, z0, z1) = (lines[a], lines[a + 1], lines[b], lines[b + 1]);
            let inside_x = ((x0 + x1) / 2.0).abs() < core_half;
            let inside_z = ((z0 + z1) / 2.0).abs() < core_half;
            if inside_x && inside_z {
                continue;
            }
            // The cell's edge along the core, if it has one: which core edge,
            // the cell's extent along it, and the two far corners facing it.
            let along_core = if inside_x && z1 == -core_half {
                Some((Edge::North, x0, x1, (a, b), (a + 1, b)))
            } else if inside_x && z0 == core_half {
                Some((Edge::South, x0, x1, (a, b + 1), (a + 1, b + 1)))
            } else if inside_z && x1 == -core_half {
                Some((Edge::West, z0, z1, (a, b), (a, b + 1)))
            } else if inside_z && x0 == core_half {
                Some((Edge::East, z0, z1, (a + 1, b), (a + 1, b + 1)))
            } else {
                None
            };
            let Some((edge, from, to, o0, o1)) = along_core else {
                let mut vertex = |a: usize, b: usize| {
                    let (x, z) = (lines[a], lines[b]);
                    if x.abs() == core_half && z.abs() == core_half {
                        // A box corner is the core's corner vertex.
                        let edge = if z < 0.0 { Edge::North } else { Edge::South };
                        mesh.boundary(core, edge, if x < 0.0 { 0 } else { width - 1 })
                    } else {
                        mesh.grid(a, b, &lines, far)
                    }
                };
                let p00 = vertex(a, b);
                let p10 = vertex(a + 1, b);
                let p01 = vertex(a, b + 1);
                let p11 = vertex(a + 1, b + 1);
                mesh.triangle(p00, p01, p10);
                mesh.triangle(p10, p01, p11);
                continue;
            };
            let count = edge.count(width, depth);
            let (first, last) = (nearest(from, count), nearest(to, count));
            let polyline: Vec<u32> = (first..=last)
                .map(|k| mesh.boundary(core, edge, k))
                .collect();
            let (o0, o1) = (
                mesh.grid(o0.0, o0.1, &lines, far),
                mesh.grid(o1.0, o1.1, &lines, far),
            );
            let middle = (polyline.len() - 1) / 2;
            for i in 0..polyline.len() - 1 {
                let apex = if i < middle { o0 } else { o1 };
                mesh.triangle(apex, polyline[i], polyline[i + 1]);
            }
            mesh.triangle(o0, polyline[middle], o1);
        }
    }
    mesh.into_mesh(core)
}

/// One of the core's four edges.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Edge {
    North,
    South,
    West,
    East,
}

impl Edge {
    /// How many core vertices run along this edge.
    fn count(self, width: usize, depth: usize) -> usize {
        match self {
            Edge::North | Edge::South => width,
            Edge::West | Edge::East => depth,
        }
    }

    /// The core grid index `(x, z)` of this edge's `k`-th vertex.
    fn vertex(self, k: usize, width: usize, depth: usize) -> (usize, usize) {
        match self {
            Edge::North => (k, 0),
            Edge::South => (k, depth - 1),
            Edge::West => (0, k),
            Edge::East => (width - 1, k),
        }
    }
}

/// The far mesh being built. Its vertices are the far grid's line
/// crossings and the core's boundary vertices, each made once and found by
/// its index - no hashing - and a corner of the core is one vertex whichever
/// edge asks for it.
struct FarMesh {
    n: usize,
    width: usize,
    depth: usize,
    core_m: f32,
    grid_at: Vec<u32>,
    core_at: Vec<u32>,
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    /// The core grid index of each vertex that is a core boundary vertex.
    from_core: Vec<Option<(usize, usize)>>,
    indices: Vec<u32>,
}

impl FarMesh {
    fn new(n: usize, width: usize, depth: usize, core_m: f32) -> Self {
        FarMesh {
            n,
            width,
            depth,
            core_m,
            grid_at: vec![u32::MAX; n * n],
            core_at: vec![u32::MAX; width * depth],
            positions: Vec::new(),
            uvs: Vec::new(),
            from_core: Vec::new(),
            indices: Vec::new(),
        }
    }

    fn push(&mut self, x: f32, y: f32, z: f32, core: Option<(usize, usize)>) -> u32 {
        let half = self.core_m / 2.0;
        self.positions.push([x, y, z]);
        self.uvs
            .push([(x + half) / self.core_m, (z + half) / self.core_m]);
        self.from_core.push(core);
        (self.positions.len() - 1) as u32
    }

    /// The vertex where grid lines `a` and `b` cross, at the far field's
    /// height.
    fn grid(&mut self, a: usize, b: usize, lines: &[f32], far: &FarField) -> u32 {
        let slot = b * self.n + a;
        if self.grid_at[slot] == u32::MAX {
            let (x, z) = (lines[a], lines[b]);
            self.grid_at[slot] = self.push(x, far.height_at(x, z), z, None);
        }
        self.grid_at[slot]
    }

    /// The core's `k`-th boundary vertex along `edge`, at its height.
    fn boundary(&mut self, core: &HeightMap, edge: Edge, k: usize) -> u32 {
        let (cx, cz) = edge.vertex(k, self.width, self.depth);
        let slot = cz * self.width + cx;
        if self.core_at[slot] == u32::MAX {
            let half = self.core_m / 2.0;
            let s = core.scale();
            let (x, z) = (cx as f32 * s - half, cz as f32 * s - half);
            self.core_at[slot] = self.push(x, core.get(cx, cz), z, Some((cx, cz)));
        }
        self.core_at[slot]
    }

    /// A triangle facing up, whichever way round its corners come; none
    /// where they are in a line.
    fn triangle(&mut self, a: u32, b: u32, c: u32) {
        let [ax, _, az] = self.positions[a as usize];
        let [bx, _, bz] = self.positions[b as usize];
        let [cx, _, cz] = self.positions[c as usize];
        // Facing +Y is a negative turn in (x, z), as the core's mesh winds.
        let turn = (bx - ax) * (cz - az) - (bz - az) * (cx - ax);
        if turn < 0.0 {
            self.indices.extend([a, b, c]);
        } else if turn > 0.0 {
            self.indices.extend([a, c, b]);
        }
    }

    fn into_mesh(self, core: &HeightMap) -> Mesh {
        let mut normals = vec![Vec3::ZERO; self.positions.len()];
        for tri in self.indices.chunks_exact(3) {
            let [p0, p1, p2] = [0, 1, 2].map(|k| Vec3::from(self.positions[tri[k] as usize]));
            let face = (p1 - p0).cross(p2 - p0);
            for &i in tri {
                normals[i as usize] += face;
            }
        }
        let normals: Vec<[f32; 3]> = normals
            .into_iter()
            .zip(&self.from_core)
            .map(|(n, core_vertex)| match *core_vertex {
                Some((x, z)) => core_normal(core, x, z),
                None => n.try_normalize().unwrap_or(Vec3::Y),
            })
            .map(Into::into)
            .collect();
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_indices(Indices::U32(self.indices));
        mesh.generate_tangents()
            .expect("far-field tangent generation failed");
        // No CPU copy, as the core's mesh (#1134).
        mesh.asset_usage = RenderAssetUsages::RENDER_WORLD;
        mesh
    }
}

/// The core mesh's own normal at its vertex `(x, z)`: the area-weighted sum
/// of its triangles there, split as `HeightMapMeshBuilder` splits each cell
/// (top-left, bottom-left, top-right; top-right, bottom-left, bottom-right).
fn core_normal(core: &HeightMap, x: usize, z: usize) -> Vec3 {
    let s = core.scale();
    let at = |x: usize, z: usize| Vec3::new(x as f32 * s, core.get(x, z), z as f32 * s);
    let mut sum = Vec3::ZERO;
    for qz in z.saturating_sub(1)..=z.min(core.height() - 2) {
        for qx in x.saturating_sub(1)..=x.min(core.width() - 2) {
            let (tl, tr, bl, br) = ((qx, qz), (qx + 1, qz), (qx, qz + 1), (qx + 1, qz + 1));
            for [a, b, c] in [[tl, bl, tr], [tr, bl, br]] {
                if [a, b, c].contains(&(x, z)) {
                    let (pa, pb, pc) = (at(a.0, a.1), at(b.0, b.1), at(c.0, c.1));
                    sum += (pb - pa).cross(pc - pa);
                }
            }
        }
    }
    // Divided by its length, as the builder does: a multiply by the
    // reciprocal differs in the last bit, and the seam would show it.
    let len = sum.length();
    if len > f32::EPSILON {
        sum / len
    } else {
        Vec3::Y
    }
}

/// The four invisible walls round a core whose square has a far field:
/// `(size, centre)` of each cuboid, just outside the core's edges, from
/// [`WALL_BELOW_M`] under its lowest ground to [`WALL_ABOVE_M`] over its
/// highest. The far field is drawn and not walked (P4, #1591, walks it), and
/// it looks like ground, so without them a visitor would step off the core
/// onto it and fall through.
pub(crate) fn boundary_walls(core: &HeightMap) -> [(Vec3, Vec3); 4] {
    let half = (core.width() - 1) as f32 * core.scale() / 2.0;
    let (low, high) = core
        .data()
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &h| {
            (lo.min(h), hi.max(h))
        });
    let (bottom, top) = (low - WALL_BELOW_M, high + WALL_ABOVE_M);
    let (y, height) = ((bottom + top) / 2.0, top - bottom);
    let (t, length) = (WALL_THICKNESS_M, 2.0 * (half + WALL_THICKNESS_M));
    let off = half + t / 2.0;
    [
        (Vec3::new(length, height, t), Vec3::new(0.0, y, -off)),
        (Vec3::new(length, height, t), Vec3::new(0.0, y, off)),
        (Vec3::new(t, height, length), Vec3::new(-off, y, 0.0)),
        (Vec3::new(t, height, length), Vec3::new(off, y, 0.0)),
    ]
}

/// The far mesh's grid lines along one axis (the square is square and
/// centred, so both axes share them): the pixel centres, less any within a
/// quarter pixel of a core edge, and the two core edges.
fn grid_lines(far: &FarField, core_half: f32) -> Vec<f32> {
    let half = far.grid as f32 * far.cell / 2.0;
    let mut lines: Vec<f32> = (0..far.grid)
        .map(|i| -half + (i as f32 + 0.5) * far.cell)
        .filter(|line| (line.abs() - core_half).abs() >= far.cell / 4.0)
        .chain([-core_half, core_half])
        .collect();
    lines.sort_by(f32::total_cmp);
    lines
}

#[cfg(test)]
impl FarField {
    /// A far field of `grid` pixels `cell` metres apart, its heights from
    /// `height(x, z)` at the pixel centres, all street space.
    pub(crate) fn from_fn(grid: u32, cell: f32, height: impl Fn(f32, f32) -> f32) -> Self {
        let half = grid as f32 * cell / 2.0;
        let centre = |i: u32| -half + (i as f32 + 0.5) * cell;
        let heights = (0..grid * grid)
            .map(|i| height(centre(i % grid), centre(i / grid)))
            .collect();
        FarField {
            grid,
            cell,
            heights,
            cover: vec![None; (grid * grid) as usize],
            wet: false,
        }
    }

    /// This far field, as one that took its core's water.
    pub(crate) fn soaked(mut self) -> Self {
        self.wet = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    fn positions(mesh: &Mesh) -> Vec<[f32; 3]> {
        match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(p)) => p.clone(),
            other => panic!("positions: {other:?}"),
        }
    }

    fn triangles(mesh: &Mesh) -> Vec<[usize; 3]> {
        match mesh.indices() {
            Some(Indices::U32(i)) => i
                .chunks_exact(3)
                .map(|t| [t[0] as usize, t[1] as usize, t[2] as usize])
                .collect(),
            other => panic!("indices: {other:?}"),
        }
    }

    /// A 160 m far field of 10 m pixels round a 40 m core of 2 m cells, both
    /// sloping, the core 3 m below the far field's slope so a seam taken
    /// from the far heights would show.
    fn scene() -> (FarField, HeightMap) {
        let far = FarField::from_fn(16, 10.0, |x, z| 30.0 + 0.05 * x - 0.02 * z);
        let mut core = HeightMap::new(21, 21, 2.0);
        for z in 0..21 {
            for x in 0..21 {
                let (wx, wz) = (x as f32 * 2.0 - 20.0, z as f32 * 2.0 - 20.0);
                core.set(
                    x,
                    z,
                    27.0 + 0.05 * wx - 0.02 * wz + 0.3 * ((x * 7 + z * 3) % 5) as f32,
                );
            }
        }
        (far, core)
    }

    #[test]
    fn the_far_mesh_is_the_square_less_the_core() {
        let (far, core) = scene();
        let mesh = build_far_mesh(&far, &core);
        let (p, tris) = (positions(&mesh), triangles(&mesh));
        let mut area = 0.0;
        for t in &tris {
            let [a, b, c] = t.map(|i| p[i]);
            let centroid = ((a[0] + b[0] + c[0]) / 3.0, (a[2] + b[2] + c[2]) / 3.0);
            assert!(
                centroid.0.abs() >= 20.0 - 1e-3 || centroid.1.abs() >= 20.0 - 1e-3,
                "a triangle inside the core at {centroid:?}"
            );
            let turn = (b[0] - a[0]) * (c[2] - a[2]) - (b[2] - a[2]) * (c[0] - a[0]);
            assert!(turn < 0.0, "every triangle faces up");
            area += -turn / 2.0;
        }
        // Pixel centre to pixel centre, 150 m, less the 40 m core.
        assert!(
            (area - (150.0 * 150.0 - 40.0 * 40.0)).abs() < 0.5,
            "area {area}"
        );
    }

    #[test]
    fn the_far_field_meets_the_core_on_every_core_boundary_vertex() {
        let (far, core) = scene();
        let p = positions(&build_far_mesh(&far, &core));
        let find = |x: f32, z: f32| {
            p.iter()
                .find(|v| (v[0] - x).abs() < 1e-4 && (v[2] - z).abs() < 1e-4)
                .map(|v| v[1])
        };
        for k in 0..21 {
            let along = k as f32 * 2.0 - 20.0;
            for (x, z, cx, cz) in [
                (along, -20.0, k, 0),
                (along, 20.0, k, 20),
                (-20.0, along, 0, k),
                (20.0, along, 20, k),
            ] {
                assert_eq!(
                    find(x, z),
                    Some(core.get(cx, cz)),
                    "the core's boundary vertex at ({x}, {z}), at the core's height"
                );
            }
        }
    }

    fn attribute(mesh: &Mesh, id: bevy::mesh::MeshVertexAttribute) -> Vec<[f32; 3]> {
        match mesh.attribute(id) {
            Some(VertexAttributeValues::Float32x3(v)) => v.clone(),
            other => panic!("{other:?}"),
        }
    }

    fn uvs(mesh: &Mesh) -> Vec<[f32; 2]> {
        match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(VertexAttributeValues::Float32x2(v)) => v.clone(),
            other => panic!("{other:?}"),
        }
    }

    /// The critic's finding (#1585): a far vertex partway along a core edge
    /// is a T-junction the rasteriser need not close. Every far vertex on
    /// the core's boundary is one of the core's vertices.
    #[test]
    fn no_far_vertex_sits_partway_along_a_core_edge() {
        let (far, core) = scene();
        for [x, _, z] in positions(&build_far_mesh(&far, &core)) {
            let on_edge =
                (x.abs() == 20.0 && z.abs() <= 20.0) || (z.abs() == 20.0 && x.abs() <= 20.0);
            if on_edge {
                for v in [x, z] {
                    let k = (v + 20.0) / 2.0;
                    assert_eq!(
                        k,
                        k.round(),
                        "a far vertex at ({x}, {z}) between core vertices"
                    );
                }
            }
        }
    }

    /// Along the boundary the far mesh has the core mesh's own normals and
    /// UVs, so neither the light nor the layers' tiles step at the seam.
    #[test]
    fn the_far_mesh_shades_and_tiles_on_from_the_core() {
        let (far, core) = scene();
        let core_mesh = bevy_symbios_ground::HeightMapMeshBuilder::new()
            .with_normal_method(bevy_symbios_ground::NormalMethod::AreaWeighted)
            .with_uv_tile_size(40.0)
            .build(&core);
        let (core_p, core_n, core_uv) = (
            attribute(&core_mesh, Mesh::ATTRIBUTE_POSITION),
            attribute(&core_mesh, Mesh::ATTRIBUTE_NORMAL),
            uvs(&core_mesh),
        );
        let far_mesh = build_far_mesh(&far, &core);
        let (far_p, far_n, far_uv) = (
            positions(&far_mesh),
            attribute(&far_mesh, Mesh::ATTRIBUTE_NORMAL),
            uvs(&far_mesh),
        );
        let mut shared = 0;
        for (i, p) in core_p.iter().enumerate() {
            // The core's mesh is drawn shifted by half its 40 m extent.
            let (x, z) = (p[0] - 20.0, p[2] - 20.0);
            let Some(j) = far_p.iter().position(|q| q[0] == x && q[2] == z) else {
                continue;
            };
            shared += 1;
            assert_eq!(far_n[j], core_n[i], "the normal at ({x}, {z})");
            assert!(
                (far_uv[j][0] - core_uv[i][0]).abs() < 1e-6
                    && (far_uv[j][1] - core_uv[i][1]).abs() < 1e-6,
                "the UV at ({x}, {z})"
            );
        }
        assert_eq!(shared, 80, "every core boundary vertex");
    }

    /// The far material's weight lookup lands each far pixel centre on its
    /// own texel's centre.
    #[test]
    fn the_far_weight_lookup_lands_on_the_texel_centres() {
        let (far, core) = scene();
        let core_m = (core.width() - 1) as f32 * core.scale();
        let (scale, offset) = far.weight_uv(core_m);
        for i in 0..far.grid {
            let x = -far.side_m() / 2.0 + (i as f32 + 0.5) * far.cell;
            let uv = (x + core_m / 2.0) / core_m;
            let texel = (i as f32 + 0.5) / far.grid as f32;
            assert!((uv * scale + offset - texel).abs() < 1e-5, "pixel {i}");
        }
    }

    #[test]
    fn the_walls_close_the_core_and_stand_clear_of_it() {
        let (_, core) = scene();
        let walls = boundary_walls(&core);
        let (low, high) = core
            .data()
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), &h| (lo.min(h), hi.max(h)));
        for (size, centre) in walls {
            let (min, max) = (centre - size / 2.0, centre + size / 2.0);
            // Below the lowest ground to above the highest.
            assert!(min.y <= low - 50.0 + 1e-3 && max.y >= high + 500.0 - 1e-3);
            // Wholly outside the 40 m core, touching its edge.
            let outside = min.x >= 20.0 - 1e-3
                || max.x <= -20.0 + 1e-3
                || min.z >= 20.0 - 1e-3
                || max.z <= -20.0 + 1e-3;
            assert!(outside, "a wall from {min} to {max} reaches into the core");
            // Long enough to meet the walls across the corners.
            assert!(size.x >= 44.0 - 1e-3 || size.z >= 44.0 - 1e-3);
        }
    }

    #[test]
    fn a_square_the_core_nearly_fills_gets_no_far_field() {
        let cfg = SovereignTerrainConfig::default();
        assert_eq!(cfg.grid_size, 512);
        let square = |size_m| GeoSquare {
            min_e: 391_000,
            min_n: 5_819_500,
            size_m,
        };
        // The default core: 512 points 2 m apart, 1,022 m.
        assert_eq!(
            far_plan(square(1_000), &cfg),
            None,
            "the core is the square"
        );
        // A ring under two far pixels wide: 29 m of 16.9 m pixels.
        assert_eq!(far_plan(square(1_080), &cfg), None);
        // Two pixels: 39 m of 17.2 m pixels.
        let small = far_plan(square(1_100), &cfg).expect("a ring 39 m wide");
        assert_eq!((small.grid, small.core_m), (64, 1_022.0));
        let big = far_plan(square(19_000), &cfg).expect("the largest square");
        // 40 m would be 475 pixels; the cap holds it to 256, 74 m each.
        assert_eq!(big.grid, FAR_GRID_MAX);
        assert!((big.cell - 74.2).abs() < 0.1);
        assert_eq!(
            (big.bbox().min_e, big.bbox().max_e - big.bbox().min_e),
            (391_000, 19_000)
        );
    }

    #[test]
    fn the_far_field_samples_between_its_pixel_centres() {
        let far = FarField::from_fn(4, 10.0, |x, z| x + 100.0 * z);
        // Centres at -15, -5, 5, 15 on each axis.
        assert_eq!(far.height_at(-15.0, -15.0), -15.0 - 1500.0);
        assert!((far.height_at(0.0, 0.0) - 0.0).abs() < 1e-3);
        assert_eq!(far.height_at(-90.0, 90.0), -15.0 + 1500.0, "clamped");
        assert_eq!(far.span_m(), 30.0);
    }
}
