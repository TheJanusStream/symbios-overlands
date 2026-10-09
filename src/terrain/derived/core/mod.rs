//! The walkable ground's derived content (#1588): Berlin's buildings, trees
//! and street furniture, drawn from the street level the core fetched
//! ([`crate::terrain::geo::street_level`]) as the region's catalogue items,
//! in three plans the derived stage spawns nearest the landing first.
//!
//! - **Buildings** ([`buildings`]): each ALKIS building as the theme's
//!   catalogue buildings on its footprint, by its use and storeys.
//! - **Trees** ([`trees`]): each inventory tree as the catalogue species
//!   nearest its genus, as tall as the real one.
//! - **Street furniture** ([`furniture`]): each surveyed item of the common
//!   kinds as the theme's matching prop, where the theme has one.
//!
//! Every copy stands on a collider - the walkable ground is walked - and
//! the record's own content comes first: nothing derived stands within
//! reach of an absolute placement (a seeded settlement, an owner's item) or
//! of the landing ([`Kept`]).

pub(crate) mod buildings;
pub(crate) mod furniture;
pub(crate) mod trees;

use crate::pds::{Placement, RoomRecord};
use crate::terrain::FinishedHeightMap;
use crate::terrain::geo::street_level::StreetLevel;

use super::fit::RoomScene;
use super::plan::Plan;

/// How far round the landing nothing derived stands (m): an arrival is
/// never inside a building.
pub(crate) const LANDING_CLEAR_M: f32 = 12.0;

/// How far past an absolute placement's ground radius nothing derived
/// stands (m).
const PLACEMENT_MARGIN_M: f32 = 2.0;

/// The ground radius an absolute placement keeps when it declares none
/// (m).
const PLACEMENT_REACH_M: f32 = 4.0;

/// The places the record keeps for its own: discs `(x, z, radius)` no
/// derived item reaches into.
pub(crate) struct Kept {
    discs: Vec<(f32, f32, f32)>,
}

impl Kept {
    /// The landing at `landing` - where a body sets down, walked ashore
    /// (#1589) - and every absolute placement of `record`, where it stands
    /// on `ground`: a snapped seeded structure walked off water and streets
    /// as the compile walks it, so Berlin's buildings keep clear of the
    /// gate, not of where its record put it.
    pub(crate) fn of(
        record: Option<&RoomRecord>,
        landing: (f32, f32),
        ground: Option<&crate::world_builder::AnchorGround<'_>>,
    ) -> Self {
        let mut discs = vec![(landing.0, landing.1, LANDING_CLEAR_M)];
        for placement in record.map(|r| r.placements.as_slice()).unwrap_or_default() {
            if let Placement::Absolute {
                transform,
                snap_to_terrain,
                avoid_water,
                avoid_water_clearance,
                ..
            } = placement
            {
                let [x, _, z] = match ground.filter(|_| *snap_to_terrain) {
                    Some(ground) => crate::world_builder::snapped_absolute_anchor(
                        ground,
                        transform,
                        *avoid_water,
                        avoid_water_clearance.0,
                    )
                    .to_array(),
                    None => transform.translation.0,
                };
                let reach = if avoid_water_clearance.0 > 0.0 {
                    avoid_water_clearance.0
                } else {
                    PLACEMENT_REACH_M
                };
                discs.push((x, z, reach + PLACEMENT_MARGIN_M));
            }
        }
        Kept { discs }
    }

    /// The landing: the first kept place.
    pub(crate) fn landing(&self) -> (f32, f32) {
        self.discs.first().map_or((0.0, 0.0), |&(x, z, _)| (x, z))
    }

    /// How far `(x, z)` is from the landing, squared: the plans keep the
    /// nearest.
    pub(crate) fn landing_distance2(&self, x: f32, z: f32) -> f32 {
        let (lx, lz) = self.landing();
        (x - lx) * (x - lx) + (z - lz) * (z - lz)
    }

    /// Whether a disc of radius `reach` at `(x, z)` keeps clear of every
    /// kept place.
    pub(crate) fn clear(&self, x: f32, z: f32, reach: f32) -> bool {
        self.discs.iter().all(|&(kx, kz, kr)| {
            let (dx, dz) = (x - kx, z - kz);
            dx * dx + dz * dz >= (kr + reach) * (kr + reach)
        })
    }
}

/// The walkable ground's plans for `level`, for `room`, keeping clear of
/// what `record` keeps - its landing as a body sets down on `heightmap`,
/// walked ashore - standing on `heightmap`.
pub(crate) fn draw_core(
    level: &StreetLevel,
    room: &RoomScene,
    record: Option<&RoomRecord>,
    heightmap: &FinishedHeightMap,
) -> Vec<Plan> {
    let landing = record
        .and_then(|record| crate::world_builder::compile::landing_on(record, heightmap))
        .map_or((0.0, 0.0), |landing| (landing.pos.0[0], landing.pos.0[1]));
    let water_y = record.and_then(|record| {
        crate::world_builder::compile::drawn_water_level(record, Some(heightmap))
    });
    let anchors = crate::world_builder::AnchorGround::new(heightmap, water_y);
    let kept = Kept::of(record, landing, Some(&anchors));
    let ground = |x: f32, z: f32| heightmap.world_height_at(x, z);
    vec![
        buildings::plan(&level.buildings, room, &kept, &ground),
        trees::plan(&level.trees, room, &kept, &ground),
        furniture::plan(&level.furniture, room, &kept, &ground),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{DefaultLanding, Fp, Fp2, Fp3, TransformData};

    #[test]
    fn the_record_keeps_its_landing_and_its_own_places() {
        let mut record = crate::pds::RoomRecord::default_for_did("did:plc:kept");
        record.default_landing = Some(DefaultLanding {
            pos: Fp2([50.0, 20.0]),
            ..Default::default()
        });
        record.placements = vec![
            Placement::Absolute {
                generator_ref: "house".to_owned(),
                transform: TransformData {
                    translation: Fp3([100.0, 3.0, 100.0]),
                    ..Default::default()
                },
                snap_to_terrain: true,
                avoid_water: false,
                avoid_water_clearance: Fp(0.0),
                seed: None,
            },
            Placement::Absolute {
                generator_ref: "villa".to_owned(),
                transform: TransformData {
                    translation: Fp3([-100.0, 3.0, 0.0]),
                    ..Default::default()
                },
                snap_to_terrain: true,
                avoid_water: true,
                avoid_water_clearance: Fp(15.0),
                seed: None,
            },
        ];
        let kept = Kept::of(Some(&record), (50.0, 20.0), None);
        // The landing, not the origin.
        assert!(!kept.clear(50.0, 20.0 + LANDING_CLEAR_M + 0.9, 1.0));
        assert!(kept.clear(50.0, 20.0 + LANDING_CLEAR_M + 1.1, 1.0));
        assert!(kept.clear(0.0, 0.0, 1.0));
        // A placement's declared reach, or the default, and the margin.
        let house = PLACEMENT_REACH_M + PLACEMENT_MARGIN_M;
        assert!(!kept.clear(100.0, 100.0 + house + 0.3, 0.4));
        assert!(kept.clear(100.0, 100.0 + house + 0.5, 0.4));
        let villa = 15.0 + PLACEMENT_MARGIN_M;
        assert!(!kept.clear(-100.0 + villa - 0.1, 0.0, 0.0));
        assert!(kept.clear(-100.0 + villa + 0.1, 0.0, 0.0));
        // No record keeps the origin.
        assert!(!Kept::of(None, (0.0, 0.0), None).clear(LANDING_CLEAR_M - 1.0, 0.0, 0.5));
    }
}
