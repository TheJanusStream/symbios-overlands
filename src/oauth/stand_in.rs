//! A session signed in to nothing (#1353, #1415), for the headless tools that
//! run the game's own signed-in systems with no account behind them: the
//! render tool's editor pictures, and the agent client's offline mode.
//!
//! Built the way the crate's tests build theirs, with every URL on
//! `example.invalid` - a name RFC 6761 guarantees never resolves - so nothing
//! the session names can be reached and nothing it would write goes
//! anywhere. It has no refresh token, so it can never rotate either.

use std::sync::Arc;

use bevy_symbios_multiuser::auth::AtprotoSession;
use proto_blue_oauth::types::TokenSet;
use proto_blue_oauth::{DpopKey, DpopNonceCache, OAuthSession};

use super::OauthRefreshCtx;
use super::capped_fetch::CappedFetcher;

/// Where a stand-in session says its PDS and authorization server are.
const NOWHERE: &str = "https://example.invalid";

/// A session for `did` / `handle` that is signed in to nothing.
pub fn stand_in_session(did: &str, handle: &str) -> Result<AtprotoSession, String> {
    let token_set = TokenSet {
        issuer: NOWHERE.into(),
        sub: did.into(),
        scope: "atproto".into(),
        access_token: "stand-in".into(),
        refresh_token: None,
        token_type: "DPoP".into(),
        expires_at: None,
        aud: None,
    };
    let dpop_key = DpopKey::generate().map_err(|e| format!("a DPoP key: {e}"))?;
    Ok(AtprotoSession {
        did: did.into(),
        handle: handle.into(),
        pds_url: NOWHERE.into(),
        session: Arc::new(OAuthSession::with_fetch_handler(
            token_set,
            dpop_key,
            DpopNonceCache::new(),
            Arc::new(CappedFetcher::new()),
        )),
    })
}

/// The refresh context the game's signed-in systems require beside the
/// session. Never used to refresh: the stand-in has nothing to rotate.
pub fn stand_in_refresh_ctx() -> Result<OauthRefreshCtx, String> {
    let server_metadata = serde_json::from_value(serde_json::json!({
        "issuer": NOWHERE,
        "authorization_endpoint": format!("{NOWHERE}/authorize"),
        "token_endpoint": format!("{NOWHERE}/token"),
    }))
    .map_err(|e| format!("the stand-in server metadata: {e}"))?;
    Ok(OauthRefreshCtx {
        client: super::OauthClientRes::default().0,
        server_metadata,
        rotation_sink: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stand_in_names_nowhere_and_cannot_rotate() {
        let session = stand_in_session("did:plc:standin", "standin.example").expect("builds");
        assert!(session.pds_url.ends_with(".invalid"), "{}", session.pds_url);
        assert_eq!(session.session.token_set().refresh_token, None);

        let ctx = stand_in_refresh_ctx().expect("builds");
        assert!(ctx.rotation_sink.is_none());
        assert!(
            ctx.server_metadata
                .token_endpoint
                .ends_with(".invalid/token"),
            "{}",
            ctx.server_metadata.token_endpoint
        );
    }
}
