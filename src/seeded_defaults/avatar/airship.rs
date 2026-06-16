//! Airship design knobs for the lighter-than-air default avatar
//! family.
//!
//! Sister to [`super::vessel`]: where `VesselDesign` shapes a
//! hover-boat, `AirshipDesign` shapes an envelope-plus-gondola
//! airship. The ornament kit reuses [`VesselArchetype`] (Steam /
//! Solar / Hybrid) so the steampunk-vs-scifi axis cuts across both
//! vehicle families — a Steam airship mounts a gondola funnel, a
//! Solar one carries a panel on the gondola roof and an antenna on
//! the envelope crown.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::avatar::vessel::VesselArchetype;
use crate::seeded_defaults::hash::fnv1a_64;
use crate::seeded_defaults::scene::{pick, range_f32, unit_f32};

const AVATAR_AIRSHIP_SALT: u64 = 0xA125_41F0_A125_41F0;

/// Envelope silhouette family. Changes the gas-bag's actual shape,
/// not just its trim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopeForm {
    /// Classic cigar — untapered capsule.
    Blimp,
    /// Teardrop — strong stern taper, fat bow.
    Teardrop,
    /// Two slimmer envelopes side by side — catamaran of the sky.
    TwinHull,
}

impl EnvelopeForm {
    pub const ALL: [Self; 3] = [Self::Blimp, Self::Teardrop, Self::TwinHull];
}

/// All seeded airship knobs. Dimension scales are multipliers on the
/// builder's nominal sizes (`1.0` = nominal).
#[derive(Clone, Copy, Debug)]
pub struct AirshipDesign {
    pub archetype: VesselArchetype,
    pub envelope_form: EnvelopeForm,

    /// Envelope capsule radius scale.
    pub envelope_radius_scale: f32,
    /// Envelope capsule length scale (fore-aft).
    pub envelope_length_scale: f32,
    /// Stern taper for [`EnvelopeForm::Teardrop`]; `0.0` otherwise.
    pub envelope_taper: f32,
    /// Vertical gap between gondola roof and envelope belly.
    pub envelope_lift_scale: f32,

    /// Gondola cuboid length / width scales.
    pub gondola_length_scale: f32,
    pub gondola_width_scale: f32,

    /// Stern stabiliser fin count: 3 (Y-config) or 4 (X-config).
    pub fin_count: u32,
    /// Fin span scale.
    pub fin_scale: f32,

    /// Engine pods mounted per gondola side (`0..=2`).
    pub engine_pods_per_side: u32,

    /// Strut pairs connecting gondola to envelope (`2..=3`).
    pub strut_pairs: u32,
}

impl AirshipDesign {
    pub fn for_did(did: &str) -> Self {
        Self::for_seed(fnv1a_64(did))
    }

    /// Derive from a pre-computed seed — the manual re-roll path.
    /// `for_did(did)` is exactly `for_seed(fnv1a_64(did))`.
    pub fn for_seed(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ AVATAR_AIRSHIP_SALT);

        let archetype = pick(&VesselArchetype::ALL, &mut rng);
        let envelope_form = pick(&EnvelopeForm::ALL, &mut rng);

        let envelope_radius_scale = range_f32(&mut rng, 0.85, 1.35);
        let envelope_length_scale = range_f32(&mut rng, 0.90, 1.45);
        let envelope_taper = match envelope_form {
            EnvelopeForm::Teardrop => range_f32(&mut rng, 0.30, 0.55),
            // Sample-and-discard keeps the stream stable if a future
            // form gains a taper band.
            _ => {
                let _ = range_f32(&mut rng, 0.30, 0.55);
                0.0
            }
        };
        let envelope_lift_scale = range_f32(&mut rng, 0.85, 1.25);
        let gondola_length_scale = range_f32(&mut rng, 0.85, 1.30);
        let gondola_width_scale = range_f32(&mut rng, 0.85, 1.25);
        let fin_count = if unit_f32(&mut rng) < 0.5 { 3 } else { 4 };
        let fin_scale = range_f32(&mut rng, 0.80, 1.40);
        let engine_pods_per_side = (unit_f32(&mut rng) * 3.0) as u32; // 0..=2
        let strut_pairs = if unit_f32(&mut rng) < 0.5 { 2 } else { 3 };

        Self {
            archetype,
            envelope_form,
            envelope_radius_scale,
            envelope_length_scale,
            envelope_taper,
            envelope_lift_scale,
            gondola_length_scale,
            gondola_width_scale,
            fin_count,
            fin_scale,
            engine_pods_per_side,
            strut_pairs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = AirshipDesign::for_did("did:plc:test");
        let b = AirshipDesign::for_did("did:plc:test");
        assert_eq!(a.archetype, b.archetype);
        assert_eq!(a.envelope_form, b.envelope_form);
        assert_eq!(a.envelope_radius_scale, b.envelope_radius_scale);
        assert_eq!(a.fin_count, b.fin_count);
    }

    #[test]
    fn fields_in_range() {
        for s in 0u64..64 {
            let a = AirshipDesign::for_did(&format!("did:test:{s}"));
            assert!((0.8..=1.4).contains(&a.envelope_radius_scale));
            assert!((0.8..=1.5).contains(&a.envelope_length_scale));
            assert!((0.0..=0.55).contains(&a.envelope_taper));
            assert!((0.8..=1.3).contains(&a.envelope_lift_scale));
            assert!(a.fin_count == 3 || a.fin_count == 4);
            assert!(a.engine_pods_per_side <= 2);
            assert!(a.strut_pairs == 2 || a.strut_pairs == 3);
        }
    }

    #[test]
    fn taper_only_on_teardrop() {
        for s in 0u64..128 {
            let a = AirshipDesign::for_did(&format!("did:test:{s}"));
            match a.envelope_form {
                EnvelopeForm::Teardrop => assert!(a.envelope_taper >= 0.30),
                _ => assert_eq!(a.envelope_taper, 0.0),
            }
        }
    }
}
