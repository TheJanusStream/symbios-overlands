//! Steep-ground relocation walk (#905) — the compile-time safety net
//! under the derive-time settlement siting.
//!
//! The settlement deriver places members against a low-resolution proxy
//! of the heightmap; the proxy under-reads fine-scale steepness, so a
//! member can occasionally land on ground the full-resolution map
//! reveals to be a slope face. This walk mirrors
//! [`relocate_above_water`](super::water::relocate_above_water): it
//! slides the anchor along its bearing through the origin to the first
//! probe that is both gentle enough *and* still dry, and gives up in
//! place after a bounded probe budget (a tilted landmark beats a
//! missing one). Pure function of the shared heightmap → peers agree.
//!
//! Applied to the same placements that opt into the water walk
//! (`avoid_water: true`) — that flag is the seeded pipeline's marker,
//! so editor-authored placements are never second-guessed.

use bevy::prelude::*;

/// Slope limit (rise/run) beyond which an anchor is relocated. Sits
/// well above the derive-time `BUILD_SLOPE_LIMIT` (0.28): this net only
/// fires when the proxy was genuinely wrong about the ground, not to
/// re-litigate borderline hillsides.
const STEEP_LIMIT: f32 = 0.45;

/// Probe spacing along the bearing (m).
const STEP: f32 = 6.0;

/// Probe budget: 30 outward + 30 inward.
const MAX_PROBES: u32 = 60;

/// Freeboard the dry-check keeps over the water line, matching the
/// water walk's margin.
const FREEBOARD: f32 = 0.75;

/// Slide an anchor off over-steep ground along its bearing through the
/// origin. Candidates must be gentle at the centre and (for a non-zero
/// `clearance`) around an eight-point footprint ring, and must stay dry
/// when the room has a water line.
pub(super) fn relocate_off_steep_ground(
    hm: &bevy_symbios_ground::HeightMap,
    extent: f32,
    half: f32,
    translation: &mut Vec3,
    water_y: Option<f32>,
    clearance: f32,
) {
    let sample = |x: f32, z: f32| {
        hm.get_height_at((x + half).clamp(0.0, extent), (z + half).clamp(0.0, extent))
    };
    // Central-difference slope at one cell's spacing.
    let s = hm.scale().max(0.01);
    let slope = |x: f32, z: f32| {
        let gx = (sample(x + s, z) - sample(x - s, z)) / (2.0 * s);
        let gz = (sample(x, z + s) - sample(x, z - s)) / (2.0 * s);
        (gx * gx + gz * gz).sqrt()
    };
    let ok = |x: f32, z: f32| {
        if slope(x, z) > STEEP_LIMIT {
            return false;
        }
        if let Some(w) = water_y
            && sample(x, z) < w + FREEBOARD
        {
            return false;
        }
        if clearance <= 0.0 {
            return true;
        }
        (0..8).all(|i| {
            let a = i as f32 * std::f32::consts::TAU / 8.0;
            let (px, pz) = (x + a.sin() * clearance, z + a.cos() * clearance);
            slope(px, pz) <= STEEP_LIMIT && water_y.is_none_or(|w| sample(px, pz) >= w + FREEBOARD)
        })
    };

    let (x0, z0) = (translation.x, translation.z);
    if ok(x0, z0) {
        return;
    }
    let r0 = (x0 * x0 + z0 * z0).sqrt();
    if r0 < 1e-3 {
        return;
    }
    let (dx, dz) = (x0 / r0, z0 / r0);
    for i in 1..=MAX_PROBES {
        let sign = if i % 2 == 1 { 1.0 } else { -1.0 };
        let k = i.div_ceil(2) as f32 * STEP * sign;
        let r = r0 + k;
        if !(4.0..=half).contains(&r) {
            continue;
        }
        let (x, z) = (dx * r, dz * r);
        if ok(x, z) {
            translation.x = x;
            translation.z = z;
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 129×129, scale 1 → world [-64, 64]. Steep ramp (slope 1.0) for
    /// world X < 20, flat plateau beyond.
    fn ramp_then_plateau() -> bevy_symbios_ground::HeightMap {
        let mut hm = bevy_symbios_ground::HeightMap::new(129, 129, 1.0);
        for z in 0..129 {
            for x in 0..129 {
                let world_x = x as f32 - 64.0;
                hm.set(x, z, if world_x < 20.0 { world_x + 64.0 } else { 84.0 });
            }
        }
        hm
    }

    #[test]
    fn anchor_on_ramp_slides_to_the_plateau() {
        let hm = ramp_then_plateau();
        let mut t = Vec3::new(10.0, 0.0, 0.0);
        relocate_off_steep_ground(&hm, 128.0, 64.0, &mut t, None, 0.0);
        assert!(t.x > 20.0, "anchor should reach the plateau: {t:?}");
        assert_eq!(t.z, 0.0, "walk must stay on the bearing line");
    }

    #[test]
    fn flat_anchor_stays_put_and_hopeless_gives_up() {
        let hm = ramp_then_plateau();
        let mut flat = Vec3::new(40.0, 0.0, 0.0);
        relocate_off_steep_ground(&hm, 128.0, 64.0, &mut flat, None, 0.0);
        assert_eq!(flat.x, 40.0);

        // Uniform steep ramp everywhere: give up in place.
        let mut steep = bevy_symbios_ground::HeightMap::new(129, 129, 1.0);
        for z in 0..129 {
            for x in 0..129 {
                steep.set(x, z, x as f32);
            }
        }
        let mut t = Vec3::new(-20.0, 0.0, 10.0);
        relocate_off_steep_ground(&steep, 128.0, 64.0, &mut t, None, 0.0);
        assert_eq!((t.x, t.z), (-20.0, 10.0));
    }

    #[test]
    fn dry_check_composes_with_the_slope_walk() {
        // The plateau at world X > 20 is flat but the water line sits
        // just above the ramp's foot — a candidate that is flat but
        // drowned must be skipped. Plateau height 84 stays dry.
        let hm = ramp_then_plateau();
        let mut t = Vec3::new(10.0, 0.0, 0.0);
        relocate_off_steep_ground(&hm, 128.0, 64.0, &mut t, Some(80.0), 0.0);
        assert!(
            t.x > 20.0,
            "anchor should land on the dry flat plateau: {t:?}"
        );
        let landed = hm.get_height_at(t.x + 64.0, t.z + 64.0);
        assert!(landed >= 80.0 + FREEBOARD);
    }

    #[test]
    fn clearance_ring_pushes_past_the_slope_break() {
        // An anchor just past the slope break is flat at its centre but
        // a 10 m ring reaches back onto the ramp.
        let hm = ramp_then_plateau();
        let mut t = Vec3::new(22.0, 0.0, 0.0);
        relocate_off_steep_ground(&hm, 128.0, 64.0, &mut t, None, 10.0);
        assert!(
            t.x > 30.0,
            "ring-sampled anchor must clear the slope break: {t:?}"
        );
    }
}
