//! Arcane Gateway — the High-Fantasy bespoke social gate (#756). Replaces the
//! neutral placeholder arch for a fantasy room: a pair of rune-carved ashlar
//! columns spanned by a keystone lintel, crowned by a glowing arcane orb, with
//! a mana-lit threshold ring underfoot and drifting motes in the opening.
//!
//! The only functional element is the [`GeneratorKind::Gateway`] zone child —
//! walking into it opens the destination picker listing the room owner's
//! mutual follows. Everything else frames that zone so it reads as a portal
//! you step through: two columns flanking a ~2.6 m gap, a lintel across the
//! top, and emissive trim (orb, veil strip, floor ring, runes) held at low
//! strength so it glows arcane rather than blooming white.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the threshold slab.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, foundation_block, glow, id_quat, prim, solid,
    sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    ARCANE_PURPLE, CRYSTAL_CYAN, GOLD, MANA_TEAL, RUNE_GOLD, STONE_GREY, crystal, fx, gold,
    rune_marks, stone,
};

pub struct FantasyGateway;

impl CatalogueEntry for FantasyGateway {
    fn slug(&self) -> &'static str {
        "fantasy_gateway"
    }
    fn name(&self) -> &'static str {
        "Arcane Gateway"
    }
    fn description(&self) -> &'static str {
        "Rune-carved stone columns spanned by a keystone lintel, its arcane orb lighting the threshold you step through."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Fantasy]
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
    let slab_h = 0.3_f32;
    let slab_top = slab_h; // slab centred at slab_h/2, so top == slab_h
    let base_h = 0.3_f32; // column base plinth
    let base_top = slab_top + base_h;
    let shaft_h = 3.2_f32;
    let shaft_top = base_top + shaft_h;
    let cap_h = 0.28_f32;
    let cap_top = shaft_top + cap_h; // lintel underside
    let lintel_h = 0.55_f32;
    let px = 1.85_f32; // column centre X — a ~2.9 m clear gap between shafts

    // Threshold slab — the flat-base root (never tilt a root: every child
    // inherits its transform and would spin with it).
    let mut prims = vec![prim(
        solid(cuboid_tapered([5.4, slab_h, 3.0], 0.0, stone(STONE_GREY))),
        [0.0, slab_h * 0.5, 0.0],
        id_quat(),
    )];
    // Buried plinth so a slope-snapped gate shows stone, not daylight.
    prims.push(foundation_block(5.4, 3.0, [0.0, 0.0], 1.2));

    // Two flanking columns — base plinth, tapering ashlar shaft ringed by gold
    // string-courses, capital, and a glowing crystal finial.
    for sx in [-1.0_f32, 1.0] {
        let cx = sx * px;
        // Square base plinth.
        prims.push(prim(
            solid(cuboid_tapered(
                [0.85, base_h, 0.85],
                0.08,
                stone(STONE_GREY),
            )),
            [cx, slab_top + base_h * 0.5, 0.0],
            id_quat(),
        ));
        // Round tapering shaft.
        prims.push(prim(
            solid(cylinder_tapered(0.4, shaft_h, 12, 0.1, stone(STONE_GREY))),
            [cx, base_top + shaft_h * 0.5, 0.0],
            id_quat(),
        ));
        // Gold string-course bands hugging the shaft.
        for (y, mr) in [(base_top + 0.6, 0.45_f32), (base_top + 2.4, 0.42_f32)] {
            prims.push(prim(torus(0.06, mr, gold(GOLD)), [cx, y, 0.0], id_quat()));
        }
        // Capital block.
        prims.push(prim(
            solid(cuboid_tapered([0.78, cap_h, 0.78], 0.0, stone(STONE_GREY))),
            [cx, shaft_top + cap_h * 0.5, 0.0],
            id_quat(),
        ));
        // Glowing faceted crystal finial rising from the capital.
        prims.push(crystal(
            [cx, cap_top, 0.0],
            0.11,
            0.75,
            id_quat(),
            glow(CRYSTAL_CYAN, 1.7),
        ));
    }

    // Lintel spanning the capitals.
    prims.push(prim(
        solid(cuboid_tapered(
            [4.7, lintel_h, 0.95],
            0.0,
            stone(STONE_GREY),
        )),
        [0.0, cap_top + lintel_h * 0.5, 0.0],
        id_quat(),
    ));
    // Keystone block seated in the lintel centre, proud on the −Z hero front.
    let keystone_h = 0.75_f32;
    let keystone_cy = cap_top + lintel_h - 0.05; // straddles the lintel top
    prims.push(prim(
        solid(cuboid_tapered(
            [0.82, keystone_h, 1.06],
            0.12,
            stone(STONE_GREY),
        )),
        [0.0, keystone_cy, 0.0],
        id_quat(),
    ));

    // Arcane orb crowning the keystone — the hero emblem. A deep-saturated
    // emissive sphere at LOW strength so it reads as a lit orb, not a white
    // ball, cradled in a small gold socket.
    let orb_y = keystone_cy + keystone_h * 0.5 + 0.24;
    prims.push(prim(
        solid(cylinder_tapered(0.2, 0.18, 8, 0.5, gold(GOLD))),
        [0.0, keystone_cy + keystone_h * 0.5 + 0.02, -0.08],
        id_quat(),
    ));
    prims.push(prim(
        sphere(0.3, 6, glow(ARCANE_PURPLE, 2.6)),
        [0.0, orb_y, -0.08],
        id_quat(),
    ));

    // Threshold veil strip under the lintel — a thin deep-purple bar washing
    // the opening from above (thin trim, so it can run a touch hot without
    // blooming white).
    prims.push(prim(
        cuboid_tapered([2.7, 0.12, 0.16], 0.0, glow(ARCANE_PURPLE, 3.0)),
        [0.0, cap_top - 0.14, 0.0],
        id_quat(),
    ));
    // Mana-lit runic ring inset in the slab — the step-through threshold read.
    prims.push(prim(
        torus(0.07, 1.25, glow(MANA_TEAL, 1.6)),
        [0.0, slab_top + 0.02, 0.0],
        id_quat(),
    ));

    // Glowing gold rune clusters carved into the lintel's −Z (front) face,
    // flanking the keystone.
    for sx in [-1.0_f32, 1.0] {
        prims.extend(rune_marks(
            [sx * 1.35, cap_top + lintel_h * 0.5, -0.53],
            0.42,
            glow(RUNE_GOLD, 2.0),
        ));
    }

    // The walk-through zone between the columns: bottom at the slab top,
    // headroom under the veil strip. This is the only functional element.
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, 1.95, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: an ethereal arcane hum, mana motes rising through the
    // opening, and sparkles crackling around the crowning orb.
    root.audio = fx::arcane_hum();
    root.children
        .push(fx::mana_motes([0.0, slab_top + 0.4, 0.0], 0x0756_0A17));
    root.children
        .push(fx::arcane_sparkle([0.0, orb_y, -0.08], 0x0756_0B23));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&FantasyGateway.build(""), "fantasy_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is furniture, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = FantasyGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
