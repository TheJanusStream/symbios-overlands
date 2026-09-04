//! WASM-only callback + persisted-session resume paths.

use std::sync::Arc;

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use proto_blue_oauth::OAuthClient;

use crate::boot_params::BootParams;
use crate::oauth::{self, OauthClientRes};
use crate::state::{AppState, RelayHost};

use super::complete::{install_completed_session, spawn_complete_task};
use super::{CompleteAuthTask, CompletedSession, LoginError};

/// WASM-only: on first login-state frame, check the URL for the
/// authorization server's callback parameters. On `?code=&state=`, scrub
/// the URL so a reload cannot replay the single-use code, then kick off
/// the exchange. On `?error=` (user denied, expired request, …), scrub
/// the URL, drop the now-useless pending blob, and surface the error —
/// before #847 a deny silently re-showed the form over a stale
/// `PendingAuth` blob.
pub fn check_wasm_callback(
    mut commands: Commands,
    oauth_client: Res<OauthClientRes>,
    existing: Query<&CompleteAuthTask>,
    mut login_error: ResMut<LoginError>,
    mut ran: Local<bool>,
) {
    if *ran || !existing.is_empty() {
        return;
    }
    *ran = true;
    // The boot handoff marker (#978) held the attract backdrop off for
    // exactly this frame; from here the `CompleteAuthTask` spawned below
    // — or, on the bail-out paths, the idle form itself — is the honest
    // signal. Both this removal and that spawn ride the same command
    // queue, so no frame ever sees the marker gone with the task not yet
    // visible. `check_wasm_resume` clears it too; a second remove is a
    // no-op and the pair are ordering-independent that way.
    commands.remove_resource::<oauth::AuthHandoffPending>();
    let params = oauth::wasm::read_callback_params();
    if let Some(msg) = params.error_message() {
        warn!("OAuth callback returned an error redirect: {msg}");
        oauth::wasm::scrub_url();
        let _ = oauth::wasm::take_pending();
        login_error.0 = Some(msg);
        return;
    }
    let Some(code) = params.code else {
        return;
    };
    oauth::wasm::scrub_url();
    let Some(pending) = oauth::wasm::take_pending() else {
        warn!("OAuth callback returned ?code= but no pending auth in sessionStorage");
        login_error.0 = Some(
            "The login response arrived, but this tab had no login attempt in \
             progress (it may have started in another tab, or the browser session \
             expired). Please sign in again."
                .to_string(),
        );
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

/// Whether the persisted-session resume has already had its one shot this
/// page load (#1228 f6).
///
/// A `Resource` rather than the `Local<bool>` it used to be, because the
/// Retry button on a resume failure has to be able to re-arm it. The
/// alternative — the only exit the screen had — was "Not you? Sign in
/// differently", which throws the saved session away, and a relay outage
/// is not a reason to forget who somebody is.
///
/// Deliberately not part of [`super::LoginUiLatch`]: that latch is reset on
/// `OnEnter(AppState::Login)` so a re-entry behaves like a fresh page load,
/// and re-running the resume on every return to the form would spawn a
/// second auth task behind the one the user just escaped. This one is
/// page-load scoped, like the callback check beside it.
#[derive(Resource, Default)]
pub struct ResumeLatch {
    spent: bool,
}

impl ResumeLatch {
    /// Re-arm the one-shot so [`check_wasm_resume`] runs again next frame.
    /// The persisted blob is untouched — that is the whole difference
    /// between this and the "Not you?" hatch.
    pub fn rearm(&mut self) {
        self.spent = false;
    }
}

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
    mut latch: ResMut<ResumeLatch>,
) {
    if latch.spent
        || existing_session.is_some()
        || !existing_complete.is_empty()
        || !existing_resume.is_empty()
    {
        return;
    }
    latch.spent = true;
    // See `check_wasm_callback` — the boot handoff marker is spent once
    // this one-shot has decided, and the `ResumeAuthTask` spawned below
    // takes over as the attract backdrop's "not idle" signal.
    commands.remove_resource::<oauth::AuthHandoffPending>();
    let Some(mut blob) = oauth::wasm::load_persisted() else {
        return;
    };
    // URL/CLI boot params win over the persisted blob: a shared landmark
    // link should drop the recipient at the linked overland even though
    // their local browser remembers them at "home". The override is
    // applied in-memory here; `install_completed_session` writes it back
    // once the resume actually lands somebody in that world (#1229 f2), so
    // the blob records where the user IS rather than where they first
    // signed in.
    let (boot_did, boot_pos, boot_yaw) = boot
        .as_deref()
        .map(|b| (b.target_did.clone(), b.target_pos, b.target_yaw_deg))
        .unwrap_or((None, None, None));
    if let Some(did) = boot_did {
        blob.target_did = did;
    }
    info!("Resuming persisted session for {}", blob.handle);
    // Who this is and where it lands (#1229 f10). The card asked "Not
    // you?" while this very system logged the answer one line up.
    commands.insert_resource(super::entry::ResumeIdentity {
        handle: blob.handle.clone(),
        did: blob.did.clone(),
        target_did: blob.target_did.clone(),
    });
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
        let fut = async move {
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
            if oauth_session.is_expired_jittered()
                && let Err(e) = crate::oauth::refresh_session(&oauth_session, &refresh_ctx).await
            {
                oauth::wasm::clear_persisted();
                return Err(format!("resume refresh: {e}"));
            }
            let session = AtprotoSession {
                did: blob.did.clone(),
                handle: blob.handle.clone(),
                pds_url: blob.pds_url.clone(),
                session: oauth_session,
            };
            let service_token = crate::oauth::get_relay_service_auth(&session, &blob.relay_host)
                .await
                .map_err(|e| format!("resume get_relay_service_auth: {e}"))?;
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
        };
        // The bound #1129 introduced, which this one path never got
        // (#1228 f3). The wasm reqwest client routes through the browser's
        // fetch API and has no idle-body timeout, so an expired token on a
        // flaky network left `refresh_session` pending forever — and this
        // is the most common return path of the deployed target, sitting
        // behind a spinner whose only escape hatch forgets the user.
        //
        // The timeout arm KEEPS the persisted blob: nothing was proven
        // wrong with the saved session, only with the network, so the
        // Retry button below can re-run exactly this task.
        crate::config::http::run_or(fut, Err(crate::config::http::timed_out("session resume")))
            .await
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
    mut ui_latch: ResMut<super::LoginUiLatch>,
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
                // Keep the `has_persisted` cache honest (#1228 f6). The
                // refresh arm above clears the stored blob on its way out,
                // and the login card answers "does this machine have a
                // saved session?" once per visit — a stale yes would both
                // hide the Retry button and send `entry_plan` down its
                // Idle arm, so a landmark link would stop naming its
                // destination the moment a resume failed.
                if !super::resume_keeps_session(&msg) {
                    ui_latch.persisted = Some(false);
                }
                login_error.0 = Some(format!("Session resume failed: {msg}"));
            }
        }
    }
}
