//! One number per peer that answers "are we looking at the same world?"
//!
//! The whole thin-client premise is that every peer derives the SAME world
//! from the same record: the record is small and travels, the world is large
//! and does not (see [`crate::offload`]). Until #1146 nothing anywhere
//! measured whether that held. Both desyncs in this project's history — #51's
//! terrain mismatch and #882's lots and roads — reached the tracker as a user
//! saying the two screens looked different, because that was genuinely the
//! only evidence available: no peer computed a content hash of what it had
//! built, so no two peers could compare.
//!
//! ## What the digest is made of
//!
//! Three parts, each written by whoever derives it, combined only once all
//! three exist ([`WorldDigest::combined`]):
//!
//! * the **heightmap** — a hash of the sample grid, the direct output of the
//!   erosion and octave passes;
//! * the **splat weight map** — the per-texel channel weights, i.e. which
//!   ground material won at each cell;
//! * the **compile** — the placement fingerprints in index order plus the
//!   entity count the compile actually produced.
//!
//! Each part sees something the others cannot. The heightmap catches a
//! divergent terrain derivation; the splat map catches a flipped channel
//! choice, which is a *discrete* decision and so the one place a one-ULP
//! difference becomes visible ground texture; the compile part's entity count
//! catches a scatter that accepted a different number of instances.
//!
//! ## What it deliberately cannot see
//!
//! The compile part's fingerprints are a function of the RECORD, not of the
//! geometry the record derived into — they are the planner's change-detection
//! keys, reused here. So two peers that read the same record and then built
//! *differently shaped* trees at the *same count* agree on this digest. That
//! is not an oversight to fix by hashing every spawned transform: the entity
//! count is the derived quantity that the known divergence class (#1132's
//! slope accept/reject) actually moves, and hashing settled transforms would
//! have to run a frame after the compile, decoupling the number from the
//! event that reports it.
//!
//! Nor does it see sub-quantum float noise, on purpose — see
//! [`LENGTH_QUANTUM_M`].
//!
//! ## Advisory, always
//!
//! Digests are exchanged over the P2P wire and NOTHING gates on them. A peer
//! can send any number it likes; the worst outcome is a false
//! `PeerWorldDigestMismatch` in the log of whoever received it. That is the
//! right trade for a diagnostic: an instrument that can refuse service is an
//! instrument nobody dares leave on.

use bevy::prelude::*;

use crate::pds::RoomRecord;

/// Quantum for a length before it enters a digest: one millimetre.
///
/// Raw `f32` bits would be the obvious thing to hash and are the wrong thing.
/// Rust documents `f32::sin`/`cos`/`powf`/`exp` as platform-dependent, and the
/// derivation runs on them, so a native peer and a wasm peer can legitimately
/// differ in the last bit of a height that neither user could ever see. A
/// digest over raw bits would report those as a desync every time, and an
/// instrument that fires on every session is one that gets muted.
///
/// A millimetre is far below anything a player can perceive in a world scaled
/// in metres and far above the ULP noise (an f32 at 100 m has a ULP near
/// 8 µm), so the digest reports divergence a user could see and stays quiet
/// about arithmetic that merely rounded differently.
pub const LENGTH_QUANTUM_M: f32 = 0.001;

/// FNV-1a 64-bit. Chosen over a stronger hash because this is a
/// divergence detector, not a security boundary — collisions cost a missed
/// report, and there is no adversary who benefits from forging one (a peer
/// that wants to lie about its digest just sends a different number).
/// Vendoring twelve lines beats a dependency for that.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Incremental FNV-1a accumulator.
#[derive(Clone, Copy, Debug)]
pub struct Hasher(u64);

impl Default for Hasher {
    fn default() -> Self {
        Self(FNV_OFFSET)
    }
}

impl Hasher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }

    pub fn write_u64(&mut self, v: u64) {
        self.write(&v.to_le_bytes());
    }

    pub fn write_i64(&mut self, v: i64) {
        self.write(&v.to_le_bytes());
    }

    /// The digest so far. Never zero — a zero digest is reserved as "nothing
    /// was hashed", which keeps an empty accumulator from reading as a
    /// legitimate agreement between two peers who each built nothing.
    pub fn finish(self) -> u64 {
        if self.0 == 0 { FNV_OFFSET } else { self.0 }
    }
}

/// Quantise a length to [`LENGTH_QUANTUM_M`] for hashing.
///
/// A non-finite input lands on 0 through the saturating float→int cast, which
/// is deterministic on every target — the only property this needs. It is also
/// a value a real height can take, so a NaN and a zero hash alike; that is
/// acceptable because a NaN in the heightmap is a defect the terrain rules
/// catch on their own, not one this digest is watching for.
pub fn quantise(v: f32) -> i64 {
    (v / LENGTH_QUANTUM_M).round() as i64
}

/// Digest of a heightmap's sample grid, with its dimensions folded in so two
/// grids of different shape cannot collide by holding the same values.
pub fn heightmap_digest(width: u32, height: u32, samples: &[f32]) -> u64 {
    let mut h = Hasher::new();
    h.write_u64(u64::from(width));
    h.write_u64(u64::from(height));
    for s in samples {
        h.write_i64(quantise(*s));
    }
    h.finish()
}

/// Digest of a splat weight map's texels. Already `u8` per channel on the way
/// to the GPU, so no quantisation is needed or wanted: these bytes ARE the
/// discrete decision, and a difference in one of them is a texel of ground
/// that two peers texture differently.
pub fn splat_digest(width: u32, height: u32, texels: &[u8]) -> u64 {
    let mut h = Hasher::new();
    h.write_u64(u64::from(width));
    h.write_u64(u64::from(height));
    h.write(texels);
    h.finish()
}

/// Digest of one finished compile: the placement fingerprints in index order,
/// plus the number of entities the compile actually spawned.
///
/// Index order, not sorted: index IS placement identity here (the planner
/// keys `CompiledWorld` by it), so two records that differ only by the order
/// of their placements are two different records and should read as such.
pub fn compile_digest<'a>(
    fingerprints: impl IntoIterator<Item = Option<&'a str>>,
    entities_spawned: u32,
) -> u64 {
    let mut h = Hasher::new();
    for (index, fp) in fingerprints.into_iter().enumerate() {
        h.write_u64(index as u64);
        match fp {
            // A unit whose fingerprint failed to serialise is "always
            // rebuild" to the planner; here it is its own distinct state, so
            // a peer that could fingerprint a unit and one that could not do
            // not silently agree.
            None => h.write(b"\0unfingerprinted"),
            Some(s) => h.write(s.as_bytes()),
        }
    }
    h.write_u64(u64::from(entities_spawned));
    h.finish()
}

/// Content fingerprint of the record itself: what the world was derived FROM.
///
/// Two peers only compare world digests when this matches — otherwise a
/// difference says nothing more interesting than "one of us has not received
/// the owner's latest edit yet", which is ordinary and constant during a
/// slider drag.
///
/// Routed through `serde_json::to_value` rather than `to_vec` because
/// `RoomRecord` holds `HashMap` fields: `Value::Object` is BTreeMap-backed
/// (the `preserve_order` feature is off), so the rendering is key-sorted and
/// therefore identical on two peers whose hash iteration order is not. The
/// same reason [`crate::world_builder`]'s `unit_fingerprint` takes that
/// route.
pub fn record_fingerprint(record: &RoomRecord) -> u64 {
    let Ok(value) = serde_json::to_value(record) else {
        // A record that will not serialise has no stable identity to offer,
        // so give it one that can never match a real fingerprint rather than
        // a zero that would match every other failure.
        return u64::MAX;
    };
    let mut h = Hasher::new();
    h.write(value.to_string().as_bytes());
    h.finish()
}

/// The local peer's world digest, assembled from its three parts as their
/// producers finish.
///
/// Parts arrive out of order and at different rates: the heightmap and splat
/// map are rebuilt only when the terrain config changes, the compile runs on
/// every record edit. [`Self::combined`] therefore yields `None` until all
/// three are present, so a peer never broadcasts a digest of a half-built
/// world and provokes a mismatch against a peer that finished.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldDigest {
    /// Fingerprint of the record all three parts were derived from.
    pub record_fp: u64,
    pub heightmap: Option<u64>,
    pub splat: Option<u64>,
    pub compile: Option<u64>,
}

impl WorldDigest {
    /// Adopt a record fingerprint, clearing every part derived from a
    /// different one.
    ///
    /// Without this a room switch (or an owner edit) would combine the new
    /// record's compile with the old record's terrain and broadcast a digest
    /// of a world that never existed on any peer.
    pub fn retarget(&mut self, record_fp: u64) {
        if self.record_fp != record_fp {
            *self = Self {
                record_fp,
                ..Default::default()
            };
        }
    }

    /// The single number peers compare, or `None` while any part is missing.
    pub fn combined(&self) -> Option<u64> {
        let (hm, splat, compile) = (self.heightmap?, self.splat?, self.compile?);
        let mut h = Hasher::new();
        h.write_u64(self.record_fp);
        h.write_u64(hm);
        h.write_u64(splat);
        h.write_u64(compile);
        Some(h.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pair of properties that make this a usable instrument (#1146): a
    /// digest must be the SAME for the same world and DIFFERENT for a
    /// different one. A hash that fails the first cries wolf on every
    /// session; one that fails the second is a constant that reports nothing.
    /// Everything downstream — the peer exchange, the mismatch rule, the
    /// determinism goldens #1132 and #1133 are scored against — assumes both.
    #[test]
    fn a_digest_is_stable_for_one_world_and_moves_for_another() {
        let samples: Vec<f32> = (0..64).map(|i| i as f32 * 0.37).collect();
        let a = heightmap_digest(8, 8, &samples);
        assert_eq!(
            a,
            heightmap_digest(8, 8, &samples),
            "same grid, same digest"
        );

        let mut moved = samples.clone();
        // One centimetre on one sample of four thousand — well over the
        // quantum, well under anything the eye would call a different world.
        moved[17] += 0.01;
        assert_ne!(a, heightmap_digest(8, 8, &moved), "a moved sample moves it");

        assert_ne!(
            a,
            heightmap_digest(4, 16, &samples),
            "the same values in a differently-shaped grid are a different world"
        );
    }

    /// Below the quantum the digest holds still, and that is the point: two
    /// peers whose transcendentals differ in the last bit (#1132) have not
    /// desynced in any sense a user could observe, and a digest that fired on
    /// it would be muted within a session.
    #[test]
    fn sub_quantum_noise_does_not_move_the_digest() {
        let samples: Vec<f32> = (0..64).map(|i| i as f32 * 0.37).collect();
        let mut noisy = samples.clone();
        for s in noisy.iter_mut() {
            // A few ULPs at these magnitudes — orders of magnitude under a
            // millimetre.
            *s = f32::from_bits(s.to_bits() + 2);
        }
        assert_eq!(
            heightmap_digest(8, 8, &samples),
            heightmap_digest(8, 8, &noisy),
            "ULP-scale drift is not a desync"
        );
    }

    /// The splat map is where a one-ULP difference stops being invisible: the
    /// weights are `u8`, so a borderline channel score that rounds the other
    /// way is a texel of ground the two peers texture differently. No
    /// quantisation here — these bytes are already the decision.
    #[test]
    fn one_flipped_splat_texel_moves_the_digest() {
        let texels: Vec<u8> = (0..256).map(|i| (i % 251) as u8).collect();
        let a = splat_digest(8, 8, &texels);
        let mut flipped = texels.clone();
        flipped[100] = flipped[100].wrapping_add(1);
        assert_eq!(a, splat_digest(8, 8, &texels));
        assert_ne!(a, splat_digest(8, 8, &flipped));
    }

    /// The compile part exists to catch the divergence #1132 names: a scatter
    /// that accepts a different number of instances because a slope compare
    /// landed the other way. Same record, one tree fewer, different digest.
    #[test]
    fn a_differing_entity_count_moves_the_compile_digest() {
        let fps = [Some("unit-a"), None, Some("unit-b")];
        let a = compile_digest(fps.iter().copied(), 4096);
        assert_eq!(a, compile_digest(fps.iter().copied(), 4096));
        assert_ne!(
            a,
            compile_digest(fps.iter().copied(), 4095),
            "one instance fewer is the shape a slope accept/reject flip takes"
        );
        // Placement order is placement identity — the planner keys its
        // compiled units by index — so reordering is a different world.
        let reordered = [Some("unit-b"), None, Some("unit-a")];
        assert_ne!(a, compile_digest(reordered.iter().copied(), 4096));
    }

    /// A half-built world has no digest to offer. Before this rule existed the
    /// obvious implementation — combine whatever parts you have — would have
    /// had every peer broadcast a mismatch during its own loading screen.
    #[test]
    fn a_digest_is_withheld_until_every_part_has_landed() {
        let mut d = WorldDigest::default();
        d.retarget(7);
        assert_eq!(d.combined(), None);
        d.heightmap = Some(1);
        d.splat = Some(2);
        assert_eq!(d.combined(), None, "two of three is not a world");
        d.compile = Some(3);
        let full = d.combined().expect("all three parts landed");

        // And a part that changes changes the answer.
        d.compile = Some(4);
        assert_ne!(d.combined(), Some(full));
    }

    /// Retargeting to a new record must not leave the old record's terrain
    /// combined with the new record's compile — that digest describes a world
    /// no peer ever built, so it would mismatch against everybody.
    #[test]
    fn retargeting_to_a_new_record_drops_the_old_records_parts() {
        let mut d = WorldDigest::default();
        d.retarget(7);
        d.heightmap = Some(1);
        d.splat = Some(2);
        d.compile = Some(3);
        let before = d.combined();

        d.retarget(7);
        assert_eq!(d.combined(), before, "the same record keeps its parts");

        d.retarget(8);
        assert_eq!(
            d.combined(),
            None,
            "a different record starts from nothing rather than mixing"
        );
    }

    /// Two peers compare digests only when they agree on what they are
    /// deriving, so the record fingerprint has to be stable across the
    /// `HashMap` iteration order that differs run to run and peer to peer.
    #[test]
    fn a_record_fingerprint_survives_hash_iteration_order() {
        let mut a = RoomRecord::default();
        let mut b = RoomRecord::default();
        for name in ["oak", "pine", "rock", "hut", "well", "path", "reed", "sun"] {
            a.generators
                .insert(name.to_string(), crate::pds::Generator::default());
        }
        // Same entries, inserted in the opposite order: a `HashMap` may well
        // iterate them differently, and the fingerprint must not care.
        for name in ["sun", "reed", "path", "well", "hut", "rock", "pine", "oak"] {
            b.generators
                .insert(name.to_string(), crate::pds::Generator::default());
        }
        assert_eq!(record_fingerprint(&a), record_fingerprint(&b));

        b.generators
            .insert("extra".to_string(), crate::pds::Generator::default());
        assert_ne!(
            record_fingerprint(&a),
            record_fingerprint(&b),
            "a record with one more generator is a different record"
        );
    }
}
