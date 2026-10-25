//! Places for the crafts a visitor arrives in (#1782): a jetty for a boat, a
//! pad for a helicopter, a mooring mast for an airship, a windsock beside an
//! airstrip and stacked tyres at a car track's bends.
//!
//! A world seeded on its own ground sites them near its arrival
//! (`seeded_defaults::room::crafts`) by slug: they are
//! [`StructureRole::Port`](crate::catalogue::StructureRole::Port) entries,
//! never part of a settlement's pools, and themeless, in the plain timber,
//! concrete, steel and paint any world's visitors know. An airstrip and a
//! track are paint on the ground (`pds::SovereignTrack`); these stand beside
//! them. Every lamp on them shows from dusk to dawn.
//!
//! All are primitive-built (see [`super::util`]).

pub mod helipad;
pub mod jetty;
pub mod mooring_mast;
pub mod tyre_stack;
pub mod windsock;

use super::util::{ageing, tile, tiles_per_metre, tint};
use crate::pds::{
    Fp, Fp3, Fp64, SovereignConcreteConfig, SovereignMaterialSettings, SovereignPlankConfig,
    SovereignTextureConfig,
};

/// Weathered harbour timber: a jetty's deck, its stringers and its piles.
pub(super) const TIMBER: [f32; 3] = [0.40, 0.33, 0.25];
/// Its darker, wetter piles.
pub(super) const TIMBER_DARK: [f32; 3] = [0.22, 0.18, 0.14];
/// Poured concrete: a pad, a mast's footings.
pub(super) const CONCRETE: [f32; 3] = [0.56, 0.55, 0.52];
/// Line paint and a pole's white bands.
pub(super) const PAINT_WHITE: [f32; 3] = [0.88, 0.88, 0.85];
/// A pad's ring.
pub(super) const PAINT_YELLOW: [f32; 3] = [0.92, 0.72, 0.12];
/// A pole's red bands and an aviation light's housing.
pub(super) const SIGNAL_RED: [f32; 3] = [0.74, 0.10, 0.08];
/// A windsock's orange.
pub(super) const SOCK_ORANGE: [f32; 3] = [0.95, 0.40, 0.06];
/// Galvanised steel: a mast's lattice, a jetty's bollards.
pub(super) const STEEL: [f32; 3] = [0.34, 0.35, 0.37];
/// A tyre.
pub(super) const RUBBER: [f32; 3] = [0.05, 0.05, 0.055];

/// Sawn planks with their grain and a few knots.
pub(super) fn timber(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.88),
        uv_scale: tiles_per_metre(tile::PLANK_BOARD * 4.0),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3(tint(color, [1.22, 1.22, 1.22])),
            color_wood_dark: Fp3(tint(color, [0.58, 0.58, 0.58])),
            plank_count: Fp64(4.0),
            knot_density: Fp64(0.25),
            grain_warp: Fp64(0.4),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Timber with no grain of its own, for the parts under a deck - piles,
/// stringers - where a plank's grain would not show and costs its bytes on
/// every one.
pub(super) fn timber_plain(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        ..Default::default()
    }
}

/// Poured concrete, its formwork lines and a little staining.
pub(super) fn concrete(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        uv_scale: tiles_per_metre(tile::CONCRETE),
        texture: SovereignTextureConfig::Concrete(SovereignConcreteConfig {
            color_base: Fp3(color),
            formwork_lines: Fp64(4.0),
            formwork_depth: Fp64(0.08),
            pit_density: Fp64(0.1),
            weathering: ageing::stained(0x1782, 0.6),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Flat paint, matt: line markings, a pole's bands, a sock's cloth.
pub(super) fn paint(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.7),
        ..Default::default()
    }
}

/// Plain steel.
pub(super) fn steel(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.5),
        metallic: Fp(0.6),
        ..Default::default()
    }
}

/// A lamp's glass, lit from dusk to dawn (#1782): an edge light, an
/// aviation light.
pub(super) fn lamp(color: [f32; 3], strength: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint(color, [0.5, 0.5, 0.5])),
        emission_color: Fp3(color),
        emission_strength: Fp(strength),
        dusk_to_dawn: true,
        roughness: Fp(0.3),
        ..Default::default()
    }
}

/// Rubber, worn matt.
pub(super) fn rubber() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(RUBBER),
        roughness: Fp(0.92),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::catalogue::{CatalogueEntry, StructureRole};

    /// Every place for a craft builds a tree the sanitiser leaves alone, is
    /// a place for one, and is themeless, so that no settlement draws it.
    #[test]
    fn every_place_is_a_themeless_port_that_round_trips() {
        let entries: [&dyn CatalogueEntry; 5] = [
            &super::jetty::Jetty,
            &super::helipad::Helipad,
            &super::mooring_mast::MooringMast,
            &super::windsock::Windsock,
            &super::tyre_stack::TyreStack,
        ];
        for entry in entries {
            assert_eq!(entry.role(), StructureRole::Port, "{}", entry.slug());
            assert!(entry.themes().is_empty(), "{}", entry.slug());
            assert!(entry.serves().is_some(), "{}", entry.slug());
            assert_sanitize_stable(&entry.build(""), entry.slug());
        }
    }
}
