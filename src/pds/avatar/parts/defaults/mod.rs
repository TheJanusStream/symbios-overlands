//! Universal default parts - at least one per required slot per chassis,
//! eligible for every style (empty [`BodyPart::styles`]).
//!
//! These are the **coverage floor**: they guarantee every required
//! (chassis, slot) is fillable for any style/tier so the outfit deriver
//! never stalls on an unfillable slot while the styled kits
//! (`super`'s `#518`/`#519` content) fill in. The geometry is plain - a
//! readable, *recognisable* silhouette built from the shared primitive
//! vocabulary and finished through the seeded
//! [`MaterialKit`](crate::seeded_defaults::MaterialKit) - a shaped hull /
//! cabin / cigar envelope rather than bare capsules and slabs. Each builds
//! in its slot's local attachment frame (see the module docstring on
//! [`super`]).
//!
//! Vehicles only, since #1060: the humanoid families' parts retired with
//! the generator humanoid, and a humanoid avatar is a rigged
//! `symbios-avatar` body now.
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
//! A base part used as a family's structural root (today only the airship's
//! [`envelope`]) must **not** set `transform.scale`, because the assembler
//! mounts every other slot (gondola, fins, pods) as a child of that root and a
//! root scale would stretch and displace them. Elongated shapes are built from
//! composed primitives instead.
//!
//! The discipline outlived the bridge it was written beside. The *assembler*
//! used to set a uniform root scale in
//! [`apply_travel_pose`](super::super::default_visuals) - the airship-class
//! size bridge of #1361 - and that was never the same licence, because it is
//! uniform and applied after every slot is mounted. Both families that carried
//! one now author at the size they are drawn at (#1363, #1364), so every
//! seeded craft passes 1.0; what remains is the rule that a PART may not set
//! one, and it binds the redesigned families too - a swept body needs a
//! per-axis node scale, so it hangs off a hidden hub rather than being the
//! root itself.

// Crate-visible so the airship assembler (rigging-cable colour) + the styled
// teardrop envelope (`super::vehicle`) can share its two-hue colour scheme,
// matte envelope material, normalized window colour, and gore-seam helper
// (#789).
pub(crate) mod airship;
// Crate-visible so the styled vehicle kits (`super::vehicle`) can share the
// `shade` colour helper instead of keeping their own copy (#798).
pub(crate) mod common;
// The land-skiff's defaults (chassis, canopies, wheel) left in #1364, with
// the boat's in #1363: both families' craft types draw their own geometry off
// one profile now, so neither fills a slot.

use crate::seeded_defaults::ChassisFamily;
use crate::seeded_defaults::{OrnatenessBand, WearBand};

use super::{BodyPart, PartDef, PartSlot};
use airship::*;

const AIRSHIP: &[ChassisFamily] = &[ChassisFamily::Airship];

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

/// Every universal default part, in slot order per chassis.
pub(super) static ENTRIES: &[&dyn BodyPart] = &[
    &ENVELOPE,
    &ENVELOPE_BLIMP,
    &ENVELOPE_LOBED,
    &ENVELOPE_TWIN,
    &GONDOLA,
    &FIN,
    &POD,
];

// ---------------------------------------------------------------------------
// Airship fin - a swept stabiliser the assembler clusters at the tail.
// ---------------------------------------------------------------------------
