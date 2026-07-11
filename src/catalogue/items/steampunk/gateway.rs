//! Aether Gate — the Steampunk bespoke social gateway (#770). A riveted-iron
//! gatehouse built like a pair of boiler stacks: two banded iron pylons with
//! copper aether-risers, joined across the top by an iron lintel and a copper
//! pressure main, crowned by a great brass cog with a glowing aether core.
//! Warm amber gauges light the jambs while a cool aether veil hums across the
//! threshold, so the walk-through opening reads as a live, steam-driven gate
//! rather than a quiet stone arch.
//!
//! The one functional element is the [`GeneratorKind::Gateway`] zone child —
//! walking into it opens the destination picker. Everything else frames that
//! opening. Primitive-built (see [`crate::catalogue::items::util`]) and
//! authored in one flat ground-relative frame via [`assemble`], which
//! reparents every piece under the flat iron threshold plate (the root — never
//! tilt it, or the whole gate would spin with it).

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, quat_z, solid, sphere,
    torus, tube,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRASS, COPPER_ORANGE, GAUGE_AMBER, IRON_DARK, LAMP_GAS, brass, cog, copper, fx, glass, iron,
};

pub struct SteampunkGateway;

impl CatalogueEntry for SteampunkGateway {
    fn slug(&self) -> &'static str {
        "steampunk_gateway"
    }
    fn name(&self) -> &'static str {
        "Aether Gate"
    }
    fn description(&self) -> &'static str {
        "Brass-and-iron boiler gate crowned by a great cog with a glowing aether core."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Steampunk]
    }
    // No prosperity_band(): a gateway is placed near spawn in every seeded
    // room of the theme, so it must be available across all prosperity tiers.
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

/// The aether veil — a cool blue-green threshold glow that contrasts the warm
/// brass and names the gate. Deep-saturated at low strength so a broad-ish
/// strip reads as lit colour rather than a white lightbox.
const AETHER_TEAL: [f32; 3] = [0.42, 0.86, 0.92];

fn build_tree() -> Generator {
    let pillar_x = 1.9_f32; // supports flank a ~2.6 m walk-through gap
    let pillar_h = 3.6_f32;
    let pillar_cy = 2.1_f32; // centre → top at 3.9, under the lintel
    let lintel_y = 4.15_f32;

    // Riveted-iron threshold plate — the flat-base root. assemble() rebases
    // every other prim into this node's frame, so it stays untilted at origin.
    let mut prims = vec![prim(
        solid(cuboid_tapered([5.2, 0.3, 3.0], 0.0, iron(IRON_DARK))),
        [0.0, 0.15, 0.0],
        id_quat(),
    )];
    // Brass threshold inlay marking the walk path.
    prims.push(prim(
        solid(cuboid_tapered([2.6, 0.06, 1.3], 0.0, brass(BRASS))),
        [0.0, 0.32, 0.0],
        id_quat(),
    ));

    // Twin boiler-stack pylons flanking the opening.
    for sign in [-1.0_f32, 1.0] {
        let x = sign * pillar_x;
        // Banded iron pylon.
        prims.push(prim(
            solid(cuboid_tapered(
                [0.72, pillar_h, 0.72],
                0.06,
                iron(IRON_DARK),
            )),
            [x, pillar_cy, 0.0],
            id_quat(),
        ));
        // Brass collar bands.
        for band_y in [1.0_f32, 3.2] {
            prims.push(prim(
                solid(cuboid_tapered([0.86, 0.16, 0.86], 0.0, brass(BRASS))),
                [x, band_y, 0.0],
                id_quat(),
            ));
        }
        // Copper aether-riser standing on the plate at the outer-back corner,
        // rising past the lintel — a boiler stack venting steam.
        let rx = x + sign * 0.5;
        let rz = 0.55_f32;
        prims.push(prim(
            solid(tube(0.16, 0.10, 4.6, 10, copper(COPPER_ORANGE))),
            [rx, 2.6, rz],
            id_quat(),
        ));
        prims.push(prim(
            solid(torus(0.06, 0.2, brass(BRASS))),
            [rx, 4.9, rz],
            id_quat(),
        ));
        // Lit amber pressure gauge on the pylon front (−Z), housing seated so
        // it doesn't read as a floating tab from the side.
        prims.push(prim(
            solid(cylinder_tapered(0.22, 0.14, 12, 0.0, iron(IRON_DARK))),
            [x, 2.4, -0.42],
            quat_x(FRAC_PI_2),
        ));
        prims.push(prim(
            cylinder_tapered(0.17, 0.06, 12, 0.0, glass(GAUGE_AMBER, 2.2)),
            [x, 2.4, -0.5],
            quat_x(FRAC_PI_2),
        ));
        // Caged gas mantle on an inner bracket, warm-lighting the jamb.
        let inner_x = x - sign * 0.55;
        prims.push(prim(
            solid(cuboid_tapered([0.5, 0.05, 0.06], 0.0, brass(BRASS))),
            [(x + inner_x) * 0.5, 2.75, -0.18],
            id_quat(),
        ));
        prims.push(prim(
            solid(torus(0.03, 0.16, brass(BRASS))),
            [inner_x, 2.75, -0.18],
            quat_x(FRAC_PI_2),
        ));
        prims.push(prim(
            sphere(0.13, 3, glow(LAMP_GAS, 3.0)),
            [inner_x, 2.75, -0.18],
            id_quat(),
        ));
        // Brass finial spike atop the pylon.
        prims.push(prim(
            solid(cylinder_tapered(0.06, 0.5, 6, 0.5, brass(BRASS))),
            [x, 4.6, 0.0],
            id_quat(),
        ));
    }

    // Iron lintel spanning the pylons, with brass edge trim.
    prims.push(prim(
        solid(cuboid_tapered([4.7, 0.5, 0.9], 0.0, iron(IRON_DARK))),
        [0.0, lintel_y, 0.0],
        id_quat(),
    ));
    for trim_y in [lintel_y - 0.28, lintel_y + 0.28] {
        prims.push(prim(
            solid(cuboid_tapered([4.84, 0.1, 0.98], 0.0, brass(BRASS))),
            [0.0, trim_y, 0.0],
            id_quat(),
        ));
    }
    // Copper pressure main across the back-top, linking the two risers, with
    // brass flange joints at the ends.
    prims.push(prim(
        solid(tube(0.13, 0.08, 4.2, 10, copper(COPPER_ORANGE))),
        [0.0, 4.62, 0.55],
        quat_z(FRAC_PI_2),
    ));
    for fx_x in [-2.0_f32, 2.0] {
        prims.push(prim(
            solid(torus(0.05, 0.18, brass(BRASS))),
            [fx_x, 4.62, 0.55],
            quat_z(FRAC_PI_2),
        ));
    }

    // Brass signage banner on the lintel front (−Z) with a lit central gauge —
    // the gate's face.
    prims.push(prim(
        solid(cuboid_tapered([2.4, 0.42, 0.08], 0.0, brass(BRASS))),
        [0.0, lintel_y, -0.5],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.19, 0.05, 14, 0.0, iron(IRON_DARK))),
        [0.0, lintel_y, -0.54],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        cylinder_tapered(0.15, 0.05, 14, 0.0, glass(GAUGE_AMBER, 2.2)),
        [0.0, lintel_y, -0.6],
        quat_x(FRAC_PI_2),
    ));

    // Crown: a great brass cog facing −Z with a glowing aether core, flanked by
    // two smaller iron cogs meshing at the lintel corners — the signature
    // steampunk silhouette. cog() lies flat; quat_x(−π/2) stands it to face −Z.
    prims.push(cog(
        [0.0, 4.95, -0.2],
        quat_x(-FRAC_PI_2),
        0.95,
        0.28,
        14,
        brass(BRASS),
        iron(IRON_DARK),
    ));
    prims.push(prim(
        sphere(0.34, 3, glow(AETHER_TEAL, 3.0)),
        [0.0, 4.95, -0.42],
        id_quat(),
    ));
    for sign in [-1.0_f32, 1.0] {
        prims.push(cog(
            [sign * 1.15, 4.4, -0.2],
            quat_x(-FRAC_PI_2),
            0.45,
            0.24,
            10,
            iron(IRON_DARK),
            brass(BRASS),
        ));
    }

    // Aether veil — a thin cool glow strip humming across the top of the
    // opening. Deep-saturated at low strength so it reads as lit colour.
    prims.push(prim(
        cuboid_tapered([2.7, 0.1, 0.14], 0.0, glow(AETHER_TEAL, 2.6)),
        [0.0, 3.82, 0.0],
        id_quat(),
    ));

    // The walk-in zone between the pylons: bottom at the plate top, headroom
    // under the lintel. The one functional element.
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, 1.95, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: the aether engine's chug, steam venting from both risers.
    root.audio = fx::engine_chug();
    root.children
        .push(fx::steam_vent([2.4, 5.05, 0.55], 0x4E17_0001));
    root.children
        .push(fx::steam_vent([-2.4, 5.05, 0.55], 0x4E17_0002));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&SteampunkGateway.build(""), "steampunk_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is set-dressing, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = SteampunkGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
