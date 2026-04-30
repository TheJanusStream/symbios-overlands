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
//!   `crate::oauth::NativePendingAuthRes` and [`poll_native_callback`]
//!   drains the channel.
//!
//! ## Sub-module map
//!
//! * [`begin`] — drains [`BeginAuthTask`]s and hands the resulting URL
//!   to the platform-specific browser-launch path.
//! * [`complete`] — drains [`CompleteAuthTask`]s, installs session
//!   resources, transitions to `Loading`. Also home to the shared
//!   `install_completed_session` + `spawn_complete_task` helpers.
//! * [`native_callback`] (native only) — polls the loopback callback
//!   channel and triggers the code exchange.
//! * `wasm_resume` (wasm only) — `?code=&state=` URL parser + persisted-
//!   session resume task + its drainer.

mod begin;
mod complete;
#[cfg(not(target_arch = "wasm32"))]
mod native_callback;
#[cfg(target_arch = "wasm32")]
mod wasm_resume;

pub use begin::poll_begin_auth_task;
pub use complete::poll_complete_auth_task;
#[cfg(not(target_arch = "wasm32"))]
pub use native_callback::poll_native_callback;
#[cfg(target_arch = "wasm32")]
pub use wasm_resume::{check_wasm_callback, check_wasm_resume, poll_resume_task};

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::boot_params::BootParams;
use crate::oauth::{self, OauthClientRes, PendingAuth};
use crate::state::RelayHost;

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
