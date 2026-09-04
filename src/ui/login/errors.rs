//! Friendly login-error mapping (#848): the login pipeline's failure
//! strings are stage-prefixed Rust error chains (`discover_server: …`,
//! `callback: …`) which used to render verbatim in red under the form.
//! [`friendly_login_error`] maps the known stage prefixes to a human
//! sentence and hands the raw chain back separately for a collapsed
//! "Details" disclosure. Messages that are already plain language (form
//! validation, #847's deny/cancel copy) pass through untouched.

/// Ordered `(needle, friendly sentence)` map from pipeline stage markers
/// to human copy. Checked with `contains` (resume errors arrive wrapped,
/// e.g. `Session resume failed: resume refresh: …`), first match wins —
/// keep more specific needles above shorter ones they'd shadow.
const STAGE_MAP: &[(&str, &str)] = &[
    // #1228 f3's bound, reported. Above the refresh needle because the
    // outer `run_or` can time out *during* the refresh, and a timeout is
    // the one resume failure that proves nothing about the saved session.
    (
        "session resume timed out",
        "Couldn't restore your saved session in time — your data server or \
         the network didn't answer. Your session is still saved; use Retry.",
    ),
    (
        "resume refresh:",
        "Your saved session has expired. Please sign in again.",
    ),
    // The same fact reached from IN GAME (#1214): a write whose token
    // refresh came back `invalid_grant` mid-session. Only the terminal
    // branch of `report_publish_failure` routes a string through here — a
    // transient `refresh: timeout` must stay retryable and keeps its own
    // wording — so this needle is safe below the resume one it shadows.
    (
        "refresh: ",
        "Your session has expired. Please sign in again to save.",
    ),
    (
        "get_relay_service_auth",
        "Signed in, but couldn't reach the world relay server — it may be down. \
         Please try again in a moment.",
    ),
    (
        "resolve_pds:",
        "Signed in, but couldn't locate your account's data server. \
         Please try again in a moment.",
    ),
    (
        "getSession",
        "Signed in, but couldn't confirm your account details with your data \
         server. Please try again in a moment.",
    ),
    (
        "discover_server:",
        "Couldn't start the login — the authorization server didn't answer \
         correctly. Check the PDS address (under Advanced) and try again.",
    ),
    (
        "authorize:",
        "Couldn't start the login — the authorization server rejected the \
         request. Please try again.",
    ),
    (
        "callback:",
        "The sign-in couldn't be completed — the authorization server rejected \
         the login attempt. Please try again.",
    ),
    (
        "dpop_key_from_jwk:",
        "The sign-in couldn't be completed because of a corrupted login state. \
         Please try again.",
    ),
    (
        "store pending auth:",
        "Couldn't save the login state in this browser — storage may be \
         blocked (private browsing mode?).",
    ),
    (
        "start callback server:",
        "Couldn't open the local port that receives the login — another \
         program may be using it. Close other Overlands instances and try again.",
    ),
    // `discover_auth_server`'s transport failure: "fetch {url}: {e}".
    // Kept last among the prefixes — it's the least specific needle.
    (
        "fetch ",
        "Couldn't reach the PDS. Check the address (under Advanced) and your \
         internet connection.",
    ),
];

/// Stage markers whose failure leaves the persisted session blob intact,
/// so re-running the resume is a real option (#1228 f6).
///
/// Everything the resume can hit except the refresh arm, which invalidates
/// the refresh token server-side and drops the blob on its way out. A
/// timeout is deliberately in this set: the outer bound fires without ever
/// learning whether the saved session is good.
const RETRYABLE_STAGES: &[&str] = &[
    "get_relay_service_auth",
    "resolve_pds:",
    "getSession",
    "timed out after",
];

/// The stage that clears the saved session on its way out. Named once so
/// the retry gate and the `LoginUiLatch.persisted` invalidation cannot
/// drift from the arm in `wasm_resume::spawn_resume_task` that does it.
const TERMINAL_STAGE: &str = "resume refresh:";

/// Whether this login error left a saved session behind (#1228 f6).
///
/// The caller uses it to invalidate the `has_persisted` cache, which the
/// entry decision reads: a stale "yes" after the blob was dropped makes a
/// landmark link go Idle instead of naming its destination.
pub fn resume_keeps_session(raw: &str) -> bool {
    !raw.contains(TERMINAL_STAGE)
}

/// Whether the login screen should offer a plain **Retry** beside this
/// error (#1228 f6).
///
/// The relay being down is the likeliest transient failure on this screen,
/// and the only affordance the idle form has is *Enter the Overlands* —
/// which starts the whole OAuth dance again, bouncing the user through the
/// consent page and reloading the wasm bundle, to reach a relay call that
/// fails the same way. With a saved session in hand the client can re-run
/// just the resume, which is what the copy for these stages has always
/// promised.
///
/// `has_persisted` is false on native by construction: there is no saved
/// session to retry from, so the button never appears there.
pub fn resume_retry_offered(raw: &str, has_persisted: bool) -> bool {
    has_persisted
        && resume_keeps_session(raw)
        && RETRYABLE_STAGES.iter().any(|stage| raw.contains(stage))
}

/// Which of the login form's three fields an error is about (#1234 f14,
/// #1229 f1).
///
/// The form is built around a type-then-Enter reflex, and Enter-to-submit
/// fires on `lost_focus()` — so by the time validation runs the field has
/// already surrendered focus and the autofocus latch is spent. Every
/// validation error therefore left the user with no caret at all, and for
/// a PDS or relay error it also left them pointed at a fold that stays
/// shut. This is the answer to "which field", so the screen can act on
/// the sentences #848 wrote.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ErrorField {
    /// Not about a field the user can edit here.
    #[default]
    None,
    Destination,
    Pds,
    Relay,
}

impl ErrorField {
    /// Whether the field lives inside the collapsed "Advanced" fold, which
    /// therefore has to open before a caret in it would mean anything.
    pub fn is_advanced(self) -> bool {
        matches!(self, Self::Pds | Self::Relay)
    }
}

/// The field a login error points at.
///
/// The rule is the copy's own: a message that tells the reader to look
/// "under Advanced" opens Advanced. That keeps the two in step with no
/// second table to maintain — a reworded sentence that drops the phrase
/// stops forcing the fold, which is correct, and one that keeps it goes on
/// working.
///
/// Takes the sentence the user is SHOWN — [`friendly_login_error`]'s first
/// half — not the raw chain. Validation messages pass through that
/// untouched, so for them the two are the same string; the pipeline stages
/// are where the phrase is acquired (`discover_server:` becomes "Check the
/// PDS address (under Advanced) and try again", and the raw chain names no
/// field at all).
///
/// Anything else that names the destination is the destination field; an
/// error about neither leaves the caret where the user put it.
pub fn error_field(shown: &str) -> ErrorField {
    if shown.contains("under Advanced") {
        return if shown.contains("Relay Host") {
            ErrorField::Relay
        } else {
            ErrorField::Pds
        };
    }
    if shown.contains("destination") || shown.contains("valid DID") {
        return ErrorField::Destination;
    }
    ErrorField::None
}

/// Map a raw login-pipeline error to `(friendly sentence, Some(raw))`,
/// or pass an already-human message through as `(message, None)` — no
/// "Details" disclosure needed when there's nothing more technical to
/// show.
pub fn friendly_login_error(raw: &str) -> (String, Option<String>) {
    for (needle, friendly) in STAGE_MAP {
        if raw.contains(needle) {
            return ((*friendly).to_string(), Some(raw.to_string()));
        }
    }
    (raw.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_prefixes_map_to_friendly_copy() {
        let (msg, details) =
            friendly_login_error("discover_server: error decoding response body: expected value");
        assert!(msg.contains("authorization server"), "{msg}");
        assert!(details.is_some());
    }

    #[test]
    fn wrapped_resume_errors_still_match() {
        let (msg, details) =
            friendly_login_error("Session resume failed: resume refresh: HTTP 400 invalid_grant");
        assert!(msg.contains("expired"), "{msg}");
        assert!(details.unwrap().contains("invalid_grant"));
    }

    /// THE SEQUENCE (#1214): a save whose token refresh came back
    /// `invalid_grant`, reported from IN GAME. The sentence existed but was
    /// reachable only from the login screen, so the owner's primary feedback
    /// on a dead session was the raw `refresh: …` chain. The resume needle
    /// above shadows this one and must keep winning — the two states differ
    /// in what the user has already lost.
    #[test]
    fn an_in_game_refresh_failure_gets_the_sign_in_again_sentence() {
        let (msg, details) =
            friendly_login_error("refresh: OAuth server error: invalid_grant - revoked");
        assert!(msg.contains("expired"), "{msg}");
        assert!(msg.contains("sign in again"), "{msg}");
        assert!(details.unwrap().contains("invalid_grant"));

        // The wasm resume path's wrapped error still takes its own arm.
        let (resume, _) = friendly_login_error("Session resume failed: resume refresh: HTTP 400");
        assert_eq!(
            resume,
            "Your saved session has expired. Please sign in again."
        );
    }

    #[test]
    fn relay_and_transport_stages_map() {
        let (msg, _) = friendly_login_error("resume get_relay_service_auth: connection refused");
        assert!(msg.contains("relay"), "{msg}");
        let (msg, _) = friendly_login_error(
            "fetch https://x.example/.well-known/oauth-protected-resource: dns error",
        );
        assert!(msg.contains("Couldn't reach the PDS"), "{msg}");
    }

    /// THE SEQUENCE (#1228 f6): an owner returns on wasm, the relay is down,
    /// and the resume fails with copy promising a retry. The only button on
    /// the idle form was *Enter the Overlands*, which re-runs the entire
    /// OAuth redirect — consent page, wasm bundle reload and all — to reach
    /// the same relay call. The saved session survived that failure, so the
    /// client can re-run just the resume.
    #[test]
    fn a_relay_failure_that_kept_the_session_offers_a_retry() {
        let relay = "Session resume failed: resume get_relay_service_auth: connection refused";
        assert!(resume_keeps_session(relay));
        assert!(resume_retry_offered(relay, true));
        // Native has no saved session to retry from, so the button is
        // unreachable there rather than merely unhelpful.
        assert!(!resume_retry_offered(relay, false));
    }

    /// The refresh arm is the one that clears the blob, so it must offer
    /// neither the button nor a stale "you have a saved session" — the
    /// entry decision reads that flag and would go Idle on a landmark link
    /// it should be naming.
    #[test]
    fn an_expired_refresh_token_offers_no_retry_and_invalidates_the_cache() {
        let expired = "Session resume failed: resume refresh: HTTP 400 invalid_grant";
        assert!(!resume_keeps_session(expired));
        assert!(!resume_retry_offered(expired, true));
    }

    /// #1228 f3's timeout is retryable on purpose: the bound fires without
    /// ever learning whether the saved session is good, so the safe
    /// direction to be wrong in is "try again", not "you are logged out".
    #[test]
    fn a_resume_timeout_is_retryable_and_says_so() {
        let raw = "Session resume failed: session resume timed out after 30s";
        assert!(resume_keeps_session(raw));
        assert!(resume_retry_offered(raw, true));
        let (msg, details) = friendly_login_error(raw);
        assert!(msg.contains("Retry"), "{msg}");
        assert!(
            msg.contains("still saved"),
            "the sentence must not read as a logout: {msg}"
        );
        assert!(details.unwrap().contains("30s"));
    }

    /// A corrupt stored blob fails identically on every attempt, so a
    /// Retry button there would be a lie dressed as an escape hatch.
    #[test]
    fn a_corrupt_blob_offers_no_retry() {
        assert!(!resume_retry_offered(
            "Session resume failed: dpop_key_from_jwk: invalid key",
            true
        ));
    }

    /// THE SEQUENCE (#1234 f14): the user blanks the PDS while
    /// experimenting and presses Enter. Enter-to-submit fires on
    /// `lost_focus`, so the caret is already gone; the message says to look
    /// "under Advanced" and the fold is shut. The screen has to act on its
    /// own sentence.
    #[test]
    fn every_validation_message_points_at_the_field_it_is_about() {
        use super::super::validation::validate_form;

        let pds = validate_form("", "relay.example", "").unwrap_err();
        assert_eq!(error_field(&pds), ErrorField::Pds, "{pds}");
        assert!(error_field(&pds).is_advanced());

        let pds_scheme = validate_form("ftp://x.example", "relay.example", "").unwrap_err();
        assert_eq!(error_field(&pds_scheme), ErrorField::Pds, "{pds_scheme}");

        let relay = validate_form("bsky.social", "", "").unwrap_err();
        assert_eq!(error_field(&relay), ErrorField::Relay, "{relay}");
        assert!(error_field(&relay).is_advanced());

        for bad in ["a b", "did:", "nodots"] {
            let dest = validate_form("bsky.social", "relay.example", bad).unwrap_err();
            assert_eq!(error_field(&dest), ErrorField::Destination, "{dest}");
            assert!(!error_field(&dest).is_advanced());
        }
    }

    /// #1229 f1's half of the same mechanism: the account server is a login
    /// INPUT — `begin_authorization` discovers the authorization server
    /// from it — so the one stage error that tells a non-Bluesky user to go
    /// and change it has to open the fold it names.
    #[test]
    fn the_discovery_failure_opens_the_fold_it_tells_you_to_look_in() {
        let (friendly, _) = friendly_login_error("discover_server: expected value");
        assert!(friendly.contains("under Advanced"), "{friendly}");
        assert_eq!(error_field(&friendly), ErrorField::Pds);
    }

    /// A pipeline error about neither field leaves the caret where the user
    /// put it, rather than yanking it somewhere arbitrary.
    #[test]
    fn an_error_about_no_field_moves_no_caret() {
        assert_eq!(
            error_field("Signed in, but couldn't reach the world relay server."),
            ErrorField::None
        );
        assert!(!ErrorField::None.is_advanced());
    }

    #[test]
    fn human_messages_pass_through_without_details() {
        let human = "Login was cancelled on the authorization page. \
                     You can try again whenever you're ready.";
        assert_eq!(
            friendly_login_error(human),
            (human.to_string(), None),
            "already-friendly copy must not be rewrapped"
        );
    }
}
