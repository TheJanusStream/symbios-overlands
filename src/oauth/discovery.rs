//! OAuth client metadata + authorization-server discovery: the URLs and
//! constants the rest of the OAuth flow keys off, plus the
//! `.well-known/oauth-protected-resource` lookup that the Bluesky entryway
//! quietly fails (which we tolerate by treating the input PDS URL as the
//! authorization server directly).

use proto_blue_oauth::OAuthClientMetadata;
use serde::Deserialize;

/// Marker substring used by `proto_blue_oauth::OAuthSession::request` when it
/// detects a `401 + WWW-Authenticate: error="invalid_token"` response. The
/// library returns this as `OAuthError::RefreshFailed("Access token is
/// invalid, refresh required")` (despite the variant name, no refresh has been
/// attempted — the caller is expected to do it). We pattern-match on the
/// string here because the helpers below convert errors to `String` before
/// they reach us.
pub(super) const INVALID_TOKEN_ERR: &str = "Access token is invalid, refresh required";

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
/// - **WASM (hosted)** — `client_id` is the public `CLIENT_METADATA_URL`
///   that the authorization server fetches to read the registered redirect
///   URIs, scopes, and token-endpoint auth method.
/// - **Native (loopback)** — per the atproto OAuth spec's *loopback client*
///   exception, development builds that redirect to `127.0.0.1` cannot use
///   a hosted metadata document. Instead the `client_id` is a virtual URL
///   `http://localhost?redirect_uri=…&scope=…` and the AS derives the
///   metadata directly from those query parameters.
pub fn client_metadata() -> OAuthClientMetadata {
    // `transition:generic` is the legacy scope that grants "ability to
    // generate service auth tokens for the specific API endpoints the
    // client has access to" — the documented path for clients (like us)
    // that need `com.atproto.server.getServiceAuth` against a relay. The
    // breadth (App-Password-equivalent: full repo + blob + rpc) matches
    // what bsky.social's AS advertises today; granular Permission-Sets
    // scopes are silently dropped at grant time, so requesting them
    // would yield a token whose granted scope is just `"atproto"` and
    // the PDS would 403 service-auth calls with `ScopeMissingError`.
    let scope = "atproto transition:generic";
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
