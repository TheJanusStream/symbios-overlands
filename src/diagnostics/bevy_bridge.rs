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

        // Per-frame, in `Last`: a hitch is a single long frame, and the 1 Hz
        // scrape below aliases it away (#1144). This folds every frame's delta
        // into a running max the scrape then publishes and resets.
        app.init_resource::<FrameHitches>();
        app.add_systems(Last, fold_frame_hitches);

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
                (scrape_wasm_memory, scrape_alloc_track).run_if(on_timer(Duration::from_secs(1))),
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

/// The build caches whose lengths are scraped as gauges.
///
/// Bundled into one [`bevy::ecs::system::SystemParam`] rather than listed
/// inline: every one of
/// them retains asset handles across a rebuild, so they are read as a group
/// when attributing asset-registry growth, and grouping keeps
/// [`scrape_bevy_diagnostics`] under the argument-count lint.
///
/// All optional — the render-adjacent plugins that own them may not be
/// installed (headless tools, tests).
#[derive(bevy::ecs::system::SystemParam)]
struct CacheGauges<'w> {
    shape_mesh: Option<Res<'w, bevy_symbios_shape::cache::ShapeMeshCache>>,
    prim_mesh: Option<Res<'w, crate::world_builder::prim_cache::PrimMeshCache>>,
    prim_material: Option<Res<'w, crate::world_builder::prim_cache::PrimMaterialCache>>,
    texture: Option<Res<'w, bevy_symbios_texture::TextureCache>>,
}

/// The asset registries whose handle counts the scraper gauges — bundled to
/// keep `scrape_bevy_diagnostics` under clippy's parameter ceiling.
#[derive(bevy::ecs::system::SystemParam)]
struct AssetStores<'w> {
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<StandardMaterial>>,
    images: Res<'w, Assets<Image>>,
}

/// Scrape the Bevy diagnostics + game asset/collider counts into the registry.
/// Runs at 1 Hz. Reads `smoothed()` (falling back to the raw `value()`) so the
/// gauges are stable rather than per-frame-noisy.
/// The worst frame seen since the last scrape, and every frame that ran long.
///
/// Accumulated per frame because the thing being measured IS a single frame.
/// The 1 Hz scrape's `FrameTimeDiagnosticsPlugin::FRAME_TIME` is an EMA with a
/// ~16.5 ms time constant, so it has forgotten a 500 ms stall by the time the
/// next scrape lands, and `runtime.frame_time_spike` — which reads that one
/// sample — only ever fired on sustained load. The suite's stated purpose is
/// catching jank on wasm, where the hitch sources that matter now (a
/// rigged-body install, a world-compile slice, a texture upload, an egui panel
/// rebuild after the 0.19 train) are all sub-second events.
#[derive(Resource, Default)]
pub struct FrameHitches {
    /// Longest frame since the last scrape, in ms.
    max_ms: f64,
    /// Frames over [`FRAME_HITCH_MS`](crate::config::diagnostics::FRAME_HITCH_MS)
    /// since the last scrape, in ms, oldest first.
    hitches: Vec<f64>,
}

impl FrameHitches {
    /// Fold one frame in.
    fn observe(&mut self, ms: f64) {
        self.max_ms = self.max_ms.max(ms);
        if ms > crate::config::diagnostics::FRAME_HITCH_MS {
            // Bounded: a session that stalls every frame must not grow this
            // between two scrapes. Keeping the WORST is what a histogram of
            // hitches is for; `max_ms` is unaffected either way.
            const MAX_HELD: usize = 64;
            if self.hitches.len() < MAX_HELD {
                self.hitches.push(ms);
            }
        }
    }

    /// Publish and reset. Returns the max so the caller can gauge it.
    fn drain_into(&mut self, reg: &mut MetricsRegistry) -> f64 {
        for ms in self.hitches.drain(..) {
            reg.observe_hist(names::RUNTIME_FRAME_HITCH_MS, ms);
        }
        std::mem::replace(&mut self.max_ms, 0.0)
    }
}

/// `Last`-schedule fold of this frame's delta into [`FrameHitches`].
fn fold_frame_hitches(time: Res<Time>, mut hitches: ResMut<FrameHitches>) {
    hitches.observe(time.delta_secs_f64() * 1000.0);
}

fn scrape_bevy_diagnostics(
    store: Res<DiagnosticsStore>,
    assets: AssetStores<'_>,
    colliders: Query<(), With<avian3d::prelude::Collider>>,
    caches: CacheGauges<'_>,
    mut reg: ResMut<MetricsRegistry>,
    mut hitches: ResMut<FrameHitches>,
    // Last-seen full-rebuild counter, for the per-rebuild asset marks below.
    mut last_rebuild_seen: Local<u64>,
) {
    let read = |p: &DiagnosticPath| {
        store
            .get(p)
            .and_then(|d| d.smoothed().or_else(|| d.value()))
    };

    if let Some(v) = read(&FrameTimeDiagnosticsPlugin::FRAME_TIME) {
        reg.observe_gauge(names::RUNTIME_FRAME_TIME_MS, v);
    }
    // The per-frame fold (#1144), published once per scrape and reset. The
    // gauge is always written, including the healthy 16 ms case, so the rule
    // below reads a current value rather than the last bad one.
    let max_ms = hitches.drain_into(&mut reg);
    reg.observe_gauge(names::RUNTIME_FRAME_TIME_MAX_MS, max_ms);
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
    reg.observe_gauge(names::RUNTIME_MESH_HANDLE_COUNT, assets.meshes.len() as f64);
    reg.observe_gauge(
        names::RUNTIME_MATERIAL_HANDLE_COUNT,
        assets.materials.len() as f64,
    );
    // Image-asset registry: the dominant memory consumer across a region re-seed
    // and the one the mesh/material counts miss (caches retain `Handle<Image>`) —
    // watches whether textures actually shrink after a rebuild/logout (#625).
    reg.observe_gauge(
        names::RUNTIME_IMAGE_HANDLE_COUNT,
        assets.images.len() as f64,
    );
    reg.observe_gauge(
        names::RUNTIME_COLLIDER_COUNT,
        colliders.iter().count() as f64,
    );
    if let Some(cache) = &caches.shape_mesh {
        reg.observe_gauge(names::RUNTIME_SHAPE_MESH_CACHE_LEN, cache.len() as f64);
    }
    // Caches that survive a rebuild hold `Handle<Mesh>` / `Handle<Image>`, so
    // their lengths are the terms that explain the asset-count gauges above.
    // Without these, #919's growth was visible only as its downstream effect
    // and could be attributed only by reading the GC.
    if let Some(cache) = &caches.prim_mesh {
        reg.observe_gauge(names::RUNTIME_PRIM_MESH_CACHE_LEN, cache.len() as f64);
    }
    if let Some(cache) = &caches.prim_material {
        reg.observe_gauge(names::RUNTIME_PRIM_MATERIAL_CACHE_LEN, cache.len() as f64);
    }
    // `None` for a disk-backed store, whose entries aren't resident — left
    // unreported rather than logged as 0, which would read as "empty".
    if let Some(n) = caches.texture.as_ref().and_then(|c| c.entry_count()) {
        reg.observe_gauge(names::RUNTIME_TEXTURE_CACHE_LEN, n as f64);
    }

    // The splat material's texture-slot footprint is a compile-time constant
    // (cfg-split for the native-only stains overlay); surface it as a gauge so
    // the GUI can show headroom against the WebGL2 16-slot ceiling (C-5).
    reg.observe_gauge(
        names::RUNTIME_TEXTURE_BIND_SLOTS,
        crate::splat::SPLAT_TEXTURE_BIND_SLOTS as f64,
    );

    // Per-rebuild asset marks (#921): when the executor's full-rebuild
    // counter has advanced since the last scrape, snapshot the handle
    // counts and process memory into the rebuild-anchored mark gauges the
    // asset-growth rules read. Taken here (≤1 s after completion, after the
    // spawn commands have applied) rather than inside the executor, which
    // would sample the asset registries mid-churn. One mark per scrape even
    // if two rebuilds landed inside the second — the rules compare
    // rebuild-boundary states, and the latest boundary is the one that
    // reflects what was actually released.
    let rebuilds = reg.counter_value(names::RUNTIME_FULL_REBUILD_COUNT);
    if rebuilds > *last_rebuild_seen {
        *last_rebuild_seen = rebuilds;
        reg.observe_gauge(
            names::RUNTIME_REBUILD_IMAGE_HANDLES,
            assets.images.len() as f64,
        );
        reg.observe_gauge(
            names::RUNTIME_REBUILD_MESH_HANDLES,
            assets.meshes.len() as f64,
        );
        // RSS on native, wasm linear memory on wasm — whichever this build
        // observes into the registry above. Read back rather than
        // re-derived so the mark can never disagree with the live gauge.
        let memory = reg
            .gauge(names::RUNTIME_MEMORY_PROCESS_RSS_BYTES)
            .or_else(|| reg.gauge(names::RUNTIME_MEMORY_WASM_BYTES))
            .filter(|g| !g.is_empty())
            .map(|g| g.last());
        if let Some(bytes) = memory {
            reg.observe_gauge(names::RUNTIME_REBUILD_MEMORY_BYTES, bytes);
        }
        // Texture-cache entry count at the same boundary (#981): the
        // image-growth rule subtracts the cache's expected pin count from
        // the image marks so warm-up toward the cap doesn't read as a
        // leak. Same read-back idiom as the memory mark; absent (never
        // observed) for disk-backed stores, and the rule falls back to
        // the unadjusted deltas.
        let cache_len = reg
            .gauge(names::RUNTIME_TEXTURE_CACHE_LEN)
            .filter(|g| !g.is_empty())
            .map(|g| g.last());
        if let Some(len) = cache_len {
            reg.observe_gauge(names::RUNTIME_REBUILD_TEXTURE_CACHE_LEN, len);
        }
    }
}

/// Scrape the spatial-audio load into the registry at 1 Hz (#802): the count
/// of live *looping* voices (construct hums + avatar engine voices — the
/// sustained-lag suspect, distinct from transient one-shot SFX) and the baked
/// cache's retained entry count + byte footprint. `Option` params keep this
/// inert on any app configured without the audio assets / bake cache (e.g. a
/// minimal test app) rather than panicking on a missing resource.
fn scrape_audio_diagnostics(
    voices: Query<&PlaybackSettings, With<AudioPlayer>>,
    // The one-shot half (#1252 f316). `ContactAudioVoice` exists precisely
    // for counting — `play_contact_audio` counts it every frame against
    // `MAX_CONCURRENT_VOICES` — and nothing in the diagnostics suite asked.
    contact_voices: Query<(), With<crate::interaction::audio::ContactAudioVoice>>,
    bake_cache: Option<Res<crate::world_builder::spatial_audio::BakedAudioCache>>,
    audio_sources: Option<Res<Assets<AudioSource>>>,
    mut reg: ResMut<MetricsRegistry>,
) {
    let looping = voices
        .iter()
        .filter(|s| matches!(s.mode, PlaybackMode::Loop))
        .count();
    reg.observe_gauge(names::AUDIO_SPATIAL_ACTIVE_SINKS, looping as f64);
    reg.observe_gauge(
        names::AUDIO_CONTACT_ACTIVE_VOICES,
        contact_voices.iter().count() as f64,
    );

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

/// Does the `awaiting_peers` glare flag hold this tick?
///
/// Pure so the table can be pinned; `MatchboxSocket` wraps a live
/// `WebRtcSocket` and cannot be built in a test, and this is the part with
/// the interesting cases.
///
/// Every term is a guard against claiming glare from evidence that does not
/// support it (#1215 f399):
/// * `socket_present` — an outage is not a glare. With no socket there is no
///   handshake to stall, and the signaller's counters are cumulative, so
///   without this the flag would raise itself over a dead link.
/// * `peer_list_valid` — the last `peer_list` describes the room a socket saw.
///   Once that socket is gone, so is the claim.
/// * `connected == 0` — the definition: nobody reached a data channel. Before
///   the #1213 sweep this is what a leftover ghost `RemotePeer` pinned false,
///   silencing the rule for the rest of the session.
/// * `!connected_since` — a peer that connected and then left is a room that
///   worked, not a stalled handshake.
fn awaiting_peers(
    socket_present: bool,
    peer_list_valid: bool,
    peer_list_len: u64,
    connected: usize,
    connected_since_peer_list: bool,
) -> bool {
    socket_present
        && peer_list_valid
        && peer_list_len >= 1
        && connected == 0
        && !connected_since_peer_list
}

/// Mirror the multiuser signaller's `SignalDiagnostics` counters into the
/// registry (1 Hz, chained with the other scrapes), derive the `awaiting_peers`
/// stall flag from live [`RemotePeer`](crate::state::RemotePeer) presence, and
/// emit a `SocketPeerListReceived` event every time a new `peer_list` is
/// observed — including an empty one (#1215 f408).
///
/// This is the app's only window into the WebRTC *signalling* layer: matchbox
/// surfaces just `Connected`/`Disconnected` to the plugin, so without this a
/// glared or ICE-failed handshake (relay reported peers, no data channel ever
/// opens) is indistinguishable from being genuinely alone. See the
/// `net.signal_glare_suspected` invariant, which fires off `awaiting_peers`.
///
/// Two things the socket's own presence decides here (#1215 f399). The
/// signaller's `SignalDiagnostics` counters are cumulative and survive a
/// socket teardown, so after one the last `peer_list` is evidence about a
/// socket that no longer exists: `peer_list_valid` retires it, and
/// `awaiting_peers` is held at 0 while no socket exists at all. Without that,
/// an outage — where nothing is glaring because nothing is connected — would
/// raise the glare flag off a stale list, and the "somebody connected since
/// the welcome" latch would carry a dead socket's answer into the next one.
#[allow(clippy::too_many_arguments)]
fn scrape_signal_diagnostics(
    diag: Option<Res<bevy_symbios_multiuser::prelude::SignalDiagnosticsRes>>,
    socket: Option<Res<bevy_symbios_multiuser::prelude::MatchboxSocket>>,
    remote_peers: Query<(), With<crate::state::RemotePeer>>,
    mut reg: ResMut<MetricsRegistry>,
    mut log: ResMut<crate::diagnostics::SessionLog>,
    time: Res<Time>,
    mut last_peer_lists_seen: Local<u64>,
    mut connected_since_peer_list: Local<bool>,
    mut peer_list_valid: Local<bool>,
    mut had_socket: Local<bool>,
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

    // The socket's falling edge retires everything the last handshake told
    // us: its peer_list describes a room this client can no longer see, and
    // the "somebody connected since the welcome" answer belonged to it.
    let socket_present = socket.is_some();
    if *had_socket && !socket_present {
        *peer_list_valid = false;
        *connected_since_peer_list = false;
    }
    *had_socket = socket_present;

    // A newly-received peer_list resets the "connected since?" latch and logs
    // a one-shot event so a post-mortem can separate "joined a populated
    // room" from "alone". `peer_lists_received` is cumulative and monotonic,
    // so a strict increase is the rising edge of a fresh handshake.
    //
    // Logged at EVERY count, zero included (#1215 f408). Under the old
    // `>= 1` guard the comment's own promise was unkeepable: "welcomed into
    // an empty room" and "never handshook at all" both produced no event and
    // were indistinguishable in the log — which is the first question anyone
    // asks when a user reports an empty world, and the on-screen surfaces
    // could not answer it either. `GlareSuspected`'s replay arm already
    // filters on `count >= 1`, so its behaviour is unchanged.
    if peer_lists_received > *last_peer_lists_seen {
        *last_peer_lists_seen = peer_lists_received;
        *connected_since_peer_list = false;
        *peer_list_valid = true;
        log.info(
            time.elapsed_secs_f64(),
            crate::diagnostics::event::EventPayload::SocketPeerListReceived {
                count: peer_list_len,
            },
        );
    }
    if connected >= 1 {
        *connected_since_peer_list = true;
    }

    // `awaiting_peers`: a LIVE socket, whose relay welcome reported peers,
    // none of which have connected — and none of which have connected since
    // that peer_list, so a peer that connected then later left does not
    // re-raise the flag. The `GlareSuspected` invariant fires when this stays
    // `1` over a sustained window; an outage must not raise it, because
    // nothing is glaring when nothing is connected at all.
    let awaiting = awaiting_peers(
        socket_present,
        *peer_list_valid,
        peer_list_len,
        connected,
        *connected_since_peer_list,
    );
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

/// Mirror the tracking allocator's size-class live-byte totals into gauges
/// and log a [`EventPayload::GiantAllocation`] fingerprint for every ≥ 16 MiB
/// allocation since the last scrape (#811). `Local` tracks the drained count
/// so each fingerprint is logged exactly once; a burst deeper than the
/// allocator's ring between scrapes loses the oldest sizes, never the newest
/// (the runaway's latest doublings are the identifying ones).
#[cfg(target_arch = "wasm32")]
fn scrape_alloc_track(
    mut reg: ResMut<MetricsRegistry>,
    mut log: ResMut<crate::diagnostics::SessionLog>,
    time: Res<Time>,
    mut giants_seen: Local<u64>,
) {
    use crate::alloc_track::wasm as alloc_track;

    let (small, medium, large, giant) = alloc_track::snapshot();
    reg.observe_gauge(names::RUNTIME_ALLOC_SMALL_BYTES, small as f64);
    reg.observe_gauge(names::RUNTIME_ALLOC_MEDIUM_BYTES, medium as f64);
    reg.observe_gauge(names::RUNTIME_ALLOC_LARGE_BYTES, large as f64);
    reg.observe_gauge(names::RUNTIME_ALLOC_GIANT_BYTES, giant as f64);

    let now = time.elapsed_secs_f64();
    for bytes in alloc_track::giant_sizes_since(*giants_seen) {
        log.info(
            now,
            crate::diagnostics::event::EventPayload::GiantAllocation { bytes },
        );
    }
    *giants_seen = alloc_track::giant_total();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// THE SEQUENCE: a room with peers, our link drops, the ghosts are swept.
    /// `awaiting_peers` used to be `peer_list_len >= 1 && connected == 0 &&
    /// !connected_since`, and the socket did not appear in it at all — so the
    /// one rule that can ever say anything about the local link raised itself
    /// off a `peer_list` belonging to a socket that no longer existed, and
    /// reported an outage as a glared handshake (#1215 f399).
    #[test]
    fn an_outage_is_not_a_glare() {
        // The glare case itself, unchanged: a live socket, the relay said
        // there were peers, nobody ever connected.
        assert!(awaiting_peers(true, true, 3, 0, false));

        // No socket: nothing is handshaking, so nothing is stalled.
        assert!(!awaiting_peers(false, true, 3, 0, false));
        // Socket back, but the peer_list belonged to the dead one.
        assert!(!awaiting_peers(true, false, 3, 0, false));
    }

    /// THE SEQUENCE: a peer connects, then leaves, then the room stalls. The
    /// two suppressors that must keep working — a peer on a data channel
    /// right now, and one that reached one earlier — because a room that
    /// worked is not a glared handshake. This is the term a leftover ghost
    /// `RemotePeer` used to pin, silencing the rule for the whole session.
    #[test]
    fn a_room_that_ever_worked_is_not_glaring() {
        assert!(!awaiting_peers(true, true, 3, 1, false));
        assert!(!awaiting_peers(true, true, 3, 0, true));
        // An empty room is not a stall either — there was nobody to reach.
        assert!(!awaiting_peers(true, true, 0, 0, false));
    }

    /// THE SEQUENCE: a user reports "the world was empty". The log has to say
    /// whether the relay welcomed us into an empty room or whether we never
    /// handshook at all — the distinction the event's own comment promises.
    /// Under the old `peer_list_len >= 1` guard both produced no event and
    /// were indistinguishable (#1215 f408).
    #[test]
    fn a_welcome_into_an_empty_room_is_logged_as_such() {
        use bevy_symbios_multiuser::prelude::SignalDiagnosticsRes;
        use std::sync::atomic::Ordering::Relaxed;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<MetricsRegistry>();
        app.init_resource::<crate::diagnostics::SessionLog>();
        let diag = SignalDiagnosticsRes::default();
        // One welcome handshake, naming an empty room.
        diag.0.peer_lists_received.store(1, Relaxed);
        diag.0.last_peer_list_len.store(0, Relaxed);
        app.insert_resource(diag);
        app.add_systems(Update, scrape_signal_diagnostics);
        app.update();

        let logged: Vec<u64> = app
            .world()
            .resource::<crate::diagnostics::SessionLog>()
            .iter()
            .filter_map(|e| match e.payload {
                crate::diagnostics::event::EventPayload::SocketPeerListReceived { count } => {
                    Some(count)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            logged,
            vec![0],
            "an empty-room welcome must be on the record, with its true count"
        );

        // And a session that never handshakes logs nothing, so the two are
        // distinguishable — which is the whole point.
        let mut quiet = App::new();
        quiet.add_plugins(MinimalPlugins);
        quiet.init_resource::<MetricsRegistry>();
        quiet.init_resource::<crate::diagnostics::SessionLog>();
        quiet.insert_resource(SignalDiagnosticsRes::default());
        quiet.add_systems(Update, scrape_signal_diagnostics);
        quiet.update();
        assert!(
            !quiet
                .world()
                .resource::<crate::diagnostics::SessionLog>()
                .iter()
                .any(|e| matches!(
                    e.payload,
                    crate::diagnostics::event::EventPayload::SocketPeerListReceived { .. }
                )),
            "no welcome, no event"
        );
    }

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
