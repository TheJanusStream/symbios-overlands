//! Wasm tracking allocator (#811) — live-bytes-by-size-class accounting and a
//! giant-allocation fingerprint ring.
//!
//! The OOM investigation exhausted gauge correlation: the runaway phases
//! allocate ~0.5 GB/s in near-geometric steps while every world/render/asset
//! gauge sits flat — the signature of one collection doubling its capacity,
//! not per-frame churn. This wraps the system allocator (dlmalloc on wasm)
//! and answers the two questions correlation can't (items live in the
//! wasm-gated `wasm` submodule):
//!
//! * **shape** — live bytes per size class (`wasm::snapshot`): a runaway in
//!   the `giant` (≥ 16 MiB) class is one huge buffer; a runaway in `small` is
//!   a million-object leak;
//! * **identity** — every allocation ≥ `wasm::GIANT_BYTES` records its exact
//!   size into a ring (`wasm::giant_sizes_since`); a request for, say,
//!   768 MiB is a fingerprint that identifies its collection (and a ×2 size
//!   sequence is a `Vec` doubling caught red-handed).
//!
//! Counting is a few `Relaxed` atomic ops per alloc/dealloc — wasm is
//! single-threaded, so these compile to plain memory ops; the allocator
//! itself never allocates. Native builds keep the default allocator (real
//! profilers exist there); everything here is wasm-gated.

#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

    /// Allocations at or above this size are "giant": individually recorded
    /// in the fingerprint ring. 16 MiB is far above any per-frame or
    /// per-asset allocation the app makes on purpose.
    pub const GIANT_BYTES: usize = 16 * 1024 * 1024;

    /// Fingerprint ring length. The runaway phases observed in the field
    /// made < 10 giant allocations before saturating the heap, so 16 slots
    /// hold a full episode between 1 Hz scrapes.
    const RING: usize = 16;

    // Live-byte totals per size class (small < 64 KiB ≤ medium < 1 MiB ≤
    // large < 16 MiB ≤ giant).
    static LIVE_SMALL: AtomicU64 = AtomicU64::new(0);
    static LIVE_MEDIUM: AtomicU64 = AtomicU64::new(0);
    static LIVE_LARGE: AtomicU64 = AtomicU64::new(0);
    static LIVE_GIANT: AtomicU64 = AtomicU64::new(0);

    /// Total giant allocations ever; `GIANT_SIZES[total % RING]` is the most
    /// recent slot written.
    static GIANT_TOTAL: AtomicU64 = AtomicU64::new(0);
    static GIANT_SIZES: [AtomicU64; RING] = [const { AtomicU64::new(0) }; RING];

    fn class(bytes: usize) -> &'static AtomicU64 {
        match bytes {
            0..=0xFFFF => &LIVE_SMALL,
            0x1_0000..=0xF_FFFF => &LIVE_MEDIUM,
            0x10_0000..=0xFF_FFFF => &LIVE_LARGE,
            _ => &LIVE_GIANT,
        }
    }

    fn record(bytes: usize) {
        class(bytes).fetch_add(bytes as u64, Relaxed);
        if bytes >= GIANT_BYTES {
            let n = GIANT_TOTAL.fetch_add(1, Relaxed);
            GIANT_SIZES[(n as usize) % RING].store(bytes as u64, Relaxed);
        }
    }

    fn unrecord(bytes: usize) {
        class(bytes).fetch_sub(bytes as u64, Relaxed);
    }

    /// [`GlobalAlloc`] wrapper over the system allocator — see module docs.
    pub struct TrackingAlloc;

    // SAFETY: delegates every operation verbatim to `System` and touches only
    // static atomics on the side — no allocation, no reentrancy, no locks.
    unsafe impl GlobalAlloc for TrackingAlloc {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let p = unsafe { System.alloc(layout) };
            if !p.is_null() {
                record(layout.size());
            }
            p
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let p = unsafe { System.alloc_zeroed(layout) };
            if !p.is_null() {
                record(layout.size());
            }
            p
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) };
            unrecord(layout.size());
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            let p = unsafe { System.realloc(ptr, layout, new_size) };
            if !p.is_null() {
                unrecord(layout.size());
                record(new_size);
            }
            p
        }
    }

    /// Live bytes as `(small, medium, large, giant)`.
    pub fn snapshot() -> (u64, u64, u64, u64) {
        (
            LIVE_SMALL.load(Relaxed),
            LIVE_MEDIUM.load(Relaxed),
            LIVE_LARGE.load(Relaxed),
            LIVE_GIANT.load(Relaxed),
        )
    }

    /// Total giant allocations ever made.
    pub fn giant_total() -> u64 {
        GIANT_TOTAL.load(Relaxed)
    }

    /// The sizes of giant allocations numbered `since..giant_total()`
    /// (oldest first), clamped to the last [`RING`] — the caller tracks
    /// `since` across scrapes and logs each new fingerprint once.
    pub fn giant_sizes_since(since: u64) -> Vec<u64> {
        let total = GIANT_TOTAL.load(Relaxed);
        let start = since.max(total.saturating_sub(RING as u64));
        (start..total)
            .map(|n| GIANT_SIZES[(n as usize) % RING].load(Relaxed))
            .collect()
    }
}

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static GLOBAL_TRACKING_ALLOC: wasm::TrackingAlloc = wasm::TrackingAlloc;
