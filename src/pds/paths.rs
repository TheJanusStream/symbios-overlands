//! A seeded world's footways (#1779): paths painted on its ground from the
//! landing to its buildings.
//!
//! The paths are routed when a seeded world is derived, on the probe its
//! settlement is sited on (`seeded_defaults::room::paths`) - which turns
//! its buildings to face them (#1780) - and kept here so that every peer
//! draws the same ones.
//! They are drawn as ground paint - each heightmap vertex's splat weights
//! shifted toward one of the terrain's four ground layers by how far
//! inside a path it stands (`terrain::paths`) - and the ground under them
//! grows no grass (`ground_cover`), so nothing but the weights the ground
//! and the grass already read draws them.

use serde::Deserialize;

use super::types::{Fp, Fp2};

/// A world's paths (#1779): each of `lines` at least two points, world XZ,
/// painted `width` metres wide in ground layer `layer` (0 to 3, the
/// material's order), its middle showing `share` of that layer.
///
/// Default-eliding wire format (#695): a terrain with no paths writes no
/// paths key.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SovereignPathsConfig {
    /// The paths, each a polyline of world XZ points.
    pub lines: Vec<Vec<Fp2>>,
    /// The ground layer that paints them (0 to 3), read wide (#1119): a
    /// number past a byte - or under nought - saturates for the sanitiser to
    /// clamp, rather than failing the record's parse and losing the world.
    #[serde(deserialize_with = "wide_u8")]
    pub layer: u8,
    /// How much of that layer a path's middle shows, 0 to 1: a dirt layer
    /// at full share reads as a dark smear across snow.
    pub share: Fp,
    /// How wide each path is (m).
    pub width: Fp,
}

impl Default for SovereignPathsConfig {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            layer: 1,
            share: Fp(0.85),
            width: Fp(2.5),
        }
    }
}

crate::pds::serde_util::impl_default_eliding_serialize!(SovereignPathsConfig {
    lines,
    layer,
    share,
    width,
});

/// A byte read wide, then saturating (#1119).
fn wide_u8<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    Ok(i64::deserialize(d)?.clamp(0, i64::from(u8::MAX)) as u8)
}

impl SovereignPathsConfig {
    /// Whether the world paints no path.
    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|line| line.len() < 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::SovereignTerrainConfig;

    /// A terrain with no paths writes no paths key, so a record saved before
    /// the group existed keeps its bytes; one written without it reads as
    /// none.
    #[test]
    fn a_terrain_without_paths_writes_none_and_reads_none() {
        let terrain = SovereignTerrainConfig::default();
        let json = serde_json::to_value(&terrain).expect("the terrain writes");
        assert!(json.get("paths").is_none(), "{json}");
        let back: SovereignTerrainConfig = serde_json::from_str("{}").expect("reads");
        assert!(back.paths.is_empty());
    }

    /// A layer past a byte, or under nought, reads - saturated - and the
    /// sanitiser keeps it one of the four: never a record lost to it.
    #[test]
    fn a_layer_out_of_range_reads_and_is_clamped() {
        for (layer, read, kept) in [(300, 255, 3), (-1, 0, 0), (2, 2, 2)] {
            let json = format!(r#"{{"paths": {{"layer": {layer}}}}}"#);
            let mut terrain: SovereignTerrainConfig =
                serde_json::from_str(&json).expect("the terrain reads");
            assert_eq!(terrain.paths.layer, read, "{layer}");
            crate::pds::sanitize::Sanitize::sanitize(&mut terrain);
            assert_eq!(terrain.paths.layer, kept, "{layer}");
        }
    }

    /// A set group goes over the wire and back as the integers it is
    /// written as.
    #[test]
    fn paths_round_trip() {
        let terrain = SovereignTerrainConfig {
            paths: SovereignPathsConfig {
                lines: vec![vec![Fp2([1.5, -2.0]), Fp2([10.25, 4.0])]],
                layer: 2,
                share: Fp(0.5),
                width: Fp(3.0),
            },
            ..Default::default()
        };
        let json = serde_json::to_string(&terrain).expect("writes");
        let back: SovereignTerrainConfig = serde_json::from_str(&json).expect("reads");
        assert_eq!(back.paths, terrain.paths);
    }
}
