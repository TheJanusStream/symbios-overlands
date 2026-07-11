//! Scrap Gate — the Post-apocalyptic social gateway (#764). Two welded
//! oil-drum-and-steel pylons shored up with leaning braces carry a salvaged
//! girder across a walk-through gap, a corrugated valance and a hazard board
//! hung beneath it. A hazard-orange threshold glow, a warm caged worklight and
//! a red signal beacon light the way out.
//!
//! The functional element is the single [`GeneratorKind::Gateway`] zone child
//! centred in the opening — walking into it opens the destination picker
//! listing the room owner's mutual follows. Everything else is scavenged
//! set-dressing framing that zone as a gate you pass through.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the concrete slab. The gate front is `-Z` (hero convention): the
//! hazard board and beacon face the approaching player.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, foundation_block, glow, id_quat, prim, quat_x,
    quat_z, solid, sphere, torus, tube,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, CORRUGATED_RUST, FIRE_ORANGE, RUST_BROWN, SIGNAL_RED, STEEL_GREY, TIRE_BLACK,
    WORKLIGHT, concrete, fx, rubble_chunks, rusted, sheet, tarp,
};

pub struct PostApocGateway;

impl CatalogueEntry for PostApocGateway {
    fn slug(&self) -> &'static str {
        "post_apoc_gateway"
    }
    fn name(&self) -> &'static str {
        "Scrap Gate"
    }
    fn description(&self) -> &'static str {
        "Welded oil-drum pylons and a salvaged girder framing a hazard-lit way out of the wasteland."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::PostApoc]
    }
    // No prosperity_band: the spawn gate serves poor drifter camps and rich
    // holdouts alike, so it stays band-agnostic and wins the
    // `entries_for(PostApoc, Gateway)` query at any prosperity.
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
    // Cracked concrete forecourt slab — the flat-base root (never tilt a root:
    // every child would spin with it). Top sits at y = 0.3.
    let slab_top = 0.3_f32;
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [5.0, 0.3, 2.8],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, 0.15, 0.0],
        id_quat(),
    )];
    // Buried plinth so a slope-snapped gate shows stone, not daylight.
    prims.push(foundation_block(5.0, 2.8, [0.0, 0.0], 1.2));

    // Two salvage pylons flanking a ~2.85 m gap. Each is a welded steel post on
    // an oil-drum footing, clad on the −Z face with corrugated sheet and shored
    // by a leaning outrigger brace.
    for x in [-1.7_f32, 1.7] {
        let side = x.signum();
        // Oil-drum footing — the heavy scavenged base.
        prims.push(prim(
            solid(cylinder_tapered(0.42, 1.1, 12, 0.0, rusted(RUST_BROWN))),
            [x, slab_top + 0.55, 0.0],
            id_quat(),
        ));
        // Tapering welded steel post carrying the girder.
        prims.push(prim(
            solid(cuboid_tapered([0.55, 4.0, 0.55], 0.05, rusted(STEEL_GREY))),
            [x, slab_top + 2.0, 0.0],
            id_quat(),
        ));
        // Corrugated cladding plate riveted to the −Z (front) face.
        prims.push(prim(
            solid(cuboid_tapered(
                [0.6, 2.4, 0.08],
                0.0,
                sheet(CORRUGATED_RUST),
            )),
            [x, slab_top + 2.2, -0.3],
            id_quat(),
        ));
        // Leaning shore brace: foot planted outboard on the slab, head jammed
        // against the post — the propped-up look of a scavenged gate. quat_z
        // tips the head inboard toward the post (mirror the sign across X).
        prims.push(prim(
            solid(cylinder_tapered(0.09, 2.4, 6, 0.0, rusted(STEEL_GREY))),
            [x * 1.24, slab_top + 1.15, 0.1],
            quat_z(0.255 * side),
        ));
    }

    // Wasteland grit at the pylon feet: a slumped bald tyre one side, a heap of
    // collapse rubble the other — asymmetric, as scavenged sites always are.
    prims.push(prim(
        solid(torus(0.16, 0.4, tarp(TIRE_BLACK))),
        [-2.3, slab_top + 0.12, 0.7],
        quat_z(0.25),
    ));
    prims.extend(rubble_chunks([2.2, slab_top, 0.6], 0.8, 0.5, 3));

    // Span: a heavy salvaged I-beam girder resting across the posts.
    prims.push(prim(
        solid(cuboid_tapered([4.0, 0.5, 0.65], 0.0, rusted(STEEL_GREY))),
        [0.0, slab_top + 4.25, 0.0],
        id_quat(),
    ));
    // Corrugated valance skirting the front edge of the girder.
    prims.push(prim(
        solid(cuboid_tapered(
            [3.8, 0.6, 0.08],
            0.0,
            sheet(CORRUGATED_RUST),
        )),
        [0.0, slab_top + 3.75, -0.32],
        id_quat(),
    ));
    // Hazard board hung under the girder, face turned to the −Z approach and
    // tilted down toward the walker — the gate's signage / emblem.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.7, 0.75, 0.06],
            0.0,
            tarp([0.62, 0.5, 0.1]),
        )),
        [0.0, slab_top + 3.25, -0.42],
        quat_x(0.12),
    ));

    // Threshold lighting. Emissive discipline: the broad under-girder strip
    // runs a deep-saturated hazard orange at LOW strength so it reads as lit
    // colour, not white bloom; the small caged bulb and the beacon orb are
    // thin/tiny, so they can run hot.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.8, 0.1, 0.14],
            0.0,
            glow(FIRE_ORANGE, 2.0),
        )),
        [0.0, slab_top + 3.68, -0.05],
        id_quat(),
    ));
    // Warm caged worklight hung over the opening — a rusted ring cage around a
    // salvaged bulb.
    prims.push(prim(
        solid(tube(0.16, 0.13, 0.34, 6, rusted(STEEL_GREY))),
        [0.0, slab_top + 3.1, -0.45],
        id_quat(),
    ));
    prims.push(prim(
        sphere(0.12, 3, glow(WORKLIGHT, 3.0)),
        [0.0, slab_top + 3.1, -0.45],
        id_quat(),
    ));
    // Red signal beacon bolted to the crown of the girder — the true-red
    // warning light the ruin pass can snuff.
    prims.push(prim(
        sphere(0.16, 4, glow(SIGNAL_RED, 5.0)),
        [0.0, slab_top + 4.7, 0.0],
        id_quat(),
    ));

    // The walk-in zone centred in the opening: bottom at the slab top, headroom
    // up to the valance / hazard board.
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, slab_top + 1.6, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: the desolate wasteland wind and a low drift of ash
    // through the gate mouth.
    root.audio = fx::desolate_wind();
    root.children
        .push(fx::ash_drift([0.0, 0.7, -1.0], 0x0A57_6A7E));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&PostApocGateway.build(""), "post_apoc_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is furniture, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = PostApocGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
