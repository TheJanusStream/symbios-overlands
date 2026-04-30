//! OAuth 2.0 + DPoP authentication for Symbios Overlands.
//!
//! This module drives the full authorization-code flow against the user's
//! PDS, producing an authenticated `AtprotoSession` that
//! [`bevy_symbios_multiuser`] can use to fetch a service token for the
//! relay. It is target-specific:
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
//!   `NATIVE_CALLBACK_PORT`, `CLIENT_METADATA_URL`,
//!   `SESSION_STORAGE_KEY`, `PERSISTED_SESSION_KEY`).
//! * [`refresh`] — DPoP-nonce retry (`oauth_*_with_nonce_retry`),
//!   refresh-on-expiry retry (`oauth_*_with_refresh`), and the shared
//!   `refresh_session` helper.
//! * [`auth_flow`] — `begin_authorization` and `complete_authorization`
//!   plus the `CompletedAuth` bundle.
//! * [`wasm`] — browser-side `sessionStorage` / `localStorage` plumbing
//!   (compiled only on `wasm32`).
//! * [`native_server`] — the `tiny_http` loopback callback listener
//!   (compiled only on native).

mod auth_flow;
mod discovery;
#[cfg(not(target_arch = "wasm32"))]
mod native_server;
mod refresh;
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
pub use discovery::{client_metadata, discover_auth_server};
#[cfg(not(target_arch = "wasm32"))]
pub use native_server::{
    NativeCallbackReceiver, parse_callback_query, start_native_callback_server,
};
pub use refresh::{
    fetch_session_identity, oauth_get_with_nonce_retry, oauth_get_with_refresh,
    oauth_post_with_nonce_retry, oauth_post_with_refresh, refresh_session,
};

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
#[cfg(target_arch = "wasm32")]
pub const SESSION_STORAGE_KEY: &str = "symbios_overlands_pending_auth";

/// `localStorage` key holding the serialized [`wasm::PersistedSession`]
/// across page reloads. Cleared on logout and on a refresh failure.
#[cfg(target_arch = "wasm32")]
pub const PERSISTED_SESSION_KEY: &str = "symbios_overlands_session";

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
