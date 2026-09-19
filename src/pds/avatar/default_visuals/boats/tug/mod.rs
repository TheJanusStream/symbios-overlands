//! The steam tug: a harbour tug with a tall raked funnel over her works
//! forward and a low towing deck aft - the boat of the themes with a boiler
//! in them, and the family's first craft under steam (#1370).
//!
//! One hull, one set of works - the engine casing, the wheelhouse and the
//! funnel, read off one [`Works`] - and what she works at, picked by theme
//! ([`TugVariant`]): the towing gear, or a harbour tender's cargo derrick
//! over the same deck. The hull is swept from ONE [`HullProfile`], built by
//! [`HullProfile::finless`] on the sloop's own [`SheerLaw::Spring`] - a low
//! working deck aft and a bow springing hard to a plumb stem - over a round
//! bilge. See [`hull`] for the bored shell whose own wall is her bulwark, and
//! for the two wedges that are her forefoot and her stem.
//!
//! The shape was agreed by render before any of this was written (#1359
//! rules 1, 12 and 14): `target/dump/vehicles2026-09/tug/tug.py` is the
//! python twin this module was ported from, node for node.

mod dressing;
mod gear;
mod hull;
mod works;

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{BoatBlueprint, OrnatenessTier, ParticleAura, TugVariant, WearTier};

use super::profile::{FinlessForm, HullProfile, SheerLaw};
use super::shape::underbody;
use super::{BoatCraft, BoatFeel, Propulsion, tug_colours};
use works::Works;

/// The tug's plan form, `(z fraction of LOA, half-beam fraction)` transom to
/// stem: a rounded counter aft, a short parallel body, and a full bow closing
/// to a plumb stem.
const PLAN: &[(f32, f32)] = &[
    (-0.500, 0.50),
    (-0.475, 0.70),
    (-0.440, 0.82),
    (-0.380, 0.92),
    (-0.280, 0.98),
    (-0.140, 1.00),
    (0.000, 1.00),
    (0.120, 0.97),
    (0.230, 0.90),
    (0.320, 0.79),
    (0.400, 0.62),
    (0.460, 0.40),
    (0.500, 0.08),
];

/// Her proportions over the shared blueprint, on the sloop's Spring law and
/// no sheer of her own: a low working deck aft (0.07-0.08 of her length at
/// the sheer's low point) and a bow springing to about 0.17 of it, a round
/// bilge whose `section` is exactly her depth per half-beam (see [`hull`]),
/// and the keel she drags as the allowance under her canoe body.
const FORM: FinlessForm = FinlessForm {
    beam: 1.06,
    freeboard: 0.70,
    bow_rise: 1.60,
    stern_rise: 0.60,
    section: 1.00,
    allowance: 0.028,
    sheer: SheerLaw::Spring,
};

/// A tyre fender's radius and width, as fractions of the length. The tyres
/// hang outboard of the sheer, so her overall beam is theirs.
const TYRE_R: f32 = 0.028;
const TYRE_W: f32 = 0.013;

pub(super) struct Tug;

impl BoatCraft for Tug {
    fn profile(&self, bp: &BoatBlueprint, _seed: u64) -> HullProfile {
        profile_of(bp)
    }

    fn build(&self, ctx: &PartCtx, hull: &HullProfile) -> Generator {
        build_variant(ctx, hull, TugVariant::for_seed(ctx.seed))
    }

    fn feel(&self) -> BoatFeel {
        // The legacy barge's numbers - the scow's - as the brief asks: the
        // starting point. A tug is all engine and turns on her heel, so
        // #1381's per-type sweep is where she may part from the scow.
        BoatFeel {
            mass_factor: 8.0,
            drive_accel: 6.5,
            turn_accel: 4.0,
            linear_damping: 2.2,
            angular_damping: 8.0,
        }
    }

    fn fx_mount(&self, aura: ParticleAura, hull: &HullProfile, _seed: u64) -> [f32; 3] {
        let works = Works::new(hull);
        match aura {
            // Steam - her own, or the wake her boiler promotes (#1370) - and a
            // wood-fired stack's embers leave the funnel's mouth: the point
            // the funnel's own placement gives at its full height, rake
            // included. Nothing on her funnel moves with the tiers, so this
            // mount reads no seed.
            ParticleAura::Steam | ParticleAura::Embers => works.mouth(),
            // A wake - which a drawn tug never trails, but the mount is total
            // - leaves the after end of the wetted length just under the
            // surface, the sloop's rule.
            ParticleAura::Wake => [0.0, -hull.draft * 0.12, hull.waterline().0],
            // A flourish rises over the wheelhouse roof.
            _ => [
                0.0,
                works.wtop + hull.freeboard * 0.6,
                works.wheelhouse_zm(),
            ],
        }
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::Steam
    }

    fn overall_beam(&self, hull: &HullProfile, _seed: u64) -> f32 {
        // The tyre fenders hang outboard of the sheer.
        2.0 * (hull.half_beam + hull.loa * TYRE_W)
    }
}

/// The tug's hull for a blueprint.
pub(super) fn profile_of(bp: &BoatBlueprint) -> HullProfile {
    HullProfile::finless(bp, &FORM, PLAN)
}

/// Her funnel's mouth, where her steam or her embers leave it.
#[cfg(test)]
pub(super) fn funnel_mouth(hull: &HullProfile) -> [f32; 3] {
    Works::new(hull).mouth()
}

/// The tug working as `variant`, dressed for the tiers `ctx` carries - what
/// [`Tug::build`] draws with the seed's own variant, and what the guards
/// sweep every variant through. The child order is the twin's, node for
/// node.
pub(super) fn build_variant(ctx: &PartCtx, hull: &HullProfile, variant: TugVariant) -> Generator {
    let c = tug_colours(ctx);
    let battered = ctx.wear == WearTier::Battered;
    let mut root = hull::root(hull, &c);
    let kids = &mut root.children;
    hull::skin(kids, hull, &c);
    hull::deck(kids, hull, &c);
    hull::keel(kids, hull, &c);
    underbody(kids, hull, &c.antifoul, &c.bronze, 0.0, true);
    gear::bitts(kids, hull, &c);
    gear::puddings(kids, hull, &c);
    let works = Works::new(hull);
    works::casing(kids, hull, &c, &works);
    works::wheelhouse(kids, hull, &c, &works);
    works::funnel(kids, hull, &c, &works, ctx.wear);
    match variant {
        TugVariant::Towing => {
            gear::hook(kids, hull, &c, &works);
            gear::arches(kids, hull, &c, battered);
        }
        TugVariant::Derrick => gear::derrick(kids, hull, &c, &works),
    }
    gear::tyres(kids, hull, &c, battered);
    if ctx.ornateness != OrnatenessTier::Plain {
        dressing::boat(kids, hull, &c, &works, battered);
        dressing::vents(kids, hull, &c, &works);
    }
    if ctx.ornateness == OrnatenessTier::Ornate {
        dressing::signal_mast(kids, hull, &c, &works);
        if variant.tows() {
            gear::hawser(kids, hull, &c);
        }
    }
    if ctx.wear != WearTier::Pristine {
        dressing::deck_patch(kids, hull, &c);
    }
    root
}
