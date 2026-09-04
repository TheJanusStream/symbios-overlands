//! Signing in again without losing the world you just built (#1214 f397).
//!
//! When the OAuth refresh token dies mid-session — revoked, rotated, or
//! simply old enough — every authenticated write fails identically and for
//! good. Before this module the owner's only two doors were a Save that
//! could never succeed and a "Discard & log out" that threw the session's
//! edits away, because the Live records are memory-only and nothing
//! persists a draft. An hour of placing things, against a button that
//! cannot work.
//!
//! The third door is here: re-run the same authorization flow `ui::login`
//! drives, and install its result **in place** — a new `AtprotoSession`,
//! `OauthRefreshCtx` and service token, with `AppState` left in `InGame`
//! and every Live record untouched.
//!
//! ## The DID guard is the load-bearing part
//!
//! The Live records in memory belong to the DID that was signed in when
//! they were edited. Installing a *different* account's session over them
//! would point the next Save at somebody else's repo — the same class of
//! defect the record-integrity tranche exists to prevent, arrived at from a
//! recovery path. So [`install_reauthenticated_session`] refuses any
//! completion whose DID does not match, keeps the expired state up, and
//! says which account is required.
//!
//! ## What differs on wasm, and why the copy differs with it
//!
//! Authorization is a full-page redirect on the web build
//! (`oauth::wasm::navigate_to`), so the tab — and with it every unsaved
//! edit — is gone the moment the flow starts. The in-place install is
//! therefore native-only in effect, and the modal says so rather than
//! offering a recovery it cannot deliver. Persisting the dirty records
//! across that redirect would fix it and is a larger piece of work than
//! this door: see the note left on #1214.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::signaller::TokenSourceRes;

use crate::pds::{AvatarRecord, InventoryRecord, RoomRecord};
use crate::state::{AppState, CurrentRoomDid, PublishFeedback, PublishStatus, RelayHost};

/// The session cannot write any more, and the owner has unsaved work.
///
/// Inserted by [`detect_expired_session`] off the terminal publish status
/// `report_publish_failure` records (see
/// [`crate::oauth::refresh_is_terminal`]), removed when a matching account
/// signs back in. Session-scoped: `logout::session_scoped_resources!` drops
/// it, so a fresh login never opens under the previous session's banner.
#[derive(Resource, Debug, Clone)]
pub struct SessionExpired {
    /// The DID the expired session held. A re-auth MUST land on this one —
    /// the Live records in memory are its records.
    pub did: String,
    /// That DID's handle, for the copy: "@alice" is what the owner knows.
    pub handle: String,
    /// `Time::elapsed_secs_f64` when the expiry was first observed.
    pub at_secs: f64,
    /// The owner closed the modal. The state stands (Save is still
    /// refused, and the account chip still offers the door) — this only
    /// stops the dialog re-taking the screen on every frame.
    pub dismissed: bool,
    /// Why the last attempt did not settle it, if one did not: chiefly a
    /// re-auth that landed on a different account.
    pub notice: Option<String>,
}

/// Raise [`SessionExpired`] as soon as any record's last write was terminal.
///
/// Derived from the three publish statuses rather than pushed from
/// `report_publish_failure`, so the trigger cannot drift per record: the
/// fact is about the *session*, and whichever editor happened to be the one
/// that tried to save is an accident of timing.
pub fn detect_expired_session(
    mut commands: Commands,
    expired: Option<Res<SessionExpired>>,
    session: Option<Res<AtprotoSession>>,
    room: Res<PublishFeedback<RoomRecord>>,
    avatar: Res<PublishFeedback<AvatarRecord>>,
    inventory: Res<PublishFeedback<InventoryRecord>>,
    time: Res<Time>,
) {
    if expired.is_some() {
        return;
    }
    let Some(session) = session else {
        return;
    };
    let terminal = [&room.status, &avatar.status, &inventory.status]
        .into_iter()
        .any(|s| matches!(s, PublishStatus::Failed { terminal: true, .. }));
    if !terminal {
        return;
    }
    commands.insert_resource(SessionExpired {
        did: session.did.clone(),
        handle: session.handle.clone(),
        at_secs: time.elapsed_secs_f64(),
        dismissed: false,
        notice: None,
    });
}

/// Is an authorization round-trip in flight right now? Both halves of the
/// flow spawn a task entity, and the modal has nothing to offer while
/// either is outstanding.
#[derive(bevy::ecs::system::SystemParam)]
pub struct ReauthInFlight<'w, 's> {
    begin: Query<'w, 's, (), With<crate::ui::login::BeginAuthTask>>,
    complete: Query<'w, 's, (), With<crate::ui::login::CompleteAuthTask>>,
}

impl ReauthInFlight<'_, '_> {
    fn any(&self) -> bool {
        !self.begin.is_empty() || !self.complete.is_empty()
    }
}

/// The one sentence naming what the browser build cannot preserve.
///
/// Kept beside the flow rather than at the button, so the modal body and
/// the confirm label cannot end up describing different outcomes.
pub fn redirect_cost() -> Option<&'static str> {
    if cfg!(target_arch = "wasm32") {
        Some(
            "Signing in reloads this page, and unsaved edits are lost with it. \
             Nothing else on this screen can save them either — the session that \
             would have written them is gone.",
        )
    } else {
        None
    }
}

/// The modal offered while the session is expired.
///
/// Rendered in `EguiPrimaryContextPass` under `AppState::InGame` while
/// [`SessionExpired`] exists and has not been dismissed.
#[allow(clippy::too_many_arguments)]
pub fn reauth_modal(
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut expired: ResMut<SessionExpired>,
    session: Option<Res<AtprotoSession>>,
    relay_host: Option<Res<RelayHost>>,
    room_did: Option<Res<CurrentRoomDid>>,
    oauth_client: Option<Res<crate::oauth::OauthClientRes>>,
    in_flight: ReauthInFlight,
) {
    if expired.dismissed {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let waiting = in_flight.any();
    let mut begin = false;
    let mut dismiss = false;

    crate::ui::confirm::note_modal_open(ctx);
    let modal = egui::Modal::new(egui::Id::new("session-expired")).show(ctx, |ui| {
        let th = crate::ui::theme::current(ui.ctx());
        ui.heading("Your session has expired");
        ui.add_space(4.0);
        ui.label(format!(
            "Your data server stopped accepting @{}'s sign-in, so nothing can \
             be saved until you sign in again.",
            expired.handle
        ));
        if let Some(cost) = redirect_cost() {
            ui.add_space(4.0);
            ui.colored_label(th.status.warn, cost);
        } else {
            ui.add_space(4.0);
            ui.label("Your unsaved edits stay exactly as they are while you do.");
        }
        if let Some(notice) = &expired.notice {
            ui.add_space(4.0);
            ui.colored_label(th.status.error, notice);
        }
        ui.add_space(8.0);
        if waiting {
            ui.colored_label(th.status.info, "Waiting for your browser…");
            return;
        }
        ui.horizontal(|ui| {
            let label = if redirect_cost().is_some() {
                "Sign in again (reloads the page)"
            } else {
                "Sign in again"
            };
            begin = ui
                .button(label)
                .on_hover_text(format!(
                    "Opens your browser to sign in as @{} again",
                    expired.handle
                ))
                .clicked();
            dismiss = ui
                .button("Not now")
                .on_hover_text(
                    "Closes this dialog. Saving stays disabled, and the account \
                     menu still offers to sign in again.",
                )
                .clicked();
        });
    });

    // Esc / backdrop click is "Not now" — the shared modal contract
    // (#1236 f53). While the browser leg is in flight there is no button
    // to be equivalent TO, so the dialog holds; it says "Waiting for your
    // browser…" and that is the state it is describing.
    if modal.should_close() && !waiting && !begin {
        dismiss = true;
    }
    if dismiss {
        expired.dismissed = true;
        expired.notice = None;
    }
    if begin
        && let (Some(session), Some(relay), Some(client)) =
            (session.as_deref(), relay_host.as_deref(), oauth_client)
    {
        expired.notice = None;
        let target_did = room_did.map(|r| r.0.clone()).unwrap_or_default();
        crate::ui::login::spawn_begin_auth_task(
            &mut commands,
            client.0.clone(),
            session.pds_url.clone(),
            relay.0.clone(),
            target_did,
        );
    }
}

/// Everything a completed re-authorization replaces, as one param bundle.
///
/// The three publish statuses ride along because a terminal `Failed` is
/// what disables Save: leaving them behind would sign the owner back in and
/// keep refusing the save they signed in to make.
#[derive(bevy::ecs::system::SystemParam)]
pub struct ReauthSinks<'w> {
    pub token_source: Option<Res<'w, TokenSourceRes>>,
    pub room: ResMut<'w, PublishFeedback<RoomRecord>>,
    pub avatar: ResMut<'w, PublishFeedback<AvatarRecord>>,
    pub inventory: ResMut<'w, PublishFeedback<InventoryRecord>>,
}

/// Does this completion belong to the session that expired?
///
/// Split out and pure because it is the one property that must not be got
/// wrong: a mismatch installed anyway would leave the previous owner's
/// world, body and stash in memory pointed at a stranger's repo, and the
/// very next Save would write them there.
pub fn reauth_refusal(
    expected: &SessionExpired,
    arrived_did: &str,
    arrived_handle: &str,
) -> Option<String> {
    if expected.did == arrived_did {
        return None;
    }
    Some(format!(
        "That signed you in as @{arrived_handle}, but the unsaved work here \
         belongs to @{}. Sign in as @{} to save it.",
        expected.handle, expected.handle
    ))
}

/// Retire a refusal that was about a session which no longer exists.
///
/// `Idle`, never `Success`: no write landed, and the record is still dirty.
/// A transient failure is left alone — it is still this session's news, and
/// the owner has not seen it acknowledged.
fn clear_terminal(status: &mut PublishStatus) -> bool {
    if matches!(status, PublishStatus::Failed { terminal: true, .. }) {
        *status = PublishStatus::Idle;
        return true;
    }
    false
}

/// Install a re-authorized session over the expired one, in place.
///
/// Deliberately narrower than `login::complete::install_completed_session`:
/// it must NOT set `AppState`, insert `CurrentRoomDid`, or re-point
/// `SymbiosMultiuserConfig` — the last of those is what a portal hop does,
/// and it would tear the socket down and sweep the room's peers for no
/// reason. Nor does it insert a fresh `TokenSourceRes`: the live signaller
/// holds a clone of the one that was threaded into it at connect time, so a
/// replacement resource would leave the socket reading the dead token
/// forever. The token is *set* through the existing source, exactly as the
/// periodic refresh does.
///
/// Returns the refusal when the account does not match, so the caller can
/// put it back on the modal.
pub fn install_reauthenticated_session(
    commands: &mut Commands,
    expired: &SessionExpired,
    completed: crate::ui::login::CompletedSession,
    sinks: ReauthSinks<'_>,
) -> Option<String> {
    let crate::ui::login::CompletedSession {
        session,
        refresh_ctx,
        service_token,
        ..
    } = completed;
    if let Some(refusal) = reauth_refusal(expired, &session.did, &session.handle) {
        return Some(refusal);
    }
    info!(
        "Re-authenticated in place as {} ({})",
        session.handle, session.did
    );
    if let Some(ts) = sinks.token_source.as_deref() {
        ts.0.set(Some(service_token));
    }
    commands.insert_resource(session);
    commands.insert_resource(refresh_ctx);
    // The refusal that disabled Save was about a session that no longer
    // exists. Clearing to `Idle` rather than `Success` is deliberate: no
    // write landed, and the records are still dirty.
    let ReauthSinks {
        mut room,
        mut avatar,
        mut inventory,
        ..
    } = sinks;
    for status in [&mut room.status, &mut avatar.status, &mut inventory.status] {
        clear_terminal(status);
    }
    commands.remove_resource::<SessionExpired>();
    None
}

/// Run condition for the login pipeline's poll systems: the login screen,
/// or an in-game re-authentication (#1214).
pub fn login_flow_is_running(
    state: Res<State<AppState>>,
    expired: Option<Res<SessionExpired>>,
) -> bool {
    *state.get() == AppState::Login || expired.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expired() -> SessionExpired {
        SessionExpired {
            did: String::from("did:plc:alice"),
            handle: String::from("alice"),
            at_secs: 1.0,
            dismissed: false,
            notice: None,
        }
    }

    /// THE SEQUENCE: the owner's session expires with an hour of unsaved
    /// world in memory, they click "Sign in again", and the browser signs
    /// them in as their OTHER account. Installing that session would leave
    /// @alice's world, body and stash in memory pointed at @bob's repo, and
    /// the next Save — the very thing they signed in to do — would write
    /// them there.
    #[test]
    fn a_re_auth_as_a_different_account_is_refused_by_name() {
        let expected = expired();
        assert_eq!(reauth_refusal(&expected, "did:plc:alice", "alice"), None);

        let refusal = reauth_refusal(&expected, "did:plc:bob", "bob")
            .expect("a different DID must never be installed over these records");
        assert!(refusal.contains("@bob"), "{refusal}");
        assert!(refusal.contains("@alice"), "{refusal}");

        // A handle change on the SAME account is not a different account —
        // handles are mutable on atproto and the DID is the identity.
        assert_eq!(
            reauth_refusal(&expected, "did:plc:alice", "alice.example.com"),
            None,
        );
    }

    /// The login pipeline's poll systems used to be gated on the login
    /// screen alone, which is why there was no way to run the flow from
    /// in-game at all. The condition has to open for an expired session and
    /// close again the moment it is settled.
    #[test]
    fn the_login_pipeline_runs_for_an_in_game_re_auth() {
        fn probe(state: AppState, expired: Option<SessionExpired>) -> bool {
            let mut world = World::new();
            world.insert_resource(State::new(state));
            if let Some(e) = expired {
                world.insert_resource(e);
            }
            world
                .run_system_once(login_flow_is_running)
                .expect("the condition runs")
        }
        use bevy::ecs::system::RunSystemOnce;

        assert!(probe(AppState::Login, None), "the login screen, as before");
        assert!(
            !probe(AppState::InGame, None),
            "an ordinary session must not have the login pipeline running"
        );
        assert!(
            probe(AppState::InGame, Some(expired())),
            "an expired session is the whole point"
        );
    }

    /// THE SEQUENCE: the owner signs back in as the right account. Their
    /// Save button is disabled by a `Failed { terminal: true }` recorded
    /// against a session that no longer exists — leaving it would sign them
    /// in and then keep refusing the save they signed in to make.
    #[test]
    fn a_successful_re_auth_retires_the_refusal_it_was_about() {
        let mut terminal = PublishStatus::Failed {
            at_secs: 4.0,
            message: String::from("Your session has expired."),
            terminal: true,
        };
        assert!(clear_terminal(&mut terminal));
        assert!(
            matches!(terminal, PublishStatus::Idle),
            "Idle, not Success — no write landed and the record is still dirty"
        );

        // A transient failure is this session's news and stays on screen.
        let mut transient = PublishStatus::Failed {
            at_secs: 4.0,
            message: String::from("502 Bad Gateway"),
            terminal: false,
        };
        assert!(!clear_terminal(&mut transient));
        assert!(matches!(transient, PublishStatus::Failed { .. }));

        let mut publishing = PublishStatus::Publishing { since_secs: 1.0 };
        assert!(!clear_terminal(&mut publishing));
    }

    /// THE SEQUENCE: the owner presses Ctrl+S in the Avatar editor, the
    /// refresh token is dead, and the failure lands against the AVATAR
    /// record. The fact is about the session, so the door has to open
    /// whichever editor happened to be the one that tried.
    #[test]
    fn any_record_s_terminal_failure_raises_the_door() {
        use bevy::ecs::system::RunSystemOnce;

        fn app_with(status: PublishStatus, which: usize) -> App {
            let mut app = App::new();
            app.add_plugins(bevy::MinimalPlugins);
            app.insert_resource(AtprotoSession {
                did: String::from("did:plc:alice"),
                handle: String::from("alice"),
                pds_url: String::from("https://pds.example"),
                // Never used here — the detector reads only `did` and
                // `handle` — but `AtprotoSession` owns a live signing
                // session, so the fixture builds a real (unused) one.
                session: std::sync::Arc::new(proto_blue_oauth::session::OAuthSession::new(
                    proto_blue_oauth::types::TokenSet {
                        issuer: String::from("https://as.example"),
                        sub: String::from("did:plc:alice"),
                        scope: String::from("atproto"),
                        access_token: String::from("access"),
                        refresh_token: Some(String::from("refresh")),
                        token_type: String::from("DPoP"),
                        expires_at: Some(String::from("2099-01-01T00:00:00Z")),
                        aud: Some(String::from("https://as.example")),
                    },
                    proto_blue_oauth::DpopKey::generate().expect("DPoP key"),
                    proto_blue_oauth::DpopNonceCache::new(),
                )),
            });
            app.init_resource::<PublishFeedback<RoomRecord>>();
            app.init_resource::<PublishFeedback<AvatarRecord>>();
            app.init_resource::<PublishFeedback<InventoryRecord>>();
            match which {
                0 => {
                    app.world_mut()
                        .resource_mut::<PublishFeedback<RoomRecord>>()
                        .status = status
                }
                1 => {
                    app.world_mut()
                        .resource_mut::<PublishFeedback<AvatarRecord>>()
                        .status = status
                }
                _ => {
                    app.world_mut()
                        .resource_mut::<PublishFeedback<InventoryRecord>>()
                        .status = status
                }
            }
            app
        }

        for which in 0..3 {
            let mut app = app_with(
                PublishStatus::Failed {
                    at_secs: 1.0,
                    message: String::from("expired"),
                    terminal: true,
                },
                which,
            );
            app.world_mut()
                .run_system_once(detect_expired_session)
                .expect("detector runs");
            let expired = app
                .world()
                .get_resource::<SessionExpired>()
                .expect("record {which} must raise the door");
            assert_eq!(expired.did, "did:plc:alice");
            assert_eq!(expired.handle, "alice");
        }

        // A transient failure must NOT raise it: disabling Save and
        // demanding a re-login over a 502 would be worse than the bug.
        let mut app = app_with(
            PublishStatus::Failed {
                at_secs: 1.0,
                message: String::from("502 Bad Gateway"),
                terminal: false,
            },
            0,
        );
        app.world_mut()
            .run_system_once(detect_expired_session)
            .expect("detector runs");
        assert!(app.world().get_resource::<SessionExpired>().is_none());
    }

    /// The web build cannot keep the page across an authorization redirect,
    /// so the modal must not promise that unsaved edits survive it. One
    /// sentence, read by both the body and the button label.
    #[test]
    fn the_web_build_says_what_the_redirect_costs() {
        let cost = redirect_cost();
        if cfg!(target_arch = "wasm32") {
            assert!(cost.expect("wasm names the cost").contains("lost"));
        } else {
            assert!(
                cost.is_none(),
                "native keeps the process, so there is no cost to name"
            );
        }
    }
}
