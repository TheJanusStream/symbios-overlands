//! The middle ring (#1587, epic #1580): Berlin's buildings round the
//! walkable ground, drawn as the region's own catalogue buildings.
//!
//! Past the walkable ground's walls the city goes on. Over a box reaching
//! [`RING_M`] past them, two renders at [`RING_CELL_M`] a pixel say where
//! its buildings stand: the land use, and the surface model - the ground
//! with everything standing on it - which, less the far field's ground, is
//! how high each thing rises. The ring is cut into lots [`LOT_M`] a side,
//! and a lot is built where enough of it is building: built-up land
//! standing [`MIN_STANDING_M`] or more over the ground ([`decode_ring`]).
//! Its building stands where the lot's building is and faces the lot's
//! nearest street, and how high Berlin's building there stands picks which
//! building it is. The lots nearest the walls come first, and at most
//! [`MAX_RING_BUILDINGS`] are kept: the whole ring of all but the densest
//! squares.
//!
//! What stands on each lot is the region's own: its theme's catalogue
//! buildings, drawn at their catalogue size or smaller by
//! [`crate::terrain::derived::ring`]. The tallest of Berlin's - a church
//! tower, a dome, a high-rise - take the theme's landmarks, and a taller
//! one of the rest a bigger building.
//!
//! The surface model has no way to tell a tree from a roof, so a courtyard
//! of tall trees can read as building. The ring is seen from a kilometre
//! off, where that matters little; the street-level core (#1588) reads the
//! buildings' own footprints instead.
//!
//! Like the far field it is drawn and never walked, and derived again on
//! every visit: none of it is saved.

use geodata::berlin::LandUse;
use geodata::request::Bbox;

use super::far::{FarField, FarPlan};

/// How far past the walkable ground's edge the ring reaches (m).
pub(crate) const RING_M: f32 = 1_000.0;

/// The ring renders' pixel size the plan aims at (m): fine enough to tell
/// a building from its courtyard and from the street.
pub(crate) const RING_CELL_M: f32 = 4.0;

/// The most pixels a ring render has a side.
const RING_GRID_MAX: u32 = 1_024;

/// The fewest pixels a ring render has a side.
const RING_GRID_MIN: u32 = 64;

/// The side of a ring lot (m): one catalogue building at most on each.
pub(crate) const LOT_M: f32 = 30.0;

/// How far a lot's building may stand from the middle of its lot (m), each
/// way: past it, a lot's building would crowd its neighbour's.
const MAX_SHIFT_M: f32 = LOT_M / 4.0;

/// How high over the ground the surface must stand for a pixel to be
/// building rather than ground (m): past cars, walls and hedges.
pub(crate) const MIN_STANDING_M: f32 = 3.0;

/// The share of a lot's pixels that must be building for it to be built.
const BUILT_SHARE: f32 = 0.35;

/// The most lots the ring keeps: past it the nearest the walls are kept. A
/// kilometre of central Berlin round a seeded core is about 4,000.
pub(crate) const MAX_RING_BUILDINGS: usize = 6_000;

/// The ring round a far field's core: the square, out to [`RING_M`] past
/// the core on every side, in renders of `grid` pixels a side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RingPlan {
    /// The renders' box, centred on the square.
    pub bbox: Bbox,
    /// Pixels per side.
    pub grid: u32,
    /// Metres per pixel.
    pub cell: f32,
    /// The core's extent (m).
    pub core_m: f32,
}

impl RingPlan {
    /// The box's side (m).
    pub(crate) fn side_m(&self) -> f32 {
        (self.bbox.max_e - self.bbox.min_e) as f32
    }
}

/// The ring round `far`'s core. Every square with a far field has one, at
/// most the square.
pub(crate) fn ring_plan(far: &FarPlan) -> RingPlan {
    let square = far.square;
    let side = (far.core_m + 2.0 * RING_M)
        .min(square.size_m as f32)
        .round() as i64;
    let (centre_e, centre_n) = square.centre();
    let half = side as f64 / 2.0;
    let (min_e, min_n) = (
        (centre_e - half).round() as i64,
        (centre_n - half).round() as i64,
    );
    let grid = ((side as f32 / RING_CELL_M).ceil() as u32).clamp(RING_GRID_MIN, RING_GRID_MAX);
    RingPlan {
        bbox: Bbox {
            min_e,
            min_n,
            max_e: min_e + side,
            max_n: min_n + side,
        },
        grid,
        cell: side as f32 / grid as f32,
        core_m: far.core_m,
    }
}

/// One lot of the ring, in world coordinates round the square's centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RingLot {
    /// Where its building stands: the middle of the lot's building.
    pub x: f32,
    pub z: f32,
    /// The turn about +Y that faces a catalogue building's front, its local
    /// -Z, to the lot's nearest street.
    pub yaw: f32,
    /// The radius a building standing at `(x, z)` may have and keep to its
    /// lot (m).
    pub room: f32,
    /// How high Berlin's building on the lot rises over the ground (m): as
    /// high as nine tenths of it.
    pub standing: f32,
    /// How far past the walkable ground's edge the lot's building stands
    /// (m).
    pub beyond: f32,
}

/// The ring's lots, nearest the walls first.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Ring {
    lots: Vec<RingLot>,
}

impl Ring {
    pub(crate) fn lots(&self) -> &[RingLot] {
        &self.lots
    }

    /// A ring of `lots`, as given.
    #[cfg(test)]
    pub(crate) fn from_lots(lots: Vec<RingLot>) -> Self {
        Ring { lots }
    }
}

/// Decode the ring renders and cut the ring's lots (see the module docs),
/// measuring how high things stand from `far`'s ground. No lot reaches into
/// the core.
pub(crate) fn decode_ring(
    plan: &RingPlan,
    land_use_legend: &[u8],
    land_use: &[u8],
    surface_legend: &[u8],
    surface: &[u8],
    far: &FarField,
) -> Result<Ring, String> {
    let cover = super::ground::decode_cover(land_use_legend, land_use, plan.grid)?;
    let surface = decode_surface(surface_legend, surface, plan.grid)?;
    let (n, cell) = (plan.grid as usize, plan.cell);
    let half = plan.side_m() / 2.0;
    let centre = |i: usize| -half + (i as f32 + 0.5) * cell;
    let standing: Vec<f32> = (0..n * n)
        .map(|i| surface[i] - far.height_at(centre(i % n), centre(i / n)))
        .collect();
    let building: Vec<bool> = (0..n * n)
        .map(|i| built_up(cover[i]) && standing[i] >= MIN_STANDING_M)
        .collect();
    let to_street = street_distance(&cover, n);
    let pixels = Pixels {
        plan,
        building: &building,
        standing: &standing,
        to_street: &to_street,
    };

    let core_half = plan.core_m / 2.0;
    let across = (plan.side_m() / LOT_M).floor() as usize;
    let lot_centre = |i: usize| -half + (i as f32 + 0.5) * LOT_M;
    let mut lots = Vec::new();
    for b in 0..across {
        for a in 0..across {
            let (lx, lz) = (lot_centre(a), lot_centre(b));
            // A lot that reaches into the core is the walkable ground's.
            if lx.abs() < core_half + LOT_M / 2.0 && lz.abs() < core_half + LOT_M / 2.0 {
                continue;
            }
            lots.extend(pixels.cut_lot(lx, lz).map(|lot| RingLot {
                beyond: lot.x.abs().max(lot.z.abs()) - core_half,
                ..lot
            }));
        }
    }
    // Nearest the walls first, then north to south and west to east.
    lots.sort_by(|p, q| {
        p.beyond
            .total_cmp(&q.beyond)
            .then(p.z.total_cmp(&q.z))
            .then(p.x.total_cmp(&q.x))
    });
    lots.truncate(MAX_RING_BUILDINGS);
    Ok(Ring { lots })
}

/// What the ring renders say per pixel, row-major from the north-west.
struct Pixels<'a> {
    plan: &'a RingPlan,
    /// Whether a building stands there.
    building: &'a [bool],
    /// How high the surface stands over the ground (m).
    standing: &'a [f32],
    /// Four-neighbour steps to the nearest street.
    to_street: &'a [u32],
}

impl Pixels<'_> {
    /// The lot whose middle is `(lx, lz)`, if enough of it is building: its
    /// building stands at the middle of the lot's building pixels, at most
    /// [`MAX_SHIFT_M`] off the lot's middle.
    fn cut_lot(&self, lx: f32, lz: f32) -> Option<RingLot> {
        let (n, cell) = (self.plan.grid as usize, self.plan.cell);
        let half = self.plan.side_m() / 2.0;
        // The pixels whose centres lie in the lot.
        let span = |v: f32| {
            let first = ((v - LOT_M / 2.0 + half) / cell - 0.5).ceil().max(0.0) as usize;
            let last = ((v + LOT_M / 2.0 + half) / cell - 0.5).floor() as usize;
            first..=last.min(n - 1)
        };
        let (mut count, mut heights, mut sum) = (0usize, Vec::new(), (0.0f32, 0.0f32));
        for z in span(lz) {
            for x in span(lx) {
                count += 1;
                let i = z * n + x;
                if self.building[i] {
                    heights.push(self.standing[i]);
                    sum = (sum.0 + x as f32, sum.1 + z as f32);
                }
            }
        }
        if heights.is_empty() || (heights.len() as f32) < BUILT_SHARE * count as f32 {
            return None;
        }
        let built = heights.len() as f32;
        let (px, pz) = (sum.0 / built, sum.1 / built);
        let world = |p: f32| -half + (p + 0.5) * cell;
        let (dx, dz) = (
            (world(px) - lx).clamp(-MAX_SHIFT_M, MAX_SHIFT_M),
            (world(pz) - lz).clamp(-MAX_SHIFT_M, MAX_SHIFT_M),
        );
        heights.sort_by(f32::total_cmp);
        let standing = heights[(heights.len() * 9 / 10).min(heights.len() - 1)];
        let (x, z) = (lx + dx, lz + dz);
        Some(RingLot {
            x,
            z,
            yaw: self.facing(x, z),
            room: LOT_M / 2.0 - dx.abs().max(dz.abs()),
            standing,
            beyond: 0.0,
        })
    }

    /// The turn that faces a catalogue building's front to the street
    /// nearest world `(x, z)`: down the slope of the distance to it. A
    /// rotation by `yaw` about +Y turns the front, local -Z, to
    /// `(-sin yaw, -cos yaw)`, so the front looks down the slope when
    /// `yaw = atan2(dd/dx, dd/dz)`. Unturned where the slope is flat.
    fn facing(&self, x: f32, z: f32) -> f32 {
        let (n, cell) = (self.plan.grid as usize, self.plan.cell);
        let half = self.plan.side_m() / 2.0;
        let pixel = |v: f32| ((v + half) / cell - 0.5).round().clamp(0.0, (n - 1) as f32) as usize;
        let (px, pz) = (pixel(x), pixel(z));
        let d = |x: usize, z: usize| self.to_street[z.min(n - 1) * n + x.min(n - 1)] as f32;
        let (gx, gz) = (
            d(px + 1, pz) - d(px.saturating_sub(1), pz),
            d(px, pz + 1) - d(px, pz.saturating_sub(1)),
        );
        if gx == 0.0 && gz == 0.0 {
            0.0
        } else {
            libm::atan2f(gx, gz)
        }
    }
}

/// Decode a surface render through its legend: `grid` x `grid` heights,
/// metres above sea level.
fn decode_surface(legend: &[u8], render: &[u8], grid: u32) -> Result<Vec<f32>, String> {
    let legend = geodata::legend::parse_value_legend(legend)
        .map_err(|e| format!("Berlin's surface legend could not be read: {e}"))?;
    let image = geodata::raster::decode_png(render, grid, grid)
        .map_err(|e| format!("Berlin's surface could not be read: {e}"))?;
    let heights = geodata::raster::decode_terrain(&image, &legend)
        .map_err(|e| format!("Berlin's surface could not be read: {e}"))?;
    Ok(heights.heights)
}

/// Whether a building on land of this class is one of the city's: the
/// built-up uses, and the weekend cottages.
fn built_up(cover: Option<LandUse>) -> bool {
    matches!(
        cover,
        Some(
            LandUse::Housing
                | LandUse::Mixed
                | LandUse::Core
                | LandUse::Commercial
                | LandUse::PublicSpecial
                | LandUse::Utility
                | LandUse::Cottage
        )
    )
}

/// Four-neighbour steps from each pixel to the nearest street pixel: the
/// street space, which the land use leaves empty. `u32::MAX` everywhere on
/// a render with no street.
pub(super) fn street_distance(cover: &[Option<LandUse>], n: usize) -> Vec<u32> {
    let mut steps = vec![u32::MAX; n * n];
    let mut queue = std::collections::VecDeque::new();
    for (i, c) in cover.iter().enumerate() {
        if c.is_none() {
            steps[i] = 0;
            queue.push_back(i);
        }
    }
    while let Some(i) = queue.pop_front() {
        let (x, z) = (i % n, i / n);
        let next = [
            (x > 0).then(|| i - 1),
            (x + 1 < n).then(|| i + 1),
            (z > 0).then(|| i - n),
            (z + 1 < n).then(|| i + n),
        ];
        for j in next.into_iter().flatten() {
            if steps[j] == u32::MAX {
                steps[j] = steps[i] + 1;
                queue.push_back(j);
            }
        }
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{Fp, SovereignTerrainConfig};
    use crate::terrain::geo::far;
    use geodata::GeoSquare;

    fn fixture(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/crates/geodata/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// The recorded Museumsinsel square, 600 m, round a 198 m core: the
    /// geo job's own small-core test square.
    fn museum_far_plan() -> FarPlan {
        let square = GeoSquare {
            min_e: 391_200,
            min_n: 5_819_700,
            size_m: 600,
        };
        let cfg = SovereignTerrainConfig {
            grid_size: 100,
            cell_scale: Fp(2.0),
            ..Default::default()
        };
        far::far_plan(square, &cfg).expect("a far field")
    }

    /// The square's ring decoded from its recorded renders, on its far
    /// field at the Spree's level.
    fn museum_ring() -> (RingPlan, Ring) {
        let plan = museum_far_plan();
        let far = far::decode_far(
            &plan,
            &fixture("dgm1_legend.json"),
            &fixture("dgm1_391200_5819700_600m_64px.png"),
            &fixture("landuse_legend.json"),
            &fixture("landuse_391200_5819700_600m_64px.png"),
            Some(30.57),
        )
        .unwrap();
        let ring_plan = ring_plan(&plan);
        let ring = decode_ring(
            &ring_plan,
            &fixture("landuse_legend.json"),
            &fixture("landuse_391200_5819700_600m_150px.png"),
            &fixture("dom_legend.json"),
            &fixture("dom_391200_5819700_600m_150px.png"),
            &far,
        )
        .unwrap();
        (ring_plan, ring)
    }

    #[test]
    fn a_ring_reaches_a_kilometre_past_the_core_and_no_further_than_its_square() {
        // The small square is all ring: the recorded renders' own box.
        let small = ring_plan(&museum_far_plan());
        assert_eq!(
            small.bbox,
            Bbox {
                min_e: 391_200,
                min_n: 5_819_700,
                max_e: 391_800,
                max_n: 5_820_300
            }
        );
        assert_eq!((small.grid, small.cell, small.core_m), (150, 4.0, 198.0));
        // A big square's ring is the core and a kilometre round it, centred.
        let square = GeoSquare {
            min_e: 380_000,
            min_n: 5_810_000,
            size_m: 10_000,
        };
        let cfg = SovereignTerrainConfig::default();
        let plan = far::far_plan(square, &cfg).expect("a far field");
        let big = ring_plan(&plan);
        let side = (plan.core_m + 2.0 * RING_M).round() as i64;
        assert_eq!(big.bbox.max_e - big.bbox.min_e, side);
        assert_eq!(big.bbox.max_n - big.bbox.min_n, side);
        assert!((big.bbox.min_e + big.bbox.max_e - 2 * 385_000).abs() <= 1);
        assert!((big.bbox.min_n + big.bbox.max_n - 2 * 5_815_000).abs() <= 1);
        assert!(big.grid <= RING_GRID_MAX && (big.cell - RING_CELL_M).abs() < 0.01);
    }

    /// Cut from the surface model and the land use, the ring's lots stand
    /// where ALKIS - another survey altogether - has buildings: 128 of the
    /// 145 lots of the recorded square have one within the reach their
    /// building may fill. Most of the rest are courtyards of tall trees,
    /// which a surface model cannot tell from roofs.
    #[test]
    fn the_rings_lots_stand_on_berlins_buildings_round_the_core() {
        let (plan, ring) = museum_ring();
        let lots = ring.lots();
        assert_eq!(lots.len(), 145);
        let core_half = plan.core_m / 2.0;
        let mut last = f32::MIN;
        for lot in lots {
            // A building that keeps to its lot keeps off the core.
            assert!(
                lot.x.abs() - lot.room >= core_half || lot.z.abs() - lot.room >= core_half,
                "{lot:?}"
            );
            assert!((LOT_M / 4.0..=LOT_M / 2.0).contains(&lot.room), "{lot:?}");
            assert!(lot.standing >= MIN_STANDING_M, "{lot:?}");
            assert_eq!(lot.beyond, lot.x.abs().max(lot.z.abs()) - core_half);
            assert!(lot.beyond >= last, "nearest the walls first");
            last = lot.beyond;
        }

        let legends = geodata::berlin::STOREYS.layers.iter().map(|layer| {
            geodata::legend::parse_class_legend(&fixture(&format!("storeys_legend_{layer}.json")))
                .unwrap()
        });
        let legend = geodata::legend::ClassLegend::concat(legends).unwrap();
        let table = geodata::berlin::storey_table(&legend);
        let image = geodata::raster::decode_png(
            &fixture("storeys_391200_5819700_600m_300px.png"),
            300,
            300,
        )
        .unwrap();
        let storeys = geodata::raster::decode_classes(&image, &legend).unwrap();
        // The storeys render's 2 m pixels, over the same square.
        let alkis = |col: i32, row: i32| {
            (0..300).contains(&col)
                && (0..300).contains(&row)
                && table
                    .get(usize::from(storeys.classes[(row * 300 + col) as usize]))
                    .copied()
                    .flatten()
                    .is_some()
        };
        let on_buildings = lots
            .iter()
            .filter(|lot| {
                let (col, row) = (
                    ((lot.x + 300.0) / 2.0) as i32,
                    ((lot.z + 300.0) / 2.0) as i32,
                );
                let reach = (lot.room / 2.0) as i32;
                (-reach..=reach).any(|dr| (-reach..=reach).any(|dc| alkis(col + dc, row + dr)))
            })
            .count();
        assert_eq!(on_buildings, 128);
        // And the tall ones are tall: the museums and churches of Mitte.
        let tallest = lots.iter().map(|l| l.standing).fold(0.0, f32::max);
        assert!((50.0..70.0).contains(&tallest), "{tallest}");
    }

    #[test]
    fn a_building_faces_its_nearest_street() {
        // A 20-pixel square of housing with a street down its west edge
        // and along its south edge.
        let n = 20;
        let plan = RingPlan {
            bbox: Bbox {
                min_e: 0,
                min_n: 0,
                max_e: 80,
                max_n: 80,
            },
            grid: n,
            cell: 4.0,
            core_m: 0.0,
        };
        let n = n as usize;
        let cover: Vec<Option<LandUse>> = (0..n * n)
            .map(|i| (i % n != 0 && i / n != n - 1).then_some(LandUse::Housing))
            .collect();
        let to_street = street_distance(&cover, n);
        let pixels = Pixels {
            plan: &plan,
            building: &vec![true; n * n],
            standing: &vec![10.0; n * n],
            to_street: &to_street,
        };
        // The front, local -Z, turned by `yaw` looks along
        // (-sin yaw, -cos yaw).
        let front = |x: f32, z: f32| {
            let yaw = pixels.facing(x, z);
            (-yaw.sin(), -yaw.cos())
        };
        // Near the west street: west, -x.
        let (fx, fz) = front(-30.0, -20.0);
        assert!(fx < -0.99 && fz.abs() < 0.01, "({fx}, {fz})");
        // Near the south street: south, +z.
        let (fx, fz) = front(10.0, 34.0);
        assert!(fz > 0.99 && fx.abs() < 0.01, "({fx}, {fz})");
    }

    #[test]
    fn a_surface_render_that_does_not_decode_says_so() {
        let plan = museum_far_plan();
        let far = far::FarField::from_fn(plan.grid, plan.cell, |_, _| 30.0);
        let error = decode_ring(
            &ring_plan(&plan),
            &fixture("landuse_legend.json"),
            &fixture("landuse_391200_5819700_600m_150px.png"),
            &fixture("dom_legend.json"),
            b"not a png",
            &far,
        )
        .unwrap_err();
        assert!(error.contains("surface could not be read"), "{error}");
    }
}
