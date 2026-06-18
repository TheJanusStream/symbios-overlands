//! Traffic light — a Modern-City prop, and the kit's lit hero. A signal post
//! with a mast arm carrying a three-lens head (red, amber, a glowing green)
//! over the intersection, humming with the low rush of traffic. Its emissive
//! lens is the trim escalation's ruin pass darkens to a dead signal.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, SIGNAL_GREEN, STEEL_GREY, concrete, enamel, fx, steel};

pub struct TrafficLight;

impl CatalogueEntry for TrafficLight {
    fn slug(&self) -> &'static str {
        "traffic_light"
    }
    fn name(&self) -> &'static str {
        "Traffic Light"
    }
    fn description(&self) -> &'static str {
        "Signal post with a three-lens head over the road, humming with traffic."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CITY_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let pole_h = 4.2;
    let head_x = 2.4;

    let mut prims = vec![
        // Concrete footing — the root.
        prim(
            solid(cuboid_tapered(
                [0.6, 0.3, 0.6],
                0.1,
                concrete(CONCRETE_GREY),
            )),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
        // Steel pole.
        prim(
            solid(cylinder_tapered(0.14, pole_h, 8, 0.15, steel(STEEL_GREY))),
            [0.0, pole_h * 0.5, 0.0],
            id_quat(),
        ),
        // Horizontal mast arm over the road (a box, so no sideways rotation).
        prim(
            solid(cuboid_tapered(
                [head_x + 0.4, 0.16, 0.16],
                0.0,
                steel(STEEL_GREY),
            )),
            [head_x * 0.5, pole_h - 0.2, 0.0],
            id_quat(),
        ),
    ];

    // Signal head: a dark enamel box hung under the arm with three lenses.
    let head_y = pole_h - 0.9;
    let mut head = prim(
        solid(cuboid_tapered(
            [0.5, 1.4, 0.45],
            0.0,
            enamel([0.1, 0.11, 0.12]),
        )),
        [head_x, head_y, 0.0],
        id_quat(),
    );
    head.audio = fx::traffic_hum();
    prims.push(head);
    // Red and amber lenses (dark), green lit.
    prims.push(prim(
        sphere(0.13, 3, enamel([0.35, 0.06, 0.05])),
        [head_x, head_y + 0.45, 0.24],
        id_quat(),
    ));
    prims.push(prim(
        sphere(0.13, 3, enamel([0.4, 0.3, 0.05])),
        [head_x, head_y, 0.24],
        id_quat(),
    ));
    prims.push(prim(
        sphere(0.14, 3, glow(SIGNAL_GREEN, 4.0)),
        [head_x, head_y - 0.45, 0.24],
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
        assert_sanitize_stable(&TrafficLight.build(""), "traffic_light");
    }

    #[test]
    fn has_signal() {
        assert!(super::super::has_emissive(&TrafficLight.build("")));
    }
}
