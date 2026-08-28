//! ATProto PDS integration: DID resolution, XRPC plumbing, and the
//! sovereign record lexicons the engine publishes to a player's own PDS.
//!
//! | Record             | Collection NSID                          | rkey   |
//! | ------------------ | ---------------------------------------- | ------ |
//! | [`RoomRecord`] (manifest) | `network.symbios.overlands.room`  | `self` |
//! | [`room::RoomGeneratorRecord`] | `network.symbios.overlands.room.generator` | `hex(fnv1a_64(child json))` |
//! | [`AvatarRecord`]   | `network.symbios.overlands.avatar`       | `self` |
//! | [`inventory::InventoryItemRecord`] | `network.symbios.overlands.inventory.item` | `hex(fnv1a_64(name))` |
//!
//! The inventory is one record **per item** (#696) — the collection is the
//! stash, read via `listRecords` and written as an atomic `applyWrites`
//! diff. [`InventoryRecord`] survives as the in-memory model, and its old
//! `network.symbios.overlands.inventory / self` monolith is still read as a
//! migration fallback (deleted by the first per-item save).
//!
//! The room publishes as a slim **manifest** at `room/self` (environment,
//! placements, traits, `generator_refs` name → rkey) plus one
//! content-addressed child record per generator (#697); pre-#697 monoliths
//! with inline `generators` still decode (version by shape). Writes commit
//! via `applyWrites` in read-safe order: children → manifest → orphan GC.
//!
//! A `RoomRecord` is composed of three top-level collections:
//!
//! * `generators`  — name → [`Generator`] map. Each entry is a hierarchical
//!   node carrying a [`GeneratorKind`] (Terrain / Water / RoadNetwork /
//!   Portal / Gateway / LSystem / Shape / `Sign` / `ParticleSystem` / one of the
//!   sixteen parametric primitives — Cuboid / Sphere / Cylinder / Capsule /
//!   Cone / Torus / Plane / Tetrahedron / Tube / Bevel / Wedge / Helix /
//!   Superellipsoid / Spine / Lathe / BlobGroup), a
//!   local [`TransformData`], and a `Vec<Generator>` of
//!   children — so a single named entry can describe an entire fractal
//!   blueprint.
//! * `placements`  — open-union [`Placement`] list describing how and where
//!   those named generators are instantiated (Absolute / Scatter / Grid).
//! * `traits`      — name → ECS-component-tag list attached to entities a
//!   generator spawns (e.g. `"sensor"` to mark a portal collider as a
//!   trigger).
//!
//! Both [`GeneratorKind`] and [`Placement`] (and every supporting open
//! union — [`SignSource`], [`EmitterShape`], [`ParticleBlendMode`],
//! [`SimulationSpace`], [`AnimationFrameMode`], [`TextureFilter`],
//! [`AlphaModeKind`], [`LocomotionConfig`]) carry `#[serde(other)] Unknown`
//! so a client visiting a record authored by a newer version of the engine
//! skips the unrecognised variants instead of crashing its deserializer.
//! This is how the schema evolves without breaking older clients.
//!
//! **DAG-CBOR float ban.** ATProto records are encoded as DAG-CBOR, which
//! forbids floats entirely — a PDS returns `400 InvalidRequest` the moment
//! it sees `0.98` in a record body. Every float-bearing field is therefore
//! wrapped in [`Fp`] (or its fixed-length array siblings [`Fp2`], [`Fp3`],
//! [`Fp4`]), which multiply by `FP_SCALE` and round to `i32` on the wire.
//! [`Fp64`] is the double-precision sibling used where the editor needs
//! `f64` precision in memory (e.g. world-builder math); it still encodes
//! to a fixed-point `i32` on the wire, just with more headroom in editor
//! code. The wrappers are transparent in editor code (`.0` returns the
//! underlying `f32` for `Fp*` and `f64` for `Fp64`), so the heightmap /
//! splat / L-system callers never see the fixed-point hop.
//!
//! ## Submodule map
//!
//! * [`asset_reference`] — canonical [`SovereignAssetReference`] open union
//!   (Url / AtprotoBlob / DidPfp / Unknown) shared by every "URL or DID"
//!   slot in the engine; [`SignSource`] is retained as a type alias.
//! * [`types`] — fixed-point wrappers ([`Fp`]/[`Fp2`]/[`Fp3`]/[`Fp4`]/[`Fp64`]),
//!   [`TransformData`], [`BiomeFilter`], [`ScatterBounds`], and the string-key
//!   serde helpers ([`u64_as_string`], [`map_u8_as_string`], [`map_u16_as_string`],
//!   [`sorted_string_map`]).
//! * [`texture`] — every `Sovereign*Config` mirror of a `bevy_symbios_texture`
//!   generator, the unified [`SovereignTextureConfig`] tagged union, and
//!   [`SovereignMaterialSettings`].
//! * [`terrain`] — [`SovereignTerrainConfig`] + splat rules + four-layer
//!   [`SovereignMaterialConfig`].
//! * [`prim`] — [`PropMeshType`] (the hierarchical primitive tree was
//!   retired; every primitive is now a first-class [`Generator`] variant).
//! * [`generator`] — the [`Generator`] hierarchical wrapper, its
//!   variant-specific [`generator::GeneratorKind`] payload (Terrain /
//!   Water / RoadNetwork / Portal / LSystem / Shape / primitives / `Sign` /
//!   `ParticleSystem`), the [`Placement`] open-union enum, and the
//!   supporting open unions [`SignSource`], [`EmitterShape`],
//!   [`ParticleBlendMode`], [`SimulationSpace`], [`AnimationFrameMode`],
//!   [`TextureFilter`], [`AlphaModeKind`], plus the [`TextureAtlas`]
//!   sprite-sheet config and per-volume [`WaterSurface`] payload.
//! * [`sanitize`] — clamp helpers + [`sanitize::limits`] for every numeric
//!   field on the wire.
//! * [`xrpc`] — DID resolution, [`FetchError`], and the common XRPC plumbing.
//! * [`record_size`] — serialized-size budgets shared by every record
//!   publish path (soft warn budget, hard pre-flight ceiling).
//! * [`avatar`] — avatar phenotype / kinematics / body + fetch/publish.
//! * [`room`] — [`Environment`], [`RoomRecord`], [`find_terrain_config`], and
//!   room-record XRPC wrappers.
//! * [`inventory`] — [`InventoryRecord`] + fetch/publish.
//! * [`audio`] — Sovereign (fixed-point, DAG-CBOR-safe) mirrors of the
//!   `bevy_symbios_audio` authoring types, rooted at
//!   [`audio::SovereignAudioConfig`].
//! * [`contact_effects`] — PDS-authored contact-effect recipes, translated
//!   into the runtime registry by the world compiler's
//!   `apply_contact_recipes`.
//! * [`material_finish`] — socio-political PBR finish pass for seeded
//!   settlement members ([`material_finish::apply_socio_finish`]).
//! * [`ruin`] — escalation-driven geometric damage pass
//!   ([`ruin::apply_ruin`]).

pub(crate) const COLLECTION: &str = "network.symbios.overlands.room";
pub(crate) const AVATAR_COLLECTION: &str = "network.symbios.overlands.avatar";
/// Pre-#696 single-record stash collection — still read as the migration
/// fallback and deleted by the first per-item save.
pub const INVENTORY_COLLECTION: &str = "network.symbios.overlands.inventory";
/// One record per stash entry (#696); the collection is the stash.
pub const INVENTORY_ITEM_COLLECTION: &str = "network.symbios.overlands.inventory.item";
/// Content-addressed child generators of the room manifest (#697):
/// `rkey = hex(fnv1a_64(child record json))`, referenced by name from the
/// manifest's `generator_refs`.
pub const ROOM_GENERATOR_COLLECTION: &str = "network.symbios.overlands.room.generator";
/// The cross-app wardrobe (#1056): one `symbios-avatar` engine record per
/// entry, tid-keyed, under the SIBLING project's lexicon — deliberately not
/// an overlands NSID, so every symbios app reads the same bodies.
pub const WARDROBE_COLLECTION: &str = "network.symbios.avatar.avatar";
/// The identity's default-body pointer (`rkey = self`), also the sibling
/// project's lexicon. Read as the spawn fallback for identities with no
/// overlands avatar record; written whenever the worn body changes.
pub const AVATAR_PROFILE_COLLECTION: &str = "network.symbios.avatar.profile";
/// One worn prop per record (#1056), tid-keyed: an owned `Generator` copy
/// plus the rig socket it hangs from and its offset transform.
pub const AVATAR_ATTACHMENT_COLLECTION: &str = "network.symbios.overlands.avatar.attachment";

/// Every collection this app **writes** to in the signed-in identity's repo.
///
/// The single source of truth for the OAuth grant: `oauth::granular_scope`
/// builds one `repo:<nsid>` permission per entry, and
/// `client_metadata_scope_covers_every_written_collection` asserts the
/// hosted metadata document carries them all. Since #736 the app asks for
/// granular scopes rather than `transition:generic`, so a collection missing
/// here is a **runtime write rejection**, not a lint — and one that only
/// shows up against a real PDS, which is exactly how the three wardrobe
/// collections shipped unscoped in #1058/#1059 (fixed in #1065).
///
/// **Add to this list in the same commit that adds a collection constant.**
/// Reads are ungated by the spec, so read-only collections do not belong
/// here; every entry below is written by some path in [`avatar::wardrobe`],
/// [`inventory`](crate::pds), or the room publish flow.
pub const WRITTEN_COLLECTIONS: &[&str] = &[
    COLLECTION,
    ROOM_GENERATOR_COLLECTION,
    AVATAR_COLLECTION,
    INVENTORY_COLLECTION,
    INVENTORY_ITEM_COLLECTION,
    // The wardrobe trio (#1054): two under the SIBLING project's lexicon,
    // because a cross-app body lives in a cross-app collection — but still
    // in the signed-in identity's own repo, so still a `repo:` grant.
    WARDROBE_COLLECTION,
    AVATAR_PROFILE_COLLECTION,
    AVATAR_ATTACHMENT_COLLECTION,
];

pub mod asset_reference;
pub mod audio;
pub mod avatar;
pub mod contact_effects;
pub mod generator;
pub mod inventory;
pub mod material_finish;
pub mod prim;
pub mod record_size;
pub mod room;
pub mod ruin;
pub mod sanitize;
pub(crate) mod serde_util;
pub mod terrain;
pub mod texture;
pub mod tid;
pub mod types;
pub mod xrpc;

// Public re-exports so existing call sites `use crate::pds::Foo;` keep working
// without churn. Submodules remain addressable (e.g. `pds::limits` → now
// `pds::sanitize::limits`) — the old top-level `limits` module path is still
// re-exported below for backwards compatibility.

pub use asset_reference::SovereignAssetReference;
pub use audio::SovereignAudioConfig;
pub use avatar::{
    AirplaneParams, AttachmentRecord, AvatarBody, AvatarRecord, CarParams, EngineAvatarRecord,
    EngineProfileRecord, GaitParams, HelicopterParams, HoverBoatParams, HumanoidParams,
    LocomotionConfig, fetch_avatar_record,
};
pub use contact_effects::{
    AudioClipSource, AudioParams, ContactEffectKind, ContactEffectRecord, ContactEffects,
    ContactPhaseKind, ContactSurfaceKind, CountModel, DecalParams, RecipeParticle,
    default_contact_effects,
};
pub use generator::{
    AlphaModeKind, AnimationFrameMode, EmitterShape, Generator, GeneratorKind, ParticleBlendMode,
    ParticleParams, Placement, SignSource, SimulationSpace, TextureAtlas, TextureFilter,
    TortureParams, WaterSurface,
};
pub use inventory::{
    InventoryItemRecord, InventoryRecord, fetch_inventory_record, publish_inventory_record,
};
pub use prim::PropMeshType;
pub use room::{
    DefaultLanding, Environment, MAX_ROAD_NETWORKS, RoomRecord, delete_room_record,
    fetch_room_record, find_road_config, find_road_configs, find_terrain_config,
    publish_room_record, reset_room_record,
};
pub use sanitize::{limits, sanitize_avatar_visuals, sanitize_generator};
pub use terrain::{
    SovereignGeneratorKind, SovereignMaterialConfig, SovereignSplatRule, SovereignTerrainConfig,
};
pub use texture::{
    SovereignAshlarConfig, SovereignAsphaltConfig, SovereignBarkConfig, SovereignBrickConfig,
    SovereignBroadleafConfig, SovereignCactusSkinConfig, SovereignChainLinkConfig,
    SovereignChitinConfig, SovereignCobblestoneConfig, SovereignConcreteConfig, SovereignCorrosion,
    SovereignCorrugatedConfig, SovereignCrackedEarthConfig, SovereignCreviceDirt,
    SovereignEdgeWear, SovereignEnamelConfig, SovereignEncausticConfig, SovereignFabricConfig,
    SovereignFlameConfig, SovereignFlowerConfig, SovereignForestFloorConfig, SovereignFrondConfig,
    SovereignGrassTuftConfig, SovereignGravelConfig, SovereignGroundConfig, SovereignIceConfig,
    SovereignIronGrilleConfig, SovereignLavaConfig, SovereignLeafConfig, SovereignLeafSpriteConfig,
    SovereignLichenConfig, SovereignLogEndConfig, SovereignMarbleConfig, SovereignMaterialSettings,
    SovereignMetalConfig, SovereignMossConfig, SovereignNeedleConfig, SovereignObsidianConfig,
    SovereignParquetConfig, SovereignPaversConfig, SovereignPetalConfig, SovereignPlankConfig,
    SovereignPuffConfig, SovereignReedConfig, SovereignRingConfig, SovereignRockConfig,
    SovereignSandConfig, SovereignShardConfig, SovereignShingleConfig, SovereignSnowConfig,
    SovereignSnowflakeConfig, SovereignSoftDiscConfig, SovereignSolarPanelConfig,
    SovereignSparkConfig, SovereignStainedGlassConfig, SovereignStreaks, SovereignStuccoConfig,
    SovereignTextureConfig, SovereignThatchConfig, SovereignTruchetConfig, SovereignTwigConfig,
    SovereignWainscotingConfig, SovereignWeatheringConfig, SovereignWindowConfig,
};
pub use types::{
    BiomeFilter, FP_SCALE, Fp, Fp2, Fp3, Fp4, Fp64, ScatterBounds, ScatterNaturalness,
    TransformData, WaterRelation, map_u8_as_string, map_u16_as_string, sorted_string_map,
    u64_as_string,
};
pub use xrpc::{DidDocument, DidService, FetchError, resolve_handle, resolve_pds};
