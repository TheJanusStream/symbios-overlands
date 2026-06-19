//! Step pyramid — the Mesoamerican landmark. Four battered limestone
//! terraces climbing to a red-stuccoed temple cella crowned by a roof comb,
//! with a steep central staircase up the front face and a sacred fire
//! burning at the summit. A slow ritual drum sounds from its base. ~15 m
//! tall, so it anchors the city and reads as a temple-mountain across the
//! home region.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid, sphere};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    FIRE_ORANGE, GOLD_WARM, LIMESTONE_PALE, STUCCO_CREAM, STUCCO_RED, fx, gold, limestone, painted,
};

pub struct StepPyramid;

impl CatalogueEntry for StepPyramid {
    fn slug(&self) -> &'static str {
        "step_pyramid"
    }
    fn name(&self) -> &'static str {
        "Step Pyramid"
    }
    fn description(&self) -> &'static str {
        "Terraced limestone pyramid with a stair to a red temple and a sacred fire."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MESO_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 18.0,
            min_spawn_dist: 60.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Battered terraces: (half-width, height), shrinking and stacking up.
    let tiers = [(9.0_f32, 3.0_f32), (7.0, 3.0), (5.2, 2.8), (3.6, 2.6)];

    let mut prims = vec![
        // Buried base course — the root.
        prim(
            solid(cuboid_tapered(
                [19.0, 0.6, 19.0],
                0.0,
                limestone(LIMESTONE_PALE),
            )),
            [0.0, 0.3, 0.0],
            id_quat(),
        ),
    ];

    let mut y = 0.6;
    for (hw, h) in tiers {
        prims.push(prim(
            solid(cuboid_tapered(
                [hw * 2.0, h, hw * 2.0],
                0.06,
                limestone(LIMESTONE_PALE),
            )),
            [0.0, y + h * 0.5, 0.0],
            id_quat(),
        ));
        y += h;
    }
    let summit = y;

    // Central staircase up the front (+Z) face, receding as it climbs.
    let steps = 18;
    let z_bottom = tiers[0].0 + 0.2;
    let z_top = tiers[tiers.len() - 1].0 + 0.2;
    for i in 0..steps {
        let t = i as f32 / (steps - 1) as f32;
        let sy = t * summit;
        let sz = z_bottom + (z_top - z_bottom) * t;
        prims.push(prim(
            solid(cuboid_tapered(
                [5.0, summit / steps as f32 + 0.2, 0.8],
                0.0,
                limestone(STUCCO_CREAM),
            )),
            [0.0, sy + 0.1, sz],
            id_quat(),
        ));
    }
    // Flanking stair balustrades (low stepped curbs).
    for i in (0..steps).step_by(2) {
        let t = i as f32 / (steps - 1) as f32;
        let sy = t * summit;
        let sz = z_bottom + (z_top - z_bottom) * t;
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.5, 0.8, 0.9], 0.0, painted(STUCCO_RED))),
                [sx * 3.0, sy + 0.3, sz],
                id_quat(),
            ));
        }
    }

    // Temple cella on the summit: red stucco walls, a dark doorway, and a
    // tall roof comb.
    prims.push(prim(
        solid(cuboid_tapered([6.0, 3.2, 5.0], 0.0, painted(STUCCO_RED))),
        [0.0, summit + 1.6, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [1.6, 2.2, 0.6],
            0.0,
            painted([0.1, 0.06, 0.05]),
        )),
        [0.0, summit + 1.1, 2.4],
        id_quat(),
    ));
    // Roof comb (crestería) with a gold sun disc.
    prims.push(prim(
        solid(cuboid_tapered([5.0, 2.6, 0.5], 0.25, painted(STUCCO_RED))),
        [0.0, summit + 4.5, -1.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.2, 1.2, 0.2], 0.0, gold(GOLD_WARM))),
        [0.0, summit + 4.6, -0.7],
        id_quat(),
    ));

    // Sacred fire on a low altar before the temple doorway.
    let fire_z = 1.9;
    prims.push(prim(
        solid(cuboid_tapered(
            [1.4, 0.7, 1.4],
            0.1,
            limestone(STUCCO_CREAM),
        )),
        [0.0, summit + 0.35, fire_z],
        id_quat(),
    ));
    prims.push(prim(
        sphere(0.55, 3, glow(FIRE_ORANGE, 5.0)),
        [0.0, summit + 0.9, fire_z],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: the sacred fire's flame and embers, a ritual drum.
    root.children
        .push(fx::sacred_flame([0.0, summit + 1.1, fire_z], 0x5AC0_F1E0));
    root.children
        .push(fx::fire_embers([0.0, summit + 1.4, fire_z], 0xE3BE_F1E0));
    root.audio = fx::ritual_drum();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&StepPyramid.build(""), "step_pyramid");
    }

    #[test]
    fn has_sacred_fire() {
        assert!(crate::catalogue::items::util::has_emissive(
            &StepPyramid.build("")
        ));
    }
}
