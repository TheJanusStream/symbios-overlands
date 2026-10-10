//! Berlin's street house in the pirate theme's dress (#1598): a Caribbean
//! colonial townhouse on a harbour street - a coral-stone ground storey
//! of arched shop bays or barred parlour windows, limewashed upper
//! storeys in the sun-faded colours of a port, every window between a pair
//! of louvred shutters, timber galleries on the storeys over the street,
//! and a shingle roof or a flat one behind a parapet.
//!
//! - **One house, one limewash.** The colour is rolled once at the lot and
//!   inherited by every upper wall below it that names no material of its
//!   own; the stone ground storey, the shutters, the galleries and the roof
//!   name theirs.
//! - **Shops or a home.** A trading house has shop bays either side of its
//!   entrance, each a window over a plinth with a door where the bay is
//!   wide enough; a house of homes has tall parlour windows over a sill.
//! - **Shutters, one colour a house** (`Pick`ed once): a leaf either side
//!   of every front window, standing proud of the limewash.
//! - **Galleries** (`Pick`ed once per house): a timber balcony with a
//!   railing at every window of every upper storey - a gallery the whole
//!   frontage where the bays meet - or none.
//! - **The roof is shingle or flat**, `Pick`ed once: a gable along the
//!   street, its ends the party walls, or a deck behind a parapet.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    HULL_OAK, LAMP_TALLOW, OAK_JOINERY, SHINGLE_GREY, STONE_LIME, ashlar, board, limewash, shingle,
    tar,
};

/// The townhouse (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "pirate_street_house",
    name: "Colonial Townhouse",
    description: "A Caribbean colonial townhouse: a coral-stone ground storey of shops or \
                  parlours, limewashed storeys of shuttered windows, timber galleries, and a \
                  shingle or a flat roof.",
    themes: &[ThemeArchetype::Pirate],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // High rooms against the heat: the stone ground storey and the
    // limewashed storeys over it.
    storey_m: (4.4, 3.7),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BB7_0001,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Cornice", "Balcony", "Shutter",
    ],
};

/// A harbour's limewash colours, coral stone, painted shutters, and the
/// oak and shingle over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.86, 0.84, 0.78],
        panes: (2, 4),
        room: LAMP_TALLOW,
        shop: [1.0, 0.84, 0.54],
    });
    m.extend([
        ("Ochre".to_string(), limewash([0.84, 0.66, 0.38])),
        ("Coral".to_string(), limewash([0.86, 0.58, 0.50])),
        ("SeaGreen".to_string(), limewash([0.52, 0.72, 0.62])),
        ("Sky".to_string(), limewash([0.56, 0.70, 0.80])),
        ("Lime".to_string(), limewash([0.90, 0.88, 0.80])),
        ("Stone".to_string(), ashlar([0.74, 0.68, 0.56], 0xA5_0001)),
        ("Trim".to_string(), limewash([0.92, 0.90, 0.84])),
        ("ShutGreen".to_string(), board([0.18, 0.40, 0.30])),
        ("ShutBlue".to_string(), board([0.20, 0.34, 0.52])),
        ("ShutRed".to_string(), board([0.54, 0.18, 0.14])),
        ("Door".to_string(), board(OAK_JOINERY)),
        ("Gallery".to_string(), board(HULL_OAK)),
        ("Rail".to_string(), board([0.86, 0.84, 0.78])),
        ("Shingle".to_string(), shingle(SHINGLE_GREY)),
        ("Deck".to_string(), tar([0.30, 0.28, 0.25])),
        ("Coping".to_string(), ashlar(STONE_LIME, 0xA5_0002)),
    ]);
    m
}
