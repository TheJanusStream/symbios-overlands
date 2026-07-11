//! Stadium Gate — the Sports/Recreation bespoke social gateway (#769). The
//! turnstile entrance to a floodlit ground: two board-formed concrete
//! entrance piers flanking the walk-in, a steel crossbeam marquee bridging
//! them, a lit segmented scoreboard slung across the front under the
//! swelling murmur of the stand, and a pair of small floodlight heads
//! crowning the span. Cool LED threshold strips frame the mouth so the gate
//! reads as an active portal without blooming white.
//!
//! The one functional element is the [`GeneratorKind::Gateway`] zone between
//! the piers — walking into it opens the destination picker. Everything else
//! is set-dressing that frames that opening as a gate you pass through.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, STEEL_GREY, concrete, fx, steel};

pub struct SportsRecGateway;

impl CatalogueEntry for SportsRecGateway {
    fn slug(&self) -> &'static str {
        "sports_rec_gateway"
    }
    fn name(&self) -> &'static str {
        "Stadium Gate"
    }
    fn description(&self) -> &'static str {
        "Turnstile entrance under a lit scoreboard marquee and the murmur of the stand."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SportsRec]
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

// Deep electric-blue LED for the threshold strips — a cool active-portal
// tone that echoes the zone veil without blooming to white.
const THRESHOLD_LED: [f32; 3] = [0.28, 0.55, 1.0];

fn build_tree() -> Generator {
    // Forecourt apron — the flat-base root (never tilt a root: assemble
    // rebases every child under it, so a rotated root spins the whole gate).
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [5.2, 0.3, 3.0],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, 0.15, 0.0],
        id_quat(),
    )];

    // Two board-formed concrete entrance piers flanking a 2.6 m walk-in,
    // lightly tapered so they read as stadium buttresses rather than posts.
    // Inner faces sit at x = ±1.3, opening the mouth to 2.6 m.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.8, 4.2, 0.9],
                0.06,
                concrete(CONCRETE_GREY),
            )),
            [sx * 1.7, 2.1, 0.0],
            id_quat(),
        ));
        // A vertical LED strip up the inner face of each pier, framing the
        // opening. Thin trim, so it runs a touch hotter than the broad faces.
        prims.push(prim(
            cuboid_tapered([0.06, 3.0, 0.12], 0.0, glow(THRESHOLD_LED, 4.5)),
            [sx * 1.31, 1.85, -0.32],
            id_quat(),
        ));
    }

    // Steel crossbeam marquee bridging the piers. Its bottom (y=4.10) tucks
    // just inside the pier tops (y=4.20) so no horizontal faces sit coplanar.
    prims.push(prim(
        solid(cuboid_tapered([4.4, 0.5, 1.0], 0.0, steel(STEEL_GREY))),
        [0.0, 4.35, 0.0],
        id_quat(),
    ));

    // Lit segmented scoreboard slung across the crossbeam front (the −Z hero
    // face), proud of the beam so it doesn't z-fight the panel. The swelling
    // crowd murmur of a full stand rides on the board's first cell.
    let mut marquee = super::score_display(0.0, 4.37, -0.57, 3.3, 0.58);
    marquee[0].audio = fx::crowd_murmur();
    prims.extend(marquee);

    // Two small floodlight heads crowning the span, on short steel mounts,
    // facing the −Z front — gridded lamp cells that read as an array rather
    // than a single white slab. Emissive trim the ruin pass can snuff.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.12, 0.3, 0.12], 0.0, steel(STEEL_GREY))),
            [sx * 1.15, 4.72, -0.1],
            id_quat(),
        ));
        for g in super::lamp_bank([sx * 1.15, 4.95, -0.28], 0.75, 0.5, 3, 2, -1.0) {
            prims.push(g);
        }
    }

    // Cool threshold bar tucked just under the beam, spanning the mouth — a
    // broad lit strip at low strength so it reads as lit colour, not glare.
    prims.push(prim(
        cuboid_tapered([2.6, 0.1, 0.16], 0.0, glow(THRESHOLD_LED, 2.5)),
        [0.0, 4.02, -0.35],
        id_quat(),
    ));

    // The walk-in zone between the piers: bottom at the apron top, headroom
    // up to the marquee. This is the gate's one functional element.
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
        assert_sanitize_stable(&SportsRecGateway.build(""), "sports_rec_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is furniture, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = SportsRecGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
