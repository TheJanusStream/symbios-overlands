//! Browser-side helpers for reading/writing [`super::PendingAuth`] into
//! `sessionStorage`, inspecting the callback query string, and scrubbing
//! the URL bar after a successful exchange so reloads don't re-trigger
//! the `code` exchange (which would 400 — the authorization code is
//! single-use).
//!
//! Also home to the [`PersistedSession`] blob written to `localStorage`
//! after a successful login so a page reload restores the session
//! without forcing the user back through the OAuth dance.

use proto_blue_oauth::{OAuthServerMetadata, types::TokenSet};
use serde::{Deserialize, Serialize};

use super::{PERSISTED_SESSION_KEY, PendingAuth, SESSION_STORAGE_KEY};
use super::discovery::WASM_REDIRECT_URI;

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

/// Parse `?code=&state=` out of the current URL, returning `(code,
/// state)` if both are present.
pub fn read_callback_params() -> Option<(String, String)> {
    let window = web_sys::window()?;
    let search = window.location().search().ok()?;
    // URL query parser via `web_sys::UrlSearchParams` would need the
    // "UrlSearchParams" feature enabled. A tiny hand-roll is lighter.
    let query = search.trim_start_matches('?');
    let mut code = None;
    let mut state = None;
    for pair in query.split('&') {
        let mut it = pair.splitn(2, '=');
        let k = it.next().unwrap_or("");
        let v = it.next().unwrap_or("");
        let decoded = percent_decode(v);
        match k {
            "code" => code = Some(decoded),
            "state" => state = Some(decoded),
            _ => {}
        }
    }
    Some((code?, state?))
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

/// Navigate the current tab to an authorization URL — this terminates
/// the running Bevy app. Returns immediately on error.
pub fn navigate_to(url: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().assign(url);
    }
}

/// Minimal percent-decoder for query values (handles `%HH` escapes
/// and `+` as space). OAuth callback values contain URL-encoded
/// characters and we don't want to pull in a full urlencoding crate.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let h = hex(bytes[i + 1]);
                let l = hex(bytes[i + 2]);
                match (h, l) {
                    (Some(h), Some(l)) => {
                        out.push((h << 4) | l);
                        i += 3;
                    }
                    _ => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

// ──────────────────────────────────────────────────────────────────
// Persisted-session blob (localStorage)
// ──────────────────────────────────────────────────────────────────

/// Serializable bundle of everything a fresh page load needs to rebuild
/// the in-memory `OAuthSession` + `AtprotoSession` + `OauthRefreshCtx`
/// without re-running the OAuth dance. Stored as JSON under
/// [`PERSISTED_SESSION_KEY`] in `localStorage`.
///
/// The DPoP private key lives here as a JWK (`serde_json::Value`) — the
/// `DpopKey` runtime type isn't directly serialisable, but
/// `proto_blue_oauth::client::dpop_key_from_jwk` turns the JWK back into
/// one cheaply on resume.
///
/// # Threat model note
///
/// The persisted blob contains the DPoP private key in cleartext —
/// anyone with read access to this origin's `localStorage` can mint
/// authenticated requests until the refresh token expires. That is the
/// same trust boundary every browser-resident OAuth client operates
/// under (cf. RFC 8252 §8.6); we accept it for the sake of
/// reload-resilience. If you're targeting a stricter threat model,
/// disable persistence at compile time and force re-auth on every load.
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
    /// rationale as `relay_host` — we want the reload to land the user
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

/// Persist a freshly-built session blob. Called once at login completion.
pub fn save_persisted(session: &PersistedSession) -> Result<(), String> {
    let storage =
        local_storage().ok_or_else(|| "localStorage unavailable (private mode?)".to_string())?;
    let json = serde_json::to_string(session).map_err(|e| format!("serialize session: {e}"))?;
    storage
        .set_item(PERSISTED_SESSION_KEY, &json)
        .map_err(|e| format!("localStorage.setItem: {e:?}"))
}

/// Read the persisted session blob, returning `None` if no blob is
/// stored or it can't be deserialised. A deserialisation failure also
/// clears the blob — once we've decided we can't use it, leaving it in
/// place would just trigger the same failure on every subsequent load.
pub fn load_persisted() -> Option<PersistedSession> {
    let storage = local_storage()?;
    let raw = storage.get_item(PERSISTED_SESSION_KEY).ok().flatten()?;
    match serde_json::from_str::<PersistedSession>(&raw) {
        Ok(v) => Some(v),
        Err(_) => {
            let _ = storage.remove_item(PERSISTED_SESSION_KEY);
            None
        }
    }
}

/// Drop the persisted blob. Called on logout and whenever a refresh
/// fails terminally (refresh token rejected → user must re-authenticate).
pub fn clear_persisted() {
    if let Some(storage) = local_storage() {
        let _ = storage.remove_item(PERSISTED_SESSION_KEY);
    }
}

/// Rotate the `token_set` portion of the persisted blob in place. Called
/// after every successful `OAuthSession::refresh` so a subsequent reload
/// doesn't restore the now-stale access token.
///
/// No-ops (returns `Ok`) when no persisted blob exists — refresh is
/// allowed without persistence, e.g. on native or when the user
/// explicitly opted out of localStorage.
pub fn update_persisted_token_set(new_token_set: &TokenSet) -> Result<(), String> {
    let Some(mut blob) = load_persisted() else {
        return Ok(());
    };
    blob.token_set = new_token_set.clone();
    save_persisted(&blob)
}
