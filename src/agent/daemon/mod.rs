//! `agent run` (#1415): the game's own client, headless, resumed from a saved
//! session and playing as the agent.
//!
//! The app is [`crate::build_client_app`] - the plugin list the game runs,
//! not a copy of it - hosted without a window: no winit, no sound, and a
//! primary window that exists only as data, because the UI and the gizmo
//! read one. Nothing renders to it, so the world costs the GPU nothing until
//! something asks to see it. The world camera still exists, because a person
//! walks relative to it and so will the agent.
//!
//! Three things differ from a person's session, and each is small:
//!
//! * **Sign-in** is the saved session, handed to the game's own login
//!   installer as a finished sign-in ([`resume`]), so the agent enters its
//!   world by the path a person's login takes.
//! * **Settings** live in the agent's own directory, never in the files a
//!   person signed in on this machine uses.
//! * **Ending**: the process exits and never logs out. The game's logout
//!   REVOKES the session, and the saved one has to outlive the process. A
//!   session the server refused, one that expires in play, or anything that
//!   takes the client back to its login screen ends the process with an
//!   error saying so.

mod movement;
mod observe;
mod resume;
mod serve;
mod speech;
mod status;
mod travel;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, mpsc};

use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowResolution};
use bevy_egui::EguiContextSettings;

use crate::state::{AppState, CurrentRoomDid};
use crate::ui::login::LoginError;
use crate::ui::reauth::SessionExpired;

use super::control;
use super::session_file::AgentSession;

/// Who the daemon plays as.
pub enum Identity {
    /// The account a saved session signs in as.
    Saved {
        session_file: PathBuf,
        session: Box<AgentSession>,
    },
    /// Nobody: a stand-in signed in to nothing, alone in a world (#1415).
    Offline,
}

impl Identity {
    /// The DID the daemon plays as.
    pub fn did(&self) -> &str {
        match self {
            Self::Saved { session, .. } => &session.did,
            Self::Offline => crate::config::agent::OFFLINE_DID,
        }
    }
}

/// What `agent run` was asked to do.
pub struct RunRequest {
    pub identity: Identity,
    /// The world to enter, by its owner's DID; `None` is the agent's own.
    pub room_did: Option<String>,
}

/// Build the headless client, resume the session into it, and run until the
/// app exits.
///
/// The control socket is bound before anything else, so a second daemon for
/// the same account is refused before it has built a world - the relay would
/// only have let one of them into a room.
pub fn run(request: RunRequest) -> Result<ExitCode, String> {
    let prefs_dir = agent_prefs_dir()?;
    let RunRequest { identity, room_did } = request;
    let room_did = room_did.unwrap_or_else(|| identity.did().to_owned());
    let events = Arc::new(control::events::EventLog::new(
        crate::config::agent::EVENT_CAPACITY,
        instance_name(),
    ));
    let (requests, inbox) = mpsc::channel();
    let socket_path = control::socket_path(identity.did())?;
    let socket = control::server::listen(&socket_path, requests, Arc::clone(&events))?;
    info!("Listening for commands on {}", socket_path.display());

    let mut app = App::new();
    // Before the builder: `PrefsPlugin` only initialises its store, so this
    // one stands, and the agent's settings never land in a person's files.
    app.insert_resource(crate::prefs::PrefsStore::Dir(prefs_dir));
    crate::build_client_app(
        &mut app,
        crate::boot_params::BootParams::default(),
        crate::ClientShell::Headless {
            frame: crate::config::agent::FRAME,
            compute_threads: crate::config::agent::COMPUTE_THREADS,
        },
    );
    spawn_primary_window(app.world_mut());
    app.insert_resource(resume::PendingResume { identity, room_did })
        .insert_resource(serve::ControlInbox::new(inbox))
        .insert_resource(observe::EventSink(events))
        .add_systems(Startup, resume::begin_resume)
        .add_systems(
            PreUpdate,
            (
                turn_off_ime,
                movement::steer
                    .after(bevy::input::InputSystems)
                    .run_if(resource_exists::<movement::Movement>),
            ),
        )
        .add_systems(
            Update,
            (
                end_if_sign_in_failed,
                end_if_session_expired,
                serve::serve_requests,
                observe::record_peers,
                observe::record_chat,
                travel::record_travel,
                travel::withdraw_unanswered_guard,
            ),
        )
        .add_systems(OnEnter(AppState::Login), end_if_back_at_login)
        .add_systems(
            OnEnter(AppState::InGame),
            (announce_arrival, observe::record_arrival),
        );
    let exit = app.run();
    drop(socket);
    Ok(exit_code(exit))
}

/// A name for this run of the daemon, so a client can tell its event numbers
/// from an earlier run's: the process id and the start time.
fn instance_name() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    )
}

/// `<config dir>/agent/prefs/`: the daemon's own settings.
fn agent_prefs_dir() -> Result<PathBuf, String> {
    use crate::config::agent::{HOME_DIR, PREFS_DIR};
    let config = crate::prefs::config_dir()
        .ok_or("there is no config directory for the agent's settings; set HOME")?;
    Ok(config.join(HOME_DIR).join(PREFS_DIR))
}

/// The primary window the UI and the gizmo read, as the render tool's
/// editor host spawns one: a size and a scale factor, with no winit behind
/// it and no surface, so nothing is ever drawn to it.
fn spawn_primary_window(world: &mut World) {
    let (width, height) = crate::config::agent::VIEWPORT;
    world.spawn((
        Window {
            resolution: WindowResolution::new(width, height).with_scale_factor_override(1.0),
            focused: true,
            ..default()
        },
        PrimaryWindow,
    ));
}

/// bevy_egui asks winit to place an input-method box for a focused text
/// field every frame, and warns every frame that there is no winit window to
/// ask - sixty lines a second in the daemon's log. The daemon has neither a
/// keyboard nor an input method, so it turns the request off on the one
/// context there is, as soon as that context exists.
fn turn_off_ime(mut contexts: Query<&mut EguiContextSettings, Added<EguiContextSettings>>) {
    for mut settings in &mut contexts {
        settings.enable_ime = false;
    }
}

/// The resume was refused, or never answered: say why and stop, rather than
/// idle on a login screen nobody can see.
fn end_if_sign_in_failed(login_error: Res<LoginError>, mut exit: MessageWriter<AppExit>) {
    if let Some(message) = login_error.0.as_deref() {
        error!("The agent could not sign in: {message}");
        exit.write(AppExit::error());
    }
}

/// The session died in play - the refresh token was refused. A person gets
/// a sign-in-again dialog; the agent's operator gets an exit that says to
/// run `agent login`.
fn end_if_session_expired(expired: Option<Res<SessionExpired>>, mut exit: MessageWriter<AppExit>) {
    if expired.is_some() {
        error!(
            "The agent's session expired and cannot be refreshed; sign the account in \
             again with `agent login`"
        );
        exit.write(AppExit::error());
    }
}

/// The client starts on its login screen; coming back to it means the agent
/// has left the world, which only a logout or an aborted load does - and
/// neither is something a running agent should outlive.
fn end_if_back_at_login(mut entries: Local<u32>, mut exit: MessageWriter<AppExit>) {
    *entries += 1;
    if *entries > 1 {
        error!("The agent left the world for the login screen; stopping");
        exit.write(AppExit::error());
    }
}

fn announce_arrival(room: Option<Res<CurrentRoomDid>>) {
    if let Some(room) = room {
        info!("The agent is in the world of {}", room.0);
    }
}

fn exit_code(exit: AppExit) -> ExitCode {
    match exit {
        AppExit::Success => ExitCode::SUCCESS,
        AppExit::Error(code) => ExitCode::from(code.get()),
    }
}
