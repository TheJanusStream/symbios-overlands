//! Berlin's street house in the coastal resort's dress (#1598): an Art
//! Deco resort hotel of the beachfront - pastel stucco storeys banded by
//! thin concrete eyebrows over every window, stacked steel-railed
//! balconies, a ground floor of cafe terraces under striped awnings or of
//! lobby windows, and a stepped parapet with a tall sign fin over its
//! entrance.
//!
//! - **One hotel, one pastel.** The stucco's colour is rolled once at the
//!   lot and inherited by every wall below it that names no material of
//!   its own; the eyebrows, the rails, the awnings and the fin name theirs.
//! - **Cafes or a lobby.** A trading hotel has cafe bays either side of
//!   its entrance, each a window over a plinth under a striped awning,
//!   a door where the bay is wide enough; one of rooms has lobby windows.
//! - **Eyebrows**: a thin shading ledge over every front window, the line
//!   Miami Beach is known for, and a white band over the ground storey.
//! - **Balconies stack** (`Pick`ed once): at the end bays of every storey
//!   or not at all, each a slab with a steel rail.
//! - **The crown**: a parapet stepped up over the middle of the front, a
//!   sign fin standing out of it over the entrance, and a flat roof.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::{Fp, Fp3, SovereignMaterialSettings};
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    AWNING_RED, AWNING_TEAL, AWNING_WHITE, LAMP_WARM, STEEL_GREY, STUCCO_WHITE, canvas, concrete,
    enamel, steel, stucco,
};

/// The hotel (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "coastal_resort_street_house",
    name: "Deco Resort Hotel",
    description: "A pastel Art Deco beach hotel: eyebrow ledges over every window, stacked \
                  steel-railed balconies, cafe terraces under striped awnings, and a stepped \
                  parapet with a tall sign fin.",
    themes: &[ThemeArchetype::CoastalResort],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // A lobby's high ground storey, and the hotel's storeys of rooms.
    storey_m: (4.2, 3.2),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BAA_0001,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Canopy", "Balcony", "Fin",
    ],
};

/// A sign's neon: its colour, lit.
fn neon(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        emission_color: Fp3(color),
        emission_strength: Fp(2.4),
        roughness: Fp(0.4),
        ..Default::default()
    }
}

/// The beachfront's pastels over stucco, white bands and eyebrows, steel
/// rails, striped canvas and a neon fin.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.93, 0.92, 0.88],
        panes: (2, 2),
        room: LAMP_WARM,
        shop: [1.0, 0.92, 0.74],
    });
    m.extend([
        ("Pink".to_string(), stucco([0.92, 0.70, 0.70])),
        ("Mint".to_string(), stucco([0.66, 0.86, 0.76])),
        ("Lemon".to_string(), stucco([0.94, 0.88, 0.60])),
        ("Sky".to_string(), stucco([0.66, 0.80, 0.90])),
        ("Lilac".to_string(), stucco([0.78, 0.72, 0.88])),
        ("Trim".to_string(), stucco(STUCCO_WHITE)),
        ("Rail".to_string(), steel(STEEL_GREY)),
        ("Door".to_string(), enamel([0.16, 0.42, 0.44])),
        ("AwningRed".to_string(), canvas(AWNING_RED, AWNING_WHITE)),
        ("AwningTeal".to_string(), canvas(AWNING_TEAL, AWNING_WHITE)),
        ("Fin".to_string(), enamel([0.94, 0.93, 0.90])),
        ("Neon".to_string(), neon([1.0, 0.36, 0.56])),
        ("NeonBlue".to_string(), neon([0.30, 0.82, 0.96])),
        ("Deck".to_string(), concrete([0.62, 0.60, 0.56])),
    ]);
    m
}
