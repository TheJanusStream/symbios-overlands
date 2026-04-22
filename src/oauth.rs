//! OAuth 2.0 + DPoP authentication for Symbios Overlands.
//!
//! This module drives the full authorization-code flow against the user's
//! PDS, producing an authenticated `AtprotoSession` that [`bevy_symbios_multiuser`]
//! can use to fetch a service token for the relay. It is target-specific:
//!
//! - **WASM** — redirect-based: the page navigates away to the auth server,
//!   state is parked in `sessionStorage`, and the callback lands back on the
//!   hosted page with `?code=&state=` query parameters.
//! - **Native** — loopback-based: a background `tiny_http` server on
//!   `127.0.0.1:3456` catches the redirect, and `webbrowser::open` launches
//!   the user's default browser to the auth URL.
//!
//! Identity discovery is intentionally minimal. Rather than pull in a full
//! DID/handle resolver (and a DNS stack that does not build on `wasm32`),
//! the login UI asks the user for their PDS URL directly and we discover the
//! authorization server via `{pds}/.well-known/oauth-protected-resource`
//! (atproto OAuth §3.2) — which *is* a sufficient CORS-reachable discovery
//! path on every target.

use std::sync::Arc;

use bevy::prelude::*;
use proto_blue_oauth::{
    AuthState, OAuthClient, OAuthClientMetadata, OAuthServerMetadata, OAuthSession,
    client::dpop_key_from_jwk, types::TokenSet,
};
use serde::{Deserialize, Serialize};

/// Hosted `client-metadata.json` URL. Per the atproto OAuth profile this URL
/// *is* the `client_id`: the authorization server fetches it to learn the
/// registered redirect URIs, scopes, and token-endpoint auth method. Used
/// only on WASM — native builds use the loopback `client_id` pattern and
/// require no hosted document.
#[cfg(target_arch = "wasm32")]
pub const CLIENT_METADATA_URL: &str =
    "https://thejanusstream.github.io/symbios-overlands/client-metadata.json";

/// The production redirect URI served by GitHub Pages alongside the hosted
/// client-metadata document. Must match the first entry in
/// `assets/client-metadata.json#redirect_uris`.
#[cfg(target_arch = "wasm32")]
pub const WASM_REDIRECT_URI: &str = "https://thejanusstream.github.io/symbios-overlands";

/// Loopback port the native build listens on for the OAuth callback.
/// Native uses the loopback-client `client_id` pattern (see
/// [`client_metadata`]) and does not consume the hosted metadata document,
/// so this URI is not listed in `assets/client-metadata.json`.
#[cfg(not(target_arch = "wasm32"))]
pub const NATIVE_CALLBACK_PORT: u16 = 3456;

/// The loopback redirect URI registered for the native build.
#[cfg(not(target_arch = "wasm32"))]
pub fn native_redirect_uri() -> String {
    format!("http://127.0.0.1:{NATIVE_CALLBACK_PORT}/callback")
}

/// Build the client metadata we hand to proto-blue-oauth.
///
/// Two different `client_id` strategies are used depending on target:
///
/// - **WASM (hosted)** — `client_id` is a public URL
///   ([`CLIENT_METADATA_URL`]) that the authorization server fetches to
///   read the registered redirect URIs, scopes, and token-endpoint auth
///   method. Enabled by Bluesky's `client_id_metadata_document_supported`.
/// - **Native (loopback)** — per the atproto OAuth spec's *loopback client*
///   exception, development builds that redirect to `127.0.0.1` cannot use
///   a hosted metadata document (hosted clients are not allowed to redirect
///   to loopback). Instead the `client_id` is a virtual URL
///   `http://localhost?redirect_uri=…&scope=…` and the AS derives the
///   metadata directly from those query parameters. This matches what the
///   `bsky.social` AS expects for native callback flows; without it the
///   PAR endpoint rejects the request with HTTP 400.
pub fn client_metadata() -> OAuthClientMetadata {
    let scope = "atproto transition:generic network.symbios.overlands.room network.symbios.overlands.avatar";
    #[cfg(target_arch = "wasm32")]
    {
        OAuthClientMetadata {
            client_id: CLIENT_METADATA_URL.into(),
            redirect_uris: vec![WASM_REDIRECT_URI.into()],
            response_types: Some(vec!["code".into()]),
            grant_types: Some(vec!["authorization_code".into(), "refresh_token".into()]),
            scope: Some(scope.into()),
            token_endpoint_auth_method: Some("none".into()),
            token_endpoint_auth_signing_alg: None,
            application_type: Some("web".into()),
            dpop_bound_access_tokens: Some(true),
            client_name: Some("Symbios Overlands".into()),
            client_uri: Some("https://thejanusstream.github.io/symbios-overlands".into()),
            logo_uri: None,
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let redirect = native_redirect_uri();
        let client_id = format!(
            "http://localhost?redirect_uri={}&scope={}",
            urlencode_query_value(&redirect),
            urlencode_query_value(scope),
        );
        OAuthClientMetadata {
            client_id,
            redirect_uris: vec![redirect],
            response_types: Some(vec!["code".into()]),
            grant_types: Some(vec!["authorization_code".into(), "refresh_token".into()]),
            scope: Some(scope.into()),
            token_endpoint_auth_method: Some("none".into()),
            token_endpoint_auth_signing_alg: None,
            application_type: Some("native".into()),
            dpop_bound_access_tokens: Some(true),
            client_name: Some("Symbios Overlands (dev)".into()),
            client_uri: None,
            logo_uri: None,
        }
    }
}

/// Minimal RFC 3986 query-value percent-encoder. Preserves the unreserved
/// set (`A-Z a-z 0-9 - _ . ~`) and percent-escapes everything else.
/// Deliberately *not* a full URL-encoder — used only for building the
/// loopback-client `client_id` query string on native.
#[cfg(not(target_arch = "wasm32"))]
fn urlencode_query_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// In-flight OAuth authorization state persisted between the `authorize()`
/// call and the callback. On WASM this is serialized into
/// `sessionStorage`; on native it lives in a Mutex inside
/// [`PendingAuthRes`].
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
}

/// `sessionStorage` key holding the serialized [`PendingAuth`] between the
/// authorize-redirect and the callback parse on WASM.
#[cfg(target_arch = "wasm32")]
pub const SESSION_STORAGE_KEY: &str = "symbios_overlands_pending_auth";

/// Minimum well-known discovery response for an atproto OAuth PDS. See the
/// atproto OAuth spec §3.2 (Protected Resource Metadata).
#[derive(Deserialize)]
struct ProtectedResourceMetadata {
    authorization_servers: Vec<String>,
}

/// Discover the authorization server URL for whatever the user typed in
/// the "PDS" field.
///
/// Two-step lookup:
/// 1. Try `{input}/.well-known/oauth-protected-resource` — the canonical
///    resource-server metadata published by a real PDS (atproto OAuth
///    spec §3.2). On success, return the first entry in
///    `authorization_servers`.
/// 2. If step 1 404s (or decodes empty), fall back to treating the input
///    URL as the authorization server directly — which matches the real
///    topology of the Bluesky entryway (`bsky.social` is an AS, not a
///    resource server; individual PDS shards like
///    `porcini.us-east.host.bsky.network` are the actual resource servers).
///    The caller's next step (`OAuthClient::discover_server`) will then
///    probe `.well-known/oauth-authorization-server` on that URL and
///    either succeed or surface a clear error.
pub async fn discover_auth_server(http: &reqwest::Client, pds_url: &str) -> Result<String, String> {
    let base = pds_url.trim_end_matches('/');
    let url = format!("{base}/.well-known/oauth-protected-resource");
    let resp = http
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("fetch {url}: {e}"))?;
    let status = resp.status();
    if status.as_u16() == 404 {
        // Not a resource server — assume the input URL is itself the
        // authorization server (e.g. Bluesky's `bsky.social` entryway).
        return Ok(base.to_string());
    }
    if !status.is_success() {
        return Err(format!("{url}: HTTP {status}"));
    }
    let meta: ProtectedResourceMetadata = resp
        .json()
        .await
        .map_err(|e| format!("{url}: decode: {e}"))?;
    match meta.authorization_servers.into_iter().next() {
        Some(server) => Ok(server),
        // An empty list is indistinguishable from "I'm an AS, not an RS"
        // for our purposes — fall back to the input URL for the same
        // reason as the 404 branch.
        None => Ok(base.to_string()),
    }
}

/// Authenticated GET with an automatic DPoP-nonce retry.
///
/// The atproto PDS requires every DPoP proof to include a server-chosen
/// nonce (RFC 9449 §8). On the *first* request to a new origin the client
/// doesn't have one, so the server replies `401 use_dpop_nonce` with the
/// required nonce in a `DPoP-Nonce` response header. proto-blue-oauth's
/// `OAuthSession::request` already caches that header on every response
/// (see `session.rs` lines 157-164) but does **not** retry, so a single
/// replay of the same request with the same cached cache succeeds.
///
/// Returns `(status, body_text)` — on a `use_dpop_nonce` 401 the initial
/// response is discarded and only the retry's status/body are returned.
pub async fn oauth_get_with_nonce_retry(
    oauth_session: &OAuthSession,
    url: &str,
) -> Result<(reqwest::StatusCode, String), String> {
    let resp = oauth_session.get(url).await.map_err(|e| e.to_string())?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        let body = resp.text().await.unwrap_or_default();
        if body.contains("use_dpop_nonce") {
            let retry = oauth_session.get(url).await.map_err(|e| e.to_string())?;
            let retry_status = retry.status();
            let retry_body = retry.text().await.unwrap_or_default();
            return Ok((retry_status, retry_body));
        }
        return Ok((status, body));
    }
    let body = resp.text().await.unwrap_or_default();
    Ok((status, body))
}

/// Authenticated POST with an automatic DPoP-nonce retry. See
/// [`oauth_get_with_nonce_retry`] for why the retry dance is required.
pub async fn oauth_post_with_nonce_retry(
    oauth_session: &OAuthSession,
    url: &str,
    body_json: &serde_json::Value,
) -> Result<(reqwest::StatusCode, String), String> {
    let resp = oauth_session
        .post(url, body_json)
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        let body = resp.text().await.unwrap_or_default();
        if body.contains("use_dpop_nonce") {
            let retry = oauth_session
                .post(url, body_json)
                .await
                .map_err(|e| e.to_string())?;
            let retry_status = retry.status();
            let retry_body = retry.text().await.unwrap_or_default();
            return Ok((retry_status, retry_body));
        }
        return Ok((status, body));
    }
    let body = resp.text().await.unwrap_or_default();
    Ok((status, body))
}

/// Response shape from `com.atproto.server.getSession` — used after the
/// OAuth exchange to look up the handle that matches the DID in the token.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetSessionResponse {
    did: String,
    handle: String,
}

/// Fetch the user's handle and confirm the DID matches the OAuth session.
/// This is required because the authorization response only carries the DID;
/// the handle comes from the PDS session endpoint.
pub async fn fetch_session_identity(
    oauth_session: &OAuthSession,
    pds_url: &str,
) -> Result<(String, String), String> {
    let url = format!(
        "{}/xrpc/com.atproto.server.getSession",
        pds_url.trim_end_matches('/')
    );
    let (status, body) = oauth_get_with_nonce_retry(oauth_session, &url)
        .await
        .map_err(|e| format!("getSession: {e}"))?;
    if !status.is_success() {
        return Err(format!("getSession {status}: {body}"));
    }
    let parsed: GetSessionResponse =
        serde_json::from_str(&body).map_err(|e| format!("getSession decode: {e}"))?;
    Ok((parsed.did, parsed.handle))
}

/// Run the full "start the OAuth dance" flow up to the point where the
/// browser must be redirected. Returns the authorization URL *and* the
/// pending-auth blob the callback handler will need.
pub async fn begin_authorization(
    oauth_client: &OAuthClient,
    pds_url: &str,
    relay_host: &str,
    target_did: &str,
) -> Result<(String, PendingAuth), String> {
    let http = reqwest::Client::new();
    let auth_server = discover_auth_server(&http, pds_url).await?;
    let server_metadata = oauth_client
        .discover_server(&auth_server)
        .await
        .map_err(|e| format!("discover_server: {e}"))?;
    let (auth_url, auth_state) = oauth_client
        .authorize(&server_metadata)
        .await
        .map_err(|e| format!("authorize: {e}"))?;
    Ok((
        auth_url.to_string(),
        PendingAuth {
            auth_state,
            server_metadata,
            pds_url: pds_url.to_string(),
            relay_host: relay_host.to_string(),
            target_did: target_did.to_string(),
        },
    ))
}

/// Exchange an authorization `code` for an [`OAuthSession`], resolve the
/// user's actual PDS shard from their DID, and fetch the session identity
/// against that PDS. Returns `(session, did, handle, pds_url)` where
/// `pds_url` is the resolved shard (e.g.
/// `https://porcini.us-east.host.bsky.network`) — **not** the entryway the
/// user typed into the login form.
///
/// The entryway (`bsky.social`) is an *authorization server* only: OAuth
/// tokens it issues are meant for PDS access and calling
/// `com.atproto.server.getSession` on the entryway itself returns
/// `InvalidToken: "OAuth tokens are meant for PDS access only"`. The
/// resolved PDS URL is what every subsequent authenticated XRPC call
/// (`getSession`, `putRecord`, `deleteRecord`, …) must target.
pub async fn complete_authorization(
    oauth_client: &OAuthClient,
    pending: &PendingAuth,
    code: &str,
) -> Result<(Arc<OAuthSession>, String, String, String), String> {
    let token_set: TokenSet = oauth_client
        .callback(code, &pending.auth_state, &pending.server_metadata)
        .await
        .map_err(|e| format!("callback: {e}"))?;
    let did = token_set.sub.clone();
    if did.is_empty() {
        return Err("callback: token response missing `sub` (DID)".to_string());
    }
    let dpop_key = dpop_key_from_jwk(&pending.auth_state.dpop_key)
        .map_err(|e| format!("dpop_key_from_jwk: {e}"))?;
    let oauth_session = Arc::new(OAuthSession::new(
        token_set,
        dpop_key,
        oauth_client.dpop_nonces().clone(),
    ));

    let http = reqwest::Client::new();
    let pds_url = crate::pds::resolve_pds(&http, &did)
        .await
        .ok_or_else(|| format!("resolve_pds: could not resolve DID document for {did}"))?;

    let (session_did, handle) = fetch_session_identity(&oauth_session, &pds_url).await?;
    if session_did != did {
        return Err(format!(
            "getSession DID mismatch: token sub={did} but PDS returned {session_did}"
        ));
    }
    Ok((oauth_session, did, handle, pds_url))
}

// ──────────────────────────────────────────────────────────────────────────
// WASM: sessionStorage, URL parsing, and browser navigation
// ──────────────────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
pub mod wasm {
    //! Browser-side helpers for reading/writing [`super::PendingAuth`] into
    //! `sessionStorage`, inspecting the callback query string, and
    //! scrubbing the URL bar after a successful exchange so reloads don't
    //! re-trigger the `code` exchange (which would 400 — the authorization
    //! code is single-use).

    use super::{PendingAuth, SESSION_STORAGE_KEY};

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
        let scrubbed = format!("{}/", super::WASM_REDIRECT_URI);
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
}

// ──────────────────────────────────────────────────────────────────────────
// Native: loopback HTTP callback server
// ──────────────────────────────────────────────────────────────────────────

/// Channel carrying the `code` / `state` pair from the loopback callback
/// server back to the Bevy polling system.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource)]
pub struct NativeCallbackReceiver(
    pub std::sync::Mutex<std::sync::mpsc::Receiver<(String, String)>>,
);

/// Start a one-shot `tiny_http` server on the configured port, parse the
/// first `/callback?code=&state=` request it receives, send the pair
/// through a channel, and shut down. Runs on a detached thread so the Bevy
/// frame loop is not blocked.
///
/// Returns the receive side of the channel — the caller is expected to
/// insert it as [`NativeCallbackReceiver`] and poll it in a Bevy system.
#[cfg(not(target_arch = "wasm32"))]
pub fn start_native_callback_server() -> Result<std::sync::mpsc::Receiver<(String, String)>, String>
{
    use std::thread;

    let (tx, rx) = std::sync::mpsc::channel();
    let addr = format!("127.0.0.1:{NATIVE_CALLBACK_PORT}");
    let server = tiny_http::Server::http(&addr).map_err(|e| format!("bind {addr}: {e}"))?;

    thread::spawn(move || {
        // Accept a single request — authorization codes are single-use so
        // additional callbacks would be spam (a reload of the callback
        // page) at best and a replay attempt at worst.
        for req in server.incoming_requests() {
            let url = req.url().to_string();
            let (code, state) = parse_callback_query(&url);
            let html = if code.is_some() && state.is_some() {
                "<!doctype html><html><body><h2>Login successful.</h2>\
                 <p>You can close this tab and return to Symbios Overlands.</p></body></html>"
            } else {
                "<!doctype html><html><body><h2>Login failed.</h2>\
                 <p>The callback did not contain a valid <code>code</code> and \
                 <code>state</code>. Check the terminal for details.</p></body></html>"
            };
            let response = tiny_http::Response::from_string(html).with_header(
                "Content-Type: text/html; charset=utf-8"
                    .parse::<tiny_http::Header>()
                    .unwrap(),
            );
            let _ = req.respond(response);
            if let (Some(code), Some(state)) = (code, state) {
                let _ = tx.send((code, state));
                break;
            }
        }
    });

    Ok(rx)
}

/// Parse `code` and `state` out of a URL like `/callback?code=abc&state=xyz`.
#[cfg(not(target_arch = "wasm32"))]
fn parse_callback_query(url: &str) -> (Option<String>, Option<String>) {
    let Some(q_start) = url.find('?') else {
        return (None, None);
    };
    let query = &url[q_start + 1..];
    let mut code = None;
    let mut state = None;
    for pair in query.split('&') {
        let mut it = pair.splitn(2, '=');
        let k = it.next().unwrap_or("");
        let v = it.next().unwrap_or("");
        let decoded = native_percent_decode(v);
        match k {
            "code" => code = Some(decoded),
            "state" => state = Some(decoded),
            _ => {}
        }
    }
    (code, state)
}

#[cfg(not(target_arch = "wasm32"))]
fn native_percent_decode(s: &str) -> String {
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
                let h = native_hex(bytes[i + 1]);
                let l = native_hex(bytes[i + 2]);
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

#[cfg(not(target_arch = "wasm32"))]
fn native_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Shared Bevy resources
// ──────────────────────────────────────────────────────────────────────────

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_metadata_redirect_matches_target() {
        let meta = client_metadata();
        assert_eq!(meta.redirect_uris.len(), 1);
        #[cfg(target_arch = "wasm32")]
        {
            assert_eq!(meta.client_id, CLIENT_METADATA_URL);
            assert_eq!(meta.redirect_uris[0], WASM_REDIRECT_URI);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Loopback client_id pattern: http://localhost?redirect_uri=…&scope=…
            assert!(meta.client_id.starts_with("http://localhost?"));
            assert!(meta.client_id.contains("redirect_uri="));
            assert!(meta.client_id.contains("scope="));
            assert!(meta.redirect_uris[0].starts_with("http://127.0.0.1:"));
            assert_eq!(meta.application_type.as_deref(), Some("native"));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn parse_callback_query_typical() {
        let (c, s) = parse_callback_query("/callback?code=abc123&state=xyz");
        assert_eq!(c.as_deref(), Some("abc123"));
        assert_eq!(s.as_deref(), Some("xyz"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn parse_callback_query_percent_encoded() {
        let (c, _) = parse_callback_query("/callback?code=a%2Bb&state=s");
        assert_eq!(c.as_deref(), Some("a+b"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn parse_callback_query_missing() {
        let (c, s) = parse_callback_query("/callback");
        assert!(c.is_none());
        assert!(s.is_none());
    }
}
