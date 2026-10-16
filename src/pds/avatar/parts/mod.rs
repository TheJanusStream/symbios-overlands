//! The build context every seeded craft type reads.
//!
//! This module used to be the avatar's part-composition catalogue - the
//! avatar analogue of the structure catalogue ([`crate::catalogue`]): tagged
//! `BodyPart` blueprints, each declaring the slot it filled and the families,
//! styles and tiers it suited, which a seeded outfit deriver queried slot by
//! slot. The boat left that pipeline in #1363, the land-skiff in #1364, the
//! rigged humanoid never used it (#1060), and the airship - the last family
//! composed this way - became a craft-type family in #1657, which deleted
//! the catalogue, its slots and the outfit that filled them. Every seeded
//! vehicle is now drawn by one builder per craft type over one profile.
//!
//! What is left is the context those builders share, [`PartCtx`]: the seed's
//! palette, its finish kit, its body proportions, its vehicle blueprint, its
//! tiers and an optional livery override.

use crate::seeded_defaults::{
    AvatarBody, AvatarCharacter, AvatarPalette, BoatBlueprint, ChassisFamily, MaterialKit,
    OrnatenessTier, SkiffBlueprint, VehicleBlueprint, WearTier,
};

/// Everything a craft type's builder needs: the seeded colours, finishes,
/// and proportions for the avatar being built. Cheap to derive from a seed
/// via [`Self::for_seed`].
#[derive(Clone, Copy, Debug)]
pub struct PartCtx {
    pub palette: AvatarPalette,
    pub materials: MaterialKit,
    pub body: AvatarBody,
    /// Concrete vehicle proportions for the seed's chassis (`None` for the
    /// rigged family). Read through the family accessors ([`Self::boat`]).
    pub vehicle: Option<VehicleBlueprint>,
    /// The avatar seed - a builder opens its own sub-stream for stochastic
    /// detail without re-deriving the anchor.
    pub seed: u64,
    /// Which heritage livery to draw, overriding the seed's own pick
    /// ([`crate::pds::avatar::livery`]). `None` - the only value any live
    /// build path uses - means the seed answers for itself.
    ///
    /// It exists for the render tool's `--livery <index>`, which draws every
    /// scheme in a family's list on ONE hull so the list can be judged side by
    /// side rather than hunted for across seeds. Carried on the context rather
    /// than through a process-global, because a global is invisible to the
    /// nextest run that forks a process per test and would be a lie in every
    /// other caller.
    pub livery: Option<usize>,
    /// Seeded ornateness tier - what each craft type's ladder of secondary
    /// masses dresses by, so the tier reads on the geometry.
    pub ornateness: OrnatenessTier,
    /// Seeded wear tier - the discrete reading of the wear axis the
    /// [`MaterialKit`] already grimes every surface by. A finish can only
    /// darken; a boat's wear also has to read as MASSES at play distance - a
    /// tarp lashed over the coachroof, fuel cans on the side deck (#1366).
    pub wear: WearTier,
}

impl PartCtx {
    /// Derive the full build context from an avatar seed.
    ///
    /// No longer derives an `AvatarOutfit` of its own: the hat flag it used
    /// to carry existed for the humanoid hair flourish, which retired with
    /// the generator humanoid (#1060). The two-constructor split #638 added
    /// to avoid a second outfit derivation went with it.
    pub fn for_seed(seed: u64) -> Self {
        let body = AvatarBody::for_seed(seed);
        let character = AvatarCharacter::for_seed(seed);
        Self {
            palette: AvatarPalette::for_seed(seed),
            materials: MaterialKit::for_seed(seed),
            body,
            vehicle: VehicleBlueprint::from_body(&body, ChassisFamily::for_seed(seed), seed),
            seed,
            livery: None,
            ornateness: character.ornateness_tier(),
            wear: character.wear_tier(),
        }
    }

    /// The boat proportion blueprint, if this avatar is a boat. A boat has no
    /// parts since #1363, but the accessor is the vehicle families' shared
    /// shape and the boat builders reach it through their own `PartCtx`.
    pub fn boat(&self) -> Option<&BoatBlueprint> {
        self.vehicle.as_ref().and_then(VehicleBlueprint::boat)
    }

    /// The skiff proportion blueprint, if this avatar is a skiff.
    pub fn skiff(&self) -> Option<&SkiffBlueprint> {
        self.vehicle.as_ref().and_then(VehicleBlueprint::skiff)
    }
}
