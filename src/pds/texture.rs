//! Sovereign (DAG-CBOR safe) mirrors of every `bevy_symbios_texture`
//! generator configuration, along with the unified [`SovereignTextureConfig`]
//! tagged-union enum and [`SovereignMaterialSettings`] PBR wrapper.
//!
//! Most config structs are generated via the `define_sovereign_mirror!`
//! macro so adding a new generator is a single declarative block — each field
//! just names its wire kind (`fp`, `fp3`, `fp64`, `u32`, `usize`, `bool`,
//! `enum(Ty)`, `nested(SovTy)`) and default. [`SovereignGroundConfig`] and
//! [`SovereignRockConfig`] are the two hand-rolled predecessors of the macro.

use super::types::{Fp, Fp2, Fp3};
use serde::{Deserialize, Serialize};

use super::serde_util::define_sovereign_mirror;

/// The mirror type carrying an upstream config on the wire.
///
/// Implemented for every config in the per-field registry by
/// `define_texture_mirrors!`, which is also what lets a generated mirror
/// name a *nested* mirror: the registry gives the nested field's upstream
/// type, and this maps it to the `Sovereign*` that wraps it, with no
/// hand-written table to fall out of step.
pub trait HasSovereignMirror {
    /// The `Sovereign*` type that carries this config on the wire.
    type Mirror;
}

/// The upstream enum a mirror's `enum(T)` field holds.
///
/// The registry names each enum *variant* by path and a macro cannot take
/// the type off the end of one; two configs also both call their field
/// `layout` and mean different enums. Six fields, listed once — and a
/// wrong entry is a compile error, because `to_native` builds the upstream
/// config as a struct literal.
macro_rules! sovereign_enum_ty {
    (SovereignMetalConfig, style) => {
        bevy_symbios_texture::metal::MetalStyle
    };
    (SovereignPaversConfig, layout) => {
        bevy_symbios_texture::pavers::PaversLayout
    };
    (SovereignEncausticConfig, pattern) => {
        bevy_symbios_texture::encaustic::EncausticPattern
    };
    (SovereignGravelConfig, metric) => {
        bevy_symbios_texture::noise::CellMetric
    };
    (SovereignParquetConfig, layout) => {
        bevy_symbios_texture::parquet::ParquetLayout
    };
    (SovereignFabricConfig, weave) => {
        bevy_symbios_texture::fabric::WeaveKind
    };
}

/// Declare every texture mirror from `symbios_texture`'s per-field registry.
///
/// The registry carries one row per field of every upstream config — its
/// kind, its envelope, its mutation step and its UI label — and this maps
/// the *kind* to a wire representation. Nothing about any individual field
/// is written here, so an upstream field addition is no longer a hard
/// compile break needing a hand edit per config (#1304).
///
/// The registry has no idea what overlands calls its mirrors, so the
/// roster below supplies the names, zipped with the registry positionally:
/// a count mismatch fails to compile, and so does a mis-ordered pair,
/// because `to_native` would then build the wrong upstream config.
///
/// A roster entry may carry a `wire { … }` order. `serde_json` writes
/// struct fields in the order the eliding serializer is handed, and a room
/// child is content-addressed over those bytes, so the four mirrors whose
/// historical field order differs from the registry's must keep theirs or
/// every record carrying them would be re-addressed on its next publish.
macro_rules! define_texture_mirrors {
    (
        [ $( [ $Sov:ident $( wire { $($wire:ident),+ $(,)? } )? ] ),+ $(,)? ]
        $(
            $Native:ty, $header:literal, $editor:ident
            $(, fixup $fixup:ident )?
            { $(
                $kind:ident ( $field:ident $($rest:tt)* )
                $( explore ( $xlo:expr, $xhi:expr ) )?
            ),+ $(,)? }
            $( layout { $($layout:tt)* } )?
        ),+ $(,)?
    ) => {
        $(
            define_texture_mirrors!(@one
                $Sov, $Native,
                [ $( wire { $($wire),+ } )? ],
                $( $kind ($field $($rest)*) ),+ ,
            );
        )+
    };

    // Walk one config's rows, accumulating the wire field list, then
    // declare the mirror from it.
    (@one $Sov:ident, $Native:ty, [$($wire:tt)*], $($rows:tt)*) => {
        define_texture_mirrors!(@munch $Sov, $Native, [$($wire)*], [], $($rows)*);
    };

    (@munch $Sov:ident, $Native:ty, [$($wire:tt)*], [$($done:tt)*],) => {
        define_sovereign_mirror!(eliding_derived
            #[doc = concat!(
                "DAG-CBOR-safe mirror of [`", stringify!($Native), "`], \
                 declared from the upstream per-field registry."
            )]
            $Sov => $Native { $($done)* } $($wire)*
        );

        impl HasSovereignMirror for $Native {
            type Mirror = $Sov;
        }

        impl $Sov {
            /// Clamp every field into the envelope the upstream registry
            /// gives it, in place.
            ///
            /// The round trip through the native type is the point: the
            /// envelope is upstream's, so a bound tuned there reaches a
            /// record arriving here without being copied. Quantisation is
            /// lossless in both directions for an in-envelope config —
            /// `Fp`'s grid already carries the value this mirror holds.
            pub fn clamp_to_envelope(&mut self) {
                let mut native = self.to_native();
                symbios_texture::ClampToEnvelope::clamp_to_envelope(&mut native);
                *self = Self::from_native(&native);
            }
        }
    };

    // One arm per registry kind; the trailing arguments of a row (label,
    // envelope, mutation step) belong to the other consumers.
    (@munch $S:ident, $N:ty, [$($w:tt)*], [$($d:tt)*],
     seed ( $f:ident $($a:tt)* ), $($rest:tt)*) => {
        define_texture_mirrors!(@munch $S, $N, [$($w)*], [$($d)* u32 : $f,], $($rest)*);
    };
    (@munch $S:ident, $N:ty, [$($w:tt)*], [$($d:tt)*],
     f64 ( $f:ident $($a:tt)* ), $($rest:tt)*) => {
        define_texture_mirrors!(@munch $S, $N, [$($w)*], [$($d)* fp64 : $f,], $($rest)*);
    };
    (@munch $S:ident, $N:ty, [$($w:tt)*], [$($d:tt)*],
     f64_round ( $f:ident $($a:tt)* ), $($rest:tt)*) => {
        define_texture_mirrors!(@munch $S, $N, [$($w)*], [$($d)* fp64 : $f,], $($rest)*);
    };
    (@munch $S:ident, $N:ty, [$($w:tt)*], [$($d:tt)*],
     f32 ( $f:ident $($a:tt)* ), $($rest:tt)*) => {
        define_texture_mirrors!(@munch $S, $N, [$($w)*], [$($d)* fp : $f,], $($rest)*);
    };
    (@munch $S:ident, $N:ty, [$($w:tt)*], [$($d:tt)*],
     usize ( $f:ident $($a:tt)* ), $($rest:tt)*) => {
        define_texture_mirrors!(@munch $S, $N, [$($w)*], [$($d)* usize : $f,], $($rest)*);
    };
    (@munch $S:ident, $N:ty, [$($w:tt)*], [$($d:tt)*],
     color3 ( $f:ident $($a:tt)* ), $($rest:tt)*) => {
        define_texture_mirrors!(@munch $S, $N, [$($w)*], [$($d)* fp3 : $f,], $($rest)*);
    };
    (@munch $S:ident, $N:ty, [$($w:tt)*], [$($d:tt)*],
     bool ( $f:ident $($a:tt)* ), $($rest:tt)*) => {
        define_texture_mirrors!(@munch $S, $N, [$($w)*], [$($d)* bool : $f,], $($rest)*);
    };
    // The shared upstream enum rides the wire as itself; only its type has
    // to be named, and `sovereign_enum_ty!` is the six-row table for that.
    (@munch $S:ident, $N:ty, [$($w:tt)*], [$($d:tt)*],
     enum_pick ( $f:ident $($a:tt)* ), $($rest:tt)*) => {
        define_texture_mirrors!(
            @munch $S, $N, [$($w)*],
            [$($d)* enum(sovereign_enum_ty!($S, $f)) : $f,], $($rest)*
        );
    };
    // A nested config's mirror comes from the trait rather than a table.
    (@munch $S:ident, $N:ty, [$($w:tt)*], [$($d:tt)*],
     nested ( $f:ident, $sub:ty, $($a:tt)* ), $($rest:tt)*) => {
        define_texture_mirrors!(
            @munch $S, $N, [$($w)*],
            [$($d)* nested(<$sub as HasSovereignMirror>::Mirror) : $f,], $($rest)*
        );
    };
}

/// The roster: overlands' name for each mirror, in the registry's order.
///
/// This is the only per-variant list the mirrors need; everything else
/// about a field comes from upstream. Four entries carry an explicit wire
/// order because their historical field order differs from the registry's
/// — see `define_texture_mirrors!` above.
macro_rules! texture_mirrors_with_roster {
    ( $($registry:tt)* ) => {
        define_texture_mirrors!(
            [
            [SovereignBarkConfig wire {
                    seed, scale, octaves, warp_u, warp_v, warp_octaves, color_light,
                    color_dark, normal_strength, furrow_multiplier, furrow_scale_u,
                    furrow_scale_v, furrow_shape
            }],
            [SovereignRockConfig],
            [SovereignEdgeWear],
            [SovereignCorrosion],
            [SovereignCreviceDirt],
            [SovereignStreaks],
            [SovereignWeatheringConfig],
            [SovereignGroundConfig],
            [SovereignLeafConfig],
            [SovereignNeedleConfig],
            [SovereignBroadleafConfig],
            [SovereignMossConfig],
            [SovereignLichenConfig],
            [SovereignReedConfig],
            [SovereignCactusSkinConfig],
            [SovereignFrondConfig],
            [SovereignGrassTuftConfig],
            [SovereignTwigConfig],
            [SovereignBrickConfig],
            [SovereignWindowConfig],
            [SovereignPlankConfig],
            [SovereignShingleConfig],
            [SovereignStuccoConfig],
            [SovereignConcreteConfig],
            [SovereignMetalConfig wire {
                    seed, style, scale, seam_count, seam_sharpness, brush_stretch,
                    rivet_size, hole_size, roughness, metallic, rust_level, color_metal,
                    color_rust, weathering, normal_strength
            }],
            [SovereignPaversConfig wire {
                    seed, scale, aspect_ratio, grout_width, bevel, cell_variance,
                    roughness, color_stone, color_grout, layout, weathering,
                    normal_strength
            }],
            [SovereignAshlarConfig],
            [SovereignCobblestoneConfig],
            [SovereignThatchConfig],
            [SovereignMarbleConfig wire {
                    seed, scale, octaves, warp_strength, warp_octaves, vein_frequency,
                    vein_sharpness, roughness, color_base, color_vein, weathering,
                    normal_strength
            }],
            [SovereignCorrugatedConfig],
            [SovereignAsphaltConfig],
            [SovereignWainscotingConfig],
            [SovereignStainedGlassConfig],
            [SovereignIronGrilleConfig],
            [SovereignEncausticConfig],
            [SovereignSoftDiscConfig],
            [SovereignSparkConfig],
            [SovereignSnowflakeConfig],
            [SovereignPuffConfig],
            [SovereignRingConfig],
            [SovereignPetalConfig],
            [SovereignShardConfig],
            [SovereignLogEndConfig],
            [SovereignChainLinkConfig],
            [SovereignLavaConfig],
            [SovereignCrackedEarthConfig],
            [SovereignGravelConfig],
            [SovereignForestFloorConfig],
            [SovereignEnamelConfig],
            [SovereignObsidianConfig],
            [SovereignChitinConfig],
            [SovereignSolarPanelConfig],
            [SovereignParquetConfig],
            [SovereignTruchetConfig],
            [SovereignIceConfig],
            [SovereignSnowConfig],
            [SovereignSandConfig],
            [SovereignFabricConfig],
            [SovereignFlowerConfig],
            [SovereignFlameConfig],
            [SovereignLeafSpriteConfig],
            ]
            $($registry)*
        );
    };
}

symbios_texture::for_each_texture_field!(texture_mirrors_with_roster);

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
    // Vegetation ground-cover / understory billboard cards.
    GrassTuft(SovereignGrassTuftConfig),
    Frond(SovereignFrondConfig),
    Reed(SovereignReedConfig),
    Needle(SovereignNeedleConfig),
    Broadleaf(SovereignBroadleafConfig),
    Moss(SovereignMossConfig),
    Lichen(SovereignLichenConfig),
    // Additional tileable surfaces.
    Fabric(SovereignFabricConfig),
    Sand(SovereignSandConfig),
    Snow(SovereignSnowConfig),
    Ice(SovereignIceConfig),
    Lava(SovereignLavaConfig),
    // Succulent construct skin (tileable, opaque).
    CactusSkin(SovereignCactusSkinConfig),
    // Terrain surfaces added in bevy_symbios_texture 0.8.
    CrackedEarth(SovereignCrackedEarthConfig),
    Gravel(SovereignGravelConfig),
    ForestFloor(SovereignForestFloorConfig),
    // Catalogue surfaces added in bevy_symbios_texture 0.8.
    Enamel(SovereignEnamelConfig),
    Obsidian(SovereignObsidianConfig),
    Chitin(SovereignChitinConfig),
    SolarPanel(SovereignSolarPanelConfig),
    Parquet(SovereignParquetConfig),
    Truchet(SovereignTruchetConfig),
    // Alpha-masked mesh cards.
    ChainLink(SovereignChainLinkConfig),
    LogEnd(SovereignLogEndConfig),
    #[serde(other, skip_serializing)]
    Unknown,
}

impl SovereignTextureConfig {
    /// Human-readable variant name for UI combo boxes.
    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "None",
            // Renamed from "Referenced" (#1251 f354). It was the only
            // entry in a list of fifty-seven whose behaviour depends on
            // the network, sitting among plain-English materials like
            // Brick and Thatch under a word from the record schema.
            Self::Referenced { .. } => "External image",
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
            Self::GrassTuft(_) => "Grass Tuft",
            Self::Frond(_) => "Frond",
            Self::Reed(_) => "Reed",
            Self::Needle(_) => "Needle",
            Self::Broadleaf(_) => "Broadleaf",
            Self::Moss(_) => "Moss",
            Self::Lichen(_) => "Lichen",
            Self::Fabric(_) => "Fabric",
            Self::Sand(_) => "Sand",
            Self::Snow(_) => "Snow",
            Self::Ice(_) => "Ice",
            Self::Lava(_) => "Lava",
            Self::CactusSkin(_) => "Cactus Skin",
            Self::CrackedEarth(_) => "Cracked Earth",
            Self::Gravel(_) => "Gravel",
            Self::ForestFloor(_) => "Forest Floor",
            Self::Enamel(_) => "Enamel",
            Self::Obsidian(_) => "Obsidian",
            Self::Chitin(_) => "Chitin",
            Self::SolarPanel(_) => "Solar Panel",
            Self::Parquet(_) => "Parquet",
            Self::Truchet(_) => "Truchet",
            Self::ChainLink(_) => "Chain Link",
            Self::LogEnd(_) => "Log End",
            Self::Unknown => "Unknown",
        }
    }

    /// `true` when this texture is an **alpha card** — a clamp-to-edge,
    /// alpha-masked image that must span its quad exactly once (a window,
    /// stained glass, an iron grille, a leaf sprite) rather than tiling like
    /// a surface.
    ///
    /// Delegates to the upstream
    /// [`RenderProperties`](bevy_symbios_texture::RenderProperties), which is
    /// generated from the same generator registry that decides the material's
    /// alpha mode, culling and clamp-vs-repeat sampling. Deriving it here
    /// rather than re-listing the card variants keeps the *geometry* decision
    /// (how to lay out UVs) from drifting away from the *material* decision
    /// (how to sample them) when a generator is added upstream.
    pub fn is_card(&self) -> bool {
        self.to_texture_config().render_properties().is_card
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
            Self::GrassTuft(c) => T::GrassTuft(c.to_native()),
            Self::Frond(c) => T::Frond(c.to_native()),
            Self::Reed(c) => T::Reed(c.to_native()),
            Self::Needle(c) => T::Needle(c.to_native()),
            Self::Broadleaf(c) => T::Broadleaf(c.to_native()),
            Self::Moss(c) => T::Moss(c.to_native()),
            Self::Lichen(c) => T::Lichen(c.to_native()),
            Self::Fabric(c) => T::Fabric(c.to_native()),
            Self::Sand(c) => T::Sand(c.to_native()),
            Self::Snow(c) => T::Snow(c.to_native()),
            Self::Ice(c) => T::Ice(c.to_native()),
            Self::Lava(c) => T::Lava(c.to_native()),
            Self::CactusSkin(c) => T::CactusSkin(c.to_native()),
            Self::CrackedEarth(c) => T::CrackedEarth(c.to_native()),
            Self::Gravel(c) => T::Gravel(c.to_native()),
            Self::ForestFloor(c) => T::ForestFloor(c.to_native()),
            Self::Enamel(c) => T::Enamel(c.to_native()),
            Self::Obsidian(c) => T::Obsidian(c.to_native()),
            Self::Chitin(c) => T::Chitin(c.to_native()),
            Self::SolarPanel(c) => T::SolarPanel(c.to_native()),
            Self::Parquet(c) => T::Parquet(c.to_native()),
            Self::Truchet(c) => T::Truchet(c.to_native()),
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
    /// Texture repeats per **metre** of surface (#933).
    ///
    /// Every mesher emits UVs in metres of prim-local surface, so this is a
    /// physical density rather than a repeat count: `5.0` lays a 20 cm
    /// brick course, `0.5` a two-metre concrete panel, and the same value
    /// reads identically on a 0.8 m pier and an 8 m wall. The reciprocal —
    /// the tile's edge length in metres — is usually the number worth
    /// thinking in.
    ///
    /// Before #933 UVs were normalised to `0..1` over each prim, which made
    /// this a per-prim repeat count and texel density a function of prim
    /// size; every catalogue material carried a hand-tuned value to
    /// compensate.
    ///
    /// Alpha *cards* (the `Window` / foliage / sprite generators) are the
    /// exception: they upload clamp-to-edge and must span their quad
    /// exactly once, so they keep `1.0`.
    #[serde(default = "default_uv_scale")]
    pub uv_scale: Fp,
    /// Pattern slide in **metres of surface** (#957) — plain UV units under
    /// a `Fit` mapping, where UVs aren't metres. Rides the material's
    /// `uv_transform` beside [`uv_scale`](Self::uv_scale), so editing it
    /// re-keys only the `StandardMaterial`, never a mesh.
    pub uv_offset: Fp2,
    /// Pattern spin in degrees, counter-clockwise in UV space (#957).
    /// Converted to radians at the `uv_transform`; same no-mesh-rebuild
    /// property as [`uv_offset`](Self::uv_offset).
    pub uv_rotation: Fp,
    pub texture: SovereignTextureConfig,
}

crate::pds::serde_util::impl_default_eliding_serialize!(SovereignMaterialSettings {
    base_color,
    emission_color,
    emission_strength,
    roughness,
    metallic,
    uv_scale,
    uv_offset,
    uv_rotation,
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
            uv_offset: Fp2([0.0, 0.0]),
            uv_rotation: Fp(0.0),
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
    ///
    /// [`uv_offset`](Self::uv_offset) / [`uv_rotation`](Self::uv_rotation)
    /// deliberately do **not** cross this boundary — the upstream struct has
    /// no such fields (adding them there is a hard compile break per the
    /// texture-mirror discipline), so the world-builder applies them over
    /// the built `StandardMaterial` instead
    /// (`sovereign_uv_transform` in `world_builder::material`).
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

    /// #957 wire discipline: the uv_transform knobs elide at their defaults
    /// (so every already-published material fingerprint is unchanged) and
    /// round-trip when authored.
    #[test]
    fn uv_offset_rotation_elide_and_round_trip() {
        let v = serde_json::to_value(SovereignMaterialSettings::default()).expect("serialises");
        let obj = v.as_object().expect("object");
        assert!(
            !obj.contains_key("uv_offset") && !obj.contains_key("uv_rotation"),
            "default knobs must stay off the wire: {obj:?}"
        );

        let m = SovereignMaterialSettings {
            uv_offset: Fp2([0.25, -1.0]),
            uv_rotation: Fp(45.0),
            ..Default::default()
        };
        let v = serde_json::to_value(&m).expect("serialises");
        assert!(v.get("uv_offset").is_some() && v.get("uv_rotation").is_some());
        let re: SovereignMaterialSettings = serde_json::from_value(v).expect("reparses");
        assert_eq!(re, m);
    }

    /// Each sprite-card mirror must survive a `to_native()` → `from_native()`
    /// round trip unchanged. A wrong field kind or a missing field in the
    /// `define_sovereign_mirror!` invocation would diverge here.
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
        rt!(SovereignGrassTuftConfig);
        rt!(SovereignFrondConfig);
        rt!(SovereignReedConfig);
        rt!(SovereignNeedleConfig);
        rt!(SovereignBroadleafConfig);
        rt!(SovereignMossConfig);
        rt!(SovereignLichenConfig);
        rt!(SovereignFabricConfig);
        rt!(SovereignSandConfig);
        rt!(SovereignSnowConfig);
        rt!(SovereignIceConfig);
        rt!(SovereignLavaConfig);
        rt!(SovereignCactusSkinConfig);
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
            SovereignTextureConfig::CactusSkin(Default::default()),
            SovereignTextureConfig::Moss(Default::default()),
            SovereignTextureConfig::Lichen(Default::default()),
        ];
        for v in &variants {
            assert_ne!(v.label(), "Unknown", "{v:?} missing label arm");
            assert!(
                !matches!(v.to_texture_config(), T::None),
                "{v:?} collapsed to TextureConfig::None"
            );
        }
    }

    /// The generators added in `bevy_symbios_texture` 0.8 must be wired
    /// through every dispatch arm, not silently collapsing to the no-texture
    /// path.
    #[test]
    fn texture_0_8_surfaces_are_fully_wired() {
        use bevy_symbios_texture::TextureConfig as T;
        let variants = [
            SovereignTextureConfig::CrackedEarth(Default::default()),
            SovereignTextureConfig::Gravel(Default::default()),
            SovereignTextureConfig::ForestFloor(Default::default()),
            SovereignTextureConfig::Enamel(Default::default()),
            SovereignTextureConfig::Obsidian(Default::default()),
            SovereignTextureConfig::Chitin(Default::default()),
            SovereignTextureConfig::SolarPanel(Default::default()),
            SovereignTextureConfig::Parquet(Default::default()),
            SovereignTextureConfig::Truchet(Default::default()),
        ];
        for v in &variants {
            assert_ne!(v.label(), "Unknown", "{v:?} missing label arm");
            assert!(
                !matches!(v.to_texture_config(), T::None),
                "{v:?} collapsed to TextureConfig::None"
            );
            // All nine are tileable surfaces, so none may claim to be a card.
            assert!(!v.is_card(), "{v:?} is not a card");
        }
    }

    // `mirror_defaults_match_upstream` lived here: twenty-six hand-written
    // assertions that a mirror's declared default matched upstream's. Since
    // #1304 there is no declared default to drift — `Default` is
    // `Self::from_native(&Native::default())` — and the check that matters is
    // exhaustive and lives with the bytes it protects, in
    // `tests/texture_wire.rs`: `every_mirror_default_matches_upstream` covers
    // all fifty-seven, and the blessed fixture fails loudly if a default move
    // ever changes which fields elide.

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
        // The grass tuft is a foliage billboard card baked at SURFACE
        // resolution, not a particle atlas — no sprite dims.
        assert_eq!(
            SovereignTextureConfig::GrassTuft(Default::default()).sprite_atlas_dims(),
            None
        );
        assert_eq!(
            SovereignTextureConfig::Frond(Default::default()).sprite_atlas_dims(),
            None
        );
        assert_eq!(
            SovereignTextureConfig::Reed(Default::default()).sprite_atlas_dims(),
            None
        );
        assert_eq!(
            SovereignTextureConfig::Needle(Default::default()).sprite_atlas_dims(),
            None
        );
        assert_eq!(
            SovereignTextureConfig::Broadleaf(Default::default()).sprite_atlas_dims(),
            None
        );
        assert_eq!(SovereignTextureConfig::None.sprite_atlas_dims(), None);
    }

    /// The vegetation foliage cards must be fully wired: a real label and a
    /// non-`None` upstream dispatch arm.
    #[test]
    fn vegetation_cards_are_fully_wired() {
        use bevy_symbios_texture::TextureConfig as T;
        let variants = [
            SovereignTextureConfig::GrassTuft(Default::default()),
            SovereignTextureConfig::Frond(Default::default()),
            SovereignTextureConfig::Reed(Default::default()),
            SovereignTextureConfig::Needle(Default::default()),
            SovereignTextureConfig::Broadleaf(Default::default()),
        ];
        for v in &variants {
            assert_ne!(v.label(), "Unknown", "{v:?} missing label arm");
            assert!(
                !matches!(v.to_texture_config(), T::None),
                "{v:?} collapsed to TextureConfig::None"
            );
        }
    }
}
