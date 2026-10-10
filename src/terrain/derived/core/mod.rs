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

use super::SourceId;
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

/// The places the record keeps for its own before a street level is known:
/// the landing's disc, then each absolute placement's, with
/// the derived item it is a copy of where it is one made the world's own
/// (#1590). Drawn on the main thread, where the ground is; a detail
/// patch's plans (P4.2, #1597) are drawn on the compute pool against the
/// street level its decode reads there ([`Self::for_level`]).
#[derive(Clone, Debug)]
pub(crate) struct KeptPlaces {
    discs: Vec<((f32, f32, f32), Option<SourceId>)>,
}

impl KeptPlaces {
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
        let mut discs = vec![((landing.0, landing.1, LANDING_CLEAR_M), None)];
        let source = record.and_then(|r| r.geo_source.as_ref());
        // The item a copy made the world's own stands in for.
        let copy_of = |generator: &str| {
            source
                .is_some_and(|source| source.is_adopted_generator(generator))
                .then(|| crate::pds::geo_source::adopted_source_of(generator))
                .flatten()
                .and_then(SourceId::parse)
        };
        for placement in record.map(|r| r.placements.as_slice()).unwrap_or_default() {
            if let Placement::Absolute {
                generator_ref,
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
                discs.push(((x, z, reach + PLACEMENT_MARGIN_M), copy_of(generator_ref)));
            }
        }
        KeptPlaces { discs }
    }

    /// What is kept beside `level`: every place, but the copy of an item
    /// made the world's own (#1590) keeps nothing while `level` - the
    /// street level the plans are drawn from - holds the item: it stands
    /// where Berlin's stood, among its neighbours as they did. Off the
    /// ground its item was on (the square moved since, or the item is a
    /// detail patch's that is not loaded), it is a placement like any other.
    pub(crate) fn for_level(&self, level: Option<&StreetLevel>) -> Kept {
        let discs = self
            .discs
            .iter()
            .filter(|(_, copy)| {
                copy.as_ref()
                    .zip(level)
                    .is_none_or(|(id, level)| !level.holds(id))
            })
            .map(|&(disc, _)| disc)
            .collect();
        Kept { discs }
    }
}

impl Kept {
    /// The places the record keeps beside `level` ([`KeptPlaces::of`],
    /// [`KeptPlaces::for_level`]).
    #[cfg(test)]
    pub(crate) fn of(
        record: Option<&RoomRecord>,
        landing: (f32, f32),
        ground: Option<&crate::world_builder::AnchorGround<'_>>,
        level: Option<&StreetLevel>,
    ) -> Self {
        KeptPlaces::of(record, landing, ground).for_level(level)
    }

    /// Nothing kept, not even a landing: for a plan drawn on its own.
    #[cfg(test)]
    pub(crate) fn nothing() -> Self {
        Kept { discs: Vec::new() }
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

/// What `record` keeps for its own on `heightmap` ([`KeptPlaces::of`],
/// [`KeptPlaces::for_level`]): its
/// landing as a body sets down there, walked ashore, and its absolute
/// placements where they stand. The walkable ground's plans keep clear of
/// it, and - since the ring is walked (P4.1, #1596) - the ring's.
pub(crate) fn kept_for(record: Option<&RoomRecord>, heightmap: &FinishedHeightMap) -> Kept {
    let level = heightmap
        .ground()
        .and_then(|ground| ground.street_level())
        .map(|level| &**level);
    kept_places_for(record, heightmap).for_level(level)
}

/// The places `record` keeps on `heightmap`, before a street level says
/// which adopted copies stand in for its items ([`KeptPlaces`]).
pub(crate) fn kept_places_for(
    record: Option<&RoomRecord>,
    heightmap: &FinishedHeightMap,
) -> KeptPlaces {
    let landing = record
        .and_then(|record| crate::world_builder::compile::landing_on(record, heightmap))
        .map_or((0.0, 0.0), |landing| (landing.pos.0[0], landing.pos.0[1]));
    let water_y = record.and_then(|record| {
        crate::world_builder::compile::drawn_water_level(record, Some(heightmap))
    });
    let anchors = crate::world_builder::AnchorGround::new(heightmap, water_y);
    KeptPlaces::of(record, landing, Some(&anchors))
}

/// The walkable ground's plans for `level`, for `room`, keeping clear of
/// what the record keeps (`kept`), standing on `ground` - the height at
/// world `(x, z)`: the core's, or a detail patch's (P4.2, #1597).
pub(crate) fn draw_core(
    level: &StreetLevel,
    room: &RoomScene,
    kept: &Kept,
    ground: &dyn Fn(f32, f32) -> f32,
) -> Vec<Plan> {
    vec![
        buildings::plan(&level.buildings, room, kept, ground),
        trees::plan(&level.trees, room, kept, ground),
        furniture::plan(&level.furniture, room, kept, ground),
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
        // A copy of an item made the world's own keeps no disc of its own.
        let mut source = crate::pds::GeoSource::berlin(geodata::GeoSquare {
            min_e: 391_000,
            min_n: 5_819_500,
            size_m: 1_000,
        });
        source
            .set_edit("alkis:A", crate::pds::geo_source::Edit::Adopted)
            .unwrap();
        record.geo_source = Some(source);
        record.placements.push(Placement::Absolute {
            generator_ref: "alkis:A#1".to_owned(),
            transform: TransformData {
                translation: Fp3([-200.0, 0.0, -200.0]),
                ..Default::default()
            },
            snap_to_terrain: true,
            avoid_water: false,
            avoid_water_clearance: Fp(0.0),
            seed: None,
        });
        // While the walkable ground draws the item, its copy keeps nothing;
        // off the square, it is a placement like any other.
        let level = StreetLevel::new(
            vec![crate::terrain::geo::street_level::CoreBuilding {
                id: "A".into(),
                outline: vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0)],
                usage: geodata::berlin::BuildingUse::Residential,
                storeys: None,
                peak_storeys: None,
                area: 1.0,
                street_yaw: 0.0,
                development: None,
            }],
            Vec::new(),
            Vec::new(),
        );
        let elsewhere = Kept::of(Some(&record), (50.0, 20.0), None, None);
        assert!(
            !elsewhere.clear(-200.0, -200.0, 1.0),
            "off its square: a placement"
        );
        let kept = Kept::of(Some(&record), (50.0, 20.0), None, Some(&level));
        assert!(
            kept.clear(-200.0, -200.0, 1.0),
            "the adopted copy keeps nothing"
        );
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
        assert!(!Kept::of(None, (0.0, 0.0), None, None).clear(LANDING_CLEAR_M - 1.0, 0.0, 0.5));
    }
}
