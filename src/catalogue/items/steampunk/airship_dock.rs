//! Airship dock — a Steampunk secondary. An iron lattice mooring mast (four
//! inward-leaning corner legs cinched by brass band frames and crossed
//! diagonal braces) with a brass docking ring and a plank gangway, a small
//! dirigible moored alongside — a smooth copper gas-bag over an iron gondola.
//! The aerial harbour of the works.
//!
//! The envelope is a [`prim_scaled`] sphere stretched along Z into a smooth
//! ellipsoid, framed with iron rings and capped by a pointed brass nose.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the mast base.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, glow, id_quat, prim, prim_scaled, quat_mul, quat_x, quat_z,
    solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRASS, COPPER_ORANGE, IRON_DARK, WOOD_BROWN, brass, copper, fx, iron, plank};

pub struct AirshipDock;

impl CatalogueEntry for AirshipDock {
    fn slug(&self) -> &'static str {
        "airship_dock"
    }
    fn name(&self) -> &'static str {
        "Airship Dock"
    }
    fn description(&self) -> &'static str {
        "Iron mooring mast with a docking ring and a small dirigible moored alongside."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Steampunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::STEAM_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 44.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.6_f32;
    let mast_h = 10.0_f32;
    let mast_top = base_h + mast_h;
    let tilt = 0.05_f32;
    // Inward-leaning leg offset at height fraction f (legs pivot about centre).
    let half = |f: f32| 1.0 + (0.5 - f) * mast_h * tilt;

    let mut prims = vec![
        // Iron base — the root.
        prim(
            solid(cuboid_tapered([3.2, base_h, 3.2], 0.0, iron(IRON_DARK))),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Lattice mast: four inward-leaning corner legs.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.18, mast_h, 0.18], 0.0, iron(IRON_DARK))),
                [sx * 1.0, base_h + mast_h * 0.5, sz * 1.0],
                quat_mul(quat_z(sx * tilt), quat_x(-sz * tilt)),
            ));
        }
    }
    // Brass band frames cinching the lattice at three levels.
    for f in [0.18_f32, 0.55, 0.92] {
        let y = base_h + mast_h * f;
        let h = half(f);
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [h * 2.0 + 0.18, 0.12, 0.14],
                    0.0,
                    brass(BRASS),
                )),
                [0.0, y, sz * h],
                id_quat(),
            ));
        }
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.14, 0.12, h * 2.0 + 0.18],
                    0.0,
                    brass(BRASS),
                )),
                [sx * h, y, 0.0],
                id_quat(),
            ));
        }
    }
    // Crossed iron diagonal braces on the front (−Z) and back faces.
    for sz in [-1.0_f32, 1.0] {
        for s in [-1.0_f32, 1.0] {
            let h = half(0.55);
            let span = 2.0 * h;
            let rise = mast_h * 0.74;
            let len = (span * span + rise * rise).sqrt();
            prims.push(prim(
                solid(cuboid_tapered([0.09, len, 0.09], 0.0, iron(IRON_DARK))),
                [0.0, base_h + mast_h * 0.55, sz * h],
                quat_z(s * span.atan2(rise)),
            ));
        }
    }
    // Brass docking ring at the top.
    prims.push(prim(
        solid(torus(0.14, 0.78, brass(BRASS))),
        [0.0, mast_top + 0.2, 0.0],
        id_quat(),
    ));
    // Plank gangway reaching out toward the gondola.
    prims.push(prim(
        solid(cuboid_tapered([3.0, 0.2, 1.0], 0.0, plank(WOOD_BROWN))),
        [2.0, mast_top - 2.5, 3.2],
        id_quat(),
    ));

    // Moored dirigible: a smooth copper gas-bag (scaled-sphere ellipsoid laid
    // along Z), framed with iron rings and tapering to a nose.
    let ship_z = 5.6_f32;
    let ship_y = 9.0_f32;
    prims.push(prim_scaled(
        solid(sphere(1.4, 6, copper(COPPER_ORANGE))),
        [0.0, ship_y, ship_z],
        id_quat(),
        [1.0, 1.0, 2.6],
    ));
    // Frame rings around the bag.
    for dz in [-1.4_f32, 0.0, 1.4] {
        let r = 1.4 * (1.0 - (dz / 3.64).powi(2)).max(0.05).sqrt() + 0.04;
        prims.push(prim(
            solid(torus(0.06, r, iron(IRON_DARK))),
            [0.0, ship_y, ship_z + dz],
            quat_x(FRAC_PI_2),
        ));
    }
    // Pointed brass nose at the −Z front end.
    prims.push(prim(
        solid(cone(0.5, 1.0, 8, brass(BRASS))),
        [0.0, ship_y, ship_z - 4.1],
        quat_x(-FRAC_PI_2),
    ));
    // Tail fins in a cross near the +Z tip, where the tapering bag is narrow
    // enough that the blades clearly protrude as a cruciform tail.
    let fin_z = ship_z + 3.5;
    prims.push(prim(
        solid(cuboid_tapered(
            [1.9, 0.14, 1.4],
            0.55,
            copper(COPPER_ORANGE),
        )),
        [0.0, ship_y, fin_z],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.14, 1.9, 1.4],
            0.55,
            copper(COPPER_ORANGE),
        )),
        [0.0, ship_y, fin_z],
        id_quat(),
    ));
    // Iron gondola slung beneath, with a lit window band.
    prims.push(prim(
        solid(cuboid_tapered([1.1, 0.55, 2.6], 0.12, iron(IRON_DARK))),
        [0.0, ship_y - 1.7, ship_z],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.08, 0.32, 2.0], 0.0, glow([1.0, 0.5, 0.14], 2.3)),
            [sx * 0.56, ship_y - 1.65, ship_z],
            id_quat(),
        ));
    }

    let mut root = assemble(prims);
    // Signature life: steam venting from the mast head.
    root.children
        .push(fx::steam_vent([0.0, mast_top + 0.4, 0.0], 0x57EA_D0C2));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&AirshipDock.build(""), "airship_dock");
    }
}
