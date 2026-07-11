//! Lychgate — the Gothic-Horror social gateway (#758). The roofed churchyard
//! gate where coffins once rested before burial: two dressed-ashlar piers
//! carry a steep dead-timber gable, a pointed-arch frames the opening, and a
//! rose-glass lantern hangs lit over the threshold. A wrought-iron cross
//! crowns the front gable and cold graveyard mist creeps at its foot.
//!
//! The only functional element is the [`GeneratorKind::Gateway`] zone child
//! centred in the walk-through opening — stepping into it opens the
//! destination picker. Everything else is set-dressing that reads the frame
//! as a gate you pass beneath. This bespoke entry wins the
//! `entries_for(GothicHorror, Gateway)` query over the neutral placeholder.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the flagstone base.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, foundation_block, id_quat,
    prim, quat_z, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    DEADWOOD, IRON_BLACK, STAINED_GLOW, STONE_DARK, STONE_MOSS, fx, iron, mossy, pointed_arch,
    stained, stone, wood,
};

pub struct GothicHorrorGateway;

impl CatalogueEntry for GothicHorrorGateway {
    fn slug(&self) -> &'static str {
        "gothic_horror_gateway"
    }
    fn name(&self) -> &'static str {
        "Lychgate"
    }
    fn description(&self) -> &'static str {
        "Roofed churchyard lychgate, its rose lantern lit over the coffin-rest threshold."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.5,
            min_spawn_dist: 8.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let st = || stone(STONE_DARK);
    let ir = || iron(IRON_BLACK);
    let pier_x = 1.6_f32; // pier centres flanking a ~2.6 m opening
    let zf = -1.3_f32; // -Z gable front (hero convention)

    // Churchyard-cobble threshold flagstone — the flat-base root. Never tilt
    // a root: `assemble` applies its transform to every child.
    let mut prims = vec![prim(
        solid(cuboid_tapered([4.2, 0.3, 2.4], 0.0, mossy(STONE_MOSS))),
        [0.0, 0.15, 0.0],
        id_quat(),
    )];
    prims.push(foundation_block(4.2, 2.4, [0.0, 0.0], 1.0));

    // Two dressed-ashlar piers: stepped plinth, lightly battered shaft, cap.
    for s in [-1.0_f32, 1.0] {
        let x = s * pier_x;
        prims.push(prim(
            solid(cuboid_tapered([0.78, 0.3, 0.9], 0.0, st())),
            [x, 0.15, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.55, 2.7, 0.7], 0.04, st())),
            [x, 1.65, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.74, 0.26, 0.86], 0.0, st())),
            [x, 3.13, 0.0],
            id_quat(),
        ));
    }

    // Stone lintel bridging the piers, carrying the roof.
    prims.push(prim(
        solid(cuboid_tapered([3.9, 0.42, 0.8], 0.0, st())),
        [0.0, 3.47, 0.0],
        id_quat(),
    ));

    // Two-centred pointed arch framing the opening on the front face — the
    // theme's signature Gothic silhouette springing from the pier inners.
    prims.extend(pointed_arch([0.0, 1.0, zf + 0.9], 1.3, 0.12, st()));

    // Steep dead-timber gable roof: ridge along Z, gable triangles face ±Z.
    prims.push(prim(
        solid(cuboid_tapered_xz(
            [4.0, 1.6, 2.6],
            [0.9, 0.0],
            wood(DEADWOOD),
        )),
        [0.0, 4.45, 0.0],
        id_quat(),
    ));
    // Ridge purlin along the apex.
    prims.push(prim(
        solid(cuboid_tapered([0.18, 0.16, 2.7], 0.0, wood(DEADWOOD))),
        [0.0, 5.2, 0.0],
        id_quat(),
    ));
    // Barge boards trimming both gable ends (front and back triangles).
    let barge_ang = 0.896_f32; // from eave (±2.0, 3.65) up to apex (0, 5.25)
    for gz in [zf - 0.04, -zf + 0.04] {
        prims.push(prim(
            solid(cuboid_tapered([0.14, 2.56, 0.12], 0.0, wood(DEADWOOD))),
            [1.0, 4.45, gz],
            quat_z(barge_ang),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.14, 2.56, 0.12], 0.0, wood(DEADWOOD))),
            [-1.0, 4.45, gz],
            quat_z(-barge_ang),
        ));
    }

    // Wrought-iron cross crowning the front gable apex — the churchyard emblem
    // facing the -Z approach.
    prims.push(prim(
        solid(cuboid_tapered([0.12, 0.8, 0.1], 0.0, ir())),
        [0.0, 5.6, zf - 0.05],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.5, 0.13, 0.1], 0.0, ir())),
        [0.0, 5.72, zf - 0.05],
        id_quat(),
    ));

    // Hanging rose-glass lantern lighting the threshold, hung under the front
    // overhang. Deep-saturated stained glow at low strength so the pane reads
    // as lit colour, not white bloom; the iron cage runs cold.
    let lz = -0.8_f32;
    prims.push(prim(
        solid(cylinder_tapered(0.018, 0.85, 6, 0.0, ir())),
        [0.0, 3.95, lz],
        id_quat(),
    ));
    prims.push(prim(cone(0.15, 0.18, 4, ir()), [0.0, 3.42, lz], id_quat()));
    prims.push(prim(
        cuboid_tapered([0.2, 0.28, 0.2], 0.0, stained(STAINED_GLOW, 3.0)),
        [0.0, 3.19, lz],
        id_quat(),
    ));
    // Iron cage posts at the lantern corners.
    for cx in [-0.09_f32, 0.09] {
        for cz in [-0.09_f32, 0.09] {
            prims.push(prim(
                cuboid_tapered([0.03, 0.28, 0.03], 0.0, ir()),
                [cx, 3.19, lz + cz],
                id_quat(),
            ));
        }
    }
    prims.push(prim(
        cuboid_tapered([0.24, 0.05, 0.24], 0.0, ir()),
        [0.0, 3.02, lz],
        id_quat(),
    ));
    prims.push(prim(sphere(0.05, 6, ir()), [0.0, 2.98, lz], id_quat()));

    // The walk-in zone: bottom at the flagstone top, headroom up to the lintel.
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, 1.9, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: cold graveyard mist creeping at the gate's foot.
    root.children
        .push(fx::ground_mist([0.0, 0.3, zf - 1.0], 0x60F0_1CE7));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&GothicHorrorGateway.build(""), "gothic_horror_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is furniture, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = GothicHorrorGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
