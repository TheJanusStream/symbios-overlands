//! The rover's ladder: masses that read at play distance, never trinkets
//! (#1359 F6), and wear where the chase camera looks. Cumulatively:
//!
//! * **Adorned**: the STERN PALLET, a sample box each side bolted across the
//!   deck's after face.
//! * **Ornate**: the variant's own SECOND MASS - a wing panel folded out
//!   beside the first, a head plate lapped under the shell's fore end, or a
//!   second stele on the slab's far quarter - and the ANTENNA WHIP with its
//!   red beacon, the tallest thing she carries.
//! * **Worn**: the near-REAR rim in bare steel, drawn with the wheels.
//! * **Battered**: a DEAD CELL in the surveyor's panel, which is a livery
//!   slot rather than a node, or a panel patched in primer on the
//!   monolith's roof or the carapace's shell.
//!
//! # Her masses go on the stern, and that is a render result
//!
//! The pallet was first drawn on the open foredeck, where the solar panel
//! hid both boxes completely at 12 m. The deck's AFTER FACE is the face a
//! camera at yaw 315 looks straight at, and it is also the one placement all
//! three variants share, because every one of them leaves her stern clear.

use crate::pds::avatar::livery::RoverColours;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{OrnatenessTier, RoverVariant, WearTier};

use super::instruments::{CELLS, Panel, SLAB_W, Shell, slab_top};
use super::{
    NEAR, NO_TURN, RoverPlan, id_quat, line, plate, prim, solid, superellipsoid, tapered_plate,
};

/// One mass: it draws itself onto the kids.
type Mass = fn(&mut Vec<Generator>, &RoverPlan, &RoverColours);

/// What the tiers add to a rover: the masses in the order they are drawn,
/// the wheel whose rim is bare steel, and the solar cell that has died.
///
/// The last two are LIVERY slots rather than nodes, which is why they travel
/// with the masses instead of being drawn here: the wheels and the panel are
/// built before the dressing is.
pub(super) struct Ladder {
    masses: Vec<Mass>,
    /// The index into [`BodyPlan::wheels`](super::BodyPlan::wheels) of a
    /// worn machine's odd rim.
    pub(super) odd_rim: Option<usize>,
    /// The index into the surveyor's cell plates of a battered machine's
    /// dead one.
    pub(super) dead_cell: Option<usize>,
}

/// What the tiers add, cumulatively.
pub(super) fn ladder(plan: &RoverPlan, o: OrnatenessTier, w: WearTier) -> Ladder {
    let mut masses: Vec<Mass> = Vec::new();
    if o != OrnatenessTier::Plain {
        masses.push(pallet);
    }
    if o == OrnatenessTier::Ornate {
        masses.push(match plan.variant {
            RoverVariant::Surveyor => wing_panel,
            RoverVariant::Carapace => head_plate,
            RoverVariant::Monolith => stele,
        });
        masses.push(whip);
    }
    let mut dead_cell = None;
    if w == WearTier::Battered {
        if plan.variant == RoverVariant::Surveyor {
            // The outboard cell on the NEAR side, which is the one the chase
            // quarter shows whole.
            dead_cell = Some(CELLS - 1);
        } else {
            masses.push(patch);
        }
    }
    Ladder {
        masses,
        // The near REAR wheel: the last of the six, since
        // `BodyPlan::wheels` runs front axle first and `-x` before `+x`.
        odd_rim: (w != WearTier::Pristine).then(|| plan.wheels().len() - 1),
        dead_cell,
    }
}

/// Draw the ladder's masses, in order.
pub(super) fn dress(
    kids: &mut Vec<Generator>,
    plan: &RoverPlan,
    c: &RoverColours,
    ladder: &Ladder,
) {
    for mass in &ladder.masses {
        mass(kids, plan, c);
    }
}

/// Adorned: the STERN PALLET - a sample box each side bolted across the
/// deck's after face, the taller one on the near side so the two read as a
/// pallet rather than as a pair.
///
/// Both reach INSIDE the deck's after plane and under its top: a plate
/// deck's stern is a PLANE, not a swept form's ball, so anything laid on it
/// has to reach in (the armoured car's stern rails, #1375).
fn pallet(kids: &mut Vec<Generator>, plan: &RoverPlan, c: &RoverColours) {
    let z = plan.tail_z();
    for (s, hf) in [(NEAR, 0.115f32), (-NEAR, 0.086)] {
        let h = hf * plan.length;
        kids.push(plate(
            [plan.half_w() * 0.86, h, plan.at(0.105)],
            &c.boxes,
            [
                s * plan.half_w() * 0.48,
                plan.top() - h * 0.42,
                z - plan.at(0.040),
            ],
            NO_TURN,
            0.14,
        ));
    }
}

/// Ornate: an ANTENNA WHIP off the deck's far quarter, with a red beacon cap
/// at its tip.
///
/// **The cap counts, not the tip.** The whip's tip is clamped a cap's
/// half-height under rule 6's line so the highest DRAWN point lands under
/// it: clamping the tip alone left the beacon standing above the cap, at
/// 2.766 m over the ground at the 3.60 m corner against 2.740 m now. The
/// margin is 6 cm, so a planted fault smaller than that proves nothing
/// (#1374).
///
/// On a carapace it rises off the SHELL'S CROWN rather than off the deck,
/// which the shell overhangs.
fn whip(kids: &mut Vec<Generator>, plan: &RoverPlan, c: &RoverColours) {
    let l = plan.length;
    let (x, z) = (-NEAR * plan.half_w() * 0.82, -0.300 * l);
    let base = if plan.variant == RoverVariant::Carapace {
        Shell::of(plan).crown(z) - plan.at(0.030)
    } else {
        plan.top()
    };
    let tip = (base + 0.400 * l).min(plan.cap_y() - plan.at(0.012));
    kids.push(line(
        &[
            ([x, base - plan.at(0.012), z], plan.at(0.0085)),
            ([x, (base + tip) * 0.5, z - plan.at(0.012)], plan.at(0.0060)),
            ([x, tip, z - plan.at(0.040)], plan.at(0.0045)),
        ],
        6,
        &c.arm,
    ));
    let beacon = [
        (0.0, 0.0),
        (plan.at(0.012), plan.at(0.004)),
        (plan.at(0.012), plan.at(0.020)),
        (0.0, plan.at(0.024)),
    ];
    kids.push(solid(
        &beacon,
        8,
        false,
        &c.tip,
        [x, tip - plan.at(0.012), z - plan.at(0.040)],
        NO_TURN,
    ));
}

/// Ornate, the surveyor's: a SECOND panel, a wing folded out on the far side
/// of the first at the same tilt - read in the first panel's own frame, so
/// it lies flush with it whatever the seed.
fn wing_panel(kids: &mut Vec<Generator>, plan: &RoverPlan, c: &RoverColours) {
    let p = Panel::of(plan);
    let w = p.half_w * 0.80;
    let dx = -NEAR * (p.half_w + w * 0.5 - plan.at(0.004));
    kids.push(plate(
        [w, p.thick, p.run * 0.72],
        &c.frame,
        p.on(dx, 0.0, -p.run * 0.10),
        p.rot(),
        0.04,
    ));
    kids.push(plate(
        [w * 0.84, p.thick, p.run * 0.66],
        &c.cell,
        p.on(dx, plan.at(0.004), -p.run * 0.10),
        p.rot(),
        0.04,
    ));
}

/// Ornate, the carapace's: a HEAD PLATE lapped under the shell's fore end,
/// the way a beetle's pronotum laps its wing cases.
fn head_plate(kids: &mut Vec<Generator>, plan: &RoverPlan, c: &RoverColours) {
    let s = Shell::of(plan);
    kids.push(prim(
        superellipsoid(
            [s.half[0] * 0.70, s.half[1] * 0.74, s.half[2] * 0.34],
            1.0,
            1.0,
            12,
            20,
            c.shell.clone(),
        ),
        [0.0, s.at[1] - plan.at(0.012), s.at[2] + s.half[2] * 0.86],
        id_quat(),
    ));
}

/// Ornate, the monolith's: a second, lower STELE on the slab's far quarter,
/// a lit band round it - and held under rule 6's cap by construction, as the
/// slab it stands beside is.
fn stele(kids: &mut Vec<Generator>, plan: &RoverPlan, c: &RoverColours) {
    let l = plan.length;
    let (x, z) = (-NEAR * plan.half_w() * SLAB_W * 0.50, -0.200 * l);
    let h = (0.170 * l).min(plan.cap_y() - slab_top(plan));
    let y0 = slab_top(plan) - plan.at(0.008);
    kids.push(tapered_plate(
        [plan.at(0.070), h, plan.at(0.050)],
        &c.body,
        [x, y0 + h * 0.5, z],
        NO_TURN,
        0.05,
        [0.30, 0.30],
        [0.0; 2],
    ));
    kids.push(plate(
        [plan.at(0.074), plan.at(0.014), plan.at(0.054)],
        &c.strip,
        [x, y0 + h * 0.62, z],
        NO_TURN,
        0.05,
    ));
}

/// Battered, where the camera looks: a panel patched in primer on the
/// monolith's roof or on the carapace's shell.
///
/// The surveyor's wear is a DEAD CELL instead - [`ladder`] hands it to the
/// panel as a slot - so she adds no node at this tier at all.
fn patch(kids: &mut Vec<Generator>, plan: &RoverPlan, c: &RoverColours) {
    let l = plan.length;
    match plan.variant {
        RoverVariant::Monolith => {
            kids.push(plate(
                [
                    plan.half_w() * SLAB_W * 0.62,
                    plan.at(0.012),
                    plan.at(0.120),
                ],
                &c.primer,
                [
                    NEAR * plan.half_w() * SLAB_W * 0.10,
                    slab_top(plan) - plan.at(0.004),
                    -0.200 * l,
                ],
                NO_TURN,
                0.06,
            ));
        }
        RoverVariant::Carapace => {
            let s = Shell::of(plan);
            let z = s.at[2] - s.half[2] * 0.30;
            kids.push(prim(
                superellipsoid(
                    [s.half[0] * 0.40, 0.020 * l, s.half[2] * 0.22],
                    1.0,
                    1.0,
                    8,
                    12,
                    c.primer.clone(),
                ),
                [NEAR * s.half[0] * 0.30, s.crown(z) - plan.at(0.034), z],
                id_quat(),
            ));
        }
        // A battered surveyor's cell is a slot, and `ladder` never lists
        // this mass for her.
        RoverVariant::Surveyor => {}
    }
}
