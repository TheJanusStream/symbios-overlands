//! The runabout: a planing launch, the boat family's first craft under power
//! (#1372).
//!
//! Three variants over one idiom, picked by theme ([`RunaboutVariant`]): the
//! varnished coastal launch, the neon skimmer and the SportsRec power
//! catamaran. Each is a [`Launch`]: its own plan form, its planing
//! proportions over the shared blueprint, its cockpit stations and its build.
//! Every one is swept from ONE [`HullProfile`], built by
//! [`HullProfile::finless`]: a sheer that FALLS aft to a wide flat transom,
//! and a draft derived from the hull (she has no fin keel, and her hover is
//! a quarter of it). See [`hull`] for the polygon section that makes her
//! chines.
//!
//! The shape was agreed by render before any of this was written (#1359
//! rules 1, 12 and 14): `target/dump/vehicles2026-09/runabout/runabout.py`
//! is the python twin this module was ported from, node for node.

mod catamaran;
mod coastal;
mod dressing;
mod hull;
mod skimmer;

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{BoatBlueprint, ParticleAura, RunaboutVariant};

use super::profile::{FinlessForm, HullProfile, SheerLaw};
use super::{BoatCraft, BoatFeel, Propulsion, RunaboutColours, runabout_colours};
use hull::{HULL_HOLLOW, line, sole_depth};

/// Every runabout's deck line: lowest at the transom, the rise gathering
/// forward as `(zf + 0.5)^1.6`, as a runabout's foredeck sweeps up to her
/// stem (#1372).
const FALLING: SheerLaw = SheerLaw::Falling { pow: 1.6 };

/// What makes a runabout variant: its plan form, its planing proportions,
/// and where its cockpit runs.
pub(super) struct LaunchForm {
    /// `(z fraction of LOA, half-beam fraction)` transom to stem.
    plan: &'static [(f32, f32)],
    planing: FinlessForm,
    /// The dash - the cockpit's forward end - as a fraction of the length.
    screen: f32,
    /// The cockpit's after end, likewise.
    cockpit_aft: f32,
}

impl LaunchForm {
    /// The cockpit's after and forward ends (m).
    fn cockpit(&self, hull: &HullProfile) -> (f32, f32) {
        (self.cockpit_aft * hull.loa, self.screen * hull.loa)
    }
}

/// One runabout variant.
trait Launch {
    fn form(&self) -> &'static LaunchForm;

    /// Draw her into the root's children, in true metres, bow `+Z`.
    fn build(
        &self,
        kids: &mut Vec<Generator>,
        hull: &HullProfile,
        c: &RunaboutColours,
        ctx: &PartCtx,
    );

    /// Her overall beam (m) - what her collider is as wide as.
    fn overall_beam(&self, hull: &HullProfile) -> f32 {
        2.0 * hull.half_beam
    }

    /// The structural root. `apply_travel_pose` OVERWRITES the root's
    /// translation, so whatever the root is sits at the waterline centre -
    /// which on a bored monohull is the void under her sole. So the root is a
    /// SPINE, whose points are its own: here an engine bed from the shell's
    /// inner bottom up to the sole, honestly touching both.
    fn root(&self, hull: &HullProfile, c: &RunaboutColours) -> Generator {
        let l = hull.loa;
        let (za, zf) = self.form().cockpit(hull);
        let zm = (za + zf) * 0.5;
        let sole = hull.sheer_z(zm) - sole_depth(hull);
        let bottom = hull.sheer_z(zm) - hull.half_beam_at(zm) * hull.section * HULL_HOLLOW;
        line(
            &[
                ([0.0, bottom - l * 0.006, zm], l * 0.02),
                ([0.0, sole, zm], l * 0.02),
            ],
            6,
            &c.interior,
        )
    }
}

/// The builder for a variant.
fn launch(v: RunaboutVariant) -> &'static dyn Launch {
    match v {
        RunaboutVariant::Coastal => &coastal::Coastal,
        RunaboutVariant::Skimmer => &skimmer::Skimmer,
        RunaboutVariant::Catamaran => &catamaran::Catamaran,
    }
}

pub(super) struct Runabout;

impl BoatCraft for Runabout {
    fn profile(&self, bp: &BoatBlueprint, seed: u64) -> HullProfile {
        profile_of(bp, RunaboutVariant::for_seed(seed))
    }

    fn build(&self, ctx: &PartCtx, hull: &HullProfile) -> Generator {
        build_variant(ctx, hull, RunaboutVariant::for_seed(ctx.seed))
    }

    fn feel(&self) -> BoatFeel {
        // The legacy catamaran's numbers, as the brief asks: light and
        // nimble. A placeholder for #1381's per-type sweep.
        BoatFeel {
            mass_factor: 2.4,
            drive_accel: 13.0,
            turn_accel: 10.0,
            linear_damping: 1.0,
            angular_damping: 4.0,
        }
    }

    fn fx_mount(&self, aura: ParticleAura, hull: &HullProfile, _seed: u64) -> [f32; 3] {
        match aura {
            // Wake and steam leave the after end of the wetted length just
            // under the surface - which on a planing hull is the transom
            // itself, immersed, and on the catamaran the tunnel between her
            // transoms.
            ParticleAura::Steam | ParticleAura::Wake => {
                [0.0, -hull.draft * 0.12, hull.waterline().0]
            }
            // A flourish rises over the cockpit.
            _ => {
                let [x, y, z] = hull.deck_at(-0.08);
                [x, y + hull.freeboard * 1.2, z]
            }
        }
    }

    fn propulsion(&self) -> Propulsion {
        // A launch under power: the family's engine note (#1383).
        Propulsion::Engine
    }

    fn overall_beam(&self, hull: &HullProfile, seed: u64) -> f32 {
        launch(RunaboutVariant::for_seed(seed)).overall_beam(hull)
    }
}

/// The runabout's hull for a blueprint on a named variant.
pub(super) fn profile_of(bp: &BoatBlueprint, v: RunaboutVariant) -> HullProfile {
    let form = launch(v).form();
    HullProfile::finless(bp, &form.planing, form.plan)
}

/// The runabout on a named variant, dressed for the tiers `ctx` carries -
/// what [`Runabout::build`] draws with the seed's own variant, and what the
/// guards sweep every variant through.
pub(super) fn build_variant(ctx: &PartCtx, hull: &HullProfile, v: RunaboutVariant) -> Generator {
    let c = runabout_colours(ctx, v);
    let l = launch(v);
    let mut root = l.root(hull, &c);
    l.build(&mut root.children, hull, &c, ctx);
    root
}

/// Her overall beam on a named variant - the guards' width sweep.
#[cfg(test)]
pub(super) fn overall_beam_of(hull: &HullProfile, v: RunaboutVariant) -> f32 {
    launch(v).overall_beam(hull)
}
