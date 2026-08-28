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

/// Prefix of the placeholder DID method used by ATProto's PLC directory.
const DID_PLC_PREFIX: &str = "did:plc:";
/// Prefix of the W3C `did:web` method, the other method ATProto identity
/// resolution accepts.
const DID_WEB_PREFIX: &str = "did:web:";

/// Whether `did` uses a DID method the ATProto network can resolve.
///
/// Locally-minted DIDs — the synthetic `did:attract:…` the login backdrop
/// stamps on its demo world, for one — have no DID document and no AppView
/// profile, so every network lookup for them is a guaranteed failure. Callers
/// that would otherwise hit the network check this first, so a synthetic
/// identity costs no round-trip and logs no error the user could act on.
pub fn is_resolvable_did(did: &str) -> bool {
    did.starts_with(DID_PLC_PREFIX) || did.starts_with(DID_WEB_PREFIX)
}

/// Resolve a DID to its ATProto PDS endpoint by fetching the DID document.
pub async fn resolve_pds(client: &reqwest::Client, did: &str) -> Option<String> {
    let url = if did.starts_with(DID_PLC_PREFIX) {
        format!("https://plc.directory/{}", did)
    } else {
        let rest = did.strip_prefix(DID_WEB_PREFIX)?;
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

/// Byte budget for one `applyWrites` request body (#1115).
///
/// A write count is not the binding limit. The reference PDS caps the whole
/// XRPC **JSON body** at 150 KiB (`jsonLimit` in
/// `packages/pds/src/index.ts`), so a batch of well under two hundred
/// writes is rejected the moment the records it carries sum past that —
/// which a first publish of a heavy seeded room does easily, since it
/// writes every child at once and the largest single child in the
/// catalogue is already ~91 KiB. The failure was an opaque 413 that the
/// per-record ceiling could never pre-empt, because that ceiling is
/// measured per record and this limit is per request.
///
/// 120 KiB leaves room for the `{repo, writes:[…]}` envelope and each
/// write's `$type`/`collection`/`rkey` fields on top of the record values.
pub(crate) const MAX_APPLY_WRITES_BYTES: usize = 120 * 1024;

impl RepoWrite {
    /// Serialized size of this write as it will appear in the request's
    /// `writes` array, plus a comma. Used to pack batches under
    /// [`MAX_APPLY_WRITES_BYTES`].
    pub(crate) fn wire_bytes(&self) -> usize {
        serde_json::to_vec(self).map_or(0, |v| v.len()) + 1
    }
}

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

/// Pack writes into `applyWrites` batches under **both** caps: the
/// write-count commit limit and the request-body byte budget (#1115).
///
/// Counting writes alone was the bug. The reference PDS caps the XRPC JSON
/// body at 150 KiB, so a first publish of a heavy seeded room — every child
/// created in one batch, the largest already ~91 KiB — was rejected whole
/// with an opaque 413 long before two hundred writes were reached. The
/// per-record ceiling could not pre-empt it either: that measures one
/// record, this limits a request.
///
/// The input order is preserved exactly, so the read-safe sequencing
/// (creates → manifest → deletes) still holds across every chunk boundary.
/// A single write larger than the whole budget is an error rather than a
/// batch that would certainly be refused: it cannot be split, and saying so
/// here names the record instead of leaving a 413 to explain it.
pub(crate) fn chunk_writes(ordered: Vec<RepoWrite>) -> Result<Vec<Vec<RepoWrite>>, String> {
    let mut batches: Vec<Vec<RepoWrite>> = Vec::new();
    let mut batch: Vec<RepoWrite> = Vec::new();
    let mut batch_bytes = 0usize;
    for write in ordered {
        let bytes = write.wire_bytes();
        if bytes > MAX_APPLY_WRITES_BYTES {
            return Err(format!(
                "a single record is {} — past the {} the PDS accepts in one request; \
                 remove content from it and retry",
                crate::pds::record_size::human_bytes(bytes),
                crate::pds::record_size::human_bytes(MAX_APPLY_WRITES_BYTES),
            ));
        }
        let full = batch.len() == MAX_APPLY_WRITES || batch_bytes + bytes > MAX_APPLY_WRITES_BYTES;
        if full && !batch.is_empty() {
            batches.push(std::mem::take(&mut batch));
            batch_bytes = 0;
        }
        batch_bytes += bytes;
        batch.push(write);
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    Ok(batches)
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
    // The byte twin of the count cap (#1115). Refused here so the caller
    // hears which limit it hit, rather than a bare 413 from the PDS.
    let batch_bytes: usize = writes.iter().map(RepoWrite::wire_bytes).sum();
    if batch_bytes > MAX_APPLY_WRITES_BYTES {
        return Err(format!(
            "applyWrites batch is {} — past the {} request-body budget; split the batch",
            crate::pds::record_size::human_bytes(batch_bytes),
            crate::pds::record_size::human_bytes(MAX_APPLY_WRITES_BYTES),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A create carrying `bytes` of payload, near enough for packing tests.
    fn sized_write(rkey: &str, bytes: usize) -> RepoWrite {
        RepoWrite::Create {
            collection: "network.symbios.overlands.room.generator".into(),
            rkey: rkey.into(),
            value: serde_json::json!({ "blob": "x".repeat(bytes) }),
        }
    }

    /// #1115: the reference PDS caps the XRPC JSON body at 150 KiB, so a
    /// first publish of a heavy seeded room — every child created in one
    /// batch — was rejected whole with an opaque 413 while the count cap
    /// (200 writes) sat unreached and the per-record ceiling (measured per
    /// record, not per request) could never pre-empt it.
    #[test]
    fn batches_are_packed_under_the_request_body_budget() {
        // Ten 40 KiB children: 400 KiB total, well inside 200 writes.
        let writes: Vec<RepoWrite> = (0..10)
            .map(|i| sized_write(&format!("child{i}"), 40 * 1024))
            .collect();
        let batches = chunk_writes(writes).expect("packs");

        assert!(batches.len() > 1, "400 KiB cannot ride in one request");
        for batch in &batches {
            let bytes: usize = batch.iter().map(RepoWrite::wire_bytes).sum();
            assert!(
                bytes <= MAX_APPLY_WRITES_BYTES,
                "batch of {bytes} B exceeds the budget"
            );
            assert!(batch.len() <= MAX_APPLY_WRITES);
            assert!(!batch.is_empty(), "an empty batch is a wasted round trip");
        }
    }

    /// The read-safe ordering (creates → manifest → deletes) is the whole
    /// reason a torn publish is survivable, so packing must never reorder.
    #[test]
    fn packing_preserves_the_order_it_was_given() {
        let writes: Vec<RepoWrite> = (0..12)
            .map(|i| sized_write(&format!("r{i:02}"), 30 * 1024))
            .collect();
        let flattened: Vec<RepoWrite> = chunk_writes(writes.clone())
            .expect("packs")
            .into_iter()
            .flatten()
            .collect();
        assert_eq!(flattened, writes);
    }

    #[test]
    fn many_small_writes_still_stop_at_the_count_cap() {
        // The count cap must keep binding where bytes do not.
        let writes: Vec<RepoWrite> = (0..250).map(|i| sized_write(&format!("t{i}"), 8)).collect();
        let batches = chunk_writes(writes).expect("packs");
        assert!(batches.iter().all(|b| b.len() <= MAX_APPLY_WRITES));
        assert_eq!(batches.iter().map(Vec::len).sum::<usize>(), 250);
    }

    #[test]
    fn a_single_record_past_the_budget_is_named_rather_than_413ed() {
        // It cannot be split, so a batch would certainly be refused. Saying
        // so locally beats letting the PDS answer with a bare 413.
        let err = chunk_writes(vec![sized_write("whale", MAX_APPLY_WRITES_BYTES + 1)])
            .expect_err("refused");
        assert!(err.contains("single record"), "{err}");
        assert!(err.contains("remove content"), "offers a way out: {err}");
    }

    #[test]
    fn an_empty_plan_produces_no_batches() {
        assert!(chunk_writes(Vec::new()).expect("packs").is_empty());
    }

    #[test]
    fn network_did_methods_are_resolvable() {
        assert!(is_resolvable_did("did:plc:vpkhqolt662uhesyj6nxm7ys"));
        assert!(is_resolvable_did("did:web:example.com"));
        assert!(is_resolvable_did("did:web:example.com:u:alice"));
    }

    #[test]
    fn synthetic_and_malformed_dids_are_not_resolvable() {
        // The login backdrop's demo-world owner — the case that used to
        // fire a 400 profile-fetch warning on every attract scene.
        assert!(!is_resolvable_did("did:attract:19fa5262e75"));
        assert!(!is_resolvable_did("did:key:z6MkhaXgBZDvot"));
        assert!(!is_resolvable_did("did:plc"));
        assert!(!is_resolvable_did("plc:vpkhqolt662uhesyj6nxm7ys"));
        assert!(!is_resolvable_did(""));
    }
}
