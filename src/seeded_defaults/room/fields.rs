//! Fields and orchards (#1791): a world seeded on its own ground whose
//! theme farms - a farm, a medieval village, a Norse steading, a feudal
//! Japanese village, a Mesoamerican town, a frontier town, a solarpunk
//! commune - sites near its arrival one to three fields, rectangles of
//! ground in strips of its theme's crops in turn
//! ([`crate::pds::SovereignField`], painted on its ground and coloured by
//! the ground's shader), and, where its theme grows fruit, an orchard: its
//! fruit tree in rows ([`ORCHARD`], a `Placement::Grid`).
//!
//! Each is sited on the probe its settlement was, after its places for
//! crafts ([`super::crafts`]) and clear of all they keep clear - the
//! settlement, its paths, its feature, the view down to its landmark and
//! the places themselves - and of each other: on dry ground under the snow
//! line that falls gently across its strips or rows and barely along them,
//! so each runs along the contour; on level ground they run toward the
//! landing. Each takes the first point of the arrival's lattice whose ground
//! takes it, those the arrival frames before those it does not and nearest
//! first among each, at its drawn size and then smaller. A world goes
//! without what no ground near takes, and a biome nothing is farmed in -
//! ice, tundra, badlands - grows neither.
//!
//! The scatters keep off both (`world_builder::compile::scatter`), the
//! grass off a field (`ground_cover`); an orchard's trees count in the
//! seeded vegetation budget, reserved before the scatters are fitted to it
//! (`build`).

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use super::crafts::{Ground, MARGIN, Siting};
use crate::pds::{Fp, Fp2, Fp3, Fp4, Placement, SovereignField, TransformData};
use crate::seeded_defaults::{BiomeArchetype, LandformArchetype, ThemeArchetype, range_f32};
pub(crate) use crate::world_builder::compile::scatter::{ORCHARD, ORCHARD_CROWN as CROWN};

/// Sub-stream salt distinct from every sibling room deriver.
const FIELDS_STREAM_SALT: u64 = 0x1791_F1E1_D5A1_7791;

/// How far from the landing a field's or an orchard's middle stands at the
/// most (m).
const FARTHEST: f32 = 220.0;

/// How far off the view's heading the arrival still frames a point, as the
/// cosine of that angle (about 34 degrees): the lens's 45 degrees tall at
/// 16 by 9, less a little.
const FRAMED: f32 = 0.83;

/// How far apart a field's or an orchard's ground is read on the probe (m).
const READ: f32 = 6.0;

/// How steeply a field's ground may rise from one reading to the next along
/// its strips, and across them (rise/run), on the probe - which reads
/// slopes about half as steep as the full map draws them (#1782's critic).
const FIELD_ALONG: f32 = 0.05;
const FIELD_ACROSS: f32 = 0.08;

/// The same for an orchard's rows: trees stand on a steeper hillside than a
/// plough works.
const ORCHARD_ALONG: f32 = 0.05;
const ORCHARD_ACROSS: f32 = 0.12;

/// The share of those a field keeps on craggy land, whose fine relief the
/// probe does not see at all (#1788): a census of seeds 0..300 found seven
/// in eight craggy worlds' fields on ground steeper than 0.18 at its
/// ninetieth percentile, on the full map, under the limits above.
const CRAGGY_SHARE: f32 = 0.5;

/// How far under the snow line a field's or an orchard's ground keeps (m).
const UNDER_SNOW: f32 = 4.0;

/// Ground falling less steeply than this across a field's reach (rise/run)
/// lies level: its strips run toward the landing.
const LEVEL: f32 = 0.015;

/// How far either side of a middle the ground's fall is read (m): a field's
/// reach, not a cell's.
const FALL_REACH: f32 = 15.0;

/// How wide each row of the ground a field or an orchard keeps is, at the
/// most (m): the rows together reach every point of it.
const ROW: f32 = 5.0;

/// The sizes a field or an orchard is tried at in turn, as shares of its
/// drawn size.
const SHRINK: [f32; 3] = [1.0, 0.75, 0.5];

/// How many trees an orchard stands in each row, and how many rows: an
/// apple is 3,890 triangles.
pub const ORCHARD_TREES: u32 = 5;
pub const ORCHARD_ROWS: u32 = 4;

/// How far apart an orchard's trees stand along a row, and its rows (m): an
/// apple's crown is about 7 m across.
pub const TREE_GAP: f32 = 8.0;
pub const ROW_GAP: f32 = 9.0;

/// How wide a field's strips are, at the least and the most (m).
const STRIP: (f32, f32) = (4.0, 7.0);

// A field's crops, sRGB: what each strip shows from above.
const PLOUGHED: [f32; 3] = [0.36, 0.25, 0.15];
const SPRING_WHEAT: [f32; 3] = [0.38, 0.52, 0.20];
const RIPE_WHEAT: [f32; 3] = [0.80, 0.66, 0.32];
const BARLEY: [f32; 3] = [0.72, 0.68, 0.42];
const PADDY: [f32; 3] = [0.40, 0.60, 0.22];
const RIPE_RICE: [f32; 3] = [0.74, 0.64, 0.30];
const TEA: [f32; 3] = [0.18, 0.34, 0.14];
const RED_EARTH: [f32; 3] = [0.52, 0.28, 0.16];
const MAIZE: [f32; 3] = [0.36, 0.50, 0.16];
const RIPE_MAIZE: [f32; 3] = [0.78, 0.64, 0.30];
const DUST: [f32; 3] = [0.62, 0.50, 0.34];
const LAVENDER: [f32; 3] = [0.54, 0.44, 0.70];
const CLOVER: [f32; 3] = [0.30, 0.48, 0.20];

/// What a theme's farm grows.
#[derive(Clone, Copy, Debug)]
pub struct Farm {
    /// How many fields it lays out, at the least and the most.
    fields: (u32, u32),
    /// Their crops: each field grows two or three of them in turn.
    crops: &'static [[f32; 3]],
    /// How long and how deep a field is at the most (m): each is drawn
    /// between three fifths and the whole of each.
    size: [f32; 2],
    /// Its orchard's fruit tree, a catalogue slug, if it grows one.
    pub orchard: Option<&'static str>,
    /// The share of the probe's slope limits its fields keep
    /// ([`CRAGGY_SHARE`] on craggy land).
    steepest: f32,
}

/// What `theme` farms in `biome` on `landform`, if it farms there at all.
pub fn farm(
    theme: ThemeArchetype,
    biome: BiomeArchetype,
    landform: LandformArchetype,
) -> Option<Farm> {
    use ThemeArchetype as T;
    if matches!(
        biome,
        BiomeArchetype::Glacial | BiomeArchetype::Tundra | BiomeArchetype::Badlands
    ) {
        return None;
    }
    let steepest = if landform == LandformArchetype::Craggy {
        CRAGGY_SHARE
    } else {
        1.0
    };
    let grain: &'static [[f32; 3]] = &[PLOUGHED, SPRING_WHEAT, RIPE_WHEAT, BARLEY];
    let farm = match theme {
        T::RuralFarmland => Farm {
            fields: (2, 3),
            crops: grain,
            size: [80.0, 50.0],
            orchard: Some("lsys_apple"),
            steepest,
        },
        T::Medieval => Farm {
            fields: (1, 2),
            crops: grain,
            size: [60.0, 36.0],
            orchard: Some("lsys_apple"),
            steepest,
        },
        T::Nordic => Farm {
            fields: (1, 2),
            crops: &[PLOUGHED, BARLEY, SPRING_WHEAT],
            size: [50.0, 32.0],
            orchard: Some("lsys_apple"),
            steepest,
        },
        T::FeudalJapan => Farm {
            fields: (1, 2),
            crops: &[PADDY, RIPE_RICE, TEA],
            size: [44.0, 30.0],
            orchard: Some("lsys_flowering_tree"),
            steepest,
        },
        T::Mesoamerican => Farm {
            fields: (1, 2),
            crops: &[RED_EARTH, MAIZE, RIPE_MAIZE],
            size: [56.0, 36.0],
            orchard: None,
            steepest,
        },
        T::WildWest => Farm {
            fields: (1, 2),
            crops: &[DUST, RIPE_WHEAT, PLOUGHED],
            size: [70.0, 40.0],
            orchard: None,
            steepest,
        },
        T::Solarpunk => Farm {
            fields: (1, 2),
            crops: &[CLOVER, LAVENDER, RIPE_WHEAT],
            size: [50.0, 32.0],
            orchard: Some("lsys_apple"),
            steepest,
        },
        _ => return None,
    };
    Some(farm)
}

/// A field as drawn, before its ground is found: its length, its strips'
/// width and how many, and its crops in turn.
#[derive(Clone, Debug)]
struct Drawn {
    length: f32,
    strip: f32,
    strips: u32,
    crops: Vec<[f32; 3]>,
}

impl Drawn {
    /// Its rectangle's half length and half depth at `share` of its drawn
    /// size: whole strips, two at the least.
    fn half(&self, share: f32) -> [f32; 2] {
        let strips = ((self.strips as f32 * share).round() as u32).max(2);
        [self.length * share * 0.5, strips as f32 * self.strip * 0.5]
    }
}

/// A rectangle on the ground: its middle, the unit direction its length
/// runs along, and its half length and half depth (m).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Patch {
    middle: [f32; 2],
    along: [f32; 2],
    half: [f32; 2],
}

impl Patch {
    /// The point `u` along it and `v` across it from its middle.
    fn at(&self, u: f32, v: f32) -> [f32; 2] {
        let across = [-self.along[1], self.along[0]];
        [
            self.middle[0] + self.along[0] * u + across[0] * v,
            self.middle[1] + self.along[1] * u + across[1] * v,
        ]
    }

    /// The ground it covers: rows along its length, each [`ROW`] across at
    /// the most, which together reach every point of it.
    fn ground(&self) -> Vec<Ground> {
        let rows = (self.half[1] / ROW).ceil().max(1.0) as usize;
        let r = self.half[1] / rows as f32;
        (0..rows)
            .map(|k| {
                let v = -self.half[1] + r * (2 * k + 1) as f32;
                Ground {
                    a: self.at(-self.half[0], v),
                    b: self.at(self.half[0], v),
                    r,
                }
            })
            .collect()
    }

    /// Whether the probe reads it as ground that takes it: dry over the
    /// water and under `snow_y` at every reading, rising no more than
    /// `along` (rise/run) from one reading to the next along it and
    /// `across` across it. Read row by row, refused at the first reading
    /// that fails: most ground fails early.
    fn takes(&self, siting: &Siting, snow_y: f32, along: f32, across: f32) -> bool {
        let n = self
            .half
            .map(|half| (2.0 * half / READ).ceil().max(1.0) as usize);
        let step = [
            2.0 * self.half[0] / n[0] as f32,
            2.0 * self.half[1] / n[1] as f32,
        ];
        let rise = [along * step[0], across * step[1]];
        let mut before: Vec<f32> = Vec::with_capacity(n[0] + 1);
        let mut row: Vec<f32> = Vec::with_capacity(n[0] + 1);
        for j in 0..=n[1] {
            row.clear();
            for i in 0..=n[0] {
                let at = self.at(
                    -self.half[0] + step[0] * i as f32,
                    -self.half[1] + step[1] * j as f32,
                );
                let height = siting.probe.height_between(at);
                if !siting.dry(at)
                    || height > snow_y
                    || i > 0 && (height - row[i - 1]).abs() > rise[0]
                    || j > 0 && (height - before[i]).abs() > rise[1]
                {
                    return false;
                }
                row.push(height);
            }
            std::mem::swap(&mut row, &mut before);
        }
        true
    }
}

/// An orchard: its middle, the unit direction its rows run along, and its
/// fruit tree's catalogue slug.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orchard {
    pub middle: [f32; 2],
    pub along: [f32; 2],
    pub slug: &'static str,
}

impl Orchard {
    /// How far its outer trees' crowns reach from its middle, along its
    /// rows and across them (m).
    fn half() -> [f32; 2] {
        [
            (ORCHARD_TREES - 1) as f32 * TREE_GAP * 0.5 + CROWN,
            (ORCHARD_ROWS - 1) as f32 * ROW_GAP * 0.5 + CROWN,
        ]
    }

    /// Its placement: a grid of [`ORCHARD`] turned so the grid's rows (its
    /// X) run along `along`, each tree on the ground under it and turned
    /// its own way.
    pub fn placement(&self) -> Placement {
        // A turn about +Y takes +X to (cos, -sin): libm's (#1132).
        let yaw = libm::atan2f(-self.along[1], self.along[0]);
        let half = yaw * 0.5;
        Placement::Grid {
            generator_ref: ORCHARD.to_string(),
            transform: TransformData {
                translation: Fp3([self.middle[0], 0.0, self.middle[1]]),
                rotation: Fp4([0.0, libm::sinf(half), 0.0, libm::cosf(half)]),
                scale: Fp3([1.0, 1.0, 1.0]),
            },
            counts: [ORCHARD_TREES, 1, ORCHARD_ROWS],
            gaps: Fp3([TREE_GAP, 1.0, ROW_GAP]),
            snap_to_terrain: true,
            random_yaw: true,
        }
    }
}

/// A world's fields and its orchard, sited by [`site`].
#[derive(Clone, Debug, Default)]
pub struct FarmPlaces {
    pub fields: Vec<SovereignField>,
    pub orchard: Option<Orchard>,
}

/// Site the fields and the orchard `farm` grows (see the module docs),
/// drawn from `seed`'s stream of its own, under the snow line at `snow_y`;
/// each keeps its ground from those sited after it.
pub fn site(siting: &mut Siting, farm: &Farm, seed: u64, snow_y: f32) -> FarmPlaces {
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ FIELDS_STREAM_SALT);
    // Every draw first, so a field the ground does not take moves no other.
    let (least, most) = farm.fields;
    let count = least + rng.next_u32() % (most - least + 1);
    let drawn: Vec<Drawn> = (0..count)
        .map(|_| {
            let length = range_f32(&mut rng, 0.6, 1.0) * farm.size[0];
            let strip = range_f32(&mut rng, STRIP.0, STRIP.1);
            let depth = range_f32(&mut rng, 0.6, 1.0) * farm.size[1];
            let strips = ((depth / strip).round() as u32).max(2);
            let kinds = (2 + (rng.next_u32() & 1) as usize).min(farm.crops.len());
            let start = rng.next_u32() as usize % farm.crops.len();
            let crops = (0..kinds)
                .map(|k| farm.crops[(start + k) % farm.crops.len()])
                .collect();
            Drawn {
                length,
                strip,
                strips,
                crops,
            }
        })
        .collect();
    let snow_y = snow_y - UNDER_SNOW;
    let mut places = FarmPlaces::default();
    for field in &drawn {
        let Some(patch) = SHRINK.into_iter().find_map(|share| {
            place(
                siting,
                field.half(share),
                snow_y,
                FIELD_ALONG * farm.steepest,
                FIELD_ACROSS * farm.steepest,
            )
        }) else {
            continue;
        };
        siting.kept.0.extend(patch.ground());
        places.fields.push(SovereignField {
            center: Fp2(patch.middle),
            size: Fp2([patch.half[0] * 2.0, patch.half[1] * 2.0]),
            bearing: Fp(bearing(patch.along)),
            strip: Fp(field.strip),
            crops: field.crops.iter().copied().map(Fp3).collect(),
        });
    }
    if let Some(slug) = farm.orchard
        && let Some(patch) = place(
            siting,
            Orchard::half(),
            snow_y,
            ORCHARD_ALONG,
            ORCHARD_ACROSS,
        )
    {
        siting.kept.0.extend(patch.ground());
        places.orchard = Some(Orchard {
            middle: patch.middle,
            along: patch.along,
            slug,
        });
    }
    places
}

/// The first patch of `half` (see [`Patch`]) whose ground takes it - read
/// with `along` and `across` (see [`Patch::takes`]) - clear of what is kept:
/// at the arrival's lattice's points, those it frames first and nearest
/// first among each, its length along the contour there.
fn place(siting: &Siting, half: [f32; 2], snow_y: f32, along: f32, across: f32) -> Option<Patch> {
    let points = siting.lattice(1.0, FARTHEST, half[0] + half[1]);
    let (framed, aside): (Vec<_>, Vec<_>) = points
        .into_iter()
        .partition(|&(at, _)| siting.off_the_view(at) >= FRAMED);
    framed.into_iter().chain(aside).find_map(|(middle, _)| {
        // Its middle first, the cheapest reading, which most ground fails.
        if !siting.dry(middle) || siting.probe.height_between(middle) > snow_y {
            return None;
        }
        let patch = Patch {
            middle,
            along: contour(siting, middle)?,
            half,
        };
        (patch.takes(siting, snow_y, along, across)
            && patch
                .ground()
                .iter()
                .all(|ground| siting.kept.clear(ground, MARGIN)))
        .then_some(patch)
    })
}

/// The way a field's strips or an orchard's rows at `at` run: along the
/// contour where the ground falls across [`FALL_REACH`], toward the landing
/// where it lies level.
fn contour(siting: &Siting, at: [f32; 2]) -> Option<[f32; 2]> {
    let probe = siting.probe;
    let h = |dx: f32, dz: f32| probe.height_between([at[0] + dx, at[1] + dz]);
    let gx = (h(FALL_REACH, 0.0) - h(-FALL_REACH, 0.0)) / (2.0 * FALL_REACH);
    let gz = (h(0.0, FALL_REACH) - h(0.0, -FALL_REACH)) / (2.0 * FALL_REACH);
    // Square root, not `hypot` (#1132): the fall turns the field.
    let fall = (gx * gx + gz * gz).sqrt();
    if fall >= LEVEL {
        return Some([-gz / fall, gx / fall]);
    }
    let landing = siting.view.landing;
    let (dx, dz) = (landing[0] - at[0], landing[1] - at[1]);
    let far = (dx * dx + dz * dz).sqrt();
    (far > 1e-3).then(|| [dx / far, dz / far])
}

/// The compass bearing (degrees, clockwise from north, folded into
/// `[0, 180)`) of a field whose strips run along `along`
/// ([`SovereignField::along`]) - libm's.
fn bearing(along: [f32; 2]) -> f32 {
    let degrees = libm::atan2f(along[0], -along[1]).to_degrees();
    let folded = degrees.rem_euclid(180.0);
    if folded >= 180.0 { 0.0 } else { folded }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_defaults::room::crafts::Kept;
    use crate::seeded_defaults::room::siting::TerrainProbe;
    use crate::seeded_defaults::room::viewpoint::Viewpoint;

    /// A hillside: the sea west of x = -300, the land rising east from it
    /// 5 cm a metre to a plateau at 20 m, flat beyond x = 100.
    fn hillside() -> TerrainProbe {
        const GRID: usize = 96;
        const CELL: f32 = 10.0;
        let half = (GRID - 1) as f32 * CELL * 0.5;
        let height = |x: f32| ((x + 300.0) * 0.05).min(20.0);
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

    fn view() -> Viewpoint {
        Viewpoint {
            landing: [0.0, 0.0],
            toward: [0.0, -1.0],
            pitch: 0.2,
            score: 0.0,
        }
    }

    fn siting<'a>(probe: &'a TerrainProbe, view: &'a Viewpoint) -> Siting<'a> {
        Siting {
            probe,
            view,
            kept: Kept(vec![Ground::disc(view.landing, 14.0)]),
            wind: [0.0, 1.0],
            theme: ThemeArchetype::RuralFarmland,
        }
    }

    /// A farm sites its fields and its orchard: every one clear of what
    /// was kept and of each other, its ground dry, its strips and its rows
    /// along the contour where the land falls (north-south on a hillside
    /// rising east) and toward the landing where it lies flat, its crops
    /// two or three of the theme's.
    #[test]
    fn a_farm_sites_its_fields_and_orchard_along_the_contour() {
        let probe = hillside();
        let view = view();
        let farm = farm(
            ThemeArchetype::RuralFarmland,
            BiomeArchetype::Meadow,
            LandformArchetype::Rolling,
        )
        .expect("a farm");
        for seed in 0..12 {
            let mut siting = siting(&probe, &view);
            let places = site(&mut siting, &farm, seed, 1_000.0);
            assert!((2..=3).contains(&places.fields.len()), "{seed}: {places:?}");
            let orchard = places.orchard.expect("an orchard");
            assert_eq!(orchard.slug, "lsys_apple");
            let mut grounds: Vec<(Vec<Ground>, [f32; 2])> = Vec::new();
            for field in &places.fields {
                let patch = Patch {
                    middle: field.center.0,
                    along: field.along(),
                    half: [field.size.0[0] * 0.5, field.size.0[1] * 0.5],
                };
                assert!(
                    (2..=3).contains(&field.crops.len()),
                    "{seed}: {:?}",
                    field.crops
                );
                let strips = field.size.0[1] / field.strip.0;
                assert!(
                    (strips - strips.round()).abs() < 1e-3,
                    "{seed}: whole strips"
                );
                grounds.push((patch.ground(), patch.middle));
            }
            let patch = Patch {
                middle: orchard.middle,
                along: orchard.along,
                half: Orchard::half(),
            };
            grounds.push((patch.ground(), patch.middle));
            let landing = Ground::disc(view.landing, 14.0);
            for (k, (ground, middle)) in grounds.iter().enumerate() {
                assert!(
                    ground.iter().all(|g| g.apart(&landing) >= MARGIN - 1e-3),
                    "{seed}: {k} keeps off the landing"
                );
                for (other, _) in &grounds[k + 1..] {
                    for a in ground {
                        for b in other {
                            assert!(a.apart(b) >= MARGIN - 1e-3, "{seed}: {k} overlaps");
                        }
                    }
                }
                assert!(probe.height_between(*middle) >= 2.0, "{seed}: dry");
            }
            for field in &places.fields {
                let along = field.along();
                let middle = field.center.0;
                if middle[0] < 100.0 - FALL_REACH {
                    assert!(along[0].abs() < 1e-3, "{seed}: along the contour {along:?}");
                } else if middle[0] > 100.0 + FALL_REACH {
                    let toward = [-middle[0], -middle[1]];
                    let far = toward[0].hypot(toward[1]);
                    let dot = (along[0] * toward[0] + along[1] * toward[1]) / far;
                    assert!(dot.abs() > 0.999, "{seed}: toward the landing {along:?}");
                }
            }
        }
    }

    /// Ground under the snow line or the sea takes no field, and a theme or
    /// a biome that farms nothing grows none.
    #[test]
    fn no_field_grows_on_snow_water_or_an_unfarmed_world() {
        let probe = hillside();
        let view = view();
        let farm = farm(
            ThemeArchetype::Medieval,
            BiomeArchetype::Meadow,
            LandformArchetype::Rolling,
        )
        .expect("a farm");
        let places = site(&mut siting(&probe, &view), &farm, 3, 0.0);
        assert!(places.fields.is_empty() && places.orchard.is_none());
        assert!(
            super::farm(
                ThemeArchetype::ModernCity,
                BiomeArchetype::Meadow,
                LandformArchetype::Rolling
            )
            .is_none()
        );
        assert!(
            super::farm(
                ThemeArchetype::Nordic,
                BiomeArchetype::Glacial,
                LandformArchetype::Rolling
            )
            .is_none()
        );
        assert!(
            super::farm(
                ThemeArchetype::Mesoamerican,
                BiomeArchetype::Jungle,
                LandformArchetype::Rolling
            )
            .is_some_and(|farm| farm.orchard.is_none())
        );
    }

    /// An orchard's grid stands its trees as the compile does
    /// (`compile::executor`): its rows along `along`, each tree within its
    /// patch.
    #[test]
    fn an_orchards_rows_run_along_its_bearing() {
        for along in [[1.0, 0.0], [0.0, 1.0], [0.6, -0.8], [-0.8, -0.6]] {
            let orchard = Orchard {
                middle: [30.0, -40.0],
                along,
                slug: "lsys_apple",
            };
            let Placement::Grid {
                transform,
                counts,
                gaps,
                ..
            } = orchard.placement()
            else {
                panic!("a grid");
            };
            let anchor = bevy::prelude::Transform::from(&transform);
            let [cx, _, cz] = counts;
            let [gx, _, gz] = gaps.0;
            let local = |ix: u32, iz: u32| {
                bevy::math::Vec3::new(
                    -((cx as f32 - 1.0) * gx) / 2.0 + ix as f32 * gx,
                    0.0,
                    -((cz as f32 - 1.0) * gz) / 2.0 + iz as f32 * gz,
                )
            };
            let first = anchor.transform_point(local(0, 0));
            let next = anchor.transform_point(local(1, 0));
            let row = (next - first) / gx;
            assert!(
                (row.x - along[0]).abs() < 1e-5 && (row.z - along[1]).abs() < 1e-5,
                "{along:?}: {row:?}"
            );
            let half = Orchard::half();
            for ix in 0..cx {
                for iz in 0..cz {
                    let at = anchor.transform_point(local(ix, iz));
                    let (dx, dz) = (at.x - 30.0, at.z + 40.0);
                    let u = dx * along[0] + dz * along[1];
                    let v = -dx * along[1] + dz * along[0];
                    assert!(u.abs() <= half[0] - CROWN + 1e-3, "{along:?}: {u}");
                    assert!(v.abs() <= half[1] - CROWN + 1e-3, "{along:?}: {v}");
                }
            }
        }
    }

    /// A bearing folds into a half turn: strips running south are strips
    /// running north.
    #[test]
    fn a_bearing_folds_into_a_half_turn() {
        for (along, expected) in [
            ([0.0, -1.0], 0.0),
            ([1.0, 0.0], 90.0),
            ([0.0, 1.0], 0.0),
            ([-1.0, 0.0], 90.0),
        ] {
            let b = bearing(along);
            assert!((b - expected).abs() < 1e-4, "{along:?}: {b}");
            let field = SovereignField {
                bearing: Fp(b),
                ..Default::default()
            };
            let back = field.along();
            let dot = back[0] * along[0] + back[1] * along[1];
            assert!(dot.abs() > 0.9999, "{along:?}: {back:?}");
        }
    }
}
