//! The DPoP key a session signs with, rebuilt from a stored JWK (#1409).
//!
//! proto-blue's `dpop_key_from_jwk` checks the key's curve and nothing
//! else: the private scalar `d` is first decoded when the key signs its
//! first proof, and a `d` that does not decode to 32 bytes PANICS there,
//! inside `generic-array` (a slice turned into a fixed-size array with
//! `.into()`). A stored JWK is whatever the storage holds - edited by hand,
//! truncated, written by another build - and a panic in the browser aborts
//! the whole app, frozen on its last frame with the bad session still saved
//! to do it again. So a stored key is checked here, where a bad one is an
//! error the caller can recover from: its `d` must be exactly the 43
//! base64url characters a 32-byte scalar takes (both supported curves'
//! scalars are 32 bytes), and it must sign one proof, which turns a scalar
//! outside its curve's range into an error rather than a panic.

use proto_blue_oauth::DpopKey;
use proto_blue_oauth::client::dpop_key_from_jwk;
use serde_json::Value;

/// base64url characters, unpadded, in a 32-byte value.
const SCALAR_CHARS: usize = 43;

/// Where the check's one proof says it is going. The proof is thrown away
/// unsent; the name is on RFC 6761's never-resolving `.invalid`.
const CHECK_URI: &str = "https://dpop-key-check.invalid/";

/// The DPoP key `jwk` describes, or why it cannot be used.
pub fn saved_dpop_key(jwk: &Value) -> Result<DpopKey, String> {
    let key = dpop_key_from_jwk(jwk).map_err(|e| e.to_string())?;
    let d = jwk
        .get("d")
        .and_then(Value::as_str)
        .ok_or("the key has no private part")?;
    let base64url = |b: u8| b.is_ascii_alphanumeric() || b == b'-' || b == b'_';
    if d.len() != SCALAR_CHARS || !d.bytes().all(base64url) {
        return Err(format!(
            "the key's private part is {} characters of something, not the {SCALAR_CHARS} \
             base64url characters of a key",
            d.len()
        ));
    }
    proto_blue_oauth::build_dpop_proof(&key, "POST", CHECK_URI, None, None)
        .map_err(|e| format!("the key does not sign: {e}"))?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_key() -> Value {
        DpopKey::generate().expect("a key").private_jwk
    }

    #[test]
    fn a_good_key_comes_back_and_signs() {
        let jwk = a_key();
        let key = saved_dpop_key(&jwk).expect("a good key");
        assert_eq!(key.private_jwk, jwk);
    }

    /// THE CASE (#1409): a `d` of three bytes made proto-blue's first proof
    /// panic ("left: 3, right: 32"). Every malformed `d` is an error here -
    /// too short, too long, not base64url, 43 characters whose last one
    /// carries bits a 32-byte value cannot have - and none of them panics.
    #[test]
    fn a_malformed_private_part_is_an_error_not_a_panic() {
        let with_d = |d: &str| {
            let mut jwk = a_key();
            jwk["d"] = Value::String(d.to_owned());
            saved_dpop_key(&jwk)
        };
        let good_d = a_key()["d"].as_str().expect("d").to_owned();
        for d in [
            "AQAB".to_owned(),
            format!("{good_d}AA"),
            format!("{}!", &good_d[..SCALAR_CHARS - 1]),
            format!("{}_", &good_d[..SCALAR_CHARS - 1]),
        ] {
            assert!(with_d(&d).is_err(), "{d:?} was taken");
        }
        let mut no_d = a_key();
        no_d.as_object_mut().expect("an object").remove("d");
        assert!(saved_dpop_key(&no_d).is_err());
    }

    /// A 32-byte `d` of all zeros is the right length and no key at all:
    /// the signing check refuses it.
    #[test]
    fn a_scalar_outside_the_curve_is_refused() {
        let mut jwk = a_key();
        jwk["d"] = Value::String("A".repeat(SCALAR_CHARS));
        let why = saved_dpop_key(&jwk).expect_err("refused");
        assert!(why.contains("does not sign"), "{why}");
    }

    #[test]
    fn another_curve_is_refused() {
        let mut jwk = a_key();
        jwk["crv"] = Value::String("P-384".into());
        assert!(saved_dpop_key(&jwk).is_err());
    }
}
