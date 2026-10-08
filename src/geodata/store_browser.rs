//! The geodata store's web half: the browser's Cache API.
//!
//! Pure I/O; what to keep and for how long is decided in `store.rs`. Kept
//! self-contained on purpose - only std, `web-sys`, `js-sys`,
//! `wasm-bindgen` and `wasm-bindgen-futures` - so that a scratch crate can
//! compile this very file and run it in a real browser, which is how it was
//! verified (#1582): no test of this repository runs in a browser.
//!
//! Every call degrades to "nothing kept": `caches` is absent outside a
//! secure context, and a browser may refuse storage in a private window.

use js_sys::{Array, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Cache, CacheStorage, Headers, Response, ResponseInit};

/// The response header an entry's store time travels in, as Unix seconds.
pub const STORED_AT_HEADER: &str = "x-symbios-stored-at";

fn caches() -> Option<CacheStorage> {
    web_sys::window()?.caches().ok()
}

async fn open(cache_name: &str) -> Option<Cache> {
    let cache = JsFuture::from(caches()?.open(cache_name)).await.ok()?;
    cache.dyn_into::<Cache>().ok()
}

/// The body kept for `url` in `cache_name`, and when it was kept.
pub async fn read(cache_name: &str, url: &str) -> Option<(i64, Vec<u8>)> {
    let cache = open(cache_name).await?;
    let found = JsFuture::from(cache.match_with_str(url)).await.ok()?;
    let response = found.dyn_into::<Response>().ok()?;
    let stored = response.headers().get(STORED_AT_HEADER).ok()??;
    let stored: i64 = stored.parse().ok()?;
    let buffer = JsFuture::from(response.array_buffer().ok()?).await.ok()?;
    Some((stored, Uint8Array::new(&buffer).to_vec()))
}

/// Keep `body` for `url` in `cache_name`, stamped `stored_at`.
pub async fn write(cache_name: &str, url: &str, body: &[u8], stored_at: i64) {
    let Some(cache) = open(cache_name).await else {
        return;
    };
    let Ok(headers) = Headers::new() else {
        return;
    };
    if headers
        .set(STORED_AT_HEADER, &stored_at.to_string())
        .is_err()
    {
        return;
    }
    let init = ResponseInit::new();
    init.set_status(200);
    init.set_headers(&headers);
    // The binding takes the body mutably; the browser copies it either way.
    let mut copy = body.to_vec();
    let Ok(response) = Response::new_with_opt_u8_array_and_init(Some(&mut copy), &init) else {
        return;
    };
    let _ = JsFuture::from(cache.put_with_str(url, &response)).await;
}

/// Forget what `cache_name` keeps for `url`.
pub async fn remove(cache_name: &str, url: &str) {
    if let Some(cache) = open(cache_name).await {
        let _ = JsFuture::from(cache.delete_with_str(url)).await;
    }
}

/// Delete every cache whose name starts with `prefix`, except `keep`.
pub async fn sweep(prefix: &str, keep: &str) {
    let Some(storage) = caches() else {
        return;
    };
    let Ok(names) = JsFuture::from(storage.keys()).await else {
        return;
    };
    let Ok(names) = names.dyn_into::<Array>() else {
        return;
    };
    for name in names.iter().filter_map(|n| n.as_string()) {
        if name.starts_with(prefix) && name != keep {
            let _ = JsFuture::from(storage.delete(&name)).await;
        }
    }
}
