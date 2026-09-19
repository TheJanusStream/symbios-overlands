//! The coastal launch: a varnished gentleman's runabout (#1372) - the
//! CoastalResort and CivicCampus runabout. Her hull is mahogany under gloss
//! varnish whatever the scheme (the BRIGHTWORK finish, not the Plank texture,
//! #784), her decks are planked, and the scheme's colour is on her benches.

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::OrnatenessTier;

use super::super::RunaboutColours;
use super::super::profile::{FinlessForm, HullProfile};
use super::dressing::{Hatch, flagstaff, surrey_top, wear_ladder};
use super::hull::{
    DECK_CROWN, HULL_HOLLOW, UPRIGHT, bench, bench_half_width, cockpit_sole, deck_run, panel, skin,
    steering_wheel, stem_band, underbody, windscreen,
};
use super::{FALLING, Launch, LaunchForm};

/// The launch's plan form, `(z fraction of LOA, half-beam fraction)` transom
/// to stem: the half-beam held FULL to a wide flat transom and a fine raked
/// entry. The keel is `sheer - half_beam x section`, so where the beam falls
/// away the forefoot cuts up by itself and the stem leans forward over it.
const PLAN: &[(f32, f32)] = &[
    (-0.500, 0.94),
    (-0.380, 0.98),
    (-0.200, 1.00),
    (-0.020, 1.00),
    (0.120, 0.95),
    (0.240, 0.84),
    (0.340, 0.67),
    (0.420, 0.46),
    (0.470, 0.27),
    (0.500, 0.06),
];

pub(super) struct Coastal;

const FORM: LaunchForm = LaunchForm {
    plan: PLAN,
    planing: FinlessForm {
        beam: 1.22,
        freeboard: 0.70,
        bow_rise: 1.05,
        stern_rise: 0.0,
        section: 0.64,
        allowance: 0.040,
        sheer: FALLING,
    },
    screen: 0.080,
    cockpit_aft: -0.250,
};

impl Launch for Coastal {
    fn form(&self) -> &'static LaunchForm {
        &FORM
    }

    fn build(
        &self,
        kids: &mut Vec<Generator>,
        hull: &HullProfile,
        c: &RunaboutColours,
        ctx: &PartCtx,
    ) {
        let l = hull.loa;
        let cockpit = FORM.cockpit(hull);
        let (za, zf) = cockpit;
        skin(kids, hull, c, 0.0, HULL_HOLLOW);
        underbody(kids, hull, &c.antifoul, &c.bronze, 0.0, true);
        // The planked foredeck from the dash to the stem and the after deck
        // from the cockpit to the transom - the cockpit is the gap.
        kids.push(deck_run(hull, zf, hull.stem_z() - l * 0.004, &c.deck, 0.0));
        kids.push(deck_run(hull, hull.transom_z(), za, &c.deck, 0.0));
        let sole_y = cockpit_sole(kids, hull, c, cockpit);
        let half = bench_half_width(hull, cockpit, sole_y);
        // The driver's bench just abaft the dash, the rear bench against the
        // after deck.
        bench(
            kids,
            hull,
            c,
            zf - l * 0.085,
            l * 0.065,
            sole_y,
            half,
            l * 0.050,
            l * 0.070,
            0.30,
        );
        bench(
            kids,
            hull,
            c,
            za + l * 0.050,
            l * 0.060,
            sole_y,
            half,
            l * 0.050,
            l * 0.060,
            0.30,
        );
        windscreen(
            kids,
            hull,
            c,
            zf + l * 0.005,
            l * 0.070,
            l * 0.030,
            0.93,
            true,
        );
        steering_wheel(
            kids,
            hull,
            c,
            [
                -hull.half_beam * 0.38,
                hull.sheer_z(zf) + l * 0.018,
                zf - l * 0.022,
            ],
            l * 0.028,
        );
        // The engine hatch: a raised varnished box on the after deck.
        let hz = (hull.transom_z() + za) * 0.5 + l * 0.02;
        let hatch_len = (za - hull.transom_z()) * 0.60;
        let hy = hull.sheer_z(hz) + hull.half_beam_at(hz) * DECK_CROWN * 0.6;
        kids.push(panel(
            [hull.half_beam_at(hz) * 1.10, l * 0.030, hatch_len],
            &c.hatch,
            [0.0, hy + l * 0.008, hz],
            UPRIGHT,
            l * 0.02,
        ));
        stem_band(kids, hull, c);
        if ctx.ornateness != OrnatenessTier::Plain {
            // A sunpad on the engine hatch, in the scheme's own leather.
            kids.push(panel(
                [hull.half_beam_at(hz) * 1.00, l * 0.022, hatch_len * 0.86],
                &c.upholstery,
                [0.0, hy + l * 0.030, hz],
                UPRIGHT,
                l * 0.03,
            ));
            flagstaff(kids, hull, c);
        }
        if ctx.ornateness == OrnatenessTier::Ornate {
            surrey_top(kids, hull, c, cockpit);
        }
        wear_ladder(
            kids,
            hull,
            c,
            ctx,
            cockpit,
            &Hatch {
                z: hz,
                len: hatch_len,
                top: hy + l * 0.023,
            },
        );
    }
}
