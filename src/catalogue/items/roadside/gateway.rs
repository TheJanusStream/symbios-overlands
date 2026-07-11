//! Welcome Arch — the Roadside theme's bespoke social gateway (#765). The
//! neutral placeholder gateway is re-skinned here as a small-town highway
//! welcome gantry: twin steel
//! posts on board-formed concrete footings, a box-truss span, a glowing
//! sodium-amber arch springing over the top, and a backlit WELCOME marquee
//! hung across the opening. The blacktop runs straight through the middle.
//!
//! As with every gateway the only functional element is the single
//! [`GeneratorKind::Gateway`] zone child centred in the opening — walking
//! into it opens the destination picker listing the room owner's mutual
//! follows. Everything else frames that zone so it reads as a gate you
//! drive through rather than a billboard you pass.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the forecourt slab.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, glow, id_quat, prim, quat_x, solid, sphere, torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    ASPHALT_DARK, CONCRETE_GREY, NEON_RED, SIGN_AMBER, STEEL_GREY, asphalt, concrete, sign_board,
    steel,
};

pub struct RoadsideGateway;

impl CatalogueEntry for RoadsideGateway {
    fn slug(&self) -> &'static str {
        "roadside_gateway"
    }
    fn name(&self) -> &'static str {
        "Welcome Arch"
    }
    fn description(&self) -> &'static str {
        "Twin steel posts under a glowing sodium-amber arch and a lit WELCOME marquee — the drive-through gantry that greets the strip."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Roadside]
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
    // Forecourt blacktop slab — the flat-base root. Never tilt a root: every
    // child inherits its transform, so a rotated slab would spin the whole
    // gantry. The road runs straight through the middle of this pad.
    let mut prims = vec![prim(
        solid(cuboid_tapered([6.0, 0.3, 3.0], 0.0, asphalt(ASPHALT_DARK))),
        [0.0, 0.15, 0.0],
        id_quat(),
    )];

    // Twin posts flanking a ~2.9 m drive-through: board-formed concrete
    // footing + a lightly tapered structural-steel column on top.
    for x in [-1.7_f32, 1.7] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.95, 0.7, 0.95],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [x, 0.65, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.5, 4.4, 0.5], 0.06, steel(STEEL_GREY))),
            [x, 3.2, 0.0],
            id_quat(),
        ));
    }

    // Box-truss span bridging the two columns.
    prims.push(prim(
        solid(cuboid_tapered([4.5, 0.6, 0.75], 0.0, steel(STEEL_GREY))),
        [0.0, 5.3, 0.0],
        id_quat(),
    ));

    // The crown: a half-torus arch springing from the span. `path_cut
    // [0.0, 0.5]` keeps the θ∈[0, π] semicircle (feet at ±major_r); the
    // −90° X tip stands that flat semicircle upright into an arch rising in
    // +Y. A structural steel ring set back, and a sodium-amber neon tube
    // hugging its −Z front — a thin tube, so the glow can run hot without
    // washing to white.
    prims.push(prim(
        with_cut(
            torus(0.14, 1.7, steel(STEEL_GREY)),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        ),
        [0.0, 5.5, 0.12],
        quat_x(-FRAC_PI_2),
    ));
    prims.push(prim(
        with_cut(
            torus(0.11, 1.7, glow(SIGN_AMBER, 5.0)),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        ),
        [0.0, 5.5, -0.14],
        quat_x(-FRAC_PI_2),
    ));
    // Beacon orb capping the arch apex.
    prims.push(prim(
        sphere(0.22, 2, glow(SIGN_AMBER, 4.0)),
        [0.0, 7.25, -0.14],
        id_quat(),
    ));

    // Backlit WELCOME marquee hung under the span on the −Z hero front. The
    // deep-saturated neon-red cells + the helper's dark cell gaps keep a broad
    // lit board reading as lit signage rather than a blown-out slab.
    prims.extend(sign_board(
        [0.0, 4.7, -0.5],
        [3.4, 0.7],
        (7, 1),
        NEON_RED,
        2.5,
        -1.0,
    ));

    // Threshold accents framing the opening: a neon jamb strip up the front
    // inner edge of each post, and a low light bar across the walk-through
    // line — deep-saturated amber, thin trim hot, the ground bar low.
    for x in [-1.4_f32, 1.4] {
        prims.push(prim(
            cuboid_tapered([0.1, 3.4, 0.06], 0.0, glow(SIGN_AMBER, 5.0)),
            [x, 2.7, -0.27],
            id_quat(),
        ));
    }
    prims.push(prim(
        cuboid_tapered([2.6, 0.08, 0.14], 0.0, glow(SIGN_AMBER, 3.0)),
        [0.0, 0.36, -0.4],
        id_quat(),
    ));

    // The walk-through zone between the posts: floor (slab top) to just under
    // the marquee, centred in the 2.9 m opening.
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
        assert_sanitize_stable(&RoadsideGateway.build(""), "roadside_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is set-dressing, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = RoadsideGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
