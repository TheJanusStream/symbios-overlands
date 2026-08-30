//! Authenticated GET / POST helpers with two layered retry dances:
//!
//! 1. **DPoP nonce retry** — atproto PDS requires a server-chosen nonce on
//!    every DPoP proof (RFC 9449 §8). The first request to a new origin
//!    has none, so the server replies `401 use_dpop_nonce` with the nonce
//!    in a `DPoP-Nonce` response header. proto-blue-oauth caches that
//!    header automatically but doesn't retry, so we replay once.
//!
//! 2. **Refresh-on-expiry retry** — wraps the nonce-retry helpers with
//!    proactive expiry checks (`session.is_expired_jittered()`) and a
//!    reactive refresh on `invalid_token`. Every authenticated PDS write
//!    routes through these so a long-idle session self-heals against the
//!    ~30 min – 2 h access-token lifetime instead of failing the user's
//!    click.
//!
//! The nonce-retry helpers return [`OAuthError`] rather than a `String`
//! because the layer above them has to *branch* on one variant of it
//! (see [`oauth_post_with_refresh`]). Stringifying at this boundary is
//! what forced the old `e.contains("Access token is invalid, …")` match:
//! a literal copied out of proto-blue-oauth that nothing would have
//! rechecked if upstream reworded it. The `String` conversion now happens
//! once, at the top of each dance, where nobody matches on it again.

use proto_blue_oauth::{OAuthError, OAuthSession};
use serde::Deserialize;

use super::OauthRefreshCtx;

/// Authenticated GET with an automatic DPoP-nonce retry.
///
/// Returns `(status, body_text)` — on a `use_dpop_nonce` 401 the initial
/// response is discarded and only the retry's status/body are returned.
///
/// There is deliberately no `oauth_get_with_refresh` sibling to
/// [`oauth_post_with_refresh`]. This helper has exactly one caller,
/// [`fetch_session_identity`], which runs immediately after the token
/// exchange — the access token it uses is seconds old, so there is
/// nothing for a refresh wrapper to heal. An unused symmetric helper
/// would be a claim of coverage that no call site relies on.
pub async fn oauth_get_with_nonce_retry(
    oauth_session: &OAuthSession,
    url: &str,
) -> Result<(reqwest::StatusCode, String), OAuthError> {
    let resp = oauth_session.get(url).await?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        let body = resp.text().await.unwrap_or_default();
        if body.contains("use_dpop_nonce") {
            let retry = oauth_session.get(url).await?;
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
) -> Result<(reqwest::StatusCode, String), OAuthError> {
    let resp = oauth_session.post(url, body_json).await?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        let body = resp.text().await.unwrap_or_default();
        if body.contains("use_dpop_nonce") {
            let retry = oauth_session.post(url, body_json).await?;
            let retry_status = retry.status();
            let retry_body = retry.text().await.unwrap_or_default();
            return Ok((retry_status, retry_body));
        }
        return Ok((status, body));
    }
    let body = resp.text().await.unwrap_or_default();
    Ok((status, body))
}

/// Refresh the OAuth access token and re-persist the rotated `TokenSet`
/// to the WASM session blob. On native this is a thin pass-through.
///
/// `proto_blue_oauth::OAuthSession::refresh` is internally mutex-serialised
/// so concurrent callers share one `/token` round-trip; we trust that
/// guarantee and don't re-serialise on top.
pub async fn refresh_session(
    session: &OAuthSession,
    refresh: &OauthRefreshCtx,
) -> Result<(), String> {
    session
        .refresh(&refresh.client, &refresh.server_metadata)
        .await
        .map_err(|e| format!("refresh: {e}"))?;
    #[cfg(target_arch = "wasm32")]
    {
        // Persist the rotated token set so a subsequent reload doesn't
        // come back with the now-stale access token from before refresh.
        // Any failure here is non-fatal — the session in memory is still
        // good for this run; we just won't survive a reload until the
        // next refresh.
        if let Err(e) = super::wasm::update_persisted_token_set(&session.token_set()) {
            bevy::prelude::warn!("update_persisted_token_set: {e}");
        }
    }
    Ok(())
}

/// Authenticated POST that proactively refreshes an expired access token
/// and reactively retries once on `invalid_token`.
///
/// Wraps [`oauth_post_with_nonce_retry`] with the refresh dance proto-blue
/// expects callers to perform: it does NOT auto-refresh, only signals the
/// need via `OAuthError::RefreshFailed`. Call this from every authenticated
/// PDS write so a session that has been idle past the access-token lifetime
/// (~30 min – 2 h on ATProto PDSes) self-heals instead of failing the user's
/// click.
///
/// # Why a bare `RefreshFailed` match is safe here
///
/// proto-blue-oauth 0.2.6 builds that variant in two places that mean
/// opposite things: `session.rs:239`, "the resource server says this
/// access token is stale, go refresh", and `client.rs:688`, "this token
/// set has no refresh token at all". Matching the second as the first
/// would turn an unrecoverable session into an infinite refresh loop.
/// They cannot collide here because the only error this match inspects
/// comes from `OAuthSession::post`, which never reaches
/// `OAuthClient::refresh_token`; the "no refresh token" case can only
/// surface from `session.refresh()`, which we call through
/// [`refresh_session`] and whose errors are already a distinct `String`
/// prefixed `refresh: `.
///
/// # What a bare 401 does
///
/// A 401 whose `WWW-Authenticate` header is missing (or names neither
/// the `DPoP` nor the `Bearer` scheme, or omits `error="invalid_token"`)
/// is not an error at all upstream — it comes back as `Ok(resp)`. That
/// is deliberate and we keep it: RFC 6750 §3 requires the challenge
/// header on a 401, so a bare one is a server saying something other
/// than "your token expired", and refreshing on every one of them would
/// spend a `/token` round-trip on each malformed request the PDS
/// rejects. The status reaches the caller, which reports it.
pub async fn oauth_post_with_refresh(
    session: &OAuthSession,
    refresh: &OauthRefreshCtx,
    url: &str,
    body_json: &serde_json::Value,
) -> Result<(reqwest::StatusCode, String), String> {
    if session.is_expired_jittered() {
        refresh_session(session, refresh).await?;
    }
    match oauth_post_with_nonce_retry(session, url, body_json).await {
        Ok(pair) => Ok(pair),
        Err(OAuthError::RefreshFailed(_)) => {
            refresh_session(session, refresh).await?;
            oauth_post_with_nonce_retry(session, url, body_json)
                .await
                .map_err(|e| e.to_string())
        }
        Err(e) => Err(e.to_string()),
    }
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

/// The refresh dance, driven through a scripted transport.
///
/// Every one of these tests asserts what the *network* saw — how many
/// times `/token` was called, which URL was hit first, what access token
/// the replayed request carried — rather than that a helper returned
/// `Ok`. The bug this module exists to catch is a branch that stops
/// firing: a helper that quietly gives up on the retry still returns a
/// perfectly good `Ok((401, body))`, so only the call log distinguishes
/// a session that self-healed from one that never tried.
///
/// Native-only: the dance is target-independent but `#[tokio::test]`
/// needs a runtime, and the suite has no wasm runner.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use proto_blue_common::fetch::{
        FetchError, FetchHandler, HttpHeaders, HttpRequest, HttpResponse,
    };
    use proto_blue_oauth::types::TokenSet;
    use proto_blue_oauth::{
        DpopKey, DpopNonceCache, OAuthClient, OAuthServerMetadata, OAuthSession,
    };

    use super::*;

    const ISSUER: &str = "https://pds.example";
    const TOKEN_ENDPOINT: &str = "https://pds.example/oauth/token";
    const RESOURCE: &str = "https://pds.example/xrpc/com.atproto.repo.applyWrites";
    const GET_RESOURCE: &str = "https://pds.example/xrpc/com.atproto.server.getSession";

    /// A `FetchHandler` that replays a canned script and records every
    /// request it was handed.
    ///
    /// Two queues rather than one, keyed on the token endpoint, because
    /// the assertions are about *which* endpoint was called and in what
    /// order — a single queue would let a test pass by consuming the
    /// refresh reply on a resource request.
    struct Scripted {
        token: Mutex<VecDeque<HttpResponse>>,
        resource: Mutex<VecDeque<HttpResponse>>,
        seen: Mutex<Vec<HttpRequest>>,
    }

    impl Scripted {
        fn new(token: Vec<HttpResponse>, resource: Vec<HttpResponse>) -> Arc<Self> {
            Arc::new(Self {
                token: Mutex::new(token.into()),
                resource: Mutex::new(resource.into()),
                seen: Mutex::new(Vec::new()),
            })
        }

        /// The requests seen, in the order the transport saw them.
        fn log(&self) -> Vec<HttpRequest> {
            self.seen.lock().unwrap().clone()
        }

        fn hits(&self, url: &str) -> usize {
            self.log().iter().filter(|r| r.url == url).count()
        }
    }

    #[async_trait]
    impl FetchHandler for Scripted {
        async fn fetch(&self, req: HttpRequest) -> Result<HttpResponse, FetchError> {
            let url = req.url.clone();
            self.seen.lock().unwrap().push(req);
            let queue = if url == TOKEN_ENDPOINT {
                &self.token
            } else {
                &self.resource
            };
            queue
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| FetchError::Other(format!("no scripted reply left for {url}")))
        }
    }

    fn reply(status: u16, body: &str, headers: &[(&str, &str)]) -> HttpResponse {
        let mut map = HttpHeaders::new();
        for (k, v) in headers {
            map.insert((*k).to_ascii_lowercase(), (*v).to_string());
        }
        HttpResponse {
            status,
            headers: map,
            body: body.as_bytes().to_vec(),
        }
    }

    /// A 200 from `/token` handing back a rotated pair.
    fn rotated_token(access: &str, refresh: &str) -> HttpResponse {
        reply(
            200,
            &serde_json::json!({
                "access_token": access,
                "token_type": "DPoP",
                "refresh_token": refresh,
                "expires_in": 3600,
                "sub": "did:plc:tester",
            })
            .to_string(),
            &[("content-type", "application/json")],
        )
    }

    fn token_set(access: &str, expires_at: &str) -> TokenSet {
        TokenSet {
            issuer: ISSUER.into(),
            sub: "did:plc:tester".into(),
            scope: "atproto".into(),
            access_token: access.into(),
            refresh_token: Some("refresh-1".into()),
            token_type: "DPoP".into(),
            expires_at: Some(expires_at.into()),
            aud: Some(ISSUER.into()),
        }
    }

    /// Build a session and a refresh context that share one scripted
    /// transport, so both halves of the dance land in the same call log.
    fn rig(
        script: &Arc<Scripted>,
        access: &str,
        expires_at: &str,
    ) -> (OAuthSession, OauthRefreshCtx) {
        let fetcher: Arc<dyn FetchHandler> = script.clone();
        let session = OAuthSession::with_fetch_handler(
            token_set(access, expires_at),
            DpopKey::generate().expect("DPoP key"),
            DpopNonceCache::new(),
            fetcher.clone(),
        );
        let server_metadata: OAuthServerMetadata = serde_json::from_value(serde_json::json!({
            "issuer": ISSUER,
            "authorization_endpoint": format!("{ISSUER}/oauth/authorize"),
            "token_endpoint": TOKEN_ENDPOINT,
        }))
        .expect("server metadata");
        let ctx = OauthRefreshCtx {
            client: Arc::new(OAuthClient::with_fetch_handler(
                super::super::client_metadata(),
                fetcher,
            )),
            server_metadata,
        };
        (session, ctx)
    }

    /// Read the claims out of a compact JWS without verifying it. The
    /// nonce assertion below has to look *inside* the DPoP proof: two
    /// proofs for the same request differ anyway (fresh `jti` and `iat`
    /// each time), so "the header changed" would pass whether or not the
    /// server's nonce was picked up.
    fn dpop_claims(proof: &str) -> serde_json::Value {
        let payload = proof
            .split('.')
            .nth(1)
            .expect("compact JWS has three parts");
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut acc: u32 = 0;
        let mut bits: u32 = 0;
        let mut out: Vec<u8> = Vec::new();
        for c in payload.bytes() {
            let Some(v) = ALPHABET.iter().position(|&a| a == c) else {
                continue;
            };
            acc = (acc << 6) | v as u32;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push(((acc >> bits) & 0xff) as u8);
            }
        }
        serde_json::from_slice(&out).expect("DPoP claims are JSON")
    }

    /// **The nonce the server chose reaches the replayed proof** (#1151).
    ///
    /// Sequence: POST an applyWrites batch to an origin this session has
    /// never spoken to, so the first proof carries no `nonce` claim; the
    /// PDS answers `401 use_dpop_nonce` with the nonce in a header.
    /// The helper replays once, and the replay is only useful if the
    /// nonce cache upstream filled on the way past is actually read.
    /// Only the retry's status and body are returned — the first
    /// response is a protocol handshake, not an answer.
    #[tokio::test]
    async fn a_use_dpop_nonce_401_is_replayed_with_the_servers_nonce() {
        let script = Scripted::new(
            vec![],
            vec![
                reply(
                    401,
                    r#"{"error":"use_dpop_nonce"}"#,
                    &[
                        ("dpop-nonce", "nonce-from-server"),
                        ("www-authenticate", r#"DPoP error="use_dpop_nonce""#),
                    ],
                ),
                reply(200, r#"{"ok":true}"#, &[]),
            ],
        );
        let (session, ctx) = rig(&script, "access-1", "2099-01-01T00:00:00Z");

        let (status, body) =
            oauth_post_with_refresh(&session, &ctx, RESOURCE, &serde_json::json!({}))
                .await
                .expect("the retry succeeds");

        assert_eq!(status.as_u16(), 200, "the retry's status, not the 401");
        assert_eq!(body, r#"{"ok":true}"#, "the retry's body, not the 401's");
        assert_eq!(script.hits(RESOURCE), 2, "sent once, replayed once");
        assert_eq!(script.hits(TOKEN_ENDPOINT), 0, "a nonce is not an expiry");

        let log = script.log();
        let first = dpop_claims(log[0].headers.get("dpop").expect("first proof"));
        let retry = dpop_claims(log[1].headers.get("dpop").expect("retry proof"));
        assert!(
            first.get("nonce").is_none(),
            "the first proof cannot know a nonce it has not been told"
        );
        assert_eq!(
            retry.get("nonce").and_then(|v| v.as_str()),
            Some("nonce-from-server"),
            "the replay must carry the nonce the 401 handed back"
        );
    }

    /// **An expired token is refreshed before the write is attempted**
    /// (#1151).
    ///
    /// The proactive half of the dance. `is_expired_jittered` is true, so
    /// `/token` must be the *first* thing on the wire — a refresh that
    /// happened after a doomed POST would still leave the token set
    /// rotated, which is why the assertion is on the order of the log
    /// and not on its contents.
    #[tokio::test]
    async fn an_expired_token_is_refreshed_before_the_post_is_sent() {
        let script = Scripted::new(
            vec![rotated_token("access-2", "refresh-2")],
            vec![reply(200, r#"{"ok":true}"#, &[])],
        );
        let (session, ctx) = rig(&script, "access-1", "2020-01-01T00:00:00Z");

        let (status, _) = oauth_post_with_refresh(&session, &ctx, RESOURCE, &serde_json::json!({}))
            .await
            .expect("the write succeeds on a refreshed token");

        assert_eq!(status.as_u16(), 200);
        let log = script.log();
        assert_eq!(log.len(), 2, "one refresh, one write");
        assert_eq!(log[0].url, TOKEN_ENDPOINT, "refresh comes first");
        assert_eq!(log[1].url, RESOURCE);
        assert_eq!(
            log[1].headers.get("authorization").map(String::as_str),
            Some("DPoP access-2"),
            "the write must use the token the refresh minted"
        );
    }

    /// **An `invalid_token` 401 refreshes once and replays the write**
    /// (#1151).
    ///
    /// The reactive half, and the branch this issue was filed about: it
    /// used to key on `e.contains("Access token is invalid, refresh
    /// required")`, a literal copied out of proto-blue-oauth. Sequence:
    /// a session idle past the access-token lifetime whose `expires_at`
    /// still reads fresh (a PDS may revoke early, and a resumed wasm
    /// session can carry a stale expiry) POSTs; the PDS answers 401 with
    /// the RFC 6750 challenge header. One `/token` round-trip, one
    /// replay, and the replay carries the rotated token — assert all
    /// three, because a branch that silently stopped firing would return
    /// a perfectly plausible `Ok((401, body))`.
    #[tokio::test]
    async fn an_invalid_token_401_refreshes_once_and_replays_the_post() {
        let script = Scripted::new(
            vec![rotated_token("access-2", "refresh-2")],
            vec![
                reply(
                    401,
                    r#"{"error":"InvalidToken"}"#,
                    &[("www-authenticate", r#"DPoP error="invalid_token""#)],
                ),
                reply(200, r#"{"ok":true}"#, &[]),
            ],
        );
        let (session, ctx) = rig(&script, "access-1", "2099-01-01T00:00:00Z");

        let (status, body) =
            oauth_post_with_refresh(&session, &ctx, RESOURCE, &serde_json::json!({}))
                .await
                .expect("the replay succeeds");

        assert_eq!(status.as_u16(), 200);
        assert_eq!(body, r#"{"ok":true}"#);
        assert_eq!(script.hits(TOKEN_ENDPOINT), 1, "exactly one refresh");
        assert_eq!(script.hits(RESOURCE), 2, "sent once, replayed once");
        assert_eq!(
            session.token_set().access_token,
            "access-2",
            "the rotated token set is what the session carries afterwards"
        );

        let log = script.log();
        assert_eq!(
            log[0].headers.get("authorization").map(String::as_str),
            Some("DPoP access-1"),
        );
        assert_eq!(
            log[2].headers.get("authorization").map(String::as_str),
            Some("DPoP access-2"),
            "the replay must use the new token — replaying the stale one \
             would 401 again and look like a PDS fault"
        );
    }

    /// **A refresh that fails propagates, and does not replay the write**
    /// (#1151).
    ///
    /// Sequence: 401 `invalid_token`, then the PDS rejects the refresh
    /// token itself (`400 invalid_grant` — the shape a revoked or
    /// already-rotated refresh token comes back as). The session is
    /// unrecoverable without a re-login, so the error has to reach the
    /// user rather than be spent on a second doomed POST. The `refresh: `
    /// prefix is load-bearing: `wasm_resume` keys the "clear the
    /// persisted blob" decision on a refresh failure, and the status
    /// line the owner reads is this string.
    #[tokio::test]
    async fn a_failed_refresh_propagates_and_the_post_is_not_replayed() {
        let script = Scripted::new(
            vec![reply(
                400,
                r#"{"error":"invalid_grant","error_description":"refresh token revoked"}"#,
                &[("content-type", "application/json")],
            )],
            vec![reply(
                401,
                r#"{"error":"InvalidToken"}"#,
                &[("www-authenticate", r#"DPoP error="invalid_token""#)],
            )],
        );
        let (session, ctx) = rig(&script, "access-1", "2099-01-01T00:00:00Z");

        let err = oauth_post_with_refresh(&session, &ctx, RESOURCE, &serde_json::json!({}))
            .await
            .expect_err("a rejected refresh token cannot be healed");

        assert!(
            err.starts_with("refresh: "),
            "callers and the wasm resume path both key on this prefix, got {err:?}"
        );
        assert!(
            err.contains("invalid_grant"),
            "the server's reason survives: {err:?}"
        );
        assert_eq!(script.hits(TOKEN_ENDPOINT), 1, "no refresh storm");
        assert_eq!(
            script.hits(RESOURCE),
            1,
            "the write must not be replayed on a token that was never rotated"
        );
        assert_eq!(
            session.token_set().access_token,
            "access-1",
            "a failed refresh leaves the token set alone"
        );
    }

    /// **A 401 without the challenge header is a status, not a refresh**
    /// (#1151).
    ///
    /// Pinning a decision, not a discovery: proto-blue-oauth only raises
    /// `RefreshFailed` when the 401 carries `WWW-Authenticate` with
    /// `error="invalid_token"` under a `DPoP`/`Bearer` scheme. A bare
    /// 401 is `Ok(resp)` upstream, and we keep it that way — RFC 6750 §3
    /// requires the challenge on a genuine token rejection, so a 401
    /// without one is the PDS saying something else, and refreshing on
    /// every such response would spend a `/token` round-trip on each
    /// malformed request it turns away.
    #[tokio::test]
    async fn a_bare_401_is_returned_to_the_caller_without_refreshing() {
        let script = Scripted::new(vec![], vec![reply(401, r#"{"error":"AuthMissing"}"#, &[])]);
        let (session, ctx) = rig(&script, "access-1", "2099-01-01T00:00:00Z");

        let (status, body) =
            oauth_post_with_refresh(&session, &ctx, RESOURCE, &serde_json::json!({}))
                .await
                .expect("a bare 401 is a status the caller reports, not a helper error");

        assert_eq!(status.as_u16(), 401);
        assert_eq!(body, r#"{"error":"AuthMissing"}"#);
        assert_eq!(script.hits(TOKEN_ENDPOINT), 0, "nothing to refresh");
        assert_eq!(script.hits(RESOURCE), 1, "and nothing to replay");
    }

    /// **The GET path does its own nonce dance** (#1151).
    ///
    /// `fetch_session_identity` is the only caller of
    /// `oauth_get_with_nonce_retry`, and it runs on a token seconds old
    /// — which is why there is no `oauth_get_with_refresh` to test. What
    /// it does need is the nonce retry, because it is usually the very
    /// first request this process makes to the origin and therefore the
    /// one that always gets the `use_dpop_nonce` challenge.
    #[tokio::test]
    async fn get_session_survives_the_first_use_dpop_nonce_challenge() {
        let script = Scripted::new(
            vec![],
            vec![
                reply(
                    401,
                    r#"{"error":"use_dpop_nonce"}"#,
                    &[("dpop-nonce", "nonce-from-server")],
                ),
                reply(
                    200,
                    r#"{"did":"did:plc:tester","handle":"tester.example"}"#,
                    &[],
                ),
            ],
        );
        let (session, _ctx) = rig(&script, "access-1", "2099-01-01T00:00:00Z");

        let (did, handle) = fetch_session_identity(&session, ISSUER)
            .await
            .expect("the retry carries the nonce and succeeds");

        assert_eq!(did, "did:plc:tester");
        assert_eq!(handle, "tester.example");
        assert_eq!(script.hits(GET_RESOURCE), 2, "sent once, replayed once");
    }
}
