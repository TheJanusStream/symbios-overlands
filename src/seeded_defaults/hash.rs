//! Deterministic FNV-1a 64-bit hash.
//!
//! Single source of truth for the DID → seed transform used across the
//! project (terrain seed in [`crate::pds::RoomRecord::default_for_did`],
//! avatar palette in [`crate::pds::AvatarRecord::default_for_did`], and
//! every downstream `seeded_defaults` deriver). Every peer visiting the
//! same DID derives the identical seed locally — there is no
//! authoritative server.

/// FNV-1a 64-bit hash of a string. Bit-exact across platforms by
/// construction (only `u64` ops on `u8` inputs).
pub fn fnv1a_64(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_is_offset_basis() {
        assert_eq!(fnv1a_64(""), 0xcbf29ce484222325);
    }

    #[test]
    fn known_vector_a() {
        // FNV-1a("a") — published reference value.
        assert_eq!(fnv1a_64("a"), 0xaf63dc4c8601ec8c);
    }

    #[test]
    fn distinct_dids_distinct_hashes() {
        let a = fnv1a_64("did:plc:abc");
        let b = fnv1a_64("did:plc:def");
        assert_ne!(a, b);
    }
}
