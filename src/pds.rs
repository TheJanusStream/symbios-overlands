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

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;

const COLLECTION: &str = "network.symbios.overlands.room";

// ---------------------------------------------------------------------------
// Fixed-point serde wrapper types (DAG-CBOR float workaround)
// ---------------------------------------------------------------------------
//
// DAG-CBOR is strict about numeric types — any `0.98` in the record body
// earns a `400 InvalidRequest` from the PDS. `Fp` wraps an `f32` and
// serialises as `i32` (×10_000); `Fp2`/`Fp3`/`Fp4` do the same for small
// fixed arrays. Rust-side callers still work with plain floats via the
// public `.0` field / `From` conversions.

const FP_SCALE: f32 = 10_000.0;

/// Fixed-point `f32` wrapper — (de)serialises as `i32` scaled by 10_000.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fp(pub f32);

impl Fp {
    pub const ZERO: Fp = Fp(0.0);
    pub const ONE: Fp = Fp(1.0);
}

impl From<f32> for Fp {
    fn from(v: f32) -> Self {
        Fp(v)
    }
}

impl From<Fp> for f32 {
    fn from(v: Fp) -> f32 {
        v.0
    }
}

impl Serialize for Fp {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i32((self.0 * FP_SCALE).round() as i32)
    }
}

impl<'de> Deserialize<'de> for Fp {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Fp(i32::deserialize(d)? as f32 / FP_SCALE))
    }
}

/// Fixed-point `[f32; 2]` wrapper.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fp2(pub [f32; 2]);

impl From<[f32; 2]> for Fp2 {
    fn from(v: [f32; 2]) -> Self {
        Fp2(v)
    }
}

impl Serialize for Fp2 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let ints = [
            (self.0[0] * FP_SCALE).round() as i32,
            (self.0[1] * FP_SCALE).round() as i32,
        ];
        ints.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Fp2 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let ints = <[i32; 2]>::deserialize(d)?;
        Ok(Fp2([ints[0] as f32 / FP_SCALE, ints[1] as f32 / FP_SCALE]))
    }
}

/// Fixed-point `[f32; 3]` wrapper.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fp3(pub [f32; 3]);

impl From<[f32; 3]> for Fp3 {
    fn from(v: [f32; 3]) -> Self {
        Fp3(v)
    }
}

impl Serialize for Fp3 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let ints = [
            (self.0[0] * FP_SCALE).round() as i32,
            (self.0[1] * FP_SCALE).round() as i32,
            (self.0[2] * FP_SCALE).round() as i32,
        ];
        ints.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Fp3 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let ints = <[i32; 3]>::deserialize(d)?;
        Ok(Fp3([
            ints[0] as f32 / FP_SCALE,
            ints[1] as f32 / FP_SCALE,
            ints[2] as f32 / FP_SCALE,
        ]))
    }
}

/// Fixed-point `[f32; 4]` wrapper (quaternions, RGBA colours).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fp4(pub [f32; 4]);

impl From<[f32; 4]> for Fp4 {
    fn from(v: [f32; 4]) -> Self {
        Fp4(v)
    }
}

impl Serialize for Fp4 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let ints = [
            (self.0[0] * FP_SCALE).round() as i32,
            (self.0[1] * FP_SCALE).round() as i32,
            (self.0[2] * FP_SCALE).round() as i32,
            (self.0[3] * FP_SCALE).round() as i32,
        ];
        ints.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Fp4 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let ints = <[i32; 4]>::deserialize(d)?;
        Ok(Fp4([
            ints[0] as f32 / FP_SCALE,
            ints[1] as f32 / FP_SCALE,
            ints[2] as f32 / FP_SCALE,
            ints[3] as f32 / FP_SCALE,
        ]))
    }
}

/// `f64` fixed-point wrapper — same scaling rules as [`Fp`]. Used for
/// noise frequency/scale fields in `bevy_symbios_texture` which operate
/// in double precision.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fp64(pub f64);

impl From<f64> for Fp64 {
    fn from(v: f64) -> Self {
        Fp64(v)
    }
}

impl Serialize for Fp64 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i32((self.0 * FP_SCALE as f64).round() as i32)
    }
}

impl<'de> Deserialize<'de> for Fp64 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Fp64(i32::deserialize(d)? as f64 / FP_SCALE as f64))
    }
}

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

/// Rigid-body transform encoded as fixed-point arrays on the wire.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TransformData {
    pub translation: Fp3,
    /// Quaternion in `[x, y, z, w]` order.
    pub rotation: Fp4,
    pub scale: Fp3,
}

impl Default for TransformData {
    fn default() -> Self {
        Self {
            translation: Fp3([0.0; 3]),
            rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
            scale: Fp3([1.0; 3]),
        }
    }
}

/// Scatter region shape for `Placement::Scatter`.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum ScatterBounds {
    #[serde(rename = "circle")]
    Circle { center: Fp2, radius: Fp },
    #[serde(rename = "rect")]
    Rect { center: Fp2, extents: Fp2 },
}

impl Default for ScatterBounds {
    fn default() -> Self {
        ScatterBounds::Circle {
            center: Fp2([0.0, 0.0]),
            radius: Fp(64.0),
        }
    }
}

// ---------------------------------------------------------------------------
// Terrain generator payload (ported from symbios-ground-lab)
// ---------------------------------------------------------------------------

/// Which base terrain algorithm to run. Mirrors `ground-lab::GeneratorKind`
/// but tagged for lexicon-safe serde.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SovereignGeneratorKind {
    FbmNoise,
    DiamondSquare,
    #[default]
    VoronoiTerracing,
}

/// Full terrain configuration stored inside a `Generator::Terrain` variant.
/// This is a serialisable mirror of `ground-lab::TerrainConfig` — all `f32`
/// fields are wrapped in [`Fp`] so the record stays DAG-CBOR compliant.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SovereignTerrainConfig {
    // Grid / world
    pub grid_size: u32,
    pub cell_scale: Fp,
    pub height_scale: Fp,

    // Algorithm selection
    pub generator_kind: SovereignGeneratorKind,
    pub seed: u64,

    // FBM params
    pub octaves: u32,
    pub persistence: Fp,
    pub lacunarity: Fp,
    pub base_frequency: Fp,

    // Diamond Square params
    pub ds_roughness: Fp,

    // Voronoi params
    pub voronoi_num_seeds: u32,
    pub voronoi_num_terraces: u32,

    // Hydraulic erosion
    pub erosion_enabled: bool,
    pub erosion_drops: u32,
    pub inertia: Fp,
    pub erosion_rate: Fp,
    pub deposition_rate: Fp,
    pub evaporation_rate: Fp,
    pub capacity_factor: Fp,

    // Thermal erosion
    pub thermal_enabled: bool,
    pub thermal_iterations: u32,
    pub thermal_talus_angle: Fp,

    // Material (splat) config
    pub material: SovereignMaterialConfig,
}

impl Default for SovereignTerrainConfig {
    fn default() -> Self {
        Self {
            grid_size: 512,
            cell_scale: Fp(2.0),
            height_scale: Fp(50.0),

            generator_kind: SovereignGeneratorKind::VoronoiTerracing,
            seed: 42,

            octaves: 6,
            persistence: Fp(0.5),
            lacunarity: Fp(2.0),
            base_frequency: Fp(4.0),

            ds_roughness: Fp(0.5),

            voronoi_num_seeds: 1000,
            voronoi_num_terraces: 2,

            erosion_enabled: true,
            erosion_drops: 50_000,
            inertia: Fp(0.05),
            erosion_rate: Fp(0.3),
            deposition_rate: Fp(0.3),
            evaporation_rate: Fp(0.02),
            capacity_factor: Fp(8.0),

            thermal_enabled: true,
            thermal_iterations: 30,
            thermal_talus_angle: Fp(0.05),

            material: SovereignMaterialConfig::default(),
        }
    }
}

/// Splat rule for a single texture layer. `[0, 1]` normalised height; slope
/// is raw gradient magnitude in `[0, ∞)` (1.0 ≈ 45°).
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SovereignSplatRule {
    pub height_min: Fp,
    pub height_max: Fp,
    pub slope_min: Fp,
    pub slope_max: Fp,
    pub sharpness: Fp,
}

/// Procedural "ground" texture parameters (grass / dirt / snow layers).
/// Mirrors `bevy_symbios_texture::ground::GroundConfig` with fixed-point wrappers.
#[derive(Serialize, Deserialize, Clone, Debug)]
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
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SovereignRockConfig {
    pub seed: u32,
    pub scale: Fp64,
    pub octaves: u32,
    pub attenuation: Fp64,
    pub color_light: Fp3,
    pub color_dark: Fp3,
    pub normal_strength: Fp,
}

/// Four-layer splat/texture configuration for a terrain generator.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SovereignMaterialConfig {
    pub texture_size: u32,
    pub tile_scale: Fp,
    /// Splat rules for channels R, G, B, A — Grass / Dirt / Rock / Snow.
    pub rules: [SovereignSplatRule; 4],
    pub grass: SovereignGroundConfig,
    pub dirt: SovereignGroundConfig,
    pub rock: SovereignRockConfig,
    pub snow: SovereignGroundConfig,
}

impl Default for SovereignMaterialConfig {
    fn default() -> Self {
        Self {
            texture_size: 512,
            tile_scale: Fp(90.0),
            rules: [
                // R — Grass
                SovereignSplatRule {
                    height_min: Fp(0.0),
                    height_max: Fp(0.45),
                    slope_min: Fp(0.0),
                    slope_max: Fp(0.30),
                    sharpness: Fp(0.5),
                },
                // G — Dirt
                SovereignSplatRule {
                    height_min: Fp(0.30),
                    height_max: Fp(0.65),
                    slope_min: Fp(0.0),
                    slope_max: Fp(0.55),
                    sharpness: Fp(0.5),
                },
                // B — Rock
                SovereignSplatRule {
                    height_min: Fp(0.0),
                    height_max: Fp(1.0),
                    slope_min: Fp(0.25),
                    slope_max: Fp(1.0),
                    sharpness: Fp(0.5),
                },
                // A — Snow
                SovereignSplatRule {
                    height_min: Fp(0.88),
                    height_max: Fp(1.0),
                    slope_min: Fp(0.0),
                    slope_max: Fp(1.0),
                    sharpness: Fp(4.0),
                },
            ],
            grass: SovereignGroundConfig {
                seed: 1,
                macro_scale: Fp64(2.5),
                macro_octaves: 4,
                micro_scale: Fp64(10.0),
                micro_octaves: 3,
                micro_weight: Fp64(0.3),
                color_dry: Fp3([0.07, 0.12, 0.03]),
                color_moist: Fp3([0.03, 0.07, 0.01]),
                normal_strength: Fp(4.5),
            },
            dirt: SovereignGroundConfig {
                seed: 13,
                macro_scale: Fp64(2.0),
                macro_octaves: 5,
                micro_scale: Fp64(8.0),
                micro_octaves: 4,
                micro_weight: Fp64(0.35),
                color_dry: Fp3([0.52, 0.40, 0.26]),
                color_moist: Fp3([0.28, 0.20, 0.12]),
                normal_strength: Fp(2.0),
            },
            rock: SovereignRockConfig {
                seed: 7,
                scale: Fp64(3.0),
                octaves: 8,
                attenuation: Fp64(2.0),
                color_light: Fp3([0.37, 0.42, 0.36]),
                color_dark: Fp3([0.22, 0.20, 0.18]),
                normal_strength: Fp(4.0),
            },
            snow: SovereignGroundConfig {
                seed: 99,
                macro_scale: Fp64(4.0),
                macro_octaves: 3,
                micro_scale: Fp64(12.0),
                micro_octaves: 3,
                micro_weight: Fp64(0.4),
                color_dry: Fp3([0.95, 0.95, 0.98]),
                color_moist: Fp3([0.80, 0.82, 0.88]),
                normal_strength: Fp(0.8),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// L-system generator payload (ported from lsystem-explorer)
// ---------------------------------------------------------------------------

/// Per-slot material settings for an L-system generator — a trimmed-down
/// mirror of `bevy_symbios::materials::MaterialSettings` without the per-
/// texture config (which can round-trip later via `PropMeshType`-style
/// tagged enums).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SovereignMaterialSettings {
    pub base_color: Fp3,
    pub emission_color: Fp3,
    pub emission_strength: Fp,
    pub roughness: Fp,
    pub metallic: Fp,
}

impl Default for SovereignMaterialSettings {
    fn default() -> Self {
        Self {
            base_color: Fp3([0.6, 0.4, 0.2]),
            emission_color: Fp3([0.0, 0.0, 0.0]),
            emission_strength: Fp(0.0),
            roughness: Fp(0.5),
            metallic: Fp(0.0),
        }
    }
}

/// Prop mesh shapes for `PropMeshType` slots. Mirrors
/// `lsystem-explorer::PropMeshType`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PropMeshType {
    #[default]
    Leaf,
    Twig,
    Sphere,
    Cone,
    Cylinder,
    Cube,
}

// ---------------------------------------------------------------------------
// Open unions: Generators and Placements
// ---------------------------------------------------------------------------

/// Blueprint for something that can be spawned into a room.  Open union:
/// unknown tags deserialize to `Unknown` instead of failing.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "$type")]
// The Terrain variant carries a full `SovereignTerrainConfig` (~400 bytes);
// boxing it would force serde through a wrapping layer that breaks the
// current round-trip tests and the Raw JSON editor format. Generators are
// kept by owning HashMaps, not in hot paths, so the size penalty is fine.
#[allow(clippy::large_enum_variant)]
pub enum Generator {
    #[serde(rename = "network.symbios.gen.terrain")]
    Terrain(SovereignTerrainConfig),

    #[serde(rename = "network.symbios.gen.water")]
    Water { level_offset: Fp },

    #[serde(rename = "network.symbios.gen.shape")]
    Shape { style: String, floors: u32 },

    #[serde(rename = "network.symbios.gen.lsystem")]
    LSystem {
        source_code: String,
        finalization_code: String,
        iterations: u32,
        seed: u64,
        angle: Fp,
        step: Fp,
        width: Fp,
        elasticity: Fp,
        tropism: Option<Fp3>,
        /// Material slot id → PBR settings.
        materials: HashMap<u8, SovereignMaterialSettings>,
        /// Prop id → mesh shape.
        prop_mappings: HashMap<u16, PropMeshType>,
        prop_scale: Fp,
        mesh_resolution: u32,
    },

    #[serde(other)]
    Unknown,
}

/// Where and how a `Generator` is instantiated.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "$type")]
pub enum Placement {
    #[serde(rename = "network.symbios.place.absolute")]
    Absolute {
        generator_ref: String,
        transform: TransformData,
    },

    #[serde(rename = "network.symbios.place.scatter")]
    Scatter {
        generator_ref: String,
        bounds: ScatterBounds,
        count: u32,
        local_seed: u64,
        /// Optional biome filter — scatter points whose dominant splat
        /// channel does not match this id are discarded.
        /// `0 = Grass, 1 = Dirt, 2 = Rock, 3 = Snow`. `None` = everywhere.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        biome_filter: Option<u8>,
    },

    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// Root room record
// ---------------------------------------------------------------------------

/// Non-spatial environment state — sky / sun / fog tint.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Environment {
    pub sun_color: Fp3,
}

/// The full recipe: environment + generators + placements + traits. Acts as
/// a Bevy `Resource` so `world_builder.rs` can compile it into ECS entities.
#[derive(Serialize, Deserialize, Clone, Debug, Resource)]
pub struct RoomRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    pub environment: Environment,
    pub generators: HashMap<String, Generator>,
    pub placements: Vec<Placement>,
    /// Maps a generator name to a list of trait strings (e.g.
    /// `"collider_heightfield"`, `"sensor"`) the world compiler should attach
    /// to every entity that generator spawns.
    pub traits: HashMap<String, Vec<String>>,
}

impl RoomRecord {
    /// Zero-configuration homeworld. When a client visits a DID whose owner
    /// has never saved a custom record, this builds the canonical default
    /// recipe on the fly — a base terrain plus a base water plane — so the
    /// world builder always has something valid to compile.
    pub fn default_for_did(did: &str) -> Self {
        // Synthesise a per-owner terrain seed from the DID so every freshly
        // visited overland has unique topography without requiring the owner
        // to touch their record. FNV-1a 64-bit — deterministic across peers.
        let did_seed = {
            let mut hash: u64 = 0xcbf29ce484222325;
            for byte in did.bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
            hash
        };
        let terrain_cfg = SovereignTerrainConfig {
            seed: did_seed,
            ..SovereignTerrainConfig::default()
        };

        let mut generators = HashMap::new();
        generators.insert("base_terrain".to_string(), Generator::Terrain(terrain_cfg));
        generators.insert(
            "base_water".to_string(),
            Generator::Water {
                level_offset: Fp(0.0),
            },
        );

        let placements = vec![
            Placement::Absolute {
                generator_ref: "base_terrain".to_string(),
                transform: TransformData::default(),
            },
            Placement::Absolute {
                generator_ref: "base_water".to_string(),
                transform: TransformData::default(),
            },
        ];

        let mut traits = HashMap::new();
        traits.insert(
            "base_terrain".to_string(),
            vec!["collider_heightfield".to_string(), "ground".to_string()],
        );

        Self {
            lex_type: COLLECTION.into(),
            environment: Environment {
                sun_color: Fp3(crate::config::lighting::SUN_COLOR),
            },
            generators,
            placements,
            traits,
        }
    }
}

impl Default for RoomRecord {
    fn default() -> Self {
        Self::default_for_did("")
    }
}

// ---------------------------------------------------------------------------
// DID Document types (shared with avatar.rs on WASM)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DidDocument {
    #[serde(default)]
    pub service: Vec<DidService>,
}

#[derive(Deserialize)]
pub struct DidService {
    pub id: String,
    #[serde(rename = "serviceEndpoint")]
    pub service_endpoint: String,
}

// ---------------------------------------------------------------------------
// PDS resolution
// ---------------------------------------------------------------------------

/// Build the DID-document URL for a `did:web` identifier, following the W3C
/// did:web spec rules for path-based identifiers and percent-encoded ports.
///
/// * `did:web:example.com`             → `https://example.com/.well-known/did.json`
/// * `did:web:example.com:u:alice`     → `https://example.com/u/alice/did.json`
/// * `did:web:example.com%3A8080`      → `https://example.com:8080/.well-known/did.json`
fn did_web_document_url(rest: &str) -> String {
    let (domain_enc, path) = match rest.split_once(':') {
        Some((d, p)) => (d, Some(p.replace(':', "/"))),
        None => (rest, None),
    };
    let domain = domain_enc.replace("%3A", ":");
    match path {
        Some(path) => format!("https://{}/{}/did.json", domain, path),
        None => format!("https://{}/.well-known/did.json", domain),
    }
}

/// Resolve a DID to its ATProto PDS endpoint by fetching the DID document.
pub async fn resolve_pds(client: &reqwest::Client, did: &str) -> Option<String> {
    let url = if did.starts_with("did:plc:") {
        format!("https://plc.directory/{}", did)
    } else if let Some(rest) = did.strip_prefix("did:web:") {
        did_web_document_url(rest)
    } else {
        return None;
    };
    let doc: DidDocument = client.get(&url).send().await.ok()?.json().await.ok()?;
    doc.service
        .iter()
        .find(|s| s.id == "#atproto_pds")
        .map(|s| s.service_endpoint.clone())
}

// ---------------------------------------------------------------------------
// Read: fetch room record from the room owner's PDS
// ---------------------------------------------------------------------------

/// Wrapper for the `getRecord` XRPC response.
#[derive(Deserialize)]
struct GetRecordResponse {
    value: RoomRecord,
}

/// Fetch the room customisation record from the given DID's PDS.
/// Returns `None` on 404 (no record yet) or any network error.
pub async fn fetch_room_record(client: &reqwest::Client, did: &str) -> Option<RoomRecord> {
    let pds = resolve_pds(client, did).await?;
    let url = format!(
        "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey=self",
        pds, did, COLLECTION
    );
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let wrapper: GetRecordResponse = resp.json().await.ok()?;
    Some(wrapper.value)
}

// ---------------------------------------------------------------------------
// Write: publish room record to the authenticated user's PDS
// ---------------------------------------------------------------------------

/// Payload for `com.atproto.repo.putRecord`.
#[derive(Serialize)]
struct PutRecordRequest<'a> {
    repo: &'a str,
    collection: &'a str,
    rkey: &'a str,
    record: &'a RoomRecord,
}

/// Write (upsert) the room record to the authenticated user's own PDS.
pub async fn publish_room_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    record: &RoomRecord,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;

    let url = format!("{}/xrpc/com.atproto.repo.putRecord", pds);
    let body = PutRecordRequest {
        repo: &session.did,
        collection: COLLECTION,
        rkey: "self",
        record,
    };

    let resp = client
        .post(&url)
        .bearer_auth(&session.access_jwt)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.status().is_success() {
        Ok(())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(format!("putRecord failed: {} — {}", status, body))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
            biome_filter: Some(0),
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
