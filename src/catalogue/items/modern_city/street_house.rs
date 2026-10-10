//! Berlin's street house in the modern city's dress (#1598): the Altbau of
//! the Gruenderzeit as the inner districts stand - a rendered or
//! yellow-brick front over a ground floor of shops, tall windows under
//! stucco hoods, a stack of balconies at each end where the house has them,
//! a heavy cornice, and a slate mansard or a flat roof behind the attic.
//!
//! The reference street building: the conventions every theme's street
//! buildings keep are in [`crate::catalogue::items::street`], and this, with
//! its rules in `street_house.cga`, is how they read in a grammar.
//!
//! - **One house, one cladding.** The cladding is rolled once at the lot
//!   and inherited by every wall below it that names no material of its
//!   own; trim, doors, the fascia and the roof name theirs.
//! - **Shops or a home.** A trading house (`Trade`) has shop bays either
//!   side of its entrance, each a window over a plinth, a door where the
//!   bay is wide enough, and a fascia for its sign; a house of homes has
//!   raised ground-floor windows over a high sill.
//! - **Proud parts are cut from the full face**, never from the inset one:
//!   the string course over the ground floor and the cornice run the whole
//!   frontage, standing outside the wall's plane, so a row of houses shows
//!   one unbroken line where its neighbours' courses meet at the party
//!   wall. Behind them is nothing - the course is the wall there.
//! - **Balconies stack.** `Pick` decides once per house whether its end
//!   bays carry balconies, so a house has a balcony on every storey of its
//!   end bays or none: a solid balustrade, as the Altbau's often are.
//! - **The roof is a mansard or flat**, `Pick`ed once: a gambrel along the
//!   street, its gable ends the party walls - so a row's mansards meet
//!   gable to gable - or a deck behind the attic.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{LAMP_WARM, brick, concrete, enamel, slate, stucco, timber};

/// The house (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "city_street_house",
    name: "City Street House",
    description: "A Berlin Altbau: a rendered or yellow-brick front over shops, tall windows \
                  under stucco hoods, a cornice, and a slate mansard or a flat roof.",
    themes: &[ThemeArchetype::ModernCity],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // The Altbau's tall rooms: the ground storey, plinth to string course,
    // and every storey above it.
    storey_m: (4.4, 3.6),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BA0_0001,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Cornice", "Fascia", "Balcony",
    ],
};

/// Berlin's render colours, its yellow clinker, and the trim, doors, signs
/// and roofs over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.88, 0.87, 0.83],
        panes: (2, 3),
        room: LAMP_WARM,
        shop: [1.0, 0.93, 0.78],
    });
    m.extend([
        ("Ochre".to_string(), stucco([0.80, 0.66, 0.44])),
        ("Grey".to_string(), stucco([0.66, 0.66, 0.64])),
        ("Cream".to_string(), stucco([0.86, 0.82, 0.72])),
        ("Rose".to_string(), stucco([0.80, 0.63, 0.57])),
        ("YellowBrick".to_string(), brick([0.78, 0.66, 0.43])),
        ("Trim".to_string(), stucco([0.88, 0.86, 0.80])),
        ("Fascia".to_string(), enamel([0.10, 0.17, 0.15])),
        ("Door".to_string(), timber([0.33, 0.22, 0.14])),
        ("Slate".to_string(), slate([0.26, 0.28, 0.32])),
        ("Deck".to_string(), concrete([0.21, 0.21, 0.22])),
    ]);
    m
}
