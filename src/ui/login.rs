//! Login form UI.
//!
//! Collects PDS endpoint, handle, app password, relay host, and an optional
//! destination DID, then spawns a single async task that calls
//! `create_session` followed by `get_service_auth` to exchange the session
//! for a service-bound JWT the relay will accept.  On success the task
//! installs the `AtprotoSession`, `TokenSourceRes`, `SymbiosMultiuserConfig`,
//! and `CurrentRoomDid` resources and transitions the app to `Loading`.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::{
    AtprotoCredentials, AtprotoSession, create_session, get_service_auth,
};
use bevy_symbios_multiuser::prelude::*;
use bevy_symbios_multiuser::signaller::{TokenSource, TokenSourceRes};
use std::marker::PhantomData;

use crate::protocol::OverlandsMessage;
use crate::state::{AppState, CurrentRoomDid, RelayHost};

/// (session, service_token)
///
/// The error arm is a pre-formatted `String` rather than `SymbiosError` so we
/// can control the exact wording on the way out — upstream already wraps
/// failures with a `"authentication failed: ..."` prefix via its `Display`
/// impl, and re-wrapping through `SymbiosError::AuthFailed` doubled it up
/// ("authentication failed: create_session: authentication failed: ...").
type LoginOutcome = Result<(AtprotoSession, String), String>;

#[derive(Component)]
pub struct AuthTask {
    task: bevy::tasks::Task<LoginOutcome>,
    target_did: Option<String>,
}

/// Latest login failure, shown underneath the login form.
///
/// Must be a Bevy `Resource` rather than a `Local` on either UI system —
/// `login_ui` and `poll_auth_task` have their own independent `Local`
/// instances, so previously `poll_auth_task` wrote the error into a state
/// the form rendering system never read, making every failure silent.
#[derive(Resource, Default)]
pub struct LoginError(pub Option<String>);

#[derive(Clone)]
pub struct LoginFormState {
    pds: String,
    handle: String,
    password: String,
    relay_host: String,
    target_did: String,
}

impl Default for LoginFormState {
    fn default() -> Self {
        Self {
            pds: crate::config::login::DEFAULT_PDS.into(),
            handle: crate::config::login::DEFAULT_HANDLE.into(),
            password: crate::config::login::DEFAULT_PASSWORD.into(),
            relay_host: crate::config::login::DEFAULT_RELAY_HOST.into(),
            target_did: crate::config::login::DEFAULT_TARGET_DID.into(),
        }
    }
}

pub fn login_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut form: Local<LoginFormState>,
    login_error: Res<LoginError>,
    tasks: Query<&AuthTask>,
) {
    egui::Window::new("Symbios Overlands — Login")
        .collapsible(false)
        .resizable(false)
        .show(contexts.ctx_mut().unwrap(), |ui| {
            ui.label("Authenticate via your ATProto PDS to enter the overlands.");
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label("PDS:");
                ui.text_edit_singleline(&mut form.pds);
            });
            ui.horizontal(|ui| {
                ui.label("Handle:");
                ui.text_edit_singleline(&mut form.handle);
            });
            ui.horizontal(|ui| {
                ui.label("App Password:");
                ui.add(egui::TextEdit::singleline(&mut form.password).password(true));
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

            if tasks.is_empty() {
                if ui.button("Enter the Overlands").clicked() {
                    commands.insert_resource(LoginError(None));
                    let creds = AtprotoCredentials {
                        pds_url: form.pds.clone(),
                        identifier: form.handle.clone(),
                        password: form.password.clone(),
                    };
                    info!(
                        "Login attempt: pds={} handle={} relay={} target_did={}",
                        creds.pds_url,
                        creds.identifier,
                        form.relay_host.trim(),
                        if form.target_did.trim().is_empty() {
                            "<home>"
                        } else {
                            form.target_did.trim()
                        }
                    );
                    let relay_host = form.relay_host.trim().to_string();
                    let service_did = format!("did:web:{}", relay_host);
                    commands.insert_resource(RelayHost(relay_host));

                    // Route the ATProto `create_session` + service-auth
                    // round-trip onto the IO pool — these are blocking HTTP
                    // calls that must not starve compute workers.
                    //
                    // Each step is labelled in its error so the user can see
                    // which leg of the handshake failed — `create_session`
                    // (bad handle/password/PDS URL) vs. `get_service_auth`
                    // (relay host unreachable or refusing the DID audience).
                    let pool = bevy::tasks::IoTaskPool::get();
                    let task = pool.spawn(async move {
                        let do_auth = async {
                            let client = crate::config::http::default_client();
                            let session = create_session(&client, &creds)
                                .await
                                .map_err(|e| format_auth_error("create_session", e))?;
                            let service_token =
                                get_service_auth(&client, &session, &creds.pds_url, &service_did)
                                    .await
                                    .map_err(|e| format_auth_error("get_service_auth", e))?;
                            Ok((session, service_token))
                        };
                        #[cfg(target_arch = "wasm32")]
                        {
                            do_auth.await
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()
                                .unwrap()
                                .block_on(do_auth)
                        }
                    });
                    let target_did = {
                        let t = form.target_did.trim();
                        if t.is_empty() {
                            None
                        } else {
                            Some(t.to_string())
                        }
                    };
                    commands.spawn(AuthTask { task, target_did });
                }
            } else {
                ui.spinner();
                ui.label("Authenticating…");
            }

            if let Some(err) = &login_error.0 {
                ui.colored_label(egui::Color32::RED, err);
            }
        });
}

/// Render a `SymbiosError` from `create_session` / `get_service_auth` into a
/// user-facing message. Strips the redundant `"authentication failed: "`
/// prefix the upstream `Display` impl adds (so we don't end up with
/// `"authentication failed: create_session: authentication failed: ..."`)
/// and promotes HTTP 429 into a dedicated rate-limit message — users
/// hitting this almost always think it's their password or the relay, not a
/// server-side throttle with its own cooldown clock.
fn format_auth_error(step: &str, err: bevy_symbios_multiuser::error::SymbiosError) -> String {
    let inner = err.to_string();
    let inner = inner
        .strip_prefix("authentication failed: ")
        .unwrap_or(&inner);

    if inner.contains("429") || inner.contains("RateLimitExceeded") {
        format!(
            "{step} — the PDS is rate-limiting this IP/account (HTTP 429). Wait a few minutes \
             before retrying; repeated attempts will extend the cooldown. Raw: {inner}"
        )
    } else {
        format!("{step}: {inner}")
    }
}

pub fn poll_auth_task(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut AuthTask)>,
    mut next_state: ResMut<NextState<AppState>>,
    mut login_error: ResMut<LoginError>,
    relay_host: Option<Res<RelayHost>>,
) {
    for (entity, mut auth) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut auth.task))
        else {
            continue;
        };

        let target_did = auth.target_did.clone();
        commands.entity(entity).despawn();

        match result {
            Ok((session, service_token)) => {
                info!("Authenticated as {}", session.did);

                let room_did = target_did.unwrap_or_else(|| session.did.clone());
                commands.insert_resource(CurrentRoomDid(room_did.clone()));

                commands.insert_resource(session);

                let source: TokenSource =
                    std::sync::Arc::new(std::sync::RwLock::new(Some(service_token)));
                commands.insert_resource(TokenSourceRes(source));

                let host = relay_host.as_deref().map(|r| r.0.as_str()).unwrap_or("");
                commands.insert_resource(SymbiosMultiuserConfig::<OverlandsMessage> {
                    room_url: format!("wss://{}/overlands/{}", host, room_did),
                    ice_servers: None,
                    _marker: PhantomData,
                });

                next_state.set(AppState::Loading);
            }
            Err(msg) => {
                // Show the specific failure in the form (red banner) *and*
                // surface it via `warn!` so terminal users also see it —
                // silent failures previously made misconfigured PDS URLs,
                // app passwords, and relay hosts all look identical.
                warn!("Login failed: {msg}");
                login_error.0 = Some(msg);
            }
        }
    }
}
