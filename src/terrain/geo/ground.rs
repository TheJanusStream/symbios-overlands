//! What real Berlin says about a geodata region's ground beside its heights
//! (#1586): the land use covering each cell of the core, and its water.
//!
//! The land-use layer (Umweltatlas "Reale Nutzung" 2015) is rendered over
//! the terrain's own box and grid, so its pixel `(x, z)` is the cover of
//! heightmap cell `(x, z)`. Each class is drawn with one of the region's
//! four splat layers, by the role every seeded theme gives that layer (0 the
//! flat green ground, 1 bare earth, 2 stone, 3 the summit):
//!
//! | Layer | Classes |
//! | --- | --- |
//! | 0, green | forest, park, meadow, cemetery, allotments, weekend cottages, sport, tree nursery, the two vegetated fallows |
//! | 1, earth | farmland, bare fallow, construction sites, water beds, and the built-up blocks until buildings stand on them |
//! | 2, stone | street space, city squares, rail and airfields |
//!
//! So the theme dresses Berlin: its grass, sand or moss on the green, its
//! earth on the blocks, its rock on the streets.
//!
//! A scatter's biome filter names natural ground, and Berlin's built-up
//! blocks, streets, squares, rail, sport grounds, construction sites and
//! water are none of it: a seeded stand of trees grows in the parks and
//! forests, not across the streets ([`GeoGround::scatter_layer_at`]).
//!
//! Water is [`geodata::water::settle`]'s (#1724): each body at its own
//! level, its bed carved below it and its shore eased through it, so each
//! body's own surface, drawn over its own cells ([`GeoGround::water`]),
//! shows Berlin's water and nothing else - a lake on the plateau and the
//! river below it, or a lock's two reaches, each at its own level.

use std::sync::Arc;

use bevy_symbios_ground::WeightMap;
use geodata::berlin::LandUse;

use super::street_level::StreetLevel;
use crate::urban::RoadParts;
use crate::water::bodies::WaterBody;

/// The scatter layer of ground that is no natural ground: matched by no
/// biome filter.
pub(crate) const NOT_NATURAL: u8 = u8::MAX;

/// The land use under each cell of a geodata region's core, and its water.
/// Rides in [`super::super::FinishedHeightMap`] with the heights it was
/// decoded beside, so the two land, and go, together.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GeoGround {
    /// Cells per side: the heightmap's grid.
    grid: u32,
    /// Metres between cells: the heightmap's scale.
    cell: f32,
    /// Per cell, row-major from the north-west corner as the heightmap's
    /// data runs: the land use drawn there, `None` for street space and for
    /// a fill the legend does not name.
    cover: Vec<Option<LandUse>>,
    /// The core's water, each body at its own level (#1724) and drawn over
    /// its own cells; none for a dry core. Its cells are shared, so the
    /// clones the contact classifier keeps do not copy them.
    water: Vec<WaterBody>,
    /// Berlin's streets on the core, meshed (#1595), where it has any.
    streets: Option<StreetMeshes>,
    /// Its buildings, trees and street furniture (#1588), where they were
    /// had. Shared, so the clones the contact classifier keeps do not copy
    /// them.
    street_level: Option<Arc<StreetLevel>>,
}

/// Berlin's streets meshed on a core (#1595), shared: the clones the
/// contact classifier keeps do not copy them. The one spawn that draws them
/// takes them out ([`Self::take`]), so the ground keeps no second copy of
/// buffers the GPU already holds for the rest of the visit. Two are equal
/// when they hold the same meshes.
#[derive(Clone)]
pub(crate) struct StreetMeshes(Arc<std::sync::Mutex<Option<RoadParts>>>);

impl StreetMeshes {
    /// Take the meshes out, to spawn them: none are left after.
    pub(crate) fn take(&self) -> Option<RoadParts> {
        self.0.lock().ok()?.take()
    }

    /// What `read` says of the meshes, while they are not yet taken.
    pub(crate) fn with<R>(&self, read: impl FnOnce(&RoadParts) -> R) -> Option<R> {
        self.0.lock().ok()?.as_ref().map(read)
    }
}

impl PartialEq for StreetMeshes {
    fn eq(&self, other: &Self) -> bool {
        if Arc::ptr_eq(&self.0, &other.0) {
            return true;
        }
        match (self.0.lock(), other.0.lock()) {
            (Ok(a), Ok(b)) => *a == *b,
            _ => false,
        }
    }
}

impl std::fmt::Debug for StreetMeshes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.with(|p| (p.chains, p.junctions)) {
            Some((streets, junctions)) => {
                write!(f, "StreetMeshes({streets} streets, {junctions} junctions)")
            }
            None => write!(f, "StreetMeshes(spawned)"),
        }
    }
}

impl GeoGround {
    /// The core's water: each body at its own level, metres above sea
    /// level, and the cells it is drawn over (#1724). Empty for a dry core.
    pub(crate) fn water(&self) -> &[WaterBody] {
        &self.water
    }

    /// The level of the water drawn over cell `cell` of the core, the
    /// highest body's where two meet there, or `None` where none is.
    pub(super) fn level_over(&self, cell: (u32, u32)) -> Option<f32> {
        self.water
            .iter()
            .filter(|body| body.cells.mask.contains(cell))
            .map(|body| body.level)
            .reduce(f32::max)
    }

    /// Berlin's streets on the core, meshed.
    pub(crate) fn streets(&self) -> Option<&StreetMeshes> {
        self.streets.as_ref()
    }

    /// Give the ground its streets.
    pub(super) fn set_streets(&mut self, parts: RoadParts) {
        self.streets = Some(StreetMeshes(Arc::new(std::sync::Mutex::new(Some(parts)))));
    }

    /// The walkable ground's buildings, trees and street furniture.
    pub(crate) fn street_level(&self) -> Option<&Arc<StreetLevel>> {
        self.street_level.as_ref()
    }

    /// Give the ground its street level.
    pub(super) fn set_street_level(&mut self, level: StreetLevel) {
        self.street_level = Some(Arc::new(level));
    }

    /// The land use of every cell, row-major from the north-west corner.
    pub(super) fn cover(&self) -> &[Option<LandUse>] {
        &self.cover
    }

    /// Which cells are water, and the bridges over it that the streets
    /// crossing `bridges` ride ([`water_mask`]): the mask the core's water
    /// was settled with, given the same `bridges`.
    pub(super) fn wet(&self, bridges: Option<&[bool]>) -> Vec<bool> {
        water_mask(&self.cover, self.grid as usize, self.cell, bridges)
    }

    /// The splat weight map: one texel per cell, all of it on the cell's
    /// layer.
    pub(crate) fn weight_map(&self) -> WeightMap {
        one_hot_weights(&self.cover, self.grid as usize)
    }

    /// The four layers' weights at heightmap-local `(lx, lz)` metres,
    /// interpolated between the cells around it as the GPU samples the
    /// weight map. Clamped to the core's edge.
    pub(crate) fn weights_at_local(&self, lx: f32, lz: f32) -> [f32; 4] {
        weights_between(&self.cover, self.grid as usize, self.cell, lx, lz)
    }

    /// The splat layer a scatter's biome filter sees at world `(x, z)`: the
    /// nearest cell's layer where it is natural ground, else [`NOT_NATURAL`].
    pub(crate) fn scatter_layer_at(&self, world_x: f32, world_z: f32) -> u8 {
        let cover = self.cover_at(world_x, world_z);
        if is_natural(cover) {
            layer(cover) as u8
        } else {
            NOT_NATURAL
        }
    }

    /// The land use of the ground's cell nearest world `(x, z)`, the world
    /// centring the ground on the origin as it does the heightmap - held at
    /// its edge past it.
    pub(crate) fn cover_at(&self, world_x: f32, world_z: f32) -> Option<LandUse> {
        let last = self.grid.saturating_sub(1) as usize;
        let half = last as f32 * self.cell * 0.5;
        let cell_of = |w: f32| (((w + half) / self.cell).round().clamp(0.0, last as f32)) as usize;
        self.cover[cell_of(world_z) * (last + 1) + cell_of(world_x)]
    }
}

#[cfg(test)]
impl GeoGround {
    /// A core of `grid` x `grid` cells `cell` metres apart, `cover` row by
    /// row from the north-west corner, its water `water`.
    pub(crate) fn from_cover(
        grid: u32,
        cell: f32,
        cover: Vec<Option<LandUse>>,
        water: Vec<WaterBody>,
    ) -> Self {
        assert_eq!(cover.len(), (grid * grid) as usize, "one cover a cell");
        GeoGround {
            grid,
            cell,
            cover,
            water,
            streets: None,
            street_level: None,
        }
    }

    /// `cover`'s water on `heights` as a test's ground has it: every body
    /// of its water cells, 4-connected, at `level`, drawn over those cells
    /// and the ring round them, whatever the heights under them.
    pub(crate) fn water_of_cover(
        heights: &bevy_symbios_ground::HeightMap,
        cover: &[Option<LandUse>],
        level: f32,
    ) -> Vec<WaterBody> {
        let side = heights.width();
        let water: Vec<bool> = cover.iter().map(|&c| c == Some(LandUse::Water)).collect();
        let mut seen = vec![false; water.len()];
        let mut bodies = Vec::new();
        for start in 0..water.len() {
            if !water[start] || seen[start] {
                continue;
            }
            seen[start] = true;
            let (mut stack, mut wet) = (vec![start], Vec::new());
            while let Some(i) = stack.pop() {
                wet.push(i as u32);
                let (x, z) = (i % side, i / side);
                let sides = [
                    (x > 0, i.wrapping_sub(1)),
                    (x + 1 < side, i + 1),
                    (z > 0, i.wrapping_sub(side)),
                    (z + 1 < side, i + side),
                ];
                for (on_grid, j) in sides {
                    if on_grid && water[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
            wet.sort_unstable();
            bodies.push(geodata::water::Body { level, wet });
        }
        let settled = geodata::water::Water {
            bodies,
            ..Default::default()
        };
        water_bodies(heights.data(), side as u32, heights.scale(), &settled)
    }

    /// This ground with `parts` as its streets.
    pub(crate) fn with_streets(mut self, parts: RoadParts) -> Self {
        self.set_streets(parts);
        self
    }

    /// This ground with `level` as its street level.
    pub(crate) fn with_street_level(mut self, level: StreetLevel) -> Self {
        self.set_street_level(level);
        self
    }
}

/// The four layers' weights at local `(lx, lz)` metres over `cover` - `grid`
/// x `grid` points `cell` metres apart, row-major from the north-west - as
/// the GPU samples a one-hot weight map of it: interpolated between the
/// points round it, clamped to its edge.
pub(super) fn weights_between(
    cover: &[Option<LandUse>],
    grid: usize,
    cell: f32,
    lx: f32,
    lz: f32,
) -> [f32; 4] {
    let last = grid.saturating_sub(1);
    let (gx, gz) = (
        (lx / cell).clamp(0.0, last as f32),
        (lz / cell).clamp(0.0, last as f32),
    );
    let (x0, z0) = (gx.floor() as usize, gz.floor() as usize);
    let (x1, z1) = ((x0 + 1).min(last), (z0 + 1).min(last));
    let (fx, fz) = (gx - x0 as f32, gz - z0 as f32);
    let mut weights = [0.0; 4];
    for (x, z, share) in [
        (x0, z0, (1.0 - fx) * (1.0 - fz)),
        (x1, z0, fx * (1.0 - fz)),
        (x0, z1, (1.0 - fx) * fz),
        (x1, z1, fx * fz),
    ] {
        weights[layer(cover[z * (last + 1) + x])] += share;
    }
    weights
}

/// The splat layer a land-use class is drawn with (see the module docs).
pub(crate) fn layer(cover: Option<LandUse>) -> usize {
    match cover {
        Some(
            LandUse::Forest
            | LandUse::Park
            | LandUse::Meadow
            | LandUse::Cemetery
            | LandUse::Allotment
            | LandUse::Cottage
            | LandUse::Sport
            | LandUse::Nursery
            | LandUse::FallowMeadow
            | LandUse::FallowMixed,
        ) => 0,
        Some(
            LandUse::Farmland
            | LandUse::FallowBare
            | LandUse::Construction
            | LandUse::Water
            | LandUse::Housing
            | LandUse::Mixed
            | LandUse::Core
            | LandUse::Commercial
            | LandUse::PublicSpecial
            | LandUse::Utility,
        ) => 1,
        Some(LandUse::Square | LandUse::Traffic) | None => 2,
    }
}

/// Whether a scatter may grow on a class: natural ground - green, farmland,
/// bare fallow - not the built city, its open spaces kept clear (sport,
/// squares, construction sites) or its water.
pub(crate) fn is_natural(cover: Option<LandUse>) -> bool {
    matches!(
        cover,
        Some(
            LandUse::Forest
                | LandUse::Park
                | LandUse::Meadow
                | LandUse::Cemetery
                | LandUse::Allotment
                | LandUse::Cottage
                | LandUse::Nursery
                | LandUse::FallowMeadow
                | LandUse::FallowMixed
                | LandUse::Farmland
                | LandUse::FallowBare
        )
    )
}

/// Decode a land-use render of the core through its legend, and settle the
/// core's water on `heights` (the decoded terrain, `grid` x `grid` cells
/// `cell` metres apart): its beds carved, under the bridges Berlin's
/// streets cross too (`bridges`, see [`water_mask`]), the rest of the
/// ground kept above its level. On an error `heights` is untouched.
pub(crate) fn decode_ground(
    legend: &[u8],
    render: &[u8],
    heights: &mut [f32],
    grid: u32,
    cell: f32,
    bridges: Option<&[bool]>,
) -> Result<GeoGround, String> {
    let cover = decode_cover(legend, render, grid)?;
    let wet = water_mask(&cover, grid as usize, cell, bridges);
    let settled = geodata::water::settle(heights, &wet, grid, grid, cell);
    Ok(GeoGround {
        grid,
        cell,
        cover,
        water: water_bodies(heights, grid, cell, &settled),
        streets: None,
        street_level: None,
    })
}

/// The `settled` water's bodies on the core whose settled heights are
/// `heights`, `grid` x `grid` cells `cell` metres apart: each drawn over
/// the cells its water covers and the ring round them, its level reaching
/// up its bank ([`crate::water::bodies::body_round`]) - but never over
/// another body's water, where two lie side by side at a lock.
pub(super) fn water_bodies(
    heights: &[f32],
    grid: u32,
    cell: f32,
    settled: &geodata::water::Water,
) -> Vec<WaterBody> {
    let mut whose = vec![u32::MAX; heights.len()];
    for (k, body) in settled.bodies.iter().enumerate() {
        for &i in &body.wet {
            whose[i as usize] = k as u32;
        }
    }
    let cells = crate::water::bodies::CellGrid {
        width: grid,
        height: grid,
        scale: cell,
    };
    settled
        .bodies
        .iter()
        .enumerate()
        .filter(|(_, body)| !body.wet.is_empty())
        .map(|(k, body)| {
            let wet: Vec<usize> = body.wet.iter().map(|&i| i as usize).collect();
            let theirs = |j: usize| whose[j] == u32::MAX || whose[j] == k as u32;
            WaterBody {
                level: body.level,
                cells: Arc::new(crate::water::bodies::body_round_on(
                    heights, cells, body.level, &wet, theirs,
                )),
            }
        })
        .collect()
}

/// Decode a land-use render of `grid` x `grid` pixels through its legend:
/// the land use under each pixel, row-major from the north-west corner.
pub(super) fn decode_cover(
    legend: &[u8],
    render: &[u8],
    grid: u32,
) -> Result<Vec<Option<LandUse>>, String> {
    let legend = geodata::legend::parse_class_legend(legend)
        .map_err(|e| format!("Berlin's land-use legend could not be read: {e}"))?;
    let table = geodata::berlin::land_use_table(&legend);
    if table.iter().all(Option::is_none) {
        return Err("Berlin's land-use legend names no class this build knows.".to_owned());
    }
    let image = geodata::raster::decode_png(render, grid, grid)
        .map_err(|e| format!("Berlin's land use could not be read: {e}"))?;
    let classes = geodata::raster::decode_classes(&image, &legend)
        .map_err(|e| format!("Berlin's land use could not be read: {e}"))?;
    Ok(classes
        .classes
        .iter()
        .map(|&id| table.get(usize::from(id)).copied().flatten())
        .collect())
}

/// The widest bridge the water runs on under (m): see [`water_mask`].
const MAX_BRIDGE_M: f32 = 40.0;

/// Which pixels of `cover` - `side` x `side` pixels `cell` metres apart -
/// are water: its water, and the bridges over it that Berlin's streets
/// cross (#1595).
///
/// The land use maps a bridge as the street it carries, so a river crossed
/// by one is two bodies with a strip of street space between them, and
/// settled as land the strip stood as a dam across the river. A run of
/// street space along a row or a column, no longer than [`MAX_BRIDGE_M`],
/// with water at both its ends and a street drawn over it - one of the
/// `bridges` cells a drawn street axis crosses - is the water's: the river
/// runs on under the bridge, and the street rides over it as a deck. With
/// no `bridges` - the streets not fetched - and on a footbridge or a rail
/// bridge, which no street deck would cover, the strip stays the dam it
/// was, and can be walked.
pub(super) fn water_mask(
    cover: &[Option<LandUse>],
    side: usize,
    cell: f32,
    bridges: Option<&[bool]>,
) -> Vec<bool> {
    let water = |c: Option<LandUse>| c == Some(LandUse::Water);
    let mut wet: Vec<bool> = cover.iter().map(|&c| water(c)).collect();
    let Some(bridges) = bridges else {
        return wet;
    };
    let longest = (MAX_BRIDGE_M / cell.max(0.01)).floor() as usize;
    // Every row, then every column, as the step between its pixels.
    let lines = (0..side)
        .map(|r| (r * side, 1))
        .chain((0..side).map(|c| (c, side)));
    for (first, step) in lines {
        let at = |i: usize| first + i * step;
        let mut i = 0;
        while i < side {
            if cover[at(i)].is_some() {
                i += 1;
                continue;
            }
            let start = i;
            while i < side && cover[at(i)].is_none() {
                i += 1;
            }
            let bounded =
                start > 0 && i < side && water(cover[at(start - 1)]) && water(cover[at(i)]);
            let crossed = (start..i).any(|j| bridges[at(j)]);
            if bounded && crossed && i - start <= longest {
                for j in start..i {
                    wet[at(j)] = true;
                }
            }
        }
    }
    wet
}

/// A one-texel-per-cell weight map, all of each texel on its cell's layer.
pub(super) fn one_hot_weights(cover: &[Option<LandUse>], side: usize) -> WeightMap {
    WeightMap {
        data: cover
            .iter()
            .map(|&cover| {
                let mut texel = [0; 4];
                texel[layer(cover)] = 255;
                texel
            })
            .collect(),
        width: side,
        height: side,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground(grid: u32, cover: Vec<Option<LandUse>>) -> GeoGround {
        GeoGround::from_cover(grid, 2.0, cover, Vec::new())
    }

    #[test]
    fn every_class_has_a_layer_and_the_city_is_no_natural_ground() {
        for class in LandUse::ALL {
            assert!(
                layer(Some(class)) < 3,
                "{class:?} is drawn, and not as summit"
            );
        }
        // The layer roles a scatter's filter relies on: trees ask for 0 and
        // 1, boulders for 1 and 2.
        assert_eq!(layer(Some(LandUse::Park)), 0);
        assert_eq!(layer(Some(LandUse::Housing)), 1);
        assert_eq!(layer(None), 2, "street space is stone");
        for city in [
            None,
            Some(LandUse::Housing),
            Some(LandUse::Core),
            Some(LandUse::Square),
            Some(LandUse::Traffic),
            Some(LandUse::Sport),
            Some(LandUse::Construction),
            Some(LandUse::Water),
        ] {
            assert!(!is_natural(city), "{city:?}");
        }
        assert!(is_natural(Some(LandUse::Forest)) && is_natural(Some(LandUse::Farmland)));
    }

    /// #1595: a bridge is the street it carries in the land use; the water
    /// runs on under it where a street is drawn over it, and nowhere else
    /// street space meets water.
    #[test]
    fn the_water_runs_on_under_its_bridges() {
        // A river down columns 14 and 15 of a 30-cell square; a street
        // crosses it on row 4 and a footbridge on row 20, a quay runs
        // beside it on column 17, and on row 8 a street of 13 cells runs
        // between a pond at column 0 and the river. The streets drawn are
        // row 4's, column 17's and row 8's.
        let side = 30;
        let mut cover = vec![Some(LandUse::Housing); side * side];
        let at = |x: usize, z: usize| z * side + x;
        let mut drawn = vec![false; side * side];
        for z in 0..side {
            cover[at(14, z)] = Some(LandUse::Water);
            cover[at(15, z)] = Some(LandUse::Water);
            cover[at(17, z)] = None;
            drawn[at(17, z)] = true;
        }
        for x in 14..=15 {
            cover[at(x, 4)] = None;
            cover[at(x, 20)] = None;
        }
        for x in 0..side {
            drawn[at(x, 4)] = true;
            drawn[at(x, 8)] = true;
        }
        for x in 1..14 {
            cover[at(x, 8)] = None;
        }
        cover[at(0, 8)] = Some(LandUse::Water);
        let wet = water_mask(&cover, side, 4.0, Some(&drawn));
        assert!(
            wet[at(14, 4)] && wet[at(15, 4)],
            "the bridge's span is the river's"
        );
        assert!(
            !wet[at(14, 20)],
            "no street over the footbridge: it stays a walk"
        );
        assert!(
            !wet[at(17, 4)],
            "the quay is no bridge: water on one side only"
        );
        assert!(
            (1..14).all(|x| !wet[at(x, 8)]),
            "52 m of street between two waters is a street"
        );
        let fine = water_mask(&cover, side, 3.0, Some(&drawn));
        assert!(
            (1..14).all(|x| fine[at(x, 8)]),
            "at 3 m cells the same 13 are 39 m: a bridge"
        );
        // With no streets nothing is bridged: the dams stay walkable.
        let bare = water_mask(&cover, side, 4.0, None);
        assert!(!bare[at(14, 4)]);
        assert_eq!(bare.iter().filter(|&&w| w).count(), 2 * side - 4 + 1);
    }

    #[test]
    fn the_weight_map_puts_each_cell_on_its_layer() {
        let g = ground(
            2,
            vec![
                Some(LandUse::Park),
                None,
                Some(LandUse::Housing),
                Some(LandUse::Forest),
            ],
        );
        let map = g.weight_map();
        assert_eq!((map.width, map.height), (2, 2));
        assert_eq!(
            map.data,
            vec![
                [255, 0, 0, 0],
                [0, 0, 255, 0],
                [0, 255, 0, 0],
                [255, 0, 0, 0]
            ]
        );
    }

    #[test]
    fn weights_blend_between_cells_and_clamp_at_the_edge() {
        // Park west, street east, 2 m apart.
        let g = ground(
            2,
            vec![Some(LandUse::Park), None, Some(LandUse::Park), None],
        );
        assert_eq!(g.weights_at_local(0.0, 0.0), [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(g.weights_at_local(1.0, 1.0), [0.5, 0.0, 0.5, 0.0]);
        assert_eq!(g.weights_at_local(50.0, -3.0), [0.0, 0.0, 1.0, 0.0]);
    }

    #[test]
    fn a_scatter_sees_natural_layers_and_nothing_on_the_city() {
        // 3 x 3 cells, 2 m apart: the world spans -2..2, cell (1, 1) at 0.
        let mut cover = vec![None; 9];
        cover[4] = Some(LandUse::Forest);
        cover[0] = Some(LandUse::FallowBare);
        let g = ground(3, cover);
        assert_eq!(g.scatter_layer_at(0.0, 0.0), 0);
        assert_eq!(g.scatter_layer_at(-2.0, -2.0), 1, "the north-west cell");
        assert_eq!(g.scatter_layer_at(2.0, 0.0), NOT_NATURAL, "street");
        assert_eq!(g.scatter_layer_at(0.9, -0.9), 0, "the nearest cell");
        assert_eq!(g.scatter_layer_at(-90.0, -90.0), 1, "clamped to the edge");
    }
}
