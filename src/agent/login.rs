//! `agent login` (#1414): sign the agent's own account in once, in a browser,
//! and save the session so the daemon can resume it with nobody at the
//! keyboard.
//!
//! This is the game's own native sign-in, run from a terminal instead of the
//! login screen: the same loopback OAuth client, the same granular scope, the
//! same capped transport. So the agent can do what the app does for a
//! player, which is to write the Overlands collections and mint relay tokens,
//! and nothing else on the account. And the tool never sees a password: the
//! operator types it into the account's own sign-in page.

use std::sync::Arc;
use std::sync::mpsc::RecvTimeoutError;

use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::config::agent::LOGIN_WAIT;
use crate::config::http::block_on;
use crate::oauth::{self, CompletedAuth, NativeCallbackOutcome, PendingAuth};

use super::session_file::{AgentSession, SessionStore, names_account};

/// What the operator asked `login` to do.
pub struct LoginRequest {
    /// The account's PDS or sign-in server, normalised to a URL.
    pub pds_url: String,
    /// The relay the agent will join rooms through, as a bare host.
    pub relay_host: String,
    /// The account the agent is: any other is refused, not saved.
    pub account: Option<String>,
    /// Open the sign-in page in a browser, rather than only printing it.
    pub open_browser: bool,
}

/// A session `login` saved.
pub struct SignedIn {
    pub did: String,
    pub handle: String,
    pub session_file: std::path::PathBuf,
}

/// Run the whole sign-in: start it, wait for the operator's consent in the
/// browser, finish it, check the account, prove the relay grant, and save.
pub fn login(request: &LoginRequest, store: &SessionStore) -> Result<SignedIn, String> {
    let client = oauth::OauthClientRes::default().0;
    let (auth_url, pending) = block_on(oauth::begin_authorization(
        &client,
        &request.pds_url,
        &request.relay_host,
        "",
    ))?;
    let code = wait_for_consent(&auth_url, &pending, request.open_browser)?;
    let completed = block_on(oauth::complete_authorization(&client, &pending, &code))?;
    confirm_account(
        request.account.as_deref(),
        &completed.did,
        &completed.handle,
    )?;
    prove_relay_grant(&completed, &request.relay_host)?;
    let did = completed.did.clone();
    let handle = completed.handle.clone();
    let session_file = store
        .save(&saved_session(completed, &request.relay_host))
        .map_err(|e| e.to_string())?;
    Ok(SignedIn {
        did,
        handle,
        session_file,
    })
}

/// Listen for the authorization server's redirect, send the operator to the
/// sign-in page, and wait for their answer.
fn wait_for_consent(
    auth_url: &str,
    pending: &PendingAuth,
    open_browser: bool,
) -> Result<String, String> {
    // The loopback listener only accepts the redirect that carries this
    // attempt's own random `state`, as the app's login screen does.
    let expected_state = pending.auth_state.app_state.clone().unwrap_or_default();
    let (outcomes, mut listener) = oauth::start_native_callback_server(expected_state)?;
    eprintln!(
        "Sign in as the AGENT's account (not your own) at:\n\n  {auth_url}\n\n\
         Waiting up to {} minutes...",
        LOGIN_WAIT.as_secs() / 60
    );
    if open_browser && let Err(e) = webbrowser::open(auth_url) {
        eprintln!("Could not open a browser ({e}); open the address above by hand.");
    }
    let outcome = outcomes.recv_timeout(LOGIN_WAIT);
    listener.shutdown();
    match outcome {
        Ok(NativeCallbackOutcome::Code(code)) => Ok(code),
        Ok(NativeCallbackOutcome::Error(message)) => Err(message),
        Err(RecvTimeoutError::Timeout) => Err(format!(
            "no sign-in arrived within {} minutes; run `agent login` again",
            LOGIN_WAIT.as_secs() / 60
        )),
        Err(RecvTimeoutError::Disconnected) => {
            Err("the sign-in listener stopped before a sign-in arrived".to_owned())
        }
    }
}

/// Refuse any account but the one the operator named.
///
/// A browser already signed in to the operator's own account will happily
/// authorize THAT one, and an agent signed in as its operator is worse than
/// none: the relay allows one connection per identity per room, so it would
/// take the operator's own place in every world they share.
fn confirm_account(expected: Option<&str>, did: &str, handle: &str) -> Result<(), String> {
    match expected {
        Some(expected) if !names_account(expected, did, handle) => Err(format!(
            "signed in as @{handle} ({did}), not {expected}; nothing was saved. Sign \
             that account out of the sign-in page (or use a private window) and run \
             `agent login` again"
        )),
        _ => Ok(()),
    }
}

/// Mint one relay token before saving anything, so a session that cannot
/// join a room is found out now rather than when the daemon first starts.
///
/// The minting is the PDS's alone - it signs a token for the relay's DID,
/// and the relay is not contacted - so a failure here is about the grant,
/// not the relay being down.
fn prove_relay_grant(completed: &CompletedAuth, relay_host: &str) -> Result<(), String> {
    let session = AtprotoSession {
        did: completed.did.clone(),
        handle: completed.handle.clone(),
        pds_url: completed.pds_url.clone(),
        session: Arc::clone(&completed.session),
    };
    block_on(oauth::get_relay_service_auth(&session, relay_host))
        .map(drop)
        .map_err(|e| format!("the account signed in, but cannot mint a relay token: {e}"))
}

/// The session as it is saved: the tokens as they stand now, and the key
/// they are bound to.
fn saved_session(completed: CompletedAuth, relay_host: &str) -> AgentSession {
    AgentSession {
        token_set: completed.session.token_set(),
        did: completed.did,
        handle: completed.handle,
        pds_url: completed.pds_url,
        relay_host: relay_host.to_owned(),
        dpop_jwk: completed.dpop_jwk,
        server_metadata: completed.server_metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_account_is_accepted_when_none_was_named() {
        assert!(confirm_account(None, "did:plc:who", "who.test").is_ok());
    }

    #[test]
    fn the_named_account_is_accepted_by_did_or_handle() {
        assert!(confirm_account(Some("did:plc:agent"), "did:plc:agent", "agent.test").is_ok());
        assert!(confirm_account(Some("@Agent.Test"), "did:plc:agent", "agent.test").is_ok());
    }

    /// The case this check exists for: the browser was still signed in as
    /// the operator. Nothing is saved, and the refusal names who DID sign in
    /// so the operator can see what happened.
    #[test]
    fn another_account_is_refused_and_named() {
        let err = confirm_account(Some("agent.test"), "did:plc:operator", "operator.test")
            .expect_err("refused");

        assert!(err.contains("@operator.test"), "{err}");
        assert!(err.contains("nothing was saved"), "{err}");
    }
}
