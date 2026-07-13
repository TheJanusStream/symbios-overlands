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
// Crate-visible so the styled vehicle kits (`super::vehicle`) can share the
// `shade` colour helper instead of keeping their own copy (#798).
pub(crate) mod common;
mod humanoid;
// Crate-visible so the land-skiff assembler + the styled chassis / wheel
// variants (`super::vehicle`) can share its blueprint-derived dims, colour
// scheme, and wheel-anchor / fender contract (#788).
pub(crate) mod skiff;

use crate::seeded_defaults::ChassisFamily;
use crate::seeded_defaults::{OrnatenessBand, WearBand};

use super::{BodyPart, PartDef, PartSlot};
use airship::*;
use boat::*;
use humanoid::*;
use skiff::*;

const HUMANOID: &[ChassisFamily] = &[ChassisFamily::Humanoid];
const BOAT: &[ChassisFamily] = &[ChassisFamily::Boat];
const AIRSHIP: &[ChassisFamily] = &[ChassisFamily::Airship];
const SKIFF: &[ChassisFamily] = &[ChassisFamily::Skiff];

static HEAD: PartDef = PartDef {
    slug: "default_head",
    slot: PartSlot::Head,
    chassis: HUMANOID,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: head,
};
static TORSO: PartDef = PartDef {
    slug: "default_torso",
    slot: PartSlot::Torso,
    chassis: HUMANOID,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: torso,
};
static COAT: PartDef = PartDef {
    slug: "default_torso_coat",
    slot: PartSlot::Torso,
    chassis: HUMANOID,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: coat,
};
static ARM: PartDef = PartDef {
    slug: "default_arm",
    slot: PartSlot::Arm,
    chassis: HUMANOID,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: arm,
};
static LEG: PartDef = PartDef {
    slug: "default_leg",
    slot: PartSlot::Leg,
    chassis: HUMANOID,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: leg,
};
static HULL: PartDef = PartDef {
    slug: "default_hull",
    slot: PartSlot::Hull,
    chassis: BOAT,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: hull,
};
static HULL_CATAMARAN: PartDef = PartDef {
    slug: "default_hull_catamaran",
    slot: PartSlot::Hull,
    chassis: BOAT,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: hull_catamaran,
};
static HULL_TRIMARAN: PartDef = PartDef {
    slug: "default_hull_trimaran",
    slot: PartSlot::Hull,
    chassis: BOAT,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: hull_trimaran,
};
static HULL_BARGE: PartDef = PartDef {
    slug: "default_hull_barge",
    slot: PartSlot::Hull,
    chassis: BOAT,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: hull_barge,
};
static DECK: PartDef = PartDef {
    slug: "default_deck",
    slot: PartSlot::Deck,
    chassis: BOAT,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: deck,
};
static MAST: PartDef = PartDef {
    slug: "default_mast",
    slot: PartSlot::Mast,
    chassis: BOAT,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: mast,
};
static ENVELOPE: PartDef = PartDef {
    slug: "default_envelope",
    slot: PartSlot::Envelope,
    chassis: AIRSHIP,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: envelope,
};
static ENVELOPE_BLIMP: PartDef = PartDef {
    slug: "default_envelope_blimp",
    slot: PartSlot::Envelope,
    chassis: AIRSHIP,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: envelope_blimp,
};
static ENVELOPE_LOBED: PartDef = PartDef {
    slug: "default_envelope_lobed",
    slot: PartSlot::Envelope,
    chassis: AIRSHIP,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: envelope_lobed,
};
static ENVELOPE_TWIN: PartDef = PartDef {
    slug: "default_envelope_twin",
    slot: PartSlot::Envelope,
    chassis: AIRSHIP,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: envelope_twin,
};
static GONDOLA: PartDef = PartDef {
    slug: "default_gondola",
    slot: PartSlot::Gondola,
    chassis: AIRSHIP,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: gondola,
};
static FIN: PartDef = PartDef {
    slug: "default_fin",
    slot: PartSlot::Fin,
    chassis: AIRSHIP,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: fin,
};
static POD: PartDef = PartDef {
    slug: "default_pod",
    slot: PartSlot::Pod,
    chassis: AIRSHIP,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: pod,
};
static CHASSIS: PartDef = PartDef {
    slug: "default_chassis",
    slot: PartSlot::Chassis,
    chassis: SKIFF,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: chassis,
};
static CANOPY: PartDef = PartDef {
    slug: "default_canopy",
    slot: PartSlot::Canopy,
    chassis: SKIFF,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: canopy,
};
static CANOPY_ROADSTER: PartDef = PartDef {
    slug: "skiff_canopy_roadster",
    slot: PartSlot::Canopy,
    chassis: SKIFF,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: canopy_roadster,
};
static CANOPY_COUPE: PartDef = PartDef {
    slug: "skiff_canopy_coupe",
    slot: PartSlot::Canopy,
    chassis: SKIFF,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: canopy_coupe,
};
static WHEEL: PartDef = PartDef {
    slug: "default_wheel",
    slot: PartSlot::Wheel,
    chassis: SKIFF,
    styles: &[],
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
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
    &POD,
    &CHASSIS,
    &CANOPY,
    &CANOPY_ROADSTER,
    &CANOPY_COUPE,
    &WHEEL,
];

// ---------------------------------------------------------------------------
// Airship fin — a swept stabiliser the assembler clusters at the tail.
// ---------------------------------------------------------------------------
