//! The longship: a wide, shallow, keel-less DOUBLE-ENDER under one square
//! sail, with a row of shields along her gunwale - the boat of the Norse,
//! medieval, fae and classical themes (#1369), and the last craft type in
//! either family to be built.
//!
//! Two variants by THEME ([`LongshipVariant`]): the LONGSHIP everywhere but
//! AncientClassical, and the GALLEY there - the same hull with a bronze beak
//! at her forefoot and a bank of oars shipped along her gunwale, which is
//! what makes a galley read as one at 12 m.
//!
//! Her hull is swept from ONE [`HullProfile`], built by
//! [`HullProfile::finless`] on her own [`SheerLaw::Crescent`] - the fifth
//! law, and the first whose low point is amidships, because a double-ender's
//! two ends rise EQUALLY and no pair of factors on two independently
//! jittered blueprint sheers can hold that. See [`hull`] for the bored shell
//! whose cut rim is her gunwale, her clinker strakes and the shield row;
//! [`rig`] for the mast the air-draft cap holds by construction and the
//! square sail whose path runs athwartships; and [`dressing`] for the
//! steering oar that IS her draft, the crew's tent, the serpent prow and the
//! galley's ram and oars.
//!
//! The shape was agreed by render before any of this was written (#1359
//! rules 1, 12 and 14):
//! `target/dump/vehicles2026-09/longship/longship.py` is the python twin
//! this module was ported from, node for node, and `portcheck.py` beside it
//! is what says so.

mod dressing;
mod hull;
mod rig;

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::avatar::character::AvatarCharacter;
use crate::seeded_defaults::avatar::mood;
use crate::seeded_defaults::{BoatBlueprint, LongshipVariant, OrnatenessTier, ParticleAura};

use super::profile::{FinlessForm, HullProfile, SheerLaw};
use super::{BoatCraft, BoatFeel, BoatIdle, Propulsion, longship_colours};

/// Her plan form, `(z fraction of LOA, half-beam fraction)` stern post to
/// stem. SYMMETRIC, and fine at BOTH ends: a double-ender has no transom at
/// all, and the plan is what says so before the sheer does.
const PLAN: &[(f32, f32)] = &[
    (-0.500, 0.060),
    (-0.445, 0.330),
    (-0.370, 0.620),
    (-0.255, 0.860),
    (-0.120, 0.985),
    (0.000, 1.000),
    (0.120, 0.985),
    (0.255, 0.860),
    (0.370, 0.620),
    (0.445, 0.330),
    (0.500, 0.060),
];

/// Her proportions over the shared blueprint: narrow (an L:B of 4.6 to 5.4
/// against the blueprint's 3.2 to 3.8), LOW in the waist - her freeboard
/// amidships is about 0.04 L, against the junk's 0.08, which is what a
/// longship's is - a shallow round bilge, and her two ends rising equally on
/// [`SheerLaw::Crescent`].
///
/// `stern_rise` is the same 2.0 as `bow_rise` and is never read: Crescent
/// takes one rise from `bow_rise` and lays it at both ends. See
/// [`FinlessForm::stern_rise`].
///
/// Her allowance is the STEERING OAR, whose blade foot is her draft.
const FORM: FinlessForm = FinlessForm {
    beam: 0.700,
    freeboard: 0.375,
    bow_rise: 2.00,
    stern_rise: 2.00,
    section: 0.800,
    allowance: 0.018,
    sheer: SheerLaw::Crescent,
};

pub(super) struct Longship;

impl BoatCraft for Longship {
    fn profile(&self, bp: &BoatBlueprint, _seed: u64) -> HullProfile {
        profile_of(bp)
    }

    fn build(&self, ctx: &PartCtx, hull: &HullProfile) -> Generator {
        build_tiered(ctx, hull, LongshipKind::for_seed(ctx.seed))
    }

    fn feel(&self) -> BoatFeel {
        // #1381's sweep, agreed by the owner on 2026-09-20. The straight
        // line is the placeholder's and was right: she is the fastest hull
        // in the fleet under oar or sail, 25.3 km/h to the sloop's 21.7,
        // reached in 1.66 s. What moved is the TURN - a long shallow
        // keel-less hull skids, and at 133 deg/s she was out-turning a
        // keelboat. turn_accel 7.5 -> 5.0 and angular_damping 5.5 -> 6.5
        // give 75.2 deg/s and a circle of 10.7 m = 4.1 of her own lengths,
        // against the sloop's 2.4: she out-runs a yacht and loses to her in
        // a turn, which is the whole of what the hull says.
        BoatFeel {
            mass_factor: 3.6,
            drive_accel: 9.8,
            turn_accel: 5.0,
            linear_damping: 1.4,
            angular_damping: 6.5,
        }
    }

    fn idle(&self) -> BoatIdle {
        // Long, shallow and keel-less: she rolls freely, and she is the only
        // hull in the fleet that both heaves and lists MORE than the sloop.
        // 18-90 mm of heave, 1.4-7.0 degrees of list.
        BoatIdle {
            heave: 1.2,
            list: 1.4,
        }
    }

    fn fx_mount(&self, aura: ParticleAura, hull: &HullProfile, _seed: u64) -> [f32; 3] {
        match aura {
            // The sloop's rule, and 59 of her 75 seeds carry it: the after
            // end of the wetted length, just under the surface.
            ParticleAura::Wake => [0.0, -hull.draft * 0.22, hull.waterline().0],
            // A flourish needs a mount and may never be inside a volume
            // (#1367). On an OPEN boat under a square sail the one place
            // that is neither inside her shell nor under her cloth is over
            // her masthead, so it rises there. Nothing the tiers move is
            // read, so this mount reads no seed.
            _ => {
                let r = rig::Rig::new(hull);
                [0.0, r.top + hull.loa * 0.030, r.step_z]
            }
        }
    }

    fn propulsion(&self) -> Propulsion {
        // She SAILS (#1369 Q-J). Not a drive of her own: on 59 of her 75
        // seeds her oars are not drawn at all, so a rowing beat would be a
        // drive nobody can see - and `propulsion` is per CRAFT, not per
        // variant, so the galley's bank cannot buy a rowed voice for the
        // longship. The junk earned `Battened` because her battens are drawn
        // on every junk.
        Propulsion::Sail
    }

    fn overall_beam(&self, hull: &HullProfile, seed: u64) -> f32 {
        overall_beam_of(hull, LongshipVariant::for_seed(seed))
    }
}

/// What a longship's SEED decides about her, as against what her tiers dress
/// her in: the variant her theme draws, and whether her theme is one the
/// serpent belongs to.
///
/// Two answers off one theme, derived together so the character is resolved
/// once, and carried as a value so the guards can sweep every combination of
/// them rather than only the ones a seed happens to reach.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LongshipKind {
    pub(super) variant: LongshipVariant,
    /// NORSE_FEY - Nordic and Fantasy, 37 of her 75 seeds. A stem-head beast
    /// on a classical galley or a medieval trader is the costume error the
    /// affinity table exists to avoid, so this is not simply "Adorned".
    pub(super) serpent: bool,
}

impl LongshipKind {
    pub(super) fn for_seed(seed: u64) -> Self {
        Self::for_style(AvatarCharacter::for_seed(seed).style)
    }

    fn for_style(style: crate::seeded_defaults::scene::ThemeArchetype) -> Self {
        Self {
            variant: LongshipVariant::for_style(style),
            serpent: mood::holds(mood::NORSE_FEY, style),
        }
    }

    /// Every combination of the two, for the guards. Three of the four are
    /// reached by a live seed - a classical GALLEY carries no serpent, a
    /// medieval LONGSHIP none either, and a Norse or fae one does - and the
    /// fourth, a galley under a serpent, is swept because it is the HEAVIEST
    /// thing the type can draw and because nothing in the builder makes the
    /// two answers exclusive.
    #[cfg(test)]
    pub(super) const ALL: [Self; 4] = [
        Self {
            variant: LongshipVariant::Longship,
            serpent: false,
        },
        Self {
            variant: LongshipVariant::Longship,
            serpent: true,
        },
        Self {
            variant: LongshipVariant::Galley,
            serpent: false,
        },
        Self {
            variant: LongshipVariant::Galley,
            serpent: true,
        },
    ];

    /// A label for a guard's failure message.
    #[cfg(test)]
    pub(super) fn label(self) -> &'static str {
        match (self.variant, self.serpent) {
            (LongshipVariant::Longship, false) => "longship",
            (LongshipVariant::Longship, true) => "longship under a serpent",
            (LongshipVariant::Galley, false) => "galley",
            (LongshipVariant::Galley, true) => "galley under a serpent",
        }
    }
}

/// The longship's hull for a blueprint.
pub(super) fn profile_of(bp: &BoatBlueprint) -> HullProfile {
    HullProfile::finless(bp, &FORM, PLAN)
}

/// What stands outboard of her sheer (m), which her collider is as wide as.
///
/// The SHIELDS hang on the gunwale and stand a shield's thickness proud on
/// every longship; a galley's SHIPPED oar stands its blade well outboard of
/// that, and is the wider answer wherever she carries them.
pub(super) fn overall_beam_of(hull: &HullProfile, v: LongshipVariant) -> f32 {
    let l = hull.loa;
    let out = (hull.half_beam + l * 0.010).max(match v {
        LongshipVariant::Galley => hull.half_beam + l * 0.115,
        LongshipVariant::Longship => hull.half_beam,
    });
    2.0 * out
}

/// The resolution her HULL's own sweeps are drawn at - the shell, her bottom
/// paint and her two end plugs. The connectivity helper reads a swept
/// polygon as a round tube and so reads those too deep (#1393), which is why
/// a guard on her lowest point has to set them aside.
#[cfg(test)]
pub(super) const HULL_RES: u32 = hull::HULL_RES;

/// Her lowest point (m, under her waterline): the steering oar's blade foot.
#[cfg(test)]
pub(super) fn blade_foot(hull: &HullProfile) -> f32 {
    dressing::blade_foot(hull)
}

/// The height her rig is resolved to (m, over her waterline).
#[cfg(test)]
pub(super) fn top_of_rig(hull: &HullProfile) -> f32 {
    rig::top_of_rig(hull)
}

/// Whether the air-draft cap clamped this hull's mast.
#[cfg(test)]
pub(super) fn mast_is_capped(hull: &HullProfile) -> bool {
    rig::mast_is_capped(hull)
}

/// The longship dressed for the tiers `ctx` carries, on `variant` - what
/// [`Longship::build`] draws with the seed's own, and what the guards sweep
/// every tier and both variants through. The child order is the twin's, node
/// for node.
pub(super) fn build_tiered(ctx: &PartCtx, hull: &HullProfile, kind: LongshipKind) -> Generator {
    let c = longship_colours(ctx);
    let mut root = hull::root(hull, &c);
    let kids = &mut root.children;
    hull::skin(kids, hull, &c);
    hull::floor_boards(kids, hull, &c);
    hull::strakes(kids, hull, &c);
    hull::shield_row(kids, hull, &c, ctx.wear);
    let r = rig::Rig::new(hull);
    rig::rig(kids, hull, &c, &r, ctx.wear);
    rig::vane(kids, hull, &c, &r);
    dressing::steering_oar(kids, hull, &c);
    if kind.variant == LongshipVariant::Galley {
        dressing::ram(kids, hull, &c);
        dressing::oars(kids, hull, &c);
    }
    if ctx.ornateness != OrnatenessTier::Plain {
        dressing::awning(kids, hull, &c);
        if kind.serpent {
            dressing::serpent(kids, hull, &c);
        }
    }
    if ctx.ornateness == OrnatenessTier::Ornate {
        rig::banner(kids, hull, &c, &r);
    }
    root
}
