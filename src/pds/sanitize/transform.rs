//! [`TransformData`] sanitiser: clamps every component so downstream
//! Bevy/Avian constructors can't be fed NaN, infinities, or non-positive
//! scales.

use super::Sanitize;
use crate::pds::types::{Fp3, Fp4, TransformData};

impl Sanitize for TransformData {
    fn sanitize(&mut self) {
        let finite = |v: f32, default: f32| if v.is_finite() { v } else { default };
        let clamp_pos = |v: f32| {
            if v.is_finite() {
                v.clamp(0.001, 1_000.0)
            } else {
                1.0
            }
        };
        let clamp_offset = |v: f32| {
            if v.is_finite() {
                v.clamp(-10_000.0, 10_000.0)
            } else {
                0.0
            }
        };
        self.translation = Fp3([
            clamp_offset(self.translation.0[0]),
            clamp_offset(self.translation.0[1]),
            clamp_offset(self.translation.0[2]),
        ]);
        let rot = [
            finite(self.rotation.0[0], 0.0),
            finite(self.rotation.0[1], 0.0),
            finite(self.rotation.0[2], 0.0),
            finite(self.rotation.0[3], 1.0),
        ];
        let len_sq = rot[0] * rot[0] + rot[1] * rot[1] + rot[2] * rot[2] + rot[3] * rot[3];
        // Kept as it is when already unit, so a saved rotation reloads
        // unchanged (#1565); far-off ones keep their direction, unclamped.
        self.rotation = if len_sq > 1e-6 {
            Fp4(super::common::settle_unit_quat(rot))
        } else {
            Fp4([0.0, 0.0, 0.0, 1.0])
        };
        self.scale = Fp3([
            clamp_pos(self.scale.0[0]),
            clamp_pos(self.scale.0[1]),
            clamp_pos(self.scale.0[2]),
        ]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize::common::tests::rotation_sweep;

    fn saved(t: &TransformData) -> String {
        serde_json::to_string(t).expect("serialise")
    }

    fn with_rotation(q: [f32; 4]) -> TransformData {
        TransformData {
            rotation: Fp4(q),
            ..Default::default()
        }
    }

    /// The rule #1565 replaced: renormalise every rotation.
    ///
    /// The test below also saves rotations the wire holds as the identity
    /// though memory does not: their first save must leave the rotation out
    /// as a reload's does, or the two saves differ by a key.
    fn renormalised(q: [f32; 4]) -> [f32; 4] {
        let inv = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3])
            .sqrt()
            .recip();
        q.map(|c| c * inv)
    }

    /// #1565: a rotation written from memory, reloaded and sanitised, saves
    /// again as the same bytes - and sanitise leaves the reloaded record as
    /// it is. Renormalising every rotation broke this for one rotation in
    /// sixty of the sweep (435 of 26 406) on its first reload; the sweep must
    /// still reach such cases.
    #[test]
    fn a_saved_rotation_reloads_and_saves_again_unchanged() {
        let mut moved_by_renormalising = 0;
        let mut written_as_identity = 0;
        for q in rotation_sweep() {
            let mut grown = with_rotation(q);
            grown.sanitize();
            if grown.rotation != Fp4([0.0, 0.0, 0.0, 1.0]) && grown.is_identity() {
                written_as_identity += 1;
            }
            let bytes = saved(&grown);
            let reloaded: TransformData = serde_json::from_str(&bytes).expect("decode");
            let mut settled = reloaded.clone();
            settled.sanitize();
            assert_eq!(settled, reloaded, "sanitise moved the reloaded {q:?}");
            assert_eq!(saved(&settled), bytes, "{q:?} saved again differently");

            let first = saved(&with_rotation(renormalised(q)));
            let decoded: TransformData = serde_json::from_str(&first).expect("decode");
            if saved(&with_rotation(renormalised(decoded.rotation.0))) != first {
                moved_by_renormalising += 1;
            }
        }
        assert!(
            moved_by_renormalising >= 100,
            "the sweep no longer reaches the drift: {moved_by_renormalising} moved"
        );
        assert!(
            written_as_identity > 0,
            "the sweep no longer holds a rotation the wire writes as the identity"
        );
    }
}
