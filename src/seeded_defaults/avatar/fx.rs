//! Seeded avatar FX spec — which signature particle aura and spatial-audio
//! voice an avatar carries, derived from the shared [`AvatarCharacter`]
//! anchor.
//!
//! This is a *spec* deriver in the room's style: it picks discrete FX
//! flavours from the anchor's [`ThemeArchetype`] style + [`ChassisFamily`]
//! and a continuous `intensity`, but builds no geometry. The build side
//! (`crate::pds::avatar::default_visuals::fx`) turns the spec into the
//! actual `ParticleSystem` emitter node + [`SovereignAudioConfig`], reusing
//! the shared catalogue FX toolkit.
//!
//! [`SovereignAudioConfig`]: crate::pds::SovereignAudioConfig
//!
//! Gating mirrors the room's theme accents: only signature styles emit an
//! aura (a cyberpunk avatar trails neon motes, a steampunk one vents
//! steam); the mundane styles stay clean. The voice respects the chassis —
//! a vehicle hums, a luminous figure shimmers, an ordinary figure is
//! silent — so a humanoid never sounds like an idling engine.

use super::character::AvatarCharacter;
use super::chassis::ChassisFamily;
use crate::seeded_defaults::scene::ThemeArchetype;

/// The signature particle aura an avatar trails. Most *styles* carry
/// [`Self::None`]; the speculative / frontier styles each get a flavour.
///
/// The first six are the style-signature auras (picked by
/// [`Self::for_style`]); the last three are the *chassis-signature* floors a
/// vehicle falls back to when its style rolls [`Self::None`] — so no craft
/// idles in dead-clean air (a boat always leaves a wake, an airship always
/// vents, a skiff always trails exhaust). See [`AvatarFx::for_character`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleAura {
    /// No aura — the avatar reads clean.
    None,
    /// Pale rising steam / exhaust — steampunk funnels, industrial vents.
    Steam,
    /// Faint rising neon motes in the accent colour — cyberpunk, monolith.
    NeonHaze,
    /// A downward jet plume — solar / space thrusters.
    Thruster,
    /// Slow drifting arcane / biolume motes — fantasy, alien-organic.
    ArcaneMotes,
    /// Warm upward embers — post-apoc, wild-west braziers / scorched gear.
    Embers,
    /// Boat chassis floor: a low whitewater wake-mist off the stern.
    Wake,
    /// Airship chassis floor: a soft vapour puff venting under the gondola.
    Vent,
    /// Skiff chassis floor: a thin exhaust wisp off the tailpipe.
    Exhaust,
}

impl ParticleAura {
    /// The aura a style trails. Exhaustive over [`ThemeArchetype`] so a new
    /// theme must classify itself.
    fn for_style(style: ThemeArchetype) -> Self {
        use ThemeArchetype::*;
        match style {
            Cyberpunk | AlienMonolithic => Self::NeonHaze,
            Fantasy | AlienOrganic => Self::ArcaneMotes,
            SpaceOutpost | Solarpunk => Self::Thruster,
            Steampunk | IndustrialPark | Roadside => Self::Steam,
            PostApoc | WildWest => Self::Embers,
            Medieval | AncientClassical | Nordic | FeudalJapan | Mesoamerican | ModernCity
            | Suburban | RuralFarmland | CoastalResort | CivicCampus | SportsRec | GothicHorror => {
                Self::None
            }
        }
    }

    /// Whether this aura's density scales with wear (smoke/embers/spray from
    /// working gear) rather than ornateness (decorative motes). The three
    /// chassis floors are mechanical exhaust / churn, so they track wear too.
    fn driven_by_wear(self) -> bool {
        matches!(
            self,
            Self::Steam | Self::Embers | Self::Wake | Self::Vent | Self::Exhaust
        )
    }

    /// The chassis-signature aura a vehicle falls back to when its *style*
    /// rolls [`Self::None`] — the diegetic wake / vent / exhaust every craft
    /// carries. Humanoids have no floor (they read clean).
    fn family_floor(chassis: ChassisFamily) -> Self {
        match chassis {
            ChassisFamily::Boat => Self::Wake,
            ChassisFamily::Airship => Self::Vent,
            ChassisFamily::Skiff => Self::Exhaust,
            ChassisFamily::Humanoid => Self::None,
        }
    }

    /// The effective aura for a style + chassis: the style signature, floored
    /// to the chassis signature when the style is clean, and with the
    /// downward [`Self::Thruster`] jet reserved for airborne craft — a
    /// surface boat / skiff rolling a thruster style would otherwise fire a
    /// jet straight into the water or road, so it trails its wake / exhaust
    /// instead.
    fn effective(style_aura: Self, chassis: ChassisFamily) -> Self {
        if chassis == ChassisFamily::Humanoid {
            return style_aura;
        }
        match style_aura {
            Self::None => Self::family_floor(chassis),
            Self::Thruster if chassis != ChassisFamily::Airship => Self::family_floor(chassis),
            other => other,
        }
    }
}

/// The spatial-audio voice an avatar emits at its body. Kept small; the
/// build side maps each to a synth patch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AvatarVoice {
    /// Silent.
    None,
    /// A low mechanical drone — vehicle engines / industrial styles.
    EngineHum,
    /// A buzzing electric hum — neon styles.
    NeonBuzz,
    /// A soft tonal shimmer — arcane / biolume / solar styles.
    ArcaneShimmer,
}

/// All seeded avatar FX.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AvatarFx {
    pub aura: ParticleAura,
    pub voice: AvatarVoice,
    /// Emitter rate / population multiplier (`~0.7..1.6`). Decorative auras
    /// scale with ornateness; smoke/ember auras scale with wear.
    pub intensity: f32,
}

impl AvatarFx {
    pub fn for_did(did: &str) -> Self {
        Self::for_character(&AvatarCharacter::for_did(did))
    }

    pub fn for_seed(seed: u64) -> Self {
        Self::for_character(&AvatarCharacter::for_seed(seed))
    }

    /// Derive the FX spec from the shared avatar anchor.
    pub fn for_character(c: &AvatarCharacter) -> Self {
        // The style picks the flavour; the chassis floors it so every vehicle
        // trails a diegetic wake / vent / exhaust even under a clean style,
        // and a surface craft never fires a downward thruster jet.
        let aura = ParticleAura::effective(ParticleAura::for_style(c.style), c.chassis);
        let voice = voice_for(c.style, c.chassis);
        // Decorative auras (motes / neon / thruster) thicken with ornateness;
        // smoke / ember auras thicken with wear (a battered engine smokes).
        let driver = if aura.driven_by_wear() {
            c.wear
        } else {
            c.ornateness
        };
        let intensity = 0.7 + 0.9 * driver.clamp(0.0, 1.0);
        Self {
            aura,
            voice,
            intensity,
        }
    }
}

/// The voice for a style+chassis. Luminous styles speak first (neon /
/// arcane), then any vehicle chassis hums; an ordinary figure is silent.
fn voice_for(style: ThemeArchetype, chassis: ChassisFamily) -> AvatarVoice {
    use ThemeArchetype::*;
    match style {
        Cyberpunk | AlienMonolithic => AvatarVoice::NeonBuzz,
        Fantasy | AlienOrganic | Solarpunk => AvatarVoice::ArcaneShimmer,
        _ if chassis != ChassisFamily::Humanoid => AvatarVoice::EngineHum,
        _ => AvatarVoice::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = AvatarFx::for_did("did:plc:abc");
        let b = AvatarFx::for_did("did:plc:abc");
        assert_eq!(a, b);
    }

    #[test]
    fn every_style_classifies_an_aura() {
        // Exhaustive match means this can't panic; assert the signature
        // styles light up and a mundane one stays clean.
        for style in ThemeArchetype::ALL {
            let _ = ParticleAura::for_style(style);
        }
        assert_eq!(
            ParticleAura::for_style(ThemeArchetype::Cyberpunk),
            ParticleAura::NeonHaze
        );
        assert_eq!(
            ParticleAura::for_style(ThemeArchetype::Steampunk),
            ParticleAura::Steam
        );
        assert_eq!(
            ParticleAura::for_style(ThemeArchetype::Medieval),
            ParticleAura::None
        );
    }

    #[test]
    fn every_vehicle_family_and_style_carries_an_aura() {
        // The chassis floor guarantees no vehicle idles in dead-clean air:
        // for every non-humanoid family, every style resolves to a non-None
        // aura (the style signature, or the family wake / vent / exhaust).
        for chassis in [
            ChassisFamily::Boat,
            ChassisFamily::Airship,
            ChassisFamily::Skiff,
        ] {
            for style in ThemeArchetype::ALL {
                let aura = ParticleAura::effective(ParticleAura::for_style(style), chassis);
                assert_ne!(
                    aura,
                    ParticleAura::None,
                    "{chassis:?} + {style:?} left a vehicle with no aura"
                );
            }
        }
        // The floors are exactly the family signatures.
        assert_eq!(
            ParticleAura::effective(ParticleAura::None, ChassisFamily::Boat),
            ParticleAura::Wake
        );
        assert_eq!(
            ParticleAura::effective(ParticleAura::None, ChassisFamily::Airship),
            ParticleAura::Vent
        );
        assert_eq!(
            ParticleAura::effective(ParticleAura::None, ChassisFamily::Skiff),
            ParticleAura::Exhaust
        );
    }

    #[test]
    fn downward_thruster_is_reserved_for_airborne_craft() {
        // A Solarpunk / SpaceOutpost boat or skiff would fire a jet into the
        // water / road — it trails its family wake / exhaust instead. An
        // airship keeps the thruster wash, and a humanoid is unchanged.
        assert_eq!(
            ParticleAura::effective(ParticleAura::Thruster, ChassisFamily::Boat),
            ParticleAura::Wake
        );
        assert_eq!(
            ParticleAura::effective(ParticleAura::Thruster, ChassisFamily::Skiff),
            ParticleAura::Exhaust
        );
        assert_eq!(
            ParticleAura::effective(ParticleAura::Thruster, ChassisFamily::Airship),
            ParticleAura::Thruster
        );
        assert_eq!(
            ParticleAura::effective(ParticleAura::Thruster, ChassisFamily::Humanoid),
            ParticleAura::Thruster
        );
    }

    #[test]
    fn a_styled_vehicle_keeps_its_style_aura() {
        // The floor only fills a clean style — a themed aura is preserved
        // (a steampunk boat still steams; it does not fall back to a wake).
        assert_eq!(
            ParticleAura::effective(ParticleAura::Steam, ChassisFamily::Boat),
            ParticleAura::Steam
        );
        assert_eq!(
            ParticleAura::effective(ParticleAura::NeonHaze, ChassisFamily::Skiff),
            ParticleAura::NeonHaze
        );
        // A humanoid never floors — a clean style stays clean.
        assert_eq!(
            ParticleAura::effective(ParticleAura::None, ChassisFamily::Humanoid),
            ParticleAura::None
        );
    }

    #[test]
    fn humanoid_never_hums_like_an_engine() {
        // A non-luminous humanoid is silent; a vehicle of the same style
        // hums. (Medieval is non-luminous, so it exercises the chassis arm.)
        let mut human = AvatarCharacter::for_seed(1);
        human.style = ThemeArchetype::Medieval;
        human.chassis = ChassisFamily::Humanoid;
        assert_eq!(AvatarFx::for_character(&human).voice, AvatarVoice::None);

        let mut boat = human;
        boat.chassis = ChassisFamily::Boat;
        assert_eq!(AvatarFx::for_character(&boat).voice, AvatarVoice::EngineHum);
    }

    #[test]
    fn luminous_style_speaks_even_as_a_humanoid() {
        let mut c = AvatarCharacter::for_seed(2);
        c.style = ThemeArchetype::Cyberpunk;
        c.chassis = ChassisFamily::Humanoid;
        assert_eq!(AvatarFx::for_character(&c).voice, AvatarVoice::NeonBuzz);
    }

    #[test]
    fn decorative_aura_thickens_with_ornateness_smoke_with_wear() {
        // NeonHaze (decorative) tracks ornateness; Steam tracks wear.
        let mut neon = AvatarCharacter::for_seed(3);
        neon.style = ThemeArchetype::Cyberpunk;
        neon.ornateness = 0.0;
        neon.wear = 1.0; // wear must NOT move a decorative aura
        let lo = AvatarFx::for_character(&neon).intensity;
        neon.ornateness = 1.0;
        let hi = AvatarFx::for_character(&neon).intensity;
        assert!(hi > lo, "neon should thicken with ornateness");

        let mut steam = AvatarCharacter::for_seed(4);
        steam.style = ThemeArchetype::Steampunk;
        steam.ornateness = 1.0; // ornateness must NOT move a smoke aura
        steam.wear = 0.0;
        let clean = AvatarFx::for_character(&steam).intensity;
        steam.wear = 1.0;
        let smoky = AvatarFx::for_character(&steam).intensity;
        assert!(smoky > clean, "steam should thicken with wear");
    }

    #[test]
    fn intensity_stays_bounded() {
        for s in 0u64..64 {
            let fx = AvatarFx::for_seed(s);
            assert!((0.6..=1.7).contains(&fx.intensity), "intensity OOB: {fx:?}");
        }
    }
}
