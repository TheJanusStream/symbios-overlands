//! Berlin's low building in the frontier theme's dress (#1598): where it
//! trades, a general store behind a false front - a plank boardwalk and a
//! tin veranda on posts, display windows either side of a double door, and
//! the store's name on a board high over the street; where it houses
//! people, a row of clapboard cabins under one tin roof, each with its
//! window and its door on a porch.
//!
//! - **The store**: one coat of paint rolled for it, its display windows on
//!   a plinth, the false front flat, stepped or pedimented (`Pick`ed once),
//!   a low tin roof behind it with its gable turned to the street.
//! - **The cabins**: as many as the frontage holds, every household its own
//!   door colour, a porch roof on two posts over each door, and a tin
//!   gable along the street whose ends are the party walls.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{CLAP_RED, CLAP_TAN, CLAP_WHITE, TIN_GREY, WOOD_RAW, canvas, lap_siding, tin};

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "wild_west_street_low",
    name: "Frontier General Store",
    description: "A general store behind a false front, with display windows, a plank \
                  boardwalk and a tin veranda - or, where nothing trades, a row of clapboard \
                  cabins with porches under one tin roof.",
    themes: &[ThemeArchetype::WildWest],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // The store's high sales floor, and a loft storey over it.
    storey_m: (4.0, 3.0),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BB6_0003,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall",
        "Door",
        "Pane",
        "Roof",
        "Trim",
        "Cornice",
        "Fascia",
        "Canopy",
        "Post",
        "Boardwalk",
    ],
};

/// The store's and the cabins' paint over clapboard, raw boards, tin, and
/// the doors, trim and signs over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.36, 0.26, 0.16],
        panes: (2, 2),
        room: [1.0, 0.80, 0.46],
        shop: [1.0, 0.86, 0.58],
    });
    m.extend([
        ("ClapRed".to_string(), lap_siding(CLAP_RED)),
        ("ClapWhite".to_string(), lap_siding(CLAP_WHITE)),
        ("ClapTan".to_string(), lap_siding(CLAP_TAN)),
        ("ClapOchre".to_string(), lap_siding([0.70, 0.55, 0.30])),
        ("Weathered".to_string(), lap_siding([0.52, 0.47, 0.40])),
        ("Trim".to_string(), lap_siding([0.86, 0.84, 0.78])),
        ("Sign".to_string(), canvas([0.16, 0.14, 0.12])),
        ("SignRed".to_string(), canvas([0.46, 0.14, 0.10])),
        ("Lettering".to_string(), canvas([0.88, 0.80, 0.56])),
        ("Door".to_string(), lap_siding([0.30, 0.20, 0.12])),
        ("DoorGreen".to_string(), lap_siding([0.22, 0.32, 0.22])),
        ("DoorBlue".to_string(), lap_siding([0.22, 0.28, 0.40])),
        ("Boardwalk".to_string(), lap_siding(WOOD_RAW)),
        ("Post".to_string(), lap_siding([0.42, 0.32, 0.20])),
        ("Tin".to_string(), tin(TIN_GREY)),
    ]);
    m
}
