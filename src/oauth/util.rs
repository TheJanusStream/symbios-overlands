//! Small shared helpers for the OAuth callback plumbing, compiled on
//! both targets (the wasm query-string parser and the native loopback
//! listener each need the same decoder — #657 deduped the twins, and
//! #847 promoted the query parser itself into [`parse_query_params`]).

/// Recognised parameters of an OAuth authorization-server callback query
/// string, percent-decoded. `error` / `error_description` carry the
/// RFC 6749 §4.1.2.1 error redirect — most commonly `access_denied`
/// when the user clicks *Deny* on the consent page. Before #847 both
/// targets extracted only `code` / `state`, so a deny left the native
/// listener waiting forever and the wasm page silently re-showing the
/// form.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

impl CallbackParams {
    /// Human-readable message for an AS error redirect, or `None` when
    /// the callback carries no `error` parameter. `access_denied` gets a
    /// plain-language message because it is the everyday "user changed
    /// their mind" case, not a fault.
    pub fn error_message(&self) -> Option<String> {
        let error = self.error.as_deref()?;
        Some(match (error, self.error_description.as_deref()) {
            ("access_denied", _) => "Login was cancelled on the authorization page. \
                 You can try again whenever you're ready."
                .to_string(),
            (_, Some(desc)) => format!("Login failed: {error} — {desc}"),
            (_, None) => format!("Login failed: {error}"),
        })
    }
}

/// Parse a raw query string (no leading `?`) into [`CallbackParams`],
/// quietly skipping unrecognised keys (`iss`, tracking params, …).
pub(super) fn parse_query_params(query: &str) -> CallbackParams {
    let mut params = CallbackParams::default();
    for pair in query.split('&') {
        let mut it = pair.splitn(2, '=');
        let k = it.next().unwrap_or("");
        let v = it.next().unwrap_or("");
        let decoded = percent_decode(v);
        match k {
            "code" => params.code = Some(decoded),
            "state" => params.state = Some(decoded),
            "error" => params.error = Some(decoded),
            "error_description" => params.error_description = Some(decoded),
            _ => {}
        }
    }
    params
}

/// Minimal percent-decoder for query values (handles `%HH` escapes
/// and `+` as space). OAuth callback values contain URL-encoded
/// characters and we don't want to pull in a full urlencoding crate.
pub(super) fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let h = hex(bytes[i + 1]);
                let l = hex(bytes[i + 2]);
                match (h, l) {
                    (Some(h), Some(l)) => {
                        out.push((h << 4) | l);
                        i += 3;
                    }
                    _ => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
