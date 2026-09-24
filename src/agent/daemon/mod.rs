//! `agent run` (#1415): the game's own client, headless, resumed from a saved
//! session and playing as the agent.
//!
//! The app is [`crate::build_client_app`] - the plugin list the game runs,
//! not a copy of it - hosted without a window: no winit, no sound, and a
//! primary window that exists only as data, because the UI and the gizmo
//! read one. The world camera still exists, because a person walks relative
//! to it and so will the agent, but it is parked - inactive - so the world
//! costs the renderer nothing until something asks to see it (`look`).
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

mod edit;
mod gifts;
mod look;
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

use crate::camera::IsWorldCamera;
use crate::state::{AppState, CurrentRoomDid};
use crate::ui::login::LoginError;
use crate::ui::reauth::SessionExpired;

use super::admin::Admin;
use super::control;
use super::session_file::AgentSession;

/// Who the daemon plays as.
pub enum Identity {
    /// The account a saved session signs in as.
    Saved {
        session_file: PathBuf,
        session: Box<AgentSession>,
    },
    /// Nobody: a stand-in signed in to nothing, alone in a world (#1415),
    /// as `did` - whose seeded world and body it takes (#1421).
    Offline { did: String },
}

impl Identity {
    /// The DID the daemon plays as.
    pub fn did(&self) -> &str {
        match self {
            Self::Saved { session, .. } => &session.did,
            Self::Offline { did } => did,
        }
    }
}

/// What `agent run` was asked to do.
pub struct RunRequest {
    pub identity: Identity,
    /// The world to enter, by its owner's DID; `None` is the agent's own.
    pub room_did: Option<String>,
    /// The one player whose chat the agent hears; `None` hears nobody.
    pub admin: Option<Admin>,
    /// Offline, fly the test airplane in place of the stand-in's own
    /// locomotion (#1431).
    pub wear_airplane: bool,
    /// Whether the agent may save its world and avatar to its account
    /// (#1422). It edits either way.
    pub allow_save: bool,
}

/// Build the headless client, resume the session into it, and run until the
/// app exits.
///
/// The control socket is bound before anything else, so a second daemon for
/// the same account is refused before it has built a world - the relay would
/// only have let one of them into a room.
pub fn run(request: RunRequest) -> Result<ExitCode, String> {
    let prefs_dir = agent_prefs_dir()?;
    let RunRequest {
        identity,
        room_did,
        admin,
        wear_airplane,
        allow_save,
    } = request;
    let edit_profile = edit::EditProfile {
        allow_save,
        offline: matches!(identity, Identity::Offline { .. }),
    };
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
    match admin {
        Some(admin) => {
            info!(
                "Hearing chat from the admin {} only; every other line is dropped unread",
                admin_name(&admin)
            );
            app.insert_resource(admin);
        }
        None => info!("No admin was named, so the agent hears no chat at all"),
    }
    if edit_profile.allow_save {
        info!("The agent may save its world and avatar to its account");
    } else {
        info!("The agent may edit, but not save: it was started without --allow-save");
    }
    if wear_airplane {
        info!("Testing: the stand-in flies the default airplane (#1431)");
        app.insert_resource(WearAirplane).add_systems(
            PreUpdate,
            wear_the_airplane.run_if(resource_exists::<WearAirplane>),
        );
    }
    app.insert_resource(resume::PendingResume { identity, room_did })
        .insert_resource(edit_profile)
        // No offer dialog nobody could answer: the agent answers offers
        // itself (#1423).
        .insert_resource(gifts::OffersAnsweredElsewhere)
        .init_resource::<gifts::OfferWatch>()
        .insert_resource(serve::ControlInbox::new(inbox))
        .insert_resource(observe::EventSink(events))
        .add_systems(Startup, resume::begin_resume)
        .add_systems(
            PreUpdate,
            (
                turn_off_ime,
                park_world_camera,
                movement::steer
                    .after(bevy::input::InputSystems)
                    .run_if(resource_exists::<movement::Movement>),
                movement::park
                    .after(bevy::input::InputSystems)
                    .after(movement::steer),
            ),
        )
        .add_systems(
            Update,
            (
                end_if_sign_in_failed,
                end_if_session_expired,
                serve::serve_requests,
                look::advance.after(serve::serve_requests),
                observe::record_peers,
                observe::record_chat,
                travel::record_travel,
                travel::withdraw_unanswered_guard,
                edit::record_saves,
                gifts::record_answers,
            ),
        )
        .add_systems(PostUpdate, gifts::sort_offers)
        .add_systems(OnEnter(AppState::Login), end_if_back_at_login)
        .add_systems(
            OnEnter(AppState::InGame),
            (announce_arrival, observe::record_arrival),
        );
    let exit = app.run();
    drop(socket);
    Ok(exit_code(exit))
}

/// Testing only (#1431): the offline stand-in flies the default airplane.
/// No seeded body is an airplane, and the preset exists for published
/// records, which an offline agent has none of.
#[derive(Resource)]
struct WearAirplane;

/// Put the default airplane on whatever body the stand-in has, as its
/// locomotion only - its body stays - wherever it goes: the game's own
/// hot-swap rebuilds the physics as it does for an edit in the avatar
/// editor. The stored record changes with it, so the swap is not an unsaved
/// edit for the travel guard to stop the agent over.
fn wear_the_airplane(
    live: Option<ResMut<crate::state::LiveAvatarRecord>>,
    stored: Option<ResMut<crate::state::StoredAvatarRecord>>,
) {
    use crate::pds::avatar::LocomotionPreset as _;
    use crate::pds::{AirplaneParams, LocomotionConfig};
    for mut record in [
        live.map(|r| r.map_unchanged(|r| &mut r.0)),
        stored.map(|r| r.map_unchanged(|r| &mut r.0)),
    ]
    .into_iter()
    .flatten()
    {
        // Read first: a `&mut` taken where nothing changes still marks it
        // changed, and a changed record is rebuilt.
        if !matches!(record.locomotion, LocomotionConfig::Airplane(_)) {
            record.locomotion = AirplaneParams::default().into_config();
        }
    }
}

/// `@handle (did)`, or the bare DID when the admin was named by one.
fn admin_name(admin: &Admin) -> String {
    match &admin.handle {
        Some(handle) => format!("@{handle} ({})", admin.did),
        None => admin.did.clone(),
    }
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

/// The world camera draws to a window with no surface, and Bevy still
/// prepares its view every frame the camera is active - visibility,
/// extraction, specialisation, the shadow cascades and the GPU's mesh
/// preprocessing - for a picture nobody will see. So it is parked the moment
/// it exists: its orbit still follows the body and still turns, which is
/// all a walk steers by, and a picture brings a camera of its own (#1420).
/// Measured idle, offline, 30 Hz: 25-28% of a core active against 20-21%
/// parked, and the GPU sampled busy at up to 6% against never; the first
/// picture then pays its pipelines once, 390 ms against 155 ms after.
fn park_world_camera(mut cameras: Query<&mut Camera, (IsWorldCamera, Added<Camera>)>) {
    for mut camera in &mut cameras {
        camera.is_active = false;
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

/// `value` to the hundredth - centimetres, for a length - as the f64 that
/// JSON prints. Rounding an f32 does not survive the trip: 0.15 as an f32
/// is 0.15000000596..., and JSON widens it and prints every digit (#1428).
/// Adding zero turns the `-0.0` a hair left of the axis rounds to into
/// `0.0`.
fn hundredths(value: f32) -> f64 {
    (f64::from(value) * 100.0).round() / 100.0 + 0.0
}

/// [`hundredths`] of each of a point's coordinates.
fn hundredths3(point: Vec3) -> [f64; 3] {
    [
        hundredths(point.x),
        hundredths(point.y),
        hundredths(point.z),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// THE CASE: `status` printed `facing: [-0.15000000596046448, ...]`
    /// under a comment promising centimetres. The control line is the old
    /// rounding, still printing the noise - so this test would have caught
    /// it.
    #[test]
    fn a_rounded_number_prints_the_way_it_reads() {
        let f32_rounded = ((-0.1501_f32) * 100.0).round() / 100.0;
        assert_ne!(
            serde_json::json!(f32_rounded).to_string(),
            "-0.15",
            "control: rounding in f32 prints the noise"
        );

        assert_eq!(serde_json::json!(hundredths(-0.1501)).to_string(), "-0.15");
        assert_eq!(
            serde_json::json!(hundredths(-0.001)).to_string(),
            "0.0",
            "right_m: -0.0 was seen live"
        );
        assert_eq!(
            serde_json::json!(hundredths3(Vec3::new(4.73, -104.9, 18.004))).to_string(),
            "[4.73,-104.9,18.0]"
        );
    }
}
