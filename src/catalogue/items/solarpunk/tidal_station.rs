//! Tidal station - a Solarpunk secondary that leads a settlement on its
//! shore (#1796). A turbine hall of timber posts and tall glazing under a
//! living turf roof, its two generator sets seen through the glass; a row of
//! wave-energy arms along the deck's -Z (water) edge, each riding a float on
//! a hydraulic ram; and a slender lattice mast carrying a wind turbine 22 m
//! up over the quay.
//!
//! Primitive-built on a footing; the hall, each arm and the mast is a
//! subtree rooted where it stands, so the ruin pass leans each whole.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, SQRT_2, TAU};

use crate::catalogue::items::nordic::gable_roof;
use crate::catalogue::items::util::{
    attach, cone, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, lit_interior, nest,
    plane, prim, prim_scaled, quat_x, quat_y, quat_z, solid, sphere, strut, tube,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_PALE, GLASS_CLEAN, LEAF_GREEN, MOSS_GREEN, STEEL_GREY, STEEL_WHITE, TIMBER_WARM,
    concrete, crop_tufts, foliage, pane_grid, steel, timber,
};

/// The spacing circle the settlement keeps round the station: the deck's
/// half diagonal.
const CLEARANCE: f32 = 13.5;
/// The concrete deck: its width along the shore, its depth inland and the
/// height of its top.
const DECK_W: f32 = 23.0;
const DECK_D: f32 = 15.0;
const DECK_TOP: f32 = 0.5;
/// The deck's water-side (-Z) edge.
const FRONT: f32 = -DECK_D * 0.5;
/// The hall: its X and Z extents and its wall height.
const HALL_X: [f32; 2] = [-10.0, 5.0];
const HALL_Z: [f32; 2] = [-3.0, 5.0];
const WALL_H: f32 = 6.2;
/// Where the mast stands.
const MAST: [f32; 2] = [8.5, 2.5];

pub struct TidalStation;

impl CatalogueEntry for TidalStation {
    fn slug(&self) -> &'static str {
        "tidal_station"
    }
    fn name(&self) -> &'static str {
        "Tidal Station"
    }
    fn description(&self) -> &'static str {
        "Green-roofed timber-and-glass turbine hall with wave-energy floats and a lattice turbine mast."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SOLAR_BAND
    }
    /// It works the tide, so it stands where the land meets the water.
    fn site(&self) -> crate::catalogue::SitePreference {
        crate::catalogue::SitePreference::Shore
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut root = footing(DECK_W, DECK_D, [0.0, 0.0], CLEARANCE);
    attach(
        &mut root,
        prim(
            solid(cuboid_tapered(
                [DECK_W, DECK_TOP, DECK_D],
                0.0,
                concrete(CONCRETE_PALE),
            )),
            [0.0, DECK_TOP * 0.5, 0.0],
            id_quat(),
        ),
    );
    attach(&mut root, hall());
    attach(&mut root, planter());
    for x in [-9.0_f32, -5.0, -1.0, 3.0, 7.0] {
        attach(&mut root, wave_arm(x));
    }
    attach(&mut root, mast());
    // Signature life: the swell working through the floats.
    root.audio = crate::catalogue::items::fx::water_trickle();
    root
}

/// The turbine hall: timber end and back walls, a glazed -Z front between
/// timber posts on a sill under a header, a warm-lit back wall inside, two
/// generator sets on their plinths, and a turf gable roof.
fn hall() -> Generator {
    let [x0, x1] = HALL_X;
    let [z0, z1] = HALL_Z;
    let cx = (x0 + x1) * 0.5;
    let cz = (z0 + z1) * 0.5;
    let h = WALL_H + 0.02;
    let y = DECK_TOP - 0.02 + h * 0.5;
    let top = DECK_TOP + WALL_H;
    let inner_w = x1 - x0 - 0.56;
    let wood = timber(TIMBER_WARM);
    // The back wall, set between the end walls and 4 cm inside their faces.
    let back = prim(
        solid(cuboid_tapered([inner_w, h, 0.28], 0.0, wood.clone())),
        [cx, y, z1 - 0.18],
        id_quat(),
    );
    let mut parts = Vec::new();
    for ex in [x0 + 0.15, x1 - 0.15] {
        parts.push(prim(
            solid(cuboid_tapered([0.3, h, z1 - z0], 0.0, wood.clone())),
            [ex, y, cz],
            id_quat(),
        ));
    }
    // The glazed front: posts, a sill and a header, glass cards between.
    let fz = z0 + 0.15;
    let posts = [-6.25_f32, -2.5, 1.25];
    for px in posts {
        parts.push(prim(
            solid(cuboid_tapered([0.3, h, 0.3], 0.0, wood.clone())),
            [px, y, fz],
            id_quat(),
        ));
    }
    let sill_h = 0.7_f32;
    parts.push(prim(
        solid(cuboid_tapered([inner_w, sill_h, 0.36], 0.0, wood.clone())),
        [cx, DECK_TOP + sill_h * 0.5 - 0.01, fz],
        id_quat(),
    ));
    let header_h = 0.5_f32;
    parts.push(prim(
        solid(cuboid_tapered(
            [inner_w, header_h + 0.02, 0.36],
            0.0,
            wood.clone(),
        )),
        [cx, top - header_h * 0.5 + 0.01, fz],
        id_quat(),
    ));
    let glass_y0 = DECK_TOP + sill_h;
    let glass_y1 = top - header_h;
    let edges = [x0 + 0.3, posts[0], posts[1], posts[2], x1 - 0.3];
    for (k, bay) in edges.windows(2).enumerate() {
        // The end bays run to the end walls; the rest stop at the posts.
        let a = if k == 0 { bay[0] } else { bay[0] + 0.15 };
        let b = if k == 3 { bay[1] } else { bay[1] - 0.15 };
        parts.push(prim(
            plane(
                [b - a, glass_y1 - glass_y0],
                pane_grid(GLASS_CLEAN, 0.0, (3, 5)),
            ),
            [(a + b) * 0.5, (glass_y0 + glass_y1) * 0.5, fz],
            quat_x(-FRAC_PI_2),
        ));
    }
    // The room's light on the back wall's inner face, after dusk.
    parts.push(prim(
        cuboid_tapered(
            [inner_w - 0.2, WALL_H - 1.0, 0.04],
            0.0,
            crate::catalogue::items::street::lit_from_dusk(lit_interior([0.86, 0.74, 0.52], 0.5)),
        ),
        [cx, DECK_TOP + WALL_H * 0.5, z1 - 0.34],
        id_quat(),
    ));
    // The generator sets: a green drum laid along X on a concrete plinth.
    for gx in [-6.3_f32, -0.6] {
        let plinth = prim(
            solid(cuboid_tapered(
                [3.4, 0.6, 2.0],
                0.0,
                concrete(CONCRETE_PALE),
            )),
            [gx, DECK_TOP + 0.28, 2.0],
            id_quat(),
        );
        let drum = prim(
            solid(cylinder_tapered(
                1.05,
                3.0,
                20,
                0.0,
                steel([0.30, 0.50, 0.34]),
            )),
            [gx, DECK_TOP + 1.6, 2.0],
            quat_z(FRAC_PI_2),
        );
        let shaft_end = prim(
            solid(cylinder_tapered(0.45, 0.5, 14, 0.0, steel(STEEL_GREY))),
            [gx + 1.7, DECK_TOP + 1.6, 2.0],
            quat_z(FRAC_PI_2),
        );
        parts.push(nest(plinth, vec![drum, shaft_end]));
    }
    // The living roof, with a row of tufts along its ridge.
    let rise = 2.3_f32;
    parts.push(gable_roof(
        [x1 - x0 + 0.9, rise, z1 - z0 + 0.9],
        [cx, top + rise * 0.5, cz],
        foliage(MOSS_GREEN),
    ));
    parts.extend(crop_tufts(
        [cx, top + rise - 0.12, cz],
        [x1 - x0 - 2.0, 0.0],
        7,
        1,
        0.55,
        foliage(LEAF_GREEN),
    ));
    nest(back, parts)
}

/// A timber planter along the hall's glazed front, planted.
fn planter() -> Generator {
    let cx = (HALL_X[0] + HALL_X[1]) * 0.5;
    let z = HALL_Z[0] - 0.75;
    let bx = prim(
        solid(cuboid_tapered([9.0, 0.6, 0.9], 0.0, timber(TIMBER_WARM))),
        [cx, DECK_TOP + 0.29, z],
        id_quat(),
    );
    let tufts = crop_tufts(
        [cx, DECK_TOP + 0.56, z],
        [8.0, 0.2],
        7,
        1,
        0.5,
        foliage(LEAF_GREEN),
    );
    nest(bx, tufts)
}

/// One wave-energy arm at `x`: a pedestal on the deck, an arm reaching out
/// and down to a float resting on the deck's water edge, and a hydraulic ram
/// from the deck to the arm's middle.
fn wave_arm(x: f32) -> Generator {
    let pivot = [x, DECK_TOP + 1.25, -5.6];
    let float_at = [x, DECK_TOP + 0.1, FRONT - 0.1];
    let pedestal = prim(
        solid(cuboid_tapered([0.8, 1.4, 0.9], 0.0, steel(STEEL_GREY))),
        [x, DECK_TOP + 0.68, pivot[2]],
        id_quat(),
    );
    let mid = [
        (pivot[0] + float_at[0]) * 0.5,
        (pivot[1] + float_at[1]) * 0.5,
        (pivot[2] + float_at[2]) * 0.5,
    ];
    let parts = vec![
        strut(pivot, float_at, 0.16, 10, steel(STEEL_WHITE)),
        prim(
            solid(cylinder_tapered(0.24, 1.0, 12, 0.0, steel(STEEL_GREY))),
            pivot,
            quat_z(FRAC_PI_2),
        ),
        prim_scaled(
            solid(sphere(1.05, 6, steel(STEEL_WHITE))),
            float_at,
            id_quat(),
            [1.0, 0.55, 1.0],
        ),
        // A green band round the float's waist.
        prim(
            tube(1.07, 0.9, 0.16, 20, steel(LEAF_GREEN)),
            float_at,
            id_quat(),
        ),
        strut(
            [x, DECK_TOP - 0.02, mid[2] + 0.2],
            mid,
            0.07,
            8,
            steel(STEEL_GREY),
        ),
    ];
    nest(pedestal, parts)
}

/// The lattice mast: four legs leaning in from a concrete pad to a small
/// top deck, square collars and a zig-zag of braces, carrying a nacelle with
/// a three-bladed rotor facing -Z and a red warning light.
fn mast() -> Generator {
    let [x, z] = MAST;
    let pad_top = DECK_TOP + 0.4;
    let top = 21.0_f32;
    let (foot, head) = (1.0_f32, 0.25_f32);
    let hw = |y: f32| foot - (foot - head) * (y - pad_top) / (top - pad_top);
    let white = steel(STEEL_WHITE);
    let pad = prim(
        solid(cuboid_tapered(
            [2.6, 0.42, 2.6],
            0.0,
            concrete(CONCRETE_PALE),
        )),
        [x, DECK_TOP + 0.19, z],
        id_quat(),
    );
    let mut parts = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            parts.push(strut(
                [x + sx * foot, pad_top - 0.1, z + sz * foot],
                [x + sx * head, top + 0.05, z + sz * head],
                0.1,
                8,
                white.clone(),
            ));
        }
    }
    let collars = [pad_top + 0.3, 6.0, 11.0, 16.0];
    for (k, cy) in collars.into_iter().enumerate() {
        let w = hw(cy);
        parts.push(prim(
            tube(w * SQRT_2 + 0.1, w * SQRT_2 - 0.09, 0.16, 4, white.clone()),
            [x, cy + k as f32 * 0.003, z],
            quat_y(FRAC_PI_4),
        ));
    }
    // A zig-zag of braces up the -Z face.
    let levels = [collars[0], collars[1], collars[2], collars[3], top];
    for (k, pair) in levels.windows(2).enumerate() {
        let (y0, y1) = (pair[0], pair[1]);
        let s = if k % 2 == 0 { 1.0 } else { -1.0 };
        parts.push(strut(
            [x - s * hw(y0), y0, z - hw(y0)],
            [x + s * hw(y1), y1, z - hw(y1)],
            0.05,
            6,
            steel(STEEL_GREY),
        ));
    }
    // The top deck and the turbine on it.
    let deck = prim(
        solid(cuboid_tapered([0.9, 0.4, 0.9], 0.0, steel(STEEL_GREY))),
        [x, top + 0.2, z],
        id_quat(),
    );
    let hub_y = top + 0.78;
    let nacelle = prim(
        solid(cuboid_tapered([0.9, 0.8, 2.2], 0.1, white.clone())),
        [x, hub_y, z],
        id_quat(),
    );
    let hub = [x, hub_y, z - 1.25];
    let mut rotor = vec![
        prim(solid(sphere(0.38, 4, steel(STEEL_GREY))), hub, id_quat()),
        prim(
            solid(cone(0.36, 0.6, 10, white.clone())),
            [x, hub_y, hub[2] - 0.35],
            quat_x(-FRAC_PI_2),
        ),
        prim(
            sphere(0.12, 4, glow([1.0, 0.09, 0.07], 2.6)),
            [x, hub_y + 0.46, z + 0.6],
            id_quat(),
        ),
    ];
    let blade = 4.4_f32;
    for i in 0..3 {
        let a = i as f32 / 3.0 * TAU;
        let c = blade * 0.5 + 0.3;
        rotor.push(prim(
            solid(cuboid_tapered([0.62, blade, 0.14], 0.7, white.clone())),
            [hub[0] - a.sin() * c, hub[1] + a.cos() * c, hub[2] - 0.05],
            quat_z(a),
        ));
    }
    let nacelle = nest(nacelle, rotor);
    parts.push(nest(deck, vec![nacelle]));
    nest(pad, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{
        assert_no_glazing_on_solids, assert_sanitize_stable, has_emissive,
    };

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&TidalStation.build(""), "tidal_station");
    }

    #[test]
    fn nothing_z_fights_and_nothing_floats() {
        let built = TidalStation.build("");
        assert_no_z_fighting(&built, "tidal_station");
        assert_nothing_floats(&built, "tidal_station");
    }

    #[test]
    fn glazing_is_cards_and_the_hall_lights_after_dusk() {
        let built = TidalStation.build("");
        assert_no_glazing_on_solids(&built, "tidal_station");
        assert!(has_emissive(&built));
    }

    /// A landmark's record stays well inside the 100 KiB budget.
    #[test]
    fn stays_well_inside_the_record_budget() {
        let bytes = crate::pds::record_size::serialized_record_bytes(&TidalStation.build(""))
            .expect("serialises");
        assert!(bytes < 60_000, "{bytes} B");
    }

    /// The mast stands tall: its turbine tops 22 m.
    #[test]
    fn the_mast_stands_tall() {
        let top = crate::catalogue::items::measure::solids(&TidalStation.build(""))
            .iter()
            .map(|s| s.bounds.max.y)
            .fold(0.0_f32, f32::max);
        assert!(top > 22.0, "the tallest solid tops {top} m");
    }
}
