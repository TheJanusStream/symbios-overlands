//! ATProto PDS integration: DID resolution, XRPC plumbing, and the three
//! sovereign record lexicons the engine publishes to a player's own PDS.
//!
//! | Record             | Collection NSID                          | rkey   |
//! | ------------------ | ---------------------------------------- | ------ |
//! | [`RoomRecord`]     | `network.symbios.overlands.room`         | `self` |
//! | [`AvatarRecord`]   | `network.symbios.overlands.avatar`       | `self` |
//! | [`InventoryRecord`] | `network.symbios.overlands.inventory`   | `self` |
//!
//! A `RoomRecord` is composed of three open unions:
//!
//! * `generators`  — named hierarchical generators (terrain / water / shape /
//!   lsystem / portal). Every generator carries a transform and a `children`
//!   list, so a single named entry can describe an entire fractal blueprint.
//! * `placements`  — how and where those generators are instantiated
//!   (absolute / scatter / grid)
//! * `traits`      — ECS components attached to entities a generator spawns
//!
//! Every union uses `#[serde(other)] Unknown` so a client visiting a record
//! authored by a newer version of the engine skips the unrecognised variants
//! instead of crashing its deserializer. This is how the schema evolves
//! without breaking older clients.
//!
//! **DAG-CBOR float ban.** ATProto records are encoded as DAG-CBOR, which
//! forbids floats entirely — a PDS returns `400 InvalidRequest` the moment
//! it sees `0.98` in a record body. Every float-bearing field is therefore
//! wrapped in [`Fp`] (or its fixed-length array siblings [`Fp2`], [`Fp3`],
//! [`Fp4`]), which multiply by `FP_SCALE` and round to `i32` on the wire.
//! The wrappers are fully transparent in editor code (`.0` returns the
//! underlying `f32`), so the heightmap / splat / L-system callers never see
//! the fixed-point hop.
//!
//! ## Submodule map
//!
//! * [`types`] — fixed-point wrappers ([`Fp`]/[`Fp2`]/[`Fp3`]/[`Fp4`]/[`Fp64`]),
//!   [`TransformData`], [`BiomeFilter`], [`ScatterBounds`], and the string-key
//!   serde helpers ([`u64_as_string`], [`map_u8_as_string`], [`map_u16_as_string`]).
//! * [`texture`] — every `Sovereign*Config` mirror of a `bevy_symbios_texture`
//!   generator, the unified [`SovereignTextureConfig`] tagged union, and
//!   [`SovereignMaterialSettings`].
//! * [`terrain`] — [`SovereignTerrainConfig`] + splat rules + four-layer
//!   [`SovereignMaterialConfig`].
//! * [`prim`] — [`PropMeshType`] (the hierarchical primitive tree was
//!   retired; every primitive is now a first-class [`Generator`] variant).
//! * [`generator`] — the [`Generator`] hierarchical wrapper, its
//!   variant-specific [`generator::GeneratorKind`] payload, and the
//!   [`Placement`] open-union enum.
//! * [`sanitize`] — clamp helpers + [`sanitize::limits`] for every numeric
//!   field on the wire.
//! * [`xrpc`] — DID resolution, [`FetchError`], and the common XRPC plumbing.
//! * [`avatar`] — avatar phenotype / kinematics / body + fetch/publish.
//! * [`room`] — [`Environment`], [`RoomRecord`], [`find_terrain_config`], and
//!   room-record XRPC wrappers.
//! * [`inventory`] — [`InventoryRecord`] + fetch/publish.

pub(crate) const COLLECTION: &str = "network.symbios.overlands.room";
pub(crate) const AVATAR_COLLECTION: &str = "network.symbios.overlands.avatar";
pub const INVENTORY_COLLECTION: &str = "network.symbios.overlands.inventory";

pub mod avatar;
pub mod generator;
pub mod inventory;
pub mod prim;
pub mod room;
pub mod sanitize;
pub mod terrain;
pub mod texture;
pub mod types;
pub mod xrpc;

// Public re-exports so existing call sites `use crate::pds::Foo;` keep working
// without churn. Submodules remain addressable (e.g. `pds::limits` → now
// `pds::sanitize::limits`) — the old top-level `limits` module path is still
// re-exported below for backwards compatibility.

pub use avatar::{
    AirplaneParams, AvatarRecord, CarParams, HelicopterParams, HoverBoatParams, HumanoidParams,
    LocomotionConfig, fetch_avatar_record, publish_avatar_record,
};
pub use generator::{
    AlphaModeKind, AnimationFrameMode, EmitterShape, Generator, GeneratorKind, ParticleBlendMode,
    Placement, SignSource, SimulationSpace, TextureAtlas, TextureFilter, WaterSurface,
};
pub use inventory::{InventoryRecord, fetch_inventory_record, publish_inventory_record};
pub use prim::PropMeshType;
pub use room::{
    Environment, RoomRecord, delete_room_record, fetch_room_record, find_terrain_config,
    publish_room_record, reset_room_record,
};
pub use sanitize::{limits, sanitize_avatar_visuals, sanitize_generator};
pub use terrain::{
    SovereignGeneratorKind, SovereignMaterialConfig, SovereignSplatRule, SovereignTerrainConfig,
};
pub use texture::{
    SovereignAshlarConfig, SovereignAsphaltConfig, SovereignBarkConfig, SovereignBrickConfig,
    SovereignCobblestoneConfig, SovereignConcreteConfig, SovereignCorrugatedConfig,
    SovereignEncausticConfig, SovereignGroundConfig, SovereignIronGrilleConfig,
    SovereignLeafConfig, SovereignMarbleConfig, SovereignMaterialSettings, SovereignMetalConfig,
    SovereignPaversConfig, SovereignPlankConfig, SovereignRockConfig, SovereignShingleConfig,
    SovereignStainedGlassConfig, SovereignStuccoConfig, SovereignTextureConfig,
    SovereignThatchConfig, SovereignTwigConfig, SovereignWainscotingConfig, SovereignWindowConfig,
};
pub use types::{
    BiomeFilter, FP_SCALE, Fp, Fp2, Fp3, Fp4, Fp64, ScatterBounds, TransformData, WaterRelation,
    map_u8_as_string, map_u16_as_string, u64_as_string,
};
pub use xrpc::{DidDocument, DidService, FetchError, resolve_pds};
