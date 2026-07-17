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
    (
        "resume refresh:",
        "Your saved session has expired. Please sign in again.",
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

    #[test]
    fn relay_and_transport_stages_map() {
        let (msg, _) = friendly_login_error("resume get_relay_service_auth: connection refused");
        assert!(msg.contains("relay"), "{msg}");
        let (msg, _) = friendly_login_error(
            "fetch https://x.example/.well-known/oauth-protected-resource: dns error",
        );
        assert!(msg.contains("Couldn't reach the PDS"), "{msg}");
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
