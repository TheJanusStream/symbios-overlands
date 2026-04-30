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
        self.rotation = if len_sq > 1e-6 {
            let inv = len_sq.sqrt().recip();
            Fp4([rot[0] * inv, rot[1] * inv, rot[2] * inv, rot[3] * inv])
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
