//! Ticket booth — a Sports/Recreation secondary. A small kiosk with a lit
//! ticket window under a canopy and a pair of turnstiles. The entrance gate
//! of the ground.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the pad.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, GLASS_TINT, SCORE_LIT, SEAT_RED, STEEL_GREY, concrete, enamel, glass, steel,
};

pub struct TicketBooth;

impl CatalogueEntry for TicketBooth {
    fn slug(&self) -> &'static str {
        "ticket_booth"
    }
    fn name(&self) -> &'static str {
        "Ticket Booth"
    }
    fn description(&self) -> &'static str {
        "Kiosk with a lit ticket window under a canopy beside a pair of turnstiles."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SportsRec]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SPORTS_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.5,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let pad_h = 0.3_f32;

    let mut prims = vec![
        // Concrete pad — the root.
        prim(
            solid(cuboid_tapered(
                [5.0, pad_h, 3.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, pad_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Booth box.
    prims.push(prim(
        solid(cuboid_tapered([2.6, 3.0, 2.0], 0.0, enamel(SEAT_RED))),
        [-1.0, pad_h + 1.5, 0.0],
        id_quat(),
    ));
    // Lit ticket window on the −Z render front.
    prims.push(prim(
        cuboid_tapered([1.6, 0.9, 0.15], 0.0, glass(GLASS_TINT, 1.4)),
        [-1.0, pad_h + 1.4, -1.02],
        id_quat(),
    ));
    // Counter shelf proud under the window.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.8, 0.12, 0.4],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [-1.0, pad_h + 0.85, -1.18],
        id_quat(),
    ));
    // Canopy over the window.
    prims.push(prim(
        solid(cuboid_tapered([3.0, 0.2, 1.0], 0.0, enamel(SEAT_RED))),
        [-1.0, pad_h + 2.3, -1.35],
        id_quat(),
    ));
    // Lit TICKETS sign band — deep-saturated so it reads lit, not washed white.
    prims.push(prim(
        cuboid_tapered([2.2, 0.5, 0.1], 0.0, glow(SCORE_LIT, 1.8)),
        [-1.0, pad_h + 2.85, -1.06],
        id_quat(),
    ));

    // Two turnstiles beside the booth, set forward at the entry line.
    for sz in [-0.6_f32, 0.6] {
        prims.push(prim(
            solid(cylinder_tapered(0.12, 1.1, 8, 0.0, steel(STEEL_GREY))),
            [1.6, pad_h + 0.55, sz - 0.8],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.9, 0.08, 0.08], 0.0, steel(STEEL_GREY))),
            [2.0, pad_h + 0.9, sz - 0.8],
            id_quat(),
        ));
    }
    // Queue guide rail feeding the turnstiles.
    prims.push(prim(
        solid(cuboid_tapered([0.06, 0.06, 1.6], 0.0, steel(STEEL_GREY))),
        [1.05, pad_h + 0.5, -0.6],
        id_quat(),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&TicketBooth.build(""), "ticket_booth");
    }
}
