//! Resume a saved agent session into the game's own login path (#1415).
//!
//! The browser build does the same thing on a reload
//! (`ui::login::wasm_resume`): rebuild the OAuth session from what was saved,
//! refresh it if it has expired, and mint the relay token a room needs. The
//! differences are where the session comes from - the agent's private file -
//! and where its rotations go: back into that file, through the session's
//! [`SessionFileSink`], the moment each one lands.

use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use bevy_symbios_multiuser::auth::AtprotoSession;
use proto_blue_oauth::client::dpop_key_from_jwk;
use proto_blue_oauth::{OAuthClient, OAuthSession};

use crate::oauth::{self, OauthRefreshCtx};
use crate::ui::login::{CompleteAuthTask, CompletedSession};

use super::super::session_file::{AgentSession, SessionFileSink};
use super::Identity;

/// Who the daemon plays as, and where it goes. Taken once, at startup.
#[derive(Resource)]
pub(super) struct PendingResume {
    pub identity: Identity,
    /// The world to enter: its owner's DID.
    pub room_did: String,
}

/// Startup: start signing in - resuming the saved session, or standing in
/// offline - as a [`CompleteAuthTask`] the game's own
/// `poll_complete_auth_task` installs. That takes the app from `Login`
/// through `Loading` into the world exactly as a person's sign-in does.
pub(super) fn begin_resume(
    mut commands: Commands,
    pending: Option<ResMut<PendingResume>>,
    client: Res<oauth::OauthClientRes>,
) {
    let Some(mut pending) = pending else {
        return;
    };
    let room_did = std::mem::take(&mut pending.room_did);
    let identity = std::mem::replace(&mut pending.identity, Identity::Offline);
    commands.remove_resource::<PendingResume>();
    let task = match identity {
        Identity::Saved {
            session_file,
            session,
        } => {
            info!(
                "Resuming the saved session for @{} ({})",
                session.handle, session.did
            );
            commands.insert_resource(crate::state::RelayHost(session.relay_host.clone()));
            let session = *session;
            let sink = Arc::new(SessionFileSink::new(session_file, session.clone()));
            let client = Arc::clone(&client.0);
            IoTaskPool::get().spawn(async move {
                crate::config::http::run_or(
                    resume(client, session, sink, room_did),
                    Err(crate::config::http::timed_out("agent session resume")),
                )
                .await
            })
        }
        Identity::Offline => {
            info!("Standing in offline: nobody will see the agent, and nothing is saved");
            commands.insert_resource(crate::state::RelayHost(
                crate::config::agent::OFFLINE_RELAY_HOST.to_owned(),
            ));
            IoTaskPool::get().spawn(async move { stand_in(room_did) })
        }
    };
    commands.spawn(CompleteAuthTask::resumed_elsewhere(task));
}

/// A sign-in to nothing: the offline agent's session, which reaches no PDS
/// and no relay.
fn stand_in(room_did: String) -> Result<CompletedSession, String> {
    use crate::config::agent::{OFFLINE_DID, OFFLINE_HANDLE};
    Ok(CompletedSession {
        session: oauth::stand_in::stand_in_session(OFFLINE_DID, OFFLINE_HANDLE)?,
        refresh_ctx: oauth::stand_in::stand_in_refresh_ctx()?,
        service_token: "offline".to_owned(),
        room_did,
        spawn_pos: None,
        spawn_yaw_deg: None,
    })
}

/// Rebuild the session, refresh it if it has expired, and mint the relay
/// token - the three things a reload does, against the agent's file.
async fn resume(
    client: Arc<OAuthClient>,
    saved: AgentSession,
    sink: Arc<SessionFileSink>,
    room_did: String,
) -> Result<CompletedSession, String> {
    let dpop_key =
        dpop_key_from_jwk(&saved.dpop_jwk).map_err(|e| format!("the saved DPoP key: {e}"))?;
    // The capped transport every other session uses (#1176); this one runs
    // for as long as the daemon does.
    let oauth_session = Arc::new(OAuthSession::with_fetch_handler(
        saved.token_set.clone(),
        dpop_key,
        client.dpop_nonces().clone(),
        Arc::new(oauth::capped_fetch::CappedFetcher::new()),
    ));
    let refresh_ctx = OauthRefreshCtx {
        client,
        server_metadata: saved.server_metadata.clone(),
        rotation_sink: Some(sink),
    };
    if oauth_session.is_expired_jittered() {
        oauth::refresh_session(&oauth_session, &refresh_ctx)
            .await
            .map_err(|e| refresh_refused(&e))?;
    }
    let session = AtprotoSession {
        did: saved.did,
        handle: saved.handle,
        pds_url: saved.pds_url,
        session: oauth_session,
    };
    let service_token = oauth::get_relay_service_auth(&session, &saved.relay_host)
        .await
        .map_err(|e| format!("minting a relay token: {e}"))?;
    Ok(CompletedSession {
        session,
        refresh_ctx,
        service_token,
        room_did,
        spawn_pos: None,
        spawn_yaw_deg: None,
    })
}

/// Say what a failed refresh means for the operator: a dead refresh token
/// needs a fresh sign-in, anything else may pass on its own.
fn refresh_refused(error: &str) -> String {
    if oauth::refresh_is_terminal(error) {
        format!(
            "the saved session is no longer accepted ({error}); sign the account in \
             again with `agent login`"
        )
    } else {
        format!("refreshing the saved session failed, possibly for now ({error})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dead_refresh_token_asks_for_a_fresh_sign_in() {
        let said = refresh_refused("refresh: OAuth server error: invalid_grant - revoked");
        assert!(said.contains("agent login"), "{said}");
    }

    #[test]
    fn a_transient_refresh_failure_does_not_ask_for_a_sign_in() {
        let said = refresh_refused("refresh: fetch error: operation timed out");
        assert!(!said.contains("agent login"), "{said}");
        assert!(said.contains("timed out"), "{said}");
    }
}
