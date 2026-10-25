//! A place for every craft (#1782): a world seeded on its own ground sites,
//! near its arrival, a jetty on its water with a boat or two moored at it, a
//! pad for a helicopter, a mast for an airship, an airstrip for an airplane
//! along its wind with a windsock beside it, and a track for a car with
//! tyres at its bends. Walking has the paths (#1779).
//!
//! Each is sited on the probe its settlement was, after the settlement, its
//! paths and its signature feature - clear of all of them, of the view down
//! to the landmark and of the places sited before it - and nearest the
//! landing of the ground that takes it - the first that does, read nearest
//! first - the airstrip and the track kept to the view's sides. A craft goes without where no ground near takes its
//! place: a jetty where no water lies near, an airstrip on ground that
//! nowhere runs flat for its length. A settlement that already stands a
//! place for a craft (a boathouse, a steampunk airship dock) sites no
//! second, and a theme with its own (the dock, a space outpost's landing
//! pad) stands that in place of the cross-theme one
//! ([`CatalogueEntry::serves`]).
//!
//! The pad and the mast are built as a building is, kept off the water and
//! snapped onto the highest ground under them. The jetty stands at the
//! water, not on the ground: placed at its deck's height over the sea, its
//! boats afloat beside its head. The airstrip and the track are tracks
//! (`pds::SovereignTrack`), painted as the paths are and kept clear of trees
//! and rocks; their windsock and their tyres stand on the ground beside.
//!
//! The probe under-reads relief (#1773) and reads poorly near the sea
//! (#1777), so each place asks more of it than a building does: flatter
//! ground, more freeboard, a margin all round. What it still misreads the
//! compile settles on the ground it draws
//! (`world_builder::compile::pad::place_stands`): a jetty with no shore
//! along it or too little water at its head, a pad or a mast the water
//! would push off its site or whose ground falls past its footing, and a
//! windsock or tyres in water are not drawn, nor a track mostly under water
//! painted (`terrain::paths::drawn`).

mod jetty;
mod pad;
mod strip;
mod track;

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use super::paths::{distance2, segments_apart};
use super::siting::TerrainProbe;
use super::viewpoint::Viewpoint;
use crate::catalogue::{CatalogueEntry, Craft};
use crate::pds::{Fp, Fp3, Fp4, Generator, Placement, SovereignTrack, TransformData};
use crate::seeded_defaults::ThemeArchetype;

pub use jetty::Jetty;
pub use pad::Standing;
pub use strip::Strip;
pub use track::Track;

/// Sub-stream salt distinct from every sibling room deriver.
const CRAFTS_STREAM_SALT: u64 = 0x1782_C4AF_1782_C4AF;

/// How near the landing a place stands at the least, and the most (m).
const NEAREST: f32 = 30.0;
const FARTHEST: f32 = 170.0;

/// How far a place keeps off the ground of what stands round it (m).
const MARGIN: f32 = 6.0;

/// How far off the view's heading a big place - an airstrip, a track -
/// keeps, as the cosine of that angle (about 35 degrees): beside the view,
/// not across it.
const BESIDE_THE_VIEW: f32 = 0.82;

/// How much freeboard a place's ground keeps over the water, on the probe
/// (m): the probe misses hollows the sea floods on the full map and reads
/// the shore by metres off (#1777, #1782's critic found airstrips in
/// ponds at 0.75).
const FREEBOARD: f32 = 2.0;

/// Ground a place keeps clear, or keeps clear of: every point within `r`
/// of the segment `a`-`b` - a disc where the two are one point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ground {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub r: f32,
}

impl Ground {
    /// A disc `r` round `at`.
    pub fn disc(at: [f32; 2], r: f32) -> Self {
        Self { a: at, b: at, r }
    }

    /// How far apart this and `other` stand, edge to edge: under nought
    /// where they overlap.
    pub fn apart(&self, other: &Ground) -> f32 {
        segments_apart([self.a, self.b], [other.a, other.b]) - self.r - other.r
    }
}

/// What a world's places keep clear of, growing as each is sited.
pub struct Kept(pub Vec<Ground>);

impl Kept {
    /// Whether `ground` keeps `margin` off all that is kept.
    pub fn clear(&self, ground: &Ground, margin: f32) -> bool {
        self.0.iter().all(|kept| kept.apart(ground) >= margin)
    }
}

/// Where a world's places are sited: its probe, its arrival, what they keep
/// clear of, its wind and its theme.
pub struct Siting<'a> {
    pub probe: &'a TerrainProbe,
    pub view: &'a Viewpoint,
    pub kept: Kept,
    /// The world's wind, a direction on the ground (the clouds' and the
    /// vanes').
    pub wind: [f32; 2],
    pub theme: ThemeArchetype,
}

impl Siting<'_> {
    /// The points a probe cell apart - `stride` cells - between
    /// [`NEAREST`] and `farthest` of the landing and `edge` inside the
    /// probe's ground, each with its distance from the landing: nearest
    /// first, and in rows among equals, so the first that takes a place is
    /// every peer's.
    fn lattice(&self, stride: f32, farthest: f32, edge: f32) -> Vec<([f32; 2], f32)> {
        let landing = self.view.landing;
        let step = self.probe.cell_size() * stride;
        let steps = (farthest / step).ceil() as i32;
        let bound = self.probe.half_extent() - edge;
        let mut points = Vec::new();
        for j in -steps..=steps {
            for i in -steps..=steps {
                let at = [landing[0] + i as f32 * step, landing[1] + j as f32 * step];
                let far = distance2(at, landing).sqrt();
                if (NEAREST..=farthest).contains(&far)
                    && at[0].abs() <= bound
                    && at[1].abs() <= bound
                {
                    points.push((at, far));
                }
            }
        }
        points.sort_by(|a, b| a.1.total_cmp(&b.1));
        points
    }

    /// How far `at`'s bearing from the landing stands off the view's
    /// heading, as the cosine of the angle: 1 straight ahead.
    fn off_the_view(&self, at: [f32; 2]) -> f32 {
        let landing = self.view.landing;
        let (dx, dz) = (at[0] - landing[0], at[1] - landing[1]);
        let far = (dx * dx + dz * dz).sqrt();
        if far <= 0.0 {
            return 1.0;
        }
        (dx * self.view.toward[0] + dz * self.view.toward[1]) / far
    }

    /// Whether the probe reads `at` as dry ground with [`FREEBOARD`] over
    /// the water.
    fn dry(&self, at: [f32; 2]) -> bool {
        self.probe.height_between(at) >= self.probe.water_y() + FREEBOARD
    }
}

/// A place for a craft: sited round the arrival, keeping its ground from
/// those sited after it, and wired into the world.
trait Place: Sized {
    /// Its best site, or none where no ground near takes it.
    fn site(siting: &Siting) -> Option<Self>;
    /// The ground it keeps from the places sited after it.
    fn ground(&self) -> Vec<Ground>;
    /// What it adds to the world.
    fn wire(&self, wiring: &mut Wiring);
}

/// A world's places, one a craft at the most, sited by [`site`].
#[derive(Debug, Default)]
pub struct CraftPlaces {
    pub jetty: Option<Jetty>,
    pub pad: Option<Standing>,
    pub mast: Option<Standing>,
    pub strip: Option<Strip>,
    pub track: Option<Track>,
}

/// Site a world's places (see the module docs): `served` the crafts its
/// settlement already stands a place for.
pub fn site(mut siting: Siting, served: &[Craft]) -> CraftPlaces {
    let mut places = CraftPlaces::default();
    // The order each is sited in is the order each keeps clear of the
    // others: the jetty first, the water's edge being scarce.
    fn take<P: Place>(siting: &mut Siting, wanted: bool) -> Option<P> {
        let place = wanted.then(|| P::site(siting)).flatten()?;
        siting.kept.0.extend(place.ground());
        Some(place)
    }
    let wanted = |craft| !served.contains(&craft);
    places.jetty = take(&mut siting, wanted(Craft::Boat));
    places.pad = if wanted(Craft::Helicopter) {
        pad::site(&siting, Craft::Helicopter).inspect(|pad| siting.kept.0.extend(pad.ground()))
    } else {
        None
    };
    places.mast = if wanted(Craft::Airship) {
        pad::site(&siting, Craft::Airship).inspect(|mast| siting.kept.0.extend(mast.ground()))
    } else {
        None
    };
    places.strip = take(&mut siting, wanted(Craft::Airplane));
    places.track = take(&mut siting, wanted(Craft::Car));
    places
}

/// What a world's places add to it: generators by name, their placements,
/// and the tracks its ground paints.
#[derive(Debug, Default)]
pub struct Wiring {
    pub generators: Vec<(String, Generator)>,
    pub placements: Vec<Placement>,
    pub tracks: Vec<SovereignTrack>,
    /// The world's owner, its water line, and its own stream's draws.
    did: String,
    water_y: f32,
    boats: usize,
}

impl CraftPlaces {
    /// What these places add to the world of `did` whose water stands at
    /// `water_y`, drawn from `seed`'s stream of its own.
    pub fn wire(&self, did: &str, water_y: f32, seed: u64) -> Wiring {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ CRAFTS_STREAM_SALT);
        let mut wiring = Wiring {
            did: did.to_string(),
            water_y,
            // One boat or two at the jetty.
            boats: 1 + (rng.next_u32() & 1) as usize,
            ..Default::default()
        };
        if let Some(jetty) = &self.jetty {
            jetty.wire(&mut wiring);
        }
        for standing in self.pad.iter().chain(&self.mast) {
            standing.wire(&mut wiring);
        }
        if let Some(strip) = &self.strip {
            strip.wire(&mut wiring);
        }
        if let Some(track) = &self.track {
            track.wire(&mut wiring);
        }
        wiring
    }

    /// How many places stand.
    pub fn count(&self) -> usize {
        usize::from(self.jetty.is_some())
            + usize::from(self.pad.is_some())
            + usize::from(self.mast.is_some())
            + usize::from(self.strip.is_some())
            + usize::from(self.track.is_some())
    }
}

/// How wide an airstrip is painted (m).
pub fn strip_width() -> f32 {
    strip::WIDTH
}

/// How wide a car's track is painted (m).
pub fn track_width() -> f32 {
    track::WIDTH
}

/// The yaw that turns a part's front (its catalogue -Z) to face `toward`,
/// a direction on the ground - libm's (#1132), as the settlement's.
fn facing(toward: [f32; 2]) -> f32 {
    libm::atan2f(-toward[0], -toward[1])
}

/// How a place's placement stands.
#[derive(Clone, Copy)]
enum Stands {
    /// Built as a building is: snapped onto the highest ground within its
    /// clearance and kept off the water.
    Built(f32),
    /// On the ground under it, the scatters keeping off its clearance by
    /// its name (`compile::scatter::KEPT_BY_NAME`).
    Snapped(f32),
    /// Where it is placed, unsnapped - at the water - the scatters keeping
    /// off its clearance by its name.
    Placed(f32),
}

/// A placement of `generator` at `at` (world XZ) and `y` - its offset over
/// the ground where it is snapped - turned `yaw` about +Y, standing as
/// `stands` says.
fn placed(generator: &str, at: [f32; 2], y: f32, yaw: f32, stands: Stands) -> Placement {
    let half = yaw * 0.5;
    let (snap_to_terrain, avoid_water, clearance) = match stands {
        Stands::Built(clearance) => (true, true, clearance),
        Stands::Snapped(clearance) => (true, false, clearance),
        Stands::Placed(clearance) => (false, false, clearance),
    };
    Placement::Absolute {
        generator_ref: generator.to_string(),
        transform: TransformData {
            translation: Fp3([at[0], y, at[1]]),
            // libm's (#1132), as a member's.
            rotation: Fp4([0.0, libm::sinf(half), 0.0, libm::cosf(half)]),
            scale: Fp3([1.0, 1.0, 1.0]),
        },
        snap_to_terrain,
        avoid_water,
        avoid_water_clearance: Fp(clearance),
        seed: None,
        params: Default::default(),
    }
}

/// The theme's own entry that is a place for `craft`, if it has one.
fn own_entry(theme: ThemeArchetype, craft: Craft) -> Option<&'static dyn CatalogueEntry> {
    crate::catalogue::items::ENTRIES
        .iter()
        .copied()
        .find(|entry| entry.serves() == Some(craft) && entry.themes().contains(&theme))
}

/// A unit direction `degrees` turned from `from` (a unit direction),
/// counter-clockwise seen from above as the yaw turns - libm's.
fn turned(from: [f32; 2], degrees: f32) -> [f32; 2] {
    let (s, c) = (
        libm::sinf(degrees.to_radians()),
        libm::cosf(degrees.to_radians()),
    );
    [from[0] * c + from[1] * s, -from[0] * s + from[1] * c]
}

/// `v` scaled to unit length, or none for no direction.
fn unit(v: [f32; 2]) -> Option<[f32; 2]> {
    let length = (v[0] * v[0] + v[1] * v[1]).sqrt();
    (length > 1e-6).then(|| [v[0] / length, v[1] / length])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A coast: the sea west of x = 0, its floor shelving 8 cm a metre; a
    /// bank rising 6 cm a metre to 3 m east of it, and flat land beyond.
    fn coast() -> TerrainProbe {
        const GRID: usize = 96;
        const CELL: f32 = 10.0;
        let half = (GRID - 1) as f32 * CELL * 0.5;
        let height = |x: f32| {
            if x < 0.0 {
                x * 0.08
            } else {
                (x * 0.06).min(3.0)
            }
        };
        let data = (0..GRID * GRID)
            .map(|i| height((i % GRID) as f32 * CELL - half))
            .collect();
        let map = gen_jobs::HeightmapData {
            width: GRID as u32,
            height: GRID as u32,
            scale: CELL,
            data,
        };
        TerrainProbe::new(&map, 0.0, crate::seeded_defaults::BUILD_SLOPE_LIMIT)
    }

    /// An arrival 120 m in from the coast, looking inland.
    fn view() -> Viewpoint {
        Viewpoint {
            landing: [120.0, 0.0],
            toward: [1.0, 0.0],
            pitch: 0.2,
            score: 0.0,
        }
    }

    fn siting<'a>(probe: &'a TerrainProbe, view: &'a Viewpoint) -> Siting<'a> {
        Siting {
            probe,
            view,
            kept: Kept(vec![Ground::disc(view.landing, 12.0)]),
            wind: [0.0, 1.0],
            theme: ThemeArchetype::Medieval,
        }
    }

    /// On a coast every craft gets its place: the jetty off a bank near the
    /// water line straight out to sea; the pad, the mast,
    /// the strip and the track on the land - each clear of the arrival and of
    /// the others, and the strip and the track beside the view, not across
    /// it; the strip within a third of a quarter turn of the wind, its
    /// upwind end into it.
    #[test]
    fn a_coast_takes_a_place_for_every_craft() {
        let probe = coast();
        let view = view();
        let places = site(siting(&probe, &view), &[]);
        assert_eq!(places.count(), 5, "{places:?}");
        let jetty = places.jetty.as_ref().expect("a jetty");
        assert!(jetty.out[0] < -0.95, "out to sea, west: {:?}", jetty.out);
        let bank = probe.height_between(jetty.land);
        assert!((-1.0..=3.0).contains(&bank), "near the water line: {bank}");
        let mut grounds = vec![Ground::disc(view.landing, 12.0)];
        grounds.extend(jetty.ground());
        let pad = places.pad.expect("a pad");
        let mast = places.mast.expect("a mast");
        assert_eq!((pad.slug, mast.slug), ("helipad", "mooring_mast"));
        grounds.extend(pad.ground());
        grounds.extend(mast.ground());
        let strip = places.strip.expect("a strip");
        let run = strip.run();
        assert!(
            run[1].abs() >= 30f32.to_radians().cos() - 1e-4,
            "near the wind: {run:?}"
        );
        assert!(
            strip.upwind[1] < strip.downwind[1],
            "its upwind end into the wind"
        );
        grounds.extend(strip.ground());
        grounds.extend(places.track.expect("a track").ground());
        for (i, a) in grounds.iter().enumerate() {
            for b in &grounds[i + 1..] {
                assert!(a.apart(b) >= 0.0, "{a:?} overlaps {b:?}");
            }
        }
    }

    /// A craft the settlement already serves gets no second place, and a
    /// theme with its own place for a craft stands that one.
    #[test]
    fn a_served_craft_gets_no_second_and_a_theme_stands_its_own() {
        let probe = coast();
        let view = view();
        let places = site(siting(&probe, &view), &[Craft::Helicopter, Craft::Boat]);
        assert!(places.pad.is_none() && places.jetty.is_none());
        assert!(places.mast.is_some());
        let mut steampunk = siting(&probe, &view);
        steampunk.theme = ThemeArchetype::Steampunk;
        let places = site(steampunk, &[]);
        assert_eq!(places.mast.map(|mast| mast.slug), Some("airship_dock"));
        let mut outpost = siting(&probe, &view);
        outpost.theme = ThemeArchetype::SpaceOutpost;
        let places = site(outpost, &[]);
        assert_eq!(places.pad.map(|pad| pad.slug), Some("landing_pad"));
    }

    /// What the places add: each generator once, by the names the compile
    /// keeps the scatters off (`compile::scatter::KEPT_BY_NAME`), the jetty
    /// at its deck's height over the water, its boats part of it, the pad
    /// and the mast built as buildings are, and the strip and the track
    /// painted at their own widths.
    #[test]
    fn the_places_wire_their_generators_placements_and_tracks() {
        let probe = coast();
        let view = view();
        let places = site(siting(&probe, &view), &[]);
        let wiring = places.wire("did:plc:wiring", 0.0, 7);
        let names: Vec<&str> = wiring
            .generators
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        for name in [
            crate::world_builder::compile::scatter::JETTY,
            crate::world_builder::compile::scatter::WINDSOCK,
            crate::world_builder::compile::scatter::TYRES,
            "craft_pad",
            "craft_mast",
        ] {
            assert_eq!(names.iter().filter(|&&n| n == name).count(), 1, "{name}");
        }
        for placement in &wiring.placements {
            let Placement::Absolute {
                generator_ref,
                transform,
                snap_to_terrain,
                avoid_water,
                ..
            } = placement
            else {
                panic!("an absolute placement");
            };
            let y = transform.translation.0[1];
            match generator_ref.as_str() {
                "craft_jetty" => assert!(
                    !snap_to_terrain
                        && (y - crate::catalogue::items::crafts::jetty::FREEBOARD).abs() < 1e-6
                ),
                "craft_pad" | "craft_mast" => assert!(*snap_to_terrain && *avoid_water),
                _ => assert!(*snap_to_terrain && !*avoid_water),
            }
        }
        let widths: Vec<f32> = wiring.tracks.iter().map(|track| track.width.0).collect();
        assert_eq!(widths, vec![strip::WIDTH, track::WIDTH]);
    }

    /// Ground apart edge to edge: a disc and a way round a segment, a
    /// crossing, and an overlap read under nought.
    #[test]
    fn ground_reads_apart_edge_to_edge() {
        let way = Ground {
            a: [0.0, 0.0],
            b: [20.0, 0.0],
            r: 3.0,
        };
        assert!((way.apart(&Ground::disc([10.0, 10.0], 2.0)) - 5.0).abs() < 1e-5);
        let across = Ground {
            a: [10.0, -5.0],
            b: [10.0, 5.0],
            r: 1.0,
        };
        assert!((way.apart(&across) + 4.0).abs() < 1e-5);
        assert!(way.apart(&Ground::disc([30.0, 0.0], 2.0)) > 4.9);
    }

    /// A turn of a quarter, as a yaw turns: -Z (straight on) to -X (the
    /// left), and back the other way.
    #[test]
    fn a_turn_turns_as_a_yaw_does() {
        let left = turned([0.0, -1.0], 90.0);
        assert!(
            (left[0] + 1.0).abs() < 1e-6 && left[1].abs() < 1e-6,
            "{left:?}"
        );
        let back = turned(left, -90.0);
        assert!(
            back[0].abs() < 1e-6 && (back[1] + 1.0).abs() < 1e-6,
            "{back:?}"
        );
        // A yaw's own turn takes the front the same way.
        let yaw = facing([0.0, -1.0]) + 90f32.to_radians();
        let front = [-libm::sinf(yaw), -libm::cosf(yaw)];
        assert!((front[0] - left[0]).abs() < 1e-6 && (front[1] - left[1]).abs() < 1e-6);
    }
}
