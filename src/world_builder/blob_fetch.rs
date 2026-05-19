//! Shared capped HTTPS / ATProto-blob byte fetch.
//!
//! Extracted from [`super::image_cache`] (#262) so the Phase-4 audio
//! cue cache ([`crate::interaction::audio`]) reuses the exact same
//! battle-tested capped-streaming fetch instead of duplicating the
//! wasm/native split and the OOM guard. The only per-caller knobs are
//! the byte cap (`max_bytes`) and a short `ctx` label used in warn
//! logs so a typo'd URL is debuggable without the asset silently going
//! missing.
//!
//! `IoTaskPool` is the right home for these blocking ATProto HTTP
//! fetches; pinning a compute worker on a socket read would stall
//! procedural terrain / texture generation.

use bevy::prelude::*;

/// Direct HTTPS GET, body streamed and capped at `max_bytes`. Returns
/// `None` (logged at warn) on connection error, non-success status,
/// oversized body, or read failure. A hostile URL (an infinite stream
/// like `/dev/zero` over HTTP, or a multi-gigabyte asset) would
/// otherwise pull the whole response into memory and OOM every guest.
pub(crate) async fn fetch_url_bytes(
    client: &reqwest::Client,
    url: &str,
    max_bytes: usize,
    ctx: &str,
) -> Option<Vec<u8>> {
    let resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(e) => {
            warn!("{ctx} URL fetch failed for {url}: {e}");
            return None;
        }
    };
    if !resp.status().is_success() {
        warn!("{ctx} URL fetch returned {} for {url}", resp.status());
        return None;
    }
    // Pre-flight: if the server advertises a length already over the
    // cap, don't even start streaming.
    if let Some(len) = resp.content_length()
        && len as usize > max_bytes
    {
        warn!("{ctx} body too large: Content-Length {len} exceeds {max_bytes} for {url}");
        return None;
    }
    read_capped_body(resp, url, max_bytes, ctx).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn read_capped_body(
    mut resp: reqwest::Response,
    url: &str,
    max_bytes: usize,
    ctx: &str,
) -> Option<Vec<u8>> {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        match resp.chunk().await {
            Ok(Some(chunk)) => {
                if buf.len().saturating_add(chunk.len()) > max_bytes {
                    warn!("{ctx} body exceeded cap of {max_bytes} bytes mid-stream for {url}");
                    return None;
                }
                buf.extend_from_slice(&chunk);
            }
            Ok(None) => return Some(buf),
            Err(e) => {
                warn!("{ctx} body read failed for {url}: {e}");
                return None;
            }
        }
    }
}

// On WASM the browser fetch API has already buffered the body by the
// time reqwest hands back the `Response`; `chunk()` isn't exposed and
// mid-stream cancellation isn't possible. The `Content-Length`
// pre-check in `fetch_url_bytes` already rejects the obvious case;
// this post-check catches servers that lie about / omit the header.
#[cfg(target_arch = "wasm32")]
async fn read_capped_body(
    resp: reqwest::Response,
    url: &str,
    max_bytes: usize,
    ctx: &str,
) -> Option<Vec<u8>> {
    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            warn!("{ctx} body read failed for {url}: {e}");
            return None;
        }
    };
    if bytes.len() > max_bytes {
        warn!("{ctx} body exceeded cap of {max_bytes} bytes (post-fetch) for {url}");
        return None;
    }
    Some(bytes.to_vec())
}

/// ATProto blob fetch via `com.atproto.sync.getBlob`. Resolves the
/// DID's PDS first, then GETs the blob endpoint (capped, same as
/// [`fetch_url_bytes`]).
pub(crate) async fn fetch_blob_bytes(
    client: &reqwest::Client,
    did: &str,
    cid: &str,
    max_bytes: usize,
    ctx: &str,
) -> Option<Vec<u8>> {
    let pds = match crate::pds::resolve_pds(client, did).await {
        Some(p) => p,
        None => {
            warn!("{ctx} DID {did} did not resolve to a PDS");
            return None;
        }
    };
    let blob_url = format!("{pds}/xrpc/com.atproto.sync.getBlob?did={did}&cid={cid}");
    fetch_url_bytes(client, &blob_url, max_bytes, ctx).await
}
