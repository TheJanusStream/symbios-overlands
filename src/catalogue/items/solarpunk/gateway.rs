//! Living Gateway — the Solarpunk bespoke social gateway (#767), the themed
//! replacement for the neutral placeholder arch. Two vine-wrapped timber
//! columns rise from planted urns, spanned by a heavy beam crowned with a
//! living turf roof and a pair of sun-catching PV panels; a warm radiant sun
//! emblem faces the hero front and a fresh grow-light sill lights the walk
//! through. Birdsong and drifting pollen make the whole gate breathe.
//!
//! The only functional element is the single [`GeneratorKind::Gateway`] zone
//! centred in the opening — walking into it opens the destination picker of
//! the room owner's mutual follows. Everything else is eco-quarter
//! set-dressing framing that opening so it reads as a threshold you grow
//! through.

use std::f32::consts::{FRAC_PI_2, TAU};

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, foundation_block, glow, helix, id_quat, prim, prim_scaled, quat_x,
    quat_z, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_PALE, CROP_GREEN, DOME_GLOW, LAMP_WARM, LEAF_GREEN, MOSS_GREEN, PV_BLUE, TIMBER_WARM,
    concrete, crop_tufts, foliage, fx, pv, timber,
};

pub struct SolarpunkGateway;

impl CatalogueEntry for SolarpunkGateway {
    fn slug(&self) -> &'static str {
        "solarpunk_gateway"
    }
    fn name(&self) -> &'static str {
        "Living Gateway"
    }
    fn description(&self) -> &'static str {
        "Vine-wrapped living columns under a turf-roofed span, a warm solar sun and grow-lit threshold marking the way onward."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
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
    // Layout: two timber columns at x = ±POST_X leave a ~2.6 m opening between
    // their inner faces; a beam bridges their heads and carries the turf roof.
    let post_x = 1.7_f32;
    let post_w = 0.55_f32;
    let post_h = 3.6_f32;
    let slab_top = 0.3_f32;
    let post_top = slab_top + post_h; // 3.9
    let lintel_h = 0.5_f32;
    let lintel_d = 0.7_f32;
    let lintel_y = post_top + lintel_h * 0.5; // 4.15
    let lintel_top = post_top + lintel_h; // 4.4
    let panel_z = -(lintel_d * 0.5 + 0.03); // hero-front face of the beam

    // Eco-concrete forecourt slab — the FLAT-BASE ROOT (never tilt a root:
    // every child would spin with it).
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [5.2, slab_top, 2.6],
            0.0,
            concrete(CONCRETE_PALE),
        )),
        [0.0, slab_top * 0.5, 0.0],
        id_quat(),
    )];
    // Buried plinth so a slope-snapped gate shows stone, not daylight.
    prims.push(foundation_block(5.2, 2.6, [0.0, 0.0], 1.0));

    // Two living columns flanking the opening: a lightly tapered timber post, a
    // climbing foliage vine spiralling up it, and a planted urn at its foot.
    for sx in [-1.0_f32, 1.0] {
        let x = sx * post_x;
        // Timber post.
        prims.push(prim(
            solid(cuboid_tapered(
                [post_w, post_h, post_w],
                0.05,
                timber(TIMBER_WARM),
            )),
            [x, slab_top + post_h * 0.5, 0.0],
            id_quat(),
        ));
        // Climbing vine — a foliage helix wrapping the post from foot to head
        // (decorative, so no collider). 3.4 turns over ~3 m of rise.
        prims.push(prim(
            helix(0.42, 0.07, 0.9, 3.4, 16, foliage(LEAF_GREEN)),
            [x, slab_top + post_h * 0.5, 0.0],
            id_quat(),
        ));
        // Planted urn at the column foot, spilling leafy greens — the gate
        // flanked by growing beds.
        prims.push(prim(
            solid(cuboid_tapered([0.8, 0.5, 0.8], 0.0, timber(TIMBER_WARM))),
            [x, slab_top + 0.25, 0.0],
            id_quat(),
        ));
        prims.extend(crop_tufts(
            [x, slab_top + 0.5, 0.0],
            [0.6, 0.6],
            2,
            2,
            0.36,
            foliage(CROP_GREEN),
        ));
    }

    // Heavy timber lintel bridging the column heads — the span.
    prims.push(prim(
        solid(cuboid_tapered(
            [4.6, lintel_h, lintel_d],
            0.0,
            timber(TIMBER_WARM),
        )),
        [0.0, lintel_y, 0.0],
        id_quat(),
    ));

    // Living turf roof crowning the span — a matte green soil strip planted
    // with a row of leafy crops, the eco-quarter's green-roof signature.
    prims.push(prim(
        solid(cuboid_tapered([4.2, 0.22, 0.62], 0.0, foliage(MOSS_GREEN))),
        [0.0, lintel_top + 0.11, 0.0],
        id_quat(),
    ));
    prims.extend(crop_tufts(
        [0.0, lintel_top + 0.22, 0.0],
        [3.0, 0.4],
        8,
        1,
        0.36,
        foliage(CROP_GREEN),
    ));
    // A pair of sun-catching PV panels riding the crown ends, tilted to the
    // sky — the clean-energy half of the theme's identity.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.7, 0.05, 0.55], 0.0, pv(PV_BLUE))),
            [sx * 1.95, lintel_top + 0.24, 0.0],
            quat_x(sx * 0.3),
        ));
    }

    // Hero emblem on the −Z front of the lintel: a warm radiant sun — a glowing
    // disc inside a ring, ringed by eight short rays. The ring and rays are
    // thin trim so they run hot without blooming white; the broad disc face
    // stays low so it reads as warm-lit, not a white blank.
    prims.push(prim(
        torus(0.06, 0.34, glow(LAMP_WARM, 3.2)),
        [0.0, lintel_y, panel_z - 0.02],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim_scaled(
        sphere(0.2, 6, glow(LAMP_WARM, 2.2)),
        [0.0, lintel_y, panel_z],
        id_quat(),
        [1.0, 1.0, 0.35],
    ));
    for i in 0..8 {
        let theta = i as f32 / 8.0 * TAU;
        prims.push(prim(
            cuboid_tapered([0.05, 0.24, 0.04], 0.0, glow(LAMP_WARM, 4.5)),
            [theta.cos() * 0.46, lintel_y + theta.sin() * 0.46, panel_z],
            quat_z(theta - FRAC_PI_2),
        ));
    }

    // Threshold accents echoing the growing life: a fresh grow-light sill under
    // the beam and a floor sill line across the opening, both deep-saturated
    // green at low strength so they read as lit colour, not white bloom.
    prims.push(prim(
        cuboid_tapered([2.6, 0.1, 0.14], 0.0, glow(DOME_GLOW, 3.0)),
        [0.0, post_top - 0.15, -0.22],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([2.6, 0.06, 0.2], 0.0, glow(DOME_GLOW, 2.2)),
        [0.0, slab_top + 0.03, -0.22],
        id_quat(),
    ));

    // The walk-in zone between the columns: floor at the slab top, headroom up
    // under the lintel. The only functional element.
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, 1.9, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: birdsong over the gate and a soft drift of pollen through
    // the opening, so the Living Gateway literally breathes.
    root.audio = fx::birdsong();
    root.children
        .push(fx::pollen_drift([0.0, 2.0, 0.0], 0x5011_6A7E));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&SolarpunkGateway.build(""), "solarpunk_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is set-dressing, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = SolarpunkGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
