//! OAuth 2.0 + DPoP authentication for Symbios Overlands.
//!
//! This module drives the full authorization-code flow against the user's
//! PDS, producing an authenticated `AtprotoSession` plus the relay
//! service-auth token minted by [`get_relay_service_auth`]. It is
//! target-specific:
//!
//! - **WASM** — redirect-based: the page navigates away to the auth server,
//!   state is parked in `sessionStorage`, and the callback lands back on
//!   the hosted page with `?code=&state=` query parameters.
//! - **Native** — loopback-based: a background `tiny_http` server on
//!   `127.0.0.1:3456` catches the redirect, and `webbrowser::open`
//!   launches the user's default browser to the auth URL.
//!
//! Identity discovery is intentionally minimal. Rather than pull in a full
//! DID/handle resolver (and a DNS stack that does not build on `wasm32`),
//! the login UI asks the user for their PDS URL directly and we discover
//! the authorization server via
//! `{pds}/.well-known/oauth-protected-resource` (atproto OAuth §3.2) —
//! which *is* a sufficient CORS-reachable discovery path on every target.
//!
//! ## Sub-module map
//!
//! * [`discovery`] — client-metadata builder, authorization-server
//!   discovery, and the URL-bar constants (`WASM_REDIRECT_URI`,
//!   `NATIVE_CALLBACK_PORT`, `CLIENT_METADATA_URL`).
//! * module root — the wasm browser-storage keys: `SESSION_STORAGE_KEY`
//!   (`sessionStorage`, parks the pending auth across the redirect) and
//!   `PERSISTED_SESSION_KEY` (`localStorage`, persists the session across
//!   page reloads).
//! * [`refresh`] — DPoP-nonce retry (`oauth_*_with_nonce_retry`),
//!   refresh-on-expiry retry (`oauth_*_with_refresh`), and the shared
//!   `refresh_session` helper.
//! * [`service_token`] — periodic re-mint of the short-lived relay
//!   service-auth token (#714) so every WebRTC (re)connect presents a
//!   valid token instead of the login-time one.
//! * [`auth_flow`] — `begin_authorization` and `complete_authorization`
//!   plus the `CompletedAuth` bundle.
//! * [`util`] — shared callback plumbing (the percent-decoder both the
//!   wasm query parser and the native listener use).
//! * `wasm` — browser-side `sessionStorage` / `localStorage` plumbing
//!   (compiled only on `wasm32`).
//! * [`native_server`] — the `tiny_http` loopback callback listener
//!   (compiled only on native).

mod auth_flow;
mod discovery;
#[cfg(not(target_arch = "wasm32"))]
mod native_server;
mod refresh;
mod service_token;
mod util;
#[cfg(target_arch = "wasm32")]
pub mod wasm;

use std::sync::Arc;

use bevy::prelude::*;
use proto_blue_oauth::{AuthState, OAuthClient, OAuthServerMetadata};
use serde::{Deserialize, Serialize};

pub use auth_flow::{CompletedAuth, begin_authorization, complete_authorization};
#[cfg(target_arch = "wasm32")]
pub use discovery::{CLIENT_METADATA_URL, WASM_REDIRECT_URI};
#[cfg(not(target_arch = "wasm32"))]
pub use discovery::{NATIVE_CALLBACK_PORT, native_redirect_uri};
pub use discovery::{RELAY_SERVICE_LXM, granular_scope};
pub use discovery::{client_metadata, discover_auth_server};
#[cfg(not(target_arch = "wasm32"))]
pub use native_server::{
    NativeCallbackOutcome, NativeCallbackReceiver, NativeCallbackServerHandle,
    NativeCallbackServerRes, parse_callback_query, start_native_callback_server,
};
pub use refresh::{
    fetch_session_identity, oauth_get_with_nonce_retry, oauth_post_with_nonce_retry,
    oauth_post_with_refresh, refresh_is_terminal, refresh_session,
};
pub use service_token::{
    get_relay_service_auth, poll_service_token_refresh, schedule_service_token_refresh,
};
pub use util::CallbackParams;

/// Remember which overland the browser should come back to on the next
/// reload (#1229 f2).
///
/// Target-neutral so the two sites that decide where the player *is* —
/// `ui::login::complete::install_completed_session` at sign-in and
/// `player::portal::poll_portal_travel_tasks` on arrival — can call it
/// without a `cfg` of their own. Native has no persisted session, so this
/// is a no-op there.
///
/// Best effort: a `localStorage` write that fails (private browsing, a
/// full origin quota) costs the reload its destination and nothing else,
/// and the session in memory is unaffected — the same posture
/// [`refresh_session`] takes for the rotated token set.
pub fn remember_room(room_did: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Err(e) = wasm::update_persisted_target_did(room_did) {
        warn!("update_persisted_target_did: {e}");
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = room_did;
}

/// In-flight OAuth authorization state persisted between the `authorize()`
/// call and the callback. On WASM this is serialized into
/// `sessionStorage`; on native it lives in a Mutex inside
/// [`NativePendingAuthRes`].
#[derive(Serialize, Deserialize, Clone)]
pub struct PendingAuth {
    pub auth_state: AuthState,
    pub server_metadata: OAuthServerMetadata,
    pub pds_url: String,
    /// Relay host captured from the login form, carried across the redirect
    /// so the room URL can be reassembled after the browser hop.
    pub relay_host: String,
    /// Target DID (for portal jumps) or empty for "home" — same UX as 0.2.
    pub target_did: String,
    /// Optional spawn position from the URL/CLI boot params, carried across
    /// the OAuth redirect so the post-callback spawn lands the user where
    /// the landmark link asked. `None` ⇒ random spawn-scatter as before.
    #[serde(default)]
    pub target_pos: Option<crate::boot_params::TargetPos>,
    /// Optional spawn yaw (degrees) from the URL/CLI boot params. `None` ⇒
    /// keep the heightmap-derived surface tilt with no extra rotation.
    #[serde(default)]
    pub target_yaw_deg: Option<f32>,
}

/// `sessionStorage` key holding the serialized [`PendingAuth`] between the
/// authorize-redirect and the callback parse on WASM.
///
/// Defined on every target although only the wasm-only `wasm` submodule
/// reads it (#1147) — hence plain text rather than a link, since that
/// module does not exist for rustdoc on native: browser
/// storage is a flat map shared by every app on the origin, so the
/// namespacing rule these keys follow is worth a test — and a test behind
/// `cfg(target_arch = "wasm32")` is a test nothing runs, since the suite has
/// no wasm runner. A `&str` costs nothing on native.
pub const SESSION_STORAGE_KEY: &str = "symbios_overlands_pending_auth";

/// `localStorage` key holding the serialized `wasm::PersistedSession`
/// across page reloads. Cleared on logout and on a refresh failure.
/// Defined unconditionally for the reason on [`SESSION_STORAGE_KEY`].
pub const PERSISTED_SESSION_KEY: &str = "symbios_overlands_session";

/// WASM-only marker: this page load began as an *auth handoff* rather
/// than a cold visit — the URL carries an OAuth callback (`?code=` or
/// `?error=`), or `localStorage` holds a session to resume. Inserted in
/// [`crate::run`] before the first frame, so the fact is settled outside
/// the ECS schedule entirely.
///
/// Why it can't be a system-side check (#978): the OAuth redirect
/// destroys the wasm app, so the callback lands on a *fresh* boot into
/// [`AppState::Login`](crate::state::AppState::Login). On frame 1
/// `ui::login::check_wasm_callback` spawns its
/// [`CompleteAuthTask`](crate::ui::login::CompleteAuthTask) through
/// `Commands`, and that spawn is not visible to any other system until
/// the queue flushes — so `attract::start_attract_scene`, sharing the
/// same unordered `Update` tuple, saw an idle login screen and seeded a
/// whole demo world that the imminent `Login → Loading` transition threw
/// away. Reading the browser state at app-build time has no such race.
///
/// Removed by whichever frame-1 one-shot resolves the handoff
/// (`check_wasm_callback` / `check_wasm_resume`); from then on the
/// in-flight auth *tasks* are the authority.
#[cfg(target_arch = "wasm32")]
#[derive(Resource)]
pub struct AuthHandoffPending;

/// Bevy resource holding everything `OAuthSession::refresh` needs.
///
/// The `OAuthSession` itself only carries the token set + DPoP key; refreshing
/// against the `/token` endpoint additionally requires the `OAuthClient` (for
/// its DPoP-nonce cache and its client metadata) and the `OAuthServerMetadata`
/// (which names the token endpoint URL). We persist both alongside the
/// session so any system holding `&AtprotoSession` can also borrow this and
/// drive a refresh without re-running the discovery dance.
///
/// Inserted by `ui::login::poll_complete_auth_task` after a successful OAuth
/// callback (and on WASM by the resume-from-localStorage system on boot).
/// Removed by `logout::cleanup_on_logout`.
#[derive(Resource, Clone)]
pub struct OauthRefreshCtx {
    pub client: Arc<OAuthClient>,
    pub server_metadata: OAuthServerMetadata,
}

/// Shared [`OAuthClient`] used by every login attempt this session. Wraps
/// an `Arc` so the async login task can hold its own handle without
/// borrowing the resource across await points.
#[derive(Resource, Clone)]
pub struct OauthClientRes(pub Arc<OAuthClient>);

impl Default for OauthClientRes {
    fn default() -> Self {
        Self(Arc::new(OAuthClient::new(client_metadata())))
    }
}

/// Native-side holder for the in-flight [`PendingAuth`] + callback
/// receiver. WASM parks the pending blob in `sessionStorage` instead, so
/// this resource exists only for native targets.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource, Default)]
pub struct NativePendingAuthRes(pub std::sync::Mutex<Option<PendingAuth>>);

/// Native-only: the authorization URL of the in-flight login attempt,
/// retained for the waiting UI's "Copy login URL" affordance. Before
/// #847 the URL was discarded right after the `webbrowser::open` call —
/// even when that call *failed* — leaving no recovery path short of
/// restarting the flow. Inserted alongside the callback listener
/// resources and removed with them (callback consumed, error redirect,
/// or user cancel).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource)]
pub struct NativeAuthUrl(pub String);

#[cfg(test)]
mod tests {
    /// THE SEQUENCE (#1229 f2): a visitor is onboarded through a friend's
    /// landmark link, walks home through a gateway, and reloads the page —
    /// and lands back in the friend's world. The persisted blob's
    /// `target_did` was written once at login completion and never again,
    /// so every later visit resumed into whichever room the session first
    /// entered, forever. Its own doc says the opposite ("we want the
    /// reload to land the user back in the room they were viewing").
    ///
    /// [`remember_room`] is the fix, and the property that keeps it fixed
    /// is that it is called wherever the current room CHANGES. Source
    /// scanning, the idiom `oauth::service_token` uses, because the write
    /// itself is `localStorage` — invisible on native, where the tests run
    /// — while "did somebody add a third way to change rooms" is a
    /// question about the code.
    #[test]
    fn every_site_that_changes_the_current_room_remembers_it() {
        /// The two files allowed to install or reassign `CurrentRoomDid`
        /// — the portal-arrival poll and the login installer — sorted, as
        /// the walk collects them. Both must call `remember_room`; a third
        /// file appearing here is a new travel path that owes the saved
        /// session the same answer.
        const ROOM_WRITERS: &[&str] = &["src/player/portal.rs", "src/ui/login/complete.rs"];

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut found: Vec<String> = Vec::new();
        let mut stack = vec![root.join("src")];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("src is readable") {
                let path = entry.expect("a readable entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") {
                    continue;
                }
                // This file quotes both needles to look for them.
                if path.ends_with("oauth/mod.rs") {
                    continue;
                }
                let source = std::fs::read_to_string(&path).expect("a readable file");
                // The definition, the teardown and the test fixtures do not
                // move anybody: only an install of a fresh value and a
                // mutable borrow can.
                let writes = source.contains("commands.insert_resource(CurrentRoomDid(")
                    || source.contains("ResMut<CurrentRoomDid>");
                if !writes {
                    continue;
                }
                let rel = path
                    .strip_prefix(root)
                    .expect("under the manifest dir")
                    .to_string_lossy()
                    .replace('\\', "/");
                assert!(
                    source.contains("remember_room"),
                    "{rel} changes the current room without calling \
                     oauth::remember_room — a reload will send the user back \
                     to wherever they last were told to be (#1229 f2)"
                );
                found.push(rel);
            }
        }
        found.sort();
        assert_eq!(
            found,
            ROOM_WRITERS
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            "the set of room writers moved; each one owes the saved session \
             an update"
        );
    }
}
