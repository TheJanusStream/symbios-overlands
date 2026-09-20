//! The sloop: a small sailing boat, and the boat family's universal floor.
//!
//! Every dimension here is a fraction of the seeded hull, read off
//! [`HullProfile`], so a 1.7 m seed and a 4.2 m one are the same boat at two
//! sizes rather than two different mistakes. The two exceptions are the ones
//! that *cannot* scale: the sanitiser's dimension floor ([`super::MIN_DIM`])
//! and the gateway air-draft cap ([`super::AIR_DRAFT_CAP`]), which is an
//! absolute height above the ground and so makes a big boat's rig relatively
//! shorter.
//!
//! The shape was agreed by render before any of this was written (#1359 rules
//! 1, 12 and 14). #1363 agreed the hero - `target/dump/vehicles2026-09/
//! sloop2.py` - and #1366 agreed what finishes her, prototyped over a python
//! twin of this module verified node for node against the live build
//! (`sloop2/sloop3.py`, `rigs.py`, `masses.py`):
//!
//! - [`rig`]: five rigs, each resolved against the air-draft cap by its own
//!   highest point, picked per seed by
//!   [`SloopRig`].
//! - four hull forms, each nothing but a plan station list ([`plan`]),
//!   picked by [`SloopHull`].
//! - [`dressing`]: the ladder of secondary masses by ornateness and wear.
//!
//! # Why the rigs are short
//!
//! A bermudan sloop's air draft is about 1.5 times her length. At 2.8 m that
//! is 4.2 m, and the lowest seeded gateway lintel is 2.86 m. A gaff rig puts
//! the same sail area under a short mast by hanging it from a spar that peaks
//! aft; a gunter does it with a yard; the one bermudan here is a STUMPY one,
//! whose whole rig is the air draft. Every rig on this boat has to fit inside
//! a box about as tall as she is long.

mod dressing;
mod hull;
mod rig;

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{BoatBlueprint, ParticleAura, SloopHull, SloopRig};

use super::super::common::{cuboid, id_quat, prim};
use super::profile::HullProfile;
use super::{BoatCraft, BoatFeel, BoatIdle, Propulsion, boat_colours, dim};
use rig::{Rig, rigging};

/// Section depth per unit half-beam. The knob that turns a plan form into a
/// hull: 1.10 puts the load waterline at about 0.73 of the overall length,
/// which is a sailing boat's. Lower and she is a shallow dish whose ends lift
/// clear of the water; higher and she is a deep narrow canoe.
const SECTION: f32 = 1.10;

/// The agreed plan form: `(z fraction of LOA, half-beam fraction)` from
/// transom to stem. Ten stations, well inside the sanitiser's sixteen.
/// Maximum beam just abaft midships, a full run aft to a real transom, and a
/// bow that comes to a point over the last tenth.
const TRANSOM: &[(f32, f32)] = &[
    (-0.500, 0.78),
    (-0.420, 0.88),
    (-0.300, 0.96),
    (-0.150, 1.00),
    (0.000, 0.99),
    (0.150, 0.93),
    (0.280, 0.80),
    (0.380, 0.58),
    (0.450, 0.34),
    (0.500, 0.06),
];

/// A PLUMB STEM: the half-beam held fuller to within a twentieth of the stem
/// and then collapsing, so the forefoot stays down and the entry is near
/// vertical - a workboat's bow.
const PLUMB_STEM: &[(f32, f32)] = &[
    (-0.500, 0.78),
    (-0.420, 0.88),
    (-0.300, 0.96),
    (-0.150, 1.00),
    (0.000, 0.99),
    (0.150, 0.94),
    (0.280, 0.86),
    (0.380, 0.72),
    (0.460, 0.46),
    (0.500, 0.07),
];

/// A RAKED STEM: the beam falls away early, so the forefoot cuts up and the
/// stem leans forward over a long fine entry.
const RAKED_STEM: &[(f32, f32)] = &[
    (-0.500, 0.78),
    (-0.420, 0.88),
    (-0.300, 0.96),
    (-0.150, 1.00),
    (0.000, 0.97),
    (0.150, 0.88),
    (0.280, 0.68),
    (0.380, 0.44),
    (0.450, 0.24),
    (0.500, 0.05),
];

/// A CANOE STERN: the run aft closes to a point instead of a transom. The
/// rudder follows it for free, because it hangs on the profile's own after
/// end; the cockpit well had to learn to read the profile first (#1366
/// defect 1), or its after end walked out through the planking.
const CANOE_STERN: &[(f32, f32)] = &[
    (-0.500, 0.07),
    (-0.440, 0.42),
    (-0.370, 0.74),
    (-0.250, 0.94),
    (-0.100, 1.00),
    (0.030, 0.98),
    (0.180, 0.90),
    (0.320, 0.72),
    (0.430, 0.40),
    (0.500, 0.06),
];

/// The plan station list for a hull form - the ONLY thing a hull form is.
/// The keel line, the boot top, the cove, the rails, the deck, the cockpit and
/// every mount follow it without being told.
fn plan(form: SloopHull) -> &'static [(f32, f32)] {
    match form {
        SloopHull::Transom => TRANSOM,
        SloopHull::PlumbStem => PLUMB_STEM,
        SloopHull::RakedStem => RAKED_STEM,
        SloopHull::CanoeStern => CANOE_STERN,
    }
}

pub(super) struct Sloop;

impl BoatCraft for Sloop {
    fn profile(&self, bp: &BoatBlueprint, seed: u64) -> HullProfile {
        profile_of(bp, SloopHull::for_seed(seed))
    }

    fn build(&self, ctx: &PartCtx, hull: &HullProfile) -> Generator {
        build_rigged(ctx, hull, SloopRig::for_seed(ctx.seed))
    }

    fn feel(&self) -> BoatFeel {
        // The retired monohull's numbers exactly. The hull *arrangements*
        // carried the feel before #1363 and a sloop is what the monohull was;
        // keeping them unchanged means the drive the owner validated with the
        // scale bridge (#1361) is the drive that ships.
        //
        // #1381's sweep DROVE her and kept her, and she is the yardstick the
        // other five boats were set against: 21.7 km/h, 90% of it in 1.55 s,
        // 112.0 deg/s, round in 6.2 m = 2.4 of her own 2.57 m length. Only
        // the runabout is quicker in a straight line and only the tug turns
        // in fewer of her own lengths, and both were chosen against this row.
        BoatFeel {
            mass_factor: 4.0,
            drive_accel: 9.0,
            turn_accel: 7.0,
            linear_damping: 1.5,
            angular_damping: 6.0,
        }
    }

    fn idle(&self) -> BoatIdle {
        // THE BASELINE, and the only one that is a definition rather than a
        // choice: every other hull's swell is a multiple of the sloop's, so
        // hers is 1.0 by construction. On the seeded amplitude band that is
        // 15-75 mm of heave (1.6-8.2 px at the game's 12 m camera) and
        // 1.0-5.0 degrees of list, which is what shipped and what the owner
        // has been looking at since #1361.
        BoatIdle {
            heave: 1.0,
            list: 1.0,
        }
    }

    fn propulsion(&self) -> Propulsion {
        // A sloop is a sailing rig; her voice is the wash and the wind in
        // it, never an engine's (owner decision C1 on #1368).
        Propulsion::Sail
    }

    fn overall_beam(&self, hull: &HullProfile, _seed: u64) -> f32 {
        // Her blueprint's beam: she is exactly as wide as her profile.
        hull.half_beam * 2.0
    }

    fn fx_mount(&self, aura: ParticleAura, hull: &HullProfile, _seed: u64) -> [f32; 3] {
        match aura {
            // A sloop has no funnel, so steam and wake both leave at the
            // stern, low, where a hovering hull's spray would.
            ParticleAura::Steam | ParticleAura::Wake => {
                // The after end of the wetted length, not the transom: on a
                // hull with this much rocker the transom can be clear of the
                // water, and a wake hung there would trail from thin air. Just
                // under the surface, because that is where spray leaves.
                [0.0, -hull.draft * 0.12, hull.waterline().0]
            }
            // Anything else is a flourish and belongs over the deck, amidships
            // where the boat is widest and it cannot hang over the side.
            _ => {
                let [x, y, z] = hull.cabin();
                [x, y + hull.freeboard * 0.6, z]
            }
        }
    }
}

/// The sloop's hull for a blueprint on a named hull form.
pub(super) fn profile_of(bp: &BoatBlueprint, form: SloopHull) -> HullProfile {
    HullProfile::new(bp, SECTION, plan(form))
}

/// The sloop on a named rig, dressed for the tiers `ctx` carries - what
/// [`Sloop::build`] draws with the seed's own rig, and what the guards sweep
/// every rig through.
pub(super) fn build_rigged(ctx: &PartCtx, hull: &HullProfile, rig: SloopRig) -> Generator {
    let c = boat_colours(ctx);
    let rigging = rigging(rig);
    let heights = Rig::new(hull, rigging);
    // Root: a hidden hub inside the canoe body. The hull cannot BE the
    // structural root - an elliptical section needs a per-axis node scale and
    // a structural root may not carry one (#798, the root-scale discipline) -
    // so the tree hangs off a cube small enough to be a pixel, amidships
    // where the hull is deepest, which is where the legacy `boat_root` box's
    // poking-out-at-the-ends bug becomes impossible rather than merely fixed.
    let hub = dim(hull.loa * 0.007);
    let mut root = prim(
        cuboid([hub; 3], c.timber.clone()),
        [0.0, -hull.freeboard * 0.2, 0.0],
        id_quat(),
    );
    let kids = &mut root.children;
    hull::skin(kids, hull, &c);
    hull::underbody(kids, hull, &c);
    hull::deck(kids, hull, &c);
    hull::deck_furniture(kids, hull, &c);
    rigging.build(&heights, kids, hull, &c);
    dressing::dress(kids, hull, &heights, &c, ctx.ornateness, ctx.wear);
    root
}

/// The height of this hull's rig above her design waterline on `rig` (m) -
/// its highest point as DERIVED, which is what the air-draft cap is resolved
/// against. The guards check the drawn tree against it as well.
#[cfg(test)]
pub(super) fn top_of_rig(hull: &HullProfile, rig: SloopRig) -> f32 {
    Rig::new(hull, rigging(rig)).top()
}
