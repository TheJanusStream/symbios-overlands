//! The working scow: a flat-bottomed punt barge, the boat family's first
//! craft poled rather than sailed or driven (#1373) - the grimy, frontier
//! and farm themes' boat.
//!
//! One hull, one deckhouse, and a load picked by theme ([`ScowLoad`]):
//! crates and casks, hay, scrap, or the freight load under a small stern
//! wheel. The loads share everything else, so they are one match in
//! [`cargo`] rather than a trait. The hull is swept from ONE [`HullProfile`],
//! built by [`HullProfile::finless`] on a [`SheerLaw::Swim`] deck line - flat
//! through the hold, sweeping up hard at both ends - over a near-constant
//! beam, so where the plan closes the flat bottom lifts out of the water and
//! each end is a small raked transom face: a punt's ends, from the station
//! radii and the sheer alone. See [`hull`] for the polygon section that makes
//! her bottom flat.
//!
//! The shape was agreed by render before any of this was written (#1359
//! rules 1, 12 and 14): `target/dump/vehicles2026-09/scow/scow.py` is the
//! python twin this module was ported from, node for node.

mod cargo;
mod dressing;
mod gear;
mod house;
mod hull;

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{
    AvatarCharacter, BoatBlueprint, OrnatenessTier, ParticleAura, ScowLoad, WearTier,
};

use super::profile::{FinlessForm, HullProfile, SheerLaw};
use super::{BoatCraft, BoatFeel, Propulsion, scow_colours};
use cargo::Hold;
use house::House;

/// The scow's plan form, `(z fraction of LOA, half-beam fraction)` transom to
/// stem: parallel for the middle of her length and closing only a little to
/// a WIDE end at both ends - a scow is square-ended in plan, and the ends'
/// closing is what sweeps her bottom up out of the water.
const PLAN: &[(f32, f32)] = &[
    (-0.500, 0.90),
    (-0.460, 0.96),
    (-0.400, 0.99),
    (-0.300, 1.00),
    (0.000, 1.00),
    (0.300, 1.00),
    (0.400, 0.99),
    (0.460, 0.95),
    (0.500, 0.88),
];

/// Her proportions over the shared blueprint: beamy (L/B 2.5-3.0), low in
/// the waist, her ends swept up, and a rubbing batten's allowance under her
/// flat bottom for the draft. `section` is her depth per half-beam, so the
/// profile's keel is the drawn flat bottom (see [`hull`]).
const FORM: FinlessForm = FinlessForm {
    beam: 1.28,
    freeboard: 0.62,
    bow_rise: 0.85,
    stern_rise: 1.80,
    section: 0.66,
    allowance: 0.006,
    sheer: SheerLaw::Swim { pow: 3 },
};

/// The open hold, aft end to forward end, as fractions of the length.
const HOLD: (f32, f32) = (-0.110, 0.310);

/// The deckhouse, likewise.
const HOUSE: (f32, f32) = (-0.350, -0.130);

/// The samson post on the foredeck, and the fire drum abaft it.
const BITTS_ZF: f32 = 0.420;
const FIRE_ZF: f32 = 0.380;

/// How far a battered scow's stovepipe is knocked askew (rad about the
/// length) - it reads at 12 m, and her steam leaves it where it leans.
const BATTERED_LEAN: f32 = 0.20;

/// The hold's two ends (m).
fn hold_z(hull: &HullProfile) -> (f32, f32) {
    (HOLD.0 * hull.loa, HOLD.1 * hull.loa)
}

/// The stovepipe's lean for a wear tier.
fn lean(wear: WearTier) -> f32 {
    if wear == WearTier::Battered {
        BATTERED_LEAN
    } else {
        0.0
    }
}

pub(super) struct Scow;

impl BoatCraft for Scow {
    fn profile(&self, bp: &BoatBlueprint, _seed: u64) -> HullProfile {
        profile_of(bp)
    }

    fn build(&self, ctx: &PartCtx, hull: &HullProfile) -> Generator {
        build_load(ctx, hull, ScowLoad::for_seed(ctx.seed))
    }

    fn feel(&self) -> BoatFeel {
        // The legacy barge's numbers, as the brief asks: heavy. A
        // placeholder for #1381's per-type sweep.
        BoatFeel {
            mass_factor: 8.0,
            drive_accel: 6.5,
            turn_accel: 4.0,
            linear_damping: 2.2,
            angular_damping: 8.0,
        }
    }

    fn fx_mount(&self, aura: ParticleAura, hull: &HullProfile, seed: u64) -> [f32; 3] {
        let house = House::new(hull);
        match aura {
            // Steam leaves the stovepipe's open top - where it leans, on a
            // battered scow, which is why this mount takes the seed.
            ParticleAura::Steam => {
                house.stovepipe_top(hull, lean(AvatarCharacter::for_seed(seed).wear_tier()))
            }
            // Embers leave the fire drum's mouth, which the two Embers themes'
            // loads - scrap and the stern wheel - are exactly the ones to draw.
            ParticleAura::Embers => gear::fire_mouth(hull),
            // The wake leaves the after end of the wetted length just under
            // the surface, the sloop's rule: her after end is swept up clear
            // of the water.
            ParticleAura::Wake => [0.0, -hull.draft * 0.12, hull.waterline().0],
            // A flourish rises over the deckhouse roof.
            _ => [0.0, house.roof_y(0.0) + hull.freeboard * 0.6, house.zm],
        }
    }

    fn propulsion(&self) -> Propulsion {
        // Poled and sculled, the stern-wheel scow too: her wheel is her stern
        // gear, drawn, and she keeps the family's quiet voice.
        Propulsion::Poled
    }

    fn overall_beam(&self, hull: &HullProfile, _seed: u64) -> f32 {
        // The stern wheel sits inside her beam and the sweep trails astern.
        2.0 * hull.half_beam
    }
}

/// The scow's hull for a blueprint.
pub(super) fn profile_of(bp: &BoatBlueprint) -> HullProfile {
    HullProfile::finless(bp, &FORM, PLAN)
}

/// The scow carrying a named load, dressed for the tiers `ctx` carries -
/// what [`Scow::build`] draws with the seed's own load, and what the guards
/// sweep every load through. The child order is the twin's, node for node.
pub(super) fn build_load(ctx: &PartCtx, hull: &HullProfile, load: ScowLoad) -> Generator {
    let c = scow_colours(ctx);
    let mut root = hull::root(hull, &c);
    let kids = &mut root.children;
    hull::skin(kids, hull, &c);
    hull::decks(kids, hull, &c);
    let floor_y = hull::hold_floor(kids, hull, &c);
    let house = house::deckhouse(kids, hull, &c);
    house::stovepipe(kids, hull, &c, &house, lean(ctx.wear));
    gear::bitts(kids, hull, &c);
    gear::pole(kids, hull, &c, &house);
    if load.stern_wheel() {
        gear::paddle_wheel(kids, hull, &c, ctx.wear == WearTier::Battered);
    } else {
        gear::sweep_oar(kids, hull, &c);
    }
    if load.fire_drum() {
        gear::brazier(kids, hull, &c);
    }
    let hold = Hold::new(hull, floor_y);
    let top = cargo::stow(kids, hull, &c, &hold, load, ctx.ornateness);
    if ctx.wear != WearTier::Pristine {
        dressing::deck_patch(kids, hull, &c);
    }
    if ctx.wear == WearTier::Battered {
        dressing::roof_patch(kids, hull, &c, &house);
        dressing::tarp_over(kids, hull, &c, &hold, top);
    }
    root
}

/// Whether the tiers pile a load a second course high.
fn piled(orn: OrnatenessTier) -> bool {
    orn != OrnatenessTier::Plain
}
