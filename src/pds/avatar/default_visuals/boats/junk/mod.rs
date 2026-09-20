//! The junk: a battened-lug trader with a high flat transom over a raised
//! poop, a low bluff bow and two sails fanned up aft - the boat of the
//! eastern and the ritual old-world themes (#1371).
//!
//! One hull and one rig on every seed, no variant: a mainsail and a foresail,
//! each ONE flattened sweep whose stations are its battens (see [`rig`]). Her
//! tiers dress her in what reads at 12 m - a stern lantern and a mat shelter
//! at Adorned, a mizzen at Ornate, a replaced sail panel when she is worn and
//! a torn-out one when battered. The hull is swept from ONE [`HullProfile`],
//! built by [`HullProfile::finless`] on her own [`SheerLaw::Poop`] over the
//! scow's flat bottom: see [`hull`] for the bored shell whose wall is her
//! bulwark and the level poop deck over the break, and [`stern`] for the
//! transom board, the roundel on it and the slotted rudder whose foot is her
//! draft.
//!
//! The shape was agreed by render before any of this was written (#1359
//! rules 1, 12 and 14): `target/dump/vehicles2026-09/junk/junk.py` is the
//! python twin this module was ported from, node for node.

mod dressing;
mod hull;
mod rig;
mod stern;

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{BoatBlueprint, OrnatenessTier, ParticleAura};

use super::profile::{FinlessForm, HullProfile, SheerLaw};
use super::{BoatCraft, BoatFeel, BoatIdle, Propulsion, junk_colours};

/// The junk's plan form, `(z fraction of LOA, half-beam fraction)` transom
/// to stem: a WIDE flat transom, full through the middle, and a bluff bow
/// closing only to a broad flat headboard - the plan a junk is built on.
const PLAN: &[(f32, f32)] = &[
    (-0.500, 0.88),
    (-0.460, 0.92),
    (-0.400, 0.96),
    (-0.300, 0.985),
    (-0.150, 1.00),
    (0.000, 1.00),
    (0.150, 0.975),
    (0.280, 0.92),
    (0.380, 0.84),
    (0.450, 0.74),
    (0.500, 0.62),
];

/// Her proportions over the shared blueprint: beamy, low in the waist, a
/// small rise to a low bow and a big one into her poop on her own
/// [`SheerLaw::Poop`], a flat-bottomed section whose `section` is her depth
/// per half-beam (see [`hull`]), and her rudder as the allowance under her
/// canoe body - its foot is her draft.
const FORM: FinlessForm = FinlessForm {
    beam: 1.12,
    freeboard: 0.72,
    bow_rise: 0.25,
    stern_rise: 4.5,
    section: 0.80,
    allowance: 0.020,
    sheer: SheerLaw::Poop,
};

pub(super) struct Junk;

impl BoatCraft for Junk {
    fn profile(&self, bp: &BoatBlueprint, _seed: u64) -> HullProfile {
        profile_of(bp)
    }

    fn build(&self, ctx: &PartCtx, hull: &HullProfile) -> Generator {
        build_tiered(ctx, hull)
    }

    fn feel(&self) -> BoatFeel {
        // #1381's sweep, agreed by the owner on 2026-09-20. She carried the
        // sloop's tuple bit for bit until here, and only her longer hull made
        // her turn differently at all. A battened-lug trader is laden:
        // slower than a yacht, and her battens let her point well but she
        // does not accelerate. Measured: 16.3 km/h (4.52 m/s) to the sloop's
        // 21.7, 90% of it in 1.45 s, 47.9 deg/s, round in 10.8 m = 3.5 of
        // her own 3.05 m against the sloop's 2.4.
        BoatFeel {
            mass_factor: 4.0,
            drive_accel: 7.2,
            turn_accel: 4.5,
            linear_damping: 1.6,
            angular_damping: 6.5,
        }
    }

    fn idle(&self) -> BoatIdle {
        // Laden and beamy: a loaded trader is damped by her own cargo.
        // 12-60 mm of heave, 0.7-3.5 degrees of list.
        BoatIdle {
            heave: 0.8,
            list: 0.7,
        }
    }

    fn fx_mount(&self, aura: ParticleAura, hull: &HullProfile, _seed: u64) -> [f32; 3] {
        match aura {
            // Every junk seed's: the after end of the wetted length, just
            // under the surface - the sloop's rule, which on a junk is under
            // the poop's break, where her flat run leaves the water.
            ParticleAura::Wake => [0.0, -hull.draft * 0.12, hull.waterline().0],
            // A flourish rises over the rig every junk stands. Nothing on it
            // moves with the tiers - the mizzen is left out - so this mount
            // reads no seed.
            _ => [0.0, rig::standing_top(hull) + hull.freeboard * 0.3, 0.0],
        }
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::Battened
    }

    fn overall_beam(&self, hull: &HullProfile, _seed: u64) -> f32 {
        // Nothing she draws stands outboard of her sheer: the sails, yards
        // and battens lie in the centreline plane, sheeted amidships, and the
        // eyes lie on her bow.
        2.0 * hull.half_beam
    }
}

/// The junk's hull for a blueprint.
pub(super) fn profile_of(bp: &BoatBlueprint) -> HullProfile {
    HullProfile::finless(bp, &FORM, PLAN)
}

/// Her rudder's foot, the lowest point she draws (m, under her waterline).
#[cfg(test)]
pub(super) fn rudder_foot(hull: &HullProfile) -> f32 {
    stern::rudder_foot(hull)
}

/// The junk dressed for the tiers `ctx` carries - what [`Junk::build`]
/// draws with the seed's own, and what the guards sweep every tier through.
/// The child order is the twin's, node for node.
pub(super) fn build_tiered(ctx: &PartCtx, hull: &HullProfile) -> Generator {
    let c = junk_colours(ctx);
    let mut root = hull::root(hull, &c);
    let kids = &mut root.children;
    hull::skin(kids, hull, &c);
    hull::decks(kids, hull, &c);
    stern::transom(kids, hull, &c);
    stern::rudder(kids, hull, &c);
    let sails = rig::design(hull, ctx.ornateness == OrnatenessTier::Ornate);
    rig::rig(kids, hull, &c, &sails, ctx.wear);
    dressing::eyes(kids, hull, &c);
    stern::roundel(kids, hull, &c);
    stern::quarter_windows(kids, hull, &c);
    if ctx.ornateness != OrnatenessTier::Plain {
        dressing::lantern(kids, hull, &c);
        dressing::mat_shelter(kids, hull, &c);
    }
    root
}
