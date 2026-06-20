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
    capsule, cone, cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, quat_z, sphere, torus,
    with_torture,
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

    let mut head = prim(sphere(r, 4, skin.clone()), [0.0, 0.0, 0.0], id_quat());

    // Neck — its base disappears into the torso collar (the assembler seats
    // the head just above the shoulders).
    head.children.push(prim(
        cylinder(0.045, 0.14, 10, skin.clone()),
        [0.0, -r - 0.05, 0.0],
        id_quat(),
    ));

    // Eyes + brows. The face is on -Z (the assembler never turns the head).
    for s in [-1.0f32, 1.0] {
        head.children.push(prim(
            sphere(0.024, 2, eye.clone()),
            [s * r * 0.36, r * 0.06, -r * 0.9],
            id_quat(),
        ));
        head.children.push(prim(
            cuboid([0.05, 0.013, 0.02], hair.clone()),
            [s * r * 0.36, r * 0.26, -r * 0.92],
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

    // Hair — a crown cap on every head, plus a per-seed flourish for variety.
    let mut crown = prim(
        sphere(r * 1.04, 4, hair.clone()),
        [0.0, r * 0.26, r * 0.06],
        id_quat(),
    );
    crown.transform.scale = Fp3([1.06, 0.82, 1.12]);
    head.children.push(crown);
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
    torso
}

fn arm(ctx: &PartCtx) -> Generator {
    let r = 0.055 * ctx.body.limb_thickness_scale;
    let len = 0.46;
    let skin = ctx.materials.skin(ctx.palette.skin_tone);
    // Shoulder pivot at the origin; the arm hangs down -Y.
    let mut arm = prim(
        capsule(r, len, skin.clone()),
        [0.0, -len * 0.5, 0.0],
        id_quat(),
    );
    // Short sleeve cap at the shoulder, tying the bare arm to the shirt.
    arm.children.push(prim(
        cylinder(
            r * 1.5,
            0.12,
            12,
            ctx.materials.body(ctx.palette.primary_accent),
        ),
        [0.0, len * 0.5 - 0.05, 0.0],
        id_quat(),
    ));
    // Hand mitt at the wrist (slightly elongated sphere).
    let mut hand = prim(
        sphere(r * 1.5, 3, skin),
        [0.0, -len * 0.5 - 0.02, 0.0],
        id_quat(),
    );
    hand.transform.scale = Fp3([1.0, 1.2, 1.1]);
    arm.children.push(hand);
    arm
}

fn leg(ctx: &PartCtx) -> Generator {
    let r = 0.07 * ctx.body.limb_thickness_scale;
    let len = 0.6;
    // Trousers: a darker shade of the primary so legs read as one outfit
    // with the shirt rather than a clashing accent.
    let trousers = ctx.materials.body(shade(ctx.palette.primary_accent, 0.6));
    let shoe = ctx.materials.body(ctx.palette.secondary_accent);
    // Hip pivot at the origin; the leg hangs down -Y.
    let mut leg = prim(capsule(r, len, trousers), [0.0, -len * 0.5, 0.0], id_quat());
    // Foot — a forward-pointing shoe (-Z is the front).
    let foot = prim(
        cuboid([r * 1.7, 0.06, 0.2], shoe),
        [0.0, -len * 0.5 - 0.02, -0.06],
        id_quat(),
    );
    leg.children.push(foot);
    leg
}

// ---------------------------------------------------------------------------
// Boat
// ---------------------------------------------------------------------------

fn hull(ctx: &PartCtx) -> Generator {
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let rail = ctx.materials.metal(ctx.palette.secondary_accent);
    // Main hull box (structural root — no root scale, see module docstring).
    let mut hull = prim(
        cuboid([0.66, 0.32, 1.7], body.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Pointed prow at the bow (+Z), a 4-sided cone laid forward like
    // `boat_bow_ram`; its base buries inside the hull so there's no seam.
    hull.children.push(prim(
        cone(0.33, 0.7, 4, body),
        [0.0, 0.0, 1.0],
        quat_xyzw(quat_x(-FRAC_PI_2)),
    ));
    // Gunwale rails along each side so the deck reads as inside a hull, not
    // a slab on a pallet.
    for s in [-1.0f32, 1.0] {
        hull.children.push(prim(
            cuboid([0.05, 0.08, 1.6], rail.clone()),
            [s * 0.33, 0.18, 0.0],
            id_quat(),
        ));
    }
    hull
}

fn deck(ctx: &PartCtx) -> Generator {
    // Deck planks sit just inside the hull width (narrower than the hull top).
    let planks = ctx.materials.body(shade(ctx.palette.primary_accent, 0.8));
    let mut deck = prim(
        cuboid([0.62, 0.1, 1.55], planks),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // A low cabin block toward the stern.
    deck.children.push(prim(
        cuboid(
            [0.4, 0.22, 0.5],
            ctx.materials.body(ctx.palette.primary_accent),
        ),
        [0.0, 0.16, -0.45],
        id_quat(),
    ));
    deck
}

fn mast(ctx: &PartCtx) -> Generator {
    let h = 1.4;
    let pole = ctx.materials.metal(ctx.palette.tertiary_accent);
    // Base pivot at the origin; the mast rises +Y.
    let mut mast = prim(
        cylinder(0.045, h, 12, pole.clone()),
        [0.0, h * 0.5, 0.0],
        id_quat(),
    );
    // A fore-and-aft canvas sail (normal ±X) hung off the mast.
    mast.children.push(prim(
        cuboid(
            [0.02, 0.9, 0.85],
            ctx.materials.cloth(ctx.palette.secondary_accent),
        ),
        [0.0, 0.05, -0.1],
        id_quat(),
    ));
    // Boom along the foot of the sail (a cylinder laid onto +Z).
    mast.children.push(prim(
        cylinder(0.03, 0.85, 8, pole),
        [0.0, -h * 0.5 + 0.32, -0.1],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    mast
}

// ---------------------------------------------------------------------------
// Airship
// ---------------------------------------------------------------------------

fn envelope(ctx: &PartCtx) -> Generator {
    let body = ctx.materials.body(ctx.palette.primary_accent);
    // A cigar built from overlapping lobes rather than a scaled sphere, so
    // the assembler's gondola / fins (mounted as children of this root) are
    // not stretched + flung by a root scale.
    let mut env = prim(sphere(0.78, 4, body.clone()), [0.0, 0.0, 0.0], id_quat());
    env.children.push(prim(
        sphere(0.6, 4, body.clone()),
        [0.0, 0.0, 0.75],
        id_quat(),
    ));
    env.children
        .push(prim(sphere(0.62, 4, body), [0.0, 0.0, -0.7], id_quat()));
    // Nose cap accent at the bow (+Z).
    env.children.push(prim(
        sphere(0.2, 3, ctx.materials.trim(ctx.palette.tertiary_accent)),
        [0.0, 0.0, 1.2],
        id_quat(),
    ));
    env
}

fn gondola(ctx: &PartCtx) -> Generator {
    let body = ctx.materials.body(ctx.palette.secondary_accent);
    let mut g = prim(
        cuboid([0.5, 0.3, 1.0], body.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Rounded keel underneath.
    g.children.push(prim(
        cuboid(
            [0.46, 0.12, 0.9],
            ctx.materials.body(shade(ctx.palette.secondary_accent, 0.7)),
        ),
        [0.0, -0.2, 0.0],
        id_quat(),
    ));
    // Lit portholes along each side.
    for s in [-1.0f32, 1.0] {
        for z in [-0.3f32, 0.0, 0.3] {
            g.children.push(prim(
                cylinder(
                    0.05,
                    0.04,
                    10,
                    ctx.materials.glow(ctx.palette.tertiary_accent),
                ),
                [s * 0.26, 0.05, z],
                quat_xyzw(quat_z(FRAC_PI_2)),
            ));
        }
    }
    g
}

// ---------------------------------------------------------------------------
// Skiff
// ---------------------------------------------------------------------------

fn chassis(ctx: &PartCtx) -> Generator {
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let skirt = ctx.materials.body(shade(ctx.palette.primary_accent, 0.55));
    // Body slab (structural root — no root scale).
    let mut c = prim(
        cuboid([0.8, 0.22, 1.6], body.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Lower bumper skirt.
    c.children.push(prim(
        cuboid([0.84, 0.1, 1.55], skirt),
        [0.0, -0.14, 0.0],
        id_quat(),
    ));
    // Raised cabin toward the rear (the canopy seats on this).
    c.children.push(prim(
        cuboid([0.6, 0.26, 0.7], body.clone()),
        [0.0, 0.22, -0.15],
        id_quat(),
    ));
    // Hood line at the front.
    c.children.push(prim(
        cuboid([0.7, 0.12, 0.5], body),
        [0.0, 0.1, 0.55],
        id_quat(),
    ));
    c
}

fn canopy(ctx: &PartCtx) -> Generator {
    let mut c = prim(
        sphere(0.3, 3, ctx.materials.glass(ctx.palette.secondary_accent)),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Flatten the sphere into a windshield bubble rather than a gumball.
    c.transform.scale = Fp3([1.0, 0.62, 1.15]);
    c
}

fn wheel(ctx: &PartCtx) -> Generator {
    // Dark rubber regardless of palette — a wheel reads wrong in accent paint.
    let tyre = ctx.materials.metal([0.07, 0.07, 0.08]);
    let hub = ctx.materials.trim(ctx.palette.tertiary_accent);
    let mut w = prim(cylinder(0.2, 0.16, 16, tyre), [0.0, 0.0, 0.0], id_quat());
    // Hubcaps on both faces — the assembler lays the wheel on its axle, so a
    // single-side cap would face inward on one side of the car.
    for s in [-1.0f32, 1.0] {
        w.children.push(prim(
            cylinder(0.09, 0.03, 12, hub.clone()),
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
    // A thin tapered fin centred on its mount; the assembler rotates each
    // copy into a cruciform tail. Centred at the origin (not pre-raised) so
    // the assembler's rotation spins it about its own centre cleanly.
    // Tapered so it reads as a swept stabiliser rather than a floating square.
    prim(
        with_torture(
            cuboid(
                [0.04, 0.42, 0.5],
                ctx.materials.body(ctx.palette.tertiary_accent),
            ),
            0.0,
            0.45,
            [0.0, 0.0, 0.0],
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    )
}
