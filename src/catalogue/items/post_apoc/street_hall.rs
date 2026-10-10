//! Berlin's hall in the wasteland theme's dress (#1600): where it works, a
//! salvage depot in a works hall that outlived its estate - rusted and
//! patched corrugated sheet, its roller doors jammed half down, boarded or
//! gone to a tarp, its ribbon windows broken; where it trades, a scrap
//! market - stalls behind half-drawn shutters under a tarp awning on
//! rusted posts, a hand-painted sign and worklights.
//!
//! - **One hall, one sheet**, rolled once at the lot; its ends are its
//!   party walls, blank, so halls stand flush in a row. Bay by bay a `%`
//!   rule patches it with another sheet, a blue or green panel off
//!   something bigger.
//! - **Every window is a window still**: the ribbon of high windows is
//!   glazed, boarded or sheeted pane by pane.
//! - **A depot or a market.** A depot (`Trade` 0) has a plank door at one
//!   end and bays of roller doors along the rest, each over a concrete
//!   apron and each jammed half down, boarded across or hung with a tarp,
//!   some bays blank; a market (`Trade` 1) has its stalls along the whole
//!   front, under the awning and a row of sign planks and worklights.
//! - **The roof** is a works' low gable or its sawtooth, `Pick`ed once,
//!   its sheet patched strip by strip with rust.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{WORKLIGHT, sheet, tarp};

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "post_apoc_street_hall",
    name: "Salvage Depot",
    description: "A works hall of rusted, patched sheet: roller doors jammed, boarded or hung \
                  with tarp under broken high windows - or, where it trades, a scrap market under \
                  a tarp awning and hand-painted signs.",
    themes: &[ThemeArchetype::PostApoc],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A works hall's clear height, and an office storey over it.
    storey_m: (6.0, 3.8),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BB5_0005,
    materials,
    round_meshes: &["Post"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Board", "Patch", "Shutter", "Fascia", "Post",
        "Awning", "Lamp",
    ],
};

/// The tenement's palette - render, concrete, plank, sheet, rust, tarp and
/// sign - with the shanties' painted sheet, a red tarp, the roof sheet and
/// the worklight.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_house::materials();
    m.extend([
        ("BlueSheet".to_string(), sheet([0.26, 0.34, 0.40])),
        ("GreenSheet".to_string(), sheet([0.32, 0.38, 0.28])),
        ("TarpRed".to_string(), tarp([0.46, 0.20, 0.16])),
        ("Roof".to_string(), sheet([0.30, 0.27, 0.24])),
        ("Lamp".to_string(), glow(WORKLIGHT, 2.4)),
    ]);
    m
}
