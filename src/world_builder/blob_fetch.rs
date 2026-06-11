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

/// Largest per-axis pixel dimension accepted from a fetched image.
///
/// The byte caps above bound the *compressed* transfer, but a "pixel
/// bomb" — a kilobyte-sized PNG declaring e.g. 30000×30000 of uniform
/// colour — expands by orders of magnitude on decode and can OOM the
/// WASM heap in one allocation. Capping each axis bounds the decoded
/// frame to 4096×4096×4 B ≈ 64 MiB worst-case, generous for every
/// legitimate avatar / sign / splat source.
pub(crate) const MAX_IMAGE_DIMENSION: u32 = 4096;

/// Decode fetched image bytes after a header-only dimension probe.
///
/// Returns `None` (logged at warn, tagged with `ctx`) when the format
/// can't be sniffed or either axis exceeds [`MAX_IMAGE_DIMENSION`] —
/// the full-frame allocation never happens for a rejected image. All
/// decode paths for network-supplied image bytes (peer avatars, sign
/// sources, Referenced splat layers) must come through here rather
/// than calling `image::load_from_memory` directly.
pub(crate) fn decode_image_capped(bytes: &[u8], ctx: &str) -> Option<image::DynamicImage> {
    let reader = match image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format() {
        Ok(reader) => reader,
        Err(e) => {
            warn!("{ctx} image format probe failed: {e}");
            return None;
        }
    };
    let (w, h) = match reader.into_dimensions() {
        Ok(dims) => dims,
        Err(e) => {
            warn!("{ctx} image dimension probe failed: {e}");
            return None;
        }
    };
    if w == 0 || h == 0 || w > MAX_IMAGE_DIMENSION || h > MAX_IMAGE_DIMENSION {
        warn!("{ctx} image rejected: {w}×{h} px exceeds the {MAX_IMAGE_DIMENSION} px per-axis cap");
        return None;
    }
    match image::load_from_memory(bytes) {
        Ok(img) => Some(img),
        Err(e) => {
            warn!("{ctx} image decode failed: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PNG IEEE CRC-32 (reflected, poly 0xEDB88320) — enough to build a
    /// syntactically valid header chunk without pulling in a crc crate.
    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    fn png_chunk(ty: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(ty);
        out.extend_from_slice(data);
        let mut crc_input = ty.to_vec();
        crc_input.extend_from_slice(data);
        out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
        out
    }

    /// A header-only PNG declaring `w × h` 8-bit RGBA — the shape of a
    /// "pixel bomb": tiny on the wire, enormous after decode.
    fn png_declaring(w: u32, h: u32) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&w.to_be_bytes());
        ihdr.extend_from_slice(&h.to_be_bytes());
        // bit depth 8, colour type 6 (RGBA), deflate, std filter, no interlace
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes.extend(png_chunk(b"IHDR", &ihdr));
        bytes.extend(png_chunk(b"IDAT", &[]));
        bytes
    }

    #[test]
    fn rejects_pixel_bomb_before_decode() {
        // ~3.4 GiB decoded from under 100 bytes on the wire. The
        // dimension probe must reject it without attempting the
        // allocation.
        let bomb = png_declaring(30_000, 30_000);
        assert!(bomb.len() < 100, "bomb should be tiny on the wire");
        assert!(decode_image_capped(&bomb, "test").is_none());
    }

    #[test]
    fn rejects_single_oversized_axis() {
        assert!(decode_image_capped(&png_declaring(1, MAX_IMAGE_DIMENSION + 1), "test").is_none());
        assert!(decode_image_capped(&png_declaring(MAX_IMAGE_DIMENSION + 1, 1), "test").is_none());
    }

    #[test]
    fn accepts_small_real_image() {
        let img = image::DynamicImage::new_rgba8(4, 4);
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        let decoded = decode_image_capped(&buf, "test").expect("4×4 PNG must decode");
        assert_eq!((decoded.width(), decoded.height()), (4, 4));
    }

    #[test]
    fn rejects_garbage_bytes() {
        assert!(decode_image_capped(&[0u8; 16], "test").is_none());
        assert!(decode_image_capped(&[], "test").is_none());
    }
}
