//! High-level OAuth dance: [`begin_authorization`] (called from the login
//! UI to mint an authorization URL) and [`complete_authorization`] (called
//! from the callback handlers once the AS has redirected back with a code).

use std::sync::Arc;

use bevy::prelude::*;
use proto_blue_oauth::{OAuthClient, OAuthServerMetadata, OAuthSession, client::dpop_key_from_jwk};

use super::PendingAuth;
use super::discovery::discover_auth_server;
use super::refresh::fetch_session_identity;

/// Bundle of everything [`complete_authorization`] produces. Held briefly by
/// the login pipeline before being split into the long-lived `AtprotoSession`
/// + `OauthRefreshCtx` resources (and, on WASM, persisted to localStorage).
pub struct CompletedAuth {
    pub session: Arc<OAuthSession>,
    pub did: String,
    pub handle: String,
    pub pds_url: String,
    /// Server metadata as returned by `discover_server`. Needed for the
    /// `/token` refresh round-trip and stashed in `OauthRefreshCtx`.
    pub server_metadata: OAuthServerMetadata,
    /// JWK form of the DPoP private key. Carried so the WASM persistence
    /// layer can rebuild the `OAuthSession` after a page reload — the
    /// `DpopKey` inside the session is not directly serialisable.
    pub dpop_jwk: serde_json::Value,
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
    info!(
        "OAuth scope being requested: {:?}",
        oauth_client.client_metadata.scope
    );
    info!(
        "AS scopes_supported: {:?}",
        server_metadata.scopes_supported
    );
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
            // Spawn-pose params are filled in by the login pipeline after
            // this returns — they live in `BootParams`, not the OAuth
            // discovery flow.
            target_pos: None,
            target_yaw_deg: None,
        },
    ))
}

/// Exchange an authorization `code` for an [`OAuthSession`], resolve the
/// user's actual PDS shard from their DID, and fetch the session identity
/// against that PDS. Returns a [`CompletedAuth`] bundle whose
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
) -> Result<CompletedAuth, String> {
    let token_set: proto_blue_oauth::types::TokenSet = oauth_client
        .callback(code, &pending.auth_state, &pending.server_metadata)
        .await
        .map_err(|e| format!("callback: {e}"))?;
    info!("OAuth scope granted by AS: {:?}", token_set.scope);
    let did = token_set.sub.clone();
    if did.is_empty() {
        return Err("callback: token response missing `sub` (DID)".to_string());
    }
    let dpop_jwk = pending.auth_state.dpop_key.clone();
    let dpop_key = dpop_key_from_jwk(&dpop_jwk).map_err(|e| format!("dpop_key_from_jwk: {e}"))?;
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
    Ok(CompletedAuth {
        session: oauth_session,
        did,
        handle,
        pds_url,
        server_metadata: pending.server_metadata.clone(),
        dpop_jwk,
    })
}
