//! Shared ATProto XRPC plumbing: DID resolution, [`FetchError`],
//! [`XrpcError`], and the `applyWrites` layer every publish path commits
//! through — [`RepoWrite`], [`record_exists`] (the create-vs-update probe),
//! [`chunk_writes`] and [`apply_writes`].

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

/// Byte cap for reading an XRPC **error** body (#1124).
///
/// Two orders of magnitude below [`MAX_FETCH_BODY_BYTES`] because an XRPC
/// error envelope is `{"error":"...","message":"..."}` — a few hundred
/// bytes. The only thing callers do with this text is look for
/// `RecordNotFound` and put the rest in a log line or a status message,
/// so nothing legitimate comes close.
pub(crate) const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;

/// Read a non-2xx response body as text under [`MAX_ERROR_BODY_BYTES`].
///
/// The hole this closes (#1124): [`decode_record_json`] was written
/// precisely because a hostile PDS can stream an unbounded body, but only
/// the SUCCESS path was routed through it. Every error branch still called
/// `resp.text().await.unwrap_or_default()`, which buffers without limit —
/// so the entire defence was bypassable by answering with a status that is
/// neither 2xx nor 404. Peer avatar fetches fire automatically when anyone
/// joins a room, which made that a zero-click OOM against every guest, and
/// on wasm the heap never shrinks so it is a hard crash.
///
/// Returns an empty string rather than an error when the body is oversized
/// or unreadable: callers are already on a failure path and are deciding
/// what to *say* about it. An absent body degrades to "no `RecordNotFound`
/// marker, no detail to log", which is the same thing
/// `unwrap_or_default()` did — the difference is that it can no longer be
/// reached by an allocation the client could not survive.
pub(crate) async fn read_capped_text(resp: reqwest::Response) -> String {
    if let Some(len) = resp.content_length()
        && len as usize > MAX_ERROR_BODY_BYTES
    {
        return String::new();
    }
    read_capped_body(resp, MAX_ERROR_BODY_BYTES)
        .await
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
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
    if let Some(hit) = cached_pds(did) {
        return Some(hit);
    }
    let url = if did.starts_with(DID_PLC_PREFIX) {
        format!("https://plc.directory/{}", did)
    } else {
        let rest = did.strip_prefix(DID_WEB_PREFIX)?;
        did_web_document_url(rest)
    };
    let doc: DidDocument = fetch_did_json(client, &url).await?;
    let endpoint = doc
        .service
        .iter()
        .find(|s| s.id == "#atproto_pds")
        .map(|s| s.service_endpoint.clone())?;
    // A DID document is written by whoever controls the DID, and every
    // record fetch that follows aims at whatever it names (#1127). An
    // endpoint that is not https, or that points at an address only this
    // client can reach, is refused here rather than at each of the dozen
    // call sites downstream.
    if !crate::pds::sanitize::is_fetchable_endpoint(&endpoint) {
        bevy::log::warn!("{did} names a PDS endpoint this client will not follow: {endpoint}");
        return None;
    }
    remember_pds(did, &endpoint);
    Some(endpoint)
}

/// How many DID → PDS resolutions are remembered for the session (#1126).
///
/// A room brings in as many DIDs as it has occupants plus whatever its
/// portals point at; 256 covers that many times over and bounds a peer set
/// that churns DIDs deliberately.
const MAX_PDS_CACHE_ENTRIES: usize = 256;

/// Session-lifetime DID → PDS endpoint cache, FIFO-bounded.
///
/// Every record fetch begins by resolving the owner's DID document, which
/// for a `did:plc` means a request to plc.directory — a third party, shared
/// by the whole network. Rigged-body resolution made that per *reference
/// set*, so a peer editing their outfit re-resolved the same DID on every
/// debounced keystroke, on every client in the room (#1126). Caching turns
/// the fan-out's DID hop into one request per DID per session, and it
/// helps every other caller — peer avatar fetches, room loads, portal hops
/// — for free.
///
/// **Only successes are remembered.** Caching a failure would pin a
/// transient DNS blip for the rest of the session, which is the opposite of
/// the degrade-and-recover behaviour every fetch path here is built on.
///
/// The cost is that a DID migrating to a new PDS mid-session keeps
/// resolving to the old one until the page is reloaded. Migrations are rare
/// and a reload is the natural remedy; an unbounded stream of requests to
/// somebody else's directory service is the worse of the two.
static PDS_CACHE: std::sync::LazyLock<std::sync::Mutex<PdsCache>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(PdsCache::default()));

/// The map plus its insert-order queue, in the same shape `PeerAvatarCache`
/// and `BskyProfileCache` use.
#[derive(Default)]
struct PdsCache {
    by_did: std::collections::HashMap<String, String>,
    order: std::collections::VecDeque<String>,
}

fn cached_pds(did: &str) -> Option<String> {
    // A poisoned lock degrades to a cache miss rather than a panic: the
    // worst case is the request we were already making.
    PDS_CACHE.lock().ok()?.by_did.get(did).cloned()
}

fn remember_pds(did: &str, endpoint: &str) {
    let Ok(mut cache) = PDS_CACHE.lock() else {
        return;
    };
    if cache
        .by_did
        .insert(did.to_string(), endpoint.to_string())
        .is_none()
    {
        cache.order.push_back(did.to_string());
    }
    while cache.order.len() > MAX_PDS_CACHE_ENTRIES {
        match cache.order.pop_front() {
            Some(oldest) => {
                cache.by_did.remove(&oldest);
            }
            None => break,
        }
    }
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

/// User-facing phrasing for a failed fetch (#1141).
///
/// Added because a surface that reports "could not load" has to say what
/// went wrong — a wardrobe listing that fails on an expired token and one
/// that fails on a 500 want different things from the owner, and until
/// this existed both rendered as nothing at all. `Debug` stays the shape
/// the `warn!` lines log; this is the half a person reads.
impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DidResolutionFailed => write!(f, "that identity's PDS could not be resolved"),
            Self::Network(detail) => write!(f, "network — {detail}"),
            Self::PdsError(status) => write!(f, "the PDS answered {status}"),
            Self::Decode(detail) => write!(f, "the response could not be read — {detail}"),
        }
    }
}

/// Error envelope returned by ATProto XRPC endpoints on non-2xx responses,
/// e.g. `{"error":"RecordNotFound","message":"Could not locate record..."}`.
#[derive(Deserialize)]
pub(crate) struct XrpcError {
    pub error: Option<String>,
    #[allow(dead_code)]
    pub message: Option<String>,
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

    /// The record this write addresses. Two writes with the same key in one
    /// batch are a contradiction whatever their verbs.
    pub(crate) fn key(&self) -> (&str, &str) {
        match self {
            Self::Create {
                collection, rkey, ..
            }
            | Self::Update {
                collection, rkey, ..
            }
            | Self::Delete { collection, rkey } => (collection.as_str(), rkey.as_str()),
        }
    }

    /// The lexicon verb, for error messages and the batch summary.
    pub(crate) fn verb(&self) -> &'static str {
        match self {
            Self::Create { .. } => "create",
            Self::Update { .. } => "update",
            Self::Delete { .. } => "delete",
        }
    }
}

/// A one-line description of a batch: how many of each verb, and the keys.
///
/// Attached to an `applyWrites` failure (#1185) because the PDS's answer to a
/// malformed batch is a bare `500 Internal Server Error` with no indication of
/// WHICH write it choked on — and until this existed, neither did ours. A
/// report of "save failed on one target and not the other" was unactionable:
/// the request that failed left no trace of its own shape.
///
/// Keys only, never values: a record body can be ~90 KiB and carries the
/// owner's content, neither of which belongs in a log line.
pub(crate) fn describe_batch(writes: &[RepoWrite]) -> String {
    let (mut creates, mut updates, mut deletes) = (0usize, 0usize, 0usize);
    for write in writes {
        match write {
            RepoWrite::Create { .. } => creates += 1,
            RepoWrite::Update { .. } => updates += 1,
            RepoWrite::Delete { .. } => deletes += 1,
        }
    }
    let keys: Vec<String> = writes
        .iter()
        .map(|w| {
            let (collection, rkey) = w.key();
            // The collection NSID's last segment is enough to tell the
            // wardrobe from an attachment from the avatar record.
            let short = collection.rsplit('.').next().unwrap_or(collection);
            format!("{} {short}/{rkey}", w.verb())
        })
        .collect();
    format!(
        "{} write(s) [{creates} create, {updates} update, {deletes} delete]: {}",
        writes.len(),
        keys.join(", ")
    )
}

/// The widest integer the AT Data Model can carry: `Number.MAX_SAFE_INTEGER`.
///
/// **Not `i64::MAX`, and the difference is the whole of #1186.** The spec
/// describes integers as signed 64-bit, and every atproto implementation in
/// the ecosystem parses records with JavaScript's `JSON.parse`, where a number
/// is an IEEE-754 double. Past 2^53 the parse is lossy — 8399497595966614310
/// comes back as 8399497595966615000 — and the reference encoder refuses to
/// store the result rather than write a value it cannot round-trip:
///
/// ```text
/// Non-integer numbers (8399497595966615000) are not supported by the AT Data Model
/// ```
///
/// That throw escapes the PDS's handler, so what the client sees is a bare
/// `500 Internal Server Error` with an empty message — no field named, no
/// indication the fault is its own. Measured against `@atproto/common`'s
/// `cborEncode`: 2^53-1 and -(2^53-1) encode, 2^53 and beyond throw.
pub(crate) const MAX_WIRE_INT: i64 = 9_007_199_254_740_991;

/// Every integer in `value` that the PDS cannot store, as `(path, value)`.
///
/// Walks the record rather than trusting the types that built it, because the
/// hazard is spread across records nothing relates: `seed` is `u64` on the
/// terrain config, on two generator shapes and on an avatar part, and any of
/// them past [`MAX_WIRE_INT`] is the same opaque 500.
fn unstorable_ints(value: &serde_json::Value, path: &str, out: &mut Vec<(String, i128)>) {
    match value {
        serde_json::Value::Number(n) => {
            let magnitude = n
                .as_i64()
                .map(i128::from)
                .or_else(|| n.as_u64().map(i128::from));
            if let Some(v) = magnitude
                && (v > i128::from(MAX_WIRE_INT) || v < -i128::from(MAX_WIRE_INT))
            {
                out.push((path.to_string(), v));
            }
        }
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                unstorable_ints(child, &format!("{path}.{key}"), out);
            }
        }
        serde_json::Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                unstorable_ints(child, &format!("{path}[{index}]"), out);
            }
        }
        _ => {}
    }
}

/// Refuse a record carrying an integer the PDS cannot store (#1186).
///
/// Named here so the failure says WHICH FIELD. The PDS's answer is a bare 500
/// that names nothing, and #1186 was invisible for exactly that reason: the
/// rigged-avatar publish path has never worked for any identity — the wardrobe
/// record's `seed` is drawn from a full-width `u64` and clears 2^53 with
/// probability 1 - 2^-11 — and the only symptom was a save that failed with an
/// error indistinguishable from the PDS being down.
pub(crate) fn preflight_wire_ints(value: &serde_json::Value, label: &str) -> Result<(), String> {
    let mut bad = Vec::new();
    unstorable_ints(value, "", &mut bad);
    if bad.is_empty() {
        return Ok(());
    }
    let named: Vec<String> = bad
        .iter()
        .take(4)
        .map(|(path, v)| format!("{}{path} = {v}", label))
        .collect();
    Err(format!(
        "{} carries {} integer(s) past ±{MAX_WIRE_INT}, which the PDS cannot store \
         (it answers with an opaque 500): {}",
        label,
        bad.len(),
        named.join(", ")
    ))
}

/// Refuse a batch that addresses the same record twice (#1185).
///
/// **The PDS answers a malformed batch with a 500, not a 400.** The reference
/// implementation validates each write against its lexicon and then hands the
/// batch to the repo layer, where a second operation on a key already touched
/// in the same commit throws out of the MST — an exception, not an XRPC error,
/// so it surfaces as a bare `InternalServerError`. That is indistinguishable
/// from the PDS having a bad day, and it is what a user sees: "Save failed:
/// 500", with no way to tell that their own client sent a contradiction.
///
/// Keyed on `(collection, rkey)` regardless of verb, because every pairing is
/// wrong: two creates race, a create beside a delete cannot both be intended,
/// and an update beside a delete is a plan that has not decided.
pub(crate) fn validate_batch(writes: &[RepoWrite]) -> Result<(), String> {
    let mut seen: std::collections::HashMap<(&str, &str), &'static str> =
        std::collections::HashMap::new();
    for write in writes {
        if let Some(first) = seen.insert(write.key(), write.verb()) {
            let (collection, rkey) = write.key();
            return Err(format!(
                "applyWrites batch addresses {collection}/{rkey} twice ({first} then {}) — \
                 the PDS commits a batch as one MST transaction and answers a repeated key \
                 with an opaque 500, so it is refused here instead",
                write.verb()
            ));
        }
    }
    Ok(())
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

/// `true` when `collection/rkey` exists in `did`'s repo, `false` when the
/// PDS says it does not.
///
/// The create-vs-update probe every `applyWrites` plan needs: the two
/// operations are distinct verbs, and picking the wrong one fails the whole
/// atomic batch. `room_self_exists` is this same probe, specialised to the
/// room manifest and written first (#697); the avatar bundle needs it four
/// times over (#1117), so it lives here.
///
/// ATProto signals "no such record" as `400` with `RecordNotFound` in the
/// body, NOT as `404`, so both have to be read as absence. Anything else is
/// an error rather than a guess — guessing `false` would turn a transient
/// blip into an `#create` over a record that exists.
pub(crate) async fn record_exists(
    client: &reqwest::Client,
    pds: &str,
    did: &str,
    collection: &str,
    rkey: &str,
) -> Result<bool, String> {
    let url = format!(
        "{pds}/xrpc/com.atproto.repo.getRecord?repo={did}&collection={collection}&rkey={rkey}"
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("existence check ({collection}/{rkey}): {e}"))?;
    let status = resp.status();
    if status.is_success() {
        return Ok(true);
    }
    if status.as_u16() == 404 {
        return Ok(false);
    }
    let body = read_capped_text(resp).await;
    if body.contains("RecordNotFound") {
        return Ok(false);
    }
    Err(format!(
        "existence check ({collection}/{rkey}) failed: {status} — {body}"
    ))
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
    // Every publish path in the app funnels through here, so this is the one
    // place a contradictory batch can be caught before it becomes a 500 the
    // user cannot act on (#1185).
    validate_batch(&writes)?;
    // And refuse a record the PDS would choke on rather than reject (#1186).
    // Here rather than in each planner because the hazard is spread across
    // record types that share nothing but this wire.
    for write in &writes {
        match write {
            RepoWrite::Create {
                collection,
                rkey,
                value,
            }
            | RepoWrite::Update {
                collection,
                rkey,
                value,
            } => preflight_wire_ints(value, &format!("{collection}/{rkey}"))?,
            RepoWrite::Delete { .. } => {}
        }
    }
    let url = format!("{}/xrpc/com.atproto.repo.applyWrites", pds);
    let shape = describe_batch(&writes);
    let body = serde_json::json!({ "repo": session.did, "writes": writes });
    let (status, body) =
        crate::oauth::oauth_post_with_refresh(&session.session, refresh, &url, &body).await?;
    if status.is_success() {
        Ok(())
    } else {
        // The batch's shape rides the error. A 500 from the reference PDS
        // carries no indication of which write it choked on, so without this
        // the only evidence a bug report can offer is that a save failed.
        Err(format!(
            "applyWrites failed: {status} — {body} (batch: {shape})"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A contradictory batch is refused here, not by the PDS** (#1185).
    ///
    /// The reference implementation validates each write against its lexicon
    /// and then hands the batch to the repo layer, where a second operation on
    /// a key already touched in the same commit throws out of the MST. That is
    /// an exception rather than an XRPC error, so it comes back as a bare
    /// `500 Internal Server Error` with an empty message — indistinguishable
    /// from the PDS being down, and impossible for a user to act on. Every
    /// pairing is wrong, so the check is keyed on the record and ignores the
    /// verbs: two creates race, a write beside a delete cannot both be meant.
    #[test]
    fn a_batch_that_addresses_one_record_twice_is_refused_locally() {
        let value = serde_json::json!({});
        let create = |rkey: &str| RepoWrite::Create {
            collection: String::from("network.symbios.test"),
            rkey: rkey.into(),
            value: value.clone(),
        };
        let delete = |rkey: &str| RepoWrite::Delete {
            collection: String::from("network.symbios.test"),
            rkey: rkey.into(),
        };

        validate_batch(&[create("a"), create("b"), delete("c")]).expect("distinct keys are fine");

        let err = validate_batch(&[create("a"), create("a")])
            .expect_err("the same key created twice is a contradiction");
        assert!(err.contains("twice"), "{err}");
        assert!(err.contains("network.symbios.test/a"), "{err}");

        let err = validate_batch(&[create("a"), delete("a")])
            .expect_err("written and deleted in one commit is a plan that has not decided");
        assert!(err.contains("create then delete"), "{err}");

        // Same rkey in a DIFFERENT collection is a different record and must
        // stay legal — the avatar bundle writes `self` in three of them.
        validate_batch(&[
            create("self"),
            RepoWrite::Delete {
                collection: String::from("network.symbios.other"),
                rkey: String::from("self"),
            },
        ])
        .expect("one rkey in two collections is two records");
    }

    /// The failure message has to identify the request (#1185). A 500 from the
    /// PDS names nothing, so without this a report of "save failed" carries no
    /// evidence of what was sent — which is exactly why the wasm-only failure
    /// this issue came from could not be reproduced from the report alone.
    /// Keys only: a record body is up to ~90 KiB of the owner's content.
    #[test]
    fn a_batch_description_names_the_verbs_and_keys_but_not_the_values() {
        let shape = describe_batch(&[
            RepoWrite::Create {
                collection: String::from("network.symbios.avatar.avatar"),
                rkey: String::from("3labc"),
                value: serde_json::json!({ "secret": "do not log me" }),
            },
            RepoWrite::Delete {
                collection: String::from("network.symbios.overlands.avatar.attachment"),
                rkey: String::from("3lxyz"),
            },
        ]);
        assert!(shape.contains("2 write(s)"), "{shape}");
        assert!(shape.contains("1 create, 0 update, 1 delete"), "{shape}");
        assert!(shape.contains("create avatar/3labc"), "{shape}");
        assert!(shape.contains("delete attachment/3lxyz"), "{shape}");
        assert!(
            !shape.contains("do not log me"),
            "a record value must never reach a log line: {shape}"
        );
    }

    /// **#1186.** The PDS answers an unstorable integer with a bare 500, so
    /// this is the only place the field can be named. Measured against
    /// `@atproto/common`'s `cborEncode`: 2^53-1 encodes, 2^53 throws
    /// "Non-integer numbers ... are not supported by the AT Data Model" —
    /// because `JSON.parse` has already turned it into a different number by
    /// the time the encoder sees it.
    #[test]
    fn an_integer_past_the_wire_ceiling_is_named_rather_than_500ed() {
        // The exact value from the failing save, and its neighbours at the
        // boundary the reference encoder actually enforces.
        let record = serde_json::json!({
            "$type": "network.symbios.avatar.avatar",
            "seed": 8_399_497_595_966_614_310u64,
        });
        let err = preflight_wire_ints(&record, "wardrobe")
            .expect_err("a full-width seed is not storable");
        assert!(err.contains(".seed"), "the field must be named: {err}");
        assert!(err.contains("8399497595966614310"), "{err}");

        preflight_wire_ints(
            &serde_json::json!({ "seed": MAX_WIRE_INT }),
            "at the ceiling",
        )
        .expect("2^53-1 is the largest the PDS stores");
        preflight_wire_ints(
            &serde_json::json!({ "seed": -MAX_WIRE_INT }),
            "at the floor",
        )
        .expect("and its negative");
        preflight_wire_ints(&serde_json::json!({ "seed": MAX_WIRE_INT + 1 }), "one past")
            .expect_err("one past the ceiling is not");
    }

    /// The walk has to reach a value wherever it is, because the hazard is
    /// spread across records that share nothing: `seed` is a `u64` on the
    /// terrain config, on two generator shapes and on an avatar part.
    #[test]
    fn the_wire_int_walk_reaches_nested_and_arrayed_values() {
        let deep = serde_json::json!({
            "environment": {
                "generators": [
                    { "name": "ok", "seed": 12_345 },
                    { "name": "bad", "seed": 9_007_199_254_740_992i64 },
                ]
            }
        });
        let err = preflight_wire_ints(&deep, "room").expect_err("the nested seed is unstorable");
        assert!(
            err.contains(".environment.generators[1].seed"),
            "the path must locate it: {err}"
        );
        assert!(
            !err.contains("[0]"),
            "the safe sibling is not reported: {err}"
        );
    }

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

/// #1126: the DID → PDS cache is bounded and never remembers a failure.
///
/// A unit test rather than a network one: what needs pinning is the
/// eviction policy and the fact that only successes are stored, neither of
/// which involves a socket. `nextest` runs each test in its own process, so
/// the process-wide cache is not shared between them.
#[cfg(test)]
mod pds_cache_tests {
    use super::*;

    #[test]
    fn a_remembered_endpoint_is_returned_without_a_lookup() {
        assert_eq!(cached_pds("did:plc:absent"), None, "a cold cache misses");
        remember_pds("did:plc:known", "https://pds.example");
        assert_eq!(
            cached_pds("did:plc:known").as_deref(),
            Some("https://pds.example")
        );
    }

    /// A peer set that churns DIDs must not grow the cache without limit —
    /// the same shape the peer avatar cache is bounded against.
    #[test]
    fn the_cache_evicts_oldest_first_past_its_bound() {
        for i in 0..(MAX_PDS_CACHE_ENTRIES + 10) {
            remember_pds(&format!("did:plc:{i}"), &format!("https://pds{i}.example"));
        }
        assert_eq!(
            cached_pds("did:plc:0"),
            None,
            "the first inserted is the first evicted"
        );
        let newest = MAX_PDS_CACHE_ENTRIES + 9;
        assert!(
            cached_pds(&format!("did:plc:{newest}")).is_some(),
            "the most recent survives"
        );
    }

    /// Re-remembering a DID must not push a second entry into the eviction
    /// queue, or repeated resolutions of one DID would evict everything
    /// else while the map itself stayed small.
    #[test]
    fn re_remembering_a_did_does_not_grow_the_eviction_queue() {
        for _ in 0..(MAX_PDS_CACHE_ENTRIES * 2) {
            remember_pds("did:plc:repeat", "https://pds.example");
        }
        remember_pds("did:plc:other", "https://other.example");
        assert!(
            cached_pds("did:plc:repeat").is_some() && cached_pds("did:plc:other").is_some(),
            "one DID resolved many times occupies one slot"
        );
    }
}

/// #1124: the error-body cap, exercised against a real socket.
///
/// A mock-server crate would be a new dependency for three tests; a
/// `TcpListener` on an ephemeral port is enough to serve exactly the two
/// shapes that matter — a server that declares an oversized body, and one
/// that declares nothing and just keeps writing.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod error_body_cap_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Serve one HTTP 400 with `body`, then close. When `declare_length`
    /// is false the response omits `Content-Length` and relies on
    /// connection-close framing, which is how a hostile PDS defeats the
    /// cheap pre-check and forces the streaming cap to do the work.
    fn serve_once(body: Vec<u8>, declare_length: bool) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        std::thread::spawn(move || {
            let Ok((mut sock, _)) = listener.accept() else {
                return;
            };
            // Read the request head so the client never sees a reset.
            let mut scratch = [0u8; 4096];
            let _ = sock.read(&mut scratch);
            let mut head = String::from("HTTP/1.1 400 Bad Request\r\n");
            head.push_str("Content-Type: application/json\r\n");
            if declare_length {
                head.push_str(&format!("Content-Length: {}\r\n", body.len()));
            } else {
                head.push_str("Connection: close\r\n");
            }
            head.push_str("\r\n");
            if sock.write_all(head.as_bytes()).is_ok() {
                let _ = sock.write_all(&body);
            }
            let _ = sock.flush();
        });
        format!("http://{addr}/")
    }

    fn read_error_body(url: &str) -> String {
        crate::config::http::block_on(async {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("client");
            let resp = client.get(url).send().await.expect("response");
            assert!(!resp.status().is_success(), "the fixture serves a 400");
            read_capped_text(resp).await
        })
    }

    /// The honest-but-huge server: `Content-Length` says the body is over
    /// the cap, so it is refused before a single byte is buffered.
    #[test]
    fn a_declared_oversized_error_body_is_refused() {
        let url = serve_once(vec![b'x'; MAX_ERROR_BODY_BYTES + 4096], true);
        assert!(
            read_error_body(&url).is_empty(),
            "an oversized error body must not reach the caller"
        );
    }

    /// The interesting one. No `Content-Length`, so the pre-check cannot
    /// fire and the streaming cap is the only thing standing between a
    /// hostile PDS and an unbounded allocation. This is the shape the
    /// original defence missed: `decode_record_json` guarded the success
    /// path, and answering with any non-2xx status walked straight past it.
    #[test]
    fn an_undeclared_oversized_error_body_is_still_capped() {
        let url = serve_once(vec![b'x'; MAX_ERROR_BODY_BYTES + 4096], false);
        assert!(
            read_error_body(&url).is_empty(),
            "the streaming cap must stop a body that declares no length"
        );
    }

    /// The control, and the reason the cap is 64 KiB rather than something
    /// tight: a real XRPC error envelope must still arrive intact, or every
    /// "no record yet" becomes a spurious `PdsError` and the client stops
    /// substituting the seeded default.
    #[test]
    fn a_real_error_envelope_still_reaches_the_caller() {
        let envelope = br#"{"error":"RecordNotFound","message":"Could not locate record"}"#;
        for declare_length in [true, false] {
            let url = serve_once(envelope.to_vec(), declare_length);
            let body = read_error_body(&url);
            assert!(
                body.contains("RecordNotFound"),
                "the marker every caller branches on must survive \
                 (declare_length={declare_length})"
            );
            let parsed: XrpcError = serde_json::from_str(&body).expect("still valid JSON");
            assert_eq!(parsed.error.as_deref(), Some("RecordNotFound"));
        }
    }
}
