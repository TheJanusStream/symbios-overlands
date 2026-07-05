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
    blob_ellipsoid, blob_group, capsule, cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw,
    quat_z, sphere, spine, torus, with_shape,
};
use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;

use super::super::PartCtx;
use super::common::shade;

mod hair;
mod head;

pub(super) use head::head;

/// Trunk shaping shared by [`torso`] and [`coat`] (#690): one BlobGroup
/// whose blended elements are the waist column, chest-out and upper-back
/// masses, and the shoulder yoke — a single seamless skin where the old
/// capsule + bolted-ellipsoid stack showed intersection seams ("the trunk
/// capsule alone is a straight sausage in profile"). The front-to-back
/// flattening (`bp.depth`) is baked into every element's Z semi-axis, so
/// the root carries NO scale — surface children no longer inherit a
/// Z-squash (multiply authored Z offsets by `bp.depth` instead), and the
/// old root-scale squash trap is gone. `fullness` bulks the coat trunk.
fn trunk(
    ctx: &PartCtx,
    chest_r: f32,
    fullness: f32,
    shell: SovereignMaterialSettings,
) -> Generator {
    let bp = &ctx.blueprint;
    // Same waist:chest ratio whatever chest the caller passes (the coat
    // trunk is a touch bulkier than the shirt's).
    let waist_r = chest_r * (bp.waist_r / bp.chest_r);
    let len = bp.trunk_len;
    let d = bp.depth;
    let yoke_y = bp.shoulder_y - bp.torso_y;
    let blend = chest_r * 0.45;
    let mid_r = (waist_r + chest_r) * 0.5;
    let elements = vec![
        // Waist / hip column.
        blob_ellipsoid(
            [0.0, -len * 0.30, 0.0],
            [waist_r, len * 0.32, waist_r * d],
            id_quat(),
            blend,
        ),
        // Mid column keeping the waist→chest V continuous.
        blob_ellipsoid(
            [0.0, len * 0.02, 0.0],
            [mid_r * 0.96, len * 0.30, mid_r * 0.85 * d],
            id_quat(),
            blend,
        ),
        // Chest-out mass — positioned to reproduce the old bolted
        // ellipsoid's front surface exactly (assembler-mounted chest
        // accessories seat against it).
        // The Y semi-axis reaches down past the assembler's ornament line
        // (torso_y - 0.04·len), where mounted pins expect a front surface
        // of at least 0.92·chest_r — the blob must keep that contract.
        blob_ellipsoid(
            [0.0, yoke_y * 0.42, -chest_r * 0.45 * d],
            [chest_r * 0.92 * fullness, len * 0.42, chest_r * 0.62 * d],
            id_quat(),
            blend,
        ),
        // Upper-back / shoulder-blade mass (same back-surface contract).
        blob_ellipsoid(
            [0.0, yoke_y * 0.62, chest_r * 0.50 * d],
            [chest_r * 0.88 * fullness, len * 0.30, chest_r * 0.42 * d],
            id_quat(),
            blend,
        ),
        // Shoulder yoke sloping down to the arm mounts.
        blob_ellipsoid(
            [0.0, yoke_y, 0.0],
            [
                bp.shoulder_x + bp.arm_r * 0.7 * fullness,
                chest_r * 0.46,
                chest_r * 0.90 * d,
            ],
            id_quat(),
            blend * 0.8,
        ),
    ];
    prim(blob_group(elements, 40, shell), [0.0, 0.0, 0.0], id_quat())
}

pub(super) fn torso(ctx: &PartCtx) -> Generator {
    let bp = &ctx.blueprint;
    let chest_r = bp.chest_r;
    let waist_r = bp.waist_r;
    let len = bp.trunk_len;
    let shirt = ctx.materials.body(ctx.palette.primary_accent);
    let collar = ctx.materials.trim(ctx.palette.secondary_accent);
    let belt = ctx.materials.trim(ctx.palette.tertiary_accent);

    // One blended trunk skin: waist column + chest/back masses + shoulder
    // yoke as BlobGroup elements (see [`trunk`]). The root carries no
    // scale, so surface children bake `bp.depth` into their Z offsets and
    // squash themselves with leaf scale where they wrap the trunk.
    let mut torso = trunk(ctx, chest_r, 1.0, shirt);
    let yoke_y = bp.shoulder_y - bp.torso_y;
    let d = bp.depth;

    // Collar ring at the neckline, squashed to the trunk's oval section.
    let mut ring = prim(
        torus(0.02, bp.neck_r * 1.35, collar.clone()),
        [0.0, yoke_y + chest_r * 0.30, 0.0],
        id_quat(),
    );
    ring.transform.scale = Fp3([1.0, 1.0, d]);
    torso.children.push(ring);
    // Centre placket — a short front seam on the chest mass (its front
    // is near-vertical, so the strip stays flush where the tapering trunk
    // would let it float).
    torso.children.push(prim(
        cuboid([chest_r * 0.13, len * 0.30, 0.015], collar),
        [0.0, yoke_y * 0.30, -(chest_r * 1.02 * d + 0.006)],
        id_quat(),
    ));
    // Belt at the waist — gives the trunk a waistline instead of a smooth tube.
    let mut belt_ring = prim(
        torus(0.02, waist_r * 1.02, belt),
        [0.0, -len * 0.42, 0.0],
        id_quat(),
    );
    belt_ring.transform.scale = Fp3([1.0, 1.0, d]);
    torso.children.push(belt_ring);
    // Shirt hem — a soft flare ring at the trunk bottom breaking the
    // torso-into-trousers column.
    let mut hem = prim(
        torus(
            0.025,
            waist_r * 1.0,
            ctx.materials.body(shade(ctx.palette.primary_accent, 0.82)),
        ),
        [0.0, -len * 0.5, 0.0],
        id_quat(),
    );
    hem.transform.scale = Fp3([1.0, 1.0, d]);
    torso.children.push(hem);
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

    // Fuller blended trunk than the shirt's (see [`trunk`]); the root is
    // unscaled, so authored Z offsets bake in `bp.depth`.
    let mut torso = trunk(ctx, chest_r, 1.06, shell);
    let yoke_y = bp.shoulder_y - bp.torso_y;
    let d = bp.depth;
    // Lapel V — two lining-colour strips angled outward at the throat,
    // seated on the chest mass so they stay flush on tapered trunks.
    for s in [-1.0f32, 1.0] {
        torso.children.push(prim(
            cuboid([0.03, len * 0.55, 0.02], lining.clone()),
            [
                s * chest_r * 0.30,
                len * 0.14,
                -(chest_r * 1.02 * d + 0.005),
            ],
            quat_xyzw(quat_z(s * 0.35)),
        ));
    }
    // Stand collar — a short ring standing at the neckline, squashed to
    // the trunk's oval section.
    let mut stand = prim(
        cylinder(bp.neck_r * 1.5, 0.09, 12, collar),
        [0.0, yoke_y + chest_r * 0.30, 0.0],
        id_quat(),
    );
    stand.transform.scale = Fp3([1.0, 1.0, d]);
    torso.children.push(stand);
    // Button row down the centre.
    for i in 0..3 {
        let by = len * (0.20 - 0.24 * i as f32);
        torso.children.push(prim(
            sphere(0.014, 2, btn.clone()),
            [
                0.0,
                by,
                -(bp.trunk_radius_at(bp.torso_y + by).max(chest_r * 0.9) * d + 0.014),
            ],
            id_quat(),
        ));
    }
    // Belt at the waist, squashed to the trunk's oval section.
    let mut belt = prim(
        torus(0.022, bp.waist_r * 1.06, btn),
        [0.0, -len * 0.42, 0.0],
        id_quat(),
    );
    belt.transform.scale = Fp3([1.0, 1.0, d]);
    torso.children.push(belt);
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

    // One Spine prim replaces the old shoulder→upper→elbow→forearm capsule
    // chain (#690): the elbow bend is baked into the spline path, the
    // shoulder→elbow→wrist taper into the per-point radii, and the joint
    // flows smoothly instead of showing a sphere-joint seam. The assembler
    // still rotates the shoulder root for splay; hand / cuff children sit
    // at the *computed* bent-frame positions the chain used to produce.
    let (st, ct) = theta.sin_cos();
    let elbow_y = -(l1 + r * 0.30);
    // A point `v` below the elbow along the bent forearm axis (front is -Z).
    let bent = |v: f32| [0.0, elbow_y - v * ct, -v * st];

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

    // The whole bare arm: shoulder → elbow → wrist through one spline, a
    // slight mid-upper station keeping the biceps line full.
    let arm_pts = [
        ([0.0, -r * 0.15, 0.0], r * 0.98),
        ([0.0, elbow_y * 0.55, 0.0], r * 0.96),
        ([0.0, elbow_y, 0.0], elbow_r),
        (bent(l2), wrist_r),
    ];
    let mut limb = prim(
        spine(&arm_pts, 12, skin.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Short sleeve cap over the top of the upper arm.
    limb.children.push(prim(
        capsule(r * 1.08, l1 * 0.5, sleeve),
        [0.0, -r * 0.30 - l1 * 0.28, 0.0],
        id_quat(),
    ));
    // Wrist cuff, aligned with the bent forearm axis.
    limb.children.push(prim(
        cylinder(wrist_r * 1.12, 0.03, 8, cuff),
        bent(l2),
        quat_xyzw(quat_x(theta)),
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
        bent(l2 + hand * 0.26),
        quat_xyzw(quat_x(theta)),
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
    limb.children.push(palm);
    shoulder.children.push(limb);
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

    // One Spine prim replaces the old hip→thigh→knee→shin chain (#690):
    // the knee bend is baked into the path, the hip→knee→ankle taper into
    // the per-point radii, and a calf station keeps the shin from reading
    // as a straight taper. The hip root stays the assembler's pivot.
    let (st, ct) = theta.sin_cos();
    // A point `v` below the knee along the bent shin axis (front is -Z).
    let bent = |v: f32| [0.0, -l1 - v * ct, -v * st];

    // Hip root = hip joint at the origin (the assembler's hip pivot). Kept
    // barely wider than the thigh so it doesn't bulge through the pelvis.
    let mut hip = prim(
        sphere(hip_r * 1.02, 2, trousers.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    let leg_pts = [
        ([0.0, -hip_r * 0.2, 0.0], hip_r),
        ([0.0, -l1, 0.0], r),
        (bent(l2 * 0.35), r * 0.92),
        (bent(l2), ankle_r),
    ];
    let mut limb = prim(
        spine(&leg_pts, 12, trousers.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Trouser cuff at the ankle, aligned with the bent shin axis.
    limb.children.push(prim(
        cylinder(ankle_r * 1.14, 0.04, 8, trousers),
        bent(l2 - 0.02),
        quat_xyzw(quat_x(theta)),
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
        {
            let a = bent(l2 + sole_drop);
            [a[0], a[1], a[2] - foot_l * 0.37]
        },
        quat_xyzw(quat_x(theta)),
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
    limb.children.push(foot);
    hip.children.push(limb);
    hip
}
