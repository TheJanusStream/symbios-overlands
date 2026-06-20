//! Stadium — the Sports/Recreation landmark and the kit's lit hero. A mown
//! pitch ringed by four banks of stepped, colour-blocked seating, four
//! corner floodlight masts and a big lit scoreboard beyond one end. ~30 m
//! across, so it anchors the complex and reads as the ground from across the
//! home region. Its floodlights and scoreboard are the trim escalation's
//! ruin pass snuffs to a dark, empty bowl, and a crowd murmurs in the stands.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the pitch.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    FLOOD_LIT, LINE_WHITE, PITCH_GREEN, SCORE_AMBER, SEAT_BLUE, SEAT_RED, STEEL_GREY, enamel, fx,
    painted, steel, turf,
};

pub struct Stadium;

impl CatalogueEntry for Stadium {
    fn slug(&self) -> &'static str {
        "stadium"
    }
    fn name(&self) -> &'static str {
        "Stadium"
    }
    fn description(&self) -> &'static str {
        "Mown pitch ringed by stepped seating, floodlight masts and a lit scoreboard."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SportsRec]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SPORTS_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 22.0,
            min_spawn_dist: 60.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Mown pitch — the root.
        prim(
            solid(cuboid_tapered([20.0, 0.2, 14.0], 0.0, turf(PITCH_GREEN))),
            [0.0, 0.1, 0.0],
            id_quat(),
        ),
    ];
    // Halfway line and centre circle painted on the turf.
    prims.push(prim(
        cuboid_tapered([0.3, 0.06, 14.0], 0.0, painted(LINE_WHITE)),
        [0.0, 0.23, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        torus(0.06, 2.2, painted(LINE_WHITE)),
        [0.0, 0.23, 0.0],
        id_quat(),
    ));

    // North & South stands — three tiers stepping up and back along Z.
    for sz in [-1.0_f32, 1.0] {
        for t in 0..3 {
            let tf = t as f32;
            prims.push(prim(
                solid(cuboid_tapered([22.0, 0.6, 2.0], 0.0, enamel(SEAT_BLUE))),
                [0.0, 0.8 + tf * 1.0, sz * (8.5 + tf * 1.8)],
                id_quat(),
            ));
        }
    }
    // East & West stands — three tiers stepping up and back along X.
    for sx in [-1.0_f32, 1.0] {
        for t in 0..3 {
            let tf = t as f32;
            prims.push(prim(
                solid(cuboid_tapered([2.0, 0.6, 16.0], 0.0, enamel(SEAT_RED))),
                [sx * (11.5 + tf * 1.8), 0.8 + tf * 1.0, 0.0],
                id_quat(),
            ));
        }
    }

    // Four corner floodlight masts with lit heads — emissive.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cylinder_tapered(0.4, 12.0, 8, 0.1, steel(STEEL_GREY))),
            [sx * 14.0, 6.0, sz * 10.0],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([2.4, 0.8, 1.4], 0.0, glow(FLOOD_LIT, 4.0)),
            [sx * 14.0, 12.3, sz * 10.0],
            id_quat(),
        ));
    }

    // Scoreboard on two posts beyond the north end — emissive screen.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.4, 7.0, 0.4], 0.0, steel(STEEL_GREY))),
            [sx * 2.6, 3.5, 15.5],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered(
            [6.6, 3.4, 0.5],
            0.0,
            enamel([0.12, 0.12, 0.14]),
        )),
        [0.0, 7.2, 15.5],
        id_quat(),
    ));
    let mut screen = prim(
        cuboid_tapered([6.0, 2.8, 0.12], 0.0, glow(SCORE_AMBER, 4.0)),
        [0.0, 7.2, 15.78],
        id_quat(),
    );
    screen.audio = fx::tannoy_hum();
    prims.push(screen);

    let mut root = assemble(prims);
    // Signature life: the crowd murmur in the stands, dust over the pitch.
    root.audio = fx::crowd_murmur();
    root.children
        .push(fx::field_dust([0.0, 1.0, 0.0], 0x05F0_5A11));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Stadium.build(""), "stadium");
    }

    #[test]
    fn has_lit_floods_and_scoreboard() {
        assert!(crate::catalogue::items::util::has_emissive(
            &Stadium.build("")
        ));
    }
}
