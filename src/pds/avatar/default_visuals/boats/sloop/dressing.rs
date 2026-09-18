//! What a sloop carries on deck by ornateness and by wear (#1366) - the
//! owner's ladder, agreed on the phase-1 renders:
//!
//! | tier     | adds                                            |
//! |----------|-------------------------------------------------|
//! | every    | a laid coachroof over the cabin trunk           |
//! | Adorned  | a boom tent over the cockpit                    |
//! | Ornate   | the boom tent, a tender, a stern lantern, an anchor |
//! | Worn     | a tarp lashed over the coachroof                |
//! | Battered | the tarp, and fuel cans on the side deck        |
//!
//! ORNATENESS ADDS SECONDARY MASSES, NOT TRINKETS, and WEAR IS MASSES TOO,
//! never texture noise: at 109 px a metre a ball on a post is a pixel and a
//! grime pattern is a smear, but a tender on the foredeck and a tarp on the
//! coachroof change the boat's silhouette. The chase camera looks down 22.9
//! degrees, so everything here is on deck or aloft; the fender row the brief
//! suggested was rendered and dropped, because the topsides it hangs on roll
//! away out of that view.
//!
//! Every mass is placed off the hull profile, the trunk's own stations or a
//! spar the rig already drew - never off a restated fraction - which is what
//! lets the whole ladder stand on any rig and any hull form.

use crate::pds::generator::Generator;
use crate::seeded_defaults::{OrnatenessTier, WearTier};

use super::super::profile::HullProfile;
use super::super::{BoatColours, dim};
use super::hull::{COCKPIT, TRUNK_CROWN, block, deck_y, line, sweep, trunk_path};
use super::rig::Rig;

/// Dress the boat for her tiers.
pub(super) fn dress(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    rig: &Rig,
    c: &BoatColours,
    ornateness: OrnatenessTier,
    wear: WearTier,
) {
    trunk_top(kids, hull, c);
    if ornateness >= OrnatenessTier::Adorned {
        boom_tent(kids, hull, rig, c);
    }
    if ornateness == OrnatenessTier::Ornate {
        dinghy(kids, hull, c);
        stern_lantern(kids, hull, c);
        anchor(kids, hull, c);
    }
    if wear >= WearTier::Worn {
        tarp(kids, hull, c);
    }
    if wear == WearTier::Battered {
        cans(kids, hull, c);
    }
}

/// A laid coachroof: a band round the top of the trunk's own crown, in the
/// deck's timber rather than the topsides' paint.
///
/// On every boat, because it is not ornament. It is #1363's leftover item 5 -
/// at 12 m the trunk and the cockpit read as one light band amidships - and a
/// timber coachroof is what separates them.
fn trunk_top(kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
    let pts: Vec<([f32; 3], f32)> = trunk_path(hull)
        .into_iter()
        .map(|(p, r)| (p, r * 1.012))
        .collect();
    kids.push(sweep(
        &pts,
        20,
        [1.0, TRUNK_CROWN, 1.0],
        [0.13, 0.37],
        c.timber.clone(),
    ));
}

/// An awning over the cockpit - one node, because it is a crowned sweep whose
/// rim lands on the coaming and whose crown lands on the boom.
///
/// It needs no stanchions: a boat already carries a spar right over her
/// cockpit and a real boom tent hangs off it. The crown is read off the
/// rig's own boom, so a bermudan's longer, higher boom carries it too.
fn boom_tent(kids: &mut Vec<Generator>, hull: &HullProfile, rig: &Rig, c: &BoatColours) {
    let loa = hull.loa;
    let inset = hull.half_beam * 0.35;
    let pts: Vec<([f32; 3], f32)> = [
        COCKPIT.0 * loa + loa * 0.02,
        -0.30 * loa,
        COCKPIT.1 * loa - loa * 0.01,
    ]
    .iter()
    .map(|&z| {
        (
            [0.0, hull.sheer_z(z) + loa * 0.0079, z],
            (hull.half_beam_at(z) * 0.985 - inset).max(0.02),
        )
    })
    .collect();
    let ([_, rim_y, mid_z], mid_r) = pts[1];
    let rise = rig.boom_y_at(mid_z) - rim_y;
    kids.push(sweep(
        &pts,
        14,
        [1.0, rise / mid_r, 1.0],
        [0.0, 0.5],
        c.awning.clone(),
    ));
}

/// A tender stowed bottom-up on the foredeck.
///
/// An UPPER half-pipe of her own small profile, so she is the same idiom as
/// the boat under her. Her rim is sunk to the deck EDGE's height rather than
/// the crown's, so the cambered deck rises into her and she is bedded along
/// her whole length - the deck is her chocks. Her beam at each station is
/// held inside the foredeck under it, so a fine raked stem narrows her bow
/// rather than letting it hang over the side.
fn dinghy(kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
    let loa = hull.loa;
    let (centre, half_len, beam) = (0.355 * loa, 0.135 * loa, 0.058 * loa);
    let pts: Vec<([f32; 3], f32)> = [(-1.0f32, 0.30), (-0.4, 1.00), (0.45, 0.92), (1.0, 0.34)]
        .iter()
        .map(|&(f, rf)| {
            let z = centre + half_len * f;
            (
                [0.0, deck_y(hull, z) + loa * 0.004, z],
                dim((beam * rf).min(hull.half_beam_at(z) * 0.75)),
            )
        })
        .collect();
    kids.push(sweep(
        &pts,
        14,
        [1.0, 0.78, 1.0],
        [0.0, 0.5],
        c.dinghy.clone(),
    ));
    // A gunwale line round her, which is what says "boat" rather than "lump".
    for side in [-1.0f32, 1.0] {
        let rail: Vec<([f32; 3], f32)> = pts
            .iter()
            .map(|&([_, y, z], r)| ([side * r * 0.92, y + loa * 0.001, z], dim(loa * 0.0048)))
            .collect();
        kids.push(line(&rail, 8, c.timber.clone()));
    }
}

/// A lantern on a short post on the starboard quarter - off the centreline,
/// because the tiller sweeps the middle of that deck. The lantern is the
/// boat's lit window material.
fn stern_lantern(kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
    let loa = hull.loa;
    let z = -0.468 * loa;
    let x = hull.half_beam_at(z) * 0.72;
    let foot = hull.sheer_z(z) - loa * 0.004;
    let top = foot + loa * 0.070;
    kids.push(line(
        &[
            ([x, foot, z], dim(loa * 0.0060)),
            ([x, top, z], dim(loa * 0.0052)),
        ],
        8,
        c.brightwork.clone(),
    ));
    kids.push(block(
        [loa * 0.030, loa * 0.034, loa * 0.030],
        c.window.clone(),
        [x, top + loa * 0.012, z],
    ));
}

/// A stocked anchor lying on the foredeck, to starboard of the centreline -
/// on one bow, as a real one is stowed.
///
/// Three spines and no rotated node: a spine takes explicit point positions,
/// so a thing lying on its side costs nothing in the sanitise round trip.
fn anchor(kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
    let loa = hull.loa;
    let x = hull.half_beam * 0.30;
    let (ring_z, crown_z) = (0.455 * loa, 0.215 * loa);
    let on_deck = |z: f32, lift: f32| deck_y(hull, z) + lift;
    let r = dim(loa * 0.0090);
    kids.push(line(
        &[
            ([x, on_deck(ring_z, loa * 0.010), ring_z], dim(loa * 0.0062)),
            ([x, on_deck(crown_z, loa * 0.012), crown_z], r),
        ],
        8,
        c.lead.clone(),
    ));
    // The stock, across the shank just inside the ring.
    let stock_z = ring_z - loa * 0.016;
    let stock_y = on_deck(stock_z, loa * 0.010);
    kids.push(line(
        &[
            ([x - loa * 0.062, stock_y, stock_z], dim(loa * 0.0055)),
            ([x + loa * 0.062, stock_y, stock_z], dim(loa * 0.0055)),
        ],
        6,
        c.lead.clone(),
    ));
    // The arms, swept back from the crown.
    let arm_y = on_deck(crown_z, loa * 0.012);
    kids.push(line(
        &[
            (
                [x - loa * 0.054, arm_y, crown_z + loa * 0.030],
                dim(loa * 0.0075),
            ),
            ([x, arm_y, crown_z], r),
            (
                [x + loa * 0.054, arm_y, crown_z + loa * 0.030],
                dim(loa * 0.0075),
            ),
        ],
        8,
        c.lead.clone(),
    ));
}

/// A tarp lashed over the coachroof: a sweep over the trunk's own stations at
/// a hair more radius, so it lies on the crown it covers and stops where the
/// trunk's after three quarters do. Its colour is the scheme's canvas well
/// down in value - in the canvas itself a tarp on a white boat is invisible.
fn tarp(kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
    let loa = hull.loa;
    let pts: Vec<([f32; 3], f32)> = trunk_path(hull)
        .into_iter()
        .take(4)
        .map(|([x, y, z], r)| ([x, y + loa * 0.003, z], r * 1.07))
        .collect();
    kids.push(sweep(
        &pts,
        16,
        [1.0, TRUNK_CROWN * 0.92, 1.0],
        [0.0, 0.5],
        c.tarp.clone(),
    ));
}

/// Two cans lashed on the side decks beside the cockpit, one a side.
fn cans(kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
    let loa = hull.loa;
    let side_r = hull.half_beam * 0.18;
    let h = loa * 0.042;
    for (side, zf) in [(1.0f32, -0.30f32), (-1.0, -0.375)] {
        let z = zf * loa;
        let x = hull.half_beam_at(z) * 0.985 - side_r;
        kids.push(block(
            [loa * 0.026, h, loa * 0.034],
            c.lead.clone(),
            [side * x, deck_y(hull, z) + h * 0.40, z],
        ));
    }
}
