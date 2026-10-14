//! Berlin's walkable ground seeded into the record (#1588, #1647; epics
//! #1580, #1636): the buildings, trees and street furniture of a square
//! ([`core`]), planned ([`plan`]) and written into the room's record as
//! instance sets ([`seed`]) once its street level lands, adopted on save
//! and drawn as every set is ([`crate::world_builder::instances`]). Nothing
//! is drawn from the data any more: what a square holds is the record's,
//! and an owner edits a seeded copy as any set's (#1644).
//!
//! [`seed_berlin`] seeds the live record, wherever it is a Berlin room not
//! seeded for its own square and scene, from a heightmap built for its own
//! terrain (a portal's or a square move's old heightmap never seeds the new
//! record) and a whole street level: every visitor seeds an unsaved square
//! the same. In the owner's own room it seeds the stored record as well: a
//! room seeded alike in both reads as saved - a fresh room, or one saved
//! before its street level landed - while a reseeding the owner's square
//! move or re-roll asked for reads unsaved, the stored record being another
//! terrain's until a save. The owner's default is seeded the same way
//! ([`SeededDefault`]), so the World Editor's Reset to default is not
//! offered to an untouched Kiez; a visitor, whose editor never opens here,
//! builds none.

pub(crate) mod core;
pub(crate) mod fit;
pub(crate) mod plan;
pub(crate) mod seed;
pub(crate) mod streets;

use bevy::prelude::*;

use crate::pds::RoomRecord;
use crate::state::{CurrentRoomDid, LiveRoomRecord, StoredRoomRecord, records_differ};

use super::{FinishedHeightMap, HeightMapSource, heightmap_is_current};

/// The room's default record seeded as its records are (#1647): what the
/// World Editor's Reset to default restores and compares the live record
/// with. Held only in the signed-in owner's own room; unseeded where the
/// default is not a Berlin room, and until a heightmap of its own terrain
/// lands where it is one on another terrain than the live record's.
#[derive(Resource, Clone, Debug)]
pub(crate) struct SeededDefault {
    pub did: String,
    pub record: RoomRecord,
}

/// Whether [`seed_berlin`] has anything new to look at: a heightmap landed,
/// or the room or one of its records changed.
pub(crate) fn seeding_due(
    heightmap: Option<Res<FinishedHeightMap>>,
    source: Option<Res<HeightMapSource>>,
    room: Option<Res<CurrentRoomDid>>,
    live: Option<Res<LiveRoomRecord>>,
    stored: Option<Res<StoredRoomRecord>>,
) -> bool {
    heightmap.is_some_and(|it| it.is_changed())
        || source.is_some_and(|it| it.is_changed())
        || room.is_some_and(|it| it.is_changed())
        || live.is_some_and(|it| it.is_changed())
        || stored.is_some_and(|it| it.is_changed())
}

/// Seed the room's square into its records (see the module docs): run when
/// a heightmap lands or a record changes ([`seeding_due`]).
#[allow(clippy::too_many_arguments)]
pub(crate) fn seed_berlin(
    mut commands: Commands,
    heightmap: Option<Res<FinishedHeightMap>>,
    source: Option<Res<HeightMapSource>>,
    room: Option<Res<CurrentRoomDid>>,
    live: Option<ResMut<LiveRoomRecord>>,
    stored: Option<ResMut<StoredRoomRecord>>,
    default: Option<Res<SeededDefault>>,
    signals: Option<ResMut<crate::state::RoomWriteSignals>>,
    session: Option<Res<bevy_symbios_multiuser::auth::AtprotoSession>>,
) {
    let (Some(heightmap), Some(room), Some(mut live)) = (heightmap, room, live) else {
        return;
    };
    // A level that left a layer out seeds nothing; one still to land, and a
    // ground with none, too.
    if !heightmap
        .ground()
        .and_then(|ground| ground.street_level())
        .is_some_and(|level| level.is_complete())
    {
        return;
    }
    let did = room.0.as_str();
    let source = source.as_deref();
    let wants = |record: &RoomRecord| {
        heightmap_is_current(record, source) && seed::wanted_mark(record, did).is_some()
    };
    let live_wants = wants(&live.0);
    // The stored record and the default are the owner's editor's baselines:
    // a visitor seeds neither, and never builds a default room it would not
    // read.
    let owns = session.as_deref().is_some_and(|session| session.did == did);
    let stored_wants = owns && stored.as_ref().is_some_and(|stored| wants(&stored.0));
    // The default: built for a room it was not built for, seeded where it
    // waited on its own terrain's heightmap.
    let default_record = match default.as_deref() {
        _ if !owns => None,
        Some(default) if default.did == did => {
            wants(&default.record).then(|| default.record.clone())
        }
        _ => Some(RoomRecord::default_for_did(did)),
    };
    if !(live_wants || stored_wants || default_record.is_some()) {
        return;
    }
    // A record alike with the live one before it was seeded seeds alike:
    // one planning for both.
    let before = live.0.clone();
    let started = bevy::platform::time::Instant::now();
    if live_wants && seed::seed(&mut live.bypass_change_detection().0, did, &heightmap) {
        let (sets, copies) = seeded_count(&live.0);
        info!(
            "Berlin seeded into the record: {copies} copies in {sets} sets, {:.0} ms",
            started.elapsed().as_secs_f64() * 1_000.0
        );
        live.set_changed();
        // Folded into the edit that asked for it, as the lot layer's
        // writes are: undoing a square move takes its seeding with it,
        // and never leaves an unseeded record to be seeded straight back.
        if let Some(mut signals) = signals {
            signals.derived = true;
        }
    }
    // Any other seeded on its own, where this heightmap is its terrain's.
    let seeded = |record: RoomRecord| {
        if !records_differ(&record, &before) {
            return live.0.clone();
        }
        let mut record = record;
        if heightmap_is_current(&record, source) {
            seed::seed(&mut record, did, &heightmap);
        }
        record
    };
    if stored_wants && let Some(mut stored) = stored {
        stored.0 = seeded(stored.0.clone());
    }
    if let Some(record) = default_record {
        commands.insert_resource(SeededDefault {
            did: did.to_owned(),
            record: seeded(record),
        });
    }
}

/// The seeded sets of `record` and the copies they hold.
fn seeded_count(record: &RoomRecord) -> (usize, usize) {
    record
        .placements
        .iter()
        .filter_map(|placement| match placement {
            crate::pds::Placement::Instances {
                generator_ref,
                copies,
                ..
            } if seed::is_seeded_generator(generator_ref) => Some(copies.len()),
            _ => None,
        })
        .fold((0, 0), |(sets, total), copies| (sets + 1, total + copies))
}

#[cfg(test)]
mod tests {
    use super::seed::tests::{berlin_room, heightmap_with, lit_city, street_level};
    use super::*;
    use crate::pds::geo_source::BERLIN;
    use crate::state::RoomWriteSignals;

    /// A DID whose own default room is a Kiez of Berlin.
    fn berlin_did() -> String {
        (0..10_000)
            .map(|i| format!("did:plc:kiez{i}"))
            .find(|did| {
                crate::seeded_defaults::RegionSource::for_seed(crate::seeded_defaults::fnv1a_64(
                    did,
                ))
                .berlin_square()
                .is_some()
            })
            .expect("a DID whose room is Berlin's")
    }

    /// The seeder as the terrain plugin runs it, over the room `did` owns
    /// with `live` and `stored`, on `heightmap`, its owner signed in.
    fn app(did: &str, live: RoomRecord, stored: RoomRecord, heightmap: FinishedHeightMap) -> App {
        let mut app = App::new();
        app.insert_resource(CurrentRoomDid(did.to_owned()))
            .insert_resource(LiveRoomRecord(live))
            .insert_resource(StoredRoomRecord(stored))
            .insert_resource(heightmap)
            .insert_resource(
                crate::oauth::stand_in::stand_in_session(did, "owner.test").expect("a session"),
            )
            .init_resource::<RoomWriteSignals>()
            .add_systems(Update, seed_berlin.run_if(seeding_due));
        app
    }

    /// Whether `record` is a Berlin room seeded for its own square and
    /// scene.
    fn seeded(record: &RoomRecord, did: &str) -> bool {
        record
            .geo_source
            .as_ref()
            .is_some_and(|source| source.dataset == BERLIN)
            && seed::wanted_mark(record, did).is_none()
    }

    /// `record` with its square moved a kilometre east.
    fn moved(mut record: RoomRecord) -> RoomRecord {
        if let Some(source) = record.geo_source.as_mut() {
            *source = source.moved_to(geodata::GeoSquare {
                min_e: source.min_e + 1_000,
                ..source.square()
            });
        }
        record
    }

    /// #1647: an untouched Kiez - a fresh room, its live and stored records
    /// both the DID's default - is seeded alike in both and in the default:
    /// it reads saved, offers no Reset to default, and the seeding folds
    /// into the undo entry it came with.
    #[test]
    fn an_untouched_kiez_is_seeded_alike_in_live_stored_and_default() {
        let did = berlin_did();
        let default = RoomRecord::default_for_did(&did);
        let mut app = app(
            &did,
            default.clone(),
            default,
            heightmap_with(street_level()),
        );
        app.update();
        let world = app.world();
        let live = &world.resource::<LiveRoomRecord>().0;
        let stored = &world.resource::<StoredRoomRecord>().0;
        let default = world.resource::<SeededDefault>();
        assert!(seeded(live, &did));
        assert!(!records_differ(live, stored), "it reads saved");
        assert_eq!(default.did, did);
        assert!(
            !records_differ(live, &default.record),
            "no Reset to default is offered"
        );
        assert!(world.resource::<RoomWriteSignals>().derived);
        // A frame later nothing more is seeded.
        let before = live.clone();
        app.update();
        assert!(!records_differ(
            &app.world().resource::<LiveRoomRecord>().0,
            &before
        ));
    }

    /// The owner's square move reseeds the live record alone: the stored
    /// one, seeded for the square it holds, is left, and the world reads
    /// unsaved until a save.
    #[test]
    fn the_owners_square_move_reseeds_the_live_record_and_reads_unsaved() {
        let did = lit_city();
        let room = berlin_room();
        let mut app = app(&did, room.clone(), room, heightmap_with(street_level()));
        app.update();
        let live = moved(app.world().resource::<LiveRoomRecord>().0.clone());
        app.world_mut().resource_mut::<LiveRoomRecord>().0 = live;
        app.update();
        let world = app.world();
        let live = &world.resource::<LiveRoomRecord>().0;
        let stored = &world.resource::<StoredRoomRecord>().0;
        assert!(seeded(live, &did) && seeded(stored, &did));
        let mark = |record: &RoomRecord| record.geo_source.as_ref().and_then(|s| s.seeded);
        assert_ne!(mark(live), mark(stored));
        assert!(records_differ(live, stored), "it reads unsaved");
    }

    /// A heightmap built for another terrain - a portal's, or the square's
    /// before it moved - seeds nothing; the record's own does.
    #[test]
    fn a_heightmap_of_another_terrain_seeds_nothing() {
        let did = lit_city();
        let room = berlin_room();
        let mut app = app(&did, room.clone(), room, heightmap_with(street_level()));
        app.insert_resource(HeightMapSource(Some("another terrain".into())));
        app.update();
        let world = app.world();
        assert!(!seeded(&world.resource::<LiveRoomRecord>().0, &did));
        assert!(!seeded(&world.resource::<StoredRoomRecord>().0, &did));
        let own = crate::terrain::terrain_source_key(&world.resource::<LiveRoomRecord>().0);
        app.insert_resource(HeightMapSource(own));
        app.update();
        let world = app.world();
        assert!(seeded(&world.resource::<LiveRoomRecord>().0, &did));
        assert!(seeded(&world.resource::<StoredRoomRecord>().0, &did));
    }

    /// A street level that left a layer out seeds nothing, and holds no
    /// default either: the next whole one seeds them.
    #[test]
    fn an_incomplete_level_seeds_nothing() {
        let did = lit_city();
        let room = berlin_room();
        let mut level = street_level();
        level.mark_incomplete();
        let mut app = app(&did, room.clone(), room, heightmap_with(level));
        app.update();
        let world = app.world();
        assert!(!seeded(&world.resource::<LiveRoomRecord>().0, &did));
        assert!(!seeded(&world.resource::<StoredRoomRecord>().0, &did));
        assert!(!world.contains_resource::<SeededDefault>());
        app.insert_resource(heightmap_with(street_level()));
        app.update();
        let world = app.world();
        assert!(seeded(&world.resource::<LiveRoomRecord>().0, &did));
        assert!(seeded(&world.resource::<StoredRoomRecord>().0, &did));
        assert!(world.contains_resource::<SeededDefault>());
    }

    /// An owner's edit made before the square's data landed: the live and
    /// the stored record, no longer alike, are seeded each on its own - the
    /// same seeding in both - and the world reads unsaved for the edit
    /// alone.
    #[test]
    fn an_edit_before_the_data_lands_is_seeded_round_and_still_unsaved() {
        let did = lit_city();
        let stored = berlin_room();
        let mut live = stored.clone();
        live.environment.sun_illuminance.0 += 1.0;
        let mut app = app(&did, live, stored, heightmap_with(street_level()));
        app.update();
        let world = app.world();
        let live = &world.resource::<LiveRoomRecord>().0;
        let stored = &world.resource::<StoredRoomRecord>().0;
        assert!(seeded(live, &did) && seeded(stored, &did));
        assert!(records_differ(live, stored), "the edit is still unsaved");
        let mut unedited = live.clone();
        unedited.environment.sun_illuminance = stored.environment.sun_illuminance;
        assert!(!records_differ(&unedited, stored), "and nothing else is");
    }

    /// A visitor seeds the live record alone: the stored record and the
    /// default are the owner's editor's, and a visitor never builds the
    /// default room it would not read.
    #[test]
    fn a_visitor_seeds_the_live_record_alone() {
        let did = berlin_did();
        let default = RoomRecord::default_for_did(&did);
        let mut app = app(
            &did,
            default.clone(),
            default,
            heightmap_with(street_level()),
        );
        app.insert_resource(
            crate::oauth::stand_in::stand_in_session("did:plc:guest", "guest.test")
                .expect("a session"),
        );
        app.update();
        let world = app.world();
        assert!(seeded(&world.resource::<LiveRoomRecord>().0, &did));
        assert!(!seeded(&world.resource::<StoredRoomRecord>().0, &did));
        assert!(!world.contains_resource::<SeededDefault>());
    }

    /// A frame with nothing to seed writes nothing: no change tick on the
    /// stored record, the live one or the default, which would read as an
    /// edit (a broadcast, an undo step, a dirty row).
    #[test]
    fn a_frame_with_nothing_to_seed_stamps_no_change() {
        let did = berlin_did();
        let default = RoomRecord::default_for_did(&did);
        let mut app = app(
            &did,
            default.clone(),
            default,
            heightmap_with(street_level()),
        );
        app.update();
        let ticks = |app: &mut App| {
            let world = app.world_mut();
            (
                world.resource_ref::<LiveRoomRecord>().last_changed(),
                world.resource_ref::<StoredRoomRecord>().last_changed(),
                world.resource_ref::<SeededDefault>().last_changed(),
            )
        };
        let before = ticks(&mut app);
        // An edit the seeding has nothing to say to: the seeder runs, and
        // leaves the stored record and the default alone.
        app.world_mut()
            .resource_mut::<LiveRoomRecord>()
            .0
            .environment
            .sun_illuminance
            .0 += 1.0;
        app.update();
        let after = ticks(&mut app);
        assert_ne!(before.0, after.0, "the edit itself");
        assert_eq!((before.1, before.2), (after.1, after.2));
    }

    /// The default on another square than the live record's waits for a
    /// heightmap of its own: the owner who resets to it gets it seeded,
    /// alike with the live record.
    #[test]
    fn a_default_on_another_terrain_waits_for_its_own_heightmap() {
        let did = berlin_did();
        let default = RoomRecord::default_for_did(&did);
        let live = moved(default.clone());
        let key = crate::terrain::terrain_source_key(&live);
        let mut app = app(&did, live.clone(), live, heightmap_with(street_level()));
        app.insert_resource(HeightMapSource(key));
        app.update();
        let world = app.world();
        assert!(seeded(&world.resource::<LiveRoomRecord>().0, &did));
        assert!(
            !seeded(&world.resource::<SeededDefault>().record, &did),
            "not on its own terrain"
        );
        // Reset to default: its own heightmap lands.
        app.world_mut().resource_mut::<LiveRoomRecord>().0 = default.clone();
        app.insert_resource(HeightMapSource(crate::terrain::terrain_source_key(
            &default,
        )));
        app.update();
        let world = app.world();
        let held = &world.resource::<SeededDefault>().record;
        assert!(seeded(held, &did));
        assert!(!records_differ(&world.resource::<LiveRoomRecord>().0, held));
    }
}
