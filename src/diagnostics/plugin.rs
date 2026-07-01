//! `DiagnosticsPlugin` (Pillar A-5) — the Bevy wiring for the session log: it
//! constructs the [`SessionLog`] (attaching the native NDJSON sink and arming
//! the panic hook), records the boot [`StartupSnapshot`] as the very first
//! event, and registers the flush systems (periodic + on `AppExit`).
//!
//! The plugin reads the [`BootParams`] resource, so `run()` (A-7) must insert
//! `boot` **before** adding this plugin. An optional [`DiagDirOverride`]
//! resource redirects the native sink/panic dir without touching env vars
//! (used by tests).
//!
//! [`StartupSnapshot`]: crate::diagnostics::event::EventPayload::StartupSnapshot

use bevy::app::AppExit;
use bevy::ecs::message::MessageReader;
use bevy::prelude::*;

use crate::boot_params::BootParams;
use crate::config::diagnostics as cfg;
use crate::diagnostics::SessionLog;
use crate::diagnostics::event::{EventPayload, Severity, SnapshotPhase};
use crate::diagnostics::snapshot::build_startup_snapshot;
use crate::state::DiagnosticsLog;

/// Optional override for the native diagnostics directory (tests inject this to
/// avoid mutating the process-wide `SYMBIOS_DIAG_DIR`). Ignored on wasm.
#[derive(Resource, Default, Clone)]
pub struct DiagDirOverride(pub Option<std::path::PathBuf>);

/// Installs the session-log pipeline. Additive — no existing system changes.
pub struct DiagnosticsPlugin;

impl Plugin for DiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        // Extract everything needed from the world into owned values first so
        // the immutable borrow is released before we mutate `app`.
        let boot_payload = app
            .world()
            .get_resource::<BootParams>()
            .map(|b| build_startup_snapshot(SnapshotPhase::Boot, b, None, None));
        #[cfg(not(target_arch = "wasm32"))]
        let dir_override = app
            .world()
            .get_resource::<DiagDirOverride>()
            .and_then(|d| d.0.clone());

        let mut log = SessionLog::default();

        // Native: attach the file sink + arm the panic hook, all under the same
        // resolved directory. On wasm the log stays in-memory (ring + download).
        #[cfg(not(target_arch = "wasm32"))]
        {
            use crate::diagnostics::{panic, sink::Sink};
            if let Some(dir) = dir_override.or_else(Sink::resolve_dir) {
                log.set_sink(Sink::open_in(&dir, None));
                panic::arm(dir);
                panic::install_hook();
            }
        }

        // The boot snapshot is always seq 0 — the self-describing header.
        if let Some(payload) = boot_payload {
            log.record(0.0, Severity::Info, payload);
        }

        app.init_resource::<DiagnosticsLog>()
            .insert_resource(log)
            .add_systems(Update, forward_legacy_events)
            .add_systems(Last, (flush_periodically, flush_on_app_exit));
    }
}

/// Drain the legacy free-text [`DiagnosticsLog`] buffer into the unified
/// [`SessionLog`] as `Legacy` events (Pillar A-6). This keeps the ~17 call
/// sites still emitting `diagnostics.push(..)` visible in the HUD + file until
/// they migrate to typed variants (A-9), with no divergence between the two.
fn forward_legacy(diag: &mut DiagnosticsLog, log: &mut SessionLog) {
    for (t, text) in diag.take_pending() {
        log.record(t, Severity::Info, EventPayload::Legacy { text });
    }
}

fn forward_legacy_events(mut diag: ResMut<DiagnosticsLog>, mut log: ResMut<SessionLog>) {
    forward_legacy(&mut diag, &mut log);
}

/// Record the `phase=Session` startup snapshot on entering `Loading`, once the
/// authenticated DID/relay are known. Registered by `run()` (A-7) into the
/// existing `OnEnter(AppState::Loading)` set, so the log is keyed to a DID even
/// though the sink opened back at boot with the DID still unknown.
pub fn record_session_snapshot(
    mut log: ResMut<SessionLog>,
    boot: Res<BootParams>,
    session: Option<Res<bevy_symbios_multiuser::auth::AtprotoSession>>,
    relay: Option<Res<crate::state::RelayHost>>,
    time: Res<Time>,
) {
    let did = session.as_ref().map(|s| s.did.as_str());
    let relay = relay.as_ref().map(|r| r.0.as_str());
    let payload = build_startup_snapshot(SnapshotPhase::Session, &boot, did, relay);
    log.record(time.elapsed_secs_f64(), Severity::Info, payload);
}

/// Whether the sink should be flushed this tick: only when there is pending
/// data, and either enough events have accrued or enough time has elapsed.
/// Pulled out as a pure fn for unit testing.
fn should_flush(pending: usize, secs_since_flush: f64) -> bool {
    pending > 0
        && (pending >= cfg::FLUSH_EVERY_N_EVENTS || secs_since_flush >= cfg::FLUSH_INTERVAL_SECS)
}

/// Flush the durable sink on the `FLUSH_INTERVAL_SECS` / `FLUSH_EVERY_N_EVENTS`
/// cadence so a hard kill loses at most a small tail. No-op when nothing is
/// pending or the sink is disabled.
fn flush_periodically(mut log: ResMut<SessionLog>, time: Res<Time>, mut last_flush: Local<f64>) {
    let now = time.elapsed_secs_f64();
    if should_flush(log.pending_since_flush(), now - *last_flush) {
        log.flush();
        *last_flush = now;
    }
}

/// On app exit, record a `SessionEnd` marker and flush so the file closes with
/// a clean terminal record (vs. a panic file's crash sentinel).
fn flush_on_app_exit(
    mut exits: MessageReader<AppExit>,
    mut log: ResMut<SessionLog>,
    time: Res<Time>,
) {
    if exits.read().next().is_some() {
        let now = time.elapsed_secs_f64();
        log.record(
            now,
            Severity::Info,
            EventPayload::SessionEnd {
                reason: "app_exit".into(),
            },
        );
        log.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_moves_legacy_lines_into_session_log_in_order() {
        let mut diag = DiagnosticsLog::default();
        diag.push(0.5, "first".into());
        diag.push(1.5, "second".into());
        let mut log = SessionLog::with_capacity(16);
        forward_legacy(&mut diag, &mut log);

        // Buffer drained, both lines now in the unified stream as Legacy events
        // carrying their original timestamps.
        assert!(diag.take_pending().is_empty());
        let lines: Vec<(f64, String)> = log
            .iter()
            .map(|e| (e.t_mono_secs, e.payload.short_line()))
            .collect();
        assert_eq!(lines, vec![(0.5, "first".into()), (1.5, "second".into())]);
    }

    #[test]
    fn plugin_writes_boot_snapshot_to_the_session_file() {
        use bevy::prelude::*;

        let dir = std::env::temp_dir().join(format!("symbios-diag-plugin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(BootParams {
                target_did: Some("did:plc:me".into()),
                target_pos: None,
                target_yaw_deg: None,
                pds: None,
                relay: None,
                autosubmit: false,
            })
            .insert_resource(DiagDirOverride(Some(dir.clone())))
            .add_plugins(DiagnosticsPlugin);
        app.update();
        // Flush the BufWriter the boot snapshot wrote into at plugin build.
        app.world_mut().resource_mut::<SessionLog>().flush();

        let latest = dir.join(crate::config::diagnostics::LATEST_FILENAME);
        let body = std::fs::read_to_string(&latest).expect("session-latest.jsonl written");
        let first: crate::diagnostics::event::SessionEvent =
            serde_json::from_str(body.lines().next().expect("at least one line")).unwrap();
        assert_eq!(first.seq, 0, "boot snapshot is the first record");
        assert!(matches!(first.payload, EventPayload::StartupSnapshot(_)));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn flush_gate_respects_count_and_interval() {
        // Nothing pending → never flush, even past the interval.
        assert!(!should_flush(0, 1000.0));
        // Pending but neither threshold reached → wait.
        assert!(!should_flush(1, 0.0));
        // Enough events accrued → flush regardless of time.
        assert!(should_flush(cfg::FLUSH_EVERY_N_EVENTS, 0.0));
        // Enough time elapsed with anything pending → flush.
        assert!(should_flush(1, cfg::FLUSH_INTERVAL_SECS));
    }
}
