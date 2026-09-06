//! The HTTP transport every proto-blue OAuth request runs on, with a byte
//! ceiling on the response body (#1176).
//!
//! # Why this is a transport and not a call-site check
//!
//! #1176 was filed against seven `resp.text().await.unwrap_or_default()`
//! calls on the token and session paths, on the reading that they buffer an
//! unbounded body the way the record-fetch paths #1124 capped did. They do
//! not, and the difference is invisible at the call site because the
//! spelling is identical.
//!
//! `OAuthSession::get` / `::post` return
//! [`proto_blue_common::fetch::HttpResponse`], not `reqwest::Response`. Its
//! body is a `Vec<u8>` that is **already fully buffered**, and upstream says
//! so on `HttpResponse::text`: *"`async` for source-compatibility with
//! reqwest callers even though no I/O is performed here — the body is
//! already buffered."* `text()` is `String::from_utf8(self.body)`, which
//! reuses that allocation. Capping there would truncate a `String` that has
//! already cost whatever the server chose to send; there is nothing left to
//! refuse.
//!
//! The unbounded read is one layer down, in the [`FetchHandler`] the OAuth
//! client and session are constructed with. proto-blue's own
//! `ReqwestFetcher::fetch` ends with
//!
//! ```text
//! let body = response.bytes().await?.to_vec();
//! ```
//!
//! `bytes()` buffers the whole stream with no ceiling, and `.to_vec()` then
//! copies it again — so a hostile or broken PDS costs twice the body it
//! sends. That handler is injectable
//! ([`proto_blue_oauth::OAuthClient::with_fetch_handler`],
//! [`proto_blue_oauth::OAuthSession::with_fetch_handler`]), the seam the
//! refresh-dance tests already use to script a transport. Replacing it fixes
//! every proto-blue request at once, rather than the seven that happened to
//! be enumerated — including the token exchange and the refresh, which were
//! not on the list and are the two that run before a session exists.
//!
//! # What else changed by having our own transport
//!
//! proto-blue's default is a bare `reqwest::Client::new()`. Routing through
//! [`crate::config::http::default_client`] instead means the OAuth path
//! picks up what every other outbound request here already had and it did
//! not: a request and connect timeout, the redirect policy that keeps a
//! server from steering us onto a private host, the pinned user agent, and
//! `use_rustls_tls()` (#1154) rather than the OpenSSL backend feature
//! unification selects by default.

use async_trait::async_trait;
use proto_blue_common::fetch::{FetchError, FetchHandler, HttpRequest, HttpResponse};
// Only the native arm builds a request and re-collects headers; the wasm arm
// hands both straight to `WebFetcher`.
#[cfg(not(target_arch = "wasm32"))]
use proto_blue_common::fetch::{HttpHeaders, HttpMethod};

/// Ceiling on one OAuth/authorization-server response body.
///
/// The same number [`crate::pds::xrpc::MAX_FETCH_BODY_BYTES`] uses for one
/// peer-controlled body, deliberately rather than something tuned to what
/// these endpoints actually return. Every body on this transport is small
/// JSON — a token set, a `getSession` identity, an `applyWrites` result
/// list — so a tight cap would look defensible and would be a latent bug:
/// the cost of guessing low is a legitimate response silently refused, on
/// the paths that log a user in. This bounds the allocation without
/// pretending to know the payload.
pub const MAX_OAUTH_BODY_BYTES: usize = crate::pds::xrpc::MAX_FETCH_BODY_BYTES;

/// A [`FetchHandler`] that refuses a response body over [`Self::cap`].
///
/// Construct with [`CappedFetcher::new`]; install with
/// `OAuthClient::with_fetch_handler` / `OAuthSession::with_fetch_handler`.
#[derive(Clone)]
pub struct CappedFetcher {
    cap: usize,
    #[cfg(not(target_arch = "wasm32"))]
    client: reqwest::Client,
    #[cfg(target_arch = "wasm32")]
    inner: proto_blue_common::fetch::WebFetcher,
}

impl CappedFetcher {
    /// A transport capped at [`MAX_OAUTH_BODY_BYTES`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_cap(MAX_OAUTH_BODY_BYTES)
    }

    /// A transport capped at `cap` bytes. Exists for the tests, which would
    /// otherwise have to move 16 MiB across a socket to prove the ceiling
    /// holds.
    #[must_use]
    pub fn with_cap(cap: usize) -> Self {
        Self {
            cap,
            #[cfg(not(target_arch = "wasm32"))]
            client: crate::config::http::default_client(),
            #[cfg(target_arch = "wasm32")]
            inner: proto_blue_common::fetch::WebFetcher::new(),
        }
    }
}

impl Default for CappedFetcher {
    fn default() -> Self {
        Self::new()
    }
}

/// The error a refused body reports.
///
/// [`FetchError::Body`] rather than a new variant: to every caller above
/// this — `exchange_code`, `refresh_token`, `oauth_get_with_nonce_retry` —
/// an oversized body is a body that could not be read, which is what that
/// variant already means. It matters that this is not [`FetchError::Timeout`]
/// or a `RefreshFailed`: [`crate::oauth::refresh::refresh_is_terminal`]
/// classifies only `invalid_grant` and "No refresh token" as terminal, so a
/// refusal here stays retryable, which is right — the next attempt may reach
/// a PDS that is behaving.
fn oversized(seen: usize, cap: usize) -> FetchError {
    FetchError::Body(format!(
        "response body exceeds the {cap}-byte cap (saw at least {seen}); refusing to buffer it"
    ))
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl FetchHandler for CappedFetcher {
    async fn fetch(&self, req: HttpRequest) -> Result<HttpResponse, FetchError> {
        let method = match req.method {
            HttpMethod::Get => reqwest::Method::GET,
            HttpMethod::Post => reqwest::Method::POST,
            HttpMethod::Put => reqwest::Method::PUT,
            HttpMethod::Delete => reqwest::Method::DELETE,
            HttpMethod::Patch => reqwest::Method::PATCH,
            HttpMethod::Head => reqwest::Method::HEAD,
            HttpMethod::Options => reqwest::Method::OPTIONS,
        };

        let mut builder = self.client.request(method, &req.url);
        for (key, value) in &req.headers {
            builder = builder.header(key, value);
        }
        if let Some(body) = req.body {
            builder = builder.body(body);
        }

        let mut response = builder.send().await.map_err(reqwest_to_fetch_error)?;
        let status = response.status().as_u16();

        let mut headers = HttpHeaders::new();
        for (name, value) in response.headers() {
            if let Ok(v) = value.to_str() {
                headers.insert(name.as_str().to_lowercase(), v.to_string());
            }
        }

        // Cheap refusal for a server that declares an oversized body.
        if let Some(len) = response.content_length()
            && len as usize > self.cap
        {
            return Err(oversized(len as usize, self.cap));
        }

        // And the one that does the actual work: a server can omit
        // `Content-Length` and frame with connection-close, or simply lie.
        // Accumulating chunk by chunk is what makes the ceiling real —
        // `bytes()` would have bought the whole body before we could look.
        let mut body: Vec<u8> = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    if body.len().saturating_add(chunk.len()) > self.cap {
                        return Err(oversized(body.len() + chunk.len(), self.cap));
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(e) => return Err(FetchError::Body(e.to_string())),
            }
        }

        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

// On wasm the browser's fetch API has already buffered the body by the time
// reqwest (or gloo) hands back a response: `chunk()` is not exposed and
// mid-stream cancellation is not possible. So this arm wraps `WebFetcher`
// and checks after the fact, which bounds what reaches OUR heap but cannot
// stop the browser from allocating it first.
//
// The same limitation, for the same reason, as `pds::xrpc::read_capped_body`
// — recorded here rather than left to be rediscovered. It is not nothing:
// the wasm heap never shrinks, so refusing to copy an oversized body into a
// long-lived `HttpResponse` is what keeps a single hostile reply from
// raising the floor for the rest of the session.
#[cfg(target_arch = "wasm32")]
#[async_trait(?Send)]
impl FetchHandler for CappedFetcher {
    async fn fetch(&self, req: HttpRequest) -> Result<HttpResponse, FetchError> {
        let resp = self.inner.fetch(req).await?;
        if resp.body.len() > self.cap {
            return Err(oversized(resp.body.len(), self.cap));
        }
        Ok(resp)
    }
}

/// Mirrors proto-blue's own classification, which is private to that crate.
///
/// Order matters and is theirs: `is_request` is broad and would swallow the
/// more specific cases, and connection-refused / DNS / TLS all surface as
/// `is_request` in reqwest 0.12. Kept identical so replacing the transport
/// does not reclassify a failure the layers above already branch on.
#[cfg(not(target_arch = "wasm32"))]
fn reqwest_to_fetch_error(e: reqwest::Error) -> FetchError {
    if e.is_timeout() {
        FetchError::Timeout
    } else if e.is_builder() {
        FetchError::InvalidUrl(e.to_string())
    } else if e.is_body() || e.is_decode() {
        FetchError::Body(e.to_string())
    } else if e.is_connect() || e.is_request() {
        FetchError::Network(e.to_string())
    } else {
        FetchError::Other(e.to_string())
    }
}

/// The ceiling, against a real socket.
///
/// These assert what the transport *refuses*, not that a helper returned
/// `Ok`: an oversized body that is quietly truncated and an oversized body
/// that is rejected both leave the caller with a short string, and only the
/// error distinguishes them.
///
/// Native-only: the cap is target-independent but the wasm arm has no
/// `chunk()` to exercise and the suite has no wasm runner.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    use super::*;

    const CAP: usize = 4096;

    /// Serve one 200 with `body`, then close. With `declare_length` false
    /// the response omits `Content-Length` and frames by connection-close,
    /// which is how a server defeats the cheap pre-check and leaves the
    /// streaming loop to do the work.
    fn serve_once(body: Vec<u8>, declare_length: bool) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        std::thread::spawn(move || {
            let Ok((mut sock, _)) = listener.accept() else {
                return;
            };
            // Read the request head so the client never sees a reset.
            let mut scratch = [0u8; 4096];
            let _ = sock.read(&mut scratch);
            let mut head = String::from("HTTP/1.1 200 OK\r\n");
            head.push_str("Content-Type: application/json\r\n");
            if declare_length {
                head.push_str(&format!("Content-Length: {}\r\n", body.len()));
            } else {
                head.push_str("Connection: close\r\n");
            }
            head.push_str("\r\n");
            if sock.write_all(head.as_bytes()).is_ok() {
                let _ = sock.write_all(&body);
            }
            let _ = sock.flush();
        });
        format!("http://{addr}/")
    }

    fn fetch(url: &str) -> Result<HttpResponse, FetchError> {
        crate::config::http::block_on(async {
            CappedFetcher::with_cap(CAP)
                .fetch(HttpRequest::get(url))
                .await
        })
    }

    #[test]
    fn a_body_within_the_cap_arrives_whole() {
        let url = serve_once(vec![b'x'; CAP], true);
        let resp = fetch(&url).expect("within the cap");
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body.len(), CAP, "the cap is inclusive");
    }

    #[test]
    fn a_declared_oversized_body_is_refused_before_it_is_read() {
        let url = serve_once(vec![b'x'; CAP + 1], true);
        let err = fetch(&url).expect_err("over the cap");
        assert!(
            matches!(err, FetchError::Body(_)),
            "an oversized body reads as a body error, not a network one: {err:?}"
        );
    }

    #[test]
    fn an_undeclared_oversized_body_is_refused_by_the_streaming_loop() {
        // No Content-Length, so the pre-check cannot fire and only the
        // chunk loop stands between a hostile server and the heap.
        let url = serve_once(vec![b'x'; CAP * 4], false);
        let err = fetch(&url).expect_err("over the cap");
        assert!(
            matches!(err, FetchError::Body(_)),
            "the streaming cap must refuse what the pre-check cannot see: {err:?}"
        );
    }

    #[test]
    fn status_and_headers_survive_the_transport() {
        let url = serve_once(b"{\"ok\":true}".to_vec(), true);
        let resp = fetch(&url).expect("small body");
        assert_eq!(resp.status, 200);
        assert_eq!(
            resp.headers.get("content-type").map(String::as_str),
            Some("application/json"),
            "header names are lowercased, as proto-blue's own handler does"
        );
        assert_eq!(resp.body, b"{\"ok\":true}");
    }

    /// The whole fix is *which constructor* the production paths call, and
    /// nothing observable changes if one of them goes back to `::new` — the
    /// app logs in exactly the same, right up until a PDS answers with a
    /// body it chose the size of. `OAuthClient` and `OAuthSession` keep
    /// their fetcher private, so there is no handle to assert on from
    /// outside; the rule has to be held over the source.
    ///
    /// Borrows the walk and the test-cut from the UI scans rather than
    /// re-deriving them, which is what that module is `pub(crate)` for.
    #[test]
    fn no_production_path_builds_an_uncapped_oauth_transport() {
        use crate::ui::fonts::glyph_coverage_tests::{non_test_source, rust_sources_under};

        // Split needles: a scan that bans a string is itself a file
        // containing that string, and this one found its own line first
        // time out. `non_test_source` does not save it — that helper cuts
        // at `#[cfg(test)]`, and this module is gated
        // `#[cfg(all(test, not(target_arch = "wasm32")))]`, which does not
        // match, so the scan reads its own test module.
        let session_new = concat!("OAuthSession", "::new(");
        let client_new = concat!("OAuthClient", "::new(");

        // The three production constructions, by file. A count alone is a
        // weak control here: `refresh.rs`'s scripted-transport tests are
        // gated `#[cfg(all(test, …))]` too, so `non_test_source` does not
        // cut them and their two `with_fetch_handler` calls would satisfy a
        // bare threshold on their own.
        let must_cap = [
            "src/oauth/mod.rs",            // the shared OAuthClient
            "src/oauth/auth_flow.rs",      // the session a login builds
            "src/ui/login/wasm_resume.rs", // the session a reload rebuilds
        ];

        let mut uncapped: Vec<String> = Vec::new();
        let mut capping: Vec<String> = Vec::new();
        for path in rust_sources_under("src") {
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{} readable: {e}", path.display()));
            let source = non_test_source(&source);
            let rel = path
                .strip_prefix(env!("CARGO_MANIFEST_DIR"))
                .unwrap_or(&path)
                .display()
                .to_string();
            for (i, line) in source.lines().enumerate() {
                if line.contains(session_new) || line.contains(client_new) {
                    uncapped.push(format!("{rel}:{}", i + 1));
                }
                if line.contains("with_fetch_handler(") {
                    capping.push(rel.clone());
                }
            }
        }

        // A scan that reads nothing passes clean, so prove it read the
        // three files the rule is actually about before believing it.
        for file in must_cap {
            assert!(
                capping.iter().any(|seen| seen == file),
                "the scan did not find a `with_fetch_handler` construction in \
                 {file} — either that path went back to an uncapped transport \
                 or this scan has gone blind. Saw: {capping:?}"
            );
        }
        assert!(
            uncapped.is_empty(),
            "{uncapped:?} construct an OAuth client or session with `::new`, \
             which installs proto-blue's uncapped `ReqwestFetcher` — the body \
             read there has no ceiling (#1176). Use `with_fetch_handler` with \
             a `CappedFetcher`."
        );
    }

    #[test]
    fn the_default_cap_is_the_one_peer_body_ceiling() {
        assert_eq!(
            MAX_OAUTH_BODY_BYTES,
            crate::pds::xrpc::MAX_FETCH_BODY_BYTES,
            "the OAuth transport and the record fetch paths bound one body \
             the same way; a divergence here is a number someone guessed"
        );
    }
}
