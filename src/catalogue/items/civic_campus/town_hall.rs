//! Town hall — the Civic/Campus landmark and the kit's lit hero. A
//! neoclassical stone hall behind a marble columned portico and pediment,
//! crowned by a verdigris copper dome lantern, its tall windows and flanking
//! lamps glowing over the steps. ~14 m wide, so it anchors the quarter and
//! reads as the seat of the town from across the home region. Its windows,
//! lamps and lit lantern are the trim escalation's ruin pass snuffs to a
//! dark, shuttered hall.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the base.

use crate::catalogue::items::space_outpost::dome_ribs;
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, foundation_block, glow, id_quat,
    prim, solid, sphere, torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CLOCK_LIT, CONCRETE_GREY, COPPER_VERDIGRIS, GLASS_TINT, LAMP_WARM, MARBLE_WHITE, STEEL_GREY,
    STONE_PALE, WINDOW_WARM, column, concrete, copper, fx, glass, marble, steel, stone,
};

pub struct TownHall;

impl CatalogueEntry for TownHall {
    fn slug(&self) -> &'static str {
        "town_hall"
    }
    fn name(&self) -> &'static str {
        "Town Hall"
    }
    fn description(&self) -> &'static str {
        "Neoclassical stone hall with a marble portico, copper dome lantern and lit windows."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CivicCampus]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CAMPUS_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 14.0,
            min_spawn_dist: 52.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.9_f32;
    let body_h = 6.0_f32;
    let body_top = base_h + body_h;
    // The portico, steps, doors, lit windows and lamps all face the -Z render
    // front (the contact sheet's lead tile looks down -Z); the plain stone
    // flanks and back fall away toward +Z.
    let fz = -1.0_f32;

    let mut prims = vec![
        // Marble stylobate base — the root.
        prim(
            solid(cuboid_tapered(
                [16.0, base_h, 12.0],
                0.0,
                marble(MARBLE_WHITE),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    prims.push(foundation_block(16.0, 12.0, [0.0, 0.0], 1.5));

    // Stone hall body.
    prims.push(prim(
        solid(cuboid_tapered([13.0, body_h, 9.0], 0.0, stone(STONE_PALE))),
        [0.0, base_h + body_h * 0.5, 0.0],
        id_quat(),
    ));
    // Lit tall windows across the front behind the colonnade, set proud of the
    // -4.5 front wall.
    prims.push(prim(
        cuboid_tapered([10.0, 2.8, 0.2], 0.0, glass(GLASS_TINT, 1.3)),
        [0.0, base_h + 2.6, fz * 4.55],
        id_quat(),
    ));
    // Bronze entrance doors.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.2, 3.0, 0.3],
            0.0,
            copper(COPPER_VERDIGRIS),
        )),
        [0.0, base_h + 1.5, fz * 4.62],
        id_quat(),
    ));

    // Front steps descending to the quad — each course sits a touch lower and
    // further out, so no two tread faces share a plane.
    for k in 0..3 {
        let kf = k as f32;
        prims.push(prim(
            solid(cuboid_tapered(
                [12.0 - kf * 0.4, 0.3, 1.0],
                0.0,
                marble(MARBLE_WHITE),
            )),
            [0.0, base_h - 0.2 - kf * 0.3, fz * (5.6 + kf * 0.9)],
            id_quat(),
        ));
    }

    // Marble colonnade across the front — proper based-and-capitalled columns.
    for x in [-5.0_f32, -3.0, -1.0, 1.0, 3.0, 5.0] {
        prims.extend(column(
            x,
            fz * 5.0,
            base_h,
            body_h - 0.7,
            0.5,
            marble(MARBLE_WHITE),
        ));
    }
    // Architrave + frieze entablature over the colonnade.
    prims.push(prim(
        solid(cuboid_tapered([12.6, 0.9, 1.5], 0.0, marble(MARBLE_WHITE))),
        [0.0, base_h + body_h - 0.5, fz * 4.9],
        id_quat(),
    ));
    // Triangular pediment gable: pinch the front X width to an apex ridge,
    // keep the full depth (a gable, not a hipped pyramid).
    prims.push(prim(
        solid(cuboid_tapered_xz(
            [12.6, 1.7, 1.5],
            [0.99, 0.0],
            marble(MARBLE_WHITE),
        )),
        [0.0, base_h + body_h + 0.35, fz * 4.9],
        id_quat(),
    ));

    // Roof slab + a verdigris copper dome on a lit lantern drum.
    prims.push(prim(
        solid(cuboid_tapered(
            [13.4, 0.4, 9.4],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, body_top + 0.2, 0.0],
        id_quat(),
    ));
    prims.extend(dome_lantern(body_top + 0.4));

    // Flanking entrance lamps on steel posts — emissive globes out front.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.1, 2.2, 8, 0.0, steel(STEEL_GREY))),
            [sx * 5.5, base_h + 1.1, fz * 6.0],
            id_quat(),
        ));
        prims.push(prim(
            sphere(0.3, 3, glow(LAMP_WARM, 3.0)),
            [sx * 5.5, base_h + 2.4, fz * 6.0],
            id_quat(),
        ));
    }

    let mut root = assemble(prims);
    // Signature life: a calm quad bed and drifting seed-fluff out front.
    root.audio = fx::campus_calm();
    root.children
        .push(fx::seed_drift([0.0, 1.5, fz * 9.0], 0x0C1F_5A11));
    root
}

/// The crowning lantern + verdigris copper dome, built around `base_y` (the top
/// of the roof slab). A stone drum ringed with copper colonnettes and a warm
/// lit lantern band, a cornice, a true hemisphere cap with a meridian rib cage,
/// and a gilt finial orb. Returned for the [`assemble`] list.
fn dome_lantern(base_y: f32) -> Vec<Generator> {
    let drum_r = 2.0_f32;
    let drum_h = 1.6_f32;
    let drum_top = base_y + drum_h;
    let mut out = vec![
        // Stone drum.
        prim(
            solid(cylinder_tapered(drum_r, drum_h, 18, 0.0, stone(STONE_PALE))),
            [0.0, base_y + drum_h * 0.5, 0.0],
            id_quat(),
        ),
        // Warm lit lantern band recessed inside the colonnette ring.
        prim(
            cylinder_tapered(drum_r - 0.2, drum_h - 0.5, 16, 0.0, glow(WINDOW_WARM, 1.6)),
            [0.0, base_y + drum_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    // Copper colonnettes standing proud of the lit band so it reads as a ringed
    // lantern, not a washed glowing cylinder.
    for k in 0..8 {
        let a = k as f32 / 8.0 * std::f32::consts::TAU;
        out.push(prim(
            solid(cuboid_tapered(
                [0.14, drum_h - 0.3, 0.14],
                0.0,
                copper(COPPER_VERDIGRIS),
            )),
            [
                a.cos() * (drum_r + 0.02),
                base_y + drum_h * 0.5,
                a.sin() * (drum_r + 0.02),
            ],
            id_quat(),
        ));
    }
    // Cornice ring atop the drum.
    out.push(prim(
        solid(torus(0.14, drum_r + 0.05, copper(COPPER_VERDIGRIS))),
        [0.0, drum_top, 0.0],
        id_quat(),
    ));
    // True hemisphere dome cap (upper latitude band of a sphere).
    let dome_r = 2.05_f32;
    out.push(prim(
        solid(with_cut(
            sphere(dome_r, 6, copper(COPPER_VERDIGRIS)),
            [0.0, 1.0],
            [0.5, 1.0],
            0.0,
        )),
        [0.0, drum_top, 0.0],
        id_quat(),
    ));
    // Meridian rib cage over the dome.
    out.extend(dome_ribs(
        [0.0, drum_top, 0.0],
        dome_r + 0.03,
        6,
        copper(COPPER_VERDIGRIS),
    ));
    // Gilt finial: a copper neck and a lit orb.
    out.push(prim(
        solid(cylinder_tapered(
            0.18,
            0.6,
            10,
            0.4,
            copper(COPPER_VERDIGRIS),
        )),
        [0.0, drum_top + dome_r, 0.0],
        id_quat(),
    ));
    out.push(prim(
        sphere(0.28, 3, glow(CLOCK_LIT, 2.0)),
        [0.0, drum_top + dome_r + 0.5, 0.0],
        id_quat(),
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&TownHall.build(""), "town_hall");
    }

    #[test]
    fn has_lit_windows_and_lamps() {
        assert!(crate::catalogue::items::util::has_emissive(
            &TownHall.build("")
        ));
    }
}
