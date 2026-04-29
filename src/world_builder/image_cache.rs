//! Coalescing cache for image bytes fetched from a [`SignSource`]. A
//! room scattering many [`Sign`](crate::pds::GeneratorKind::Sign) panels
//! that all point at the same source — a banner repeated across a market
//! stall row, ten doorplates carrying a guild logo, every tile of a
//! gallery wall holding the same artist's pfp — would otherwise issue one
//! HTTPS round trip and one image decode per panel. Here, the first
//! panel records a `Pending` task and every later panel sharing that
//! source key enqueues its material on the pending list. When the task
//! finishes, the poll system paints the resulting texture into every
//! queued material at once and promotes the entry to `Ready` so any
//! *future* panel pointing at the same source paints synchronously
//! without a fetch.
//!
//! Three resolver paths land here, all keyed by the same
//! [`SignSourceKey`]:
//!
//! * **URL** — direct HTTPS GET via the project's shared `reqwest`
//!   client. CORS is the host's responsibility on web; a server that
//!   doesn't serve `Access-Control-Allow-Origin: *` produces a fetch
//!   error logged once and the panel falls back to its tint colour.
//! * **AtprotoBlob** — resolves the DID's PDS, then calls
//!   `com.atproto.sync.getBlob?did=…&cid=…`. Same path Portal's avatar
//!   fetch already uses for WASM, lifted here so any blob CID works,
//!   not just `app.bsky.actor.profile.avatar`.
//! * **DidPfp** — fetches `app.bsky.actor.getProfile` for the DID, then
//!   resolves the avatar URL through the same fallback Portal already
//!   has. Equivalent to what Portal does today, but pluggable into any
//!   [`Sign`](crate::pds::GeneratorKind::Sign) generator rather than
//!   only the Portal top face.
//!
//! `IoTaskPool` is the right home for a blocking ATProto HTTP fetch; the
//! compute pool is sized to physical cores and pinning every worker on a
//! socket read would hang procedural texture / terrain generation.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use std::collections::{HashMap, VecDeque};

use crate::pds::SignSource;

/// Hard cap on the number of bytes a single fetched image body may
/// contribute to the cache. A hostile [`Sign`](crate::pds::GeneratorKind::Sign)
/// or [`ParticleSystem`](crate::pds::GeneratorKind::ParticleSystem) can
/// otherwise point at an infinite stream (`/dev/zero` over HTTP) or a
/// multi-gigabyte payload and OOM every connecting client. 16 MiB
/// comfortably covers any reasonable PNG/JPEG/WebP atlas while staying
/// well below the headroom of low-end WebGL clients.
pub const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;

/// Maximum number of distinct source keys held in the cache before
/// FIFO-evicting the oldest entry. Without a bound, an attacker can
/// stream `AvatarStateUpdate`s carrying a fresh randomised
/// [`SignSource::Url`] every frame and force every guest's client to
/// stash unbounded textures in RAM/VRAM. Evicting a cache entry does
/// not unpaint live materials — the `Image` asset stays alive via the
/// material's strong handle — so the only cost of eviction is that a
/// later request for the same URL has to re-fetch.
pub const MAX_CACHE_ENTRIES: usize = 256;

/// Sampler filter applied when an [`Image`] is registered in
/// `Assets<Image>`. Mirrors [`crate::pds::TextureFilter`] but lives in
/// the world-builder layer so the cache module doesn't need to depend
/// on the open-union forward-compat fallback (every cache request
/// resolves to a concrete filter, with `Linear` standing in for any
/// forward-compat `Unknown` value at the call site).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum SamplerFilter {
    #[default]
    Linear,
    Nearest,
}

impl SamplerFilter {
    fn as_image_filter(self) -> ImageFilterMode {
        match self {
            SamplerFilter::Linear => ImageFilterMode::Linear,
            SamplerFilter::Nearest => ImageFilterMode::Nearest,
        }
    }
}

/// Cache key for a [`SignSource`]. Mirrors the open-union variants but
/// drops `Unknown` (which never resolves to a fetchable resource — it
/// represents a forward-compat record from a future engine version).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SignSourceKey {
    Url(String),
    AtprotoBlob { did: String, cid: String },
    DidPfp(String),
}

impl SignSourceKey {
    /// Try to derive a cache key from a [`SignSource`]. Returns `None`
    /// for `Unknown` and for inputs whose required fields are empty
    /// (e.g. a placeholder `Url` with no URL set yet — fetching an
    /// empty string would 404 every time and we'd spin the cache).
    pub fn from_source(source: &SignSource) -> Option<Self> {
        match source {
            SignSource::Url { url } if !url.is_empty() => Some(SignSourceKey::Url(url.clone())),
            SignSource::AtprotoBlob { did, cid } if !did.is_empty() && !cid.is_empty() => {
                Some(SignSourceKey::AtprotoBlob {
                    did: did.clone(),
                    cid: cid.clone(),
                })
            }
            SignSource::DidPfp { did } if !did.is_empty() => {
                Some(SignSourceKey::DidPfp(did.clone()))
            }
            _ => None,
        }
    }
}

/// Cache entry per [`SignSourceKey`]: either a list of materials waiting
/// on the in-flight fetch, or a finished `Handle<Image>` ready to paint
/// synchronously.
pub enum BlobImageEntry {
    /// HTTPS / blob fetch is in flight. Each subsequent caller for this
    /// source pushes its material handle here so the poll system can
    /// drain them all on completion.
    Pending(Vec<Handle<StandardMaterial>>),
    /// Image is GPU-resident. Subsequent callers paint synchronously by
    /// cloning the handle into their own material.
    Ready(Handle<Image>),
}

/// Cache key combining a source identity with its sampler filter. Two
/// requests for the same URL with different filters produce two
/// distinct GPU images so a smooth-Linear panel and a Nearest pixel-
/// art panel can coexist. The fetched bytes are still shared at the
/// network layer — the second filter request hits the same in-flight
/// task and replays the bytes through a second decode pass.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlobImageKey {
    pub source: SignSourceKey,
    pub filter: SamplerFilter,
}

/// Source-keyed coalescing cache for image fetches. Cleared on room
/// transitions so a new room can re-fetch sources that may have
/// updated upstream — most relevant for `DidPfp`, which is
/// intentionally self-updating.
///
/// Bounded by [`MAX_CACHE_ENTRIES`]. Insert order is tracked in a
/// secondary `VecDeque`; when a new key would push the entry count
/// over the cap, the oldest entry is dropped from both the map and
/// the deque before the insert lands. Reads do not refresh order
/// (FIFO, not LRU) — keeping the bookkeeping cheap on the read path.
#[derive(Resource, Default)]
pub struct BlobImageCache {
    pub by_source: HashMap<BlobImageKey, BlobImageEntry>,
    insert_order: VecDeque<BlobImageKey>,
}

impl BlobImageCache {
    pub fn clear(&mut self) {
        self.by_source.clear();
        self.insert_order.clear();
    }

    /// Insert (or replace) `entry` for `key`, evicting the oldest entry
    /// first if the cache is at [`MAX_CACHE_ENTRIES`]. Replacing an
    /// existing key (e.g. `Pending` → `Ready`) leaves its order
    /// position alone so a recently-completed entry isn't artificially
    /// kept around longer than the FIFO would otherwise allow.
    pub fn insert_bounded(&mut self, key: BlobImageKey, entry: BlobImageEntry) {
        if !self.by_source.contains_key(&key) {
            while self.insert_order.len() >= MAX_CACHE_ENTRIES {
                match self.insert_order.pop_front() {
                    Some(oldest) => {
                        self.by_source.remove(&oldest);
                    }
                    None => break,
                }
            }
            self.insert_order.push_back(key.clone());
        }
        self.by_source.insert(key, entry);
    }

    /// Remove the entry for `key` from both the map and the
    /// insertion-order deque. Returns the removed entry if any.
    pub fn remove(&mut self, key: &BlobImageKey) -> Option<BlobImageEntry> {
        let removed = self.by_source.remove(key);
        if removed.is_some() {
            self.insert_order.retain(|k| k != key);
        }
        removed
    }
}

/// In-flight image fetch task, attached to a throwaway entity so the
/// task survives across room rebuilds and is naturally GC'd when its
/// despawn-on-completion runs. Carries the cache key (source + filter)
/// so the poll system can route the result back into the cache and
/// build an Image with the right sampler descriptor.
#[derive(Component)]
pub struct BlobImageTask {
    pub key: BlobImageKey,
    pub task: Task<Option<Vec<u8>>>,
}

/// Resolve a [`SignSource`] to a `Handle<Image>` painting on
/// `material`, using the default (`Linear`) sampler filter. Sign
/// generators and the Portal top-face pfp use this path. For
/// particles that need pixel-art `Nearest` filtering, see
/// [`request_blob_image_filtered`].
pub fn request_blob_image(
    commands: &mut Commands,
    cache: &mut BlobImageCache,
    materials: &mut Assets<StandardMaterial>,
    material: &Handle<StandardMaterial>,
    source: &SignSource,
) {
    request_blob_image_filtered(
        commands,
        cache,
        materials,
        material,
        source,
        SamplerFilter::Linear,
    );
}

/// Resolve a [`SignSource`] + sampler-filter pair to a
/// `Handle<Image>`. Returns immediately for cache hits; for cache
/// misses the material is enqueued and a fetch task is spawned (or
/// attached to an existing pending entry for the same source+filter)
/// so completion lands on every queued material at once. No-ops for
/// `SignSource::Unknown` and for sources with empty required fields.
pub fn request_blob_image_filtered(
    commands: &mut Commands,
    cache: &mut BlobImageCache,
    materials: &mut Assets<StandardMaterial>,
    material: &Handle<StandardMaterial>,
    source: &SignSource,
    filter: SamplerFilter,
) {
    let Some(source_key) = SignSourceKey::from_source(source) else {
        return;
    };
    let key = BlobImageKey {
        source: source_key,
        filter,
    };

    match cache.by_source.get_mut(&key) {
        // Cache hit — paint synchronously.
        Some(BlobImageEntry::Ready(img_handle)) => {
            let img = img_handle.clone();
            if let Some(mat) = materials.get_mut(material) {
                mat.base_color_texture = Some(img);
            }
        }
        // Fetch already in flight — enqueue.
        Some(BlobImageEntry::Pending(list)) => {
            list.push(material.clone());
        }
        // First requester for this key — register pending and spawn the
        // task.
        None => {
            cache.insert_bounded(key.clone(), BlobImageEntry::Pending(vec![material.clone()]));

            let pool = IoTaskPool::get();
            let source_for_task = key.source.clone();
            let task = pool.spawn(async move {
                let fut = fetch_bytes_for(source_for_task);
                #[cfg(target_arch = "wasm32")]
                {
                    fut.await
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .ok()
                        .and_then(|rt| rt.block_on(fut))
                }
            });
            commands.spawn(BlobImageTask { key, task });
        }
    }
}

/// Drain finished blob image fetches and paint the resulting texture
/// onto every material that was waiting on this source. Failed fetches
/// drop the pending entry so a future request gets a fresh attempt
/// instead of being permanently stuck on a transient network blip.
pub fn poll_blob_image_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut BlobImageTask)>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: ResMut<BlobImageCache>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        commands.entity(entity).despawn();

        // Take ownership of the pending list while leaving the entry's
        // FIFO position in `insert_order` intact — calling
        // `cache.remove` here would forfeit the slot, and the
        // subsequent `insert_bounded(Ready)` would then re-queue the
        // key at the back of the deque, artificially extending its
        // lifespan past what the documented FIFO contract allows. By
        // holding the slot we let `insert_bounded` take its
        // "key already present → leave order alone" path on
        // promotion.
        let pending = match cache.by_source.get_mut(&task.key) {
            Some(BlobImageEntry::Pending(list)) => std::mem::take(list),
            Some(BlobImageEntry::Ready(_)) => {
                // Promoted by a duplicate task — drop this result.
                continue;
            }
            None => continue,
        };

        let Some(bytes) = result else {
            // Fetch failed. Drop the pending entry so the next requester
            // for this key gets a fresh attempt rather than stalling
            // forever behind a transient failure.
            cache.remove(&task.key);
            continue;
        };
        let Ok(dyn_img) = image::load_from_memory(&bytes) else {
            warn!("Failed to decode image bytes for sign source");
            cache.remove(&task.key);
            continue;
        };
        let mut img = Image::from_dynamic(
            dyn_img,
            true,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        // Honour the requested sampler filter — Linear (default) gives
        // the soft filtering Sign panels and smooth particles want;
        // Nearest preserves crisp texel edges for pixel-art atlases.
        let filter = task.key.filter.as_image_filter();
        img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            mag_filter: filter,
            min_filter: filter,
            mipmap_filter: filter,
            address_mode_u: ImageAddressMode::ClampToEdge,
            address_mode_v: ImageAddressMode::ClampToEdge,
            address_mode_w: ImageAddressMode::ClampToEdge,
            ..default()
        });
        let img_handle = images.add(img);
        for mat_handle in pending {
            if let Some(mat) = materials.get_mut(&mat_handle) {
                mat.base_color_texture = Some(img_handle.clone());
            }
        }
        cache.insert_bounded(task.key.clone(), BlobImageEntry::Ready(img_handle));
    }
}

/// Fetch the raw bytes for a source key. Routes by variant: URL hits
/// the URL directly, `AtprotoBlob` resolves the DID's PDS and calls
/// `getBlob`, `DidPfp` calls `app.bsky.actor.getProfile` and follows
/// the avatar URL the way `crate::avatar::fetch_avatar_bytes` already
/// does for the Portal top face.
async fn fetch_bytes_for(key: SignSourceKey) -> Option<Vec<u8>> {
    let client = crate::config::http::default_client();
    match key {
        SignSourceKey::Url(url) => fetch_url_bytes(&client, &url).await,
        SignSourceKey::AtprotoBlob { did, cid } => fetch_blob_bytes(&client, &did, &cid).await,
        SignSourceKey::DidPfp(did) => {
            // Reuse the existing pfp fetcher rather than reimplementing the
            // bsky/atproto fork — `fetch_avatar_bytes` already handles the
            // wasm-vs-native CDN/CORS split.
            let result = crate::avatar::fetch_avatar_bytes(did).await;
            result.bytes
        }
    }
}

/// Direct HTTPS GET. Returns `None` on connection error, non-success
/// status, oversized body, or body-read failure — every such case is
/// logged at warn so authors can debug a typo'd URL without the panel
/// silently going missing.
///
/// The body is streamed and capped at [`MAX_IMAGE_BYTES`]: a hostile URL
/// (an infinite stream like `/dev/zero` over HTTP, or a multi-gigabyte
/// asset) would otherwise pull the entire response into memory and OOM
/// every guest who walks into the room.
async fn fetch_url_bytes(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let mut resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(e) => {
            warn!("Sign URL fetch failed for {url}: {e}");
            return None;
        }
    };
    if !resp.status().is_success() {
        warn!("Sign URL fetch returned {} for {url}", resp.status());
        return None;
    }
    // Pre-flight check: if the server advertises a length and it already
    // exceeds the cap, don't even start streaming.
    if let Some(len) = resp.content_length()
        && len as usize > MAX_IMAGE_BYTES
    {
        warn!("Sign URL body too large: Content-Length {len} exceeds {MAX_IMAGE_BYTES} for {url}");
        return None;
    }
    let mut buf: Vec<u8> = Vec::new();
    loop {
        match resp.chunk().await {
            Ok(Some(chunk)) => {
                if buf.len().saturating_add(chunk.len()) > MAX_IMAGE_BYTES {
                    warn!(
                        "Sign URL body exceeded cap of {MAX_IMAGE_BYTES} bytes mid-stream for {url}"
                    );
                    return None;
                }
                buf.extend_from_slice(&chunk);
            }
            Ok(None) => return Some(buf),
            Err(e) => {
                warn!("Sign URL body read failed for {url}: {e}");
                return None;
            }
        }
    }
}

/// ATProto blob fetch via `com.atproto.sync.getBlob`. Resolves the DID's
/// PDS first, then calls the blob endpoint. Same path the WASM portal
/// avatar fetch already takes — generalised here to any blob CID, not
/// just an avatar.
async fn fetch_blob_bytes(client: &reqwest::Client, did: &str, cid: &str) -> Option<Vec<u8>> {
    let pds = match crate::pds::resolve_pds(client, did).await {
        Some(p) => p,
        None => {
            warn!("Sign DID {did} did not resolve to a PDS");
            return None;
        }
    };
    let blob_url = format!("{pds}/xrpc/com.atproto.sync.getBlob?did={did}&cid={cid}");
    fetch_url_bytes(client, &blob_url).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url_key(s: &str) -> BlobImageKey {
        BlobImageKey {
            source: SignSourceKey::Url(s.to_string()),
            filter: SamplerFilter::Linear,
        }
    }

    /// `insert_bounded` evicts the oldest entry once `MAX_CACHE_ENTRIES`
    /// is reached so an attacker spamming randomised
    /// [`SignSource::Url`] values via `AvatarStateUpdate` can't grow the
    /// cache without bound. Replacing an existing key (e.g. Pending →
    /// Ready) must not count as a fresh insert.
    #[test]
    fn cache_evicts_oldest_when_over_capacity() {
        let mut cache = BlobImageCache::default();
        // Fill the cache exactly to capacity. Each insert must remain.
        for i in 0..MAX_CACHE_ENTRIES {
            cache.insert_bounded(
                url_key(&format!("https://example.test/{i}")),
                BlobImageEntry::Pending(Vec::new()),
            );
        }
        assert_eq!(cache.by_source.len(), MAX_CACHE_ENTRIES);
        assert_eq!(cache.insert_order.len(), MAX_CACHE_ENTRIES);
        assert!(
            cache
                .by_source
                .contains_key(&url_key("https://example.test/0")),
            "before overflow, the oldest entry should still be present"
        );

        // Push one over the cap. The oldest URL ("…/0") must be evicted
        // and the newcomer kept.
        cache.insert_bounded(
            url_key("https://example.test/overflow"),
            BlobImageEntry::Pending(Vec::new()),
        );
        assert_eq!(cache.by_source.len(), MAX_CACHE_ENTRIES);
        assert!(
            !cache
                .by_source
                .contains_key(&url_key("https://example.test/0")),
            "oldest entry must be evicted when overflowing"
        );
        assert!(
            cache
                .by_source
                .contains_key(&url_key("https://example.test/overflow")),
            "new entry must land in the cache"
        );

        // Replacing an existing key (Pending → Ready style) must not
        // re-add to the order deque (preserves FIFO position) and must
        // not evict another entry.
        let stable_key = url_key("https://example.test/1");
        let prior_order_len = cache.insert_order.len();
        cache.insert_bounded(stable_key.clone(), BlobImageEntry::Pending(Vec::new()));
        assert_eq!(cache.insert_order.len(), prior_order_len);
        assert!(cache.by_source.contains_key(&stable_key));
    }

    /// `remove` drops the entry from both the map and the insertion-
    /// order deque so a removed key isn't double-counted against the
    /// capacity ceiling on a subsequent insert.
    #[test]
    fn cache_remove_clears_insertion_order() {
        let mut cache = BlobImageCache::default();
        let k = url_key("https://example.test/x");
        cache.insert_bounded(k.clone(), BlobImageEntry::Pending(Vec::new()));
        assert_eq!(cache.insert_order.len(), 1);
        let _ = cache.remove(&k);
        assert!(cache.by_source.is_empty());
        assert!(cache.insert_order.is_empty());
    }

    /// Promoting a `Pending` entry to `Ready` via `insert_bounded` must
    /// preserve the entry's FIFO position. The previous
    /// `poll_blob_image_tasks` implementation called `remove` to extract
    /// the pending list, which forfeited the slot and let
    /// `insert_bounded` re-queue the key at the back — artificially
    /// extending the just-completed entry's lifespan past the FIFO bound.
    /// This test pins the documented contract.
    #[test]
    fn promotion_preserves_fifo_position() {
        let mut cache = BlobImageCache::default();
        let early = url_key("https://example.test/early");
        let middle = url_key("https://example.test/middle");
        let late = url_key("https://example.test/late");

        cache.insert_bounded(early.clone(), BlobImageEntry::Pending(Vec::new()));
        cache.insert_bounded(middle.clone(), BlobImageEntry::Pending(Vec::new()));
        cache.insert_bounded(late.clone(), BlobImageEntry::Pending(Vec::new()));

        // Promote the middle entry to Ready — order must not change.
        cache.insert_bounded(middle.clone(), BlobImageEntry::Ready(Handle::default()));

        let order: Vec<&BlobImageKey> = cache.insert_order.iter().collect();
        assert_eq!(
            order,
            vec![&early, &middle, &late],
            "Pending → Ready promotion must leave the entry in its original FIFO slot"
        );
    }

    /// `clear` empties both the map and the order tracker so a room
    /// transition resets the cache cleanly without leaking stale
    /// insertion-order entries.
    #[test]
    fn cache_clear_resets_both_structures() {
        let mut cache = BlobImageCache::default();
        for i in 0..4 {
            cache.insert_bounded(
                url_key(&format!("https://example.test/{i}")),
                BlobImageEntry::Pending(Vec::new()),
            );
        }
        cache.clear();
        assert!(cache.by_source.is_empty());
        assert!(cache.insert_order.is_empty());
    }
}
