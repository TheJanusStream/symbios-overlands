//! One hull profile per boat, and every line and mount read off it.
//!
//! The boat's counterpart of the airship's
//! [`EnvProfile`](crate::pds::avatar::parts::defaults::airship), and for the
//! same reason. The airship works because a single profile function feeds the
//! envelope, every trim line and every mount, so nothing it carries can float;
//! the boat did not, because its hull was a blob iso-surface nobody could
//! predict, and the rails, the bow fitting and the funnel were seated on
//! guessed fractions with hand-tuned embed constants to stop them hanging in
//! open air (#1359 diagnosis, #1363 item 1).
//!
//! # The shape of it
//!
//! A profile is up to [`MAX_SWEEP_POINTS`] stations. Each carries the
//! **plan form** (the half-beam at the deck edge) and the **sheer** (that deck
//! edge's height above the design waterline). The keel line is *not* a third
//! free number: one swept tube has one radius per station, which sets the
//! section's width and its depth together, so
//!
//! ```text
//! keel(z) = sheer(z) - half_beam(z) x section
//! ```
//!
//! and the hull's rocker falls out of its plan form. That coupling is the
//! point rather than a limitation - it is what makes a fine bow also a shallow
//! forefoot, the way a real hull's is - but it is also the profile's one
//! tuning knob: `section` is what the waterline length is set with (1.10 puts
//! a sloop's at 0.73 of her overall length).
//!
//! Every dimension is in TRUE METRES, hull centred on the design waterline at
//! the origin, bow at `+Z`. The assembler applies the travel yaw.
//!
//! # Three sheer laws, and a draft a type may derive
//!
//! A sailing boat's deck line SPRINGS: it sweeps up toward both ends from a
//! low point abaft midships. A planing launch's FALLS: it is highest at the
//! stem and lowest at a wide flat transom (#1372). A working scow's SWIMS:
//! flat through her hold and sweeping up hard at both ends, which is what
//! lifts her flat bottom out of the water into raked punt ends (#1373). That
//! is the one thing about the deck line a type chooses ([`SheerLaw`]);
//! everything read off it follows. And neither the launch nor the scow has a
//! fin keel, so her draft - which sets her hover - is not the blueprint's fin
//! draft but the deepest point of her own canoe body plus an allowance for a
//! skeg or a rubbing batten ([`HullProfile::finless`]).
//!
//! The steam tug (#1370) is the third finless type, and the first on the
//! sloop's own Spring law: a low towing deck aft, a bow springing hard to
//! her stem. Her allowance is the keel she drags, drawn as its own part.

use crate::pds::sanitize::limits::MAX_SWEEP_POINTS;
use crate::seeded_defaults::BoatBlueprint;

/// Where the sheer bottoms out, as a fraction of the overall length from
/// amidships. Abaft midships, because a boat's deck line sweeps up hardest
/// forward and its lowest point sits aft of centre - a sheer that bottoms out
/// exactly amidships reads as a symmetrical banana. A type reads it where a
/// part follows the sheer's own rise - the steam tug's bulwark (#1370).
pub(super) const SHEER_LOW: f32 = -0.08;

/// Where a fore-and-aft rig steps its mast, as a fraction of the overall
/// length forward of amidships - about 30 % of the length abaft the stem,
/// which is where a sloop's is.
const MAST_ZF: f32 = 0.196;

/// The cabin trunk's centre and the cockpit's centre, likewise. The trunk sits
/// just abaft the mast and the cockpit abaft the trunk, which is the standing
/// arrangement of a small fore-and-aft-rigged boat.
const CABIN_ZF: f32 = 0.030;
const COCKPIT_ZF: f32 = -0.271;

/// How a hull's deck edge rises and falls along her length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SheerLaw {
    /// A sailing boat's: up toward both ends from [`SHEER_LOW`], hardest
    /// forward - the freeboard, plus `sheer_bow` at the stem and
    /// `sheer_stern` at the transom, each rising as the square of the
    /// distance from the low point.
    Spring,
    /// A planing launch's (#1372): lowest at the transom and rising toward
    /// the stem as `(zf + 0.5)^pow`, `sheer_bow` over the freeboard there.
    /// `pow` over one keeps the rise forward, where a runabout's foredeck
    /// sweeps up to her stem.
    Falling { pow: f32 },
    /// A working scow's (#1373): [`Spring`](Self::Spring)'s low point and
    /// its two rises, laid as `u^pow` rather than the square - flat through
    /// the hold and sweeping up hard at both ends, a punt's swim. An integer
    /// power, so the deck line is a product of multiplies and never a libm
    /// call.
    Swim { pow: i32 },
}

/// A finless type's proportions over the shared blueprint (#1372, #1373,
/// #1370): each a factor on the blueprint's own number, so a stance still
/// moves the launch, the scow or the tug the way it moves the sloop.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FinlessForm {
    /// Half-beam over the blueprint's.
    pub(crate) beam: f32,
    /// Freeboard at the sheer's lowest point over the blueprint's freeboard.
    pub(crate) freeboard: f32,
    /// Sheer rise at the stem over the blueprint's `sheer_bow`.
    pub(crate) bow_rise: f32,
    /// Sheer rise at the transom over the blueprint's `sheer_stern` - zero on
    /// a launch, whose falling sheer does not read it.
    pub(crate) stern_rise: f32,
    /// Section depth per unit half-beam.
    pub(crate) section: f32,
    /// What hangs under the deepest point of the canoe body - a launch's
    /// skeg, a scow's rubbing batten, a tug's keel - as a fraction of the
    /// length.
    pub(crate) allowance: f32,
    /// How the deck line is laid.
    pub(crate) sheer: SheerLaw,
}

/// One station of the hull: everything about the boat at one point along her
/// length, all in metres from the design waterline.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Station {
    /// Distance from amidships as a fraction of the overall length - the
    /// plan form's own number, so a part keyed to a station can name it
    /// without re-deriving it from `z`.
    pub(crate) zf: f32,
    /// Distance from amidships, bow positive.
    pub(crate) z: f32,
    /// Half-beam at the deck edge.
    pub(crate) half_beam: f32,
    /// The deck edge's height above the waterline.
    pub(crate) sheer: f32,
    /// The bottom of the canoe body under it - derived, never authored.
    pub(crate) keel: f32,
}

/// A boat's hull, as the one function every part of her is read off.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HullProfile {
    /// Overall length, stem to transom (m).
    pub(crate) loa: f32,
    /// Maximum half-beam (m).
    pub(crate) half_beam: f32,
    /// Deck-edge height above the waterline at the sheer's lowest point (m).
    pub(crate) freeboard: f32,
    /// Sheer rise at the stem and at the transom (m).
    sheer_bow: f32,
    sheer_stern: f32,
    /// How those rises are laid along her.
    sheer_law: SheerLaw,
    /// Section depth per unit half-beam - the one knob that turns a plan form
    /// into a hull. See the module docs.
    pub(crate) section: f32,
    /// Depth of the deepest point of the underbody below the waterline (m).
    /// The keel, not the canoe body: a fin hangs below the hull.
    pub(crate) draft: f32,
    /// `(z fraction of LOA, half-beam fraction)` transom to stem - the type's
    /// own plan form, and the only thing about a hull that is a table.
    plan: &'static [(f32, f32)],
}

impl HullProfile {
    /// The profile for a seeded hull of this type: the blueprint's true
    /// dimensions under the type's own plan form and section depth.
    pub(crate) fn new(bp: &BoatBlueprint, section: f32, plan: &'static [(f32, f32)]) -> Self {
        debug_assert!(
            plan.len() >= 2 && plan.len() <= MAX_SWEEP_POINTS,
            "a plan form is 2..={MAX_SWEEP_POINTS} stations, got {}",
            plan.len()
        );
        Self {
            loa: bp.hull_len,
            half_beam: bp.beam * 0.5,
            freeboard: bp.freeboard,
            sheer_bow: bp.sheer_bow,
            sheer_stern: bp.sheer_stern,
            sheer_law: SheerLaw::Spring,
            section,
            draft: bp.draft,
            plan,
        }
    }

    /// A FINLESS hull for this blueprint - a planing launch (#1372), a
    /// working scow (#1373) or a steam tug (#1370): the type's own plan form
    /// and section, the blueprint's dimensions under the type's own factors,
    /// the type's own [`SheerLaw`], and a draft DERIVED from the hull itself -
    /// the deepest point of the canoe body plus the form's allowance, since
    /// none of them has a fin and her hover is a quarter of this draft.
    pub(crate) fn finless(
        bp: &BoatBlueprint,
        form: &FinlessForm,
        plan: &'static [(f32, f32)],
    ) -> Self {
        let mut hull = Self::new(bp, form.section, plan);
        hull.half_beam = bp.beam * 0.5 * form.beam;
        hull.freeboard = bp.freeboard * form.freeboard;
        hull.sheer_bow = bp.sheer_bow * form.bow_rise;
        hull.sheer_stern = bp.sheer_stern * form.stern_rise;
        hull.sheer_law = form.sheer;
        let deepest = hull
            .stations()
            .iter()
            .map(|s| s.keel)
            .fold(f32::INFINITY, f32::min);
        hull.draft = -deepest + hull.loa * form.allowance;
        hull
    }

    /// The deck edge's height above the waterline at plan fraction `zf`: the
    /// freeboard, plus the rise the hull's [`SheerLaw`] lays along her.
    pub(crate) fn sheer_at(&self, zf: f32) -> f32 {
        if let SheerLaw::Falling { pow } = self.sheer_law {
            return self.freeboard + self.sheer_bow * (zf + 0.5).max(0.0).powf(pow);
        }
        let (end, rise) = if zf >= SHEER_LOW {
            (0.5, self.sheer_bow)
        } else {
            (-0.5, self.sheer_stern)
        };
        let u = (zf - SHEER_LOW) / (end - SHEER_LOW);
        match self.sheer_law {
            SheerLaw::Swim { pow } => self.freeboard + rise * u.powi(pow),
            // The sloop's square, bit for bit as it always was.
            _ => self.freeboard + rise * u * u,
        }
    }

    /// Every station, transom to stem.
    pub(crate) fn stations(&self) -> Vec<Station> {
        self.plan
            .iter()
            .map(|&(zf, rf)| {
                let half_beam = (rf * self.half_beam).max(0.012);
                let sheer = self.sheer_at(zf);
                Station {
                    zf,
                    z: zf * self.loa,
                    half_beam,
                    sheer,
                    keel: sheer - half_beam * self.section,
                }
            })
            .collect()
    }

    /// Linear read of a station field at any `z`, so the underbody and the
    /// deck furniture size themselves off the same hull the skin does rather
    /// than off a second copy of it. `field` picks the member.
    fn read(&self, z: f32, field: impl Fn(&Station) -> f32) -> f32 {
        let st = self.stations();
        let (first, last) = (st[0], st[st.len() - 1]);
        if z <= first.z {
            return field(&first);
        }
        for pair in st.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if z <= b.z {
                let t = (z - a.z) / (b.z - a.z);
                return field(&a) + (field(&b) - field(&a)) * t;
            }
        }
        field(&last)
    }

    /// Half-beam at the deck edge at `z` (m).
    pub(crate) fn half_beam_at(&self, z: f32) -> f32 {
        self.read(z, |s| s.half_beam)
    }

    /// The deck edge's height above the waterline at `z` (m).
    pub(crate) fn sheer_z(&self, z: f32) -> f32 {
        self.read(z, |s| s.sheer)
    }

    /// The bottom of the canoe body at `z` (m, negative under water).
    pub(crate) fn keel_at(&self, z: f32) -> f32 {
        self.read(z, |s| s.keel)
    }

    /// The stem and the transom (m from amidships).
    pub(crate) fn stem_z(&self) -> f32 {
        self.loa * 0.5
    }
    pub(crate) fn transom_z(&self) -> f32 {
        -self.loa * 0.5
    }

    /// A point on the centreline of the deck at plan fraction `zf`.
    pub(crate) fn deck_at(&self, zf: f32) -> [f32; 3] {
        [0.0, self.sheer_at(zf), zf * self.loa]
    }

    /// A point on the deck EDGE at plan fraction `zf`, on `side` (`-1` port,
    /// `+1` starboard) - where a chainplate or a fender lands.
    pub(crate) fn deck_edge(&self, zf: f32, side: f32) -> [f32; 3] {
        let z = zf * self.loa;
        [side * self.half_beam_at(z), self.sheer_at(zf), z]
    }

    // --- named mount stations ----------------------------------------------
    //
    // A type mounts on these rather than on a fraction it wrote down, which is
    // what stops a cabin hovering over a hull it does not fit (the seeded
    // canopy that only the default chassis ever reached, #1359 diagnosis).
    // Dressing stations - a lamp here, a coil of rope there - belong with the
    // slice that dresses a boat (#1379) and are deliberately absent until
    // something reads them.

    /// The deck point a fore-and-aft rig steps its mast on.
    pub(crate) fn mast_step(&self) -> [f32; 3] {
        self.deck_at(MAST_ZF)
    }

    /// The deck point a cabin trunk is centred on.
    pub(crate) fn cabin(&self) -> [f32; 3] {
        self.deck_at(CABIN_ZF)
    }

    /// The deck point a cockpit well is centred on.
    pub(crate) fn cockpit(&self) -> [f32; 3] {
        self.deck_at(COCKPIT_ZF)
    }

    /// The stemhead - where a bowsprit, a figurehead or a forestay lands.
    pub(crate) fn bow_fitting(&self) -> [f32; 3] {
        self.deck_at(0.5)
    }

    /// The transom top - where a rudder head, a stern light or a wake anchors.
    pub(crate) fn stern_fitting(&self) -> [f32; 3] {
        self.deck_at(-0.5)
    }

    /// Where the canoe body crosses the design waterline, aft and forward -
    /// the load waterline's ends. Read rather than authored, so a hull whose
    /// plan form or section changes carries an honest waterline length.
    pub(crate) fn waterline(&self) -> (f32, f32) {
        let st = self.stations();
        let cross =
            |a: &Station, b: &Station| a.z + (b.z - a.z) * (0.0 - a.keel) / (b.keel - a.keel);
        // The submerged run, then each of its ends refined to the crossing -
        // unless the run reaches a hull end, where the wetted length simply
        // stops at the transom or the stem. Taking the min and max of the
        // crossings instead would collapse to one point on a hull that only
        // crosses once, which is a stern that never comes out of the water.
        let (Some(i), Some(j)) = (
            st.iter().position(|s| s.keel <= 0.0),
            st.iter().rposition(|s| s.keel <= 0.0),
        ) else {
            // A hull entirely clear of her own waterline is not a hull; answer
            // with her ends so a caller never sees an empty range.
            return (self.transom_z(), self.stem_z());
        };
        let aft = if i == 0 {
            st[0].z
        } else {
            cross(&st[i - 1], &st[i])
        };
        let fwd = if j + 1 == st.len() {
            st[j].z
        } else {
            cross(&st[j], &st[j + 1])
        };
        (aft, fwd)
    }
}
