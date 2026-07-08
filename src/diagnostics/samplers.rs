//! Per-subsystem metric samplers (Spine E-4) — the thin helpers the network,
//! loading, offload and player systems call to feed the shared
//! [`MetricsRegistry`]. Each helper wraps one `names::` const + the right
//! registry op (counter `incr`, gauge `observe_gauge`, histogram `observe_hist`)
//! so a call site names an *event* (`peer_connected`) rather than a metric
//! string + unit, keeping the `names::` catalogue referenced in exactly one
//! place (no metric-string drift across ~15 call sites).
//!
//! Latency helpers take a duration in **seconds** (what `Time::elapsed_secs_f64`
//! diffs yield at every call site) and convert to the `.latency_ms` histograms'
//! milliseconds here, so the seconds→ms conversion lives once.
//!
//! The E-3 Bevy-diagnostics bridge already feeds the `runtime.*` gauges; these
//! samplers cover the per-subsystem `net` / `loading` / `offload` metrics plus
//! `runtime.respawn.count`, which only the domain systems can observe.

use crate::diagnostics::MetricsRegistry;
use crate::diagnostics::names;

/// Seconds → milliseconds, for the `*.latency_ms` histograms.
fn ms(secs: f64) -> f64 {
    secs * 1000.0
}

// ---- network / multiuser --------------------------------------------------

/// A peer connection was observed.
pub fn peer_connected(m: &mut MetricsRegistry) {
    m.incr(names::NET_PEER_CONNECTED_COUNT);
}

/// A peer disconnection was observed.
pub fn peer_disconnected(m: &mut MetricsRegistry) {
    m.incr(names::NET_PEER_DISCONNECTED_COUNT);
}

/// An identity claim was rejected as spoofed (claimed DID ≠ authenticated DID).
pub fn identity_spoof_rejected(m: &mut MetricsRegistry) {
    m.incr(names::NET_IDENTITY_SPOOFED_COUNT);
}

/// A remote transform sample was rejected (NaN/Inf or out-of-bounds).
pub fn transform_rejected(m: &mut MetricsRegistry) {
    m.incr(names::NET_TRANSFORM_REJECTED_COUNT);
}

/// Remote-peer jitter-buffer playout latency (the render delay), in seconds.
pub fn jitter_playout_latency_secs(m: &mut MetricsRegistry, secs: f64) {
    m.observe_hist(names::NET_JITTER_PLAYOUT_LATENCY_MS, ms(secs));
}

/// A peer avatar-record fetch resolved; `secs` is its spawn→resolve latency.
pub fn avatar_fetch_latency_secs(m: &mut MetricsRegistry, secs: f64) {
    m.observe_hist(names::NET_AVATAR_FETCH_LATENCY_MS, ms(secs));
}

/// A peer avatar fetch resolved to a record or the DID-seeded default.
pub fn avatar_fetch_succeeded(m: &mut MetricsRegistry) {
    m.incr(names::NET_AVATAR_FETCH_SUCCESS_COUNT);
}

/// A peer avatar fetch errored (transient failure).
pub fn avatar_fetch_failed(m: &mut MetricsRegistry) {
    m.incr(names::NET_AVATAR_FETCH_FAIL_COUNT);
}

/// The local user accepted an incoming item offer.
pub fn offer_accepted(m: &mut MetricsRegistry) {
    m.incr(names::NET_OFFER_ACCEPTED_COUNT);
}

/// The local user declined an incoming item offer.
pub fn offer_declined(m: &mut MetricsRegistry) {
    m.incr(names::NET_OFFER_DECLINED_COUNT);
}

/// An incoming offer was auto-declined because a dialog was already open.
pub fn offer_auto_declined_busy(m: &mut MetricsRegistry) {
    m.incr(names::NET_OFFER_AUTO_DECLINED_BUSY_COUNT);
}

/// Record the serialized size (bytes) of a reliable broadcast that went
/// through the chunking path (#716).
pub fn broadcast_payload_bytes(m: &mut MetricsRegistry, bytes: usize) {
    m.observe_gauge(names::NET_BROADCAST_PAYLOAD_BYTES, bytes as f64);
}

/// A reliable broadcast was refused for exceeding the max-payload ceiling.
pub fn broadcast_oversize_dropped(m: &mut MetricsRegistry) {
    m.incr(names::NET_BROADCAST_OVERSIZE_DROPPED_COUNT);
}

// ---- loading / state machine ----------------------------------------------

/// A PDS record fetch resolved; `secs` is its spawn→resolve latency.
pub fn record_fetch_latency_secs(m: &mut MetricsRegistry, secs: f64) {
    m.observe_hist(names::LOADING_RECORD_FETCH_LATENCY_MS, ms(secs));
}

/// A record fetch fired a retry.
pub fn record_fetch_retry(m: &mut MetricsRegistry) {
    m.incr(names::LOADING_RECORD_FETCH_RETRY_COUNT);
}

/// The loading gate closed; `secs` is total wall time spent in the gate.
pub fn loading_gate_total_secs(m: &mut MetricsRegistry, secs: f64) {
    m.observe_gauge(names::LOADING_GATE_TOTAL_SECS, secs);
}

// ---- async / offload ------------------------------------------------------

/// A heightmap generation job completed; `secs` is its spawn→complete latency.
pub fn heightmap_latency_secs(m: &mut MetricsRegistry, secs: f64) {
    m.observe_hist(names::OFFLOAD_HEIGHTMAP_LATENCY_MS, ms(secs));
}

/// An ambient-audio bake job completed; `secs` is its spawn→complete latency.
pub fn ambient_bake_latency_secs(m: &mut MetricsRegistry, secs: f64) {
    m.observe_hist(names::OFFLOAD_AMBIENT_BAKE_LATENCY_MS, ms(secs));
}

/// A splat/texture bake job completed; `secs` is its spawn→complete latency.
pub fn texture_bake_latency_secs(m: &mut MetricsRegistry, secs: f64) {
    m.observe_hist(names::OFFLOAD_TEXTURE_BAKE_LATENCY_MS, ms(secs));
}

/// An offloaded job yielded an unexpected result variant (a job failure).
pub fn offload_job_error(m: &mut MetricsRegistry) {
    m.incr(names::OFFLOAD_JOB_ERROR_COUNT);
}

// ---- runtime health -------------------------------------------------------

/// The local player fell through the terrain and was respawned.
pub fn player_respawned(m: &mut MetricsRegistry) {
    m.incr(names::RUNTIME_RESPAWN_COUNT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_increment_under_the_expected_names() {
        let mut m = MetricsRegistry::default();
        peer_connected(&mut m);
        peer_connected(&mut m);
        identity_spoof_rejected(&mut m);
        player_respawned(&mut m);
        assert_eq!(
            m.counter(names::NET_PEER_CONNECTED_COUNT).unwrap().value(),
            2
        );
        assert_eq!(
            m.counter(names::NET_IDENTITY_SPOOFED_COUNT)
                .unwrap()
                .value(),
            1
        );
        assert_eq!(m.counter(names::RUNTIME_RESPAWN_COUNT).unwrap().value(), 1);
    }

    #[test]
    fn latency_helpers_convert_seconds_to_millis() {
        let mut m = MetricsRegistry::default();
        record_fetch_latency_secs(&mut m, 1.5); // 1.5 s → 1500 ms
        heightmap_latency_secs(&mut m, 0.25); // 0.25 s → 250 ms
        let d = m
            .hist_distro(names::LOADING_RECORD_FETCH_LATENCY_MS)
            .unwrap();
        assert_eq!(d.max, 1500.0);
        let h = m.hist_distro(names::OFFLOAD_HEIGHTMAP_LATENCY_MS).unwrap();
        assert_eq!(h.max, 250.0);
    }

    #[test]
    fn gate_total_records_a_gauge() {
        let mut m = MetricsRegistry::default();
        loading_gate_total_secs(&mut m, 4.2);
        assert_eq!(m.gauge(names::LOADING_GATE_TOTAL_SECS).unwrap().last(), 4.2);
    }
}
