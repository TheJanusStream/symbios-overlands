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

/// Largest number of pixels a fetched image may declare before it is
/// refused without being decoded.
///
/// The byte caps above bound the *compressed* transfer, but a "pixel bomb" —
/// a kilobyte-sized PNG declaring e.g. 30000×30000 of uniform colour —
/// expands by orders of magnitude on decode and can OOM the wasm heap in one
/// allocation. Pixels are the unit that actually costs memory: this bound is
/// 4096×4096, exactly the worst case the old per-axis cap of 4096 permitted,
/// so nothing that decoded before is refused now.
///
/// **Why pixels and not axes (#1130).** A per-axis cap turned away shapes that
/// cost *less* than an accepted square — a 6000×1200 panorama is 7 MP against
/// a 4096-square's 16.7 MP, yet only the panorama was refused. That mattered
/// because the wasm profile-picture path fetches the owner's ORIGINAL upload
/// from their PDS (cdn.bsky.app serves no CORS headers, so the resized CDN
/// copy the native path uses is unreachable): a peer whose avatar happened to
/// be a wide or tall photograph rendered normally for native peers and as a
/// permanent blank spacer for everyone on the web build. Same ceiling on
/// memory, fewer people missing from the room.
pub(crate) const MAX_IMAGE_PIXELS: u64 = 4096 * 4096;

/// Per-axis sanity bound, held well above anything a real source produces.
///
/// [`MAX_IMAGE_PIXELS`] is the memory bound; this only refuses degenerate
/// aspect ratios (a 1×16777216 strip is inside the pixel budget and is not an
/// image anybody uploaded), where decoder row-handling is least exercised.
pub(crate) const MAX_IMAGE_AXIS: u32 = 16384;

/// Decode fetched image bytes after a header-only dimension probe, then
/// downscale to fit `working_max` on both axes.
///
/// Returns `None` (logged at warn, tagged with `ctx`) when the format can't be
/// sniffed or the declared frame exceeds [`MAX_IMAGE_PIXELS`] /
/// [`MAX_IMAGE_AXIS`] — the full-frame allocation never happens for a rejected
/// image. All decode paths for network-supplied image bytes (peer avatars,
/// sign sources, Referenced splat layers) must come through here rather than
/// calling `image::load_from_memory` directly.
///
/// **What `working_max` does and does not bound (#1128).** It bounds what is
/// *retained*: the returned image, whatever the caller does with it, and the
/// GPU texture it becomes. It does NOT bound the decode itself, which still
/// materialises the source frame at full size — `image` exposes no
/// general decode-at-reduced-scale, so the only bound available on the spike
/// is [`MAX_IMAGE_PIXELS`]. Every caller must name a working size rather than
/// defaulting to "as large as the cap allows", because on wasm the retained
/// half is permanent and the transient half is merely a high-water mark.
pub(crate) fn decode_image_capped(
    bytes: &[u8],
    ctx: &str,
    working_max: u32,
) -> Option<image::DynamicImage> {
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
    if w == 0 || h == 0 || w > MAX_IMAGE_AXIS || h > MAX_IMAGE_AXIS {
        warn!("{ctx} image rejected: {w}×{h} px is outside the {MAX_IMAGE_AXIS} px axis bound");
        return None;
    }
    if u64::from(w) * u64::from(h) > MAX_IMAGE_PIXELS {
        warn!(
            "{ctx} image rejected: {w}×{h} px is {} pixels, over the {MAX_IMAGE_PIXELS}-pixel cap",
            u64::from(w) * u64::from(h)
        );
        return None;
    }
    let img = match image::load_from_memory(bytes) {
        Ok(img) => img,
        Err(e) => {
            warn!("{ctx} image decode failed: {e}");
            return None;
        }
    };
    Some(downscale_to_fit(img, working_max))
}

/// Shrink `img` so neither axis exceeds `working_max`, preserving aspect
/// ratio. Returns it untouched when it already fits — the common case, and
/// one that must not pay a resample.
fn downscale_to_fit(img: image::DynamicImage, working_max: u32) -> image::DynamicImage {
    if img.width() <= working_max && img.height() <= working_max {
        return img;
    }
    // `resize` fits inside the box and keeps the aspect ratio, so a 4096×1024
    // sign resolved against a 2048 box lands at 2048×512 rather than being
    // squashed square. Triangle is the same filter the avatar and splat paths
    // already use.
    img.resize(
        working_max,
        working_max,
        image::imageops::FilterType::Triangle,
    )
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

    /// A working box big enough never to resample, for the tests that are
    /// about acceptance rather than about downscaling.
    const NO_SHRINK: u32 = MAX_IMAGE_AXIS;

    #[test]
    fn rejects_pixel_bomb_before_decode() {
        // ~3.4 GiB decoded from under 100 bytes on the wire. The
        // dimension probe must reject it without attempting the
        // allocation.
        let bomb = png_declaring(30_000, 30_000);
        assert!(bomb.len() < 100, "bomb should be tiny on the wire");
        assert!(decode_image_capped(&bomb, "test", NO_SHRINK).is_none());
    }

    /// A strip inside the pixel budget but absurd in shape is still refused:
    /// [`MAX_IMAGE_PIXELS`] is the memory bound, [`MAX_IMAGE_AXIS`] the
    /// shape one, and dropping the second along with the per-axis cap would
    /// have handed the decoders a row length nothing legitimate produces.
    #[test]
    fn rejects_a_degenerate_strip_even_inside_the_pixel_budget() {
        let strip = png_declaring(MAX_IMAGE_AXIS + 1, 4);
        assert!(
            u64::from(MAX_IMAGE_AXIS + 1) * 4 < MAX_IMAGE_PIXELS,
            "the strip must be inside the pixel budget for this test to mean anything"
        );
        assert!(decode_image_capped(&strip, "test", NO_SHRINK).is_none());
        assert!(
            decode_image_capped(&png_declaring(4, MAX_IMAGE_AXIS + 1), "test", NO_SHRINK).is_none()
        );
    }

    /// The #1130 sequence, stated as a policy test.
    ///
    /// A peer's avatar blob is 6000×1200 — a panorama, 7.2 MP, well under half
    /// what a 4096-square costs to decode. The old per-axis cap refused it on
    /// its width alone, so that peer had a profile picture on native (which
    /// fetches the resized CDN copy) and a permanent blank spacer on wasm
    /// (which must fetch the original from the PDS, because cdn.bsky.app
    /// serves no CORS headers). Same person, two different rooms, depending
    /// on which build you joined from.
    #[test]
    fn a_wide_image_inside_the_pixel_budget_is_accepted() {
        const WIDE_PIXELS: u64 = 6000 * 1200;
        const _: () = assert!(
            WIDE_PIXELS < MAX_IMAGE_PIXELS,
            "a 6000×1200 panorama is inside the pixel budget"
        );
        let wide = png_declaring(6000, 1200);
        // The header-only fixture has no pixel data, so the decode itself
        // fails — but it must fail at the DECODER, having passed the policy
        // gate, which is what the old per-axis cap denied it.
        let (w, h) = image::ImageReader::new(std::io::Cursor::new(&wide))
            .with_guessed_format()
            .expect("probe")
            .into_dimensions()
            .expect("dimensions");
        assert_eq!((w, h), (6000, 1200));
        assert!(
            u64::from(w) * u64::from(h) <= MAX_IMAGE_PIXELS && w <= MAX_IMAGE_AXIS,
            "6000×1200 must pass the policy gate — this is the shape #1130 blanked"
        );
    }

    /// And the ceiling has not moved: the worst case a decode may attempt is
    /// still exactly a 4096-square, which is what the old per-axis cap
    /// permitted. The point of #1130 was to stop refusing images that cost
    /// LESS than that, not to start accepting ones that cost more.
    #[test]
    fn the_worst_case_decode_is_still_a_4096_square() {
        assert_eq!(MAX_IMAGE_PIXELS, 4096 * 4096);
        assert!(decode_image_capped(&png_declaring(4097, 4097), "test", NO_SHRINK).is_none());
    }

    #[test]
    fn accepts_small_real_image() {
        let img = image::DynamicImage::new_rgba8(4, 4);
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        let decoded = decode_image_capped(&buf, "test", NO_SHRINK).expect("4×4 PNG must decode");
        assert_eq!((decoded.width(), decoded.height()), (4, 4));
    }

    /// The retention half of #1128: what comes back is the working size, not
    /// the source size. A sign whose source is 512-square resolved against a
    /// 64 px box must land at 64 px — otherwise the cache's byte budget is
    /// measuring a number the caller never controls.
    #[test]
    fn a_source_larger_than_the_working_box_comes_back_shrunk() {
        let img = image::DynamicImage::new_rgba8(512, 256);
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        let decoded = decode_image_capped(&buf, "test", 64).expect("512×256 PNG must decode");
        assert_eq!(
            (decoded.width(), decoded.height()),
            (64, 32),
            "the aspect ratio must survive the shrink — a squashed sign is a \
             visible bug, and `resize` fits the box rather than filling it"
        );
    }

    /// And an image already inside the box is handed back untouched, so the
    /// overwhelmingly common case pays no resample.
    #[test]
    fn a_source_inside_the_working_box_is_not_resampled() {
        let img = image::DynamicImage::new_rgba8(32, 16);
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        let decoded = decode_image_capped(&buf, "test", 64).expect("32×16 PNG must decode");
        assert_eq!((decoded.width(), decoded.height()), (32, 16));
    }

    /// The blob policy, stated declaratively (#1153).
    ///
    /// `decode_image_capped` sniffs the format from magic bytes, so every
    /// decoder compiled into the binary is reachable from an untrusted PDS
    /// blob or peer avatar — and the dimension cap above bounds a pixel
    /// bomb, not decoder complexity. EXR, TIFF and GIF have each had panic
    /// and pathological-allocation CVEs; nothing in this app has ever asked
    /// to read one.
    ///
    /// Asserting on `reading_enabled` rather than on decode results is
    /// deliberate: it fails the day someone widens the `image` feature list
    /// in Cargo.toml, which is the change that would silently reopen this,
    /// and it says which formats are policy rather than leaving a reader to
    /// infer it from a list of rejected fixtures.
    #[test]
    fn only_the_three_policy_formats_are_compiled_in() {
        use image::ImageFormat;
        for allowed in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::WebP] {
            assert!(
                allowed.reading_enabled(),
                "{allowed:?} is required by a live path (PDS blobs, bsky CDN PFPs)"
            );
        }
        for denied in [
            ImageFormat::Tiff,
            ImageFormat::OpenExr,
            ImageFormat::Gif,
            ImageFormat::Bmp,
            ImageFormat::Ico,
            ImageFormat::Tga,
            ImageFormat::Pnm,
            ImageFormat::Qoi,
            ImageFormat::Dds,
            ImageFormat::Farbfeld,
            ImageFormat::Avif,
        ] {
            assert!(
                !denied.reading_enabled(),
                "{denied:?} is compiled in and reachable from hostile bytes — \
                 widen the image feature list only with a reason"
            );
        }
    }

    /// And the behavioural half: bytes announcing an excluded format are
    /// turned away by `decode_image_capped` rather than reaching a decoder.
    #[test]
    fn headers_for_excluded_formats_are_turned_away() {
        // Magic bytes only — a real decoder would need far more, which is
        // the point: rejection must happen before anything parses these.
        let tiff_le = b"II\x2a\x00\x08\x00\x00\x00".to_vec();
        let tiff_be = b"MM\x00\x2a\x00\x00\x00\x08".to_vec();
        let exr = vec![0x76, 0x2f, 0x31, 0x01, 0x02, 0x00, 0x00, 0x00];
        let gif = b"GIF89a\x01\x00\x01\x00".to_vec();
        let bmp = b"BM\x46\x00\x00\x00\x00\x00".to_vec();
        for (name, bytes) in [
            ("tiff-le", tiff_le),
            ("tiff-be", tiff_be),
            ("exr", exr),
            ("gif", gif),
            ("bmp", bmp),
        ] {
            assert!(
                decode_image_capped(&bytes, "test", NO_SHRINK).is_none(),
                "{name} must not decode"
            );
        }
    }

    /// The control the test above needs: the policy formats really do still
    /// decode, so the subtraction did not quietly break peer avatars.
    /// PNG and JPEG round-trip through the crate's own encoders; `image`
    /// 0.25 ships WebP as decode-only, so its half is covered by
    /// `only_the_three_policy_formats_are_compiled_in`.
    #[test]
    fn the_allowed_formats_still_decode() {
        for format in [image::ImageFormat::Png, image::ImageFormat::Jpeg] {
            // JPEG has no alpha, so encode from RGB8 for both.
            let img = image::DynamicImage::new_rgb8(4, 4);
            let mut buf = Vec::new();
            img.write_to(&mut std::io::Cursor::new(&mut buf), format)
                .unwrap_or_else(|e| panic!("{format:?} must encode: {e}"));
            let decoded = decode_image_capped(&buf, "test", NO_SHRINK)
                .unwrap_or_else(|| panic!("{format:?} must decode"));
            assert_eq!((decoded.width(), decoded.height()), (4, 4));
        }
    }

    #[test]
    fn rejects_garbage_bytes() {
        assert!(decode_image_capped(&[0u8; 16], "test", NO_SHRINK).is_none());
        assert!(decode_image_capped(&[], "test", NO_SHRINK).is_none());
    }
}
