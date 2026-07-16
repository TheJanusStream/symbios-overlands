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
impl Rule for TerrainColliderMissing {
    fn header(&self) -> &RuleHeader {
        &TERRAIN_COLLIDER_MISSING
    }
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        // The loading gate guarantees a solid terrain collider before InGame,
        // so zero colliders here means the terrain body failed to spawn.
        let n = gauge_last(cx, names::RUNTIME_COLLIDER_COUNT)?;
        Some(if n < 1.0 {
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
            player_y: None,
            ground_y: None,
            nan_body_count: 0,
            orphan_avatar_count: 0,
            respawns_recent: 0,
        }
    }

    #[test]
    fn terrain_collider_missing_fires_at_zero_colliders() {
        let mut m = MetricsRegistry::default();
        m.observe_gauge(names::RUNTIME_COLLIDER_COUNT, 0.0);
        assert!(TerrainColliderMissing.eval(&ctx(&m)).unwrap().is_violated());
        m.observe_gauge(names::RUNTIME_COLLIDER_COUNT, 3.0);
        assert_eq!(TerrainColliderMissing.eval(&ctx(&m)), Some(Verdict::Clear));
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
