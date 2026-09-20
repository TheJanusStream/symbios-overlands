//! The junk's eyes and her ornateness ladder, as masses that read at 12 m.
//!
//! The EYE on each bow is her oculus, a turned disc in the seed's accent
//! with a dark pupil. The bow is the far end from the chase camera, so it
//! reads from the bow quarter and never from behind her; the roundel on her
//! transom is her identity there (see `stern`).
//!
//! ORNATENESS: Adorned carries a stern lantern on a gooseneck over her
//! starboard quarter - every junk's, whatever her theme - and a mat shelter
//! over the main deck abaft the mainmast; Ornate adds a mizzen (see `rig`).
//! WEAR is on her mainsail, where the camera looks (see `rig`): a replaced
//! panel when she is worn, a torn-out one when battered.

use std::f32::consts::PI;

use crate::pds::generator::Generator;

use super::super::JunkColours;
use super::super::profile::HullProfile;
use super::super::shape::{UPRIGHT, line, sweep, turned};
use super::hull::{deck_edge, lay_on, main_deck, poop_deck, side_point};

/// The eyes' station and radius, as fractions of the length.
const EYE_ZF: f32 = 0.435;
const EYE_R: f32 = 0.042;

/// The mat shelter's after and fore ends, as fractions of the length: over
/// the main deck abaft the mainmast, under the boom.
const SHELTER: (f32, f32) = (-0.235, -0.075);

/// An eye on each bow, laid on the flared topsides a hand under the sheer:
/// a disc in the seed's accent and a dark pupil proud of it.
pub(super) fn eyes(kids: &mut Vec<Generator>, hull: &HullProfile, c: &JunkColours) {
    let l = hull.loa;
    let z = EYE_ZF * l;
    let r = l * EYE_R;
    for side in [-1.0f32, 1.0] {
        let (p, n) = side_point(hull, z, 0.40, side);
        let rot = lay_on(n);
        let base = [p[0] - n[0] * l * 0.004, p[1] - n[1] * l * 0.004, z];
        kids.push(turned(
            &[(r, 0.0), (r, l * 0.006)],
            16,
            false,
            &c.mark,
            base,
            rot,
            0.0,
        ));
        kids.push(turned(
            &[(r * 0.46, l * 0.004), (r * 0.46, l * 0.009)],
            12,
            false,
            &c.inlay,
            base,
            rot,
            0.0,
        ));
    }
}

/// Adorned: a stern lantern on a gooseneck - the legacy kits.rs idea
/// (#1359) - over the starboard quarter, clear of an Ornate junk's mizzen to
/// port: an iron post up from the poop inside her rail, its arm curling aft
/// over the transom, and a paper lantern hung from its tip, lit the colour a
/// flame is.
pub(super) fn lantern(kids: &mut Vec<Generator>, hull: &HullProfile, c: &JunkColours) {
    let l = hull.loa;
    let t = hull.transom_z();
    let z = t + l * 0.035;
    let x = hull.half_beam_at(z) * 0.78;
    let y0 = poop_deck(hull) - l * 0.004;
    let top = hull.sheer_z(t) + l * 0.11;
    let tip = [x * 0.92, top + l * 0.010, t - l * 0.030];
    kids.push(line(
        &[
            ([x, y0, z], l * 0.0060),
            ([x, top - l * 0.010, z], l * 0.0055),
            ([x * 0.97, top + l * 0.012, z - l * 0.030], l * 0.0050),
            (tip, l * 0.0045),
        ],
        8,
        &c.iron,
    ));
    let (r, h) = (l * 0.020, l * 0.042);
    let hang = [tip[0], tip[1] - l * 0.012 - h, tip[2]];
    kids.push(line(
        &[
            ([tip[0], tip[1] + l * 0.002, tip[2]], l * 0.0025),
            ([hang[0], hang[1] + h * 0.9, hang[2]], l * 0.0025),
        ],
        5,
        &c.iron,
    ));
    kids.push(turned(
        &[(r * 0.45, 0.0), (r, h * 0.28), (r, h * 0.72), (r * 0.45, h)],
        14,
        true,
        &c.lamp,
        hang,
        UPRIGHT,
        0.0,
    ));
}

/// Adorned: a woven-mat shelter over the main deck abaft the mainmast - a
/// barrel of matting on the deck, an upper half-pipe on its level stretch
/// (the scow's roof idiom) - and two bamboo hoops over it.
pub(super) fn mat_shelter(kids: &mut Vec<Generator>, hull: &HullProfile, c: &JunkColours) {
    let l = hull.loa;
    let (z0, z1) = (SHELTER.0 * l, SHELTER.1 * l);
    let zs = [z0, (z0 + z1) * 0.5, z1];
    let r = zs
        .iter()
        .map(|&z| deck_edge(hull, z, main_deck(hull, z)))
        .fold(f32::INFINITY, f32::min)
        * 0.74;
    let y = zs
        .iter()
        .map(|&z| main_deck(hull, z))
        .fold(f32::NEG_INFINITY, f32::max)
        - l * 0.004;
    let hgt = l * 0.075;
    kids.push(sweep(
        &[
            ([0.0, y, z0], r * 0.96),
            ([0.0, y, (z0 + z1) * 0.5], r),
            ([0.0, y, z1], r * 0.96),
        ],
        14,
        [1.0, hgt / r, 1.0],
        [0.0, 0.5],
        &c.mat,
        0.0,
    ));
    for f in [0.2f32, 0.8] {
        let z = z0 + (z1 - z0) * f;
        let rr = r * (0.97 + 0.03 * (1.0 - (f - 0.5).abs() * 2.0)) * 1.012;
        let hoop: Vec<_> = [0.02, PI * 0.25, PI * 0.5, PI * 0.75, PI - 0.02]
            .into_iter()
            .map(|a: f32| ([-rr * a.cos(), y + hgt / r * rr * a.sin(), z], l * 0.0045))
            .collect();
        kids.push(line(&hoop, 6, &c.batten));
    }
}
