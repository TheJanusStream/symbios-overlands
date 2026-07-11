//! Temple Gateway — the Mesoamerican bespoke social gateway (#761). A
//! walk-through temple doorway framed by two battered talud-tablero pylons,
//! spanned by a cream stone lintel carrying a red glyph frieze and a
//! roof-comb crest with a beaten-gold sun disc on the front (−Z) face. Two
//! threshold braziers throw warm firelight across the opening. The single
//! functional element is the [`GeneratorKind::Gateway`] zone the player walks
//! into to open the destination picker; everything else is set-dressing that
//! reads as a temple gate you pass through.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    FIRE_ORANGE, GOLD_WARM, JADE_GREEN, LIMESTONE_PALE, STONE_GREY, STUCCO_CREAM, STUCCO_RED,
    cobble, gold, jade, limestone, painted,
};

pub struct MesoamericanGateway;

impl CatalogueEntry for MesoamericanGateway {
    fn slug(&self) -> &'static str {
        "mesoamerican_gateway"
    }
    fn name(&self) -> &'static str {
        "Temple Gateway"
    }
    fn description(&self) -> &'static str {
        "Walk-through temple gate: talud-tablero pylons under a glyph lintel and a gold sun disc, lit by threshold braziers."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
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
    // Temple forecourt slab — the flat-base root. Never tilt a root: every
    // child inherits its transform, so a rotated slab would spin the gate.
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [6.0, 0.5, 3.4],
            0.0,
            limestone(LIMESTONE_PALE),
        )),
        [0.0, 0.25, 0.0],
        id_quat(),
    )];

    // Two battered talud-tablero pylons flanking a ~2.65 m walk-through gap.
    // Each is a sloped limestone talud base, an oversailing cream tablero
    // body with a recessed red glyph panel on the front (−Z) face, and a
    // stepped cornice cap — the signature Mesoamerican platform silhouette.
    for sx in [-1.0_f32, 1.0] {
        let px = sx * 2.0;
        // Battered talud base.
        prims.push(prim(
            solid(cuboid_tapered(
                [1.3, 1.5, 1.3],
                0.18,
                limestone(LIMESTONE_PALE),
            )),
            [px, 1.25, 0.0],
            id_quat(),
        ));
        // Oversailing tablero body.
        prims.push(prim(
            solid(cuboid_tapered(
                [1.35, 1.6, 1.15],
                0.0,
                limestone(STUCCO_CREAM),
            )),
            [px, 2.8, 0.0],
            id_quat(),
        ));
        // Recessed red stucco glyph panel on the front face.
        prims.push(prim(
            cuboid_tapered([0.9, 1.1, 0.1], 0.0, painted(STUCCO_RED)),
            [px, 2.8, -0.6],
            id_quat(),
        ));
        // Jade glyph boss centred on the panel.
        prims.push(prim(
            cuboid_tapered([0.34, 0.34, 0.12], 0.1, jade(JADE_GREEN)),
            [px, 2.8, -0.63],
            id_quat(),
        ));
        // Stepped cornice cap.
        prims.push(prim(
            solid(cuboid_tapered(
                [1.45, 0.35, 1.25],
                0.15,
                limestone(STUCCO_CREAM),
            )),
            [px, 3.775, 0.0],
            id_quat(),
        ));
    }

    // Cream stone lintel spanning the pylons.
    prims.push(prim(
        solid(cuboid_tapered(
            [5.4, 0.6, 1.25],
            0.0,
            limestone(STUCCO_CREAM),
        )),
        [0.0, 4.15, 0.0],
        id_quat(),
    ));
    // Red glyph frieze band across the lintel front.
    prims.push(prim(
        cuboid_tapered([4.6, 0.34, 0.14], 0.0, painted(STUCCO_RED)),
        [0.0, 4.15, -0.63],
        id_quat(),
    ));

    // Roof-comb crest above the lintel — a stepped openwork crown, cream over
    // red, tapering to a limestone finial, in the temple-mountain idiom.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.4, 0.5, 0.7],
            0.1,
            limestone(STUCCO_CREAM),
        )),
        [0.0, 4.7, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.6, 0.5, 0.6], 0.15, painted(STUCCO_RED))),
        [0.0, 5.2, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.9, 0.5, 0.5],
            0.4,
            limestone(STUCCO_CREAM),
        )),
        [0.0, 5.7, 0.0],
        id_quat(),
    ));
    // Beaten-gold sun disc emblem on the crest, laid flat to face the front
    // (−Z), with a jade boss at its hub.
    prims.push(prim(
        solid(cylinder_tapered(0.62, 0.18, 20, 0.0, gold(GOLD_WARM))),
        [0.0, 5.05, -0.5],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        sphere(0.18, 4, jade(JADE_GREEN)),
        [0.0, 5.05, -0.62],
        id_quat(),
    ));

    // Threshold accents. A warm firelight strip glowing under the lintel —
    // a thin band, deep-saturated at moderate strength so it reads as an
    // active threshold rather than a white lightbox.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.4, 0.14, 0.2],
            0.0,
            glow(FIRE_ORANGE, 3.0),
        )),
        [0.0, 3.78, -0.2],
        id_quat(),
    ));
    // Two brazier torches on stone corbels flanking the opening — small hot
    // emissive orbs lighting the way in.
    for sx in [-1.0_f32, 1.0] {
        let bx = sx * 1.25;
        prims.push(prim(
            solid(cuboid_tapered([0.3, 0.18, 0.4], 0.0, cobble(STONE_GREY))),
            [bx, 2.1, -0.3],
            id_quat(),
        ));
        prims.push(prim(
            sphere(0.17, 4, glow(FIRE_ORANGE, 6.0)),
            [bx, 2.35, -0.3],
            id_quat(),
        ));
    }

    // The single functional element: the walk-in zone centred in the opening,
    // bottom at the slab top and headroom under the lintel.
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, 1.9, 0.0],
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
        assert_sanitize_stable(&MesoamericanGateway.build(""), "mesoamerican_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is furniture, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = MesoamericanGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
