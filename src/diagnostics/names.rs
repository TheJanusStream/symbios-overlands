//! Metric name registry (Spine E-2) — the single vocabulary the samplers, the
//! GUI, the session log and the offline analyzer all reference, so a metric
//! written under one name is read under the same name everywhere (no drift).
//!
//! Naming scheme: `<subsystem>.<noun>.<unit>` in dotted lower_snake, with
//! subsystem prefixes `runtime` | `net` | `loading` | `offload` | `record`.
//! The unit
//! suffix hints the value type by convention (`.ms`/`.secs`/`.bytes`/`.pct`
//! are gauges or histograms; `.count` is a counter), but the authoritative
//! shape is the [`MetricKind`] recorded alongside each name in [`ALL`].
//!
//! Each `&'static str` doubles as the `HashMap` key in
//! [`MetricsRegistry`](crate::diagnostics::MetricsRegistry), so lookups are
//! pointer-cheap and a mistyped name is a missing const (compile error) rather
//! than a silently orphaned bucket.

use crate::diagnostics::registry::MetricKind;

// ---- runtime health -------------------------------------------------------
/// Smoothed frame time (ms), scraped from `FrameTimeDiagnosticsPlugin` at 1 Hz.
pub const RUNTIME_FRAME_TIME_MS: &str = "runtime.frame_time.ms";
/// Smoothed frames per second.
pub const RUNTIME_FPS: &str = "runtime.fps";
/// Live entity count (`EntityCountDiagnosticsPlugin`).
pub const RUNTIME_ENTITY_COUNT: &str = "runtime.entity.count";
/// `Assets<Mesh>` handle count — the asset-leak watch.
pub const RUNTIME_MESH_HANDLE_COUNT: &str = "runtime.mesh_handle.count";
/// `Assets<StandardMaterial>` handle count.
pub const RUNTIME_MATERIAL_HANDLE_COUNT: &str = "runtime.material_handle.count";
/// `Assets<Image>` handle count — the texture-asset registry. The dominant
/// memory consumer across a region re-seed (512² splat/procedural textures), and
/// the one the mesh/material counts miss: caches that survive a rebuild hold
/// `Handle<Image>`, so this gauge reveals whether image assets actually shrink
/// after a rebuild/logout (#625).
pub const RUNTIME_IMAGE_HANDLE_COUNT: &str = "runtime.image_handle.count";
/// Physics collider count — a double-count signals a duplicate terrain body.
pub const RUNTIME_COLLIDER_COUNT: &str = "runtime.collider.count";
/// Upstream `ShapeMeshCache` length — the documented unbounded-growth leak.
pub const RUNTIME_SHAPE_MESH_CACHE_LEN: &str = "runtime.shape_mesh_cache.len";
/// Process resident memory (bytes), native only (`SystemInformationDiagnosticsPlugin`).
pub const RUNTIME_MEMORY_PROCESS_RSS_BYTES: &str = "runtime.memory.process_rss_bytes";
/// WebAssembly linear-memory size (bytes), wasm only — the heap-never-shrinks watch.
pub const RUNTIME_MEMORY_WASM_BYTES: &str = "runtime.memory.wasm_bytes";
/// Process CPU usage percent, native only.
pub const RUNTIME_CPU_USAGE_PCT: &str = "runtime.cpu.usage_pct";
/// Times the player fell through terrain and was respawned.
pub const RUNTIME_RESPAWN_COUNT: &str = "runtime.respawn.count";
/// Terrain splat material's texture bind-slot footprint — the WebGL2 16-slot
/// ceiling watch (one higher on native, which keeps the stains overlay).
pub const RUNTIME_TEXTURE_BIND_SLOTS: &str = "runtime.texture_bind_slots";

// ---- network / multiuser --------------------------------------------------
/// Peer connections observed.
pub const NET_PEER_CONNECTED_COUNT: &str = "net.peer.connected_count";
/// Peer disconnections observed.
pub const NET_PEER_DISCONNECTED_COUNT: &str = "net.peer.disconnected_count";
/// Identity claims rejected as spoofed (DID ≠ relay-authenticated binding).
pub const NET_IDENTITY_SPOOFED_COUNT: &str = "net.identity.spoofed_count";
/// Transform samples rejected (NaN/Inf or out-of-bounds magnitude).
pub const NET_TRANSFORM_REJECTED_COUNT: &str = "net.transform.rejected_count";
/// Remote-peer jitter-buffer playout latency (ms).
pub const NET_JITTER_PLAYOUT_LATENCY_MS: &str = "net.jitter.playout_latency_ms";
/// Peer avatar-record fetch latency (ms).
pub const NET_AVATAR_FETCH_LATENCY_MS: &str = "net.avatar_fetch.latency_ms";
/// Peer avatar fetches that resolved to a record or default.
pub const NET_AVATAR_FETCH_SUCCESS_COUNT: &str = "net.avatar_fetch.success_count";
/// Peer avatar fetches that errored.
pub const NET_AVATAR_FETCH_FAIL_COUNT: &str = "net.avatar_fetch.fail_count";
/// Item offers the local user accepted.
pub const NET_OFFER_ACCEPTED_COUNT: &str = "net.offer.accepted_count";
/// Item offers the local user declined.
pub const NET_OFFER_DECLINED_COUNT: &str = "net.offer.declined_count";
/// Incoming offers auto-declined because a dialog was already open (busy-gate).
pub const NET_OFFER_AUTO_DECLINED_BUSY_COUNT: &str = "net.offer.auto_declined_busy_count";
/// Size of the most recent relay `peer_list` — how many peers were already in
/// the room when we joined. Mirrored from the multiuser signaller's
/// `SignalDiagnostics` (which matchbox otherwise hides). The load-bearing glare
/// signal: a value ≥ 1 while zero peers ever reach `Connected` means the WebRTC
/// handshake stalled.
pub const NET_SIGNAL_PEER_LIST_LEN: &str = "net.signal.peer_list_len";
/// Cumulative SDP offers this client initiated (it is the designated Offerer).
pub const NET_SIGNAL_OFFERS_INITIATED: &str = "net.signal.offers_initiated";
/// Cumulative SDP offers this client sent over the signalling channel.
pub const NET_SIGNAL_OFFERS_SENT: &str = "net.signal.offers_sent";
/// Cumulative SDP offers this client received (it answers these).
pub const NET_SIGNAL_OFFERS_RECEIVED: &str = "net.signal.offers_received";
/// Cumulative SDP answers this client sent.
pub const NET_SIGNAL_ANSWERS_SENT: &str = "net.signal.answers_sent";
/// Cumulative SDP answers this client received (completes an offer it made).
pub const NET_SIGNAL_ANSWERS_RECEIVED: &str = "net.signal.answers_received";
/// 0/1 flag: the relay reported peers in the room but none have completed the
/// WebRTC handshake yet (and we have not connected since that peer_list). The
/// `GlareSuspected` invariant fires when this stays `1` over a sustained window.
pub const NET_SIGNAL_AWAITING_PEERS: &str = "net.signal.awaiting_peers";

// ---- loading / state machine ----------------------------------------------
/// PDS record-fetch latency (ms), spawn → resolve.
pub const LOADING_RECORD_FETCH_LATENCY_MS: &str = "loading.record_fetch.latency_ms";
/// Record-fetch retries fired.
pub const LOADING_RECORD_FETCH_RETRY_COUNT: &str = "loading.record_fetch.retry_count";
/// Total wall time (secs) spent in the loading gate for the last room enter.
pub const LOADING_GATE_TOTAL_SECS: &str = "loading.gate.total_secs";

// ---- PDS record sizes -------------------------------------------------------
// Serialized `putRecord` payload size at the most recent publish attempt —
// the single-record-boundary watch (#694): budgets live in
// `crate::pds::record_size` (100 KiB soft / 900 KiB hard pre-flight ceiling).

/// Room record bytes at the last publish attempt.
pub const RECORD_SIZE_ROOM_BYTES: &str = "record.size.room_bytes";
/// Avatar record bytes at the last publish attempt.
pub const RECORD_SIZE_AVATAR_BYTES: &str = "record.size.avatar_bytes";
/// Largest single inventory-item record at the last publish attempt — the
/// stash is one record per item (#696), so the per-record budget applies
/// to the biggest item rather than the whole stash.
pub const RECORD_SIZE_INVENTORY_BYTES: &str = "record.size.inventory_bytes";

// ---- async / offload ------------------------------------------------------
/// Heightmap generation latency (ms).
pub const OFFLOAD_HEIGHTMAP_LATENCY_MS: &str = "offload.heightmap.latency_ms";
/// Ambient-audio bake latency (ms).
pub const OFFLOAD_AMBIENT_BAKE_LATENCY_MS: &str = "offload.ambient_bake.latency_ms";
/// Splat/texture bake latency (ms).
pub const OFFLOAD_TEXTURE_BAKE_LATENCY_MS: &str = "offload.texture_bake.latency_ms";
/// Offloaded jobs that failed or timed out.
pub const OFFLOAD_JOB_ERROR_COUNT: &str = "offload.job.error_count";

/// Every known metric with its value shape, so the registry can pre-seed empty
/// entries (the GUI shows a named-but-empty metric rather than nothing) and the
/// GUI can enumerate the full catalogue. Keep in sync with the consts above —
/// the `all_names_are_unique_and_listed` test guards against omissions.
pub const ALL: &[(&str, MetricKind)] = &[
    // runtime
    (RUNTIME_FRAME_TIME_MS, MetricKind::Gauge),
    (RUNTIME_FPS, MetricKind::Gauge),
    (RUNTIME_ENTITY_COUNT, MetricKind::Gauge),
    (RUNTIME_MESH_HANDLE_COUNT, MetricKind::Gauge),
    (RUNTIME_MATERIAL_HANDLE_COUNT, MetricKind::Gauge),
    (RUNTIME_IMAGE_HANDLE_COUNT, MetricKind::Gauge),
    (RUNTIME_COLLIDER_COUNT, MetricKind::Gauge),
    (RUNTIME_SHAPE_MESH_CACHE_LEN, MetricKind::Gauge),
    (RUNTIME_MEMORY_PROCESS_RSS_BYTES, MetricKind::Gauge),
    (RUNTIME_MEMORY_WASM_BYTES, MetricKind::Gauge),
    (RUNTIME_CPU_USAGE_PCT, MetricKind::Gauge),
    (RUNTIME_TEXTURE_BIND_SLOTS, MetricKind::Gauge),
    (RUNTIME_RESPAWN_COUNT, MetricKind::Counter),
    // net
    (NET_PEER_CONNECTED_COUNT, MetricKind::Counter),
    (NET_PEER_DISCONNECTED_COUNT, MetricKind::Counter),
    (NET_IDENTITY_SPOOFED_COUNT, MetricKind::Counter),
    (NET_TRANSFORM_REJECTED_COUNT, MetricKind::Counter),
    (NET_JITTER_PLAYOUT_LATENCY_MS, MetricKind::Histogram),
    (NET_AVATAR_FETCH_LATENCY_MS, MetricKind::Histogram),
    (NET_AVATAR_FETCH_SUCCESS_COUNT, MetricKind::Counter),
    (NET_AVATAR_FETCH_FAIL_COUNT, MetricKind::Counter),
    (NET_OFFER_ACCEPTED_COUNT, MetricKind::Counter),
    (NET_OFFER_DECLINED_COUNT, MetricKind::Counter),
    (NET_OFFER_AUTO_DECLINED_BUSY_COUNT, MetricKind::Counter),
    (NET_SIGNAL_PEER_LIST_LEN, MetricKind::Gauge),
    (NET_SIGNAL_OFFERS_INITIATED, MetricKind::Gauge),
    (NET_SIGNAL_OFFERS_SENT, MetricKind::Gauge),
    (NET_SIGNAL_OFFERS_RECEIVED, MetricKind::Gauge),
    (NET_SIGNAL_ANSWERS_SENT, MetricKind::Gauge),
    (NET_SIGNAL_ANSWERS_RECEIVED, MetricKind::Gauge),
    (NET_SIGNAL_AWAITING_PEERS, MetricKind::Gauge),
    // loading
    (LOADING_RECORD_FETCH_LATENCY_MS, MetricKind::Histogram),
    (LOADING_RECORD_FETCH_RETRY_COUNT, MetricKind::Counter),
    (LOADING_GATE_TOTAL_SECS, MetricKind::Gauge),
    // record sizes
    (RECORD_SIZE_ROOM_BYTES, MetricKind::Gauge),
    (RECORD_SIZE_AVATAR_BYTES, MetricKind::Gauge),
    (RECORD_SIZE_INVENTORY_BYTES, MetricKind::Gauge),
    // offload
    (OFFLOAD_HEIGHTMAP_LATENCY_MS, MetricKind::Histogram),
    (OFFLOAD_AMBIENT_BAKE_LATENCY_MS, MetricKind::Histogram),
    (OFFLOAD_TEXTURE_BAKE_LATENCY_MS, MetricKind::Histogram),
    (OFFLOAD_JOB_ERROR_COUNT, MetricKind::Counter),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn all_names_are_unique_and_well_formed() {
        let mut seen = HashSet::new();
        for (name, _) in ALL {
            assert!(seen.insert(*name), "duplicate metric name {name}");
            // Dotted `<subsystem>.<noun>[.<unit>]` under a known subsystem.
            let segs: Vec<&str> = name.split('.').collect();
            assert!(
                segs.len() >= 2,
                "name {name} must be at least <subsystem>.<noun>"
            );
            assert!(
                matches!(
                    segs[0],
                    "runtime" | "net" | "loading" | "offload" | "record"
                ),
                "name {name} has unknown subsystem prefix"
            );
        }
    }

    #[test]
    fn counter_names_end_in_count() {
        for (name, kind) in ALL {
            if *kind == MetricKind::Counter {
                assert!(
                    name.ends_with("count"),
                    "counter {name} should end in 'count'"
                );
            }
        }
    }
}
