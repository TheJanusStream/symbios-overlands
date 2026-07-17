//! Native-only callback handoff: poll the loopback
//! [`NativeCallbackReceiver`](crate::oauth::NativeCallbackReceiver)
//! channel, drain the matching
//! [`NativePendingAuthRes`](crate::oauth::NativePendingAuthRes), and spawn
//! the exchange task — or surface the authorization server's error
//! redirect (user denied, expired request, …) into [`LoginError`].

use bevy::prelude::*;

use crate::oauth::{self, OauthClientRes};

use super::complete::spawn_complete_task;
use super::{CompleteAuthTask, LoginError};

/// Native-only: poll the loopback callback channel until an outcome
/// arrives. On a code, drain the pending-auth resource and kick off the
/// exchange; on an AS error redirect (or a lost pending blob), surface a
/// plain-language [`LoginError`] instead of waiting forever.
pub fn poll_native_callback(
    mut commands: Commands,
    receiver: Option<Res<oauth::NativeCallbackReceiver>>,
    pending_res: Option<Res<oauth::NativePendingAuthRes>>,
    oauth_client: Res<OauthClientRes>,
    complete_tasks: Query<&CompleteAuthTask>,
    mut login_error: ResMut<LoginError>,
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
    let outcome = {
        let guard = match receiver.0.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        match guard.try_recv() {
            Ok(outcome) => outcome,
            Err(_) => return,
        }
    };
    // Every arm below is terminal for this login attempt: the listener
    // thread broke its accept loop right after sending the outcome, so
    // drop all the listener resources — that joins the thread and closes
    // the socket, freeing the port for any future attempt.
    commands.remove_resource::<oauth::NativeCallbackReceiver>();
    commands.remove_resource::<oauth::NativePendingAuthRes>();
    commands.remove_resource::<oauth::NativeCallbackServerRes>();
    commands.remove_resource::<oauth::NativeAuthUrl>();
    match outcome {
        oauth::NativeCallbackOutcome::Code(code) => {
            let pending = pending_res.0.lock().ok().and_then(|mut g| g.take());
            let Some(pending) = pending else {
                warn!("OAuth callback received but no pending auth stored");
                login_error.0 = Some(
                    "The login response arrived, but the app no longer had a login \
                     attempt in progress. Please sign in again."
                        .to_string(),
                );
                return;
            };
            spawn_complete_task(&mut commands, oauth_client.0.clone(), pending, code);
        }
        oauth::NativeCallbackOutcome::Error(msg) => {
            warn!("OAuth callback returned an error redirect: {msg}");
            login_error.0 = Some(msg);
        }
    }
}
