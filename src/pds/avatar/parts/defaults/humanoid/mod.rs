//! Humanoid defaults: head / torso / coat / arm / leg. Built in each slot's local attachment frame — see the module
//! docstring on [`super::super`] (`parts`).
//!
//! Every dimension comes from the seeded [`HumanoidBlueprint`]
//! (`ctx.blueprint`) so the parts, the assembler's anchors, and the
//! locomotion capsule share one proportion contract: canon landmark ratios
//! (wrist at the crotch line, legs ~half the figure, shoulder span in
//! head-heights, distal limb taper) banded by the avatar's
//! [`StylizationTier`](crate::seeded_defaults::StylizationTier).

use crate::pds::avatar::default_visuals::common::{
    blob_box, blob_capsule, blob_carve, blob_cone, blob_ellipsoid, blob_group, blob_sphere,
    capsule, cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, quat_z, sphere, torus,
};
use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;
use crate::seeded_defaults::HumanoidBlueprint;

use super::super::PartCtx;
use super::common::shade;

mod hair;
mod head;

pub(super) use head::head;

/// Trunk shaping shared by [`torso`] and [`coat`] — the classical
/// two-mass armature (#726): a ribcage egg pitched slightly **back**, an
/// abdomen mass pitched slightly **forward** (the S-curve every figure
/// canon demands — coaxial masses read as a bowling pin), a visibly
/// narrower waist connector between them, flank carves that pinch the
/// waist from the sides only, and trapezius cones sloping from the neck
/// base down to the arm mounts (replacing the old horizontal
/// "coat-hanger" yoke bar). The front-to-back flattening (`bp.depth`) is
/// baked into every element's Z semi-axis, so the root carries NO scale —
/// surface children bake `bp.depth` into their Z offsets instead.
/// `fullness` bulks the coat trunk.
///
/// Contract: chest accessories no longer seat against a fixed radius —
/// [`HumanoidBlueprint::trunk_front_z`] samples these masses' actual front
/// surface so a decal tracks the pectoral bulge on every seed (#727). Any
/// change to the pectoral / abdomen / waist masses here must be mirrored in
/// that method.
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
    let blend = chest_r * 0.42;
    // Opposing tilts: ribcage top toward the back (+Z), abdomen top toward
    // the front (−Z). Small angles — the S has to survive under clothing,
    // not read as a lean (round-2 pulled these back from 0.10/−0.14: the
    // stronger pair read as a kyphotic hump + pot belly on half the seeds).
    let back_tilt = quat_xyzw(quat_x(0.07));
    let fwd_tilt = quat_xyzw(quat_x(-0.10));
    let mut elements = vec![
        // Ribcage egg — the upper-trunk primary mass. Depth pulled in from
        // 0.68→0.58 (#728-A: barrel/pigeon chest on ~10/14 seeds — the
        // thorax projected too far in Z) and the +Z bias dropped to 0 so it
        // no longer bulges behind (#728 sev3 cobra-hood on 6300350204994988827).
        blob_ellipsoid(
            [0.0, yoke_y * 0.40, 0.0],
            [chest_r * 0.87 * fullness, len * 0.40, chest_r * 0.58 * d],
            back_tilt,
            blend,
        ),
        // Pectoral / chest-front plane. Forward projection cut ~14 %
        // (offset 0.42→0.36, semi 0.56→0.48 → front peak ≈ 0.84·chest_r·d,
        // was 0.98) so the chest reads flatter in profile (#728-A). Chest
        // accessories seat on the *sampled* front via `trunk_front_z`, so
        // this reduction flows through to their depth automatically (#727).
        blob_ellipsoid(
            [0.0, yoke_y * 0.42, -chest_r * 0.36 * d],
            [chest_r * 0.86 * fullness, len * 0.42, chest_r * 0.48 * d],
            id_quat(),
            blend,
        ),
        // Upper-back / shoulder-blade mass — flattened (offset 0.34→0.26,
        // semi 0.34→0.28 → back reach ≈ 0.54·chest_r·d, was 0.68) to kill
        // the kyphotic yoke hump (#728-E, seen on 3 seeds).
        blob_ellipsoid(
            [0.0, yoke_y * 0.60, chest_r * 0.26 * d],
            [chest_r * 0.82 * fullness, len * 0.30, chest_r * 0.28 * d],
            back_tilt,
            blend,
        ),
        // Waist connector — narrower than the trunk masses, but widened
        // 0.90→0.94 (X) / 0.80→0.86 (Z) so the chest→waist→hip loft is
        // continuous instead of tucking to a hard shelf above the belt
        // (#728-B, ~9 seeds).
        blob_ellipsoid(
            [0.0, -len * 0.16, 0.0],
            [waist_r * 0.94, len * 0.24, waist_r * 0.86 * d],
            id_quat(),
            blend * 0.9,
        ),
        // Abdomen / lower trunk: belly curve forward, lumbar hollow behind.
        blob_ellipsoid(
            [0.0, -len * 0.36, -waist_r * 0.03 * d],
            [waist_r * 0.96, len * 0.30, waist_r * 0.84 * d],
            fwd_tilt,
            blend,
        ),
    ];
    // Flank pinch — carve a shallow scoop from each side at waist height so
    // the front silhouette waists without thinning the belly/lumbar depth.
    // Carves sit right after the trunk masses so the traps / yoke added
    // below are never eaten by them (elements evaluate in list order). Bite
    // softened (moved out 1.58→1.66, blend 0.5→0.6) so the waist tuck reads
    // as a curve, not the abrupt shelf reviewers flagged (#728-B).
    for s in [-1.0f32, 1.0] {
        elements.push(blob_carve(blob_ellipsoid(
            [s * waist_r * 1.66, -len * 0.16, 0.0],
            [waist_r * 0.48, len * 0.22, waist_r * 1.10 * d],
            id_quat(),
            blend * 0.6,
        )));
    }
    // Trapezius cones: neck base sloping down-and-out to each arm mount —
    // the ramped neck→shoulder silhouette (kills the coat-hanger bar).
    for s in [-1.0f32, 1.0] {
        elements.push(blob_cone(
            [
                s * bp.shoulder_x * 0.52,
                yoke_y + chest_r * 0.10,
                chest_r * 0.06 * d,
            ],
            chest_r * 0.30,
            bp.shoulder_x * 0.44,
            chest_r * 0.11,
            quat_xyzw(quat_z(s * 1.10)),
            blend * 0.8,
        ));
    }
    // Shoulder yoke core — slimmer than the old bar and stopping INSIDE
    // the shoulder pivots, so the arm roots stay outboard of the trunk
    // hull instead of being swallowed by it (round-2 §3.8).
    elements.push(blob_ellipsoid(
        [0.0, yoke_y * 0.96, 0.0],
        [
            (bp.shoulder_x - bp.arm_r * 0.15) * fullness.min(1.0),
            chest_r * 0.30,
            // Depth pulled 0.70→0.60·d to match the flattened ribcage /
            // upper-back so the yoke doesn't reintroduce a back hump (#728-E).
            chest_r * 0.60 * d,
        ],
        id_quat(),
        blend * 0.7,
    ));
    // Neck root — a short column the head part's neck sinks into, so the
    // junction is trunk-swallows-neck rather than head-on-a-shelf. Sits a
    // touch behind the axis (the chest-forward masses made the neck read
    // as emerging from a hollow), and its upward reach is capped by the
    // blueprint's actual chin gap — on toy-tier bodies the huge chest made
    // the old fixed chest-relative column swallow the chin.
    // Radius widened 1.25→1.35 and pushed back 0.05→0.07·d so the column
    // fully clothes the back of the neck: flattening the upper back (#728-E)
    // pulled the rear shell in enough to expose a skin speck at the
    // nape-to-collar junction on one seed (final-verify collateral). Narrow
    // and high, so it doesn't reintroduce the upper-back hump.
    let nr_reach = (bp.neck_len + bp.neck_r * 0.4).min(chest_r * 0.52);
    let nr_half = (chest_r * 0.22).min(nr_reach * 0.5);
    elements.push(blob_capsule(
        [0.0, yoke_y + nr_reach - nr_half, chest_r * 0.07 * d],
        bp.neck_r * 1.35,
        nr_half,
        id_quat(),
        blend * 0.6,
    ));
    prim(blob_group(elements, 40, shell), [0.0, 0.0, 0.0], id_quat())
}

/// The trunk's *rendered* front surface Z at torso-local `y`: the raw
/// ellipsoid envelope [`HumanoidBlueprint::trunk_front_z`] pushed forward by
/// `margin_frac · chest_r · depth`, because the BlobGroup's smooth-union
/// skin bulges ahead of the raw ellipsoids. The margin is per-decal: a tiny
/// button needs a large fraction (~0.12) to read proud, but a large ornament
/// — which extends forward on its own geometry — needs only a small one
/// (~0.03), or its whole body floats off the chest in profile (the round-2
/// regression, #727). More-negative = further forward.
fn seat_surface_z(bp: &HumanoidBlueprint, chest_r: f32, y: f32, margin_frac: f32) -> f32 {
    bp.trunk_front_z(chest_r, y) - margin_frac * chest_r * bp.depth
}

/// Frontmost [`seat_surface_z`] across a decal's vertical span — where a
/// thin card seats so it reads flush at the sternum and only barely proud
/// at the ends. Round 1's variable-depth strip made a thick proud slab
/// instead (#727); a thin card at the frontmost sample is the lesser evil
/// for a flat decal on a convex chest.
fn seat_front_z(
    bp: &HumanoidBlueprint,
    chest_r: f32,
    y_center: f32,
    y_half: f32,
    margin_frac: f32,
) -> f32 {
    let mut z_front = f32::INFINITY;
    for i in 0..=4 {
        let y = y_center + y_half * (i as f32 / 2.0 - 1.0);
        z_front = z_front.min(seat_surface_z(bp, chest_r, y, margin_frac));
    }
    z_front
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

    // Collar ring at the neckline — major radius tucked into the trunk's
    // neck-root column so the ring reads as a worn collar, not a floating
    // hoop (#726 conformal-attachment pass). Height tracks the trunk's
    // capped neck-root so toy-tier collars don't ride up over the chin.
    // Hugs tighter (major 1.28→1.20·neck_r) so the ring reads as a worn
    // collar seam rather than a hoop standing off the neck (#727-C). Height
    // kept near the original (0.72→0.70, not 0.66 — dropping it further
    // exposed bare neck/chest on 14912211707486551165).
    let neck_line = yoke_y + ((bp.neck_len + bp.neck_r * 0.4).min(chest_r * 0.52)) * 0.70;
    let mut ring = prim(
        torus(0.02, bp.neck_r * 1.20, collar.clone()),
        [0.0, neck_line, chest_r * 0.05 * d],
        id_quat(),
    );
    ring.transform.scale = Fp3([1.0, 1.0, d]);
    torso.children.push(ring);
    // Centre placket — a thin front seam seated at the blend-compensated
    // chest surface so it reads flush at the sternum. Kept THIN (0.02): a
    // variable-depth card read as a thick proud slab in round 1 (#727).
    torso.children.push(prim(
        cuboid([chest_r * 0.13, len * 0.28, 0.02], collar),
        [
            0.0,
            yoke_y * 0.30,
            seat_front_z(bp, chest_r, yoke_y * 0.30, len * 0.14, 0.03),
        ],
        id_quat(),
    ));
    // Belt at the waist — major radius matched to the abdomen mass's
    // cross-section at this height so the band hugs the trunk (the old
    // fixed 1.02·waist ring stood proud of the pinched-in waist).
    let mut belt_ring = prim(
        torus(0.02, waist_r * 0.94, belt),
        [0.0, -len * 0.42, 0.0],
        id_quat(),
    );
    belt_ring.transform.scale = Fp3([1.0, 1.0, d]);
    torso.children.push(belt_ring);
    // Shirt hem — a soft flare ring where the abdomen mass rounds off; its
    // major radius follows that shrinking cross-section (≈0.88·waist at
    // -0.5·len) so the hem hugs the surface instead of hovering.
    let mut hem = prim(
        torus(
            0.025,
            waist_r * 0.88,
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
    // Lapel V — two thin lining-colour strips angled outward at the throat,
    // seated at the blend-compensated chest surface (kept thin so they read
    // as seams, not proud slabs — the round-1 regression, #727).
    let lz = seat_front_z(bp, chest_r, len * 0.14, len * 0.24, 0.05);
    for s in [-1.0f32, 1.0] {
        torso.children.push(prim(
            cuboid([0.03, len * 0.52, 0.02], lining.clone()),
            [s * chest_r * 0.30, len * 0.14, lz],
            quat_xyzw(quat_z(s * 0.35)),
        ));
    }
    // Stand collar — a short ring standing at the neckline, squashed to
    // the trunk's oval section; height tracks the trunk's capped neck-root
    // (toy-tier chests are huge relative to their chin gap).
    let neck_line = yoke_y + ((bp.neck_len + bp.neck_r * 0.4).min(chest_r * 0.52)) * 0.72;
    let mut stand = prim(
        cylinder(bp.neck_r * 1.5, 0.09, 12, collar),
        [0.0, neck_line, chest_r * 0.05 * d],
        id_quat(),
    );
    stand.transform.scale = Fp3([1.0, 1.0, d]);
    torso.children.push(stand);
    // Button row down the centre — each button centred on the
    // blend-compensated chest surface at its own height, so its full radius
    // reads proud. Round 1 seated them on the RAW envelope (behind the
    // blend-bulged skin) and they sank to invisibility (#727 min-standoff).
    for i in 0..3 {
        let by = len * (0.20 - 0.24 * i as f32);
        torso.children.push(prim(
            sphere(0.014, 2, btn.clone()),
            [0.0, by, seat_surface_z(bp, chest_r, by, 0.12)],
            id_quat(),
        ));
    }
    // Belt at the waist, squashed to the trunk's oval section and matched
    // to the coat trunk's abdomen cross-section so it hugs the surface.
    let mut belt = prim(
        torus(0.022, bp.waist_r * 0.96, btn),
        [0.0, -len * 0.42, 0.0],
        id_quat(),
    );
    belt.transform.scale = Fp3([1.0, 1.0, d]);
    torso.children.push(belt);
    torso
}

pub(super) fn arm(ctx: &PartCtx) -> Generator {
    use std::f32::consts::PI;
    let bp = &ctx.blueprint;
    let r = bp.arm_r;
    let (l1, l2) = (bp.upper_arm, bp.forearm); // upper arm, forearm
    // Canon thick-to-thin rhythm with a real *wrist minimum*: the joint
    // pinches to ~half the forearm peak, then the palm flares back out
    // (pinch-then-flare is what separates a hand from a tube end).
    // Arm thickened (#728-C top-heavy, ~10/12 seeds): the elbow / upper-arm
    // / forearm radii bumped so the limb no longer reads as a thin cylinder
    // under a bulbous shoulder. The wrist minimum is left alone — the taper
    // to the wrist is what reads as an arm, not a tube.
    let elbow_r = r * 0.86;
    let wrist_r = r * bp.limb_taper * 0.72;
    let theta = 0.22_f32; // elbow rest bend forward (front is -Z)
    let skin = ctx.materials.skin(ctx.palette.skin_tone);
    let sleeve = ctx.materials.body(ctx.palette.primary_accent);
    let cuff = ctx.materials.trim(ctx.palette.secondary_accent);

    let (st, ct) = theta.sin_cos();
    let elbow_y = -(l1 + r * 0.20);
    // A point `v` below the elbow along the bent forearm axis (front is -Z).
    let bent = |v: f32| [0.0, elbow_y - v * ct, -v * st];

    // Shoulder root = the pivot the assembler rotates the whole arm about.
    // It must carry NO scale — node scale propagates down the Bevy child
    // hierarchy (the old scaled-cap root squashed the whole chain). A small
    // ball swallowed by the trunk's trapezius cones.
    let mut shoulder = prim(
        sphere(r * 0.70, 3, sleeve.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );

    // The whole bare arm as ONE blended skin (#726): truncated-cone upper
    // arm and forearm whose fat ends overlap the joint balls (round 2: a
    // point-tipped cone reads as a teardrop and the silhouette DISCONNECTS
    // at the joints — every segment must arrive with ≥ ~40 % of its base
    // radius) → elbow ball → drumstick swell → wrist minimum → mitten
    // hand. Everything is X-symmetric: the assembler mirrors the single
    // arm by rotation, not reflection, so all shaping lives in Y/Z. The
    // skin stays a shade narrower than the sleeve overlay so cloth always
    // covers skin at the shoulder.
    let kb = r * 0.45;
    let hand = bp.hand_len;
    // Palm flares past the wrist but stays a mitten, not an oar: width is
    // floored against the wrist (speck-hand pole) and capped against the
    // forearm (plank pole); depth stays well under width so it reads flat.
    let hand_w = (hand * 0.40).max(wrist_r * 2.4).min(r * 1.9);
    let hand_d = (wrist_r * 1.7).min(hand_w * 0.62);
    let elements = vec![
        // Upper arm: base tucked under the sleeve's deltoid cap, tip
        // overlapping the elbow ball (cone flipped apex-down via π).
        blob_cone(
            [0.0, elbow_y * 0.50, 0.0],
            r * 0.95,
            -elbow_y * 0.55,
            elbow_r * 0.85,
            quat_xyzw(quat_x(PI)),
            kb,
        ),
        // Biceps front swell, staggered high on the segment.
        blob_ellipsoid(
            [0.0, elbow_y * 0.42, -r * 0.42],
            [r * 0.60, l1 * 0.28, r * 0.52],
            id_quat(),
            kb,
        ),
        // Elbow ball — both segments sink into it (no sausage-link point).
        blob_sphere([0.0, elbow_y, 0.0], elbow_r, kb * 0.8),
        // Forearm: base swallowed by the elbow, tip carrying the wrist
        // radius, along the bent axis (π + θ maps the cone's +Y onto it).
        blob_cone(
            bent(l2 * 0.45),
            r * 0.90,
            l2 * 0.55,
            wrist_r * 0.95,
            quat_xyzw(quat_x(PI + theta)),
            kb,
        ),
        // Drumstick swell just below the elbow (widened symmetrically).
        blob_ellipsoid(
            bent(l2 * 0.25),
            [r * 0.85, l2 * 0.24, r * 0.64],
            quat_xyzw(quat_x(theta)),
            kb,
        ),
        // Wrist minimum — small blend keeps the undercut visible.
        blob_ellipsoid(
            bent(l2),
            [wrist_r * 1.05, wrist_r * 0.85, wrist_r * 0.90],
            quat_xyzw(quat_x(theta)),
            kb * 0.5,
        ),
        // Palm — a blend-rounded box, wider than the wrist, long axis
        // colinear with the forearm.
        blob_box(
            bent(l2 + hand * 0.26),
            [hand_w * 0.50, hand * 0.22, hand_d * 0.50],
            quat_xyzw(quat_x(theta)),
            kb * 0.55,
        ),
        // Finger mitten — a gentle relaxed curl only (round 2: a strong
        // curl read as the whole hand tilted off the forearm axis).
        blob_ellipsoid(
            bent(l2 + hand * 0.54),
            [hand_w * 0.46, hand * 0.20, hand_d * 0.42],
            quat_xyzw(quat_x(theta + 0.22)),
            kb * 0.6,
        ),
    ];
    let mut limb = prim(blob_group(elements, 44, skin), [0.0, 0.0, 0.0], id_quat());

    // Sleeve overlay: the deltoid mass IS the sleeve cap (round 2: a
    // skin-coloured deltoid poked through the cloth on almost every seed),
    // over a shirt capsule whose hem line reads via the colour change to
    // skin — no floating cuff ring.
    // Deltoid cap shrunk 1.12→1.08·r and the sleeve tube thickened
    // 1.06→1.14·r (#728-C): narrows the shoulder-to-arm diameter ratio that
    // made the figure top-heavy. Round 1 shrank the cap to 1.05, which let
    // shoulder skin poke through on one seed — 1.08 keeps cloth over skin.
    let mut delt = prim(
        sphere(r * 1.08, 3, sleeve.clone()),
        [0.0, -r * 0.05, 0.0],
        id_quat(),
    );
    delt.transform.scale = Fp3([1.0, 0.85, 0.95]);
    limb.children.push(delt);
    limb.children.push(prim(
        capsule(r * 1.14, l1 * 0.5, sleeve),
        [0.0, -r * 0.25 - l1 * 0.26, 0.0],
        id_quat(),
    ));
    // Wrist cuff, aligned with the bent forearm axis and snug on the wrist
    // minimum (wristwear wraps the joint, it doesn't define it).
    limb.children.push(prim(
        cylinder(wrist_r * 1.12, 0.03, 8, cuff),
        bent(l2 * 0.97),
        quat_xyzw(quat_x(theta)),
    ));
    shoulder.children.push(limb);
    shoulder
}

pub(super) fn leg(ctx: &PartCtx) -> Generator {
    use std::f32::consts::PI;
    let bp = &ctx.blueprint;
    // Girth is authored at the knee; the thigh flares up from it and the
    // shin tapers down to a real ankle minimum.
    let r = bp.leg_r;
    let hip_r = r * 1.10;
    let ankle_r = r * bp.limb_taper * 0.80;
    let (l1, l2) = (bp.thigh, bp.shin);
    // Side-view S (#726): the thigh mass rides forward of the hip-knee
    // line while the shin drops slightly BACK from the knee (+Z) and the
    // foot then reaches forward — the old forward-tilted shin drew the
    // figure leaning backwards.
    let theta = 0.10_f32;
    // Trousers: a darker shade of the primary so legs read as one outfit with
    // the shirt rather than a clashing accent.
    let trousers = ctx.materials.body(shade(ctx.palette.primary_accent, 0.6));
    let shoe_mat = ctx.materials.body(ctx.palette.secondary_accent);

    let (st, ct) = theta.sin_cos();
    // A point `v` below the knee along the bent shin axis (+Z = backward).
    let bent = |v: f32| [0.0, -l1 - v * ct, v * st];

    // Hip root = hip joint at the origin (the assembler's hip pivot); a
    // small ball swallowed by the pelvis and the thigh root, NO scale.
    let mut hip = prim(
        sphere(hip_r * 0.9, 2, trousers.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );

    // The whole trousered leg as ONE blended skin: thigh-root mass →
    // tapered thigh cone (apex sunk into the knee ball, arriving with
    // volume — never a pinch-point) → knee → tapered shin with a calf
    // swell on the back face → ankle minimum. X-symmetric: the assembler
    // places both legs from this one build.
    let kb = r * 0.5;
    let elements = vec![
        // Hip / thigh-root mass. (Reverted to baseline with the rest of the
        // #729 seat rework — see the note in the assembler's pelvis build.)
        blob_ellipsoid(
            [0.0, -hip_r * 0.25, 0.0],
            [hip_r * 1.0, hip_r * 0.95, hip_r * 1.0],
            id_quat(),
            kb,
        ),
        // Thigh: base at the hip, tip overlapping the knee ball with real
        // volume (round 2: point-tipped segments disconnect at joints),
        // mass biased forward of the hip-knee line.
        blob_cone(
            [0.0, -l1 * 0.48, -r * 0.16],
            hip_r * 0.98,
            l1 * 0.55,
            r * 0.80,
            quat_xyzw(quat_x(PI)),
            kb,
        ),
        // Quad swell on the front face, staggered high.
        blob_ellipsoid(
            [0.0, -l1 * 0.42, -r * 0.44],
            [r * 0.68, l1 * 0.26, r * 0.52],
            id_quat(),
            kb,
        ),
        // NOTE (#729): a 5th glute→thigh attempt — a leg-group posterior
        // sub-gluteal fill — was render-tested here (2 variants, all tiers)
        // and REVERTED. Unlike the 4 pelvis-side attempts (which cleft-
        // regressed), a leg-group mass is cleft-immune, but it cannot fix
        // the undercut either: the seat overhangs a void ≈2·leg_r deep
        // behind the thigh, while the thigh's own back surface sits at
        // ≈0.83·leg_r, so any fill big enough to reach under the glute
        // projects ≈1.2·leg_r PROUD of the thigh (a saddlebag), and any
        // fill shallow enough to stay tucked is invisible (confirmed: both
        // 0.86·r and 1.08·r back-reach variants read identical to baseline).
        // The glute is also laterally offset from the leg mount, oppositely
        // per tier, so an X-symmetric single-leg fill can't even align with
        // it. There is no sweet spot; the mild sev2 undercut is the best
        // available read short of unifying the pelvis+thigh into one group
        // (a larger restructure with its own cleft risks). See the pelvis
        // build's #729 note in default_visuals/humanoid.rs.
        // Knee ball — both segments sink into it.
        blob_sphere([0.0, -l1, 0.0], r * 0.82, kb * 0.75),
        // Shin: knee → ankle along the bent-back axis (π − θ maps the
        // cone's +Y onto that axis), tip carrying the ankle radius so the
        // leg enters the boot with silhouette instead of a needle.
        blob_cone(
            bent(l2 * 0.45),
            r * 0.78,
            l2 * 0.55,
            ankle_r * 0.95,
            quat_xyzw(quat_x(PI - theta)),
            kb,
        ),
        // Calf swell on the BACK face, upper third of the shin — below
        // it the leg is bone-and-tendon only.
        blob_ellipsoid(
            {
                let p = bent(l2 * 0.30);
                [p[0], p[1], p[2] + r * 0.30]
            },
            [r * 0.72, l2 * 0.22, r * 0.60],
            quat_xyzw(quat_x(-theta)),
            kb,
        ),
        // Ankle minimum — small blend keeps the undercut crisp.
        blob_ellipsoid(
            bent(l2),
            [ankle_r * 1.05, ankle_r * 0.90, ankle_r * 0.95],
            quat_xyzw(quat_x(-theta)),
            kb * 0.5,
        ),
    ];
    let mut limb = prim(
        blob_group(elements, 44, trousers.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Trouser cuff snug above the ankle, aligned with the bent shin axis.
    limb.children.push(prim(
        cylinder(ankle_r * 1.18, 0.035, 8, trousers),
        bent(l2 * 0.93),
        quat_xyzw(quat_x(-theta)),
    ));

    // ---- Foot: built from the GROUND UP (#726) -------------------------
    // The shoe loaf's lowest point (the heel ball's underside) is computed
    // to land exactly at the blueprint's ground line, so every seed's feet
    // are planted by construction, and the ankle axis enters ~70 % along
    // the foot: a real heel projects behind the leg while the toe reaches
    // forward. No separate sole slab — the flat plate read as a detached
    // platform under the rounded loaf (#731); the loaf grounds itself.
    let ankle = bent(l2);
    let ankle_z = ankle[2];
    let foot_l = bp.foot_len;
    let ground_y = -bp.leg_total();
    // Heel-ball bottom sits 0.20·ankle_r under `base_y`; lift the loaf by
    // exactly that so the heel kisses the ground line.
    let base_y = ground_y + ankle_r * 0.20;
    // One-loaf upper: ankle collar + heel ball + instep block + toe cap
    // blended into a single wedge — higher at the heel/ankle, tapering to
    // the toe, wide enough (≥1.2× the ankle) to read from the back view.
    // The collar wraps the shin's entry point so the leg visually sinks
    // INTO the boot (round 2: shins ended mid-air above a saddle between
    // heel and instep). Instep and toe bottoms sit a hair above the ground
    // line — a slight natural toe spring instead of a full flat slab.
    let kf = ankle_r * 0.6;
    let loaf = vec![
        blob_sphere([0.0, base_y + ankle_r * 1.05, ankle_z], ankle_r * 1.18, kf),
        blob_sphere(
            [0.0, base_y + ankle_r * 0.85, ankle_z + foot_l * 0.14],
            ankle_r * 1.05,
            kf,
        ),
        blob_box(
            [0.0, base_y + ankle_r * 0.42, ankle_z - foot_l * 0.18],
            [ankle_r * 1.25, ankle_r * 0.60, foot_l * 0.30],
            id_quat(),
            kf,
        ),
        blob_ellipsoid(
            [0.0, base_y + ankle_r * 0.26, ankle_z - foot_l * 0.52],
            [ankle_r * 1.10, ankle_r * 0.42, foot_l * 0.20],
            id_quat(),
            kf,
        ),
    ];
    limb.children.push(prim(
        blob_group(loaf, 28, shoe_mat),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    hip.children.push(limb);
    hip
}
