//! Universal default parts — at least one per required slot per chassis,
//! eligible for every style (empty [`BodyPart::styles`]).
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
    capsule, cone, cuboid, cylinder, id_quat, prim, quat_mul, quat_x, quat_xyzw, quat_z, sphere,
    torus, with_cut, with_torture,
};
use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
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

    // Neck — a tapered column flaring at its base (trapezius) so it rises from
    // the shoulders instead of floating; its base sinks into the torso collar
    // (the assembler seats the head a clear neck-length above the shoulders).
    head.children.push(prim(
        with_torture(
            cylinder(0.052, 0.18, 10, skin.clone()),
            0.0,
            0.62,
            [0.0, 0.0, 0.0],
        ),
        [0.0, -r - 0.055, 0.0],
        id_quat(),
    ));

    // Eyes + brows. The face is on -Z (the assembler never turns the head).
    for s in [-1.0f32, 1.0] {
        // White sclera in a shallow socket with a smaller dark iris in front,
        // so each eye reads as an eye instead of merging with the brow into a
        // single dark bar (the old same-tone eye+brow pairing).
        let mut socket = prim(
            sphere(0.028, 2, sclera.clone()),
            [s * r * 0.37, r * 0.0, -r * 0.88],
            id_quat(),
        );
        socket.children.push(prim(
            sphere(0.017, 2, eye.clone()),
            [0.0, 0.0, -0.018],
            id_quat(),
        ));
        head.children.push(socket);
        // Brow — thin and lifted clear of the eye.
        head.children.push(prim(
            cuboid([0.046, 0.010, 0.018], hair.clone()),
            [s * r * 0.37, r * 0.28, -r * 0.92],
            id_quat(),
        ));
    }
    // Nose nub + mouth.
    head.children.push(prim(
        cuboid([0.026, 0.04, 0.05], skin.clone()),
        [0.0, -r * 0.10, -r * 0.96],
        id_quat(),
    ));
    head.children.push(prim(
        cuboid(
            [0.055, 0.016, 0.02],
            ctx.materials.cloth(shade(ctx.palette.skin_tone, 0.5)),
        ),
        [0.0, -r * 0.44, -r * 0.88],
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
    // A single profile-cut dome: the cut-latitude rim *is* the hairline, so a
    // backward tilt sweeps it up at the brow and down at the nape — one clean
    // prim replacing the old cap + back-mass + temples stack. The kept band
    // reaches a little below the equator so it wraps the sides / back of the
    // head; the dome sits a touch larger than the skull to read as hair.
    // Crown cap (a flattened sphere seated high and tilted back so the front rim
    // lifts to the forehead). NB: a single profile-cut dome was trialled here and
    // rejected — its one flat rim can't both expose the forehead and cover the
    // nape, and it leaves a seam against the back mass; the multi-mass below
    // reads as hair far better. The cut prims earn their keep in the catalogue.
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
                    cuboid([r * 1.5, r * 2.0, 0.05], hair),
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

fn torso(ctx: &PartCtx) -> Generator {
    let r = 0.155 * ctx.body.shoulder_width_scale;
    let shirt = ctx.materials.body(ctx.palette.primary_accent);
    let collar = ctx.materials.trim(ctx.palette.secondary_accent);
    let belt = ctx.materials.trim(ctx.palette.tertiary_accent);
    // Tapered trunk — a negative taper flares the top, reading as a chest that
    // narrows to the waist.
    let mut torso = prim(
        with_torture(capsule(r, 0.5, shirt.clone()), 0.0, -0.12, [0.0, 0.0, 0.0]),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Shoulder yoke — a wide, flattened ellipsoid laid across the top of the
    // trunk, sloping down to the arm mounts. This is what gives the figure
    // real shoulders: the arm parts now carry only a small deltoid cap, so
    // without the yoke they read as ball pauldrons pinned to a tube. Built
    // from a unit sphere + scale (kept off the structural-root rule since the
    // torso isn't a family root).
    let mut yoke = prim(sphere(1.0, 3, shirt.clone()), [0.0, 0.21, 0.0], id_quat());
    yoke.transform.scale = Fp3([r * 1.62, r * 0.44, r * 0.95]);
    torso.children.push(yoke);
    // Collar ring at the neckline — a small secondary-accent band.
    torso.children.push(prim(
        torus(0.02, r * 0.55, collar.clone()),
        [0.0, 0.31, 0.0],
        id_quat(),
    ));
    // Centre placket down the chest — a narrow front seam (reads as a button
    // line, not a panel), seated just clear of and below the pfp badge.
    torso.children.push(prim(
        cuboid([r * 0.14, 0.22, 0.015], collar),
        [0.0, -0.02, -(r + 0.006)],
        id_quat(),
    ));
    // Belt at the waist — gives the trunk a waistline instead of a smooth tube.
    torso.children.push(prim(
        torus(0.02, r * 0.95, belt),
        [0.0, -0.16, 0.0],
        id_quat(),
    ));
    torso
}

/// A second universal torso — a buttoned coat (stand collar, lapel V, button
/// row) so the bare-required-slot population isn't all the plain shirt. Builds
/// to the same centred frame + shoulder yoke as [`torso`], so the assembler
/// mounts arms / head identically.
fn coat(ctx: &PartCtx) -> Generator {
    let r = 0.155 * ctx.body.shoulder_width_scale;
    let shell = ctx.materials.body(ctx.palette.primary_accent);
    let lining = ctx.materials.cloth(shade(ctx.palette.primary_accent, 0.6));
    let collar = ctx.materials.trim(ctx.palette.secondary_accent);
    let btn = ctx.materials.trim(ctx.palette.tertiary_accent);
    // Trunk — a slightly straighter taper than the shirt so the coat reads
    // boxier / heavier.
    let mut torso = prim(
        with_torture(capsule(r, 0.52, shell.clone()), 0.0, -0.10, [0.0, 0.0, 0.0]),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Shoulder yoke (as the shirt) — a touch broader for the coat's bulk.
    let mut yoke = prim(sphere(1.0, 3, shell.clone()), [0.0, 0.22, 0.0], id_quat());
    yoke.transform.scale = Fp3([r * 1.67, r * 0.46, r * 0.98]);
    torso.children.push(yoke);
    // Lapel V — two lining-colour strips angled outward at the throat, meeting
    // low on the chest, so the coat reads as open-collared over a shirt.
    for s in [-1.0f32, 1.0] {
        torso.children.push(prim(
            cuboid([0.03, 0.34, 0.02], lining.clone()),
            [s * r * 0.30, 0.03, -(r + 0.005)],
            quat_xyzw(quat_z(s * 0.35)),
        ));
    }
    // Stand collar — a short ring standing at the neckline.
    torso.children.push(prim(
        cylinder(r * 0.6, 0.09, 12, collar),
        [0.0, 0.31, 0.0],
        id_quat(),
    ));
    // Button row down the centre.
    for y in [0.10f32, -0.02, -0.14] {
        torso.children.push(prim(
            sphere(0.014, 2, btn.clone()),
            [0.0, y, -(r + 0.012)],
            id_quat(),
        ));
    }
    // Belt at the waist.
    torso.children.push(prim(
        torus(0.022, r * 0.96, btn),
        [0.0, -0.16, 0.0],
        id_quat(),
    ));
    torso
}

fn arm(ctx: &PartCtx) -> Generator {
    let r = 0.05 * ctx.body.limb_thickness_scale;
    let (l1, l2) = (0.25, 0.22); // upper arm, forearm
    let theta = 0.18_f32; // gentle elbow bend forward (front is -Z) — relaxed
    let skin = ctx.materials.skin(ctx.palette.skin_tone);
    let sleeve = ctx.materials.body(ctx.palette.primary_accent);
    let cuff = ctx.materials.trim(ctx.palette.secondary_accent);

    // A true kinematic chain: shoulder → upper arm → elbow → forearm → hand,
    // each segment a *child* of the one above and pinned to its parent's far
    // end, so a joint rotation propagates down the chain (the assembler's
    // shoulder splay swings the whole arm; the elbow bend swings forearm +
    // hand together) and each segment is authored in its own local frame.

    // Shoulder root = a compact deltoid cap (shirt colour). Kept small + flat
    // (the torso's shoulder yoke carries the bulk) so it just rounds off where
    // the arm meets the yoke instead of protruding as a ball pauldron in
    // profile. The pivot the assembler rotates the whole arm about.
    let mut shoulder = prim(
        sphere(r * 0.95, 3, sleeve.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    shoulder.transform.scale = Fp3([0.92, 0.62, 0.82]);

    // Upper arm (shoulder → elbow): bare-skin capsule centred at -l1/2, a
    // direct child of the shoulder.
    let mut upper = prim(
        capsule(r, l1, skin.clone()),
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
        sphere(r * 1.0, 2, skin.clone()),
        [0.0, -l1 * 0.5, 0.0],
        quat_xyzw(quat_x(theta)),
    );
    // Forearm (elbow → wrist): child of the elbow, centred at -l2/2 in the
    // (already bent) elbow frame.
    let mut forearm = prim(
        capsule(r * 0.86, l2, skin.clone()),
        [0.0, -l2 * 0.5, 0.0],
        id_quat(),
    );
    // Wrist cuff at the forearm's far end.
    forearm.children.push(prim(
        cylinder(r * 0.96, 0.03, 8, cuff),
        [0.0, -l2 * 0.5, 0.0],
        id_quat(),
    ));
    // Hand: palm + a cupped finger block just past the wrist. Kept left/right
    // symmetric — the assembler mirrors the single arm by rotation, not
    // reflection, so an offset thumb would face the wrong way on one side.
    let mut palm = prim(
        cuboid([r * 1.5, r * 1.4, r * 0.8], skin.clone()),
        [0.0, -l2 * 0.5 - r * 0.75, 0.0],
        id_quat(),
    );
    palm.children.push(prim(
        cuboid([r * 1.4, r * 1.05, r * 0.55], skin),
        [0.0, -r * 1.25, -r * 0.06],
        id_quat(),
    ));
    forearm.children.push(palm);
    elbow.children.push(forearm);
    upper.children.push(elbow);
    shoulder.children.push(upper);
    shoulder
}

fn leg(ctx: &PartCtx) -> Generator {
    // Thicker + longer than a token limb so the legs carry the torso instead
    // of reading as thin pipes under a barrel (the "lollipop" silhouette).
    let r = 0.072 * ctx.body.limb_thickness_scale;
    let (l1, l2) = (0.36, 0.33); // thigh, shin
    let theta = 0.13_f32; // knee bend forward — strong enough to read in profile
    // Trousers: a darker shade of the primary so legs read as one outfit with
    // the shirt rather than a clashing accent.
    let trousers = ctx.materials.body(shade(ctx.palette.primary_accent, 0.6));
    let shoe = ctx.materials.body(ctx.palette.secondary_accent);

    // Kinematic chain mirroring the arm: hip → thigh → knee → shin → foot,
    // each segment a child pinned to its parent's far end, so the knee bend
    // carries the shin + foot together.

    // Hip root = hip joint at the origin (the assembler's hip pivot).
    let mut hip = prim(
        sphere(r * 1.18, 2, trousers.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Thigh (hip → knee).
    let mut thigh = prim(
        capsule(r, l1, trousers.clone()),
        [0.0, -l1 * 0.5, 0.0],
        id_quat(),
    );
    // Knee node: child of the thigh, at its far end, carrying the forward bend.
    let mut knee = prim(
        sphere(r * 1.02, 2, trousers.clone()),
        [0.0, -l1 * 0.5, 0.0],
        quat_xyzw(quat_x(theta)),
    );
    // Shin (knee → ankle): child of the knee, centred at -l2/2 in the bent frame.
    let mut shin = prim(
        capsule(r * 0.9, l2, trousers.clone()),
        [0.0, -l2 * 0.5, 0.0],
        id_quat(),
    );
    // Trouser cuff at the ankle.
    shin.children.push(prim(
        cylinder(r * 1.05, 0.04, 8, trousers),
        [0.0, -l2 * 0.5 + 0.02, 0.0],
        id_quat(),
    ));
    // Shoe — a forward-pointing shoe at the ankle (-Z is the front): a thin
    // dark sole biased forward (so it doesn't jut behind the heel) carrying a
    // single rounded upper (a capsule laid along the foot) + a toe cap, reading
    // as one clean shoe rather than a slab + blob stack. Child of the shin (so
    // it tracks the knee bend), its upper seated high enough to swallow the
    // shin/ankle seam.
    let sole = ctx
        .materials
        .metal(shade(ctx.palette.secondary_accent, 0.45));
    let mut foot = prim(
        cuboid([r * 1.3, 0.03, 0.19], sole),
        [0.0, -l2 * 0.5 - 0.055, -0.07],
        id_quat(),
    );
    // Rounded upper laid horizontally along the foot (capsule axis Y → Z),
    // seated low so it swallows the thin sole rather than perching on it.
    let mut upper = prim(
        capsule(0.055, 0.1, shoe.clone()),
        [0.0, 0.04, -0.03],
        quat_xyzw(quat_x(FRAC_PI_2)),
    );
    upper.transform.scale = Fp3([1.1, 1.0, 1.0]);
    foot.children.push(upper);
    // Toe cap rounding the front.
    let mut toe = prim(sphere(0.05, 2, shoe), [0.0, 0.045, -0.12], id_quat());
    toe.transform.scale = Fp3([1.25, 0.85, 1.0]);
    foot.children.push(toe);
    shin.children.push(foot);
    knee.children.push(shin);
    thigh.children.push(knee);
    hip.children.push(thigh);
    hip
}

// ---------------------------------------------------------------------------
// Boat
// ---------------------------------------------------------------------------

/// A hidden structural core for a boat hull at the waterline origin. The boat
/// assembler overwrites the root transform (travel yaw + hover drop) and mounts
/// the deck / mast / bow to it, so the root must stay an **unscaled** cuboid;
/// the visible hull is built from its children.
fn boat_root(body: &SovereignMaterialSettings) -> Generator {
    prim(
        cuboid([0.2, 0.14, 0.9], body.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    )
}

/// Placement + dimensions for one boat hull built by [`boat_hull_body`].
#[derive(Clone, Copy)]
struct HullSpec {
    /// Lateral offset of this hull from the centreline.
    x: f32,
    /// Beam (full width).
    beam: f32,
    /// Overall hull length.
    length: f32,
    /// Above-waterline height (freeboard).
    freeboard: f32,
}

/// Build one boat hull into `parent` at the spec's lateral offset: an
/// above-waterline topsides box, a pointed cone prow at the bow (+Z) flattened
/// to the hull's section, a dark below-waterline belly, and a waterline boot
/// stripe. Shared by the monohull, the catamaran's two pontoons, and the
/// trimaran's hulls so every form reads as the same vessel, just arranged
/// differently.
fn boat_hull_body(
    parent: &mut Generator,
    body: &SovereignMaterialSettings,
    below: &SovereignMaterialSettings,
    stripe: &SovereignMaterialSettings,
    spec: HullSpec,
) {
    let HullSpec {
        x,
        beam,
        length,
        freeboard,
    } = spec;
    // A short aft topsides box leaves the forward ~40 % of the hull to the prow,
    // so the pointed bow — not a flat full-beam box wall — is what's seen
    // head-on. A gentle flare (negative taper) keeps the deck off a plain slab.
    let box_len = length * 0.58;
    let z_off = -length * 0.12;
    parent.children.push(prim(
        with_torture(
            cuboid([beam, freeboard, box_len], body.clone()),
            0.0,
            -0.1,
            [0.0, 0.0, 0.0],
        ),
        [x, freeboard * 0.15, z_off],
        id_quat(),
    ));
    // Pointed cone prow (apex +Z) forming the forward hull: its base meets the
    // box front (a touch wider, so it caps the box rather than leaving a flat
    // wall) and it tapers to the bow tip, so the craft reads pointed from
    // head-on. quat_x(+90°) sends the cone apex (+Y) to +Z; the node Z-scale
    // squashes the round section to the hull's freeboard.
    let mut prow = prim(
        cone(beam * 0.52, length * 0.52, 14, body.clone()),
        [x, freeboard * 0.22, length * 0.43],
        quat_xyzw(quat_x(FRAC_PI_2)),
    );
    prow.transform.scale = Fp3([0.96, 1.0, freeboard / (beam * 1.04)]);
    parent.children.push(prow);
    // Dark below-waterline belly — a shallow V-keel (negative taper flares the
    // waterline + narrows the keel) so the underbody reads as a hull bottom,
    // not a flat skid plate.
    parent.children.push(prim(
        with_torture(
            cuboid([beam * 0.8, freeboard * 0.78, box_len], below.clone()),
            0.0,
            -0.35,
            [0.0, 0.0, 0.0],
        ),
        [x, -freeboard * 0.42, z_off],
        id_quat(),
    ));
    // Waterline boot stripe down each flank.
    for s in [-1.0f32, 1.0] {
        parent.children.push(prim(
            cuboid([0.016, freeboard * 0.18, box_len * 0.85], stripe.clone()),
            [x + s * beam * 0.5, -freeboard * 0.05, z_off],
            id_quat(),
        ));
    }
}

fn hull(ctx: &PartCtx) -> Generator {
    // Monohull — a single sleek launch hull with gunwale rails.
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let below = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.4));
    let stripe = ctx.materials.accent(ctx.palette.secondary_accent);
    let rail = ctx.materials.metal(ctx.palette.tertiary_accent);

    let mut root = boat_root(&body);
    boat_hull_body(
        &mut root,
        &body,
        &below,
        &stripe,
        HullSpec {
            x: 0.0,
            beam: 0.5,
            length: 1.32,
            freeboard: 0.26,
        },
    );
    // Gunwale rails along each deck edge.
    for s in [-1.0f32, 1.0] {
        root.children.push(prim(
            cuboid([0.03, 0.035, 0.98], rail.clone()),
            [s * 0.24, 0.17, -0.06],
            id_quat(),
        ));
    }
    root
}

fn hull_catamaran(ctx: &PartCtx) -> Generator {
    // Catamaran — two slim pontoon hulls under a connecting deck bridge.
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let below = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.4));
    let stripe = ctx.materials.accent(ctx.palette.secondary_accent);
    let bridge = ctx.materials.body(shade(ctx.palette.primary_accent, 0.8));

    let mut root = boat_root(&body);
    // Two slim pontoon hulls set well apart so the twin-hull tunnel reads.
    for s in [-1.0f32, 1.0] {
        boat_hull_body(
            &mut root,
            &body,
            &below,
            &stripe,
            HullSpec {
                x: s * 0.33,
                beam: 0.24,
                length: 1.24,
                freeboard: 0.2,
            },
        );
    }
    // An *open* bridge — a narrow centre deck spanning the tunnel plus two
    // cross-beams reaching the outer hulls — rather than a slab that buries the
    // catamaran's defining gap.
    root.children.push(prim(
        cuboid([0.34, 0.07, 0.62], bridge.clone()),
        [0.0, 0.13, -0.05],
        id_quat(),
    ));
    for z in [0.3f32, -0.4] {
        root.children.push(prim(
            cuboid([0.72, 0.05, 0.09], bridge.clone()),
            [0.0, 0.1, z],
            id_quat(),
        ));
    }
    root
}

fn hull_trimaran(ctx: &PartCtx) -> Generator {
    // Trimaran — a central main hull flanked by two small outrigger amas on
    // cross-beams.
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let below = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.4));
    let stripe = ctx.materials.accent(ctx.palette.secondary_accent);
    let beam_mat = ctx.materials.metal(ctx.palette.tertiary_accent);

    let mut root = boat_root(&body);
    boat_hull_body(
        &mut root,
        &body,
        &below,
        &stripe,
        HullSpec {
            x: 0.0,
            beam: 0.42,
            length: 1.32,
            freeboard: 0.26,
        },
    );
    for s in [-1.0f32, 1.0] {
        boat_hull_body(
            &mut root,
            &body,
            &below,
            &stripe,
            HullSpec {
                x: s * 0.44,
                beam: 0.14,
                length: 0.82,
                freeboard: 0.13,
            },
        );
        // Cross-beam (aka) tying the ama to the main hull.
        root.children.push(prim(
            cuboid([0.4, 0.04, 0.08], beam_mat.clone()),
            [s * 0.24, 0.1, 0.06],
            id_quat(),
        ));
    }
    root
}

fn hull_barge(ctx: &PartCtx) -> Generator {
    // Barge — a wide, flat, boxy hull with raked punt ends and gunwale walls.
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let below = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.4));
    let wall = ctx.materials.body(shade(ctx.palette.primary_accent, 0.85));
    let rail = ctx.materials.metal(ctx.palette.secondary_accent);

    let mut root = boat_root(&body);
    // Wide flat hull box.
    root.children.push(prim(
        cuboid([0.72, 0.22, 1.2], body.clone()),
        [0.0, 0.03, 0.0],
        id_quat(),
    ));
    // Dark flat bottom.
    root.children.push(prim(
        cuboid([0.66, 0.1, 1.12], below.clone()),
        [0.0, -0.12, 0.0],
        id_quat(),
    ));
    // Raked punt ends (bow lifts forward, stern lifts aft).
    for (z, ang) in [(0.66f32, -0.5f32), (-0.62, 0.5)] {
        root.children.push(prim(
            cuboid([0.7, 0.04, 0.34], body.clone()),
            [0.0, 0.08, z],
            quat_xyzw(quat_x(ang)),
        ));
    }
    // Gunwale walls around the deck perimeter.
    for s in [-1.0f32, 1.0] {
        root.children.push(prim(
            cuboid([0.04, 0.1, 1.16], wall.clone()),
            [s * 0.34, 0.13, 0.0],
            id_quat(),
        ));
    }
    root.children.push(prim(
        cuboid([0.66, 0.1, 0.04], wall),
        [0.0, 0.13, -0.6],
        id_quat(),
    ));
    // Rubbing rail down each flank.
    for s in [-1.0f32, 1.0] {
        root.children.push(prim(
            cuboid([0.03, 0.04, 1.18], rail.clone()),
            [s * 0.37, 0.04, 0.0],
            id_quat(),
        ));
    }
    root
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
    // A short boat mast: a slightly aft-raked pole rising from the deck pivot
    // (origin) with a spreader crossbar and a masthead nav light.
    let pole = ctx.materials.metal(ctx.palette.secondary_accent);
    let light = ctx.materials.glow(ctx.palette.tertiary_accent);

    let mut root = prim(
        cylinder(0.018, 0.42, 8, pole.clone()),
        [0.0, 0.21, 0.0],
        quat_xyzw(quat_x(-0.05)),
    );
    // Spreader crossbar near the top.
    root.children.push(prim(
        cuboid([0.26, 0.02, 0.02], pole),
        [0.0, 0.12, 0.0],
        id_quat(),
    ));
    // Masthead nav light.
    root.children
        .push(prim(sphere(0.03, 2, light), [0.0, 0.23, 0.0], id_quat()));
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
    // Rounded fender arching over each wheel — a path-cut half-cylinder (open
    // underneath) laid on the axle (X), so the mudguard follows the tyre's
    // curve instead of a square box. The half-tube's arch apex is the +Z
    // semicircle, so after laying it on the axle (`quat_z`) we roll it
    // `-FRAC_PI_2` about that axle to lift the apex up over the wheel (+Y) with
    // the open side facing the ground, rather than arching sideways.
    for sx in [-1.0f32, 1.0] {
        for sz in [-1.0f32, 1.0] {
            let fender = with_cut(
                cylinder(0.27, 0.16, 16, lower.clone()),
                [0.0, 0.5],
                [0.0, 1.0],
                0.0,
            );
            c.children.push(prim(
                fender,
                [sx * 0.42, -0.04, sz * 0.55],
                quat_xyzw(quat_mul(quat_x(-FRAC_PI_2), quat_z(FRAC_PI_2))),
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
    // Tyre: a torus gives a rounded tread cross-section — a real tyre, not a
    // flat-sided disc (outer radius ≈ major + minor).
    let mut w = prim(torus(0.06, 0.15, tyre), [0.0, 0.0, 0.0], id_quat());
    // Rim plate filling the hub (shares the torus axis; the assembler lays the
    // whole wheel onto its axle).
    let mut rim_disc = prim(
        cylinder(0.11, 0.12, 16, rim.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    for s in [-1.0f32, 1.0] {
        // Cross spokes + hub cap on each rim face.
        rim_disc.children.push(prim(
            cuboid([0.2, 0.02, 0.04], rim.clone()),
            [0.0, s * 0.06, 0.0],
            id_quat(),
        ));
        rim_disc.children.push(prim(
            cuboid([0.04, 0.02, 0.2], rim.clone()),
            [0.0, s * 0.06, 0.0],
            id_quat(),
        ));
        rim_disc.children.push(prim(
            cylinder(0.045, 0.04, 8, hub.clone()),
            [0.0, s * 0.07, 0.0],
            id_quat(),
        ));
    }
    w.children.push(rim_disc);
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
static COAT: FnPart = FnPart {
    slug: "default_torso_coat",
    name: "Buttoned Coat",
    slot: PartSlot::Torso,
    chassis: HUMANOID,
    build: coat,
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
    name: "Monohull",
    slot: PartSlot::Hull,
    chassis: BOAT,
    build: hull,
};
static HULL_CATAMARAN: FnPart = FnPart {
    slug: "default_hull_catamaran",
    name: "Catamaran",
    slot: PartSlot::Hull,
    chassis: BOAT,
    build: hull_catamaran,
};
static HULL_TRIMARAN: FnPart = FnPart {
    slug: "default_hull_trimaran",
    name: "Trimaran",
    slot: PartSlot::Hull,
    chassis: BOAT,
    build: hull_trimaran,
};
static HULL_BARGE: FnPart = FnPart {
    slug: "default_hull_barge",
    name: "Barge",
    slot: PartSlot::Hull,
    chassis: BOAT,
    build: hull_barge,
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
    &HEAD,
    &TORSO,
    &COAT,
    &ARM,
    &LEG,
    &HULL,
    &HULL_CATAMARAN,
    &HULL_TRIMARAN,
    &HULL_BARGE,
    &DECK,
    &MAST,
    &ENVELOPE,
    &GONDOLA,
    &FIN,
    &CHASSIS,
    &CANOPY,
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
