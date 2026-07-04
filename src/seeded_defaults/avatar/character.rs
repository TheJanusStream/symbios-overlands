//! Avatar-character anchor: the per-avatar seed-derived tuple that every
//! downstream avatar deriver reads to coordinate its output.
//!
//! The avatar analogue of [`super::super::scene::SceneCharacter`]. Sampling
//! palette, materials, proportions, FX, and part selection independently
//! from the avatar seed gives clashing avatars (neon palette + rustic
//! cloth + arcane motes on one figure). Sampling them from a shared
//! [`AvatarCharacter`] produces coherent avatars ("weathered medieval
//! footman", "ornate cyberpunk skiff") because each downstream deriver
//! biases its samples around the same anchor.
//!
//! Two discrete axes anchor the design space — the [`ChassisFamily`] (the
//! body plan: humanoid / boat / airship / skiff) and the
//! [`ThemeArchetype`] *style* (deliberately the **same** enum the room
//! uses, so a cyberpunk avatar and a cyberpunk room speak one style
//! vocabulary). Two continuous socio-style axes follow — `ornateness`
//! (plain ↔ ornate) and `wear` (pristine ↔ battered) — read via
//! [`OrnatenessTier`] / [`WearTier`] and the catalogue-eligibility bands
//! [`OrnatenessBand`] / [`WearBand`], exactly as the room reads prosperity
//! and escalation.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use super::chassis::ChassisFamily;
use crate::seeded_defaults::hash::fnv1a_64;
use crate::seeded_defaults::scene::{ThemeArchetype, pick, signed_unit_f32, unit_f32};

/// Sub-stream salt for the character anchor — distinct from every
/// per-domain avatar deriver salt so the anchor's draws never alias a
/// downstream stream.
const AVATAR_CHARACTER_SALT: u64 = 0xA7A7_C4A7_C4A7_A7A7;

/// Ornamentation tier — the discrete reading of the continuous
/// [`AvatarCharacter::ornateness`] axis (plain → ornate). Thresholded into
/// thirds. Drives ornament-slot density (hats, finials, pauldrons, trim)
/// and which cross-style ornament pool a part draws from.
///
/// Variants are declared plainest-first so the derived [`Ord`] matches the
/// axis direction — [`OrnatenessBand`] relies on that ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OrnatenessTier {
    /// Bottom third — bare, functional, no extra trim.
    Plain,
    /// Middle third — a little ornament; one or two accents.
    Adorned,
    /// Top third — heavily decorated; finials, filigree, full kit.
    Ornate,
}

impl OrnatenessTier {
    pub const ALL: [Self; 3] = [Self::Plain, Self::Adorned, Self::Ornate];

    /// Threshold a `[0, 1]` ornateness value into equal thirds.
    pub fn from_unit(ornateness: f32) -> Self {
        match ornateness {
            o if o < 1.0 / 3.0 => Self::Plain,
            o if o < 2.0 / 3.0 => Self::Adorned,
            _ => Self::Ornate,
        }
    }

    /// Human-readable display name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Plain => "Plain",
            Self::Adorned => "Adorned",
            Self::Ornate => "Ornate",
        }
    }
}

/// Wear tier — the discrete reading of the continuous
/// [`AvatarCharacter::wear`] axis (pristine → battered). Thresholded into
/// thirds. Drives material finish (gloss ↔ grime), surface darkening /
/// oxidation, and battle-damage / patina part variants.
///
/// Variants are declared cleanest-first so the derived [`Ord`] matches the
/// axis direction — [`WearBand`] relies on that ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WearTier {
    /// Bottom third — clean, polished, factory-fresh.
    Pristine,
    /// Middle third — used, lightly scuffed and dulled.
    Worn,
    /// Top third — beaten up: grime, oxidation, visible damage.
    Battered,
}

impl WearTier {
    pub const ALL: [Self; 3] = [Self::Pristine, Self::Worn, Self::Battered];

    /// Threshold a `[0, 1]` wear value into equal thirds.
    pub fn from_unit(wear: f32) -> Self {
        match wear {
            w if w < 1.0 / 3.0 => Self::Pristine,
            w if w < 2.0 / 3.0 => Self::Worn,
            _ => Self::Battered,
        }
    }

    /// Human-readable display name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Pristine => "Pristine",
            Self::Worn => "Worn",
            Self::Battered => "Battered",
        }
    }
}

/// Surface-finish register — a per-avatar coin-flip between a saturated,
/// glossy, glow-forward look and a deeper, restrained naturalistic one. Read
/// by [`super::palette`] (accent chroma / lightness) and [`super::materials`]
/// (gloss + emissive strength) so the population splits between punchy
/// stylised avatars and grounded realistic ones rather than all reading the
/// same. Orthogonal to `style`: a medieval avatar can be Bold (heraldic,
/// vivid) or Naturalistic (muddy, worn), and likewise for every style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinishRegister {
    /// Saturated accents, glossier surfaces, stronger glow on luminous styles.
    Bold,
    /// Deeper, more naturalistic hues with restrained gloss and glow.
    Naturalistic,
}

/// Inclusive ornateness-tier affinity band a body part advertises: the
/// contiguous span of [`OrnatenessTier`]s an avatar may have for the part
/// to be eligible. `ANY` (the default) spans every tier, so untagged
/// parts are always eligible. Relies on [`OrnatenessTier`]'s
/// plainest-first [`Ord`]. One instantiation of the shared
/// [`Band`](crate::seeded_defaults::band::Band) (#654), like the scene
/// axes' `ProsperityBand`.
pub type OrnatenessBand = crate::seeded_defaults::band::Band<OrnatenessTier>;

impl crate::seeded_defaults::band::BandTier for OrnatenessTier {
    const MIN: Self = OrnatenessTier::Plain;
    const MAX: Self = OrnatenessTier::Ornate;
    fn label(self) -> &'static str {
        OrnatenessTier::label(self)
    }
}

/// Inclusive wear-tier affinity band — the [`WearTier`] analogue of
/// [`OrnatenessBand`]. `ANY` is the default.
pub type WearBand = crate::seeded_defaults::band::Band<WearTier>;

impl crate::seeded_defaults::band::BandTier for WearTier {
    const MIN: Self = WearTier::Pristine;
    const MAX: Self = WearTier::Battered;
    fn label(self) -> &'static str {
        WearTier::label(self)
    }
}

/// Per-avatar anchor read by every downstream deriver (palette, material
/// kit, proportions, FX, part selection). Cheap to recompute from the DID;
/// typically derived once when an avatar loads and threaded through the
/// deriver call graph.
///
/// Independent of [`crate::seeded_defaults::scene::SceneCharacter`]: an
/// avatar reads the same regardless of which room it stands in.
#[derive(Clone, Copy, Debug)]
pub struct AvatarCharacter {
    /// The seed this anchor was derived from. Carried so every downstream
    /// deriver can open its own salted sub-stream (`seed ^ DERIVER_SALT`)
    /// without re-hashing the DID — the anchor is the single seed source.
    pub seed: u64,
    /// Anchor hue (degrees `[0, 360)`) for the OkLCH palette deriver.
    pub base_hue_deg: f32,
    /// `[-1, 1]` cool → warm bias. Shifts accent colours and material
    /// tones toward blue/cyan (`-1`) or amber/orange (`+1`).
    pub temperature: f32,
    /// Body plan — humanoid / boat / airship / skiff. Picked via the
    /// existing [`ChassisFamily::for_seed`] so this anchor stays
    /// bit-compatible with the standalone chassis pick.
    pub chassis: ChassisFamily,
    /// Aesthetic / cultural style — the **same** enum the room uses for its
    /// artificial-structure theme, so an avatar and a room can share one
    /// style vocabulary. Drives palette mood, material kit, ornament pool,
    /// and FX flavour.
    pub style: ThemeArchetype,
    /// `[0, 1]` ornamentation axis: `0` is bare, `1` is heavily decorated.
    /// Read via [`Self::ornateness_tier`]; drives ornament-slot density and
    /// material richness.
    pub ornateness: f32,
    /// `[0, 1]` wear axis: `0` is factory-fresh, `1` is battered. Read via
    /// [`Self::wear_tier`]; drives material finish, surface darkening, and
    /// damage / patina part variants.
    pub wear: f32,
    /// Surface-finish register (bold/stylised vs naturalistic) — a coin-flip
    /// that splits the population between vivid and grounded looks.
    pub finish: FinishRegister,
}

impl AvatarCharacter {
    /// Derive the character anchor from an avatar-owner DID. Stable across
    /// peers because [`fnv1a_64`] is bit-exact and [`ChaCha8Rng`] is
    /// deterministic.
    pub fn for_did(did: &str) -> Self {
        Self::for_seed(fnv1a_64(did))
    }

    /// Derive from a pre-computed seed — the manual re-roll path.
    /// `for_did(did)` is exactly `for_seed(fnv1a_64(did))`.
    pub fn for_seed(seed: u64) -> Self {
        // The chassis is drawn by the existing standalone pick (its own
        // salt) so this anchor is bit-compatible with `ChassisFamily::
        // for_seed` and the two never diverge.
        let chassis = ChassisFamily::for_seed(seed);

        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ AVATAR_CHARACTER_SALT);
        let base_hue_deg = unit_f32(&mut rng) * 360.0;
        let temperature = signed_unit_f32(&mut rng);
        let style = pick(&ThemeArchetype::ALL, &mut rng);
        // The socio-style axes are the last two draws, orthogonal to
        // everything above: appending them leaves every prior field
        // bit-identical to before they existed (the same discipline
        // `SceneCharacter` uses for its prosperity / escalation axes).
        let ornateness = unit_f32(&mut rng);
        let wear = unit_f32(&mut rng);
        // Appended last (orthogonal draw) so every prior field stays
        // bit-identical to before the register existed.
        let finish = if unit_f32(&mut rng) < 0.5 {
            FinishRegister::Bold
        } else {
            FinishRegister::Naturalistic
        };

        Self {
            seed,
            base_hue_deg,
            temperature,
            chassis,
            style,
            ornateness,
            wear,
            finish,
        }
    }

    /// Discrete ornamentation reading of [`Self::ornateness`], thresholded
    /// into equal thirds of `[0, 1]`.
    pub fn ornateness_tier(&self) -> OrnatenessTier {
        OrnatenessTier::from_unit(self.ornateness)
    }

    /// Discrete wear reading of [`Self::wear`], thresholded into equal
    /// thirds of `[0, 1]`.
    pub fn wear_tier(&self) -> WearTier {
        WearTier::from_unit(self.wear)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn determinism_across_calls() {
        let a = AvatarCharacter::for_did("did:plc:abc");
        let b = AvatarCharacter::for_did("did:plc:abc");
        assert_eq!(a.base_hue_deg, b.base_hue_deg);
        assert_eq!(a.temperature, b.temperature);
        assert_eq!(a.chassis, b.chassis);
        assert_eq!(a.style, b.style);
        assert_eq!(a.ornateness, b.ornateness);
        assert_eq!(a.wear, b.wear);
    }

    #[test]
    fn for_did_equals_for_seed_of_hashed_did() {
        let did = "did:plc:anchor";
        let a = AvatarCharacter::for_did(did);
        let b = AvatarCharacter::for_seed(fnv1a_64(did));
        assert_eq!(a.base_hue_deg, b.base_hue_deg);
        assert_eq!(a.style, b.style);
        assert_eq!(a.ornateness, b.ornateness);
    }

    #[test]
    fn chassis_matches_standalone_pick() {
        // The anchor must agree with the standalone chassis pick for every
        // seed — they share a salt-chain and downstream wiring relies on it.
        for s in 0u64..128 {
            assert_eq!(
                AvatarCharacter::for_seed(s).chassis,
                ChassisFamily::for_seed(s),
                "anchor chassis diverged from standalone pick at seed {s}"
            );
        }
    }

    #[test]
    fn fields_in_range() {
        for s in 0u64..64 {
            let c = AvatarCharacter::for_seed(s);
            assert!((0.0..360.0).contains(&c.base_hue_deg));
            assert!((-1.0..1.0).contains(&c.temperature));
            assert!((0.0..=1.0).contains(&c.ornateness));
            assert!((0.0..=1.0).contains(&c.wear));
        }
    }

    #[test]
    fn socio_axes_non_degenerate() {
        // Neither axis is stuck on one tier across seeds (a degenerate draw
        // would collapse to a single tier and break band gating).
        let mut ornateness_tiers: Vec<OrnatenessTier> = Vec::new();
        let mut wear_tiers: Vec<WearTier> = Vec::new();
        for s in 0u64..96 {
            let c = AvatarCharacter::for_seed(s);
            if !ornateness_tiers.contains(&c.ornateness_tier()) {
                ornateness_tiers.push(c.ornateness_tier());
            }
            if !wear_tiers.contains(&c.wear_tier()) {
                wear_tiers.push(c.wear_tier());
            }
        }
        assert_eq!(ornateness_tiers.len(), 3, "ornateness tiers degenerate");
        assert_eq!(wear_tiers.len(), 3, "wear tiers degenerate");
    }

    #[test]
    fn finish_register_varies_and_is_deterministic() {
        assert_eq!(
            AvatarCharacter::for_seed(7).finish,
            AvatarCharacter::for_seed(7).finish
        );
        let (mut bold, mut nat) = (false, false);
        for s in 0u64..64 {
            match AvatarCharacter::for_seed(s).finish {
                FinishRegister::Bold => bold = true,
                FinishRegister::Naturalistic => nat = true,
            }
        }
        assert!(bold && nat, "finish register collapsed to one variant");
    }

    #[test]
    fn style_varies_across_seeds() {
        // The style draw is wired and not stuck on one variant.
        let mut seen: Vec<ThemeArchetype> = Vec::new();
        for s in 0u64..64 {
            let t = AvatarCharacter::for_seed(s).style;
            if !seen.contains(&t) {
                seen.push(t);
            }
        }
        assert!(seen.len() >= 5, "style pick looks degenerate: {seen:?}");
    }

    #[test]
    fn tier_thresholds_split_into_thirds() {
        let tier_at = |v: f32| {
            let mut c = AvatarCharacter::for_seed(0);
            c.ornateness = v;
            c.wear = v;
            (c.ornateness_tier(), c.wear_tier())
        };
        assert_eq!(tier_at(0.0), (OrnatenessTier::Plain, WearTier::Pristine));
        assert_eq!(tier_at(0.33), (OrnatenessTier::Plain, WearTier::Pristine));
        assert_eq!(tier_at(0.34), (OrnatenessTier::Adorned, WearTier::Worn));
        assert_eq!(tier_at(0.66), (OrnatenessTier::Adorned, WearTier::Worn));
        assert_eq!(tier_at(0.67), (OrnatenessTier::Ornate, WearTier::Battered));
        assert_eq!(tier_at(1.0), (OrnatenessTier::Ornate, WearTier::Battered));
    }

    #[test]
    fn band_any_accepts_every_tier() {
        for t in OrnatenessTier::ALL {
            assert!(OrnatenessBand::ANY.accepts(t));
        }
        for t in WearTier::ALL {
            assert!(WearBand::ANY.accepts(t));
        }
    }

    #[test]
    fn band_only_and_range_gate_correctly() {
        let ornate = OrnatenessBand::only(OrnatenessTier::Ornate);
        assert!(ornate.accepts(OrnatenessTier::Ornate));
        assert!(!ornate.accepts(OrnatenessTier::Plain));
        assert!(!ornate.accepts(OrnatenessTier::Adorned));

        // Plain..=Adorned excludes only the top tier.
        let low = OrnatenessBand::range(OrnatenessTier::Plain, OrnatenessTier::Adorned);
        assert!(low.accepts(OrnatenessTier::Plain));
        assert!(low.accepts(OrnatenessTier::Adorned));
        assert!(!low.accepts(OrnatenessTier::Ornate));

        let battered = WearBand::only(WearTier::Battered);
        assert!(battered.accepts(WearTier::Battered));
        assert!(!battered.accepts(WearTier::Pristine));
    }

    #[test]
    fn band_labels_read_naturally() {
        assert_eq!(OrnatenessBand::ANY.label(), "Any");
        assert_eq!(
            OrnatenessBand::only(OrnatenessTier::Ornate).label(),
            "Ornate"
        );
        assert_eq!(
            OrnatenessBand::range(OrnatenessTier::Plain, OrnatenessTier::Adorned).label(),
            "Plain–Adorned"
        );
        assert_eq!(WearBand::ANY.label(), "Any");
    }

    #[test]
    fn distinct_dids_vary() {
        let a = AvatarCharacter::for_did("did:plc:abc");
        let b = AvatarCharacter::for_did("did:plc:def");
        // At least one field differs; hue is the most sensitive.
        assert!((a.base_hue_deg - b.base_hue_deg).abs() > 1e-6);
    }
}
