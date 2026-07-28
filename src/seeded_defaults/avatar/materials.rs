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

use bevy_symbios_texture::{fabric::WeaveKind, metal::MetalStyle};

use crate::pds::texture::{
    SovereignChitinConfig, SovereignEnamelConfig, SovereignFabricConfig, SovereignMaterialSettings,
    SovereignMetalConfig, SovereignTextureConfig,
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

    /// The weave a family's loose cloth is worked in. Cloth is where the
    /// families differ most by touch rather than by gloss, so this is the
    /// one place the weave kind is chosen rather than left plain.
    fn cloth_weave(self) -> WeaveKind {
        match self {
            // Technical fabric over a hard shell — flat and even.
            Self::Metal | Self::Clean => WeaveKind::Plain,
            // Workwear: canvas and denim carry a diagonal wale.
            Self::Matte => WeaveKind::Twill,
            // Robes and drapery, where the long floats catch the light.
            Self::Organic => WeaveKind::Satin,
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

/// Whether a style's "skin" is carapace rather than hide. Style-level, not
/// family-level: the Organic family also holds Fantasy and FeudalJapan, and
/// plating a fantasy avatar's face in insect chitin is a worse read than
/// leaving it flat.
fn style_is_chitinous(style: ThemeArchetype) -> bool {
    matches!(style, ThemeArchetype::AlienOrganic)
}

/// A seeded material-finish kit. Cheap to recompute from the anchor;
/// holds the style finish family + continuous wear so each role method
/// bakes a consistent finish.
#[derive(Clone, Copy, Debug)]
pub struct MaterialKit {
    family: FinishFamily,
    luminous: bool,
    /// Whether [`MaterialKit::skin`] is carapace rather than hide.
    chitinous: bool,
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
            chitinous: style_is_chitinous(c.style),
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

    /// Matte fabric / canvas — clothing, envelope canvas, awnings. Woven in
    /// the family's own weave, and coarser than [`Self::body`]: loose cloth
    /// hangs in bigger folds than a fitted panel, so the thread reads at a
    /// larger pitch.
    pub fn cloth(&self, color: [f32; 3]) -> SovereignMaterialSettings {
        let mut m = self.finish(color, 0.0, 0.85);
        let base = m.base_color.0;
        m.uv_scale = Fp(1.2);
        m.texture = SovereignTextureConfig::Fabric(SovereignFabricConfig {
            weave: self.family.cloth_weave(),
            color_warp: Fp3(base),
            color_weft: Fp3(shade01(base, 0.82)),
            // Coarser than the body's 22: a garment's weave is visible, a
            // fitted panel's is a texture you only notice up close.
            thread_count: Fp64(15.0),
            fuzz: Fp64((0.3 + 0.3 * self.wear as f64).min(0.9)),
            ..Default::default()
        });
        m
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
        let base = m.base_color.0;
        // Fittings are small — a buckle or finial is a few centimetres — so
        // the tile has to shrink with them or the peening mips to flat brass.
        m.uv_scale = Fp(9.0);
        m.texture = SovereignTextureConfig::Metal(SovereignMetalConfig {
            // Beaten, not brushed: ornament is worked by hand, and the
            // dimples catch light from every angle where brushing only
            // answers along one.
            style: MetalStyle::Hammered,
            scale: Fp64(7.0),
            color_metal: Fp3(base),
            color_rust: Fp3([0.34, 0.24, 0.10]),
            roughness: Fp64(0.3),
            metallic: Fp(0.85),
            // Maintained ornament barely tarnishes, even on a battered avatar.
            rust_level: Fp64((0.02 + 0.18 * self.wear as f64).min(0.35)),
            ..Default::default()
        });
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
            let mut m = self.finish(color, 0.4, 0.45);
            let base = m.base_color.0;
            m.uv_scale = Fp(2.0);
            m.texture = match self.family {
                // A sprayed accent stripe over a hard shell is enamel: the
                // fired coat's only feature is a fine orange-peel, which is
                // what separates it from the brushed panel underneath.
                FinishFamily::Metal | FinishFamily::Clean => {
                    SovereignTextureConfig::Enamel(SovereignEnamelConfig {
                        color: Fp3(base),
                        color_body: Fp3(shade01(base, 0.7)),
                        gloss_roughness: Fp(0.2),
                        // Old lacquer crazes; a fresh coat does not.
                        crackle: Fp(0.45 * self.wear),
                        ..Default::default()
                    })
                }
                // A sash or heraldic panel is cloth, not paint — so it takes
                // the same weave as the rest of the avatar's clothing.
                FinishFamily::Matte | FinishFamily::Organic => {
                    SovereignTextureConfig::Fabric(SovereignFabricConfig {
                        weave: self.family.cloth_weave(),
                        color_warp: Fp3(base),
                        color_weft: Fp3(shade01(base, 0.82)),
                        thread_count: Fp64(18.0),
                        fuzz: Fp64((0.25 + 0.25 * self.wear as f64).min(0.9)),
                        ..Default::default()
                    })
                }
            };
            m
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
    ///
    /// Deliberately untextured: glass reads by its reflection, and any
    /// surface pattern on a visor competes with the eyes behind it. Same for
    /// [`Self::glow`], whose jewels and running lights are small enough that
    /// a tiled surface would arrive as noise rather than detail.
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
        let mut m = SovereignMaterialSettings {
            base_color: Fp3(color),
            metallic: Fp(0.0),
            roughness: Fp(0.65),
            ..Default::default()
        };
        if self.chitinous {
            // Carapace, not hide. The generator lays six plates across a
            // tile, so 2 tiles/m puts a plate at roughly a hand's width and
            // a forearm carries a few rather than one flat shell. The
            // default 6 tiles/m would be a 2.8 cm scale — correct for an
            // insect, invisible on an avatar.
            m.metallic = Fp(0.35);
            m.roughness = Fp(0.35);
            m.uv_scale = Fp(2.0);
            m.texture = SovereignTextureConfig::Chitin(SovereignChitinConfig {
                color: Fp3(color),
                color_deep: Fp3(shade01(color, 0.35)),
                gloss_roughness: Fp(0.3),
                metallic: Fp(0.35),
                // The default hairline suture is drawn for a surface seen
                // close up; on a limb a few metres away it mips away
                // entirely, taking the read of separate plates with it.
                seam_width: Fp64(0.02),
                seam_depth: Fp(0.9),
                plate_relief: Fp64(0.7),
                iridescence: Fp(if self.bold { 0.35 } else { 0.18 }),
                ..Default::default()
            });
        }
        m
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

    /// Every textured role must survive the sanitiser untouched. A kit that
    /// authors a value outside the sanitiser's envelope still *renders* —
    /// the sanitiser quietly rewrites it — but the record then differs from
    /// what the kit produced, so a round-trip through the PDS mutates the
    /// avatar. Only a fixpoint check catches that.
    #[test]
    fn every_role_is_a_sanitiser_fixpoint() {
        use crate::pds::sanitize::Sanitize;

        for style in ThemeArchetype::ALL {
            for wear in [0.0f32, 0.5, 1.0] {
                for finish in [FinishRegister::Bold, FinishRegister::Naturalistic] {
                    let mut c = AvatarCharacter::for_seed(11);
                    c.style = style;
                    c.wear = wear;
                    c.finish = finish;
                    let kit = MaterialKit::for_character(&c);
                    let col = [0.55, 0.42, 0.30];
                    let roles: [(&str, SovereignMaterialSettings); 7] = [
                        ("body", kit.body(col)),
                        ("cloth", kit.cloth(col)),
                        ("metal", kit.metal(col)),
                        ("trim", kit.trim(col)),
                        ("accent", kit.accent(col)),
                        ("glass", kit.glass(col)),
                        ("skin", kit.skin(col)),
                    ];
                    for (name, m) in roles {
                        let mut sanitised = m.clone();
                        sanitised.sanitize();
                        assert_eq!(
                            m, sanitised,
                            "{style:?}/{finish:?}/wear={wear} {name} is rewritten by the sanitiser"
                        );
                    }
                }
            }
        }
    }

    /// The point of #997: these roles used to be flat colour.
    #[test]
    fn dressed_roles_actually_carry_a_texture() {
        for style in ThemeArchetype::ALL {
            let mut c = AvatarCharacter::for_seed(12);
            c.style = style;
            let kit = MaterialKit::for_character(&c);
            let col = [0.5, 0.45, 0.4];
            assert!(
                !matches!(kit.cloth(col).texture, SovereignTextureConfig::None),
                "{style:?} cloth is untextured"
            );
            assert!(
                !matches!(kit.trim(col).texture, SovereignTextureConfig::None),
                "{style:?} trim is untextured"
            );
            // A luminous accent is emissive rather than textured — the glow
            // is the feature, so it keeps its flat self-lit treatment.
            if !kit.emissive_accents() {
                assert!(
                    !matches!(kit.accent(col).texture, SovereignTextureConfig::None),
                    "{style:?} non-luminous accent is untextured"
                );
            }
        }
    }

    #[test]
    fn only_alien_organic_wears_chitin() {
        for style in ThemeArchetype::ALL {
            let mut c = AvatarCharacter::for_seed(13);
            c.style = style;
            let kit = MaterialKit::for_character(&c);
            let is_chitin = matches!(
                kit.skin([0.5, 0.4, 0.35]).texture,
                SovereignTextureConfig::Chitin(_)
            );
            assert_eq!(
                is_chitin,
                style == ThemeArchetype::AlienOrganic,
                "{style:?} skin chitin mismatch"
            );
        }
    }

    /// Cloth is where the families are meant to differ by hand rather than
    /// gloss, so the weaves must not all collapse to one kind.
    #[test]
    fn families_weave_their_cloth_differently() {
        use std::collections::BTreeSet;
        let weaves: BTreeSet<_> = ThemeArchetype::ALL
            .iter()
            .map(|&style| {
                let mut c = AvatarCharacter::for_seed(14);
                c.style = style;
                match MaterialKit::for_character(&c).cloth([0.5; 3]).texture {
                    SovereignTextureConfig::Fabric(f) => format!("{:?}", f.weave),
                    other => panic!("{style:?} cloth is not fabric: {other:?}"),
                }
            })
            .collect();
        assert!(
            weaves.len() >= 3,
            "cloth weave collapsed to {weaves:?} — the families read alike"
        );
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
