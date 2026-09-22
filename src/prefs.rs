//! Local UI-state persistence (#820), kept per account (#1407).
//!
//! Machine-local preferences that describe how THIS client presents the
//! app - which panels are open ([`crate::ui::toolbar::UiPanels`],
//! including the first-run Controls hint's dismissed state), where each
//! window was left, the [`LocalSettings`] toggles, the gizmo frame, the
//! master mute and the mute list. They are deliberately NOT PDS records:
//! they say nothing about the world, so they live in local files (native)
//! / `localStorage` (wasm).
//!
//! ## Whose settings (#1407)
//!
//! Every account that signs in on this machine keeps its own set, under its
//! DID. There used to be one set per machine, so accounts took over each
//! other's windows, theme and sound, and two copies of the app signed in
//! side by side overwrote each other's whole file on every save. Only the
//! login screen's settings ([`LoginScreenSettings`]: its theme, its
//! interface size and the live world backdrop) are the machine's, because
//! the login screen is the one screen every account shares.
//!
//! The live resources always hold ONE set: the signed-in account's, or,
//! while nobody is signed in, defaults under the login screen's theme and
//! size. [`follow_session_prefs`] swaps them when the session's DID
//! changes. It writes the outgoing account's set to that account's slot
//! FIRST, so a change made a moment before logout can never be saved under
//! the next account, and only then installs the incoming one.
//! [`PrefsOwner`] records whose set is live, and every save is addressed
//! by it. While the login screen's set is live,
//! [`mirror_login_screen_settings`] carries the theme and size its controls
//! change into [`LoginScreenSettings`].
//!
//! ## Where they live
//!
//! [`PrefsStore`] addresses three kinds of slot:
//!
//! * the machine's: `machine.json` / `symbios_overlands_machine_prefs_v1`;
//! * one per account: `accounts/<DID>.json` (the DID percent-encoded, see
//!   [`account_file_name`]) / `symbios_overlands_account_prefs_v1:<DID>`;
//! * the shared file every build before #1407 wrote: `prefs.json` /
//!   `symbios_overlands_prefs_v1`. It is READ-ONLY now. An account with no
//!   slot of its own starts from a copy of it (owner decision 2026-09-22),
//!   so no account lost its layout to the upgrade, and the machine's slot
//!   is seeded from it once.
//!
//! Native files sit in `$XDG_CONFIG_HOME/symbios-overlands/`, falling back
//! to `%APPDATA%` (Windows) then `~/.config`. With nowhere to write - no
//! base directory, a browser without storage - the store is a map that
//! lasts as long as the process, so the accounts of one run still keep
//! their settings apart.
//!
//! ## Saving
//!
//! [`save_prefs_when_changed`] watches the live resources with Bevy change
//! detection and writes a snapshot after a short trailing debounce, so
//! toggling five panels in two seconds costs one write, not five. A corrupt
//! or unreadable slot degrades to its starting point and heals itself on
//! the next save - the same philosophy as the OAuth session blob
//! (`crate::oauth::wasm`).
//!
//! CONTRACT for systems touching a watched resource (#879): mutate it
//! GUARDED - `bypass_change_detection` + `set_changed` on a real edit,
//! or a local copy written back conditionally. An egui widget holding
//! `&mut resource.field` (`Window::open`, `toggle_value`, …) derefs
//! mutably every frame and flags a change even when nothing moved;
//! before the guards, that re-armed the trailing debounce forever and
//! prefs only reached disk at logout. [`SAVE_MAX_LATENCY_SECS`] is the
//! backstop if a future writer forgets.
//!
//! Schema stability: [`AccountPrefs`] and [`MachinePrefs`] only ever GROW
//! `Option` fields (`#[serde(default)]` everywhere), so an old slot loads
//! under a newer binary (missing fields stay `None`) and an older binary
//! ignores fields a newer one wrote.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::audio_mute::AudioMuted;
use crate::editor_gizmo::GizmoFramePref;
use crate::state::{LocalSettings, LoginScreenSettings, MutedDids};
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

/// `localStorage` key of the machine's slot (#1407), and the in-memory
/// store's, which borrows the browser's names. Namespaced like the OAuth
/// session blob's key so the origin's storage stays legible in devtools.
const MACHINE_KEY: &str = "symbios_overlands_machine_prefs_v1";

/// Prefix of an account's `localStorage` key; the DID follows verbatim.
const ACCOUNT_KEY_PREFIX: &str = "symbios_overlands_account_prefs_v1:";

/// The pre-#1407 shared blob's key. Read, never written.
const LEGACY_KEY: &str = "symbios_overlands_prefs_v1";

// ---------------------------------------------------------------------
// Persisted shapes.
// ---------------------------------------------------------------------

/// One account's settings on this machine (#1407).
///
/// All fields are `Option` + `#[serde(default)]`: absent means "no
/// opinion, keep the resource's default" - distinct from an explicitly
/// saved default value. A save always writes every field.
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct AccountPrefs {
    /// Open/closed state of every toolbar-managed window, including the
    /// Controls hint - persisting `controls: false` after the first
    /// "Got it" is what makes the first-run hint first-run-only.
    #[serde(default)]
    pub panels: Option<UiPanels>,
    /// Client-side presentation toggles, the account's theme and
    /// interface size among them.
    #[serde(default)]
    pub settings: Option<LocalSettings>,
    /// Last-shown rect of every managed window (#833), keyed by
    /// [`crate::ui::layout::UiWindow::key`] - an account's arranged
    /// layout beats the computed defaults on the next run.
    #[serde(default)]
    pub windows: Option<WindowLayout>,
    /// DIDs this account has muted (#844, #1223 f292) - the durable
    /// mute list a reconnecting peer can no longer reset.
    #[serde(default)]
    pub muted: Option<MutedDids>,
    /// Gizmo frame + snap preferences (#871). A serde mirror rather than
    /// the resource itself: the upstream `GizmoOrientation` doesn't
    /// implement serde, and mirroring keeps the on-disk schema
    /// independent of upstream enum shape.
    #[serde(default)]
    pub gizmo: Option<GizmoPrefs>,
    /// Master mute (#1276 f38). The last preference the app forgot.
    ///
    /// [`AudioMuted`] defaults to `true` and its doc asserted an app-level
    /// persistence that lived nowhere, so every launch was silent and the
    /// procedural soundtrack had to be rediscovered as a toolbar glyph each
    /// session. Absent still means muted: only an explicitly remembered
    /// `false` unsilences a session.
    #[serde(default)]
    pub audio: Option<AudioPrefs>,
}

impl AccountPrefs {
    /// Snapshot the live resources for saving.
    fn capture(
        panels: &UiPanels,
        settings: &LocalSettings,
        windows: &WindowLayout,
        muted: &MutedDids,
        gizmo: &GizmoFramePref,
        audio: &AudioMuted,
    ) -> Self {
        Self {
            panels: Some(panels.clone()),
            settings: Some(settings.clone()),
            windows: Some(windows.clone()),
            muted: Some(muted.clone()),
            gizmo: Some(gizmo.into()),
            audio: Some(audio.into()),
        }
    }
}

/// The machine's own slot (#1407): the login screen's settings, plus the
/// one piece of a pre-#1223 file that is still waiting for an owner.
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct MachinePrefs {
    #[serde(default)]
    pub login_screen: Option<LoginScreenSettings>,
    /// The machine-wide mute list every build before #1223 kept, carried
    /// here out of the old shared file until the next account signs in and
    /// takes it. See [`LegacyMutedDids`].
    #[serde(default)]
    pub unclaimed_mutes: Option<MutedDids>,
}

impl MachinePrefs {
    fn capture(login: &LoginScreenSettings, unclaimed: &LegacyMutedDids) -> Self {
        Self {
            login_screen: Some(login.clone()),
            unclaimed_mutes: unclaimed.0.clone(),
        }
    }
}

/// The shared file every build before #1407 wrote. Read as a starting
/// point, never written.
///
/// `settings` is read raw rather than as [`LocalSettings`], because one of
/// its fields - `login_world_backdrop` - is the machine's now and lives in
/// [`LoginScreenSettings`]; [`LocalSettings`] no longer has anywhere to put
/// it.
#[derive(Deserialize, Default, Debug)]
struct LegacyPrefs {
    #[serde(default)]
    panels: Option<UiPanels>,
    #[serde(default)]
    settings: Option<serde_json::Value>,
    #[serde(default)]
    windows: Option<WindowLayout>,
    /// The one list for the whole machine that builds before #1223 kept.
    #[serde(default)]
    muted_dids: Option<MutedDids>,
    /// Every account's mute list, keyed by owner DID (#1223 f292).
    #[serde(default)]
    muted_by_owner: Option<HashMap<String, HashSet<String>>>,
    #[serde(default)]
    gizmo: Option<GizmoPrefs>,
    #[serde(default)]
    audio: Option<AudioPrefs>,
}

impl LegacyPrefs {
    /// The old `settings` object as today's [`LocalSettings`]. The backdrop
    /// field it may carry is ignored here - [`Self::machine`] reads it.
    fn local_settings(&self) -> Option<LocalSettings> {
        serde_json::from_value(self.settings.clone()?).ok()
    }

    /// The machine's slot as the old file describes it: its theme, size and
    /// backdrop become the login screen's, and a pre-#1223 mute list waits
    /// for the first account to sign in.
    fn machine(&self) -> MachinePrefs {
        let mut login = LoginScreenSettings::default();
        if let Some(settings) = self.local_settings() {
            login.theme = settings.theme;
            login.ui_scale = settings.ui_scale;
        }
        if let Some(backdrop) = self
            .settings
            .as_ref()
            .and_then(|settings| settings.get("login_world_backdrop"))
            .and_then(serde_json::Value::as_bool)
        {
            login.world_backdrop = backdrop;
        }
        MachinePrefs {
            login_screen: Some(login),
            unclaimed_mutes: self.muted_dids.clone().filter(|list| !list.0.is_empty()),
        }
    }
}

/// An account's first settings on this machine (#1407).
///
/// A copy of the old shared file where there is one - owner decision
/// 2026-09-22, so every account kept its layout through the upgrade - under
/// the login screen's theme and size, which is what a new account starts
/// with, and with the account's OWN mute list from the old file (#1223
/// f292), never another's.
fn seed_account(
    legacy: Option<&LegacyPrefs>,
    owner: &str,
    login: &LoginScreenSettings,
) -> AccountPrefs {
    let mut settings = legacy
        .and_then(LegacyPrefs::local_settings)
        .unwrap_or_default();
    settings.theme = login.theme;
    settings.ui_scale = login.ui_scale;
    AccountPrefs {
        panels: legacy.and_then(|l| l.panels.clone()),
        settings: Some(settings),
        windows: legacy.and_then(|l| l.windows.clone()),
        muted: legacy
            .and_then(|l| l.muted_by_owner.as_ref()?.get(owner).cloned())
            .map(MutedDids),
        gizmo: legacy.and_then(|l| l.gizmo.clone()),
        audio: legacy.and_then(|l| l.audio),
    }
}

/// What the live [`LocalSettings`] hold while nobody is signed in: the
/// defaults, under the login screen's theme and size.
fn login_screen_local_settings(login: &LoginScreenSettings) -> LocalSettings {
    LocalSettings {
        theme: login.theme,
        ui_scale: login.ui_scale,
        ..Default::default()
    }
}

/// Serde mirror of [`crate::audio_mute::AudioMuted`] (#1276 f38).
///
/// A struct rather than a bare `Option<bool>` for the reason
/// [`GizmoPrefs`] is one: the on-disk schema is independent of the
/// resource, and a second audio preference (a master gain, if one is ever
/// built - there is none today, see `audio_mute`'s module doc on why
/// `GlobalVolume` is not it) grows a field here rather than a sibling key.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioPrefs {
    pub muted: bool,
}

impl From<&AudioMuted> for AudioPrefs {
    fn from(m: &AudioMuted) -> Self {
        Self { muted: m.0 }
    }
}

impl From<&AudioPrefs> for AudioMuted {
    fn from(p: &AudioPrefs) -> Self {
        Self(p.muted)
    }
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

// ---------------------------------------------------------------------
// The store.
// ---------------------------------------------------------------------

/// One addressable piece of the store (#1407).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot<'a> {
    /// The machine's settings: [`MachinePrefs`].
    Machine,
    /// One account's settings, by DID: [`AccountPrefs`].
    Account(&'a str),
    /// The pre-#1407 shared file: [`LegacyPrefs`]. Never written.
    Legacy,
}

impl Slot<'_> {
    /// The slot's `localStorage` key, which the in-memory store uses too.
    fn storage_key(self) -> String {
        match self {
            Slot::Machine => MACHINE_KEY.to_owned(),
            Slot::Account(did) => format!("{ACCOUNT_KEY_PREFIX}{did}"),
            Slot::Legacy => LEGACY_KEY.to_owned(),
        }
    }

    /// The slot's file, relative to the prefs directory.
    #[cfg(not(target_arch = "wasm32"))]
    fn file(self) -> std::path::PathBuf {
        match self {
            Slot::Machine => "machine.json".into(),
            Slot::Account(did) => std::path::Path::new("accounts").join(account_file_name(did)),
            Slot::Legacy => "prefs.json".into(),
        }
    }
}

/// An account's file name (#1407): the DID with every byte outside
/// `[A-Za-z0-9._-]` percent-encoded, then `.json`.
///
/// A DID is `did:plc:…` or `did:web:host[:path…]`; the colons alone make it
/// an illegal file name on Windows, and a `did:web` may carry `%` of its
/// own. Encoding every other byte - `%` included - keeps the mapping
/// one-to-one, so two accounts never share a file, and keeps the name a
/// single path component: no `/` or `\` survives, and it cannot be `.` or
/// `..` because it ends in `.json`.
#[cfg(not(target_arch = "wasm32"))]
fn account_file_name(did: &str) -> String {
    use std::fmt::Write as _;
    let mut name = String::with_capacity(did.len() + 16);
    for byte in did.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_') {
            name.push(char::from(byte));
        } else {
            let _ = write!(name, "%{byte:02X}");
        }
    }
    name.push_str(".json");
    name
}

/// Where the prefs live (#1407).
///
/// A resource rather than a pair of free functions so a test can hand the
/// systems a store of its own instead of the user's real one, and so a
/// machine with nowhere to write still keeps each account's settings apart
/// for the life of the process.
#[derive(Resource, Clone, Debug)]
pub enum PrefsStore {
    /// One file per slot under this directory (native).
    #[cfg(not(target_arch = "wasm32"))]
    Dir(std::path::PathBuf),
    /// The page's `localStorage` (wasm).
    #[cfg(target_arch = "wasm32")]
    Browser,
    /// A map that lasts as long as the process: the fallback when the
    /// platform store is unavailable, and what tests run against.
    Memory(Arc<Mutex<BTreeMap<String, String>>>),
}

impl Default for PrefsStore {
    fn default() -> Self {
        Self::platform()
    }
}

impl PrefsStore {
    /// `$XDG_CONFIG_HOME/symbios-overlands/`, falling back to `%APPDATA%`
    /// (Windows) then `~/.config` - or, with no base directory at all
    /// (headless CI without HOME), [`Self::memory`].
    #[cfg(not(target_arch = "wasm32"))]
    pub fn platform() -> Self {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("APPDATA").map(std::path::PathBuf::from))
            .or_else(|| {
                std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
            });
        base.map_or_else(Self::memory, |base| {
            Self::Dir(base.join("symbios-overlands"))
        })
    }

    /// The page's `localStorage` - or, in a browser that offers none
    /// (some private-browsing modes), [`Self::memory`].
    #[cfg(target_arch = "wasm32")]
    pub fn platform() -> Self {
        if local_storage().is_some() {
            Self::Browser
        } else {
            Self::memory()
        }
    }

    /// A fresh, empty store that lasts as long as the process.
    pub fn memory() -> Self {
        Self::Memory(Arc::default())
    }

    fn read(&self, slot: Slot<'_>) -> Option<String> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Dir(dir) => std::fs::read_to_string(dir.join(slot.file())).ok(),
            #[cfg(target_arch = "wasm32")]
            Self::Browser => local_storage()?
                .get_item(&slot.storage_key())
                .ok()
                .flatten(),
            Self::Memory(map) => map.lock().ok()?.get(&slot.storage_key()).cloned(),
        }
    }

    fn write(&self, slot: Slot<'_>, json: &str) -> Result<(), String> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Dir(dir) => write_file_atomically(&dir.join(slot.file()), json),
            #[cfg(target_arch = "wasm32")]
            Self::Browser => local_storage()
                .ok_or_else(|| "localStorage unavailable".to_owned())?
                .set_item(&slot.storage_key(), json)
                .map_err(|e| format!("{e:?}")),
            Self::Memory(map) => {
                map.lock()
                    .map_err(|e| e.to_string())?
                    .insert(slot.storage_key(), json.to_owned());
                Ok(())
            }
        }
    }

    /// Read and parse one slot. `None` when it is empty, and when it no
    /// longer parses - a slot that then heals on its next save.
    fn load<T: DeserializeOwned>(&self, slot: Slot<'_>) -> Option<T> {
        let raw = self.read(slot)?;
        serde_json::from_str(&raw)
            .inspect_err(|e| warn!("prefs {slot:?} unreadable ({e}); starting it over"))
            .ok()
    }

    /// Serialise and write one slot. Best effort: a failure is logged and
    /// the live settings are unaffected.
    fn save<T: Serialize>(&self, slot: Slot<'_>, value: &T) {
        let written = serde_json::to_string_pretty(value)
            .map_err(|e| e.to_string())
            .and_then(|json| self.write(slot, &json));
        if let Err(e) = written {
            warn!("failed to save prefs {slot:?}: {e}");
        }
    }
}

/// Write `json` to `path` through a sibling temp file and a rename, so a
/// crash - or a second copy of the app saving the machine's slot at the
/// same moment - leaves the old contents or the new, never half of each.
/// The temp name carries the process id, so two copies never share one.
#[cfg(not(target_arch = "wasm32"))]
fn write_file_atomically(path: &std::path::Path, json: &str) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("{} has no directory", path.display()))?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", std::process::id()));
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

// ---------------------------------------------------------------------
// Resources.
// ---------------------------------------------------------------------

/// Whose settings the live resources hold (#1407): the signed-in account's
/// DID, or `None` while the login screen's are live. Written only by
/// [`follow_session_prefs`]; every save is addressed by it.
#[derive(Resource, Default, Clone, Debug, PartialEq, Eq)]
pub struct PrefsOwner(pub Option<String>);

/// A pre-#1223 machine-wide mute list, waiting for its owner (#1223 f292,
/// #1407).
///
/// Builds before #1223 kept one list for the whole machine. It belongs to
/// whoever was using the machine, and the first account to sign in is the
/// best available answer - so [`follow_session_prefs`] TAKES it for that
/// account and leaves `None` behind, and the second account inherits
/// nothing, which is the defect #1223 fixed. Until then it rides in the
/// machine's slot as [`MachinePrefs::unclaimed_mutes`].
#[derive(Resource, Default, Debug)]
pub struct LegacyMutedDids(pub Option<MutedDids>);

/// The live resources an account's settings are installed into, as one
/// parameter (#1407).
#[derive(SystemParam)]
pub struct LivePrefs<'w> {
    panels: ResMut<'w, UiPanels>,
    settings: ResMut<'w, LocalSettings>,
    windows: ResMut<'w, WindowLayout>,
    muted: ResMut<'w, MutedDids>,
    gizmo: ResMut<'w, GizmoFramePref>,
    audio: ResMut<'w, AudioMuted>,
}

impl LivePrefs<'_> {
    fn capture(&self) -> AccountPrefs {
        AccountPrefs::capture(
            &self.panels,
            &self.settings,
            &self.windows,
            &self.muted,
            &self.gizmo,
            &self.audio,
        )
    }

    /// Replace every live resource with `prefs` - a `None` field with the
    /// resource's default, and missing settings with the login screen's
    /// theme and size over defaults. Each is written only where it differs
    /// (#879), so a swap to an identical set stamps nothing.
    fn install(&mut self, prefs: &AccountPrefs, login: &LoginScreenSettings) {
        self.panels
            .set_if_neq(prefs.panels.clone().unwrap_or_default());
        self.settings.set_if_neq(
            prefs
                .settings
                .clone()
                .unwrap_or_else(|| login_screen_local_settings(login)),
        );
        self.windows
            .set_if_neq(prefs.windows.clone().unwrap_or_default());
        self.muted
            .set_if_neq(prefs.muted.clone().unwrap_or_default());
        self.gizmo.set_if_neq(
            prefs
                .gizmo
                .as_ref()
                .map(GizmoFramePref::from)
                .unwrap_or_default(),
        );
        // Absent means muted (#1276 f38), which is also what the login
        // screen gets: it has no mute control, so a sound left on by the
        // last account would be one nobody there can turn off.
        self.audio.set_if_neq(
            prefs
                .audio
                .as_ref()
                .map(AudioMuted::from)
                .unwrap_or_default(),
        );
    }
}

// ---------------------------------------------------------------------
// Systems.
// ---------------------------------------------------------------------

/// The prefs chain, for a system that has to see an account swap on the
/// frame it happens (`ui::layout::forget_window_state_on_account_change`).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PrefsSystems;

/// Everything the prefs layer needs, registered in one place (#1407).
///
/// The live resources are registered by their own modules too;
/// `init_resource` keeps whichever came first. Registering them here as
/// well is what keeps the systems below runnable on an EMPTY store - a
/// first visit, or a slot that no longer parses - which #1317 learnt the
/// hard way: under Bevy 0.19 a missing required parameter is a panic, and
/// on wasm that freezes the canvas on the login screen with no UI.
pub struct PrefsPlugin;

impl Plugin for PrefsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PrefsStore>()
            .init_resource::<PrefsOwner>()
            .init_resource::<LoginScreenSettings>()
            .init_resource::<LegacyMutedDids>()
            .init_resource::<UiPanels>()
            .init_resource::<LocalSettings>()
            .init_resource::<WindowLayout>()
            .init_resource::<MutedDids>()
            .init_resource::<GizmoFramePref>()
            .init_resource::<AudioMuted>()
            // Restore at startup, then follow the session and persist
            // (debounced) whenever something changes. Every AppState: a
            // session appears in `Login`, leaves on the way out of `InGame`
            // or an aborted `Loading`, and the login screen's own controls
            // are drawn before either.
            .add_systems(Startup, load_prefs_at_startup)
            .add_systems(
                Update,
                (
                    follow_session_prefs,
                    mirror_login_screen_settings,
                    save_prefs_when_changed,
                )
                    .chain()
                    .in_set(PrefsSystems),
            );
    }
}

/// Startup: install the login screen's settings.
///
/// Nobody is signed in yet, so the live set is the login screen's theme
/// and size over defaults; an account's own set arrives with its session,
/// through [`follow_session_prefs`]. The first run after #1407 finds no
/// machine slot and seeds one from the old shared file, so the login
/// screen looks exactly as it did before the upgrade.
pub fn load_prefs_at_startup(mut commands: Commands, store: Res<PrefsStore>) {
    let machine = store
        .load::<MachinePrefs>(Slot::Machine)
        .unwrap_or_else(|| {
            store
                .load::<LegacyPrefs>(Slot::Legacy)
                .map(|legacy| legacy.machine())
                .unwrap_or_default()
        });
    let login = machine.login_screen.unwrap_or_default();
    commands.insert_resource(login_screen_local_settings(&login));
    commands.insert_resource(login);
    commands.insert_resource(LegacyMutedDids(machine.unclaimed_mutes));
}

/// Keep the live settings on the signed-in account's set (#1407).
///
/// Runs every frame and acts when the session's DID differs from
/// [`PrefsOwner`] - a sign-in, the wasm resume, a logout, an aborted
/// loading screen - so no door into or out of a session has to remember
/// it. #1214's in-place re-authenticate inserts a new session for the SAME
/// DID, and that is deliberately nothing: reloading would throw away
/// whatever changed since the last save.
///
/// The order is the point. The outgoing account's set is written to its
/// slot BEFORE the incoming one replaces it, while the live resources
/// still hold it: the debounced save is up to five seconds behind, and a
/// save that fired after the swap would file the last few changes under
/// the wrong account - the #1223 f292 defect in general form. Which is
/// also why logout no longer resets the mute list itself: that ran first
/// and would have saved an empty list here.
pub fn follow_session_prefs(
    session: Option<Res<AtprotoSession>>,
    mut owner: ResMut<PrefsOwner>,
    store: Res<PrefsStore>,
    login: Res<LoginScreenSettings>,
    mut legacy_mutes: ResMut<LegacyMutedDids>,
    mut live: LivePrefs,
) {
    let signed_in = session.as_deref().map(|session| session.did.as_str());
    if signed_in == owner.0.as_deref() {
        return;
    }
    if let Some(outgoing) = owner.0.as_deref() {
        store.save(Slot::Account(outgoing), &live.capture());
    }
    match signed_in {
        None => live.install(&AccountPrefs::default(), &login),
        Some(did) => {
            let mut prefs = store
                .load::<AccountPrefs>(Slot::Account(did))
                .unwrap_or_else(|| {
                    seed_account(
                        store.load::<LegacyPrefs>(Slot::Legacy).as_ref(),
                        did,
                        &login,
                    )
                });
            // Bypassed, because a `&mut` through the `ResMut` stamps the
            // resource whether or not there is anything to take (#879).
            if let Some(list) = legacy_mutes.bypass_change_detection().0.take() {
                legacy_mutes.set_changed();
                prefs.muted.get_or_insert_default().0.extend(list.0);
                // One-time and not repeatable, so both halves are written
                // now rather than a debounce later: a quit in between would
                // hand the list to whoever signed in next.
                store.save(Slot::Account(did), &prefs);
                store.save(Slot::Machine, &MachinePrefs::capture(&login, &legacy_mutes));
            }
            live.install(&prefs, &login);
        }
    }
    owner.0 = signed_in.map(str::to_owned);
}

/// While nobody is signed in, the theme and size on screen ARE the login
/// screen's (#1407): carry what its controls change - the login card's
/// picker, Ctrl+plus / Ctrl+minus through `theme::sync_ui_scale` - into
/// [`LoginScreenSettings`], where they survive the next account's session
/// and the next launch.
///
/// Guarded both ways: nothing while an account's set is live, and a write
/// only when the pair actually differs, because the swap back at logout
/// installs these very values and must not re-save them.
pub fn mirror_login_screen_settings(
    owner: Res<PrefsOwner>,
    settings: Res<LocalSettings>,
    mut login: ResMut<LoginScreenSettings>,
) {
    if owner.0.is_some() || !settings.is_changed() {
        return;
    }
    if login.theme != settings.theme || login.ui_scale != settings.ui_scale {
        login.theme = settings.theme;
        login.ui_scale = settings.ui_scale;
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

/// Trailing-debounce state for [`save_prefs_when_changed`], and which of
/// the two slots the burst touched.
#[derive(Default)]
pub struct SaveDebounce {
    pending: Option<PendingSave>,
    machine: bool,
    account: bool,
}

/// Step the debounce: a change (re)arms the trailing deadline - clamped
/// to the max-latency cap the burst's FIRST change fixed - and a
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

/// Persist the live settings shortly after the last change: the account's
/// set to the signed-in account's slot, the login screen's to the
/// machine's. Change detection also fires on a swap's own install - that
/// lone extra write of identical data is harmless, and it is what first
/// persists a newly seeded account.
///
/// A burst still pending when its account signs out is not lost:
/// [`follow_session_prefs`] wrote that account's slot on the way out, and
/// with nobody signed in the account half of the burst has nowhere to go.
#[allow(clippy::too_many_arguments)]
pub fn save_prefs_when_changed(
    store: Res<PrefsStore>,
    owner: Res<PrefsOwner>,
    panels: Res<UiPanels>,
    settings: Res<LocalSettings>,
    windows: Res<WindowLayout>,
    muted: Res<MutedDids>,
    // Guarded-dirty at the source (#871): the editors borrow the pref
    // bypassed and tick it only on a real toggle/edit.
    gizmo: Res<GizmoFramePref>,
    // Same discipline (#1276 f38): the toolbar toggle and the Settings
    // checkbox both copy the bool out, hand the WIDGET the local, and
    // write back only on a real click - so this ticks on a toggle and
    // never merely because a panel that shows it is open.
    audio: Res<AudioMuted>,
    login: Res<LoginScreenSettings>,
    legacy_mutes: Res<LegacyMutedDids>,
    time: Res<Time>,
    mut debounce: Local<SaveDebounce>,
) {
    let account = owner.0.is_some()
        && (panels.is_changed()
            || settings.is_changed()
            || windows.is_changed()
            || muted.is_changed()
            || gizmo.is_changed()
            || audio.is_changed());
    let machine = login.is_changed() || legacy_mutes.is_changed();
    debounce.account |= account;
    debounce.machine |= machine;
    let (pending, fire) = debounce_step(
        debounce.pending,
        account || machine,
        time.elapsed_secs_f64(),
    );
    debounce.pending = pending;
    if !fire {
        return;
    }
    if std::mem::take(&mut debounce.machine) {
        store.save(Slot::Machine, &MachinePrefs::capture(&login, &legacy_mutes));
    }
    if std::mem::take(&mut debounce.account)
        && let Some(did) = owner.0.as_deref()
    {
        store.save(
            Slot::Account(did),
            &AccountPrefs::capture(&panels, &settings, &windows, &muted, &gizmo, &audio),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::UserTheme;

    const ALICE: &str = "did:plc:alice";
    const BOB: &str = "did:plc:bob";
    const CAROL: &str = "did:plc:carol";

    /// A signed-in account. The prefs layer reads only `did`; the rest is
    /// the live signing session the type insists on (the fixture
    /// `ui::room`'s tests use).
    fn session(did: &str) -> AtprotoSession {
        AtprotoSession {
            did: String::from(did),
            handle: String::from("someone"),
            pds_url: String::from("https://pds.example"),
            session: Arc::new(proto_blue_oauth::session::OAuthSession::new(
                proto_blue_oauth::types::TokenSet {
                    issuer: String::from("https://as.example"),
                    sub: String::from(did),
                    scope: String::from("atproto"),
                    access_token: String::from("access"),
                    refresh_token: Some(String::from("refresh")),
                    token_type: String::from("DPoP"),
                    expires_at: Some(String::from("2099-01-01T00:00:00Z")),
                    aud: Some(String::from("https://as.example")),
                },
                proto_blue_oauth::DpopKey::generate().expect("DPoP key"),
                proto_blue_oauth::DpopNonceCache::new(),
            )),
        }
    }

    /// The prefs layer alone, over `store`, through its first frame. Time
    /// stands still unless [`settle`] moves it, so a debounced save fires
    /// only where a test asks for one.
    fn prefs_app(store: &PrefsStore) -> App {
        let mut app = App::new();
        app.insert_resource(store.clone())
            .init_resource::<Time>()
            .add_plugins(PrefsPlugin);
        app.update();
        app
    }

    fn sign_in(app: &mut App, did: &str) {
        app.insert_resource(session(did));
        app.update();
    }

    fn sign_out(app: &mut App) {
        app.world_mut().remove_resource::<AtprotoSession>();
        app.update();
    }

    /// Let every pending debounced save fire: one frame for a change made
    /// since the last to register and arm the debounce, then one past its
    /// deadline.
    fn settle(app: &mut App) {
        app.update();
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f64(
                SAVE_MAX_LATENCY_SECS + 1.0,
            ));
        app.update();
    }

    fn account_slot(store: &PrefsStore, did: &str) -> AccountPrefs {
        store
            .load(Slot::Account(did))
            .expect("the account has a slot")
    }

    fn machine_slot(store: &PrefsStore) -> MachinePrefs {
        store.load(Slot::Machine).expect("the machine has a slot")
    }

    /// The owner's request (#1407), in the order it was put: settings made
    /// by one account are that account's, and the next one to sign in on
    /// the same machine - in the same run - starts from its own.
    #[test]
    fn each_account_keeps_its_own_settings() {
        let store = PrefsStore::memory();
        let mut app = prefs_app(&store);

        sign_in(&mut app, ALICE);
        {
            let world = app.world_mut();
            world.resource_mut::<UiPanels>().chat = true;
            world.resource_mut::<LocalSettings>().theme = UserTheme::Light;
            world
                .resource_mut::<WindowLayout>()
                .rects
                .insert("chat".to_owned(), [890.0, 40.0, 380.0, 400.0]);
            world.resource_mut::<AudioMuted>().0 = false;
            world
                .resource_mut::<MutedDids>()
                .set("did:plc:harasser", true);
        }
        app.update();
        sign_out(&mut app);

        // The login screen has no mute control, so it is silent whatever
        // the last account chose.
        assert!(
            app.world().resource::<AudioMuted>().0,
            "the login screen is muted"
        );

        sign_in(&mut app, BOB);
        {
            let world = app.world();
            assert!(!world.resource::<UiPanels>().chat, "alice's open Chat");
            assert_eq!(
                world.resource::<LocalSettings>().theme,
                UserTheme::Dark,
                "alice's theme"
            );
            assert!(
                world.resource::<WindowLayout>().rects.is_empty(),
                "alice's window layout"
            );
            assert!(world.resource::<AudioMuted>().0, "alice's unmute");
            assert!(
                world.resource::<MutedDids>().0.is_empty(),
                "alice's mute list"
            );
        }
        sign_out(&mut app);

        sign_in(&mut app, ALICE);
        let world = app.world();
        assert!(world.resource::<UiPanels>().chat);
        assert_eq!(world.resource::<LocalSettings>().theme, UserTheme::Light);
        assert_eq!(
            world.resource::<WindowLayout>().rects["chat"],
            [890.0, 40.0, 380.0, 400.0]
        );
        assert!(!world.resource::<AudioMuted>().0);
        assert!(world.resource::<MutedDids>().0.contains("did:plc:harasser"));
    }

    /// The sequence the swap's ordering exists for: a change made inside
    /// the debounce window, then straight out and into another account.
    /// The debounced save fires after the swap, and it must not file
    /// alice's last change under bob.
    #[test]
    fn a_change_just_before_logout_is_saved_to_the_account_that_made_it() {
        let store = PrefsStore::memory();
        let mut app = prefs_app(&store);

        sign_in(&mut app, ALICE);
        settle(&mut app);
        app.world_mut().resource_mut::<UiPanels>().people = true;
        app.update();
        sign_out(&mut app);
        assert!(
            account_slot(&store, ALICE)
                .panels
                .expect("panels saved")
                .people,
            "written on the way out, not left to a debounce that fires later"
        );

        sign_in(&mut app, BOB);
        settle(&mut app);
        assert!(
            !account_slot(&store, BOB)
                .panels
                .expect("panels saved")
                .people,
            "bob's slot holds bob's panels"
        );
        assert!(
            account_slot(&store, ALICE)
                .panels
                .expect("panels saved")
                .people,
            "and alice's still holds hers"
        );
    }

    /// The login screen's theme and size are its own (#1407, owner decision
    /// 2026-09-22): an account's pair replaces them while it is signed in,
    /// they come back at logout, and a NEW account starts from them.
    #[test]
    fn the_login_screen_keeps_its_own_theme_and_size() {
        let store = PrefsStore::memory();
        let mut app = prefs_app(&store);

        // What the login card's picker and Ctrl+plus write.
        {
            let mut settings = app.world_mut().resource_mut::<LocalSettings>();
            settings.theme = UserTheme::HighContrast;
            settings.ui_scale = 1.5;
        }
        app.update();
        let login = app.world().resource::<LoginScreenSettings>().clone();
        assert_eq!(
            (login.theme, login.ui_scale),
            (UserTheme::HighContrast, 1.5)
        );

        sign_in(&mut app, ALICE);
        {
            let settings = app.world().resource::<LocalSettings>();
            assert_eq!(
                (settings.theme, settings.ui_scale),
                (UserTheme::HighContrast, 1.5),
                "a new account starts from the login screen's pair"
            );
        }
        {
            let mut settings = app.world_mut().resource_mut::<LocalSettings>();
            settings.theme = UserTheme::Light;
            settings.ui_scale = 1.0;
        }
        app.update();
        let login = app.world().resource::<LoginScreenSettings>().clone();
        assert_eq!(
            (login.theme, login.ui_scale),
            (UserTheme::HighContrast, 1.5),
            "an account's own pair does not move the login screen's"
        );

        sign_out(&mut app);
        let settings = app.world().resource::<LocalSettings>().clone();
        assert_eq!(
            (settings.theme, settings.ui_scale),
            (UserTheme::HighContrast, 1.5),
            "logging out brings the login screen's pair back"
        );
        settle(&mut app);

        // And the next launch opens on it.
        let app = prefs_app(&store);
        let settings = app.world().resource::<LocalSettings>();
        assert_eq!(
            (settings.theme, settings.ui_scale),
            (UserTheme::HighContrast, 1.5)
        );
        assert_eq!(
            account_slot(&store, ALICE).settings.map(|s| s.theme),
            Some(UserTheme::Light),
            "alice's own theme is in her slot"
        );
    }

    /// The backdrop switch lives in the in-game Settings window but is the
    /// machine's (#1407): flipped while signed in, it lands in the
    /// machine's slot and outlives the session.
    #[test]
    fn the_backdrop_switch_is_the_machines_even_when_set_in_game() {
        let store = PrefsStore::memory();
        let mut app = prefs_app(&store);
        sign_in(&mut app, ALICE);
        app.world_mut()
            .resource_mut::<LoginScreenSettings>()
            .world_backdrop = false;
        settle(&mut app);
        sign_out(&mut app);
        sign_in(&mut app, BOB);

        assert!(!app.world().resource::<LoginScreenSettings>().world_backdrop);
        assert_eq!(
            machine_slot(&store).login_screen.map(|l| l.world_backdrop),
            Some(false)
        );
    }

    /// The upgrade (#1407, owner decision 2026-09-22). Every account that
    /// signs in after it starts from a copy of the old shared file - so none
    /// of them loses its layout - with its OWN mute list from that file
    /// (#1223 f292); the login screen takes the file's theme, size and
    /// backdrop; and the file itself is never written again.
    #[test]
    fn every_account_starts_from_a_copy_of_the_old_shared_file() {
        let store = PrefsStore::memory();
        let legacy = serde_json::json!({
            "panels": { "chat": true, "controls": false, "controls_seen": true },
            "settings": {
                "theme": "Light",
                "ui_scale": 1.25,
                "login_world_backdrop": false,
                "show_peer_nametags": false,
            },
            "windows": { "rects": { "chat": [1.0, 2.0, 3.0, 4.0] } },
            "muted_by_owner": {
                ALICE: ["did:plc:x"],
                BOB: ["did:plc:y"],
            },
            "gizmo": {
                "local_frame": false,
                "snap": true,
                "snap_distance": 0.5,
                "snap_angle_deg": 15.0,
                "snap_scale": 0.25,
            },
            "audio": { "muted": false },
        })
        .to_string();
        store.write(Slot::Legacy, &legacy).unwrap();

        let mut app = prefs_app(&store);
        let login = app.world().resource::<LoginScreenSettings>().clone();
        assert_eq!(
            (login.theme, login.ui_scale, login.world_backdrop),
            (UserTheme::Light, 1.25, false),
            "the login screen looks as it did before the upgrade"
        );

        for (did, own, other) in [
            (ALICE, "did:plc:x", "did:plc:y"),
            (BOB, "did:plc:y", "did:plc:x"),
        ] {
            sign_in(&mut app, did);
            let world = app.world();
            let panels = world.resource::<UiPanels>();
            assert!(panels.chat && !panels.controls && panels.controls_seen);
            let settings = world.resource::<LocalSettings>();
            assert_eq!(settings.theme, UserTheme::Light);
            assert!(!settings.show_peer_nametags);
            assert_eq!(
                world.resource::<WindowLayout>().rects["chat"],
                [1.0, 2.0, 3.0, 4.0]
            );
            let gizmo = world.resource::<GizmoFramePref>();
            assert_eq!(gizmo.orientation, GizmoOrientation::Global);
            assert!(gizmo.snap);
            assert!(!world.resource::<AudioMuted>().0);
            let muted = world.resource::<MutedDids>();
            assert!(muted.0.contains(own), "{did} keeps their own mute list");
            assert!(!muted.0.contains(other), "and never another account's");
            settle(&mut app);
            sign_out(&mut app);
        }

        assert_eq!(
            store.read(Slot::Legacy).as_deref(),
            Some(legacy.as_str()),
            "the old file is kept unchanged as the starting copy"
        );
    }

    /// A file from before #1223 has ONE mute list for the whole machine. The
    /// #1223 rule stands: the first account to sign in takes it, and nobody
    /// after - not the next account, and not the first account of the next
    /// launch.
    #[test]
    fn a_pre_1223_machine_wide_mute_list_goes_to_the_first_account_only() {
        let store = PrefsStore::memory();
        store
            .write(Slot::Legacy, r#"{"muted_dids": ["did:plc:old"]}"#)
            .unwrap();

        let mut app = prefs_app(&store);
        sign_in(&mut app, ALICE);
        assert!(
            app.world()
                .resource::<MutedDids>()
                .0
                .contains("did:plc:old")
        );
        sign_out(&mut app);
        sign_in(&mut app, BOB);
        assert!(app.world().resource::<MutedDids>().0.is_empty());
        drop(app);

        let mut app = prefs_app(&store);
        sign_in(&mut app, CAROL);
        assert!(
            app.world().resource::<MutedDids>().0.is_empty(),
            "a relaunch does not hand the list out again"
        );
        assert!(
            account_slot(&store, ALICE)
                .muted
                .expect("saved when taken")
                .0
                .contains("did:plc:old")
        );
    }

    /// #1214's in-place re-authenticate inserts a fresh session for the
    /// SAME account. Reloading the account's slot then would throw away
    /// everything changed since the last save.
    #[test]
    fn reauthenticating_as_the_same_account_keeps_what_is_on_screen() {
        let store = PrefsStore::memory();
        let mut app = prefs_app(&store);
        sign_in(&mut app, ALICE);
        settle(&mut app);
        app.world_mut().resource_mut::<UiPanels>().people = true;
        app.update();

        sign_in(&mut app, ALICE);
        assert!(app.world().resource::<UiPanels>().people);
    }

    /// Two copies of the app signed in side by side, as two accounts. With
    /// one shared file each save wrote its whole in-memory copy, so the last
    /// to save erased the other's changes - mute lists included.
    #[test]
    fn two_copies_of_the_app_as_two_accounts_do_not_overwrite_each_other() {
        let store = PrefsStore::memory();
        let mut first = prefs_app(&store);
        let mut second = prefs_app(&store);
        sign_in(&mut first, ALICE);
        sign_in(&mut second, BOB);

        // Bob saves first; alice's copy, which never saw his changes, saves
        // after. That order is the one that used to lose data.
        second.world_mut().resource_mut::<UiPanels>().people = true;
        second
            .world_mut()
            .resource_mut::<MutedDids>()
            .set("did:plc:harasser", true);
        settle(&mut second);
        first.world_mut().resource_mut::<UiPanels>().chat = true;
        settle(&mut first);

        let alice = account_slot(&store, ALICE);
        let bob = account_slot(&store, BOB);
        assert!(alice.panels.as_ref().is_some_and(|p| p.chat && !p.people));
        assert!(bob.panels.as_ref().is_some_and(|p| p.people && !p.chat));
        assert!(
            bob.muted.is_some_and(|m| m.0.contains("did:plc:harasser")),
            "bob's mute survives alice's copy saving after it"
        );
    }

    /// The empty-store path must leave a world the prefs systems can run in
    /// (#1317).
    ///
    /// A first visit - or a slot that no longer parses - loads nothing, so
    /// every resource the systems require has to be registered as a
    /// default. Getting it wrong is not a degraded feature, it is the whole
    /// app: under Bevy 0.19 a missing required parameter is a **panic**, and
    /// on wasm that aborts the module and freezes the canvas on the last
    /// frame it drew - which is how a missing mute-list default once shipped
    /// a login screen showing the attract backdrop and no UI.
    /// [`PrefsPlugin`] registers them itself; this runs it with nothing
    /// else, through a sign-in, a save and a sign-out.
    #[test]
    fn the_prefs_layer_runs_on_an_empty_store() {
        let store = PrefsStore::memory();
        let mut app = prefs_app(&store);
        sign_in(&mut app, ALICE);
        settle(&mut app);
        sign_out(&mut app);
        assert_eq!(app.world().resource::<PrefsOwner>().0, None);
        assert!(
            store.read(Slot::Legacy).is_none(),
            "nothing wrote the old file"
        );
    }

    /// #1226 f325. The sequence: an existing user updates the app and their
    /// prefs file predates the nametag setting entirely. `LocalSettings`
    /// grows only with `serde(default)`-compatible fields, so the missing
    /// one must come back ON - an upgrade that silently switched off the
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
        assert!(
            settings.load_external_assets,
            "and so does external-asset loading (#1248 f298) - an upgrade that \
             silently stopped following URL references would blank most of the \
             imagery in the product with no explanation"
        );
    }

    #[test]
    fn an_account_slot_round_trips_every_field() {
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
            load_external_assets: false,
            theme: UserTheme::HighContrast,
            ui_scale: 1.4,
            ..Default::default()
        };
        let mut windows = WindowLayout::default();
        windows
            .rects
            .insert("chat".to_owned(), [890.0, 40.0, 380.0, 400.0]);
        let mut muted = MutedDids::default();
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
        let prefs = AccountPrefs {
            panels: Some(panels),
            settings: Some(settings),
            windows: Some(windows),
            muted: Some(muted),
            gizmo: Some(gizmo),
            audio: Some(AudioPrefs { muted: false }),
        };
        let json = serde_json::to_string(&prefs).unwrap();
        let back: AccountPrefs = serde_json::from_str(&json).unwrap();
        assert_eq!(back, prefs);
        // The mirror round-trips through the live resource shape too:
        // a persisted World choice survives the Local default (#871).
        let restored_pref = GizmoFramePref::from(back.gizmo.as_ref().unwrap());
        assert_eq!(restored_pref.orientation, GizmoOrientation::Global);
        assert_eq!(
            GizmoPrefs::from(&restored_pref),
            *back.gizmo.as_ref().unwrap()
        );
    }

    #[test]
    fn missing_and_unknown_fields_degrade_gracefully() {
        // An empty slot → every field None, no error.
        let empty: AccountPrefs = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, AccountPrefs::default());
        let empty: MachinePrefs = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, MachinePrefs::default());
        // A slot written by a NEWER binary carries fields we don't know;
        // serde ignores them rather than failing the whole load.
        let newer: AccountPrefs =
            serde_json::from_str(r#"{"panels": null, "window_rects": {"chat": [1, 2, 3, 4]}}"#)
                .unwrap();
        assert!(newer.panels.is_none());
        // A panels object missing NEW bools fills them from Default -
        // the forward-compat contract for growing UiPanels.
        let partial: AccountPrefs = serde_json::from_str(r#"{"panels": {"chat": true}}"#).unwrap();
        let panels = partial.panels.unwrap();
        assert!(panels.chat);
        assert!(panels.controls, "missing fields take UiPanels defaults");
        // The old file's settings still carry the backdrop field; the
        // account half reads straight past it.
        let legacy: LegacyPrefs = serde_json::from_str(
            r#"{"settings": {"login_world_backdrop": false, "smooth_kinematics": false}}"#,
        )
        .unwrap();
        assert!(!legacy.local_settings().unwrap().smooth_kinematics);
        assert_eq!(
            legacy.machine().login_screen.map(|l| l.world_backdrop),
            Some(false)
        );
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
        // trailing debounce re-armed forever and never fired - prefs
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

    /// A DID names a file on every desktop platform (#1407): Windows
    /// refuses the colons every DID has, and nothing in a DID may climb
    /// out of the accounts directory.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn account_file_names_are_one_to_one_and_stay_in_their_directory() {
        assert_eq!(
            account_file_name("did:plc:abc234"),
            "did%3Aplc%3Aabc234.json"
        );
        assert_eq!(
            account_file_name("did:web:example.com%3A8080"),
            "did%3Aweb%3Aexample.com%253A8080.json",
            "a did:web's own percent sign is encoded too"
        );
        assert_ne!(
            account_file_name("did:web:a%3Ab"),
            account_file_name("did:web:a:b"),
            "which is what keeps two accounts from sharing a file"
        );
        for hostile in [
            "did:web:../../etc",
            "did:web:a/b",
            "did:web:a\\b",
            "..",
            ".",
        ] {
            let name = account_file_name(hostile);
            assert!(!name.contains('/') && !name.contains('\\'), "{name}");
            let path = std::path::Path::new(&name);
            assert_eq!(path.components().count(), 1, "{name}");
            assert!(
                matches!(
                    path.components().next(),
                    Some(std::path::Component::Normal(_))
                ),
                "{name}"
            );
        }
    }

    /// The native store end to end against a real directory: one file per
    /// slot, the rename leaving no temp file behind, and a corrupt file
    /// reading as empty (it heals on the next save) rather than a panic.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_directory_store_keeps_one_file_per_slot() {
        let dir = std::env::temp_dir().join(format!("symbios-prefs-slots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = PrefsStore::Dir(dir.clone());

        assert!(
            store.load::<MachinePrefs>(Slot::Machine).is_none(),
            "fresh install"
        );

        let alice = AccountPrefs {
            audio: Some(AudioPrefs { muted: false }),
            ..Default::default()
        };
        let machine = MachinePrefs {
            login_screen: Some(LoginScreenSettings {
                world_backdrop: false,
                ..Default::default()
            }),
            unclaimed_mutes: None,
        };
        store.save(Slot::Account(ALICE), &alice);
        store.save(Slot::Machine, &machine);

        assert!(
            dir.join("accounts")
                .join("did%3Aplc%3Aalice.json")
                .is_file()
        );
        assert!(dir.join("machine.json").is_file());
        assert!(
            !dir.join("prefs.json").exists(),
            "nothing writes the old file"
        );
        assert_eq!(
            store.load::<AccountPrefs>(Slot::Account(ALICE)),
            Some(alice)
        );
        assert_eq!(store.load::<MachinePrefs>(Slot::Machine), Some(machine));
        let leftovers: Vec<_> = [dir.clone(), dir.join("accounts")]
            .iter()
            .flat_map(|d| std::fs::read_dir(d).unwrap())
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name.to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");

        std::fs::write(dir.join("machine.json"), "{not json").unwrap();
        assert!(store.load::<MachinePrefs>(Slot::Machine).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// #1276 f38. The sequence the finding describes: unmute, quit, come
    /// back - and the world is silent again, because `AudioMuted` was the
    /// one preference nothing ever wrote.
    ///
    /// Driven through the FULL round trip - the live resource, the
    /// debounced save, a new launch, the sign-in that installs it - rather
    /// than over the struct alone, because the struct was never the part
    /// that was missing: the gap was that the save did not read the
    /// resource and the load did not install one.
    #[test]
    fn an_unmuted_choice_survives_a_restart() {
        let store = PrefsStore::memory();
        let mut app = prefs_app(&store);
        sign_in(&mut app, ALICE);
        app.world_mut().resource_mut::<AudioMuted>().0 = false;
        settle(&mut app);
        drop(app);

        let mut app = prefs_app(&store);
        sign_in(&mut app, ALICE);
        assert_eq!(
            *app.world().resource::<AudioMuted>(),
            AudioMuted(false),
            "the next launch is not silent"
        );

        // And the muted direction round-trips too, so the test is not
        // passing on `AudioMuted`'s own default.
        app.world_mut().resource_mut::<AudioMuted>().0 = true;
        settle(&mut app);
        drop(app);
        let mut app = prefs_app(&store);
        sign_in(&mut app, ALICE);
        assert_eq!(*app.world().resource::<AudioMuted>(), AudioMuted(true));
    }

    /// A prefs file written before #1276 f38 has no audio key, and that
    /// must leave the session SILENT.
    ///
    /// The opposite of the nametag rule above, and deliberately so: an
    /// absent boolean means "no opinion", and the app's no-opinion answer
    /// for sound is the muted default it has always had. An upgrade that
    /// read a missing key as "unmuted" would start playing music at
    /// somebody who had never asked for any.
    #[test]
    fn a_prefs_file_written_before_the_audio_key_still_starts_silent() {
        let store = PrefsStore::memory();
        store
            .write(Slot::Legacy, r#"{"panels":{"chat":true}}"#)
            .unwrap();
        let mut app = prefs_app(&store);
        sign_in(&mut app, ALICE);
        assert!(
            app.world().resource::<UiPanels>().chat,
            "the copy was taken"
        );
        assert!(
            app.world().resource::<AudioMuted>().0,
            "and the absent audio key left the session muted"
        );
    }
}
