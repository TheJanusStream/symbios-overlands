//! The rover's equipment deck: ONE plate, and the band round its edge that
//! carries her identity.
//!
//! **A deck is a PLATE.** A deck's whole job is to be flat, and #1375's
//! finding decides the vocabulary before the shape is drawn: a Spine pushes
//! a radial ring normal at every station whatever its resolution, so a swept
//! deck shades as a barrel. The res-4 sweep over this same plan form was
//! drawn beside it and rejected for exactly that, and its rounded crown also
//! floated the antenna whip off into a second component.
//!
//! So she is one [`tapered_plate`] with a FLAT chamfer (one bevel segment),
//! cut to the section the plan publishes, with a `taper_bottom` that draws
//! her underside in - a machined plate rather than a slab.

use crate::pds::avatar::livery::RoverColours;
use crate::pds::generator::Generator;
use crate::seeded_defaults::RoverVariant;

use super::{NO_TURN, RoverPlan, plate, tapered_plate};

/// How far the deck's underside is drawn in, per axis `[x, z]`.
const DECK_TAPER_BOTTOM: [f32; 2] = [0.12, 0.06];

/// The chamfer the deck's vertical edges are cut at, as a fraction of its
/// smaller footprint axis. The monolith's is nearly square: her whole idiom
/// is the hard edge, and a 0.10 chamfer on the deck under a slab reads as a
/// rounded tray.
const DECK_CHAMFER: f32 = 0.10;
const MONOLITH_CHAMFER: f32 = 0.03;

/// The deck and, on every variant but the carapace, the band round it.
///
/// The carapace has no band: her shell overhangs the deck's edge, so a band
/// there would be invisible, and her identity is the lit DORSAL RIDGE along
/// the shell's crown instead ([`super::instruments`]). Her deck is drawn in
/// the dark running-deck colour rather than the scheme, because the scheme
/// is on the shell.
pub(super) fn plate_and_band(kids: &mut Vec<Generator>, plan: &RoverPlan, c: &RoverColours) {
    let carapace = plan.variant == RoverVariant::Carapace;
    let (_, _, zc, run) = plan.deck_run();
    kids.push(tapered_plate(
        [plan.half_w() * 2.0, plan.depth() * 2.0, run],
        if carapace { &c.under } else { &c.body },
        [0.0, 0.0, zc],
        NO_TURN,
        if plan.variant == RoverVariant::Monolith {
            MONOLITH_CHAMFER
        } else {
            DECK_CHAMFER
        },
        [0.0; 2],
        DECK_TAPER_BOTTOM,
    ));
    if !carapace {
        band(kids, plan, c);
    }
}

/// **Identity.** A band right round the deck's edge, a hair proud of it and
/// a hair over the datum - the one trim line every variant but the carapace
/// wears, and lit on every seed she has.
///
/// It is what says "livery" on a machine whose largest surface is a solar
/// panel. Lit alone, without the rims, half the machine dies; the rims lit
/// alone leave her body unclaimed. Both were drawn (#1378 Q-H).
fn band(kids: &mut Vec<Generator>, plan: &RoverPlan, c: &RoverColours) {
    let (_, _, zc, run) = plan.deck_run();
    let proud = plan.at(0.012);
    kids.push(plate(
        [
            plan.half_w() * 2.0 + proud,
            plan.depth() * 0.36,
            run + proud,
        ],
        &c.strip,
        [0.0, plan.depth() * 0.42, zc],
        NO_TURN,
        0.10,
    ));
}
