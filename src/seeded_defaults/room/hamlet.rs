//! A seeded hamlet (#1797): a world seeded on its own ground whose theme
//! lives in villages - a farm, a medieval village, a Norse steading, a
//! feudal Japanese village, a Mesoamerican town, a frontier town, a
//! solarpunk commune, a suburb - stands a row of houses along a path past
//! its landmark's ground: its theme's detached house
//! ([`StreetKind::Detached`], #1600), each on a lot and at a seed of its
//! own, fronting the path (#1780) on whichever side its ground takes it.
//!
//! The houses are one instance set ([`HAMLET`], `Placement::Instances`,
//! #1643): the kind's one generator (#1646), each house a copy and a
//! variant - its seed, and its lot's frontage, depth and storeys as the
//! grammar's values - drawn merged, and met as a building's voxel shell. A
//! set draws its generator's geometry alone, so a hamlet's chimneys do not
//! smoke: a calm world's smoking hearths (#1782) are its members'.
//!
//! The path is one of the settlement's (#1779), or a lane of the hamlet's
//! own: a straight path leaving one of them - from a spot every
//! [`ANCHOR_STEP`] down each, along any of [`BEARINGS`] bearings - for as
//! far as its ground takes a path, [`LANE_MOST`] at the most, and on
//! [`LANE_PAST`] past its last house. A hamlet is one row: each house
//! within [`ROW_GAP`] down its way of the last, a lane's first that near
//! its start. The way whose row takes the most houses is the hamlet's, a
//! path of the settlement's before a lane and a shorter lane before a
//! longer; a world whose ways take fewer than [`LEAST`] has none.
//! A lane is one more of the paths the terrain paints, though no props line
//! it.
//!
//! Each house is sited on the probe its settlement was, last of all it
//! sites - after its paths and the props that line them, its signature
//! feature, its places for crafts and its fields, so a hamlet takes no
//! craft's place - clear of all of them, the gate, the monument and the
//! view down to the landmark, and on ground a footing covers, as a member's is
//! ([`super::settlement`]'s relief rule), with the freeboard a craft's
//! place keeps - the compile walks no copy off the water. The compile
//! stands each as it does a member, sunk on the highest ground within its
//! reach (`world_builder::compile::pad::copy_footing`): its lot and the
//! steps before it, the ground the scatters keep off.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use super::crafts::{Ground, Kept};
use super::paths::{distance2, off_segment};
use super::settlement::{MEMBER_SINK, RELIEF_MARGIN, SITE_SLOPE, facing};
use super::siting::{FootprintGround, TerrainProbe};
use crate::catalogue::items::foundation;
use crate::catalogue::items::street::{StreetFit, StreetKind};
use crate::catalogue::{CatalogueEntry, StructureRole, entries_for_room};
use crate::pds::instances::{COPY_SCALE_ONE, centimetres, tenths_of_degree};
use crate::pds::{
    Generator, InstanceCopy, InstanceSolid, InstanceVariant, Placement, TransformData,
};
use crate::seeded_defaults::{LandformArchetype, ProsperityTier, SceneCharacter, ThemeArchetype};
pub(crate) use crate::world_builder::compile::scatter::HAMLET;
use crate::world_builder::compile::scatter::house_reach;

/// Sub-stream salt distinct from every sibling room deriver.
const HAMLET_STREAM_SALT: u64 = 0x1797_4A31_E7AB_1797;

/// The fewest houses a hamlet stands.
pub const LEAST: usize = 3;

/// How far apart down its path the spots a house is tried at are (m).
const STEP: f32 = 2.0;

/// How far past its path's edge a house's ground keeps (m) - its ground
/// reaches its steps, and its garden fronts the way - and off the ground of
/// all else, a building's ground being a spacing circle already.
const SETBACK: f32 = 1.0;

/// How far short of its [`SETBACK`] a house's ground may come of what it
/// keeps off (m): a rounding, as its own path stands exactly that far off.
const ROUNDING: f32 = 0.01;

/// How far apart two houses' grounds keep (m).
const GAP: f32 = 1.5;

/// How far a house's ground keeps off the primary landmark's (m): the
/// landmark stands in a space of its own, the hamlet past it.
const PAST_LANDMARK: f32 = 8.0;

/// How far inside the probe's ground a house's ground keeps, and a lane
/// runs (m).
const EDGE: f32 = 10.0;

/// How far from the landing a house stands at the most (m): the hamlet is
/// the settlement's, near its arrival as its fields are (`fields::FARTHEST`).
/// Without it, eleven hamlets of 39 in seeds 0..300 stood 280 to 800 m out,
/// at a far member's door.
const FARTHEST: f32 = 220.0;

/// How far over the water a house's ground stands at the least, on the
/// probe (m): the primary landmark's (`settlement::LANDMARK_FREEBOARD`), as
/// the full map's ground lies up to about 3.5 m under the probe's near the
/// water and the compile walks no copy off it. At a craft's place's 2 m,
/// three houses of 379 in seeds 0..300 stood in the sea.
const FREEBOARD: f32 = 4.0;

/// How far apart down the settlement's paths the spots a lane may leave
/// them from are (m).
const ANCHOR_STEP: f32 = 6.0;

/// How many bearings, evenly round, a lane is tried along.
const BEARINGS: usize = 16;

/// How far from the path it leaves a lane's first spot is (m): past that
/// path's edge and its own.
const LANE_START: f32 = 6.0;

/// How long a lane runs at the most (m).
const LANE_MOST: f32 = 150.0;

/// How far past the spot of its last house a lane runs (m).
const LANE_PAST: f32 = 6.0;

/// How far down its way a house stands from the last at the most (m), and a
/// lane's first from its start: past that the row has ended. Without it, of
/// a world's paths 500 m long, one stood two of its hamlet's seven houses at
/// a far member's door, 350 m from the rest.
const ROW_GAP: f32 = 30.0;

/// The most houses `theme`'s hamlet stands, or `None` for a theme that
/// stands none: each that farms (`super::fields`), and the suburb.
pub fn most_houses(theme: ThemeArchetype) -> Option<usize> {
    use ThemeArchetype as T;
    match theme {
        T::Suburban => Some(8),
        T::Medieval | T::FeudalJapan => Some(7),
        T::Mesoamerican | T::Solarpunk => Some(6),
        T::RuralFarmland | T::Nordic | T::WildWest => Some(5),
        _ => None,
    }
}

/// A world's hamlet: its theme's detached house, the seed its one
/// generator grows at, where each house stands, and its lane where it has
/// one of its own - a straight path, from where it leaves the settlement's
/// to its end.
pub struct Hamlet {
    pub entry: &'static dyn CatalogueEntry,
    pub seed: u64,
    pub houses: Vec<House>,
    pub lane: Option<[[f32; 2]; 2]>,
}

/// One house of a hamlet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct House {
    /// Its lot's middle (world XZ).
    pub at: [f32; 2],
    /// Its yaw, its front to its path.
    pub yaw_rad: f32,
    /// Its lot and storeys.
    pub fit: StreetFit,
    /// The seed its grammar draws with.
    pub seed: u64,
}

impl House {
    /// The ground it keeps, `(spot, radius)`: its building's reach round its
    /// lot's middle ([`reach`]).
    pub fn ground(&self) -> ([f32; 2], f32) {
        (self.at, reach(self.fit))
    }
}

impl Hamlet {
    /// Its one generator: its kind's (#1646), finished and ruined by the
    /// world's prosperity and escalation as a Berlin street building is -
    /// it leans nowhere, and its copies sink alike.
    pub fn generator(&self, prosperity: f32, escalation: f32) -> Generator {
        let spec = self
            .entry
            .street()
            .expect("a hamlet's house is a street building");
        crate::terrain::grow_flush(
            self.entry,
            spec.generator(),
            self.seed,
            (prosperity, escalation),
            1.0,
        )
    }

    /// Its placement: one set of its houses, snapped, each house a copy and
    /// a variant of its own - its seed and its lot's grammar values.
    pub fn placement(&self) -> Placement {
        let spec = self
            .entry
            .street()
            .expect("a hamlet's house is a street building");
        Placement::Instances {
            generator_ref: HAMLET.to_string(),
            transform: TransformData::default(),
            snap_to_terrain: true,
            solid: InstanceSolid::Shell,
            variants: self
                .houses
                .iter()
                .map(|house| InstanceVariant {
                    seed: Some(house.seed),
                    params: spec.fit_values(house.fit),
                })
                .collect(),
            copies: self
                .houses
                .iter()
                .enumerate()
                .map(|(variant, house)| InstanceCopy {
                    x_cm: centimetres(house.at[0]),
                    z_cm: centimetres(house.at[1]),
                    // A copy turns clockwise, a yaw counter-clockwise.
                    yaw_ddeg: tenths_of_degree(-house.yaw_rad.to_degrees()),
                    variant: variant as u32,
                    scale_pm: COPY_SCALE_ONE,
                    // Sunk as a member is, on the highest ground under it.
                    y_cm: centimetres(-MEMBER_SINK),
                })
                .collect(),
        }
    }
}

/// How far a house of `fit` reaches from its lot's middle, turned any way
/// (m): the ground the scatters keep off round it
/// (`compile::scatter::house_reach`), so the two never differ.
pub(crate) fn reach(fit: StreetFit) -> f32 {
    house_reach(fit.frontage_m(), fit.depth_m())
}

/// A house's lot and storeys, drawn by the world's prosperity: a poor
/// world's small and of one storey, a rich world's broad.
fn drawn_fit(rng: &mut ChaCha8Rng, prosperity: ProsperityTier) -> StreetFit {
    let (frontages, depths, storeys): (&[u16], &[u16], &[u8]) = match prosperity {
        ProsperityTier::Poor => (&[8, 10], &[8, 10], &[1]),
        ProsperityTier::Modest => (&[8, 10, 12], &[8, 10, 12], &[1, 2]),
        ProsperityTier::Rich => (&[10, 12, 16], &[10, 12], &[1, 2, 2]),
    };
    let frontage = frontages[rng.next_u32() as usize % frontages.len()];
    let depth = depths[rng.next_u32() as usize % depths.len()];
    let storeys = storeys[rng.next_u32() as usize % storeys.len()];
    StreetFit::new(frontage, depth, storeys, false).snapped(StreetKind::Detached)
}

/// What a hamlet keeps off: all its world keeps clear (`kept`), and its
/// primary landmark's ground - and its world's landing, which its houses
/// keep near.
struct Grounds<'a> {
    kept: &'a Kept,
    landmark: Ground,
    landing: [f32; 2],
}

/// The hamlet `scene`'s theme stands along the paths `lines` (each a
/// polyline, `width` wide) over `probe`, or a lane leaving them, within
/// [`FARTHEST`] of the `landing`, past the ground of the primary `landmark`,
/// `(spot, radius)`, and clear of `kept` - every building and prop of the
/// settlement, every path, the gate, the monument, the view down to the
/// landmark, the feature and the places for crafts and the fields sited
/// before it - drawn by the world's `seed`. None where its theme stands no
/// hamlet, its land is craggy, or no way takes [`LEAST`] houses.
#[allow(clippy::too_many_arguments)]
pub fn site(
    probe: &TerrainProbe,
    landing: [f32; 2],
    landmark: ([f32; 2], f32),
    lines: &[Vec<[f32; 2]>],
    width: f32,
    kept: &Kept,
    scene: &SceneCharacter,
    seed: u64,
) -> Option<Hamlet> {
    // Craggy land's fine relief lies under the probe's cells (#1788): its
    // houses would stand over daylight, or in the sea, where it read dry
    // ground - eight of the ten over daylight in seeds 0..300, and the one
    // a stricter reading of its ground moved into the sea, were craggy
    // land's.
    if scene.landform == LandformArchetype::Craggy {
        return None;
    }
    let most = most_houses(scene.theme)?;
    let entry = entries_for_room(
        scene.theme,
        StructureRole::Secondary,
        scene.prosperity_tier(),
        scene.escalation_tier(),
    )
    .find(|entry| {
        entry
            .street()
            .is_some_and(|spec| spec.kind == StreetKind::Detached)
    })?;
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ HAMLET_STREAM_SALT);
    // Every draw first, so a house the ground does not take moves no other.
    let grown = rng.next_u64();
    let drawn: Vec<(StreetFit, u64)> = (0..most)
        .map(|_| {
            let fit = drawn_fit(&mut rng, scene.prosperity_tier());
            (fit, rng.next_u64())
        })
        .collect();
    let half = width * 0.5;
    let grounds = Grounds {
        kept,
        landmark: Ground::disc(landmark.0, landmark.1),
        landing,
    };
    // The settlement's own paths first, the first of equals.
    let mut houses: Vec<House> = Vec::new();
    for line in lines {
        let along = along(probe, line, half, &grounds, &drawn);
        if along.len() > houses.len() {
            houses = along;
        }
    }
    // Then a lane, where no path takes them all: one with more houses, or
    // as many on a shorter lane than the last.
    let mut lane: Option<([[f32; 2]; 2], f32)> = None;
    if houses.len() < most {
        for line in lines {
            for (_, from, _) in spots(line, ANCHOR_STEP) {
                for bearing in 0..BEARINGS {
                    let turn = bearing as f32 * std::f32::consts::TAU / BEARINGS as f32;
                    // libm's (#1132): the lanes decide where houses stand.
                    let toward = [libm::sinf(turn), libm::cosf(turn)];
                    let Some((beside, length)) =
                        lane_of(probe, from, toward, half, &grounds, &drawn)
                    else {
                        continue;
                    };
                    let shorter = lane.is_some_and(|(_, best)| length < best);
                    if beside.len() > houses.len() || beside.len() == houses.len() && shorter {
                        let end = [from[0] + toward[0] * length, from[1] + toward[1] * length];
                        houses = beside;
                        lane = Some(([from, end], length));
                    }
                }
            }
        }
    }
    (houses.len() >= LEAST).then_some(Hamlet {
        entry,
        seed: grown,
        houses,
        lane: lane.map(|(ends, _)| ends),
    })
}

/// The spots `step` metres apart down `line` from its start, each with how
/// far down the line it is and the way the line runs there (a unit
/// direction).
fn spots(line: &[[f32; 2]], step: f32) -> Vec<(f32, [f32; 2], [f32; 2])> {
    let mut spots = Vec::new();
    // How far down the line the stretch ahead starts, and how far into it
    // its next spot is.
    let (mut start, mut ahead) = (0.0, 0.0);
    for pair in line.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let length = distance2(a, b).sqrt();
        if length <= 0.0 {
            continue;
        }
        let down = [(b[0] - a[0]) / length, (b[1] - a[1]) / length];
        while ahead <= length {
            let at = [a[0] + down[0] * ahead, a[1] + down[1] * ahead];
            spots.push((start + ahead, at, down));
            ahead += step;
        }
        start += length;
        ahead -= length;
    }
    spots
}

/// The longest row of the houses of `drawn`, in their order, that stand
/// beside the path `line`, `half` its half width - the first of equals: at
/// each of its spots, [`STEP`] apart, the next house where it stands
/// ([`beside`]), a row ending where none has stood for [`ROW_GAP`] and the
/// next starting with the first house again.
fn along(
    probe: &TerrainProbe,
    line: &[[f32; 2]],
    half: f32,
    grounds: &Grounds<'_>,
    drawn: &[(StreetFit, u64)],
) -> Vec<House> {
    let (mut longest, mut row): (Vec<House>, Vec<House>) = (Vec::new(), Vec::new());
    // How far down the line the row's last house stands.
    let mut last = 0.0;
    for (t, at, down) in spots(line, STEP) {
        if !row.is_empty() && t - last > ROW_GAP {
            let ended = std::mem::take(&mut row);
            if ended.len() > longest.len() {
                longest = ended;
            }
        }
        let Some(&next) = drawn.get(row.len()) else {
            break;
        };
        if let Some(house) = beside(probe, at, down, half, grounds, &row, next) {
            row.push(house);
            last = t;
        }
    }
    if row.len() > longest.len() {
        longest = row;
    }
    longest
}

/// The houses of `drawn`, in their order, that stand beside a lane leaving
/// a path at `from` along the unit direction `toward`, and how long the lane
/// runs: none where its first stretch, to [`LANE_START`], comes near what
/// the world keeps clear but the way it leaves; then on, a [`STEP`] at a
/// time while its ground takes a path - inside the probe's ground, dry and
/// walkable as a path's (`paths::FREEBOARD`, `paths::MAX_SLOPE`), and clear
/// of all the world keeps clear - the next house beside each spot
/// ([`beside`]), until none has stood for [`ROW_GAP`], and the lane on
/// [`LANE_PAST`] past its last house's spot where its ground takes it so
/// far. None where no house stands within [`ROW_GAP`] of its start.
fn lane_of(
    probe: &TerrainProbe,
    from: [f32; 2],
    toward: [f32; 2],
    half: f32,
    grounds: &Grounds<'_>,
    drawn: &[(StreetFit, u64)],
) -> Option<(Vec<House>, f32)> {
    let bound = probe.half_extent() - EDGE;
    // Its first stretch keeps clear of all but the way it leaves: what
    // `from` stands on.
    let start = [
        from[0] + toward[0] * LANE_START,
        from[1] + toward[1] * LANE_START,
    ];
    let stretch = Ground {
        a: from,
        b: start,
        r: half,
    };
    let clear = grounds
        .kept
        .0
        .iter()
        .filter(|kept| off_segment(from, kept.a, kept.b) > kept.r)
        .all(|kept| kept.apart(&stretch) >= 0.0);
    if !clear {
        return None;
    }
    let mut houses = Vec::new();
    // The furthest spot its ground takes, and its last house's.
    let (mut walked, mut last) = (None, None);
    let mut t = LANE_START;
    while t <= LANE_MOST {
        let at = [from[0] + toward[0] * t, from[1] + toward[1] * t];
        let way = Ground::disc(at, half);
        let walks = at[0].abs() <= bound
            && at[1].abs() <= bound
            && probe.height_between(at) >= probe.water_y() + super::paths::FREEBOARD
            && probe.slope_at(at) <= super::paths::MAX_SLOPE
            && grounds.kept.clear(&way, SETBACK);
        if !walks {
            break;
        }
        walked = Some(t);
        // Its row has ended, or never began near its start.
        if t - last.unwrap_or(LANE_START) > ROW_GAP {
            break;
        }
        match drawn.get(houses.len()) {
            Some(&next) => {
                if let Some(house) = beside(probe, at, toward, half, grounds, &houses, next) {
                    houses.push(house);
                    last = Some(t);
                }
            }
            None if last.is_some_and(|last| t >= last + LANE_PAST) => break,
            None => {}
        }
        t += STEP;
    }
    let last = last?;
    let walked = walked.unwrap_or(last);
    Some((houses, walked.min(last + LANE_PAST)))
}

/// The house `fit` at `seed` beside the spot `at` of a way running `down`
/// there, `half` its half width: on its left, or else on its right, its
/// ground [`SETBACK`] off the way's edge, where its middle is within
/// [`FARTHEST`] of the landing, its ground keeps inside the probe's ground,
/// clear of `grounds` and of `houses`, and the probe's ground takes it
/// ([`takes`]).
fn beside(
    probe: &TerrainProbe,
    at: [f32; 2],
    down: [f32; 2],
    half: f32,
    grounds: &Grounds<'_>,
    houses: &[House],
    (fit, seed): (StreetFit, u64),
) -> Option<House> {
    let bound = probe.half_extent() - EDGE;
    let r = reach(fit);
    let off = half + SETBACK + r;
    for side in [1.0_f32, -1.0] {
        let out = [-down[1] * side, down[0] * side];
        let middle = [at[0] + out[0] * off, at[1] + out[1] * off];
        let ground = Ground::disc(middle, r);
        let clear = distance2(middle, grounds.landing) <= FARTHEST * FARTHEST
            && middle[0].abs() <= bound - r
            && middle[1].abs() <= bound - r
            && ground.apart(&grounds.landmark) >= PAST_LANDMARK
            && grounds.kept.clear(&ground, SETBACK - ROUNDING)
            && houses.iter().all(|house| {
                let (at, r) = house.ground();
                Ground::disc(at, r).apart(&ground) >= GAP
            });
        if !clear {
            continue;
        }
        if takes(probe, middle, fit) {
            return Some(House {
                at: middle,
                yaw_rad: facing([-out[0], -out[1]]),
                fit,
                seed,
            });
        }
    }
    None
}

/// Whether the probe's ground at `at` takes a house of `fit`: dry with
/// [`FREEBOARD`], no steeper than a member's site and within the relief its
/// footing covers over its [`reach`] - the ground it stands on - as the full
/// map is taken to have it, as a member's ground is
/// (`settlement::member_fits`).
fn takes(probe: &TerrainProbe, at: [f32; 2], fit: StreetFit) -> bool {
    let footing = reach(fit);
    let covers = foundation::required_depth(footing) + MEMBER_SINK;
    let refuses = |ground: &FootprintGround| {
        ground.relief * RELIEF_MARGIN > covers
            || ground.steepest > SITE_SLOPE
            || !ground.is_dry()
            || ground.above_water < FREEBOARD
    };
    probe.footprint_unless(at, footing, refuses).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A probe of level ground `height` over a sea at nought, 640 m across.
    fn level(height: f32) -> TerrainProbe {
        let grid = 65;
        let map = gen_jobs::HeightmapData {
            width: grid as u32,
            height: grid as u32,
            scale: 10.0,
            data: vec![height; grid * grid],
        };
        TerrainProbe::new(&map, 0.0, 0.3)
    }

    /// A modest, calm world of `theme` on rolling land.
    fn scene(theme: ThemeArchetype) -> SceneCharacter {
        SceneCharacter {
            theme,
            landform: LandformArchetype::Rolling,
            prosperity: 0.5,
            escalation: 0.0,
            ..SceneCharacter::for_seed(1)
        }
    }

    /// What a world keeps clear: `lines`, each a path 2.5 m wide, the
    /// landmark's ground, and `more`.
    fn kept(lines: &[Vec<[f32; 2]>], landmark: ([f32; 2], f32), more: &[Ground]) -> Kept {
        let mut kept: Vec<Ground> = lines
            .iter()
            .flat_map(|line| line.windows(2))
            .map(|pair| Ground {
                a: pair[0],
                b: pair[1],
                r: 1.25,
            })
            .collect();
        kept.push(Ground::disc(landmark.0, landmark.1));
        kept.extend_from_slice(more);
        Kept(kept)
    }

    /// Whether `house` fronts onto the way `a`-`b`: its front (-Z) turned
    /// to the nearest point of it.
    fn fronts(house: &House, a: [f32; 2], b: [f32; 2]) -> bool {
        let toward = [b[0] - a[0], b[1] - a[1]];
        let length2 = toward[0] * toward[0] + toward[1] * toward[1];
        let t = (((house.at[0] - a[0]) * toward[0] + (house.at[1] - a[1]) * toward[1]) / length2)
            .clamp(0.0, 1.0);
        let near = [a[0] + toward[0] * t, a[1] + toward[1] * t];
        let (dx, dz) = (near[0] - house.at[0], near[1] - house.at[1]);
        let front = [-house.yaw_rad.sin(), -house.yaw_rad.cos()];
        (front[0] * dx + front[1] * dz) / (dx * dx + dz * dz).sqrt() > 0.999
    }

    /// #1797: on level ground a medieval world's hamlet stands its seven
    /// houses beside its path - on both sides, each fronting it, its
    /// ground its setback off the path's edge, past the landmark's and
    /// clear of the others - and needs no lane. Its set is a copy and a
    /// variant a house: the house's seed and its lot's grammar values, its
    /// spot, its turn and a member's sink.
    #[test]
    fn a_hamlet_stands_beside_its_path_each_house_fronting_it() {
        let probe = level(10.0);
        let landmark = ([0.0, 0.0], 12.0);
        let line = vec![[0.0, 30.0], [0.0, 250.0]];
        let lines = vec![line.clone()];
        let hamlet = site(
            &probe,
            [0.0, 20.0],
            landmark,
            &lines,
            2.5,
            &kept(&lines, landmark, &[]),
            &scene(ThemeArchetype::Medieval),
            7,
        )
        .expect("a hamlet");
        assert_eq!(hamlet.houses.len(), 7);
        assert_eq!(hamlet.lane, None);
        let path = Ground {
            a: line[0],
            b: line[1],
            r: 1.25,
        };
        for (i, house) in hamlet.houses.iter().enumerate() {
            let (at, r) = house.ground();
            let ground = Ground::disc(at, r);
            assert!(
                (ground.apart(&path) - SETBACK).abs() < 1e-3,
                "{i}: {house:?}"
            );
            assert!(ground.apart(&Ground::disc(landmark.0, landmark.1)) >= PAST_LANDMARK);
            assert!(fronts(house, line[0], line[1]), "{i}: {house:?}");
            for other in &hamlet.houses[i + 1..] {
                let (at, r) = other.ground();
                assert!(ground.apart(&Ground::disc(at, r)) >= GAP);
            }
        }
        let sides = |east: bool| hamlet.houses.iter().any(|h| (h.at[0] > 0.0) == east);
        assert!(sides(true) && sides(false), "both sides of the path");
        let Placement::Instances {
            generator_ref,
            variants,
            copies,
            ..
        } = hamlet.placement()
        else {
            panic!("a set");
        };
        assert_eq!(generator_ref, HAMLET);
        assert_eq!(copies.len(), 7);
        for ((copy, variant), house) in copies.iter().zip(&variants).zip(&hamlet.houses) {
            assert_eq!(variant.seed, Some(house.seed));
            assert_eq!(variant.params.0["Frontage"].0, house.fit.frontage_m());
            assert_eq!(variant.params.0["Depth"].0, house.fit.depth_m());
            assert_eq!(copy.position()[0], centimetres(house.at[0]) as f32 / 100.0);
            assert_eq!(copy.y_cm, -35);
            let front = copy.rotation() * bevy::math::Vec3::NEG_Z;
            let want = [-house.yaw_rad.sin(), -house.yaw_rad.cos()];
            assert!((front.x - want[0]).abs() < 1e-3 && (front.z - want[1]).abs() < 1e-3);
        }
        let seeds: std::collections::BTreeSet<u64> =
            variants.iter().filter_map(|v| v.seed).collect();
        assert_eq!(seeds.len(), 7, "a seed a house");
    }

    /// #1797: where nothing beside its paths takes a house - here a wall of
    /// ground kept clear down both sides - a hamlet takes a lane of its
    /// own: a straight path leaving one of them, its houses beside it and
    /// fronting it, the lane run past its last.
    #[test]
    fn a_hamlet_takes_a_lane_where_its_paths_take_no_house() {
        let probe = level(10.0);
        let landmark = ([0.0, 0.0], 12.0);
        let line = vec![[0.0, 30.0], [0.0, 150.0]];
        let lines = vec![line.clone()];
        let walls: Vec<Ground> = [-6.0_f32, 6.0]
            .iter()
            .map(|&x| Ground {
                a: [x, 20.0],
                b: [x, 150.0],
                r: 3.0,
            })
            .collect();
        let hamlet = site(
            &probe,
            [0.0, 100.0],
            landmark,
            &lines,
            2.5,
            &kept(&lines, landmark, &walls),
            &scene(ThemeArchetype::Nordic),
            11,
        )
        .expect("a hamlet");
        let [from, end] = hamlet.lane.expect("a lane");
        assert!(
            off_segment(from, line[0], line[1]) < 1e-3,
            "it leaves the path"
        );
        let lane = Ground {
            a: from,
            b: end,
            r: 1.25,
        };
        assert!(hamlet.houses.len() >= LEAST);
        for house in &hamlet.houses {
            let (at, r) = house.ground();
            assert!(
                (Ground::disc(at, r).apart(&lane) - SETBACK).abs() < 1e-3,
                "{house:?}"
            );
            assert!(fronts(house, from, end), "{house:?}");
            for wall in &walls {
                assert!(Ground::disc(at, r).apart(wall) >= SETBACK - ROUNDING);
            }
        }
    }

    /// #1797: every theme's hamlet's one generator - its kind's, finished
    /// and ruined at either end of prosperity and at every escalation tier -
    /// publishes as a record inside the soft budget, with room for its name
    /// and type.
    #[test]
    fn every_hamlets_generator_publishes_within_the_budget() {
        use crate::pds::record_size::{SOFT_RECORD_BUDGET_BYTES, serialized_record_bytes};
        const WRAPPER: usize = 160;
        let mut themes = 0;
        for &theme in ThemeArchetype::ALL.iter() {
            if most_houses(theme).is_none() {
                continue;
            }
            themes += 1;
            let entry = crate::catalogue::entries_for(theme, StructureRole::Secondary)
                .find(|entry| {
                    entry
                        .street()
                        .is_some_and(|spec| spec.kind == StreetKind::Detached)
                })
                .expect("a detached house");
            for prosperity in [0.0, 1.0] {
                for escalation in [0.0, 0.4, 0.7, 1.0] {
                    let hamlet = Hamlet {
                        entry,
                        seed: 4242,
                        houses: Vec::new(),
                        lane: None,
                    };
                    let bytes = serialized_record_bytes(&hamlet.generator(prosperity, escalation))
                        .expect("a generator serialises")
                        + WRAPPER;
                    assert!(
                        bytes <= SOFT_RECORD_BUDGET_BYTES,
                        "{theme:?} at {prosperity}, {escalation}: {bytes} B"
                    );
                }
            }
        }
        assert_eq!(themes, 8, "the themes that stand a hamlet");
    }

    /// #1797: no hamlet where its theme stands none, on craggy land, where
    /// no ground beside its paths stands over the water, or far from the
    /// landing.
    #[test]
    fn no_hamlet_stands_where_its_world_takes_none() {
        let landmark = ([0.0, 0.0], 12.0);
        let lines = vec![vec![[0.0, 30.0], [0.0, 250.0]]];
        let kept = kept(&lines, landmark, &[]);
        let landed = |landing: [f32; 2], probe: &TerrainProbe, scene: &SceneCharacter| {
            site(probe, landing, landmark, &lines, 2.5, &kept, scene, 7).is_some()
        };
        let sited =
            |probe: &TerrainProbe, scene: &SceneCharacter| landed([0.0, 20.0], probe, scene);
        assert!(sited(&level(10.0), &scene(ThemeArchetype::Medieval)));
        assert!(!sited(&level(10.0), &scene(ThemeArchetype::ModernCity)));
        let craggy = SceneCharacter {
            landform: LandformArchetype::Craggy,
            ..scene(ThemeArchetype::Medieval)
        };
        assert!(!sited(&level(10.0), &craggy));
        // Under the freeboard a house keeps over the water.
        assert!(!sited(
            &level(FREEBOARD - 0.5),
            &scene(ThemeArchetype::Medieval)
        ));
        // The path's nearest spot 330 m from where the world arrives.
        assert!(!landed(
            [0.0, -300.0],
            &level(10.0),
            &scene(ThemeArchetype::Medieval)
        ));
    }
}
