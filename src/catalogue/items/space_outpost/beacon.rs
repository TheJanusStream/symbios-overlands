//! Beacon — a Space-Outpost prop. A landing beacon: a steel mast topped by a
//! glowing red light with a small solar cell. Scatter clutter marking the
//! base perimeter; its light is emissive trim the ruin pass can darken.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_mul, quat_x, quat_y,
    quat_z, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BEACON_RED, PV_BLUE, STEEL_DARK, pv, pv_panel, steel};

pub struct Beacon;

impl CatalogueEntry for Beacon {
    fn slug(&self) -> &'static str {
        "beacon"
    }
    fn name(&self) -> &'static str {
        "Beacon"
    }
    fn description(&self) -> &'static str {
        "Steel mast topped by a glowing red light with a small solar cell."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SpaceOutpost]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::OUTPOST_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.6,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mast_top = 2.3_f32;
    let mut prims = vec![
        // Steel foot — the root.
        prim(
            solid(cuboid_tapered([0.6, 0.18, 0.6], 0.0, steel(STEEL_DARK))),
            [0.0, 0.09, 0.0],
            id_quat(),
        ),
    ];
    // Three tripod legs bracing the mast: each foot plants wide on the
    // ground (radius 0.6) and the top converges in at the mast collar
    // (radius 0.12, y≈0.95). The leg leans inward going up — `quat_z(beta)`
    // tilts a vertical leg in the radial plane, then `quat_y(-a)` yaws it to
    // its azimuth. (The old version tilted about the centre, splaying the
    // tops outward into air and driving the feet through the foot block.)
    let r_foot = 0.6_f32;
    let r_top = 0.12_f32;
    let h_top = 0.95_f32;
    let leg_len = ((r_foot - r_top).powi(2) + h_top * h_top).sqrt();
    let beta = (r_foot - r_top).atan2(h_top);
    for i in 0..3 {
        let a = i as f32 / 3.0 * TAU;
        let r_mid = (r_foot + r_top) * 0.5;
        prims.push(prim(
            solid(cuboid_tapered(
                [0.09, leg_len, 0.09],
                0.0,
                steel(STEEL_DARK),
            )),
            [a.cos() * r_mid, h_top * 0.5, a.sin() * r_mid],
            quat_mul(quat_y(-a), quat_z(beta)),
        ));
    }
    // Mast.
    prims.push(prim(
        solid(cylinder_tapered(0.1, 2.2, 6, 0.05, steel(STEEL_DARK))),
        [0.0, 1.2, 0.0],
        id_quat(),
    ));
    // Angled solar cell partway up (a framed mini PV panel).
    let mut cell = pv_panel(0.5, 0.6, pv(PV_BLUE), steel(STEEL_DARK));
    cell.transform.translation = crate::pds::Fp3([0.0, 1.7, 0.28]);
    cell.transform.rotation = quat_x(-0.55);
    prims.push(cell);

    // Caged lamp head at the top: a base disc, a ring of cage bars, the
    // glowing light inside, and a cap — so it reads as a beacon fixture.
    prims.push(prim(
        solid(cylinder_tapered(0.24, 0.08, 10, 0.0, steel(STEEL_DARK))),
        [0.0, mast_top, 0.0],
        id_quat(),
    ));
    for i in 0..5 {
        let a = i as f32 / 5.0 * TAU;
        prims.push(prim(
            solid(cylinder_tapered(0.025, 0.46, 4, 0.0, steel(STEEL_DARK))),
            [a.cos() * 0.2, mast_top + 0.27, a.sin() * 0.2],
            id_quat(),
        ));
    }
    prims.push(prim(
        sphere(0.2, 4, glow(BEACON_RED, 2.5)),
        [0.0, mast_top + 0.27, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.24, 0.1, 10, 0.6, steel(STEEL_DARK))),
        [0.0, mast_top + 0.55, 0.0],
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
        assert_sanitize_stable(&Beacon.build(""), "beacon");
    }
}
