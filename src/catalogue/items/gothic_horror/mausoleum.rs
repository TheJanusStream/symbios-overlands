//! Mausoleum — a Gothic-Horror secondary. A columned stone tomb under a
//! pediment, an iron gate barring its door and a small lit window above. The
//! family crypt of the necropolis; its window is emissive trim the ruin pass
//! can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the base.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, foundation_block, id_quat, prim,
    quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{IRON_BLACK, STAINED_GLOW, STONE_DARK, fx, iron, pointed_arch, spire, stained, stone};

pub struct Mausoleum;

impl CatalogueEntry for Mausoleum {
    fn slug(&self) -> &'static str {
        "mausoleum"
    }
    fn name(&self) -> &'static str {
        "Mausoleum"
    }
    fn description(&self) -> &'static str {
        "Columned stone tomb under a pediment with an iron gate and a small lit window."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.6_f32;
    let body_h = 3.6_f32;
    let body_top = base_h + body_h;
    let half_d = 2.0_f32;
    let zf = -half_d; // -Z hero front
    let proud = |k: f32| zf - k;
    let st = || stone(STONE_DARK);

    let mut prims = vec![
        // Stone base — the root.
        prim(
            solid(cuboid_tapered([6.0, base_h, 5.0], 0.0, st())),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    prims.push(foundation_block(6.0, 5.0, [0.0, 0.0], 1.2));

    // Tomb body.
    prims.push(prim(
        solid(cuboid_tapered([4.5, body_h, 4.0], 0.0, st())),
        [0.0, base_h + body_h * 0.5, 0.0],
        id_quat(),
    ));

    // ---- West front (-Z): pointed-arch iron gate, oculus, gable. ----
    // Engaged colonnettes flanking the portal.
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.26, body_h, 10, 0.03, st())),
            [s * 1.5, base_h + body_h * 0.5, proud(0.1)],
            id_quat(),
        ));
        // Pinnacle capital.
        prims.extend(spire([s * 1.5, body_top, proud(0.1)], 0.34, 1.0, st()));
    }
    // Pointed-arch portal: jambs + arch.
    let portal_half = 0.85_f32;
    let portal_spring = base_h + 1.9;
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.22, portal_spring - base_h, 0.4],
                0.0,
                st(),
            )),
            [
                s * portal_half,
                base_h + (portal_spring - base_h) * 0.5,
                proud(0.04),
            ],
            id_quat(),
        ));
    }
    prims.extend(pointed_arch(
        [0.0, portal_spring, proud(0.04)],
        portal_half,
        0.13,
        st(),
    ));
    // Iron gate: dark recess + vertical bars.
    prims.push(prim(
        cuboid_tapered([portal_half * 1.7, 2.4, 0.1], 0.0, iron(IRON_BLACK)),
        [0.0, base_h + 1.2, proud(-0.06)],
        id_quat(),
    ));
    for i in 0..4 {
        let x = -0.6 + i as f32 * 0.4;
        prims.push(prim(
            solid(cylinder_tapered(0.04, 2.4, 6, 0.0, iron(IRON_BLACK))),
            [x, base_h + 1.2, proud(0.0)],
            id_quat(),
        ));
    }
    // Oculus (small rose) above the gate.
    let oc_y = body_top - 0.55;
    prims.push(prim(
        cylinder_tapered(0.5, 0.2, 16, 0.0, stained(STAINED_GLOW, 2.2)),
        [0.0, oc_y, zf],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        solid(torus(0.08, 0.55, st())),
        [0.0, oc_y, proud(0.04)],
        quat_x(FRAC_PI_2),
    ));
    for (bw, bh) in [(1.0_f32, 0.07_f32), (0.07, 1.0)] {
        prims.push(prim(
            cuboid_tapered([bw, bh, 0.12], 0.0, st()),
            [0.0, oc_y, proud(0.05)],
            id_quat(),
        ));
    }

    // Cornice.
    prims.push(prim(
        solid(cuboid_tapered([4.8, 0.32, 4.3], 0.0, st())),
        [0.0, body_top + 0.16, 0.0],
        id_quat(),
    ));
    // Steep gable roof — ridge along Z, gable faces the -Z front.
    prims.push(prim(
        solid(cuboid_tapered_xz([4.7, 2.2, 4.2], [0.9, 0.0], st())),
        [0.0, body_top + 1.42, 0.0],
        id_quat(),
    ));
    // Gable-apex cross finial.
    prims.push(prim(
        solid(cuboid_tapered([0.13, 1.0, 0.13], 0.0, st())),
        [0.0, body_top + 2.6, zf + 0.3],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.55, 0.13, 0.13], 0.0, st())),
        [0.0, body_top + 2.85, zf + 0.3],
        id_quat(),
    ));
    // Corner pinnacles on the cornice.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.extend(spire(
            [sx * 2.1, body_top + 0.3, sz * 1.85],
            0.26,
            1.1,
            st(),
        ));
    }

    let mut root = assemble(prims);
    // Signature life: mist creeping around the tomb out front.
    root.children
        .push(fx::ground_mist([0.0, 0.3, zf - 2.5], 0x60F0_3A12));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Mausoleum.build(""), "mausoleum");
    }
}
