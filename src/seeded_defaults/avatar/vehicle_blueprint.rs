//! Vehicle proportion blueprint.
//!
//! Turns the seeded [`AvatarBody`] multiplier knobs (plus a vehicle
//! [`VehicleStance`] register sampled the way the body samples its
//! [`StylizationTier`](super::body::StylizationTier)) into concrete
//! world-space proportions and mount landmarks for one vehicle chassis. The
//! part builders ([`crate::pds::avatar::parts`]) size their geometry from it
//! and the family assembler ([`crate::pds::avatar::default_visuals`]) reads
//! the *same* landmarks for its mount anchors - so the two can never drift
//! (the fixed-anchor / part-internal-constant coupling that floated stacks
//! and bows off mis-sized hulls, #782/#783).
//!
//! Per-family structs behind the [`VehicleBlueprint`] enum: a boat and an
//! airship have genuinely different landmarks (deck line vs belly line), so
//! each family exposes only its own, and a part reads its family's blueprint
//! or nothing. Families are added as their redesigns wire them; a chassis
//! with no blueprint yet (and the rigged humanoid family, whose body is a
//! parametric `symbios-avatar` record rather than assembled parts) yields
//! `None`.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use super::body::AvatarBody;
use super::chassis::ChassisFamily;
use crate::seeded_defaults::scene::range_f32;

/// Sub-stream salt so the blueprint's stance + jitter draws are decorrelated
/// from every sibling avatar deriver (body, palette, outfit, …).
const VEHICLE_BLUEPRINT_SALT: u64 = 0x0EE1_C0DE_0EE1_C0DE;

/// The overall build register a vehicle is drawn in - the vehicle counterpart
/// of the humanoid [`StylizationTier`](super::body::StylizationTier). Sampled
/// first, then the continuous proportion knobs are banded by it so they
/// covary: a `Heavy` hull always arrives wide and tall-sided, never on a
/// racer's low narrow freeboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleStance {
    /// Short and tall-for-its-length - a stubby tug / runabout.
    Compact,
    /// Long, low and narrow - a racer / cutter.
    Sleek,
    /// Wide and tall-sided - a hauler / barge.
    Heavy,
}

impl VehicleStance {
    /// Weighted seeded pick: Sleek 40 % / Heavy 30 % / Compact 30 %.
    fn sample(rng: &mut impl RngCore) -> Self {
        let roll = range_f32(rng, 0.0, 1.0);
        if roll < 0.40 {
            Self::Sleek
        } else if roll < 0.70 {
            Self::Heavy
        } else {
            Self::Compact
        }
    }

    /// `(length multiplier, length:beam ratio, freeboard fraction)` for a boat
    /// of this stance (#1363).
    ///
    /// The stance moves the hull **inside** the brief's reference bands
    /// (length:beam 3.2-3.8, freeboard 0.10-0.12 L) rather than scaling it out
    /// of them, so a Heavy hull is the beamiest, tallest-sided boat that is
    /// still a boat and a Sleek one the narrowest, lowest. The old factors
    /// multiplied a 2.6:1 nominal by up to 1.16 on the beam, which is how
    /// every seed arrived as a turtle shell.
    fn boat_factors(self) -> (f32, f32, f32) {
        match self {
            // Short, beamy and tall-sided - a stubby working launch.
            Self::Compact => (0.93, 3.30, 0.118),
            // Long, narrow and low - a cutter.
            Self::Sleek => (1.07, 3.75, 0.102),
            // Wide and deep-sided, but not short - a hauler.
            Self::Heavy => (0.98, 3.25, 0.118),
        }
    }

    /// Gondola-size multiplier for this stance - a Heavy airship carries a
    /// roomier car, a Sleek one a trimmer pod.
    fn airship_gondola_factor(self) -> f32 {
        match self {
            Self::Compact => 0.95,
            Self::Sleek => 0.92,
            Self::Heavy => 1.10,
        }
    }

    /// `(length, radius)` multipliers the stance applies to the Lathe envelope
    /// profile (#791): a Sleek ship is long and slim, a Heavy one short and
    /// fat, a Compact one stubby. Centred so the population average stays near
    /// each form's hand-tuned nominal.
    fn airship_env_factors(self) -> (f32, f32) {
        match self {
            Self::Compact => (0.92, 1.02),
            Self::Sleek => (1.13, 0.9),
            Self::Heavy => (0.93, 1.14),
        }
    }

    /// `(length, width, height)` multipliers for a skiff of this stance
    /// (#1364).
    ///
    /// The stance moves the machine **inside** the brief's reference bands
    /// (wheelbase 0.62-0.66 L, wheel diameter 0.22-0.25 L, beltline about
    /// 0.30 L, overall height 0.36-0.42 L) rather than scaling it out of
    /// them, exactly as [`Self::boat_factors`] does for a hull. The old
    /// factors ran the length from 0.90 to 1.12 on top of an undamped body
    /// knob, which is a fleet spanning 1.7 m to 3.9 m - two different
    /// vehicles rather than one at two sizes.
    fn skiff_factors(self) -> (f32, f32, f32) {
        match self {
            // Short, narrow and tall-bodied - a stubby town runabout.
            Self::Compact => (0.94, 0.98, 1.05),
            // Long, low and lean - a racer.
            Self::Sleek => (1.06, 0.96, 0.95),
            // Wide and tall-sided, and not short - a hauler.
            Self::Heavy => (0.99, 1.08, 1.03),
        }
    }
}

/// Concrete boat proportions in **true metres**, with the hull's design
/// waterline at the origin and the bow at `+Z` (#1363).
///
/// Every field is a real dimension of a real small boat, inside the bands the
/// redesign brief set from reference craft: length:beam 3.2-3.8, freeboard
/// 0.10-0.12 of the length, sheer rising about 0.06 L at the stem and 0.025 L
/// at the transom, draft about 0.10 L. [`VehicleStance`] moves a hull *inside*
/// those bands rather than outside them, which is the difference between a
/// seeded fleet of boats and a seeded fleet of shapes: the old blueprint's
/// 2.6:1 length:beam and 20 %-of-length freeboard made every hull a turtle
/// shell, whatever the stance said (#1359 diagnosis).
///
/// No mount landmarks any more. They belonged to a part catalogue that seated
/// a deck, a mast and a funnel on guessed fractions of the hull; the
/// redesigned families read every station off their own `HullProfile`, which
/// is derived from exactly these numbers, so a trim line or a mast step cannot
/// drift from the hull it sits on.
#[derive(Clone, Copy, Debug)]
pub struct BoatBlueprint {
    pub stance: VehicleStance,
    /// Overall hull length, stem to transom (m) - the nominal every other
    /// dimension here is a fraction of.
    pub hull_len: f32,
    /// Maximum beam, full width (m).
    pub beam: f32,
    /// Deck-edge height above the design waterline at the sheer's lowest
    /// point (m).
    pub freeboard: f32,
    /// How much higher the deck edge runs at the stem than at that lowest
    /// point (m) - the sheer's forward rise.
    pub sheer_bow: f32,
    /// The same at the transom (m), always the smaller of the two: a boat's
    /// sheer sweeps up hardest forward.
    pub sheer_stern: f32,
    /// Depth of the deepest point of the underbody - the keel - below the
    /// waterline (m). What the craft's hover height is derived from, since a
    /// hovering boat has to clear the ground by its own draft (#1361).
    pub draft: f32,
}

impl BoatBlueprint {
    fn derive(body: &AvatarBody, rng: &mut ChaCha8Rng) -> Self {
        let stance = VehicleStance::sample(rng);
        let (len_f, beam_ratio, fb_frac) = stance.boat_factors();
        // Overall size rides the body height knob, DAMPED (#1363). Owner
        // decision 1 of the redesign is boats of about 2.6-3.0 m, and the
        // body knob swings +-30 %: taken raw it drew boats from 2.1 m to
        // 3.7 m, and the big end of that is the half of the fleet whose rig
        // the air-draft cap has to cut down, because the cap is an absolute
        // height above the ground and a 3.7 m hull cannot carry a
        // proportional mast through a 2.86 m gateway. Damping the knob keeps
        // "a bigger person sails a bigger boat" true without making a quarter
        // of the fleet under-rigged.
        let size = 1.0 + (body.height_scale - 1.0) * BODY_SIZE_DAMPING;
        // The beam ratio rides shoulder width - a broad-shouldered avatar
        // sails a beamier boat - and everything is clamped back into the
        // brief's bands afterwards, so a clamp corner is still a boat.
        let hull_len = NOMINAL_HULL_LEN * size * len_f * range_f32(rng, 0.96, 1.04);
        let ratio = (beam_ratio / body.shoulder_width_scale).clamp(3.2, 3.8);
        let freeboard = hull_len * (fb_frac * range_f32(rng, 0.97, 1.03)).clamp(0.10, 0.12);
        Self {
            stance,
            hull_len,
            beam: hull_len / ratio,
            freeboard,
            sheer_bow: hull_len * 0.060 * range_f32(rng, 0.92, 1.08),
            sheer_stern: hull_len * 0.025 * range_f32(rng, 0.92, 1.08),
            draft: hull_len * 0.100 * range_f32(rng, 0.95, 1.05),
        }
    }
}

/// Overall hull length (m) a nominal seeded boat is drawn at: airship class,
/// per owner decision 1 of the redesign (boats about 2.8 m against the
/// airship's 3.15 m and a 1.7 m person). Still a scale model of a bigger craft
/// - lit ports, no pilot - like the airship's 0.9 m gondola.
///
/// Since #1363 this is the length the parts are **authored** at as well, so
/// there is no scale bridge between the two any more: the boat assembler's
/// uniform root scale died with the legacy pipeline it was carrying.
pub const NOMINAL_HULL_LEN: f32 = 2.8;

/// How much of the body's +-30 % height knob a hull's or a machine's overall
/// length takes (#1363, #1364).
///
/// It exists for the BOAT, where it is load-bearing: the air-draft cap is an
/// absolute height above the ground, so an undamped fleet ran 2.12-3.74 m and
/// a quarter of it was visibly under-rigged. The skiff has no such cap, and
/// **measured over 40 000 skiff seeds the damping is a small effect there**:
/// 2.26-3.08 m damped against 2.09-3.28 undamped, with 58 % against 52 % of
/// the fleet inside owner decision 1's 2.5-2.8 m. Most of the skiff's spread
/// was taken out by narrowing its stance factors instead
/// ([`VehicleStance::skiff_factors`]). It is shared rather than split because
/// both families want the same thing of it and neither wants a different
/// number; if the owner would rather a bigger person drove a visibly bigger
/// car, the skiff can take the knob raw for the cost of that 6 %.
const BODY_SIZE_DAMPING: f32 = 0.45;

/// Airship proportions. Each envelope **form** is a seeded Lathe body of
/// revolution whose length + girth the `len_mult` / `radius_mult` here perturb
/// (#791); its mount *landmarks* (belly line, tail station, fin ring radius,
/// pod line) are read straight off that same profile by the assembler (see the
/// vehicle assembler's `airship_mounts` + `airship_profile`) - so a fat blimp
/// and a slim zeppelin each seat their slung gondola / cruciform fins / engine
/// pods on *their own* body, and they stay seated as the profile stretches (the
/// envelope-invariant-anchor bug that floated the twin's rigging clear of its
/// belly is gone by construction).
#[derive(Clone, Copy, Debug)]
pub struct AirshipBlueprint {
    /// Overall build register - read by the locomotion tuning (#794); today it
    /// biases the gondola size.
    pub stance: VehicleStance,
    /// Gondola size multiplier.
    pub gondola_scale: f32,
    /// Lathe-envelope length multiplier (#791) - scales each form's profile
    /// length so the population spans a continuum of silhouettes, not a few
    /// fixed sizes. The assembler scales the belly / tail / fin / pod mounts by
    /// the same factors so the slung parts stay seated.
    pub len_mult: f32,
    /// Lathe-envelope girth (max-radius) multiplier (#791).
    pub radius_mult: f32,
}

impl AirshipBlueprint {
    fn derive(body: &AvatarBody, rng: &mut ChaCha8Rng) -> Self {
        let stance = VehicleStance::sample(rng);
        let gondola_scale = (body.height_scale * body.head_scale * stance.airship_gondola_factor())
            .clamp(0.85, 1.2);
        // Envelope size rides the body height (length) + shoulder-width (girth)
        // knobs and the stance, with a small per-seed jitter - the #791
        // continuum. Clamped so a Lathe profile never degenerates.
        let (len_f, rad_f) = stance.airship_env_factors();
        let len_mult = (body.height_scale * len_f * range_f32(rng, 0.95, 1.06)).clamp(0.85, 1.28);
        let radius_mult =
            (body.shoulder_width_scale * rad_f * range_f32(rng, 0.95, 1.05)).clamp(0.85, 1.2);
        Self {
            stance,
            gondola_scale,
            len_mult,
            radius_mult,
        }
    }
}

/// Concrete skiff proportions in **true metres**, with the bow at `+Z`
/// (#1364).
///
/// Every field is a real dimension of a real small car, inside the bands the
/// redesign brief set from reference machines: wheelbase 0.62-0.66 of the
/// length, wheel diameter 0.22-0.25 L, half-track about 0.21 L, beltline about
/// 0.30 L above the ground and an overall height of 0.36-0.42 L.
/// [`VehicleStance`] moves a machine *inside* those bands rather than outside
/// them.
///
/// No mount landmarks any more, and no `ride_y`. They belonged to a part
/// catalogue that seated a canopy at a fixed height only one of four chassis
/// ever reached; the redesigned family reads every station off its own
/// `BodyPlan`, which is
/// derived from exactly these numbers - including the axle line, which is now
/// simply the wheel radius above the ground and so cannot disagree with the
/// wheels standing on it.
#[derive(Clone, Copy, Debug)]
pub struct SkiffBlueprint {
    pub stance: VehicleStance,
    /// Overall length, bumper to tail (m) - the nominal every other dimension
    /// here is a fraction of.
    pub length: f32,
    /// Maximum **bodywork** width (m). Not the track: coachwork of this kind
    /// is far narrower than the wheels it stands between, which is most of
    /// what makes a machine read as a machine rather than as a slab.
    pub body_w: f32,
    /// Front to rear wheel-centre distance (m).
    pub wheelbase: f32,
    /// Wheel-centre to wheel-centre across the machine (m).
    pub track: f32,
    /// Wheel outer radius, tyre tread (m).
    pub wheel_r: f32,
    /// The bodywork's crown above the ground (m) - the beltline.
    pub beltline: f32,
    /// Overall height above the ground (m), screen included.
    pub height: f32,
}

impl SkiffBlueprint {
    fn derive(body: &AvatarBody, rng: &mut ChaCha8Rng) -> Self {
        let stance = VehicleStance::sample(rng);
        let (len_f, width_f, height_f) = stance.skiff_factors();
        // Overall size rides the body height knob, DAMPED by the same factor
        // a boat's length is (#1363). A skiff has no air-draft cap to answer
        // to, so the reason is owner decision 1 alone - "skiffs about 2.5-2.8
        // m" - and the knob swings +-30 %: taken raw it drew machines from
        // 1.7 m to 3.9 m, which is a scooter and a lorry rather than one car
        // at two sizes. Damped, the fleet sits where the decision put it and
        // "a bigger person drives a bigger car" stays true.
        let size = 1.0 + (body.height_scale - 1.0) * BODY_SIZE_DAMPING;
        let length = NOMINAL_BODY_LEN * size * len_f * range_f32(rng, 0.97, 1.03);
        // Width rides shoulder width and the wheels ride limb thickness, both
        // clamped back into the brief's bands afterwards, so a clamp corner is
        // still a car.
        let body_frac =
            (0.272 * width_f * body.shoulder_width_scale.clamp(0.92, 1.10)).clamp(0.250, 0.300);
        let track_frac = (0.420 * width_f).clamp(0.390, 0.455);
        let wheel_frac = (0.115 * body.limb_thickness_scale.clamp(0.92, 1.08)).clamp(0.110, 0.125);
        Self {
            stance,
            length,
            body_w: length * body_frac,
            wheelbase: length * (0.640 * range_f32(rng, 0.98, 1.02)).clamp(0.62, 0.66),
            track: length * track_frac,
            wheel_r: length * wheel_frac,
            beltline: length * (0.300 * height_f).clamp(0.285, 0.315),
            height: length * (0.372 * height_f).clamp(0.360, 0.420),
        }
    }
}

/// Overall length (m) a nominal seeded skiff is drawn at: airship class, per
/// owner decision 1 of the redesign (skiffs about 2.65 m against the airship's
/// 3.15 m and a 1.7 m person). Still a scale model of a bigger machine - lit
/// lamps, no driver - like the airship's 0.9 m gondola.
///
/// Since #1364 this is the length the craft is **authored** at as well, so
/// there is no scale bridge between the two any more: the skiff assembler's
/// uniform root scale died with the legacy pipeline it was carrying, and the
/// skiff was the last family to hold one.
pub const NOMINAL_BODY_LEN: f32 = 2.65;

/// Per-family vehicle proportion blueprint. One variant per chassis that has
/// been wired to the shared-landmark system; [`VehicleBlueprint::from_seed`]
/// yields `None` for a chassis without one yet (and for the humanoid).
#[derive(Clone, Copy, Debug)]
pub enum VehicleBlueprint {
    Boat(BoatBlueprint),
    Airship(AirshipBlueprint),
    Skiff(SkiffBlueprint),
}

impl VehicleBlueprint {
    /// Derive the blueprint for a seed's chassis, or `None` if that chassis
    /// has no vehicle blueprint (humanoid, or a family not yet wired).
    pub fn from_seed(seed: u64) -> Option<Self> {
        Self::from_body(
            &AvatarBody::for_seed(seed),
            ChassisFamily::for_seed(seed),
            seed,
        )
    }

    /// Derive from an already-sampled [`AvatarBody`] + chassis. `seed` opens
    /// the blueprint's own salted jitter stream (kept distinct from the body
    /// deriver's stream so the two never entangle).
    pub fn from_body(body: &AvatarBody, chassis: ChassisFamily, seed: u64) -> Option<Self> {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ VEHICLE_BLUEPRINT_SALT);
        match chassis {
            ChassisFamily::Boat => Some(Self::Boat(BoatBlueprint::derive(body, &mut rng))),
            ChassisFamily::Airship => Some(Self::Airship(AirshipBlueprint::derive(body, &mut rng))),
            ChassisFamily::Skiff => Some(Self::Skiff(SkiffBlueprint::derive(body, &mut rng))),
            // Rigged: no parts to size (#1060).
            ChassisFamily::Humanoid => None,
        }
    }

    /// The boat blueprint, if this is a boat.
    pub fn boat(&self) -> Option<&BoatBlueprint> {
        match self {
            Self::Boat(b) => Some(b),
            _ => None,
        }
    }

    /// The airship blueprint, if this is an airship.
    pub fn airship(&self) -> Option<&AirshipBlueprint> {
        match self {
            Self::Airship(a) => Some(a),
            _ => None,
        }
    }

    /// The skiff blueprint, if this is a skiff.
    pub fn skiff(&self) -> Option<&SkiffBlueprint> {
        match self {
            Self::Skiff(s) => Some(s),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = VehicleBlueprint::from_seed(4242);
        let b = VehicleBlueprint::from_seed(4242);
        assert_eq!(
            a.and_then(|x| x.boat().map(|b| (b.hull_len, b.beam))),
            b.and_then(|x| x.boat().map(|b| (b.hull_len, b.beam)))
        );
    }

    #[test]
    fn every_vehicle_gets_its_family_blueprint_and_humanoids_do_not() {
        for s in 0u64..200 {
            let bp = VehicleBlueprint::from_seed(s);
            match ChassisFamily::for_seed(s) {
                ChassisFamily::Boat => {
                    assert!(
                        bp.and_then(|b| b.boat().copied()).is_some(),
                        "seed {s} boat"
                    );
                }
                ChassisFamily::Airship => {
                    assert!(
                        bp.and_then(|b| b.airship().copied()).is_some(),
                        "seed {s} airship"
                    );
                }
                ChassisFamily::Skiff => {
                    assert!(
                        bp.and_then(|b| b.skiff().copied()).is_some(),
                        "seed {s} skiff"
                    );
                }
                ChassisFamily::Humanoid => {
                    assert!(bp.is_none(), "seed {s}: humanoid got a blueprint")
                }
            }
        }
    }

    #[test]
    fn boat_dims_stay_in_sane_range() {
        // Every boat seed must land inside a band that keeps the hull a
        // believable, sanitiser-safe size (no zero/exploded dimensions).
        let mut seen = 0;
        for s in 0u64..600 {
            let Some(b) = VehicleBlueprint::from_seed(s).and_then(|bp| bp.boat().copied()) else {
                continue;
            };
            // True metres since #1363, around a 2.8 m nominal. Owner decision
            // 1 of the redesign is "boats about 2.6-3.0 m", and the damped
            // body knob plus the stance land three quarters of the fleet in
            // 2.6-3.0 with the tails inside this: no seed is a dinghy and none
            // is a ship.
            assert!(
                (2.2..=3.4).contains(&b.hull_len),
                "seed {s} len {}",
                b.hull_len
            );
            // The brief's reference bands (#1363 item 7), which the stance
            // moves a hull INSIDE rather than out of. A clamp corner is still
            // a boat.
            // A hair of tolerance on the clamp boundaries: the ratio is
            // clamped and then divided back out of the beam, so a seed sitting
            // exactly on a bound comes back a float ulp outside it.
            const EPS: f32 = 1e-4;
            let ratio = b.hull_len / b.beam;
            assert!(
                (3.2 - EPS..=3.8 + EPS).contains(&ratio),
                "seed {s}: length:beam {ratio} is outside the band"
            );
            let fb = b.freeboard / b.hull_len;
            assert!(
                (0.10 - EPS..=0.12 + EPS).contains(&fb),
                "seed {s}: freeboard {fb} of the length is outside the band"
            );
            assert!(
                b.sheer_bow > b.sheer_stern && b.sheer_stern > 0.0,
                "seed {s}: a boat's sheer sweeps up hardest forward"
            );
            let draft = b.draft / b.hull_len;
            assert!(
                (0.09..=0.11).contains(&draft),
                "seed {s}: draft {draft} of the length is outside the band"
            );
            seen += 1;
        }
        assert!(seen > 20, "too few boats sampled: {seen}");
    }

    #[test]
    fn skiff_dims_stay_in_sane_range() {
        // Every skiff seed lands inside the #1364 brief's reference bands, in
        // true metres around a 2.65 m nominal. Measured over 40 000 seeds the
        // fleet runs 2.26-3.08 m with the median exactly on the nominal and
        // the 5th-95th at 2.39-2.94; this is that band with room for the tails
        // a smaller sample does not reach.
        let mut seen = 0;
        for s in 0u64..600 {
            let Some(b) = VehicleBlueprint::from_seed(s).and_then(|bp| bp.skiff().copied()) else {
                continue;
            };
            assert!(
                (2.1..=3.2).contains(&b.length),
                "seed {s} length {}",
                b.length
            );
            // A hair of tolerance on the clamp boundaries: each fraction is
            // clamped and then multiplied back out by the length, so a seed
            // sitting exactly on a bound comes back a float ulp outside it.
            const EPS: f32 = 1e-4;
            for (what, got, lo, hi) in [
                ("wheelbase", b.wheelbase / b.length, 0.62, 0.66),
                ("wheel diameter", 2.0 * b.wheel_r / b.length, 0.22, 0.25),
                ("half-track", 0.5 * b.track / b.length, 0.195, 0.2275),
                ("body width", b.body_w / b.length, 0.250, 0.300),
                ("beltline", b.beltline / b.length, 0.285, 0.315),
                ("height", b.height / b.length, 0.360, 0.420),
            ] {
                assert!(
                    (lo - EPS..=hi + EPS).contains(&got),
                    "seed {s}: {what} {got} of the length is outside {lo}..{hi}"
                );
            }
            // The coachwork is far narrower than the wheels it stands between,
            // which is most of what makes this read as a machine rather than a
            // slab - and what the collider has to answer to.
            assert!(
                b.body_w < b.track,
                "seed {s}: a {} m body on a {} m track is not a car of this kind",
                b.body_w,
                b.track
            );
            seen += 1;
        }
        assert!(seen > 20, "too few skiffs sampled: {seen}");
    }

    #[test]
    fn every_stance_is_reachable() {
        let mut seen = [false; 3];
        for s in 0u64..600 {
            if let Some(bp) = VehicleBlueprint::from_seed(s)
                && let Some(b) = bp.boat()
            {
                let i = match b.stance {
                    VehicleStance::Compact => 0,
                    VehicleStance::Sleek => 1,
                    VehicleStance::Heavy => 2,
                };
                seen[i] = true;
            }
        }
        assert_eq!(seen, [true; 3], "some boat stance never sampled");
    }
}
