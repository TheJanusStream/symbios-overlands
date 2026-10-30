//! The on-foot preset sized from the body it carries (#1802, owner decisions
//! D2 and D4): [`fit_on_foot`].
//!
//! A person is sized by their stature, as seeded defaults have been since
//! #1060 - the rule moved here unchanged, to the bit, so a seeded person's
//! record is the one it always was. A four-legged beast is sized from its
//! own trunk, read off the engine's rig of its skeleton with no mesh built:
//! a capsule lying along the trunk from girdle to girdle, at a radius of half
//! the trunk's depth, and the run, the mass and the jump that body's size
//! gives it.

use symbios_avatar::plan::Zone;
use symbios_avatar::{Archetype, Rig, Speed};

use super::{HumanoidParams, LocomotionPreset, OnFootShape};
use crate::pds::avatar::EngineAvatarRecord;
use crate::pds::types::Fp;

/// The stature a body plan this build cannot read is sized at (m): the canon
/// figure the preset's own default collider was cut for, which is also the
/// person the engine draws for it.
const UNKNOWN_STATURE: f32 = 1.7;

/// The Froude number a beast runs at - Shift, the record's `walk_speed` - on
/// its own legs (#1802, owner decision D4): the default person's own run,
/// the preset's 5.0 m/s on that body's 0.71 m leg, so the run key asks every
/// body for the same dimensionless speed and a cat and a horse each get
/// their own metres a second of it. A canter on four legs, between the
/// engine's trot-to-canter change at 2.5 and its gallop at 4.0. Pinned to
/// that derivation by `the_beasts_run_is_the_default_persons`.
pub(crate) const RUN_FROUDE: f32 = 3.586;

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
///   `RUN_FROUDE` on its legs; the mass is the default person's mass for
///   the default person's capsule, scaled by the volume of this one; the
///   jump impulse keeps the default person's change of speed on that mass
///   as the record's clamps hold it, so every body jumps as high as they
///   let it. Each is
///   quantised onto the wire's grid as it is fitted, so the record a peer
///   decodes is the one this client stands on. A beast whose skeleton does
///   not rig keeps `base`.
///
/// Sized from the rig the engine roots its skeleton in, without a mesh:
/// meshing a body to measure one costs about a second, and this runs on
/// every edit of the body (D3).
pub fn fit_on_foot(body: &EngineAvatarRecord, base: &HumanoidParams) -> HumanoidParams {
    match &body.archetype {
        Archetype::Humanoid(params) => person(params.height, base),
        Archetype::Quadruped(_) => beast(body, base).unwrap_or_else(|| base.clone()),
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
    let rig = Rig::from_skeleton(&body.skeleton()).ok()?;
    let trunk = Trunk::of(&rig)?;
    let person = HumanoidParams::default();
    let mass = person.mass.0 * capsule_volume(trunk.radius, trunk.span)
        / capsule_volume(person.capsule_radius.0, person.capsule_length.0);
    let mut p = base.clone();
    p.capsule_radius = Fp(trunk.radius).on_wire();
    p.capsule_length = Fp(trunk.span).on_wire();
    p.shape = OnFootShape::Lying {
        height: Fp(trunk.top),
        ahead: Fp(trunk.middle),
    };
    p.mass = Fp(mass).on_wire();
    p.walk_speed = Fp(Speed::from_froude(RUN_FROUDE).metres_per_second(&rig)).on_wire();
    // Clamped as the record is, and the shape quantised with it, so the fit
    // round-trips the wire unchanged.
    p.sanitize();
    // The jump from the mass as the record holds it: a body lighter than
    // its floor weighs the floor, and one cut from the mass it would have
    // had left the ground at a fraction of the speed. Clamped in its turn,
    // which on the heaviest bodies is the most the record allows.
    p.jump_impulse = Fp(p.mass.0 * person.jump_impulse.0 / person.mass.0).on_wire();
    p.sanitize();
    Some(p)
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

/// A capsule's volume (m³): its cylinder and the sphere its two ends make.
fn capsule_volume(radius: f32, length: f32) -> f32 {
    std::f32::consts::PI * radius * radius * (length + radius * 4.0 / 3.0)
}

/// Whether the on-foot preset `p` carries a four-legged beast under `body`
/// (#1803, #1804): a four-legged body always, a person never, and otherwise -
/// a body this build cannot read, or none resolved yet - whether the preset
/// lies. The BODY's kind, not the preset's shape: a person under a lying
/// preset someone else saved is a person.
pub(crate) fn carries_beast(body: Option<&EngineAvatarRecord>, p: &HumanoidParams) -> bool {
    match body.map(|body| &body.archetype) {
        Some(Archetype::Quadruped(_)) => true,
        Some(Archetype::Humanoid(_)) => false,
        _ => p.is_lying(),
    }
}

/// The on-foot preset of `record` when it carries a four-legged beast
/// ([`carries_beast`], over the body the record resolves), and `None` for a
/// person, a vehicle and a preset this build cannot read (#1804): what the
/// camera asks before it frames a body as a beast's.
pub(crate) fn beast_on_foot(record: &crate::pds::AvatarRecord) -> Option<&HumanoidParams> {
    let super::LocomotionConfig::Humanoid(p) = &record.locomotion else {
        return None;
    };
    let body = record
        .body
        .rigged_ref()
        .and_then(|rig| rig.resolved.as_ref())
        .map(|resolved| &resolved.body);
    carries_beast(body, p).then_some(&**p)
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
///   which the beast's fit had replaced.
///
/// Says whether it refitted. A person standing, a beast already lying, a
/// body this build cannot read, a vehicle and a body not resolved yet are
/// left as they are. Only the owner's client runs it, where its own record
/// is installed: a peer's record is drawn as it was published. Both fits
/// are deterministic, so a record refitted twice is refitted to the same
/// bytes.
pub(crate) fn restore_body_shape(record: &mut crate::pds::AvatarRecord) -> bool {
    let Some(body) = record
        .body
        .rigged_ref()
        .and_then(|rig| rig.resolved.as_ref())
        .map(|resolved| &resolved.body)
    else {
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
/// * **A person whose capsule lies** - a beast that became a person, or a
///   fresh body worn over a beast's preset - stands up on the stature rule,
///   and takes the default person's mass, jump and run: the fit keeps its
///   base's, which were the beast's.
/// * **A person standing up** is never refitted: their capsule is
///   whatever its owner set, and an edit of their body moves none of it.
/// * **A body this build cannot read** is never refitted either, lying or
///   not: its preset is a newer client's, preserved as written.
///
/// A beast whose skeleton does not rig keeps what it has, and the anchor
/// with it.
pub(crate) fn follow(
    body: &EngineAvatarRecord,
    before: Option<&EngineAvatarRecord>,
    p: &HumanoidParams,
    anchor: &mut Option<RunAnchor>,
) -> Option<HumanoidParams> {
    let fit = match body.archetype {
        Archetype::Quadruped(_) => {
            let mut fit = beast(body, p)?;
            let was_beast = before.filter(|b| matches!(b.archetype, Archetype::Quadruped(_)));
            *anchor = match was_beast {
                Some(before) if p.is_lying() => {
                    let was = beast(before, p).map(|fit| fit.walk_speed);
                    let held = RunAnchor::read(*anchor, p.walk_speed, was);
                    fit.walk_speed = RunAnchor::run_on(held, p.walk_speed, fit.walk_speed);
                    fit.sanitize();
                    held.map(|held| RunAnchor {
                        written: fit.walk_speed,
                        ..held
                    })
                }
                _ => None,
            };
            fit
        }
        Archetype::Humanoid(_) if p.is_lying() => {
            *anchor = None;
            let person = HumanoidParams::default();
            let base = HumanoidParams {
                mass: person.mass,
                jump_impulse: person.jump_impulse,
                walk_speed: person.walk_speed,
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
    let Some(body) = record
        .body
        .rigged_ref()
        .and_then(|rig| rig.resolved.as_ref())
        .map(|resolved| &resolved.body)
    else {
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
}
