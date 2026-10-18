//! The grass field's tiles (#1675): square patches of blades laid on a
//! world grid around the camera, and the meshes every tile draws one of.
//!
//! **Every tile is the same mesh.** A blade's vertices carry where it
//! stands in its tile, how far up the blade they are and which edge, and
//! two numbers of its own: its *rank* and its *seed*. The vertex shader
//! (`assets/shaders/grass.wgsl`) does the rest - it stands the blade on the
//! ground under it, lets it grow as thickly as the ground there grows
//! grass, and shapes, bends and colours it - so a few meshes serve every
//! tile of every room, and the tiles batch into a draw a mesh.
//!
//! **Rank is what makes density cheap.** The blades are laid in an order
//! whose every prefix is spread evenly over the tile ([`roots`]), and a
//! blade's rank is its place in that order. Ground that grows grass at half
//! density keeps the blades ranked below a half, and those are an even
//! half of the tile, not a clump of it. The shader thins the field with
//! distance by the same test, smoothly. So a tile needs only the blades
//! ranked below the most the shader can keep anywhere in it (#1693): the
//! thickest its ground grows ([`most_density`]) times the thinning at its
//! nearest point ([`thinning`]). Its mesh is the prefix of the near one's
//! that holds them - one of [`SHARES`] - with fewer segments a blade the
//! further it is ([`Detail`]), and a tile where nothing can grow is not
//! laid at all. A blade the shader does not grow still costs its vertices
//! a run through the shader, so this is where the field's cost is cut.
//!
//! **The reach is cheap** (#1694): past the far share the field thins as the
//! square of the distance ([`thinning`]), so a patch of screen holds as
//! many blades however far off it lies, the shader widening each to cover.
//! Far off, a tile is 16 or 32 m ([`TileKey`]): its 8 m cells side by side,
//! each drawn as an 8 m tile there would be, so the field reads the same in
//! tiles of any size, in a third as many of them. A blade's pairs stand at
//! its detail's rungs ([`Detail::rungs`]), which the shader moves onto the
//! next detail's lines before its tile turns to it.
//!
//! **Tiles repeat**, eight metres apart, so the shader turns and mirrors
//! each 8 m cell's layout by a hash of where it lies, and draws each
//! blade's shape from its world position, not its index. Neighbours turned
//! apart no longer continue each other's sequence across their edge, so the
//! blades along a seam are spread about as evenly as random places are -
//! which no one standing in a field can tell from the rest of it.

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy_symbios_ground::HeightMap;

use crate::config::grass as cfg;

/// How finely a tile's blades are drawn, by how far it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Detail {
    /// Three segments a blade.
    Near,
    /// Two segments a blade: the tile lies past where the field is thinned
    /// to [`cfg::MID_SHARE`].
    Mid,
    /// One triangle a blade: past where it is thinned to
    /// [`cfg::FAR_SHARE`].
    Far,
}

impl Detail {
    /// Near to far, as [`Self::index`] numbers them.
    pub const ALL: [Self; 3] = [Self::Near, Self::Mid, Self::Far];

    /// The most of a near tile's blades a tile at this detail needs: the
    /// share the field is thinned to where the detail begins.
    pub fn share(self) -> f32 {
        match self {
            Self::Near => 1.0,
            Self::Mid => cfg::MID_SHARE,
            Self::Far => cfg::FAR_SHARE,
        }
    }

    /// Segments a blade has at this detail.
    pub fn segments(self) -> u32 {
        self.rungs().len() as u32
    }

    /// How far up its blade (0 to 1) each pair of a blade's vertices stands
    /// at this detail, root first; the tip stands at 1, over the last. A
    /// middle blade's upper pair stands where a near one's upper pair does
    /// (#1694), so a near blade whose third-way pair is moved onto the line
    /// between the others is the middle blade exactly - `grass.wgsl`'s
    /// geomorph, which a test pins to these.
    pub fn rungs(self) -> &'static [f32] {
        match self {
            Self::Near => &NEAR_RUNGS,
            Self::Mid => &MID_RUNGS,
            Self::Far => &FAR_RUNGS,
        }
    }

    /// Its place in [`Self::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// The detail a tile needs whose nearest point lies `nearest` metres
    /// from the camera, in field `field`. The shader thins the field to the
    /// middle share by [`cfg::MID_AT`] times its full distance and to the
    /// far share by [`cfg::FAR_AT`] times it, so a tile wholly past one of
    /// them misses nothing by drawing that share.
    pub fn at(nearest: f32, field: &cfg::Field) -> Self {
        if nearest < cfg::MID_AT * field.full {
            Self::Near
        } else if nearest < cfg::FAR_AT * field.full {
            Self::Mid
        } else {
            Self::Far
        }
    }
}

/// [`Detail::rungs`], near to far.
const NEAR_RUNGS: [f32; 3] = [0.0, 1.0 / 3.0, 2.0 / 3.0];
const MID_RUNGS: [f32; 2] = [0.0, 2.0 / 3.0];
const FAR_RUNGS: [f32; 1] = [0.0];

/// The shares of a near tile's blades a tile's mesh can hold, most to
/// fewest (#1693): a tile draws the fewest that holds every blade the shader
/// can grow in it ([`share_for`]). Each is three fifths to four fifths of
/// the one before, and the middle and far details' own shares are among them, so
/// a tile at either never draws more than the detail's whole share did.
pub const SHARES: [f32; 13] = [
    1.0, 0.7, 0.5, 0.4, 0.3, 0.22, 0.15, 0.1, 0.065, 0.04, 0.025, 0.016, 0.01,
];

/// The index in [`SHARES`] a tile needs whose blades the shader keeps up to
/// rank `need`: the fewest that holds them all, the last for fewer still,
/// and `None` when it keeps none.
pub fn share_for(need: f32) -> Option<usize> {
    if need.is_nan() || need <= 0.0 {
        return None;
    }
    Some(SHARES.iter().rposition(|&share| share >= need).unwrap_or(0))
}

/// The share of a near tile's blades the shader keeps `distance` metres
/// from the camera, in field `field`: `grass.wgsl`'s `thinning`, which
/// keeps them all out to the field's full distance, eases to
/// [`cfg::MID_SHARE`] by [`cfg::MID_AT`] times it and to
/// [`cfg::FAR_SHARE`] by [`cfg::FAR_AT`] times it, and past that falls as
/// the square of the distance (#1694) - as many blades a patch of screen
/// however far it lies, which is what keeps a long reach cheap: every
/// doubling of it costs about as much as the band before.
pub fn thinning(distance: f32, field: &cfg::Field) -> f32 {
    let smooth = |from: f32, to: f32, x: f32| {
        let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let (full, mid, far) = (
        field.full,
        cfg::MID_AT * field.full,
        cfg::FAR_AT * field.full,
    );
    if distance < mid {
        1.0 + (cfg::MID_SHARE - 1.0) * smooth(full, mid, distance)
    } else if distance < far {
        cfg::MID_SHARE + (cfg::FAR_SHARE - cfg::MID_SHARE) * smooth(mid, far, distance)
    } else {
        let past = far / distance;
        cfg::FAR_SHARE * past * past
    }
}

/// The most the shader's density reaches on ground whose layers' shares
/// reach at most `shares`, the layers growing grass as thickly as
/// `layer_density`, under cover at most `cover` (`grass.wgsl`'s
/// `ground_density` and its clumps): the layers' density, and on thin
/// ground a clump at its thickest - [`cfg::CLUMP_PEAK`] times the ground's
/// own density by [`cfg::CLUMP_SHARE`] of how thin it is. The clump's gain
/// shrinks as the ground thickens, so past the thickness where the two
/// balance the most is taken there.
pub fn most_density(shares: [f32; 4], layer_density: [f32; 4], cover: f32) -> f32 {
    let ground: f32 = shares
        .iter()
        .zip(layer_density)
        .map(|(share, density)| share.max(0.0) * density.clamp(0.0, 1.0))
        .sum();
    let ground = ground.min(1.0) * cover.clamp(0.0, 1.0);
    let gain = (cfg::CLUMP_PEAK - 1.0) * cfg::CLUMP_SHARE;
    let ground = if gain > 0.0 {
        ground.min((1.0 + gain) / (2.0 * gain))
    } else {
        ground
    };
    ground * (1.0 + gain * (1.0 - ground))
}

/// Where a tile lies (#1694): its size level - 0 an 8 m tile, 1 a 16 m
/// one, 2 a 32 m one ([`MAX_LEVEL`]) - and its low corner's place on that
/// level's grid. A larger tile is its 8 m cells' blades side by side, each
/// cell drawn as an 8 m tile there would be, so a stretch of the field laid
/// in larger tiles draws what it drew in small ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileKey {
    pub level: u32,
    pub coord: IVec2,
}

/// The largest tile's level: 32 m.
pub const MAX_LEVEL: u32 = 2;

/// The 8 m cells along a side of the largest tile, by which `grass.wgsl`
/// numbers a tile's cells ([`tile_mesh`]).
pub const CELLS_A_ROW: u32 = 1 << MAX_LEVEL;

impl TileKey {
    /// The 8 m tile at `coord`.
    pub fn small(coord: IVec2) -> Self {
        Self { level: 0, coord }
    }

    /// The tile's side (m).
    pub fn side(self) -> f32 {
        cfg::TILE_M * (1u32 << self.level) as f32
    }

    /// Its low corner, world XZ.
    pub fn lo(self) -> Vec2 {
        self.coord.as_vec2() * self.side()
    }

    /// The 8 m tiles it is made of.
    pub fn cells(self) -> impl Iterator<Item = IVec2> {
        let n = 1i32 << self.level;
        let base = self.coord * n;
        (0..n).flat_map(move |j| (0..n).map(move |i| base + IVec2::new(i, j)))
    }

    /// Its four quarters, a level down; none for an 8 m tile.
    pub fn quarters(self) -> impl Iterator<Item = Self> {
        let below = self.level.checked_sub(1);
        below.into_iter().flat_map(move |level| {
            [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(i, j)| Self {
                level,
                coord: self.coord * 2 + IVec2::new(i, j),
            })
        })
    }
}

/// Blades in a near tile at a quality whose density is `density`, a share
/// of [`cfg::BLADES_PER_M2`].
pub fn blade_count(density: f32) -> usize {
    (cfg::TILE_M * cfg::TILE_M * cfg::BLADES_PER_M2 * density).round() as usize
}

/// Where the blades of a tile of `count` stand, as shares of the tile's
/// side: Mitchell's best-candidate sequence on the torus. Each blade takes
/// the best of [`CANDIDATES`] random places, the one furthest from every
/// blade before it, so every prefix of the sequence is spread as evenly as
/// a Poisson-disc set - without the rows of a lattice, which a field reads
/// as rows from any distance, or the clumps and holes of random places -
/// and as evenly across the tile's edges as inside it, distances being
/// measured round the torus.
pub(crate) fn roots(count: usize) -> Vec<Vec2> {
    let side = ((count as f32).sqrt().ceil() as usize).max(1);
    let mut grid: Vec<Vec<u32>> = vec![Vec::new(); side * side];
    let mut points: Vec<Vec2> = Vec::with_capacity(count);
    for i in 0..count {
        let mut best = (Vec2::ZERO, -1.0f32);
        for c in 0..CANDIDATES {
            let place = Vec2::new(hash(i, 2 * c + 1), hash(i, 2 * c + 2));
            let gap = nearest_squared(place, &points, &grid, side);
            if gap > best.1 {
                best = (place, gap);
            }
            if points.is_empty() {
                break;
            }
        }
        let (x, z) = cell_of(best.0, side);
        grid[z * side + x].push(i as u32);
        points.push(best.0);
    }
    points
}

/// Random places each blade of [`roots`] is the best of.
const CANDIDATES: u32 = 20;

/// The grid cell of `side` a side that share `p` lies in.
fn cell_of(p: Vec2, side: usize) -> (usize, usize) {
    let at = |v: f32| ((v * side as f32) as usize).min(side - 1);
    (at(p.x), at(p.y))
}

/// The squared distance on the unit torus from `p` to the nearest of
/// `points`, filed by cell in `grid`; infinite with none. Searches rings
/// of cells outward until no further ring can hold a nearer point.
fn nearest_squared(p: Vec2, points: &[Vec2], grid: &[Vec<u32>], side: usize) -> f32 {
    if points.is_empty() {
        return f32::INFINITY;
    }
    let cell = 1.0 / side as f32;
    let (cx, cz) = cell_of(p, side);
    let mut best = f32::INFINITY;
    for ring in 0..=side / 2 + 1 {
        // A point `ring` cells over lies at least `ring - 1` cells away.
        let floor = (ring as f32 - 1.0).max(0.0) * cell;
        if best <= floor * floor {
            break;
        }
        let r = ring as isize;
        for dz in -r..=r {
            for dx in -r..=r {
                if dx.abs().max(dz.abs()) != r {
                    continue;
                }
                let x = (cx as isize + dx).rem_euclid(side as isize) as usize;
                let z = (cz as isize + dz).rem_euclid(side as isize) as usize;
                for &j in &grid[z * side + x] {
                    let d = (p - points[j as usize]).abs();
                    let d = d.min(Vec2::ONE - d);
                    best = best.min(d.length_squared());
                }
            }
        }
    }
    best
}

/// A number in `[0, 1)` for `i` and `salt`: the PCG output permutation.
fn hash(i: usize, salt: u32) -> f32 {
    let state = (i as u32 ^ salt.wrapping_mul(0x85EB_CA6B))
        .wrapping_mul(747_796_405)
        .wrapping_add(2_891_336_453);
    let word = ((state >> ((state >> 28) + 4)) ^ state).wrapping_mul(277_803_737);
    let word = (word >> 22) ^ word;
    (word >> 8) as f32 / (1u32 << 24) as f32
}

/// Blade `i`'s own number in `[0, 1)`, for the shader to draw the blade's
/// look from where the world position alone repeats.
fn seed(i: usize) -> f32 {
    hash(i, 0)
}

/// How many of a tile's `blades` rank below `share`: blade `i`'s rank is
/// `(i + 0.5) / blades`.
pub fn blades_below(blades: usize, share: f32) -> usize {
    ((blades as f32 * share - 0.5).ceil().max(0.0) as usize).min(blades)
}

/// The mesh of a tile of size `level` ([`TileKey`]) at `detail`, holding in
/// each of its 8 m cells the blades ranked below `share` (one of
/// [`SHARES`]), from a near tile whose blades stand at `roots` ([`roots`]).
///
/// A blade is a pair of vertices at each of its detail's rungs
/// ([`Detail::rungs`]) and a tip over them: a quad between each two pairs
/// and a triangle up to the tip. Each vertex carries:
///
/// * `POSITION`: where the blade stands in its 8 m cell (m) on X and Z, and
///   how far up the blade the vertex is on Y, 0 to 1.
/// * `COLOR`: which edge (-1 left, 1 right, 0 the tip), the blade's rank
///   in `[0, 1)`, its seed, and which of the tile's 8 m cells it stands in:
///   `i + CELLS_A_ROW * j` for the cell `i` along X and `j` along Z
///   ([`CELLS_A_ROW`]). The shader writes the colour itself; the attribute
///   is there for its data, and so that Bevy's PBR fragment reads the
///   colour the shader writes.
pub fn tile_mesh(roots: &[Vec2], detail: Detail, share: f32, level: u32) -> Mesh {
    let blades = roots.len();
    let count = blades_below(blades, share);
    let rungs = detail.rungs();
    let per_blade = 2 * rungs.len() + 1;
    let n = 1u32 << level.min(MAX_LEVEL);
    let cells = (n * n) as usize;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(cells * count * per_blade);
    let mut data: Vec<[f32; 4]> = Vec::with_capacity(cells * count * per_blade);
    let mut indices: Vec<u32> = Vec::with_capacity(cells * count * (6 * (rungs.len() - 1) + 3));
    // A larger tile's 8 m cells one after another, each in rank order.
    for j in 0..n {
        for i in 0..n {
            let cell = (i + CELLS_A_ROW * j) as f32;
            for (b, root) in roots.iter().take(count).enumerate() {
                let at = *root * cfg::TILE_M;
                let rank = (b as f32 + 0.5) / blades as f32;
                let own = seed(b);
                let first = positions.len() as u32;
                for &up in rungs {
                    for side in [-1.0, 1.0] {
                        positions.push([at.x, up, at.y]);
                        data.push([side, rank, own, cell]);
                    }
                }
                positions.push([at.x, 1.0, at.y]);
                data.push([0.0, rank, own, cell]);
                for k in 0..rungs.len() as u32 - 1 {
                    let a = first + 2 * k;
                    indices.extend([a, a + 1, a + 2, a + 2, a + 1, a + 3]);
                }
                let last = first + 2 * (rungs.len() as u32 - 1);
                indices.extend([last, last + 1, last + 2]);
            }
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, data)
    .with_inserted_indices(Indices::U32(indices))
}

/// The lowest and highest ground (m) under tile `key` of heightmap `hm`,
/// centred on the origin as the terrain is: the heights of every vertex of
/// the cells the tile touches, which bound the drawn triangles.
pub fn tile_heights(hm: &HeightMap, key: TileKey) -> (f32, f32) {
    let (w, h) = (hm.width(), hm.height());
    let half = Vec2::new((w - 1) as f32, (h - 1) as f32) * hm.scale() * 0.5;
    let lo = (key.lo() + half) / hm.scale();
    let hi = lo + Vec2::splat(key.side() / hm.scale());
    let x0 = (lo.x.floor().max(0.0) as usize).min(w - 1);
    let z0 = (lo.y.floor().max(0.0) as usize).min(h - 1);
    let x1 = (hi.x.ceil().max(0.0) as usize).min(w - 1);
    let z1 = (hi.y.ceil().max(0.0) as usize).min(h - 1);
    let (mut min, mut max) = (f32::INFINITY, f32::NEG_INFINITY);
    for z in z0..=z1 {
        for x in x0..=x1 {
            let y = hm.get(x, z);
            min = min.min(y);
            max = max.max(y);
        }
    }
    (min, max)
}

/// How tall a blade can stand (m), in any room: the shader's tallest -
/// its own variation (1.45), a tall patch (1.25) and a flower's stalk
/// (1.15) on a dry room's blades, [`cfg::BLADE_HEIGHT`] and four tenths
/// more - and a little over. Mirrors `grass.wgsl`'s `height`.
pub const TALLEST_BLADE: f32 = cfg::BLADE_HEIGHT * 1.4 * 1.45 * 1.25 * 1.15 + 0.05;

/// The widest a blade's `width` is (m), at any quality: the shader widens a
/// blade as the field thins, by the inverse square root of the share it
/// keeps, and the field is thinnest at its reach - at High, a blade there
/// up to `1.3 * BLADE_WIDTH / sqrt(thinning(128 m))`, 0.64 m (#1694). A
/// flower's head is drawn 1.3 times it across ([`WIDEST_LEAN`] holds it).
/// A test holds it over every quality's widest. Mirrors `grass.wgsl`'s
/// width.
pub const WIDEST_BLADE: f32 = 0.65;

/// How far across (m) a blade can reach from its root: its tip leans at
/// most nine tenths of its height, and beside it lies at most a flower's
/// head, 0.65 of the widest blade. Mirrors `grass.wgsl`'s bend and width.
pub const WIDEST_LEAN: f32 = 0.9 * TALLEST_BLADE + 0.65 * WIDEST_BLADE;

/// The bounds of a tile `side` metres across in its own space (its origin
/// is its corner at ground level 0), over ground from `heights.0` to
/// `heights.1`: every vertex the shader can draw for it, so the tile is
/// never culled while a blade of it is on screen. A fixed bound rather than
/// the room's, so a record edit that makes the grass taller needs no tile
/// laid again.
pub fn tile_aabb(heights: (f32, f32), side: f32) -> Aabb {
    Aabb::from_min_max(
        Vec3::new(-WIDEST_LEAN, heights.0 - 0.1, -WIDEST_LEAN),
        Vec3::new(
            side + WIDEST_LEAN,
            heights.1 + TALLEST_BLADE,
            side + WIDEST_LEAN,
        ),
    )
}

/// How far (m) camera `eye` is from tile `key`'s nearest point, standing
/// on ground no higher than `top`: across the ground to the tile's square,
/// and down to its highest ground if the camera is above it. Never more
/// than any blade root in the tile is.
pub fn nearest(eye: Vec3, key: TileKey, top: f32) -> f32 {
    let lo = key.lo();
    let hi = lo + Vec2::splat(key.side());
    let xz = Vec2::new(eye.x, eye.z);
    let across = (xz - xz.clamp(lo, hi)).length();
    let down = (eye.y - top).max(0.0);
    across.hypot(down)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every prefix of the blades covers the tile evenly: cut into cells
    /// that should hold four of the first `n` blades each, no cell is empty
    /// and none holds more than ten, for small `n` as for all of them. A
    /// uniformly random set fails this - one cell in fifty is empty.
    #[test]
    fn every_prefix_spreads_over_the_tile() {
        let all = roots(5_760);
        for n in [40, 160, 1_000, 5_760] {
            let side = ((n as f32 / 4.0).sqrt().round() as usize).max(1);
            let mut cells = vec![0usize; side * side];
            for root in &all[..n] {
                let r = *root * side as f32;
                let (x, z) = ((r.x as usize).min(side - 1), (r.y as usize).min(side - 1));
                cells[z * side + x] += 1;
            }
            let (least, most) = (cells.iter().min(), cells.iter().max());
            assert!(
                least >= Some(&1) && most <= Some(&10),
                "{n}: {least:?}..{most:?}"
            );
        }
    }

    /// A tile's meshes: the far ones a prefix of the near one's blades, by
    /// rank, with fewer segments a blade - a middle blade's upper pair where
    /// a near one's stands - and a larger tile's its cells' blades, cell by
    /// cell, each as an 8 m tile's.
    #[test]
    fn far_meshes_are_a_prefix_of_the_near_one() {
        let blades = 100;
        let column = |mesh: &Mesh, k: usize| -> Vec<f32> {
            let Some(bevy::mesh::VertexAttributeValues::Float32x4(data)) =
                mesh.attribute(Mesh::ATTRIBUTE_COLOR)
            else {
                panic!("blade data");
            };
            data.iter().map(|d| d[k]).collect()
        };
        let ranks = |mesh: &Mesh| -> Vec<f32> {
            let mut ranks = column(mesh, 1);
            ranks.dedup();
            ranks
        };
        let positions = |mesh: &Mesh| -> Vec<[f32; 3]> {
            let Some(bevy::mesh::VertexAttributeValues::Float32x3(at)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("positions");
            };
            at.clone()
        };
        let roots = roots(blades);
        let near = tile_mesh(&roots, Detail::Near, 1.0, 0);
        let mid = tile_mesh(&roots, Detail::Mid, Detail::Mid.share(), 0);
        let far = tile_mesh(&roots, Detail::Far, Detail::Far.share(), 0);
        assert_eq!(near.count_vertices(), 100 * 7);
        assert_eq!(mid.count_vertices(), 40 * 5);
        assert_eq!(far.count_vertices(), 15 * 3);
        assert_eq!(near.indices().map(|i| i.len()), Some(100 * 15));
        assert_eq!(far.indices().map(|i| i.len()), Some(15 * 3));
        let (near_ranks, mid_ranks) = (ranks(&near), ranks(&mid));
        assert_eq!(&near_ranks[..mid_ranks.len()], &mid_ranks[..]);
        assert!(mid_ranks.iter().all(|&r| r < Detail::Mid.share()));
        // A blade's pairs stand at its rungs, its tip over them.
        let ups = |mesh: &Mesh, per_blade: usize| -> Vec<f32> {
            positions(mesh)[..per_blade].iter().map(|p| p[1]).collect()
        };
        assert_eq!(
            ups(&near, 7),
            [0.0, 0.0, 1.0 / 3.0, 1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0, 1.0]
        );
        assert_eq!(ups(&mid, 5), [0.0, 0.0, 2.0 / 3.0, 2.0 / 3.0, 1.0]);
        assert_eq!(ups(&far, 3), [0.0, 0.0, 1.0]);
        assert_eq!(Detail::Mid.rungs()[1], Detail::Near.rungs()[2]);
        // An 8 m tile is all cell 0; a 16 m one four cells of the 8 m one's
        // blades, numbered across rows of `CELLS_A_ROW`.
        assert!(column(&far, 3).iter().all(|&c| c == 0.0));
        let large = tile_mesh(&roots, Detail::Far, Detail::Far.share(), 1);
        assert_eq!(large.count_vertices(), 4 * far.count_vertices());
        assert_eq!(large.indices().map(|i| i.len()), Some(4 * 15 * 3));
        let cells = column(&large, 3);
        let one = far.count_vertices();
        let at = positions(&far);
        let large_at = positions(&large);
        for (k, cell) in [0.0, 1.0, CELLS_A_ROW as f32, CELLS_A_ROW as f32 + 1.0]
            .into_iter()
            .enumerate()
        {
            assert!(cells[k * one..(k + 1) * one].iter().all(|&c| c == cell));
            assert_eq!(&large_at[k * one..(k + 1) * one], &at[..]);
        }
        // Its indices are the 8 m one's, cell after cell.
        let (Some(Indices::U32(small)), Some(Indices::U32(big))) = (far.indices(), large.indices())
        else {
            panic!("u32 indices");
        };
        for k in 0..4 {
            let offset = (k * one) as u32;
            let cell = &big[k * small.len()..(k + 1) * small.len()];
            assert!(cell.iter().zip(small).all(|(&b, &s)| b == s + offset));
        }
    }

    /// A tile's key: its side and corner by its size, the 8 m cells it is
    /// made of, and its quarters a size down, which are those cells too.
    #[test]
    fn a_tile_is_its_cells() {
        let small = TileKey::small(IVec2::new(-1, 2));
        assert_eq!((small.side(), small.lo()), (8.0, Vec2::new(-8.0, 16.0)));
        assert_eq!(small.cells().collect::<Vec<_>>(), [IVec2::new(-1, 2)]);
        assert_eq!(small.quarters().count(), 0);
        let block = TileKey {
            level: MAX_LEVEL,
            coord: IVec2::new(-1, 0),
        };
        assert_eq!(block.side(), cfg::TILE_M * CELLS_A_ROW as f32);
        assert_eq!(block.lo(), Vec2::new(-32.0, 0.0));
        let cells: Vec<IVec2> = block.cells().collect();
        assert_eq!(cells.len(), 16);
        assert!(
            cells
                .iter()
                .all(|c| (-4..0).contains(&c.x) && (0..4).contains(&c.y))
        );
        let mut through_quarters: Vec<IVec2> = block
            .quarters()
            .flat_map(|q| q.quarters())
            .flat_map(TileKey::cells)
            .collect();
        let mut cells = cells;
        through_quarters.sort_by_key(|c| (c.y, c.x));
        cells.sort_by_key(|c| (c.y, c.x));
        assert_eq!(through_quarters, cells);
    }

    /// A tile's detail follows the distance to its nearest point.
    #[test]
    fn detail_follows_the_nearest_point() {
        let field = cfg::MEDIUM;
        assert_eq!(Detail::at(0.0, &field), Detail::Near);
        assert_eq!(
            Detail::at(cfg::MID_AT * field.full + 0.1, &field),
            Detail::Mid
        );
        assert_eq!(
            Detail::at(cfg::FAR_AT * field.full + 0.1, &field),
            Detail::Far
        );
        // Standing in a tile, it is nought away; one tile over, its edge.
        let eye = Vec3::new(4.0, 1.7, 4.0);
        assert_eq!(nearest(eye, TileKey::small(IVec2::ZERO), 0.0), 1.7);
        let next = nearest(eye, TileKey::small(IVec2::new(1, 0)), 0.0);
        assert!((next - 4.0f32.hypot(1.7)).abs() < 1e-5);
        // A 16 m tile's nearest point is its square's.
        let large = TileKey {
            level: 1,
            coord: IVec2::new(1, 0),
        };
        assert!((nearest(eye, large, 0.0) - 12.0f32.hypot(1.7)).abs() < 1e-5);
    }

    /// A tile's ground spans the heights of the cells it touches.
    #[test]
    fn tile_heights_bound_the_ground_under_it() {
        // 33 vertices, 1 m cells: -16..16 m. A ramp along X.
        let mut hm = HeightMap::new(33, 33, 1.0);
        for z in 0..33 {
            for x in 0..33 {
                hm.set(x, z, x as f32);
            }
        }
        // Tile (0, 0) runs 0..8 m on X: vertices 16..=24.
        assert_eq!(tile_heights(&hm, TileKey::small(IVec2::ZERO)), (16.0, 24.0));
        // Past the edge, the edge.
        assert_eq!(
            tile_heights(&hm, TileKey::small(IVec2::new(5, 0))),
            (32.0, 32.0)
        );
        // A 16 m tile, 0..16 m: vertices 16..=32.
        let large = TileKey {
            level: 1,
            coord: IVec2::ZERO,
        };
        assert_eq!(tile_heights(&hm, large), (16.0, 32.0));
    }

    /// The shares a tile's mesh can hold fall from all of them, and hold the
    /// middle and far details' own (#1693): a tile at either draws no more
    /// than the detail's whole share did. A tile needs the fewest that
    /// holds what the shader keeps, and none when it keeps none.
    #[test]
    fn a_tile_draws_the_fewest_blades_that_hold_what_grows() {
        assert_eq!(SHARES[0], 1.0);
        assert!(SHARES.windows(2).all(|w| w[1] < w[0]), "{SHARES:?}");
        for detail in Detail::ALL {
            assert!(SHARES.contains(&detail.share()), "{detail:?}");
        }
        assert_eq!(share_for(0.0), None);
        assert_eq!(share_for(-1.0), None);
        assert_eq!(share_for(f32::NAN), None);
        assert_eq!(share_for(1.0), Some(0));
        assert_eq!(share_for(1.5), Some(0));
        let mid = SHARES.iter().position(|&s| s == cfg::MID_SHARE).unwrap();
        assert_eq!(share_for(cfg::MID_SHARE), Some(mid));
        assert_eq!(share_for(0.45), Some(mid - 1));
        assert_eq!(share_for(0.001), Some(SHARES.len() - 1));
        // A mesh at a share holds every blade ranked below it, and no more.
        assert_eq!(blades_below(100, 1.0), 100);
        assert_eq!(blades_below(100, 0.4), 40);
        assert_eq!(blades_below(100, 0.0), 0);
    }

    /// The thinning is the shader's, which reads the same shares and
    /// distances from the field's uniforms (`grass::uniforms`): the
    /// function's three returns are pinned here. Past the far share it falls
    /// as the square of the distance (#1694): as many blades a patch of
    /// screen, and the last of [`SHARES`] holds what the thinnest field
    /// keeps at its reach.
    #[test]
    fn the_thinning_is_the_shaders() {
        let wgsl = include_str!("../../assets/shaders/grass.wgsl");
        for line in [
            "return mix(1.0, grass.shares.x, smoothstep(grass.reach.x, grass.reach.y, distance));",
            "return mix(grass.shares.x, grass.shares.y, smoothstep(grass.reach.y, grass.reach.z, distance));",
            "let past = grass.reach.z / distance;",
            "return grass.shares.y * past * past;",
        ] {
            assert!(wgsl.contains(line), "grass.wgsl's thinning moved: {line}");
        }
        let field = cfg::HIGH;
        let far = cfg::FAR_AT * field.full;
        assert_eq!(thinning(0.0, &field), 1.0);
        assert_eq!(thinning(field.full, &field), 1.0);
        assert!((thinning(cfg::MID_AT * field.full, &field) - cfg::MID_SHARE).abs() < 1e-6);
        assert!((thinning(far, &field) - cfg::FAR_SHARE).abs() < 1e-6);
        assert!((thinning(2.0 * far, &field) - cfg::FAR_SHARE / 4.0).abs() < 1e-6);
        for at in [1.2, 1.9, 3.1] {
            let d = at * far;
            let on_screen = thinning(d, &field) * d * d;
            assert!((on_screen / (cfg::FAR_SHARE * far * far) - 1.0).abs() < 1e-5);
        }
        let mut last = 1.0;
        for step in 0..=300 {
            let at = thinning(step as f32 * 0.5, &field);
            assert!(at <= last + 1e-6, "{step}");
            last = at;
        }
        for field in [cfg::LOW, cfg::MEDIUM, cfg::HIGH] {
            let thinnest = thinning(field.reach, &field);
            assert!(
                thinnest >= SHARES[SHARES.len() - 1],
                "{field:?}: {thinnest}"
            );
        }
    }

    /// No blade is drawn wider than [`WIDEST_BLADE`] at any quality: the
    /// shader's width (its lines pinned here) widens a blade by the inverse
    /// square root of the share kept, which is least at the reach, and holds
    /// it over a pixel and a half there too.
    #[test]
    fn the_widest_blade_bounds_every_quality() {
        let wgsl = include_str!("../../assets/shaders/grass.wgsl");
        for line in [
            "var width = grass.tip.w * (0.7 + 0.6 * r2) * inverseSqrt(max(thinning(distance), 1.0e-3));",
            "width = max(width, distance * MIN_WIDTH_PER_M)",
            "const MIN_WIDTH_PER_M: f32 = 0.0012;",
            "half_width = 0.65 * blade.width;",
            "var half_width = 0.5 * blade.width * pow(1.0 - up, 0.8);",
        ] {
            assert!(wgsl.contains(line), "grass.wgsl's width moved: {line}");
        }
        for field in [cfg::LOW, cfg::MEDIUM, cfg::HIGH] {
            let thinnest = thinning(field.reach, &field).max(1.0e-3);
            let widest = (1.3 * cfg::BLADE_WIDTH / thinnest.sqrt()).max(field.reach * 0.0012);
            assert!(widest <= WIDEST_BLADE, "{field:?}: {widest} m");
        }
    }

    /// A blade takes the shape of the fewer segments it is drawn with next
    /// (`grass.wgsl`'s geomorph): its third-way pair onto the line between
    /// the root and the upper pair, which stands where a middle blade's
    /// does - the shader's `UPPER_RUNG`, pinned here - and that onto the
    /// line to the tip. The third-way pair has finished before the upper
    /// one starts (the shader's `MORPH_FROM`, read here), so a near tile's
    /// farthest blades, past both, are already a far one's; and the shader
    /// reads a large tile's cells as its mesh numbers them, along X first.
    #[test]
    fn the_shader_morphs_to_the_meshes_rungs() {
        let wgsl = include_str!("../../assets/shaders/grass.wgsl");
        assert_eq!(NEAR_RUNGS[1] * 2.0, NEAR_RUNGS[2]);
        assert_eq!(MID_RUNGS[1], 2.0 / 3.0);
        for line in [
            "const UPPER_RUNG: f32 = 2.0 / 3.0;".to_string(),
            format!("const CELLS_A_ROW: u32 = {CELLS_A_ROW}u;"),
            "vec2<f32>(f32(in_tile % CELLS_A_ROW), f32(in_tile / CELLS_A_ROW)) * TILE_M;"
                .to_string(),
            "point = between(point, between(foot, upper, 0.5), to_mid);".to_string(),
            "between(foot, blade_point(blade, 1.0, 0.0), UPPER_RUNG)".to_string(),
        ] {
            assert!(wgsl.contains(&line), "grass.wgsl moved: {line}");
        }
        let morph_from: f32 = wgsl
            .lines()
            .find_map(|l| l.strip_prefix("const MORPH_FROM: f32 = "))
            .and_then(|v| v.trim_end_matches(';').parse().ok())
            .expect("grass.wgsl's MORPH_FROM");
        assert!(morph_from > 0.0 && morph_from < 1.0, "{morph_from}");
        assert!(morph_from * cfg::FAR_AT >= cfg::MID_AT, "{morph_from}");
    }

    /// The most a tile's ground can grow is never under what the shader
    /// grows there, clumps included: over every ground density up to the
    /// most and every clump the noise can make, the shader's density
    /// (`grass.wgsl`, whose clump literals are pinned here) stays at or
    /// under the bound - and the bound is reached.
    #[test]
    fn the_most_density_bounds_the_shaders() {
        let wgsl = include_str!("../../assets/shaders/grass.wgsl");
        assert!(wgsl.contains(&format!(
            "let clump = {} * smoothstep(0.25, 0.75,",
            cfg::CLUMP_PEAK
        )));
        assert!(wgsl.contains(&format!(
            "mix(1.0, clump, {} * (1.0 - ground_density))",
            cfg::CLUMP_SHARE
        )));
        let shader = |ground: f32, clump: f32| {
            ground * (1.0 + (clump - 1.0) * cfg::CLUMP_SHARE * (1.0 - ground))
        };
        for most in 0..=50 {
            let most = most as f32 / 50.0;
            let bound = most_density([most, 0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0], 1.0);
            let mut reached = 0.0f32;
            for ground in 0..=50 {
                let ground = most * ground as f32 / 50.0;
                for clump in 0..=22 {
                    let grown = shader(ground, clump as f32 / 10.0);
                    assert!(
                        grown <= bound + 1e-6,
                        "{most} {ground} {clump}: {grown} > {bound}"
                    );
                    reached = reached.max(grown);
                }
            }
            assert!(
                bound - reached < 0.02,
                "{most}: {bound} is loose by {}",
                bound - reached
            );
        }
        // The layers' shares weigh by their densities, and the cover caps it.
        assert_eq!(most_density([0.0; 4], [1.0; 4], 1.0), 0.0);
        assert_eq!(most_density([1.0; 4], [1.0; 4], 0.0), 0.0);
        let half = most_density([0.5, 0.5, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0], 1.0);
        assert_eq!(
            half,
            most_density([1.0, 0.0, 0.0, 0.0], [0.5, 0.0, 0.0, 0.0], 1.0)
        );
    }
}
