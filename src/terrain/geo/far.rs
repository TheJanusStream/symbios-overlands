//! The far field (#1585, epic #1580): Berlin from the core's edge out to
//! the square's, drawn as the region's horizon.
//!
//! The walkable core is a kilometre or so of street-level ground, and a
//! square may be 19 km across. Beyond the core the square is drawn coarse,
//! from one more render of the terrain and of the land use over the whole
//! square at about 40 m a pixel ([`far_plan`]). It has the city's hills, its
//! land use painted on the region's own ground layers, and its water at the
//! core's level ([`decode_far`]), so the river the core stands by runs on to
//! the horizon. It is walked (P4.1, #1596): it stands on a collider made
//! of its own mesh, and the walls that end the world stand at its edge
//! ([`edge_walls`]), the square's. Round a body past the core a detail
//! patch stands in for it (P4.2, #1597, [`super::patch`]): the far field
//! holds the patch in a slot every clone of it shares
//! ([`FarField::patch`]), its readers read the patch where it lies, its
//! colliders leave out the cells the patch fills whole
//! ([`FarField::colliders`]), and its material cuts the patch's hole in the
//! shader.
//!
//! The far field meets the core without a crack ([`build_far_ground`]). Its
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

use super::patch::PatchSlot;
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

/// How far below the lowest ground the boundary walls reach (m).
const WALL_BELOW_M: f32 = 50.0;

/// How far above the highest ground the boundary walls stand (m): past any
/// jump, and above the cloud deck.
const WALL_ABOVE_M: f32 = 500.0;

/// The collision layer the boundary walls alone belong to. They interact
/// with every layer, so they stop every body; a spatial query that leaves
/// this bit out of its mask - a particle's bounce - passes them by.
pub(crate) const WALL_LAYER: u32 = 1 << 1;

/// On the ground past the core - the far field's colliders, and a detail
/// patch's root (P4.2, #1597): what the pick rays that ask for the ground
/// take as ground beside the core's [`crate::terrain::TerrainMesh`], which
/// neither carries - one terrain is one core (P4.1).
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct FarGround;

/// On each of the far field's own colliders: what a detail patch swaps for
/// colliders with its hole cut, and back again (P4.2, #1597).
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct FarCollider;

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
    /// The detail patch standing in for it round a body past the core, while
    /// one is loaded (P4.2, #1597): read in its place wherever it lies.
    patch: PatchSlot,
}

impl FarField {
    /// The square's side (m): what the far renders cover.
    pub(crate) fn side_m(&self) -> f32 {
        self.grid as f32 * self.cell
    }

    /// The far mesh's extent (m), first pixel centre to last: what a water
    /// plane spanning the far field must cover, and no more - and, the far
    /// field being walked (P4.1), where the walkable world ends.
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

    /// The sky's half-width: past the far field's farthest edge from
    /// anywhere on it - the far field being walked (P4.1, #1596), a viewer
    /// may stand at one edge of the square and look across the whole side.
    pub(crate) fn sky_half_m(&self) -> f32 {
        self.side_m() + SKY_MARGIN_M
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

    /// The detail patch standing in for the far field round a body past the
    /// core (P4.2, #1597): its slot, which every clone of the ground shares.
    pub(crate) fn patch(&self) -> &PatchSlot {
        &self.patch
    }

    /// The ground as drawn at world `(x, z)` past the core `core`, and as
    /// walked: the detail patch's where one has loaded there (P4.2, #1597),
    /// else the far mesh's own triangle ([`Self::mesh_height_at`]). On the
    /// core, the core's own height.
    pub(crate) fn drawn_height_at(&self, core: &HeightMap, x: f32, z: f32) -> f32 {
        let core_half = (core.width() - 1) as f32 * core.scale() / 2.0;
        if x.abs() > core_half || z.abs() > core_half {
            let patched = self.patch.with(|patch| {
                patch
                    .filter(|patch| patch.holds(x, z))
                    .map(|patch| patch.height_at(x, z))
            });
            if let Some(height) = patched {
                return height;
            }
        }
        self.mesh_height_at(core, x, z)
    }

    /// The far mesh's own ground at world `(x, z)` past the core `core`, and
    /// as its colliders walk it (P4.1, #1596): the height of the far mesh's
    /// own triangle there ([`build_far_ground`]), so a body set down on it,
    /// an item snapped to it and the collider under both agree to the
    /// triangle - a far cell is two triangles, not the bilinear patch
    /// [`Self::height_at`] would read, and a cell along the core fans the
    /// core's boundary vertices out. On the core, the core's own height, as
    /// the mesh's hole leaves it; past the drawn edge, the edge's. No
    /// allocation: it is read per body, per frame. A detail patch eases into
    /// it at its edges ([`super::patch::blend`]).
    pub(crate) fn mesh_height_at(&self, core: &HeightMap, x: f32, z: f32) -> f32 {
        let core_half = (core.width() - 1) as f32 * core.scale() / 2.0;
        if x.abs() <= core_half && z.abs() <= core_half {
            let extent = 2.0 * core_half;
            return core.get_height_at(
                (x + core_half).clamp(0.0, extent),
                (z + core_half).clamp(0.0, extent),
            );
        }
        let edge = self.span_m() / 2.0;
        let (x, z) = (x.clamp(-edge, edge), z.clamp(-edge, edge));
        let at = Vec2::new(x, z);
        // A fan along the core reaches a hair past its own cell, to the core
        // vertex nearest its far corner: the cells round it are asked too.
        let (xs, nx) = self.cells_round(core_half, x);
        let (zs, nz) = self.cells_round(core_half, z);
        for &xs in &xs[..nx] {
            for &zs in &zs[..nz] {
                if let Some(height) = self.height_in_cell(core, core_half, xs, zs, at) {
                    return height;
                }
            }
        }
        self.height_at(x, z)
    }

    /// The ground's upward normal as drawn at world `(x, z)` past the core
    /// `core`: the detail patch's own where one has loaded there (P4.2),
    /// read as the core's is; elsewhere the slope of
    /// [`Self::drawn_height_at`] a metre either way, the core's own heights
    /// where a step reaches onto it.
    pub(crate) fn drawn_normal_at(&self, core: &HeightMap, x: f32, z: f32) -> Vec3 {
        let patched = self.patch.with(|patch| {
            patch
                .filter(|patch| patch.holds(x, z))
                .map(|patch| patch.normal_at(x, z))
        });
        if let Some(normal) = patched {
            return normal;
        }
        let at = |x: f32, z: f32| self.drawn_height_at(core, x, z);
        let (dx, dz) = (
            at(x + 1.0, z) - at(x - 1.0, z),
            at(x, z + 1.0) - at(x, z - 1.0),
        );
        Vec3::new(-dx, 2.0, -dz).normalize()
    }

    /// The land use at world `(x, z)` past the core: the detail patch's
    /// where one has loaded there (P4.2), else that of the far pixel it
    /// lies in.
    pub(crate) fn cover_at(&self, x: f32, z: f32) -> Option<LandUse> {
        let patched = self.patch.with(|patch| {
            patch
                .filter(|patch| patch.holds(x, z))
                .map(|patch| patch.cover_at(x, z))
        });
        if let Some(cover) = patched {
            return cover;
        }
        let half = self.side_m() / 2.0;
        let last = self.grid as usize - 1;
        let pixel = |v: f32| (((v + half) / self.cell).floor().max(0.0) as usize).min(last);
        self.cover[pixel(z) * self.grid as usize + pixel(x)]
    }

    /// The splat layer weights at world `(x, z)` past the core, as drawn:
    /// the detail patch's where one has loaded there (P4.2), else the far
    /// mesh's, all on the layer of its pixel's land use.
    pub(crate) fn weights_at(&self, x: f32, z: f32) -> [f32; 4] {
        let patched = self.patch.with(|patch| {
            patch
                .filter(|patch| patch.holds(x, z))
                .map(|patch| patch.weights_at(x, z))
        });
        if let Some(weights) = patched {
            return weights;
        }
        let mut weights = [0.0; 4];
        weights[super::ground::layer(self.cover_at(x, z))] = 1.0;
        weights
    }

    /// The far mesh's grid lines along one axis within `[lo, hi]`, as
    /// [`grid_lines`] draws them - the pixel centres, less those within a
    /// quarter pixel of a core edge, and the core's edges - found without
    /// drawing them all; ascending.
    pub(crate) fn lines_within(
        &self,
        core_half: f32,
        lo: f32,
        hi: f32,
    ) -> impl Iterator<Item = f32> + '_ {
        let half = self.grid as f32 * self.cell / 2.0;
        let index = |v: f32| ((v + half) / self.cell - 0.5).floor() as i64;
        let (first, last) = (
            index(lo).max(0),
            (index(hi) + 1).min(i64::from(self.grid) - 1),
        );
        let centres = (first..=last)
            .map(move |k| -half + (k as f32 + 0.5) * self.cell)
            .filter(move |line| (line.abs() - core_half).abs() >= self.cell / 4.0);
        let edges = [-core_half, core_half].into_iter();
        let mut merged: Vec<f32> = centres
            .chain(edges)
            .filter(|line| (lo..=hi).contains(line))
            .collect();
        merged.sort_by(f32::total_cmp);
        merged.dedup();
        merged.into_iter()
    }

    /// Where the far mesh's cells inside `rect` - world `(x, z)` - begin
    /// whole, from each of its edges: the first of its grid lines
    /// ([`grid_lines`]) at or inside its west, east, north and south edges,
    /// in that order. Between an edge and its line lie the far cells the
    /// edge crosses, which keep their colliders while a detail patch fills
    /// `rect` (P4.2, #1597); an edge on a grid line crosses none.
    pub(crate) fn inner_lines(&self, core_half: f32, rect: bevy::math::Rect) -> [f32; 4] {
        let first = |lo: f32, hi: f32| self.lines_within(core_half, lo, hi).next().unwrap_or(lo);
        let last = |lo: f32, hi: f32| self.lines_within(core_half, lo, hi).last().unwrap_or(hi);
        [
            first(rect.min.x, rect.max.x),
            last(rect.min.x, rect.max.x),
            first(rect.min.y, rect.max.y),
            last(rect.min.y, rect.max.y),
        ]
    }

    /// The far mesh's grid intervals along one axis round `v` - the one it
    /// lies in and its neighbours - and how many there are, from the lines
    /// [`grid_lines`] draws, found without drawing them all or allocating.
    fn cells_round(&self, core_half: f32, v: f32) -> ([(f32, f32); 3], usize) {
        let half = self.grid as f32 * self.cell / 2.0;
        let i = ((v + half) / self.cell - 0.5).floor() as i64;
        // Eight pixel centres round `v` and the two core edges.
        let mut lines = [0.0_f32; 10];
        let mut n = 0;
        for k in i - 3..=i + 4 {
            if !(0..i64::from(self.grid)).contains(&k) {
                continue;
            }
            let line = -half + (k as f32 + 0.5) * self.cell;
            if (line.abs() - core_half).abs() >= self.cell / 4.0 {
                lines[n] = line;
                n += 1;
            }
        }
        for line in [-core_half, core_half] {
            lines[n] = line;
            n += 1;
        }
        let lines = &mut lines[..n];
        lines.sort_by(f32::total_cmp);
        let mut kept = 0;
        for j in 0..lines.len() {
            if kept == 0 || lines[j] != lines[kept - 1] {
                lines[kept] = lines[j];
                kept += 1;
            }
        }
        let lines = &lines[..kept];
        let mut cells = [(0.0, 0.0); 3];
        let Some(k) = lines.windows(2).position(|w| w[0] <= v && v <= w[1]) else {
            return (cells, 0);
        };
        let mut count = 0;
        for j in k.saturating_sub(1)..(k + 2).min(lines.len() - 1) {
            cells[count] = (lines[j], lines[j + 1]);
            count += 1;
        }
        (cells, count)
    }

    /// The height at `at` of whichever triangle the far mesh draws over the
    /// cell `(x0, x1)` by `(z0, z1)` holds it ([`Self::cell_triangles`]);
    /// `None` where none does.
    fn height_in_cell(
        &self,
        core: &HeightMap,
        core_half: f32,
        xs: (f32, f32),
        zs: (f32, f32),
        at: Vec2,
    ) -> Option<f32> {
        self.cell_triangles(core, core_half, xs, zs, |[a, b, c]| height_in(at, a, b, c))
    }

    /// The triangles the far mesh draws over the cell `(x0, x1)` by
    /// `(z0, z1)`, as [`build_far_ground`] draws them, handed to `each` in
    /// turn until it answers: none inside the core; along a core edge, the
    /// core's boundary vertices fanned out to the cell's two far corners;
    /// elsewhere two triangles split from `(x1, z0)` to `(x0, z1)`, a corner
    /// on a core corner being the core's own vertex. Their corners come in
    /// the mesh's order, before it turns each to face up.
    fn cell_triangles<T>(
        &self,
        core: &HeightMap,
        core_half: f32,
        (x0, x1): (f32, f32),
        (z0, z1): (f32, f32),
        mut each: impl FnMut([Vec3; 3]) -> Option<T>,
    ) -> Option<T> {
        let (width, depth, scale) = (core.width(), core.height(), core.scale());
        let inside_x = ((x0 + x1) / 2.0).abs() < core_half;
        let inside_z = ((z0 + z1) / 2.0).abs() < core_half;
        if inside_x && inside_z {
            return None;
        }
        let far = |x: f32, z: f32| Vec3::new(x, self.height_at(x, z), z);
        let boundary = |edge: Edge, k: usize| {
            let (cx, cz) = edge.vertex(k, width, depth);
            Vec3::new(
                cx as f32 * scale - core_half,
                core.get(cx, cz),
                cz as f32 * scale - core_half,
            )
        };
        let along_core = if inside_x && z1 == -core_half {
            Some((Edge::North, x0, x1, far(x0, z0), far(x1, z0)))
        } else if inside_x && z0 == core_half {
            Some((Edge::South, x0, x1, far(x0, z1), far(x1, z1)))
        } else if inside_z && x1 == -core_half {
            Some((Edge::West, z0, z1, far(x0, z0), far(x0, z1)))
        } else if inside_z && x0 == core_half {
            Some((Edge::East, z0, z1, far(x1, z0), far(x1, z1)))
        } else {
            None
        };
        let Some((edge, from, to, o0, o1)) = along_core else {
            let vertex = |x: f32, z: f32| {
                if x.abs() == core_half && z.abs() == core_half {
                    let edge = if z < 0.0 { Edge::North } else { Edge::South };
                    boundary(edge, if x < 0.0 { 0 } else { width - 1 })
                } else {
                    far(x, z)
                }
            };
            let (p00, p10, p01, p11) = (
                vertex(x0, z0),
                vertex(x1, z0),
                vertex(x0, z1),
                vertex(x1, z1),
            );
            return each([p00, p01, p10]).or_else(|| each([p10, p01, p11]));
        };
        let count = edge.count(width, depth);
        let nearest =
            |along: f32| (((along + core_half) / scale).round().max(0.0) as usize).min(count - 1);
        let (first, last) = (nearest(from), nearest(to));
        let middle = first + (last - first) / 2;
        for k in first..last {
            let apex = if k < middle { o0 } else { o1 };
            if let Some(answer) = each([apex, boundary(edge, k), boundary(edge, k + 1)]) {
                return Some(answer);
            }
        }
        each([o0, boundary(edge, middle), o1])
    }

    /// The colliders the far field is walked on (P4.1, #1596): the far
    /// mesh's triangles exactly, in two parts. The plain cells - between
    /// four pixel centres, off the core - as a heightfield over the pixel
    /// grid, whose split is the mesh's; and the cells the core's edges
    /// cross or bend (the core edges' lines run through the whole square,
    /// and the centres within a quarter pixel of them are dropped), with
    /// the fans along the core, as a small triangle mesh. A triangle mesh
    /// of it all keeps 34 MB at the largest square (256 pixels round a
    /// 512-point core), its internal-edge fix the most of it; the two keep
    /// about a megabyte. Both take parry's internal-edge fix, as the
    /// core's heightfield does (#1538). Empty where parry refuses either -
    /// the far field is then drawn, not walked.
    ///
    /// With a `hole` - world `(x, z)` - the cells wholly inside it are left
    /// out: a detail patch stands there on a collider of its own (P4.2,
    /// #1597). The cells its edges cross keep theirs, and the patch draws
    /// their ground there ([`FarField::inner_lines`]).
    pub(crate) fn colliders(
        &self,
        core: &HeightMap,
        hole: Option<bevy::math::Rect>,
    ) -> Vec<avian3d::prelude::Collider> {
        use avian3d::parry::shape::{
            HeightField, HeightFieldCellStatus, HeightFieldFlags, SharedShape,
        };
        use avian3d::parry::utils::Array2;
        use avian3d::prelude::{Collider, TrimeshFlags};
        let core_half = (core.width() - 1) as f32 * core.scale() / 2.0;
        let grid = self.grid as usize;
        let half = self.grid as f32 * self.cell / 2.0;
        let centre = |i: usize| -half + (i as f32 + 0.5) * self.cell;
        let kept = |line: f32| (line.abs() - core_half).abs() >= self.cell / 4.0;
        // A pixel interval the mesh draws as one cell: both ends kept, no
        // core edge between them.
        let plain: Vec<bool> = (0..grid - 1)
            .map(|i| {
                let (a, b) = (centre(i), centre(i + 1));
                kept(a)
                    && kept(b)
                    && !(a < -core_half && -core_half < b)
                    && !(a < core_half && core_half < b)
            })
            .collect();
        let inside = |i: usize| centre(i).abs() < core_half && centre(i + 1).abs() < core_half;
        // A cell wholly inside the hole.
        let holed = |(x0, x1): (f32, f32), (z0, z1): (f32, f32)| {
            hole.is_some_and(|hole| {
                hole.min.x <= x0 && x1 <= hole.max.x && hole.min.y <= z0 && z1 <= hole.max.y
            })
        };

        // The plain cells, on the heightfield: column-major, rows along Z.
        let mut heights = Vec::with_capacity(grid * grid);
        for x in 0..grid {
            for z in 0..grid {
                heights.push(self.heights[z * grid + x]);
            }
        }
        let span = (grid - 1) as f32 * self.cell;
        let mut field = HeightField::with_flags(
            Array2::new(grid, grid, heights),
            Vec3::new(span, 1.0, span),
            HeightFieldFlags::FIX_INTERNAL_EDGES,
        );
        for z in 0..grid - 1 {
            for x in 0..grid - 1 {
                if !(plain[z] && plain[x])
                    || (inside(z) && inside(x))
                    || holed((centre(x), centre(x + 1)), (centre(z), centre(z + 1)))
                {
                    field.set_cell_status(z, x, HeightFieldCellStatus::CELL_REMOVED);
                }
            }
        }

        // The rest, as the mesh draws them, each turned to face up.
        let lines = grid_lines(self, core_half);
        let is_plain = |a: f32, b: f32| {
            let i = ((a + half) / self.cell - 0.5).round();
            i >= 0.0
                && (i as usize) < grid - 1
                && centre(i as usize) == a
                && centre(i as usize + 1) == b
                && plain[i as usize]
        };
        let mut vertices: Vec<Vec3> = Vec::new();
        let mut triangles: Vec<[u32; 3]> = Vec::new();
        for zs in lines.windows(2) {
            for xs in lines.windows(2) {
                if (is_plain(xs[0], xs[1]) && is_plain(zs[0], zs[1]))
                    || holed((xs[0], xs[1]), (zs[0], zs[1]))
                {
                    continue;
                }
                self.cell_triangles(
                    core,
                    core_half,
                    (xs[0], xs[1]),
                    (zs[0], zs[1]),
                    |[a, b, c]| {
                        let turn = (b.x - a.x) * (c.z - a.z) - (b.z - a.z) * (c.x - a.x);
                        let corners = if turn < 0.0 {
                            [a, b, c]
                        } else if turn > 0.0 {
                            [a, c, b]
                        } else {
                            return None::<()>;
                        };
                        let first = vertices.len() as u32;
                        vertices.extend(corners);
                        triangles.push([first, first + 1, first + 2]);
                        None
                    },
                );
            }
        }
        let mut colliders = vec![Collider::from(SharedShape::new(field))];
        if !triangles.is_empty() {
            let rest = Collider::try_trimesh_with_config(
                vertices.clone(),
                triangles.clone(),
                TrimeshFlags::FIX_INTERNAL_EDGES,
            )
            .or_else(|error| {
                warn!("geodata: the far field's edges take no internal-edge fix ({error:?})");
                Collider::try_trimesh(vertices, triangles)
            });
            match rest {
                Ok(rest) => colliders.push(rest),
                Err(error) => {
                    warn!(
                        "geodata: the far field makes no collider ({error:?}) - it is drawn, not walked"
                    );
                    return Vec::new();
                }
            }
        }
        colliders
    }

    /// The height at world `(x, z)`, between the pixel centres round it, the
    /// world centring the square on the origin. Clamped to the outermost
    /// centres. The far mesh's vertices stand at it; between them the mesh
    /// is triangles ([`Self::drawn_height_at`]).
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

/// The height at `at` of the triangle `(a, b, c)` - in `(x, z)` and up -
/// where `at` lies on it, a whisker of rounding allowed; `None` off it, or
/// for a triangle with no area.
fn height_in(at: Vec2, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (a2, b2, c2) = (a.xz(), b.xz(), c.xz());
    let area = (b2 - a2).perp_dot(c2 - a2);
    if area.abs() < 1e-9 {
        return None;
    }
    let s = (at - a2).perp_dot(c2 - a2) / area;
    let t = (b2 - a2).perp_dot(at - a2) / area;
    const EPS: f32 = 1e-5;
    (s >= -EPS && t >= -EPS && s + t <= 1.0 + EPS)
        .then_some(a.y + s * (b.y - a.y) + t * (c.y - a.y))
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
        patch: PatchSlot::default(),
    })
}

/// The far field's mesh and the collider it stands on.
///
/// The mesh is the square minus the core, in world coordinates round the
/// origin as the core is drawn, with no CPU copy. Its grid lines are the far
/// renders' pixel centres and the core's four edges. A cell whose edge lies
/// along the core takes the core's boundary vertices on that edge - from the
/// one nearest each of its corners, so no far vertex sits partway along a
/// core edge - at the core's height, and fans them out to its far corners:
/// the two meshes share their boundary vertex for vertex. There the far mesh
/// also takes the core's own normals, so the light runs on across the seam.
/// All other cells are two triangles, and every triangle faces up. Its UVs
/// are the core's mapping run on past the core's edges, so the layers' tiles
/// run on across the seam; [`FarField::weight_uv`] turns them into the far
/// weight map's.
///
/// The colliders (P4.1, #1596; [`FarField::colliders`]) are the same
/// triangles, so the ground walked is the ground drawn, the core's hole in
/// it and the seam to the core's boundary vertex for vertex. They take
/// parry's internal-edge fix, as the core's heightfield does (#1538):
/// without it a wheel crossing from one triangle to the next can meet the
/// edge between them as a wall. Where the triangles make no collider at
/// all (parry refuses a degenerate mesh) there are none, with a warning:
/// the far field is then drawn and not walked, the walls standing at the
/// core's edge ([`core_walls`]) as they did before.
pub(crate) fn build_far_ground(
    far: &FarField,
    core: &HeightMap,
) -> (Mesh, Vec<avian3d::prelude::Collider>) {
    (
        far_mesh(far, core).into_mesh(core),
        far.colliders(core, None),
    )
}

/// The far field's mesh alone, as [`build_far_ground`] draws it.
#[cfg(test)]
pub(crate) fn build_far_mesh(far: &FarField, core: &HeightMap) -> Mesh {
    far_mesh(far, core).into_mesh(core)
}

/// The far field's triangles, before they are a [`Mesh`].
fn far_mesh(far: &FarField, core: &HeightMap) -> FarMesh {
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
    mesh
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
pub(super) fn core_normal(core: &HeightMap, x: usize, z: usize) -> Vec3 {
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

/// The four invisible walls that end a Berlin region's world, at the far
/// field's edge - the square's, where its drawn ground ends - since the far
/// field is walked (P4.1, #1596): `(size, centre)` of each cuboid, just
/// outside that edge, from [`WALL_BELOW_M`] under the lowest ground, core
/// or far, to [`WALL_ABOVE_M`] over the highest.
pub(crate) fn edge_walls(far: &FarField, core: &HeightMap) -> [(Vec3, Vec3); 4] {
    walls_round(far.span_m() / 2.0, core.data().iter().chain(&far.heights))
}

/// The walls round the core alone, just outside its edges: where the far
/// field is drawn and cannot be walked - its triangles made no collider -
/// the world ends at the core, as it did before P4.1.
pub(crate) fn core_walls(core: &HeightMap) -> [(Vec3, Vec3); 4] {
    walls_round(
        (core.width() - 1) as f32 * core.scale() / 2.0,
        core.data().iter(),
    )
}

/// Four walls just outside `half` of the origin either way, from
/// [`WALL_BELOW_M`] under the lowest of `heights` to [`WALL_ABOVE_M`] over
/// the highest: `(size, centre)` of each cuboid.
fn walls_round<'a>(half: f32, heights: impl Iterator<Item = &'a f32>) -> [(Vec3, Vec3); 4] {
    let (low, high) = heights.fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &h| {
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
            patch: PatchSlot::default(),
        }
    }

    /// Set each pixel's land use to `cover(x, z)` at its centre.
    pub(crate) fn set_cover(&mut self, cover: impl Fn(f32, f32) -> Option<LandUse>) {
        let half = self.grid as f32 * self.cell / 2.0;
        let centre = |i: u32| -half + (i as f32 + 0.5) * self.cell;
        for i in 0..self.grid * self.grid {
            self.cover[i as usize] = cover(centre(i % self.grid), centre(i / self.grid));
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

    /// The walls end the world at the far field's edge (P4.1): wholly
    /// outside its drawn ground (pixel centres 75 m out), touching it, from
    /// below the lowest ground, core or far, to above the highest.
    #[test]
    fn the_walls_close_the_far_field_and_stand_clear_of_it() {
        let (far, core) = scene();
        let walls = edge_walls(&far, &core);
        let (low, high) = core
            .data()
            .iter()
            .chain(&far.heights)
            .fold((f32::MAX, f32::MIN), |(lo, hi), &h| (lo.min(h), hi.max(h)));
        let edge = far.span_m() / 2.0;
        assert_eq!(edge, 75.0);
        for (size, centre) in walls {
            let (min, max) = (centre - size / 2.0, centre + size / 2.0);
            assert!(min.y <= low - 50.0 + 1e-3 && max.y >= high + 500.0 - 1e-3);
            let outside = min.x >= edge - 1e-3
                || max.x <= -edge + 1e-3
                || min.z >= edge - 1e-3
                || max.z <= -edge + 1e-3;
            assert!(
                outside,
                "a wall from {min} to {max} reaches onto the ground"
            );
            // Long enough to meet the walls across the corners.
            assert!(size.x >= 2.0 * edge + 4.0 - 1e-3 || size.z >= 2.0 * edge + 4.0 - 1e-3);
        }
    }

    /// The ground as drawn is the far mesh's own triangles (P4.1): sampled
    /// at random points over the far field, it reads what a brute-force
    /// search of the built mesh does - in the plain cells, the fans along
    /// the core and the cells at its corners - on a core whose edge falls
    /// clear of the pixel centres and on one that drops a line within a
    /// quarter pixel of it.
    #[test]
    fn the_ground_as_drawn_is_the_far_meshs_triangles() {
        for core_points in [21_usize, 25] {
            let far = FarField::from_fn(16, 10.0, |x, z| {
                30.0 + 0.05 * x - 0.02 * z + 3.0 * (x * 0.07).sin() * (z * 0.05).cos()
            });
            let mut core = HeightMap::new(core_points, core_points, 2.0);
            let half = (core_points - 1) as f32;
            for z in 0..core_points {
                for x in 0..core_points {
                    let (wx, wz) = (x as f32 * 2.0 - half, z as f32 * 2.0 - half);
                    core.set(
                        x,
                        z,
                        27.0 + 0.05 * wx - 0.02 * wz + 0.3 * ((x * 7 + z * 3) % 5) as f32,
                    );
                }
            }
            let mesh = build_far_mesh(&far, &core);
            let (p, tris) = (positions(&mesh), triangles(&mesh));
            let brute = |x: f32, z: f32| {
                tris.iter().find_map(|t| {
                    let [a, b, c] = t.map(|i| Vec3::from(p[i]));
                    height_in(Vec2::new(x, z), a, b, c)
                })
            };
            let mut seed = 0x2545_f491_4f6c_dd1d_u64;
            let mut next = || {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                (seed >> 11) as f32 / (1u64 << 53) as f32
            };
            let mut sampled = 0;
            while sampled < 2_000 {
                let (x, z) = (next() * 150.0 - 75.0, next() * 150.0 - 75.0);
                if x.abs() <= half && z.abs() <= half {
                    continue;
                }
                let drawn = far.drawn_height_at(&core, x, z);
                let truth = brute(x, z).expect("the far mesh covers the square less the core");
                assert!(
                    (drawn - truth).abs() < 1e-3,
                    "core {core_points}: ({x}, {z}) drawn {drawn}, mesh {truth}"
                );
                sampled += 1;
            }
        }
    }

    /// The far field stands on colliders of its own triangles (P4.1): a
    /// body comes down on them where the mesh is drawn, at the drawn height,
    /// in a plain cell, on a fan along the core and in a cell a core edge's
    /// line crosses; and through the core's hole onto nothing of them, the
    /// core's own heightfield being the ground there. So on a core whose
    /// edge falls clear of the pixel centres and on one that drops a line.
    #[test]
    fn the_far_field_is_walked_on_its_drawn_triangles() {
        for core_points in [21_usize, 25] {
            let far = FarField::from_fn(16, 10.0, |x, z| {
                30.0 + 0.05 * x - 0.02 * z + 3.0 * (x * 0.07).sin() * (z * 0.05).cos()
            });
            let mut core = HeightMap::new(core_points, core_points, 2.0);
            let half = (core_points - 1) as f32;
            for z in 0..core_points {
                for x in 0..core_points {
                    core.set(x, z, 27.0 + 0.3 * ((x * 7 + z * 3) % 5) as f32);
                }
            }
            let (mesh, colliders) = build_far_ground(&far, &core);
            assert_eq!(colliders.len(), 2, "the plain cells and the rest");
            assert_eq!(
                positions(&mesh).len(),
                positions(&build_far_mesh(&far, &core)).len()
            );
            let down = |x: f32, z: f32| {
                colliders
                    .iter()
                    .filter_map(|collider| {
                        collider.cast_ray(
                            Vec3::ZERO,
                            Quat::IDENTITY,
                            Vec3::new(x, 500.0, z),
                            Vec3::NEG_Y,
                            1_000.0,
                            true,
                        )
                    })
                    .map(|(distance, _)| 500.0 - distance)
                    .reduce(f32::max)
            };
            let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
            let mut next = || {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                (seed >> 11) as f32 / (1u64 << 53) as f32
            };
            let mut sampled = 0;
            while sampled < 2_000 {
                let (x, z) = (next() * 149.0 - 74.5, next() * 149.0 - 74.5);
                if x.abs() <= half + 0.01 && z.abs() <= half + 0.01 {
                    continue;
                }
                let drawn = far.drawn_height_at(&core, x, z);
                let walked = down(x, z).expect("the far field is ground everywhere it is drawn");
                assert!(
                    (walked - drawn).abs() < 1e-3,
                    "core {core_points}: ({x}, {z}) walked {walked}, drawn {drawn}"
                );
                sampled += 1;
            }
            // Inside the core: no far ground at all; past the edge, nothing.
            assert_eq!(down(3.0, -7.0), None);
            assert_eq!(down(79.0, 0.0), None);
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
