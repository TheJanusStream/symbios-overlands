//! WASM-only callback + persisted-session resume paths.

use std::sync::Arc;

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::{AtprotoSession, get_service_auth};
use proto_blue_oauth::OAuthClient;

use crate::boot_params::BootParams;
use crate::oauth::{self, OauthClientRes};
use crate::state::{AppState, RelayHost};

use super::complete::{install_completed_session, spawn_complete_task};
use super::{CompleteAuthTask, CompletedSession, LoginError};

/// WASM-only: on first login-state frame, check the URL for `?code=&state=`
/// returned by the authorization server. If present, scrub the URL so a
/// reload cannot replay the single-use code, then kick off the exchange.
pub fn check_wasm_callback(
    mut commands: Commands,
    oauth_client: Res<OauthClientRes>,
    existing: Query<&CompleteAuthTask>,
    mut ran: Local<bool>,
) {
    if *ran || !existing.is_empty() {
        return;
    }
    *ran = true;
    let Some((code, _state)) = oauth::wasm::read_callback_params() else {
        return;
    };
    oauth::wasm::scrub_url();
    let Some(pending) = oauth::wasm::take_pending() else {
        warn!("OAuth callback returned ?code= but no pending auth in sessionStorage");
        return;
    };
    commands.insert_resource(RelayHost(pending.relay_host.clone()));
    spawn_complete_task(&mut commands, oauth_client.0.clone(), pending, code);
}

/// In-flight task that rebuilds an `AtprotoSession` from a persisted blob,
/// refreshes the access token if it's expired, and fetches a fresh service
/// token from the relay. Drained by [`poll_resume_task`]. Mirrors
/// [`CompleteAuthTask`]'s shape so the post-login installation step is
/// shared.
#[derive(Component)]
pub struct ResumeAuthTask(bevy::tasks::Task<Result<CompletedSession, String>>);

/// One-shot system that fires on the first frame in `AppState::Login` and
/// kicks off a [`ResumeAuthTask`] if a valid persisted session is on disk.
/// A bad blob (deserialise failure) is silently dropped by `load_persisted`,
/// so the worst-case behaviour is "show the login form anyway."
#[allow(clippy::too_many_arguments)]
pub fn check_wasm_resume(
    mut commands: Commands,
    oauth_client: Res<OauthClientRes>,
    existing_complete: Query<&CompleteAuthTask>,
    existing_resume: Query<&ResumeAuthTask>,
    existing_session: Option<Res<AtprotoSession>>,
    boot: Option<Res<BootParams>>,
    mut ran: Local<bool>,
) {
    if *ran
        || existing_session.is_some()
        || !existing_complete.is_empty()
        || !existing_resume.is_empty()
    {
        return;
    }
    *ran = true;
    let Some(mut blob) = oauth::wasm::load_persisted() else {
        return;
    };
    // URL/CLI boot params win over the persisted blob: a shared landmark
    // link should drop the recipient at the linked overland even though
    // their local browser remembers them at "home". The blob itself is
    // not rewritten — the override is applied in-memory only, so the
    // next reload (without the URL params) restores the persisted view.
    let (boot_did, boot_pos, boot_yaw) = boot
        .as_deref()
        .map(|b| (b.target_did.clone(), b.target_pos, b.target_yaw_deg))
        .unwrap_or((None, None, None));
    if let Some(did) = boot_did {
        blob.target_did = did;
    }
    info!("Resuming persisted session for {}", blob.handle);
    commands.insert_resource(RelayHost(blob.relay_host.clone()));
    spawn_resume_task(
        &mut commands,
        oauth_client.0.clone(),
        blob,
        boot_pos,
        boot_yaw,
    );
}

/// Spawn the async task that rebuilds the session from `blob`. Splits cleanly
/// from `spawn_complete_task` because the callback exchange is skipped — the
/// token set is already in hand from localStorage; we only need to rebuild
/// the `OAuthSession` object and (if expired) refresh.
fn spawn_resume_task(
    commands: &mut Commands,
    client: Arc<OAuthClient>,
    blob: oauth::wasm::PersistedSession,
    spawn_pos: Option<crate::boot_params::TargetPos>,
    spawn_yaw_deg: Option<f32>,
) {
    use proto_blue_oauth::OAuthSession;
    use proto_blue_oauth::client::dpop_key_from_jwk;

    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let dpop_key =
            dpop_key_from_jwk(&blob.dpop_jwk).map_err(|e| format!("dpop_key_from_jwk: {e}"))?;
        let oauth_session = Arc::new(OAuthSession::new(
            blob.token_set.clone(),
            dpop_key,
            client.dpop_nonces().clone(),
        ));
        let refresh_ctx = crate::oauth::OauthRefreshCtx {
            client: client.clone(),
            server_metadata: blob.server_metadata.clone(),
        };
        // If the persisted access token has expired, rotate it before any
        // downstream call. A failure here is terminal — the refresh token
        // has been invalidated server-side and the user must re-auth — so
        // drop the persisted blob and surface the error to the login UI.
        if oauth_session.is_expired_jittered() {
            if let Err(e) = crate::oauth::refresh_session(&oauth_session, &refresh_ctx).await {
                oauth::wasm::clear_persisted();
                return Err(format!("resume refresh: {e}"));
            }
        }
        let session = AtprotoSession {
            did: blob.did.clone(),
            handle: blob.handle.clone(),
            pds_url: blob.pds_url.clone(),
            session: oauth_session,
        };
        let service_did = format!("did:web:{}", blob.relay_host);
        let service_token = get_service_auth(&session, &service_did)
            .await
            .map_err(|e| format!("resume get_service_auth: {e}"))?;
        let room_did = if blob.target_did.is_empty() {
            session.did.clone()
        } else {
            blob.target_did.clone()
        };
        Ok::<_, String>(CompletedSession {
            session,
            refresh_ctx,
            service_token,
            room_did,
            spawn_pos,
            spawn_yaw_deg,
        })
    });
    commands.spawn(ResumeAuthTask(task));
}

/// Drain finished [`ResumeAuthTask`]s. Shares the same installation steps as
/// `poll_complete_auth_task`: insert session/refresh resources, transition
/// to `Loading`. On error, log + show the login form so the user can retry.
pub fn poll_resume_task(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut ResumeAuthTask)>,
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
            Ok(completed) => {
                info!(
                    "Resumed session {} ({}); skipping login form",
                    completed.session.handle, completed.session.did
                );
                install_completed_session(
                    &mut commands,
                    &mut next_state,
                    completed,
                    relay_host.as_deref(),
                );
            }
            Err(msg) => {
                warn!("Resume failed: {msg}");
                login_error.0 = Some(format!("Session resume failed: {msg}"));
            }
        }
    }
}
