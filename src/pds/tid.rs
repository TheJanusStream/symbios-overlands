//! Client-side ATProto TID record keys (#1056).
//!
//! The wardrobe (`network.symbios.avatar.avatar`) and attachment
//! (`network.symbios.overlands.avatar.attachment`) collections are tid-keyed
//! by their lexicons: many records per identity, ordered by creation. Every
//! other overlands collection either uses `rkey = self` or a deterministic
//! content hash ([`super::inventory`]), so this is the first place a TID has
//! to be minted — and it is minted **client-side** rather than delegated to
//! `createRecord`, because the publish paths here are `putRecord`/`applyWrites`
//! upserts that need to know the rkey before the request goes out (and want
//! to keep it stable across retries of the same save).
//!
//! Format per the atproto spec: 13 characters of sortable base32
//! (`234567abcdefghijklmnopqrstuvwxyz`), encoding a 64-bit value of
//! `(microseconds since UNIX epoch) << 10 | 10-bit clock id`, top bit zero.
//! The clock id only disambiguates two TIDs minted in the same microsecond
//! in the same repo, so entropy hashed from the owner's DID is plenty —
//! this deliberately avoids `getrandom`, which needs a JS backend on wasm.
//!
//! The clock is [`chrono::Utc`], which is wasm-safe through the `wasmbind`
//! feature already in the tree (#846) — `std::time` panics on wasm32.

/// The sortable base32 alphabet TIDs are written in.
const TID_ALPHABET: &[u8; 32] = b"234567abcdefghijklmnopqrstuvwxyz";

/// A TID for this instant, disambiguated by `entropy` (any stable per-owner
/// value; the caller hashes the DID). Later calls in the same process sort
/// after earlier ones whenever the microsecond clock has advanced.
pub fn tid_now(entropy: u64) -> String {
    let micros = chrono::Utc::now().timestamp_micros().max(0) as u64;
    tid_for(micros, entropy)
}

/// The TID for a given microsecond timestamp and entropy — split from
/// [`tid_now`] so tests can pin exact strings without a clock.
pub fn tid_for(micros: u64, entropy: u64) -> String {
    // 53 usable timestamp bits (top bit must stay 0), 10 clock-id bits.
    let value = ((micros & 0x001F_FFFF_FFFF_FFFF) << 10) | (entropy & 0x3FF);
    let mut out = [0u8; 13];
    for (index, slot) in out.iter_mut().enumerate() {
        // 13 base32 digits cover 65 bits; the first digit carries the two
        // top (always-zero) bits, so shift from the high end downward.
        let shift = 60usize.saturating_sub(index * 5);
        *slot = TID_ALPHABET[((value >> shift) & 0x1F) as usize];
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tid_is_thirteen_chars_of_the_sortable_alphabet() {
        let tid = tid_now(0x2A5);
        assert_eq!(tid.len(), 13);
        assert!(tid.bytes().all(|b| TID_ALPHABET.contains(&b)), "{tid}");
    }

    #[test]
    fn later_microseconds_sort_later() {
        // Lexicographic order IS creation order — the property the lexicons
        // key on, and the reason the alphabet is the sortable one rather
        // than RFC 4648.
        let earlier = tid_for(1_700_000_000_000_000, 5);
        let later = tid_for(1_700_000_000_000_001, 5);
        assert!(later > earlier);
    }

    #[test]
    fn entropy_only_breaks_ties_within_one_microsecond() {
        let a = tid_for(1_700_000_000_000_000, 1);
        let b = tid_for(1_700_000_000_000_000, 2);
        assert_ne!(a, b);
        assert_eq!(a[..11], b[..11], "entropy only reaches the low bits");
    }
}
