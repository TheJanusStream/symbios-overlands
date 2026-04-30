//! Drains [`BeginAuthTask`]s — the in-flight `authorize()` round-trip
//! that produces the AS authorization URL plus a [`PendingAuth`] blob.
//! On WASM we navigate the tab to the URL; on native we start the
//! loopback callback server and launch the system browser.

use bevy::prelude::*;

use crate::oauth;

use super::{BeginAuthTask, LoginError};

/// Drain finished [`BeginAuthTask`]s. On success either navigates the tab
/// (WASM) or launches the system browser (native); on failure surfaces the
/// error into [`LoginError`].
pub fn poll_begin_auth_task(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut BeginAuthTask)>,
    mut login_error: ResMut<LoginError>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        match result {
            Ok((auth_url, pending)) => {
                info!("OAuth authorize URL obtained; handing off to browser.");
                #[cfg(target_arch = "wasm32")]
                {
                    if let Err(e) = oauth::wasm::store_pending(&pending) {
                        let msg = format!("store pending auth: {e}");
                        warn!("{msg}");
                        login_error.0 = Some(msg);
                        continue;
                    }
                    oauth::wasm::navigate_to(&auth_url);
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    // Lift the random `state` parameter out of the
                    // pending auth blob and hand it to the loopback
                    // callback server so it can reject any request
                    // whose `state=` value doesn't match — without
                    // this, any other browser tab can brick the
                    // listener with a forged callback. The library
                    // always populates `app_state`; the explicit
                    // `unwrap_or_default()` keeps us defensive against
                    // a future library change that might omit it.
                    let expected_state = pending.auth_state.app_state.clone().unwrap_or_default();
                    match oauth::start_native_callback_server(expected_state) {
                        Ok(rx) => {
                            commands.insert_resource(oauth::NativeCallbackReceiver(
                                std::sync::Mutex::new(rx),
                            ));
                            commands.insert_resource(oauth::NativePendingAuthRes(
                                std::sync::Mutex::new(Some(pending)),
                            ));
                            if let Err(e) = webbrowser::open(&auth_url) {
                                let msg = format!("open browser: {e}");
                                warn!("{msg}");
                                login_error.0 = Some(msg);
                            }
                        }
                        Err(e) => {
                            let msg = format!("start callback server: {e}");
                            warn!("{msg}");
                            login_error.0 = Some(msg);
                        }
                    }
                }
                // `pending` is moved into storage above on both targets.
                let _ = &login_error;
            }
            Err(msg) => {
                warn!("begin_authorization: {msg}");
                login_error.0 = Some(msg);
            }
        }
    }
}
