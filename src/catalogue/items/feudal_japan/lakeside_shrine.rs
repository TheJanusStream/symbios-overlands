//! Lakeside shrine - a Feudal-Japan secondary at the water's edge. A
//! worship hall (haiden) raised on posts over a raked-gravel court: white
//! plaster between vermilion pillars, a shoji front lit from dusk, an
//! engawa veranda with a lacquered rail and steps, and a deep cypress-bark
//! roof with upswept corners. In front of it on -Z, the water side, a pair
//! of stone lanterns and a great vermilion torii stand over a boardwalk
//! that runs out onto the lake.
//!
//! The settlement turns a shore building's front to the water, so the
//! torii, the boardwalk and the hall's open front are the -Z hero face; the
//! torii stands on the court, within the footing, not in the water.

use crate::catalogue::items::street::lit_from_dusk;
use crate::catalogue::items::util::{
    attach, cone, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, footing, glow, id_quat,
    nest, prim, quat_y, quat_z, solid, sphere, tile, tiles_per_metre, wedge,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignGroundConfig, SovereignMaterialSettings,
    SovereignTextureConfig,
};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    FEUDAL_BAND, GOLD, LACQUER_RED, LANTERN_GLOW, PAPER_CREAM, PLASTER_WHITE, STONE_GREY,
    THATCH_STRAW, TIMBER_DARK, lacquer, paper, plaster, stone, thatch, timber,
};

/// Spacing circle the settlement keeps round the shrine.
const CLEARANCE: f32 = 12.5;

/// The court: its half-extents on X, its back edge and front edge on Z,
/// and the top of its gravel.
const COURT_X: f32 = 8.6;
const COURT_BACK: f32 = 8.8;
const COURT_FRONT: f32 = -8.0;
const COURT_TOP: f32 = 0.3;

/// The hall: its centre on Z, half its width and depth, the veranda's
/// floor and the eave line.
const HALL_Z: f32 = 2.6;
const HALL_X: f32 = 4.5;
const HALL_D: f32 = 3.5;
const FLOOR: f32 = 1.5;
const EAVE: f32 = 4.7;

/// The torii's place on Z, its pillars' half spacing and its height.
const TORII_Z: f32 = -6.9;
const TORII_SPAN: f32 = 2.7;
const TORII_H: f32 = 6.0;

/// The boardwalk: half its width, its far end and its deck top.
const WALK_HALF: f32 = 1.2;
const WALK_END: f32 = -12.0;
const WALK_TOP: f32 = 0.44;

pub struct LakesideShrine;

impl CatalogueEntry for LakesideShrine {
    fn slug(&self) -> &'static str {
        "lakeside_shrine"
    }
    fn name(&self) -> &'static str {
        "Lakeside Shrine"
    }
    fn description(&self) -> &'static str {
        "Vermilion worship hall raised on posts by the water, a great torii over a boardwalk before it."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    /// A shrine to the lake's kami stands at its edge.
    fn site(&self) -> crate::catalogue::SitePreference {
        crate::catalogue::SitePreference::Shore
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::FeudalJapan]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        FEUDAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Raked white gravel - the court the hall stands in.
fn gravel() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.74, 0.72, 0.66]),
        roughness: Fp(0.95),
        uv_scale: tiles_per_metre(tile::GROUND),
        texture: SovereignTextureConfig::Ground(SovereignGroundConfig {
            color_dry: Fp3([0.80, 0.78, 0.72]),
            color_moist: Fp3([0.62, 0.60, 0.55]),
            macro_scale: Fp64(6.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Cypress-bark shingle (hiwadabuki): fine layered bark, a deep red-brown.
fn cypress_bark() -> SovereignMaterialSettings {
    thatch([0.34, 0.20, 0.13])
}

/// Shoji paper with the hall's lamplight behind it from dusk.
fn lit_paper() -> SovereignMaterialSettings {
    lit_from_dusk(SovereignMaterialSettings {
        emission_color: Fp3(LANTERN_GLOW),
        emission_strength: Fp(0.9),
        ..paper(PAPER_CREAM)
    })
}

/// The hall on its posts, the veranda deck as its root: stilts under the
/// deck, white walls between vermilion pillars, the shoji front with its
/// lattice and the sacred rope over it, the rail and the steps, and the
/// roof of cypress bark.
fn hall() -> Generator {
    let (hx, hd, cz) = (HALL_X, HALL_D, HALL_Z);
    let front = cz - hd;
    let (vx, vd) = (hx + 1.5, hd + 1.3);
    let vfront = cz - vd;
    let mut parts = Vec::new();

    // Stilts under the veranda, down into the gravel.
    for x in [-vx + 0.4, -1.9, 1.9, vx - 0.4] {
        for z in [vfront + 0.4, cz + vd - 0.4] {
            parts.push(stilt(x, z));
        }
    }
    for sx in [-1.0_f32, 1.0] {
        parts.push(stilt(sx * (vx - 0.4), cz));
    }
    // A tie beam round the stilts, under the deck.
    for z in [vfront + 0.4, cz + vd - 0.4] {
        parts.push(prim(
            solid(cuboid_tapered(
                [2.0 * vx - 0.5, 0.22, 0.24],
                0.0,
                timber(TIMBER_DARK),
            )),
            [0.0, FLOOR - 0.6, z],
            id_quat(),
        ));
    }

    // The walls, run up into the roof.
    let wall_h = EAVE + 0.15 - FLOOR;
    let walls = prim(
        solid(cuboid_tapered(
            [2.0 * hx, wall_h, 2.0 * hd],
            0.0,
            plaster(PLASTER_WHITE),
        )),
        [0.0, FLOOR + wall_h * 0.5, cz],
        id_quat(),
    );
    let mut on_walls = Vec::new();
    // Vermilion pillars proud of the corners and the front bays.
    for x in [-hx, -1.5, 1.5, hx] {
        on_walls.push(pillar(x, front - 0.05));
    }
    for sx in [-1.0_f32, 1.0] {
        on_walls.push(pillar(sx * hx, cz + hd + 0.05));
    }
    // The shoji front between the pillars, and its kumiko lattice proud
    // of it in dark timber.
    let (sill, head) = (FLOOR + 0.15, EAVE - 0.75);
    on_walls.push(prim(
        cuboid_tapered([2.0 * hx - 0.5, head - sill, 0.06], 0.0, lit_paper()),
        [0.0, (sill + head) * 0.5, front - 0.02],
        id_quat(),
    ));
    for k in 0..9 {
        let x = -hx + 0.6 + k as f32 * (2.0 * hx - 1.2) / 8.0;
        if (x.abs() - 1.5).abs() < 0.25 {
            continue; // a pillar stands there
        }
        on_walls.push(prim(
            cuboid_tapered([0.06, head - sill, 0.06], 0.0, timber(TIMBER_DARK)),
            [x, (sill + head) * 0.5, front - 0.07],
            id_quat(),
        ));
    }
    for y in [sill + 0.9, sill + 1.8] {
        on_walls.push(prim(
            cuboid_tapered([2.0 * hx - 0.5, 0.06, 0.06], 0.0, timber(TIMBER_DARK)),
            [0.0, y, front - 0.11],
            id_quat(),
        ));
    }
    // Head beam over the screens, carrying the sacred straw rope and its
    // zigzag papers.
    on_walls.push(prim(
        solid(cuboid_tapered(
            [2.0 * hx + 0.3, 0.4, 0.3],
            0.0,
            lacquer(LACQUER_RED),
        )),
        [0.0, head + 0.2, front - 0.1],
        id_quat(),
    ));
    on_walls.push(prim(
        cylinder_tapered(0.14, 3.6, 10, 0.0, thatch(THATCH_STRAW)),
        [0.0, head - 0.1, front - 0.3],
        quat_z(std::f32::consts::FRAC_PI_2),
    ));
    for x in [-1.0_f32, 0.0, 1.0] {
        on_walls.push(prim(
            cuboid_tapered([0.14, 0.5, 0.03], 0.0, paper(PLASTER_WHITE)),
            [x, head - 0.45, front - 0.3],
            id_quat(),
        ));
    }
    // A bracket course under the eaves - the shadow line of the tokyo.
    on_walls.push(prim(
        solid(cuboid_tapered(
            [2.0 * hx + 0.7, 0.35, 2.0 * hd + 0.7],
            0.0,
            timber(TIMBER_DARK),
        )),
        [0.0, EAVE - 0.12, cz],
        id_quat(),
    ));
    on_walls.extend(roof(cz, vx + 0.9, vd + 0.9));
    parts.push(nest(walls, on_walls));

    // Rail round the veranda's front, open at the steps.
    for sx in [-1.0_f32, 1.0] {
        let (a, b) = (1.75_f32, vx - 0.15);
        parts.push(prim(
            cuboid_tapered([b - a, 0.1, 0.1], 0.0, lacquer(LACQUER_RED)),
            [sx * (a + b) * 0.5, FLOOR + 0.8, vfront + 0.12],
            id_quat(),
        ));
        for x in [a, (a + b) * 0.5, b] {
            parts.push(prim(
                solid(cuboid_tapered(
                    [0.12, 0.85, 0.12],
                    0.0,
                    lacquer(LACQUER_RED),
                )),
                [sx * x, FLOOR + 0.42, vfront + 0.12],
                id_quat(),
            ));
        }
    }
    // Steps up from the gravel to the veranda, each a little narrower and
    // set a little further under the deck, so no two share a face.
    for k in 1..=3 {
        let top = COURT_TOP + 0.3 * k as f32;
        let run = 0.4 * (4 - k) as f32 + 0.1;
        parts.push(prim(
            solid(cuboid_tapered(
                [3.3 - 0.1 * k as f32, top - COURT_TOP + 0.05, run],
                0.0,
                timber(TIMBER_DARK),
            )),
            [
                0.0,
                (COURT_TOP - 0.05 + top) * 0.5,
                vfront - run * 0.5 + 0.1 + 0.03 * k as f32,
            ],
            id_quat(),
        ));
    }
    // The offering box at the head of the steps.
    parts.push(prim(
        solid(cuboid_tapered([1.4, 0.7, 0.7], 0.0, timber(TIMBER_DARK))),
        [0.0, FLOOR + 0.35, front - 0.9],
        id_quat(),
    ));
    parts.push(super::fx::incense_wisp(
        [0.0, FLOOR + 0.8, front - 0.9],
        0x5A1D_E00A,
    ));

    nest(
        prim(
            solid(cuboid_tapered(
                [2.0 * vx, 0.22, 2.0 * vd],
                0.0,
                timber(TIMBER_DARK),
            )),
            [0.0, FLOOR - 0.11, cz],
            id_quat(),
        ),
        parts,
    )
}

/// One stilt under the veranda, from the gravel up into the deck.
fn stilt(x: f32, z: f32) -> Generator {
    let (bottom, top) = (COURT_TOP - 0.1, FLOOR - 0.12);
    prim(
        solid(cylinder_tapered(
            0.17,
            top - bottom,
            10,
            0.0,
            lacquer(LACQUER_RED),
        )),
        [x, (bottom + top) * 0.5, z],
        id_quat(),
    )
}

/// One vermilion hall pillar from the floor up into the bracket course.
fn pillar(x: f32, z: f32) -> Generator {
    let (bottom, top) = (FLOOR - 0.05, EAVE - 0.1);
    prim(
        solid(cylinder_tapered(
            0.17,
            top - bottom,
            12,
            0.0,
            lacquer(LACQUER_RED),
        )),
        [x, (bottom + top) * 0.5, z],
        id_quat(),
    )
}

/// The hall's roof: a deep eave board, a ridged cap of cypress bark, the
/// four corners swept up, and a ridge with gilded ends.
fn roof(cz: f32, ex: f32, ez: f32) -> Vec<Generator> {
    let board_y = EAVE + 0.15;
    let cap_h = 2.7_f32;
    let mut v = vec![
        prim(
            solid(cuboid_tapered(
                [2.0 * ex, 0.24, 2.0 * ez],
                0.0,
                cypress_bark(),
            )),
            [0.0, board_y, cz],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered_xz(
                [2.0 * ex - 0.6, cap_h, 2.0 * ez - 0.6],
                [0.45, 0.88],
                cypress_bark(),
            )),
            [0.0, board_y + 0.1 + cap_h * 0.5, cz],
            id_quat(),
        ),
    ];
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            v.push(prim(
                wedge([1.7, 0.7, 1.7], cypress_bark()),
                [sx * (ex - 0.55), board_y + 0.2, cz + sz * (ez - 0.55)],
                quat_y((-sx).atan2(-sz)),
            ));
        }
    }
    let ridge_y = board_y + 0.1 + cap_h;
    let ridge_l = (2.0 * ex - 0.6) * 0.55 + 0.6;
    v.push(prim(
        solid(cuboid_tapered(
            [ridge_l, 0.4, 0.45],
            0.0,
            timber(TIMBER_DARK),
        )),
        [0.0, ridge_y + 0.12, cz],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        v.push(prim(
            solid(cuboid_tapered([0.3, 0.6, 0.55], 0.3, lacquer(GOLD))),
            [sx * ridge_l * 0.5, ridge_y + 0.35, cz],
            id_quat(),
        ));
    }
    v
}

/// The great torii: lacquered pillars on stone footings, the tie beam, the
/// double lintel swept up at its ends, and the plaque strut.
fn torii() -> Generator {
    let (span, h, z) = (TORII_SPAN, TORII_H, TORII_Z);
    let base = COURT_TOP;
    let top = base + h;
    let pillar_at = |sx: f32| {
        prim(
            solid(cylinder_tapered(0.34, h, 14, 0.1, lacquer(LACQUER_RED))),
            [sx * span, base + h * 0.5, z],
            id_quat(),
        )
    };
    let mut parts = vec![pillar_at(1.0)];
    for sx in [-1.0_f32, 1.0] {
        // Stone footing collar and a black-lacquered sleeve over it.
        parts.push(prim(
            solid(cylinder_tapered(0.55, 0.45, 12, 0.15, stone(STONE_GREY))),
            [sx * span, base + 0.2, z],
            id_quat(),
        ));
        parts.push(prim(
            solid(cylinder_tapered(
                0.4,
                0.6,
                14,
                0.0,
                lacquer([0.08, 0.07, 0.07]),
            )),
            [sx * span, base + 0.55, z],
            id_quat(),
        ));
    }
    let nuki_y = base + h * 0.72;
    parts.push(prim(
        solid(cuboid_tapered(
            [2.0 * span + 1.2, 0.45, 0.5],
            0.0,
            lacquer(LACQUER_RED),
        )),
        [0.0, nuki_y, z],
        id_quat(),
    ));
    parts.push(prim(
        solid(cuboid_tapered(
            [2.0 * span + 1.4, 0.5, 0.72],
            0.0,
            lacquer(LACQUER_RED),
        )),
        [0.0, top + 0.2, z],
        id_quat(),
    ));
    // Kasagi: black-capped, its ends swept up.
    let kasagi_w = 2.0 * span + 0.8;
    let kasagi_y = top + 0.68;
    let cap = || lacquer([0.10, 0.08, 0.08]);
    parts.push(prim(
        solid(cuboid_tapered([kasagi_w, 0.5, 0.95], 0.0, cap())),
        [0.0, kasagi_y, z],
        id_quat(),
    ));
    let (tip, phi) = (1.4_f32, 0.17_f32);
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered([tip, 0.46, 0.9], 0.2, cap())),
            [
                sx * (kasagi_w * 0.5 + tip * 0.5 * phi.cos() - 0.1),
                kasagi_y + tip * 0.5 * phi.sin(),
                z,
            ],
            quat_z(sx * phi),
        ));
    }
    // Plaque strut between the tie beam and the lintel, the plaque on -Z.
    parts.push(prim(
        solid(cuboid_tapered(
            [0.4, top - nuki_y, 0.26],
            0.0,
            lacquer(LACQUER_RED),
        )),
        [0.0, (nuki_y + top) * 0.5 + 0.05, z],
        id_quat(),
    ));
    parts.push(prim(
        solid(cuboid_tapered(
            [0.8, 0.95, 0.1],
            0.0,
            lacquer([0.08, 0.07, 0.07]),
        )),
        [0.0, (nuki_y + top) * 0.5 + 0.05, z - 0.17],
        id_quat(),
    ));
    nest(pillar_at(-1.0), parts)
}

/// A Kasuga stone lantern: hex base, round shaft, hex platform, the lit
/// fire box, a hex roof and a jewel finial. Its light burns at every hour.
fn lantern(x: f32, z: f32) -> Generator {
    let b = COURT_TOP;
    let parts = vec![
        prim(
            solid(cylinder_tapered(0.17, 1.1, 10, 0.0, stone(STONE_GREY))),
            [x, b + 0.3 + 0.55, z],
            id_quat(),
        ),
        prim(
            solid(cylinder_tapered(0.42, 0.22, 6, 0.1, stone(STONE_GREY))),
            [x, b + 1.45, z],
            id_quat(),
        ),
        prim(
            cylinder_tapered(0.28, 0.5, 6, 0.0, glow(LANTERN_GLOW, 2.2)),
            [x, b + 1.8, z],
            id_quat(),
        ),
        prim(
            solid(cone(0.62, 0.5, 6, stone(STONE_GREY))),
            [x, b + 2.25, z],
            id_quat(),
        ),
        prim(
            sphere(0.13, 4, stone(STONE_GREY)),
            [x, b + 2.55, z],
            id_quat(),
        ),
    ];
    nest(
        prim(
            solid(cylinder_tapered(0.48, 0.4, 6, 0.15, stone(STONE_GREY))),
            [x, b + 0.15, z],
            id_quat(),
        ),
        parts,
    )
}

/// The boardwalk from the court out over the water, on piles.
fn boardwalk() -> Generator {
    let near = TORII_Z + 2.4;
    let len = near - WALK_END;
    let mut parts = Vec::new();
    for z in [COURT_FRONT - 0.8, -10.0, WALK_END + 0.3] {
        for sx in [-1.0_f32, 1.0] {
            let (bottom, top) = (-1.4_f32, WALK_TOP - 0.05);
            parts.push(prim(
                solid(cylinder_tapered(
                    0.13,
                    top - bottom,
                    8,
                    0.0,
                    timber(TIMBER_DARK),
                )),
                [sx * (WALK_HALF - 0.2), (bottom + top) * 0.5, z],
                id_quat(),
            ));
        }
    }
    nest(
        prim(
            solid(cuboid_tapered(
                [2.0 * WALK_HALF, 0.16, len],
                0.0,
                timber(TIMBER_DARK),
            )),
            [0.0, WALK_TOP - 0.08, (near + WALK_END) * 0.5],
            id_quat(),
        ),
        parts,
    )
}

fn build_tree() -> Generator {
    let (depth, cz) = (COURT_BACK - COURT_FRONT, (COURT_BACK + COURT_FRONT) * 0.5);
    let mut root = footing(2.0 * COURT_X, depth, [0.0, cz], CLEARANCE);
    attach(
        &mut root,
        prim(
            solid(cuboid_tapered(
                [2.0 * COURT_X - 0.3, 0.2, depth - 0.3],
                0.0,
                gravel(),
            )),
            [0.0, COURT_TOP - 0.1, cz],
            id_quat(),
        ),
    );
    attach(&mut root, hall());
    attach(&mut root, torii());
    for sx in [-1.0_f32, 1.0] {
        attach(&mut root, lantern(sx * 3.4, -4.4));
    }
    attach(&mut root, boardwalk());
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&LakesideShrine.build(""), "lakeside_shrine");
    }

    #[test]
    fn nothing_floats_or_z_fights() {
        let g = LakesideShrine.build("");
        assert_nothing_floats(&g, "lakeside_shrine");
        assert_no_z_fighting(&g, "lakeside_shrine");
    }

    #[test]
    fn the_record_stays_light() {
        let bytes = serde_json::to_vec(&LakesideShrine.build("")).unwrap().len();
        eprintln!("lakeside_shrine record: {bytes} B");
        assert!(bytes < 60_000, "{bytes} B");
    }
}
