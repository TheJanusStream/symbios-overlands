//! Cathedral — the Gothic-Horror landmark and the kit's lit hero. A tall dark
//! stone nave with a great glowing rose window and lancets, buttress piers
//! with pinnacles, a steep slate roof and twin front spires. ~14 m wide, so it
//! looms over the necropolis and reads as the cathedral from across the home
//! region. Its stained glass is the trim escalation's ruin pass snuffs to a
//! black, gutted shell.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the stone base.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, foundation_block, id_quat, prim,
    quat_x, quat_z, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    DEADWOOD, IRON_BLACK, STAINED_GLOW, STAINED_TINT, STONE_DARK, fx, iron, lancet, pointed_arch,
    spire, stained, stone, wood,
};

pub struct Cathedral;

impl CatalogueEntry for Cathedral {
    fn slug(&self) -> &'static str {
        "cathedral"
    }
    fn name(&self) -> &'static str {
        "Cathedral"
    }
    fn description(&self) -> &'static str {
        "Dark stone nave with a glowing rose window, buttress piers and twin spires."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 12.0,
            min_spawn_dist: 54.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 1.0_f32;
    let w = 11.0_f32;
    let d = 7.0_f32;
    let nave_h = 8.0_f32;
    let nave_top = base_h + nave_h;
    let half_d = d * 0.5;
    let zf = -half_d; // -Z hero front face (render FRONT)
    // Proud-of-the-front-wall offset: more negative Z stands toward the camera.
    let proud = |k: f32| zf - k;
    let st = || stone(STONE_DARK);

    let mut prims = vec![
        // Stone base — the root.
        prim(
            solid(cuboid_tapered([14.0, base_h, 9.0], 0.0, st())),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    prims.push(foundation_block(14.0, 9.0, [0.0, 0.0], 1.5));

    // Stone nave.
    prims.push(prim(
        solid(cuboid_tapered([w, nave_h, d], 0.0, st())),
        [0.0, base_h + nave_h * 0.5, 0.0],
        id_quat(),
    ));

    // Steep slate gable roof — ridge along Z so the gable faces the -Z front.
    prims.push(prim(
        solid(cuboid_tapered_xz(
            [w + 0.5, 3.8, d + 0.5],
            [0.92, 0.0],
            st(),
        )),
        [0.0, nave_top + 1.9, 0.0],
        id_quat(),
    ));
    // Gable-apex cross finial on the front.
    prims.push(prim(
        solid(cuboid_tapered([0.16, 1.3, 0.16], 0.0, st())),
        [0.0, nave_top + 3.7, zf + 0.4],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.7, 0.16, 0.16], 0.0, st())),
        [0.0, nave_top + 4.0, zf + 0.4],
        id_quat(),
    ));

    // ---- West front (-Z): pointed portal, rose window, flanking lancets. ----
    // Great pointed-arch portal.
    let portal_half = 1.0_f32;
    let portal_spring = base_h + 2.3;
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.36, portal_spring - base_h, 0.55],
                0.0,
                st(),
            )),
            [
                s * portal_half,
                base_h + (portal_spring - base_h) * 0.5,
                proud(0.05),
            ],
            id_quat(),
        ));
    }
    prims.extend(pointed_arch(
        [0.0, portal_spring, proud(0.05)],
        portal_half,
        0.18,
        st(),
    ));
    // Recessed dark timber door with iron bands.
    prims.push(prim(
        solid(cuboid_tapered(
            [portal_half * 1.7, 2.5, 0.16],
            0.06,
            wood(DEADWOOD),
        )),
        [0.0, base_h + 1.25, proud(-0.1)],
        id_quat(),
    ));
    for by in [base_h + 0.7, base_h + 1.8] {
        prims.push(prim(
            cuboid_tapered([portal_half * 1.7, 0.1, 0.06], 0.0, iron(IRON_BLACK)),
            [0.0, by, proud(0.02)],
            id_quat(),
        ));
    }

    // Great rose window: glowing light behind a stone tracery wheel.
    let rose_y = base_h + 6.0;
    prims.push(prim(
        cylinder_tapered(1.35, 0.25, 22, 0.0, stained(STAINED_GLOW, 2.6)),
        [0.0, rose_y, zf],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        solid(torus(0.16, 1.5, st())),
        [0.0, rose_y, proud(0.05)],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        solid(torus(0.1, 0.72, st())),
        [0.0, rose_y, proud(0.06)],
        quat_x(FRAC_PI_2),
    ));
    for k in 0..4 {
        prims.push(prim(
            cuboid_tapered([2.9, 0.09, 0.14], 0.0, st()),
            [0.0, rose_y, proud(0.06)],
            quat_z(k as f32 * FRAC_PI_4),
        ));
    }
    prims.push(prim(
        solid(cylinder_tapered(0.22, 0.22, 8, 0.0, st())),
        [0.0, rose_y, proud(0.08)],
        quat_x(FRAC_PI_2),
    ));

    // Flanking lit lancets either side of the portal.
    for s in [-1.0_f32, 1.0] {
        prims.extend(lancet(s * 3.2, base_h + 1.7, zf, 0.55, 2.0, 2.2));
    }

    // ---- Flanks (±X): buttress piers, pinnacles, flying buttresses, clerestory. ----
    for s in [-1.0_f32, 1.0] {
        for z in [-2.0_f32, 0.4, 2.6] {
            let px = s * (w * 0.5 + 0.5);
            let pier_h = nave_h - 1.6;
            prims.push(prim(
                solid(cuboid_tapered([0.9, pier_h, 1.1], 0.14, st())),
                [px, base_h + pier_h * 0.5, z],
                id_quat(),
            ));
            prims.extend(spire([px, base_h + pier_h, z], 0.5, 1.7, st()));
            // Flying buttress: a sloped strut from the pier top up to the
            // clerestory wall.
            let nx = s * (w * 0.5);
            let ay = base_h + pier_h;
            let by = base_h + nave_h - 0.4;
            let dx = nx - px;
            let dy = by - ay;
            let len = (dx * dx + dy * dy).sqrt();
            prims.push(prim(
                solid(cuboid_tapered([len, 0.24, 0.34], 0.0, st())),
                [(px + nx) * 0.5, (ay + by) * 0.5, z],
                quat_z(dy.atan2(dx)),
            ));
        }
        // Clerestory lit slits between the buttresses.
        for z in [-0.8_f32, 1.5] {
            prims.push(prim(
                cuboid_tapered([0.2, 3.0, 0.8], 0.0, stained(STAINED_TINT, 2.0)),
                [s * (w * 0.5 + 0.02), base_h + 4.0, z],
                id_quat(),
            ));
        }
    }

    // ---- Twin west towers with broach spires, flanking the front. ----
    for s in [-1.0_f32, 1.0] {
        let tx = s * (w * 0.5 - 0.7);
        let tz = -half_d + 0.7;
        let tower_h = nave_h + 2.0;
        let tower_top = base_h + tower_h;
        prims.push(prim(
            solid(cuboid_tapered([1.9, tower_h, 1.9], 0.02, st())),
            [tx, base_h + tower_h * 0.5, tz],
            id_quat(),
        ));
        // Belfry lancet on the tower's -Z front.
        prims.extend(lancet(tx, tower_top - 2.8, tz - 0.95, 0.42, 1.4, 1.8));
        // String-course band under the spire.
        prims.push(prim(
            solid(cuboid_tapered([2.05, 0.25, 2.05], 0.0, st())),
            [tx, tower_top - 0.15, tz],
            id_quat(),
        ));
        prims.extend(spire([tx, tower_top, tz], 1.05, 3.8, st()));
    }

    let mut root = assemble(prims);
    // Signature life: a ghostly drone in the nave, mist creeping out front.
    root.audio = fx::ghostly_drone();
    root.children
        .push(fx::ground_mist([0.0, 0.3, zf - 4.0], 0x60F0_CA12));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Cathedral.build(""), "cathedral");
    }

    #[test]
    fn has_stained_glow() {
        assert!(crate::catalogue::items::util::has_emissive(
            &Cathedral.build("")
        ));
    }
}
