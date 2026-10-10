//! Berlin's street house in the hive's dress (#1598): a ribbed chitin tower
//! house - the Altbau's frontage and storeys grown rather than built, a
//! shell of chitin plates ribbed from foot to crown, its windows pods with
//! fleshy lips and a glowing membrane deep in each, a maw for a door, and a
//! carapace or a crown of spines over the top.
//!
//! - **One house, one shell.** The chitin is rolled once at the lot and
//!   inherited by every wall below it that names no material of its own;
//!   the ribs, lips, maw and roof name theirs.
//! - **Ribbed, storey by storey.** Bone ribs stand proud between the pods
//!   and a fleshy roll - a tube laid along the street, half sunk in the
//!   shell - runs the frontage at every floor, so the storeys read as the
//!   segments of one body.
//! - **Grown, not laid out**: every pod is one of three sizes, chosen pod
//!   by pod.
//! - **Pods for windows**: each a deep opening between a fleshy sill and a
//!   fleshy brow, a chitin rim round it and a membrane far back in it,
//!   glowing where the room is lit (the `Glass` and room slots hold the
//!   membrane, not glass).
//! - **A maw for a door**: a fleshy leaf between tapering bone fangs up a
//!   chitin step. A trading house has feeding stalls either side of it,
//!   wide membranes over a plinth under a glowing spore band; a house of
//!   homes has pods over a high sill.
//! - **Brood sacs stack.** `Pick` decides once per house whether its end
//!   bays carry a fleshy sac ledge on every storey or none.
//! - **The crown is a carapace or spines**, `Pick`ed once: a dark
//!   carapace, a turned shell laid on its side along the street, ringed by
//!   bone hoops and stopping short of the party walls; or a deck bristling
//!   with bone spines.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::catalogue::items::util::{glow, window_card};
use crate::pds::{Fp, Fp3, SovereignMaterialSettings};
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BIOLUME_CYAN, BIOLUME_GREEN, CHITIN_DARK, CHITIN_GREEN, FLESH_PINK, FLESH_RED, HUSK, SAC_GLOW,
    chitin, flesh, membrane,
};

/// The tower house (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "alien_organic_street_house",
    name: "Hive Tower House",
    description: "A street house grown of ribbed chitin: segmented storeys, pod windows with \
                  fleshy lips and glowing membranes, a maw for a door, and a carapace or a crown \
                  of spines.",
    themes: &[ThemeArchetype::AlienOrganic],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // A tall ground segment, its maw and stalls, and the brood storeys.
    storey_m: (4.4, 3.5),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BB3_0001,
    materials,
    round_meshes: &["Tube", "Pod", "Carapace", "Hoop"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Rib", "Lip", "Shell", "Fascia", "Spike", "Tube",
        "Pod", "Carapace", "Hoop",
    ],
};

/// A membrane deep in a pod, lit from within: a broad wet surface glowing
/// low (#972 lesson 30), its sheen kept.
pub(crate) fn lit_membrane(color: [f32; 3], strength: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        emission_color: Fp3(color),
        emission_strength: Fp(strength),
        ..membrane(color)
    }
}

/// The hive's openings: a chitin rim round each pod (an alpha card with one
/// light, so the pod shows what is behind it), a membrane glowing teal in a
/// lit pod and a dark wet one in an unlit pod, and a stall's membrane
/// glowing green.
pub(crate) fn pod_materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.14, 0.11, 0.17],
        panes: (1, 1),
        room: BIOLUME_CYAN,
        shop: BIOLUME_GREEN,
    });
    m.extend([
        (
            "Glass".to_string(),
            window_card([0.14, 0.11, 0.17], 1, 1, 0.42, 0.14),
        ),
        (
            "ShopGlass".to_string(),
            window_card([0.14, 0.11, 0.17], 2, 1, 0.38, 0.06),
        ),
        ("RoomLit".to_string(), lit_membrane([0.10, 0.80, 0.70], 1.5)),
        ("RoomDark".to_string(), membrane([0.10, 0.20, 0.19])),
        ("ShopLit".to_string(), lit_membrane([0.30, 0.86, 0.34], 1.6)),
    ]);
    m
}

/// Chitin shells in four hues, the dark ribs, flesh and sinew, and the glow
/// of the spore bands.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = pod_materials();
    m.extend([
        ("ChitinViolet".to_string(), chitin(CHITIN_DARK)),
        ("ChitinGreen".to_string(), chitin(CHITIN_GREEN)),
        ("ChitinAmber".to_string(), chitin([0.34, 0.22, 0.10])),
        ("ChitinRust".to_string(), chitin([0.38, 0.20, 0.18])),
        ("Rib".to_string(), chitin(HUSK)),
        ("Carapace".to_string(), chitin([0.16, 0.12, 0.20])),
        ("Flesh".to_string(), flesh(FLESH_RED)),
        ("Lip".to_string(), flesh(FLESH_PINK)),
        ("Sinew".to_string(), flesh([0.42, 0.20, 0.24])),
        ("Spore".to_string(), glow(SAC_GLOW, 1.8)),
    ]);
    m
}
