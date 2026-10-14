//! The Berlin seeder (#1647, P3.2 of #1639; decisions 3, 7, 8 and 9 on
//! #1636): a square's buildings, trees and street furniture written into
//! the room's record as instance sets (#1643), adopted on save and never
//! drawn from the data again.
//!
//! Seeding is a pure function of a record and the street level its square
//! fetched ([`seed`]). The planner draws the square as it always has
//! ([`super::core`]): the theme's buildings on Berlin's footprints, the
//! catalogue's species as tall as Berlin's trees, the theme's props on its
//! surveyed furniture, every one of them clear of the record's own places -
//! its landing, its absolute placements. Each generator the plans grow
//! becomes one set of its copies ([`kiez_of`]), its generator named
//! [`PREFIX`] and its slug, anchored at the origin, snapped, and standing on
//! what the plan says a walker meets there:
//!
//! - **A street building** is its kind's one generator (#1646), its fits
//!   the set's variants, each with the seed the planner rolled for it.
//! - **A landmark or a prop** is its catalogue generator, grown as the lot
//!   layer grows one: finished and ruined by the room's prosperity and
//!   escalation.
//! - **A tree** is its species' generator, re-skin and all, each copy as
//!   tall as Berlin's tree over the height its species grows to
//!   ([`super::core::trees::GROWN_HEIGHT_M`]).
//!
//! A copy is its pose: centimetres along X and Z, its turn clockwise in
//! tenths of a degree, its lift above the ground under it, and its size.
//! Every copy is seeded (decision 9): only the record's own caps bound
//! what is written ([`plant`]), the sets in order buildings, trees,
//! furniture, so a cap cuts furniture first.
//!
//! A record carries the [`mark`] of what it was seeded for
//! ([`crate::pds::GeoSource::seeded`]): the dataset, the square and the
//! scene's seed. A record whose mark is its own is never seeded again,
//! whatever its owner did to its sets since; any other - the square moved,
//! a re-roll, or any edit of the seed the record's terrain carries, which
//! picks the scene - has its seeded content (every generator named
//! [`PREFIX`], its traits and the placements of it) replaced. A street
//! level that left a layer out is never seeded
//! ([`StreetLevel::is_complete`]): the hole would be saved.
//!
//! The ruin a fought-over room rolls for a street building is rolled once a
//! kind, as its kind is one generator: its copies sink alike.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::pds::geo_source::{BERLIN, GeoSource};
pub(crate) use crate::pds::geo_source::{SEEDED_PREFIX as PREFIX, is_seeded_generator};
use crate::pds::instances::{COPY_SCALE_ONE, centimetres, tenths_of_degree};
use crate::pds::sanitize::limits;
use crate::pds::{
    Generator, InstanceCopy, InstanceSolid, InstanceVariant, Placement, RoomRecord, TransformData,
};
use crate::seeded_defaults::fnv1a_64;
use crate::terrain::FinishedHeightMap;
use crate::terrain::geo::street_level::StreetLevel;

use super::fit::RoomScene;
use super::plan::{Grow, Plan, PlannedBuilding, Solid};

/// The smallest and largest a tree's height may scale its species, as
/// Berlin's trees were drawn before they were seeded (#1588): past these
/// the species is the wrong one, not a big or small example of it.
const HEIGHT_SCALE_RANGE: (f32, f32) = (0.3, 3.0);

/// The mark of a record seeded from `source` with the scene `scene_seed`
/// draws ([`crate::pds::GeoSource::seeded`]).
pub(crate) fn mark(source: &GeoSource, scene_seed: u64) -> u64 {
    fnv1a_64(&format!(
        "{}|{}|{}|{}|{scene_seed}",
        source.dataset, source.min_e, source.min_n, source.size_m
    ))
}

/// The mark `record`, owned by `did`, would be seeded with, where it wants
/// seeding: a Berlin room not seeded for its own square and scene. `None`
/// for any other record, and for one seeded already.
pub(crate) fn wanted_mark(record: &RoomRecord, did: &str) -> Option<u64> {
    let source = record
        .geo_source
        .as_ref()
        .filter(|source| source.dataset == BERLIN)?;
    let mark = mark(source, RoomScene::of(did, Some(record)).seed);
    (source.seeded != Some(mark)).then_some(mark)
}

/// What a seeding writes: each generator by name, and the sets of them, in
/// the order they are written.
#[derive(Clone, Debug, Default)]
pub(crate) struct Kiez {
    pub generators: Vec<(String, Generator)>,
    pub sets: Vec<Placement>,
}

/// One set as [`kiez_of`] gathers it.
struct Gathered {
    name: String,
    solid: InstanceSolid,
    variants: Vec<InstanceVariant>,
    copies: Vec<InstanceCopy>,
}

/// The sets `plans` seed, each copy's lift measured from `ground` (see the
/// module docs).
pub(crate) fn kiez_of(plans: &[Plan], ground: &dyn Fn(f32, f32) -> f32) -> Kiez {
    let mut generators: Vec<(String, Generator)> = Vec::new();
    let mut gathered: Vec<Gathered> = Vec::new();
    for plan in plans {
        // Each of the plan's buildings: its set and its variant there.
        let mut of_set: HashMap<(&str, Option<&str>, bool, Solid), usize> = HashMap::new();
        let mut placed: Vec<(usize, u32)> = Vec::with_capacity(plan.buildings.len());
        for building in &plan.buildings {
            let slug = building.entry.slug();
            let key = match building.grow {
                Grow::Built => (slug, None, false, building.solid),
                Grow::Street { .. } => (slug, None, true, building.solid),
                Grow::Plant { variant } => (slug, variant, false, building.solid),
            };
            let set = *of_set.entry(key).or_insert_with(|| {
                let name = unique_name(&generators, slug, key.1);
                generators.push((name.clone(), grown(plan, building)));
                gathered.push(Gathered {
                    name,
                    solid: match building.solid {
                        Solid::Shell => InstanceSolid::Shell,
                        Solid::Box => InstanceSolid::Box,
                        Solid::Trunk => InstanceSolid::Trunk,
                    },
                    variants: Vec::new(),
                    copies: Vec::new(),
                });
                gathered.len() - 1
            });
            let variant = match (building.grow, building.entry.street()) {
                (Grow::Street { fit, variant }, Some(spec)) => {
                    let wanted = InstanceVariant {
                        seed: Some(super::streets::variant_seed(
                            plan.seed ^ fnv1a_64(slug),
                            variant,
                        )),
                        params: spec.fit_values(fit),
                    };
                    let variants = &mut gathered[set].variants;
                    match variants.iter().position(|known| *known == wanted) {
                        Some(at) => at,
                        None => {
                            variants.push(wanted);
                            variants.len() - 1
                        }
                    }
                }
                _ => 0,
            };
            placed.push((set, u32::try_from(variant).unwrap_or(u32::MAX)));
        }
        for copy in &plan.copies {
            let Some(&(set, variant)) = placed.get(copy.building) else {
                continue;
            };
            let building = &plan.buildings[copy.building];
            let tall = copy.height.map_or(1.0, |height| {
                super::core::trees::grown_height(building.entry.slug()).map_or(1.0, |grown| {
                    (height / grown).clamp(HEIGHT_SCALE_RANGE.0, HEIGHT_SCALE_RANGE.1)
                })
            });
            gathered[set].copies.push(copy_at(
                (copy.at, copy.yaw),
                variant,
                building.scale * tall,
                ground,
            ));
        }
    }
    let mut sets = Vec::new();
    for gathered in gathered {
        // A set past what one may hold goes on in another of its generator.
        for copies in gathered.copies.chunks(limits::MAX_INSTANCE_COPIES) {
            sets.push(Placement::Instances {
                generator_ref: gathered.name.clone(),
                transform: TransformData::default(),
                snap_to_terrain: true,
                solid: gathered.solid.clone(),
                variants: gathered.variants.clone(),
                copies: copies.to_vec(),
            });
        }
    }
    // A generator no copy stands from is left out.
    generators.retain(|(name, _)| {
        sets.iter()
            .any(|set| set.generator_ref() == Some(name.as_str()))
    });
    Kiez { generators, sets }
}

/// The generator `building` of `plan` is seeded as: grown once for its set
/// at its own size - a copy carries its scale - and drawn whole, as a
/// detached copy draws it.
fn grown(plan: &Plan, building: &PlannedBuilding) -> Generator {
    let entry_seed = plan.seed ^ fnv1a_64(building.entry.slug());
    match building.grow {
        Grow::Built => crate::terrain::lots::grow_generator(
            building.entry,
            &plan.did,
            entry_seed,
            plan.character,
            1.0,
        ),
        Grow::Street { .. } => crate::terrain::lots::grow_flush(
            building.entry,
            building
                .entry
                .street()
                .expect("a street template is a street building's")
                .generator(),
            entry_seed,
            plan.character,
            1.0,
        ),
        Grow::Plant { variant } => super::plan::grow_plant(building.entry, &plan.did, variant),
    }
}

/// A name for `slug`'s set, and its re-skin's, that no generator gathered
/// so far has.
fn unique_name(taken: &[(String, Generator)], slug: &str, re_skin: Option<&str>) -> String {
    let base = match re_skin {
        Some(variant) => format!("{PREFIX}{slug}_{variant}"),
        None => format!("{PREFIX}{slug}"),
    };
    let free = |name: &str| taken.iter().all(|(held, _)| held != name);
    if free(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}_{n}"))
        .find(|name| free(name))
        .expect("a free name")
}

/// The copy a plan stands at `at`, turned by `yaw` about +Y, of `variant`,
/// at `scale`: its lift measured from `ground` under it, as the instance
/// builder stands it again. A copy's turn runs clockwise, a yaw's the other
/// way, and nothing here goes through platform maths (#1132): each client
/// writes the same numbers.
fn copy_at(
    (at, yaw): (Vec3, f32),
    variant: u32,
    scale: f32,
    ground: &dyn Fn(f32, f32) -> f32,
) -> InstanceCopy {
    let scale_pm = (scale * COPY_SCALE_ONE as f32).round();
    InstanceCopy {
        x_cm: centimetres(at.x),
        z_cm: centimetres(at.z),
        yaw_ddeg: tenths_of_degree(-yaw.to_degrees()),
        variant,
        scale_pm: if scale_pm.is_finite() {
            scale_pm.clamp(1.0, limits::MAX_INSTANCE_SCALE_PM as f32) as u32
        } else {
            COPY_SCALE_ONE
        },
        y_cm: centimetres(at.y - ground(at.x, at.z)),
    }
}

/// Take out of `record` everything a seeding wrote: every generator named
/// [`PREFIX`], its traits and the placements of it. Whether there was any.
pub(crate) fn unplant(record: &mut RoomRecord) -> bool {
    let held = (
        record.generators.len(),
        record.traits.len(),
        record.placements.len(),
    );
    record
        .generators
        .retain(|name, _| !is_seeded_generator(name));
    record.traits.retain(|name, _| !is_seeded_generator(name));
    record
        .placements
        .retain(|placement| !placement.generator_ref().is_some_and(is_seeded_generator));
    held != (
        record.generators.len(),
        record.traits.len(),
        record.placements.len(),
    )
}

/// Write `kiez` into `record` as its seeded content, marked `mark`: any
/// seeded before it taken out first ([`unplant`]).
///
/// No further than the record may hold: its copies, its placements and its
/// items, each counted with the record's own first, the sets written in
/// order until one of them is full - a set's copies cut short where the
/// copies run out, a set left out where the placements or the items do. So
/// the last sets go first, furniture before trees before buildings, and the
/// record holds just what every peer's sanitiser keeps of it: the owner
/// sees what visitors see, and no item of the owner's is cut for one the
/// seeding brought.
pub(crate) fn plant(record: &mut RoomRecord, kiez: &Kiez, mark: u64) {
    unplant(record);
    let mut copies_left =
        limits::MAX_ROOM_INSTANCE_COPIES.saturating_sub(crate::pds::instances::room_copies(record));
    for set in &kiez.sets {
        let (Some(name), Placement::Instances { copies, .. }) = (set.generator_ref(), set) else {
            continue;
        };
        let new_item = !record.generators.contains_key(name);
        if copies_left == 0
            || record.placements.len() >= limits::MAX_PLACEMENTS
            || (new_item && record.generators.len() >= limits::MAX_GENERATORS)
        {
            continue;
        }
        if new_item {
            let Some((_, generator)) = kiez.generators.iter().find(|(held, _)| held == name) else {
                continue;
            };
            record.generators.insert(name.to_owned(), generator.clone());
        }
        let mut set = set.clone();
        if let Placement::Instances { copies: kept, .. } = &mut set {
            kept.truncate(copies_left);
        }
        copies_left -= copies.len().min(copies_left);
        record.placements.push(set);
    }
    if let Some(source) = record.geo_source.as_mut() {
        source.seeded = Some(mark);
    }
}

/// Seed `record`, owned by `did`, from the street level `heightmap`'s
/// ground carries (see the module docs), where it wants seeding and the
/// level is whole. Whether it wrote anything.
pub(crate) fn seed(record: &mut RoomRecord, did: &str, heightmap: &FinishedHeightMap) -> bool {
    let Some(mark) = wanted_mark(record, did) else {
        return false;
    };
    let Some(level) = heightmap
        .ground()
        .and_then(|ground| ground.street_level())
        .filter(|level| level.is_complete())
    else {
        return false;
    };
    let kiez = kiez_for(record, did, level, heightmap);
    plant(record, &kiez, mark);
    true
}

/// The content the street level `level` seeds `record` with, standing on
/// `heightmap`: the plans drawn clear of the record's own places.
pub(crate) fn kiez_for(
    record: &RoomRecord,
    did: &str,
    level: &StreetLevel,
    heightmap: &FinishedHeightMap,
) -> Kiez {
    let scene = RoomScene::of(did, Some(record));
    let kept = super::core::kept_for(Some(record), heightmap);
    let plans = super::core::draw_core(level, &scene, &kept, &|x, z| {
        heightmap.world_height_at(x, z)
    });
    kiez_of(&plans, &|x, z| {
        crate::world_builder::snapped_ground_y(&heightmap.0, x, z, None)
    })
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::seeded_defaults::ThemeArchetype;
    use crate::terrain::geo::GeoGround;
    use crate::terrain::geo::street_level::{CoreBuilding, CoreFurniture, CoreTree};
    use geodata::berlin::{BuildingUse, FurnitureKind};

    /// A walkable ground of one terrace, one linden 16 m tall and one
    /// street lamp, south of the landing.
    pub(crate) fn street_level() -> StreetLevel {
        let outline = vec![
            (-30.0, 40.0),
            (30.0, 40.0),
            (30.0, 52.0),
            (-30.0, 52.0),
            (-30.0, 40.0),
        ];
        StreetLevel::new(
            vec![CoreBuilding {
                id: "DEBE00YY11100001".into(),
                outline,
                usage: BuildingUse::Residential,
                storeys: Some(5),
                peak_storeys: Some(5),
                area: 720.0,
                street_yaw: 0.0,
                development: None,
            }],
            vec![CoreTree {
                id: "00008100:0001".into(),
                x: -20.0,
                z: 30.0,
                genus: Some("Tilia".to_owned()),
                height: Some(16.0),
                crown: Some(9.0),
                girth: Some(140.0),
            }],
            vec![CoreFurniture {
                id: "40477-2210001-00".into(),
                kind: FurnitureKind::Lamp,
                x: 0.0,
                z: 34.0,
                yaw: 0.0,
                length: None,
            }],
        )
    }

    /// A DID whose room is a modern city that grows street lamps.
    pub(crate) fn lit_city() -> String {
        (0..10_000)
            .map(|i| format!("did:plc:lit{i}"))
            .find(|did| {
                let scene = RoomScene::for_did(did);
                scene.theme().0 == ThemeArchetype::ModernCity
                    && !super::super::core::furniture::matches_for(&scene)[&FurnitureKind::Lamp]
                        .is_empty()
            })
            .expect("a lit city")
    }

    /// A heightmap whose ground carries `level`, on a slope so a lift is
    /// measured from somewhere.
    pub(crate) fn heightmap_with(level: StreetLevel) -> FinishedHeightMap {
        let mut heights = bevy_symbios_ground::HeightMap::new(8, 8, 2.0);
        for (i, h) in heights.data_mut().iter_mut().enumerate() {
            *h = (i % 8) as f32 * 0.25;
        }
        let ground = GeoGround::from_cover(8, 2.0, vec![None; 64], None).with_street_level(level);
        FinishedHeightMap(heights, Some(ground))
    }

    /// A Berlin room with no terrain of its own, so its scene is its DID's.
    pub(crate) fn berlin_room() -> RoomRecord {
        let mut record = RoomRecord::default();
        record.generators.clear();
        record.placements.clear();
        record.traits.clear();
        record.default_landing = None;
        record.geo_source = Some(GeoSource::berlin(geodata::GeoSquare {
            min_e: 392_543,
            min_n: 5_815_884,
            size_m: 1_000,
        }));
        record
    }

    /// The seeded sets of `record`.
    fn sets(record: &RoomRecord) -> Vec<(&str, &Vec<InstanceCopy>, &Vec<InstanceVariant>)> {
        record
            .placements
            .iter()
            .filter_map(|placement| match placement {
                Placement::Instances {
                    generator_ref,
                    copies,
                    variants,
                    ..
                } if is_seeded_generator(generator_ref) => {
                    Some((generator_ref.as_str(), copies, variants))
                }
                _ => None,
            })
            .collect()
    }

    /// #1647: a square is seeded as its plans draw it - every planned copy
    /// a set's, of a generator named for its catalogue item, each standing
    /// where its plan stood it, the tree as tall as Berlin's - and marked.
    #[test]
    fn a_square_is_seeded_as_its_plans_draw_it() {
        let did = lit_city();
        let heightmap = heightmap_with(street_level());
        let mut record = berlin_room();
        let level = heightmap.ground().unwrap().street_level().unwrap().clone();
        let plans = super::super::core::draw_core(
            &level,
            &RoomScene::of(&did, Some(&record)),
            &super::super::core::kept_for(Some(&record), &heightmap),
            &|x, z| heightmap.world_height_at(x, z),
        );
        assert!(seed(&mut record, &did, &heightmap), "seeded");
        assert_eq!(wanted_mark(&record, &did), None, "marked");
        let seeded = sets(&record);
        let planned: usize = plans.iter().map(|plan| plan.copies.len()).sum();
        let copies: usize = seeded.iter().map(|(_, copies, _)| copies.len()).sum();
        assert!(planned >= 3, "{planned}");
        assert_eq!(copies, planned, "every planned copy");
        for (name, _, _) in &seeded {
            assert!(record.generators.contains_key(*name), "{name}");
        }
        // Each copy stands where its plan stood it, as the builder stands
        // it again: on the ground under it plus its lift, turned as it was.
        let ground =
            |x: f32, z: f32| crate::world_builder::snapped_ground_y(&heightmap.0, x, z, None);
        let mut stood = seeded.iter().flat_map(|(_, copies, _)| copies.iter());
        for plan in &plans {
            for planned in &plan.copies {
                let copy = stood.next().expect("a copy for each");
                let [x, y, z] = copy.position();
                let at = planned.at;
                assert!(
                    (x - at.x).abs() <= 0.006 && (z - at.z).abs() <= 0.006,
                    "{copy:?} {at}"
                );
                assert!((ground(x, z) + y - at.y).abs() <= 0.02, "{copy:?} {at}");
                assert!(
                    copy.rotation().angle_between(planned.pose().rotation) < 0.002,
                    "{copy:?} {:?}",
                    planned.pose().rotation
                );
            }
        }
        // The linden: as tall as Berlin's, over its species' grown height.
        let (_, trees, _) = seeded
            .iter()
            .find(|(name, _, _)| name.contains("lsys_ternary_props"))
            .expect("the linden's set");
        let grown = super::super::core::trees::grown_height("lsys_ternary_props").unwrap();
        assert_eq!(
            trees[0].scale_pm,
            (16.0 / grown * COPY_SCALE_ONE as f32).round() as u32
        );
        // The terrace: a street kind's one generator, its fit a variant.
        let (_, _, variants) = seeded
            .iter()
            .find(|(_, _, variants)| !variants.is_empty())
            .expect("a street building's set");
        assert!(
            variants
                .iter()
                .all(|v| v.seed.is_some() && v.params.0.contains_key("Frontage"))
        );
    }

    /// Seeding is a function of the record and the street level: twice, the
    /// same record - compared as values, as a generator map serialises in
    /// its own hash order.
    #[test]
    fn seeding_twice_writes_the_same_bytes() {
        let did = lit_city();
        let heightmap = heightmap_with(street_level());
        let (mut one, mut two) = (berlin_room(), berlin_room());
        seed(&mut one, &did, &heightmap);
        seed(&mut two, &did, &heightmap);
        assert_eq!(
            serde_json::to_value(&one).unwrap(),
            serde_json::to_value(&two).unwrap()
        );
    }

    /// A record seeded for its own square is never seeded again, whatever
    /// its owner did to what was seeded; one whose square moved is, its old
    /// content replaced.
    #[test]
    fn a_seeded_record_is_left_alone_and_a_moved_square_reseeds() {
        let did = lit_city();
        let heightmap = heightmap_with(street_level());
        let mut record = berlin_room();
        assert!(seed(&mut record, &did, &heightmap));
        let seeded = record.clone();
        assert!(!seed(&mut record, &did, &heightmap), "left alone");
        // The owner takes a set out: still left alone.
        record.placements.pop();
        let edited = record.clone();
        assert!(!seed(&mut record, &did, &heightmap));
        assert_eq!(
            serde_json::to_value(&record).unwrap(),
            serde_json::to_value(&edited).unwrap()
        );
        // The square moved: seeded anew, the old content gone, nothing
        // doubled.
        if let Some(source) = record.geo_source.as_mut() {
            source.min_e += 1_000;
        }
        assert!(wanted_mark(&record, &did).is_some());
        assert!(seed(&mut record, &did, &heightmap));
        assert_eq!(sets(&record).len(), sets(&seeded).len());
        assert_eq!(record.generators.len(), seeded.generators.len());
    }

    /// A re-roll draws another scene - its terrain's seed - over the same
    /// square: the mark no longer matches, and the square is seeded anew.
    #[test]
    fn a_rerolled_scene_reseeds() {
        use crate::pds::{GeneratorKind, SovereignTerrainConfig};
        let did = lit_city();
        let heightmap = heightmap_with(street_level());
        let terrain = |seed: u64| {
            Generator::from_kind(GeneratorKind::Terrain(SovereignTerrainConfig {
                seed,
                ..Default::default()
            }))
        };
        let mut record = berlin_room();
        record.generators.insert("terrain".into(), terrain(7));
        assert!(seed(&mut record, &did, &heightmap));
        let marked = record.geo_source.as_ref().unwrap().seeded;
        assert_eq!(wanted_mark(&record, &did), None);
        record.generators.insert("terrain".into(), terrain(8));
        assert!(wanted_mark(&record, &did).is_some(), "another scene");
        assert!(seed(&mut record, &did, &heightmap));
        assert_ne!(record.geo_source.as_ref().unwrap().seeded, marked);
    }

    /// #1647: a seeding writes no more than the record may hold, its own
    /// content counted first: where its copies run out the first set is cut
    /// short and the rest left out, and where its items run out no item of
    /// the owner's makes way. The record is then just what every peer's
    /// sanitiser keeps of it.
    #[test]
    fn a_seeding_holds_no_more_than_the_record_may() {
        let did = lit_city();
        let heightmap = heightmap_with(street_level());
        let own_set = |copies: usize| Placement::Instances {
            generator_ref: "post".into(),
            transform: TransformData::default(),
            snap_to_terrain: true,
            solid: InstanceSolid::Shell,
            variants: Vec::new(),
            copies: vec![InstanceCopy::at(0.0, 0.0, 0.0); copies],
        };
        let mut unbounded = berlin_room();
        assert!(seed(&mut unbounded, &did, &heightmap));
        let seeded_sets = sets(&unbounded).len();
        assert!(seeded_sets >= 2, "{seeded_sets}");

        // Two copies left: the first set gets them, the rest none.
        let mut room = berlin_room();
        room.generators
            .insert("post".into(), Generator::default_cuboid());
        let full = limits::MAX_ROOM_INSTANCE_COPIES - 2;
        while crate::pds::instances::room_copies(&room) < full {
            let n =
                (full - crate::pds::instances::room_copies(&room)).min(limits::MAX_INSTANCE_COPIES);
            room.placements.push(own_set(n));
        }
        assert!(seed(&mut room, &did, &heightmap));
        let seeded = sets(&room);
        assert_eq!(seeded.len(), 1, "the first set alone");
        assert_eq!(seeded[0].1.len(), 2);
        assert_eq!(
            crate::pds::instances::room_copies(&room),
            limits::MAX_ROOM_INSTANCE_COPIES
        );
        assert_eq!(wanted_mark(&room, &did), None, "and marked");
        let mut decoded = room.clone();
        decoded.sanitize();
        assert_eq!(
            serde_json::to_value(&decoded.placements).unwrap(),
            serde_json::to_value(&room.placements).unwrap(),
            "what a peer keeps"
        );

        // One item left: the first set's alone, the owner's all kept.
        let mut room = berlin_room();
        for i in 0..limits::MAX_GENERATORS - 1 {
            room.generators
                .insert(format!("own_{i:03}"), Generator::default_cuboid());
        }
        assert!(seed(&mut room, &did, &heightmap));
        assert_eq!(room.generators.len(), limits::MAX_GENERATORS);
        assert!(
            (0..limits::MAX_GENERATORS - 1)
                .all(|i| room.generators.contains_key(&format!("own_{i:03}")))
        );
        let seeded = sets(&room);
        assert!(!seeded.is_empty() && seeded.len() < seeded_sets);
        assert!(seeded.iter().all(|(name, _, _)| *name == seeded[0].0));
    }

    /// A street level that left a layer out is not seeded: the hole would
    /// be saved. Nor is a room that is not Berlin's.
    #[test]
    fn an_incomplete_level_or_another_room_is_not_seeded() {
        let did = lit_city();
        let mut level = street_level();
        level.mark_incomplete();
        let mut record = berlin_room();
        assert!(!seed(&mut record, &did, &heightmap_with(level)));
        assert_eq!(record.geo_source.as_ref().unwrap().seeded, None);
        assert!(record.placements.is_empty());

        let mut procedural = berlin_room();
        procedural.geo_source = None;
        assert_eq!(wanted_mark(&procedural, &did), None);
        assert!(!seed(
            &mut procedural,
            &did,
            &heightmap_with(street_level())
        ));
    }
}
