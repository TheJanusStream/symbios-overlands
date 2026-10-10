//! Berlin's low building in the wasteland theme's dress (#1598): where it
//! houses people, a row of scrap shanties knocked together in a gap of the
//! street; where it trades, a scrap trader's shop with a tarp awning on
//! poles and a hand-painted sign.
//!
//! - **The shanties**: shack by shack along the lot, each in its own
//!   sheet (rust, steel, a blue or green panel off something bigger), a
//!   door and a window apiece, a lean-to roof of sheet over the row; where
//!   it has two storeys, a second storey of plank over them.
//! - **The trader's shop**: counter windows behind half-drawn shutters
//!   either side of a door, a tarp awning along the front on rusted posts,
//!   a sign plank over it and a worklight; a flat roof of sheet behind a
//!   parapet.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    CONCRETE_GREY, CORRUGATED_RUST, PLANK_GREY, RUST_BROWN, SIGN_YELLOW, STEEL_GREY, TARP_FADED,
    WORKLIGHT, concrete, enamel, plank, rusted, sheet, tarp,
};

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "post_apoc_street_low",
    name: "Scrap Shanty Row",
    description: "A row of scrap shanties in rusted and salvaged sheet - or, where the street \
                  trades, a scrap trader's shop under a tarp awning and a hand-painted sign.",
    themes: &[ThemeArchetype::PostApoc],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // A shack's low storey, and a plank loft over it.
    storey_m: (3.6, 2.7),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BB5_0003,
    materials,
    round_meshes: &["Post"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Board", "Patch", "Shutter", "Fascia", "Post",
        "Awning", "Lamp",
    ],
};

/// Rusted, bare and painted sheet, salvaged plank and block, and the
/// trader's tarp, sign and worklight.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.34, 0.33, 0.30],
        panes: (2, 1),
        room: [1.0, 0.72, 0.40],
        shop: [1.0, 0.80, 0.52],
    });
    // Lamp and fire light, not mains: the lit rooms glow low and orange.
    m.insert("RoomLit".to_string(), glow([0.92, 0.52, 0.22], 1.3));
    m.extend([
        ("RustSheet".to_string(), sheet(CORRUGATED_RUST)),
        ("Sheet".to_string(), sheet(STEEL_GREY)),
        ("BlueSheet".to_string(), sheet([0.26, 0.34, 0.40])),
        ("GreenSheet".to_string(), sheet([0.32, 0.38, 0.28])),
        ("Plank".to_string(), plank(PLANK_GREY)),
        ("Block".to_string(), concrete(CONCRETE_GREY)),
        ("Rust".to_string(), rusted(RUST_BROWN)),
        ("Tarp".to_string(), tarp(TARP_FADED)),
        ("TarpRed".to_string(), tarp([0.46, 0.20, 0.16])),
        ("Door".to_string(), plank([0.30, 0.26, 0.22])),
        ("Roof".to_string(), sheet([0.30, 0.27, 0.24])),
        ("Sign".to_string(), enamel(SIGN_YELLOW)),
        ("Lamp".to_string(), glow(WORKLIGHT, 2.4)),
    ]);
    m
}
