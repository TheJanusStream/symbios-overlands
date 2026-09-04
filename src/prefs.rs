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
//! CONTRACT for systems touching a watched resource (#879): mutate it
//! GUARDED — `bypass_change_detection` + `set_changed` on a real edit,
//! or a local copy written back conditionally. An egui widget holding
//! `&mut resource.field` (`Window::open`, `toggle_value`, …) derefs
//! mutably every frame and flags a change even when nothing moved;
//! before the guards, that re-armed the trailing debounce forever and
//! prefs only reached disk at logout. [`SAVE_MAX_LATENCY_SECS`] is the
//! backstop if a future writer forgets.
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

/// Hard ceiling from the FIRST pending change to the save (#879). A
/// trailing debounce alone can be starved forever by a system that
/// mutably derefs a watched resource every frame (egui's
/// `.open(&mut …)` / `toggle_value(&mut …)` patterns did exactly that,
/// so prefs only ever hit disk at logout, and closing the tab/app lost
/// the whole session's changes). The known writers are guarded at the
/// source now; this cap is the backstop that turns any future
/// regression into "saves every few seconds" instead of "never saves".
const SAVE_MAX_LATENCY_SECS: f64 = 5.0;

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
    ///
    /// LEGACY, machine-wide (#1223 f292). Kept only so an existing
    /// installation's list survives the upgrade: it is adopted by the next
    /// account to sign in and then cleared. New writes go to
    /// [`Self::muted_by_owner`].
    #[serde(default)]
    pub muted_dids: Option<crate::state::MutedDids>,
    /// Every account's mute list on this machine, keyed by owner DID
    /// (#1223 f292). A block list is a statement about who *you* will not
    /// hear; a shared computer used to hand one user's to the next.
    #[serde(default)]
    pub muted_by_owner: Option<crate::state::MutedByOwner>,
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
        muted_by_owner: &crate::state::MutedByOwner,
        gizmo: &GizmoFramePref,
    ) -> Self {
        Self {
            panels: Some(panels.clone()),
            settings: Some(settings.clone()),
            windows: Some(windows.clone()),
            // Never written again (#1223 f292): the legacy machine-wide
            // list is migrated on the first sign-in after the upgrade and
            // must not be resurrected by a later save, or it would leak
            // back into the next account.
            muted_dids: None,
            muted_by_owner: Some(muted_by_owner.clone()),
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
    // Both, and neither becomes `MutedDids` yet (#1223 f292): whose list
    // applies is not known until somebody signs in, and prefs load at
    // startup. `adopt_owner_mute_list` picks one when a session appears.
    commands.insert_resource(prefs.muted_by_owner.unwrap_or_default());
    commands.insert_resource(LegacyMutedDids(prefs.muted_dids));
    if let Some(gizmo) = prefs.gizmo {
        commands.insert_resource(GizmoFramePref::from(&gizmo));
    }
}

/// A pending save: the trailing-debounce deadline and the hard cap set
/// by the first change of the burst.
#[derive(Clone, Copy, PartialEq, Debug)]
struct PendingSave {
    /// Fires when quiet until here (re-armed by each change)…
    due: f64,
    /// …but never later than here (fixed at the burst's first change).
    cap: f64,
}

/// Trailing-debounce state for [`save_prefs_when_changed`].
#[derive(Default)]
pub struct SaveDebounce(Option<PendingSave>);

/// Step the debounce: a change (re)arms the trailing deadline — clamped
/// to the max-latency cap the burst's FIRST change fixed — and a
/// deadline that has come due fires exactly once. Pure so the state
/// machine is unit-testable.
fn debounce_step(
    pending: Option<PendingSave>,
    changed: bool,
    now: f64,
) -> (Option<PendingSave>, bool) {
    let pending = if changed {
        let cap = pending
            .map(|p| p.cap)
            .unwrap_or(now + SAVE_MAX_LATENCY_SECS);
        Some(PendingSave {
            due: (now + SAVE_DEBOUNCE_SECS).min(cap),
            cap,
        })
    } else {
        pending
    };
    match pending {
        Some(p) if now >= p.due => (None, true),
        other => (other, false),
    }
}

/// Watch [`UiPanels`] + [`LocalSettings`] + [`WindowLayout`] and persist
/// a snapshot shortly after the last change. Change detection also fires
/// on the startup load's own insert — that lone extra write of identical
/// data is harmless and keeps the system free of special cases.
#[allow(clippy::too_many_arguments)]
pub fn save_prefs_when_changed(
    panels: Res<UiPanels>,
    settings: Res<LocalSettings>,
    windows: Res<WindowLayout>,
    muted_dids: Res<crate::state::MutedDids>,
    mut muted_by_owner: ResMut<crate::state::MutedByOwner>,
    session: Option<Res<bevy_symbios_multiuser::auth::AtprotoSession>>,
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
    // Fold the live list back under its owner before capturing (#1223
    // f292). Guarded, because the fold itself must not dirty the resource
    // on a frame where nothing moved.
    if let Some(owner) = session.as_deref().map(|s| s.did.as_str())
        && muted_dids.is_changed()
        && muted_by_owner.for_owner(owner) != *muted_dids
    {
        muted_by_owner.set_owner(owner, &muted_dids);
    }
    let (pending, fire) = debounce_step(debounce.0, changed, time.elapsed_secs_f64());
    debounce.0 = pending;
    if fire {
        save(&PersistedPrefs::capture(
            &panels,
            &settings,
            &windows,
            &muted_by_owner,
            &gizmo,
        ));
    }
}

/// The legacy machine-wide mute list read from prefs at startup, held only
/// until somebody signs in (#1223 f292).
///
/// A newtype rather than a bare `Option` so it can be a resource and so
/// [`adopt_owner_mute_list`] can take it once and leave `None` behind: the
/// migration happens for the FIRST account to sign in after the upgrade,
/// and must not repeat for the second.
#[derive(Resource, Default, Debug)]
pub struct LegacyMutedDids(pub Option<crate::state::MutedDids>);

/// Install the signed-in owner's mute list, and migrate the legacy
/// machine-wide one on the first sign-in after the upgrade (#1223 f292).
///
/// Runs whenever an `AtprotoSession` appears — the ordinary login, the wasm
/// resume, and #1214's in-place re-authenticate all insert one, and none of
/// them should have to remember this.
pub fn adopt_owner_mute_list(
    session: Option<Res<bevy_symbios_multiuser::auth::AtprotoSession>>,
    mut by_owner: ResMut<crate::state::MutedByOwner>,
    mut legacy: ResMut<LegacyMutedDids>,
    mut muted_dids: ResMut<crate::state::MutedDids>,
) {
    let Some(session) = session else {
        return;
    };
    if !session.is_added() {
        return;
    }
    let owner = session.did.as_str();
    let mut list = by_owner.for_owner(owner);
    // The machine's pre-#1223 list belongs to whoever was using the
    // machine, and the first person to sign in after the upgrade is the
    // best available answer. Taken, not copied: a second account signing in
    // on the same machine must not inherit it, which is the whole defect.
    if let Some(legacy) = legacy.0.take() {
        list.0.extend(legacy.0);
        by_owner.set_owner(owner, &list);
    }
    // Written through `ResMut`, not `insert_resource`: a command applies at
    // the end of the schedule, so `save_prefs_when_changed` — chained
    // immediately after this — would still see the PREVIOUS owner's list
    // alongside the new session and fold one user's blocks under the other's
    // account. Which is the exact defect (#1223 f292) this is fixing.
    if *muted_dids != list {
        *muted_dids = list;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1226 f325. The sequence: an existing user updates the app and their
    /// prefs file predates the nametag setting entirely. `LocalSettings`
    /// grows only with `serde(default)`-compatible fields, so the missing
    /// one must come back ON — an upgrade that silently switched off the
    /// only in-world identity the product has would look like the feature
    /// never shipped.
    #[test]
    fn a_prefs_file_written_before_the_nametag_setting_still_loads() {
        let older = r#"{"smooth_kinematics":false}"#;
        let settings: LocalSettings = serde_json::from_str(older).expect("older prefs load");
        assert!(!settings.smooth_kinematics, "the field it did carry");
        assert!(
            settings.show_peer_nametags,
            "and the one it did not defaults on"
        );
    }

    #[test]
    fn prefs_round_trip_preserves_both_fields() {
        let panels = UiPanels {
            chat: true,
            controls: false,
            ..Default::default()
        };
        let settings = LocalSettings {
            smooth_kinematics: false,
            // Every non-default field must be spelled out here, or the test
            // proves only that the DEFAULT survives the wire (#1226 f325).
            show_peer_nametags: false,
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
        let mut by_owner = crate::state::MutedByOwner::default();
        by_owner.set_owner("did:plc:me", &muted);
        let prefs = PersistedPrefs {
            panels: Some(panels.clone()),
            settings: Some(settings.clone()),
            windows: Some(windows),
            muted_dids: Some(muted),
            muted_by_owner: Some(by_owner),
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
        let restored_settings = back.settings.clone().unwrap();
        assert!(!restored_settings.smooth_kinematics);
        assert!(
            !restored_settings.show_peer_nametags,
            "a switched-off nametag preference survives the wire (#1226)"
        );
        assert_eq!(
            back.windows.unwrap().rects["chat"],
            [890.0, 40.0, 380.0, 400.0]
        );
        assert!(back.muted_dids.unwrap().0.contains("did:plc:harasser"));
        assert!(
            back.muted_by_owner
                .unwrap()
                .for_owner("did:plc:me")
                .0
                .contains("did:plc:harasser"),
            "the account-scoped list is what a save writes now (#1223 f292)"
        );
    }

    /// #1223 f292. The sequence: two people share a computer. One mutes a
    /// harasser; the other signs in and that person is invisible to them,
    /// with no way to discover why — a muted peer renders as a hidden body
    /// and a faint dot, and there was no list to look at anywhere.
    #[test]
    fn one_users_block_list_does_not_reach_the_next_account() {
        let mut by_owner = crate::state::MutedByOwner::default();
        let mut mine = crate::state::MutedDids::default();
        mine.set("did:plc:harasser", true);
        by_owner.set_owner("did:plc:alice", &mine);

        assert!(
            by_owner
                .for_owner("did:plc:alice")
                .0
                .contains("did:plc:harasser")
        );
        assert!(
            by_owner.for_owner("did:plc:bob").0.is_empty(),
            "bob never muted anybody"
        );

        // Unmuting everyone leaves no record of who was signed in here.
        mine.set("did:plc:harasser", false);
        by_owner.set_owner("did:plc:alice", &mine);
        assert!(by_owner.0.is_empty());
    }

    /// The one-time migration: the pre-#1223 machine-wide list belongs to
    /// whoever was using the machine, so the FIRST account to sign in after
    /// the upgrade adopts it — and the second must not, which is the defect
    /// being fixed. `LegacyMutedDids` is taken, not read.
    #[test]
    fn the_legacy_machine_wide_list_is_adopted_once_and_only_once() {
        use bevy::prelude::*;

        let mut legacy = crate::state::MutedDids::default();
        legacy.set("did:plc:oldharasser", true);

        let mut world = World::new();
        world.insert_resource(crate::state::MutedByOwner::default());
        world.insert_resource(LegacyMutedDids(Some(legacy)));

        // Alice signs in first and inherits the machine's history.
        {
            let by_owner = world.resource::<crate::state::MutedByOwner>();
            let mut list = by_owner.for_owner("did:plc:alice");
            let taken = world
                .resource_mut::<LegacyMutedDids>()
                .0
                .take()
                .expect("the legacy list is there for the first sign-in");
            list.0.extend(taken.0);
            world
                .resource_mut::<crate::state::MutedByOwner>()
                .set_owner("did:plc:alice", &list);
        }
        assert!(
            world
                .resource::<crate::state::MutedByOwner>()
                .for_owner("did:plc:alice")
                .0
                .contains("did:plc:oldharasser")
        );
        assert!(
            world.resource::<LegacyMutedDids>().0.is_none(),
            "taken, so bob's sign-in finds nothing to inherit"
        );
        assert!(
            world
                .resource::<crate::state::MutedByOwner>()
                .for_owner("did:plc:bob")
                .0
                .is_empty()
        );
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
        // A change arms the deadline (and fixes the burst's cap).
        let (pending, fire) = debounce_step(None, true, 10.0);
        assert_eq!(pending.map(|p| p.due), Some(10.0 + SAVE_DEBOUNCE_SECS));
        assert!(!fire);
        // A further change pushes the deadline out (trailing debounce).
        let (pending, fire) = debounce_step(pending, true, 10.5);
        assert_eq!(pending.map(|p| p.due), Some(10.5 + SAVE_DEBOUNCE_SECS));
        assert!(!fire);
        // Quiet but not yet due → keep waiting.
        let (pending, fire) = debounce_step(pending, false, 11.0);
        assert_eq!(pending.map(|p| p.due), Some(10.5 + SAVE_DEBOUNCE_SECS));
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

    #[test]
    fn continuous_changes_cannot_starve_the_save() {
        // #879 regression shape: a "changed" signal every frame. The old
        // trailing debounce re-armed forever and never fired — prefs
        // reached disk only at logout. The max-latency cap must force a
        // save within SAVE_MAX_LATENCY_SECS of the burst's first change.
        let mut pending = None;
        let mut fired_at = None;
        let dt = 1.0 / 60.0;
        for frame in 0..(20.0 / dt) as u64 {
            let now = 10.0 + frame as f64 * dt;
            let (next, fire) = debounce_step(pending, true, now);
            pending = next;
            if fire {
                fired_at = Some(now);
                break;
            }
        }
        let fired_at = fired_at.expect("cap must force a save under continuous changes");
        assert!(
            fired_at <= 10.0 + SAVE_MAX_LATENCY_SECS + 0.1,
            "save fired at {fired_at}, later than the cap"
        );
        // And the cycle restarts cleanly: the next change re-arms.
        let (pending, fire) = debounce_step(pending, true, fired_at + 1.0);
        assert!(pending.is_some());
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
            muted_by_owner: None,
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
