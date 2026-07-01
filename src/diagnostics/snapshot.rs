//! Startup snapshot — the first record of every session (Pillar A-4).
//!
//! Enough build/environment context to key a log to a DID and correlate it
//! across runs: crate version, git sha (from `build.rs`), target arch, build
//! profile, the boot params, and — once authenticated — the session DID/relay.
//!
//! It is emitted in two phases (see [`SnapshotPhase`]): a `Boot` snapshot at
//! app build, before login (DID unknown), and a `Session` snapshot on
//! Login → Loading with the authenticated DID/relay filled in, so the analyzer
//! can attribute the file to a DID even though the sink opened earlier.

use crate::boot_params::BootParams;
use crate::diagnostics::event::{EventPayload, SnapshotPhase, StartupInfo};

/// Short git sha baked in by `build.rs`, or `"unknown"` if that script was not
/// run (kept as `option_env!` so the crate compiles without it).
fn git_sha() -> String {
    option_env!("SYMBIOS_GIT_SHA")
        .unwrap_or("unknown")
        .to_string()
}

/// Build a [`EventPayload::StartupSnapshot`] for the given phase. `session_did`
/// / `relay` are `None` in the `Boot` phase and filled from the authenticated
/// session in the `Session` phase (falling back to the boot params' relay when
/// no override is supplied).
pub fn build_startup_snapshot(
    phase: SnapshotPhase,
    boot: &BootParams,
    session_did: Option<&str>,
    relay: Option<&str>,
) -> EventPayload {
    let boot_pos = boot
        .target_pos
        .as_ref()
        .map(|p| [p.x, p.y.unwrap_or(0.0), p.z]);
    let relay = relay.map(str::to_string).or_else(|| boot.relay.clone());

    EventPayload::StartupSnapshot(Box::new(StartupInfo {
        phase,
        version: env!("CARGO_PKG_VERSION").to_string(),
        git_sha: git_sha(),
        target_arch: std::env::consts::ARCH.to_string(),
        profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
        .to_string(),
        wasm: cfg!(target_arch = "wasm32"),
        boot_target_did: boot.target_did.clone(),
        boot_pos,
        boot_yaw_deg: boot.target_yaw_deg,
        pds: boot.pds.clone(),
        relay,
        session_did: session_did.map(str::to_string),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot_params::{BootParams, TargetPos};
    use crate::diagnostics::event::{SessionEvent, Severity};

    fn boot() -> BootParams {
        BootParams {
            target_did: Some("did:plc:room".into()),
            target_pos: Some(TargetPos {
                x: 1.0,
                y: Some(2.0),
                z: 3.0,
            }),
            target_yaw_deg: Some(90.0),
            pds: Some("https://pds.example".into()),
            relay: Some("wss://relay.example".into()),
            autosubmit: true,
        }
    }

    #[test]
    fn boot_phase_has_no_session_did_but_full_build_info() {
        let p = build_startup_snapshot(SnapshotPhase::Boot, &boot(), None, None);
        let EventPayload::StartupSnapshot(info) = &p else {
            panic!("expected StartupSnapshot");
        };
        assert_eq!(info.phase, SnapshotPhase::Boot);
        assert!(!info.version.is_empty());
        assert_eq!(info.target_arch, std::env::consts::ARCH);
        assert_eq!(info.boot_pos, Some([1.0, 2.0, 3.0]));
        assert_eq!(info.relay.as_deref(), Some("wss://relay.example"));
        assert!(info.session_did.is_none());
        // Categorised as a session snapshot regardless of phase.
        assert_eq!(p.category(), crate::diagnostics::event::Category::Snapshot);
    }

    #[test]
    fn session_phase_fills_did_and_relay_override() {
        let p = build_startup_snapshot(
            SnapshotPhase::Session,
            &boot(),
            Some("did:plc:me"),
            Some("wss://relay.authenticated"),
        );
        let EventPayload::StartupSnapshot(info) = &p else {
            panic!("expected StartupSnapshot");
        };
        assert_eq!(info.session_did.as_deref(), Some("did:plc:me"));
        assert_eq!(info.relay.as_deref(), Some("wss://relay.authenticated"));
    }

    #[test]
    fn snapshot_round_trips_as_the_first_ndjson_line() {
        let p = build_startup_snapshot(SnapshotPhase::Session, &boot(), Some("did:plc:me"), None);
        let ev = SessionEvent::new(0, 0.0, Some(1), Severity::Info, p);
        let line = serde_json::to_string(&ev).unwrap();
        assert!(line.contains("\"kind\":\"StartupSnapshot\""));
        let back: SessionEvent = serde_json::from_str(&line).unwrap();
        assert_eq!(ev, back);
    }
}
