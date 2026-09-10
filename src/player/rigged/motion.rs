use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;
use bevy_symbios_avatar::{Drive, Drove};
use symbios_avatar::anim::driver::Hold;

use crate::player::emote::EmoteRequest;
use crate::player::humanoid::{WaterState, humanoid_water_state};
use crate::state::LocalPlayer;
use crate::water::WaterSurfaces;

use super::{RiggedRoot, RiggedTrail};

/// Tell every built body what its chassis is doing, for
/// [`bevy_symbios_avatar::drive_avatar_bodies`] to drive it with (#1171).
///
/// **This file used to be the driver.** The state machine it ran — airborne
/// as a state, the eased pace, the cycle carried across a change of gait, the
/// idle handed the stance it arrived in, the inertialized source switches —
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
/// The engine's own floor — a level plane at the rigged root's `y = 0` — is
/// what `drive_avatar_bodies` supplies, and it is exactly the floor this file
/// used to build: the root is offset so `y = 0` is the chassis collider's
/// bottom, and slopes are carried by the chassis pose the way the collider
/// itself carries them. So this app needs none of the escape hatch a body
/// with a driver and no [`Drive`] gets.
pub(in crate::player) fn fill_rigged_drive(
    time: Res<Time>,
    water: Option<Res<WaterSurfaces>>,
    mut bodies: Query<(&ChildOf, &Transform, &mut Drive, &mut RiggedTrail), With<RiggedRoot>>,
    chassis: Query<(&GlobalTransform, Option<&LinearVelocity>)>,
    locals: Query<(), With<LocalPlayer>>,
    hold: Res<crate::player::RigHold>,
) {
    let delta = time.delta_secs();
    if delta <= 0.0 {
        return;
    }
    for (child_of, root, mut drive, mut trail) in &mut bodies {
        let Ok((transform, velocity)) = chassis.get(child_of.parent()) else {
            continue;
        };
        let position = transform.translation();
        match velocity {
            // A local chassis carries an avian velocity, and its SIGN is the
            // whole of the driver's airborne state machine — an `abs` here
            // would land the body at every apex.
            Some(velocity) => {
                drive.velocity = velocity.0;
                drive.at = position;
            }
            // Remote peers are kinematic playout, so their speed is read off
            // the smoothed transform itself — differenced against where the
            // chassis was, which is [`RiggedTrail`] rather than `Drive::at`
            // for the one reason that component exists: a body on its first
            // frame has no previous position, and calling the install-time
            // snapshot one would start a peer arriving away from the origin
            // on a colossal launch. Over the delta of the frame that position
            // was seen on, not this one's: the transform read here is last
            // frame's playout (see [`RiggedTrail`], #1323).
            None => {
                drive.velocity = trail
                    .last
                    .map_or(Vec3::ZERO, |(last, over)| (position - last) / over);
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
        // `None` rather than straight ahead: it leaves the stride exactly as
        // the speed axis built it, which is what this app wants — the chassis
        // is aimed down its own travel by `Transform::looking_to`, so facing
        // and heading are kept in step by something else. A body that could
        // strafe or back up would derive one.
        drive.heading = None;
        // **In deep water before anything else** (#1074). The rigged root is
        // offset so `y = 0` is the chassis collider's bottom, which makes its
        // own transform the body's half-height — the same figure the
        // controller classifies with, so the animation and the physics cannot
        // disagree about whether this body is swimming.
        //
        // Only `Swimming` animates as one: a wading body has its feet on the
        // bottom and is walking, which is what the controller does with it
        // too. Which is a fact about the world's water rather than about the
        // body, and so is this app's to answer rather than the engine's.
        let half_height = -root.translation.y;
        drive.swimming = water.as_ref().is_some_and(|water| {
            matches!(
                humanoid_water_state(
                    position.y,
                    Vec2::new(position.x, position.z),
                    half_height * 2.0,
                    water,
                ),
                WaterState::Swimming { .. }
            )
        });
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
        // out from under the freshly detached part — selecting visibly shifted
        // it (#1106). [`Hold::Pose`] writes no pose at all, so the last one
        // stays applied (the joint writer runs on `Changed<AvatarPose>` only)
        // and the motion resumes from exactly where it paused.
        //
        // Peers are never held; neither is the local body outside those
        // editor states.
        drive.hold = if locals.contains(child_of.parent()) {
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
/// driver takes — so it fires once without this having to clear it — and the
/// per-body cooldown that makes a peer pasting "hi hi hi hi" wave once is
/// [`symbios_avatar::anim::driver::DriverConfig::gesture_cooldown`], measured
/// on the driver's own clock. There is no missing-clip case either: every
/// [`crate::player::emote::Emote`] names an engine gesture, guarded by test
/// rather than checked per request.
pub(in crate::player) fn start_emotes(
    mut requests: MessageReader<EmoteRequest>,
    mut bodies: Query<(&ChildOf, &mut Drive), With<RiggedRoot>>,
) {
    for request in requests.read() {
        for (child_of, mut drive) in &mut bodies {
            if child_of.parent() != request.chassis {
                continue;
            }
            drive.gesture(request.emote.gesture_name());
        }
    }
}

/// Count the frames on which some body's contact solve strained (#1078).
///
/// **Per frame rather than per body**, so a crowd cannot inflate one defect —
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
