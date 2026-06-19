//! Seeded avatar outfit — the slot-filling deriver, avatar analogue of
//! [`super::super::room::settlement::Settlement`].
//!
//! Given an [`AvatarCharacter`], it fills every *required* slot for the
//! chassis and rolls each *optional* slot (hat, ornament, stack, exhaust)
//! against the avatar's ornateness, by querying the tagged part catalogue
//! ([`crate::pds::avatar::parts::parts_for_avatar`]). The result is a flat
//! list of `(slot, part-slug)` choices the assembler
//! ([`crate::pds::avatar::default_visuals`]) resolves (via
//! [`crate::pds::avatar::parts::by_slug`]) and positions per the slot's
//! frame convention.
//!
//! Selection prefers the avatar's [`ThemeArchetype`] style at its ornateness
//! / wear tiers; if a band leaves a slot's pool empty it widens to the
//! unbanded style pool, and the universal default parts guarantee every
//! *required* slot resolves regardless. Optional slots that no part serves
//! yet simply stay empty (graceful while the styled kits fill in).

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use super::character::{AvatarCharacter, OrnatenessTier, WearTier};
use super::chassis::ChassisFamily;
use crate::pds::avatar::parts::{
    PartSlot, optional_slots, parts_for, parts_for_avatar, required_slots,
};
use crate::seeded_defaults::scene::{ThemeArchetype, pick, unit_f32};

/// Sub-stream salt distinct from every sibling avatar deriver.
const OUTFIT_STREAM_SALT: u64 = 0x0107_F17E_0107_F17E;

/// One filled slot: which slot, and the chosen part's stable slug.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutfitPart {
    pub slot: PartSlot,
    /// Stable [`BodyPart::slug`](crate::pds::avatar::parts::BodyPart::slug);
    /// resolved by the assembler through
    /// [`by_slug`](crate::pds::avatar::parts::by_slug).
    pub slug: &'static str,
}

/// The full set of parts that compose an avatar: the chassis plus an ordered
/// list of filled slots (required slots first, then any rolled optionals).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvatarOutfit {
    pub chassis: ChassisFamily,
    pub parts: Vec<OutfitPart>,
}

impl AvatarOutfit {
    pub fn for_did(did: &str) -> Self {
        Self::from_character(&AvatarCharacter::for_did(did))
    }

    pub fn for_seed(seed: u64) -> Self {
        Self::from_character(&AvatarCharacter::for_seed(seed))
    }

    /// Fill the avatar's slots from the part catalogue, keyed on the anchor.
    pub fn from_character(c: &AvatarCharacter) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(c.seed ^ OUTFIT_STREAM_SALT);
        let ornateness = c.ornateness_tier();
        let wear = c.wear_tier();
        let mut parts = Vec::new();

        // Required slots: always filled. `widen` lets selection fall back to
        // the unbanded style pool (the universal defaults) so a required slot
        // is never left empty by an over-tight band.
        for &slot in required_slots(c.chassis) {
            if let Some(slug) =
                pick_slug(c.chassis, slot, c.style, ornateness, wear, true, &mut rng)
            {
                parts.push(OutfitPart { slot, slug });
            }
        }

        // Optional slots: rolled against ornateness, then filled only if a
        // part serves the slot *at the avatar's tiers* — the bands hard-gate
        // here (no widen), so a plain avatar never picks up an ornate-only
        // accent, and a slot no part serves yet stays empty.
        for &slot in optional_slots(c.chassis) {
            if roll_optional(ornateness, &mut rng)
                && let Some(slug) =
                    pick_slug(c.chassis, slot, c.style, ornateness, wear, false, &mut rng)
            {
                parts.push(OutfitPart { slot, slug });
            }
        }

        Self {
            chassis: c.chassis,
            parts,
        }
    }
}

/// Pick a part slug for `slot` from the band-gated styled pool. When `widen`
/// is set and the bands leave the pool empty, fall back to the unbanded style
/// pool (which still includes the universal defaults) — used for required
/// slots so they're never left empty. Optional slots pass `widen = false` so
/// their bands hard-gate. Yields `None` only if nothing serves the slot.
fn pick_slug(
    chassis: ChassisFamily,
    slot: PartSlot,
    style: ThemeArchetype,
    ornateness: OrnatenessTier,
    wear: WearTier,
    widen: bool,
    rng: &mut ChaCha8Rng,
) -> Option<&'static str> {
    let banded: Vec<&'static str> = parts_for_avatar(chassis, slot, style, ornateness, wear)
        .map(|p| p.slug())
        .collect();
    let pool = if banded.is_empty() && widen {
        parts_for(chassis, slot, style).map(|p| p.slug()).collect()
    } else {
        banded
    };
    if pool.is_empty() {
        None
    } else {
        Some(pick(&pool, rng))
    }
}

/// Roll whether an optional slot is included — likelier the more ornate the
/// avatar.
fn roll_optional(ornateness: OrnatenessTier, rng: &mut ChaCha8Rng) -> bool {
    let p = match ornateness {
        OrnatenessTier::Plain => 0.20,
        OrnatenessTier::Adorned => 0.55,
        OrnatenessTier::Ornate => 0.85,
    };
    unit_f32(rng) < p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::avatar::parts::by_slug;
    use crate::seeded_defaults::hash::fnv1a_64;

    #[test]
    fn deterministic() {
        assert_eq!(
            AvatarOutfit::for_did("did:plc:abc"),
            AvatarOutfit::for_did("did:plc:abc")
        );
    }

    #[test]
    fn for_did_equals_for_seed_of_hashed_did() {
        let did = "did:plc:outfit";
        assert_eq!(
            AvatarOutfit::for_did(did),
            AvatarOutfit::for_seed(fnv1a_64(did))
        );
    }

    #[test]
    fn every_required_slot_is_filled_for_every_seed() {
        for s in 0u64..200 {
            let outfit = AvatarOutfit::for_seed(s);
            for &slot in required_slots(outfit.chassis) {
                assert!(
                    outfit.parts.iter().any(|p| p.slot == slot),
                    "seed {s} ({:?}) missing required slot {slot:?}",
                    outfit.chassis
                );
            }
        }
    }

    #[test]
    fn chosen_parts_resolve_and_serve_the_chassis() {
        for s in 0u64..200 {
            let outfit = AvatarOutfit::for_seed(s);
            for choice in &outfit.parts {
                let part = by_slug(choice.slug)
                    .unwrap_or_else(|| panic!("seed {s}: unknown slug {}", choice.slug));
                assert_eq!(
                    part.slot(),
                    choice.slot,
                    "slot mismatch for {}",
                    choice.slug
                );
                assert!(
                    part.chassis().contains(&outfit.chassis),
                    "{} does not serve {:?}",
                    choice.slug,
                    outfit.chassis
                );
            }
        }
    }

    #[test]
    fn no_duplicate_slots() {
        // The deriver fills each slot at most once.
        for s in 0u64..120 {
            let outfit = AvatarOutfit::for_seed(s);
            let mut seen = Vec::new();
            for p in &outfit.parts {
                assert!(
                    !seen.contains(&p.slot),
                    "seed {s}: slot {:?} filled twice",
                    p.slot
                );
                seen.push(p.slot);
            }
        }
    }

    #[test]
    fn optional_inclusion_rises_with_ornateness() {
        // The optional gate is monotone in ornateness — Ornate avatars roll
        // optionals far more often than Plain ones.
        let count = |tier: OrnatenessTier| {
            let mut rng = ChaCha8Rng::seed_from_u64(0xABCD);
            (0..1000).filter(|_| roll_optional(tier, &mut rng)).count()
        };
        let plain = count(OrnatenessTier::Plain);
        let ornate = count(OrnatenessTier::Ornate);
        assert!(
            ornate > plain + 200,
            "ornateness gate not monotone: plain={plain} ornate={ornate}"
        );
    }
}
