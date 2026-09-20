//! The armoured car's ladder: masses that read at play distance, never
//! trinkets (#1359 F6), and wear where the chase camera looks. Cumulatively:
//!
//! - the PostApoc **raider** carries her up-armour on every tier: two
//!   applique slabs bolted on her driver's plate, a pile of scavenged kit
//!   lashed on her rear deck and a slab on her near flank;
//! - **Adorned**: a STOWAGE BIN on each flank over the arch line, a BEDROLL
//!   lashed along the near one and a TOW CABLE coiled on the glacis;
//! - **Ornate**: JERRYCANS in a rack on the near flank as well;
//! - **Battered**: a panel patched in PRIMER on the rear deck.
//!
//! A worn machine's near-front rim in bare steel is drawn with the wheels.
//!
//! The raider's mass goes where the CAMERA is: her applique drawn on the
//! glacis alone did not register at all from the stern quarter, which is the
//! side a chase camera spends its time on. The flank slab and the rear-deck
//! pile do.

use crate::pds::avatar::livery::ArmouredColours;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{ArmouredVariant, OrnatenessTier, WearTier};

use super::hull::{glacis_box, glacis_z};
use super::{ArmouredPlan, NEAR, NO_TURN, along_z, line, plate, solid};

/// One mass: it draws itself onto the kids.
type Mass = fn(&mut Vec<Generator>, &ArmouredPlan, &ArmouredColours);

/// What an ornateness tier adds, cumulatively.
fn ladder(o: OrnatenessTier) -> &'static [Mass] {
    match o {
        OrnatenessTier::Plain => &[],
        OrnatenessTier::Adorned => &[stowage, tow_cable],
        OrnatenessTier::Ornate => &[stowage, tow_cable, jerrycans],
    }
}

/// What a wear tier adds - the wear the wheels do not draw.
fn wear(w: WearTier) -> &'static [Mass] {
    match w {
        WearTier::Pristine | WearTier::Worn => &[],
        WearTier::Battered => &[patch],
    }
}

/// The variant's own masses, then the tier's, then the wear's.
pub(super) fn dress(
    kids: &mut Vec<Generator>,
    plan: &ArmouredPlan,
    c: &ArmouredColours,
    o: OrnatenessTier,
    w: WearTier,
) {
    let up_armour: &[Mass] = match plan.variant {
        ArmouredVariant::Works => &[],
        ArmouredVariant::Raider => &[applique],
    };
    for mass in up_armour.iter().chain(ladder(o)).chain(wear(w)) {
        mass(kids, plan, c);
    }
}

/// Adorned: a stowage bin on each flank over the arch line, and a bedroll
/// lashed along the near one - the kit a working machine carries.
fn stowage(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours) {
    let z = plan.at(-0.300);
    let (x, y) = plan.flank(z, 0.30);
    for s in [-1.0f32, 1.0] {
        kids.push(plate(
            [plan.at(0.110), plan.at(0.055), plan.at(0.150)],
            &c.bin,
            [s * (x + plan.at(0.020)), y + plan.at(0.010), z],
            plan.on_flank(s),
            0.12,
        ));
    }
    let zb = plan.at(0.050);
    let (xb, yb) = plan.flank(zb, 0.48);
    let roll = [
        (0.0, 0.0),
        (plan.at(0.022), 0.0),
        (plan.at(0.022), plan.at(0.135)),
        (0.0, plan.at(0.135)),
    ];
    kids.push(solid(
        &roll,
        10,
        true,
        &c.roll,
        [
            NEAR * (xb + plan.at(0.010)),
            yb + plan.at(0.008),
            zb - plan.at(0.068),
        ],
        along_z(),
    ));
}

/// How many points the coiled cable is drawn over, and the arc it sweeps
/// across the glacis (rad).
const COIL: usize = 7;
const COIL_ARC: (f32, f32) = (-1.9, 3.8);

/// Adorned: a tow cable coiled on the glacis, its ends shackled down - the
/// one piece of kit every armoured car carries where you can see it.
///
/// Its points read off [`glacis_z`], so it lies ON the ramp at whatever
/// height each one reaches.
fn tow_cable(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours) {
    let (_, yc, half_h, _) = glacis_box(plan);
    let hw = plan.half_width_at(plan.nose()) * 0.74;
    let bed = plan.at(0.004);
    let pts: Vec<([f32; 3], f32)> = (0..COIL)
        .map(|k| {
            let a = COIL_ARC.0 + (k as f32 / (COIL - 1) as f32) * COIL_ARC.1;
            let y = yc - half_h + 2.0 * half_h * (0.32 + 0.24 * a.cos());
            ([a.sin() * hw, y, glacis_z(plan, y, bed)], plan.at(0.010))
        })
        .collect();
    kids.push(line(&pts, 6, &c.arm));
}

/// Ornate: two cans in a rack on the near flank abaft the stowage bin.
fn jerrycans(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours) {
    for zf in [-0.430f32, -0.360] {
        let z = plan.at(zf);
        let (x, y) = plan.flank(z, 0.34);
        kids.push(plate(
            [plan.at(0.048), plan.at(0.038), plan.at(0.090)],
            &c.can,
            [NEAR * (x + plan.at(0.016)), y + plan.at(0.008), z],
            plan.on_flank(NEAR),
            0.10,
        ));
    }
}

/// The raider's up-armour: two slabs bolted flat on the driver's plate and
/// not quite square with it, a pile of scavenged kit lashed on the rear deck
/// and a slab on the near flank - a hand's width proud of each.
///
/// The kit pile and the slabs on the deck read [`ArmouredPlan::top_y`] and
/// [`ArmouredPlan::top_half`], which abaft the fighting compartment are the
/// REAR DECK's: written against the crown they would float over open air.
fn applique(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours) {
    let zf = plan.nose();
    let (hw, crown) = (plan.half_width_at(zf), plan.crown_at(zf));
    for (dx, dy, w, h) in [
        (-0.30f32, 0.36f32, 0.34f32, 0.42f32),
        (0.26, 0.00, 0.30, 0.38),
    ] {
        kids.push(plate(
            [hw * w * 2.0, h * crown * 1.5, plan.at(0.016)],
            &c.plate,
            [hw * dx * 2.0, crown * dy, zf + plan.at(0.004)],
            NO_TURN,
            0.06,
        ));
    }
    let zd = plan.at(-0.375);
    kids.push(plate(
        [plan.top_half(zd) * 1.20, plan.at(0.055), plan.at(0.115)],
        &c.bin,
        [0.0, plan.top_y(zd) + plan.at(0.026), zd],
        NO_TURN,
        0.20,
    ));
    let z = plan.at(-0.030);
    let (x, y) = plan.flank(z, 0.46);
    kids.push(plate(
        [plan.at(0.230), plan.at(0.018), plan.at(0.120)],
        &c.plate,
        [NEAR * (x + plan.at(0.006)), y + plan.at(0.004), z],
        plan.on_flank(NEAR),
        0.06,
    ));
}

/// Battered: a panel patched in primer on the rear deck, where the chase
/// camera looks.
fn patch(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours) {
    let z = plan.at(-0.360);
    kids.push(plate(
        [plan.top_half(z) * 1.30, plan.at(0.014), plan.at(0.140)],
        &c.primer,
        [0.0, plan.top_y(z) + plan.at(0.004), z],
        NO_TURN,
        0.06,
    ));
}
