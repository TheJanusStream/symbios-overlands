//! Local UI-state persistence (#820).
//!
//! Machine-local preferences that describe how THIS client presents the
//! app — which panels are open ([`crate::ui::toolbar::UiPanels`],
//! including the first-run Controls hint's dismissed state) and the
//! [`crate::state::LocalSettings`] toggles. They are deliberately NOT
//! PDS records: they say nothing about the world or the identity, so
//! they live in a local file (native) / `localStorage` (wasm) and are
//! shared by every account that logs in from this machine.
//!
//! Flow: [`load_prefs_at_startup`] reads the store once and overwrites
//! the freshly-initialised resources; [`save_prefs_when_changed`]
//! watches both resources with Bevy change detection and writes a
//! snapshot after a short trailing debounce, so toggling five panels in
//! two seconds costs one write, not five. A corrupt or unreadable store
//! degrades to defaults and heals itself on the next save — the same
//! philosophy as the OAuth session blob (`crate::oauth::wasm`).
//!
//! Schema stability: [`PersistedPrefs`] only ever GROWS `Option` fields
//! (`#[serde(default)]` everywhere), so an old file loads under a newer
//! binary (missing fields stay `None`) and an older binary ignores
//! fields a newer one wrote. Per-window rects landed as `windows`
//! (#833); planned growth: the DID-keyed mute list (#844).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::editor_gizmo::GizmoFramePref;
use crate::state::LocalSettings;
use crate::ui::layout::WindowLayout;
use crate::ui::toolbar::UiPanels;
use transform_gizmo_bevy::GizmoOrientation;

/// Trailing debounce for [`save_prefs_when_changed`]: a save fires this
/// many seconds after the LAST change, collapsing toggle bursts into
/// one write. Long enough to absorb a window-arranging session, short
/// enough that a quit right after a toggle still usually persists it.
const SAVE_DEBOUNCE_SECS: f64 = 1.0;

/// `localStorage` key on wasm. Namespaced like the OAuth session blob's
/// key so the origin's storage stays legible in devtools.
#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "symbios_overlands_prefs_v1";

/// Everything this machine remembers about its UI. All fields are
/// `Option` + `#[serde(default)]`: absent-in-file means "no opinion,
/// keep the resource's default" — distinct from an explicitly-saved
/// default value.
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct PersistedPrefs {
    /// Open/closed state of every toolbar-managed window, including the
    /// Controls hint — persisting `controls: false` after the first
    /// "Got it" is what makes the first-run hint first-run-only.
    #[serde(default)]
    pub panels: Option<UiPanels>,
    /// Client-side presentation toggles (peer smoothing today; UI scale
    /// and friends land here later).
    #[serde(default)]
    pub settings: Option<LocalSettings>,
    /// Last-shown rect of every managed window (#833), keyed by
    /// [`crate::ui::layout::UiWindow::key`] — a machine's arranged
    /// layout beats the computed defaults on the next run.
    #[serde(default)]
    pub windows: Option<WindowLayout>,
    /// DIDs muted by the local user (#844) — the durable mute list a
    /// reconnecting peer can no longer reset.
    #[serde(default)]
    pub muted_dids: Option<crate::state::MutedDids>,
    /// Gizmo frame + snap preferences (#871). A serde mirror rather than
    /// the resource itself: the upstream `GizmoOrientation` doesn't
    /// implement serde, and mirroring keeps the on-disk schema
    /// independent of upstream enum shape.
    #[serde(default)]
    pub gizmo: Option<GizmoPrefs>,
}

/// Serde mirror of [`GizmoFramePref`] (#871).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GizmoPrefs {
    pub local_frame: bool,
    pub snap: bool,
    pub snap_distance: f32,
    pub snap_angle_deg: f32,
    pub snap_scale: f32,
}

impl From<&GizmoFramePref> for GizmoPrefs {
    fn from(pref: &GizmoFramePref) -> Self {
        Self {
            local_frame: pref.orientation == GizmoOrientation::Local,
            snap: pref.snap,
            snap_distance: pref.snap_distance,
            snap_angle_deg: pref.snap_angle_deg,
            snap_scale: pref.snap_scale,
        }
    }
}

impl From<&GizmoPrefs> for GizmoFramePref {
    fn from(prefs: &GizmoPrefs) -> Self {
        Self {
            orientation: if prefs.local_frame {
                GizmoOrientation::Local
            } else {
                GizmoOrientation::Global
            },
            snap: prefs.snap,
            snap_distance: prefs.snap_distance,
            snap_angle_deg: prefs.snap_angle_deg,
            snap_scale: prefs.snap_scale,
        }
    }
}

impl PersistedPrefs {
    /// Snapshot the live resources for saving.
    fn capture(
        panels: &UiPanels,
        settings: &LocalSettings,
        windows: &WindowLayout,
        muted_dids: &crate::state::MutedDids,
        gizmo: &GizmoFramePref,
    ) -> Self {
        Self {
            panels: Some(panels.clone()),
            settings: Some(settings.clone()),
            windows: Some(windows.clone()),
            muted_dids: Some(muted_dids.clone()),
            gizmo: Some(gizmo.into()),
        }
    }
}

// ---------------------------------------------------------------------
// Storage backends.
// ---------------------------------------------------------------------

/// Native store: `$XDG_CONFIG_HOME/symbios-overlands/prefs.json`,
/// falling back to `%APPDATA%` (Windows) then `~/.config`. `None` when
/// no base directory can be resolved (headless CI without HOME) — the
/// app then simply runs without persistence.
#[cfg(not(target_arch = "wasm32"))]
fn native_prefs_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(std::path::PathBuf::from))
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
        })?;
    Some(base.join("symbios-overlands").join("prefs.json"))
}

/// Read + parse a prefs file. Split from [`load`] so tests can exercise
/// the round-trip against a temp path.
#[cfg(not(target_arch = "wasm32"))]
fn load_from_path(path: &std::path::Path) -> Option<PersistedPrefs> {
    let raw = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&raw) {
        Ok(prefs) => Some(prefs),
        Err(e) => {
            warn!("prefs file unreadable ({e}); using defaults");
            None
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn save_to_path(path: &std::path::Path, prefs: &PersistedPrefs) -> Result<(), String> {
    let json = serde_json::to_string_pretty(prefs).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, json).map_err(|e| e.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn load() -> Option<PersistedPrefs> {
    load_from_path(&native_prefs_path()?)
}

#[cfg(not(target_arch = "wasm32"))]
fn save(prefs: &PersistedPrefs) {
    let Some(path) = native_prefs_path() else {
        return;
    };
    if let Err(e) = save_to_path(&path, prefs) {
        warn!("failed to save prefs to {}: {e}", path.display());
    }
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

#[cfg(target_arch = "wasm32")]
fn load() -> Option<PersistedPrefs> {
    let raw = local_storage()?.get_item(STORAGE_KEY).ok().flatten()?;
    match serde_json::from_str(&raw) {
        Ok(prefs) => Some(prefs),
        Err(e) => {
            warn!("prefs blob unreadable ({e}); using defaults");
            None
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn save(prefs: &PersistedPrefs) {
    let Ok(json) = serde_json::to_string(prefs) else {
        return;
    };
    let Some(storage) = local_storage() else {
        // Private-browsing mode without storage: run without persistence,
        // mirroring how the OAuth blob degrades.
        return;
    };
    if let Err(e) = storage.set_item(STORAGE_KEY, &json) {
        warn!("failed to save prefs to localStorage: {e:?}");
    }
}

// ---------------------------------------------------------------------
// Systems.
// ---------------------------------------------------------------------

/// Startup: overwrite the `init_resource` defaults with whatever the
/// store remembers. Field-by-field, so a file that only knows about
/// panels leaves `LocalSettings` at its default.
pub fn load_prefs_at_startup(mut commands: Commands) {
    let Some(prefs) = load() else {
        return;
    };
    if let Some(panels) = prefs.panels {
        commands.insert_resource(panels);
    }
    if let Some(settings) = prefs.settings {
        commands.insert_resource(settings);
    }
    if let Some(windows) = prefs.windows {
        commands.insert_resource(windows);
    }
    if let Some(muted_dids) = prefs.muted_dids {
        commands.insert_resource(muted_dids);
    }
    if let Some(gizmo) = prefs.gizmo {
        commands.insert_resource(GizmoFramePref::from(&gizmo));
    }
}

/// Trailing-debounce state for [`save_prefs_when_changed`]: the session
/// second at which the pending save falls due.
#[derive(Default)]
pub struct SaveDebounce(Option<f64>);

/// Step the debounce: a change (re)arms the deadline; an armed deadline
/// that has come due fires exactly once. Pure so the state machine is
/// unit-testable.
fn debounce_step(pending: Option<f64>, changed: bool, now: f64) -> (Option<f64>, bool) {
    if changed {
        return (Some(now + SAVE_DEBOUNCE_SECS), false);
    }
    match pending {
        Some(deadline) if now >= deadline => (None, true),
        other => (other, false),
    }
}

/// Watch [`UiPanels`] + [`LocalSettings`] + [`WindowLayout`] and persist
/// a snapshot shortly after the last change. Change detection also fires
/// on the startup load's own insert — that lone extra write of identical
/// data is harmless and keeps the system free of special cases.
pub fn save_prefs_when_changed(
    panels: Res<UiPanels>,
    settings: Res<LocalSettings>,
    windows: Res<WindowLayout>,
    muted_dids: Res<crate::state::MutedDids>,
    gizmo: Res<GizmoFramePref>,
    time: Res<Time>,
    mut debounce: Local<SaveDebounce>,
) {
    let changed = panels.is_changed()
        || settings.is_changed()
        || windows.is_changed()
        || muted_dids.is_changed()
        // Guarded-dirty at the source (#871): the editors borrow the
        // pref bypassed and tick it only on a real toggle/edit.
        || gizmo.is_changed();
    let (pending, fire) = debounce_step(debounce.0, changed, time.elapsed_secs_f64());
    debounce.0 = pending;
    if fire {
        save(&PersistedPrefs::capture(
            &panels,
            &settings,
            &windows,
            &muted_dids,
            &gizmo,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_round_trip_preserves_both_fields() {
        let panels = UiPanels {
            chat: true,
            controls: false,
            ..Default::default()
        };
        let settings = LocalSettings {
            smooth_kinematics: false,
            ..Default::default()
        };
        let mut windows = WindowLayout::default();
        windows
            .rects
            .insert("chat".to_owned(), [890.0, 40.0, 380.0, 400.0]);
        let mut muted = crate::state::MutedDids::default();
        assert!(muted.set("did:plc:harasser", true));
        // Re-muting an already-muted DID reports "no change".
        assert!(!muted.set("did:plc:harasser", true));
        let gizmo = GizmoPrefs {
            local_frame: false,
            snap: true,
            snap_distance: 0.5,
            snap_angle_deg: 15.0,
            snap_scale: 0.25,
        };
        let prefs = PersistedPrefs {
            panels: Some(panels.clone()),
            settings: Some(settings.clone()),
            windows: Some(windows),
            muted_dids: Some(muted),
            gizmo: Some(gizmo),
        };
        let json = serde_json::to_string(&prefs).unwrap();
        let back: PersistedPrefs = serde_json::from_str(&json).unwrap();
        assert_eq!(back, prefs);
        // The mirror round-trips through the live resource shape too:
        // a persisted World choice survives the Local default (#871).
        let restored_pref = GizmoFramePref::from(back.gizmo.as_ref().unwrap());
        assert_eq!(restored_pref.orientation, GizmoOrientation::Global);
        assert_eq!(
            GizmoPrefs::from(&restored_pref),
            *back.gizmo.as_ref().unwrap()
        );
        let restored = back.panels.unwrap();
        assert!(restored.chat);
        assert!(!restored.controls);
        assert!(!back.settings.unwrap().smooth_kinematics);
        assert_eq!(
            back.windows.unwrap().rects["chat"],
            [890.0, 40.0, 380.0, 400.0]
        );
        assert!(back.muted_dids.unwrap().0.contains("did:plc:harasser"));
    }

    #[test]
    fn missing_and_unknown_fields_degrade_gracefully() {
        // Old file with no fields at all → both None, no error.
        let empty: PersistedPrefs = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, PersistedPrefs::default());
        // A file written by a NEWER binary carries fields we don't know;
        // serde ignores them rather than failing the whole load.
        let newer: PersistedPrefs =
            serde_json::from_str(r#"{"panels": null, "window_rects": {"chat": [1, 2, 3, 4]}}"#)
                .unwrap();
        assert!(newer.panels.is_none());
        // A panels object missing NEW bools fills them from Default —
        // the forward-compat contract for growing UiPanels.
        let partial: PersistedPrefs =
            serde_json::from_str(r#"{"panels": {"chat": true}}"#).unwrap();
        let panels = partial.panels.unwrap();
        assert!(panels.chat);
        assert!(panels.controls, "missing fields take UiPanels defaults");
    }

    #[test]
    fn debounce_arms_extends_and_fires_once() {
        // A change arms the deadline.
        let (pending, fire) = debounce_step(None, true, 10.0);
        assert_eq!(pending, Some(10.0 + SAVE_DEBOUNCE_SECS));
        assert!(!fire);
        // A further change pushes the deadline out (trailing debounce).
        let (pending, fire) = debounce_step(pending, true, 10.5);
        assert_eq!(pending, Some(10.5 + SAVE_DEBOUNCE_SECS));
        assert!(!fire);
        // Quiet but not yet due → keep waiting.
        let (pending, fire) = debounce_step(pending, false, 11.0);
        assert_eq!(pending, Some(10.5 + SAVE_DEBOUNCE_SECS));
        assert!(!fire);
        // Due → fire exactly once and disarm.
        let (pending, fire) = debounce_step(pending, false, 12.0);
        assert_eq!(pending, None);
        assert!(fire);
        // Idle afterwards → nothing.
        let (pending, fire) = debounce_step(pending, false, 13.0);
        assert_eq!(pending, None);
        assert!(!fire);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_save_and_load_round_trip_through_a_real_file() {
        let dir = std::env::temp_dir().join(format!("symbios-prefs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("prefs.json");

        // Missing file → None (fresh install).
        assert!(load_from_path(&path).is_none());

        let panels = UiPanels {
            diagnostics: true,
            ..Default::default()
        };
        let prefs = PersistedPrefs {
            panels: Some(panels),
            settings: None,
            windows: None,
            muted_dids: None,
            gizmo: None,
        };
        save_to_path(&path, &prefs).unwrap();
        let back = load_from_path(&path).unwrap();
        assert_eq!(back, prefs);

        // Corrupt file → None (self-heals on next save) rather than a panic.
        std::fs::write(&path, "{not json").unwrap();
        assert!(load_from_path(&path).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
