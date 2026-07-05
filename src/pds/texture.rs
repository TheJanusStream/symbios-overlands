//! Sovereign (DAG-CBOR safe) mirrors of every `bevy_symbios_texture`
//! generator configuration, along with the unified [`SovereignTextureConfig`]
//! tagged-union enum and [`SovereignMaterialSettings`] PBR wrapper.
//!
//! Most config structs are generated via the `define_sovereign_texture_cfg!`
//! macro so adding a new generator is a single declarative block — each field
//! just names its wire kind (`fp`, `fp3`, `fp64`, `u32`, `usize`, `bool`,
//! `enum(Ty)`, `nested(SovTy)`) and default. [`SovereignGroundConfig`] and
//! [`SovereignRockConfig`] are the two hand-rolled predecessors of the macro.

use super::types::{Fp, Fp3, Fp64};
use serde::{Deserialize, Serialize};

/// Procedural "ground" texture parameters (grass / dirt / snow layers).
/// Mirrors `bevy_symbios_texture::ground::GroundConfig` with fixed-point wrappers.
///
/// Default-eliding wire format (#695), like the macro-generated mirrors.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
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

crate::pds::serde_util::impl_default_eliding_serialize!(SovereignGroundConfig {
    seed,
    macro_scale,
    macro_octaves,
    micro_scale,
    micro_octaves,
    micro_weight,
    color_dry,
    color_moist,
    normal_strength,
});

/// Procedural "rock" texture parameters. Mirrors
/// `bevy_symbios_texture::rock::RockConfig`.
///
/// Default-eliding wire format (#695), like the macro-generated mirrors.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SovereignRockConfig {
    pub seed: u32,
    pub scale: Fp64,
    pub octaves: u32,
    pub attenuation: Fp64,
    pub color_light: Fp3,
    pub color_dark: Fp3,
    pub normal_strength: Fp,
}

crate::pds::serde_util::impl_default_eliding_serialize!(SovereignRockConfig {
    seed,
    scale,
    octaves,
    attenuation,
    color_light,
    color_dark,
    normal_strength,
});

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
        // Default-eliding wire format (#695): only fields differing from
        // the declared defaults are written; the container
        // `#[serde(default)]` restores them on read. A default-valued
        // texture config collapses to `{}` on the wire.
        #[derive(Deserialize, Clone, Debug, PartialEq)]
        #[serde(default)]
        pub struct $sov {
            $( pub $field: define_sovereign_texture_cfg!(@ty $kind $(($sub))?), )+
        }

        crate::pds::serde_util::impl_default_eliding_serialize!($sov {
            $( $field ),+
        });

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
    usize : warp_octaves = 3,
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
    usize : warp_octaves = 3,
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

// --- Particle sprite cards -------------------------------------------------
// Alpha-silhouette billboard sheets. `variant_rows`/`variant_cols` bake an
// N×M atlas of per-cell-seeded variants; the particle system's RandomFrame
// mode draws one cell per particle for per-particle shape variety. Every
// count-shaped field is clamped by the upstream generator at bake time, and
// re-clamped at the record boundary in `sanitize/material.rs`.

define_sovereign_texture_cfg!(SovereignSoftDiscConfig => bevy_symbios_texture::soft_disc::SoftDiscConfig {
    u32   : seed = 0,
    usize : variant_rows = 1,
    usize : variant_cols = 1,
    fp3   : color_core = [1.0, 0.98, 0.9],
    fp3   : color_halo = [1.0, 0.72, 0.25],
    fp64  : core_radius = 0.15,
    fp64  : falloff = 2.5,
    fp64  : ellipticity = 0.0,
    fp64  : scale_jitter = 0.15,
    fp    : normal_strength = 1.0,
});

define_sovereign_texture_cfg!(SovereignSparkConfig => bevy_symbios_texture::spark::SparkConfig {
    u32   : seed = 0,
    usize : variant_rows = 2,
    usize : variant_cols = 2,
    usize : points = 4,
    fp3   : color_core = [1.0, 0.95, 0.8],
    fp3   : color_tip = [1.0, 0.45, 0.1],
    fp64  : core_radius = 0.12,
    fp64  : arm_sharpness = 3.0,
    fp64  : falloff = 1.8,
    fp64  : length_jitter = 0.3,
    fp    : normal_strength = 1.0,
});

define_sovereign_texture_cfg!(SovereignSnowflakeConfig => bevy_symbios_texture::snowflake::SnowflakeConfig {
    u32   : seed = 0,
    usize : variant_rows = 2,
    usize : variant_cols = 2,
    usize : arms = 6,
    fp3   : color = [0.92, 0.96, 1.0],
    fp64  : core_radius = 0.12,
    fp64  : arm_width = 0.045,
    usize : branch_pairs = 3,
    fp64  : branch_angle = 1.05,
    fp64  : branch_scale = 0.45,
    fp64  : softness = 0.02,
    fp    : normal_strength = 1.5,
});

define_sovereign_texture_cfg!(SovereignPuffConfig => bevy_symbios_texture::puff::PuffConfig {
    u32   : seed = 0,
    usize : variant_rows = 2,
    usize : variant_cols = 2,
    fp3   : color_base = [0.86, 0.86, 0.9],
    fp3   : color_shadow = [0.52, 0.52, 0.58],
    fp64  : noise_scale = 3.0,
    usize : octaves = 4,
    fp64  : warp = 0.45,
    fp64  : density = 0.9,
    fp64  : edge_falloff = 2.0,
    fp64  : contrast = 1.3,
    fp    : normal_strength = 1.0,
});

define_sovereign_texture_cfg!(SovereignRingConfig => bevy_symbios_texture::ring::RingConfig {
    u32   : seed = 0,
    usize : variant_rows = 1,
    usize : variant_cols = 1,
    fp3   : color = [0.85, 0.93, 1.0],
    fp64  : radius = 0.6,
    fp64  : thickness = 0.12,
    fp64  : falloff = 2.0,
    fp64  : waviness = 0.0,
    usize : wave_count = 6,
    fp64  : radius_jitter = 0.1,
    fp    : normal_strength = 1.0,
});

define_sovereign_texture_cfg!(SovereignPetalConfig => bevy_symbios_texture::petal::PetalConfig {
    u32   : seed = 0,
    usize : variant_rows = 2,
    usize : variant_cols = 2,
    fp3   : color_base = [0.98, 0.72, 0.82],
    fp3   : color_edge = [0.93, 0.5, 0.66],
    fp3   : color_throat = [0.99, 0.88, 0.55],
    fp64  : length = 0.92,
    fp64  : width = 0.6,
    fp64  : peak = 0.65,
    fp64  : tip_notch = 0.08,
    fp64  : curl = 0.4,
    fp64  : asymmetry = 0.15,
    fp    : normal_strength = 1.5,
});

define_sovereign_texture_cfg!(SovereignShardConfig => bevy_symbios_texture::shard::ShardConfig {
    u32   : seed = 0,
    usize : variant_rows = 2,
    usize : variant_cols = 2,
    fp3   : color_base = [0.46, 0.43, 0.4],
    fp3   : color_edge = [0.24, 0.22, 0.21],
    usize : sides = 5,
    fp64  : irregularity = 0.45,
    fp64  : edge_band = 0.18,
    fp64  : grain = 0.35,
    fp    : normal_strength = 2.5,
});

define_sovereign_texture_cfg!(SovereignLeafSpriteConfig => bevy_symbios_texture::leaf_sprite::LeafSpriteConfig {
    u32   : seed = 0,
    usize : variant_rows = 2,
    usize : variant_cols = 2,
    nested(SovereignLeafConfig) : leaf = SovereignLeafConfig::default(),
    fp64  : shape_jitter = 0.5,
    fp    : tint_jitter = 0.25,
});

define_sovereign_texture_cfg!(SovereignFlameConfig => bevy_symbios_texture::flame::FlameConfig {
    u32   : seed = 0,
    usize : variant_rows = 2,
    usize : variant_cols = 2,
    fp64  : elongation = 1.6,
    fp64  : turbulence = 0.55,
    fp64  : lean_jitter = 0.25,
    fp64  : falloff = 1.6,
    fp3   : color_core = [1.0, 0.97, 0.78],
    fp3   : color_mid = [1.0, 0.55, 0.10],
    fp3   : color_tip = [0.85, 0.16, 0.02],
    fp    : normal_strength = 1.0,
});

define_sovereign_texture_cfg!(SovereignFlowerConfig => bevy_symbios_texture::flower::FlowerConfig {
    u32   : seed = 0,
    usize : variant_rows = 2,
    usize : variant_cols = 2,
    nested(SovereignPetalConfig) : petal = SovereignPetalConfig::default(),
    usize : petal_count = 6,
    fp64  : center_radius = 0.14,
    fp3   : center_color = [0.96, 0.78, 0.25],
    fp64  : dot_density = 0.5,
    fp    : normal_strength = 1.5,
});

// --- Additional tileable surfaces ------------------------------------------
// Opaque repeat-tiling textures (the render-properties catch-all already
// treats them as surfaces). `Lava` additionally emits a glow map: the
// upstream patch system wires `emissive_texture` and defaults the emissive
// factor to white, so the crust glows without any extra material wiring.

define_sovereign_texture_cfg!(SovereignFabricConfig => bevy_symbios_texture::fabric::FabricConfig {
    u32  : seed = 29,
    fp64 : thread_count = 24.0,
    fp64 : thread_width = 0.85,
    fp64 : weave_contrast = 0.6,
    fp64 : fuzz = 0.35,
    fp3  : color_warp = [0.55, 0.36, 0.24],
    fp3  : color_weft = [0.62, 0.44, 0.30],
    fp   : normal_strength = 3.0,
});

define_sovereign_texture_cfg!(SovereignSandConfig => bevy_symbios_texture::sand::SandConfig {
    u32  : seed = 91,
    fp64 : ripple_count = 10.0,
    fp64 : ripple_warp = 0.6,
    fp64 : grain_density = 0.12,
    fp64 : grain_scale = 24.0,
    fp3  : color_crest = [0.86, 0.74, 0.52],
    fp3  : color_trough = [0.62, 0.50, 0.34],
    fp   : normal_strength = 2.5,
});

define_sovereign_texture_cfg!(SovereignSnowConfig => bevy_symbios_texture::snow::SnowConfig {
    u32   : seed = 73,
    fp64  : drift_scale = 2.5,
    usize : drift_octaves = 4,
    fp64  : sparkle_density = 0.08,
    fp64  : crust_roughness = 0.85,
    fp3   : color_snow = [0.93, 0.95, 0.99],
    fp3   : color_shadow = [0.62, 0.70, 0.86],
    fp    : normal_strength = 1.8,
});

define_sovereign_texture_cfg!(SovereignIceConfig => bevy_symbios_texture::ice::IceConfig {
    u32  : seed = 117,
    fp64 : scale = 3.0,
    fp64 : crack_density = 4.0,
    fp64 : vein_sharpness = 7.0,
    fp64 : frost_level = 0.25,
    fp3  : color_ice = [0.72, 0.84, 0.94],
    fp3  : color_crack = [0.30, 0.44, 0.62],
    fp   : normal_strength = 1.5,
});

define_sovereign_texture_cfg!(SovereignLavaConfig => bevy_symbios_texture::lava::LavaConfig {
    u32  : seed = 666,
    fp64 : plate_scale = 6.0,
    fp64 : crack_width = 0.14,
    fp64 : glow_falloff = 1.6,
    fp3  : color_crust = [0.08, 0.07, 0.07],
    fp3  : color_glow = [1.0, 0.45, 0.06],
    fp   : emissive_intensity = 1.0,
    fp   : normal_strength = 4.0,
});

// --- Alpha-masked mesh cards -----------------------------------------------
// Card-kind silhouettes (like Leaf / Window): clamp-to-edge, alpha-masked.
// ChainLink fences a wire mesh; LogEnd is a cut-log cross-section. Their
// `cell_count` / `ring_count` / `crack_count` are `fp64` frequencies bounded
// by the texture size, so they need no extra sanitiser clamp.

define_sovereign_texture_cfg!(SovereignChainLinkConfig => bevy_symbios_texture::chain_link::ChainLinkConfig {
    u32  : seed = 83,
    fp64 : cell_count = 8.0,
    fp64 : wire_radius = 0.07,
    fp64 : weave_depth = 0.6,
    fp64 : rust_level = 0.2,
    fp3  : color_wire = [0.62, 0.64, 0.66],
    fp3  : color_rust = [0.45, 0.24, 0.10],
    fp   : normal_strength = 3.0,
});

define_sovereign_texture_cfg!(SovereignLogEndConfig => bevy_symbios_texture::log_end::LogEndConfig {
    u32  : seed = 7,
    fp64 : ring_count = 14.0,
    fp64 : ring_warp = 0.35,
    fp64 : ring_contrast = 1.8,
    fp64 : crack_count = 5.0,
    fp64 : bark_width = 0.07,
    fp3  : color_early = [0.78, 0.62, 0.42],
    fp3  : color_late = [0.48, 0.33, 0.18],
    fp3  : color_bark = [0.30, 0.20, 0.12],
    fp   : normal_strength = 2.5,
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
    /// External asset pointer — an HTTPS URL or an ATProto-blob CID hosted
    /// on a peer's PDS. Resolved at room-compile time through the shared
    /// [`BlobImageCache`] and slotted into the layer / construct material
    /// the same way a procedurally-baked variant would be. Lets a room
    /// pull in explicit textures (hand-authored, photographed, traded
    /// across the network) alongside the procedural-generator catalogue
    /// without having to encode every pixel in the room record.
    ///
    /// `source` is held inside a named field rather than as a tuple
    /// payload so the inner
    /// [`SovereignAssetReference`](super::asset_reference::SovereignAssetReference)'s
    /// own
    /// `#[serde(tag = "$type")]` discriminator nests cleanly inside the
    /// outer texture-config discriminator instead of colliding with it.
    ///
    /// [`BlobImageCache`]: crate::world_builder::image_cache::BlobImageCache
    Referenced {
        source: super::asset_reference::SovereignAssetReference,
    },
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
    // Particle sprite cards (alpha-silhouette billboard atlases).
    SoftDisc(SovereignSoftDiscConfig),
    Spark(SovereignSparkConfig),
    Snowflake(SovereignSnowflakeConfig),
    Puff(SovereignPuffConfig),
    Ring(SovereignRingConfig),
    Petal(SovereignPetalConfig),
    Shard(SovereignShardConfig),
    LeafSprite(SovereignLeafSpriteConfig),
    Flame(SovereignFlameConfig),
    Flower(SovereignFlowerConfig),
    // Additional tileable surfaces.
    Fabric(SovereignFabricConfig),
    Sand(SovereignSandConfig),
    Snow(SovereignSnowConfig),
    Ice(SovereignIceConfig),
    Lava(SovereignLavaConfig),
    // Alpha-masked mesh cards.
    ChainLink(SovereignChainLinkConfig),
    LogEnd(SovereignLogEndConfig),
    #[serde(other)]
    Unknown,
}

impl SovereignTextureConfig {
    /// Human-readable variant name for UI combo boxes.
    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Referenced { .. } => "Referenced",
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
            Self::SoftDisc(_) => "Soft Disc",
            Self::Spark(_) => "Spark",
            Self::Snowflake(_) => "Snowflake",
            Self::Puff(_) => "Puff",
            Self::Ring(_) => "Ring",
            Self::Petal(_) => "Petal",
            Self::Shard(_) => "Shard",
            Self::LeafSprite(_) => "Leaf Sprite",
            Self::Flame(_) => "Flame",
            Self::Flower(_) => "Flower",
            Self::Fabric(_) => "Fabric",
            Self::Sand(_) => "Sand",
            Self::Snow(_) => "Snow",
            Self::Ice(_) => "Ice",
            Self::Lava(_) => "Lava",
            Self::ChainLink(_) => "Chain Link",
            Self::LogEnd(_) => "Log End",
            Self::Unknown => "Unknown",
        }
    }

    /// Convert this wire-format variant into the upstream
    /// [`bevy_symbios_texture::TextureConfig`] tagged-union the
    /// `build_procedural_material_async` helper consumes.
    ///
    /// `None` and the catch-all `Unknown` variant both collapse to
    /// `TextureConfig::None` so a future variant deserialised by an older
    /// binary lands cleanly on the no-texture path instead of panicking.
    pub fn to_texture_config(&self) -> bevy_symbios_texture::TextureConfig {
        use bevy_symbios_texture::TextureConfig as T;
        match self {
            // `Referenced` collapses to `None` here because the upstream
            // procedural-texture builder has no equivalent variant — the
            // referenced asset is materialised on a separate resolver path
            // (BlobImageCache) and painted into the material once fetched.
            Self::None | Self::Unknown | Self::Referenced { .. } => T::None,
            Self::Leaf(c) => T::Leaf(c.to_native()),
            Self::Twig(c) => T::Twig(c.to_native()),
            Self::Bark(c) => T::Bark(c.to_native()),
            Self::Window(c) => T::Window(c.to_native()),
            Self::StainedGlass(c) => T::StainedGlass(c.to_native()),
            Self::IronGrille(c) => T::IronGrille(c.to_native()),
            Self::Ground(c) => T::Ground(c.to_native()),
            Self::Rock(c) => T::Rock(c.to_native()),
            Self::Brick(c) => T::Brick(c.to_native()),
            Self::Plank(c) => T::Plank(c.to_native()),
            Self::Shingle(c) => T::Shingle(c.to_native()),
            Self::Stucco(c) => T::Stucco(c.to_native()),
            Self::Concrete(c) => T::Concrete(c.to_native()),
            Self::Metal(c) => T::Metal(c.to_native()),
            Self::Pavers(c) => T::Pavers(c.to_native()),
            Self::Ashlar(c) => T::Ashlar(c.to_native()),
            Self::Cobblestone(c) => T::Cobblestone(c.to_native()),
            Self::Thatch(c) => T::Thatch(c.to_native()),
            Self::Marble(c) => T::Marble(c.to_native()),
            Self::Corrugated(c) => T::Corrugated(c.to_native()),
            Self::Asphalt(c) => T::Asphalt(c.to_native()),
            Self::Wainscoting(c) => T::Wainscoting(c.to_native()),
            Self::Encaustic(c) => T::Encaustic(c.to_native()),
            Self::SoftDisc(c) => T::SoftDisc(c.to_native()),
            Self::Spark(c) => T::Spark(c.to_native()),
            Self::Snowflake(c) => T::Snowflake(c.to_native()),
            Self::Puff(c) => T::Puff(c.to_native()),
            Self::Ring(c) => T::Ring(c.to_native()),
            Self::Petal(c) => T::Petal(c.to_native()),
            Self::Shard(c) => T::Shard(c.to_native()),
            Self::LeafSprite(c) => T::LeafSprite(c.to_native()),
            Self::Flame(c) => T::Flame(c.to_native()),
            Self::Flower(c) => T::Flower(c.to_native()),
            Self::Fabric(c) => T::Fabric(c.to_native()),
            Self::Sand(c) => T::Sand(c.to_native()),
            Self::Snow(c) => T::Snow(c.to_native()),
            Self::Ice(c) => T::Ice(c.to_native()),
            Self::Lava(c) => T::Lava(c.to_native()),
            Self::ChainLink(c) => T::ChainLink(c.to_native()),
            Self::LogEnd(c) => T::LogEnd(c.to_native()),
        }
    }

    /// Atlas dimensions `(rows, cols)` for a particle sprite-card variant,
    /// or `None` for non-sprite configs (surfaces, foliage cards, None).
    ///
    /// When a sprite drives a procedural particle texture, these are the
    /// `variant_rows × variant_cols` of the baked atlas — one cell per
    /// seeded variant — which the emitter copies onto its `texture_atlas`
    /// so a `RandomFrame` draw shows a different variant per particle.
    pub fn sprite_atlas_dims(&self) -> Option<(u32, u32)> {
        match self {
            Self::SoftDisc(c) => Some((c.variant_rows, c.variant_cols)),
            Self::Spark(c) => Some((c.variant_rows, c.variant_cols)),
            Self::Snowflake(c) => Some((c.variant_rows, c.variant_cols)),
            Self::Puff(c) => Some((c.variant_rows, c.variant_cols)),
            Self::Ring(c) => Some((c.variant_rows, c.variant_cols)),
            Self::Petal(c) => Some((c.variant_rows, c.variant_cols)),
            Self::Shard(c) => Some((c.variant_rows, c.variant_cols)),
            Self::LeafSprite(c) => Some((c.variant_rows, c.variant_cols)),
            Self::Flame(c) => Some((c.variant_rows, c.variant_cols)),
            Self::Flower(c) => Some((c.variant_rows, c.variant_cols)),
            _ => None,
        }
    }
}

/// Per-slot material settings for an L-system generator — mirrors
/// `bevy_symbios::materials::MaterialSettings` with DAG-CBOR-safe numeric
/// fields. The embedded [`SovereignTextureConfig`] carries the full config
/// for whichever `bevy_symbios_texture` generator drives this slot (if any).
/// Default-eliding wire format (#695): fields matching
/// [`SovereignMaterialSettings::default`] are omitted on write (the
/// `texture: None` slot alone was ~20 bytes on every one of a prop's dozens
/// of prims) and restored by the container `#[serde(default)]`.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SovereignMaterialSettings {
    pub base_color: Fp3,
    pub emission_color: Fp3,
    pub emission_strength: Fp,
    pub roughness: Fp,
    pub metallic: Fp,
    #[serde(default = "default_uv_scale")]
    pub uv_scale: Fp,
    pub texture: SovereignTextureConfig,
}

crate::pds::serde_util::impl_default_eliding_serialize!(SovereignMaterialSettings {
    base_color,
    emission_color,
    emission_strength,
    roughness,
    metallic,
    uv_scale,
    texture,
});

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

impl SovereignMaterialSettings {
    /// `true` when the whole struct equals its default — the wire-format
    /// skip predicate for prim `material` fields (#695).
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Convert to the upstream PBR settings struct
    /// [`bevy_symbios_texture::MaterialSettings`] consumed by
    /// [`bevy_symbios_texture::build_procedural_material_async`]. The
    /// `Fp`-wrapped fields collapse to plain `f32`/`[f32; 3]`, and the
    /// embedded [`SovereignTextureConfig`] is forwarded through
    /// [`SovereignTextureConfig::to_texture_config`].
    pub fn to_native(&self) -> bevy_symbios_texture::MaterialSettings {
        bevy_symbios_texture::MaterialSettings {
            base_color: self.base_color.0,
            emission_color: self.emission_color.0,
            emission_strength: self.emission_strength.0,
            roughness: self.roughness.0,
            metallic: self.metallic.0,
            uv_scale: self.uv_scale.0,
            texture: self.texture.to_texture_config(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each sprite-card mirror must survive a `to_native()` → `from_native()`
    /// round trip unchanged. A wrong field kind or a missing field in the
    /// `define_sovereign_texture_cfg!` invocation would diverge here.
    #[test]
    fn sprite_mirrors_round_trip() {
        macro_rules! rt {
            ($sov:ty) => {{
                let c = <$sov>::default();
                assert_eq!(
                    c,
                    <$sov>::from_native(&c.to_native()),
                    concat!(stringify!($sov), " round trip diverged")
                );
            }};
        }
        rt!(SovereignSoftDiscConfig);
        rt!(SovereignSparkConfig);
        rt!(SovereignSnowflakeConfig);
        rt!(SovereignPuffConfig);
        rt!(SovereignRingConfig);
        rt!(SovereignPetalConfig);
        rt!(SovereignShardConfig);
        rt!(SovereignLeafSpriteConfig);
        rt!(SovereignFlameConfig);
        rt!(SovereignFlowerConfig);
        rt!(SovereignFabricConfig);
        rt!(SovereignSandConfig);
        rt!(SovereignSnowConfig);
        rt!(SovereignIceConfig);
        rt!(SovereignLavaConfig);
        rt!(SovereignChainLinkConfig);
        rt!(SovereignLogEndConfig);
    }

    /// The new tileable surfaces must be fully wired: a non-"Unknown" label
    /// and a non-`None` upstream dispatch arm.
    #[test]
    fn surface_variants_are_wired_as_surfaces() {
        use bevy_symbios_texture::TextureConfig as T;
        let variants = [
            SovereignTextureConfig::Fabric(Default::default()),
            SovereignTextureConfig::Sand(Default::default()),
            SovereignTextureConfig::Snow(Default::default()),
            SovereignTextureConfig::Ice(Default::default()),
            SovereignTextureConfig::Lava(Default::default()),
        ];
        for v in &variants {
            assert_ne!(v.label(), "Unknown", "{v:?} missing label arm");
            assert!(
                !matches!(v.to_texture_config(), T::None),
                "{v:?} collapsed to TextureConfig::None"
            );
        }
    }

    /// Every sprite variant must carry a non-"Unknown" label and convert to a
    /// non-`None` upstream `TextureConfig` — i.e. it is wired through all the
    /// dispatch arms, not silently collapsing to the no-texture path.
    #[test]
    fn sprite_variants_are_fully_wired() {
        use bevy_symbios_texture::TextureConfig as T;
        let variants = [
            SovereignTextureConfig::SoftDisc(Default::default()),
            SovereignTextureConfig::Spark(Default::default()),
            SovereignTextureConfig::Snowflake(Default::default()),
            SovereignTextureConfig::Puff(Default::default()),
            SovereignTextureConfig::Ring(Default::default()),
            SovereignTextureConfig::Petal(Default::default()),
            SovereignTextureConfig::Shard(Default::default()),
            SovereignTextureConfig::LeafSprite(Default::default()),
            SovereignTextureConfig::Flame(Default::default()),
            SovereignTextureConfig::Flower(Default::default()),
        ];
        for v in &variants {
            assert_ne!(v.label(), "Unknown", "{v:?} missing label arm");
            assert!(
                !matches!(v.to_texture_config(), T::None),
                "{v:?} collapsed to TextureConfig::None"
            );
        }
    }

    /// `sprite_atlas_dims` reports a sprite's variant grid and `None` for
    /// non-sprite configs — the switch the particle baker uses to size the
    /// atlas and decide whether `RandomFrame` has anything to vary.
    #[test]
    fn sprite_atlas_dims_only_for_sprites() {
        let snow = SovereignTextureConfig::Snowflake(SovereignSnowflakeConfig {
            variant_rows: 4,
            variant_cols: 3,
            ..Default::default()
        });
        assert_eq!(snow.sprite_atlas_dims(), Some((4, 3)));

        // Surfaces, foliage cards, and None are not atlas sprites.
        assert_eq!(
            SovereignTextureConfig::Lava(Default::default()).sprite_atlas_dims(),
            None
        );
        assert_eq!(
            SovereignTextureConfig::Bark(Default::default()).sprite_atlas_dims(),
            None
        );
        assert_eq!(SovereignTextureConfig::None.sprite_atlas_dims(), None);
    }
}
