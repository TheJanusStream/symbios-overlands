//! Periodic refresh of the relay **service-auth** token (#714).
//!
//! [`get_service_auth`] mints a short-lived (~60 s on bsky) JWT that the WebRTC
//! signaller presents to the relay on every (re)connect. It is fetched once at
//! login (see [`crate::ui::login::poll_complete_auth_task`]) and, without this
//! module, never renewed — so any reconnect (portal hop, dead-socket respawn,
//! network flap) more than a token-lifetime after login re-handshakes with an
//! **expired** token, and the relay rejects it with HTTP 401 (its `validate_exp`
//! hardening). On native the signaller fast-fails 4xx and backs off, reusing
//! the same dead token forever; on wasm the browser hides the status and the
//! blind-retry budget is exhausted. Either way the peer cannot (re)join the
//! room, which is why a full logout/login (re-issuing a fresh token) was the
//! only recovery.
//!
//! This module keeps [`TokenSourceRes`] continuously fresh by re-issuing the
//! service token on a fixed cadence ([`config::network::SERVICE_TOKEN_REFRESH_SECS`])
//! well inside its lifetime, so the token the signaller reads at reconnect time
//! is always valid. The refresh also proactively renews the underlying OAuth
//! access token when it is near expiry, so a long session self-heals the same
//! way authenticated PDS writes do (see [`super::refresh`]).

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use bevy_symbios_multiuser::auth::{AtprotoSession, get_service_auth};
use bevy_symbios_multiuser::signaller::TokenSourceRes;

use crate::config;
use crate::oauth::OauthRefreshCtx;
use crate::state::RelayHost;

/// An in-flight service-token refresh. Its result is the fresh token string,
/// or an error to log (the next cadence tick retries).
#[derive(Component)]
pub struct ServiceTokenRefreshTask(Task<Result<String, String>>);

/// Spawn a service-token refresh on a fixed cadence while a session is active,
/// unless one is already in flight. The freshly-minted token is installed by
/// [`poll_service_token_refresh`].
///
/// Runs unconditionally in `Update`; the `Option<Res<…>>` gates make it inert
/// until login installs the session/token/relay resources, and reset the
/// cadence on logout so the next login starts a fresh schedule.
#[allow(clippy::too_many_arguments)]
pub fn schedule_service_token_refresh(
    mut commands: Commands,
    time: Res<Time>,
    session: Option<Res<AtprotoSession>>,
    refresh_ctx: Option<Res<OauthRefreshCtx>>,
    relay_host: Option<Res<RelayHost>>,
    token_source: Option<Res<TokenSourceRes>>,
    in_flight: Query<(), With<ServiceTokenRefreshTask>>,
    mut next_at: Local<f64>,
    mut initialized: Local<bool>,
) {
    let (Some(session), Some(refresh_ctx), Some(relay_host), Some(_token_source)) =
        (session, refresh_ctx, relay_host, token_source)
    else {
        // Logged out (or not yet logged in): re-arm so the next login begins a
        // fresh cadence rather than firing immediately off a stale timer.
        *initialized = false;
        return;
    };

    let now = time.elapsed_secs_f64();
    if !*initialized {
        *initialized = true;
        *next_at = now + config::network::SERVICE_TOKEN_REFRESH_SECS;
        return;
    }
    if now < *next_at || !in_flight.is_empty() {
        return;
    }
    *next_at = now + config::network::SERVICE_TOKEN_REFRESH_SECS;

    let session = session.clone();
    let refresh_ctx = refresh_ctx.clone();
    let service_did = format!("did:web:{}", relay_host.0);

    let pool = IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            // Keep the underlying OAuth access token fresh so the DPoP-signed
            // getServiceAuth call itself does not 401 on a long-idle session —
            // mirrors the write path's proactive refresh in `super::refresh`.
            if session.session.is_expired_jittered() {
                super::refresh::refresh_session(&session.session, &refresh_ctx).await?;
            }
            get_service_auth(&session, &service_did)
                .await
                .map_err(|e| e.to_string())
        };
        #[cfg(target_arch = "wasm32")]
        {
            fut.await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::config::http::block_on(fut)
        }
    });
    commands.spawn(ServiceTokenRefreshTask(task));
}

/// Drain a finished [`ServiceTokenRefreshTask`] and install the fresh token
/// into [`TokenSourceRes`] so the next signaller (re)connect reads it. A failed
/// refresh is logged and left for the next cadence tick to retry — the current
/// token may still be valid, and a transient PDS error must not tear anything
/// down.
pub fn poll_service_token_refresh(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut ServiceTokenRefreshTask)>,
    token_source: Option<Res<TokenSourceRes>>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        match result {
            Ok(token) => {
                if let Some(ts) = &token_source {
                    ts.0.set(Some(token));
                    debug!("relay service-auth token refreshed");
                }
            }
            Err(e) => {
                warn!("relay service-auth token refresh failed (will retry): {e}");
            }
        }
    }
}
