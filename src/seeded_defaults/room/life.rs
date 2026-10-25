//! Signs of life (#1782): smoke from a calm world's chimneys, and gulls
//! wheeling over its water.
//!
//! Most catalogue buildings with a hearth carry its smoke already; a grammar
//! house draws its chimney from its rules, so where it stands is only known
//! once they are drawn. [`light_the_hearths`] draws them as the house's
//! placement will (`catalogue::items::measure::chimney_tops`) and lights
//! smoke over most of its chimneys and stacks - in a calm world only, whose
//! buildings stand whole (a ruin's chimney is fallen, or cold).
//!
//! [`gulls`] finds open water nearest the arrival, past its shore and open
//! all round, where a flock wheels ([`catalogue::items::life::gull_flock`](crate::catalogue::items::life)),
//! placed over the water at [`GULL_HEIGHT`], unsnapped. A world with no
//! open water near has none.

use bevy::math::Vec3;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use super::siting::TerrainProbe;
use super::viewpoint::Viewpoint;
use crate::pds::Generator;

/// How far over a chimney's top its smoke starts (m).
const SMOKE_UP: f32 = 0.25;
/// Sub-stream salt for which hearths are lit, distinct from every sibling
/// deriver's.
const HEARTH_STREAM_SALT: u64 = 0x1782_5E0C_1782_5E0C;
/// How far from the landing a flock's water lies at the most (m).
const GULL_WATER: f32 = 220.0;
/// How far past the shore's nearest water the flock may wheel, each tried
/// in turn (m): where open water first lies all round it.
const GULL_OUT: [f32; 6] = [25.0, 45.0, 65.0, 85.0, 105.0, 125.0];
/// How far round the flock's hub the probe must read open water (m): its
/// birds fly out to 20 m, and a cliff beside open water is not open water.
const GULL_DISC: f32 = 24.0;
/// How deep the probe must read that water, at the least (m).
const GULL_DEPTH: f32 = 0.5;
/// How high over the water the flock wheels (m).
pub const GULL_HEIGHT: f32 = 16.0;

/// Light smoke over most of the chimneys and stacks `member`'s grammars
/// draw with `grammar_seed` (see the module docs): one in three is cold,
/// drawn from the seed. Each smoke is a child of the member's root, in its
/// frame.
pub fn light_the_hearths(member: &mut Generator, grammar_seed: u64) {
    let tops = crate::catalogue::items::measure::chimney_tops(member, grammar_seed);
    if tops.is_empty() {
        return;
    }
    let to_local = crate::catalogue::items::measure::transform_of(&member.transform)
        .compute_affine()
        .inverse();
    let mut rng = ChaCha8Rng::seed_from_u64(grammar_seed ^ HEARTH_STREAM_SALT);
    for top in tops {
        let draw = rng.next_u64();
        if draw % 3 == 0 {
            continue;
        }
        let at = to_local.transform_point3(top + Vec3::Y * SMOKE_UP);
        member
            .children
            .push(crate::catalogue::items::fx::chimney_smoke(
                at.to_array(),
                draw,
            ));
    }
}

/// Where a world's gulls wheel (see the module docs), world XZ: past the
/// shore of the water nearest its arrival - out along the way it lies to
/// where the probe first reads water, then on to the first of [`GULL_OUT`]
/// with open water all round it ([`open_water`]) - or none with no water
/// near, or none open.
pub fn gulls(probe: &TerrainProbe, view: &Viewpoint) -> Option<[f32; 2]> {
    let landing = view.landing;
    let way = probe.toward_water(landing, GULL_WATER)?;
    let water = probe.water_y();
    let along = |s: f32| [landing[0] + way[0] * s, landing[1] + way[1] * s];
    // The nearest wet cell's middle lies within reach that way, so its edge
    // does: half a cell at a time out to it.
    let step = probe.cell_size() * 0.5;
    let mut shore = 0.0;
    while probe.height_between(along(shore)) >= water {
        shore += step;
        if shore > GULL_WATER + probe.cell_size() {
            return None;
        }
    }
    GULL_OUT
        .into_iter()
        .map(|out| along(shore + out))
        .find(|&hub| open_water(probe, hub))
}

/// Whether the probe reads open water all round `hub`: [`GULL_DEPTH`]
/// deep at it and on rings out to [`GULL_DISC`], inside the map.
fn open_water(probe: &TerrainProbe, hub: [f32; 2]) -> bool {
    // A unit circle's eighths.
    const D: f32 = std::f32::consts::FRAC_1_SQRT_2;
    const ROUND: [[f32; 2]; 8] = [
        [1.0, 0.0],
        [D, D],
        [0.0, 1.0],
        [-D, D],
        [-1.0, 0.0],
        [-D, -D],
        [0.0, -1.0],
        [D, -D],
    ];
    let edge = probe.half_extent() - GULL_DISC;
    let deep = |at: [f32; 2]| probe.water_y() - probe.height_between(at) >= GULL_DEPTH;
    hub[0].abs() <= edge
        && hub[1].abs() <= edge
        && deep(hub)
        && [0.5, 1.0].into_iter().all(|ring| {
            ROUND
                .iter()
                .all(|&[x, z]| deep([hub[0] + x * GULL_DISC * ring, hub[1] + z * GULL_DISC * ring]))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure::{chimney_tops, transform_of};
    use crate::pds::GeneratorKind;

    /// A grammar house smokes from over its chimney's top, the smoke a child
    /// of its root in its frame; across seeds, most chimneys are lit and
    /// some stay cold.
    #[test]
    fn a_hall_house_smokes_from_its_chimney() {
        let entry = crate::catalogue::by_slug("medieval_street_detached").expect("the hall house");
        let (mut lit, mut chimneys) = (0, 0);
        for seed in 0..24u64 {
            let mut house = entry.build("");
            let before = house.children.len();
            light_the_hearths(&mut house, seed);
            let tops = chimney_tops(&entry.build(""), seed);
            assert!(!tops.is_empty(), "{seed}: a hall house draws a chimney");
            chimneys += tops.len();
            lit += house.children.len() - before;
            let root = transform_of(&house.transform).compute_affine();
            for smoke in &house.children[before..] {
                assert!(matches!(smoke.kind, GeneratorKind::ParticleSystem { .. }));
                let at = root.transform_point3(Vec3::from_array(smoke.transform.translation.0));
                assert!(
                    tops.iter()
                        .any(|top| (at - (*top + Vec3::Y * SMOKE_UP)).length() < 1e-3),
                    "{seed}: smoke at {at} over no chimney of {tops:?}"
                );
            }
        }
        assert!(
            lit > 0 && lit < chimneys,
            "{lit} of {chimneys} chimneys lit"
        );
    }

    /// A probe of `height` at world XZ over a 96-cell grid ten metres a
    /// cell, its water at 0.
    fn probe(height: impl Fn(f32) -> f32) -> TerrainProbe {
        const GRID: usize = 96;
        const CELL: f32 = 10.0;
        let half = (GRID - 1) as f32 * CELL * 0.5;
        let map = gen_jobs::HeightmapData {
            width: GRID as u32,
            height: GRID as u32,
            scale: CELL,
            data: (0..GRID * GRID)
                .map(|i| height((i % GRID) as f32 * CELL - half))
                .collect(),
        };
        TerrainProbe::new(&map, 0.0, crate::seeded_defaults::BUILD_SLOPE_LIMIT)
    }

    /// Gulls wheel past the shore of the water nearest the arrival, over
    /// it; a world with no water near has none.
    #[test]
    fn gulls_wheel_over_the_water_nearest_the_arrival() {
        let view = Viewpoint {
            landing: [120.0, 0.0],
            toward: [1.0, 0.0],
            pitch: 0.2,
            score: 0.0,
        };
        let coast = probe(|x| {
            if x < 0.0 {
                x * 0.08
            } else {
                (x * 0.06).min(3.0)
            }
        });
        let at = gulls(&coast, &view).expect("water lies near");
        // The shore at x = 0 as the probe reads it, between its samples, the
        // way to it toward the nearest wet cell's middle, half a cell aside:
        // the first distance past it with open water all round - the sea's
        // floor shelving 8 cm a metre, half a metre deep from 6.25 m out.
        assert!(
            at[0] < -(GULL_DISC + 6.25) && at[0] > -(GULL_OUT[5] + 10.0) && at[1].abs() <= 10.0,
            "past the shore, west over open water: {at:?}"
        );
        assert!(open_water(&coast, at));
        let inland = probe(|_| 5.0);
        assert_eq!(gulls(&inland, &view), None);
        // A strait too narrow to be open water all round: none.
        let strait = probe(|x| if x.abs() < 20.0 { -3.0 } else { 3.0 });
        let over = Viewpoint {
            landing: [120.0, 0.0],
            ..view
        };
        assert_eq!(gulls(&strait, &over), None);
    }
}
