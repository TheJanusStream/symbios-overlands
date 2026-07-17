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
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    if let Some(len) = resp.content_length()
        && len as usize > cap
    {
        return None;
    }
    read_capped_body(resp, cap).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn read_capped_body(mut resp: reqwest::Response, cap: usize) -> Option<Vec<u8>> {
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

// On WASM the browser fetch API has already buffered the body by the
// time reqwest hands back the `Response`; `chunk()` isn't exposed and
// mid-stream cancellation isn't possible. The `Content-Length`
// pre-check in `fetch_capped_bytes` already rejects the obvious case;
// this post-check catches servers that lie about / omit the header.
#[cfg(target_arch = "wasm32")]
async fn read_capped_body(resp: reqwest::Response, cap: usize) -> Option<Vec<u8>> {
    let bytes = resp.bytes().await.ok()?;
    if bytes.len() > cap {
        return None;
    }
    Some(bytes.to_vec())
}

/// Public size-bounded GET for binary blobs. Used by the avatar fetch
/// path so a peer-controlled CDN / PDS can't stream us a runaway body.
pub async fn fetch_blob_bytes_capped(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    fetch_capped_bytes(client, url, MAX_FETCH_BODY_BYTES).await
}

/// Decode the body of an already-successful XRPC `getRecord` response as
/// JSON, streaming it under [`MAX_FETCH_BODY_BYTES`] instead of buffering
/// the whole thing.
///
/// The `reqwest::Response::json()` shortcut every record fetch used to
/// call buffers the entire body into RAM before parsing, so a hostile PDS
/// named in a peer's DID document could answer `com.atproto.repo.getRecord`
/// with an infinitely-streaming (or multi-gigabyte) body and OOM any
/// client that fetches that peer's room / avatar / inventory record. The
/// caller has already validated the status code (and peeled off the
/// 404 / `RecordNotFound` cases), so this only handles the success path.
pub(crate) async fn decode_record_json<T: DeserializeOwned>(
    resp: reqwest::Response,
) -> Result<T, FetchError> {
    // Cheap early reject when the server is honest about an oversized body.
    if let Some(len) = resp.content_length()
        && len as usize > MAX_FETCH_BODY_BYTES
    {
        return Err(FetchError::Decode(format!(
            "record body {len} bytes exceeds {MAX_FETCH_BODY_BYTES}-byte cap"
        )));
    }
    let bytes = read_capped_body(resp, MAX_FETCH_BODY_BYTES)
        .await
        .ok_or_else(|| {
            FetchError::Decode(format!(
                "record body exceeded {MAX_FETCH_BODY_BYTES}-byte cap"
            ))
        })?;
    serde_json::from_slice(&bytes).map_err(|e| FetchError::Decode(e.to_string()))
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

/// Resolve an ATProto `@handle` to its DID via the public AppView's
/// unauthenticated `com.atproto.identity.resolveHandle` — CORS-reachable
/// on wasm, same pattern as the login feed's `getAuthorFeed` (#848).
///
/// Errors are plain-language, suitable for direct display on the login
/// form: the everyday failure is a typo'd handle, not a transport fault.
pub async fn resolve_handle(client: &reqwest::Client, handle: &str) -> Result<String, String> {
    #[derive(Deserialize)]
    struct ResolveHandleResp {
        did: String,
    }
    let url = url::Url::parse_with_params(
        "https://public.api.bsky.app/xrpc/com.atproto.identity.resolveHandle",
        [("handle", handle)],
    )
    .map_err(|e| format!("Couldn't build the handle lookup URL: {e}"))?;
    let resp = client.get(url).send().await.map_err(|e| {
        format!("Couldn't reach the network to look up @{handle} — {e}. Check your connection.")
    })?;
    if !resp.status().is_success() {
        // The AppView answers 400 `HandleNotFound` for unknown handles —
        // by far the likeliest cause is a typo.
        return Err(format!(
            "Couldn't find an account for @{handle} — check the spelling."
        ));
    }
    let body: ResolveHandleResp = resp
        .json()
        .await
        .map_err(|e| format!("Handle lookup for @{handle} returned an unreadable answer: {e}"))?;
    Ok(body.did)
}

/// Resolve a DID to its ATProto PDS endpoint by fetching the DID document.
pub async fn resolve_pds(client: &reqwest::Client, did: &str) -> Option<String> {
    let url = if did.starts_with("did:plc:") {
        format!("https://plc.directory/{}", did)
    } else {
        let rest = did.strip_prefix("did:web:")?;
        did_web_document_url(rest)
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

/// Hard cap the reference PDS puts on one `com.atproto.repo.applyWrites`
/// batch. [`apply_writes`] refuses larger batches locally so the caller
/// hears "split the batch" instead of a server 400.
pub(crate) const MAX_APPLY_WRITES: usize = 200;

/// One write of a `com.atproto.repo.applyWrites` batch. The `$type` tags
/// are the lexicon's union refs, so a `Vec<RepoWrite>` serializes directly
/// as the request's `writes` array.
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "$type")]
pub(crate) enum RepoWrite {
    #[serde(rename = "com.atproto.repo.applyWrites#create")]
    Create {
        collection: String,
        rkey: String,
        value: serde_json::Value,
    },
    #[serde(rename = "com.atproto.repo.applyWrites#update")]
    Update {
        collection: String,
        rkey: String,
        value: serde_json::Value,
    },
    #[serde(rename = "com.atproto.repo.applyWrites#delete")]
    Delete { collection: String, rkey: String },
}

/// Commit a batch of record writes to the authenticated user's repo in ONE
/// atomic commit via `com.atproto.repo.applyWrites` — either every write
/// lands or none do, so multi-record layouts (inventory items, later the
/// room manifest + children of Stage 3) can never be observed torn by a
/// crash or a mid-batch rejection.
pub(crate) async fn apply_writes(
    pds: &str,
    session: &bevy_symbios_multiuser::auth::AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    writes: Vec<RepoWrite>,
) -> Result<(), String> {
    if writes.len() > MAX_APPLY_WRITES {
        return Err(format!(
            "applyWrites batch of {} exceeds the {MAX_APPLY_WRITES}-write commit cap — split the batch",
            writes.len()
        ));
    }
    let url = format!("{}/xrpc/com.atproto.repo.applyWrites", pds);
    let body = serde_json::json!({ "repo": session.did, "writes": writes });
    let (status, body) =
        crate::oauth::oauth_post_with_refresh(&session.session, refresh, &url, &body).await?;
    if status.is_success() {
        Ok(())
    } else {
        Err(format!("applyWrites failed: {} — {}", status, body))
    }
}
