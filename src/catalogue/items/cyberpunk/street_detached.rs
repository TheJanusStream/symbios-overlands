//! Berlin's detached house in the cyberpunk theme's dress (#1600): a
//! prefab capsule home dropped on a suburban plot - a box of panelled
//! metal or corrugated steel on a concrete pad, slot windows with grille
//! shutters and AC boxes under them, a neon line round its waist, a lit
//! door under a steel hood, and its roof crowded with solar panels and an
//! antenna mast.
//!
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so its roof slab reaches out over them to the lot's edges and
//!   no further.
//! - **One capsule, one skin, one neon.** The cladding is rolled once at
//!   the lot, and the neon's colour - cyan, magenta, lime or amber - is
//!   `Pick`ed once, for the waist line and the door's tube.
//! - **Windows on all four sides**: slot windows, each one glazed or half
//!   behind a grille shutter, an AC box hung under one in three (`%` rules
//!   window by window), storey on storey.
//! - **The roof** (`Pick`ed once): a flat slab reaching out to the lot's
//!   edges with a neon fascia, solar panels and a mast on it, or a
//!   mono-pitch shed roof rising to the back.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{concrete, corrugated, metal};

/// The capsule home (see the module docs); its rules are
/// `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "cyberpunk_street_detached",
    name: "Capsule Home",
    description: "A prefab capsule home on its plot: a panelled metal box with grilled slot \
                  windows and AC units, a neon line round its waist, and solar panels and an \
                  antenna mast on its roof.",
    themes: &[ThemeArchetype::Cyberpunk],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // Prefab modules, stacked: one height each.
    storey_m: (3.1, 2.9),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BA3_0004,
    materials,
    round_meshes: &["Mast"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Neon", "AC", "Grille", "Mast", "Solar", "Canopy",
    ],
};

/// The tenement's metals, concrete, neons, holo signs, AC boxes and
/// grilles, and the capsules' pale and corrugated skins and their solar
/// panels.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_house::materials();
    m.extend([
        ("Capsule".to_string(), metal([0.58, 0.60, 0.63])),
        ("BoxTeal".to_string(), corrugated([0.14, 0.34, 0.32])),
        ("BoxRust".to_string(), corrugated([0.45, 0.28, 0.18])),
        ("Pad".to_string(), concrete([0.30, 0.30, 0.32])),
        ("Solar".to_string(), metal([0.06, 0.10, 0.22])),
    ]);
    m
}
