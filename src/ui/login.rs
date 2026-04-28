//! OAuth 2.0 + DPoP login flow.
//!
//! Collects PDS, Relay Host, and an optional destination DID, then drives
//! the atproto OAuth authorization-code flow via [`crate::oauth`]. The
//! authenticated handle is learnt back from the authorization response,
//! so no handle input is needed from the user. The flow is target-specific:
//!
//! - **WASM** — `sessionStorage` carries the pending-auth blob across the
//!   page redirect; the callback lands back on the hosted page with
//!   `?code=&state=` and `check_wasm_callback` kicks off the code
//!   exchange on the next frame.
//! - **Native** — a background `tiny_http` loopback server catches the
//!   redirect; the pending-auth blob lives in
//!   [`crate::oauth::NativePendingAuthRes`] and `poll_native_callback`
//!   drains the channel.

use std::marker::PhantomData;
use std::sync::Arc;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::{AtprotoSession, get_service_auth};
use bevy_symbios_multiuser::prelude::*;
use bevy_symbios_multiuser::signaller::{TokenSource, TokenSourceRes};
use proto_blue_oauth::OAuthClient;

use crate::boot_params::BootParams;
use crate::oauth::{self, OauthClientRes, PendingAuth};
use crate::protocol::OverlandsMessage;
use crate::state::{AppState, CurrentRoomDid, PendingSpawnPlacement, RelayHost};

/// (auth_url, pending)
type BeginOutcome = Result<(String, PendingAuth), String>;

/// Bundle returned by the fresh-login + resume async tasks. Replaces a
/// 4-tuple so adding the optional spawn pose did not turn the call sites
/// into positional-noise.
pub struct CompletedSession {
    pub session: AtprotoSession,
    pub refresh_ctx: crate::oauth::OauthRefreshCtx,
    pub service_token: String,
    pub room_did: String,
    /// Carried from the URL/CLI boot params (fresh login) or from the
    /// `BootParams` resource (resume). `None` ⇒ random spawn scatter.
    pub spawn_pos: Option<crate::boot_params::TargetPos>,
    pub spawn_yaw_deg: Option<f32>,
}

type CompleteOutcome = Result<CompletedSession, String>;

/// In-flight authorization initiation (PAR + `authorize`). On completion we
/// either navigate the tab (WASM) or launch the system browser (native).
#[derive(Component)]
pub struct BeginAuthTask(bevy::tasks::Task<BeginOutcome>);

/// In-flight `code` → token exchange + service-token round-trip, running
/// after the OAuth callback delivers an authorization code.
#[derive(Component)]
pub struct CompleteAuthTask(bevy::tasks::Task<CompleteOutcome>);

/// Latest login-pipeline failure, shown underneath the login form.
///
/// Kept as a Bevy `Resource` rather than a `Local` on either UI system so
/// the rendering system and the polling system share a single authoritative
/// buffer — a `Local<LoginError>` would give each system its own private
/// copy and silently swallow every message.
#[derive(Resource, Default)]
pub struct LoginError(pub Option<String>);

#[derive(Clone)]
pub struct LoginFormState {
    pds: String,
    relay_host: String,
    target_did: String,
}

impl Default for LoginFormState {
    fn default() -> Self {
        Self {
            pds: crate::config::login::DEFAULT_PDS.into(),
            relay_host: crate::config::login::DEFAULT_RELAY_HOST.into(),
            target_did: crate::config::login::DEFAULT_TARGET_DID.into(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn login_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut form: Local<LoginFormState>,
    mut prefilled: Local<bool>,
    mut autosubmitted: Local<bool>,
    boot: Option<Res<BootParams>>,
    login_error: Res<LoginError>,
    oauth_client: Res<OauthClientRes>,
    begin_tasks: Query<&BeginAuthTask>,
    complete_tasks: Query<&CompleteAuthTask>,
) {
    // First-frame pre-fill from URL/CLI boot params. Done as a one-shot
    // (`*prefilled` latch) so a subsequent re-render does not stomp on
    // edits the user made after landing on the form. `pds` / `relay`
    // fall back to the form defaults when not provided so an empty boot
    // input behaves identically to the prior release.
    if !*prefilled
        && let Some(boot) = boot.as_deref()
        && boot.is_any()
    {
        if let Some(did) = &boot.target_did {
            form.target_did = did.clone();
        }
        if let Some(pds) = &boot.pds {
            form.pds = pds.clone();
        }
        if let Some(relay) = &boot.relay {
            form.relay_host = relay.clone();
        }
        *prefilled = true;
    }
    egui::Window::new("Symbios Overlands — Login")
        .collapsible(false)
        .resizable(false)
        .show(contexts.ctx_mut().unwrap(), |ui| {
            ui.label("Authenticate via your ATProto PDS (OAuth 2.0) to enter the overlands.");
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label("PDS:");
                ui.text_edit_singleline(&mut form.pds);
            });
            ui.horizontal(|ui| {
                ui.label("Relay Host:");
                ui.text_edit_singleline(&mut form.relay_host);
            });
            ui.horizontal(|ui| {
                ui.label("Destination DID (blank = Home):");
                ui.text_edit_singleline(&mut form.target_did);
            });

            ui.add_space(8.0);

            let redirecting = !begin_tasks.is_empty();
            let completing = !complete_tasks.is_empty();
            let mut begin_now = false;
            if !redirecting && !completing {
                if ui.button("Enter the Overlands").clicked() {
                    begin_now = true;
                }
                // Auto-submit when the URL/CLI supplied a destination DID.
                // Latched on `*autosubmitted` so we never double-fire even
                // if the form re-renders before the BeginAuthTask spawns.
                // Only `did` triggers this; `pds` / `relay` alone pre-fill
                // but leave the click to the user.
                //
                // On WASM, a persisted session resume is preferred: it
                // skips the OAuth redirect entirely and `check_wasm_resume`
                // already applies the URL `did=` override. Autosubmitting
                // on top would spawn two competing auth tasks; suppress
                // ourselves and let the resume path handle the link.
                #[cfg(target_arch = "wasm32")]
                let has_persisted = oauth::wasm::load_persisted().is_some();
                #[cfg(not(target_arch = "wasm32"))]
                let has_persisted = false;
                if !*autosubmitted
                    && !has_persisted
                    && let Some(b) = boot.as_deref()
                    && b.autosubmit
                {
                    begin_now = true;
                    *autosubmitted = true;
                }
                if !begin_now {
                    // Idle state — render nothing extra. The button above
                    // is the only affordance.
                }
            } else {
                ui.spinner();
                ui.label(if completing {
                    "Completing authentication…"
                } else {
                    "Opening your PDS authorization page…"
                });
            }
            if begin_now {
                commands.insert_resource(LoginError(None));
                let relay_host = form.relay_host.trim().to_string();
                let pds_url = form.pds.trim().to_string();
                let target_did = form.target_did.trim().to_string();
                let boot_pos = boot.as_deref().and_then(|b| b.target_pos);
                let boot_yaw = boot.as_deref().and_then(|b| b.target_yaw_deg);
                info!(
                    "OAuth begin: pds={} relay={} target_did={}",
                    pds_url,
                    relay_host,
                    if target_did.is_empty() {
                        "<home>"
                    } else {
                        target_did.as_str()
                    }
                );
                commands.insert_resource(RelayHost(relay_host.clone()));

                let client = oauth_client.0.clone();
                let pool = bevy::tasks::IoTaskPool::get();
                let task = pool.spawn(async move {
                    let fut = async move {
                        let (auth_url, mut pending) =
                            oauth::begin_authorization(&client, &pds_url, &relay_host, &target_did)
                                .await?;
                        // Carry the URL/CLI spawn pose across the OAuth
                        // redirect — the AS strips our query params, so
                        // this is the only path that survives.
                        pending.target_pos = boot_pos;
                        pending.target_yaw_deg = boot_yaw;
                        Ok::<_, String>((auth_url, pending))
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
                commands.spawn(BeginAuthTask(task));
            }

            if let Some(err) = &login_error.0 {
                ui.colored_label(egui::Color32::RED, err);
            }
        });
}

/// Drain finished [`BeginAuthTask`]s. On success either navigates the tab
/// (WASM) or launches the system browser (native); on failure surfaces the
/// error into [`LoginError`].
pub fn poll_begin_auth_task(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut BeginAuthTask)>,
    mut login_error: ResMut<LoginError>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        match result {
            Ok((auth_url, pending)) => {
                info!("OAuth authorize URL obtained; handing off to browser.");
                #[cfg(target_arch = "wasm32")]
                {
                    if let Err(e) = oauth::wasm::store_pending(&pending) {
                        let msg = format!("store pending auth: {e}");
                        warn!("{msg}");
                        login_error.0 = Some(msg);
                        continue;
                    }
                    oauth::wasm::navigate_to(&auth_url);
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    // Lift the random `state` parameter out of the
                    // pending auth blob and hand it to the loopback
                    // callback server so it can reject any request
                    // whose `state=` value doesn't match — without
                    // this, any other browser tab can brick the
                    // listener with a forged callback. The library
                    // always populates `app_state`; the explicit
                    // `unwrap_or_default()` keeps us defensive against
                    // a future library change that might omit it.
                    let expected_state = pending.auth_state.app_state.clone().unwrap_or_default();
                    match oauth::start_native_callback_server(expected_state) {
                        Ok(rx) => {
                            commands.insert_resource(oauth::NativeCallbackReceiver(
                                std::sync::Mutex::new(rx),
                            ));
                            commands.insert_resource(oauth::NativePendingAuthRes(
                                std::sync::Mutex::new(Some(pending)),
                            ));
                            if let Err(e) = webbrowser::open(&auth_url) {
                                let msg = format!("open browser: {e}");
                                warn!("{msg}");
                                login_error.0 = Some(msg);
                            }
                        }
                        Err(e) => {
                            let msg = format!("start callback server: {e}");
                            warn!("{msg}");
                            login_error.0 = Some(msg);
                        }
                    }
                }
                // `pending` is moved into storage above on both targets.
                let _ = &login_error;
            }
            Err(msg) => {
                warn!("begin_authorization: {msg}");
                login_error.0 = Some(msg);
            }
        }
    }
}

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
fn install_completed_session(
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

// ───────────────────────────────────────────────────────────────────────────
// Callback handoff
// ───────────────────────────────────────────────────────────────────────────

/// Spawn the async task that exchanges `code` for tokens, builds the
/// [`AtprotoSession`], and fetches the relay service token.
fn spawn_complete_task(
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

/// WASM-only: on first login-state frame, check the URL for `?code=&state=`
/// returned by the authorization server. If present, scrub the URL so a
/// reload cannot replay the single-use code, then kick off the exchange.
#[cfg(target_arch = "wasm32")]
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

/// Native-only: poll the loopback callback channel until a `(code, state)`
/// pair arrives, then drain the pending-auth resource and kick off the
/// exchange.
#[cfg(not(target_arch = "wasm32"))]
pub fn poll_native_callback(
    mut commands: Commands,
    receiver: Option<Res<oauth::NativeCallbackReceiver>>,
    pending_res: Option<Res<oauth::NativePendingAuthRes>>,
    oauth_client: Res<OauthClientRes>,
    complete_tasks: Query<&CompleteAuthTask>,
) {
    if !complete_tasks.is_empty() {
        return;
    }
    let Some(receiver) = receiver else {
        return;
    };
    let Some(pending_res) = pending_res else {
        return;
    };
    let code = {
        let guard = match receiver.0.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        match guard.try_recv() {
            Ok((code, _state)) => code,
            Err(_) => return,
        }
    };
    let pending = pending_res.0.lock().ok().and_then(|mut g| g.take());
    let Some(pending) = pending else {
        warn!("OAuth callback received but no pending auth stored");
        return;
    };
    commands.remove_resource::<oauth::NativeCallbackReceiver>();
    commands.remove_resource::<oauth::NativePendingAuthRes>();
    spawn_complete_task(&mut commands, oauth_client.0.clone(), pending, code);
}

// ───────────────────────────────────────────────────────────────────────────
// WASM: resume from localStorage
// ───────────────────────────────────────────────────────────────────────────

/// In-flight task that rebuilds an `AtprotoSession` from a persisted blob,
/// refreshes the access token if it's expired, and fetches a fresh service
/// token from the relay. Drained by [`poll_resume_task`]. Mirrors
/// [`CompleteAuthTask`]'s shape so the post-login installation step is
/// shared.
#[cfg(target_arch = "wasm32")]
#[derive(Component)]
pub struct ResumeAuthTask(bevy::tasks::Task<CompleteOutcome>);

/// One-shot system that fires on the first frame in `AppState::Login` and
/// kicks off a [`ResumeAuthTask`] if a valid persisted session is on disk.
/// A bad blob (deserialise failure) is silently dropped by `load_persisted`,
/// so the worst-case behaviour is "show the login form anyway."
#[cfg(target_arch = "wasm32")]
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
    commands.insert_resource(crate::state::RelayHost(blob.relay_host.clone()));
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
#[cfg(target_arch = "wasm32")]
fn spawn_resume_task(
    commands: &mut Commands,
    client: std::sync::Arc<OAuthClient>,
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
/// [`poll_complete_auth_task`]: insert session/refresh resources, transition
/// to `Loading`. On error, log + show the login form so the user can retry.
#[cfg(target_arch = "wasm32")]
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
