//! Berlin's hall in the classical theme's dress (#1600): a Roman horreum -
//! the brick warehouse of Ostia's docks, its doors in travertine portals
//! and its windows slits set high - or, where it trades, a macellum's
//! market hall, an arcade of shop bays between travertine piers round a
//! columned portal.
//!
//! - **One hall, one face.** Its brick or plaster is rolled once at the lot;
//!   its gable ends are its party walls, blank, so halls stand flush in a
//!   row.
//! - **A horreum or a macellum.** A works hall (`Trade` 0) has its cart doors
//!   in travertine portals over stone thresholds, a door for people beside the
//!   first, and blank brick between them; a market hall (`Trade` 1) has shop
//!   bays between travertine piers, each over its counter under a painted sign,
//!   and a portal of two turned columns under a pediment in the middle of its
//!   front. Both have a ribbon of small high windows under brick arches, and an
//!   upper storey of them where it has one.
//! - **A low tiled roof** along the street over a travertine cornice.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{SANDSTONE_WEATHERED, brick, sandstone};

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "ancient_street_hall",
    name: "Roman Horreum",
    description: "A Roman warehouse of brick: cart doors in travertine portals under a ribbon of \
                  small arched windows and a low tiled roof - or, where it trades, a market \
                  hall's arcade of shop bays round a columned portal.",
    themes: &[ThemeArchetype::AncientClassical],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A warehouse's tall store, and a floor of granaries over it.
    storey_m: (6.0, 4.0),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BA1_0005,
    materials,
    round_meshes: &["Column"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Cornice", "Column", "Fascia",
    ],
};

/// The tabernae's and the domus's palette (`street_low`), a darker brick
/// and the weathered tufa of a warehouse's footings.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.extend([
        ("BrickDark".to_string(), brick([0.55, 0.29, 0.19])),
        ("Tufa".to_string(), sandstone(SANDSTONE_WEATHERED)),
    ]);
    m
}
