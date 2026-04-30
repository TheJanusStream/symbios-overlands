//! Native-only callback handoff: poll the loopback [`NativeCallbackReceiver`]
//! channel, drain the matching [`NativePendingAuthRes`], and spawn the
//! exchange task.

use bevy::prelude::*;

use crate::oauth::{self, OauthClientRes};

use super::CompleteAuthTask;
use super::complete::spawn_complete_task;

/// Native-only: poll the loopback callback channel until a `(code, state)`
/// pair arrives, then drain the pending-auth resource and kick off the
/// exchange.
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
