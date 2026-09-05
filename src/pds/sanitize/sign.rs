//! Sanitiser for [`SignSource`] (URL / atproto blob / DID-pfp variants)
//! and the paired `Sign` generator clamp. Defends against megabyte
//! URLs, NaN / negative panel sizes, and UV repeat factors so high they
//! pin the fragment shader on a sub-pixel texel pattern.

use super::Sanitize;
use super::common::clamp_finite;
use super::limits;
use crate::pds::generator::{AlphaModeKind, SignSource};
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::{Fp, Fp2, truncate_on_char_boundary};

impl Sanitize for SignSource {
    fn sanitize(&mut self) {
        match self {
            SignSource::Url { url } => {
                truncate_on_char_boundary(url, limits::MAX_SIGN_URL_BYTES);
                if !is_fetchable_reference(url) {
                    // Blanked rather than kept-and-skipped so the rejection
                    // survives a round trip through the record: an editor
                    // that loads and re-publishes must not carry a
                    // reference this client refused to follow.
                    url.clear();
                }
            }
            SignSource::AtprotoBlob { did, cid } => {
                truncate_on_char_boundary(did, limits::MAX_SIGN_DID_BYTES);
                truncate_on_char_boundary(cid, limits::MAX_SIGN_CID_BYTES);
            }
            SignSource::DidPfp { did } => {
                truncate_on_char_boundary(did, limits::MAX_SIGN_DID_BYTES);
            }
            SignSource::Unknown => {}
        }
    }
}

/// Whether a PDS service endpoint from a DID document is one this client
/// will talk to (#1127).
///
/// Same rule as [`is_fetchable_reference`], and the same reasoning: the
/// document is written by whoever controls the DID, and resolving it aims
/// every subsequent record fetch at the endpoint it names.
pub(crate) fn is_fetchable_endpoint(endpoint: &str) -> bool {
    is_fetchable_reference(endpoint)
}

/// Whether an asset-reference URL is one this client will follow (#1127).
///
/// Asset references arrive inside records authored by *other identities* —
/// a room reached through a portal or a gateway is a stranger's — and every
/// visitor's client fetches whatever they name. Unrestricted, that made a
/// record a way to point every visitor at an arbitrary address: `http://`
/// to a beacon that logs who visited, or, on native clients, to loopback
/// and RFC1918 addresses that only exist inside the visitor's own network.
/// Nothing is read back to the author, so this is a blind request — which
/// makes it useful for reconnaissance and tracking rather than exfiltration,
/// and no less worth refusing.
///
/// **This raises the bar; it does not close SSRF.** A public hostname can
/// still resolve to a private address, and catching that needs a check at
/// connect time against the resolved IP, which the HTTP client does not
/// expose. What this removes is the trivial case: naming the address
/// outright.
///
/// `http://` is accepted only for loopback, and only in a debug build, so
/// that a locally-served asset still works while developing. Release builds
/// require `https` unconditionally.
///
/// `pub(crate)` since #1248: the rule is enforced by DELETING the URL, so the
/// only way the owner can ever see it is for the editor to ask the same
/// question before the sanitiser gets there. [`refusal_reason`] is the
/// sentence that goes with it.
pub(crate) fn is_fetchable_reference(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        // Includes the empty string, which is the Default and means
        // "nothing referenced" rather than a rejection.
        return false;
    };
    match parsed.scheme() {
        "https" => !is_private_host(&parsed),
        #[cfg(debug_assertions)]
        "http" => parsed.host_str().is_some_and(is_loopback_name),
        // file://, data://, ftp://, gopher:// and everything else: a
        // reference is an HTTPS GET, and no other scheme has a meaning
        // here that is worth the surface.
        _ => false,
    }
}

/// Why a typed asset URL will be refused, or `None` if it will be followed
/// (#1248 f79 / f340).
///
/// The refusal is enforced by `url.clear()` in the sanitiser — a policy that
/// deletes the owner's typed text about a quarter of a second after they type
/// it, with nothing said. Keeping the *reason* beside the predicate is what
/// lets the field refuse the draft and explain itself instead, so the text is
/// never destroyed and the rule becomes legible rather than something to keep
/// tripping over.
///
/// The empty string is not a refusal: it is the default, and means "nothing
/// referenced".
pub(crate) fn refusal_reason(url: &str) -> Option<String> {
    if url.is_empty() || is_fetchable_reference(url) {
        return None;
    }
    let Ok(parsed) = url::Url::parse(url) else {
        return Some("Not a full web address yet — it needs to start https://".to_string());
    };
    if parsed.scheme() != "https" {
        return Some(format!(
            "Only https addresses are loaded — {}:// is refused.",
            parsed.scheme()
        ));
    }
    Some(
        "Addresses inside a private network are refused — this one would only \
         work on your own machine."
            .to_string(),
    )
}

/// Whether the host is written as an address rather than a name, and that
/// address is one only the visitor can reach.
///
/// Only literals are judged — a name is left to DNS, per the caveat on
/// [`is_fetchable_reference`].
fn is_private_host(parsed: &url::Url) -> bool {
    match parsed.host() {
        Some(url::Host::Ipv4(ip)) => {
            ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified()
        }
        Some(url::Host::Ipv6(ip)) => {
            ip.is_loopback()
                || ip.is_unspecified()
                // Unique-local (fc00::/7) and link-local (fe80::/10); the
                // stable `Ipv6Addr` accessors for these are still unstable,
                // so the segment test is written out.
                || (ip.segments()[0] & 0xfe00) == 0xfc00
                || (ip.segments()[0] & 0xffc0) == 0xfe80
        }
        Some(url::Host::Domain(name)) => is_loopback_name(name),
        None => true,
    }
}

/// `localhost` and its subdomains, which resolve to loopback by convention
/// (RFC 6761) without being written as an address.
fn is_loopback_name(name: &str) -> bool {
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    name == "localhost" || name.ends_with(".localhost")
}

/// Clamp every numeric field on a `Sign` generator, bound its source
/// strings, and fold the legacy UV window into the material (#964). Mirrors
/// the inline-fields layout of `GeneratorKind::Sign` so the dispatcher can
/// pass each field through.
pub(super) fn sanitize_sign(
    source: &mut SignSource,
    size: &mut Fp2,
    uv_repeat: &mut Fp2,
    uv_offset: &mut Fp2,
    material: &mut SovereignMaterialSettings,
    alpha_mode: &mut AlphaModeKind,
) {
    source.sanitize();

    let s = limits::MAX_SIGN_SIZE;
    size.0[0] = clamp_finite(size.0[0], 0.01, s, 1.0);
    size.0[1] = clamp_finite(size.0[1], 0.01, s, 1.0);

    migrate_legacy_uv_window(uv_repeat, uv_offset, material);

    // After the fold, so the migrated values go through the same clamps as
    // an authored one.
    material.sanitize();

    if let AlphaModeKind::Mask { cutoff } = alpha_mode {
        cutoff.0 = clamp_finite(cutoff.0, 0.0, 1.0, 0.5);
    }
}

/// Fold a pre-#964 Sign's `uv_repeat` / `uv_offset` into the material's UV
/// transform and reset them to the identity.
///
/// The two used to be baked into the panel mesh's four vertex UVs
/// (`uv = offset + repeat · t`); they are now the material's
/// `uv_scale` / `uv_offset`, applied by the same affine every other surface
/// in the app goes through. Matching the old look is arithmetic: with the
/// mesh now emitting a plain `0..1` quad, `sovereign_uv_transform` samples
/// `scale · t + scale · material_offset`, so `scale = repeat` and
/// `material_offset = offset / repeat`.
///
/// **Idempotent by construction** — the reset to the identity is what makes
/// a second sanitize pass a no-op, which the sanitize fixpoint requires.
///
/// The one lossy case is a legacy *anisotropic* window: a single `uv_scale`
/// cannot say "twice across U, once across V". The larger repeat wins, so an
/// axis may show *less* of the image than before but never more — a crop is
/// recoverable by eye, invented content is not.
fn migrate_legacy_uv_window(
    uv_repeat: &mut Fp2,
    uv_offset: &mut Fp2,
    material: &mut SovereignMaterialSettings,
) {
    let r_lo = limits::MIN_SIGN_UV_REPEAT;
    let r_hi = limits::MAX_SIGN_UV_REPEAT;
    let ru = clamp_finite(uv_repeat.0[0], r_lo, r_hi, 1.0);
    let rv = clamp_finite(uv_repeat.0[1], r_lo, r_hi, 1.0);
    let o = limits::MAX_SIGN_UV_OFFSET;
    let ou = clamp_finite(uv_offset.0[0], -o, o, 0.0);
    let ov = clamp_finite(uv_offset.0[1], -o, o, 0.0);

    let repeat = ru.max(rv);
    // Compose rather than assign: a material that already carries a scale
    // (authored after the unification) keeps it, and the legacy identity
    // leaves it untouched.
    let scale = material.uv_scale.0 * repeat;
    material.uv_scale = Fp(scale);
    if scale.abs() > f32::EPSILON {
        material.uv_offset = Fp2([
            material.uv_offset.0[0] + ou / scale,
            material.uv_offset.0[1] + ov / scale,
        ]);
    }

    *uv_repeat = Fp2([1.0, 1.0]);
    *uv_offset = Fp2([0.0, 0.0]);
}

#[cfg(test)]
mod reference_url_tests {
    //! #1127: an asset reference is an address a *stranger's* record makes
    //! every visitor's client fetch. These pin which addresses survive
    //! sanitisation.
    use super::*;

    fn sanitized(url: &str) -> String {
        let mut source = SignSource::Url { url: url.into() };
        source.sanitize();
        match source {
            SignSource::Url { url } => url,
            other => panic!("variant changed: {other:?}"),
        }
    }

    /// The regression. A room reached through a portal is authored by
    /// someone the visitor has never met; before this, its sign, audio and
    /// splat references could name any address at all, and every visitor's
    /// client issued a blind GET to it on load.
    #[test]
    fn addresses_only_the_visitor_can_reach_are_dropped() {
        for hostile in [
            "http://beacon.example/who-visited",
            "https://127.0.0.1/admin",
            "https://localhost:8080/",
            "https://LOCALHOST/",
            "https://dev.localhost/",
            "https://10.0.0.5/",
            "https://192.168.1.1/",
            "https://172.16.0.1/",
            "https://169.254.169.254/latest/meta-data/",
            "https://[::1]/",
            "https://[fd00::1]/",
            "https://[fe80::1]/",
            "https://0.0.0.0/",
            "file:///etc/passwd",
            "data:text/html,<script>alert(1)</script>",
            "ftp://example.com/x",
            "javascript:alert(1)",
            "not a url at all",
        ] {
            assert!(
                sanitized(hostile).is_empty(),
                "{hostile} must not survive sanitisation"
            );
        }
    }

    /// The control. Refusing everything would be a safe and useless rule —
    /// ordinary hosted assets must still load.
    #[test]
    fn ordinary_https_references_survive_untouched() {
        for benign in [
            "https://cdn.example.com/sign.png",
            "https://example.com/a/b/c.webp?v=2",
            "https://sub.domain.example/asset.jpg#frag",
        ] {
            assert_eq!(sanitized(benign), benign, "{benign} must still load");
        }
    }

    /// A rejected reference is BLANKED, not merely skipped at fetch time.
    /// An editor that opens such a room and re-publishes it must not carry
    /// the address back onto the PDS for the next visitor to be pointed at.
    #[test]
    fn a_rejected_reference_does_not_survive_a_round_trip() {
        let mut source = SignSource::Url {
            url: "http://beacon.example/".into(),
        };
        source.sanitize();
        assert_eq!(source, SignSource::Url { url: String::new() });
    }

    /// `http://` to loopback is a development convenience and nothing more,
    /// so it lives behind `debug_assertions`. Asserted in both directions
    /// because which one runs depends on the profile, and that has already
    /// moved once: when this was written `[profile.test-release]` inherited
    /// release and so had debug assertions OFF, and #1147 then turned them
    /// on. A test written for only one branch would have silently swapped
    /// which rule it was checking.
    ///
    /// Note what that means today: the gate exercises the DEBUG branch, so
    /// the release rule — loopback over plain http refused outright — is
    /// checked by the `else` arm only when someone builds without
    /// assertions. The one-line assertion below it covers the case that
    /// actually matters either way.
    #[test]
    fn plain_http_to_loopback_is_a_debug_build_affordance_only() {
        let local = sanitized("http://localhost:2583/asset.png");
        if cfg!(debug_assertions) {
            assert_eq!(
                local, "http://localhost:2583/asset.png",
                "a locally served asset still loads while developing"
            );
        } else {
            assert!(local.is_empty(), "release builds require https");
        }
        // Never, in either profile: plain http to somewhere else.
        assert!(sanitized("http://example.com/x.png").is_empty());
    }

    /// The same predicate guards DID-document service endpoints, which
    /// aim every subsequent record fetch.
    #[test]
    fn a_pds_endpoint_follows_the_same_rule_as_a_reference() {
        assert!(is_fetchable_endpoint("https://pds.example.com"));
        assert!(!is_fetchable_endpoint("http://pds.example.com"));
        assert!(!is_fetchable_endpoint("https://192.168.1.20:2583"));
        assert!(!is_fetchable_endpoint(""));
    }

    /// #1248 f79 / f340: the rule is enforced by DELETING the owner's typed
    /// text, so the editor has to be able to ask the same question and get
    /// a sentence back. The two must agree exactly — a field that refused
    /// what the sanitiser accepts would block a working URL, and one that
    /// accepted what the sanitiser refuses would hand the text back to the
    /// blanking.
    #[test]
    fn the_refusal_reason_agrees_with_the_rule_it_explains() {
        let cases = [
            "https://cdn.example.org/a.png",
            "http://example.org/a.png",
            "https://127.0.0.1/a.png",
            "https://10.0.0.5/a.png",
            "file:///etc/passwd",
            "ftp://example.org/a.png",
            "https:/",
            "not a url",
        ];
        for url in cases {
            assert_eq!(
                refusal_reason(url).is_none(),
                is_fetchable_reference(url),
                "{url}: the field and the sanitiser must answer the same"
            );
        }
        // Every refusal says something, and says it about this URL.
        assert!(
            refusal_reason("ftp://example.org/a.png").is_some_and(|r| r.contains("ftp")),
            "the reason names the scheme it refused"
        );
        assert!(refusal_reason("https:/").is_some_and(|r| r.contains("https://")));
        assert!(refusal_reason("https://10.0.0.5/a.png").is_some_and(|r| r.contains("private")));
    }

    /// The empty string is the default and means "nothing referenced". A
    /// freshly-added Sign must not open with a refusal printed under it.
    #[test]
    fn an_empty_url_is_not_a_refusal() {
        assert_eq!(refusal_reason(""), None);
        assert!(!is_fetchable_reference(""), "and it is still not fetchable");
    }
}
