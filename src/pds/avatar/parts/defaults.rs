//! Universal default parts — one per required slot per chassis, eligible
//! for every style (empty [`BodyPart::styles`]).
//!
//! These are the **coverage floor**: they guarantee every required
//! (chassis, slot) is fillable for any style/tier so the outfit deriver
//! never stalls on an unfillable slot while the styled kits
//! (`super`'s `#518`/`#519` content) fill in. The geometry is deliberately
//! plain — a readable silhouette built from the shared primitive
//! vocabulary and finished through the seeded
//! [`MaterialKit`](crate::seeded_defaults::MaterialKit) — not the
//! refined styled parts. Each builds in its slot's local attachment frame
//! (see the module docstring on [`super`]).

use crate::pds::avatar::default_visuals::common::{
    capsule, cuboid, cylinder, id_quat, prim, sphere,
};
use crate::pds::generator::Generator;
use crate::seeded_defaults::ChassisFamily;

use super::{BodyPart, PartCtx, PartSlot};

const HUMANOID: &[ChassisFamily] = &[ChassisFamily::Humanoid];
const BOAT: &[ChassisFamily] = &[ChassisFamily::Boat];
const AIRSHIP: &[ChassisFamily] = &[ChassisFamily::Airship];
const SKIFF: &[ChassisFamily] = &[ChassisFamily::Skiff];

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
    let mut head = prim(
        sphere(r, 3, ctx.materials.skin(ctx.palette.skin_tone)),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    for side in [-1.0f32, 1.0] {
        head.children.push(prim(
            sphere(r * 0.16, 2, ctx.materials.cloth(ctx.palette.eye_color)),
            [side * r * 0.34, r * 0.1, -r * 0.88],
            id_quat(),
        ));
    }
    head
}

fn torso(ctx: &PartCtx) -> Generator {
    let r = 0.155 * ctx.body.shoulder_width_scale;
    prim(
        capsule(r, 0.5, ctx.materials.body(ctx.palette.primary_accent)),
        [0.0, 0.0, 0.0],
        id_quat(),
    )
}

fn arm(ctx: &PartCtx) -> Generator {
    let r = 0.058 * ctx.body.limb_thickness_scale;
    let len = 0.5;
    // Shoulder pivot at the origin; the arm hangs down -Y.
    prim(
        capsule(r, len, ctx.materials.skin(ctx.palette.skin_tone)),
        [0.0, -len * 0.5, 0.0],
        id_quat(),
    )
}

fn leg(ctx: &PartCtx) -> Generator {
    let r = 0.07 * ctx.body.limb_thickness_scale;
    let len = 0.6;
    // Hip pivot at the origin; the leg hangs down -Y.
    prim(
        capsule(r, len, ctx.materials.body(ctx.palette.secondary_accent)),
        [0.0, -len * 0.5, 0.0],
        id_quat(),
    )
}

// ---------------------------------------------------------------------------
// Boat
// ---------------------------------------------------------------------------

fn hull(ctx: &PartCtx) -> Generator {
    prim(
        cuboid(
            [0.7, 0.3, 2.2],
            ctx.materials.body(ctx.palette.secondary_accent),
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    )
}

fn deck(ctx: &PartCtx) -> Generator {
    prim(
        cuboid(
            [1.2, 0.12, 1.8],
            ctx.materials.body(ctx.palette.primary_accent),
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    )
}

fn mast(ctx: &PartCtx) -> Generator {
    let h = 1.4;
    // Base pivot at the origin; the mast rises +Y.
    prim(
        cylinder(
            0.05,
            h,
            12,
            ctx.materials.metal(ctx.palette.tertiary_accent),
        ),
        [0.0, h * 0.5, 0.0],
        id_quat(),
    )
}

// ---------------------------------------------------------------------------
// Airship
// ---------------------------------------------------------------------------

fn envelope(ctx: &PartCtx) -> Generator {
    let mut env = prim(
        sphere(0.8, 3, ctx.materials.body(ctx.palette.primary_accent)),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Stretch the gas-bag fore-aft into a cigar.
    env.transform.scale = crate::pds::types::Fp3([1.0, 1.0, 1.8]);
    env
}

fn gondola(ctx: &PartCtx) -> Generator {
    prim(
        cuboid(
            [0.5, 0.3, 1.0],
            ctx.materials.body(ctx.palette.secondary_accent),
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    )
}

fn fin(ctx: &PartCtx) -> Generator {
    prim(
        cuboid(
            [0.05, 0.5, 0.4],
            ctx.materials.body(ctx.palette.tertiary_accent),
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    )
}

// ---------------------------------------------------------------------------
// Skiff
// ---------------------------------------------------------------------------

fn chassis(ctx: &PartCtx) -> Generator {
    prim(
        cuboid(
            [0.8, 0.25, 1.6],
            ctx.materials.body(ctx.palette.primary_accent),
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    )
}

fn canopy(ctx: &PartCtx) -> Generator {
    prim(
        sphere(0.35, 3, ctx.materials.glass(ctx.palette.secondary_accent)),
        [0.0, 0.0, 0.0],
        id_quat(),
    )
}

fn wheel(ctx: &PartCtx) -> Generator {
    // Dark rubber regardless of palette — a wheel reads wrong in accent paint.
    prim(
        cylinder(0.25, 0.18, 16, ctx.materials.metal([0.08, 0.08, 0.09])),
        [0.0, 0.0, 0.0],
        id_quat(),
    )
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
