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
use super::common::{HAIR_SALT, seed_choice, shade};

pub(super) fn head(ctx: &PartCtx) -> Generator {
    let bp = &ctx.blueprint;
    let r = bp.head_r;
    // Feature scale — facial features were authored against the old fixed
    // r = 0.13 skull; keep them proportional on every tier's head.
    let k = r / 0.13;
    let skin = ctx.materials.skin(ctx.palette.skin_tone);
    let hair = ctx.materials.cloth(ctx.palette.hair_color);
    let eye = ctx.materials.cloth(ctx.palette.eye_color);
    let sclera = ctx.materials.cloth([0.9, 0.9, 0.88]);

    // Skull: a base sphere with a narrower jaw so the head reads as a face with
    // a chin. The hair (not a skin dome) provides the top silhouette, so no bare
    // skull-cap can show above the hairline.
    let mut head = prim(sphere(r, 4, skin.clone()), [0.0, 0.0, 0.0], id_quat());
    let mut jaw = prim(
        sphere(r * 0.78, 3, skin.clone()),
        [0.0, -r * 0.42, -r * 0.16],
        id_quat(),
    );
    jaw.transform.scale = Fp3([0.96, 0.92, 1.02]);
    head.children.push(jaw);

    // Neck — a short thick column (a thin bare cylinder is the weakest read
    // on a primitive figure), flaring at its base (trapezius) so it rises
    // from the shoulders instead of floating. It spans chin→collar with a
    // sink into both so no seam shows; Toy-tier necks are near-zero and the
    // head seats straight onto the shoulders.
    let neck_l = bp.neck_len + 0.10;
    head.children.push(prim(
        with_torture(
            cylinder(bp.neck_r * 1.25, neck_l, 10, skin.clone()),
            0.0,
            0.35,
            [0.0, 0.0, 0.0],
        ),
        [0.0, -(r * 1.12 - 0.05 + neck_l * 0.5), 0.0],
        id_quat(),
    ));

    // Eyes + brows. The face is on -Z (the assembler never turns the head).
    for s in [-1.0f32, 1.0] {
        // White sclera in a shallow socket with a smaller dark iris in front,
        // so each eye reads as an eye instead of merging with the brow into a
        // single dark bar (the old same-tone eye+brow pairing).
        let mut socket = prim(
            sphere(0.028 * k, 2, sclera.clone()),
            [s * r * 0.37, r * 0.0, -r * 0.88],
            id_quat(),
        );
        socket.children.push(prim(
            sphere(0.017 * k, 2, eye.clone()),
            [0.0, 0.0, -0.018 * k],
            id_quat(),
        ));
        head.children.push(socket);
        // Brow — thin and lifted clear of the eye.
        head.children.push(prim(
            cuboid(
                [0.046 * k, (0.010 * k).max(0.01), (0.018 * k).max(0.01)],
                hair.clone(),
            ),
            [s * r * 0.37, r * 0.28, -r * 0.92],
            id_quat(),
        ));
    }
    // Nose nub + mouth.
    head.children.push(prim(
        cuboid([0.026 * k, 0.04 * k, 0.05 * k], skin.clone()),
        [0.0, -r * 0.10, -r * 0.96],
        id_quat(),
    ));
    head.children.push(prim(
        cuboid(
            [0.055 * k, (0.016 * k).max(0.01), (0.02 * k).max(0.01)],
            ctx.materials.cloth(shade(ctx.palette.skin_tone, 0.5)),
        ),
        [0.0, -r * 0.44, -r * 0.88],
        id_quat(),
    ));
    // Ears.
    for s in [-1.0f32, 1.0] {
        head.children.push(prim(
            sphere(0.022 * k, 2, skin.clone()),
            [s * (r + 0.004), -r * 0.02, r * 0.02],
            id_quat(),
        ));
    }

    // Hair — a crown cap covering the whole top of the head, tilted backward so
    // its front rim lifts to the upper forehead (a clean hairline) while the
    // crown stays fully covered (no bare skull-cap shows); a back/nape mass and
    // temples frame the face. Reads as a haircut, not a swim cap. A per-seed
    // flourish adds variety on top.
    // NB: a single profile-cut dome was trialled here and rejected — its one
    // flat rim can't both expose the forehead and cover the nape, and it
    // leaves a seam against the back mass; the multi-mass below reads as hair
    // far better. The cut prims earn their keep in the catalogue.
    let mut cap = prim(
        sphere(r, 4, hair.clone()),
        [0.0, r * 0.68, r * 0.06],
        quat_xyzw(quat_x(-0.30)),
    );
    cap.transform.scale = Fp3([1.08, 0.66, 1.18]);
    head.children.push(cap);
    // Back/nape mass bridging the dome down to the neck so no skin shows behind.
    let mut back = prim(
        sphere(r * 0.85, 3, hair.clone()),
        [0.0, r * 0.05, r * 0.42],
        id_quat(),
    );
    back.transform.scale = Fp3([1.12, 1.05, 0.85]);
    head.children.push(back);
    // Temples framing the face, tucked back clear of the eyes.
    for s in [-1.0f32, 1.0] {
        head.children.push(prim(
            sphere(r * 0.32, 2, hair.clone()),
            [s * r * 0.84, r * 0.1, r * 0.12],
            id_quat(),
        ));
    }
    // A hat clips the long-hair / tuft flourishes, so only add one bare-headed.
    // Six styles spread the bare-headed population so seeded avatars vary.
    if !ctx.has_hat {
        match seed_choice(ctx.seed, HAIR_SALT, 6) {
            0 => {} // cropped — crown only
            1 => {
                // Long hair falling down the back (+Z is behind the face).
                head.children.push(prim(
                    cuboid([r * 1.5, r * 2.0, 0.05 * k], hair),
                    [0.0, -r * 0.7, r * 0.62],
                    id_quat(),
                ));
            }
            2 => {
                // Topknot tuft.
                head.children.push(prim(
                    sphere(r * 0.42, 3, hair),
                    [0.0, r * 1.15, r * 0.05],
                    id_quat(),
                ));
            }
            3 => {
                // Ponytail — a small tie at the back of the crown + a tail
                // dropping behind the nape.
                head.children.push(prim(
                    sphere(r * 0.3, 2, hair.clone()),
                    [0.0, r * 0.55, r * 0.6],
                    id_quat(),
                ));
                head.children.push(prim(
                    capsule(r * 0.32, r * 1.5, hair),
                    [0.0, -r * 0.3, r * 0.66],
                    id_quat(),
                ));
            }
            4 => {
                // Bun gathered at the back of the crown.
                let mut bun = prim(
                    sphere(r * 0.5, 3, hair),
                    [0.0, r * 0.85, r * 0.5],
                    id_quat(),
                );
                bun.transform.scale = Fp3([1.0, 0.92, 1.0]);
                head.children.push(bun);
            }
            _ => {
                // Swept fringe — a forelock angled across the upper brow.
                head.children.push(prim(
                    cuboid([r * 1.45, r * 0.45, r * 0.5], hair),
                    [r * 0.18, r * 0.52, -r * 0.72],
                    id_quat(),
                ));
            }
        }
    }
    head
}

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
    // Collar ring at the neckline.
    torso.children.push(prim(
        torus(0.02, bp.neck_r * 1.35, collar.clone()),
        [0.0, yoke_y + chest_r * 0.30, 0.0],
        id_quat(),
    ));
    // Centre placket down the chest — a narrow front seam (reads as a button
    // line, not a panel), seated just clear of and below the pfp badge.
    torso.children.push(prim(
        cuboid([chest_r * 0.14, len * 0.44, 0.015], collar),
        [0.0, -len * 0.04, -(chest_r + 0.006)],
        id_quat(),
    ));
    // Belt at the waist — gives the trunk a waistline instead of a smooth tube.
    torso.children.push(prim(
        torus(0.02, waist_r * 1.02, belt),
        [0.0, -len * 0.42, 0.0],
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
    // Lapel V — two lining-colour strips angled outward at the throat, meeting
    // low on the chest, so the coat reads as open-collared over a shirt.
    for s in [-1.0f32, 1.0] {
        torso.children.push(prim(
            cuboid([0.03, len * 0.65, 0.02], lining.clone()),
            [s * chest_r * 0.30, len * 0.06, -(chest_r + 0.005)],
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
        torso.children.push(prim(
            sphere(0.014, 2, btn.clone()),
            [0.0, len * (0.20 - 0.24 * i as f32), -(chest_r + 0.012)],
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
        sphere(r * 0.95, 3, sleeve.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    cap.transform.scale = Fp3([0.92, 0.55, 0.82]);
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
        [0.0, -l1 * 0.5, 0.0],
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
