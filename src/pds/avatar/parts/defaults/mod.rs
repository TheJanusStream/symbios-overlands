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

// Crate-visible so the airship assembler (rigging-cable colour) + the styled
// teardrop envelope (`super::vehicle`) can share its two-hue colour scheme,
// matte envelope material, normalized window colour, and gore-seam helper
// (#789).
pub(crate) mod airship;
mod boat;
mod common;
mod humanoid;
// Crate-visible so the land-skiff assembler + the styled chassis / wheel
// variants (`super::vehicle`) can share its blueprint-derived dims, colour
// scheme, and wheel-anchor / fender contract (#788).
pub(crate) mod skiff;

use crate::seeded_defaults::ChassisFamily;

use super::{BodyPart, PartSlot};
use airship::*;
use boat::*;
use common::FnPart;
use humanoid::*;
use skiff::*;

const HUMANOID: &[ChassisFamily] = &[ChassisFamily::Humanoid];
const BOAT: &[ChassisFamily] = &[ChassisFamily::Boat];
const AIRSHIP: &[ChassisFamily] = &[ChassisFamily::Airship];
const SKIFF: &[ChassisFamily] = &[ChassisFamily::Skiff];

static HEAD: FnPart = FnPart {
    slug: "default_head",
    slot: PartSlot::Head,
    chassis: HUMANOID,
    build: head,
};
static TORSO: FnPart = FnPart {
    slug: "default_torso",
    slot: PartSlot::Torso,
    chassis: HUMANOID,
    build: torso,
};
static COAT: FnPart = FnPart {
    slug: "default_torso_coat",
    slot: PartSlot::Torso,
    chassis: HUMANOID,
    build: coat,
};
static ARM: FnPart = FnPart {
    slug: "default_arm",
    slot: PartSlot::Arm,
    chassis: HUMANOID,
    build: arm,
};
static LEG: FnPart = FnPart {
    slug: "default_leg",
    slot: PartSlot::Leg,
    chassis: HUMANOID,
    build: leg,
};
static HULL: FnPart = FnPart {
    slug: "default_hull",
    slot: PartSlot::Hull,
    chassis: BOAT,
    build: hull,
};
static HULL_CATAMARAN: FnPart = FnPart {
    slug: "default_hull_catamaran",
    slot: PartSlot::Hull,
    chassis: BOAT,
    build: hull_catamaran,
};
static HULL_TRIMARAN: FnPart = FnPart {
    slug: "default_hull_trimaran",
    slot: PartSlot::Hull,
    chassis: BOAT,
    build: hull_trimaran,
};
static HULL_BARGE: FnPart = FnPart {
    slug: "default_hull_barge",
    slot: PartSlot::Hull,
    chassis: BOAT,
    build: hull_barge,
};
static DECK: FnPart = FnPart {
    slug: "default_deck",
    slot: PartSlot::Deck,
    chassis: BOAT,
    build: deck,
};
static MAST: FnPart = FnPart {
    slug: "default_mast",
    slot: PartSlot::Mast,
    chassis: BOAT,
    build: mast,
};
static ENVELOPE: FnPart = FnPart {
    slug: "default_envelope",
    slot: PartSlot::Envelope,
    chassis: AIRSHIP,
    build: envelope,
};
static ENVELOPE_BLIMP: FnPart = FnPart {
    slug: "default_envelope_blimp",
    slot: PartSlot::Envelope,
    chassis: AIRSHIP,
    build: envelope_blimp,
};
static ENVELOPE_LOBED: FnPart = FnPart {
    slug: "default_envelope_lobed",
    slot: PartSlot::Envelope,
    chassis: AIRSHIP,
    build: envelope_lobed,
};
static ENVELOPE_TWIN: FnPart = FnPart {
    slug: "default_envelope_twin",
    slot: PartSlot::Envelope,
    chassis: AIRSHIP,
    build: envelope_twin,
};
static GONDOLA: FnPart = FnPart {
    slug: "default_gondola",
    slot: PartSlot::Gondola,
    chassis: AIRSHIP,
    build: gondola,
};
static FIN: FnPart = FnPart {
    slug: "default_fin",
    slot: PartSlot::Fin,
    chassis: AIRSHIP,
    build: fin,
};
static CHASSIS: FnPart = FnPart {
    slug: "default_chassis",
    slot: PartSlot::Chassis,
    chassis: SKIFF,
    build: chassis,
};
static CANOPY: FnPart = FnPart {
    slug: "default_canopy",
    slot: PartSlot::Canopy,
    chassis: SKIFF,
    build: canopy,
};
static CANOPY_ROADSTER: FnPart = FnPart {
    slug: "skiff_canopy_roadster",
    slot: PartSlot::Canopy,
    chassis: SKIFF,
    build: canopy_roadster,
};
static CANOPY_COUPE: FnPart = FnPart {
    slug: "skiff_canopy_coupe",
    slot: PartSlot::Canopy,
    chassis: SKIFF,
    build: canopy_coupe,
};
static WHEEL: FnPart = FnPart {
    slug: "default_wheel",
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
    &ENVELOPE_BLIMP,
    &ENVELOPE_LOBED,
    &ENVELOPE_TWIN,
    &GONDOLA,
    &FIN,
    &CHASSIS,
    &CANOPY,
    &CANOPY_ROADSTER,
    &CANOPY_COUPE,
    &WHEEL,
];

// ---------------------------------------------------------------------------
// Airship fin — a swept stabiliser the assembler clusters at the tail.
// ---------------------------------------------------------------------------
