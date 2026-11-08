//! The on-foot preset sized from the body it carries (#1802, owner decisions
//! D2 and D4; #1875 for winged bodies): [`fit_on_foot`].
//!
//! A person is sized by their stature, as seeded defaults have been since
//! #1060 - the rule moved here unchanged, to the bit, so a seeded person's
//! record is the one it always was. A four-legged beast is sized from its
//! own trunk, read off the engine's rig of its skeleton with no mesh built:
//! a capsule lying along the trunk from girdle to girdle, at a radius of half
//! the trunk's depth, and the run, the mass and the jump that body's size
//! gives it. A winged body is sized by its leading family's posture - a bat
//! or a glider as a beast is, a bird standing upright in a capsule as tall as
//! it is - and given the `flight` section its body flies by, its wingbeat
//! the engine's own.

use symbios_avatar::anim::driver::{Driver, Inputs, level_ground};
use symbios_avatar::anim::{Flight, FlightMode};
use symbios_avatar::plan::Zone;
use symbios_avatar::{Archetype, Kinship, Rig, Speed, WingedFamily, WingedParams, Wings};

use super::{FlightParams, HumanoidParams, LocomotionPreset, OnFootShape};
use crate::pds::avatar::{EngineAvatarRecord, RiggedKind};
use crate::pds::types::Fp;

/// The stature a body plan this build cannot read is sized at (m): the canon
/// figure the preset's own default collider was cut for, which is also the
/// person the engine draws for it.
const UNKNOWN_STATURE: f32 = 1.7;

/// The Froude number a beast runs at - Shift, the record's `walk_speed` - on
/// its own hip height (#1802, owner decision D4): the default person's own run,
/// the preset's 5.0 m/s on that body's 0.71 m leg, so the run key asks every
/// body for the same dimensionless speed and a cat and a horse each get
/// their own metres a second of it. A canter on four legs, between the
/// engine's trot-to-canter change at 2.5 and its gallop at 4.0. Pinned to
/// that derivation by `the_beasts_run_is_the_default_persons`. A winged body
/// runs at it too, on its own legs (#1875).
pub(crate) const RUN_FROUDE: f32 = 3.586;

/// How much each family can flap (#1875, research-11 section 8.4): a bird
/// fully, a bat about 0.7, a glider not at all. A cross flaps by its shares,
/// and one a glider leads not at all, as its surface is a glider's (the
/// owner's decision on #1859: a glider only glides, off a strong leap).
const FLAP_POWER: (f32, f32, f32) = (1.0, 0.7, 0.0);

/// The least a winged body that flaps pushes each beat by, as a share of a
/// full-power beat (#1877): a bat's power. The flap key pushes `beat_speed`
/// times the power, and a cross a glider shares half of flaps at 0.35 to
/// 0.51 - pushed by that alone, it beat along at its stall and came down,
/// spending stamina: a bird-led one climbing -0.02 m/s at 1.3 against a
/// 3.2 m/s stall, a bat-led one 0.2 at 1.8 against 2.0. So a weaker flapper's
/// `beat_speed` is raised to push as a bat's power does, and its power still
/// spends its stamina sooner: a bird, a bat and every cross at or over a
/// bat's power are fitted as they were.
const WEAKEST_PUSH: f32 = FLAP_POWER.1;

/// How much harder than a flapper of its stand a winged body that cannot
/// flap takes off (#1877, the owner's decision on #1859: "the glider
/// glide-only with a strong leap"), on top of the prototype's own glider's
/// leap (`flight::GLIDER_LEAP`). With a flapper's take-off a glider leapt
/// 0.35 m from level ground and was down 0.61 s later, never gliding; twice
/// it, it leaps about 1.35 m and glides about 3.7 m.
const GLIDER_TAKEOFF: f32 = 2.0;

/// What `body` stands on when it goes on foot: `base` with the collider and
/// everything sized by the body refitted, and every other tuning - the
/// dampings, the controller's rates, the water speeds - kept.
///
/// * **A person** (and a body plan this build cannot read) gets the capsule
///   the stature rule gives - upright, a tenth of the stature and a little
///   more for its radius - and nothing else of `base` moves.
/// * **A four-legged beast** gets a capsule lying along its trunk
///   ([`OnFootShape::Lying`]): its length the span from the hind girdle to
///   the fore, its radius half the trunk's depth between them, its middle
///   the girdles' midpoint, and the shape's height the top of the trunk
///   over the feet, which its water depths are shares of. The run is
///   `RUN_FROUDE` on its hip height; the mass is the default person's mass for
///   the default person's capsule, scaled by the volume of this one; the
///   jump impulse keeps the default person's change of speed on that mass
///   as the record's clamps hold it, so every body jumps as high as they
///   let it. Each is
///   quantised onto the wire's grid as it is fitted, so the record a peer
///   decodes is the one this client stands on. A beast whose skeleton does
///   not rig keeps `base`.
/// * **A winged body** (#1875) is fitted by its leading family's posture
///   (a cross's is its leading family's, [`Kinship::leading`]). One a bat
///   or a glider leads goes on four legs and is fitted as a beast is. One a
///   bird leads stands upright in a capsule of its own: as tall as the bird
///   stands to the top of its head, its radius half its trunk's width, and
///   the run, the mass and the jump sized as a beast's are. Not by the
///   stature rule, whose clamps would stand a 0.3 m bird in a 0.76 m
///   capsule; the record's own floors hold it instead, so a body under about
///   0.2 m is the one thing taller than the capsule it stands in. Either way
///   it is given the `flight` section its body flies by ([`winged_flight`]),
///   keeping the handling of one `base` already carries
///   ([`FlightParams::keep_handling`]). A winged body whose skeleton does not
///   rig keeps `base`.
///
/// Sized from the rig the engine roots its skeleton in, without a mesh:
/// meshing a body to measure one costs about a second, and this runs on
/// every edit of the body (D3).
pub fn fit_on_foot(body: &EngineAvatarRecord, base: &HumanoidParams) -> HumanoidParams {
    match &body.archetype {
        Archetype::Humanoid(params) => person(params.height, base),
        Archetype::Quadruped(_) => beast(body, base).unwrap_or_else(|| base.clone()),
        Archetype::Winged(params) => winged(body, params, base).unwrap_or_else(|| base.clone()),
        Archetype::Unknown { .. } => person(UNKNOWN_STATURE, base),
    }
}

/// The stature rule (#1060), as `default_visuals::humanoid_locomotion` wrote
/// it: a person is roughly a tenth of their height across the shoulders, and
/// the clamp keeps a rolled extreme inside collider sanity.
fn person(stature: f32, base: &HumanoidParams) -> HumanoidParams {
    let mut p = base.clone();
    p.capsule_radius = Fp((stature * 0.11).clamp(0.18, 0.34));
    p.capsule_length = Fp((stature - 2.0 * p.capsule_radius.0).max(0.4));
    p.shape = OnFootShape::Upright;
    p
}

/// A beast's fit, or `None` for a skeleton the engine cannot rig or one with
/// no girdle for a fore or a hind leg.
fn beast(body: &EngineAvatarRecord, base: &HumanoidParams) -> Option<HumanoidParams> {
    lying(&Rig::from_skeleton(&body.skeleton()).ok()?, base)
}

/// The capsule lying along `rig`'s trunk, and what its size gives it: a
/// beast's fit, and a winged body's a bat or a glider leads.
fn lying(rig: &Rig, base: &HumanoidParams) -> Option<HumanoidParams> {
    let trunk = Trunk::of(rig)?;
    let mut p = base.clone();
    p.capsule_radius = Fp(trunk.radius).on_wire();
    p.capsule_length = Fp(trunk.span).on_wire();
    p.shape = OnFootShape::Lying {
        height: Fp(trunk.top),
        ahead: Fp(trunk.middle),
    };
    sized(&mut p, rig, trunk.radius, trunk.span);
    Some(p)
}

/// `p`'s run, mass and jump sized by the body on `rig`, whose collider is a
/// capsule of `radius` and `length`: the mass is the default person's for
/// the default person's capsule, scaled by this one's volume; the run
/// [`RUN_FROUDE`] on its legs; the jump the default person's change of speed
/// on the mass as the record holds it.
fn sized(p: &mut HumanoidParams, rig: &Rig, radius: f32, length: f32) {
    let person = HumanoidParams::default();
    let mass = person.mass.0 * capsule_volume(radius, length)
        / capsule_volume(person.capsule_radius.0, person.capsule_length.0);
    p.mass = Fp(mass).on_wire();
    p.walk_speed = Fp(Speed::from_froude(RUN_FROUDE).metres_per_second(rig)).on_wire();
    // Clamped as the record is, and the shape quantised with it, so the fit
    // round-trips the wire unchanged.
    p.sanitize();
    // The jump from the mass as the record holds it: a body lighter than
    // its floor weighs the floor, and one cut from the mass it would have
    // had left the ground at a fraction of the speed. Clamped in its turn,
    // which on the heaviest bodies is the most the record allows.
    p.jump_impulse = Fp(p.mass.0 * person.jump_impulse.0 / person.mass.0).on_wire();
    p.sanitize();
}

/// A winged body's fit (#1875), or `None` for a skeleton the engine cannot
/// rig: by its leading family's posture, with the flight section its body
/// flies by - the numbers its body sizes refitted, and the handling of a
/// section `base` already carries kept ([`FlightParams::keep_handling`]).
fn winged(
    body: &EngineAvatarRecord,
    params: &WingedParams,
    base: &HumanoidParams,
) -> Option<HumanoidParams> {
    let rig = Rig::from_skeleton(&body.skeleton()).ok()?;
    let kinship = Kinship::of(params);
    let mut p = match kinship.leading {
        WingedFamily::Bird => upright(&rig, base)?,
        WingedFamily::Bat | WingedFamily::Glider => lying(&rig, base)?,
    };
    let mut flight = winged_flight(&rig, kinship, p.stand_height());
    if let Some(owned) = &base.flight {
        flight.keep_handling(owned);
    }
    p.flight = Some(flight);
    p.sanitize();
    Some(p)
}

/// The upright capsule a bird-led body stands in (#1875), and what its size
/// gives it: as tall as the body stands, to the top of its head, and as wide
/// as its trunk. `None` for a rig with no trunk.
fn upright(rig: &Rig, base: &HumanoidParams) -> Option<HumanoidParams> {
    let is_trunk = |zone: Zone| matches!(zone, Zone::Pelvis | Zone::Abdomen | Zone::Chest);
    let drawn = rig.joints.iter().filter(|joint| !joint.marker);
    // A joint's reach up is its radius times its section's `y`, as a beast's
    // trunk depth is read ([`Trunk::of`]); across, its section's `x`.
    let top = drawn
        .clone()
        .map(|joint| joint.position.y + joint.radius * joint.scale.y)
        .fold(f32::MIN, f32::max);
    let half_width = drawn
        .filter(|joint| is_trunk(joint.zone))
        .map(|joint| joint.position.x.abs() + joint.radius * joint.scale.x)
        .fold(f32::MIN, f32::max);
    if !(top > 0.0 && half_width > 0.0) {
        return None;
    }
    let radius = half_width.min(top * 0.5);
    let length = top - 2.0 * radius;
    let mut p = base.clone();
    p.capsule_radius = Fp(radius).on_wire();
    p.capsule_length = Fp(length).on_wire();
    p.shape = OnFootShape::Upright;
    sized(&mut p, rig, radius, length);
    Some(p)
}

/// The flight section a winged body on `rig` flies by (#1875), standing
/// `stand_height` m tall: [`FlightParams::for_height`]'s speeds, its flap
/// power by its families ([`FLAP_POWER`]), and its wingbeat the engine's own
/// ([`engine_wingbeat`]). The two wingbeat laws differ - `for_height` keys
/// the beat on how tall the collider stands, the engine on the trunk's
/// length - and left unfitted the flap key's beat ran about 1.7 times the
/// one the wings are drawn at.
///
/// **A beat's push keeps the push a second `for_height` gives** (#1877): the
/// flap key beats `beat_speed` at `wingbeat_rate`, so a slower beat pushes
/// that much harder each time. Set to the engine's beat alone, a bat's
/// (2.6 a second against the 8.6 `for_height` gives a body standing 0.21 m)
/// pushed a third as hard: its take-off fell back before the stall speed
/// and it flapped along the ground, never flying. Kept, a bat climbs about
/// 1.7 m/s and glides at its trim; an owl 1.05 against 0.78, near the
/// stand-in's metre a second the prototype was accepted at.
///
/// **A weak flapper pushes as a bat's power does** ([`WEAKEST_PUSH`]), and
/// **a body that cannot flap leaps** ([`GLIDER_TAKEOFF`]).
pub(crate) fn winged_flight(rig: &Rig, kinship: Kinship, stand_height: f32) -> FlightParams {
    let mut flight = FlightParams::for_height(stand_height);
    flight.flight_power = Fp(if kinship.flaps() {
        kinship.mix(FLAP_POWER.0, FLAP_POWER.1, FLAP_POWER.2)
    } else {
        0.0
    });
    if let Some(rate) = engine_wingbeat(rig, kinship).filter(|rate| *rate > 0.0) {
        flight.beat_speed = Fp(flight.beat_speed.0 * flight.wingbeat_rate.0 / rate);
        flight.wingbeat_rate = Fp(rate);
    }
    if flight.can_flap() {
        flight.beat_speed =
            Fp(flight.beat_speed.0 * (WEAKEST_PUSH / flight.flight_power.0).max(1.0));
    } else {
        flight.takeoff_speed = Fp(flight.takeoff_speed.0 * GLIDER_TAKEOFF);
    }
    flight.sanitize();
    flight
}

/// The cruise beat a body on `rig` of `kinship` flaps at, beats a second, as
/// the engine draws it: one frame of its own driver told to flap, read back
/// off what it posed ([`symbios_avatar::Flown::rate`]). A rig of a skeleton
/// has no wings - they are grown with the mesh - so it is lent an empty set
/// carrying the kinship: the beat reads the families and the trunk, which
/// are the skeleton's, and nothing grown. About a tenth of a millisecond,
/// where building the body to fly it is about a quarter of a second.
fn engine_wingbeat(rig: &Rig, kinship: Kinship) -> Option<f32> {
    let mut rig = rig.clone();
    rig.wings = Some(Wings {
        kinship: Some(kinship),
        ..Wings::default()
    });
    let mut driver = Driver::seeded(0);
    let inputs = Inputs {
        delta: 1.0 / 60.0,
        flight: Some(Flight::new(FlightMode::Flap)),
        ..Default::default()
    };
    driver.drive(&rig, &inputs, level_ground);
    driver
        .flight(&rig)
        .map(|flown| flown.rate)
        .filter(|rate| rate.is_finite())
}

/// A beast's trunk, as its collider is cut from it (m).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Trunk {
    /// The span from the hind girdle to the fore, along the body.
    span: f32,
    /// Half the trunk's depth between them: its back's top to its belly.
    radius: f32,
    /// The top of the trunk over the feet.
    top: f32,
    /// The girdles' midpoint along the body, ahead of the rig's origin: the
    /// capsule's middle.
    middle: f32,
}

impl Trunk {
    /// Read off `rig`: the girdles are the trunk joints its legs hang from,
    /// and the depth is the reach of every trunk ring between them, up and
    /// down, through its own section (a ring's vertical reach is its radius
    /// times its section's `y`, [`symbios_avatar::rig::Joint::scale`]).
    fn of(rig: &Rig) -> Option<Self> {
        let is_trunk = |zone: Zone| matches!(zone, Zone::Pelvis | Zone::Abdomen | Zone::Chest);
        let girdle = |fore: bool| {
            rig.joints
                .iter()
                .find_map(|joint| match (joint.zone, joint.parent) {
                    (Zone::UpperLimb(limb), Some(parent))
                        if limb.is_fore() == fore && is_trunk(rig.joints[parent].zone) =>
                    {
                        Some(rig.joints[parent].position.z)
                    }
                    _ => None,
                })
        };
        let (hind, fore) = (girdle(false)?, girdle(true)?);
        let (back, front) = (hind.min(fore), hind.max(fore));
        let (mut top, mut bottom) = (f32::MIN, f32::MAX);
        for joint in &rig.joints {
            let z = joint.position.z;
            if is_trunk(joint.zone) && !joint.marker && (back..=front).contains(&z) {
                let reach = joint.radius * joint.scale.y;
                top = top.max(joint.position.y + reach);
                bottom = bottom.min(joint.position.y - reach);
            }
        }
        (top > bottom).then_some(Self {
            span: front - back,
            radius: (top - bottom) * 0.5,
            top,
            middle: (back + front) * 0.5,
        })
    }
}

/// A beast's lying capsule in its rig's frame (#1833): the collider
/// [`fit_on_foot`] cuts from this same rig, its axis level along the body's
/// `z` from girdle to girdle at its own radius over the rig's floor, so its
/// underside is the floor at `y = 0`.
///
/// What a body with no collider of its own is asked about as though it had
/// one - a peer, whose chassis is played out rather than simulated, and the
/// render tool's walker: what that capsule would rest on, and where.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LyingCapsule {
    /// The capsule's radius, and its axis' height over the floor.
    pub(crate) radius: f32,
    /// Where its axis starts and ends along the rig's `z`: the girdles.
    pub(crate) from: f32,
    pub(crate) to: f32,
}

impl LyingCapsule {
    /// `rig`'s, or `None` for a rig with no trunk to lie along.
    pub(crate) fn of(rig: &Rig) -> Option<Self> {
        let trunk = Trunk::of(rig)?;
        let half = trunk.span * 0.5;
        Some(Self {
            radius: trunk.radius,
            from: trunk.middle - half,
            to: trunk.middle + half,
        })
    }

    /// The capsule as a collider in the rig's frame, its underside on the
    /// floor.
    pub(crate) fn collider(&self) -> avian3d::prelude::Collider {
        let axis = |z: f32| bevy::math::Vec3::new(0.0, self.radius, z);
        avian3d::prelude::Collider::capsule_endpoints(self.radius, axis(self.from), axis(self.to))
    }
}

/// A capsule's volume (m³): its cylinder and the sphere its two ends make.
fn capsule_volume(radius: f32, length: f32) -> f32 {
    std::f32::consts::PI * radius * radius * (length + radius * 4.0 / 3.0)
}

/// Whether the on-foot preset `p` carries a four-legged beast under `body`
/// (#1803, #1804): a four-legged body always, a person never, a winged body
/// never - a bat or a glider goes on four legs and lies on a beast's capsule,
/// but is framed as a winged body (#1875), told a winged body's words (#1878,
/// `crate::player::emote::WingedEmote`) and dressed at a winged body's
/// sockets (#1879, `crate::pds::avatar::WINGED_SOCKETS`) - and otherwise - a
/// body this build cannot read, or none resolved yet - whether the preset
/// lies. The BODY's kind, not the preset's
/// shape: a person under a lying preset someone else saved is a person.
pub(crate) fn carries_beast(body: Option<&EngineAvatarRecord>, p: &HumanoidParams) -> bool {
    match body.map(RiggedKind::of_record) {
        Some(RiggedKind::Beast) => true,
        Some(RiggedKind::Person | RiggedKind::Winged(_)) => false,
        Some(RiggedKind::Unknown) | None => p.is_lying(),
    }
}

/// The body `record` wears, when it is a rigged one and resolved.
fn worn_body(record: &crate::pds::AvatarRecord) -> Option<&EngineAvatarRecord> {
    record
        .body
        .rigged_ref()
        .and_then(|rig| rig.resolved.as_ref())
        .map(|resolved| &resolved.body)
}

/// The on-foot preset of `record` when it carries a four-legged beast
/// ([`carries_beast`], over the body the record resolves), and `None` for a
/// person, a winged body, a vehicle and a preset this build cannot read
/// (#1804): what the camera asks before it frames a body as a beast's.
pub(crate) fn beast_on_foot(record: &crate::pds::AvatarRecord) -> Option<&HumanoidParams> {
    let super::LocomotionConfig::Humanoid(p) = &record.locomotion else {
        return None;
    };
    carries_beast(worn_body(record), p).then_some(&**p)
}

/// The on-foot preset of `record` when the body it resolves is a winged one
/// (#1875), and `None` for any other body, a vehicle and a body not resolved
/// yet: what the agent's status and the chat's gesture hint ask.
pub(crate) fn winged_on_foot(record: &crate::pds::AvatarRecord) -> Option<&HumanoidParams> {
    let super::LocomotionConfig::Humanoid(p) = &record.locomotion else {
        return None;
    };
    worn_body(record)
        .is_some_and(|body| RiggedKind::of_record(body).is_winged())
        .then_some(&**p)
}

/// Stand an on-foot record back on the shape its body takes, on its owner's
/// client (#1802, #1803): the owner's defence against another client
/// writing the preset and the body apart.
///
/// * **A four-legged beast whose preset is not lying** - a client older
///   than the shape reads a lying record, drops the key it does not know
///   and writes it back upright, leaving the beast hung from a person's
///   upright capsule as tall as its trunk is long, 2.28 m on a horse - is
///   refitted to its body ([`fit_on_foot`]).
/// * **A person whose preset lies** - a wardrobe entry re-saved as a person
///   by another app or the standalone viewer, under a record still carrying
///   the beast's collider - is stood up by [`follow`]'s person rule: the
///   stature rule's capsule, and the default person's mass, jump and run,
///   which the beast's fit had replaced. A flight section, which a person
///   may wear (#1861), is kept.
/// * **A winged body** (#1875) whose preset lies or stands against its
///   leading family's posture - a client older than the shape dropped a
///   bat's - is refitted to its body. One whose collider is its posture's
///   but whose flight section is gone - a client older than the section
///   dropped the key it did not know (#1861) - gets back the section its body
///   flies by ([`fit_on_foot`]'s), and nothing else of its preset moves. A
///   winged body's kind can be read off its record, so its section can be
///   given back; a generator body's visuals are no sound witness of wings,
///   and nothing gives one a section it was not given.
///
/// Says whether it refitted. A person standing, a beast already lying, a
/// winged body standing as its posture does with its section, a body this
/// build cannot read, a vehicle and a body not resolved yet are left as they
/// are. Only the owner's client runs it, where its own record is installed:
/// a peer's record is drawn as it was published. Every fit is deterministic,
/// so a record refitted twice is refitted to the same bytes.
pub(crate) fn restore_body_shape(record: &mut crate::pds::AvatarRecord) -> bool {
    let Some(body) = worn_body(record) else {
        return false;
    };
    let super::LocomotionConfig::Humanoid(p) = &record.locomotion else {
        return false;
    };
    let fit = match body.archetype {
        Archetype::Quadruped(_) if !p.is_lying() => {
            Some(fit_on_foot(body, p)).filter(HumanoidParams::is_lying)
        }
        Archetype::Humanoid(_) if p.is_lying() => follow(body, None, p, &mut None),
        Archetype::Winged(_) => {
            let on_four = RiggedKind::of_record(body).on_four();
            let fit = fit_on_foot(body, p);
            if p.is_lying() != on_four {
                Some(fit).filter(|fit| fit.is_lying() == on_four)
            } else if !p.flies() {
                fit.flight.map(|flight| HumanoidParams {
                    flight: Some(flight),
                    ..(**p).clone()
                })
            } else {
                None
            }
        }
        _ => None,
    };
    let Some(fit) = fit else {
        return false;
    };
    record.locomotion = fit.into_config();
    true
}

/// A tuned beast's run as it is carried across a run of edits of its body
/// (#1803): the run its owner left, the run the body they left it on
/// derives, and the run the carry last wrote.
///
/// The editor holds one across frames, so a drag of a body slider scales
/// the owner's run from where they left it on every frame rather than from
/// the frame before. Scaling from the frame before compounded one rounding
/// onto the wire's grid per frame: thirty steps out and thirty back on a
/// beast's height returned its tuned run 8.486 m/s as 8.4859. From the
/// anchor the run is a function of the body alone, so a body brought back
/// derives the anchor's own run, bit for bit, and gets the owner's back.
/// It is held only while the record still runs at what the carry wrote:
/// a run the owner set since, or an undo, starts a new one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RunAnchor {
    /// The run the owner left.
    run: Fp,
    /// What the body they left it on derives.
    derived: Fp,
    /// The run the carry last wrote.
    written: Fp,
}

impl RunAnchor {
    /// The anchor a record running at `run` is carried from: `held` while
    /// the record still runs at what it wrote, else a new one from the body
    /// before the edit, which derives `was` - `None` when that body does
    /// not rig, and so says nothing about whether `run` was tuned.
    fn read(held: Option<Self>, run: Fp, was: Option<Fp>) -> Option<Self> {
        held.filter(|anchor| anchor.written == run).or_else(|| {
            was.map(|derived| Self {
                run,
                derived,
                written: run,
            })
        })
    }

    /// The run a record running at `run` gets on a body that derives `now`,
    /// carried from `held`; with no anchor to carry it from - the body
    /// before the edit did not rig - the owner's run as it is, since
    /// nothing says it was the old body's own rather than tuned.
    fn run_on(held: Option<Self>, run: Fp, now: Fp) -> Fp {
        held.map_or(run, |held| held.carry(now))
    }

    /// The run on a body that derives `now`: the body's own when the owner
    /// never moved theirs off what their body derived, theirs exactly on a
    /// body that derives what theirs did, and theirs scaled by the change
    /// otherwise.
    fn carry(self, now: Fp) -> Fp {
        if self.run == self.derived {
            now
        } else if now == self.derived || self.derived.0 <= 0.0 {
            self.run
        } else {
            Fp(self.run.0 * now.0 / self.derived.0).on_wire()
        }
    }
}

/// The on-foot preset `p` refitted to the body it carries after an edit of
/// that body (#1803, owner decision D3), or `None` when nothing it holds
/// would change. `before` is the body as it was before the edit, `None` when
/// there was none to compare with; `anchor` is what a tuned beast's run is
/// carried from ([`RunAnchor`]), held by the caller across a run of edits.
///
/// * **A beast** gets [`fit_on_foot`]: its collider, its mass and its jump
///   follow the body on every edit. The run follows it too, but as the
///   owner left it: a run the owner never moved off what their body
///   derived is the new body's own, and one they did move keeps its
///   distance from it - their run scaled by the new body's derived run over
///   the one they left it on, so a tuned dog that grows keeps the same
///   canter on longer legs. A run left on a body that does not rig is kept
///   as it is, since nothing says whether it was tuned. A body becoming a
///   beast takes the beast's own run (D4), whatever the person ran at.
/// * **A winged body** (#1875) is followed as a beast is - its collider by
///   its leading family's posture, its mass, its jump and the flight numbers
///   its body sizes from the body on every edit, and its run as its owner
///   left it - across a change of family too, a bird made a bat. The flight
///   section's handling - stamina, recovery, glide ratio, bank, roll and
///   pitch - is its owner's, kept through every edit
///   ([`FlightParams::keep_handling`]). A body becoming winged takes its own
///   run, and its own flight but for the handling of a section it already
///   carried.
/// * **A person whose capsule lies** - a beast that became a person, or a
///   fresh body worn over a beast's preset - stands up on the stature rule,
///   and takes the default person's mass, jump and run: the fit keeps its
///   base's, which were the beast's. **So does a person who was a winged
///   body before the edit** (#1875), section or not: a bird stands in a
///   capsule of its own, which the stature rule replaces. Told by `before`'s
///   kind, never by the preset's section - a person or a beast may fly
///   (#1861), and keeps its section through every edit of its body - and only
///   a winged body's section, sized for its wings, is dropped: a winged body
///   made a person or a beast flies no more.
/// * **A person standing up** is never refitted: their capsule is
///   whatever its owner set, and an edit of their body moves none of it,
///   their flight section included.
/// * **A body this build cannot read** is never refitted either, lying or
///   not: its preset is a newer client's, preserved as written.
///
/// A beast or a winged body whose skeleton does not rig keeps what it has,
/// and the anchor with it.
pub(crate) fn follow(
    body: &EngineAvatarRecord,
    before: Option<&EngineAvatarRecord>,
    p: &HumanoidParams,
    anchor: &mut Option<RunAnchor>,
) -> Option<HumanoidParams> {
    // Whether the preset was a winged body's: told by the body before the
    // edit, never by the preset's section, which any on-foot body may wear
    // (#1861) - a person or a beast that flies was never winged.
    let was_winged = before.is_some_and(|before| matches!(before.archetype, Archetype::Winged(_)));
    let fit = match &body.archetype {
        Archetype::Quadruped(_) | Archetype::Winged(_) => {
            let derive = |body: &EngineAvatarRecord| match &body.archetype {
                Archetype::Quadruped(_) => beast(body, p).map(|mut fit| {
                    // A winged body's section was sized for its wings: the
                    // beast it became does not fly by it.
                    if was_winged {
                        fit.flight = None;
                    }
                    fit
                }),
                Archetype::Winged(params) => winged(body, params, p),
                _ => None,
            };
            let mut fit = derive(body)?;
            // The body before the edit, when the preset was fitted to its
            // kind: a lying beast's, a flying winged body's.
            let was = before.filter(|before| match (&body.archetype, &before.archetype) {
                (Archetype::Quadruped(_), Archetype::Quadruped(_)) => p.is_lying(),
                (Archetype::Winged(_), Archetype::Winged(_)) => p.flies(),
                _ => false,
            });
            *anchor = match was {
                Some(before) => {
                    let was = derive(before).map(|fit| fit.walk_speed);
                    let held = RunAnchor::read(*anchor, p.walk_speed, was);
                    fit.walk_speed = RunAnchor::run_on(held, p.walk_speed, fit.walk_speed);
                    fit.sanitize();
                    held.map(|held| RunAnchor {
                        written: fit.walk_speed,
                        ..held
                    })
                }
                None => None,
            };
            fit
        }
        Archetype::Humanoid(_) if p.is_lying() || was_winged => {
            *anchor = None;
            let person = HumanoidParams::default();
            let base = HumanoidParams {
                mass: person.mass,
                jump_impulse: person.jump_impulse,
                walk_speed: person.walk_speed,
                flight: p.flight.clone().filter(|_| !was_winged),
                ..p.clone()
            };
            fit_on_foot(body, &base)
        }
        _ => {
            *anchor = None;
            return None;
        }
    };
    (fit != *p).then_some(fit)
}

/// [`follow`] over a whole record: refit its on-foot preset to the body it
/// resolves, and say whether anything moved. A vehicle preset, an
/// unresolved body and a generator body are left as they are - a vehicle is
/// not sized by who rides it.
///
/// The avatar editor calls it in the same frame as the body edit it
/// follows, before the change is flushed, so the edit and the refit land on
/// one change tick and one undo step restores both.
pub(crate) fn follow_body(
    record: &mut crate::pds::AvatarRecord,
    before: Option<&EngineAvatarRecord>,
    anchor: &mut Option<RunAnchor>,
) -> bool {
    let Some(body) = worn_body(record) else {
        return false;
    };
    let super::LocomotionConfig::Humanoid(p) = &record.locomotion else {
        return false;
    };
    let Some(fit) = follow(body, before, p, anchor) else {
        return false;
    };
    record.locomotion = fit.into_config();
    true
}

/// The default body of `family`, as the engine's family picker makes one -
/// the dog, the cat and the horse the tests fit.
#[cfg(test)]
pub(crate) fn default_beast(family: symbios_avatar::plan::Family) -> EngineAvatarRecord {
    let mut params = symbios_avatar::plan::QuadrupedParams::default();
    params.set_kind(family, None, 0.0);
    EngineAvatarRecord {
        archetype: Archetype::Quadruped(params),
        ..EngineAvatarRecord::default()
    }
}

/// A record wearing `family`'s default body on foot, its locomotion fitted
/// to it - what the editor's refit will leave (#1802).
#[cfg(test)]
pub(crate) fn beast_record(family: symbios_avatar::plan::Family) -> crate::pds::AvatarRecord {
    let mut record = crate::pds::AvatarRecord::default_for_did("did:plc:beast-bench");
    record.locomotion =
        fit_on_foot(&default_beast(family), &HumanoidParams::default()).into_config();
    record
}

/// The default body of `family`, as the engine's family picker makes one -
/// the owl, the fruit bat and the sugar glider (#1875).
#[cfg(test)]
pub(crate) fn default_winged(family: WingedFamily) -> EngineAvatarRecord {
    let mut params = WingedParams::default();
    params.set_kind(family, None, 0.0);
    EngineAvatarRecord {
        archetype: Archetype::Winged(params),
        ..EngineAvatarRecord::default()
    }
}

/// A record wearing `body`, resolved, on foot with its locomotion fitted to
/// it - what the editor's refit leaves on a winged body (#1875). Resolved,
/// because a winged body is told by its body: an unresolved one is told by
/// its preset's shape alone, and a bat's lies as a beast's does.
#[cfg(test)]
pub(crate) fn fitted_record(body: EngineAvatarRecord) -> crate::pds::AvatarRecord {
    let mut record = crate::pds::AvatarRecord::wearing("3jzfcijpj2z2a");
    record.locomotion = fit_on_foot(&body, &HumanoidParams::default()).into_config();
    if let Some(rig) = record.body.rigged_mut() {
        rig.resolved = Some(crate::pds::avatar::ResolvedRig {
            body,
            attachments: Vec::new(),
        });
    }
    record
}

#[cfg(test)]
mod tests {
    use super::*;
    use symbios_avatar::plan::Family;

    fn beast_of(family: Family) -> EngineAvatarRecord {
        default_beast(family)
    }

    /// **The camera asks the body's kind, not the preset's shape** (#1804):
    /// a person under a beast's lying preset is a person and a beast under
    /// an upright one a beast. Only a body not resolved yet goes by the shape,
    /// and a vehicle is framed as a vehicle whoever rides it.
    #[test]
    fn a_beast_on_foot_is_told_by_its_body() {
        let wearing = |body: Option<EngineAvatarRecord>, lying: bool| {
            let mut record = crate::pds::AvatarRecord::wearing("3jzfcijpj2z2a");
            if let Some(rig) = record.body.rigged_mut() {
                rig.resolved = body.map(|body| crate::pds::avatar::ResolvedRig {
                    body,
                    attachments: Vec::new(),
                });
            }
            if lying {
                record.locomotion = beast_record(Family::Dog).locomotion;
            } else {
                record.locomotion = HumanoidParams::default().into_config();
            }
            record
        };
        let person = || EngineAvatarRecord::default();
        let dog = || Some(default_beast(Family::Dog));
        assert!(beast_on_foot(&wearing(dog(), true)).is_some());
        assert!(beast_on_foot(&wearing(dog(), false)).is_some());
        assert!(beast_on_foot(&wearing(Some(person()), true)).is_none());
        assert!(beast_on_foot(&wearing(Some(person()), false)).is_none());
        assert!(beast_on_foot(&wearing(None, true)).is_some());
        assert!(beast_on_foot(&wearing(None, false)).is_none());
        let mut riding = wearing(dog(), true);
        riding.locomotion = crate::pds::LocomotionConfig::Car(Box::default());
        assert!(beast_on_foot(&riding).is_none());
    }

    /// **A beast's collider lies along its trunk, girdle to girdle** (#1802,
    /// D2): longer than it is tall; its length the span from the joint its
    /// hind legs hang from - the pelvis, the rig's root - to the one its
    /// neck does, the withers; its depth the trunk's, every trunk ring
    /// between them inside it from the top of the back down; and its head
    /// and its tail beyond its ends.
    #[test]
    fn a_beasts_collider_lies_along_its_trunk() {
        for family in [Family::Dog, Family::Cat, Family::Horse] {
            let body = beast_of(family);
            let fit = fit_on_foot(&body, &HumanoidParams::default());
            let rig = Rig::from_skeleton(&body.skeleton()).expect("a beast rigs");
            let OnFootShape::Lying { height, ahead } = fit.shape else {
                panic!("a {family:?} is not lying on its capsule: {:?}", fit.shape);
            };
            let (r, span) = (fit.capsule_radius.0, fit.capsule_length.0);
            let (long, tall) = (span + 2.0 * r, fit.total_height());
            assert!(
                long > tall * 1.5,
                "a {family:?}'s collider is {long:.3} m long and {tall:.3} m tall"
            );
            assert_eq!(tall, 2.0 * r, "a lying capsule is its diameter tall");
            assert_eq!(fit.stand_height(), height.0);
            let withers = rig
                .joints
                .iter()
                .find(|j| j.zone == Zone::Neck)
                .and_then(|j| j.parent)
                .map(|parent| rig.joints[parent].position.z)
                .expect("a neck");
            let pelvis = rig.joints[0].position.z;
            assert!(
                (span - (withers - pelvis)).abs() < 1e-4,
                "a {family:?}'s capsule is {span:.4} m between its girdles, which are {:.4} \
                 m apart",
                withers - pelvis
            );
            // Its middle is theirs, so its ends are at the girdles.
            assert!(
                (ahead.0 - (withers + pelvis) * 0.5).abs() < 1e-4,
                "a {family:?}'s capsule is centred {:.4} m ahead of its origin, its girdles \
                 {:.4}",
                ahead.0,
                (withers + pelvis) * 0.5
            );
            for joint in rig.joints.iter().filter(|j| {
                matches!(j.zone, Zone::Pelvis | Zone::Abdomen | Zone::Chest) && !j.marker
            }) {
                let reach = joint.radius * joint.scale.y;
                assert!(
                    joint.position.y + reach <= height.0 + 1e-4
                        && joint.position.y - reach >= height.0 - tall - 1e-4,
                    "a {family:?}'s trunk ring at {:?} reaches outside its {tall:.3} m depth",
                    joint.position
                );
            }
            // Its head and its tail reach beyond its ends.
            let reach = |zone: Zone, ahead: f32| {
                rig.joints
                    .iter()
                    .filter(|j| j.zone == zone)
                    .map(|j| (j.position.z + ahead * j.radius) * ahead)
                    .fold(f32::MIN, f32::max)
            };
            assert!(
                reach(Zone::Head, 1.0) > ahead.0 + long * 0.5,
                "a {family:?}'s head"
            );
            assert!(
                reach(Zone::Tail, -1.0) > -ahead.0 + long * 0.5,
                "a {family:?}'s tail"
            );
            // Every fitted value is on the wire's grid.
            let read_back: HumanoidParams =
                serde_json::from_value(serde_json::to_value(&fit).expect("writes")).expect("reads");
            assert_eq!(
                read_back, fit,
                "a {family:?}'s fit is not on the wire's grid"
            );
        }
    }

    /// **A person's fit is the stature rule, to the bit** (#1802): what the
    /// seeded records were sized by since #1060, over 2000 rolled people -
    /// upright, and every other field of the base untouched.
    #[test]
    fn a_persons_fit_is_the_stature_rule() {
        let base = HumanoidParams {
            walk_speed: Fp(4.25),
            mass: Fp(91.0),
            ..HumanoidParams::default()
        };
        for seed in 0u64..2000 {
            let body = crate::pds::avatar::wardrobe::engine_default_for_seed(seed);
            let Archetype::Humanoid(params) = &body.archetype else {
                panic!("seed {seed} rolled no person");
            };
            let stature = params.height;
            let mut want = base.clone();
            want.capsule_radius = Fp((stature * 0.11).clamp(0.18, 0.34));
            want.capsule_length = Fp((stature - 2.0 * want.capsule_radius.0).max(0.4));
            let fit = fit_on_foot(&body, &base);
            assert_eq!(
                (
                    fit.capsule_radius.0.to_bits(),
                    fit.capsule_length.0.to_bits()
                ),
                (
                    want.capsule_radius.0.to_bits(),
                    want.capsule_length.0.to_bits()
                ),
                "seed {seed}"
            );
            assert_eq!(fit, want, "seed {seed}");
        }
    }

    /// The beast's run is the default person's, as a Froude number (D4).
    #[test]
    fn the_beasts_run_is_the_default_persons() {
        let person = Rig::from_skeleton(&EngineAvatarRecord::default().skeleton()).expect("rigs");
        let run = Speed::new(&person, HumanoidParams::default().walk_speed.0).froude();
        assert!(
            (run - RUN_FROUDE).abs() < 1e-3,
            "the default person runs at Froude {run}, not {RUN_FROUDE}"
        );
    }

    /// `family`'s default body, shrunk or grown to the end of every size
    /// axis the engine has: the smallest beast it rolls, or the largest.
    fn extreme_beast(family: Family, largest: bool) -> EngineAvatarRecord {
        let mut body = beast_of(family);
        let Archetype::Quadruped(params) = &mut body.archetype else {
            unreachable!("a beast");
        };
        let way = if largest { 1.0 } else { -1.0 };
        params.height = if largest { 4.24 } else { 0.05 };
        params.body_length = 3.0 * way;
        params.leg_length = -3.0 * way;
        params.chest_depth = 3.0 * way;
        body
    }

    /// **The smallest and the largest beasts jump as fast as the record's
    /// clamps let them** (#1802, D4): the jump is cut from the mass as the
    /// record holds it, so a body lighter than the mass's floor, which
    /// weighs the floor, still leaves the ground at the default person's
    /// speed - cut from the mass it would have had, it left at next to
    /// nothing - and the heaviest leave at the most the impulse's ceiling
    /// gives their mass, never more.
    #[test]
    fn a_beasts_jump_keeps_its_speed_at_both_ends() {
        let person = HumanoidParams::default();
        let leaves = person.jump_impulse.0 / person.mass.0;
        for family in [Family::Dog, Family::Cat, Family::Horse] {
            let small = fit_on_foot(&extreme_beast(family, false), &person);
            assert!(small.is_lying(), "the smallest {family:?} lies");
            assert_eq!(small.mass.0, 0.1, "the premise: the mass's floor holds it");
            let off = small.jump_impulse.0 / small.mass.0 - leaves;
            assert!(
                off.abs() < 1e-3,
                "the smallest {family:?} leaves the ground {off:+} m/s off"
            );
            let large = fit_on_foot(&extreme_beast(family, true), &person);
            assert!(large.is_lying(), "the largest {family:?} lies");
            let most = leaves.min(50_000.0 / large.mass.0);
            let off = large.jump_impulse.0 / large.mass.0 - most;
            assert!(
                off.abs() < 1e-3,
                "the largest {family:?}, {:.0} kg, leaves the ground {off:+} m/s off the {most:.3} \
                 m/s its clamps allow",
                large.mass.0
            );
        }
    }

    /// Mass scales with the body and the jump keeps its change of speed (D4):
    /// a horse outweighs a dog, which outweighs a cat, and every one of them
    /// leaves the ground at the default person's speed.
    #[test]
    fn a_beasts_mass_scales_with_it_and_its_jump_keeps_its_speed() {
        let person = HumanoidParams::default();
        let fit = |family| fit_on_foot(&beast_of(family), &person);
        let (cat, dog, horse) = (fit(Family::Cat), fit(Family::Dog), fit(Family::Horse));
        assert!(cat.mass.0 < dog.mass.0 && dog.mass.0 < horse.mass.0);
        let leaves = person.jump_impulse.0 / person.mass.0;
        for body in [cat, dog, horse] {
            let off = body.jump_impulse.0 / body.mass.0 - leaves;
            assert!(
                off.abs() < 1e-3,
                "{body:?} leaves the ground {off:+} m/s off"
            );
        }
    }

    /// **A beast's tuned run is carried across an edit of its body, an
    /// untuned one is the new body's** (#1803, D3 and D4): every other
    /// fitted value is the new body's either way. A run the owner never
    /// moved is exactly what the new body derives; one they set half as
    /// fast again stays half as fast again over the new body's own.
    #[test]
    fn a_beasts_run_follows_its_body_as_its_owner_left_it() {
        let before = beast_of(Family::Dog);
        let mut after = before.clone();
        let Archetype::Quadruped(params) = &mut after.archetype else {
            unreachable!("a beast");
        };
        params.height *= 1.4;
        let untuned = fit_on_foot(&before, &HumanoidParams::default());
        let grown = fit_on_foot(&after, &untuned);
        assert_ne!(grown.walk_speed, untuned.walk_speed, "the premise");
        assert_eq!(
            follow(&after, Some(&before), &untuned, &mut None),
            Some(grown.clone())
        );
        let tuned = HumanoidParams {
            walk_speed: Fp(untuned.walk_speed.0 * 1.5).on_wire(),
            ..untuned.clone()
        };
        let followed = follow(&after, Some(&before), &tuned, &mut None).expect("refitted");
        let ratio = followed.walk_speed.0 / grown.walk_speed.0;
        assert!(
            (ratio - 1.5).abs() < 1e-3,
            "the tuned run is {ratio} of the body's"
        );
        assert_eq!(
            HumanoidParams {
                walk_speed: grown.walk_speed,
                ..followed.clone()
            },
            grown,
            "only the run is the owner's"
        );
        // An edit that sizes nothing - a colour - moves nothing, the tuned
        // run included.
        assert_eq!(follow(&before, Some(&before), &tuned, &mut None), None);
    }

    /// **A person standing up is never refitted, and a person becoming a
    /// beast runs as the beast does** (#1803): the person's run, mass and
    /// jump are theirs until they are a beast, and then the beast's own
    /// (D4) - not the person's tuned run carried over as if it were one.
    #[test]
    fn a_person_is_never_refitted_and_becomes_a_beast_whole() {
        let person = crate::pds::avatar::wardrobe::engine_default_for_did("did:plc:follow");
        let tuned = HumanoidParams {
            capsule_radius: Fp(0.33),
            walk_speed: Fp(7.5),
            mass: Fp(95.0),
            ..HumanoidParams::default()
        };
        let mut taller = person.clone();
        if let Archetype::Humanoid(p) = &mut taller.archetype {
            p.height += 0.2;
        }
        assert_eq!(follow(&taller, Some(&person), &tuned, &mut None), None);
        assert_eq!(follow(&taller, None, &tuned, &mut None), None);
        let dog = beast_of(Family::Dog);
        assert_eq!(
            follow(&dog, Some(&person), &tuned, &mut None),
            Some(fit_on_foot(&dog, &tuned)),
            "a person becoming a beast"
        );
    }

    /// **A body this build cannot read is never refitted** (#1803): its
    /// on-foot preset is a newer client's, preserved as written - lying,
    /// upright, tuned or not - so a keystroke in its name moves none of it.
    /// Read as a person, a lying preset would have stood up on the canon
    /// figure's capsule with the default person's mass and run.
    #[test]
    fn a_body_this_build_cannot_read_is_never_refitted() {
        let mut body = crate::pds::avatar::wardrobe::engine_default_for_did("did:plc:unknown");
        body.archetype = Archetype::Unknown {
            type_name: String::from("network.symbios.avatar.archetype#bird"),
            fields: serde_json::Map::new(),
        };
        let lying = HumanoidParams {
            capsule_radius: Fp(0.2),
            capsule_length: Fp(0.7),
            mass: Fp(33.0),
            walk_speed: Fp(7.7),
            shape: OnFootShape::Lying {
                height: Fp(0.9),
                ahead: Fp(0.1),
            },
            ..HumanoidParams::default()
        };
        let mut renamed = body.clone();
        renamed.name = String::from("Renamed");
        for p in [lying, HumanoidParams::default()] {
            assert_eq!(follow(&renamed, Some(&body), &p, &mut None), None, "{p:?}");
            assert_eq!(follow(&renamed, None, &p, &mut None), None, "{p:?}");
        }
    }

    /// **A tuned run comes back exactly from an edit that goes out and
    /// back** (#1803): thirty steps of a beast's height up and thirty down,
    /// each refitted from the frame before as a drag refits it, carried from
    /// one [`RunAnchor`] - the owner's run, never the frame before's
    /// rounding of it - come back to the record they left, on every family
    /// and step tried. Carried frame to frame, with no anchor held, the
    /// ratio and its rounding onto the wire's grid compound, and some of
    /// the same drags bring the 1.5x run back a step of that grid off: the
    /// premise, checked here, that this test can fail.
    #[test]
    fn a_tuned_run_comes_back_from_an_edit_out_and_back() {
        let mut beasts: Vec<EngineAvatarRecord> = [Family::Dog, Family::Cat, Family::Horse]
            .into_iter()
            .map(beast_of)
            .collect();
        let mut rolled = EngineAvatarRecord::rolled(
            "Beast",
            Archetype::Quadruped(symbios_avatar::QuadrupedParams::default()),
            11,
        );
        rolled.sanitize();
        beasts.push(rolled);
        let mut drifted = 0;
        for start in &beasts {
            let untuned = fit_on_foot(start, &HumanoidParams::default());
            let tuned = HumanoidParams {
                walk_speed: Fp(untuned.walk_speed.0 * 1.5).on_wire(),
                ..untuned
            };
            for stride in [0.003f32, 0.007, 0.01] {
                let sized = |step: i32| {
                    let mut body = start.clone();
                    if let Archetype::Quadruped(q) = &mut body.archetype {
                        q.height += stride * step as f32;
                    }
                    body
                };
                // `held` keeps the anchor across the drag, as the editor
                // does; without it every frame reads a new one.
                let drag = |held: bool| {
                    let (mut anchor, mut p, mut before) = (None, tuned.clone(), start.clone());
                    let mut moved = false;
                    for step in (1..=30).chain((0..30).rev()) {
                        let body = sized(step);
                        if !held {
                            anchor = None;
                        }
                        if let Some(fit) = follow(&body, Some(&before), &p, &mut anchor) {
                            p = fit;
                        }
                        moved |= p.walk_speed != tuned.walk_speed;
                        before = body;
                    }
                    assert!(moved, "the premise: the run moved on the way");
                    p
                };
                assert_eq!(drag(true), tuned, "a stride of {stride} did not come back");
                drifted += usize::from(drag(false) != tuned);
            }
        }
        assert!(
            drifted > 0,
            "the premise: carried frame to frame, no drag drifts"
        );
    }

    /// **A run left on a body that does not rig is kept, not read as
    /// untuned** (#1803): nothing says what that body derives, so nothing
    /// says the owner's run was its own - kept as it is, it is not traded
    /// for the new body's. An anchor already held carries on through it.
    #[test]
    fn a_run_left_on_a_body_that_does_not_rig_is_kept() {
        let (run, now) = (Fp(8.486), Fp(6.0));
        assert_eq!(RunAnchor::read(None, run, None), None);
        assert_eq!(
            RunAnchor::run_on(RunAnchor::read(None, run, None), run, now),
            run,
            "a run left on a body that does not rig was traded for the new body's"
        );
        let held = RunAnchor::read(None, run, Some(Fp(5.6573))).expect("read");
        assert_eq!(RunAnchor::read(Some(held), run, None), Some(held));
        assert_eq!(held.carry(Fp(5.6573)), run);
        assert_ne!(held.carry(now), now, "a tuned run read as untuned");
        let untuned = RunAnchor::read(None, Fp(5.6573), Some(Fp(5.6573))).expect("read");
        assert_eq!(untuned.carry(now), now);
    }

    /// A winged body of `family`, crossed with `hybrid` at `blend` when one
    /// is named, at `size` when one is: the family picker's body, the blend
    /// set exactly (a change of kind would round it into its own range).
    fn winged_body(
        family: WingedFamily,
        hybrid: Option<(WingedFamily, f32)>,
        size: Option<f32>,
    ) -> EngineAvatarRecord {
        let mut body = default_winged(family);
        let Archetype::Winged(params) = &mut body.archetype else {
            unreachable!("winged");
        };
        if let Some((other, blend)) = hybrid {
            params.set_kind(family, Some(other), blend);
            params.blend = blend;
        }
        if let Some(size) = size {
            params.size = size;
        }
        body
    }

    /// Every winged body the fit is asked of: each family's own, and the
    /// crosses either side of an even blend and on it.
    fn winged_bodies() -> Vec<(String, EngineAvatarRecord)> {
        use WingedFamily::{Bat, Bird, Glider};
        let mut bodies: Vec<(String, EngineAvatarRecord)> = [Bird, Bat, Glider]
            .into_iter()
            .map(|family| (format!("{family:?}"), winged_body(family, None, None)))
            .collect();
        for (family, other) in [(Bird, Bat), (Bird, Glider), (Bat, Glider), (Bat, Bird)] {
            for blend in [0.49, 0.5, 0.51] {
                bodies.push((
                    format!("{family:?} x {other:?} at {blend}"),
                    winged_body(family, Some((other, blend)), None),
                ));
            }
        }
        bodies
    }

    fn leading(body: &EngineAvatarRecord) -> WingedFamily {
        let Archetype::Winged(params) = &body.archetype else {
            unreachable!("winged");
        };
        Kinship::of(params).leading
    }

    /// **A winged body is fitted by its leading family's posture** (#1875,
    /// the owner's decision on #1859): a bat- or a glider-led body exactly as
    /// a beast of its trunk is, lying girdle to girdle; a bird-led one
    /// upright, as tall as it stands to the top of its head and as wide as its
    /// trunk - every family, and every cross either side of an even blend
    /// and on it, which its second family leads. Every one is given a flight
    /// section, and a bird's mass and run are its size's as a beast's are.
    #[test]
    fn a_winged_body_gets_its_leading_familys_shape() {
        let base = HumanoidParams::default();
        for (name, body) in winged_bodies() {
            let fit = fit_on_foot(&body, &base);
            let rig = Rig::from_skeleton(&body.skeleton()).expect("a winged body rigs");
            assert!(fit.flies(), "{name}: no flight section");
            if let Some(flight) = &fit.flight {
                println!(
                    "{name}: {} r {:.4} length {:.4} stands {:.4} m, {:.2} kg, run {:.3} m/s, \
                     jump {:.1}; flight power {:.2}, beat {:.3}/s, trim {:.3}, stall {:.3}, \
                     take-off {:.3}",
                    if fit.is_lying() { "lying" } else { "upright" },
                    fit.capsule_radius.0,
                    fit.capsule_length.0,
                    fit.stand_height(),
                    fit.mass.0,
                    fit.walk_speed.0,
                    fit.jump_impulse.0,
                    flight.flight_power.0,
                    flight.wingbeat_rate.0,
                    flight.trim_speed.0,
                    flight.stall_speed.0,
                    flight.takeoff_speed.0
                );
            }
            let mut flightless = fit.clone();
            flightless.flight = None;
            match leading(&body) {
                WingedFamily::Bird => {
                    assert_eq!(fit.shape, OnFootShape::Upright, "{name}");
                    let top = rig
                        .joints
                        .iter()
                        .filter(|joint| !joint.marker)
                        .map(|joint| joint.position.y + joint.radius * joint.scale.y)
                        .fold(f32::MIN, f32::max);
                    assert!(
                        (fit.total_height() - top).abs() < 2e-4,
                        "{name}: a {:.3} m capsule on a bird {top:.3} m tall",
                        fit.total_height()
                    );
                    assert!(
                        fit.capsule_radius.0 < fit.total_height() * 0.5,
                        "{name}: a ball"
                    );
                    assert!(fit.mass.0 < base.mass.0 * 0.5, "{name}: {} kg", fit.mass.0);
                    let run = Speed::from_froude(RUN_FROUDE).metres_per_second(&rig);
                    assert_eq!(fit.walk_speed, Fp(run).on_wire(), "{name}");
                }
                WingedFamily::Bat | WingedFamily::Glider => {
                    assert!(fit.is_lying(), "{name}: {:?}", fit.shape);
                    assert_eq!(
                        Some(flightless),
                        lying(&rig, &base),
                        "{name}: not a beast's fit of its trunk"
                    );
                }
            }
        }
    }

    /// **A bird's capsule is the bird's, not a person's** (#1875): the
    /// smallest bird the engine rolls - and every size up from it - stands in
    /// a capsule no taller than itself and two centimetres more, where the
    /// stature rule's clamps would have stood a 0.39 m owl in a 0.76 m one.
    /// The record's own floors hold under about 0.2 m, which no roll reaches.
    #[test]
    fn a_small_birds_capsule_is_no_taller_than_it_stands() {
        let (smallest, largest) = symbios_avatar::plan::winged_size_range();
        let steps = 12;
        for step in 0..=steps {
            let size = smallest + (largest - smallest) * step as f32 / steps as f32;
            let body = winged_body(WingedFamily::Bird, None, Some(size));
            let fit = fit_on_foot(&body, &HumanoidParams::default());
            let rig = Rig::from_skeleton(&body.skeleton()).expect("rigs");
            let top = rig
                .joints
                .iter()
                .filter(|joint| !joint.marker)
                .map(|joint| joint.position.y + joint.radius * joint.scale.y)
                .fold(f32::MIN, f32::max);
            assert!(
                fit.stand_height() <= top + 0.02,
                "a {size:.2} owl {top:.3} m tall stands in a {:.3} m capsule",
                fit.stand_height()
            );
            if step == 0 {
                let person = person(top, &HumanoidParams::default());
                assert!(
                    person.total_height() > top + 0.3,
                    "the premise: the stature rule stands it in {:.3} m",
                    person.total_height()
                );
            }
        }
    }

    /// **A winged fit is on the wire's grid, and refitting it moves nothing**
    /// (#1875): every family and cross, the flight section included, read
    /// back from its JSON as a peer reads it and fitted again over itself.
    #[test]
    fn a_winged_fit_is_idempotent_on_the_wire_grid() {
        for (name, body) in winged_bodies() {
            let fit = fit_on_foot(&body, &HumanoidParams::default());
            let read_back: HumanoidParams =
                serde_json::from_value(serde_json::to_value(&fit).expect("writes")).expect("reads");
            assert_eq!(read_back, fit, "{name}: off the wire's grid");
            assert_eq!(fit_on_foot(&body, &fit), fit, "{name}: a refit moved it");
            let mut sanitized = fit.clone();
            sanitized.sanitize();
            assert_eq!(sanitized, fit, "{name}: the sanitiser moved it");
        }
    }

    /// The beat the engine itself draws a built `body` flapping at: built
    /// whole, wings grown, and flown a few frames by its own driver.
    fn drawn_wingbeat(body: &EngineAvatarRecord) -> f32 {
        let avatar = symbios_avatar::Avatar::build_with(
            body,
            &symbios_avatar::AvatarConfig {
                atlas: 16,
                ..Default::default()
            },
        )
        .expect("a winged body builds");
        assert!(avatar.rig.wings.is_some(), "the premise: grown wings");
        let mut driver = Driver::seeded(9);
        for _ in 0..10 {
            let inputs = Inputs {
                delta: 1.0 / 60.0,
                flight: Some(Flight::new(FlightMode::Flap)),
                ..Default::default()
            };
            driver.drive(&avatar.rig, &inputs, level_ground);
        }
        driver.flight(&avatar.rig).expect("flying").rate
    }

    /// **The flap key beats at the rate the wings are drawn at** (#1875):
    /// within 5 % of the engine's own beat for the built body, for the owl,
    /// the bat, the glider and the crosses either side of an even blend.
    /// Unfitted, the section's rate keyed on how tall the collider stands,
    /// which on a bat lying 0.2 m tall beat three times too fast: the premise,
    /// checked, that this can fail.
    #[test]
    fn a_winged_bodys_wingbeat_is_the_engines() {
        let mut worst_unfitted = 0.0f32;
        for (name, body) in winged_bodies()
            .into_iter()
            .filter(|(name, _)| !name.contains("at 0.5") || name.starts_with("Bird x Bat"))
        {
            let fit = fit_on_foot(&body, &HumanoidParams::default());
            let flight = fit.flight.as_ref().expect("a flight section");
            let drawn = drawn_wingbeat(&body);
            let off = flight.wingbeat_rate.0 / drawn - 1.0;
            assert!(
                off.abs() < 0.05,
                "{name}: the key beats {:.3} a second, the wings {drawn:.3}",
                flight.wingbeat_rate.0
            );
            let unfitted = FlightParams::for_height(fit.stand_height()).wingbeat_rate.0;
            worst_unfitted = worst_unfitted.max((unfitted / drawn - 1.0).abs());
        }
        assert!(
            worst_unfitted > 0.5,
            "the premise: the height law was within {worst_unfitted:.2} of the engine's"
        );
    }

    /// **The smallest winged body beats at the rate its wings are drawn at,
    /// and pushes as hard a second** (#1885): every family and every cross
    /// either side of an even blend and on it, from the record's size floor
    /// up, is fitted the engine's own wingbeat to within 5 % - the flap key
    /// beats as the wings do - and a beat's push times the wingbeat is the
    /// push a second [`FlightParams::for_height`] gives it (by
    /// [`WEAKEST_PUSH`] for a weak flapper). The engine beats 22 to 24 a
    /// second at the floor; held to 12 by the sanitiser, a tiny body's key beat
    /// half as fast as its wings, and since its push a beat had been cut to
    /// keep the push a second, it climbed on half its push.
    #[test]
    fn the_smallest_winged_body_beats_at_its_engine_rate() {
        let (floor, _) = WingedParams::size_envelope();
        let mut fastest = 0.0f32;
        for (name, body) in winged_bodies() {
            for size in [floor, floor * 1.5, floor * 2.0] {
                let mut body = body.clone();
                let Archetype::Winged(params) = &mut body.archetype else {
                    unreachable!("winged");
                };
                params.size = size;
                let kinship = Kinship::of(params);
                let fit = fit_on_foot(&body, &HumanoidParams::default());
                let flight = fit.flight.as_ref().expect("a flight section");
                if !flight.can_flap() {
                    continue;
                }
                let rig = Rig::from_skeleton(&body.skeleton()).expect("rigs");
                let engine = engine_wingbeat(&rig, kinship).expect("an engine beat");
                fastest = fastest.max(engine);
                let label = format!("{name} at size {size:.3}");
                assert!(
                    (flight.wingbeat_rate.0 / engine - 1.0).abs() < 0.05,
                    "{label}: the key beats {:.2} a second, the wings {engine:.2}",
                    flight.wingbeat_rate.0
                );
                let unfitted = FlightParams::for_height(fit.stand_height());
                let weak = (WEAKEST_PUSH / flight.flight_power.0).max(1.0);
                let want = unfitted.beat_speed.0 * unfitted.wingbeat_rate.0 * weak;
                let pushed = flight.beat_speed.0 * flight.wingbeat_rate.0;
                assert!(
                    (pushed / want - 1.0).abs() < 0.05,
                    "{label}: pushed {pushed:.2} m/s a second, {want:.2} wanted"
                );
            }
        }
        println!("the fastest engine beat at the floor: {fastest:.2} a second");
        assert!(
            fastest > 12.0,
            "the premise: {fastest:.2} a second at the floor"
        );
    }

    /// **A glider only glides; a bird flaps fully, a bat at 0.7, a cross by
    /// its shares** (#1875, the owner's decision on #1859): a body a glider
    /// leads has no flap power, so its take-off is the strong leap; any other
    /// flaps by its families, a glider's share adding nothing.
    #[test]
    fn a_winged_bodys_flap_power_is_its_families() {
        use WingedFamily::{Bat, Bird, Glider};
        let power = |body: &EngineAvatarRecord| {
            fit_on_foot(body, &HumanoidParams::default())
                .flight
                .expect("a flight section")
                .flight_power
                .0
        };
        assert_eq!(power(&winged_body(Bird, None, None)), 1.0);
        assert_eq!(power(&winged_body(Bat, None, None)), 0.7);
        assert_eq!(power(&winged_body(Glider, None, None)), 0.0);
        // A bird crossed 0.49 toward a bat: 0.51 of a bird and 0.49 of a bat.
        let cross = power(&winged_body(Bird, Some((Bat, 0.49)), None));
        assert!((cross - (0.51 + 0.49 * 0.7)).abs() < 1e-3, "{cross}");
        // Led by the bird, a glider's share flaps nothing.
        let cross = power(&winged_body(Bird, Some((Glider, 0.49)), None));
        assert!((cross - 0.51).abs() < 1e-3, "{cross}");
        // Led by the glider at an even blend or past it, nothing at all.
        for blend in [0.5, 0.51] {
            assert_eq!(power(&winged_body(Bird, Some((Glider, blend)), None)), 0.0);
            assert_eq!(power(&winged_body(Bat, Some((Glider, blend)), None)), 0.0);
        }
        let glider = fit_on_foot(&winged_body(Glider, None, None), &HumanoidParams::default());
        assert!(!glider.flight.expect("a section").can_flap());
    }

    /// **A winged body follows its edits as a beast does, across a change of
    /// family too** (#1875): its collider, mass, flight section and untuned
    /// run are the new body's own, a bird made a bat lies down and a bat made
    /// a bird stands up; and a tuned run keeps its distance from the body's.
    #[test]
    fn a_winged_body_follows_its_body_across_a_change_of_family() {
        use WingedFamily::{Bat, Bird};
        let (owl, bat) = (winged_body(Bird, None, None), winged_body(Bat, None, None));
        let on_owl = fit_on_foot(&owl, &HumanoidParams::default());
        assert_eq!(
            follow(&bat, Some(&owl), &on_owl, &mut None),
            Some(fit_on_foot(&bat, &on_owl)),
            "a bird made a bat"
        );
        let on_bat = fit_on_foot(&bat, &HumanoidParams::default());
        assert!(on_bat.is_lying() && !on_owl.is_lying(), "the premise");
        assert_eq!(
            follow(&owl, Some(&bat), &on_bat, &mut None),
            Some(fit_on_foot(&owl, &on_bat)),
            "a bat made a bird"
        );
        // An edit that sizes nothing moves nothing.
        assert_eq!(follow(&owl, Some(&owl), &on_owl, &mut None), None);
        // A grown bird runs further, and a tuned run keeps its share of it.
        let mut grown = owl.clone();
        if let Archetype::Winged(params) = &mut grown.archetype {
            params.size *= 1.3;
        }
        let tuned = HumanoidParams {
            walk_speed: Fp(on_owl.walk_speed.0 * 1.5).on_wire(),
            ..on_owl.clone()
        };
        let followed = follow(&grown, Some(&owl), &tuned, &mut None).expect("refitted");
        let own = fit_on_foot(&grown, &on_owl);
        assert_ne!(own.walk_speed, on_owl.walk_speed, "the premise");
        let ratio = followed.walk_speed.0 / own.walk_speed.0;
        assert!(
            (ratio - 1.5).abs() < 1e-3,
            "the tuned run is {ratio} of the body's"
        );
    }

    /// **Into and out of a winged body, both ways** (#1875): a person or a
    /// beast made winged takes the winged body's own fit, flight and run; a
    /// winged body made a person stands up on the stature rule with the
    /// default person's mass, jump and run, and a winged body made a beast
    /// lies on the beast's fit - and neither flies any more.
    #[test]
    fn a_body_into_and_out_of_winged_is_refitted_both_ways() {
        let person = crate::pds::avatar::wardrobe::engine_default_for_did("did:plc:follow");
        let dog = beast_of(Family::Dog);
        let tuned_person = HumanoidParams {
            walk_speed: Fp(7.5),
            ..HumanoidParams::default()
        };
        for family in [WingedFamily::Bird, WingedFamily::Bat, WingedFamily::Glider] {
            let winged = winged_body(family, None, None);
            let own = fit_on_foot(&winged, &tuned_person);
            assert_eq!(
                follow(&winged, Some(&person), &tuned_person, &mut None),
                Some(own.clone()),
                "a person made a {family:?}"
            );
            let on_dog = fit_on_foot(&dog, &HumanoidParams::default());
            assert_eq!(
                follow(&winged, Some(&dog), &on_dog, &mut None),
                Some(fit_on_foot(&winged, &on_dog)),
                "a beast made a {family:?}"
            );

            let stood = follow(&person, Some(&winged), &own, &mut None).expect("stood up");
            let default = HumanoidParams::default();
            assert!(!stood.flies(), "a {family:?} made a person still flies");
            assert!(!stood.is_lying());
            assert_eq!(
                (stood.mass, stood.jump_impulse, stood.walk_speed),
                (default.mass, default.jump_impulse, default.walk_speed),
                "a {family:?} made a person"
            );
            assert_eq!(
                stood,
                fit_on_foot(
                    &person,
                    &HumanoidParams {
                        flight: None,
                        ..stood.clone()
                    }
                )
            );
            let laid = follow(&dog, Some(&winged), &own, &mut None).expect("laid down");
            assert!(!laid.flies(), "a {family:?} made a beast still flies");
            let mut want = fit_on_foot(&dog, &own);
            want.flight = None;
            assert_eq!(laid, want, "a {family:?} made a beast");
        }
    }

    /// **A winged record on its owner's client gets back what an older
    /// client dropped** (#1875): its flight section as its body flies, the
    /// rest of its preset untouched; and a bat's lying shape, refitted to its
    /// body. A winged body standing as it should, with its section, is left
    /// exactly as it is.
    #[test]
    fn a_winged_record_gets_back_what_an_older_client_dropped() {
        for family in [WingedFamily::Bird, WingedFamily::Bat, WingedFamily::Glider] {
            let fitted = fitted_record(default_winged(family));
            let mut again = fitted.clone();
            assert!(
                !restore_body_shape(&mut again),
                "{family:?}: refitted a fit"
            );
            assert_eq!(again, fitted);

            let mut dropped = fitted.clone();
            let super::super::LocomotionConfig::Humanoid(p) = &mut dropped.locomotion else {
                unreachable!("on foot");
            };
            p.flight = None;
            p.walk_speed = Fp(6.5);
            assert!(restore_body_shape(&mut dropped), "{family:?}: not restored");
            let super::super::LocomotionConfig::Humanoid(p) = &dropped.locomotion else {
                unreachable!("on foot");
            };
            let super::super::LocomotionConfig::Humanoid(want) = &fitted.locomotion else {
                unreachable!("on foot");
            };
            assert_eq!(p.flight, want.flight, "{family:?}: the section");
            assert_eq!(p.walk_speed, Fp(6.5), "{family:?}: the owner's run moved");

            if family != WingedFamily::Bird {
                // An older client wrote the bat back upright.
                let mut stood = fitted.clone();
                let super::super::LocomotionConfig::Humanoid(p) = &mut stood.locomotion else {
                    unreachable!("on foot");
                };
                p.shape = OnFootShape::Upright;
                p.flight = None;
                assert!(restore_body_shape(&mut stood), "{family:?}: left upright");
                assert_eq!(stood, fitted, "{family:?}: refitted to another fit");
            }
        }
    }

    /// A person whose owner tuned the fields a refit could move, wearing
    /// the prototype's flight section (#1861).
    fn flying_person() -> HumanoidParams {
        HumanoidParams {
            capsule_radius: Fp(0.31),
            capsule_length: Fp(1.07),
            mass: Fp(71.0),
            walk_speed: Fp(6.2),
            jump_impulse: Fp(520.0),
            flight: Some(FlightParams::default()),
            ..HumanoidParams::default()
        }
    }

    /// **A person who flies keeps their section and their tuning through an
    /// edit of their body** (#1875's critic, #1861): an on-foot body of any
    /// kind may carry one, and only a body that WAS winged has a winged
    /// body's section to drop. A rename or a taller stature leaves a standing
    /// person exactly as they were, through [`follow`] and [`follow_body`].
    #[test]
    fn a_flying_person_keeps_their_section_through_an_edit() {
        let person = crate::pds::avatar::wardrobe::engine_default_for_did("did:plc:follow");
        let tuned = flying_person();
        let mut renamed = person.clone();
        renamed.name = String::from("Renamed");
        let mut taller = person.clone();
        if let Archetype::Humanoid(params) = &mut taller.archetype {
            params.height += 0.2;
        }
        for (edit, body) in [("renamed", &renamed), ("taller", &taller)] {
            assert_eq!(
                follow(body, Some(&person), &tuned, &mut None),
                None,
                "a flying person {edit}"
            );
            assert_eq!(follow(body, None, &tuned, &mut None), None, "{edit}");
        }
        let mut record = fitted_record(person.clone());
        record.locomotion = tuned.clone().into_config();
        let kept = record.clone();
        if let Some(rig) = record.body.rigged_mut()
            && let Some(resolved) = rig.resolved.as_mut()
        {
            resolved.body = renamed.clone();
        }
        assert!(
            !follow_body(&mut record, Some(&person), &mut None),
            "a rename refitted a flying person"
        );
        assert_eq!(record.locomotion, kept.locomotion);
    }

    /// **A beast that flies keeps its section through an edit of its body**
    /// (#1875's critic, #1861): a rename moves nothing, and the section
    /// rides a refit of its collider.
    #[test]
    fn a_flying_beast_keeps_its_section_through_an_edit() {
        let dog = beast_of(Family::Dog);
        let mut on_dog = fit_on_foot(&dog, &HumanoidParams::default());
        on_dog.flight = Some(FlightParams::default());
        let mut named = dog.clone();
        named.name = String::from("Renamed");
        assert_eq!(
            follow(&named, Some(&dog), &on_dog, &mut None),
            None,
            "a flying beast renamed"
        );
        let mut grown = dog.clone();
        if let Archetype::Quadruped(params) = &mut grown.archetype {
            params.height *= 1.3;
        }
        let refit = follow(&grown, Some(&dog), &on_dog, &mut None).expect("refitted");
        assert_ne!(refit.capsule_length, on_dog.capsule_length, "the premise");
        assert_eq!(
            refit.flight, on_dog.flight,
            "a grown beast lost its section"
        );
    }

    /// **The owner's install stands a person under a lying preset up with
    /// their flight section on** (#1875's critic, #1803): the beast's
    /// collider, mass, jump and run go, and the section, which a person may
    /// wear, stays.
    #[test]
    fn a_person_stood_up_on_install_keeps_their_section() {
        let person = crate::pds::avatar::wardrobe::engine_default_for_did("did:plc:follow");
        let mut on_dog = fit_on_foot(&beast_of(Family::Dog), &HumanoidParams::default());
        on_dog.flight = Some(FlightParams::default());
        let mut lying = fitted_record(person.clone());
        lying.locomotion = HumanoidParams {
            flight: Some(FlightParams::default()),
            ..on_dog.clone()
        }
        .into_config();
        assert!(restore_body_shape(&mut lying), "a person left lying");
        let super::super::LocomotionConfig::Humanoid(stood) = &lying.locomotion else {
            unreachable!("on foot");
        };
        assert!(!stood.is_lying());
        assert_eq!(
            stood.flight,
            Some(FlightParams::default()),
            "a person stood up lost its section"
        );
    }

    /// **A person over a winged body's fit stands up as a person, section or
    /// not** (#1875's critic): the body before the edit says the preset was a
    /// winged body's, so a bird whose section an agent cleared, made a
    /// person, is not left in the bird's capsule at the bird's mass.
    #[test]
    fn a_person_over_a_winged_fit_without_its_section_stands_up_as_a_person() {
        let person = crate::pds::avatar::wardrobe::engine_default_for_did("did:plc:follow");
        for family in [WingedFamily::Bird, WingedFamily::Bat, WingedFamily::Glider] {
            let winged = winged_body(family, None, None);
            let own = fit_on_foot(&winged, &HumanoidParams::default());
            let bare = HumanoidParams {
                flight: None,
                ..own.clone()
            };
            let with = follow(&person, Some(&winged), &own, &mut None).expect("stood up");
            let without = follow(&person, Some(&winged), &bare, &mut None)
                .unwrap_or_else(|| panic!("a {family:?} with no section left as it was"));
            assert_eq!(without, with, "a {family:?} with no section");
            let default = HumanoidParams::default();
            assert_eq!(
                (without.mass, without.walk_speed),
                (default.mass, default.walk_speed)
            );
        }
    }

    /// **A winged body's handling is its owner's, and its size refits**
    /// (#1875's critic): through a rename, or a body grown, the flight
    /// numbers the body sizes - its flap power, trim, stall, beat, wingbeat
    /// and take-off - are the new body's, and the ones it does not - its
    /// stamina, recovery, glide ratio, bank, roll and pitch - are the owner's.
    #[test]
    fn a_winged_bodys_handling_is_kept_and_its_size_refits() {
        for family in [WingedFamily::Bird, WingedFamily::Bat, WingedFamily::Glider] {
            let winged = winged_body(family, None, None);
            let own = fit_on_foot(&winged, &HumanoidParams::default());
            let mut tuned = own.clone();
            let flight = tuned.flight.as_mut().expect("a winged fit flies");
            flight.flight_power = Fp(0.5);
            flight.trim_speed = Fp(9.0);
            flight.stall_speed = Fp(2.0);
            flight.beat_speed = Fp(4.5);
            flight.wingbeat_rate = Fp(7.0);
            flight.takeoff_speed = Fp(6.0);
            flight.stamina_beats = Fp(48.0);
            flight.stamina_recovery = Fp(20.0);
            flight.glide_ratio = Fp(9.0);
            flight.max_bank = Fp(30.0);
            flight.roll_rate = Fp(300.0);
            flight.pitch_rate = Fp(150.0);
            flight.sanitize();
            let handling = |p: &HumanoidParams| {
                let f = p.flight.as_ref().expect("flies");
                [
                    f.stamina_beats,
                    f.stamina_recovery,
                    f.glide_ratio,
                    f.max_bank,
                    f.roll_rate,
                    f.pitch_rate,
                ]
            };
            let sized = |p: &HumanoidParams| {
                let f = p.flight.as_ref().expect("flies");
                [
                    f.flight_power,
                    f.trim_speed,
                    f.stall_speed,
                    f.beat_speed,
                    f.wingbeat_rate,
                    f.takeoff_speed,
                ]
            };
            assert!(
                sized(&tuned)
                    .iter()
                    .zip(sized(&own))
                    .all(|(tuned, own)| *tuned != own),
                "{family:?}: the premise, every sized number tuned off"
            );
            let mut renamed = winged.clone();
            renamed.name = String::from("Renamed");
            let mut grown = winged.clone();
            if let Archetype::Winged(params) = &mut grown.archetype {
                params.size *= 1.3;
            }
            for (edit, body) in [("renamed", &renamed), ("grown", &grown)] {
                let refit = follow(body, Some(&winged), &tuned, &mut None).expect("refitted");
                assert_eq!(
                    handling(&refit),
                    handling(&tuned),
                    "{family:?} {edit}: the owner's handling"
                );
                assert_eq!(
                    sized(&refit),
                    sized(&fit_on_foot(body, &HumanoidParams::default())),
                    "{family:?} {edit}: the body's size"
                );
            }
        }
    }
}
