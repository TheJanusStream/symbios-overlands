//! Built-in ECS-state invariant rules (Pillar D-3).
//!
//! Unlike the log-expressible rules (D-2), these read live engine state — the
//! metrics registry (collider / asset / frame-time / ShapeMeshCache gauges from
//! E-3) and the per-tick ECS scalars the tick system pre-gathers into
//! [`LiveCtx`] (player-vs-ground, NaN bodies, orphan visuals, respawns). They
//! are therefore **live-only**: no `replay` body, since a captured log can't
//! re-derive them. When they fire, the resulting `InvariantViolation` event is
//! recorded, so the offline analyzer can still *surface* (though not re-derive)
//! that they tripped.

use crate::diagnostics::anomaly::registry::InvariantRegistry;
use crate::diagnostics::anomaly::rule::{DebouncePolicy, LiveCtx, Rule, RuleHeader, Verdict};
use crate::diagnostics::event::{Severity, Subsystem};
use crate::diagnostics::names;
use crate::state::AppState;

/// Register the D-3 ECS-state rules. Called from [`super::rules::register_builtins`].
pub fn register_ecs_rules(reg: &mut InvariantRegistry) {
    reg.register(TerrainColliderMissing);
    reg.register(PlayerFellThroughTerrain);
    reg.register(NanInPhysics);
    reg.register(AssetHandleSpike);
    reg.register(AssetGrowthAcrossRebuilds);
    reg.register(MemoryRetentionAcrossRebuilds);
    reg.register(ShapeMeshCacheGrowth);
    reg.register(RespawnThrashing);
    reg.register(OrphanAvatarVisual);
    reg.register(FrameTimeSpike);
    reg.register(LoopingVoicesOverload);
    reg.register(WasmMemoryHigh);
    reg.register(WasmMemoryCritical);
}

/// Growth of a gauge across its retained sparkline window (newest − oldest),
/// or `None` if it has fewer than two samples.
fn gauge_window_growth(cx: &LiveCtx, name: &str) -> Option<f64> {
    let g = cx.metrics.gauge(name)?;
    if g.len() < 2 {
        return None;
    }
    let oldest = g.iter().next()?;
    Some(g.last() - oldest)
}

fn gauge_last(cx: &LiveCtx, name: &str) -> Option<f64> {
    Some(cx.metrics.gauge(name)?.last())
}

// --- TerrainColliderMissing -------------------------------------------------
struct TerrainColliderMissing;
const TERRAIN_COLLIDER_MISSING: RuleHeader = RuleHeader {
    id: "runtime.terrain_collider_missing",
    subsystem: Subsystem::Runtime,
    severity: Severity::Critical,
    debounce: DebouncePolicy::OncePerCondition,
    description: "no physics collider present in-game (nothing solid to stand on)",
    when_state: Some(AppState::InGame),
};
/// In-game dwell before the never-seen arm of `TerrainColliderMissing`
/// may fire (#869): the 1 Hz collider gauge's newest sample can predate
/// the terrain body on the InGame entry frame, which put a permanent
/// (OncePerCondition) CRITICAL into every session report at t≈gate-exit.
const TERRAIN_GRACE_SECS: f64 = 5.0;

impl Rule for TerrainColliderMissing {
    fn header(&self) -> &RuleHeader {
        &TERRAIN_COLLIDER_MISSING
    }
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        // The loading gate guarantees a solid terrain collider before InGame,
        // so zero colliders means the terrain body failed to spawn — but the
        // gauge samples at 1 Hz, so give the entry frame its grace (#869):
        // fire immediately only when colliders were SEEN and then vanished;
        // otherwise require a few seconds of in-game dwell first.
        let g = cx.metrics.gauge(names::RUNTIME_COLLIDER_COUNT)?;
        if g.last() >= 1.0 {
            return Some(Verdict::Clear);
        }
        let seen_then_lost = g.iter().any(|v| v >= 1.0);
        let dwell_elapsed = cx
            .ingame_elapsed_secs
            .is_some_and(|t| t >= TERRAIN_GRACE_SECS);
        Some(if seen_then_lost || dwell_elapsed {
            Verdict::violated("0 colliders in-game — terrain body missing")
        } else {
            Verdict::Clear
        })
    }
}

// --- PlayerFellThroughTerrain -----------------------------------------------
/// Rule threshold: the respawn safety net (`respawn_if_fallen`) teleports
/// the player back at `cfg::rover::FALL_BELOW_GROUND` within the same 64 Hz
/// physics tick, so this 1 Hz rule can only ever observe a depth beyond
/// that margin if the net FAILED to catch (heightmap gone, respawn system
/// wedged). Deriving from the same constant keeps the two from drifting
/// into overlap, where every ordinary fall would double-report (#672).
const FALL_BELOW_GROUND_M: f32 = crate::config::rover::FALL_BELOW_GROUND + 5.0;

struct PlayerFellThroughTerrain;
const PLAYER_FELL: RuleHeader = RuleHeader {
    id: "runtime.player_fell_through_terrain",
    subsystem: Subsystem::Runtime,
    severity: Severity::Error,
    debounce: DebouncePolicy::OncePerCondition,
    description: "local player dropped well below the terrain surface (respawn net missed)",
    when_state: Some(AppState::InGame),
};
impl Rule for PlayerFellThroughTerrain {
    fn header(&self) -> &RuleHeader {
        &PLAYER_FELL
    }
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let (y, ground) = (cx.player_y?, cx.ground_y?);
        Some(if y < ground - FALL_BELOW_GROUND_M {
            Verdict::violated(format!(
                "player y={y:.0} is {:.0}m below ground",
                ground - y
            ))
        } else {
            Verdict::Clear
        })
    }
}

// --- NanInPhysics -----------------------------------------------------------
struct NanInPhysics;
const NAN_IN_PHYSICS: RuleHeader = RuleHeader {
    id: "runtime.nan_in_physics",
    subsystem: Subsystem::Runtime,
    severity: Severity::Error,
    debounce: DebouncePolicy::OncePerCondition,
    description: "a dynamic physics body has a non-finite transform/velocity",
    when_state: None,
};
impl Rule for NanInPhysics {
    fn header(&self) -> &RuleHeader {
        &NAN_IN_PHYSICS
    }
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        Some(if cx.nan_body_count > 0 {
            Verdict::violated(format!(
                "{} physics bodies with NaN/Inf state",
                cx.nan_body_count
            ))
        } else {
            Verdict::Clear
        })
    }
}

// --- AssetHandleSpike -------------------------------------------------------
/// Mesh-handle growth over the ~2 min sparkline window that suggests a leak
/// (well above what a normal room load or a few prop spawns add). Tunable.
const MESH_GROWTH_LEAK: f64 = 5000.0;

struct AssetHandleSpike;
const ASSET_HANDLE_SPIKE: RuleHeader = RuleHeader {
    id: "runtime.asset_handle_spike",
    subsystem: Subsystem::Runtime,
    severity: Severity::Warn,
    debounce: DebouncePolicy::Interval(60.0),
    description: "mesh-handle count is growing steeply (possible asset leak)",
    when_state: None,
};
impl Rule for AssetHandleSpike {
    fn header(&self) -> &RuleHeader {
        &ASSET_HANDLE_SPIKE
    }
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let growth = gauge_window_growth(cx, names::RUNTIME_MESH_HANDLE_COUNT)?;
        Some(if growth > MESH_GROWTH_LEAK {
            Verdict::violated(format!("mesh handles +{growth:.0} over the window"))
        } else {
            Verdict::Clear
        })
    }
}

// --- AssetGrowthAcrossRebuilds ----------------------------------------------
// The #919-shaped watchdog. That leak was reported HEALTHY for a whole
// session because nothing watched the discriminating signal: not the handle
// count's *value* (legitimate load moves it arbitrarily) but the count
// **never falling across consecutive full rebuilds** — the boundary where
// everything unreferenced should have been released. The executor counts
// full rebuilds and the 1 Hz scraper snapshots handle counts into the
// `runtime.rebuild.*` mark gauges when the counter advances; these rules
// read that rebuild-anchored series, not the wall-clock sparkline.

/// Consecutive rebuild-to-rebuild deltas examined (needs one more mark).
const REBUILD_DELTA_WINDOW: usize = 4;
/// Handle growth over the window that reads as a leak. Calibrated against
/// the two known sessions: leaking (#919) stepped ~+90 images / ~+100
/// meshes per re-roll (window sum ≳ +360); healthy alternates sign and sums
/// well under ±60.
const ASSET_GROWTH_LEAK_TOTAL: f64 = 120.0;

/// The last [`REBUILD_DELTA_WINDOW`] rebuild-to-rebuild deltas of a mark
/// gauge, or `None` until enough full rebuilds have happened.
fn rebuild_deltas(cx: &LiveCtx, name: &str) -> Option<Vec<f64>> {
    let g = cx.metrics.gauge(name)?;
    if g.len() < REBUILD_DELTA_WINDOW + 1 {
        return None;
    }
    let marks: Vec<f64> = g.iter().collect();
    let tail = &marks[marks.len() - (REBUILD_DELTA_WINDOW + 1)..];
    Some(tail.windows(2).map(|w| w[1] - w[0]).collect())
}

/// `Some(total growth)` when the class leaked across the window: every
/// rebuild added handles (never falls) and the total is past the floor.
fn leaking_class(deltas: &[f64]) -> Option<f64> {
    let total: f64 = deltas.iter().sum();
    (deltas.iter().all(|d| *d >= 0.0) && total >= ASSET_GROWTH_LEAK_TOTAL).then_some(total)
}

struct AssetGrowthAcrossRebuilds;
const ASSET_GROWTH_ACROSS_REBUILDS: RuleHeader = RuleHeader {
    id: "runtime.asset_growth_across_rebuilds",
    subsystem: Subsystem::Runtime,
    severity: Severity::Warn,
    debounce: DebouncePolicy::Interval(60.0),
    description: "asset handles grew on every recent full rebuild and never fell (leak signature)",
    when_state: None,
};
impl Rule for AssetGrowthAcrossRebuilds {
    fn header(&self) -> &RuleHeader {
        &ASSET_GROWTH_ACROSS_REBUILDS
    }
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let image = rebuild_deltas(cx, names::RUNTIME_REBUILD_IMAGE_HANDLES);
        let mesh = rebuild_deltas(cx, names::RUNTIME_REBUILD_MESH_HANDLES);
        // Dormant until enough rebuilds; one class having a window is enough.
        if image.is_none() && mesh.is_none() {
            return None;
        }
        let image_leak = image.as_deref().and_then(leaking_class);
        let mesh_leak = mesh.as_deref().and_then(leaking_class);
        Some(match (image_leak, mesh_leak) {
            (None, None) => Verdict::Clear,
            (img, msh) => {
                let parts: Vec<String> = [("images", img), ("meshes", msh)]
                    .into_iter()
                    .filter_map(|(label, g)| g.map(|t| format!("{label} +{t:.0}")))
                    .collect();
                Verdict::violated(format!(
                    "handles rose on each of the last {REBUILD_DELTA_WINDOW} full rebuilds \
                     without ever falling ({})",
                    parts.join(", "),
                ))
            }
        })
    }
}

// --- MemoryRetentionAcrossRebuilds ------------------------------------------
/// Memory growth over the rebuild window that is worth a line when handles
/// are flat: ~52 MB/re-roll was measured after #919's handle leak was fixed
/// (allocator retention, #625), so four rebuilds ≈ 200 MB.
const RETENTION_MEMORY_TOTAL_BYTES: f64 = 150.0 * 1024.0 * 1024.0;

struct MemoryRetentionAcrossRebuilds;
const MEMORY_RETENTION_ACROSS_REBUILDS: RuleHeader = RuleHeader {
    id: "runtime.memory_retention_across_rebuilds",
    subsystem: Subsystem::Runtime,
    severity: Severity::Info,
    debounce: DebouncePolicy::Interval(300.0),
    description: "process memory climbs across full rebuilds while asset handles stay flat \
                  (allocator retention, not a handle leak)",
    when_state: None,
};
impl Rule for MemoryRetentionAcrossRebuilds {
    fn header(&self) -> &RuleHeader {
        &MEMORY_RETENTION_ACROSS_REBUILDS
    }
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let memory = rebuild_deltas(cx, names::RUNTIME_REBUILD_MEMORY_BYTES)?;
        // Only attribute the climb to retention when the handle story is
        // genuinely flat — if handles are moving, the growth rule above owns
        // the diagnosis and this one stays quiet rather than excusing it.
        let flat = |name: &str| {
            rebuild_deltas(cx, name)
                .is_some_and(|d| d.iter().sum::<f64>().abs() < ASSET_GROWTH_LEAK_TOTAL)
        };
        let grew: f64 = memory.iter().sum();
        Some(
            if grew >= RETENTION_MEMORY_TOTAL_BYTES
                && flat(names::RUNTIME_REBUILD_IMAGE_HANDLES)
                && flat(names::RUNTIME_REBUILD_MESH_HANDLES)
            {
                Verdict::violated(format!(
                    "+{:.0} MB over the last {REBUILD_DELTA_WINDOW} full rebuilds with asset \
                     handles flat — allocator retention (#625), expected on wasm, not a leak",
                    grew / (1024.0 * 1024.0)
                ))
            } else {
                Verdict::Clear
            },
        )
    }
}

// --- ShapeMeshCacheGrowth ---------------------------------------------------
/// Upstream `ShapeMeshCache` growth over the window — the documented
/// unbounded-growth leak. Tunable.
const SHAPE_CACHE_GROWTH_LEAK: f64 = 500.0;

struct ShapeMeshCacheGrowth;
const SHAPE_MESH_CACHE_GROWTH: RuleHeader = RuleHeader {
    id: "runtime.shape_mesh_cache_growth",
    subsystem: Subsystem::Runtime,
    severity: Severity::Warn,
    debounce: DebouncePolicy::Interval(60.0),
    description: "upstream ShapeMeshCache is growing unbounded (known leak)",
    when_state: None,
};
impl Rule for ShapeMeshCacheGrowth {
    fn header(&self) -> &RuleHeader {
        &SHAPE_MESH_CACHE_GROWTH
    }
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let growth = gauge_window_growth(cx, names::RUNTIME_SHAPE_MESH_CACHE_LEN)?;
        Some(if growth > SHAPE_CACHE_GROWTH_LEAK {
            Verdict::violated(format!("ShapeMeshCache +{growth:.0} over the window"))
        } else {
            Verdict::Clear
        })
    }
}

// --- RespawnThrashing -------------------------------------------------------
const RESPAWN_THRASH_LIMIT: u32 = 2;

struct RespawnThrashing;
const RESPAWN_THRASHING: RuleHeader = RuleHeader {
    id: "runtime.respawn_thrashing",
    subsystem: Subsystem::Runtime,
    severity: Severity::Warn,
    debounce: DebouncePolicy::Interval(10.0),
    description: "the player is respawning repeatedly in a short window",
    when_state: Some(AppState::InGame),
};
impl Rule for RespawnThrashing {
    fn header(&self) -> &RuleHeader {
        &RESPAWN_THRASHING
    }
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        Some(if cx.respawns_recent > RESPAWN_THRASH_LIMIT {
            Verdict::violated(format!(
                "{} respawns in the recent window",
                cx.respawns_recent
            ))
        } else {
            Verdict::Clear
        })
    }
}

// --- OrphanAvatarVisual -----------------------------------------------------
struct OrphanAvatarVisual;
const ORPHAN_AVATAR_VISUAL: RuleHeader = RuleHeader {
    id: "runtime.orphan_avatar_visual",
    subsystem: Subsystem::Runtime,
    severity: Severity::Info,
    debounce: DebouncePolicy::OncePerCondition,
    description: "avatar-visual entities are orphaned from any chassis",
    when_state: Some(AppState::InGame),
};
impl Rule for OrphanAvatarVisual {
    fn header(&self) -> &RuleHeader {
        &ORPHAN_AVATAR_VISUAL
    }
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        Some(if cx.orphan_avatar_count > 0 {
            Verdict::violated(format!("{} orphan avatar visuals", cx.orphan_avatar_count))
        } else {
            Verdict::Clear
        })
    }
}

// --- FrameTimeSpike ---------------------------------------------------------
/// Smoothed frame time above this (ms) is a sustained hitch (~< 20 fps).
const FRAME_SPIKE_MS: f64 = 50.0;

struct FrameTimeSpike;
const FRAME_TIME_SPIKE: RuleHeader = RuleHeader {
    id: "runtime.frame_time_spike",
    subsystem: Subsystem::Runtime,
    severity: Severity::Warn,
    debounce: DebouncePolicy::Interval(10.0),
    description: "sustained low frame rate",
    when_state: None,
};
impl Rule for FrameTimeSpike {
    fn header(&self) -> &RuleHeader {
        &FRAME_TIME_SPIKE
    }
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let ms = gauge_last(cx, names::RUNTIME_FRAME_TIME_MS)?;
        Some(if ms > FRAME_SPIKE_MS {
            Verdict::violated(format!("smoothed frame time {ms:.0}ms"))
        } else {
            Verdict::Clear
        })
    }
}

// --- LoopingVoicesOverload ---------------------------------------------------
/// Live looping spatial voices above this is audio overload (#802/#837):
/// every voice is a per-frame spatialise-and-mix, and past this count the
/// mixer drags the frame long before anything else looks unhealthy. A
/// dense themed room lands in the low tens; overload cases observed in
/// #802 ran well past this.
const LOOPING_VOICES_OVERLOAD_COUNT: f64 = 48.0;

struct LoopingVoicesOverload;
const LOOPING_VOICES_OVERLOAD: RuleHeader = RuleHeader {
    // `Offload` subsystem so the toolbar dot and tab badges route to the
    // Offload tab — the one that renders the Audio health card (with its
    // interpretation line + inline mute shortcut).
    id: "audio.looping_voices_overload",
    subsystem: Subsystem::Offload,
    severity: Severity::Warn,
    debounce: DebouncePolicy::Interval(30.0),
    description: "too many looping audio voices — mixing may drag the frame rate",
    when_state: None,
};
impl Rule for LoopingVoicesOverload {
    fn header(&self) -> &RuleHeader {
        &LOOPING_VOICES_OVERLOAD
    }
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let voices = gauge_last(cx, names::AUDIO_SPATIAL_ACTIVE_SINKS)?;
        Some(if voices > LOOPING_VOICES_OVERLOAD_COUNT {
            Verdict::violated(format!("{voices:.0} looping voices"))
        } else {
            Verdict::Clear
        })
    }
}

// --- WasmMemoryHigh / WasmMemoryCritical --------------------------------------
// The wasm32 linear memory tops out at 4 GiB and NEVER SHRINKS (dlmalloc keeps
// every grown page), so heap growth is a one-way trip: once allocation fails,
// the panic machinery itself can't allocate its message and the client dies as
// a bare `unreachable` trap — with the in-memory session log lost (#811, field
// crash at ~4 GiB). These rules turn the existing `runtime.memory.wasm_bytes`
// gauge into an escalating early warning while there is still headroom to
// download the log, save, and reload the tab. Native has no such gauge, so
// `eval` yields no verdict there and the rules stay dormant.

/// Warn tier — plenty of headroom left, but the ratchet only goes up.
const WASM_MEMORY_HIGH_BYTES: f64 = 2.5 * 1024.0 * 1024.0 * 1024.0;
/// Critical tier — allocation failure is plausibly one big compile away.
const WASM_MEMORY_CRITICAL_BYTES: f64 = 3.25 * 1024.0 * 1024.0 * 1024.0;

struct WasmMemoryHigh;
const WASM_MEMORY_HIGH: RuleHeader = RuleHeader {
    id: "runtime.wasm_memory_high",
    subsystem: Subsystem::Runtime,
    severity: Severity::Warn,
    debounce: DebouncePolicy::Interval(120.0),
    description: "wasm heap past 2.5 GiB — it never shrinks; plan to save and reload the tab",
    when_state: None,
};
impl Rule for WasmMemoryHigh {
    fn header(&self) -> &RuleHeader {
        &WASM_MEMORY_HIGH
    }
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let bytes = gauge_last(cx, names::RUNTIME_MEMORY_WASM_BYTES)?;
        Some(if bytes > WASM_MEMORY_HIGH_BYTES {
            Verdict::violated(format!(
                "wasm heap {:.2} GiB of the 4 GiB ceiling",
                bytes / (1024.0 * 1024.0 * 1024.0)
            ))
        } else {
            Verdict::Clear
        })
    }
}

struct WasmMemoryCritical;
const WASM_MEMORY_CRITICAL: RuleHeader = RuleHeader {
    id: "runtime.wasm_memory_critical",
    subsystem: Subsystem::Runtime,
    severity: Severity::Critical,
    debounce: DebouncePolicy::Interval(30.0),
    description: "wasm heap past 3.25 GiB — OOM abort imminent; download the log, save, reload NOW",
    when_state: None,
};
impl Rule for WasmMemoryCritical {
    fn header(&self) -> &RuleHeader {
        &WASM_MEMORY_CRITICAL
    }
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let bytes = gauge_last(cx, names::RUNTIME_MEMORY_WASM_BYTES)?;
        Some(if bytes > WASM_MEMORY_CRITICAL_BYTES {
            Verdict::violated(format!(
                "wasm heap {:.2} GiB — the 4 GiB wall is next",
                bytes / (1024.0 * 1024.0 * 1024.0)
            ))
        } else {
            Verdict::Clear
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::MetricsRegistry;

    /// A LiveCtx with sane defaults; tests override the fields they exercise.
    fn ctx<'a>(metrics: &'a MetricsRegistry) -> LiveCtx<'a> {
        LiveCtx {
            now_secs: 100.0,
            state: AppState::InGame,
            metrics,
            loading_elapsed_secs: None,
            ingame_elapsed_secs: Some(60.0),
            player_y: None,
            ground_y: None,
            nan_body_count: 0,
            orphan_avatar_count: 0,
            respawns_recent: 0,
        }
    }

    #[test]
    fn terrain_collider_missing_fires_at_zero_colliders() {
        // The shared ctx() dwell (60 s) is past the grace window, so the
        // never-seen arm fires like the pre-#869 rule did.
        let mut m = MetricsRegistry::default();
        m.observe_gauge(names::RUNTIME_COLLIDER_COUNT, 0.0);
        assert!(TerrainColliderMissing.eval(&ctx(&m)).unwrap().is_violated());
        m.observe_gauge(names::RUNTIME_COLLIDER_COUNT, 3.0);
        assert_eq!(TerrainColliderMissing.eval(&ctx(&m)), Some(Verdict::Clear));
    }

    #[test]
    fn terrain_collider_missing_grace_gates_the_entry_frame() {
        // Entry-frame shape (#869): only zero samples so far (the 1 Hz
        // gauge predates the terrain body) and sub-grace dwell → Clear.
        let mut m = MetricsRegistry::default();
        m.observe_gauge(names::RUNTIME_COLLIDER_COUNT, 0.0);
        let mut cx = ctx(&m);
        cx.ingame_elapsed_secs = Some(1.0);
        assert_eq!(TerrainColliderMissing.eval(&cx), Some(Verdict::Clear));
        // Past the grace with still no collider ever seen → genuine miss.
        cx.ingame_elapsed_secs = Some(TERRAIN_GRACE_SECS + 1.0);
        assert!(TerrainColliderMissing.eval(&cx).unwrap().is_violated());
    }

    #[test]
    fn terrain_collider_missing_seen_then_lost_fires_inside_grace() {
        // Colliders existed and vanished — that is never a startup blip,
        // so it must fire even inside the grace window.
        let mut m = MetricsRegistry::default();
        m.observe_gauge(names::RUNTIME_COLLIDER_COUNT, 142.0);
        m.observe_gauge(names::RUNTIME_COLLIDER_COUNT, 0.0);
        let mut cx = ctx(&m);
        cx.ingame_elapsed_secs = Some(1.0);
        assert!(TerrainColliderMissing.eval(&cx).unwrap().is_violated());
    }

    #[test]
    fn frame_time_spike_fires_above_threshold() {
        let mut m = MetricsRegistry::default();
        m.observe_gauge(names::RUNTIME_FRAME_TIME_MS, 60.0);
        assert!(FrameTimeSpike.eval(&ctx(&m)).unwrap().is_violated());
        m.observe_gauge(names::RUNTIME_FRAME_TIME_MS, 16.6);
        assert_eq!(FrameTimeSpike.eval(&ctx(&m)), Some(Verdict::Clear));
    }

    #[test]
    fn looping_voices_overload_fires_above_threshold() {
        let mut m = MetricsRegistry::default();
        // No gauge yet (no audio ever spawned) → dormant, not violated.
        assert!(LoopingVoicesOverload.eval(&ctx(&m)).is_none());
        m.observe_gauge(names::AUDIO_SPATIAL_ACTIVE_SINKS, 72.0);
        assert!(LoopingVoicesOverload.eval(&ctx(&m)).unwrap().is_violated());
        m.observe_gauge(names::AUDIO_SPATIAL_ACTIVE_SINKS, 12.0);
        assert_eq!(LoopingVoicesOverload.eval(&ctx(&m)), Some(Verdict::Clear));
    }

    #[test]
    fn nan_and_fall_and_orphan_read_live_ctx() {
        let m = MetricsRegistry::default();
        let mut c = ctx(&m);
        c.nan_body_count = 2;
        assert!(NanInPhysics.eval(&c).unwrap().is_violated());

        c.nan_body_count = 0;
        c.player_y = Some(-40.0);
        c.ground_y = Some(4.0);
        assert!(PlayerFellThroughTerrain.eval(&c).unwrap().is_violated());

        c.orphan_avatar_count = 1;
        assert!(OrphanAvatarVisual.eval(&c).unwrap().is_violated());
    }

    #[test]
    fn wasm_memory_rules_escalate_with_the_heap() {
        let mut m = MetricsRegistry::default();
        // No gauge (native) → both rules dormant.
        assert!(WasmMemoryHigh.eval(&ctx(&m)).is_none());
        assert!(WasmMemoryCritical.eval(&ctx(&m)).is_none());

        // Healthy heap → both clear.
        m.observe_gauge(names::RUNTIME_MEMORY_WASM_BYTES, 1.0e9);
        assert_eq!(WasmMemoryHigh.eval(&ctx(&m)), Some(Verdict::Clear));
        assert_eq!(WasmMemoryCritical.eval(&ctx(&m)), Some(Verdict::Clear));

        // Past the warn tier, under the critical tier.
        m.observe_gauge(names::RUNTIME_MEMORY_WASM_BYTES, 2.8e9);
        assert!(WasmMemoryHigh.eval(&ctx(&m)).unwrap().is_violated());
        assert_eq!(WasmMemoryCritical.eval(&ctx(&m)), Some(Verdict::Clear));

        // Near the wall → both fire.
        m.observe_gauge(names::RUNTIME_MEMORY_WASM_BYTES, 3.6e9);
        assert!(WasmMemoryHigh.eval(&ctx(&m)).unwrap().is_violated());
        assert!(WasmMemoryCritical.eval(&ctx(&m)).unwrap().is_violated());
    }

    /// Feed a mark gauge a series of per-rebuild snapshots.
    fn observe_marks(m: &mut MetricsRegistry, name: &'static str, marks: &[f64]) {
        for v in marks {
            m.observe_gauge(name, *v);
        }
    }

    /// Calibration against the two real sessions that motivated the rule
    /// (#919 / #921). The leaking one stepped ~+90 images per re-roll and
    /// never fell; the healthy one oscillates around a plateau. The
    /// discriminator is monotonicity across rebuilds, not any threshold on
    /// the value — a healthy session's plateau can sit *higher* than a
    /// young leaking session and must still read as clear.
    #[test]
    fn asset_growth_across_rebuilds_separates_leak_from_plateau() {
        // Leaking shape (session 1784638603215): every delta positive.
        let mut m = MetricsRegistry::default();
        observe_marks(
            &mut m,
            names::RUNTIME_REBUILD_IMAGE_HANDLES,
            &[41.0, 131.0, 221.0, 311.0, 401.0],
        );
        let v = AssetGrowthAcrossRebuilds.eval(&ctx(&m)).unwrap();
        assert!(v.is_violated(), "the #919 shape must fire");

        // Healthy shape (session 1784657742693): deltas alternate sign
        // around a plateau — clear, despite sitting numerically higher.
        let mut m = MetricsRegistry::default();
        observe_marks(
            &mut m,
            names::RUNTIME_REBUILD_IMAGE_HANDLES,
            &[198.0, 240.0, 210.0, 232.0, 228.0],
        );
        assert_eq!(
            AssetGrowthAcrossRebuilds.eval(&ctx(&m)),
            Some(Verdict::Clear)
        );

        // Monotone but tiny growth (a few variants accumulating in a
        // bounded cache) stays under the floor — clear.
        let mut m = MetricsRegistry::default();
        observe_marks(
            &mut m,
            names::RUNTIME_REBUILD_IMAGE_HANDLES,
            &[200.0, 210.0, 215.0, 220.0, 228.0],
        );
        assert_eq!(
            AssetGrowthAcrossRebuilds.eval(&ctx(&m)),
            Some(Verdict::Clear)
        );
    }

    #[test]
    fn asset_growth_is_dormant_until_enough_rebuilds() {
        let mut m = MetricsRegistry::default();
        // Four marks = three deltas: one short of the window.
        observe_marks(
            &mut m,
            names::RUNTIME_REBUILD_IMAGE_HANDLES,
            &[41.0, 131.0, 221.0, 311.0],
        );
        assert!(AssetGrowthAcrossRebuilds.eval(&ctx(&m)).is_none());
    }

    /// The retention rule owns exactly the class the growth rule doesn't:
    /// memory climbing while handles are flat (#625's ~52 MB/re-roll). When
    /// handles are moving too, it stays quiet — the growth rule owns that
    /// diagnosis and one condition must not double-report as both.
    #[test]
    fn memory_retention_fires_only_when_handles_are_flat() {
        const MB: f64 = 1024.0 * 1024.0;
        let flat_handles = [200.0, 205.0, 198.0, 203.0, 200.0];
        // Healthy-session memory shape: ~52 MB per re-roll, monotone.
        let climbing = [715.0 * MB, 767.0 * MB, 819.0 * MB, 871.0 * MB, 923.0 * MB];

        let mut m = MetricsRegistry::default();
        observe_marks(&mut m, names::RUNTIME_REBUILD_MEMORY_BYTES, &climbing);
        observe_marks(&mut m, names::RUNTIME_REBUILD_IMAGE_HANDLES, &flat_handles);
        observe_marks(&mut m, names::RUNTIME_REBUILD_MESH_HANDLES, &flat_handles);
        let v = MemoryRetentionAcrossRebuilds.eval(&ctx(&m)).unwrap();
        assert!(v.is_violated(), "retention with flat handles must inform");

        // Same memory climb during a handle leak → the growth rule's case,
        // not this one's.
        let mut m = MetricsRegistry::default();
        observe_marks(&mut m, names::RUNTIME_REBUILD_MEMORY_BYTES, &climbing);
        observe_marks(
            &mut m,
            names::RUNTIME_REBUILD_IMAGE_HANDLES,
            &[41.0, 131.0, 221.0, 311.0, 401.0],
        );
        observe_marks(&mut m, names::RUNTIME_REBUILD_MESH_HANDLES, &flat_handles);
        assert_eq!(
            MemoryRetentionAcrossRebuilds.eval(&ctx(&m)),
            Some(Verdict::Clear)
        );
        assert!(
            AssetGrowthAcrossRebuilds
                .eval(&ctx(&m))
                .unwrap()
                .is_violated()
        );

        // Flat memory → clear.
        let mut m = MetricsRegistry::default();
        observe_marks(
            &mut m,
            names::RUNTIME_REBUILD_MEMORY_BYTES,
            &[900.0 * MB, 910.0 * MB, 905.0 * MB, 915.0 * MB, 912.0 * MB],
        );
        observe_marks(&mut m, names::RUNTIME_REBUILD_IMAGE_HANDLES, &flat_handles);
        observe_marks(&mut m, names::RUNTIME_REBUILD_MESH_HANDLES, &flat_handles);
        assert_eq!(
            MemoryRetentionAcrossRebuilds.eval(&ctx(&m)),
            Some(Verdict::Clear)
        );
    }

    #[test]
    fn shape_cache_growth_needs_a_window_and_a_big_delta() {
        let mut m = MetricsRegistry::default();
        // Single sample → no window → no verdict.
        m.observe_gauge(names::RUNTIME_SHAPE_MESH_CACHE_LEN, 10.0);
        assert!(ShapeMeshCacheGrowth.eval(&ctx(&m)).is_none());
        // Grow far past the leak threshold across the window.
        m.observe_gauge(names::RUNTIME_SHAPE_MESH_CACHE_LEN, 10_000.0);
        assert!(ShapeMeshCacheGrowth.eval(&ctx(&m)).unwrap().is_violated());
    }
}
