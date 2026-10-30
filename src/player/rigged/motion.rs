use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;
use bevy_symbios_avatar::{AvatarBody, AvatarPose, Drive, Drove};
use symbios_avatar::anim::driver::Hold;
use symbios_avatar::{Heading, Zone};

use crate::player::emote::{BeastEmote, Emote, EmoteRequest};
use crate::player::humanoid::{WaterColumn, WaterState, humanoid_water_state, water_state};
use crate::state::{LiveAvatarRecord, LocalPlayer, RemotePeer};
use crate::water::WaterSurfaces;

use super::{RiggedRoot, RiggedTrail, stands_on_four};

/// Where the crown of a swimmer afloat at the surface is drawn, in metres
/// above the waterline (#1324): the eyes about at the water, the head out.
///
/// The engine poses a swim "at whatever depth the caller puts it", laying the
/// crawl prone about the body's standing hip height, and the chassis is a
/// vertical capsule floating at `humanoid::SWIM_FLOAT`. A treading body drew
/// its crown 0.08 m out of the water there, and a crawling one 0.5 m under it
/// with its hips at −0.7 m. So while a swimmer is afloat the drawn body is
/// raised until its crown sits here - a crawl comes up to the surface, a
/// tread barely moves - and it only ever rises: a body whose crown is already
/// higher is left where it is.
pub(in crate::player) const SURFACE_CROWN: f32 = 0.1;

/// How far a beast peer's chassis may wander and still be read as standing
/// still, metres (#1805, [`hold_still`]).
const STILL_WITHIN: f32 = 0.03;

/// How quickly the drawn body follows that lift, seconds (#1324). The pose it
/// is read from strokes, rolls and surges; a first-order lag this long keeps
/// the stroke from bobbing the whole body and still carries a swimmer from a
/// tread into a crawl, or down on a dive, without a pop.
const LIFT_RESPONSE: f32 = 0.3;

/// The slowest travel a heading is read from, metres a second (#1746): the
/// speed under which the walk controller calls travel directionless. Slower
/// than this a body has no travel to face against, and a peer's differenced
/// positions are mostly noise.
const HEADING_MIN_SPEED: f32 = 0.1;

/// How far a chassis moves in one frame before the fill reads it as placed
/// there rather than walked there, metres (#1752): further than any walk
/// covers in a frame - the walk speed's ceiling, 50 m/s, is 0.8 m at 60 Hz -
/// so a gateway's landing or a return to the spawn, which sets the facing as
/// well as the place, is not read as a turn as fast as the frame was short.
/// Read as one, a quarter turn in a frame is ninety radians a second, and the
/// body steps round on the spot for a second after it arrives. A placement
/// closer than this that sets a facing says so instead
/// ([`crate::player::FacingSet`], [`crate::player::LandingWait`]).
///
/// Nor, for a peer, as a speed (#1759): its speed is differenced from its
/// positions, and ten metres in a frame is six hundred metres a second.
const PLACED_PAST: f32 = 2.0;

/// How near the yaw a [`crate::player::FacingSet`] set the local body counts
/// as there, radians (#1752): far under a frame of any turn, far over the
/// rounding between a rotation and the transform it is propagated into.
const LANDED_WITHIN: f32 = 1e-3;

/// A peer looking out of its own eyes (#1746), as its last
/// [`crate::protocol::OverlandsMessage::FirstPerson`] said: its body faces
/// where it looks, so its rig reads its stride off its travel against its
/// facing. Set on the peer's chassis by the network's inbound handler; a peer
/// that never says so - an older build among them - keeps the stride its
/// travel gives it. The local body answers the same question from the
/// camera's own view instead.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct InFirstPerson;

/// Which way a body travels against the way it faces (#1746), as the engine
/// takes it: the horizontal of `velocity` turned into the body's own frame -
/// forward `+Z`, left `+X` - by `facing`, the yaw that carries that frame onto
/// the world ([`Drive::facing`]). `None` under [`HEADING_MIN_SPEED`], which
/// leaves the stride as the speed built it.
///
/// A diagonal is a heading in its own right and the engine's stride is
/// continuous across all of them (`symbios_avatar::Heading`), so a body
/// swinging from a walk into a strafe has nothing to pop on.
pub(crate) fn heading_of(velocity: Vec3, facing: f32) -> Option<Heading> {
    let travel = Vec3::new(velocity.x, 0.0, velocity.z);
    (travel.length() >= HEADING_MIN_SPEED)
        .then(|| Heading::toward(Quat::from_rotation_y(-facing) * travel))
}

/// How far a yaw of `from` has to turn to reach `to`, the short way round, in
/// radians - positive toward the body's left, the engine's sense of a turn.
fn turned_from(from: f32, to: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (to - from + PI).rem_euclid(TAU) - PI
}

/// A beast peer's differenced speed, with a chassis that has stopped held at
/// no speed at all until it is really going somewhere (#1805).
///
/// The engine gets a resting beast up at 0.1 m/s of travel. Its owner's client
/// reads that off the chassis' own velocity and a peer's off this difference,
/// which at a crawl is far noisier: a sender under that speed sends only every
/// 30th tick, about twice a second, so the playout sits on the last sample and
/// then crosses most of the gap in a frame. A chassis at rest is still on level
/// ground, but on a steep slope it creeps, a dog about 12 mm/s on 28 degrees,
/// and that reads as 0.28 m/s for a frame at 60 Hz and more at 144: its owner
/// sees it sit, and every peer would see it get up.
///
/// So a body that has dropped under the engine's line stands where it is while
/// it stays within [`STILL_WITHIN`] of there, and leaving is judged over the
/// whole time since rather than over the frame that left: under the line on
/// the whole, it is at rest again where it has got to. A walk is let go at its
/// second crossing at the latest, about 6 cm in. Only the horizontal: the
/// vertical is the leap's, and nothing a creep reaches.
fn hold_still(trail: &mut RiggedTrail, position: Vec3, delta: f32, velocity: &mut Vec3) {
    const STIR: f32 = 0.1;
    let planar = |v: Vec3| Vec2::new(v.x, v.z);
    let Some((at, since)) = trail.still_at else {
        if planar(*velocity).length() < STIR {
            trail.still_at = Some((position, 0.0));
        }
        return;
    };
    let since = since + delta;
    let gone = planar(position - at).length();
    trail.still_at = if gone < STILL_WITHIN {
        Some((at, since))
    } else if gone < STIR * since {
        Some((position, 0.0))
    } else {
        None
    };
    if trail.still_at.is_some() {
        velocity.x = 0.0;
        velocity.z = 0.0;
    }
}

/// Every rigged body the fill writes: where it hangs, what it is told, its
/// trail, and - once built and posed - the body and pose the surface lift
/// reads (#1324).
type FilledBodies<'w, 's> = Query<
    'w,
    's,
    (
        &'static ChildOf,
        &'static mut Transform,
        &'static mut Drive,
        &'static mut RiggedTrail,
        Option<&'static AvatarBody>,
        Option<&'static AvatarPose>,
    ),
    With<RiggedRoot>,
>;

/// What the fill reads of a body's chassis: where it is, how fast if it knows,
/// and for a peer, whether it looks out of its eyes and what its owner's record
/// says.
type FilledChassis<'w, 's> = Query<
    'w,
    's,
    (
        &'static GlobalTransform,
        Option<&'static LinearVelocity>,
        Has<InFirstPerson>,
        Option<&'static RemotePeer>,
    ),
>;

/// Tell every built body what its chassis is doing, for
/// [`bevy_symbios_avatar::drive_avatar_bodies`] to drive it with (#1171).
///
/// **This file used to be the driver.** The state machine it ran - airborne
/// as a state, the eased pace, the cycle carried across a change of gait, the
/// idle handed the stance it arrived in, the inertialized source switches -
/// went upstream at `symbios-avatar 0.5.3` and reached Bevy at
/// `bevy_symbios_avatar 0.5.1`, where a body drives itself from its own
/// [`bevy_symbios_avatar::AvatarDriver`]. What is left here is the half only
/// this app can answer: where the chassis is, how fast, which way it faces,
/// whether the water it is standing in is deep enough to swim in, and whether
/// an editor is holding the body still.
///
/// **Carrying a [`Drive`] is what opts a body in**, so this system and
/// `drive_avatar_bodies` are one unit: a body whose [`Drive`] is not filled
/// this frame is still driven, off last frame's facts. They are ordered
/// together in [`bevy_symbios_avatar::AvatarSystems::Animate`], and a test
/// that runs one without the other measures neither (#1069).
///
/// The engine's own floor - a level plane at the rigged root's `y = 0` - is
/// what `drive_avatar_bodies` supplies, and it is exactly the floor this file
/// used to build: the root is offset so `y = 0` is the chassis collider's
/// bottom, and slopes are carried by the chassis pose the way the collider
/// itself carries them. So this app needs none of the escape hatch a body
/// with a driver and no [`Drive`] gets.
#[allow(clippy::too_many_arguments)]
pub(in crate::player) fn fill_rigged_drive(
    time: Res<Time>,
    water: Option<Res<WaterSurfaces>>,
    mut bodies: FilledBodies,
    chassis: FilledChassis,
    locals: Query<(), With<LocalPlayer>>,
    live: Option<Res<LiveAvatarRecord>>,
    hold: Res<crate::player::RigHold>,
    first_person: Option<Res<crate::camera::FirstPerson>>,
    facings: Option<MessageReader<crate::player::FacingSet>>,
    fixed: Res<Time<Fixed>>,
) {
    let delta = time.delta_secs();
    let step = fixed.timestep().as_secs_f32();
    if delta <= 0.0 {
        return;
    }
    let looking_out = first_person
        .as_deref()
        .is_some_and(|fp| fp.looks_out_on_foot());
    // Where the world last set the local body down facing, if it did since
    // the last frame - as a yaw read the way the chassis' own is below.
    let landed = facings.and_then(|mut facings| {
        facings.read().last().map(|set| {
            let forward = set.0 * Vec3::NEG_Z;
            forward.x.atan2(forward.z)
        })
    });
    for (child_of, mut root, mut drive, mut trail, body, pose) in &mut bodies {
        let Ok((transform, velocity, peer_looking_out, peer)) = chassis.get(child_of.parent())
        else {
            continue;
        };
        let local = locals.contains(child_of.parent());
        let beast = body.is_some_and(|body| stands_on_four(&body.avatar.rig));
        let position = transform.translation();
        let moved_far = trail
            .last
            .is_some_and(|(last, _)| last.distance(position) > PLACED_PAST);
        match velocity {
            // A local chassis carries an avian velocity, and its SIGN is the
            // whole of the driver's airborne state machine - an `abs` here
            // would land the body at every apex.
            Some(velocity) => {
                drive.velocity = velocity.0;
                drive.at = position;
            }
            // Remote peers are kinematic playout, so their speed is read off
            // the smoothed transform itself - differenced against where the
            // chassis was, which is [`RiggedTrail`] rather than `Drive::at`
            // for the one reason that component exists: a body on its first
            // frame has no previous position, and calling the install-time
            // snapshot one would start a peer arriving away from the origin
            // on a colossal launch. Over the delta of the frame that position
            // was seen on, not this one's: the transform read here is last
            // frame's playout (see [`RiggedTrail`], #1323).
            //
            // **Not across a placement** (#1759): a peer set down somewhere
            // else keeps the speed it had. Differenced across the jump it
            // read as hundreds of metres a second for a frame, and the eased
            // pace behind that took a second to come back down - a sprint on
            // the spot through symbios-avatar 0.12.0, a stride built for the
            // sprint and stepped at the walk's ground since 0.12.1.
            None => {
                if !moved_far {
                    drive.velocity = trail
                        .last
                        .map_or(Vec3::ZERO, |(last, over)| (position - last) / over);
                }
                if beast {
                    hold_still(&mut trail, position, delta, &mut drive.velocity);
                }
                drive.at = position;
            }
        }
        trail.last = Some((position, delta));
        // **The chassis' yaw carried THROUGH the rigged root's half turn**
        // (#1066). The engine's forward is `+Z` and Bevy's is `-Z`, so the
        // body's world forward is the chassis' own `-Z`; handing the driver
        // the chassis' rotation raw would mirror every held foothold through
        // the body and read as skating (#1193).
        let forward = transform.rotation() * Vec3::NEG_Z;
        drive.facing = forward.x.atan2(forward.z);
        // **In deep water before anything else** (#1074). The rigged root is
        // offset so `y = 0` is the chassis collider's bottom, which makes its
        // own transform the body's half-height - the same figure the
        // controller classifies with, so the animation and the physics cannot
        // disagree about whether this body is swimming.
        //
        // Only `Swimming` animates as one: a wading body has its feet on the
        // bottom and is walking, which is what the controller does with it
        // too. Which is a fact about the world's water rather than about the
        // body, and so is this app's to answer rather than the engine's.
        //
        // Swimming has two thresholds (#1324), so the classification needs
        // to know whether this body was swimming: `Drive::swimming` is what
        // this fill wrote last frame, which is that memory for a local body
        // and a remote peer alike - a peer has no controller here, and
        // classifying it by the same rule from the same positions is what
        // keeps what its owner sees and what everyone else sees the same.
        //
        // **A body whose collider is not a person standing in it reads the
        // record instead** (#1805), for two things its root cannot say. A
        // beast's height is its back's, not its lying capsule's, so it is
        // drawn swimming where its controller swims ([`WaterColumn::of`]).
        // And a chassis takes its record's shape at once while a body is only
        // rebuilt for a change of body, which for a peer waits for the owner's
        // Save: the chassis of a person turned beast plays out at the beast's
        // height with the person still hung a person's height under it, in the
        // ground to the waist, and the other way the beast hangs in the air.
        // So the hang follows the collider, as the build's own offset does:
        // whatever stands on a chassis has its feet at the collider's bottom.
        // Asked of the body that stands and of the shape it stands in, and
        // once more when a person stops lying. A person in an upright capsule
        // who has never lain keeps the root's own reading, to the bit.
        let record = if local {
            live.as_deref().map(|live| &live.0)
        } else {
            peer.and_then(|peer| peer.avatar.as_ref())
        };
        let on_foot = record
            .and_then(|record| match &record.locomotion {
                crate::pds::LocomotionConfig::Humanoid(p) => Some(&**p),
                _ => None,
            })
            .filter(|p| p.is_lying() || beast || trail.rehung);
        if let Some(p) = on_foot {
            trail.rehung = p.is_lying() || beast;
        }
        let column = on_foot.map(WaterColumn::of);
        let hung = column.map_or(root.translation.y - trail.lift, |column| -column.drop);
        let half_height = -hung;
        let was_swimming = drive.swimming;
        drive.swimming = water.as_ref().is_some_and(|water| {
            let xz = Vec2::new(position.x, position.z);
            let state = match column {
                Some(column) => water_state(was_swimming, position.y, xz, column, water),
                None => {
                    humanoid_water_state(was_swimming, position.y, xz, half_height * 2.0, water)
                }
            };
            matches!(state, WaterState::Swimming { .. })
        });
        // **Travel against facing, for a body looking out of its own eyes**
        // (#1746). In first person a walker faces where the eyes look and A,
        // D and S step it sideways and back, so its stride is read off its
        // travel against its facing: the local body while the camera looks
        // out of it, a peer while its last word said so ([`InFirstPerson`]).
        // Every other body keeps `None`, the stride as the speed built it:
        // the walk turns it toward its travel, and a heading read off the lag
        // of that turn splayed the feet 252 mm in a quarter turn, against the
        // 36 the stance agreed for it allows (#1323). Not in deep water,
        // whose stroke goes the way it faces.
        let looking = if local { looking_out } else { peer_looking_out };
        drive.heading = if looking && !drive.swimming {
            heading_of(drive.velocity, drive.facing)
        } else {
            None
        };
        // **And how fast it turns, for the same bodies only** (#1752). Looking
        // out of its eyes a body faces where they look, standing or walking,
        // so it turns as they do - and told the turn, the engine steps it round
        // on planted feet (symbios-avatar #361) where an untold turn carried
        // them round on the floor. Every other body keeps 0: the agreed orbit
        // stance was measured with the turn untold (#1323), and its facing
        // follows its travel, which the stride already walks. A body placed
        // this frame turned nothing ([`PLACED_PAST`]), and nor did one the
        // world set down facing a new way, on any frame from its word to the
        // first one the body is seen there - the steps the chassis takes to
        // turn to it, at any frame rate (#1786, [`crate::player::LandingWait`]).
        if local && let Some(yaw) = landed {
            trail.landing = Some(crate::player::LandingWait::new(yaw));
        }
        let landing = trail.landing.take();
        if let Some(mut wait) = landing
            && turned_from(wait.yaw, drive.facing).abs() >= LANDED_WITHIN
            && !wait.tick(delta, step)
        {
            trail.landing = Some(wait);
        }
        let placed = moved_far || landing.is_some();
        drive.turn = match trail.facing {
            Some((last, over)) if looking && !drive.swimming && !placed => {
                turned_from(last, drive.facing) / over
            }
            _ => 0.0,
        };
        trail.facing = Some((drive.facing, delta));
        // **Afloat, the drawn body rides the surface** (#1324; see
        // [`SURFACE_CROWN`]). Afloat is the controller's own test: swimming,
        // with the capsule's top out of the water. The crown is read off LAST
        // frame's pose, since the driver poses after this; the lift is eased,
        // so a stroke cannot bob it.
        let surface = water
            .as_ref()
            .and_then(|water| water.surface_at(Vec2::new(position.x, position.z)))
            .map(|(_, surface_y)| surface_y);
        // A column's top, for a body whose top is not its capsule's: the
        // controller's own afloat.
        let top = column.map_or(position.y + half_height, |column| {
            position.y - column.drop + column.height
        });
        let target = match (drive.swimming, surface, body, pose) {
            // A body landed this frame has no pose to read yet: a lift it
            // carried over holds, rather than sagging for the frame.
            (_, _, Some(_), None) => trail.lift,
            (true, Some(surface_y), Some(body), Some(pose)) if top >= surface_y => {
                let rig = &body.avatar.rig;
                let posed = pose.0.forward(rig);
                // A body with no head has nothing to hold out of the water.
                rig.in_zone(Zone::Head)
                    .iter()
                    .map(|&joint| posed.positions[joint].y)
                    .reduce(f32::max)
                    .map_or(0.0, |crown| {
                        (surface_y + SURFACE_CROWN - (position.y + hung + crown)).clamp(
                            0.0,
                            column.map_or(half_height, |column| column.height * 0.5),
                        )
                    })
            }
            _ => 0.0,
        };
        trail.lift += (target - trail.lift) * (1.0 - (-delta / LIFT_RESPONSE).exp());
        if trail.lift.abs() < 1e-4 && target == 0.0 {
            trail.lift = 0.0;
        }
        if root.translation.y != hung + trail.lift {
            root.translation.y = hung + trail.lift;
        }
        // **The attachment-editing holds (#1062, #1106).** An attachment
        // offset is stored in its carrying joint's *rest* frame, so while the
        // owner has the in-world gizmo on a **whole worn prop** their own body
        // is pinned to the bind pose and the drag happens in the frame the
        // record actually keeps. The pin is a hard snap, not an inertial
        // blend: a body still settling would let a gizmo release land against
        // a pose that is already gone.
        //
        // A gizmo on a **part** of a worn item holds the body **where it
        // stands** instead: the part is detached at its current world pose and
        // committed back against its parent's pose, which may be any pose so
        // long as it does not move. Re-posing to rest here moved the parent
        // out from under the freshly detached part - selecting visibly shifted
        // it (#1106). [`Hold::Pose`] writes no pose at all, so the last one
        // stays applied (the joint writer runs on `Changed<AvatarPose>` only)
        // and the motion resumes from exactly where it paused.
        //
        // Peers are never held; neither is the local body outside those
        // editor states.
        drive.hold = if local {
            match (hold.at_rest, hold.pose) {
                (true, _) => Hold::Rest,
                (false, true) => Hold::Pose,
                (false, false) => Hold::None,
            }
        } else {
            Hold::None
        };
    }
}

/// Start emotes on the bodies their requests name (#1068).
///
/// Runs in the Animate set ahead of [`fill_rigged_drive`], so a gesture
/// requested this frame is posed this frame rather than a frame late.
///
/// **A request names a chassis and this finds the body under it**, because the
/// chat and network layers hold chassis entities and know nothing of rigged
/// roots. A chassis with no rigged body drops the request silently and on
/// purpose: a boat has nothing to wave with.
///
/// The rate limit is no longer here. [`Drive::gesture`] is a *request* the
/// driver takes - so it fires once without this having to clear it - and the
/// per-body cooldown that makes a peer pasting "hi hi hi hi" wave once is
/// [`symbios_avatar::anim::driver::DriverConfig::gesture_cooldown`], measured
/// on the driver's own clock. There is no missing-clip case either: every
/// [`crate::player::emote::Emote`] names an engine gesture, guarded by test
/// rather than checked per request.
///
/// **The body that stands here chooses** (#1806), read off the rig that is
/// drawn as the hang is: what a beast plays on four legs, what a person plays
/// otherwise, and nothing when the message asked this body for nothing.
pub(in crate::player) fn start_emotes(
    mut requests: MessageReader<EmoteRequest>,
    mut bodies: Query<(&ChildOf, &mut Drive, Option<&AvatarBody>), With<RiggedRoot>>,
) {
    for request in requests.read() {
        for (child_of, mut drive, body) in &mut bodies {
            if child_of.parent() != request.chassis {
                continue;
            }
            let name = if body.is_some_and(|body| stands_on_four(&body.avatar.rig)) {
                request.beast.map(BeastEmote::gesture_name)
            } else {
                request.emote.map(Emote::gesture_name)
            };
            if let Some(name) = name {
                drive.gesture(name);
            }
        }
    }
}

/// Count the frames on which some body's contact solve strained (#1078).
///
/// **Per frame rather than per body**, so a crowd cannot inflate one defect -
/// a body that strains occasionally is a body on hard ground, and one that
/// strains constantly is a body whose goals are wrong.
///
/// Reads [`Drove`], which `drive_avatar_bodies` writes onto every body it
/// drives, and only where it was written **this** frame: a body held exactly
/// where it stands is not driven at all, and its last `Drove` would otherwise
/// go on being counted for as long as the gizmo is up.
pub(in crate::player) fn count_motion_strain(
    driven: Query<&Drove, Changed<Drove>>,
    mut metrics: Option<ResMut<crate::diagnostics::MetricsRegistry>>,
) {
    if driven.iter().any(|drove| drove.strained)
        && let Some(metrics) = metrics.as_deref_mut()
    {
        crate::diagnostics::samplers::motion_strain_frame(metrics);
    }
}
