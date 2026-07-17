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

#[test]
fn client_metadata_scope_is_granular() {
    // #736: the broad `transition:generic` grant is replaced by granular
    // permissions — write access to exactly the five Overlands collections
    // plus a concrete-lxm/wildcard-aud rpc grant for relay service auth.
    // The rpc shape matters: `rpc:*?aud=*` is spec-invalid, and pinning
    // `aud` instead of `lxm` would bake one relay's DID into the static
    // hosted metadata (the #170 hack this replaces).
    let scope = oauth::client_metadata().scope.expect("scope must be set");
    assert!(scope.starts_with("atproto "), "{scope}");
    assert!(!scope.contains("transition:generic"), "{scope}");
    for collection in [
        "network.symbios.overlands.room",
        "network.symbios.overlands.room.generator",
        "network.symbios.overlands.avatar",
        "network.symbios.overlands.inventory",
        "network.symbios.overlands.inventory.item",
    ] {
        assert!(
            scope.split(' ').any(|s| s == format!("repo:{collection}")),
            "missing repo grant for {collection} in {scope}"
        );
    }
    let rpc = format!("rpc:{}?aud=*", oauth::RELAY_SERVICE_LXM);
    assert!(scope.split(' ').any(|s| s == rpc), "{scope}");
}

#[test]
fn client_metadata_scope_matches_hosted_document() {
    // The WASM build's `client_id` is the hosted metadata URL, so the
    // authorization server reads the scope from
    // `assets/client-metadata.json` — if that file drifts from the scope
    // the code sends in the PAR request, login breaks only in production.
    let hosted: serde_json::Value =
        serde_json::from_str(include_str!("../assets/client-metadata.json"))
            .expect("client-metadata.json must parse");
    assert_eq!(
        hosted["scope"].as_str().expect("hosted scope must be set"),
        oauth::client_metadata().scope.expect("scope must be set"),
        "assets/client-metadata.json scope drifted from oauth::granular_scope()"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn parse_callback_query_typical() {
    let p = oauth::parse_callback_query("/callback?code=abc123&state=xyz");
    assert_eq!(p.code.as_deref(), Some("abc123"));
    assert_eq!(p.state.as_deref(), Some("xyz"));
    assert!(p.error.is_none());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn parse_callback_query_percent_encoded() {
    let p = oauth::parse_callback_query("/callback?code=a%2Bb&state=s");
    assert_eq!(p.code.as_deref(), Some("a+b"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn parse_callback_query_missing() {
    let p = oauth::parse_callback_query("/callback");
    assert!(p.code.is_none());
    assert!(p.state.is_none());
    assert!(p.error.is_none());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn parse_callback_query_error_redirect() {
    // #847: the AS's deny redirect (RFC 6749 §4.1.2.1) must parse —
    // before this, `error` was dropped and a deny left the native
    // listener waiting forever.
    let p = oauth::parse_callback_query(
        "/callback?error=access_denied&error_description=User%20denied%20access&state=xyz",
    );
    assert!(p.code.is_none());
    assert_eq!(p.error.as_deref(), Some("access_denied"));
    assert_eq!(p.error_description.as_deref(), Some("User denied access"));
    assert_eq!(p.state.as_deref(), Some("xyz"));
}

#[test]
fn callback_error_message_wording() {
    // The everyday deny gets plain language, not an error code…
    let denied = oauth::CallbackParams {
        error: Some("access_denied".into()),
        ..Default::default()
    };
    let msg = denied.error_message().expect("deny must produce a message");
    assert!(msg.contains("cancelled"), "{msg}");
    assert!(!msg.contains("access_denied"), "{msg}");

    // …other errors surface code + description verbatim…
    let other = oauth::CallbackParams {
        error: Some("invalid_request".into()),
        error_description: Some("request expired".into()),
        ..Default::default()
    };
    let msg = other.error_message().expect("error must produce a message");
    assert!(
        msg.contains("invalid_request") && msg.contains("request expired"),
        "{msg}"
    );

    // …and a success callback produces none.
    let ok = oauth::CallbackParams {
        code: Some("abc".into()),
        state: Some("xyz".into()),
        ..Default::default()
    };
    assert!(ok.error_message().is_none());
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
    let p = oauth::parse_callback_query("/callback?code=&state=");
    assert_eq!(p.code.as_deref(), Some(""));
    assert_eq!(p.state.as_deref(), Some(""));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn parse_callback_query_ignores_unknown_params() {
    // atproto OAuth callbacks can carry extra fields (`iss`, session-id
    // cookies, etc.). We need to quietly skip anything that isn't a
    // recognised callback parameter.
    let p = oauth::parse_callback_query(
        "/callback?iss=https%3A%2F%2Fpds.example&code=abc&state=xyz&extra=1",
    );
    assert_eq!(p.code.as_deref(), Some("abc"));
    assert_eq!(p.state.as_deref(), Some("xyz"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn parse_callback_query_plus_in_percent_encoding() {
    // `%20` should become a space. Regression guard — native callback
    // decoder has to respect the full percent-encoding alphabet.
    let p = oauth::parse_callback_query("/callback?code=hello%20world&state=s");
    assert_eq!(p.code.as_deref(), Some("hello world"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn parse_callback_query_malformed_pair_no_panic() {
    // Barewords (`?foo` with no `=`) must not panic the loopback server.
    let p = oauth::parse_callback_query("/callback?code&state=only");
    assert_eq!(p.code.as_deref(), Some(""));
    assert_eq!(p.state.as_deref(), Some("only"));
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
