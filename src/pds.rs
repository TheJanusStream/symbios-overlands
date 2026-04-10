//! ATProto PDS integration: DID resolution, room-record fetch, and upsert.
//!
//! The room's environmental state lives on the owner's PDS as a single
//! record at `collection = network.symbios.overlands.room, rkey = self`.
//! `RoomRecord` is the client-side shape of that record and doubles as a
//! Bevy `Resource` so other systems can read the active room configuration
//! without another round trip.
//!
//! ATProto records are encoded as DAG-CBOR, which forbids floats entirely.
//! Runtime fields stay as `f32` but are serialised through fixed-point
//! integer scales (`water_level_offset` × 1000, `sun_color` channels ×
//! 10 000) so the wire format stays lexicon-compliant without forcing the
//! rest of the codebase onto integer maths.

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde::{Deserialize, Serialize};

const COLLECTION: &str = "network.symbios.overlands.room";

// Fixed-point scales used by the custom serde adapters below.
const WATER_OFFSET_SCALE: f32 = 1000.0;
const SUN_COLOR_SCALE: f32 = 10000.0;

// ---------------------------------------------------------------------------
// Room record (ATProto lexicon)
// ---------------------------------------------------------------------------

/// Custom ATProto record stored at `rkey=self` in the room owner's PDS.
/// Represents the owner's environmental customisation of their overland.
#[derive(Serialize, Deserialize, Clone, Debug, Resource)]
pub struct RoomRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    #[serde(with = "fixed_point_water")]
    pub water_level_offset: f32,
    #[serde(with = "fixed_point_color")]
    pub sun_color: [f32; 3],
}

mod fixed_point_water {
    use super::WATER_OFFSET_SCALE;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &f32, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i32((v * WATER_OFFSET_SCALE).round() as i32)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
        Ok(i32::deserialize(d)? as f32 / WATER_OFFSET_SCALE)
    }
}

mod fixed_point_color {
    use super::SUN_COLOR_SCALE;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(v: &[f32; 3], s: S) -> Result<S::Ok, S::Error> {
        let ints: [i32; 3] = [
            (v[0] * SUN_COLOR_SCALE).round() as i32,
            (v[1] * SUN_COLOR_SCALE).round() as i32,
            (v[2] * SUN_COLOR_SCALE).round() as i32,
        ];
        ints.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[f32; 3], D::Error> {
        let ints = <[i32; 3]>::deserialize(d)?;
        Ok([
            ints[0] as f32 / SUN_COLOR_SCALE,
            ints[1] as f32 / SUN_COLOR_SCALE,
            ints[2] as f32 / SUN_COLOR_SCALE,
        ])
    }
}

impl Default for RoomRecord {
    fn default() -> Self {
        Self {
            lex_type: COLLECTION.into(),
            water_level_offset: 0.0,
            sun_color: crate::config::lighting::SUN_COLOR,
        }
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
