use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::{
    AtprotoCredentials, AtprotoSession, create_session, get_service_auth,
};
use bevy_symbios_multiuser::prelude::*;
use bevy_symbios_multiuser::signaller::{TokenSource, TokenSourceRes};
use std::marker::PhantomData;

use crate::protocol::OverlandsMessage;
use crate::state::{AppState, RelayHost};

/// (session, service_token)
type LoginOutcome = Result<(AtprotoSession, String), bevy_symbios_multiuser::error::SymbiosError>;

#[derive(Component)]
pub struct AuthTask(bevy::tasks::Task<LoginOutcome>);

#[derive(Clone)]
pub struct LoginFormState {
    pds: String,
    handle: String,
    password: String,
    relay_host: String,
    error: Option<String>,
}

impl Default for LoginFormState {
    fn default() -> Self {
        Self {
            pds: crate::config::login::DEFAULT_PDS.into(),
            handle: crate::config::login::DEFAULT_HANDLE.into(),
            password: crate::config::login::DEFAULT_PASSWORD.into(),
            relay_host: crate::config::login::DEFAULT_RELAY_HOST.into(),
            error: None,
        }
    }
}

pub fn login_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut form: Local<LoginFormState>,
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

            ui.add_space(8.0);

            if tasks.is_empty() {
                if ui.button("Enter the Overlands").clicked() {
                    form.error = None;
                    let creds = AtprotoCredentials {
                        pds_url: form.pds.clone(),
                        identifier: form.handle.clone(),
                        password: form.password.clone(),
                    };
                    let relay_host = form.relay_host.trim().to_string();
                    let service_did = format!("did:web:{}", relay_host);
                    commands.insert_resource(RelayHost(relay_host));

                    let pool = bevy::tasks::AsyncComputeTaskPool::get();
                    let task = pool.spawn(async move {
                        tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .unwrap()
                            .block_on(async move {
                                let client = reqwest::Client::new();
                                let session = create_session(&client, &creds).await?;
                                let service_token = get_service_auth(
                                    &client,
                                    &session,
                                    &creds.pds_url,
                                    &service_did,
                                )
                                .await?;
                                Ok((session, service_token))
                            })
                    });
                    commands.spawn(AuthTask(task));
                }
            } else {
                ui.spinner();
                ui.label("Authenticating…");
            }

            if let Some(err) = &form.error {
                ui.colored_label(egui::Color32::RED, err);
            }
        });
}

pub fn poll_auth_task(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut AuthTask)>,
    mut next_state: ResMut<NextState<AppState>>,
    mut form: Local<LoginFormState>,
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
            Ok((session, service_token)) => {
                info!("Authenticated as {}", session.did);
                commands.insert_resource(session);

                let source: TokenSource =
                    std::sync::Arc::new(std::sync::RwLock::new(Some(service_token)));
                commands.insert_resource(TokenSourceRes(source));

                let host = relay_host.as_deref().map(|r| r.0.as_str()).unwrap_or("");
                commands.insert_resource(SymbiosMultiuserConfig::<OverlandsMessage> {
                    room_url: format!("wss://{}/overlands", host),
                    ice_servers: None,
                    _marker: PhantomData,
                });

                next_state.set(AppState::Loading);
            }
            Err(e) => {
                form.error = Some(e.to_string());
            }
        }
    }
}
