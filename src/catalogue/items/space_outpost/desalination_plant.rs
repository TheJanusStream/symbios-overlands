//! Desalination plant - a Space-Outpost secondary that leads a settlement on
//! its shore (#1796). The colony's water: three ribbed storage tanks behind a
//! vaulted pump hall, intake pipes running out over the deck's -Z edge down
//! into the water, and a slender control tower whose lit cab, catwalk,
//! antennae and red beacon stand 24 m over the pad.
//!
//! Primitive-built on a footing; each part is a subtree rooted where it
//! stands on the deck, so the ruin pass leans a tank or the tower whole.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::catalogue::items::util::{
    attach, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, prim, prim_scaled,
    quat_x, quat_z, solid, sphere, torus, tube, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BEACON_RED, HAZARD_YELLOW, HULL_PANEL, HULL_WHITE, PAD_GREY, STATUS_GREEN, STEEL_DARK,
    VIEWPORT_LIT, concrete, dome_ribs, fx, hull, painted, pressure_hatch, steel,
};

/// The spacing circle the settlement keeps round the plant: the deck's
/// half diagonal.
const CLEARANCE: f32 = 14.5;
/// The concrete deck: its width along the shore, its depth inland and the
/// height of its top.
const DECK_W: f32 = 24.0;
const DECK_D: f32 = 16.0;
const DECK_TOP: f32 = 0.4;
/// The deck's water-side (-Z) edge.
const FRONT: f32 = -DECK_D * 0.5;

pub struct DesalinationPlant;

impl CatalogueEntry for DesalinationPlant {
    fn slug(&self) -> &'static str {
        "desalination_plant"
    }
    fn name(&self) -> &'static str {
        "Desalination Plant"
    }
    fn description(&self) -> &'static str {
        "Shore water works: domed storage tanks, a pump hall, intake pipes and a lit control tower."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SpaceOutpost]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::OUTPOST_BAND
    }
    /// It draws the sea, so it stands where the land meets it.
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
    attach(&mut root, deck());
    for (x, z, r, h) in [
        (-7.5_f32, 4.2_f32, 3.2_f32, 5.0_f32),
        (-0.8, 4.2, 3.2, 5.0),
        (5.2, 5.0, 2.2, 3.6),
    ] {
        attach(&mut root, tank(x, z, r, h));
    }
    attach(&mut root, pump_hall());
    for x in [-9.6_f32, -8.5, 1.95] {
        attach(&mut root, intake(x));
    }
    attach(&mut root, control_tower());
    // Signature life: the intake pumps stroking under the deck.
    root.audio = fx::hydro_pump();
    root
}

/// The concrete deck the works stand on, with a hazard stripe along its
/// water edge.
fn deck() -> Generator {
    let slab = prim(
        solid(cuboid_tapered(
            [DECK_W, DECK_TOP, DECK_D],
            0.0,
            concrete(PAD_GREY),
        )),
        [0.0, DECK_TOP * 0.5, 0.0],
        id_quat(),
    );
    let stripe = prim(
        cuboid_tapered([DECK_W - 0.4, 0.06, 0.3], 0.0, painted(HAZARD_YELLOW)),
        [0.0, DECK_TOP + 0.01, FRONT + 0.35],
        id_quat(),
    );
    nest(slab, vec![stripe])
}

/// A storage tank at `(x, z)`: a white hull drum `h` tall of radius `r`,
/// a hazard band round its waist, and a glazed-white dome caged in steel
/// ribs with a hatch at its crown.
fn tank(x: f32, z: f32, r: f32, h: f32) -> Generator {
    let base = DECK_TOP - 0.02;
    let top = base + h;
    let drum = prim(
        solid(cylinder_tapered(r, h, 24, 0.0, hull(HULL_WHITE))),
        [x, base + h * 0.5, z],
        id_quat(),
    );
    let mut parts = vec![
        prim(
            tube(
                r + 0.04,
                (r + 0.04) * 0.95,
                0.45,
                24,
                painted(HAZARD_YELLOW),
            ),
            [x, base + h * 0.62, z],
            id_quat(),
        ),
        prim(
            solid(with_cut(
                sphere(r - 0.06, 6, hull(HULL_PANEL)),
                [0.0, 1.0],
                [0.5, 1.0],
                0.0,
            )),
            [x, top, z],
            id_quat(),
        ),
        prim(
            torus(0.1, r + 0.02, steel(STEEL_DARK)),
            [x, top, z],
            id_quat(),
        ),
        prim(
            solid(cylinder_tapered(0.55, 0.3, 12, 0.2, steel(STEEL_DARK))),
            [x, top + r - 0.12, z],
            id_quat(),
        ),
    ];
    parts.extend(dome_ribs([x, top, z], r, 4, steel(STEEL_DARK)));
    nest(drum, parts)
}

/// The pump hall: a long white module under a low vault, its -Z face
/// carrying a pressure hatch, a lit window band and a hazard-framed cargo
/// door, and short feed pipes from its back into the two big tanks.
fn pump_hall() -> Generator {
    let (x0, x1) = (-10.5_f32, 2.5_f32);
    let (z0, z1) = (-5.5_f32, 0.3_f32);
    let (w, d) = (x1 - x0, z1 - z0);
    let cx = (x0 + x1) * 0.5;
    let cz = (z0 + z1) * 0.5;
    let wall_h = 4.2_f32;
    let wall_top = DECK_TOP + wall_h;
    let body = prim(
        solid(cuboid_tapered([w, wall_h + 0.02, d], 0.0, hull(HULL_PANEL))),
        [cx, DECK_TOP - 0.02 + (wall_h + 0.02) * 0.5, cz],
        id_quat(),
    );
    let mut parts = vec![
        // The low vault: a cylinder laid along X, squashed to a shallow arch;
        // its lower half is buried in the walls, its ends and eaves oversail.
        prim_scaled(
            solid(cylinder_tapered(
                d * 0.5 + 0.08,
                w + 0.3,
                24,
                0.0,
                hull(HULL_WHITE),
            )),
            [cx, wall_top, cz],
            quat_z(FRAC_PI_2),
            [0.42, 1.0, 1.0],
        ),
    ];
    // Two roof vents on the vault's crown.
    for vx in [cx - 3.0, cx + 2.4] {
        parts.push(prim(
            solid(cylinder_tapered(0.45, 0.9, 12, 0.15, steel(STEEL_DARK))),
            [vx, wall_top + d * 0.21 + 0.3, cz],
            id_quat(),
        ));
    }
    // Personnel pressure hatch with a green status light either side.
    let hatch_at = [-6.4_f32, DECK_TOP + 1.25, z0 - 0.05];
    parts.extend(hatch(
        hatch_at,
        0.95,
        hull(HULL_WHITE),
        steel(STEEL_DARK),
        crate::catalogue::items::street::lit_from_dusk(glow(VIEWPORT_LIT, 2.0)),
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            cuboid_tapered([0.2, 0.2, 0.1], 0.0, glow(STATUS_GREEN, 2.4)),
            [hatch_at[0] + sx * 1.35, hatch_at[1] + 0.7, z0 - 0.04],
            id_quat(),
        ));
    }
    // The control-room window band: three lit panes on steel sills.
    for wx in [-4.2_f32, -3.1, -2.0] {
        parts.push(prim(
            cuboid_tapered(
                [0.9, 0.8, 0.08],
                0.0,
                crate::catalogue::items::street::lit_from_dusk(glow(VIEWPORT_LIT, 1.4)),
            ),
            [wx, DECK_TOP + 2.9, z0 - 0.02],
            id_quat(),
        ));
        parts.push(prim(
            cuboid_tapered([1.0, 0.1, 0.2], 0.0, steel(STEEL_DARK)),
            [wx, DECK_TOP + 2.45, z0 - 0.08],
            id_quat(),
        ));
    }
    // Cargo door in a hazard-yellow frame.
    let door_x = 0.0_f32;
    let door_h = 3.0_f32;
    parts.push(prim(
        cuboid_tapered([2.4, door_h, 0.1], 0.0, steel(STEEL_DARK)),
        [door_x, DECK_TOP + door_h * 0.5, z0 - 0.03],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            cuboid_tapered([0.22, door_h + 0.2, 0.16], 0.0, painted(HAZARD_YELLOW)),
            [
                door_x + sx * 1.3,
                DECK_TOP + (door_h + 0.2) * 0.5,
                z0 - 0.07,
            ],
            id_quat(),
        ));
    }
    parts.push(prim(
        cuboid_tapered([2.9, 0.22, 0.2], 0.0, painted(HAZARD_YELLOW)),
        [door_x, DECK_TOP + door_h + 0.12, z0 - 0.07],
        id_quat(),
    ));
    // Feed pipes from the hall's back wall into the two big tanks.
    for tx in [-7.5_f32, -0.8] {
        parts.push(prim(
            solid(cylinder_tapered(0.3, 1.5, 12, 0.0, steel(STEEL_DARK))),
            [tx, DECK_TOP + 2.6, z1 + 0.6],
            quat_x(FRAC_PI_2),
        ));
    }
    nest(body, parts)
}

/// One intake line at `x`: a pipe on a steel saddle running from the hall's
/// front wall out to the deck's water edge, where it turns down over the
/// edge into the water, flanged at both ends.
fn intake(x: f32) -> Generator {
    let r = 0.38_f32;
    let y = DECK_TOP + 0.62;
    let wall = -5.5_f32;
    let elbow = FRONT - 0.3;
    let saddle = prim(
        solid(cuboid_tapered([0.9, 0.3, 0.45], 0.0, steel(STEEL_DARK))),
        [x, DECK_TOP + 0.13, -7.0],
        id_quat(),
    );
    let run = wall + 0.1 - elbow;
    let drop_bottom = -1.6_f32;
    let parts = vec![
        prim(
            solid(cylinder_tapered(r, run, 14, 0.0, steel(STEEL_DARK))),
            [x, y, elbow + run * 0.5],
            quat_x(FRAC_PI_2),
        ),
        prim(
            sphere(r * 1.18, 5, steel(STEEL_DARK)),
            [x, y, elbow],
            id_quat(),
        ),
        prim(
            solid(cylinder_tapered(
                r,
                y - drop_bottom,
                14,
                0.0,
                steel(STEEL_DARK),
            )),
            [x, (y + drop_bottom) * 0.5, elbow],
            id_quat(),
        ),
        // A bolted flange where the line meets the wall, and one on the drop.
        prim(
            torus(0.09, r + 0.04, hull(HULL_WHITE)),
            [x, y, wall - 0.25],
            quat_x(FRAC_PI_2),
        ),
        prim(
            torus(0.09, r + 0.04, hull(HULL_WHITE)),
            [x, DECK_TOP - 0.25, elbow],
            id_quat(),
        ),
    ];
    nest(saddle, parts)
}

/// The control tower at the plant's water corner: a white base drum, a
/// slender ringed shaft, a lit control cab on a railed catwalk, and a roof
/// carrying a comms dish, two whip antennae and a mast with a red beacon.
fn control_tower() -> Generator {
    let (x, z) = (8.6_f32, -3.0_f32);
    let base_h = 3.2_f32;
    let shaft_r = 1.0_f32;
    let shaft_bottom = DECK_TOP + base_h;
    let cab_bottom = 17.0_f32;
    let cab_h = 3.0_f32;
    let cab_r = 2.6_f32;
    let cab_top = cab_bottom + cab_h;
    let roof_top = cab_top + 0.25;

    // The roof and what stands on it.
    let roof = prim(
        solid(cylinder_tapered(
            cab_r + 0.25,
            0.25,
            24,
            0.0,
            steel(STEEL_DARK),
        )),
        [x, cab_top + 0.125, z],
        id_quat(),
    );
    let mast_h = 4.2_f32;
    let mut roof_parts = vec![
        prim(
            solid(cylinder_tapered(0.13, mast_h, 8, 0.3, steel(STEEL_DARK))),
            [x, roof_top + mast_h * 0.5 - 0.02, z],
            id_quat(),
        ),
        prim(
            sphere(0.3, 4, glow(BEACON_RED, 2.6)),
            [x, roof_top + mast_h + 0.1, z],
            id_quat(),
        ),
    ];
    for sx in [-1.0_f32, 1.0] {
        roof_parts.push(prim(
            cylinder_tapered(0.05, 2.4, 6, 0.4, steel(STEEL_DARK)),
            [x + sx * 1.6, roof_top + 1.18, z + 1.0],
            id_quat(),
        ));
    }
    // A small comms dish on a post at the roof's -Z edge, tilted up and out.
    let dish_c = [x - 1.2, roof_top + 1.25, z - 1.2];
    roof_parts.push(prim(
        solid(cylinder_tapered(0.12, 1.0, 8, 0.0, steel(STEEL_DARK))),
        [dish_c[0], roof_top + 0.48, dish_c[2] + 0.2],
        id_quat(),
    ));
    roof_parts.push(prim_scaled(
        solid(with_cut(
            sphere(0.9, 6, hull(HULL_WHITE)),
            [0.0, 1.0],
            [0.0, 0.5],
            0.0,
        )),
        dish_c,
        quat_x(-0.6),
        [1.0, 0.5, 1.0],
    ));
    let roof = nest(roof, roof_parts);

    // The cab: its lit window band behind steel mullions.
    let cab = prim(
        solid(cylinder_tapered(cab_r, cab_h, 24, 0.0, hull(HULL_WHITE))),
        [x, cab_bottom + cab_h * 0.5, z],
        id_quat(),
    );
    let band_y = cab_bottom + 1.6;
    let mut cab_parts = vec![
        prim(
            cylinder_tapered(
                cab_r + 0.04,
                1.3,
                24,
                0.0,
                crate::catalogue::items::street::lit_from_dusk(glow(VIEWPORT_LIT, 1.6)),
            ),
            [x, band_y, z],
            id_quat(),
        ),
        roof,
    ];
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU + PI / 6.0;
        cab_parts.push(prim(
            cuboid_tapered([0.14, 1.42, 0.14], 0.0, steel(STEEL_DARK)),
            [
                x + a.cos() * (cab_r + 0.06),
                band_y,
                z + a.sin() * (cab_r + 0.06),
            ],
            id_quat(),
        ));
    }
    let cab = nest(cab, cab_parts);

    // The catwalk the cab stands on, railed, over a flared collar.
    let catwalk = prim(
        solid(cylinder_tapered(
            cab_r + 0.6,
            0.14,
            24,
            0.0,
            steel(STEEL_DARK),
        )),
        [x, cab_bottom + 0.05, z],
        id_quat(),
    );
    let mut walk_parts = vec![
        cab,
        prim(
            torus(0.05, cab_r + 0.52, steel(STEEL_DARK)),
            [x, cab_bottom + 1.12, z],
            id_quat(),
        ),
        // The flared collar under the catwalk: a frustum stood on its head.
        prim(
            solid(cylinder_tapered(
                cab_r - 0.2,
                1.4,
                20,
                0.6,
                hull(HULL_PANEL),
            )),
            [x, cab_bottom - 0.66, z],
            quat_x(PI),
        ),
    ];
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU;
        walk_parts.push(prim(
            cuboid_tapered([0.08, 1.06, 0.08], 0.0, steel(STEEL_DARK)),
            [
                x + a.cos() * (cab_r + 0.52),
                cab_bottom + 0.6,
                z + a.sin() * (cab_r + 0.52),
            ],
            id_quat(),
        ));
    }
    let catwalk = nest(catwalk, walk_parts);

    // The shaft, ringed, with a cable riser up its -Z face.
    let shaft_h = cab_bottom - shaft_bottom + 0.1;
    let shaft = prim(
        solid(cylinder_tapered(
            shaft_r,
            shaft_h,
            16,
            0.08,
            hull(HULL_WHITE),
        )),
        [x, shaft_bottom - 0.05 + shaft_h * 0.5, z],
        id_quat(),
    );
    let mut shaft_parts = vec![catwalk];
    for (k, ry) in [7.5_f32, 11.5, 15.0].into_iter().enumerate() {
        // The shaft tapers, so each ring hugs the radius at its height.
        let f = (ry - shaft_bottom) / shaft_h;
        let rr = shaft_r * (1.0 - 0.08 * f) + 0.02;
        shaft_parts.push(prim(
            torus(0.12, rr, steel(STEEL_DARK)),
            [x, ry + k as f32 * 0.01, z],
            id_quat(),
        ));
    }
    shaft_parts.push(prim(
        cylinder_tapered(0.11, shaft_h - 0.6, 8, 0.0, steel(STEEL_DARK)),
        [x, shaft_bottom + shaft_h * 0.5, z - shaft_r * 0.96],
        id_quat(),
    ));
    let shaft = nest(shaft, shaft_parts);

    // The base drum, with its own hatch on the -Z face.
    let base = prim(
        solid(cylinder_tapered(
            2.3,
            base_h + 0.02,
            20,
            0.12,
            hull(HULL_PANEL),
        )),
        [x, DECK_TOP - 0.02 + (base_h + 0.02) * 0.5, z],
        id_quat(),
    );
    let mut base_parts = vec![shaft];
    base_parts.extend(hatch(
        [x, DECK_TOP + 1.2, z - 2.25],
        0.8,
        hull(HULL_WHITE),
        steel(STEEL_DARK),
        glow(STATUS_GREEN, 2.2),
    ));
    nest(base, base_parts)
}

/// The kit's [`pressure_hatch`] on a -Z wall, with a steel boss behind its
/// port: the port and its handle stand a few centimetres clear of the door
/// plate, and the boss is what holds them to it.
fn hatch(
    center: [f32; 3],
    radius: f32,
    door: crate::pds::SovereignMaterialSettings,
    rim: crate::pds::SovereignMaterialSettings,
    port: crate::pds::SovereignMaterialSettings,
) -> Vec<Generator> {
    let [x, y, z] = center;
    let mut parts = pressure_hatch(center, radius, -1.0, door, rim.clone(), port);
    parts.push(prim(
        cylinder_tapered(radius * 0.37, 0.1, 14, 0.0, rim),
        [x, y, z - 0.08],
        quat_x(FRAC_PI_2),
    ));
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_sanitize_stable, has_emissive};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&DesalinationPlant.build(""), "desalination_plant");
    }

    #[test]
    fn nothing_z_fights_and_nothing_floats() {
        let built = DesalinationPlant.build("");
        assert_no_z_fighting(&built, "desalination_plant");
        assert_nothing_floats(&built, "desalination_plant");
    }

    /// The beacon, the status lights and the cab keep the plant lit.
    #[test]
    fn keeps_its_lights() {
        assert!(has_emissive(&DesalinationPlant.build("")));
    }

    /// A landmark's record stays well inside the 100 KiB budget.
    #[test]
    fn stays_well_inside_the_record_budget() {
        let bytes = crate::pds::record_size::serialized_record_bytes(&DesalinationPlant.build(""))
            .expect("serialises");
        assert!(bytes < 60_000, "{bytes} B");
    }

    /// The control tower stands tall: its beacon tops 24 m.
    #[test]
    fn the_tower_stands_tall() {
        let top = crate::catalogue::items::measure::solids(&DesalinationPlant.build(""))
            .iter()
            .map(|s| s.bounds.max.y)
            .fold(0.0_f32, f32::max);
        assert!(top > 22.0, "the tallest solid tops {top} m");
    }
}
