//! Browser-side helpers for reading/writing [`super::PendingAuth`] into
//! `sessionStorage`, inspecting the callback query string, and scrubbing
//! the URL bar after a successful exchange so reloads don't re-trigger
//! the `code` exchange (which would 400 - the authorization code is
//! single-use).
//!
//! Also home to the [`PersistedSession`] blob written to `localStorage`
//! after a successful login so a page reload restores the session
//! without forcing the user back through the OAuth dance.
//!
//! Since #1408 there is one such blob PER ACCOUNT, and each tab remembers
//! in `sessionStorage` which account it signed in as - so two tabs can hold
//! two accounts, each restoring its own on a reload. A tab that has signed
//! in as nobody is OFFERED the browser's most recent account rather than
//! signed into it; [`saved_session_on_boot`] is the whole decision, and
//! `super::session_on_boot` the rule behind it.

use proto_blue_oauth::{OAuthServerMetadata, types::TokenSet};
use serde::{Deserialize, Serialize};

use super::discovery::WASM_REDIRECT_URI;
use super::util::{CallbackParams, parse_query_params};
use super::{KeyValue, PendingAuth, SESSION_STORAGE_KEY, SessionOnBoot, SessionStore};

/// Retrieve the browser's `sessionStorage`, or `None` if it is not
/// available (private-browsing mode in some browsers).
fn session_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.session_storage().ok().flatten()
}

/// Persist a pending auth record for later retrieval in the callback.
pub fn store_pending(pending: &PendingAuth) -> Result<(), String> {
    let storage = session_storage()
        .ok_or_else(|| "sessionStorage unavailable (private mode?)".to_string())?;
    let json =
        serde_json::to_string(pending).map_err(|e| format!("serialize pending auth: {e}"))?;
    storage
        .set_item(SESSION_STORAGE_KEY, &json)
        .map_err(|e| format!("sessionStorage.setItem: {e:?}"))
}

/// Take the stored pending auth record (clears it from storage).
pub fn take_pending() -> Option<PendingAuth> {
    let storage = session_storage()?;
    let raw = storage.get_item(SESSION_STORAGE_KEY).ok().flatten()?;
    let _ = storage.remove_item(SESSION_STORAGE_KEY);
    serde_json::from_str(&raw).ok()
}

/// Parse the recognised OAuth callback parameters (`code`, `state`,
/// `error`, `error_description`) out of the current URL's query string.
/// Returns an all-`None` [`CallbackParams`] on a bare page load.
pub fn read_callback_params() -> CallbackParams {
    // URL query parser via `web_sys::UrlSearchParams` would need the
    // "UrlSearchParams" feature enabled. A tiny hand-roll is lighter.
    web_sys::window()
        .and_then(|w| w.location().search().ok())
        .map(|search| parse_query_params(search.trim_start_matches('?')))
        .unwrap_or_default()
}

/// Replace the current URL with the bare redirect origin so reloads
/// after a successful login do not re-submit the consumed code.
///
/// The scrubbed URL has a trailing slash even though
/// [`WASM_REDIRECT_URI`] does not. Without the slash the browser treats
/// `/symbios-overlands` as a file and resolves subsequent relative
/// fetches (e.g. Bevy's `assets/shaders/splat.wgsl`) against the parent
/// directory, producing 404s for every asset loaded post-login.
pub fn scrub_url() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(history) = window.history() else {
        return;
    };
    let scrubbed = format!("{}/", WASM_REDIRECT_URI);
    let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&scrubbed));
}

/// Navigate the current tab to an authorization URL - this terminates
/// the running Bevy app. Returns immediately on error.
pub fn navigate_to(url: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().assign(url);
    }
}

// ──────────────────────────────────────────────────────────────────
// Persisted-session blob (localStorage)
// ──────────────────────────────────────────────────────────────────

/// Serializable bundle of everything a fresh page load needs to rebuild
/// the in-memory `OAuthSession` + `AtprotoSession` + `OauthRefreshCtx`
/// without re-running the OAuth dance. Stored as JSON in `localStorage`
/// under its own account's key (`super::account_session_key`, #1408).
///
/// The DPoP private key lives here as a JWK (`serde_json::Value`) - the
/// `DpopKey` runtime type isn't directly serialisable, but
/// `proto_blue_oauth::client::dpop_key_from_jwk` turns the JWK back into
/// one cheaply on resume.
///
/// # Threat model note
///
/// The persisted blob contains the DPoP private key in cleartext -
/// anyone with read access to this origin's `localStorage` can mint
/// authenticated requests until the refresh token expires. That is the
/// same trust boundary every browser-resident OAuth client operates
/// under (cf. RFC 8252 §8.6); we accept it for the sake of
/// reload-resilience. If you're targeting a stricter threat model,
/// disable persistence at compile time and force re-auth on every load.
///
/// Per-account slots (#1408) do not move that boundary, but they do make
/// the accounts that have signed in on this browser legible to anybody
/// reading its storage: a key per DID, where there used to be one for
/// whoever was last. Logging out removes that account's key.
#[derive(Serialize, Deserialize, Clone)]
pub struct PersistedSession {
    pub token_set: TokenSet,
    pub dpop_jwk: serde_json::Value,
    pub server_metadata: OAuthServerMetadata,
    pub did: String,
    pub handle: String,
    pub pds_url: String,
    /// Relay host captured at login. Carried so the resume path can
    /// rebuild `RelayHost` + `SymbiosMultiuserConfig` without prompting.
    pub relay_host: String,
    /// Destination DID at the time of login (empty = "home"). Same
    /// rationale as `relay_host` - we want the reload to land the user
    /// back in the room they were viewing.
    ///
    /// Spawn pose (`pos` / `rot`) is deliberately *not* persisted: it
    /// is a one-shot hint from a landmark link and would otherwise
    /// teleport the user back to that spot on every page reload.
    pub target_did: String,
}

/// Retrieve the browser's `localStorage`, or `None` if it is not
/// available (private-browsing modes that disable it).
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// One browser store as the [`KeyValue`] the policy is written against.
///
/// A store the browser refuses (private browsing with storage disabled)
/// becomes `None` here and every operation on it is a no-op: the session
/// lives in memory for as long as the page does, which is the posture this
/// module has always taken.
struct BrowserStore(Option<web_sys::Storage>);

impl KeyValue for BrowserStore {
    fn get(&self, key: &str) -> Option<String> {
        self.0.as_ref()?.get_item(key).ok().flatten()
    }

    fn set(&self, key: &str, value: &str) {
        if let Some(storage) = self.0.as_ref() {
            let _ = storage.set_item(key, value);
        }
    }

    fn remove(&self, key: &str) {
        if let Some(storage) = self.0.as_ref() {
            let _ = storage.remove_item(key);
        }
    }
}

/// Run `f` against this tab's view of the browser's session storage: its
/// own `sessionStorage` and the `localStorage` every tab shares.
///
/// Everything below is a few lines around this: the policy - which tab owns
/// which account, what a fresh one is offered, what a logout takes with it -
/// lives in [`super::session_store`], where the tests can reach it.
fn with_store<R>(f: impl FnOnce(&SessionStore<'_>) -> R) -> R {
    let tab = BrowserStore(session_storage());
    let shared = BrowserStore(local_storage());
    f(&SessionStore {
        tab: &tab,
        shared: &shared,
    })
}

/// Read one account's saved session. A blob that no longer deserialises is
/// dropped - once we have decided we cannot use it, leaving it in place
/// would just fail the same way on every later load.
fn read_slot(store: &SessionStore<'_>, did: &str) -> Option<PersistedSession> {
    let raw = store.slot(did)?;
    match serde_json::from_str::<PersistedSession>(&raw) {
        Ok(session) => Some(session),
        Err(_) => {
            store.drop_unreadable_slot(did);
            None
        }
    }
}

fn write_slot(store: &SessionStore<'_>, session: &PersistedSession) -> Result<(), String> {
    let json = serde_json::to_string(session).map_err(|e| format!("serialize session: {e}"))?;
    store.write_slot(&session.did, &json);
    Ok(())
}

/// A saved session this page load may act on (#1408).
pub enum SavedSession {
    /// This tab's own: restore it without asking.
    Resume(PersistedSession),
    /// The browser's most recent, for a tab that has signed in as nobody:
    /// offer it, and let the person choose it or somebody else.
    Offer(PersistedSession),
}

/// What this page load should do with the browser's saved sessions
/// (#1408). Migrates a pre-#1408 blob on the way past.
pub fn saved_session_on_boot() -> Option<SavedSession> {
    with_store(|store| match store.on_boot() {
        SessionOnBoot::Resume(did) => read_slot(store, &did).map(SavedSession::Resume),
        SessionOnBoot::Offer(did) => read_slot(store, &did).map(SavedSession::Offer),
        SessionOnBoot::None => None,
    })
}

/// Whether this page load has a saved session to restore or to offer.
///
/// The login card's one question (#1228 f6, #1408): it gates the Retry
/// button and, through `boot_params::entry_plan`, keeps a landmark link
/// from auto-submitting a fresh login over the top of either.
pub fn has_saved_session() -> bool {
    saved_session_on_boot().is_some()
}

/// Whether this page load restores a session without asking - a reload of
/// a signed-in tab (#1408). An OFFER is not a handoff: that screen is idle
/// until somebody clicks.
pub fn will_resume_without_asking() -> bool {
    matches!(saved_session_on_boot(), Some(SavedSession::Resume(_)))
}

/// Make `did` this tab's account (#1408), so its saved session is restored
/// on a reload and every later write - a rotated token set, an arrival -
/// lands in that account's slot and no other's.
///
/// Called where a tab takes an account on: the Continue button, and
/// [`save_persisted`] at the end of a fresh login.
pub fn claim_session(did: &str) {
    with_store(|store| store.claim(did));
}

/// Persist a freshly-built session blob, and make it this tab's. Called
/// once at login completion.
pub fn save_persisted(session: &PersistedSession) -> Result<(), String> {
    with_store(|store| {
        write_slot(store, session)?;
        store.claim(&session.did);
        Ok(())
    })
}

/// Read THIS TAB's saved session, or `None` when it has signed in as
/// nobody (a fresh tab - see [`saved_session_on_boot`] for what it may be
/// offered instead).
pub fn load_persisted() -> Option<PersistedSession> {
    with_store(|store| read_slot(store, &store.tab_did()?))
}

/// Forget THIS TAB's saved session. Called on logout and whenever a
/// refresh fails terminally (refresh token rejected → user must
/// re-authenticate).
///
/// Only this tab's (#1408): another tab signed in as somebody else keeps
/// its own, and the browser stops offering this account only because it
/// was the one signed out.
pub fn clear_persisted() {
    with_store(|store| store.forget_tab());
}

/// Forget a session this tab was OFFERED rather than signed into (#1408) -
/// the "Not @alice?" answer on a fresh tab.
pub fn forget_offered_session(did: &str) {
    with_store(|store| store.forget(did));
}

/// Rotate the `token_set` portion of this tab's saved session in place.
/// Called after every successful `OAuthSession::refresh` so a subsequent
/// reload doesn't restore the now-stale access token.
///
/// No-ops (returns `Ok`) when this tab has no saved session - refresh is
/// allowed without persistence, e.g. on native or when the user
/// explicitly opted out of localStorage.
pub fn update_persisted_token_set(new_token_set: &TokenSet) -> Result<(), String> {
    with_store(|store| {
        let Some(mut blob) = store.tab_did().and_then(|did| read_slot(store, &did)) else {
            return Ok(());
        };
        blob.token_set = new_token_set.clone();
        write_slot(store, &blob)
    })
}

/// Rotate the `target_did` portion of the persisted blob in place (#1229
/// f2), so a reload lands where the user actually is.
///
/// The field's own doc has always said "we want the reload to land the
/// user back in the room they were viewing", and nothing kept it: it was
/// written once at login completion and never again, so a visitor
/// onboarded through a friend's landmark link was resumed into that
/// friend's world on every later visit, and anybody who walked through a
/// portal was silently teleported back to their login-time room. Called
/// from [`crate::oauth::remember_room`] at the one site that replaces
/// `CurrentRoomDid`.
///
/// Same no-op-without-a-blob contract as
/// [`update_persisted_token_set`], and a no-op when the value has not
/// changed - this runs on every arrival, and a `localStorage` write per
/// travel is worth skipping when there is nothing to write.
pub fn update_persisted_target_did(target_did: &str) -> Result<(), String> {
    with_store(|store| {
        let Some(mut blob) = store.tab_did().and_then(|did| read_slot(store, &did)) else {
            return Ok(());
        };
        if blob.target_did == target_did {
            return Ok(());
        }
        blob.target_did = target_did.to_owned();
        write_slot(store, &blob)
    })
}
