//! Integration tests for [`symbios_overlands::oauth`].
//!
//! Covers the pieces of the atproto OAuth 2.0 + DPoP client that don't
//! require network I/O: the `OAuthClientMetadata` the authorization server
//! fetches, and the native-loopback callback parser.

use symbios_overlands::oauth;

#[test]
fn client_metadata_redirect_matches_target() {
    let meta = oauth::client_metadata();
    assert_eq!(meta.redirect_uris.len(), 1);
    #[cfg(target_arch = "wasm32")]
    {
        assert_eq!(meta.client_id, oauth::CLIENT_METADATA_URL);
        assert_eq!(meta.redirect_uris[0], oauth::WASM_REDIRECT_URI);
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
    let (c, s) = oauth::parse_callback_query("/callback?code=abc123&state=xyz");
    assert_eq!(c.as_deref(), Some("abc123"));
    assert_eq!(s.as_deref(), Some("xyz"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn parse_callback_query_percent_encoded() {
    let (c, _) = oauth::parse_callback_query("/callback?code=a%2Bb&state=s");
    assert_eq!(c.as_deref(), Some("a+b"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn parse_callback_query_missing() {
    let (c, s) = oauth::parse_callback_query("/callback");
    assert!(c.is_none());
    assert!(s.is_none());
}

// ---------------------------------------------------------------------------
// Extended coverage — not in the original inline tests but valuable.
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn parse_callback_query_empty_values() {
    // Server bug or truncated redirect — we must not panic; both values
    // parse as empty strings rather than `None`, because the key *was*
    // present. Callers decide how to handle empty codes.
    let (c, s) = oauth::parse_callback_query("/callback?code=&state=");
    assert_eq!(c.as_deref(), Some(""));
    assert_eq!(s.as_deref(), Some(""));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn parse_callback_query_ignores_unknown_params() {
    // atproto OAuth callbacks can carry extra fields (`iss`, session-id
    // cookies, etc.). We need to quietly skip anything that isn't code/state.
    let (c, s) = oauth::parse_callback_query(
        "/callback?iss=https%3A%2F%2Fpds.example&code=abc&state=xyz&extra=1",
    );
    assert_eq!(c.as_deref(), Some("abc"));
    assert_eq!(s.as_deref(), Some("xyz"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn parse_callback_query_plus_in_percent_encoding() {
    // `%20` should become a space. Regression guard — native callback
    // decoder has to respect the full percent-encoding alphabet.
    let (c, _) = oauth::parse_callback_query("/callback?code=hello%20world&state=s");
    assert_eq!(c.as_deref(), Some("hello world"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn parse_callback_query_malformed_pair_no_panic() {
    // Barewords (`?foo` with no `=`) must not panic the loopback server.
    let (c, s) = oauth::parse_callback_query("/callback?code&state=only");
    assert_eq!(c.as_deref(), Some(""));
    assert_eq!(s.as_deref(), Some("only"));
}

#[test]
fn native_callback_port_in_expected_range() {
    // Loopback ports must not collide with the OS ephemeral-port range
    // or with well-known services. The current pick (3456) is safely
    // inside the user-allocatable registered-port band.
    const _: () = assert!(oauth::NATIVE_CALLBACK_PORT >= 1024);
    const _: () = assert!(oauth::NATIVE_CALLBACK_PORT < 49_152);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_redirect_uri_embeds_callback_port() {
    let uri = oauth::native_redirect_uri();
    assert!(uri.starts_with("http://127.0.0.1:"));
    assert!(uri.contains(&oauth::NATIVE_CALLBACK_PORT.to_string()));
    assert!(uri.ends_with("/callback"));
}

#[cfg(target_arch = "wasm32")]
#[test]
fn session_storage_key_is_nonempty_and_namespaced() {
    // Session storage is a flat global map on WASM — keys need a
    // project-specific prefix so we don't collide with unrelated apps
    // hosted on the same origin. Only compiled for wasm targets, where
    // the constant actually exists.
    assert!(!oauth::SESSION_STORAGE_KEY.is_empty());
    assert!(oauth::SESSION_STORAGE_KEY.contains("symbios"));
}
