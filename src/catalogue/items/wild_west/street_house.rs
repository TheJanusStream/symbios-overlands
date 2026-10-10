//! Berlin's street house in the frontier theme's dress (#1598): a
//! false-front hotel on a boomtown's main street - clapboard storeys of
//! narrow sash windows over a ground floor of shops or parlours, a plank
//! boardwalk and a tin veranda along the street, and a tall false front
//! hiding a low tin roof behind its sign.
//!
//! - **One hotel, one coat of paint.** The clapboard's colour is rolled
//!   once at the lot and inherited by every wall below it that names no
//!   material of its own; trim, doors, posts, the sign and the roof name
//!   theirs.
//! - **The boardwalk and the veranda** run the whole frontage, so a row of
//!   copies walks as one boardwalk under one veranda: plank decking a step
//!   high, a tin canopy over the ground floor, and a post at every bay of
//!   the front standing out of the wall pier behind it.
//! - **Shops or parlours.** A trading hotel has shop windows on a plinth
//!   either side of its entrance, a door where a bay is wide enough; one of
//!   homes has parlour windows over a sill.
//! - **The false front** stands over the cornice the whole frontage and
//!   hides the roof: flat, stepped at its shoulders, or a pediment over its
//!   middle (`Pick`ed once), with a painted sign board across it.
//! - **A gallery** (`Pick`ed once): a balustrade along the veranda's roof,
//!   or its plain tin lip.
//! - **A low tin roof** behind it, its gable turned to the street, so the
//!   copies of a row meet eave to eave at their party walls.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{CLAP_RED, CLAP_TAN, CLAP_WHITE, TIN_GREY, WOOD_RAW, canvas, lap_siding, tin};

/// The hotel (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "wild_west_street_house",
    name: "Frontier Hotel",
    description: "A false-front clapboard hotel: sash-windowed storeys over shops or \
                  parlours, a plank boardwalk under a tin veranda, and a tall false front \
                  with its painted sign.",
    themes: &[ThemeArchetype::WildWest],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // A high ground storey of shops and saloon, and timber-framed storeys
    // of hotel rooms over it.
    storey_m: (4.2, 3.4),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BB6_0001,
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
        "Balcony",
    ],
};

/// The boomtown's paint over clapboard, raw boards for the walk, and the
/// tin, trim and signs over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.36, 0.26, 0.16],
        panes: (2, 3),
        room: [1.0, 0.80, 0.46],
        shop: [1.0, 0.86, 0.58],
    });
    m.extend([
        ("ClapRed".to_string(), lap_siding(CLAP_RED)),
        ("ClapWhite".to_string(), lap_siding(CLAP_WHITE)),
        ("ClapTan".to_string(), lap_siding(CLAP_TAN)),
        ("ClapOchre".to_string(), lap_siding([0.70, 0.55, 0.30])),
        ("ClapGreen".to_string(), lap_siding([0.40, 0.48, 0.38])),
        ("Trim".to_string(), lap_siding([0.86, 0.84, 0.78])),
        ("Sign".to_string(), canvas([0.16, 0.14, 0.12])),
        ("SignRed".to_string(), canvas([0.46, 0.14, 0.10])),
        ("Lettering".to_string(), canvas([0.88, 0.80, 0.56])),
        ("Door".to_string(), lap_siding([0.30, 0.20, 0.12])),
        ("Boardwalk".to_string(), lap_siding(WOOD_RAW)),
        ("Post".to_string(), lap_siding([0.42, 0.32, 0.20])),
        ("Tin".to_string(), tin(TIN_GREY)),
    ]);
    m
}
