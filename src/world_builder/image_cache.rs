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

/// Maximum decoded bytes the cache will hold across all its `Ready`
/// entries before FIFO-evicting, independent of the entry count.
///
/// **Why a count was never a memory bound (#1128).** Entries are not a fixed
/// size: [`MAX_CACHE_ENTRIES`] of them at the old decode ceiling was
/// 256 × 64 MiB ≈ 16 GiB of permitted decoded pixels, and even a careless
/// room of twenty 4K signs was ~1.3 GiB — in a project whose entire record
/// budget is 100 KiB. The two levers that make the policy mean something are
/// this budget and [`SIGN_WORKING_DIMENSION`] below: the working size caps
/// what one entry can be, and this caps what all of them together can be.
///
/// 192 MiB is roughly forty-eight full-size sign panels, far past any room a
/// person would build and far below what a low-end WebGL client can lose.
pub const MAX_CACHE_BYTES: usize = 192 * 1024 * 1024;

/// Edge length a fetched sign / particle image is downscaled to before it
/// reaches `Assets<Image>`.
///
/// These are diffuse panels seen at room distance, not reference plates, and
/// 2048 is already more texels than a sign occupies on screen at any sane
/// camera. What it replaces is the source's own dimensions, which are chosen
/// by whoever wrote the record: a 4096-square source is 64 MiB of RGBA in the
/// heap and 64 MiB of VRAM pinned by the sign's material for as long as the
/// panel is in the room. At 2048 that is 16 MiB, and the arithmetic behind
/// [`MAX_CACHE_BYTES`] closes.
pub const SIGN_WORKING_DIMENSION: u32 = 2048;

/// How many fetched images this build will decode in one frame.
///
/// **Why a count and not a time budget (#1128).** The decode is the expensive
/// half of the fetch — the transfer already runs off-thread, but
/// `image::load_from_memory` runs wherever the poll system runs, and on wasm
/// that is the one thread the frame loop is on (`IoTaskPool` there is
/// `spawn_local`, so moving the decode into the task would move it nowhere).
/// A room whose signs all resolve on the same frame therefore froze every
/// visitor for the sum of its decodes, and a hostile room of 256 sources
/// froze them for as long as it liked — long enough that a visitor could not
/// even walk out. You cannot know what a decode costs until you have done it,
/// so a time budget can only ever stop *after* the frame is already spent;
/// a count stops before. One per frame turns a single unbounded freeze into
/// panels appearing over successive frames, which is both survivable and
/// legible as loading.
pub const MAX_DECODES_PER_FRAME: usize = 1;

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
    /// Resolve a record-layer [`crate::pds::TextureFilter`] to the
    /// engine-side sampler filter. Unknown forward-compat values fall
    /// back to Linear so a forward-compat record renders smooth-filtered.
    pub fn from_record(filter: &crate::pds::TextureFilter) -> Self {
        match filter {
            crate::pds::TextureFilter::Nearest => SamplerFilter::Nearest,
            crate::pds::TextureFilter::Linear | crate::pds::TextureFilter::Unknown => {
                SamplerFilter::Linear
            }
        }
    }

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
    /// cloning the handle into their own material. `decoded_bytes` is what
    /// this entry contributes to [`MAX_CACHE_BYTES`] — carried on the entry
    /// rather than recomputed, because once the image is `RENDER_WORLD`-only
    /// its pixel buffer is gone and its size can no longer be asked for.
    Ready {
        image: Handle<Image>,
        decoded_bytes: usize,
    },
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
/// Bounded on two axes: [`MAX_CACHE_ENTRIES`] keys and [`MAX_CACHE_BYTES`] of
/// decoded pixels. Insert order is tracked in a secondary `VecDeque`; when a
/// new entry would breach either bound, the oldest entries are dropped from
/// both the map and the deque until it fits. Reads do not refresh order
/// (FIFO, not LRU) — keeping the bookkeeping cheap on the read path.
#[derive(Resource, Default)]
pub struct BlobImageCache {
    pub by_source: HashMap<BlobImageKey, BlobImageEntry>,
    insert_order: VecDeque<BlobImageKey>,
    /// Running sum of every `Ready` entry's `decoded_bytes`. Maintained by
    /// the three mutators below rather than folded on demand, so the read
    /// path stays free and the invariant has exactly three places to hold.
    decoded_bytes: usize,
}

impl BlobImageCache {
    pub fn clear(&mut self) {
        self.by_source.clear();
        self.insert_order.clear();
        self.decoded_bytes = 0;
    }

    /// Decoded bytes currently held across all `Ready` entries.
    pub fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }

    /// Insert (or replace) `entry` for `key`, first evicting oldest-first
    /// until the cache has room for it under both bounds. Replacing an
    /// existing key (e.g. `Pending` → `Ready`) leaves its order position
    /// alone so a recently-completed entry isn't artificially kept around
    /// longer than the FIFO would otherwise allow.
    pub fn insert_bounded(&mut self, key: BlobImageKey, entry: BlobImageEntry) {
        let incoming = entry_bytes(&entry);
        let replacing = self.by_source.contains_key(&key);
        if !replacing {
            while self.insert_order.len() >= MAX_CACHE_ENTRIES {
                if !self.evict_oldest() {
                    break;
                }
            }
        }
        // The byte bound is checked for a promotion too: `Pending → Ready` is
        // where bytes actually arrive, and it is not a fresh insert, so an
        // entry-count-only guard would let a room of large signs walk the
        // total up without ever tripping anything.
        let outgoing = self
            .by_source
            .get(&key)
            .map(entry_bytes)
            .unwrap_or_default();
        while self.decoded_bytes + incoming > MAX_CACHE_BYTES + outgoing {
            // Never evict the key being inserted out from under itself — it
            // is about to be overwritten, and dropping its FIFO slot here
            // would re-queue it at the back on the way back in.
            if self.insert_order.front() == Some(&key) || !self.evict_oldest() {
                break;
            }
        }
        if !self.by_source.contains_key(&key) {
            self.insert_order.push_back(key.clone());
        }
        self.decoded_bytes = self.decoded_bytes + incoming - outgoing;
        self.by_source.insert(key, entry);
    }

    /// Drop the oldest entry. Returns false when there was nothing to drop,
    /// which is the only thing that can end the eviction loops above.
    fn evict_oldest(&mut self) -> bool {
        match self.insert_order.pop_front() {
            Some(oldest) => {
                if let Some(entry) = self.by_source.remove(&oldest) {
                    self.decoded_bytes = self.decoded_bytes.saturating_sub(entry_bytes(&entry));
                }
                true
            }
            None => false,
        }
    }

    /// Remove the entry for `key` from both the map and the
    /// insertion-order deque. Returns the removed entry if any.
    pub fn remove(&mut self, key: &BlobImageKey) -> Option<BlobImageEntry> {
        let removed = self.by_source.remove(key);
        if let Some(entry) = removed.as_ref() {
            self.decoded_bytes = self.decoded_bytes.saturating_sub(entry_bytes(entry));
            self.insert_order.retain(|k| k != key);
        }
        removed
    }
}

/// What an entry contributes to the byte budget. A `Pending` entry holds a
/// list of material handles and no pixels; only a decoded image counts.
fn entry_bytes(entry: &BlobImageEntry) -> usize {
    match entry {
        BlobImageEntry::Pending(_) => 0,
        BlobImageEntry::Ready { decoded_bytes, .. } => *decoded_bytes,
    }
}

/// In-flight image fetch task, attached to a throwaway entity so the
/// task survives across room rebuilds and is naturally GC'd when its
/// despawn-on-completion runs. Carries the cache key (source + filter)
/// so the poll system can route the result back into the cache and
/// build an Image with the right sampler descriptor.
///
/// `fetched` is the landing pad for a body that arrived on a frame whose
/// [`MAX_DECODES_PER_FRAME`] budget was already spent. Polling a task
/// consumes its result, so the bytes have to be held somewhere until their
/// turn; holding them here keeps them attached to the request they belong to
/// and keeps them despawning with it if the room changes underneath.
#[derive(Component)]
pub struct BlobImageTask {
    pub key: BlobImageKey,
    pub task: Task<Option<Vec<u8>>>,
    pub fetched: Option<Vec<u8>>,
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
        Some(BlobImageEntry::Ready { image, .. }) => {
            let img = image.clone();
            if let Some(mut mat) = materials.get_mut(material) {
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
                crate::config::http::run_or(fut, None).await
            });
            commands.spawn(BlobImageTask {
                key,
                task,
                fetched: None,
            });
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
    // Decodes done this frame. Fetches that land past the budget park their
    // bytes on their own component and are picked up on a later frame — see
    // [`MAX_DECODES_PER_FRAME`] for why the budget counts decodes rather than
    // measuring their time.
    let mut decoded_this_frame = 0usize;

    for (entity, mut task) in tasks.iter_mut() {
        // Bytes already in hand from an earlier frame take precedence over a
        // fresh poll: a parked body must not be able to wait behind a queue
        // that keeps growing, or a busy room would starve its oldest signs.
        let result = match task.fetched.take() {
            Some(bytes) => Some(bytes),
            None => {
                let Some(result) =
                    futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
                else {
                    continue;
                };
                result
            }
        };

        // A failed fetch costs nothing to retire, so it is settled below
        // regardless of the budget; only a body that needs decoding waits.
        if result.is_some() && decoded_this_frame >= MAX_DECODES_PER_FRAME {
            task.fetched = result;
            continue;
        }
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
            Some(BlobImageEntry::Ready { .. }) => {
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
        decoded_this_frame += 1;
        let Some(dyn_img) =
            super::blob_fetch::decode_image_capped(&bytes, "Sign source", SIGN_WORKING_DIMENSION)
        else {
            cache.remove(&task.key);
            continue;
        };
        // Measured off the decoded frame, before `from_dynamic` consumes it:
        // this is the number the cache's byte budget is denominated in, and
        // it is also what the GPU texture will cost, since `from_dynamic`
        // uploads RGBA8.
        let decoded_bytes = (dyn_img.width() as usize)
            .saturating_mul(dyn_img.height() as usize)
            .saturating_mul(4);
        // `RENDER_WORLD`-only: these decoded images are bound straight to Sign /
        // particle materials and never sampled back on the CPU, so releasing the
        // CPU pixel buffer after the GPU upload saves the full decoded RGBA
        // (up to ~64 MiB for a 4K source) — significant on wasm, where freed
        // linear memory is never returned to the OS.
        let mut img = Image::from_dynamic(dyn_img, true, RenderAssetUsages::RENDER_WORLD);
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
            if let Some(mut mat) = materials.get_mut(&mat_handle) {
                mat.base_color_texture = Some(img_handle.clone());
            }
        }
        cache.insert_bounded(
            task.key.clone(),
            BlobImageEntry::Ready {
                image: img_handle,
                decoded_bytes,
            },
        );
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
        SignSourceKey::Url(url) => {
            super::blob_fetch::fetch_url_bytes(&client, &url, MAX_IMAGE_BYTES, "Sign").await
        }
        SignSourceKey::AtprotoBlob { did, cid } => {
            super::blob_fetch::fetch_blob_bytes(&client, &did, &cid, MAX_IMAGE_BYTES, "Sign").await
        }
        SignSourceKey::DidPfp(did) => {
            // Reuse the existing pfp fetcher rather than reimplementing the
            // bsky/atproto fork — `fetch_avatar_bytes` already handles the
            // wasm-vs-native CDN/CORS split.
            let result = crate::avatar::fetch_avatar_bytes(did).await;
            result.bytes
        }
    }
}

// The capped HTTPS GET + ATProto `getBlob` fetch live in the shared
// `super::blob_fetch` module (#262) so the audio-cue cache reuses the
// exact same wasm/native streaming + OOM-guard path. `MAX_IMAGE_BYTES`
// is the image-side cap passed through.

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
        cache.insert_bounded(
            middle.clone(),
            BlobImageEntry::Ready {
                image: Handle::default(),
                decoded_bytes: 0,
            },
        );

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
        assert_eq!(cache.decoded_bytes(), 0);
    }

    fn ready(bytes: usize) -> BlobImageEntry {
        BlobImageEntry::Ready {
            image: Handle::default(),
            decoded_bytes: bytes,
        }
    }

    /// The #1128 sequence the entry count could not see.
    ///
    /// A room's signs all point at large sources. Under the old policy the
    /// cache counted keys, so twenty of them was twenty entries — nowhere
    /// near the 256 cap — while being over a gigabyte of decoded pixels. Here
    /// the same twenty land under a byte budget and the oldest are evicted
    /// once the total would breach it, which is the only unit that describes
    /// what the cache actually costs.
    #[test]
    fn a_handful_of_large_entries_evicts_where_a_handful_of_small_ones_would_not() {
        let big = MAX_CACHE_BYTES / 4;
        let mut cache = BlobImageCache::default();
        for i in 0..6 {
            cache.insert_bounded(url_key(&format!("https://example.test/big{i}")), ready(big));
        }

        assert!(
            cache.decoded_bytes() <= MAX_CACHE_BYTES,
            "six quarter-budget entries must not all be resident: {} bytes held",
            cache.decoded_bytes()
        );
        assert!(
            cache.by_source.len() < 6,
            "the byte budget must have evicted something — an entry count of 6 \
             is nowhere near MAX_CACHE_ENTRIES and would have kept them all"
        );
        // FIFO: the survivors are the newest.
        assert!(
            cache
                .by_source
                .contains_key(&url_key("https://example.test/big5")),
            "the newest entry must survive its own insert"
        );
        assert!(
            !cache
                .by_source
                .contains_key(&url_key("https://example.test/big0")),
            "the oldest entry must be the one evicted"
        );
    }

    /// Small entries are still bounded by the key count, not the byte budget:
    /// the two limits are independent and the tighter one wins.
    #[test]
    fn many_tiny_entries_are_still_bounded_by_the_entry_count() {
        let mut cache = BlobImageCache::default();
        for i in 0..MAX_CACHE_ENTRIES + 8 {
            cache.insert_bounded(url_key(&format!("https://example.test/{i}")), ready(1024));
        }
        assert_eq!(cache.by_source.len(), MAX_CACHE_ENTRIES);
        assert!(cache.decoded_bytes() < MAX_CACHE_BYTES);
    }

    /// The promotion path is where bytes actually arrive — a fetch completes
    /// and a `Pending` entry becomes `Ready`. That is not a fresh insert, so
    /// a guard that only ran on new keys would let a room of large signs walk
    /// the total past the budget without ever tripping.
    #[test]
    fn promoting_pending_to_ready_is_charged_to_the_budget() {
        let mut cache = BlobImageCache::default();
        let key = url_key("https://example.test/sign");
        cache.insert_bounded(key.clone(), BlobImageEntry::Pending(Vec::new()));
        assert_eq!(cache.decoded_bytes(), 0, "a pending entry holds no pixels");

        cache.insert_bounded(key.clone(), ready(4 * 1024 * 1024));
        assert_eq!(cache.decoded_bytes(), 4 * 1024 * 1024);
    }

    /// Removing an entry gives its bytes back. Without this the running total
    /// only ever climbs, and after enough failed fetches the cache would
    /// evict everything on every insert while holding nothing.
    #[test]
    fn removing_an_entry_returns_its_bytes_to_the_budget() {
        let mut cache = BlobImageCache::default();
        let key = url_key("https://example.test/sign");
        cache.insert_bounded(key.clone(), ready(8 * 1024 * 1024));
        assert_eq!(cache.decoded_bytes(), 8 * 1024 * 1024);
        cache.remove(&key);
        assert_eq!(cache.decoded_bytes(), 0);
    }

    /// An entry larger than the whole budget is still stored: refusing it
    /// would mean a room with one big sign paints nothing at all, and the
    /// decode has already happened by the time the cache sees it. It simply
    /// evicts everything else, which is the FIFO behaving as documented.
    #[test]
    fn an_entry_over_the_whole_budget_is_stored_alone() {
        let mut cache = BlobImageCache::default();
        cache.insert_bounded(url_key("https://example.test/small"), ready(1024));
        cache.insert_bounded(
            url_key("https://example.test/huge"),
            ready(MAX_CACHE_BYTES * 2),
        );
        assert!(
            cache
                .by_source
                .contains_key(&url_key("https://example.test/huge")),
            "the entry the caller just decoded must be the one that survives"
        );
        assert_eq!(cache.by_source.len(), 1);
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    /// A tiny real PNG, so the decode in the system under test is a real
    /// decode and not a rejection taking a shortcut past the budget.
    fn tiny_png() -> Vec<u8> {
        let img = image::DynamicImage::new_rgba8(8, 8);
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .expect("8×8 PNG must encode");
        buf
    }

    fn key(n: usize) -> BlobImageKey {
        BlobImageKey {
            source: SignSourceKey::Url(format!("https://example.test/sign{n}")),
            filter: SamplerFilter::Linear,
        }
    }

    /// Spawn a task entity whose body has already arrived, so the test
    /// exercises the decode budget without racing a real fetch.
    fn spawn_arrived(app: &mut App, key: BlobImageKey) {
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        app.world_mut()
            .resource_mut::<BlobImageCache>()
            .insert_bounded(key.clone(), BlobImageEntry::Pending(vec![material]));
        // The task itself is never polled on this path — `fetched` short-
        // circuits it — but the component owns one, so it gets a resolved
        // stub rather than a fabricated variant.
        let task = bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default)
            .spawn(async { None });
        app.world_mut().spawn(BlobImageTask {
            key,
            task,
            fetched: Some(tiny_png()),
        });
    }

    fn harness() -> App {
        let mut app = App::new();
        // `AssetPlugin` rather than bare `init_asset`: `Assets::add` reaches
        // for the `AssetServer` to mint a handle.
        app.add_plugins((
            bevy::asset::AssetPlugin::default(),
            bevy::app::TaskPoolPlugin::default(),
        ))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .init_resource::<BlobImageCache>()
        .add_systems(Update, poll_blob_image_tasks);
        app
    }

    fn ready_count(app: &App) -> usize {
        app.world()
            .resource::<BlobImageCache>()
            .by_source
            .values()
            .filter(|e| matches!(e, BlobImageEntry::Ready { .. }))
            .count()
    }

    /// The #1128 sequence: a room whose signs all resolve on the same frame.
    ///
    /// Before the budget, every body that had landed was decoded inside one
    /// poll pass, so a room of 4096-square sources froze the frame loop for
    /// the sum of its decodes — on wasm, where `IoTaskPool` is `spawn_local`
    /// on the frame thread, with no thread to move the work to and no way for
    /// the visitor to even walk out while it drained. Three arrive here; one
    /// decodes.
    #[test]
    fn only_one_image_decodes_per_frame_however_many_have_landed() {
        let mut app = harness();
        for n in 0..3 {
            spawn_arrived(&mut app, key(n));
        }

        app.update();
        assert_eq!(
            ready_count(&app),
            MAX_DECODES_PER_FRAME,
            "the whole queue decoded in one frame — the budget is not holding"
        );
    }

    /// And the parked bodies are not dropped: successive frames drain them,
    /// so the panels appear over three frames rather than never. A budget
    /// that lost work would be worse than no budget.
    #[test]
    fn parked_bodies_drain_on_later_frames_rather_than_being_lost() {
        let mut app = harness();
        for n in 0..3 {
            spawn_arrived(&mut app, key(n));
        }

        for expected in 1..=3 {
            app.update();
            assert_eq!(
                ready_count(&app),
                expected,
                "frame {expected} should have brought exactly one more panel up"
            );
        }

        // Every task entity has retired; nothing is left holding bytes.
        assert_eq!(
            app.world_mut()
                .query::<&BlobImageTask>()
                .iter(app.world())
                .count(),
            0,
            "a task entity outlived its decode — its bytes are retained forever"
        );
    }

    /// The decoded image reaches the material that asked for it. The budget
    /// reorders when panels paint; it must not change whether they do.
    #[test]
    fn the_decoded_image_still_paints_the_waiting_material() {
        let mut app = harness();
        spawn_arrived(&mut app, key(0));
        app.update();

        let painted = app
            .world()
            .resource::<Assets<StandardMaterial>>()
            .iter()
            .filter(|(_, m)| m.base_color_texture.is_some())
            .count();
        assert_eq!(painted, 1, "the pending material was never painted");
    }
}
