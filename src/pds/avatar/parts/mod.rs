//! Avatar part-composition catalogue — the avatar analogue of the
//! structure catalogue ([`crate::catalogue`]) and its
//! [`Settlement`](crate::seeded_defaults::Settlement) slot-filler.
//!
//! An avatar is composed from tagged [`BodyPart`] blueprints rather than
//! hardcoded per-family geometry: each part declares which [`PartSlot`] it
//! fills, which [`ChassisFamily`] families and [`ThemeArchetype`] styles it
//! suits, and (optionally) the ornateness / wear bands it's appropriate
//! for. The seeded [`AvatarOutfit`](crate::seeded_defaults) deriver fills
//! each of an avatar's slots by querying [`parts_for_avatar`], exactly the
//! way the room settlement queries
//! [`entries_for_room`](crate::catalogue::entries_for_room) — so authoring a
//! new part grows avatar variety automatically.
//!
//! ## Slot frame convention
//!
//! Each part builds its geometry in a **local frame whose origin is the
//! part's attachment point**; the assembler
//! ([`super::default_visuals`]) positions the part root into the avatar.
//! By slot: [`PartSlot::Head`] / [`PartSlot::Torso`] and the vehicle body
//! slots are centred on the origin; [`PartSlot::Arm`] / [`PartSlot::Leg`]
//! hang *downward* from a shoulder / hip pivot at the origin;
//! [`PartSlot::Mast`] rises *upward* from a deck pivot at the origin.
//!
//! ## Style coverage
//!
//! A part with an empty [`BodyPart::styles`] list is **universal** — eligible
//! for every style. Shipping a universal part per required slot guarantees
//! every (chassis, slot, style) query is non-empty during content build-out,
//! so the outfit deriver never has an unfillable required slot (the avatar
//! analogue of the settlement's `FALLBACK_THEME`, but per-slot).

pub(crate) mod defaults;
pub(crate) mod humanoid;
pub(crate) mod vehicle;

use crate::pds::generator::Generator;
use crate::seeded_defaults::{
    AirshipBlueprint, AvatarBody, AvatarCharacter, AvatarOutfit, AvatarPalette, BoatBlueprint,
    ChassisFamily, FaceParams, HumanoidBlueprint, MaterialKit, OrnatenessBand, OrnatenessTier,
    SkiffBlueprint, ThemeArchetype, VehicleBlueprint, WearBand, WearTier,
};

/// One composable slot of an avatar. Flat across every chassis (a part
/// declares which families it serves via [`BodyPart::chassis`]); the
/// per-chassis required / optional split lives in [`required_slots`] /
/// [`optional_slots`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PartSlot {
    // --- Humanoid ---
    /// The head + face (carries eyes / hair attachment).
    Head,
    /// The torso / trunk.
    Torso,
    /// One arm (the assembler mirrors it left/right).
    Arm,
    /// One leg (the assembler mirrors it left/right).
    Leg,
    /// Optional headwear.
    Hat,
    // --- Boat ---
    /// The waterline hull.
    Hull,
    /// The deck the rest sits on.
    Deck,
    /// The central mast / spar.
    Mast,
    /// Optional prow ornament.
    Bow,
    /// Optional stern stack / funnel.
    Stack,
    // --- Airship ---
    /// The gas-bag envelope.
    Envelope,
    /// The slung gondola / basket.
    Gondola,
    /// A stabiliser fin.
    Fin,
    /// An engine nacelle / propulsion pod (the assembler mirrors it into an
    /// amidships pair) — the airship's visible propulsion.
    Pod,
    // --- Skiff ---
    /// The chassis slab.
    Chassis,
    /// The cockpit canopy.
    Canopy,
    /// One wheel / skid (the assembler repeats it).
    Wheel,
    /// Optional exhaust / engine ornament.
    Exhaust,
    // --- Shared ---
    /// A small cross-family decorative accent.
    Ornament,
}

/// The slots a chassis *must* fill for a complete avatar.
pub fn required_slots(chassis: ChassisFamily) -> &'static [PartSlot] {
    use PartSlot::*;
    match chassis {
        ChassisFamily::Humanoid => &[Head, Torso, Arm, Leg],
        ChassisFamily::Boat => &[Hull, Deck, Mast],
        ChassisFamily::Airship => &[Envelope, Gondola, Fin, Pod],
        ChassisFamily::Skiff => &[Chassis, Canopy, Wheel],
    }
}

/// The slots a chassis *may* fill (gated by ornateness / wear in the outfit
/// deriver). Order is the deriver's roll order.
pub fn optional_slots(chassis: ChassisFamily) -> &'static [PartSlot] {
    use PartSlot::*;
    match chassis {
        ChassisFamily::Humanoid => &[Hat, Ornament],
        ChassisFamily::Boat => &[Bow, Stack, Ornament],
        ChassisFamily::Airship => &[Ornament],
        ChassisFamily::Skiff => &[Exhaust, Ornament],
    }
}

/// Everything a [`BodyPart::build`] needs: the seeded colours, finishes,
/// and proportions for the avatar being assembled. Cheap to derive from a
/// seed via [`Self::for_seed`].
#[derive(Clone, Copy, Debug)]
pub struct PartCtx {
    pub palette: AvatarPalette,
    pub materials: MaterialKit,
    pub body: AvatarBody,
    /// Concrete humanoid skeleton dimensions derived from `body` — the
    /// shared proportion contract between the humanoid parts and the
    /// assembler. Vehicle parts ignore it.
    pub blueprint: HumanoidBlueprint,
    /// Concrete vehicle proportions + mount landmarks for the seed's chassis
    /// — the shared contract between the vehicle parts and the assembler
    /// (`None` for the humanoid, or a vehicle family not yet wired). Read
    /// through the family accessors ([`Self::boat`]).
    pub vehicle: Option<VehicleBlueprint>,
    /// Seeded face identity (head shape / expression / hair) — humanoid
    /// head builder input; tier-locked to `body.tier`.
    pub face: FaceParams,
    /// The avatar seed — parts open their own sub-stream for stochastic
    /// detail without re-deriving the anchor.
    pub seed: u64,
    /// Seeded ornateness tier — lets a part scale its *visible* detail density
    /// (gondola dressing, engine-pod richness) so the tier finally reads on the
    /// geometry, not just the optional-slot roll.
    pub ornateness: OrnatenessTier,
    /// Whether this avatar's outfit fills the [`PartSlot::Hat`] slot. Parts
    /// that would clip headwear (the hair flourish) suppress themselves when a
    /// hat is worn.
    pub has_hat: bool,
}

impl PartCtx {
    /// Derive the full build context from an avatar seed.
    pub fn for_seed(seed: u64) -> Self {
        Self::for_seed_with_hat(seed, outfit_has_hat(&AvatarOutfit::for_seed(seed)))
    }

    /// Like [`Self::for_seed`] but with `has_hat` precomputed by the caller. The four
    /// family builders already derive the `AvatarOutfit` for their own parts
    /// iteration, so they pass its hat flag in here instead of forcing a second
    /// full `AvatarOutfit::for_seed` derivation per build (#638).
    pub fn for_seed_with_hat(seed: u64, has_hat: bool) -> Self {
        let body = AvatarBody::for_seed(seed);
        Self {
            palette: AvatarPalette::for_seed(seed),
            materials: MaterialKit::for_seed(seed),
            body,
            blueprint: HumanoidBlueprint::from_body(&body),
            vehicle: VehicleBlueprint::from_body(&body, ChassisFamily::for_seed(seed), seed),
            face: FaceParams::for_seed(seed, body.tier),
            seed,
            ornateness: AvatarCharacter::for_seed(seed).ornateness_tier(),
            has_hat,
        }
    }

    /// The boat proportion blueprint, if this avatar is a boat — the boat
    /// parts and the boat assembler both size from it.
    pub fn boat(&self) -> Option<&BoatBlueprint> {
        self.vehicle.as_ref().and_then(VehicleBlueprint::boat)
    }

    /// The airship proportion blueprint, if this avatar is an airship — the
    /// envelope / gondola parts and the airship assembler both size from it.
    pub fn airship(&self) -> Option<&AirshipBlueprint> {
        self.vehicle.as_ref().and_then(VehicleBlueprint::airship)
    }

    /// The skiff proportion blueprint, if this avatar is a skiff — the chassis
    /// / wheel parts and the skiff assembler share its wheel/fender landmarks.
    pub fn skiff(&self) -> Option<&SkiffBlueprint> {
        self.vehicle.as_ref().and_then(VehicleBlueprint::skiff)
    }
}

/// Whether the outfit fills the Hat slot — the one bit of the outfit the
/// [`PartCtx`] needs (hair parts self-suppress under a hat). Takes the outfit by
/// reference so the family builders can reuse the one they already derived.
pub(crate) fn outfit_has_hat(outfit: &AvatarOutfit) -> bool {
    outfit.parts.iter().any(|p| p.slot == PartSlot::Hat)
}

/// One composable avatar part blueprint. Implementors are aggregated into
/// the [`entries`] registry; the outfit deriver selects among them by
/// querying [`parts_for_avatar`].
pub trait BodyPart: Sync {
    /// Stable identifier — written into the outfit so a re-derivation
    /// resolves the same part. Must stay stable across builds.
    fn slug(&self) -> &'static str;

    /// Which slot this part fills.
    fn slot(&self) -> PartSlot;

    /// Which chassis families this part serves. A part may serve several
    /// (a cross-family ornament).
    fn chassis(&self) -> &'static [ChassisFamily];

    /// Which styles this part suits. **Empty means universal** — eligible
    /// for every style (see the module docstring).
    fn styles(&self) -> &'static [ThemeArchetype] {
        &[]
    }

    /// Ornateness-tier span this part suits. Defaults to
    /// [`OrnatenessBand::ANY`].
    fn ornateness_band(&self) -> OrnatenessBand {
        OrnatenessBand::ANY
    }

    /// Wear-tier span this part suits. Defaults to [`WearBand::ANY`].
    fn wear_band(&self) -> WearBand {
        WearBand::ANY
    }

    /// Build the part's geometry in its local attachment frame (see the
    /// module docstring's frame convention).
    fn build(&self, ctx: &PartCtx) -> Generator;
}

/// A data-driven [`BodyPart`] for styled kits — metadata (slot, chassis,
/// styles, ornateness / wear bands) plus a build function pointer. The
/// styled humanoid / vehicle kits express their parts as a table of these
/// rather than a struct apiece.
pub(crate) struct PartDef {
    pub slug: &'static str,
    pub slot: PartSlot,
    pub chassis: &'static [ChassisFamily],
    pub styles: &'static [ThemeArchetype],
    pub ornateness: OrnatenessBand,
    pub wear: WearBand,
    pub build: fn(&PartCtx) -> Generator,
}

impl BodyPart for PartDef {
    fn slug(&self) -> &'static str {
        self.slug
    }
    fn slot(&self) -> PartSlot {
        self.slot
    }
    fn chassis(&self) -> &'static [ChassisFamily] {
        self.chassis
    }
    fn styles(&self) -> &'static [ThemeArchetype] {
        self.styles
    }
    fn ornateness_band(&self) -> OrnatenessBand {
        self.ornateness
    }
    fn wear_band(&self) -> WearBand {
        self.wear
    }
    fn build(&self, ctx: &PartCtx) -> Generator {
        (self.build)(ctx)
    }
}

/// Every part of `slot` serving `chassis` and eligible for `style`
/// (universal parts always qualify), in registry order. The avatar
/// analogue of [`crate::catalogue::entries_for`].
pub fn parts_for(
    chassis: ChassisFamily,
    slot: PartSlot,
    style: ThemeArchetype,
) -> impl Iterator<Item = &'static dyn BodyPart> {
    entries().filter(move |p| {
        p.slot() == slot
            && p.chassis().contains(&chassis)
            && (p.styles().is_empty() || p.styles().contains(&style))
    })
}

/// [`parts_for`] further gated by the avatar's ornateness / escalation
/// tiers — the avatar analogue of [`crate::catalogue::entries_for_room`].
/// Since both bands default to `ANY`, this matches [`parts_for`] until a
/// part opts into a band.
pub fn parts_for_avatar(
    chassis: ChassisFamily,
    slot: PartSlot,
    style: ThemeArchetype,
    ornateness: OrnatenessTier,
    wear: WearTier,
) -> impl Iterator<Item = &'static dyn BodyPart> {
    parts_for(chassis, slot, style)
        .filter(move |p| p.ornateness_band().accepts(ornateness) && p.wear_band().accepts(wear))
}

/// Every part that ships in the build, across all content modules. The
/// universal default parts come first so they're the stable fallback pick;
/// styled kits follow. Iterating a chain (rather than one `static` array)
/// lets each kit own its own registry slice in its own file.
pub fn entries() -> impl Iterator<Item = &'static dyn BodyPart> {
    defaults::ENTRIES
        .iter()
        .chain(humanoid::ENTRIES.iter())
        .chain(vehicle::ENTRIES.iter())
        .copied()
}

/// Resolve a part by its stable [`BodyPart::slug`]. The contract the outfit
/// deriver and assembler share: the seeded
/// [`AvatarOutfit`](crate::seeded_defaults) records a slug per slot, and the
/// assembler resolves it back to the blueprint here.
pub fn by_slug(slug: &str) -> Option<&'static dyn BodyPart> {
    entries().find(|p| p.slug() == slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_unique() {
        let all: Vec<&'static dyn BodyPart> = entries().collect();
        for (i, a) in all.iter().enumerate() {
            let dupes = all.iter().filter(|b| b.slug() == a.slug()).count();
            assert_eq!(dupes, 1, "slug {:?} repeats (index {i})", a.slug());
        }
    }

    #[test]
    fn every_required_slot_is_fillable_for_every_style() {
        // The universal defaults guarantee a non-empty pool for every
        // (chassis, required slot, style) — the contract the outfit deriver
        // relies on so no required slot is ever unfillable.
        for chassis in ChassisFamily::ALL {
            for &slot in required_slots(chassis) {
                for style in ThemeArchetype::ALL {
                    assert!(
                        parts_for(chassis, slot, style).next().is_some(),
                        "{chassis:?}/{slot:?}/{style:?} has no part"
                    );
                }
            }
        }
    }

    #[test]
    fn required_slots_fillable_across_every_tier() {
        // The band-gated query must also stay non-empty for required slots at
        // every ornateness/wear tier (universal defaults carry ANY bands).
        for chassis in ChassisFamily::ALL {
            for &slot in required_slots(chassis) {
                for o in OrnatenessTier::ALL {
                    for w in WearTier::ALL {
                        assert!(
                            parts_for_avatar(chassis, slot, ThemeArchetype::AncientClassical, o, w)
                                .next()
                                .is_some(),
                            "{chassis:?}/{slot:?} empty at {o:?}/{w:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn avatar_query_is_the_band_filtered_style_query() {
        // parts_for_avatar is exactly parts_for with the band predicate.
        for chassis in ChassisFamily::ALL {
            let slots: Vec<PartSlot> = required_slots(chassis)
                .iter()
                .chain(optional_slots(chassis))
                .copied()
                .collect();
            for slot in slots {
                for style in ThemeArchetype::ALL {
                    let base: Vec<&str> =
                        parts_for(chassis, slot, style).map(|p| p.slug()).collect();
                    for o in OrnatenessTier::ALL {
                        for w in WearTier::ALL {
                            for p in parts_for(chassis, slot, style) {
                                let accepted =
                                    p.ornateness_band().accepts(o) && p.wear_band().accepts(w);
                                let in_gated = parts_for_avatar(chassis, slot, style, o, w)
                                    .any(|q| q.slug() == p.slug());
                                assert_eq!(accepted, in_gated, "{} band mismatch", p.slug());
                            }
                            for q in parts_for_avatar(chassis, slot, style, o, w) {
                                assert!(base.contains(&q.slug()), "gated introduced {}", q.slug());
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn every_part_survives_sanitize_unchanged() {
        // A part must emit geometry already inside every sanitiser bound, or
        // a peer would render different geometry than the owner built.
        use crate::pds::sanitize_avatar_visuals;
        fn assert_tree_eq(a: &Generator, b: &Generator, slug: &str) {
            assert_eq!(a.kind, b.kind, "{slug}: kind rewritten by sanitiser");
            assert_eq!(
                a.transform.translation, b.transform.translation,
                "{slug}: translation rewritten"
            );
            assert_eq!(
                a.transform.scale, b.transform.scale,
                "{slug}: scale rewritten"
            );
            assert_eq!(a.children.len(), b.children.len(), "{slug}: child dropped");
            for (ca, cb) in a.children.iter().zip(b.children.iter()) {
                assert_tree_eq(ca, cb, slug);
            }
        }
        // Span several seeds so every part builds against a range of contexts
        // — crucially, the vehicle parts must survive sanitize at the extremes
        // of their seeded blueprint dimensions, not just at the nominal
        // fallback. Seeds chosen to cover boat / airship / skiff chassis so a
        // family part meets a real blueprint of its own family.
        for seed in [11u64, 13, 68, 2, 5, 0, 42] {
            let ctx = PartCtx::for_seed(seed);
            for part in entries() {
                let built = part.build(&ctx);
                let mut sanitized = built.clone();
                sanitize_avatar_visuals(&mut sanitized);
                assert_tree_eq(&built, &sanitized, part.slug());
            }
        }
    }

    #[test]
    fn humanoid_blob_masses_are_single_connected_skins() {
        // Union-find over each BlobGroup mesh's triangle graph: a blended
        // trunk / limb / pelvis / shoe must polygonise as ONE component.
        // This is the mechanical guard for the #726 round-2 regression —
        // needle-tipped limb segments visually separating at the joints
        // ("exploded marionette") — and for any future element retune that
        // drifts masses out of blend range. Seeds span the four
        // stylization tiers via the user-supplied humanoid seed list.
        use crate::pds::generator::GeneratorKind;
        use crate::world_builder::build_primitive_mesh;
        use bevy::mesh::VertexAttributeValues;

        fn find(parent: &mut [usize], mut a: usize) -> usize {
            while parent[a] != a {
                parent[a] = parent[parent[a]];
                a = parent[a];
            }
            a
        }
        fn components(kind: &GeneratorKind) -> usize {
            let mesh = build_primitive_mesh(kind);
            let pos = match mesh.attribute(bevy::prelude::Mesh::ATTRIBUTE_POSITION) {
                Some(VertexAttributeValues::Float32x3(p)) => p.clone(),
                _ => return 0,
            };
            let Some(indices) = mesh.indices() else {
                return 0;
            };
            // Weld coincident vertices before the union-find: the UV seam
            // splits (#739's Box/Cylindrical modes duplicate a vertex per
            // projection region — and Box is the default since #742) are
            // texture-atlas topology, not geometry, and this guard cares
            // about *geometric* connectivity. Bit-exact keys suffice
            // because the seam duplicates are verbatim copies.
            let mut weld: std::collections::HashMap<[u32; 3], usize> =
                std::collections::HashMap::with_capacity(pos.len());
            let rep: Vec<usize> = pos
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    *weld
                        .entry([p[0].to_bits(), p[1].to_bits(), p[2].to_bits()])
                        .or_insert(i)
                })
                .collect();
            let n = pos.len();
            let mut parent: Vec<usize> = (0..n).collect();
            let mut touched = vec![false; n];
            let idx: Vec<usize> = indices.iter().map(|i| rep[i]).collect();
            for tri in idx.chunks(3) {
                for &(a, b) in &[(tri[0], tri[1]), (tri[0], tri[2])] {
                    touched[a] = true;
                    touched[b] = true;
                    let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                    parent[ra] = rb;
                }
            }
            (0..n)
                .filter(|&v| touched[v] && find(&mut parent, v) == v)
                .count()
        }
        fn walk(g: &Generator, out: &mut Vec<GeneratorKind>) {
            if matches!(g.kind, GeneratorKind::BlobGroup { .. }) {
                out.push(g.kind.clone());
            }
            for c in &g.children {
                walk(c, out);
            }
        }

        // All verified humanoid-producing seeds (a vehicle seed would have
        // no blob masses and trip the non-empty assert below).
        for seed in [
            6300350204994988827u64,
            16829956693767402793,
            18102493806418102393,
            5227756743208462829,
            15252705949980194106,
            184810340591539844,
            14887495512784657594,
        ] {
            let (avatar, _) = crate::pds::avatar::default_visuals::build_for_seed(seed);
            let mut kinds = Vec::new();
            walk(&avatar, &mut kinds);
            assert!(
                !kinds.is_empty(),
                "seed {seed}: humanoid avatar lost its blob masses"
            );
            for (i, k) in kinds.iter().enumerate() {
                assert_eq!(
                    components(k),
                    1,
                    "seed {seed}: blob mass #{i} meshes as a disconnected skin"
                );
            }
        }
    }

    #[test]
    fn parts_serve_only_their_declared_chassis_and_build_deterministically() {
        let ctx = PartCtx::for_seed(42);
        for part in entries() {
            assert!(
                !part.chassis().is_empty(),
                "{} serves no chassis",
                part.slug()
            );
            let a = part.build(&ctx);
            let b = part.build(&ctx);
            assert_eq!(a, b, "{} build is non-deterministic", part.slug());
        }
    }
}
