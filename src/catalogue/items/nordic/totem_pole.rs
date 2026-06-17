//! Totem pole — a Nordic prop. A carved idol post: a stack of blocky carved
//! figures in alternating wood tones, painted with a band or two and topped
//! by a horned head with cold-glinting eyes. A god-pole raised at the edge
//! of the steading.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, glow, id_quat, prim, quat_x, quat_y, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{SHIELD_RED, WOOD_DARK, WOOD_WARM, cloth, timber};

/// Cold glint worked into the carved eyes.
const EYE_GLOW: [f32; 3] = [0.45, 0.66, 0.95];

pub struct TotemPole;

impl CatalogueEntry for TotemPole {
    fn slug(&self) -> &'static str {
        "totem_pole"
    }
    fn name(&self) -> &'static str {
        "Totem Pole"
    }
    fn description(&self) -> &'static str {
        "Carved idol post of stacked figures topped with a horned head."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Nordic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::NORDIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Buried-post base (root).
    let mut prims = vec![prim(
        solid(cuboid_tapered([0.7, 0.5, 0.7], 0.0, timber(WOOD_DARK))),
        [0.0, 0.25, 0.0],
        id_quat(),
    )];

    // Stacked carved figures, alternating tone and twist.
    let segs = [
        (0.9_f32, 0.9_f32, WOOD_WARM, 0.0_f32),
        (0.78, 0.85, WOOD_DARK, 0.5),
        (0.84, 0.9, WOOD_WARM, -0.4),
        (0.72, 0.8, WOOD_DARK, 0.3),
    ];
    let mut y = 0.5;
    for (w, h, tone, yaw) in segs {
        prims.push(prim(
            solid(cuboid_tapered([w, h, w], 0.0, timber(tone))),
            [0.0, y + h * 0.5, 0.0],
            quat_y(yaw),
        ));
        y += h;
    }

    // A painted band around one figure.
    prims.push(prim(
        cuboid_tapered([0.86, 0.18, 0.86], 0.0, cloth(SHIELD_RED, WOOD_DARK)),
        [0.0, 1.7, 0.0],
        id_quat(),
    ));

    // Horned head on top.
    prims.push(prim(
        solid(cuboid_tapered([0.8, 0.7, 0.7], 0.15, timber(WOOD_WARM))),
        [0.0, y + 0.35, 0.0],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cone(0.1, 0.5, 6, timber(WOOD_DARK)),
            [sx * 0.35, y + 0.7, 0.0],
            quat_x(sx * 0.5),
        ));
    }
    // Cold-glinting carved eyes.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.12, 0.1, 0.06], 0.0, glow(EYE_GLOW, 1.6)),
            [sx * 0.18, y + 0.4, 0.37],
            id_quat(),
        ));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&TotemPole.build(""), "totem_pole");
    }
}
