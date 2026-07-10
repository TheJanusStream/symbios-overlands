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

/// Lexicon method (`lxm`) the relay service-auth token is minted for.
///
/// Not a published lexicon — the relay never dispatches on it (its JWT
/// validation reads `iss`/`exp`/`nbf`/`aud` only). It exists so the
/// `rpc:` scope below can name a *concrete* method with a wildcard
/// audience: the permissions spec forbids `rpc:*?aud=*`, and pinning the
/// audience instead would bake one relay's DID into the static hosted
/// client metadata (the #170 per-session-client hack), breaking
/// non-default relays on WASM. Every `getServiceAuth` call must pass
/// this as `lxm` — the PDS treats an omitted `lxm` as `*`, which the
/// wildcard-audience grant does not cover.
pub const RELAY_SERVICE_LXM: &str = "network.symbios.overlands.signal";

/// The granular OAuth scope requested at login (#736, supersedes
/// `transition:generic`): write access to exactly the five Overlands
/// record collections, plus the ability to mint relay service-auth
/// tokens ([`RELAY_SERVICE_LXM`]) for any audience. Everything else the
/// app touches is unauthenticated (public AppView reads, `sync.getBlob`)
/// or covered by the base `atproto` scope (`getSession`). Repo *reads*
/// need no scope — the permission model's `action` vocabulary is
/// create/update/delete only.
///
/// Kept as a runtime builder (not a literal) so the collection NSIDs
/// can't drift from the constants the write paths actually use. The
/// hosted copy in `assets/client-metadata.json` must stay in sync — see
/// the `client_metadata_scope_matches_hosted_document` integration test.
pub fn granular_scope() -> String {
    use crate::pds::{
        AVATAR_COLLECTION, COLLECTION, INVENTORY_COLLECTION, INVENTORY_ITEM_COLLECTION,
        ROOM_GENERATOR_COLLECTION,
    };
    format!(
        "atproto repo:{COLLECTION} repo:{ROOM_GENERATOR_COLLECTION} repo:{AVATAR_COLLECTION} \
         repo:{INVENTORY_COLLECTION} repo:{INVENTORY_ITEM_COLLECTION} \
         rpc:{RELAY_SERVICE_LXM}?aud=*"
    )
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
    let scope = granular_scope();
    #[cfg(target_arch = "wasm32")]
    {
        OAuthClientMetadata {
            client_id: CLIENT_METADATA_URL.into(),
            redirect_uris: vec![WASM_REDIRECT_URI.into()],
            response_types: Some(vec!["code".into()]),
            grant_types: Some(vec!["authorization_code".into(), "refresh_token".into()]),
            scope: Some(scope),
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
            urlencode_query_value(&scope),
        );
        OAuthClientMetadata {
            client_id,
            redirect_uris: vec![redirect],
            response_types: Some(vec!["code".into()]),
            grant_types: Some(vec!["authorization_code".into(), "refresh_token".into()]),
            scope: Some(scope),
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
