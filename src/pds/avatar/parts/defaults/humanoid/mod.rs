//! Humanoid defaults: head / torso / coat / arm / leg. Built in each slot's local attachment frame — see the module
//! docstring on [`super::super`] (`parts`).
//!
//! Every dimension comes from the seeded [`HumanoidBlueprint`](crate::seeded_defaults::HumanoidBlueprint)
//! (`ctx.blueprint`) so the parts, the assembler's anchors, and the
//! locomotion capsule share one proportion contract: canon landmark ratios
//! (wrist at the crotch line, legs ~half the figure, shoulder span in
//! head-heights, distal limb taper) banded by the avatar's
//! [`StylizationTier`](crate::seeded_defaults::StylizationTier).

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::default_visuals::common::{
    capsule, cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, quat_z, sphere, torus, with_shape,
    with_torture,
};
use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;

use super::super::PartCtx;
use super::common::shade;

mod hair;
mod head;

pub(super) use head::head;

/// Trunk shaping shared by [`torso`] and [`coat`]: a capsule authored at the
/// waist radius whose torture-flare widens the top to the chest radius (the
/// athletic V), plus the whole part flattened front-to-back (`bp.depth`) so
/// the body is wider than deep instead of a barrel. Children authored on the
/// trunk surface inherit the root's Z-squash and stay flush.
fn trunk(ctx: &PartCtx, chest_r: f32, shell: SovereignMaterialSettings) -> Generator {
    let bp = &ctx.blueprint;
    // Same waist:chest ratio whatever chest the caller passes (the coat
    // trunk is a touch bulkier than the shirt's).
    let waist_r = chest_r * (bp.waist_r / bp.chest_r);
    let flare = -(chest_r / waist_r - 1.0);
    let mut t = prim(
        with_torture(
            capsule(waist_r, bp.trunk_len, shell),
            0.0,
            flare,
            [0.0, 0.0, 0.0],
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    t.transform.scale = Fp3([1.0, 1.0, bp.depth]);
    t
}

pub(super) fn torso(ctx: &PartCtx) -> Generator {
    let bp = &ctx.blueprint;
    let chest_r = bp.chest_r;
    let waist_r = bp.waist_r;
    let len = bp.trunk_len;
    let shirt = ctx.materials.body(ctx.palette.primary_accent);
    let collar = ctx.materials.trim(ctx.palette.secondary_accent);
    let belt = ctx.materials.trim(ctx.palette.tertiary_accent);

    // V-tapered trunk, flattened front-to-back (see [`trunk`]).
    let mut torso = trunk(ctx, chest_r, shirt.clone());

    // Shoulder yoke — a wide flattened ellipsoid laid across the top of the
    // trunk, sloping down to the arm mounts (real shoulders; the arm parts
    // carry only a small deltoid cap). Sized so its tips reach past the arm
    // mounts and its slope stays *above* the deltoid caps — the old smaller
    // yoke left the caps cresting the trunk line as back-view "horns".
    let yoke_y = bp.shoulder_y - bp.torso_y;
    let mut yoke = prim(sphere(1.0, 3, shirt.clone()), [0.0, yoke_y, 0.0], id_quat());
    yoke.transform.scale = Fp3([
        bp.shoulder_x + bp.arm_r * 0.7,
        chest_r * 0.45,
        chest_r * 0.92,
    ]);
    torso.children.push(yoke);
    // Chest + upper-back masses: the trunk capsule alone is a straight
    // sausage in profile — these two ellipsoids give it a chest-out /
    // shoulder-blade curve (children of the depth-squashed root, so they
    // follow the flattening).
    let mut chest = prim(
        sphere(1.0, 3, shirt.clone()),
        [0.0, yoke_y * 0.42, -chest_r * 0.45],
        id_quat(),
    );
    chest.transform.scale = Fp3([chest_r * 0.86, len * 0.34, chest_r * 0.62]);
    torso.children.push(chest);
    let mut back = prim(
        sphere(1.0, 3, shirt.clone()),
        [0.0, yoke_y * 0.62, chest_r * 0.5],
        id_quat(),
    );
    back.transform.scale = Fp3([chest_r * 0.86, len * 0.26, chest_r * 0.42]);
    torso.children.push(back);
    // Collar ring at the neckline.
    torso.children.push(prim(
        torus(0.02, bp.neck_r * 1.35, collar.clone()),
        [0.0, yoke_y + chest_r * 0.30, 0.0],
        id_quat(),
    ));
    // Centre placket — a short front seam on the *chest mass* (its front
    // is near-vertical, so the strip stays flush where the tapering trunk
    // would let it float).
    torso.children.push(prim(
        cuboid([chest_r * 0.13, len * 0.30, 0.015], collar),
        [0.0, yoke_y * 0.30, -(chest_r * 1.02 + 0.006)],
        id_quat(),
    ));
    // Belt at the waist — gives the trunk a waistline instead of a smooth tube.
    torso.children.push(prim(
        torus(0.02, waist_r * 1.02, belt),
        [0.0, -len * 0.42, 0.0],
        id_quat(),
    ));
    // Shirt hem — a soft flare ring at the trunk bottom breaking the
    // torso-into-trousers column.
    torso.children.push(prim(
        torus(
            0.025,
            waist_r * 1.0,
            ctx.materials.body(shade(ctx.palette.primary_accent, 0.82)),
        ),
        [0.0, -len * 0.5, 0.0],
        id_quat(),
    ));
    torso
}

/// A second universal torso — a buttoned coat (stand collar, lapel V, button
/// row) so the bare-required-slot population isn't all the plain shirt. Builds
/// to the same centred frame + shoulder yoke as [`torso`], so the assembler
/// mounts arms / head identically.
pub(super) fn coat(ctx: &PartCtx) -> Generator {
    let bp = &ctx.blueprint;
    // A slightly straighter, bulkier trunk than the shirt (boxier coat).
    let chest_r = bp.chest_r * 1.04;
    let len = bp.trunk_len;
    let shell = ctx.materials.body(ctx.palette.primary_accent);
    let lining = ctx.materials.cloth(shade(ctx.palette.primary_accent, 0.6));
    let collar = ctx.materials.trim(ctx.palette.secondary_accent);
    let btn = ctx.materials.trim(ctx.palette.tertiary_accent);

    let mut torso = trunk(ctx, chest_r, shell.clone());
    // Shoulder yoke (as the shirt) — a touch broader for the coat's bulk.
    let yoke_y = bp.shoulder_y - bp.torso_y;
    let mut yoke = prim(sphere(1.0, 3, shell.clone()), [0.0, yoke_y, 0.0], id_quat());
    yoke.transform.scale = Fp3([
        bp.shoulder_x + bp.arm_r * 0.8,
        chest_r * 0.47,
        chest_r * 0.95,
    ]);
    torso.children.push(yoke);
    // Chest / upper-back masses (see [`torso`]) — the coat wears them a
    // touch fuller.
    let mut chest = prim(
        sphere(1.0, 3, shell.clone()),
        [0.0, yoke_y * 0.42, -chest_r * 0.45],
        id_quat(),
    );
    chest.transform.scale = Fp3([chest_r * 0.9, len * 0.36, chest_r * 0.64]);
    torso.children.push(chest);
    let mut back = prim(
        sphere(1.0, 3, shell.clone()),
        [0.0, yoke_y * 0.62, chest_r * 0.5],
        id_quat(),
    );
    back.transform.scale = Fp3([chest_r * 0.9, len * 0.28, chest_r * 0.44]);
    torso.children.push(back);
    // Lapel V — two lining-colour strips angled outward at the throat,
    // seated on the chest mass so they stay flush on tapered trunks.
    for s in [-1.0f32, 1.0] {
        torso.children.push(prim(
            cuboid([0.03, len * 0.55, 0.02], lining.clone()),
            [s * chest_r * 0.30, len * 0.14, -(chest_r * 1.02 + 0.005)],
            quat_xyzw(quat_z(s * 0.35)),
        ));
    }
    // Stand collar — a short ring standing at the neckline.
    torso.children.push(prim(
        cylinder(bp.neck_r * 1.5, 0.09, 12, collar),
        [0.0, yoke_y + chest_r * 0.30, 0.0],
        id_quat(),
    ));
    // Button row down the centre.
    for i in 0..3 {
        let by = len * (0.20 - 0.24 * i as f32);
        torso.children.push(prim(
            sphere(0.014, 2, btn.clone()),
            [
                0.0,
                by,
                -(bp.trunk_radius_at(bp.torso_y + by).max(chest_r * 0.9) + 0.014),
            ],
            id_quat(),
        ));
    }
    // Belt at the waist.
    torso.children.push(prim(
        torus(0.022, bp.waist_r * 1.06, btn),
        [0.0, -len * 0.42, 0.0],
        id_quat(),
    ));
    torso
}

pub(super) fn arm(ctx: &PartCtx) -> Generator {
    let bp = &ctx.blueprint;
    let r = bp.arm_r;
    let (l1, l2) = (bp.upper_arm, bp.forearm); // upper arm, forearm
    // Distal taper: shoulder girth → elbow → wrist (the canon
    // thick-to-thin rhythm; the hand then flares back out).
    let elbow_r = r * 0.88;
    let wrist_r = r * bp.limb_taper;
    let theta = 0.18_f32; // gentle elbow bend forward (front is -Z) — relaxed
    let skin = ctx.materials.skin(ctx.palette.skin_tone);
    let sleeve = ctx.materials.body(ctx.palette.primary_accent);
    let cuff = ctx.materials.trim(ctx.palette.secondary_accent);

    // A true kinematic chain: shoulder → upper arm → elbow → forearm → hand,
    // each segment a *child* of the one above and pinned to its parent's far
    // end, so a joint rotation propagates down the chain (the assembler's
    // shoulder splay swings the whole arm; the elbow bend swings forearm +
    // hand together) and each segment is authored in its own local frame.

    // Shoulder root = the pivot the assembler rotates the whole arm about.
    // It must carry NO scale — node scale propagates down the Bevy child
    // hierarchy, and the old scaled-cap root silently squashed the entire
    // arm chain to 62 % of its authored length. The root is a small ball
    // swallowed by the yoke; the visible deltoid shape is a flattened cap
    // *leaf* child (shirt colour, kept flat so it rounds the shoulder off
    // under the yoke's slope instead of cresting it as a back-view horn).
    let mut shoulder = prim(
        sphere(r * 0.75, 3, sleeve.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    let mut cap = prim(
        sphere(r * 1.05, 3, sleeve.clone()),
        [0.0, -r * 0.05, 0.0],
        id_quat(),
    );
    cap.transform.scale = Fp3([0.95, 0.62, 0.88]);
    shoulder.children.push(cap);

    // Upper arm (shoulder → elbow): bare-skin capsule centred at -l1/2, a
    // direct child of the shoulder, tapering down to the elbow.
    let mut upper = prim(
        with_torture(
            capsule(elbow_r, l1, skin.clone()),
            0.0,
            -(r / elbow_r - 1.0),
            [0.0, 0.0, 0.0],
        ),
        [0.0, -l1 * 0.5 - r * 0.30, 0.0],
        id_quat(),
    );
    // Short sleeve cap over the top of the upper arm (child, in upper-local).
    upper.children.push(prim(
        capsule(r * 1.08, l1 * 0.5, sleeve),
        [0.0, l1 * 0.22, 0.0],
        id_quat(),
    ));

    // Elbow node: child of the upper arm, seated at its far end (upper-local
    // -l1/2) and carrying the forward bend. Everything below pivots here.
    let mut elbow = prim(
        sphere(elbow_r, 2, skin.clone()),
        [0.0, -l1 * 0.5, 0.0],
        quat_xyzw(quat_x(theta)),
    );
    // Forearm (elbow → wrist): child of the elbow, centred at -l2/2 in the
    // (already bent) elbow frame, tapering to the wrist.
    let mut forearm = prim(
        with_torture(
            capsule(wrist_r, l2, skin.clone()),
            0.0,
            -(elbow_r / wrist_r - 1.0),
            [0.0, 0.0, 0.0],
        ),
        [0.0, -l2 * 0.5, 0.0],
        id_quat(),
    );
    // Wrist cuff at the forearm's far end.
    forearm.children.push(prim(
        cylinder(wrist_r * 1.12, 0.03, 8, cuff),
        [0.0, -l2 * 0.5, 0.0],
        id_quat(),
    ));
    // Hand: palm + a cupped finger block just past the wrist, sized from the
    // blueprint's hand length (canon: hand ≈ face; small hands read doll-
    // like). Kept left/right symmetric — the assembler mirrors the single
    // arm by rotation, not reflection, so an offset thumb would face the
    // wrong way on one side.
    let hand = bp.hand_len;
    // Narrow slab, not a plank: canon hand *length* ≈ face, but width is
    // well under half of that, and the block needs real thickness or it
    // reads as a paddle.
    let hand_w = hand * 0.40;
    let hand_d = (wrist_r * 1.6).min(hand_w * 0.7);
    let mut palm = prim(
        cuboid([hand_w, hand * 0.52, hand_d], skin.clone()),
        [0.0, -l2 * 0.5 - hand * 0.26, 0.0],
        id_quat(),
    );
    // Finger block tapers toward the tips (a mitten, not a box). The block
    // hangs -Y, so the taper runs on the *flipped* prim: author it upside
    // down (taper draws the top in) and rotate π about X to point it down.
    let mut fingers = prim(
        with_shape(
            cuboid([hand_w * 0.92, hand * 0.40, hand_d * 0.75], skin),
            [0.35, 0.30],
            [0.0, 0.0, 0.0],
            [0.0, 0.0],
        ),
        [0.0, -hand * 0.42, -hand_d * 0.08],
        quat_xyzw(quat_x(std::f32::consts::PI)),
    );
    fingers.transform.scale = Fp3([1.0, 1.0, 1.0]);
    palm.children.push(fingers);
    forearm.children.push(palm);
    elbow.children.push(forearm);
    upper.children.push(elbow);
    shoulder.children.push(upper);
    shoulder
}

pub(super) fn leg(ctx: &PartCtx) -> Generator {
    let bp = &ctx.blueprint;
    // Girth is authored at the knee; the thigh flares up from it and the
    // shin tapers down to the ankle (continuous taper instead of the old
    // stepped-radius segments that read as stacked bamboo).
    let r = bp.leg_r;
    let hip_r = r * 1.12;
    let ankle_r = r * bp.limb_taper;
    let (l1, l2) = (bp.thigh, bp.shin);
    let theta = 0.13_f32; // knee bend forward — strong enough to read in profile
    // Trousers: a darker shade of the primary so legs read as one outfit with
    // the shirt rather than a clashing accent.
    let trousers = ctx.materials.body(shade(ctx.palette.primary_accent, 0.6));
    let shoe = ctx.materials.body(ctx.palette.secondary_accent);

    // Kinematic chain mirroring the arm: hip → thigh → knee → shin → foot,
    // each segment a child pinned to its parent's far end, so the knee bend
    // carries the shin + foot together.

    // Hip root = hip joint at the origin (the assembler's hip pivot). Kept
    // barely wider than the thigh so it doesn't bulge through the pelvis.
    let mut hip = prim(
        sphere(hip_r * 1.02, 2, trousers.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Thigh (hip → knee), flaring up toward the hip.
    let mut thigh = prim(
        with_torture(
            capsule(r, l1, trousers.clone()),
            0.0,
            -(hip_r / r - 1.0),
            [0.0, 0.0, 0.0],
        ),
        [0.0, -l1 * 0.5, 0.0],
        id_quat(),
    );
    // Knee node: child of the thigh, at its far end, carrying the forward bend.
    let mut knee = prim(
        sphere(r * 0.98, 2, trousers.clone()),
        [0.0, -l1 * 0.5, 0.0],
        quat_xyzw(quat_x(theta)),
    );
    // Shin (knee → ankle): child of the knee, centred at -l2/2 in the bent
    // frame, tapering to the ankle.
    let mut shin = prim(
        with_torture(
            capsule(ankle_r, l2, trousers.clone()),
            0.0,
            -(r * 0.95 / ankle_r - 1.0),
            [0.0, 0.0, 0.0],
        ),
        [0.0, -l2 * 0.5, 0.0],
        id_quat(),
    );
    // Trouser cuff at the ankle.
    shin.children.push(prim(
        cylinder(ankle_r * 1.14, 0.04, 8, trousers),
        [0.0, -l2 * 0.5 + 0.02, 0.0],
        id_quat(),
    ));
    // Shoe — a forward-pointing shoe at the ankle (-Z is the front), sized
    // from the blueprint's foot length so the figure reads planted: a thin
    // dark sole biased forward (so it doesn't jut behind the heel) carrying
    // a single rounded upper (a capsule laid along the foot) + a toe cap.
    // Child of the shin (so it tracks the knee bend), its upper seated high
    // enough to swallow the shin/ankle seam.
    let foot_l = bp.foot_len;
    let sole_drop = 0.015 + 0.09 * bp.head_unit;
    let sole = ctx
        .materials
        .metal(shade(ctx.palette.secondary_accent, 0.45));
    let mut foot = prim(
        cuboid([ankle_r * 2.1, 0.03, foot_l], sole),
        [0.0, -l2 * 0.5 - sole_drop, -foot_l * 0.37],
        id_quat(),
    );
    // Rounded upper laid horizontally along the foot (capsule axis Y → Z),
    // seated low so it swallows the thin sole rather than perching on it.
    let mut upper = prim(
        capsule(ankle_r * 1.15, foot_l * 0.5, shoe.clone()),
        [0.0, ankle_r * 0.8, -foot_l * 0.16],
        quat_xyzw(quat_x(FRAC_PI_2)),
    );
    upper.transform.scale = Fp3([1.1, 1.0, 1.0]);
    foot.children.push(upper);
    // Toe cap rounding the front.
    let mut toe = prim(
        sphere(ankle_r * 1.05, 2, shoe),
        [0.0, ankle_r * 0.85, -foot_l * 0.63],
        id_quat(),
    );
    toe.transform.scale = Fp3([1.25, 0.85, 1.0]);
    foot.children.push(toe);
    shin.children.push(foot);
    knee.children.push(shin);
    thigh.children.push(knee);
    hip.children.push(thigh);
    hip
}
