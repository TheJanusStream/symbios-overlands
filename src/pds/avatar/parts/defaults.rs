//! Universal default parts — one per required slot per chassis, eligible
//! for every style (empty [`BodyPart::styles`]).
//!
//! These are the **coverage floor**: they guarantee every required
//! (chassis, slot) is fillable for any style/tier so the outfit deriver
//! never stalls on an unfillable slot while the styled kits
//! (`super`'s `#518`/`#519` content) fill in. The geometry is plain — a
//! readable, *recognisable* silhouette built from the shared primitive
//! vocabulary and finished through the seeded
//! [`MaterialKit`](crate::seeded_defaults::MaterialKit) — humanoids carry a
//! neck/face/hair/hands/feet, vehicles a shaped hull / cabin / cigar
//! envelope, rather than bare capsules and slabs. Each builds in its slot's
//! local attachment frame (see the module docstring on [`super`]).
//!
//! ## Colour coherence
//!
//! Large surfaces wear the avatar's `primary_accent` (or a darkened shade of
//! it for trousers / skirts), and `secondary` / `tertiary` accents are kept
//! to small areas (collars, shoes, trim, running lights). This avoids the
//! "harlequin" reading where torso / legs / arms each took a different point
//! of the OkLCH triad.
//!
//! ## Root-scale discipline
//!
//! A base part used as a family's structural root ([`hull`], [`chassis`],
//! [`envelope`]) must **not** set `transform.scale`, because the assembler
//! mounts every other slot (deck, canopy, wheels, gondola, fins) as a child
//! of that root and a root scale would stretch + displace them. Elongated
//! shapes (the airship envelope) are built from composed primitives instead.

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::default_visuals::common::{
    capsule, cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, sphere, torus, with_torture,
};
use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::ChassisFamily;

use super::{BodyPart, PartCtx, PartSlot};

const HUMANOID: &[ChassisFamily] = &[ChassisFamily::Humanoid];
const BOAT: &[ChassisFamily] = &[ChassisFamily::Boat];
const AIRSHIP: &[ChassisFamily] = &[ChassisFamily::Airship];
const SKIFF: &[ChassisFamily] = &[ChassisFamily::Skiff];

/// Salt for the per-part hair-style draw (kept distinct from any deriver
/// stream salt so it doesn't correlate with palette / outfit choices).
const HAIR_SALT: u64 = 0x4841_4952_4841_4952;

/// Multiply a colour toward black by `f` (`0` = black, `1` = unchanged) —
/// the local "darker shade of the same hue" used for trousers / skirts /
/// bumpers so a second large surface stays tonally related to the primary.
fn shade(c: [f32; 3], f: f32) -> [f32; 3] {
    [c[0] * f, c[1] * f, c[2] * f]
}

/// A small deterministic discrete choice in `0..n` from the avatar seed and
/// a salt. Mixed through a multiply so the high bits don't correlate with
/// the low bits other derivers key off.
fn seed_choice(seed: u64, salt: u64, n: u64) -> u64 {
    ((seed ^ salt).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 60) % n
}

/// A data-driven [`BodyPart`] — metadata plus a build function pointer.
/// Universal default parts are plain enough to express as a table rather
/// than a struct apiece; the richer styled kits may use either.
pub(super) struct FnPart {
    slug: &'static str,
    name: &'static str,
    slot: PartSlot,
    chassis: &'static [ChassisFamily],
    build: fn(&PartCtx) -> Generator,
}

impl BodyPart for FnPart {
    fn slug(&self) -> &'static str {
        self.slug
    }
    fn name(&self) -> &'static str {
        self.name
    }
    fn slot(&self) -> PartSlot {
        self.slot
    }
    fn chassis(&self) -> &'static [ChassisFamily] {
        self.chassis
    }
    fn build(&self, ctx: &PartCtx) -> Generator {
        (self.build)(ctx)
    }
    // styles() empty (universal) + ornateness/wear bands ANY by default.
}

// ---------------------------------------------------------------------------
// Humanoid
// ---------------------------------------------------------------------------

fn head(ctx: &PartCtx) -> Generator {
    let r = 0.13 * ctx.body.head_scale;
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

    // Neck — its base disappears into the torso collar (the assembler seats
    // the head just above the shoulders).
    head.children.push(prim(
        cylinder(0.045, 0.14, 10, skin.clone()),
        [0.0, -r - 0.05, 0.0],
        id_quat(),
    ));

    // Eyes + brows. The face is on -Z (the assembler never turns the head).
    for s in [-1.0f32, 1.0] {
        // White sclera in a shallow socket with a smaller dark iris in front,
        // so each eye reads as an eye instead of merging with the brow into a
        // single dark bar (the old same-tone eye+brow pairing).
        let mut socket = prim(
            sphere(0.030, 2, sclera.clone()),
            [s * r * 0.40, r * 0.04, -r * 0.86],
            id_quat(),
        );
        socket.children.push(prim(
            sphere(0.015, 2, eye.clone()),
            [0.0, 0.0, -0.022],
            id_quat(),
        ));
        head.children.push(socket);
        // Brow — thin and lifted well clear of the eye.
        head.children.push(prim(
            cuboid([0.046, 0.010, 0.018], hair.clone()),
            [s * r * 0.40, r * 0.34, -r * 0.90],
            id_quat(),
        ));
    }
    // Nose nub + mouth.
    head.children.push(prim(
        cuboid([0.025, 0.035, 0.045], skin.clone()),
        [0.0, -r * 0.08, -r * 0.95],
        id_quat(),
    ));
    head.children.push(prim(
        cuboid(
            [0.05, 0.014, 0.02],
            ctx.materials.cloth(shade(ctx.palette.skin_tone, 0.7)),
        ),
        [0.0, -r * 0.42, -r * 0.86],
        id_quat(),
    ));
    // Ears.
    for s in [-1.0f32, 1.0] {
        head.children.push(prim(
            sphere(0.022, 2, skin.clone()),
            [s * (r + 0.004), -r * 0.02, r * 0.02],
            id_quat(),
        ));
    }

    // Hair — a crown cap covering the whole top of the head, tilted backward so
    // its front rim lifts to the upper forehead (a clean hairline) while the
    // crown stays fully covered (no bare skull-cap shows); a back/nape mass and
    // temples frame the face. Reads as a haircut, not a swim cap. A per-seed
    // flourish adds variety on top.
    let mut cap = prim(
        sphere(r, 4, hair.clone()),
        [0.0, r * 0.68, r * 0.06],
        quat_xyzw(quat_x(-0.30)),
    );
    cap.transform.scale = Fp3([1.08, 0.66, 1.18]);
    head.children.push(cap);
    // Back/nape mass bridging the cap down to the neck so no skin shows behind.
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
            [s * r * 0.84, r * 0.10, r * 0.12],
            id_quat(),
        ));
    }
    // A hat clips the topknot / long-hair flourish, so only add it bare-headed.
    if !ctx.has_hat {
        match seed_choice(ctx.seed, HAIR_SALT, 3) {
            0 => {} // cropped — crown only
            1 => {
                // Long hair falling down the back (+Z is behind the face).
                head.children.push(prim(
                    cuboid([r * 1.5, r * 2.0, 0.05], hair.clone()),
                    [0.0, -r * 0.7, r * 0.62],
                    id_quat(),
                ));
            }
            _ => {
                // Topknot tuft.
                head.children.push(prim(
                    sphere(r * 0.42, 3, hair),
                    [0.0, r * 1.15, r * 0.05],
                    id_quat(),
                ));
            }
        }
    }
    head
}

fn torso(ctx: &PartCtx) -> Generator {
    let r = 0.155 * ctx.body.shoulder_width_scale;
    let shirt = ctx.materials.body(ctx.palette.primary_accent);
    // Tapered trunk — a negative taper flares the top, reading as shoulders.
    let mut torso = prim(
        with_torture(capsule(r, 0.5, shirt), 0.0, -0.12, [0.0, 0.0, 0.0]),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Collar at the neckline — a small secondary-accent ring.
    torso.children.push(prim(
        torus(
            0.022,
            r * 0.7,
            ctx.materials.trim(ctx.palette.secondary_accent),
        ),
        [0.0, 0.27, 0.0],
        id_quat(),
    ));
    // Belt at the waist — gives the trunk a waistline instead of a smooth tube.
    torso.children.push(prim(
        torus(
            0.02,
            r * 0.95,
            ctx.materials.trim(ctx.palette.tertiary_accent),
        ),
        [0.0, -0.16, 0.0],
        id_quat(),
    ));
    torso
}

fn arm(ctx: &PartCtx) -> Generator {
    let r = 0.05 * ctx.body.limb_thickness_scale;
    let (l1, l2) = (0.25, 0.23); // upper arm, forearm
    let theta = 0.16_f32; // gentle elbow bend forward (front is -Z) — relaxed
    let skin = ctx.materials.skin(ctx.palette.skin_tone);
    let sleeve = ctx.materials.body(ctx.palette.primary_accent);
    let cuff = ctx.materials.trim(ctx.palette.secondary_accent);

    // Root = a deltoid shoulder cap (shirt colour) at the origin. It's the
    // pivot the assembler rotates the whole arm about, so the splay swings
    // from the shoulder rather than the mid-arm, and it adds deltoid mass.
    let mut arm = prim(
        sphere(r * 1.6, 3, sleeve.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Short sleeve over the top of the upper arm; bare skin from there down.
    arm.children.push(prim(
        capsule(r * 1.04, l1 * 0.45, sleeve),
        [0.0, -l1 * 0.2, 0.0],
        id_quat(),
    ));
    // Upper arm (shoulder → elbow).
    arm.children.push(prim(
        capsule(r, l1, skin.clone()),
        [0.0, -l1 * 0.5, 0.0],
        id_quat(),
    ));
    // Elbow joint sphere — covers the seam where the forearm bends away.
    arm.children.push(prim(
        sphere(r * 1.05, 2, skin.clone()),
        [0.0, -l1, 0.0],
        id_quat(),
    ));
    // Forearm (elbow → wrist), gently bent forward about the elbow. Its top end
    // is pinned to the elbow: rotating a capsule about its own centre would gap
    // the joint, so the centre is placed so the rotated top lands at -l1.
    let (s, c) = theta.sin_cos();
    let fa = quat_xyzw(quat_x(theta));
    arm.children.push(prim(
        capsule(r * 0.85, l2, skin.clone()),
        [0.0, -l1 - 0.5 * l2 * c, -0.5 * l2 * s],
        fa,
    ));
    // Wrist cuff + a proper hand (palm with a cupped finger block) replacing
    // the old flat paddle. Kept left/right symmetric because the assembler
    // mirrors the single arm by rotation, not reflection — an offset thumb
    // would face the wrong way on one side.
    let wrist = [0.0, -l1 - l2 * c, -l2 * s];
    arm.children
        .push(prim(cylinder(r * 1.0, 0.035, 8, cuff), wrist, fa));
    let mut palm = prim(
        cuboid([r * 1.6, r * 1.5, r * 0.85], skin.clone()),
        wrist,
        fa,
    );
    palm.children.push(prim(
        cuboid([r * 1.5, r * 1.25, r * 0.6], skin),
        [0.0, -r * 1.4, -r * 0.08],
        id_quat(),
    ));
    arm.children.push(palm);
    arm
}

fn leg(ctx: &PartCtx) -> Generator {
    let r = 0.062 * ctx.body.limb_thickness_scale;
    let (l1, l2) = (0.32, 0.30); // thigh, shin
    let theta = 0.08_f32; // slight knee bend forward — reads alive, not a crouch
    // Trousers: a darker shade of the primary so legs read as one outfit with
    // the shirt rather than a clashing accent.
    let trousers = ctx.materials.body(shade(ctx.palette.primary_accent, 0.6));
    let shoe = ctx.materials.body(ctx.palette.secondary_accent);

    // Root = hip joint at the origin (the assembler's hip pivot).
    let mut leg = prim(
        sphere(r * 1.15, 2, trousers.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Thigh (hip → knee).
    leg.children.push(prim(
        capsule(r, l1, trousers.clone()),
        [0.0, -l1 * 0.5, 0.0],
        id_quat(),
    ));
    // Knee joint sphere.
    leg.children.push(prim(
        sphere(r * 1.02, 2, trousers.clone()),
        [0.0, -l1, 0.0],
        id_quat(),
    ));
    // Shin (knee → ankle), pinned to the knee (same trick as the forearm).
    let (s, c) = theta.sin_cos();
    leg.children.push(prim(
        capsule(r * 0.9, l2, trousers.clone()),
        [0.0, -l1 - 0.5 * l2 * c, -0.5 * l2 * s],
        quat_xyzw(quat_x(theta)),
    ));
    // Trouser cuff at the ankle.
    let ankle_y = -l1 - l2 * c;
    let ankle_z = -l2 * s;
    leg.children.push(prim(
        cylinder(r * 1.05, 0.04, 8, trousers),
        [0.0, ankle_y + 0.02, ankle_z],
        id_quat(),
    ));
    // Shoe — a forward-pointing shoe at the ankle (-Z is the front): a dark
    // sole, an instep rising to the ankle, and a rounded toe cap, replacing the
    // flat block.
    let sole = ctx
        .materials
        .metal(shade(ctx.palette.secondary_accent, 0.45));
    let mut foot = prim(
        cuboid([r * 1.7, 0.045, 0.2], sole),
        [0.0, ankle_y - 0.05, ankle_z - 0.06],
        id_quat(),
    );
    foot.children.push(prim(
        cuboid([r * 1.55, 0.07, 0.12], shoe.clone()),
        [0.0, 0.05, 0.05],
        id_quat(),
    ));
    let mut toe = prim(sphere(0.055, 2, shoe), [0.0, 0.015, -0.09], id_quat());
    toe.transform.scale = Fp3([1.35, 0.75, 1.25]);
    foot.children.push(toe);
    leg.children.push(foot);
    leg
}

// ---------------------------------------------------------------------------
// Boat
// ---------------------------------------------------------------------------

fn hull(ctx: &PartCtx) -> Generator {
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let underside = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.4));
    let trim = ctx.materials.metal(ctx.palette.secondary_accent);
    let underglow = ctx.materials.glow(ctx.palette.tertiary_accent);

    // Tiny structural core at the origin (hidden inside the shell). The root
    // can't be scaled — the assembler mounts the cockpit / fin to it and
    // overwrites its transform — so the sleek pod is a scaled-ellipsoid child.
    let mut hull = prim(
        cuboid([0.3, 0.16, 1.0], body.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Sleek pod shell: a rounded, voluminous ellipsoid (rounded nose + tail),
    // widest amidships — chunky enough to read as a body, not a surfboard.
    let mut shell = prim(sphere(0.5, 4, body.clone()), [0.0, 0.0, 0.0], id_quat());
    shell.transform.scale = Fp3([0.56, 0.46, 1.22]);
    hull.children.push(shell);
    // Darker underbody ellipsoid protruding below the shell as a keel belly.
    let mut under = prim(sphere(0.5, 3, underside), [0.0, -0.1, 0.0], id_quat());
    under.transform.scale = Fp3([0.5, 0.4, 1.12]);
    hull.children.push(under);
    // Glowing hover skirt — an emissive pad under the craft (the underglow).
    hull.children.push(prim(
        cuboid([0.44, 0.04, 1.0], underglow),
        [0.0, -0.27, 0.0],
        id_quat(),
    ));
    // Accent strake along each flank.
    for s in [-1.0f32, 1.0] {
        hull.children.push(prim(
            cuboid([0.02, 0.03, 0.95], trim.clone()),
            [s * 0.29, 0.04, 0.0],
            id_quat(),
        ));
    }
    hull
}

fn deck(ctx: &PartCtx) -> Generator {
    let shell = ctx.materials.body(shade(ctx.palette.primary_accent, 0.75));
    let dash = ctx.materials.metal(ctx.palette.secondary_accent);
    let glass = ctx.materials.glass(ctx.palette.secondary_accent);

    // Cockpit tub recessed into the pod deck.
    let mut deck = prim(
        cuboid([0.38, 0.06, 0.66], shell.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Seat back toward the stern.
    deck.children.push(prim(
        cuboid([0.26, 0.16, 0.07], shell),
        [0.0, 0.1, -0.22],
        id_quat(),
    ));
    // Dashboard fairing at the front of the cockpit.
    deck.children.push(prim(
        cuboid([0.34, 0.08, 0.06], dash),
        [0.0, 0.05, 0.24],
        id_quat(),
    ));
    // Wraparound windscreen, raked back over the cockpit.
    deck.children.push(prim(
        with_torture(
            cuboid([0.36, 0.16, 0.03], glass),
            0.0,
            0.25,
            [0.0, 0.0, -0.12],
        ),
        [0.0, 0.12, 0.24],
        id_quat(),
    ));
    deck
}

fn mast(ctx: &PartCtx) -> Generator {
    // The Boat 'Mast' slot, reinterpreted for the hover-skiff as a swept dorsal
    // tail fin. The assembler mounts this at the deck centre, so the fin is
    // pushed aft to rise from the stern.
    let fin = ctx.materials.body(ctx.palette.primary_accent);
    let edge = ctx.materials.glow(ctx.palette.tertiary_accent);

    let mut root = prim(
        with_torture(
            cuboid([0.04, 0.42, 0.46], fin.clone()),
            0.0,
            0.55,
            [0.0, 0.0, -0.4],
        ),
        [0.0, 0.12, -0.42],
        id_quat(),
    );
    // Glowing trailing edge down the back of the fin.
    root.children.push(prim(
        cuboid([0.05, 0.32, 0.04], edge),
        [0.0, -0.04, -0.2],
        id_quat(),
    ));
    root
}

// ---------------------------------------------------------------------------
// Airship
// ---------------------------------------------------------------------------

fn envelope(ctx: &PartCtx) -> Generator {
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let ring = ctx.materials.metal(ctx.palette.secondary_accent);
    let nose = ctx.materials.trim(ctx.palette.tertiary_accent);
    // A *smooth* elongated gas-bag. The root carries no scale (the assembler
    // mounts gondola / fins to it and a root scale would stretch + fling them),
    // so the bag is a single scaled-ellipsoid child of a tiny hidden core —
    // replacing the old three overlapping lobes that read as a lumpy caterpillar.
    let mut env = prim(
        cuboid([0.3, 0.3, 1.5], body.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    let mut bag = prim(sphere(0.8, 4, body.clone()), [0.0, 0.0, 0.0], id_quat());
    bag.transform.scale = Fp3([0.97, 0.97, 1.7]);
    env.children.push(bag);
    // Structural frame rings encircling the bag (ring plane ⟂ Z).
    for z in [-0.55f32, 0.0, 0.55] {
        env.children.push(prim(
            torus(0.018, 0.78, ring.clone()),
            [0.0, 0.0, z],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
    // Pointed nose finial at the bow (+Z).
    env.children
        .push(prim(sphere(0.16, 3, nose), [0.0, 0.0, 1.32], id_quat()));
    env
}

fn gondola(ctx: &PartCtx) -> Generator {
    let body = ctx.materials.body(ctx.palette.secondary_accent);
    let keel = ctx
        .materials
        .body(shade(ctx.palette.secondary_accent, 0.65));
    let frame = ctx
        .materials
        .metal(shade(ctx.palette.secondary_accent, 0.5));
    let window = ctx.materials.glow(ctx.palette.tertiary_accent);
    // Main cabin hull.
    let mut g = prim(
        cuboid([0.44, 0.28, 0.92], body.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Rounded nose + tail end caps.
    for sz in [-1.0f32, 1.0] {
        let mut cap = prim(
            sphere(0.22, 3, body.clone()),
            [0.0, -0.02, sz * 0.46],
            id_quat(),
        );
        cap.transform.scale = Fp3([0.95, 0.62, 0.55]);
        g.children.push(cap);
    }
    // A continuous lit window band along each flank, broken into panes by
    // mullions, instead of a sparse row of portholes.
    for s in [-1.0f32, 1.0] {
        g.children.push(prim(
            cuboid([0.02, 0.09, 0.74], window.clone()),
            [s * 0.225, 0.04, 0.0],
            id_quat(),
        ));
        for z in [-0.24f32, 0.0, 0.24] {
            g.children.push(prim(
                cuboid([0.03, 0.11, 0.03], frame.clone()),
                [s * 0.23, 0.04, z],
                id_quat(),
            ));
        }
    }
    // Rounded keel underneath.
    g.children.push(prim(
        cuboid([0.38, 0.12, 0.84], keel),
        [0.0, -0.18, 0.0],
        id_quat(),
    ));
    // Bridge cockpit bump at the bow (+Z).
    g.children.push(prim(
        cuboid([0.3, 0.14, 0.18], frame),
        [0.0, 0.14, 0.4],
        id_quat(),
    ));
    g
}

// ---------------------------------------------------------------------------
// Skiff
// ---------------------------------------------------------------------------

fn chassis(ctx: &PartCtx) -> Generator {
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let lower = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.45));
    let trim = ctx.materials.metal(ctx.palette.secondary_accent);
    let headlight = ctx.materials.glow([1.0, 0.95, 0.8]);
    let taillight = ctx.materials.glow([0.85, 0.12, 0.1]);

    // Body tub (structural root — no root scale).
    let mut c = prim(
        cuboid([0.76, 0.2, 1.5], body.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Dark lower rocker / skirt.
    c.children.push(prim(
        cuboid([0.8, 0.12, 1.42], lower.clone()),
        [0.0, -0.13, 0.0],
        id_quat(),
    ));
    // Hood at the front (+Z), lower than the cabin.
    c.children.push(prim(
        cuboid([0.68, 0.1, 0.5], body.clone()),
        [0.0, 0.11, 0.5],
        id_quat(),
    ));
    // Cabin block toward the rear (the canopy seats on this).
    c.children.push(prim(
        cuboid([0.64, 0.18, 0.66], body.clone()),
        [0.0, 0.15, -0.18],
        id_quat(),
    ));
    // Flared wheel arch over each wheel corner.
    for sx in [-1.0f32, 1.0] {
        for sz in [-1.0f32, 1.0] {
            c.children.push(prim(
                cuboid([0.14, 0.18, 0.42], lower.clone()),
                [sx * 0.4, -0.04, sz * 0.55],
                id_quat(),
            ));
        }
    }
    // Front grille bar + headlights.
    c.children.push(prim(
        cuboid([0.5, 0.07, 0.04], trim.clone()),
        [0.0, 0.04, 0.76],
        id_quat(),
    ));
    for sx in [-1.0f32, 1.0] {
        c.children.push(prim(
            cuboid([0.12, 0.07, 0.04], headlight.clone()),
            [sx * 0.26, 0.08, 0.75],
            id_quat(),
        ));
    }
    // Rear taillights.
    for sx in [-1.0f32, 1.0] {
        c.children.push(prim(
            cuboid([0.1, 0.06, 0.04], taillight.clone()),
            [sx * 0.26, 0.08, -0.74],
            id_quat(),
        ));
    }
    // Side trim strake along each flank.
    for s in [-1.0f32, 1.0] {
        c.children.push(prim(
            cuboid([0.02, 0.04, 1.1], trim.clone()),
            [s * 0.385, 0.0, 0.0],
            id_quat(),
        ));
    }
    c
}

fn canopy(ctx: &PartCtx) -> Generator {
    let glass = ctx.materials.glass(ctx.palette.secondary_accent);
    let frame = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.45));
    // A glazed cabin greenhouse — a glass box with a roof panel and A-pillar
    // framing — rather than a gumball bubble.
    let mut c = prim(cuboid([0.5, 0.2, 0.6], glass), [0.0, 0.0, 0.0], id_quat());
    // Roof panel.
    c.children.push(prim(
        cuboid([0.52, 0.04, 0.5], frame.clone()),
        [0.0, 0.1, -0.02],
        id_quat(),
    ));
    // Front A-pillars framing the windscreen.
    for s in [-1.0f32, 1.0] {
        c.children.push(prim(
            cuboid([0.03, 0.2, 0.03], frame.clone()),
            [s * 0.24, 0.0, 0.28],
            id_quat(),
        ));
    }
    c
}

fn wheel(ctx: &PartCtx) -> Generator {
    // Dark rubber regardless of palette — a wheel reads wrong in accent paint.
    let tyre = ctx.materials.metal([0.07, 0.07, 0.08]);
    let rim = ctx.materials.metal(ctx.palette.secondary_accent);
    let hub = ctx.materials.trim(ctx.palette.tertiary_accent);
    let mut w = prim(cylinder(0.2, 0.15, 16, tyre), [0.0, 0.0, 0.0], id_quat());
    // The assembler lays the wheel on its axle, so detail both faces.
    for s in [-1.0f32, 1.0] {
        // Rim plate recessed inside the tyre.
        w.children.push(prim(
            cylinder(0.14, 0.03, 16, rim.clone()),
            [0.0, s * 0.07, 0.0],
            id_quat(),
        ));
        // Cross spokes across the rim face.
        w.children.push(prim(
            cuboid([0.26, 0.02, 0.04], rim.clone()),
            [0.0, s * 0.08, 0.0],
            id_quat(),
        ));
        w.children.push(prim(
            cuboid([0.04, 0.02, 0.26], rim.clone()),
            [0.0, s * 0.08, 0.0],
            id_quat(),
        ));
        // Hub cap centre.
        w.children.push(prim(
            cylinder(0.05, 0.05, 8, hub.clone()),
            [0.0, s * 0.09, 0.0],
            id_quat(),
        ));
    }
    w
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

static HEAD: FnPart = FnPart {
    slug: "default_head",
    name: "Plain Head",
    slot: PartSlot::Head,
    chassis: HUMANOID,
    build: head,
};
static TORSO: FnPart = FnPart {
    slug: "default_torso",
    name: "Plain Torso",
    slot: PartSlot::Torso,
    chassis: HUMANOID,
    build: torso,
};
static ARM: FnPart = FnPart {
    slug: "default_arm",
    name: "Plain Arm",
    slot: PartSlot::Arm,
    chassis: HUMANOID,
    build: arm,
};
static LEG: FnPart = FnPart {
    slug: "default_leg",
    name: "Plain Leg",
    slot: PartSlot::Leg,
    chassis: HUMANOID,
    build: leg,
};
static HULL: FnPart = FnPart {
    slug: "default_hull",
    name: "Plain Hull",
    slot: PartSlot::Hull,
    chassis: BOAT,
    build: hull,
};
static DECK: FnPart = FnPart {
    slug: "default_deck",
    name: "Plain Deck",
    slot: PartSlot::Deck,
    chassis: BOAT,
    build: deck,
};
static MAST: FnPart = FnPart {
    slug: "default_mast",
    name: "Plain Mast",
    slot: PartSlot::Mast,
    chassis: BOAT,
    build: mast,
};
static ENVELOPE: FnPart = FnPart {
    slug: "default_envelope",
    name: "Plain Envelope",
    slot: PartSlot::Envelope,
    chassis: AIRSHIP,
    build: envelope,
};
static GONDOLA: FnPart = FnPart {
    slug: "default_gondola",
    name: "Plain Gondola",
    slot: PartSlot::Gondola,
    chassis: AIRSHIP,
    build: gondola,
};
static FIN: FnPart = FnPart {
    slug: "default_fin",
    name: "Plain Fin",
    slot: PartSlot::Fin,
    chassis: AIRSHIP,
    build: fin,
};
static CHASSIS: FnPart = FnPart {
    slug: "default_chassis",
    name: "Plain Chassis",
    slot: PartSlot::Chassis,
    chassis: SKIFF,
    build: chassis,
};
static CANOPY: FnPart = FnPart {
    slug: "default_canopy",
    name: "Plain Canopy",
    slot: PartSlot::Canopy,
    chassis: SKIFF,
    build: canopy,
};
static WHEEL: FnPart = FnPart {
    slug: "default_wheel",
    name: "Plain Wheel",
    slot: PartSlot::Wheel,
    chassis: SKIFF,
    build: wheel,
};

/// Every universal default part, in slot order per chassis.
pub(super) static ENTRIES: &[&dyn BodyPart] = &[
    &HEAD, &TORSO, &ARM, &LEG, &HULL, &DECK, &MAST, &ENVELOPE, &GONDOLA, &FIN, &CHASSIS, &CANOPY,
    &WHEEL,
];

// ---------------------------------------------------------------------------
// Airship fin — a swept stabiliser the assembler clusters at the tail.
// ---------------------------------------------------------------------------

fn fin(ctx: &PartCtx) -> Generator {
    // A thin tapered, aft-swept fin centred on its mount; the assembler rotates
    // each copy into a cruciform tail. Centred at the origin (not pre-raised) so
    // the assembler's rotation spins it about its own centre cleanly. Tapered +
    // swept so it reads as a stabiliser, with a glowing trailing edge.
    let mut f = prim(
        with_torture(
            cuboid(
                [0.04, 0.44, 0.5],
                ctx.materials.body(ctx.palette.tertiary_accent),
            ),
            0.0,
            0.5,
            [0.0, 0.0, -0.22],
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Glowing trailing edge along the aft side (-Z).
    f.children.push(prim(
        cuboid(
            [0.05, 0.36, 0.04],
            ctx.materials.glow(ctx.palette.secondary_accent),
        ),
        [0.0, 0.0, -0.22],
        id_quat(),
    ));
    f
}
