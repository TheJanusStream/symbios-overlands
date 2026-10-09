//! The content hash of an answer (#1590): what a save stores for each layer
//! a world was drawn from, so a visitor whose cache holds older answers can
//! tell and fetch them again.
//!
//! A render is the same bytes for the same URL, day after day, until GDI
//! Berlin re-renders its data; a page of features is too, but for its
//! `"timeStamp"`, the moment GeoServer wrote it. So the page's last
//! timestamp, which is the collection's own (GeoServer writes it after the
//! features), is left out of its hash, and everything else counts.
//!
//! The hash is 64-bit FNV-1a, as the rest of the app's stable hashes are
//! ([`crate::seeded_defaults::fnv1a_64`]): bit-exact on every platform, and
//! enough to tell two answers apart, which is all it is for - it guards no
//! secret.

use super::GeoKind;

/// FNV-1a's offset basis and prime, 64-bit.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// What a page of features calls the moment it was written.
const TIMESTAMP_KEY: &[u8] = br#""timeStamp":""#;

/// The content hash of `body`, an answer of `kind` (see the module docs).
pub fn content_hash(kind: GeoKind, body: &[u8]) -> u64 {
    match (kind == GeoKind::Features)
        .then(|| timestamp_span(body))
        .flatten()
    {
        Some((start, end)) => fnv(fnv(FNV_OFFSET, &body[..start]), &body[end..]),
        None => fnv(FNV_OFFSET, body),
    }
}

/// One hash of several, in order: a layer's, of its answers'.
pub fn combined(hashes: impl IntoIterator<Item = u64>) -> u64 {
    hashes
        .into_iter()
        .fold(FNV_OFFSET, |hash, part| fnv(hash, &part.to_le_bytes()))
}

/// A hash as a record writes it: 16 lower-case hex digits.
pub fn hash_text(hash: u64) -> String {
    format!("{hash:016x}")
}

/// Fold `bytes` into `hash`.
fn fnv(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
    })
}

/// Where the page's last `"timeStamp":"..."` lies, key to closing quote,
/// as a byte range; `None` where it has none, or it is not closed.
fn timestamp_span(body: &[u8]) -> Option<(usize, usize)> {
    let start = body
        .windows(TIMESTAMP_KEY.len())
        .rposition(|window| window == TIMESTAMP_KEY)?;
    let value = start + TIMESTAMP_KEY.len();
    let close = body[value..].iter().position(|&b| b == b'"')?;
    Some((start, value + close + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &[u8] = br#"{"type":"FeatureCollection","features":[{"properties":{"uuid":"A"}}],"numberReturned":1,"timeStamp":"2026-10-08T19:30:27.614Z","crs":{}}"#;

    #[test]
    fn a_pages_timestamp_does_not_count_and_everything_else_does() {
        let later = br#"{"type":"FeatureCollection","features":[{"properties":{"uuid":"A"}}],"numberReturned":1,"timeStamp":"2026-10-09T08:12:00.001Z","crs":{}}"#;
        let other = br#"{"type":"FeatureCollection","features":[{"properties":{"uuid":"B"}}],"numberReturned":1,"timeStamp":"2026-10-08T19:30:27.614Z","crs":{}}"#;
        let features = |body: &[u8]| content_hash(GeoKind::Features, body);
        assert_eq!(features(PAGE), features(later));
        assert_ne!(features(PAGE), features(other));
        // A legend's bytes all count: the same span in one is data.
        assert_ne!(
            content_hash(GeoKind::Legend, PAGE),
            content_hash(GeoKind::Legend, later)
        );
        // Without a timestamp, or with one never closed, the whole body.
        let bare = br#"{"type":"FeatureCollection","features":[]}"#;
        assert_eq!(features(bare), content_hash(GeoKind::Legend, bare));
        let open = br#"{"features":[],"timeStamp":"2026"#;
        assert_eq!(features(open), content_hash(GeoKind::Legend, open));
    }

    #[test]
    fn only_the_collections_own_timestamp_is_left_out() {
        // A feature that carries the same key keeps it: the last is the
        // collection's.
        let a = br#"{"features":[{"p":{"timeStamp":"x"}}],"timeStamp":"1"}"#;
        let b = br#"{"features":[{"p":{"timeStamp":"y"}}],"timeStamp":"1"}"#;
        assert_ne!(
            content_hash(GeoKind::Features, a),
            content_hash(GeoKind::Features, b)
        );
    }

    #[test]
    fn the_hash_is_fnv1a_and_combines_in_order() {
        // FNV-1a("a"), the published reference value.
        assert_eq!(content_hash(GeoKind::Legend, b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(content_hash(GeoKind::Legend, b""), FNV_OFFSET);
        assert_ne!(combined([1, 2]), combined([2, 1]));
        assert_eq!(hash_text(0xab), "00000000000000ab");
        assert_eq!(hash_text(u64::MAX).len(), 16);
    }
}
