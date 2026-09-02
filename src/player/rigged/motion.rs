use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;
use bevy_symbios_avatar::{AvatarBody as BuiltBody, AvatarClosure, AvatarPose};
use symbios_avatar::anim::{Stage, Steps, gesture, transition};
use symbios_avatar::{Expression, Ground, Inertializer, Leap, Limb, Pose, Speed, Swim, Walk};

use crate::player::emote::EmoteRequest;
use crate::player::humanoid::{WaterState, humanoid_water_state};
use crate::state::LocalPlayer;
use crate::water::WaterSurfaces;

use super::{
    ActiveGesture, Airborne, BLEND_SECS, EMOTE_COOLDOWN, EMOTE_SECS, FALL_SPEED, IDLE_BELOW,
    LAUNCH_SPEED, LENGTHS_PER_STROKE, MotionSource, PACE_RESPONSE, RiggedMotion, RiggedRoot,
    SETTLE_SPEED,
};

/// Pose every built body from what its chassis is doing.
///
/// **The attachment-editing holds (#1062, #1106).** An attachment offset is
/// stored in its carrying joint's *rest* frame, so while the owner has the
/// in-world gizmo on a **whole worn prop** their own body is pinned to the
/// bind pose and the drag happens in the frame the record actually keeps.
/// The pin is a hard snap, not an inertial blend: a body still settling
/// would let a gizmo release land against a pose that is already gone.
///
/// A gizmo on a **part** of a worn item holds the body **where it stands**
/// instead: the part is detached at its current world pose and committed
/// back against its parent's pose, which may be any pose so long as it does
/// not move. Re-posing to rest here moved the parent out from under the
/// freshly detached part — selecting visibly shifted it (#1106). The pose
/// hold is a plain skip: nothing is inserted, so the last `AvatarPose`
/// stays applied (the joint writer runs on `Changed<AvatarPose>` only) and
/// the motion state resumes from exactly where it paused. Peers are never
/// held; neither is the local body outside those editor states.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(in crate::player) fn drive_rigged_motion(
    mut commands: Commands,
    time: Res<Time>,
    water: Option<Res<WaterSurfaces>>,
    mut bodies: Query<
        (Entity, &ChildOf, &BuiltBody, &Transform, &mut RiggedMotion),
        With<RiggedRoot>,
    >,
    chassis: Query<(&GlobalTransform, Option<&LinearVelocity>)>,
    locals: Query<(), With<LocalPlayer>>,
    avatar_editor: Option<Res<crate::ui::avatar::AvatarEditorState>>,
    mut metrics: Option<ResMut<crate::diagnostics::MetricsRegistry>>,
) {
    let delta = time.delta_secs();
    if delta <= 0.0 {
        return;
    }
    let editing_offsets = avatar_editor
        .as_deref()
        .is_some_and(|state| state.holds_rig_at_rest());
    let editing_parts = avatar_editor
        .as_deref()
        .is_some_and(|state| state.holds_rig_pose());
    // Whether ANY body strained a contact this frame (#1078) — a goal its
    // solver could not reach. Counted per frame rather than per body so a
    // crowd cannot inflate one defect, and read at the end of the loop.
    let mut strained = false;
    for (entity, child_of, body, root, mut motion) in &mut bodies {
        if editing_offsets && locals.contains(child_of.parent()) {
            hold_at_rest(&mut commands, entity, body, &mut motion, delta);
            continue;
        }
        if editing_parts && locals.contains(child_of.parent()) {
            // Held as it stands (#1106): no pose written, no state advanced.
            continue;
        }
        let Ok((transform, velocity)) = chassis.get(child_of.parent()) else {
            continue;
        };
        // Local chassis carry an avian velocity; remote peers are kinematic
        // playout, so their speed is read off the smoothed transform itself.
        let position = transform.translation();
        // Signed, not a magnitude: which WAY a body is going vertically is the
        // whole of the airborne state machine below, and `abs` threw it away.
        let (planar, vertical) = match velocity {
            Some(v) => (Vec2::new(v.0.x, v.0.z).length(), v.0.y),
            None => {
                let moved = motion
                    .last_position
                    .map_or(Vec3::ZERO, |last| (position - last) / delta);
                (Vec2::new(moved.x, moved.z).length(), moved.y)
            }
        };
        motion.last_position = Some(position);

        // **Airborne is a state, and it has to be** (#1072). Every
        // instantaneous test fails at the apex of a jump, where the vertical
        // speed is zero and the body is as airborne as it ever gets — which is
        // exactly what the old `|v_y| > 3.5` did, resuming the walk cycle and
        // planting the feet at the top of every jump.
        //
        // Two ways in and one way out. A body launches when something pushes
        // it up faster than the ground can, or falls when nothing is holding
        // it up; it lands when a body that HAS been falling stops falling,
        // which the apex cannot imitate because at the apex it is still on its
        // way down.
        let rig = &body.avatar.rig;

        // **In deep water before anything else** (#1074). The rigged root is
        // offset so `y = 0` is the chassis collider's bottom, which makes its
        // own transform the body's half-height — the same figure the
        // controller classifies with, so the animation and the physics cannot
        // disagree about whether this body is swimming.
        //
        // Only `Swimming` animates as one: a wading body has its feet on the
        // bottom and is walking, which is what the controller does with it too.
        let half_height = -root.translation.y;
        let swimming = water.as_ref().is_some_and(|water| {
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
        // A body in the water is not falling, whatever its vertical speed says
        // — and the preset swims upward at 1.8 m/s against a launch threshold
        // of 2.0, which is close enough that a peer's smoothed transform would
        // otherwise trip the leap.
        if swimming {
            motion.airborne = None;
        }

        let launched = vertical > LAUNCH_SPEED;
        let dropped = vertical < -FALL_SPEED;
        match &mut motion.airborne {
            None if swimming => {}
            None if launched || dropped => {
                // **Always a symmetric leap, and never a drop** (#1073). The
                // engine's `drop` says the floor arrived at is lower than the
                // one left, and carries that difference in the pose for the
                // whole landing — which is right for a body that owns its own
                // root and catastrophic here, where the chassis has already
                // carried it down: the two add, and the root goes underground
                // by the entire fall height.
                //
                // `Leap::new(speed)` is the same leap with the trajectory left
                // to the chassis. Built from the SPEED rather than the
                // direction, so a body that walks off a ledge gets a flight of
                // `2v/g` instead of the zero-length one `Leap::new(0.0)` has —
                // whose stage machine divides by an epsilon and reports a
                // landing from the first airborne frame, planting the feet in
                // mid-air all the way down.
                let leap = Leap::new(vertical.abs());
                motion.airborne = Some(Airborne {
                    leap,
                    // Past the wind-up: this app has no anticipation to spend
                    // on one — the impulse fires the fixed step after the key
                    // — so the leap is joined at the instant its feet leave.
                    elapsed: leap.wind_up(rig),
                    falling: vertical < -SETTLE_SPEED,
                    impact: vertical.min(0.0),
                    landed: false,
                });
            }
            Some(air) if !air.landed => {
                // Held inside the flight, because physics decides how long a
                // body is in the air and the leap only predicted it. A fall
                // that outlasts its prediction holds at the end of the arc,
                // where the tuck has returned to nothing and the legs are
                // straight — which is what a body reaching for the ground
                // does anyway.
                let flight_ends = air.leap.wind_up(rig) + air.leap.flight();
                air.elapsed = (air.elapsed + delta).min(flight_ends - f32::EPSILON);
                air.falling |= vertical < -SETTLE_SPEED;
                air.impact = air.impact.min(vertical);
                if air.falling && vertical >= -SETTLE_SPEED {
                    // Touchdown. The leap is rebuilt from the arrival that
                    // actually happened rather than the one predicted at
                    // takeoff, and its clock is set to the head of the
                    // landing — the only stage left to play. Symmetric again,
                    // so the depth is the impact's and the pose carries no
                    // trajectory (#1073).
                    let leap = Leap::new(air.impact.abs());
                    air.leap = leap;
                    air.elapsed = leap.wind_up(rig) + leap.flight();
                    air.landed = true;
                }
            }
            Some(air) => {
                air.elapsed += delta;
                // Leaving the ground again mid-landing is a new leap, not a
                // continuation of this one.
                if launched {
                    motion.airborne = None;
                }
            }
            None => {}
        }
        // A landing that has played out is over; the body stands up out of it.
        if motion
            .airborne
            .is_some_and(|air| air.leap.stage_at(rig, air.elapsed) == Stage::Standing)
        {
            motion.airborne = None;
        }
        let airborne = motion.airborne.is_some();

        // Pick the source and its cycle rate. Standing is the engine's idle,
        // travelling is the gait — and the gait's cadence is **derived from
        // the speed rather than bent toward it** (symbios-avatar #240): a
        // clip had one stride baked into it and could only be played faster
        // or slower; a generator asks the speed axis for the cadence the
        // stride it is about to take actually implies.
        // **The change of source is taken on the frame the world changes, and
        // #1071's handoff wait is deliberately NOT here.** The governor's rule
        // is that a transition should not begin while a contact is bearing
        // weight, and should hold for the next handoff — where support is
        // transferring anyway. Implemented and measured on a stop, it made
        // this worse, and the sweep says why: the skid a stop costs is how far
        // the blend must drag the PLANTED foot to reach the standing stance,
        // and that is smallest at midstance, where the stance foot is already
        // under the hip, and largest at a handoff, where the legs are at full
        // split. Waiting for the handoff steers the body from the best moment
        // to the worst.
        //
        // Measured, stopping from 1.4 m/s at every phase of the cycle, as the
        // furthest a foot that was bearing weight then slid: 15.4 mm stopping
        // near midstance and 203.0 at a handoff, worst 297.9 across the sweep;
        // with the wait, the phases it engaged on went 131.2 mm to 351.3 and
        // the worst rose to 351.3. See `a_stop_does_not_skate_further_than_it_
        // already_does`, which ratchets the figure, and #1071 for the write-up.
        // The eased pace (#1192): first-order lag toward the chassis' raw
        // speed, alive only while the gait is — see [`PACE_RESPONSE`]. The
        // gate below stays on the RAW speed on purpose, so a stop hands over
        // to the idle the frame the chassis stops (#1071's measured call) and
        // gait initiation starts from the crossing speed rather than zero.
        let paced = if planar >= IDLE_BELOW {
            let from = motion.paced.unwrap_or(planar);
            let eased = from + (planar - from) * (1.0 - (-delta / PACE_RESPONSE).exp());
            motion.paced = Some(eased);
            eased
        } else {
            motion.paced = None;
            planar
        };
        let travelling = (planar >= IDLE_BELOW).then(|| Speed::new(rig, paced));
        // A leap owns the legs for as long as it lasts: a body cannot be
        // mid-stride and mid-air at once, and pretending otherwise is how a
        // jump ends up with a walk cycle running underneath it (#1072).
        // **The stroke's clock, which the engine leaves to its caller** — a
        // tread and a crawl are the same loops at different rates, so the rate
        // is where the difference actually lives (#1074). Both ends are
        // derived: at speed one cycle carries the body [`LENGTHS_PER_STROKE`]
        // of its own length, and at rest a tread sculls at the body's own
        // pendulum frequency, `sqrt(g/L)/2pi`, which is the one rate a body of
        // this size has without anybody choosing it. A giant sculls slower for
        // the same reason it walks slower.
        let stroke_rate = || {
            let length = rig.extent().max(f32::EPSILON);
            let tread = (9.81 / length).sqrt() / std::f32::consts::TAU;
            (planar / (length * LENGTHS_PER_STROKE)).max(tread)
        };
        let (locomotion, cadence) = match (swimming, motion.airborne, travelling) {
            (true, _, _) => (MotionSource::Swim, stroke_rate()),
            (false, Some(_), _) => (MotionSource::Leap, 0.0),
            (false, None, Some(speed)) => (MotionSource::Gait, speed.cadence(rig)),
            (false, None, None) => (MotionSource::Idle, 0.0),
        };

        // **Hand the idle the stance the body arrived in** (#1071, engine
        // #276), before anything below moves the clock or forgets the speed.
        //
        // Left to itself the idle solves every contact back to its REST
        // position each frame, so a body that stops mid-stride drags the foot
        // that was bearing its weight up to a third of a metre across the
        // ground to close its stance. That is the skid this file has ratcheted
        // since #1071, and it is not a transition-timing problem: waiting for a
        // handoff and waiting for a midstance were both implemented here and
        // both measured WORSE than stopping immediately, because a body that
        // holds keeps striding while the chassis has already stopped.
        //
        // So the idle is told which contacts were carrying the body and where
        // they were. It pins those and recovers them through its own weight
        // shifts, which is the only way a foot moves without sliding.
        //
        // Read before the assignments below: `motion.gaiting` still holds the
        // speed the body was travelling at, `motion.cycle` still reads the
        // moment the last pose was DRAWN at, and `motion.source` still says
        // what the body was doing. All three are overwritten within twenty
        // lines.
        if motion.source == MotionSource::Gait
            && locomotion == MotionSource::Idle
            && let Some(before) = motion.gaiting
            && let Some(arrival) = motion.current.clone()
        {
            let gait = before.gait(rig);
            let bearing: Vec<Limb> = gait
                .limbs
                .iter()
                .enumerate()
                .filter(|(index, _)| gait.phase(*index, motion.cycle).is_stance())
                .map(|(_, &limb)| limb)
                .collect();
            motion.idler.arrive(rig, &arrival, &bearing);
        }

        // **Carry the CYCLE across a change of gait, not the number** (#1071,
        // engine #247). A cycle fraction means a different part of the step at
        // a different duty: the duty falls all the way along the speed axis
        // and STEPS at the walk-run transition, so a body that crosses it
        // mid-stride hands 0.5 to a gait where 0.5 is a different moment, and
        // a foot in mid-swing arrives planted. `phase_matched` maps it exactly
        // — the leading contact keeps the part of the step it was actually in.
        //
        // Asked every frame rather than at a detected boundary, because there
        // is no boundary to detect: the duty moves continuously as well as
        // stepping, and where it has not moved the map is the identity to the
        // bit. One `Gait` is rebuilt on the frames it has moved, which is a
        // pair of two-element vectors beside a solve that allocates poses.
        if let (Some(speed), Some(before)) = (travelling, motion.gaiting)
            && before != speed
        {
            let into = speed.gait(rig);
            let from = symbios_avatar::Gait {
                duty: before.duty(),
                ..into.clone()
            };
            motion.cycle = transition::carry_cycle(&from, &into, motion.cycle);
        }
        motion.gaiting = travelling;

        if !airborne || swimming {
            motion.cycle = (motion.cycle + delta * cadence).fract();
        }

        // Advance the emote and retire it at the end of its one play. Done
        // before the pose is built so a gesture that finished this frame does
        // not get one last frame of overlay.
        let gesture = motion.gesture.and_then(|mut active| {
            active.elapsed += delta;
            (active.elapsed < EMOTE_SECS).then_some(active)
        });
        motion.gesture = gesture;
        // A gesture is its own motion source, so ending one blends back into
        // the walk it was riding rather than snapping.
        let source = match gesture {
            Some(active) => MotionSource::Gesture(active.emote),
            None => locomotion,
        };

        // Build this frame's target pose.
        let mut pose = Pose::rest(rig);
        // The whole `Steps`, not just the stance list, since engine 0.5:
        // `Walk::settle` re-aims swing legs toward the goals `step` sent them
        // after the plant settles the pelvis, and `steps.placed` is how it
        // knows a limb is still the gait's to re-solve. A path that fills only
        // `stance` (the leap below) leaves `placed` empty, which is exactly
        // how it keeps the limbs it authored (bevy_symbios_avatar #40).
        let mut steps = Steps::default();
        // The floor under this body, in the rigged root's own frame: the root
        // is offset so `y = 0` is the chassis collider's bottom, which on a
        // standing body is the ground the physics chassis rests on. Slopes are
        // carried by the chassis pose, exactly as the collider itself handles
        // them. **One closure, given to both the stride and the plant** — the
        // gait seats its contacts on the same surface the footing solve settles
        // them onto, and handing the two different floors is what put the swing
        // arc through the hill upstream (#1069, symbios-avatar #221).
        let floor = |point: Vec3| Some(Ground::level(Vec3::new(point.x, 0.0, point.z)));
        // Kept past the match so the ankles can roll AFTER the plant: the plant
        // lays every sole flat and a roll applied before it is simply levelled
        // away, which is the order `examples/walkaudit` establishes upstream.
        let mut walking = None;
        // Always the LOCOMOTION pose, never the gesture: an emote is laid over
        // what the body is already doing rather than replacing it, so the legs
        // keep their walk and their contacts (#1068).
        //
        // The foothold ledger lives only while the gait does: the idle shifts
        // weight and fidgets feet on its own authority, so a hold surviving
        // an idle would serve a point the body has since walked away from.
        // (A held stance re-plants on stance ENTRY, so clearing here costs a
        // resumed walk nothing.) This is also the teleport reset the engine
        // asks for: every warp this app performs — portals, the respawn —
        // zeroes the chassis velocity, so the body passes through a non-gait
        // frame and the ledger clears here without any cross-system plumbing.
        // A warp that somehow keeps the body walking (a peer's network
        // correction) is the engine's own self-heal, one frame late.
        if locomotion != MotionSource::Gait {
            motion.footholds.reset();
        }
        match locomotion {
            MotionSource::Rest => {}
            MotionSource::Idle => {
                // **A body with nothing else to do stands and breathes**
                // (engine #246): breath, sway, a weight shift onto one foot,
                // and scheduled fidgets, all through `Idle::drive`, which
                // advances the schedule and poses every layer in one call —
                // a stage a caller has to remember is a stage a caller
                // forgets, and this file has the scar (#1069). The idle
                // solves and plants its own feet against the same level
                // floor the gait uses, so no settle tail runs for it below.
                motion.idler.drive(rig, &mut pose, delta);
            }
            MotionSource::Gait => {
                // **One number in, everything out** (symbios-avatar #240). The
                // gait pattern, its duty, the stride length, the foot lift and
                // the cadence above all come from how fast this body is
                // actually travelling, expressed as the Froude number so the
                // same relation fits a child, a giant and a quadruped.
                //
                // Three things follow that this file used to get wrong. The
                // body now takes a LONGER step when it moves faster instead of
                // the same step more often. It CHANGES GAIT on its own at the
                // Froude number where walking stops working, so a fast avatar
                // runs rather than speed-walking. And the trunk lean stops
                // being a constant: the lean is scaled by the stride against
                // the legs taking it, and that ratio was pinned at pace 1.0 —
                // it responds for free now, which is the whole of #239's
                // complaint against this file.
                let speed = travelling.unwrap_or(Speed::STILL);
                let gait = speed.gait(rig);
                let stride = speed.stride(rig);
                // The head of the engine's own drive sequence — step, arms,
                // lean — with the footing OFF, because an emote is laid over
                // this pose below and the feet have to be settled after that
                // rather than before (symbios-avatar #253). This file used to
                // spell the stages out and was one of the three consumers that
                // had forgotten the ankles entirely (#1069).
                //
                // **Through the ledger, not `Walk::drive` directly** (engine
                // #277): `Footholds::drive` runs the same sequence with each
                // planted contact held to the world point it went down at, fed
                // the transform the pose renders under. The body's world
                // forward is the engine's `+Z` carried through the chassis'
                // rotation AND the rigged root's half turn — which is the
                // chassis' own `-Z` (#1066) — and handing the ledger the
                // chassis' yaw raw would hold every plant mirrored through
                // the body.
                let cycle = motion.cycle;
                let forward = transform.rotation() * Vec3::NEG_Z;
                let walked = motion.footholds.drive(
                    Walk {
                        footing: None,
                        ..Walk::at(cycle)
                    },
                    rig,
                    &mut pose,
                    &gait,
                    &stride,
                    position,
                    forward.x.atan2(forward.z),
                    floor,
                );
                strained |= !walked.steps.straining.is_empty();
                steps = walked.steps;
                // The stride travels with the gait, because the tail of the
                // sequence needs it too: a turning body's ankles are turned to
                // face where they were planted, and `roll_feet` is where that
                // lands (engine #241).
                walking = Some((gait, stride));
            }
            MotionSource::Swim => {
                // **Nothing is added to `stance`**: a swimming body has
                // nothing on the ground, and handing the footing tail a
                // contact is what would drag its feet back down to a floor it
                // is nowhere near.
                //
                // No vertical correction either, and that is worth saying
                // beside the leap's, which needs one. `Swim` pitches the body
                // by rotating the ROOT joint — the pelvis — and the pelvis
                // sits about half a body height above the rigged root, which
                // is where the chassis capsule's centre is. So a body going
                // prone lies along the capsule rather than pivoting about its
                // own feet.
                Swim {
                    cycle: motion.cycle,
                    ..Swim::at(motion.cycle).toward(planar)
                }
                .drive(rig, &mut pose);
            }
            MotionSource::Leap => {
                // **The chassis owns the trajectory, so the flight's height is
                // given back** (#1072). `Leap::drive` carries the root through
                // the parabola itself, which is right for a body that owns its
                // own root and wrong here: avian is already moving the physics
                // capsule this body hangs off, and applying both flies it
                // twice. The wind-up's and the landing's heights are KEPT —
                // those are the legs compressing, which a capsule does not do.
                //
                // Taken off after the drive rather than by not asking for it,
                // because the drive plants the contacts at the height it
                // applied: in flight there is nothing to plant, so subtracting
                // afterward is exact, and on the ground there is nothing to
                // subtract.
                if let Some(air) = motion.airborne {
                    let leapt = air.leap.drive(rig, &mut pose, air.elapsed, floor);
                    strained |= !leapt.straining.is_empty();
                    if leapt.stage.is_grounded() {
                        steps.stance = rig.ground_contacts();
                    } else {
                        pose.translation.y -= leapt.height;
                    }
                }
            }
            // `locomotion` is never a gesture by construction — the gesture
            // is chosen below, from this.
            MotionSource::Gesture(_) => {}
        }
        // The emote rides over whatever the body is already doing. A
        // goal-space clip writes only the parts its own tracks address —
        // a wave is a hand, a nod is a gaze, a bow is a trunk and a gaze
        // (engine #248) — so the legs and the pelvis keep the walk or the
        // idle that carries them, and there is no joint mask here to
        // maintain: the clip's own vocabulary is the mask. Normalised time,
        // so the elapsed seconds are scaled by the one duration decision
        // ([`EMOTE_SECS`]).
        if let Some(active) = gesture
            && let Some(clip) = gesture::by_name(active.emote.gesture_name())
        {
            clip.apply(rig, &mut pose, active.elapsed / EMOTE_SECS);
        }
        // The leap keeps its own contacts: `Leap::drive` plants them itself
        // during a wind-up or a landing, and in flight there are none.
        if airborne && locomotion != MotionSource::Leap {
            steps.stance.clear();
        }
        // The tail of the drive: settle the contacts, then roll the ankles, in
        // that order — the engine owns both so this file cannot get the order
        // wrong again (symbios-avatar #253). Gait only: the idle plants its
        // own feet inside `Idle::drive`, and a resting body has nothing down.
        if let Some((gait, stride)) = &walking {
            let walked = Walk::at(motion.cycle).settle(rig, &mut pose, gait, stride, &steps, floor);
            strained |= walked.straining() > 0;
        }

        // Inertialize source switches so a walk does not snap into a stand —
        // but only where the two are different activities (#1071). Within
        // locomotion the answer is no: see [`MotionSource::family`].
        //
        // **Measured, because the obvious simplification here is wrong.** A
        // gesture looks like it should need no blend: every emote in the
        // roster is a goal-space clip that begins and ends at the body's own
        // rest offsets, so its POSITION is continuous at both ends. Its
        // velocity is not. Sampled one frame in at 60 fps, a greeting has
        // already moved a joint 74.5 mm and a refusal 101.9 — a seventh of
        // the whole gesture's travel, in one frame — and the same at the far
        // end. Dropping the blend there would put a visible snap on both ends
        // of every emote.
        if motion.source.needs_blend(source)
            && let (Some(previous), Some(current)) = (&motion.previous, &motion.current)
        {
            motion.transition = Some(Inertializer::start(
                previous, current, &pose, delta, BLEND_SECS,
            ));
        }
        motion.source = source;
        let mut posed = match &mut motion.transition {
            Some(transition) if !transition.finished() => {
                transition.advance(delta);
                transition.apply(&pose)
            }
            _ => {
                motion.transition = None;
                pose.clone()
            }
        };
        motion.previous = motion.current.take();
        motion.current = Some(posed.clone());

        // The blink rides after the blend, like the sibling viewer: smoothed
        // by a gait transition it reads as falling asleep.
        let closure = Expression::NEUTRAL.closure_at(motion.blink.advance(delta));
        if let Some(eyes) = body.avatar.parts.eyes.as_ref() {
            eyes.blink(&mut posed, closure);
        }
        commands
            .entity(entity)
            .insert((AvatarPose(posed), AvatarClosure(closure)));
    }
    if strained && let Some(metrics) = metrics.as_deref_mut() {
        crate::diagnostics::samplers::motion_strain_frame(metrics);
    }
}

/// Start emotes on the bodies their requests name (#1068).
///
/// Runs in the Animate set ahead of [`drive_rigged_motion`], so a gesture
/// requested this frame is posed this frame rather than a frame late.
///
/// **A request names a chassis and this finds the body under it**, because the
/// chat and network layers hold chassis entities and know nothing of rigged
/// roots. Two ways a request is dropped, both silent and both intended: the
/// chassis has no rigged body (a boat has nothing to wave with), or the body
/// is inside its cooldown. There is no missing-clip case any more — every
/// [`crate::player::emote::Emote`] names an engine gesture, and the pairing is guarded by test
/// rather than checked per request.
pub(in crate::player) fn start_emotes(
    mut requests: MessageReader<EmoteRequest>,
    time: Res<Time>,
    mut bodies: Query<(&ChildOf, &mut RiggedMotion), With<RiggedRoot>>,
) {
    let now = time.elapsed_secs();
    for request in requests.read() {
        for (child_of, mut motion) in &mut bodies {
            if child_of.parent() != request.chassis {
                continue;
            }
            if motion
                .gestured_at
                .is_some_and(|last| now - last < EMOTE_COOLDOWN)
            {
                continue;
            }
            motion.gesture = Some(ActiveGesture {
                emote: request.emote,
                elapsed: 0.0,
            });
            motion.gestured_at = Some(now);
        }
    }
}

/// Pin one body to the bind pose for this frame — the attachment-editing
/// hold documented on [`drive_rigged_motion`].
///
/// Deliberately bypasses the [`Inertializer`]: the point is that the joint
/// entities sit *exactly* where `symbios_avatar::Pose::rest` puts them, which
/// is the frame [`crate::player::attachments::LocalAttachment::rest_frame`]
/// reconstructs a released gizmo pose against. `previous`/`current` are still
/// kept up to date, so releasing the hold blends back out of rest normally.
/// The blink rides along — an eyelid is not a socket, and a body that stops
/// blinking reads as broken rather than as held.
fn hold_at_rest(
    commands: &mut Commands,
    entity: Entity,
    body: &BuiltBody,
    motion: &mut RiggedMotion,
    delta: f32,
) {
    let mut posed = Pose::rest(&body.avatar.rig);
    let closure = Expression::NEUTRAL.closure_at(motion.blink.advance(delta));
    if let Some(eyes) = body.avatar.parts.eyes.as_ref() {
        eyes.blink(&mut posed, closure);
    }
    motion.source = MotionSource::Rest;
    motion.transition = None;
    motion.previous = motion.current.take();
    motion.current = Some(posed.clone());
    commands
        .entity(entity)
        .insert((AvatarPose(posed), AvatarClosure(closure)));
}
