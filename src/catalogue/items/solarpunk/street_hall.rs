//! Berlin's hall in the solarpunk theme's dress (#1600): the sheds of an
//! eco-quarter's works - a timber-framed maker hall of larch boards under
//! a sawtooth of solar panels, or, where it trades, a greenhouse market
//! hall glazed to its ridges.
//!
//! - **One hall, one board**, rolled once at the lot; its sides are party
//!   walls, so halls stand flush in a row and a lone hall's ends read as
//!   its boards' own. Dark frame posts stand between its bays.
//! - **A maker hall or a market.** A maker hall (`Trade` 0) has a door for
//!   its hands at one end and bays of wide timber doors along the rest
//!   over lime aprons, some bays boarded, a ribbon of high windows over
//!   them all and climbers on the posts here and there; a market
//!   (`Trade` 1) has tall glazing between timber posts either side of its
//!   doors, planters of greens at the glass's feet and a green canopy
//!   along the whole front.
//! - **The roof** is a maker hall's sawtooth - each tooth's slope a field
//!   of solar panels and its back a glazed north light - or a sod gable
//!   with rows of panels on its sunny slope, `Pick`ed once; a market's is
//!   a row of glass gables to the street, each its own span.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{GLASS_CLEAN, STEEL_WHITE, glass, steel, timber};

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "solarpunk_street_hall",
    name: "Solar Maker Hall",
    description: "A timber maker hall of larch boards and wide doors under a sawtooth of solar \
                  panels and glazed northlights - or, where it trades, a greenhouse market \
                  hall glazed to its ridges.",
    themes: &[ThemeArchetype::Solarpunk],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A maker hall's clear height below, and a gallery storey over it.
    storey_m: (6.0, 4.0),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BAF_0005,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall",
        "Door",
        "Pane",
        "Roof",
        "Trim",
        "Frame",
        "Green",
        "Planter",
        "Canopy",
        "PV",
        "Glasshouse",
        "Fascia",
    ],
};

/// The cottages' cob, lime, larch, sod, greens, glass roof and panels
/// (`street_low::materials`), with silvered and honey board, white steel
/// and the north lights' glass.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.extend([
        ("Silver".to_string(), timber([0.58, 0.56, 0.52])),
        ("Honey".to_string(), timber([0.70, 0.54, 0.32])),
        ("Steel".to_string(), steel(STEEL_WHITE)),
        ("NorthLight".to_string(), glass(GLASS_CLEAN, 0.3)),
    ]);
    m
}
