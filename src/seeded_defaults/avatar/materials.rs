//! Seeded material-finish kit — the partner of [`super::palette`].
//!
//! Where [`super::palette::AvatarPalette`] decides an avatar's *colours*,
//! the [`MaterialKit`] decides their *finish*: how metallic / rough / self-
//! lit each surface reads, biased by the avatar's [`ThemeArchetype`] style
//! (a cyberpunk avatar's accents glow and its panels read as dark gloss
//! metal; a medieval one's are matte cloth and polished brass) and dulled
//! by the anchor `wear` (a battered avatar's surfaces are grimier, darker,
//! and rougher).
//!
//! The kit produces ready-to-use [`SovereignMaterialSettings`] for a small
//! set of named surface roles. Builders and — once the part catalogue
//! lands — part constructors pass a palette colour to a role method and get
//! back a fully-finished material, so the style/wear logic lives in exactly
//! one place instead of being re-derived per builder.

use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::texture::{
    SovereignFabricConfig, SovereignMaterialSettings, SovereignMetalConfig, SovereignTextureConfig,
};
use crate::pds::types::{Fp, Fp3, Fp64};
use crate::seeded_defaults::scene::ThemeArchetype;

use super::character::{AvatarCharacter, FinishRegister};

/// Per-style finish family — the PBR character a style gives its hard
/// surfaces, plus whether its accents are self-lit. The 23 themes group
/// into four families so the kit stays compact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FinishFamily {
    /// Gloss / industrial metal: high metallic, low roughness.
    Metal,
    /// Matte painted / fabric / stone: low metallic, high roughness.
    Matte,
    /// Living / arcane: soft sheen, self-lit accents.
    Organic,
    /// Bright clean enamel: mid metallic, mid-low roughness.
    Clean,
}

impl FinishFamily {
    fn for_style(style: ThemeArchetype) -> Self {
        use ThemeArchetype::*;
        match style {
            Cyberpunk | IndustrialPark | ModernCity | SpaceOutpost | Steampunk
            | AlienMonolithic => Self::Metal,
            Medieval | AncientClassical | Nordic | Mesoamerican | RuralFarmland | Roadside
            | PostApoc | WildWest | GothicHorror => Self::Matte,
            Fantasy | Solarpunk | AlienOrganic | FeudalJapan => Self::Organic,
            CoastalResort | CivicCampus | SportsRec | Suburban => Self::Clean,
        }
    }

    /// `(metallic, roughness)` for the family's main painted body surface.
    fn body_pbr(self) -> (f32, f32) {
        match self {
            Self::Metal => (0.55, 0.35),
            Self::Matte => (0.05, 0.85),
            Self::Organic => (0.15, 0.55),
            Self::Clean => (0.25, 0.45),
        }
    }
}

/// Whether a *specific* style lights its accents. Kept separate from the
/// finish family because luminosity doesn't track the PBR family cleanly
/// (Cyberpunk is Metal but glows; FeudalJapan is Organic but doesn't).
fn style_is_luminous(style: ThemeArchetype) -> bool {
    use ThemeArchetype::*;
    matches!(
        style,
        Cyberpunk | AlienMonolithic | AlienOrganic | Fantasy | Solarpunk | SpaceOutpost
    )
}

/// A seeded material-finish kit. Cheap to recompute from the anchor;
/// holds the style finish family + continuous wear so each role method
/// bakes a consistent finish.
#[derive(Clone, Copy, Debug)]
pub struct MaterialKit {
    family: FinishFamily,
    luminous: bool,
    /// `[0, 1]` continuous wear from the anchor — drives grime + roughness.
    wear: f32,
    /// Bold finish register — glossier surfaces + stronger glow than the
    /// naturalistic register (see [`FinishRegister`]).
    bold: bool,
}

impl MaterialKit {
    pub fn for_did(did: &str) -> Self {
        Self::for_character(&AvatarCharacter::for_did(did))
    }

    pub fn for_seed(seed: u64) -> Self {
        Self::for_character(&AvatarCharacter::for_seed(seed))
    }

    /// Derive the finish kit from the shared avatar anchor.
    pub fn for_character(c: &AvatarCharacter) -> Self {
        Self {
            family: FinishFamily::for_style(c.style),
            luminous: style_is_luminous(c.style),
            wear: c.wear.clamp(0.0, 1.0),
            bold: matches!(c.finish, FinishRegister::Bold),
        }
    }

    /// Whether this avatar's accents are self-lit. Builders/parts use it to
    /// decide between [`Self::accent`] (which already honours it) and a
    /// matte treatment for a non-accent surface.
    pub fn emissive_accents(&self) -> bool {
        self.luminous
    }

    /// Main painted body panel — hull / chassis / envelope / shirt. Carries a
    /// generated surface texture so large panels read as brushed metal or
    /// woven fabric rather than flat paint.
    pub fn body(&self, color: [f32; 3]) -> SovereignMaterialSettings {
        let (metallic, roughness) = self.family.body_pbr();
        let mut m = self.finish(color, metallic, roughness);
        let base = m.base_color.0;
        m.uv_scale = Fp(1.5);
        m.texture = self.body_texture(base);
        // Knit / woven bodies (the Matte / Organic families that get a
        // fabric texture) must read close to matte: a specular highlight on
        // a shirt sparkles along the silhouette and visually inflates the
        // torso's barrel read (#730-M1, seen on 4 seeds — it amplifies the
        // #728 chest depth). Cap metallic and floor roughness, but leave
        // enough range that the Bold register still reads glossier than
        // Naturalistic. Metal / Clean families keep their brushed-panel gloss.
        if matches!(self.family, FinishFamily::Matte | FinishFamily::Organic) {
            m.metallic = Fp(m.metallic.0.min(0.06));
            m.roughness = Fp(m.roughness.0.max(0.78));
        }
        m
    }

    /// Generated surface texture for [`Self::body`], chosen by finish family:
    /// techy / enamel families get a brushed-metal panel, matte / organic
    /// ones a woven fabric. Toned to the (already grimed) base colour.
    fn body_texture(&self, base: [f32; 3]) -> SovereignTextureConfig {
        match self.family {
            FinishFamily::Metal | FinishFamily::Clean => metal_panel_tex(base, self.wear),
            FinishFamily::Matte | FinishFamily::Organic => fabric_tex(base, self.wear),
        }
    }

    /// Matte fabric / canvas — clothing, envelope canvas, awnings.
    pub fn cloth(&self, color: [f32; 3]) -> SovereignMaterialSettings {
        self.finish(color, 0.0, 0.85)
    }

    /// Structural metal — frames, struts, masts. Brushed-panel texture.
    pub fn metal(&self, color: [f32; 3]) -> SovereignMaterialSettings {
        let mut m = self.finish(color, 0.6, 0.4);
        let base = m.base_color.0;
        m.uv_scale = Fp(1.0);
        m.texture = metal_panel_tex(base, self.wear);
        m
    }

    /// Polished ornament metal — brass fittings, finials, buckles. Stays
    /// shinier than [`Self::metal`] and resists grime a little (kept bright
    /// even when worn).
    pub fn trim(&self, color: [f32; 3]) -> SovereignMaterialSettings {
        let mut m = self.finish(color, 0.75, 0.3);
        // Ornament metal is wiped/maintained — pull a little wear back out.
        m.roughness = Fp(m.roughness.0 * 0.85);
        m
    }

    /// The feature accent surface. Self-lit for luminous styles (neon trim,
    /// arcane glow), otherwise a slightly glossier body panel so the accent
    /// still reads as the highlight.
    pub fn accent(&self, color: [f32; 3]) -> SovereignMaterialSettings {
        if self.luminous {
            // Emissive doesn't grime — a glowing element stays bright. Bold
            // pushes the glow harder than the naturalistic register.
            SovereignMaterialSettings {
                base_color: Fp3(color),
                metallic: Fp(0.3),
                roughness: Fp(0.4),
                emission_color: Fp3(color),
                emission_strength: Fp(if self.bold { 8.0 } else { 4.5 }),
                ..Default::default()
            }
        } else {
            self.finish(color, 0.4, 0.45)
        }
    }

    /// A self-lit jewel / lamp regardless of style — finials, eyes, running
    /// lights. Always glows (unlike [`Self::accent`], which only glows for
    /// luminous styles).
    pub fn glow(&self, color: [f32; 3]) -> SovereignMaterialSettings {
        SovereignMaterialSettings {
            base_color: Fp3(color),
            metallic: Fp(0.4),
            roughness: Fp(0.4),
            emission_color: Fp3(color),
            emission_strength: Fp(5.0),
            ..Default::default()
        }
    }

    /// Glassy canopy / visor. Slightly dirtier (rougher) when worn.
    pub fn glass(&self, color: [f32; 3]) -> SovereignMaterialSettings {
        SovereignMaterialSettings {
            base_color: Fp3(color),
            metallic: Fp(0.9),
            roughness: Fp(0.08 + 0.12 * self.wear),
            ..Default::default()
        }
    }

    /// Organic skin — independent of style and wear (wear is equipment
    /// grime, not biology). Softer than cloth so faces catch the sun.
    pub fn skin(&self, color: [f32; 3]) -> SovereignMaterialSettings {
        SovereignMaterialSettings {
            base_color: Fp3(color),
            metallic: Fp(0.0),
            roughness: Fp(0.65),
            ..Default::default()
        }
    }

    /// Apply the wear grime + roughness bump to a base finish: a worn
    /// surface darkens, desaturates toward its own luma, and roughens.
    fn finish(&self, color: [f32; 3], metallic: f32, roughness: f32) -> SovereignMaterialSettings {
        let grimed = grime(color, self.wear);
        // Bold reads glossier (more metallic, smoother); Naturalistic softer
        // and more matte — applied before the wear roughening.
        let (metal_mul, rough_add) = if self.bold {
            (1.25, -0.08)
        } else {
            (0.85, 0.05)
        };
        SovereignMaterialSettings {
            base_color: Fp3(grimed),
            metallic: Fp((metallic * metal_mul * (1.0 - 0.3 * self.wear)).clamp(0.0, 1.0)),
            roughness: Fp((roughness + rough_add + 0.15 * self.wear).clamp(0.0, 1.0)),
            ..Default::default()
        }
    }
}

/// Brushed-metal panel texture toned to `base` (already grimed), rustier with
/// `wear`. Values mirror the catalogue's metal kit so they round-trip the
/// sanitiser unchanged.
fn metal_panel_tex(base: [f32; 3], wear: f32) -> SovereignTextureConfig {
    SovereignTextureConfig::Metal(SovereignMetalConfig {
        style: MetalStyle::Brushed,
        color_metal: Fp3(base),
        color_rust: Fp3([0.30, 0.18, 0.10]),
        roughness: Fp64(0.45),
        metallic: Fp(0.7),
        rust_level: Fp64((0.1 + 0.45 * wear as f64).min(0.9)),
        ..Default::default()
    })
}

/// Woven-fabric surface toned to `base` (warp) with a darker weft, fuzzier
/// with `wear`.
fn fabric_tex(base: [f32; 3], wear: f32) -> SovereignTextureConfig {
    SovereignTextureConfig::Fabric(SovereignFabricConfig {
        color_warp: Fp3(base),
        // Weft contrast softened 0.76→0.84 and fuzz cut 0.4→0.22 base so the
        // weave calms at silhouette edges without going so flat the knit
        // reads as bare skin — fuzz 0.15 erased the ribbing on one seed in
        // round 2 (#730-M). Wear still coarsens it toward the battered end.
        color_weft: Fp3(shade01(base, 0.84)),
        thread_count: Fp64(22.0),
        fuzz: Fp64((0.22 + 0.25 * wear as f64).min(0.9)),
        ..Default::default()
    })
}

/// Multiply a colour toward black, clamped to gamut so the result is
/// sanitiser-stable.
fn shade01(c: [f32; 3], f: f32) -> [f32; 3] {
    [
        (c[0] * f).clamp(0.0, 1.0),
        (c[1] * f).clamp(0.0, 1.0),
        (c[2] * f).clamp(0.0, 1.0),
    ]
}

/// Darken + desaturate a colour toward grime by `wear` (`0` = untouched).
/// Battered paint loses both brightness and saturation.
fn grime(color: [f32; 3], wear: f32) -> [f32; 3] {
    let w = wear.clamp(0.0, 1.0);
    let luma = 0.299 * color[0] + 0.587 * color[1] + 0.114 * color[2];
    let desat = 0.4 * w; // pull toward grey
    let darken = 1.0 - 0.35 * w; // overall dim
    [
        ((color[0] * (1.0 - desat) + luma * desat) * darken).clamp(0.0, 1.0),
        ((color[1] * (1.0 - desat) + luma * desat) * darken).clamp(0.0, 1.0),
        ((color[2] * (1.0 - desat) + luma * desat) * darken).clamp(0.0, 1.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = MaterialKit::for_did("did:plc:abc");
        let b = MaterialKit::for_did("did:plc:abc");
        assert_eq!(a.family, b.family);
        assert_eq!(a.luminous, b.luminous);
        assert_eq!(a.wear, b.wear);
    }

    #[test]
    fn every_style_classifies() {
        // The family + luminosity tables must be exhaustive over the themes.
        for style in ThemeArchetype::ALL {
            let mut c = AvatarCharacter::for_seed(1);
            c.style = style;
            let kit = MaterialKit::for_character(&c);
            let m = kit.body([0.5, 0.4, 0.3]);
            for ch in m.base_color.0 {
                assert!((0.0..=1.0).contains(&ch), "{style:?} body OOB");
            }
            assert!((0.0..=1.0).contains(&m.metallic.0));
            assert!((0.0..=1.0).contains(&m.roughness.0));
        }
    }

    #[test]
    fn luminous_styles_glow_their_accents() {
        let mut cy = AvatarCharacter::for_seed(2);
        cy.style = ThemeArchetype::Cyberpunk;
        let kit = MaterialKit::for_character(&cy);
        assert!(kit.emissive_accents());
        assert!(kit.accent([0.8, 0.1, 0.6]).emission_strength.0 > 0.0);

        let mut med = cy;
        med.style = ThemeArchetype::Medieval;
        let kit = MaterialKit::for_character(&med);
        assert!(!kit.emissive_accents());
        assert_eq!(kit.accent([0.4, 0.3, 0.2]).emission_strength.0, 0.0);
    }

    #[test]
    fn wear_darkens_and_roughens() {
        let mut pristine = AvatarCharacter::for_seed(4);
        pristine.style = ThemeArchetype::IndustrialPark;
        pristine.wear = 0.0;
        let mut battered = pristine;
        battered.wear = 1.0;
        let col = [0.6, 0.5, 0.4];
        let p = MaterialKit::for_character(&pristine).body(col);
        let b = MaterialKit::for_character(&battered).body(col);
        let luma = |c: Fp3| 0.299 * c.0[0] + 0.587 * c.0[1] + 0.114 * c.0[2];
        assert!(luma(b.base_color) < luma(p.base_color), "battered darker");
        assert!(b.roughness.0 > p.roughness.0, "battered rougher");
    }

    #[test]
    fn metal_style_is_glossier_than_matte_style() {
        let mut metal = AvatarCharacter::for_seed(6);
        metal.wear = 0.0;
        metal.style = ThemeArchetype::Cyberpunk;
        let mut matte = metal;
        matte.style = ThemeArchetype::Medieval;
        let m = MaterialKit::for_character(&metal).body([0.5, 0.5, 0.5]);
        let t = MaterialKit::for_character(&matte).body([0.5, 0.5, 0.5]);
        assert!(m.metallic.0 > t.metallic.0, "metal more metallic");
        assert!(m.roughness.0 < t.roughness.0, "metal smoother");
    }

    #[test]
    fn bold_finish_is_glossier_than_naturalistic() {
        // Same anchor, swap only the finish register: Bold reads more metallic
        // and smoother than Naturalistic on the same body surface.
        let mut bold = AvatarCharacter::for_seed(10);
        bold.style = ThemeArchetype::Medieval;
        bold.wear = 0.0;
        bold.finish = FinishRegister::Bold;
        let mut nat = bold;
        nat.finish = FinishRegister::Naturalistic;
        let b = MaterialKit::for_character(&bold).body([0.5, 0.4, 0.3]);
        let n = MaterialKit::for_character(&nat).body([0.5, 0.4, 0.3]);
        assert!(b.metallic.0 > n.metallic.0, "bold should be more metallic");
        assert!(b.roughness.0 < n.roughness.0, "bold should be smoother");
    }

    #[test]
    fn glow_always_emits_regardless_of_style() {
        let mut c = AvatarCharacter::for_seed(7);
        c.style = ThemeArchetype::Medieval; // non-luminous
        let kit = MaterialKit::for_character(&c);
        assert!(
            kit.glow([1.0, 0.9, 0.5]).emission_strength.0 > 0.0,
            "glow must emit even for a matte style"
        );
    }
}
