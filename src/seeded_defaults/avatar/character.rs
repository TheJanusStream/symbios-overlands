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
//! Two discrete axes anchor the design space - the [`ChassisFamily`] (the
//! body plan: humanoid / boat / airship / skiff) and the
//! [`ThemeArchetype`] *style* (deliberately the **same** enum the room
//! uses, so a cyberpunk avatar and a cyberpunk room speak one style
//! vocabulary). Two continuous socio-style axes follow - `ornateness`
//! (plain ↔ ornate) and `wear` (pristine ↔ battered) - read via
//! [`OrnatenessTier`] / [`WearTier`] and the catalogue-eligibility bands
//! [`OrnatenessBand`] / [`WearBand`], exactly as the room reads prosperity
//! and escalation.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use super::chassis::ChassisFamily;
use super::craft::CraftType;
use crate::seeded_defaults::hash::fnv1a_64;
use crate::seeded_defaults::scene::{
    ThemeArchetype, find_matching_seed, pick, signed_unit_f32, unit_f32,
};

/// Sub-stream salt for the character anchor - distinct from every
/// per-domain avatar deriver salt so the anchor's draws never alias a
/// downstream stream.
const AVATAR_CHARACTER_SALT: u64 = 0xA7A7_C4A7_C4A7_A7A7;

/// Ornamentation tier - the discrete reading of the continuous
/// [`AvatarCharacter::ornateness`] axis (plain → ornate). Thresholded into
/// thirds. Drives ornament-slot density (hats, finials, pauldrons, trim)
/// and which cross-style ornament pool a part draws from.
///
/// Variants are declared plainest-first so the derived [`Ord`] matches the
/// axis direction - [`OrnatenessBand`] relies on that ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OrnatenessTier {
    /// Bottom third - bare, functional, no extra trim.
    Plain,
    /// Middle third - a little ornament; one or two accents.
    Adorned,
    /// Top third - heavily decorated; finials, filigree, full kit.
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

/// Wear tier - the discrete reading of the continuous
/// [`AvatarCharacter::wear`] axis (pristine → battered). Thresholded into
/// thirds. Drives material finish (gloss ↔ grime), surface darkening /
/// oxidation, and battle-damage / patina part variants.
///
/// Variants are declared cleanest-first so the derived [`Ord`] matches the
/// axis direction - [`WearBand`] relies on that ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WearTier {
    /// Bottom third - clean, polished, factory-fresh.
    Pristine,
    /// Middle third - used, lightly scuffed and dulled.
    Worn,
    /// Top third - beaten up: grime, oxidation, visible damage.
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

/// Surface-finish register - a per-avatar coin-flip between a saturated,
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

/// Inclusive wear-tier affinity band - the [`WearTier`] analogue of
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
    /// without re-hashing the DID - the anchor is the single seed source.
    pub seed: u64,
    /// Anchor hue (degrees `[0, 360)`) for the OkLCH palette deriver.
    pub base_hue_deg: f32,
    /// `[-1, 1]` cool → warm bias. Shifts accent colours and material
    /// tones toward blue/cyan (`-1`) or amber/orange (`+1`).
    pub temperature: f32,
    /// Body plan - humanoid / boat / airship / skiff. Picked via the
    /// existing [`ChassisFamily::for_seed`] so this anchor stays
    /// bit-compatible with the standalone chassis pick.
    pub chassis: ChassisFamily,
    /// Aesthetic / cultural style - the **same** enum the room uses for its
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
    /// Surface-finish register (bold/stylised vs naturalistic) - a coin-flip
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

    /// Derive from a pre-computed seed - the manual re-roll path.
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

/// Transient per-axis locks for the Avatar editor's pinned re-roll
/// (#1005) - the avatar analogue of
/// [`crate::seeded_defaults::scene::ScenePins`], and the reason the whole
/// feature is a seed *hunt*: unlike the room pipeline (which threads one
/// `SceneCharacter` through every deriver), the avatar derivers each
/// re-derive from the raw seed deep inside the family builders, so
/// overriding an anchor field centrally could desynchronise them. Hunting
/// a seed that *naturally* rolls the pinned axes leaves every deriver
/// untouched and mutually consistent. Pins are editor UI state only;
/// nothing is stored in the record.
///
/// # Four independent axes, and one that is not (#1380)
///
/// `chassis`, `style`, `ornateness` and `wear` are drawn from decorrelated
/// sub-streams, so *every* combination of them occurs somewhere in the seed
/// space: any pin set the editor can build is reachable, and the hunt's
/// `PIN_HUNT_CAP` is the safety net its own doc calls practically
/// unreachable.
///
/// [`Self::craft`] breaks that. It depends on the chassis absolutely (there
/// are no craft types under Airship or Humanoid) and on the style by
/// weight: a type that is neither its family's floor nor at home on a theme
/// scores zero there and can never be rolled. 184 of the 288 (type, style)
/// pairs are unreachable, so a naive combo would offer mostly pin sets no
/// seed satisfies - and each one would walk the *whole* two-million-trial
/// cap before failing, 0.38 s native and several times that on wasm, on the
/// UI thread. Three things keep the cap a safety net rather than a code
/// path: [`Self::craft_gate`] and [`Self::style_gate`] stop the UI offering
/// an unreachable pair, [`Self::lock_craft`] and [`Self::set_chassis`] hold
/// the chassis coupling so the families can never disagree, and
/// [`Self::is_reachable`] answers `None` without a single trial if one is
/// somehow built anyway.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct AvatarPins {
    pub chassis: Option<ChassisFamily>,
    pub style: Option<ThemeArchetype>,
    /// Pinned as the discrete tier; the hunt accepts any seed whose
    /// continuous ornateness falls in the tier's third, so the found
    /// avatar keeps a natural in-distribution value.
    pub ornateness: Option<OrnatenessTier>,
    /// Pinned as the discrete tier, like [`Self::ornateness`].
    pub wear: Option<WearTier>,
    /// The craft type inside the vehicle families - the one *dependent*
    /// axis, and the reason the hunt's predicate takes a seed rather than
    /// an [`AvatarCharacter`]: the type is a second draw on its own salt,
    /// so it cannot be read off the anchor.
    ///
    /// Write it through [`Self::lock_craft`] and [`Self::set_chassis`],
    /// never field-by-field: the two carry the coupling rules that keep a
    /// pinned craft and a pinned chassis in the same family.
    pub craft: Option<CraftType>,
}

impl AvatarPins {
    /// Whether `c` satisfies every pinned axis that can be read off the
    /// anchor (unpinned axes accept anything).
    ///
    /// This is the four independent axes only. [`Self::craft`] needs a
    /// second derivation and so lives in [`Self::accepts`], which is what
    /// the hunt calls.
    pub fn matches(&self, c: &AvatarCharacter) -> bool {
        self.chassis.is_none_or(|p| p == c.chassis)
            && self.style.is_none_or(|p| p == c.style)
            && self.ornateness.is_none_or(|p| p == c.ornateness_tier())
            && self.wear.is_none_or(|p| p == c.wear_tier())
    }

    /// Whether the seed `seed` satisfies every pin - the hunt's predicate.
    ///
    /// Takes the seed rather than an [`AvatarCharacter`] because the craft
    /// type is not on the anchor; it derives the anchor **once** and hands
    /// it to [`CraftType::for_character`], so a pinned craft costs one
    /// extra ChaCha8 stream per trial rather than a whole second
    /// character. The four-axis test runs first and short-circuits, so an
    /// unpinned craft costs nothing at all.
    pub fn accepts(&self, seed: u64) -> bool {
        let c = AvatarCharacter::for_seed(seed);
        self.matches(&c)
            && self
                .craft
                .is_none_or(|p| CraftType::for_character(&c) == Some(p))
    }

    /// Whether *any* seed can satisfy these pins.
    ///
    /// Always true for the four independent axes; false only for a craft
    /// pinned against a chassis of another family, or against a style whose
    /// draw gives it zero weight. The UI cannot build either (see the type
    /// doc), and this is the second line: without it an unreachable set
    /// spends the whole `PIN_HUNT_CAP` on the UI thread to learn what the
    /// affinity table answers in constant time.
    pub fn is_reachable(&self) -> bool {
        let Some(craft) = self.craft else {
            return true;
        };
        self.chassis.is_none_or(|f| f == craft.family())
            && self.style.is_none_or(|s| craft.weight(s) > 0)
    }

    /// The first seed at or after `start` satisfying every pin. With no
    /// pins this is `start` itself, so the un-pinned path is bit-identical
    /// to the pre-#1005 re-roll.
    pub fn find_seed(&self, start: u64) -> Option<u64> {
        if *self == Self::default() {
            return Some(start);
        }
        if !self.is_reachable() {
            return None;
        }
        find_matching_seed(start, |s| self.accepts(s))
    }

    /// Why the craft combo must refuse `craft` under these pins, or `None`
    /// when it is reachable. With the style unpinned nothing is refused.
    ///
    /// The reason is a **clause**, not a sentence: the row paints
    /// `"{label} - {why}"` ("Longship - never rolls on Cyberpunk"). It is
    /// painted rather than hovered because neither `on_hover_text` nor
    /// `on_disabled_hover_text` fires inside an open `ComboBox`'s rows
    /// (#1337), and a disabled option that cannot say why reads as a bug.
    pub fn craft_gate(&self, craft: CraftType) -> Option<String> {
        if let Some(chassis) = self.chassis
            && craft.family() != chassis
        {
            return Some(format!("not carried by the {} chassis", chassis.label()));
        }
        let style = self.style?;
        (craft.weight(style) == 0).then(|| format!("never rolls on {}", style.label()))
    }

    /// Why the style combo must refuse `style` under these pins, or `None`
    /// when it can reach the pinned craft - the mirror of
    /// [`Self::craft_gate`], painted the same way ("Cyberpunk - no
    /// Longship"). With the craft unpinned nothing is refused.
    pub fn style_gate(&self, style: ThemeArchetype) -> Option<String> {
        let craft = self.craft?;
        (craft.weight(style) == 0).then(|| format!("no {}", craft.label()))
    }

    /// Pin (or clear) the craft type, holding the chassis coupling.
    ///
    /// Locking a craft locks the chassis to its family: a pinned longship
    /// implies a Boat, and a chassis row reading "unlocked" while it could
    /// never change would be a lie.
    ///
    /// A no-op when nothing changes. The combo redraws every frame, and a
    /// re-write of the same value would re-key `PinHuntCache` and re-run
    /// the whole hunt on each one.
    pub fn lock_craft(&mut self, craft: Option<CraftType>) {
        if self.craft == craft {
            return;
        }
        self.craft = craft;
        if let Some(c) = craft {
            self.chassis = Some(c.family());
        }
    }

    /// Pin (or clear) the chassis family. **Any** change clears the craft
    /// pin - the brief's rule, and the only consistent one: one family's
    /// types mean nothing under another, and two of the four families have
    /// none at all. A no-op when unchanged, for the reason in
    /// [`Self::lock_craft`].
    pub fn set_chassis(&mut self, chassis: Option<ChassisFamily>) {
        if self.chassis == chassis {
            return;
        }
        self.chassis = chassis;
        self.craft = None;
    }
}

#[cfg(test)]
mod tests {
    use super::super::craft::{BoatType, SkiffType};
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
        // seed - they share a salt-chain and downstream wiring relies on it.
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

    #[test]
    fn empty_pins_hunt_returns_the_start_seed() {
        // The un-pinned re-roll must stay bit-identical to pre-#1005:
        // no pins → the clicked seed is used verbatim.
        assert_eq!(AvatarPins::default().find_seed(42), Some(42));
    }

    #[test]
    fn pinned_hunt_is_deterministic_and_satisfies_the_pins() {
        let pins = AvatarPins {
            chassis: Some(ChassisFamily::Airship),
            style: Some(ThemeArchetype::Steampunk),
            wear: Some(WearTier::Battered),
            ..Default::default()
        };
        let found = pins.find_seed(0).expect("hunt failed");
        let c = AvatarCharacter::for_seed(found);
        assert_eq!(c.chassis, ChassisFamily::Airship);
        assert_eq!(c.style, ThemeArchetype::Steampunk);
        assert_eq!(c.wear_tier(), WearTier::Battered);
        assert_eq!(pins.find_seed(0), Some(found));
    }

    #[test]
    fn a_seed_that_already_matches_is_kept() {
        // Re-rolling from a found seed with the same pins must be a
        // fixpoint, matching the World-editor contract.
        let pins = AvatarPins {
            chassis: Some(ChassisFamily::Humanoid),
            ..Default::default()
        };
        let found = pins.find_seed(7).expect("hunt failed");
        assert_eq!(pins.find_seed(found), Some(found));
    }

    #[test]
    fn every_variant_of_every_axis_is_huntable() {
        // Each axis variant pinned alone must be reachable from a fixed
        // start - a variant the hunt can never satisfy would make its
        // combo option a dead button.
        for f in ChassisFamily::ALL {
            let pins = AvatarPins {
                chassis: Some(f),
                ..Default::default()
            };
            let s = pins.find_seed(0).expect("chassis unreachable");
            assert_eq!(AvatarCharacter::for_seed(s).chassis, f);
        }
        for t in ThemeArchetype::ALL {
            let pins = AvatarPins {
                style: Some(t),
                ..Default::default()
            };
            let s = pins.find_seed(0).expect("style unreachable");
            assert_eq!(AvatarCharacter::for_seed(s).style, t);
        }
        for o in OrnatenessTier::ALL {
            let pins = AvatarPins {
                ornateness: Some(o),
                ..Default::default()
            };
            let s = pins.find_seed(0).expect("ornateness unreachable");
            assert_eq!(AvatarCharacter::for_seed(s).ornateness_tier(), o);
        }
        for w in WearTier::ALL {
            let pins = AvatarPins {
                wear: Some(w),
                ..Default::default()
            };
            let s = pins.find_seed(0).expect("wear unreachable");
            assert_eq!(AvatarCharacter::for_seed(s).wear_tier(), w);
        }
    }

    #[test]
    fn fully_pinned_hunt_succeeds() {
        // Four axes at once (~1 in 828 seeds) - the hardest set until the
        // craft joined them. Must land inside the hunt cap with a match.
        let pins = AvatarPins {
            chassis: Some(ChassisFamily::Skiff),
            style: Some(ThemeArchetype::AlienOrganic),
            ornateness: Some(OrnatenessTier::Ornate),
            wear: Some(WearTier::Pristine),
            ..Default::default()
        };
        let s = pins
            .find_seed(0xC0FF_EE00)
            .expect("full pin-set unreachable");
        assert!(pins.matches(&AvatarCharacter::for_seed(s)));
    }

    /// The five-axis worst case, MEASURED (#1380): sweeping all 936
    /// reachable pin sets from start 0, this one walks furthest - 52 571
    /// trials, 2.6 % of `PIN_HUNT_CAP`. The assertion is the margin, not
    /// the number: an affinity or draw change that pushed a legal set near
    /// the cap would turn a safety net into a stall on the UI thread.
    #[test]
    fn the_measured_worst_pin_set_stays_far_inside_the_cap() {
        let pins = AvatarPins {
            chassis: Some(ChassisFamily::Skiff),
            style: Some(ThemeArchetype::PostApoc),
            ornateness: Some(OrnatenessTier::Ornate),
            wear: Some(WearTier::Worn),
            craft: Some(CraftType::Skiff(SkiffType::Roadster)),
        };
        const START: u64 = 0;
        let s = pins
            .find_seed(START)
            .expect("the measured worst case missed");
        assert!(pins.accepts(s));
        let trials = s - START;
        assert!(
            trials < 200_000,
            "the worst legal pin set now walks {trials} trials; it was 52 571 at b1b4648, \
             and PIN_HUNT_CAP is 2 000 000 on the UI thread"
        );
    }

    // --- The craft pin: the one dependent axis (#1380) ----------------

    /// Every (type, style) pair, both families - the census the gates are
    /// measured against.
    fn every_pair() -> impl Iterator<Item = (CraftType, ThemeArchetype)> {
        CraftType::BOATS
            .into_iter()
            .chain(CraftType::SKIFFS)
            .flat_map(|c| ThemeArchetype::ALL.into_iter().map(move |t| (c, t)))
    }

    #[test]
    fn the_craft_gate_refuses_exactly_the_zero_weight_pairs() {
        // Exact in both directions, and both gates read the SAME table, so
        // the two counts must agree: a type the craft combo refuses on a
        // style is a style the style combo refuses under that type.
        let (mut refused, mut enabled) = (0, 0);
        for (craft, style) in every_pair() {
            let pins = AvatarPins {
                style: Some(style),
                ..Default::default()
            };
            let zero = craft.weight(style) == 0;
            assert_eq!(
                pins.craft_gate(craft).is_some(),
                zero,
                "craft_gate disagrees with weight() on {} / {}",
                craft.label(),
                style.label()
            );

            let mirrored = AvatarPins {
                craft: Some(craft),
                chassis: Some(craft.family()),
                ..Default::default()
            };
            assert_eq!(
                mirrored.style_gate(style).is_some(),
                zero,
                "style_gate disagrees with craft_gate on {} / {}",
                craft.label(),
                style.label()
            );

            if zero {
                refused += 1;
            } else {
                enabled += 1;
            }
        }
        // Measured at b1b4648: 92 of 144 unreachable in EACH family. Pinned
        // so that an affinity edit has to come here on purpose - the table
        // is a taste call, and a silent change to it changes which pin sets
        // an owner can even build.
        assert_eq!(
            (refused, enabled),
            (184, 104),
            "the (type, style) census moved"
        );
    }

    #[test]
    fn neither_gate_refuses_anything_while_the_other_axis_is_free() {
        // A gate that fired on an unpinned partner would grey out options
        // for no reason an owner could see or undo.
        for (craft, style) in every_pair() {
            let no_style = AvatarPins {
                chassis: Some(craft.family()),
                ..Default::default()
            };
            assert_eq!(no_style.craft_gate(craft), None);
            assert_eq!(AvatarPins::default().style_gate(style), None);
        }
    }

    #[test]
    fn the_craft_gate_refuses_the_other_familys_types() {
        // The combo only ever lists one family, but the gate is the thing
        // the reachability guarantee rests on, so it answers for the pair
        // the UI cannot build as well.
        let boat = AvatarPins {
            chassis: Some(ChassisFamily::Boat),
            ..Default::default()
        };
        assert!(
            boat.craft_gate(CraftType::Skiff(SkiffType::Rover))
                .is_some()
        );
        assert_eq!(boat.craft_gate(CraftType::Boat(BoatType::Sloop)), None);
    }

    #[test]
    fn locking_a_craft_locks_the_chassis_to_its_family() {
        // A pinned longship IS a pinned boat; a chassis row reading
        // "unlocked" beside it would be a lie.
        let mut pins = AvatarPins::default();
        pins.lock_craft(Some(CraftType::Boat(BoatType::Longship)));
        assert_eq!(pins.chassis, Some(ChassisFamily::Boat));
        pins.lock_craft(Some(CraftType::Skiff(SkiffType::Wagon)));
        assert_eq!(pins.chassis, Some(ChassisFamily::Skiff));
        // Clearing the craft leaves the chassis where the owner can see it,
        // rather than silently unlocking a second row they did not touch.
        pins.lock_craft(None);
        assert_eq!(pins.chassis, Some(ChassisFamily::Skiff));
        assert_eq!(pins.craft, None);
    }

    #[test]
    fn changing_or_clearing_the_chassis_clears_the_craft() {
        for next in [
            Some(ChassisFamily::Skiff),
            Some(ChassisFamily::Airship),
            Some(ChassisFamily::Humanoid),
            None,
        ] {
            let mut pins = AvatarPins::default();
            pins.lock_craft(Some(CraftType::Boat(BoatType::Junk)));
            pins.set_chassis(next);
            assert_eq!(pins.chassis, next);
            assert_eq!(pins.craft, None, "a junk survived a move to {next:?}");
        }
        // ...and re-picking the SAME family is not a change, so it must not
        // drop a pin the owner never touched.
        let mut pins = AvatarPins::default();
        pins.lock_craft(Some(CraftType::Boat(BoatType::Junk)));
        pins.set_chassis(Some(ChassisFamily::Boat));
        assert_eq!(pins.craft, Some(CraftType::Boat(BoatType::Junk)));
    }

    #[test]
    fn the_two_families_without_a_type_pick_have_no_craft_and_say_so() {
        for (family, seed_of) in [
            (ChassisFamily::Airship, "an airship"),
            (ChassisFamily::Humanoid, "a rigged avatar"),
        ] {
            let pins = AvatarPins {
                chassis: Some(family),
                ..Default::default()
            };
            let s = pins.find_seed(0).expect("chassis unreachable");
            let c = AvatarCharacter::for_seed(s);
            assert_eq!(CraftType::for_character(&c), None);
            let why = super::super::craft::craft_axis(&c).expect_err("a type where there is none");
            assert!(
                why.starts_with(seed_of),
                "{family:?} explains itself as {why:?}"
            );
        }
    }

    #[test]
    fn an_unreachable_pin_set_costs_no_trials() {
        // The cap is a safety net, not a code path. A Longship on
        // FeudalJapan walked all 2 000 000 trials before this - 0.38 s
        // native, several times that on wasm, on the UI thread - to learn
        // what the affinity table answers in constant time.
        let pins = AvatarPins {
            style: Some(ThemeArchetype::FeudalJapan),
            chassis: Some(ChassisFamily::Boat),
            craft: Some(CraftType::Boat(BoatType::Longship)),
            ..Default::default()
        };
        assert!(!pins.is_reachable());
        assert_eq!(pins.find_seed(0), None);
        // The four independent axes can never build one.
        for f in ChassisFamily::ALL {
            for t in ThemeArchetype::ALL {
                for o in OrnatenessTier::ALL {
                    for w in WearTier::ALL {
                        let pins = AvatarPins {
                            chassis: Some(f),
                            style: Some(t),
                            ornateness: Some(o),
                            wear: Some(w),
                            craft: None,
                        };
                        assert!(pins.is_reachable(), "{f:?}/{t:?}/{o:?}/{w:?} unreachable");
                    }
                }
            }
        }
    }

    #[test]
    fn every_pin_set_the_gate_enables_is_reachable() {
        // The census as a test: for every (type, style) pair the gate
        // ENABLES, a seed exists that rolls it. An option an owner can pick
        // and the hunt can never satisfy is the one failure this slice is
        // built to prevent, and the gate is only as good as this.
        //
        // Ornateness and wear are left free: they are independent draws,
        // so pinning them narrows each hunt without changing WHICH pairs
        // are reachable. Both sweeps were timed in one binary under one
        // profile before this one was chosen: the full 104 x 9 = 936 sets
        // cost 30.9 s under plain `cargo test` against 0.35 s here (0.458 s
        // against 0.005 s at test-release), and CI runs the unoptimised
        // one. The full space is held instead by
        // `the_measured_worst_pin_set_stays_far_inside_the_cap`, whose
        // 52 571 trials are that 936-set sweep's own measured worst.
        let mut enabled = 0;
        for (craft, style) in every_pair() {
            let pins = AvatarPins {
                chassis: Some(craft.family()),
                style: Some(style),
                craft: Some(craft),
                ..Default::default()
            };
            if !pins.is_reachable() {
                continue;
            }
            assert_eq!(pins.craft_gate(craft), None, "gated but reachable");
            let s = pins.find_seed(0).unwrap_or_else(|| {
                panic!(
                    "{} on {} is offered and unreachable",
                    craft.label(),
                    style.label()
                )
            });
            assert!(pins.accepts(s));
            enabled += 1;
        }
        assert_eq!(enabled, 104);
    }

    #[test]
    fn the_four_axis_hunt_did_not_move() {
        // Every seed here was taken at b1b4648, BEFORE the craft pin
        // touched the predicate. With `craft: None` the hunt must still
        // walk to exactly the same seed it did then: the craft is a fifth
        // axis, not a change to the four.
        const AT_B1B4648: [(ChassisFamily, ThemeArchetype, OrnatenessTier, WearTier, u64); 16] = [
            (
                ChassisFamily::Boat,
                ThemeArchetype::Nordic,
                OrnatenessTier::Plain,
                WearTier::Pristine,
                4906,
            ),
            (
                ChassisFamily::Boat,
                ThemeArchetype::Cyberpunk,
                OrnatenessTier::Adorned,
                WearTier::Battered,
                841,
            ),
            (
                ChassisFamily::Boat,
                ThemeArchetype::PostApoc,
                OrnatenessTier::Ornate,
                WearTier::Worn,
                439,
            ),
            (
                ChassisFamily::Boat,
                ThemeArchetype::CoastalResort,
                OrnatenessTier::Plain,
                WearTier::Pristine,
                493,
            ),
            (
                ChassisFamily::Skiff,
                ThemeArchetype::Nordic,
                OrnatenessTier::Adorned,
                WearTier::Worn,
                1173,
            ),
            (
                ChassisFamily::Skiff,
                ThemeArchetype::Cyberpunk,
                OrnatenessTier::Ornate,
                WearTier::Pristine,
                239,
            ),
            (
                ChassisFamily::Skiff,
                ThemeArchetype::PostApoc,
                OrnatenessTier::Plain,
                WearTier::Battered,
                786,
            ),
            (
                ChassisFamily::Skiff,
                ThemeArchetype::CoastalResort,
                OrnatenessTier::Adorned,
                WearTier::Worn,
                830,
            ),
            (
                ChassisFamily::Airship,
                ThemeArchetype::Nordic,
                OrnatenessTier::Ornate,
                WearTier::Battered,
                944,
            ),
            (
                ChassisFamily::Airship,
                ThemeArchetype::Cyberpunk,
                OrnatenessTier::Plain,
                WearTier::Worn,
                862,
            ),
            (
                ChassisFamily::Airship,
                ThemeArchetype::PostApoc,
                OrnatenessTier::Adorned,
                WearTier::Pristine,
                504,
            ),
            (
                ChassisFamily::Airship,
                ThemeArchetype::CoastalResort,
                OrnatenessTier::Ornate,
                WearTier::Battered,
                53,
            ),
            (
                ChassisFamily::Humanoid,
                ThemeArchetype::Nordic,
                OrnatenessTier::Plain,
                WearTier::Pristine,
                1,
            ),
            (
                ChassisFamily::Humanoid,
                ThemeArchetype::Cyberpunk,
                OrnatenessTier::Adorned,
                WearTier::Battered,
                365,
            ),
            (
                ChassisFamily::Humanoid,
                ThemeArchetype::PostApoc,
                OrnatenessTier::Ornate,
                WearTier::Worn,
                215,
            ),
            (
                ChassisFamily::Humanoid,
                ThemeArchetype::CoastalResort,
                OrnatenessTier::Plain,
                WearTier::Pristine,
                1003,
            ),
        ];
        for (chassis, style, ornateness, wear, was) in AT_B1B4648 {
            let pins = AvatarPins {
                chassis: Some(chassis),
                style: Some(style),
                ornateness: Some(ornateness),
                wear: Some(wear),
                craft: None,
            };
            assert_eq!(
                pins.find_seed(0),
                Some(was),
                "{chassis:?}/{style:?}/{ornateness:?}/{wear:?} moved"
            );
        }
        // And the empty set is still the identity - the un-pinned re-roll.
        assert_eq!(AvatarPins::default().find_seed(42), Some(42));
    }
}
