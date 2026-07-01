//! Bevy-diagnostics bridge + `MetricsPlugin` (Spine E-3).
//!
//! Registers Bevy's built-in diagnostic plugins (which the app did not use
//! before) and scrapes them into the shared [`MetricsRegistry`] once per second,
//! alongside a few game-specific gauges the built-ins don't cover (asset-handle
//! counts, collider count, the upstream `ShapeMeshCache` length — the
//! unbounded-growth leak watch).
//!
//! `SystemInformationDiagnosticsPlugin` is native-only; on wasm it is absent, so
//! [`scrape_wasm_memory`] substitutes a `runtime.memory.wasm_bytes` gauge read
//! straight from `WebAssembly.Memory` — the heap-never-shrinks watch.

use std::time::Duration;

use bevy::diagnostic::{
    DiagnosticPath, DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin,
};
use bevy::prelude::*;
use bevy::time::common_conditions::on_timer;

use crate::diagnostics::MetricsRegistry;
use crate::diagnostics::names;

/// Registers the Bevy diagnostic plugins + the 1 Hz scrape into the registry.
pub struct MetricsPlugin;

impl Plugin for MetricsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            EntityCountDiagnosticsPlugin::default(),
        ));
        // Process memory / CPU come from sysinfo, which is native-only; wasm
        // reads linear memory directly instead (see scrape_wasm_memory).
        #[cfg(not(target_arch = "wasm32"))]
        app.add_plugins(bevy::diagnostic::SystemInformationDiagnosticsPlugin);

        let mut registry = MetricsRegistry::default();
        registry.preseed(names::ALL);
        app.insert_resource(registry);

        app.add_systems(
            Update,
            (scrape_bevy_diagnostics, emit_metric_snapshot)
                .chain()
                .run_if(on_timer(Duration::from_secs(1))),
        );
        #[cfg(target_arch = "wasm32")]
        app.add_systems(
            Update,
            scrape_wasm_memory.run_if(on_timer(Duration::from_secs(1))),
        );
    }
}

/// One gibibyte in bytes — the `SystemInformationDiagnosticsPlugin` reports
/// process memory in GiB, but our metric is named `…_bytes`, so convert.
const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Scrape the Bevy diagnostics + game asset/collider counts into the registry.
/// Runs at 1 Hz. Reads `smoothed()` (falling back to the raw `value()`) so the
/// gauges are stable rather than per-frame-noisy.
#[allow(clippy::too_many_arguments)]
fn scrape_bevy_diagnostics(
    store: Res<DiagnosticsStore>,
    time: Res<Time>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<StandardMaterial>>,
    colliders: Query<(), With<avian3d::prelude::Collider>>,
    shape_cache: Option<Res<bevy_symbios_shape::cache::ShapeMeshCache>>,
    mut reg: ResMut<MetricsRegistry>,
) {
    let now = time.elapsed_secs_f64();
    let read = |p: &DiagnosticPath| {
        store
            .get(p)
            .and_then(|d| d.smoothed().or_else(|| d.value()))
    };

    if let Some(v) = read(&FrameTimeDiagnosticsPlugin::FRAME_TIME) {
        reg.observe_gauge(names::RUNTIME_FRAME_TIME_MS, v, now);
    }
    if let Some(v) = read(&FrameTimeDiagnosticsPlugin::FPS) {
        reg.observe_gauge(names::RUNTIME_FPS, v, now);
    }
    if let Some(v) = read(&EntityCountDiagnosticsPlugin::ENTITY_COUNT) {
        reg.observe_gauge(names::RUNTIME_ENTITY_COUNT, v, now);
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        use bevy::diagnostic::SystemInformationDiagnosticsPlugin as Sys;
        if let Some(gib) = read(&Sys::PROCESS_MEM_USAGE) {
            reg.observe_gauge(
                names::RUNTIME_MEMORY_PROCESS_RSS_BYTES,
                gib * BYTES_PER_GIB,
                now,
            );
        }
        if let Some(pct) = read(&Sys::PROCESS_CPU_USAGE) {
            reg.observe_gauge(names::RUNTIME_CPU_USAGE_PCT, pct, now);
        }
    }

    // Game-specific gauges the built-ins don't cover: asset-handle counts (leak
    // watch), collider count (a double signals a duplicate terrain body), and
    // the upstream ShapeMeshCache length (the documented unbounded-growth leak).
    reg.observe_gauge(names::RUNTIME_MESH_HANDLE_COUNT, meshes.len() as f64, now);
    reg.observe_gauge(
        names::RUNTIME_MATERIAL_HANDLE_COUNT,
        materials.len() as f64,
        now,
    );
    reg.observe_gauge(
        names::RUNTIME_COLLIDER_COUNT,
        colliders.iter().count() as f64,
        now,
    );
    if let Some(cache) = shape_cache {
        reg.observe_gauge(names::RUNTIME_SHAPE_MESH_CACHE_LEN, cache.len() as f64, now);
    }

    // The splat material's texture-slot footprint is a compile-time constant
    // (cfg-split for the native-only stains overlay); surface it as a gauge so
    // the GUI can show headroom against the WebGL2 16-slot ceiling (C-5).
    reg.observe_gauge(
        names::RUNTIME_TEXTURE_BIND_SLOTS,
        crate::splat::SPLAT_TEXTURE_BIND_SLOTS as f64,
        now,
    );
}

/// Record a flat [`MetricSnapshot`](crate::diagnostics::registry::MetricSnapshot)
/// into the session log once per second (E-5), so a post-mortem can chart metric
/// trends. Uses the **file-only** record path so these high-frequency snapshots
/// land in the durable file for the analyzer without crowding the GUI event log
/// or evicting real events from the bounded ring. Runs chained after the scrape
/// so the snapshot reflects this tick's freshly-scraped values.
fn emit_metric_snapshot(
    reg: Res<MetricsRegistry>,
    mut log: ResMut<crate::diagnostics::SessionLog>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    let snap = reg.snapshot(now);
    if snap.gauges.is_empty() && snap.counters.is_empty() && snap.histograms.is_empty() {
        return; // nothing observed yet — don't log an empty snapshot
    }
    log.record_file_only(
        now,
        crate::diagnostics::event::Severity::Trace,
        crate::diagnostics::event::EventPayload::MetricsSnapshot(Box::new(snap)),
    );
}

/// Wasm memory fallback: `SystemInformationDiagnosticsPlugin` is unavailable on
/// wasm, so read the WebAssembly linear-memory byte length directly. This is the
/// heap-never-shrinks signal for the WASM memory watch.
#[cfg(target_arch = "wasm32")]
fn scrape_wasm_memory(time: Res<Time>, mut reg: ResMut<MetricsRegistry>) {
    use wasm_bindgen::JsCast;
    if let Ok(mem) = wasm_bindgen::memory().dyn_into::<js_sys::WebAssembly::Memory>() {
        let bytes = mem
            .buffer()
            .dyn_into::<js_sys::ArrayBuffer>()
            .map(|b| b.byte_length());
        if let Ok(bytes) = bytes {
            reg.observe_gauge(
                names::RUNTIME_MEMORY_WASM_BYTES,
                bytes as f64,
                time.elapsed_secs_f64(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_plugin_builds_and_preseeds_catalogue() {
        // The 1 Hz scrape is gated off in a single-frame test, so this exercises
        // plugin construction (the Bevy-diagnostic API surface) + the preseed.
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(MetricsPlugin);
        app.update();

        let reg = app.world().resource::<MetricsRegistry>();
        // Every catalogued metric is present as an empty entry after preseed.
        assert!(reg.gauge(names::RUNTIME_FRAME_TIME_MS).is_some());
        assert!(reg.gauge(names::RUNTIME_ENTITY_COUNT).is_some());
        assert!(reg.counter(names::NET_PEER_CONNECTED_COUNT).is_some());
        assert!(
            reg.histogram(names::NET_JITTER_PLAYOUT_LATENCY_MS)
                .is_some()
        );
        // Empty until observed.
        assert!(reg.gauge(names::RUNTIME_FRAME_TIME_MS).unwrap().is_empty());
    }
}
