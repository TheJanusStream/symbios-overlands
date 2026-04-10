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
type LoginOutcome = Result<(AtprotoSession, String), bevy_symbios_multiuser::error::SymbiosError>;

#[derive(Component)]
pub struct AuthTask {
    task: bevy::tasks::Task<LoginOutcome>,
    target_did: Option<String>,
}

#[derive(Clone)]
pub struct LoginFormState {
    pds: String,
    handle: String,
    password: String,
    relay_host: String,
    target_did: String,
    error: Option<String>,
}

impl Default for LoginFormState {
    fn default() -> Self {
        Self {
            pds: crate::config::login::DEFAULT_PDS.into(),
            handle: crate::config::login::DEFAULT_HANDLE.into(),
            password: crate::config::login::DEFAULT_PASSWORD.into(),
            relay_host: crate::config::login::DEFAULT_RELAY_HOST.into(),
            target_did: String::new(),
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
            ui.horizontal(|ui| {
                ui.label("Destination DID (blank = Home):");
                ui.text_edit_singleline(&mut form.target_did);
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

                    // Route the ATProto `create_session` + service-auth
                    // round-trip onto the IO pool — these are blocking HTTP
                    // calls that must not starve compute workers.
                    let pool = bevy::tasks::IoTaskPool::get();
                    let task = pool.spawn(async move {
                        let do_auth = async {
                            let client = reqwest::Client::new();
                            let session = create_session(&client, &creds).await?;
                            let service_token =
                                get_service_auth(&client, &session, &creds.pds_url, &service_did)
                                    .await?;
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
            Err(e) => {
                form.error = Some(e.to_string());
            }
        }
    }
}
