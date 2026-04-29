//! Shared ATProto XRPC plumbing: DID resolution, [`FetchError`],
//! [`XrpcError`], and the [`PutOutcome`] discriminator used by every
//! record-upsert helper.

use serde::Deserialize;
use serde::de::DeserializeOwned;

/// Hard cap on the bytes a single peer-controlled HTTP body may
/// contribute to memory. A hostile PDS / DID-host can otherwise return
/// an infinitely-streaming body (or a multi-gigabyte payload) and
/// `reqwest::Response::bytes()` / `.json()` will buffer the whole
/// stream into RAM until the client OOMs. 16 MiB matches the cap the
/// world-builder's [`crate::world_builder::image_cache::MAX_IMAGE_BYTES`]
/// already uses for [`crate::pds::SignSource`] fetches and is well past
/// any reasonable image asset.
pub const MAX_FETCH_BODY_BYTES: usize = 16 * 1024 * 1024;

/// Tighter cap for JSON documents fetched from a DID host
/// (`did.json`, `plc.directory`). A normal DID document is well under
/// 4 KiB; 64 KiB leaves headroom for forward-compat fields without
/// letting a hostile `did:web` server stream us a multi-gigabyte JSON
/// payload that locks the async decoder buffer for the duration of
/// the parse.
pub const MAX_DID_DOCUMENT_BYTES: usize = 64 * 1024;

/// Stream `client.get(url)` to a `Vec<u8>`, aborting if the body would
/// exceed `cap`. Mirrors the world-builder's `fetch_url_bytes` chunk
/// loop — the `reqwest::Response::bytes()` shortcut buffers the entire
/// body unconditionally, so any peer-controlled URL that streams past
/// the cap would OOM the client before we got a chance to reject it.
async fn fetch_capped_bytes(client: &reqwest::Client, url: &str, cap: usize) -> Option<Vec<u8>> {
    let mut resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    if let Some(len) = resp.content_length()
        && len as usize > cap
    {
        return None;
    }
    let mut buf: Vec<u8> = Vec::new();
    loop {
        match resp.chunk().await {
            Ok(Some(chunk)) => {
                if buf.len().saturating_add(chunk.len()) > cap {
                    return None;
                }
                buf.extend_from_slice(&chunk);
            }
            Ok(None) => return Some(buf),
            Err(_) => return None,
        }
    }
}

/// Public size-bounded GET for binary blobs. Used by the avatar fetch
/// path so a peer-controlled CDN / PDS can't stream us a runaway body.
pub async fn fetch_blob_bytes_capped(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    fetch_capped_bytes(client, url, MAX_FETCH_BODY_BYTES).await
}

/// Stream `client.get(url)` and decode the body as JSON, aborting if
/// the body would exceed `MAX_DID_DOCUMENT_BYTES`. Used by
/// [`resolve_pds`] (DID document fetches) so a hostile `did:web` host
/// cannot pin client memory inside `reqwest::Response::json()`'s
/// internal buffer with a multi-gigabyte payload.
async fn fetch_did_json<T: DeserializeOwned>(client: &reqwest::Client, url: &str) -> Option<T> {
    let bytes = fetch_capped_bytes(client, url, MAX_DID_DOCUMENT_BYTES).await?;
    serde_json::from_slice(&bytes).ok()
}

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
    let doc: DidDocument = fetch_did_json(client, &url).await?;
    doc.service
        .iter()
        .find(|s| s.id == "#atproto_pds")
        .map(|s| s.service_endpoint.clone())
}

/// Outcome of a `fetch_*_record` call. A 404 means the owner has never saved
/// a custom record (ok to substitute the default); any other outcome is a
/// genuine failure that the caller must distinguish so it does not silently
/// overwrite an existing record with the default on a transient
/// DNS/timeout/5xx blip.
#[derive(Debug)]
pub enum FetchError {
    /// DID could not be resolved to a PDS endpoint (DID doc missing/invalid).
    DidResolutionFailed,
    /// Network transport failure (DNS, connection refused, timeout, etc.).
    Network(String),
    /// PDS responded but with a non-404 error status.
    PdsError(u16),
    /// The response body could not be decoded as the expected record type.
    Decode(String),
}

/// Error envelope returned by ATProto XRPC endpoints on non-2xx responses,
/// e.g. `{"error":"RecordNotFound","message":"Could not locate record..."}`.
#[derive(Deserialize)]
pub(crate) struct XrpcError {
    pub error: Option<String>,
    #[allow(dead_code)]
    pub message: Option<String>,
}

/// Result of a single `putRecord` attempt. The `ServerError` variant
/// distinguishes "the PDS's own logic blew up" (transient-or-buggy; we can
/// retry with delete-then-put) from "the PDS rejected our request" (4xx;
/// retrying won't help and we should surface the error as-is).
pub(crate) enum PutOutcome {
    Ok,
    ServerError(String),
    ClientError(String),
    Transport(String),
}
