//! The grass a world grows on its terrain, as its owner sets it (#1700).
//!
//! Every setting is optional, and one left unset - Auto, in the World
//! Editor - follows what the world already stores, as the grass always has
//! (`crate::grass::profile`): its colours are the grass tufts its ground
//! cover scatters most of, its flowers its flower cards', and where it grows
//! each ground layer's texture. So a world that sets nothing grows exactly
//! the grass it grew before, and an owner can set one thing - the tips'
//! colour, say - and leave the rest to follow the tufts.
//!
//! These settings shape the grass's look, the same for every visitor. How
//! far out it is drawn and how many blades make it up stay each visitor's own
//! Grass setting, for their hardware (`crate::grass::GrassQuality`).
//!
//! The colours are linear RGB, as the texture configs hold them, so a tuft's
//! colours read here unchanged and the editor's Match the tufts copies these
//! back onto the tufts unchanged (`crate::grass::profile::match_tufts`).

use serde::Deserialize;

use super::types::{Fp, Fp3};

/// The grass a world's terrain grows (#1700): each setting `None` - Auto -
/// follows the world's ground cover and ground layers.
///
/// Default-eliding wire format (#695): an unset setting is left off the
/// record, and a group with none set is left off the terrain.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SovereignGrassConfig {
    /// A blade's colour at its root, linear RGB.
    pub color_base: Option<Fp3>,
    /// Its colour at its tip, linear RGB.
    pub color_tip: Option<Fp3>,
    /// A dry blade's colour, linear RGB.
    pub color_dry: Option<Fp3>,
    /// The share of the blades that are dry, 0 to 1. Dry grass stands taller.
    pub dry_share: Option<Fp>,
    /// The share of the blades that are flowers, 0 to 1.
    pub flower_share: Option<Fp>,
    /// The flowers' colours, linear RGB, at most four; each flower takes one.
    pub flower_colors: Option<Vec<Fp3>>,
    /// How thickly the grass grows on each of the terrain's four ground
    /// layers (`SovereignMaterialConfig::layers`, in their order), 0 (none)
    /// to 1.
    pub layer_density: Option<[Fp; 4]>,
    /// How thick the grass grows on all of them, 0 (no grass in the world)
    /// to 1: each layer's density times this.
    pub thickness: Option<Fp>,
    /// How far the wind leans the grass, as a share of how far it leans
    /// unset: 0 stands still in any wind, 2 leans twice as far.
    pub sway: Option<Fp>,
    /// A blade's height (m) before its own variation (#1702). Unset, the
    /// tufts' dry share sets it: dry grass stands taller.
    pub height: Option<Fp>,
    /// A blade's width at its root (m) (#1702).
    pub width: Option<Fp>,
    /// How much the grass grows in tussocks and clumps (#1702): 0 an even
    /// lawn, 1 as it grows unset, 2 bunchgrass.
    pub clumpiness: Option<Fp>,
}

crate::pds::serde_util::impl_default_eliding_serialize!(SovereignGrassConfig {
    color_base,
    color_tip,
    color_dry,
    dry_share,
    flower_share,
    flower_colors,
    layer_density,
    thickness,
    sway,
    height,
    width,
    clumpiness,
});

impl SovereignGrassConfig {
    /// Whether every setting is Auto: the world grows the grass it would
    /// with no group at all.
    pub fn is_auto(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::SovereignTerrainConfig;

    /// A terrain that sets no grass writes no grass key, so every record
    /// saved before the group existed keeps its bytes, and an all-Auto group
    /// costs a record nothing.
    #[test]
    fn an_all_auto_group_is_left_off_the_terrain() {
        let terrain = SovereignTerrainConfig::default();
        assert!(terrain.grass.is_auto());
        let json = serde_json::to_value(&terrain).expect("the terrain writes");
        assert!(
            json.get("grass").is_none(),
            "an all-Auto group was written: {json}"
        );
    }

    /// A terrain written before the group existed reads as all Auto.
    #[test]
    fn a_terrain_without_the_group_reads_as_auto() {
        let terrain: SovereignTerrainConfig =
            serde_json::from_str("{}").expect("an empty terrain reads");
        assert!(terrain.grass.is_auto());
    }

    /// A set setting goes over the wire and back as the integers it is
    /// written as, and only the set ones are written.
    #[test]
    fn set_settings_round_trip_and_alone() {
        let terrain = SovereignTerrainConfig {
            grass: SovereignGrassConfig {
                color_tip: Some(Fp3([0.5, 0.4, 0.1])),
                flower_share: Some(Fp(0.25)),
                flower_colors: Some(vec![Fp3([0.3, 0.1, 0.6]), Fp3([0.9, 0.9, 0.8])]),
                layer_density: Some([Fp(1.0), Fp(0.5), Fp(0.0), Fp(0.0)]),
                sway: Some(Fp(1.5)),
                height: Some(Fp(0.8)),
                width: Some(Fp(0.09)),
                clumpiness: Some(Fp(1.25)),
                ..SovereignGrassConfig::default()
            },
            ..SovereignTerrainConfig::default()
        };
        let json = serde_json::to_value(&terrain).expect("the terrain writes");
        let grass = json.get("grass").expect("a set group is written");
        let mut keys: Vec<&str> = grass
            .as_object()
            .expect("the group is an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "clumpiness",
                "color_tip",
                "flower_colors",
                "flower_share",
                "height",
                "layer_density",
                "sway",
                "width"
            ]
        );
        assert_eq!(grass["color_tip"], serde_json::json!([5000, 4000, 1000]));
        assert_eq!(grass["sway"], serde_json::json!(15000));
        assert_eq!(grass["height"], serde_json::json!(8000));
        assert_eq!(grass["width"], serde_json::json!(900));
        assert_eq!(grass["clumpiness"], serde_json::json!(12500));
        let back: SovereignTerrainConfig =
            serde_json::from_value(json).expect("the terrain reads back");
        assert_eq!(back.grass, terrain.grass);
    }

    /// A newer client's null for a setting reads as Auto, not as a failed
    /// terrain.
    #[test]
    fn a_null_setting_reads_as_auto() {
        let terrain: SovereignTerrainConfig =
            serde_json::from_str(r#"{"grass": {"sway": null, "thickness": 5000}}"#)
                .expect("the terrain reads");
        assert_eq!(terrain.grass.sway, None);
        assert_eq!(terrain.grass.thickness, Some(Fp(0.5)));
    }
}
