//! Login-form validation (#848): catch the blank/typo'd inputs at the
//! form with a readable message, instead of letting them fail minutes
//! later deep in the pipeline with an unrelated error (a blank relay
//! used to yield `wss:///overlands/…` *after* the OAuth dance; a
//! scheme-less PDS a verbatim reqwest builder error; a typo'd
//! destination DID a ~10-minute record-fetch retry crawl into a wrong
//! default world).

/// Where the user asked to land after login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// Blank input — the user's own world.
    Home,
    /// A `did:…` identifier, pasted directly.
    Did(String),
    /// An `@handle` (domain form) that still needs resolving to a DID
    /// via `com.atproto.identity.resolveHandle` inside the begin task.
    Handle(String),
}

/// Form values that passed validation, normalised (scheme prepended,
/// stray schemes/slashes stripped, handle lowercased).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedForm {
    pub pds_url: String,
    pub relay_host: String,
    pub destination: Destination,
}

/// Validate + normalise the three login-form fields. Returns a
/// plain-language error meant for direct display under the form.
pub fn validate_form(pds: &str, relay: &str, destination: &str) -> Result<ValidatedForm, String> {
    let pds_url = validate_pds(pds)?;
    let relay_host = validate_relay(relay)?;
    let destination = validate_destination(destination)?;
    Ok(ValidatedForm {
        pds_url,
        relay_host,
        destination,
    })
}

/// PDS must be non-empty and an http(s) URL. A scheme-less host like
/// `bsky.social` is unambiguous, so we prepend `https://` rather than
/// nag about it.
fn validate_pds(pds: &str) -> Result<String, String> {
    let pds = pds.trim().trim_end_matches('/');
    if pds.is_empty() {
        return Err(
            "The PDS field (under Advanced) is empty — the default is https://bsky.social."
                .to_string(),
        );
    }
    let with_scheme = if pds.contains("://") {
        pds.to_string()
    } else {
        format!("https://{pds}")
    };
    if !with_scheme.starts_with("https://") && !with_scheme.starts_with("http://") {
        return Err(format!(
            "The PDS (under Advanced) must be an http(s):// URL — got \"{pds}\"."
        ));
    }
    Ok(with_scheme)
}

/// Relay is a bare hostname (a `wss://…` URL is assembled from it after
/// login), so strip any scheme the user pasted and require non-empty.
fn validate_relay(relay: &str) -> Result<String, String> {
    let relay = relay.trim();
    let relay = ["wss://", "ws://", "https://", "http://"]
        .iter()
        .fold(relay, |r, scheme| r.strip_prefix(scheme).unwrap_or(r))
        .trim_end_matches('/');
    if relay.is_empty() {
        return Err(
            "The Relay Host field (under Advanced) is empty — it names the server \
             that connects you to other players."
                .to_string(),
        );
    }
    Ok(relay.to_string())
}

/// Blank ⇒ home. `did:…` ⇒ shape-checked DID. Anything with a dot ⇒ an
/// `@handle` to resolve later. Everything else is a typo we can reject
/// now instead of burning the post-login record-fetch retry budget on it.
fn validate_destination(dest: &str) -> Result<Destination, String> {
    let dest = dest.trim().trim_start_matches('@');
    if dest.is_empty() {
        return Ok(Destination::Home);
    }
    if dest.contains(char::is_whitespace) {
        return Err(
            "The destination can't contain spaces — use an @handle like \
             alice.bsky.social, a did:… identifier, or leave it blank."
                .to_string(),
        );
    }
    if dest.starts_with("did:") {
        // Minimal DID shape: `did:<method>:<id>` with non-empty parts.
        let mut parts = dest.splitn(3, ':');
        let (_, method, id) = (parts.next(), parts.next(), parts.next());
        if method.is_none_or(str::is_empty) || id.is_none_or(str::is_empty) {
            return Err(format!(
                "\"{dest}\" doesn't look like a valid DID — expected \
                 something like did:plc:abc123…"
            ));
        }
        return Ok(Destination::Did(dest.to_string()));
    }
    if dest.contains('.') {
        // Handles are domains, hence case-insensitive — lowercase for a
        // canonical lookup.
        return Ok(Destination::Handle(dest.to_ascii_lowercase()));
    }
    Err(format!(
        "\"{dest}\" isn't a destination we can use — enter an @handle like \
         alice.bsky.social, a did:… identifier, or leave it blank for your own world."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_destination_is_home() {
        let v = validate_form("https://bsky.social", "relay.example", "  ").unwrap();
        assert_eq!(v.destination, Destination::Home);
    }

    #[test]
    fn schemeless_pds_gets_https() {
        let v = validate_form("bsky.social", "relay.example", "").unwrap();
        assert_eq!(v.pds_url, "https://bsky.social");
    }

    #[test]
    fn explicit_http_pds_is_kept() {
        // Local dev PDS instances are plain http.
        let v = validate_form("http://localhost:2583", "relay.example", "").unwrap();
        assert_eq!(v.pds_url, "http://localhost:2583");
    }

    #[test]
    fn blank_pds_and_relay_fail_fast() {
        assert!(validate_form("", "relay.example", "").is_err());
        assert!(validate_form("https://bsky.social", "  ", "").is_err());
    }

    #[test]
    fn relay_scheme_and_slash_are_stripped() {
        let v = validate_form("bsky.social", "wss://relay.example/", "").unwrap();
        assert_eq!(v.relay_host, "relay.example");
        let v = validate_form("bsky.social", "https://relay.example", "").unwrap();
        assert_eq!(v.relay_host, "relay.example");
    }

    #[test]
    fn handle_destination_detected_and_lowercased() {
        let v = validate_form("bsky.social", "relay.example", "@Alice.Bsky.Social").unwrap();
        assert_eq!(
            v.destination,
            Destination::Handle("alice.bsky.social".into())
        );
    }

    #[test]
    fn did_destination_shape_checked() {
        let v = validate_form("bsky.social", "relay.example", "did:plc:abc123").unwrap();
        assert_eq!(v.destination, Destination::Did("did:plc:abc123".into()));
        assert!(validate_form("bsky.social", "relay.example", "did:plc:").is_err());
        assert!(validate_form("bsky.social", "relay.example", "did:").is_err());
    }

    #[test]
    fn garbage_destination_rejected() {
        // A dotless bare word can't be a handle (handles are domains)…
        assert!(validate_form("bsky.social", "relay.example", "alice").is_err());
        // …and embedded whitespace is never valid.
        assert!(validate_form("bsky.social", "relay.example", "alice bsky social").is_err());
    }
}
