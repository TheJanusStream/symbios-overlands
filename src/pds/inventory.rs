//! Personal stash of `Generator` blueprints, keyed off the owner's DID at
//! `collection = network.symbios.overlands.inventory, rkey = self`.
//!
//! The in-world editor lets the owner tuck any generator they like into
//! their inventory, rename it, and later spawn it into whichever room they
//! happen to be editing — so a hand-authored L-system, region blueprint, or
//! deeply-nested generator hierarchy survives across rooms the same way an
//! avatar does.

use super::INVENTORY_COLLECTION;
use super::generator::Generator;
use super::sanitize::sanitize_generator;
use super::xrpc::{FetchError, XrpcError, resolve_pds};
use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-owner stash of `Generator` blueprints. Published to the owner's PDS
/// via `putRecord`; fetched once at Loading alongside the room and avatar
/// records.
#[derive(Serialize, Deserialize, Clone, Debug, Resource)]
pub struct InventoryRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    pub generators: HashMap<String, Generator>,
}

impl Default for InventoryRecord {
    fn default() -> Self {
        Self {
            lex_type: INVENTORY_COLLECTION.into(),
            generators: HashMap::new(),
        }
    }
}

impl InventoryRecord {
    /// Clamp every stored generator to the same bounds the room record
    /// enforces, and cap the overall stash size so a hostile PDS blob can't
    /// force the owner's client into a multi-megabyte allocation on login.
    /// The cap is enforced in lexicographic key order so the survivor set
    /// is deterministic (HashMap iteration is SipHash-randomised).
    pub fn sanitize(&mut self) {
        let cap = crate::config::state::MAX_INVENTORY_ITEMS;
        if self.generators.len() > cap {
            let mut keys: Vec<String> = self.generators.keys().cloned().collect();
            keys.sort();
            for key in keys.into_iter().skip(cap) {
                self.generators.remove(&key);
            }
        }
        for generator in self.generators.values_mut() {
            sanitize_generator(generator);
        }
    }
}

#[derive(Deserialize)]
struct GetInventoryResponse {
    value: InventoryRecord,
}

/// Fetch the inventory record for `did`. `Ok(None)` signals a 404 / "no
/// record yet" which the caller must treat as a clean empty stash — the
/// same convention as [`super::fetch_room_record`].
pub async fn fetch_inventory_record(
    client: &reqwest::Client,
    did: &str,
) -> Result<Option<InventoryRecord>, FetchError> {
    let pds = resolve_pds(client, did)
        .await
        .ok_or(FetchError::DidResolutionFailed)?;
    let url = format!(
        "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey=self",
        pds, did, INVENTORY_COLLECTION
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| FetchError::Network(e.to_string()))?;
    let status = resp.status();
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        if let Ok(xrpc) = serde_json::from_str::<XrpcError>(&body)
            && let Some(err) = xrpc.error.as_deref()
            && (err == "RecordNotFound"
                || (err == "InvalidRequest" && body.contains("RecordNotFound")))
        {
            return Ok(None);
        }
        return Err(FetchError::PdsError(status.as_u16()));
    }
    let mut record = resp
        .json::<GetInventoryResponse>()
        .await
        .map_err(|e| FetchError::Decode(e.to_string()))?
        .value;
    record.sanitize();
    Ok(Some(record))
}

#[derive(Serialize)]
struct PutInventoryRequest<'a> {
    repo: &'a str,
    collection: &'a str,
    rkey: &'a str,
    record: &'a InventoryRecord,
}

/// Upsert the inventory record to the signed-in user's PDS. Thin wrapper
/// around `com.atproto.repo.putRecord` — unlike `publish_room_record` there
/// is no delete-then-put recovery path, because the 5xx-on-stale-CID
/// failure mode the room publish mitigates is driven by generators that
/// reference terrain + splat assets the PDS struggles to validate; a bare
/// inventory record has no such server-side coupling.
pub async fn publish_inventory_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    record: &InventoryRecord,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;
    let url = format!("{}/xrpc/com.atproto.repo.putRecord", pds);
    let body = PutInventoryRequest {
        repo: &session.did,
        collection: INVENTORY_COLLECTION,
        rkey: "self",
        record,
    };
    let body_json = serde_json::to_value(&body).map_err(|e| format!("serialize: {e}"))?;
    let (status, body) =
        crate::oauth::oauth_post_with_nonce_retry(&session.session, &url, &body_json).await?;
    if status.is_success() {
        Ok(())
    } else {
        Err(format!(
            "putRecord (inventory) failed: {} — {}",
            status, body
        ))
    }
}
