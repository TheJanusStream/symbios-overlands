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

use std::sync::atomic::{AtomicU64, Ordering};

/// The sortable base32 alphabet TIDs are written in.
const TID_ALPHABET: &[u8; 32] = b"234567abcdefghijklmnopqrstuvwxyz";

/// Highest microsecond value this process has already minted a TID at.
///
/// The clock alone cannot carry the uniqueness this key needs (#1120). On
/// wasm `chrono`'s `wasmbind` clock is JavaScript's `Date`, so
/// `timestamp_micros()` is always a multiple of 1000 and every mint inside
/// one millisecond reads the same instant; the 10 clock-id bits do not
/// break the tie either, because the entropy the callers pass is hashed
/// from the owner's DID and is therefore a constant within a repo. Two
/// records minted in one millisecond would land on the same rkey.
static LAST_MICROS: AtomicU64 = AtomicU64::new(0);

/// A TID for this instant, disambiguated by `entropy` (any stable per-owner
/// value; the caller hashes the DID). Successive calls in one process are
/// always distinct and strictly increasing, whatever the clock does.
pub fn tid_now(entropy: u64) -> String {
    let micros = chrono::Utc::now().timestamp_micros().max(0) as u64;
    tid_at(&LAST_MICROS, micros, entropy)
}

/// [`tid_now`] with its clock and its floor supplied, so the minting path —
/// not a re-implementation of it — can be driven with a frozen clock.
fn tid_at(floor: &AtomicU64, micros: u64, entropy: u64) -> String {
    tid_for(advance(floor, micros), entropy)
}

/// The microsecond value to actually encode: the clock reading, or one past
/// the last value minted, whichever is later.
///
/// Trading exactness for uniqueness, and that is the right trade for a
/// record KEY. A burst of 1000 mints inside one millisecond ends a
/// millisecond ahead of the wall clock — a TID's timestamp is a sort order
/// with a plausible time in it, and two records that collide are worth more
/// than a microsecond of accuracy. It also makes the sequence survive a
/// clock that steps backwards (an NTP correction, a suspended laptop),
/// which the raw reading does not.
fn advance(floor: &AtomicU64, micros: u64) -> u64 {
    let mut last = floor.load(Ordering::Relaxed);
    loop {
        let next = micros.max(last.saturating_add(1));
        match floor.compare_exchange_weak(last, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(observed) => last = observed,
        }
    }
}

/// The TID a seeded default's wardrobe record lives at (#1060).
///
/// Deterministic rather than clock-minted, and that is the point: every
/// client derives the same seeded body for an identity, so they must also
/// agree on where it would be stored — otherwise the first save from one
/// device and the first save from another would publish the same body
/// twice under two keys. The encoded "timestamp" is drawn from the seed
/// and means nothing as a time; a TID's ordering only has to be
/// well-defined, and a body nobody has saved yet has no creation moment to
/// tell the truth about.
pub fn tid_for_seed(seed: u64) -> String {
    tid_for(seed >> 11, seed)
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

    /// #1120 — the sequence: mint two records inside one millisecond on
    /// wasm, where `chrono`'s clock is JS `Date` and every reading in that
    /// millisecond is the same microsecond value.
    ///
    /// Against the old `tid_now` — `tid_for(clock_reading, entropy)` with no
    /// floor — all 1000 of these are the same 13-character string, so the
    /// second record's `putRecord` upserts over the first (and since #1185's
    /// `validate_batch`, a bundle holding both fails the whole save instead).
    /// The floor is what makes that impossible rather than merely unlikely.
    ///
    /// Driven through a local floor rather than the process-global one so
    /// the assertion holds under `cargo test`'s thread-per-test runner as
    /// well as nextest's process-per-test. The clock read is the one thing
    /// this cannot cover — there is no seam to freeze `Utc::now()` — so it
    /// is frozen at the function boundary instead.
    #[test]
    fn a_frozen_clock_still_mints_distinct_increasing_tids() {
        let floor = AtomicU64::new(0);
        // A JS `Date` reading: microseconds, always a multiple of 1000.
        let frozen = 1_700_000_000_123_000u64;
        let minted: Vec<String> = (0..1000).map(|_| tid_at(&floor, frozen, 0x2A5)).collect();

        assert_eq!(
            minted
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            minted.len(),
            "1000 mints at one instant, 1000 distinct keys"
        );
        assert!(
            minted.windows(2).all(|w| w[1] > w[0]),
            "and in creation order — the lexicons key on that"
        );
        assert!(
            minted.iter().all(|t| t.len() == 13),
            "still a well-formed TID"
        );
        // The first mint is not pushed off the real instant: the floor only
        // bites once the clock has stopped moving.
        assert_eq!(minted[0], tid_for(frozen, 0x2A5));
    }

    /// A clock that steps backwards — an NTP correction mid-session — must
    /// not re-issue keys the process has already used.
    #[test]
    fn a_backwards_clock_does_not_reissue_a_key() {
        let floor = AtomicU64::new(0);
        let ahead = tid_at(&floor, 1_700_000_000_500_000, 9);
        let behind = tid_at(&floor, 1_700_000_000_000_000, 9);
        assert!(
            behind > ahead,
            "{behind} must still sort after {ahead} despite the earlier clock"
        );
    }

    #[test]
    fn entropy_only_breaks_ties_within_one_microsecond() {
        let a = tid_for(1_700_000_000_000_000, 1);
        let b = tid_for(1_700_000_000_000_000, 2);
        assert_ne!(a, b);
        assert_eq!(a[..11], b[..11], "entropy only reaches the low bits");
    }
}
