//! Sanitiser for [`SovereignGrassConfig`] (#1700): each setting a world
//! sets, kept to what the grass can draw and rounded onto the wire's grid
//! as a peer reads it back. An unset setting stays unset.

use super::Sanitize;
use super::common::clamp_finite;
use super::limits;
use crate::pds::grass::SovereignGrassConfig;
use crate::pds::types::{Fp, Fp3};

/// A share or a density, 0 to 1; a non-finite one `default`.
fn unit(v: Fp, default: f32) -> Fp {
    Fp(clamp_finite(v.0, 0.0, 1.0, default)).on_wire()
}

/// A linear colour, each channel 0 to 1; a non-finite channel 0.
fn colour(c: Fp3) -> Fp3 {
    Fp3(c.0.map(|v| clamp_finite(v, 0.0, 1.0, 0.0))).on_wire()
}

impl Sanitize for SovereignGrassConfig {
    fn sanitize(&mut self) {
        for c in [
            &mut self.color_base,
            &mut self.color_tip,
            &mut self.color_dry,
        ]
        .into_iter()
        .flatten()
        {
            *c = colour(*c);
        }
        self.dry_share = self.dry_share.map(|v| unit(v, 0.0));
        self.flower_share = self.flower_share.map(|v| unit(v, 0.0));
        if let Some(flowers) = self.flower_colors.as_mut() {
            flowers.truncate(limits::MAX_GRASS_FLOWER_COLORS);
            for c in flowers.iter_mut() {
                *c = colour(*c);
            }
        }
        if let Some(layers) = self.layer_density.as_mut() {
            for density in layers.iter_mut() {
                *density = unit(*density, 0.0);
            }
        }
        self.thickness = self.thickness.map(|v| unit(v, 1.0));
        self.sway = self
            .sway
            .map(|v| Fp(clamp_finite(v.0, 0.0, limits::MAX_GRASS_SWAY, 1.0)).on_wire());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hostile or mistaken group is kept drawable: colours and shares
    /// into 0 to 1, a NaN to its setting's own rest, the sway to its most,
    /// the flowers to four - and every value onto the wire's grid, so a
    /// sanitised group equals itself read back.
    #[test]
    fn every_setting_is_kept_drawable() {
        let mut grass = SovereignGrassConfig {
            color_base: Some(Fp3([-0.5, 0.3, f32::NAN])),
            color_tip: Some(Fp3([1.5, 0.123_456, 0.5])),
            color_dry: None,
            dry_share: Some(Fp(1.2)),
            flower_share: Some(Fp(f32::NAN)),
            flower_colors: Some(vec![Fp3([0.2, 0.2, 0.2]); 6]),
            layer_density: Some([Fp(-1.0), Fp(0.5), Fp(2.0), Fp(f32::INFINITY)]),
            thickness: Some(Fp(f32::NAN)),
            sway: Some(Fp(9.0)),
        };
        grass.sanitize();
        assert_eq!(grass.color_base, Some(Fp3([0.0, 0.3, 0.0])));
        assert_eq!(grass.color_tip, Some(Fp3([1.0, 0.1235, 0.5])));
        assert_eq!(grass.color_dry, None, "an unset colour stays unset");
        assert_eq!(grass.dry_share, Some(Fp(1.0)));
        assert_eq!(grass.flower_share, Some(Fp(0.0)));
        assert_eq!(grass.flower_colors.as_ref().map(Vec::len), Some(4));
        assert_eq!(
            grass.layer_density,
            Some([Fp(0.0), Fp(0.5), Fp(1.0), Fp(0.0)])
        );
        assert_eq!(
            grass.thickness,
            Some(Fp(1.0)),
            "a NaN thickness grows the grass"
        );
        assert_eq!(grass.sway, Some(Fp(limits::MAX_GRASS_SWAY)));

        let json = serde_json::to_string(&grass).expect("the group writes");
        let back: SovereignGrassConfig = serde_json::from_str(&json).expect("it reads back");
        assert_eq!(back, grass, "a sanitised group is what a peer reads");
    }

    /// A terrain sanitises its grass with it: a received world's hostile
    /// group is kept drawable before the grass reads it.
    #[test]
    fn a_terrain_sanitises_its_grass() {
        let mut terrain = crate::pds::SovereignTerrainConfig::default();
        terrain.grass.sway = Some(Fp(f32::INFINITY));
        terrain.grass.color_tip = Some(Fp3([2.0, -1.0, 0.5]));
        terrain.sanitize();
        assert_eq!(terrain.grass.sway, Some(Fp(1.0)));
        assert_eq!(terrain.grass.color_tip, Some(Fp3([1.0, 0.0, 0.5])));
    }
}
