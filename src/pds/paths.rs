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
//!
//! Beside them a world may carry tracks (#1782): ways wider than a path,
//! each with a width of its own - a seeded world's airstrip and its car's
//! track - painted and kept clear as the paths are ([`SovereignPathsConfig::ways`]).
//!
//! And fields (#1791): rectangles of ground ploughed into strips, each
//! strip a crop's colour in turn - painted in the paths' layer, kept clear
//! of grass and scatters, and coloured by the ground's own shader
//! ([`SovereignField`]).

use serde::{Deserialize, Serialize};

use super::types::{Fp, Fp2, Fp3};

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
    /// Ways wider than the paths, each with a width of its own (#1782): a
    /// seeded world's airstrip and its car's track, painted in the paths'
    /// layer at their share and kept clear as the paths are. None is not
    /// written, so a record saved before them keeps its bytes.
    pub tracks: Vec<SovereignTrack>,
    /// Fields (#1791): rectangles of ground in strips of crops, painted in
    /// the paths' layer, kept clear of grass and scatters, and coloured by
    /// the ground's shader. None is not written.
    pub fields: Vec<SovereignField>,
}

impl Default for SovereignPathsConfig {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            layer: 1,
            share: Fp(0.85),
            width: Fp(2.5),
            tracks: Vec::new(),
            fields: Vec::new(),
        }
    }
}

crate::pds::serde_util::impl_default_eliding_serialize!(SovereignPathsConfig {
    lines,
    layer,
    share,
    width,
    tracks,
    fields,
});

/// A field (#1791): a rectangle of ground ploughed into strips along its
/// length, each strip one of its crops' colours in turn - ploughed earth,
/// green, golden - its grain the paths' layer's.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SovereignField {
    /// Its middle, world XZ (m).
    pub center: Fp2,
    /// Its length along its strips and its depth across them (m).
    pub size: Fp2,
    /// The compass bearing its strips run along, degrees clockwise from
    /// north (-Z), east (+X) at 90: a field repeats every half turn, so the
    /// sanitiser folds it into `[0, 180)`.
    pub bearing: Fp,
    /// How wide each strip is (m).
    pub strip: Fp,
    /// Its strips' colours in turn, sRGB, one to
    /// [`MAX_FIELD_CROPS`](crate::pds::limits::MAX_FIELD_CROPS).
    pub crops: Vec<Fp3>,
}

impl Default for SovereignField {
    /// A small field of ploughed earth and green wheat.
    fn default() -> Self {
        Self {
            center: Fp2([0.0, 0.0]),
            size: Fp2([40.0, 30.0]),
            bearing: Fp(0.0),
            strip: Fp(5.0),
            crops: vec![Fp3([0.36, 0.25, 0.15]), Fp3([0.34, 0.5, 0.18])],
        }
    }
}

impl SovereignField {
    /// The unit direction its strips run along, world XZ: north is -Z.
    pub fn along(&self) -> [f32; 2] {
        let b = self.bearing.0.to_radians();
        [libm::sinf(b), -libm::cosf(b)]
    }

    /// Its four corners, world XZ, in turn round it.
    pub fn corners(&self) -> [[f32; 2]; 4] {
        let [ax, az] = self.along();
        let (across_x, across_z) = (-az, ax);
        let [cx, cz] = self.center.0;
        let (hl, hd) = (self.size.0[0] * 0.5, self.size.0[1] * 0.5);
        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(l, d)| {
            [
                cx + ax * hl * l + across_x * hd * d,
                cz + az * hl * l + across_z * hd * d,
            ]
        })
    }
}

/// A way wider than a path, with a width of its own (#1782): an airstrip,
/// a car's track.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SovereignTrack {
    /// Its line, world XZ, at least two points: a loop ends where it starts.
    pub line: Vec<Fp2>,
    /// How wide it is (m).
    pub width: Fp,
}

impl Default for SovereignTrack {
    fn default() -> Self {
        Self {
            line: Vec::new(),
            width: Fp(6.0),
        }
    }
}

/// A byte read wide, then saturating (#1119).
fn wide_u8<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    Ok(i64::deserialize(d)?.clamp(0, i64::from(u8::MAX)) as u8)
}

impl SovereignPathsConfig {
    /// Whether the world paints no path, no track and no field.
    pub fn is_empty(&self) -> bool {
        self.ways().all(|(line, _)| line.len() < 2) && self.fields.is_empty()
    }

    /// Every way the ground paints and the scatters keep off: each path at
    /// the paths' width, then each track at its own - a line and its width.
    pub fn ways(&self) -> impl Iterator<Item = (&[Fp2], f32)> + '_ {
        let paths = self
            .lines
            .iter()
            .map(|line| (line.as_slice(), self.width.0));
        paths.chain(
            self.tracks
                .iter()
                .map(|track| (track.line.as_slice(), track.width.0)),
        )
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
                ..Default::default()
            },
            ..Default::default()
        };
        let json = serde_json::to_string(&terrain).expect("writes");
        let back: SovereignTerrainConfig = serde_json::from_str(&json).expect("reads");
        assert_eq!(back.paths, terrain.paths);
    }

    /// #1782: paths with no tracks write no tracks key, so every record
    /// saved before them keeps its bytes; set ones go over the wire and back,
    /// and the ground reads every way - the paths at their width, then each
    /// track at its own - a world with only tracks painting something.
    #[test]
    fn tracks_elide_round_trip_and_paint_at_their_own_width() {
        let paths = SovereignPathsConfig {
            lines: vec![vec![Fp2([0.0, 0.0]), Fp2([0.0, 20.0])]],
            ..Default::default()
        };
        let json = serde_json::to_value(&paths).expect("writes");
        assert!(json.get("tracks").is_none(), "{json}");

        let tracked = SovereignPathsConfig {
            tracks: vec![SovereignTrack {
                line: vec![Fp2([30.0, 0.0]), Fp2([110.0, 0.0])],
                width: Fp(12.0),
            }],
            ..paths.clone()
        };
        let json = serde_json::to_string(&tracked).expect("writes");
        let back: SovereignPathsConfig = serde_json::from_str(&json).expect("reads");
        assert_eq!(back, tracked);
        let widths: Vec<f32> = tracked.ways().map(|(_, width)| width).collect();
        assert_eq!(widths, vec![2.5, 12.0]);

        let only = SovereignPathsConfig {
            lines: Vec::new(),
            ..tracked
        };
        assert!(!only.is_empty(), "a world with a track paints it");
    }
}
