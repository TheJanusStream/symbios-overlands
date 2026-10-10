//! Berlin's hall in the cyberpunk theme's dress (#1600): where it trades,
//! a neon-lit megastore - a black box with a holo band the length of its
//! front, a glazed entrance under a lit canopy and a billboard on its roof;
//! where it does not, a drone depot - corrugated steel, grille roller doors
//! in hazard-yellow frames, a ribbon of slot windows, and landing pads
//! ringed in neon on its roof.
//!
//! - **One hall, one skin, one neon.** The cladding is rolled once at the
//!   lot, and the neon's colour `Pick`ed once; its ends are its party
//!   walls, blank, so halls stand flush in a row and a lone hall's ends
//!   read as its own steel.
//! - **A depot or a store.** A depot (`Trade` 0) has a door for people at
//!   one end and bays along the rest, most a grille roller door over a
//!   concrete apron in a hazard frame, some blank; a store (`Trade` 1) has
//!   a glazed entrance between shop windows under a lit canopy, a holo
//!   band over the whole front in panels of their own colours, and a neon
//!   line along its parapet.
//! - **The roof**: a depot's is `Pick`ed once - a deck behind a parapet
//!   with lit landing pads on it, or a sawtooth of glazed north lights; a
//!   store's a deck with a billboard on legs over its front.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{concrete, corrugated, metal};

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "cyberpunk_street_hall",
    name: "Drone Depot",
    description: "A drone depot of corrugated steel: grille roller doors in hazard frames and \
                  neon-ringed landing pads on its roof - or, where it trades, a black megastore \
                  under a holo band and a rooftop billboard.",
    themes: &[ThemeArchetype::Cyberpunk],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A depot's clear height for its loaders, and a control floor over it.
    storey_m: (6.5, 4.0),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BA3_0005,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall",
        "Door",
        "Pane",
        "Roof",
        "Trim",
        "Neon",
        "Sign",
        "Canopy",
        "Grille",
        "Billboard",
        "Pad",
    ],
};

/// The tenement's metals, concrete, neons, holo signs and grilles, and
/// the depot's corrugated skins, its roller doors, its hazard paint and
/// its landing pads.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_house::materials();
    m.extend([
        ("BoxGrey".to_string(), corrugated([0.32, 0.34, 0.36])),
        ("BoxBlue".to_string(), corrugated([0.18, 0.30, 0.38])),
        ("BoxRust".to_string(), corrugated([0.45, 0.28, 0.18])),
        ("Roller".to_string(), corrugated([0.46, 0.47, 0.48])),
        ("Hazard".to_string(), metal([0.80, 0.60, 0.08])),
        ("Pad".to_string(), concrete([0.20, 0.20, 0.22])),
    ]);
    m
}
