//! Sovereign (DAG-CBOR safe) mirrors of every `bevy_symbios_texture`
//! generator configuration, along with the unified [`SovereignTextureConfig`]
//! tagged-union enum and [`SovereignMaterialSettings`] PBR wrapper.
//!
//! Individual config structs are generated via the `define_sovereign_texture_cfg!`
//! macro so adding a new generator is a single declarative block — each field
//! just names its wire kind (`fp`, `fp3`, `fp64`, `u32`, `usize`, `bool`,
//! `enum(Ty)`, `nested(SovTy)`) and default.

use super::types::{Fp, Fp3, Fp64};
use serde::{Deserialize, Serialize};

/// Procedural "ground" texture parameters (grass / dirt / snow layers).
/// Mirrors `bevy_symbios_texture::ground::GroundConfig` with fixed-point wrappers.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignGroundConfig {
    pub seed: u32,
    pub macro_scale: Fp64,
    pub macro_octaves: u32,
    pub micro_scale: Fp64,
    pub micro_octaves: u32,
    pub micro_weight: Fp64,
    pub color_dry: Fp3,
    pub color_moist: Fp3,
    pub normal_strength: Fp,
}

/// Procedural "rock" texture parameters. Mirrors
/// `bevy_symbios_texture::rock::RockConfig`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignRockConfig {
    pub seed: u32,
    pub scale: Fp64,
    pub octaves: u32,
    pub attenuation: Fp64,
    pub color_light: Fp3,
    pub color_dark: Fp3,
    pub normal_strength: Fp,
}

impl Default for SovereignGroundConfig {
    fn default() -> Self {
        Self {
            seed: 13,
            macro_scale: Fp64(2.0),
            macro_octaves: 5,
            micro_scale: Fp64(8.0),
            micro_octaves: 4,
            micro_weight: Fp64(0.35),
            color_dry: Fp3([0.52, 0.40, 0.26]),
            color_moist: Fp3([0.28, 0.20, 0.12]),
            normal_strength: Fp(2.0),
        }
    }
}

impl SovereignGroundConfig {
    pub fn to_native(&self) -> bevy_symbios_texture::ground::GroundConfig {
        bevy_symbios_texture::ground::GroundConfig {
            seed: self.seed,
            macro_scale: self.macro_scale.0,
            macro_octaves: self.macro_octaves as usize,
            micro_scale: self.micro_scale.0,
            micro_octaves: self.micro_octaves as usize,
            micro_weight: self.micro_weight.0,
            color_dry: self.color_dry.0,
            color_moist: self.color_moist.0,
            normal_strength: self.normal_strength.0,
        }
    }

    pub fn from_native(n: &bevy_symbios_texture::ground::GroundConfig) -> Self {
        Self {
            seed: n.seed,
            macro_scale: Fp64(n.macro_scale),
            macro_octaves: n.macro_octaves as u32,
            micro_scale: Fp64(n.micro_scale),
            micro_octaves: n.micro_octaves as u32,
            micro_weight: Fp64(n.micro_weight),
            color_dry: Fp3(n.color_dry),
            color_moist: Fp3(n.color_moist),
            normal_strength: Fp(n.normal_strength),
        }
    }
}

impl Default for SovereignRockConfig {
    fn default() -> Self {
        Self {
            seed: 7,
            scale: Fp64(3.0),
            octaves: 8,
            attenuation: Fp64(2.0),
            color_light: Fp3([0.37, 0.42, 0.36]),
            color_dark: Fp3([0.22, 0.20, 0.18]),
            normal_strength: Fp(4.0),
        }
    }
}

impl SovereignRockConfig {
    pub fn to_native(&self) -> bevy_symbios_texture::rock::RockConfig {
        bevy_symbios_texture::rock::RockConfig {
            seed: self.seed,
            scale: self.scale.0,
            octaves: self.octaves as usize,
            attenuation: self.attenuation.0,
            color_light: self.color_light.0,
            color_dark: self.color_dark.0,
            normal_strength: self.normal_strength.0,
        }
    }

    pub fn from_native(n: &bevy_symbios_texture::rock::RockConfig) -> Self {
        Self {
            seed: n.seed,
            scale: Fp64(n.scale),
            octaves: n.octaves as u32,
            attenuation: Fp64(n.attenuation),
            color_light: Fp3(n.color_light),
            color_dark: Fp3(n.color_dark),
            normal_strength: Fp(n.normal_strength),
        }
    }
}

/// Declarative macro that generates a `SovereignXxxConfig` mirror of an
/// upstream `bevy_symbios_texture` generator config, along with its
/// `Default`, `to_native()`, and `from_native()` impls.
///
/// Each field is declared by its *kind* (`fp`, `fp3`, `fp64`, `u32`,
/// `usize`, `bool`, `enum(Ty)`, `nested(SovTy)`) followed by `: name = default`.
/// The kind selects the wire-format wrapper and the conversion rule.
macro_rules! define_sovereign_texture_cfg {
    (
        $sov:ident => $native:path {
            $( $kind:ident $( ( $sub:ty ) )? : $field:ident = $default:expr ),+ $(,)?
        }
    ) => {
        #[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
        pub struct $sov {
            $( pub $field: define_sovereign_texture_cfg!(@ty $kind $(($sub))?), )+
        }

        impl Default for $sov {
            fn default() -> Self {
                Self {
                    $( $field: define_sovereign_texture_cfg!(@default $kind $(($sub))?, $default), )+
                }
            }
        }

        impl $sov {
            pub fn to_native(&self) -> $native {
                $native {
                    $( $field: define_sovereign_texture_cfg!(@to_native $kind $(($sub))?, self.$field), )+
                }
            }

            pub fn from_native(native: &$native) -> Self {
                Self {
                    $( $field: define_sovereign_texture_cfg!(@from_native $kind $(($sub))?, native.$field), )+
                }
            }
        }
    };

    (@ty fp)          => { Fp };
    (@ty fp3)         => { Fp3 };
    (@ty fp64)        => { Fp64 };
    (@ty u32)         => { u32 };
    (@ty usize)       => { u32 };
    (@ty bool)        => { bool };
    (@ty enum ($e:ty))   => { $e };
    (@ty nested ($t:ty)) => { $t };

    (@default fp, $v:expr)            => { Fp($v) };
    (@default fp3, $v:expr)           => { Fp3($v) };
    (@default fp64, $v:expr)          => { Fp64($v) };
    (@default u32, $v:expr)           => { $v };
    (@default usize, $v:expr)         => { $v };
    (@default bool, $v:expr)          => { $v };
    (@default enum ($e:ty), $v:expr)    => { $v };
    (@default nested ($t:ty), $v:expr)  => { $v };

    (@to_native fp, $v:expr)          => { $v.0 };
    (@to_native fp3, $v:expr)         => { $v.0 };
    (@to_native fp64, $v:expr)        => { $v.0 };
    (@to_native u32, $v:expr)         => { $v };
    (@to_native usize, $v:expr)       => { $v as usize };
    (@to_native bool, $v:expr)        => { $v };
    (@to_native enum ($e:ty), $v:expr)   => { $v.clone() };
    (@to_native nested ($t:ty), $v:expr) => { $v.to_native() };

    (@from_native fp, $v:expr)        => { Fp($v) };
    (@from_native fp3, $v:expr)       => { Fp3($v) };
    (@from_native fp64, $v:expr)      => { Fp64($v) };
    (@from_native u32, $v:expr)       => { $v };
    (@from_native usize, $v:expr)     => { $v as u32 };
    (@from_native bool, $v:expr)      => { $v };
    (@from_native enum ($e:ty), $v:expr)   => { ($v).clone() };
    (@from_native nested ($t:ty), $v:expr) => { <$t>::from_native(&$v) };
}

// --- Foliage cards ---------------------------------------------------------

define_sovereign_texture_cfg!(SovereignLeafConfig => bevy_symbios_texture::leaf::LeafConfig {
    u32  : seed = 0,
    fp3  : color_base = [0.12, 0.19, 0.11],
    fp3  : color_edge = [0.35, 0.28, 0.05],
    fp64 : serration_strength = 0.12,
    fp64 : vein_angle = 2.5,
    fp64 : micro_detail = 0.3,
    fp   : normal_strength = 1.0,
    fp64 : lobe_count = 4.0,
    fp64 : lobe_depth = 0.23,
    fp64 : lobe_sharpness = 1.0,
    fp64 : petiole_length = 0.12,
    fp64 : petiole_width = 0.022,
    fp64 : midrib_width = 0.12,
    fp64 : vein_count = 6.0,
    fp64 : venule_strength = 0.50,
});

define_sovereign_texture_cfg!(SovereignTwigConfig => bevy_symbios_texture::twig::TwigConfig {
    nested(SovereignLeafConfig) : leaf = SovereignLeafConfig::default(),
    fp3   : stem_color = [0.18, 0.08, 0.06],
    fp64  : stem_half_width = 0.021,
    usize : leaf_pairs = 4,
    fp64  : leaf_angle = std::f64::consts::FRAC_PI_2 - 0.35,
    fp64  : leaf_scale = 0.38,
    fp64  : stem_curve = 0.015,
    bool  : sympodial = true,
});

define_sovereign_texture_cfg!(SovereignBarkConfig => bevy_symbios_texture::bark::BarkConfig {
    u32   : seed = 42,
    fp64  : scale = 2.0,
    usize : octaves = 6,
    fp64  : warp_u = 0.15,
    fp64  : warp_v = 0.55,
    fp3   : color_light = [0.45, 0.28, 0.14],
    fp3   : color_dark = [0.09, 0.05, 0.03],
    fp    : normal_strength = 3.0,
    fp64  : furrow_multiplier = 0.78,
    fp64  : furrow_scale_u = 2.0,
    fp64  : furrow_scale_v = 0.48,
    fp64  : furrow_shape = 2.0,
});

define_sovereign_texture_cfg!(SovereignWindowConfig => bevy_symbios_texture::window::WindowConfig {
    u32   : seed = 42,
    fp64  : frame_width = 0.08,
    usize : panes_x = 2,
    usize : panes_y = 3,
    fp64  : mullion_thickness = 0.025,
    fp64  : corner_radius = 0.02,
    fp64  : glass_opacity = 0.30,
    fp64  : grime_level = 0.15,
    fp3   : color_frame = [0.85, 0.82, 0.78],
    fp    : normal_strength = 3.0,
});

define_sovereign_texture_cfg!(SovereignStainedGlassConfig => bevy_symbios_texture::stained_glass::StainedGlassConfig {
    u32   : seed = 63,
    usize : cell_count = 12,
    fp64  : lead_width = 0.05,
    fp    : saturation = 0.85,
    fp64  : glass_roughness = 0.06,
    fp64  : grime_level = 0.12,
    fp    : normal_strength = 2.5,
});

define_sovereign_texture_cfg!(SovereignIronGrilleConfig => bevy_symbios_texture::iron_grille::IronGrilleConfig {
    u32   : seed = 71,
    usize : bars_x = 4,
    usize : bars_y = 6,
    fp64  : bar_width = 0.04,
    bool  : round_bars = true,
    fp64  : rust_level = 0.30,
    fp3   : color_iron = [0.14, 0.13, 0.13],
    fp3   : color_rust = [0.42, 0.22, 0.08],
    fp    : normal_strength = 3.5,
});

// --- Tileable surfaces -----------------------------------------------------

define_sovereign_texture_cfg!(SovereignBrickConfig => bevy_symbios_texture::brick::BrickConfig {
    u32  : seed = 42,
    fp64 : scale = 4.0,
    fp64 : row_offset = 0.5,
    fp64 : aspect_ratio = 2.0,
    fp64 : mortar_size = 0.05,
    fp64 : bevel = 0.5,
    fp64 : cell_variance = 0.15,
    fp64 : roughness = 0.5,
    fp3  : color_brick = [0.56, 0.28, 0.18],
    fp3  : color_mortar = [0.76, 0.73, 0.67],
    fp   : normal_strength = 4.0,
});

define_sovereign_texture_cfg!(SovereignPlankConfig => bevy_symbios_texture::plank::PlankConfig {
    u32  : seed = 42,
    fp64 : plank_count = 5.0,
    fp64 : grain_scale = 12.0,
    fp64 : joint_width = 0.06,
    fp64 : stagger = 0.5,
    fp64 : knot_density = 0.25,
    fp64 : grain_warp = 0.35,
    fp3  : color_wood_light = [0.72, 0.52, 0.30],
    fp3  : color_wood_dark = [0.42, 0.26, 0.12],
    fp   : normal_strength = 2.5,
});

define_sovereign_texture_cfg!(SovereignShingleConfig => bevy_symbios_texture::shingle::ShingleConfig {
    u32  : seed = 42,
    fp64 : scale = 5.0,
    fp64 : shape_profile = 0.5,
    fp64 : overlap = 0.45,
    fp64 : stagger = 0.5,
    fp64 : moss_level = 0.18,
    fp3  : color_tile = [0.40, 0.25, 0.18],
    fp3  : color_grout = [0.18, 0.14, 0.12],
    fp   : normal_strength = 5.0,
});

define_sovereign_texture_cfg!(SovereignStuccoConfig => bevy_symbios_texture::stucco::StuccoConfig {
    u32   : seed = 13,
    fp64  : scale = 8.0,
    usize : octaves = 6,
    fp64  : roughness = 0.35,
    fp3   : color_base = [0.92, 0.89, 0.84],
    fp3   : color_shadow = [0.72, 0.70, 0.66],
    fp    : normal_strength = 2.0,
});

define_sovereign_texture_cfg!(SovereignConcreteConfig => bevy_symbios_texture::concrete::ConcreteConfig {
    u32   : seed = 17,
    fp64  : scale = 5.0,
    usize : octaves = 5,
    fp64  : roughness = 0.45,
    fp64  : formwork_lines = 4.0,
    fp64  : formwork_depth = 0.12,
    fp64  : pit_density = 0.08,
    fp3   : color_base = [0.55, 0.54, 0.52],
    fp3   : color_pit = [0.35, 0.34, 0.33],
    fp    : normal_strength = 2.5,
});

define_sovereign_texture_cfg!(SovereignMetalConfig => bevy_symbios_texture::metal::MetalConfig {
    u32  : seed = 31,
    enum(bevy_symbios_texture::metal::MetalStyle) : style = bevy_symbios_texture::metal::MetalStyle::Brushed,
    fp64 : scale = 6.0,
    fp64 : seam_count = 6.0,
    fp64 : seam_sharpness = 2.5,
    fp64 : brush_stretch = 8.0,
    fp64 : roughness = 0.25,
    fp   : metallic = 0.85,
    fp64 : rust_level = 0.15,
    fp3  : color_metal = [0.42, 0.44, 0.47],
    fp3  : color_rust = [0.42, 0.24, 0.12],
    fp   : normal_strength = 3.0,
});

define_sovereign_texture_cfg!(SovereignPaversConfig => bevy_symbios_texture::pavers::PaversConfig {
    u32  : seed = 23,
    fp64 : scale = 5.0,
    fp64 : aspect_ratio = 1.0,
    fp64 : grout_width = 0.08,
    fp64 : bevel = 0.5,
    fp64 : cell_variance = 0.10,
    fp64 : roughness = 0.30,
    fp3  : color_stone = [0.48, 0.44, 0.40],
    fp3  : color_grout = [0.28, 0.27, 0.26],
    enum(bevy_symbios_texture::pavers::PaversLayout) : layout = bevy_symbios_texture::pavers::PaversLayout::Square,
    fp   : normal_strength = 3.5,
});

define_sovereign_texture_cfg!(SovereignAshlarConfig => bevy_symbios_texture::ashlar::AshlarConfig {
    u32   : seed = 13,
    usize : rows = 4,
    usize : cols = 4,
    fp64  : mortar_size = 0.04,
    fp64  : bevel = 0.4,
    fp64  : cell_variance = 0.18,
    fp64  : chisel_depth = 0.4,
    fp64  : roughness = 0.45,
    fp3   : color_stone = [0.52, 0.50, 0.47],
    fp3   : color_mortar = [0.72, 0.70, 0.65],
    fp    : normal_strength = 4.5,
});

define_sovereign_texture_cfg!(SovereignCobblestoneConfig => bevy_symbios_texture::cobblestone::CobblestoneConfig {
    u32  : seed = 7,
    fp64 : scale = 6.0,
    fp64 : gap_width = 0.12,
    fp64 : cell_variance = 0.20,
    fp64 : roundness = 1.2,
    fp3  : color_stone = [0.46, 0.43, 0.40],
    fp3  : color_mud = [0.22, 0.18, 0.14],
    fp   : normal_strength = 5.0,
});

define_sovereign_texture_cfg!(SovereignThatchConfig => bevy_symbios_texture::thatch::ThatchConfig {
    u32  : seed = 19,
    fp64 : density = 12.0,
    fp64 : anisotropy = 8.0,
    fp64 : warp_strength = 0.15,
    fp64 : layer_count = 8.0,
    fp64 : layer_shadow = 0.55,
    fp3  : color_straw = [0.62, 0.54, 0.28],
    fp3  : color_shadow = [0.22, 0.17, 0.09],
    fp   : normal_strength = 3.5,
});

define_sovereign_texture_cfg!(SovereignMarbleConfig => bevy_symbios_texture::marble::MarbleConfig {
    u32   : seed = 55,
    fp64  : scale = 3.0,
    usize : octaves = 5,
    fp64  : warp_strength = 0.6,
    fp64  : vein_frequency = 3.0,
    fp64  : vein_sharpness = 2.0,
    fp64  : roughness = 0.08,
    fp3   : color_base = [0.92, 0.90, 0.87],
    fp3   : color_vein = [0.42, 0.38, 0.34],
    fp    : normal_strength = 1.5,
});

define_sovereign_texture_cfg!(SovereignCorrugatedConfig => bevy_symbios_texture::corrugated::CorrugatedConfig {
    u32  : seed = 31,
    fp64 : ridges = 8.0,
    fp64 : ridge_depth = 1.0,
    fp64 : roughness = 0.35,
    fp64 : rust_level = 0.25,
    fp   : metallic = 0.85,
    fp3  : color_metal = [0.72, 0.74, 0.76],
    fp3  : color_rust = [0.55, 0.30, 0.12],
    fp   : normal_strength = 4.0,
});

define_sovereign_texture_cfg!(SovereignAsphaltConfig => bevy_symbios_texture::asphalt::AsphaltConfig {
    u32  : seed = 88,
    fp64 : scale = 4.0,
    fp64 : aggregate_density = 0.22,
    fp64 : aggregate_scale = 16.0,
    fp64 : roughness = 0.90,
    fp64 : stain_level = 0.25,
    fp3  : color_base = [0.06, 0.06, 0.07],
    fp3  : color_aggregate = [0.35, 0.33, 0.30],
    fp   : normal_strength = 2.5,
});

define_sovereign_texture_cfg!(SovereignWainscotingConfig => bevy_symbios_texture::wainscoting::WainscotingConfig {
    u32   : seed = 37,
    usize : panels_x = 1,
    usize : panels_y = 2,
    fp64  : frame_width = 0.20,
    fp64  : panel_inset = 0.06,
    fp64  : grain_scale = 10.0,
    fp64  : grain_warp = 0.30,
    fp3   : color_wood_light = [0.65, 0.44, 0.20],
    fp3   : color_wood_dark = [0.28, 0.16, 0.07],
    fp    : normal_strength = 4.0,
});

define_sovereign_texture_cfg!(SovereignEncausticConfig => bevy_symbios_texture::encaustic::EncausticConfig {
    u32  : seed = 47,
    fp64 : scale = 5.0,
    enum(bevy_symbios_texture::encaustic::EncausticPattern) : pattern = bevy_symbios_texture::encaustic::EncausticPattern::Octagon,
    fp64 : grout_width = 0.06,
    fp64 : glaze_roughness = 0.04,
    fp3  : color_a = [0.72, 0.38, 0.22],
    fp3  : color_b = [0.22, 0.35, 0.65],
    fp3  : color_grout = [0.82, 0.80, 0.75],
    fp   : normal_strength = 3.0,
});

/// Internally-tagged enum carrying the full configuration of any supported
/// `bevy_symbios_texture` generator. Serialises with a `$type` discriminant
/// so newer variants round-trip safely through older clients via
/// `#[serde(other)] Unknown`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(tag = "$type")]
pub enum SovereignTextureConfig {
    #[default]
    None,
    Leaf(SovereignLeafConfig),
    Twig(SovereignTwigConfig),
    Bark(SovereignBarkConfig),
    Window(SovereignWindowConfig),
    StainedGlass(SovereignStainedGlassConfig),
    IronGrille(SovereignIronGrilleConfig),
    Ground(SovereignGroundConfig),
    Rock(SovereignRockConfig),
    Brick(SovereignBrickConfig),
    Plank(SovereignPlankConfig),
    Shingle(SovereignShingleConfig),
    Stucco(SovereignStuccoConfig),
    Concrete(SovereignConcreteConfig),
    Metal(SovereignMetalConfig),
    Pavers(SovereignPaversConfig),
    Ashlar(SovereignAshlarConfig),
    Cobblestone(SovereignCobblestoneConfig),
    Thatch(SovereignThatchConfig),
    Marble(SovereignMarbleConfig),
    Corrugated(SovereignCorrugatedConfig),
    Asphalt(SovereignAsphaltConfig),
    Wainscoting(SovereignWainscotingConfig),
    Encaustic(SovereignEncausticConfig),
    #[serde(other)]
    Unknown,
}

impl SovereignTextureConfig {
    /// Human-readable variant name for UI combo boxes.
    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Leaf(_) => "Leaf",
            Self::Twig(_) => "Twig",
            Self::Bark(_) => "Bark",
            Self::Window(_) => "Window",
            Self::StainedGlass(_) => "Stained Glass",
            Self::IronGrille(_) => "Iron Grille",
            Self::Ground(_) => "Ground",
            Self::Rock(_) => "Rock",
            Self::Brick(_) => "Brick",
            Self::Plank(_) => "Plank",
            Self::Shingle(_) => "Shingle",
            Self::Stucco(_) => "Stucco",
            Self::Concrete(_) => "Concrete",
            Self::Metal(_) => "Metal",
            Self::Pavers(_) => "Pavers",
            Self::Ashlar(_) => "Ashlar",
            Self::Cobblestone(_) => "Cobblestone",
            Self::Thatch(_) => "Thatch",
            Self::Marble(_) => "Marble",
            Self::Corrugated(_) => "Corrugated",
            Self::Asphalt(_) => "Asphalt",
            Self::Wainscoting(_) => "Wainscoting",
            Self::Encaustic(_) => "Encaustic",
            Self::Unknown => "Unknown",
        }
    }

    /// Returns `(alpha_mode, double_sided, cull_mode, is_card)` governing how
    /// the generated `StandardMaterial` and its upload path are configured.
    /// Card-style textures use clamp-to-edge sampling and alpha masking; all
    /// others are treated as opaque repeat-tiling surfaces.
    pub fn render_properties(
        &self,
    ) -> (
        bevy::prelude::AlphaMode,
        bool,
        Option<bevy::render::render_resource::Face>,
        bool,
    ) {
        use bevy::prelude::AlphaMode;
        use bevy::render::render_resource::Face;
        match self {
            Self::Leaf(_)
            | Self::Twig(_)
            | Self::Window(_)
            | Self::StainedGlass(_)
            | Self::IronGrille(_) => (AlphaMode::Mask(0.5), true, None, true),
            _ => (AlphaMode::Opaque, false, Some(Face::Back), false),
        }
    }
}

/// Per-slot material settings for an L-system generator — mirrors
/// `bevy_symbios::materials::MaterialSettings` with DAG-CBOR-safe numeric
/// fields. The embedded [`SovereignTextureConfig`] carries the full config
/// for whichever `bevy_symbios_texture` generator drives this slot (if any).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignMaterialSettings {
    pub base_color: Fp3,
    pub emission_color: Fp3,
    pub emission_strength: Fp,
    pub roughness: Fp,
    pub metallic: Fp,
    #[serde(default = "default_uv_scale")]
    pub uv_scale: Fp,
    #[serde(default)]
    pub texture: SovereignTextureConfig,
}

fn default_uv_scale() -> Fp {
    Fp(1.0)
}

impl Default for SovereignMaterialSettings {
    fn default() -> Self {
        Self {
            base_color: Fp3([0.6, 0.4, 0.2]),
            emission_color: Fp3([0.0, 0.0, 0.0]),
            emission_strength: Fp(0.0),
            roughness: Fp(0.5),
            metallic: Fp(0.0),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::None,
        }
    }
}
