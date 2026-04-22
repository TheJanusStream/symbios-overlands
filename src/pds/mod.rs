//! ATProto PDS integration: DID resolution, room-record fetch, and upsert,
//! plus the `RoomRecord` lexicon that describes a room as a data-driven
//! *recipe*.
//!
//! The record is stored at `collection = network.symbios.overlands.room,
//! rkey = self`.  A record is composed of three open unions:
//!
//! * `generators`  — named blueprints (terrain / water / shape / lsystem…)
//! * `placements`  — how and where those generators are instantiated
//! * `traits`      — ECS components attached to entities a generator spawns
//!
//! Every union uses `#[serde(other)] Unknown` so a client visiting a room
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
//! * [`prim`] — [`PrimShape`], [`PrimNode`], and [`PropMeshType`].
//! * [`generator`] — the [`Generator`] and [`Placement`] open-union enums.
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
    AvatarBody, AvatarRecord, HumanoidKinematics, HumanoidPhenotype, RoverKinematics,
    RoverPhenotype, fetch_avatar_record, publish_avatar_record,
};
pub use generator::{Generator, Placement};
pub use inventory::{InventoryRecord, fetch_inventory_record, publish_inventory_record};
pub use prim::{PrimNode, PrimShape, PropMeshType};
pub use room::{
    Environment, RoomRecord, delete_room_record, fetch_room_record, find_terrain_config,
    publish_room_record, reset_room_record,
};
pub use sanitize::{limits, sanitize_generator};
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
    BiomeFilter, Fp, Fp2, Fp3, Fp4, Fp64, ScatterBounds, TransformData, WaterRelation,
    map_u8_as_string, map_u16_as_string, u64_as_string,
};
pub use xrpc::{DidDocument, DidService, FetchError, resolve_pds};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::types::FP_SCALE;
    use super::*;

    /// Regression guard for issue #58: 64-bit seeds must serialize as JSON
    /// strings, not numbers. Numeric form would round-trip through `f64` in
    /// most parsers (including the ones in front of ATProto PDSes), losing
    /// precision above `2^53` and triggering `500 InternalServerError`
    /// from the DAG-CBOR encoder. The default DID-derived terrain seed
    /// is FNV-1a 64-bit, which routinely lands well above the safe range.
    #[test]
    fn u64_seeds_serialize_as_strings() {
        let r = RoomRecord::default_for_did("did:plc:z5yhcebtrvzblrojezn6pjgi");
        let json = serde_json::to_string(&r).expect("serialise");
        assert!(
            json.contains("\"seed\":\""),
            "terrain seed must be a string in JSON, got: {json}"
        );
        // Round-trip stays lossless.
        let back: RoomRecord = serde_json::from_str(&json).expect("deserialise");
        let original_seed = match r.generators.get("base_terrain") {
            Some(Generator::Terrain(cfg)) => cfg.seed,
            _ => panic!("expected base_terrain"),
        };
        let round_seed = match back.generators.get("base_terrain") {
            Some(Generator::Terrain(cfg)) => cfg.seed,
            _ => panic!("expected base_terrain"),
        };
        assert_eq!(original_seed, round_seed);
    }

    /// Regression guard for issue #48: a `RoomRecord` serialised via serde
    /// must contain zero JSON floating-point literals. DAG-CBOR forbids
    /// floats and the PDS returns `400 InvalidRequest` when it sees one,
    /// so any future field that forgets its `Fp*` wrapper will be caught
    /// here. Scans for a digit-dot-digit pattern so the test doesn't
    /// false-positive on the `$type` string sigil.
    #[test]
    fn default_record_serialises_without_floats() {
        let mut record = RoomRecord::default_for_did("did:plc:test");
        record.environment.sun_color = Fp3([0.98, 0.95, 0.82]);
        if let Some(Generator::Water { level_offset }) = record.generators.get_mut("base_water") {
            *level_offset = Fp(2.5);
        }
        record.placements.push(Placement::Scatter {
            generator_ref: "base_terrain".to_string(),
            bounds: ScatterBounds::Circle {
                center: Fp2([10.5, -3.25]),
                radius: Fp(7.75),
            },
            count: 4,
            local_seed: 42,
            biome_filter: BiomeFilter {
                biomes: vec![0, 2],
                water: WaterRelation::Above,
            },
            snap_to_terrain: true,
            random_yaw: true,
        });

        let json = serde_json::to_string(&record).expect("serialise record");
        let bytes = json.as_bytes();
        for i in 1..bytes.len().saturating_sub(1) {
            if bytes[i] == b'.' && bytes[i - 1].is_ascii_digit() && bytes[i + 1].is_ascii_digit() {
                panic!("expected fixed-point integers, got float in `{json}`");
            }
        }
    }

    /// Round-trip sanity: every `f32` we put in must come back equal
    /// (within the quantisation error of `FP_SCALE`).
    #[test]
    fn fixed_point_round_trip_preserves_values() {
        let original = TransformData {
            translation: Fp3([1.5, -2.25, 3.125]),
            rotation: Fp4([0.0, 0.6, 0.0, 0.8]),
            scale: Fp3([1.0, 2.0, 0.5]),
        };
        let json = serde_json::to_string(&original).unwrap();
        let decoded: TransformData = serde_json::from_str(&json).unwrap();
        let eps = 1.0 / FP_SCALE;
        for (a, b) in original
            .translation
            .0
            .iter()
            .zip(decoded.translation.0.iter())
        {
            assert!((a - b).abs() < eps, "translation drift: {a} vs {b}");
        }
        for (a, b) in original.rotation.0.iter().zip(decoded.rotation.0.iter()) {
            assert!((a - b).abs() < eps, "rotation drift: {a} vs {b}");
        }
        for (a, b) in original.scale.0.iter().zip(decoded.scale.0.iter()) {
            assert!((a - b).abs() < eps, "scale drift: {a} vs {b}");
        }
    }
}
