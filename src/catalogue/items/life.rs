//! Signs of life a seeded world carries beside its settlement (#1782): a
//! flock of gulls wheeling over its water. Not a catalogue entry - no
//! settlement's pool draws it - but built here, with the kits' vocabulary.
//!
//! A flock is a hub turning slowly about +Y with its birds round it at
//! their own radii and heights, each lying along its circle, banked into
//! the turn, its wings beating lazily on hubs of their own. Every client
//! turns them for itself (`pds::spin`), so they are drawn without colliders.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use super::util::{
    cuboid_tapered, id_quat, nest, prim, quat_mul, quat_y, quat_z, sphere, steady, superellipsoid,
    turning,
};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings, SpinTerm};

/// How many gulls a flock holds.
const GULLS: usize = 6;
/// How far round the hub they fly, the least and the most (m), and how far
/// over or under its height.
const RADIUS: (f32, f32) = (9.0, 20.0);
const HEIGHT: (f32, f32) = (-2.5, 3.0);
/// How fast the flock wheels (degrees a second): a 15 m circle in about
/// fourteen seconds.
const WHEEL: f32 = 26.0;
/// How far a gull banks into the turn (radians).
const BANK: f32 = 0.24;
/// A wing's span from the shoulder, its chord, and its dihedral (radians).
const WING: [f32; 2] = [0.55, 0.16];
const DIHEDRAL: f32 = 0.16;
/// How far a wing beats either side, and how long a beat takes (s).
const BEAT: (f32, f32) = (16.0, 1.4);

const WHITE: [f32; 3] = [0.92, 0.92, 0.90];
const GREY: [f32; 3] = [0.66, 0.68, 0.71];

fn feather(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.8),
        ..Default::default()
    }
}

/// One gull in her own frame: her body along +Z, her wings out along +-X
/// on beating hubs, the left's beat a half turn behind the right's so both
/// rise together.
fn gull(phase: f32) -> Generator {
    let body = prim(
        superellipsoid([0.07, 0.06, 0.21], 0.8, 0.8, feather(WHITE)),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    let mut wings = Vec::new();
    for side in [-1.0f32, 1.0] {
        let shoulder = [side * 0.05, 0.02, 0.02];
        let hub = prim(sphere(0.02, 4, feather(WHITE)), shoulder, id_quat());
        let wing = prim(
            cuboid_tapered([WING[0], 0.012, WING[1]], 0.0, feather(GREY)),
            [shoulder[0] + side * WING[0] * 0.5, shoulder[1], shoulder[2]],
            quat_z(side * DIHEDRAL),
        );
        let beat = SpinTerm::Swing {
            amplitude: Fp(BEAT.0),
            period: Fp(BEAT.1),
            // Within a turn, as the sanitiser holds a phase.
            phase: Fp((phase + if side < 0.0 { 180.0 } else { 0.0 }).rem_euclid(360.0)),
        };
        wings.push(turning(hub, [0.0, 0.0, 1.0], beat, vec![wing]));
    }
    nest(body, wings)
}

/// A flock of gulls (see the module docs) about its origin, drawn from
/// `seed`: where each flies round the hub and how high.
pub(crate) fn gull_flock(seed: u64) -> Generator {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut unit = || (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32;
    let hub = prim(sphere(0.02, 4, feather(WHITE)), [0.0, 0.0, 0.0], id_quat());
    let mut birds = Vec::new();
    for k in 0..GULLS {
        let a = (k as f32 + unit() * 0.6) * std::f32::consts::TAU / GULLS as f32;
        let r = RADIUS.0 + (RADIUS.1 - RADIUS.0) * unit();
        let h = HEIGHT.0 + (HEIGHT.1 - HEIGHT.0) * unit();
        let mut bird = gull(unit() * 360.0);
        // libm's (#1132): where she flies is in the record every peer
        // derives.
        let (sa, ca) = (libm::sinf(a), libm::cosf(a));
        bird.transform.translation = Fp3([r * ca, h, r * sa]);
        // The hub turns +Y, carrying a bird at bearing `a` along
        // (sin a, -cos a): her +Z turned there, then banked into the turn.
        bird.transform.rotation = quat_mul(quat_y(std::f32::consts::PI - a), quat_z(-BANK));
        birds.push(bird);
    }
    turning(hub, [0.0, 1.0, 0.0], steady(WHEEL), birds)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flock wheels about +Y with every gull on it, each gull's wings
    /// beating together, its birds at their radii and heights.
    #[test]
    fn a_flock_wheels_its_gulls_round_and_beats_their_wings() {
        let flock = gull_flock(7);
        let wheel = flock.spin.as_ref().expect("the flock turns");
        assert!(matches!(wheel.terms[..], [SpinTerm::Constant { .. }]));
        assert_eq!(flock.children.len(), GULLS);
        for bird in &flock.children {
            let [x, y, z] = bird.transform.translation.0;
            let r = x.hypot(z);
            assert!((RADIUS.0 - 1e-3..=RADIUS.1 + 1e-3).contains(&r), "{r}");
            assert!((HEIGHT.0 - 1e-3..=HEIGHT.1 + 1e-3).contains(&y), "{y}");
            let phases: Vec<f32> = bird
                .children
                .iter()
                .map(
                    |wing| match wing.spin.as_ref().map(|spin| &spin.terms[..]) {
                        Some([SpinTerm::Swing { phase, .. }]) => phase.0,
                        other => panic!("a wing that does not beat: {other:?}"),
                    },
                )
                .collect();
            assert_eq!(phases.len(), 2);
            assert!(
                ((phases[0] - phases[1]).abs() - 180.0).abs() < 1e-3,
                "{phases:?}"
            );
        }
        crate::catalogue::items::util::assert_sanitize_stable(&flock, "gull_flock");
    }
}
