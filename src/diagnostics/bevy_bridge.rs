//! Bevy-diagnostics bridge + `MetricsPlugin` (Spine E-3).
//!
//! Registers Bevy's built-in diagnostic plugins (which the app did not use
//! before) and scrapes them into the shared [`MetricsRegistry`] once per second,
//! alongside a few game-specific gauges the built-ins don't cover (asset-handle
//! counts, collider count, the upstream `ShapeMeshCache` length — the
//! unbounded-growth leak watch).
//!
//! `SystemInformationDiagnosticsPlugin` is native-only; on wasm it is absent, so
//! `scrape_wasm_memory` substitutes a `runtime.memory.wasm_bytes` gauge read
//! straight from `WebAssembly.Memory` — the heap-never-shrinks watch.

use std::time::Duration;

use bevy::audio::{AudioPlayer, AudioSource, PlaybackMode, PlaybackSettings};
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
            (
                scrape_bevy_diagnostics,
                scrape_signal_diagnostics,
                scrape_audio_diagnostics,
                scrape_visible_entities,
                emit_metric_snapshot,
            )
                .chain()
                .run_if(on_timer(Duration::from_secs(1))),
        );
        #[cfg(target_arch = "wasm32")]
        {
            app.add_systems(
                Update,
                scrape_wasm_memory.run_if(on_timer(Duration::from_secs(1))),
            );
            // Crash-surviving session-log tail (#811): recover the previous
            // session's persisted tail at boot, then persist this session's
            // tail every few seconds so an OOM trap can no longer take the
            // evidence down with the tab.
            app.add_systems(Startup, || {
                crate::diagnostics::crash_log::recover_previous_session_log();
            });
            app.add_systems(
                Update,
                crate::diagnostics::crash_log::persist_session_tail
                    .run_if(on_timer(Duration::from_secs(5))),
            );
        }
    }
}

/// One gibibyte in bytes — the `SystemInformationDiagnosticsPlugin` reports
/// process memory in GiB, but our metric is named `…_bytes`, so convert.
/// (Native-only: the wasm memory gauge reads the linear-memory byte length directly.)
#[cfg(not(target_arch = "wasm32"))]
const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Scrape the Bevy diagnostics + game asset/collider counts into the registry.
/// Runs at 1 Hz. Reads `smoothed()` (falling back to the raw `value()`) so the
/// gauges are stable rather than per-frame-noisy.
fn scrape_bevy_diagnostics(
    store: Res<DiagnosticsStore>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<StandardMaterial>>,
    images: Res<Assets<Image>>,
    colliders: Query<(), With<avian3d::prelude::Collider>>,
    shape_cache: Option<Res<bevy_symbios_shape::cache::ShapeMeshCache>>,
    mut reg: ResMut<MetricsRegistry>,
) {
    let read = |p: &DiagnosticPath| {
        store
            .get(p)
            .and_then(|d| d.smoothed().or_else(|| d.value()))
    };

    if let Some(v) = read(&FrameTimeDiagnosticsPlugin::FRAME_TIME) {
        reg.observe_gauge(names::RUNTIME_FRAME_TIME_MS, v);
    }
    if let Some(v) = read(&FrameTimeDiagnosticsPlugin::FPS) {
        reg.observe_gauge(names::RUNTIME_FPS, v);
    }
    if let Some(v) = read(&EntityCountDiagnosticsPlugin::ENTITY_COUNT) {
        reg.observe_gauge(names::RUNTIME_ENTITY_COUNT, v);
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        use bevy::diagnostic::SystemInformationDiagnosticsPlugin as Sys;
        if let Some(gib) = read(&Sys::PROCESS_MEM_USAGE) {
            reg.observe_gauge(names::RUNTIME_MEMORY_PROCESS_RSS_BYTES, gib * BYTES_PER_GIB);
        }
        if let Some(pct) = read(&Sys::PROCESS_CPU_USAGE) {
            reg.observe_gauge(names::RUNTIME_CPU_USAGE_PCT, pct);
        }
    }

    // Game-specific gauges the built-ins don't cover: asset-handle counts (leak
    // watch), collider count (a double signals a duplicate terrain body), and
    // the upstream ShapeMeshCache length (the documented unbounded-growth leak).
    reg.observe_gauge(names::RUNTIME_MESH_HANDLE_COUNT, meshes.len() as f64);
    reg.observe_gauge(names::RUNTIME_MATERIAL_HANDLE_COUNT, materials.len() as f64);
    // Image-asset registry: the dominant memory consumer across a region re-seed
    // and the one the mesh/material counts miss (caches retain `Handle<Image>`) —
    // watches whether textures actually shrink after a rebuild/logout (#625).
    reg.observe_gauge(names::RUNTIME_IMAGE_HANDLE_COUNT, images.len() as f64);
    reg.observe_gauge(
        names::RUNTIME_COLLIDER_COUNT,
        colliders.iter().count() as f64,
    );
    if let Some(cache) = shape_cache {
        reg.observe_gauge(names::RUNTIME_SHAPE_MESH_CACHE_LEN, cache.len() as f64);
    }

    // The splat material's texture-slot footprint is a compile-time constant
    // (cfg-split for the native-only stains overlay); surface it as a gauge so
    // the GUI can show headroom against the WebGL2 16-slot ceiling (C-5).
    reg.observe_gauge(
        names::RUNTIME_TEXTURE_BIND_SLOTS,
        crate::splat::SPLAT_TEXTURE_BIND_SLOTS as f64,
    );
}

/// Scrape the spatial-audio load into the registry at 1 Hz (#802): the count
/// of live *looping* voices (construct hums + avatar engine voices — the
/// sustained-lag suspect, distinct from transient one-shot SFX) and the baked
/// cache's retained entry count + byte footprint. `Option` params keep this
/// inert on any app configured without the audio assets / bake cache (e.g. a
/// minimal test app) rather than panicking on a missing resource.
fn scrape_audio_diagnostics(
    voices: Query<&PlaybackSettings, With<AudioPlayer>>,
    bake_cache: Option<Res<crate::world_builder::spatial_audio::BakedAudioCache>>,
    audio_sources: Option<Res<Assets<AudioSource>>>,
    mut reg: ResMut<MetricsRegistry>,
) {
    let looping = voices
        .iter()
        .filter(|s| matches!(s.mode, PlaybackMode::Loop))
        .count();
    reg.observe_gauge(names::AUDIO_SPATIAL_ACTIVE_SINKS, looping as f64);

    if let (Some(cache), Some(sources)) = (bake_cache, audio_sources) {
        let (entries, bytes) = cache.retained_footprint(&sources);
        reg.observe_gauge(names::AUDIO_BAKE_CACHE_ENTRIES, entries as f64);
        reg.observe_gauge(names::AUDIO_BAKE_CACHE_BYTES, bytes as f64);
    }
}

/// Post-culling visible-entity total, summed over every mesh class of every
/// view — the #811 discriminator. On WebGL2 the per-frame CPU staging
/// (instance uniforms) scales with this number, so the next captured session
/// either shows wasm heap steps tracking visible-count peaks (confirming the
/// GPU-stall staging-pileup diagnosis) or steps without a peak (refuting it).
/// 1 Hz `last`-value sampling is deliberate: the jank episodes that matter
/// run multi-second, so a peak can't hide between scrapes for long.
fn scrape_visible_entities(
    views: Query<&bevy::camera::visibility::VisibleEntities>,
    mut reg: ResMut<MetricsRegistry>,
) {
    let visible: usize = views
        .iter()
        .flat_map(|v| v.entities.values())
        .map(Vec::len)
        .sum();
    reg.observe_gauge(names::RUNTIME_VISIBLE_ENTITY_COUNT, visible as f64);
}

/// Mirror the multiuser signaller's `SignalDiagnostics` counters into the
/// registry (1 Hz, chained with the other scrapes), derive the `awaiting_peers`
/// stall flag from live [`RemotePeer`](crate::state::RemotePeer) presence, and
/// emit a `SocketPeerListReceived` event the first time each new non-empty
/// `peer_list` is observed.
///
/// This is the app's only window into the WebRTC *signalling* layer: matchbox
/// surfaces just `Connected`/`Disconnected` to the plugin, so without this a
/// glared or ICE-failed handshake (relay reported peers, no data channel ever
/// opens) is indistinguishable from being genuinely alone. See the
/// `net.signal_glare_suspected` invariant, which fires off `awaiting_peers`.
#[allow(clippy::too_many_arguments)]
fn scrape_signal_diagnostics(
    diag: Option<Res<bevy_symbios_multiuser::prelude::SignalDiagnosticsRes>>,
    remote_peers: Query<(), With<crate::state::RemotePeer>>,
    mut reg: ResMut<MetricsRegistry>,
    mut log: ResMut<crate::diagnostics::SessionLog>,
    time: Res<Time>,
    mut last_peer_lists_seen: Local<u64>,
    mut connected_since_peer_list: Local<bool>,
    mut last_auth_rejections: Local<u64>,
) {
    use std::sync::atomic::Ordering::Relaxed;
    let Some(diag) = diag else {
        return;
    };
    let d = &diag.0;

    let peer_list_len = d.last_peer_list_len.load(Relaxed);
    let peer_lists_received = d.peer_lists_received.load(Relaxed);
    let auth_rejections = d.auth_rejections.load(Relaxed);
    reg.observe_gauge(names::NET_SIGNAL_PEER_LIST_LEN, peer_list_len as f64);
    reg.observe_gauge(
        names::NET_SIGNAL_OFFERS_INITIATED,
        d.offers_initiated.load(Relaxed) as f64,
    );
    reg.observe_gauge(
        names::NET_SIGNAL_OFFERS_SENT,
        d.offers_sent.load(Relaxed) as f64,
    );
    reg.observe_gauge(
        names::NET_SIGNAL_OFFERS_RECEIVED,
        d.offers_received.load(Relaxed) as f64,
    );
    reg.observe_gauge(
        names::NET_SIGNAL_ANSWERS_SENT,
        d.answers_sent.load(Relaxed) as f64,
    );
    reg.observe_gauge(
        names::NET_SIGNAL_ANSWERS_RECEIVED,
        d.answers_received.load(Relaxed) as f64,
    );
    reg.observe_gauge(names::NET_SIGNAL_AUTH_REJECTIONS, auth_rejections as f64);

    // A relay handshake rejection (chiefly an expired-token 401) leaves no other
    // trace — the socket never opens. Emit one event per new rejection so it
    // shows up in the session log / analyzer instead of only the console.
    if auth_rejections > *last_auth_rejections {
        *last_auth_rejections = auth_rejections;
        log.warn(
            time.elapsed_secs_f64(),
            crate::diagnostics::event::EventPayload::RelayAuthRejected {
                status: d.last_reject_status.load(Relaxed),
                total: auth_rejections,
            },
        );
    }

    let connected = remote_peers.iter().count();

    // A newly-received peer_list resets the "connected since?" latch and (when
    // non-empty) logs a one-shot event so a post-mortem can separate "joined a
    // populated room" from "alone". `peer_lists_received` is cumulative and
    // monotonic, so a strict increase is the rising edge of a fresh handshake.
    if peer_lists_received > *last_peer_lists_seen {
        *last_peer_lists_seen = peer_lists_received;
        *connected_since_peer_list = false;
        if peer_list_len >= 1 {
            log.info(
                time.elapsed_secs_f64(),
                crate::diagnostics::event::EventPayload::SocketPeerListReceived {
                    count: peer_list_len,
                },
            );
        }
    }
    if connected >= 1 {
        *connected_since_peer_list = true;
    }

    // `awaiting_peers`: the relay reported peers at join, none have connected,
    // and none have connected since that peer_list — so a peer that connected
    // then later left does not re-raise the flag. The `GlareSuspected`
    // invariant fires when this stays `1` over a sustained window.
    let awaiting = peer_list_len >= 1 && connected == 0 && !*connected_since_peer_list;
    reg.observe_gauge(
        names::NET_SIGNAL_AWAITING_PEERS,
        if awaiting { 1.0 } else { 0.0 },
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
fn scrape_wasm_memory(mut reg: ResMut<MetricsRegistry>) {
    use wasm_bindgen::JsCast;
    if let Ok(mem) = wasm_bindgen::memory().dyn_into::<js_sys::WebAssembly::Memory>() {
        let bytes = mem
            .buffer()
            .dyn_into::<js_sys::ArrayBuffer>()
            .map(|b| b.byte_length());
        if let Ok(bytes) = bytes {
            reg.observe_gauge(names::RUNTIME_MEMORY_WASM_BYTES, bytes as f64);
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
        // The image-asset registry gauge (the #625 leak-signal metric).
        assert!(reg.gauge(names::RUNTIME_IMAGE_HANDLE_COUNT).is_some());
        assert!(reg.counter(names::NET_PEER_CONNECTED_COUNT).is_some());
        assert!(
            reg.histogram(names::NET_JITTER_PLAYOUT_LATENCY_MS)
                .is_some()
        );
        // Empty until observed.
        assert!(reg.gauge(names::RUNTIME_FRAME_TIME_MS).unwrap().is_empty());
    }
}
