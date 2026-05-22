//! Sanitiser for [`WaterSurface`] plus a thin helper invoked by the
//! `GeneratorKind::Water` arm of [`super::sanitize_generator`].
//! Without these clamps a hostile record can push NaN/infinity into
//! the per-fragment Gerstner-wave math, producing world-corrupting
//! normals.

use super::Sanitize;
use super::common::clamp_finite;
use super::limits;
use crate::pds::generator::WaterSurface;
use crate::pds::types::{Fp, Fp2, Fp4};

impl Sanitize for WaterSurface {
    fn sanitize(&mut self) {
        let unit = |v: f32, default: f32| clamp_finite(v, 0.0, 1.0, default);
        self.shallow_color = Fp4([
            unit(self.shallow_color.0[0], 0.0),
            unit(self.shallow_color.0[1], 0.0),
            unit(self.shallow_color.0[2], 0.0),
            unit(self.shallow_color.0[3], 1.0),
        ]);
        self.deep_color = Fp4([
            unit(self.deep_color.0[0], 0.0),
            unit(self.deep_color.0[1], 0.0),
            unit(self.deep_color.0[2], 0.0),
            unit(self.deep_color.0[3], 1.0),
        ]);
        self.roughness = Fp(unit(self.roughness.0, 0.14));
        self.metallic = Fp(unit(self.metallic.0, 0.0));
        self.reflectance = Fp(unit(self.reflectance.0, 0.3));
        self.wave_choppiness = Fp(unit(self.wave_choppiness.0, 0.3));
        self.foam_amount = Fp(unit(self.foam_amount.0, 0.25));
        self.wave_scale = Fp(clamp_finite(
            self.wave_scale.0,
            0.0,
            limits::MAX_WAVE_SCALE,
            0.7,
        ));
        self.wave_speed = Fp(clamp_finite(
            self.wave_speed.0,
            -limits::MAX_WAVE_SPEED,
            limits::MAX_WAVE_SPEED,
            1.0,
        ));
        // The shader normalises `wave_direction`; a near-zero vector would
        // produce NaN there, so fall back to the default heading when the
        // sanitised components round to zero.
        let dx = clamp_finite(self.wave_direction.0[0], -10.0, 10.0, 1.0);
        let dz = clamp_finite(self.wave_direction.0[1], -10.0, 10.0, 0.3);
        let len_sq = dx * dx + dz * dz;
        self.wave_direction = if len_sq > 1e-6 {
            Fp2([dx, dz])
        } else {
            Fp2([1.0, 0.3])
        };
        self.flow_strength = Fp(clamp_finite(
            self.flow_strength.0,
            0.0,
            limits::MAX_WATER_FLOW_STRENGTH,
            0.0,
        ));
        self.flow_amount = Fp(clamp_finite(self.flow_amount.0, 0.0, 1.0, 0.0));
        self.wake_strength = Fp(clamp_finite(
            self.wake_strength.0,
            0.0,
            limits::MAX_WATER_WAKE_STRENGTH,
            0.0,
        ));
        // Lower bound of 0.05 m matches the per-pixel footprint fade
        // threshold in detail_normal — anything finer reads as noise.
        self.wake_ripple_wavelength = Fp(clamp_finite(
            self.wake_ripple_wavelength.0,
            0.05,
            limits::MAX_WATER_WAKE_RIPPLE_WAVELENGTH,
            1.5,
        ));
        // Lower bound of 0.1 m keeps the exp(-r/R) factor from dividing
        // by a near-zero radius and producing NaN.
        self.wake_decay_radius = Fp(clamp_finite(
            self.wake_decay_radius.0,
            0.1,
            limits::MAX_WATER_WAKE_DECAY_RADIUS,
            4.0,
        ));
    }
}

/// Sanitise the `WaterSurface` carried by the
/// `GeneratorKind::Water` variant. Stays a free function (rather than
/// folding into the `Sanitize` impl) so the kind dispatcher in
/// [`super::sanitize_generator`] has one consistent call shape across
/// every variant.
pub(super) fn sanitize_water(surface: &mut WaterSurface) {
    surface.sanitize();
}
