//! Drains [`CompleteAuthTask`]s — the `code` → token exchange + service-
//! token round-trip — and installs the resulting session resources.
//! Shared installer [`install_completed_session`] is also called by the
//! WASM resume path so the two pipelines never drift on the post-auth step.

use std::marker::PhantomData;
use std::sync::Arc;

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::{AtprotoSession, get_service_auth};
use bevy_symbios_multiuser::prelude::*;
use bevy_symbios_multiuser::signaller::{TokenSource, TokenSourceRes};
use proto_blue_oauth::OAuthClient;

use crate::oauth::{self, PendingAuth};
use crate::protocol::OverlandsMessage;
use crate::state::{AppState, CurrentRoomDid, PendingSpawnPlacement, RelayHost};

use super::{CompleteAuthTask, CompletedSession, LoginError};

/// Drain finished [`CompleteAuthTask`]s. On success installs the session
/// resources and transitions to `Loading`; on failure surfaces the error
/// into [`LoginError`] so the user can retry.
pub fn poll_complete_auth_task(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut CompleteAuthTask)>,
    mut next_state: ResMut<NextState<AppState>>,
    mut login_error: ResMut<LoginError>,
    relay_host: Option<Res<RelayHost>>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        match result {
            Ok(completed) => install_completed_session(
                &mut commands,
                &mut next_state,
                completed,
                relay_host.as_deref(),
            ),
            Err(msg) => {
                warn!("Login failed: {msg}");
                login_error.0 = Some(msg);
            }
        }
    }
}

/// Shared post-auth installation: insert session resources, hand off the
/// optional spawn pose, build the relay socket config, and transition to
/// `Loading`. Used by both fresh-login (`poll_complete_auth_task`) and
/// resume (`poll_resume_task`) so the two paths can never drift on the
/// installation step.
pub(super) fn install_completed_session(
    commands: &mut Commands,
    next_state: &mut NextState<AppState>,
    completed: CompletedSession,
    relay_host: Option<&RelayHost>,
) {
    let CompletedSession {
        session,
        refresh_ctx,
        service_token,
        room_did,
        spawn_pos,
        spawn_yaw_deg,
    } = completed;
    info!("Authenticated as {} ({})", session.handle, session.did);
    commands.insert_resource(CurrentRoomDid(room_did.clone()));
    commands.insert_resource(session);
    commands.insert_resource(refresh_ctx);

    let source = TokenSource::new(Some(service_token));
    commands.insert_resource(TokenSourceRes(source));

    let host = relay_host.map(|r| r.0.as_str()).unwrap_or("");
    commands.insert_resource(SymbiosMultiuserConfig::<OverlandsMessage> {
        room_url: format!("wss://{}/overlands/{}", host, room_did),
        ice_servers: None,
        _marker: PhantomData,
    });

    if spawn_pos.is_some() || spawn_yaw_deg.is_some() {
        commands.insert_resource(PendingSpawnPlacement {
            pos: spawn_pos,
            yaw_deg: spawn_yaw_deg,
        });
    }

    next_state.set(AppState::Loading);
}

/// Spawn the async task that exchanges `code` for tokens, builds the
/// [`AtprotoSession`], and fetches the relay service token.
pub(super) fn spawn_complete_task(
    commands: &mut Commands,
    client: Arc<OAuthClient>,
    pending: PendingAuth,
    code: String,
) {
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let completed = oauth::complete_authorization(&client, &pending, &code).await?;
            let oauth::CompletedAuth {
                session: oauth_session,
                did,
                handle,
                pds_url,
                server_metadata,
                #[cfg(target_arch = "wasm32")]
                dpop_jwk,
                #[cfg(not(target_arch = "wasm32"))]
                    dpop_jwk: _,
            } = completed;
            let session = AtprotoSession {
                did,
                handle,
                pds_url,
                session: oauth_session,
            };
            let refresh_ctx = crate::oauth::OauthRefreshCtx {
                client: client.clone(),
                server_metadata: server_metadata.clone(),
            };

            // Persist the session blob to localStorage *before* the
            // service-token round-trip so a network failure on that call
            // doesn't strand the user with a usable PDS session that they
            // can't restore on reload. We re-persist the rotated token
            // set on every subsequent refresh via
            // `wasm::update_persisted_token_set`.
            #[cfg(target_arch = "wasm32")]
            {
                let blob = oauth::wasm::PersistedSession {
                    token_set: session.session.token_set(),
                    dpop_jwk,
                    server_metadata,
                    did: session.did.clone(),
                    handle: session.handle.clone(),
                    pds_url: session.pds_url.clone(),
                    relay_host: pending.relay_host.clone(),
                    target_did: pending.target_did.clone(),
                };
                if let Err(e) = oauth::wasm::save_persisted(&blob) {
                    warn!("save_persisted: {e}");
                }
            }

            let service_did = format!("did:web:{}", pending.relay_host);
            let service_token = get_service_auth(&session, &service_did)
                .await
                .map_err(|e| format!("get_service_auth: {e}"))?;
            let room_did = if pending.target_did.is_empty() {
                session.did.clone()
            } else {
                pending.target_did.clone()
            };
            Ok::<_, String>(CompletedSession {
                session,
                refresh_ctx,
                service_token,
                room_did,
                spawn_pos: pending.target_pos,
                spawn_yaw_deg: pending.target_yaw_deg,
            })
        };
        #[cfg(target_arch = "wasm32")]
        {
            fut.await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(fut)
        }
    });
    commands.spawn(CompleteAuthTask(task));
}
