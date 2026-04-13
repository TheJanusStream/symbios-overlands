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
//! it sees `0.98` in a record body. Every float-bearing field in this
//! module is therefore serialised through one of the `fp_*` adapters
//! below, which multiply by `FP_SCALE` and round to an `i32` on the wire.
//! Rust-side callers still work with plain `f32` — the fixed-point hop
//! only exists at the serde boundary.  A uniform scale of 10 000 gives
//! 0.0001 precision and a maximum representable magnitude of ~214 000,
//! which covers world-space coordinates, quaternions, colours and any
//! reasonable level offset without loss.

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const COLLECTION: &str = "network.symbios.overlands.room";

// ---------------------------------------------------------------------------
// Fixed-point serde adapters (DAG-CBOR float workaround)
// ---------------------------------------------------------------------------
//
// DAG-CBOR is strict about numeric types — any `0.98` in the record body
// earns a `400 InvalidRequest` from the PDS. These adapters multiply the
// runtime `f32` by `FP_SCALE` and round to an `i32` on the wire so the
// record stays lexicon-compliant without forcing every downstream system
// onto integer maths.
//
// Each `with` module is type-specific because `serde(with = "...")` needs
// signatures that match the annotated field's type exactly.

const FP_SCALE: f32 = 10_000.0;

mod fp_f32 {
    use super::FP_SCALE;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &f32, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i32((v * FP_SCALE).round() as i32)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
        Ok(i32::deserialize(d)? as f32 / FP_SCALE)
    }
}

mod fp_f32_2 {
    use super::FP_SCALE;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(v: &[f32; 2], s: S) -> Result<S::Ok, S::Error> {
        let ints: [i32; 2] = [
            (v[0] * FP_SCALE).round() as i32,
            (v[1] * FP_SCALE).round() as i32,
        ];
        ints.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[f32; 2], D::Error> {
        let ints = <[i32; 2]>::deserialize(d)?;
        Ok([ints[0] as f32 / FP_SCALE, ints[1] as f32 / FP_SCALE])
    }
}

mod fp_f32_3 {
    use super::FP_SCALE;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(v: &[f32; 3], s: S) -> Result<S::Ok, S::Error> {
        let ints: [i32; 3] = [
            (v[0] * FP_SCALE).round() as i32,
            (v[1] * FP_SCALE).round() as i32,
            (v[2] * FP_SCALE).round() as i32,
        ];
        ints.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[f32; 3], D::Error> {
        let ints = <[i32; 3]>::deserialize(d)?;
        Ok([
            ints[0] as f32 / FP_SCALE,
            ints[1] as f32 / FP_SCALE,
            ints[2] as f32 / FP_SCALE,
        ])
    }
}

mod fp_f32_4 {
    use super::FP_SCALE;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(v: &[f32; 4], s: S) -> Result<S::Ok, S::Error> {
        let ints: [i32; 4] = [
            (v[0] * FP_SCALE).round() as i32,
            (v[1] * FP_SCALE).round() as i32,
            (v[2] * FP_SCALE).round() as i32,
            (v[3] * FP_SCALE).round() as i32,
        ];
        ints.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[f32; 4], D::Error> {
        let ints = <[i32; 4]>::deserialize(d)?;
        Ok([
            ints[0] as f32 / FP_SCALE,
            ints[1] as f32 / FP_SCALE,
            ints[2] as f32 / FP_SCALE,
            ints[3] as f32 / FP_SCALE,
        ])
    }
}

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

/// Rigid-body transform encoded as fixed-point i32 arrays on the wire
/// (DAG-CBOR rejects floats; see the `fp_*` adapters above).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TransformData {
    #[serde(with = "fp_f32_3")]
    pub translation: [f32; 3],
    /// Quaternion in `[x, y, z, w]` order.
    #[serde(with = "fp_f32_4")]
    pub rotation: [f32; 4],
    #[serde(with = "fp_f32_3")]
    pub scale: [f32; 3],
}

impl Default for TransformData {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        }
    }
}

/// Scatter region shape for `Placement::Scatter`.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum ScatterBounds {
    #[serde(rename = "circle")]
    Circle {
        #[serde(with = "fp_f32_2")]
        center: [f32; 2],
        #[serde(with = "fp_f32")]
        radius: f32,
    },
    #[serde(rename = "rect")]
    Rect {
        #[serde(with = "fp_f32_2")]
        center: [f32; 2],
        #[serde(with = "fp_f32_2")]
        extents: [f32; 2],
    },
}

// ---------------------------------------------------------------------------
// Open unions: Generators and Placements
// ---------------------------------------------------------------------------

/// Blueprint for something that can be spawned into a room.  Open union:
/// unknown tags deserialize to `Unknown` instead of failing.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "$type")]
pub enum Generator {
    #[serde(rename = "network.symbios.gen.terrain")]
    Terrain {
        #[serde(with = "fp_f32")]
        noise_scale: f32,
        terraces: usize,
    },

    #[serde(rename = "network.symbios.gen.water")]
    Water {
        #[serde(with = "fp_f32")]
        level_offset: f32,
    },

    #[serde(rename = "network.symbios.gen.shape")]
    Shape { style: String, floors: u32 },

    #[serde(rename = "network.symbios.gen.lsystem")]
    LSystem {
        axiom: String,
        rules: HashMap<String, String>,
        iterations: u32,
    },

    #[serde(other)]
    Unknown,
}

/// Where and how a `Generator` is instantiated.  Open union, same
/// forward-compat rules as `Generator`.
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
    #[serde(with = "fp_f32_3")]
    pub sun_color: [f32; 3],
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
    pub fn default_for_did(_did: &str) -> Self {
        let mut generators = HashMap::new();
        generators.insert(
            "base_terrain".to_string(),
            Generator::Terrain {
                noise_scale: 1.0,
                terraces: 2,
            },
        );
        generators.insert(
            "base_water".to_string(),
            Generator::Water { level_offset: 0.0 },
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
            vec![
                "collider_heightfield".to_string(),
                "ground".to_string(),
            ],
        );

        Self {
            lex_type: COLLECTION.into(),
            environment: Environment {
                sun_color: crate::config::lighting::SUN_COLOR,
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
    // The first colon separates the (possibly percent-encoded) domain from the
    // optional path; any further colons inside the path become `/`.
    let (domain_enc, path) = match rest.split_once(':') {
        Some((d, p)) => (d, Some(p.replace(':', "/"))),
        None => (rest, None),
    };
    // Ports in `did:web` are percent-encoded (`%3A`); decode them so reqwest
    // produces a syntactically valid authority.
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
    /// so any future field that forgets its `fp_*` adapter will be caught
    /// here. We scan for a digit-dot-digit pattern so the test doesn't
    /// false-positive on the `$type` string sigil.
    #[test]
    fn default_record_serialises_without_floats() {
        // Use non-trivial floats so any missing adapter shows up as a
        // literal in the output — a field left at 0.0 or 1.0 would
        // serialise to `0` / `1` even without fixed-point wrapping.
        let mut record = RoomRecord::default_for_did("did:plc:test");
        record.environment.sun_color = [0.98, 0.95, 0.82];
        if let Some(Generator::Water { level_offset }) =
            record.generators.get_mut("base_water")
        {
            *level_offset = 2.5;
        }
        if let Some(Generator::Terrain { noise_scale, .. }) =
            record.generators.get_mut("base_terrain")
        {
            *noise_scale = 1.7;
        }
        record.placements.push(Placement::Scatter {
            generator_ref: "base_terrain".to_string(),
            bounds: ScatterBounds::Circle {
                center: [10.5, -3.25],
                radius: 7.75,
            },
            count: 4,
            local_seed: 42,
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
            translation: [1.5, -2.25, 3.125],
            rotation: [0.0, 0.6, 0.0, 0.8],
            scale: [1.0, 2.0, 0.5],
        };
        let json = serde_json::to_string(&original).unwrap();
        let decoded: TransformData = serde_json::from_str(&json).unwrap();
        let eps = 1.0 / FP_SCALE;
        for (a, b) in original.translation.iter().zip(decoded.translation.iter()) {
            assert!((a - b).abs() < eps, "translation drift: {a} vs {b}");
        }
        for (a, b) in original.rotation.iter().zip(decoded.rotation.iter()) {
            assert!((a - b).abs() < eps, "rotation drift: {a} vs {b}");
        }
        for (a, b) in original.scale.iter().zip(decoded.scale.iter()) {
            assert!((a - b).abs() < eps, "scale drift: {a} vs {b}");
        }
    }
}
