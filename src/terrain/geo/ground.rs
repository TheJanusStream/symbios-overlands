//! What real Berlin says about a geodata region's ground beside its heights
//! (#1586): the land use covering each cell of the core, and the level of
//! its water.
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
//! Water is [`geodata::water::settle`]'s: one level for the core, the beds
//! carved below it and all other ground kept above it, so the region's one
//! water plane, drawn at that level, shows Berlin's water and nothing else.

use bevy_symbios_ground::WeightMap;
use geodata::berlin::LandUse;

/// The scatter layer of ground that is no natural ground: matched by no
/// biome filter.
pub(crate) const NOT_NATURAL: u8 = u8::MAX;

/// The land use under each cell of a geodata region's core, and the level of
/// its water. Rides in [`super::super::FinishedHeightMap`] with the heights
/// it was decoded beside, so the two land, and go, together.
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
    /// The level of the core's water, metres above sea level, where it has
    /// any.
    water_level: Option<f32>,
}

impl GeoGround {
    /// The level of the core's water, metres above sea level: where the
    /// region draws its water plane. `None` for a dry core.
    pub(crate) fn water_level(&self) -> Option<f32> {
        self.water_level
    }

    /// The splat weight map: one texel per cell, all of it on the cell's
    /// layer.
    pub(crate) fn weight_map(&self) -> WeightMap {
        let side = self.grid as usize;
        WeightMap {
            data: self
                .cover
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

    /// The four layers' weights at heightmap-local `(lx, lz)` metres,
    /// interpolated between the cells around it as the GPU samples the
    /// weight map. Clamped to the core's edge.
    pub(crate) fn weights_at_local(&self, lx: f32, lz: f32) -> [f32; 4] {
        let last = self.grid.saturating_sub(1) as usize;
        let (gx, gz) = (
            (lx / self.cell).clamp(0.0, last as f32),
            (lz / self.cell).clamp(0.0, last as f32),
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
            weights[layer(self.cover[z * (last + 1) + x])] += share;
        }
        weights
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

    /// The land use of the cell nearest world `(x, z)`, the world centring
    /// the core on the origin as it does the heightmap.
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
    /// row from the north-west corner, its water at `water_level`.
    pub(crate) fn from_cover(
        grid: u32,
        cell: f32,
        cover: Vec<Option<LandUse>>,
        water_level: Option<f32>,
    ) -> Self {
        assert_eq!(cover.len(), (grid * grid) as usize, "one cover a cell");
        GeoGround {
            grid,
            cell,
            cover,
            water_level,
        }
    }
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
/// `cell` metres apart): its beds carved, the rest of the ground kept above
/// its level. On an error `heights` is untouched.
pub(crate) fn decode_ground(
    legend: &[u8],
    render: &[u8],
    heights: &mut [f32],
    grid: u32,
    cell: f32,
) -> Result<GeoGround, String> {
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
    let cover: Vec<Option<LandUse>> = classes
        .classes
        .iter()
        .map(|&id| table.get(usize::from(id)).copied().flatten())
        .collect();
    let wet: Vec<bool> = cover.iter().map(|&c| c == Some(LandUse::Water)).collect();
    let water = geodata::water::settle(heights, &wet, grid, grid, cell);
    Ok(GeoGround {
        grid,
        cell,
        cover,
        water_level: water.map(|w| w.level),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground(grid: u32, cover: Vec<Option<LandUse>>) -> GeoGround {
        GeoGround::from_cover(grid, 2.0, cover, None)
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
