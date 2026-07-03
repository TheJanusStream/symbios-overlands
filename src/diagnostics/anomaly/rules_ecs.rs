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
const FALL_BELOW_GROUND_M: f32 = 25.0;

struct PlayerFellThroughTerrain;
const PLAYER_FELL: RuleHeader = RuleHeader {
    id: "runtime.player_fell_through_terrain",
    subsystem: Subsystem::Runtime,
    severity: Severity::Error,
    debounce: DebouncePolicy::OncePerCondition,
    description: "local player dropped well below the terrain surface",
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
