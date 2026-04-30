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

use proto_blue_oauth::OAuthSession;
use serde::Deserialize;

use super::OauthRefreshCtx;
use super::discovery::INVALID_TOKEN_ERR;

/// Authenticated GET with an automatic DPoP-nonce retry.
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
        Err(e) if e.contains(INVALID_TOKEN_ERR) => {
            refresh_session(session, refresh).await?;
            oauth_post_with_nonce_retry(session, url, body_json).await
        }
        Err(e) => Err(e),
    }
}

/// GET counterpart to [`oauth_post_with_refresh`]. See that function's
/// docs for why both proactive and reactive refresh paths are needed.
pub async fn oauth_get_with_refresh(
    session: &OAuthSession,
    refresh: &OauthRefreshCtx,
    url: &str,
) -> Result<(reqwest::StatusCode, String), String> {
    if session.is_expired_jittered() {
        refresh_session(session, refresh).await?;
    }
    match oauth_get_with_nonce_retry(session, url).await {
        Ok(pair) => Ok(pair),
        Err(e) if e.contains(INVALID_TOKEN_ERR) => {
            refresh_session(session, refresh).await?;
            oauth_get_with_nonce_retry(session, url).await
        }
        Err(e) => Err(e),
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
