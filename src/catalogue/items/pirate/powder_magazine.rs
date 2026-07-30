//! Powder Magazine — where the harbour keeps what it must not lose.
//!
//! A low vaulted stone store behind a blast traverse: a barrel roof under a
//! turfed cap, an iron-bound door standing open on a lit floor of powder
//! casks, splayed vents in the flanks, a copper conductor down one corner,
//! and an earth bank across the approach so that if it goes, it goes upward.
//!
//! # Why it has no windows, and why that is the interesting part
//!
//! A magazine is the building whose entire design brief is *keep light and
//! spark out*. It has no glazing at all — vents, splayed so no direct line
//! reaches the powder, and one door. That makes it the third entry in this
//! kit to arrive at #972 lesson 24's answer from a different direction: the
//! battery's openings are gun ports, the boardwalk's is a serving hatch, and
//! this one's are vents. The kit now has a `Window` card and the reflex to
//! reach for it is exactly what the ledger keeps catching, so the guard here
//! states the prohibition rather than assuming it.
//!
//! # The traverse is the silhouette
//!
//! Without the blast bank this is a shed. The traverse — an earth-and-stone
//! wall standing across the door at a stand-off, so a viewer sees the store
//! *through* a gap — is what makes the prop read as dangerous rather than as
//! storage, and it does the compositional work too: a second mass at a
//! different depth is what gives a low building a silhouette at settlement
//! distance.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    attach, bonded_siding, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, face_uv_offset,
    footing, id_quat, lit_interior, nest, prim, prim_scaled, quat_x, quat_z, solid, sphere, strut,
    torus, with_cut, with_face,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::pds::generator::FaceKey;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRONZE_FITTING, DECK_HOLY, HULL_OAK, IRON_BLACK, PORT_BAND, ROPE_HEMP, SHINGLE_GREY,
    STONE_LIME, STONE_QUAY, WHARF_GREY, ashlar, board, bronze, cobbles, fx, hemp, iron, lantern,
    shingle,
};

/// Cobbled apron — the sub-root every footprint guard measures against.
const APRON: [f32; 3] = [16.0, 0.28, 14.0];
const GROUND: f32 = APRON[1];

/// The store's plinth, and its top — the magazine floor.
const PLINTH: [f32; 3] = [7.8, 0.44, 6.4];
const FLOOR: f32 = GROUND + PLINTH[1];

/// The walled store: width, height to the springing of the vault, depth.
const WALL: [f32; 3] = [7.0, 2.5, 5.6];
/// Springing line — where the wall stops and the barrel vault begins.
const SPRING: f32 = FLOOR + WALL[1];
/// Rise of the vault above the springing.
const VAULT_RISE: f32 = 1.5;
/// Crown of the vault.
const CROWN: f32 = SPRING + VAULT_RISE;

/// Hero plane — the approach elevation.
const FRONT_Z: f32 = -WALL[2] * 0.5;

/// The door: clear width and height.
const DOOR_W: f32 = 1.5;
const DOOR_H: f32 = 2.05;
/// How far the doorway is recessed, so the opening reads as a hole in a thick
/// wall rather than as a panel on a face.
const REVEAL: f32 = 0.34;

/// How far the interior's fit-out is held forward of the rear wall, and how
/// far every interior surface stays behind the wall face it meets.
const ROOM_BACK: f32 = FRONT_Z + 3.1;
const FLOOR_INSET: f32 = 0.06;

/// The blast traverse: its stand-off from the door, and its own extent.
///
/// The stand-off is the whole point — close enough to block a direct line to
/// the door, far enough that a viewer sees the lit store past its end. Flush
/// against the building it would just be a thicker wall.
const TRAVERSE_OFF: f32 = 3.4;
const TRAVERSE: [f32; 3] = [6.2, 2.3, 1.1];

pub struct PowderMagazine;

impl CatalogueEntry for PowderMagazine {
    fn slug(&self) -> &'static str {
        "powder_magazine"
    }
    fn name(&self) -> &'static str {
        "Powder Magazine"
    }
    fn description(&self) -> &'static str {
        "A vaulted stone magazine behind a blast traverse, its iron-bound door open on a lit floor \
         of powder casks."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Pirate]
    }
    fn prosperity_band(&self) -> ProsperityBand {
        PORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Coursed ashlar in the shared world course frame, on the face that will be
/// looked at (#972 lessons 2e and 18 — the centre is one expression, passed
/// to both the material and the placement).
fn coursed(center: [f32; 3], face: FaceKey, seed: u32) -> crate::pds::SovereignMaterialSettings {
    bonded_siding(ashlar(STONE_LIME, seed), face, center)
}

/// The approach elevation: piers either side of the recessed doorway, the
/// relieving arch over it, and the door itself standing open.
fn approach() -> Vec<Generator> {
    let z = FRONT_Z + WALL[2] * 0.25;
    let d = WALL[2] * 0.5;
    let mut out = Vec::new();

    // Piers flanking the doorway.
    for sx in [-1.0_f32, 1.0] {
        let w = (WALL[0] - DOOR_W) * 0.5;
        let cx = sx * (DOOR_W + w) * 0.5;
        let c = [cx, FLOOR + WALL[1] * 0.5, z];
        out.push(prim(
            solid(cuboid_tapered(
                [w, WALL[1], d],
                0.0,
                coursed(c, FaceKey::SideNz, 0xD1),
            )),
            c,
            id_quat(),
        ));
    }
    // Spandrel over the door head.
    let sp_c = [0.0, (FLOOR + DOOR_H + SPRING) * 0.5, z];
    out.push(prim(
        solid(cuboid_tapered(
            [DOOR_W, SPRING - FLOOR - DOOR_H, d],
            0.0,
            coursed(sp_c, FaceKey::SideNz, 0xD2),
        )),
        sp_c,
        id_quat(),
    ));

    // Relieving arch in voussoirs over the opening. Leaf prims, each carrying
    // its own turn, so nothing offset rides a rotation (#972 lesson 22).
    let arch_r = DOOR_W * 0.5 + 0.22;
    const VOUSSOIRS: usize = 5;
    for i in 0..VOUSSOIRS {
        let t = (i as f32 + 0.5) / VOUSSOIRS as f32;
        let a = std::f32::consts::PI * (0.14 + 0.72 * t);
        out.push(prim(
            solid(cuboid_tapered(
                [0.3, 0.44, 0.46],
                0.0,
                ashlar(STONE_LIME, 0xD3 + i as u32),
            )),
            [
                -arch_r * a.cos(),
                FLOOR + DOOR_H - 0.06 + arch_r * a.sin() * 0.3,
                FRONT_Z + 0.22,
            ],
            quat_z(a - FRAC_PI_2),
        ));
    }

    // The door, standing OPEN against its jamb — one direction vector for
    // both the centre and the turn (#972 lesson 21's corollary; the tavern's
    // leaf shipped with an identity rotation on an arc-placed centre).
    let leaf_w = DOOR_W * 0.9;
    let swing = 1.05_f32;
    // Hung at the back of the REVEAL, so the opening reads as a hole through
    // a thick wall rather than as a panel on its face.
    let hinge = [-DOOR_W * 0.5, FLOOR + DOOR_H * 0.5, FRONT_Z + REVEAL];
    let arm = [swing.cos(), -swing.sin()];
    out.push(prim(
        solid(cuboid_tapered(
            [leaf_w, DOOR_H - 0.08, 0.11],
            0.0,
            board(WHARF_GREY),
        )),
        [
            hinge[0] + leaf_w * 0.5 * arm[0],
            hinge[1],
            hinge[2] + leaf_w * 0.5 * arm[1],
        ],
        crate::catalogue::items::util::quat_y(swing),
    ));
    // Iron straps across the leaf, riding the same turn at the same centre so
    // they cannot part company with it.
    for dy in [-0.55_f32, 0.55] {
        out.push(prim(
            solid(cuboid_tapered(
                [leaf_w * 0.96, 0.13, 0.13],
                0.0,
                iron(IRON_BLACK, 0xD8),
            )),
            [
                hinge[0] + leaf_w * 0.5 * arm[0],
                hinge[1] + dy,
                hinge[2] + leaf_w * 0.5 * arm[1],
            ],
            crate::catalogue::items::util::quat_y(swing),
        ));
    }
    // Threshold stone, and two steps down to the apron with equal risers.
    out.push(prim(
        solid(cuboid_tapered(
            [DOOR_W + 0.7, 0.14, 0.7],
            0.0,
            ashlar(STONE_LIME, 0xD9),
        )),
        [0.0, FLOOR + 0.07, FRONT_Z - 0.18],
        id_quat(),
    ));
    let rise = FLOOR - GROUND;
    let treads = (rise / 0.19).round().max(2.0) as usize;
    let riser = rise / treads as f32;
    for i in 0..treads {
        let h = riser * (i + 1) as f32;
        out.push(prim(
            solid(cuboid_tapered(
                [DOOR_W + 1.0, h, 0.32],
                0.0,
                cobbles(STONE_QUAY, 0xDA + i as u32),
            )),
            [
                0.0,
                GROUND + h * 0.5,
                FRONT_Z - 0.5 - (treads - 1 - i) as f32 * 0.32 - 0.16,
            ],
            id_quat(),
        ));
    }
    out
}

/// The lit floor of casks, seen through the open door.
///
/// A magazine's whole subject is what is inside it, and the door is the only
/// aperture — so the fit-out is laid out down the SIGHTLINE from the doorway
/// rather than round the walls. Anything against a flank is invisible.
fn store() -> Vec<Generator> {
    let mut out = vec![
        // Floor and rear lining, both held back from the wall faces they meet.
        prim(
            solid(cuboid_tapered(
                [WALL[0] - 1.0, 0.1, ROOM_BACK - FRONT_Z - FLOOR_INSET],
                0.0,
                lit_interior([0.28, 0.23, 0.18], 0.15),
            )),
            [0.0, FLOOR + 0.05, (FRONT_Z + FLOOR_INSET + ROOM_BACK) * 0.5],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(
                [WALL[0] - 1.0, WALL[1] - 0.3, 0.12],
                0.0,
                lit_interior([0.44, 0.31, 0.19], 0.34),
            )),
            [0.0, FLOOR + (WALL[1] - 0.3) * 0.5, ROOM_BACK - FLOOR_INSET],
            id_quat(),
        ),
    ];
    // Powder casks in two courses on a stillage, straight down the sightline
    // from the door. Copper-hooped, not iron: iron sparks, which is the one
    // fact about a magazine worth building in.
    for (course, y) in [(0_usize, 0.44_f32), (1, 1.24)] {
        for dx in [-0.55_f32, 0.35] {
            let x = dx + course as f32 * 0.12;
            out.push(prim(
                solid(cylinder_tapered(0.36, 0.82, 12, -0.1, board(HULL_OAK))),
                [x, FLOOR + y, ROOM_BACK - 0.85],
                quat_z(FRAC_PI_2),
            ));
            out.push(prim(
                torus(0.04, 0.37, bronze(BRONZE_FITTING, 0xE0)),
                [x, FLOOR + y, ROOM_BACK - 0.85],
                quat_z(FRAC_PI_2),
            ));
        }
    }
    // A shifting-board and a scoop, and the lantern that lights the room.
    out.push(prim(
        solid(cuboid_tapered([1.9, 0.08, 0.5], 0.0, board(DECK_HOLY))),
        [0.5, FLOOR + 0.72, FRONT_Z + 1.2],
        id_quat(),
    ));
    // Hung LOW and forward, inside the cone the doorway admits: from the
    // apron the eye enters a 2 m opening almost level, so a lantern up at the
    // vault crown is behind the head and invisible (#972 lesson 10, in the
    // corrected cone form the battery arrived at).
    out.push(lantern([1.55, FLOOR + 1.25, FRONT_Z + 1.5], 0.5, 0xE1));
    out
}

/// Splayed vents in the flanks — a magazine's only other opening, and
/// deliberately not glazed.
///
/// Splayed means the outer mouth and the inner one are offset, so no straight
/// line reaches the powder. That is the actual historical device, and it is
/// also what keeps them reading as vents rather than as small windows
/// somebody forgot to fill.
fn vents() -> Vec<Generator> {
    let mut out = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        for dz in [-1.2_f32, 1.2] {
            let x = sx * WALL[0] * 0.5;
            let y = FLOOR + WALL[1] * 0.62;
            // Outer mouth: a recessed box in the flank, dark inside.
            out.push(prim(
                solid(cuboid_tapered(
                    [0.34, 0.4, 0.5],
                    0.0,
                    lit_interior([0.14, 0.12, 0.1], 0.05),
                )),
                [x - sx * 0.16, y, dz],
                id_quat(),
            ));
            // Sill and lintel stones framing it, standing proud.
            for dy in [-0.28_f32, 0.28] {
                out.push(prim(
                    solid(cuboid_tapered(
                        [0.2, 0.14, 0.72],
                        0.0,
                        ashlar(STONE_LIME, 0xE2),
                    )),
                    [x + sx * 0.06, y + dy, dz],
                    id_quat(),
                ));
            }
            // A bronze grille bar across it — bronze, for the same
            // no-spark reason the cask hoops are.
            out.push(prim(
                solid(cylinder_tapered(
                    0.035,
                    0.44,
                    6,
                    0.0,
                    bronze(BRONZE_FITTING, 0xE3),
                )),
                [x + sx * 0.02, y, dz],
                quat_x(FRAC_PI_2),
            ));
        }
    }
    out
}

/// The blast traverse: an earth bank faced in stone, standing across the
/// approach at a stand-off.
fn traverse() -> Generator {
    let z = FRONT_Z - TRAVERSE_OFF;
    let c = [0.0, GROUND + TRAVERSE[1] * 0.5, z];
    nest(
        prim(
            // Battered: a blast bank is wider at the foot, and the taper is
            // on Z alone — `cuboid_tapered` pinches BOTH axes and would round
            // the whole bank away on four sides (the barn's shipped fault).
            solid(cuboid_tapered_xz(
                TRAVERSE,
                [0.0, 0.3],
                coursed(c, FaceKey::SideNz, 0xE4),
            )),
            c,
            id_quat(),
        ),
        vec![
            // Turf cap over it, and a coping course under that.
            prim(
                solid(cuboid_tapered(
                    [TRAVERSE[0] + 0.2, 0.16, TRAVERSE[2] * 0.78],
                    0.0,
                    ashlar(STONE_LIME, 0xE5),
                )),
                [0.0, GROUND + TRAVERSE[1] + 0.08, z],
                id_quat(),
            ),
            prim(
                solid(cuboid_tapered_xz(
                    [TRAVERSE[0] + 0.1, 0.3, TRAVERSE[2] * 0.7],
                    [0.06, 0.4],
                    shingle(SHINGLE_GREY),
                )),
                [0.0, GROUND + TRAVERSE[1] + 0.31, z],
                id_quat(),
            ),
        ],
    )
}

fn build_tree() -> Generator {
    let apron_c = [0.0, GROUND * 0.5, 0.0];
    let mut paving = cobbles(STONE_QUAY, 0xD0);
    paving.uv_offset = face_uv_offset(FaceKey::Top, apron_c);

    let plinth_c = [0.0, GROUND + PLINTH[1] * 0.5, 0.0];
    let mut on_plinth = Vec::new();
    on_plinth.extend(approach());
    on_plinth.extend(store());
    on_plinth.extend(vents());

    // Flanks and rear as single slabs — the vents are recesses in them, not
    // holes through them, so a punched grid would cost prims to say nothing.
    for sx in [-1.0_f32, 1.0] {
        let c = [sx * (WALL[0] * 0.5 - 0.25), FLOOR + WALL[1] * 0.5, 0.35];
        on_plinth.push(prim(
            solid(cuboid_tapered(
                [0.5, WALL[1], WALL[2] - 0.5],
                0.0,
                coursed(
                    c,
                    if sx < 0.0 {
                        FaceKey::SideNx
                    } else {
                        FaceKey::SidePx
                    },
                    0xE6,
                ),
            )),
            c,
            id_quat(),
        ));
    }
    let back_c = [0.0, FLOOR + WALL[1] * 0.5, WALL[2] * 0.5 - 0.25];
    on_plinth.push(prim(
        solid(cuboid_tapered(
            [WALL[0], WALL[1], 0.5],
            0.0,
            coursed(back_c, FaceKey::SidePz, 0xE7),
        )),
        back_c,
        id_quat(),
    ));

    // The barrel vault: a half-cylinder laid along the building, which is the
    // one prim that says "vault" — `path_cut` keeps the upper half of the
    // sweep and `quat_x` lays its axis along Z.
    // A half-cylinder of the building's own half-span, SQUASHED to the rise.
    // A semicircular barrel over a 7 m span would stand 3.5 m above the
    // springing and the building would not be low any more; scaling the
    // section gives the segmental profile a magazine actually wears. The
    // squash goes on local Z because `quat_x(FRAC_PI_2)` sends the cylinder's
    // axis (local Y) to world Z — so local Z is what points at the sky, and
    // scaling local Y would stretch its LENGTH instead. Safe on a leaf: a
    // scale propagates to children, and this node has none.
    on_plinth.push(prim_scaled(
        solid(with_cut(
            cylinder_tapered(
                WALL[0] * 0.5,
                WALL[2] + 0.4,
                20,
                0.0,
                ashlar(STONE_LIME, 0xE8),
            ),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        )),
        [0.0, SPRING, 0.0],
        // NEGATIVE quarter turn. `path_cut` keeps the half of the sweep on
        // local +Z, and `quat_x(+FRAC_PI_2)` sends local +Z to world −Y — so
        // the positive turn hung the vault BELOW its own springing, inside the
        // store, which the stack guard caught to the millimetre. The negative
        // turn puts the kept half over the walls where a roof goes.
        quat_x(-FRAC_PI_2),
        [1.0, 1.0, VAULT_RISE / (WALL[0] * 0.5)],
    ));
    // Turfed cap over the vault — earth on the crown is what a magazine wears
    // so a burst goes up rather than out.
    on_plinth.push(prim(
        solid(cuboid_tapered_xz(
            [WALL[0] + 0.5, 0.5, WALL[2] + 0.5],
            [0.5, 0.0],
            shingle(SHINGLE_GREY),
        )),
        [0.0, CROWN - 0.1, 0.0],
        id_quat(),
    ));
    // String course ringing all four elevations at the springing — a RING, so
    // it takes the building's own centre and its projection goes into its
    // SIZE (#972 lesson 31).
    let ring_c = [0.0, SPRING - 0.12, 0.0];
    on_plinth.push(prim(
        solid(with_face(
            cuboid_tapered(
                [WALL[0] + 0.4, 0.24, WALL[2] + 0.4],
                0.0,
                coursed(ring_c, FaceKey::SideNz, 0xE9),
            ),
            FaceKey::Top,
            coursed(ring_c, FaceKey::Top, 0xE9),
        )),
        ring_c,
        id_quat(),
    ));

    // Copper conductor down the seaward corner, earthed in the apron. One
    // strut, so the run cannot end up pointing near its own earth plate.
    let corner = [WALL[0] * 0.5 - 0.1, 0.0, FRONT_Z + 0.2];
    on_plinth.push(strut(
        [corner[0], CROWN - 0.2, corner[2]],
        [corner[0] + 0.35, GROUND + 0.06, corner[2] - 0.3],
        0.035,
        6,
        bronze(BRONZE_FITTING, 0xEA),
    ));
    on_plinth.push(prim(
        sphere(0.11, 3, bronze(BRONZE_FITTING, 0xEB)),
        [corner[0], CROWN - 0.1, corner[2]],
        id_quat(),
    ));

    let mut carried = vec![
        footing(PLINTH[0], PLINTH[2], [0.0, 0.0], 8.0),
        nest(
            prim(
                solid(cuboid_tapered(PLINTH, 0.0, cobbles(STONE_QUAY, 0xEC))),
                plinth_c,
                id_quat(),
            ),
            on_plinth,
        ),
        traverse(),
    ];

    // Sentry's kit by the traverse's end, clear of both masses: a slow-match
    // tub, a hand-barrow of casks. Placed from the TRAVERSE's own extent, not
    // from the apron's, so it cannot walk into either building (#1028).
    let sx = TRAVERSE[0] * 0.5 + 0.9;
    let sz = FRONT_Z - TRAVERSE_OFF;
    carried.push(prim(
        solid(cylinder_tapered(0.34, 0.6, 12, -0.06, board(HULL_OAK))),
        [sx, GROUND + 0.3, sz],
        id_quat(),
    ));
    carried.push(prim(
        torus(0.04, 0.35, bronze(BRONZE_FITTING, 0xED)),
        [sx, GROUND + 0.5, sz],
        id_quat(),
    ));
    carried.push(prim(
        torus(0.05, 0.3, hemp(ROPE_HEMP)),
        [-sx, GROUND + 0.05, sz + 0.4],
        id_quat(),
    ));

    let mut root = nest(
        prim(
            solid(cuboid_tapered(APRON, 0.0, paving)),
            apron_c,
            id_quat(),
        ),
        carried,
    );
    root.audio = fx::harbour_swell();
    // A crate by the traverse's inboard end. Placed BEHIND the bank (toward
    // the store) rather than in front of it: outboard it reached 0.15 m past
    // the apron, which the footprint guard caught (#972 lesson 8 — derive
    // from the surface, and here the surface's near edge is what binds).
    attach(
        &mut root,
        prim(
            solid(cuboid_tapered([0.5, 0.42, 0.5], 0.2, board(DECK_HOLY))),
            [-sx + 0.4, GROUND + 0.21, sz + 0.9],
            id_quat(),
        ),
    );
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure;
    use crate::catalogue::items::util::{
        assert_cards_do_not_overlap, assert_no_glazing_on_solids, assert_no_tilted_parents,
        assert_sanitize_stable, has_emissive, rotate_by, window_cards,
    };

    fn built() -> Generator {
        PowderMagazine.build("")
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&built(), "powder_magazine");
    }

    #[test]
    fn no_rotated_node_carries_an_offset_child() {
        assert_no_tilted_parents(&built(), "powder_magazine");
    }

    /// A magazine has NO glazing — vents and one door, nothing else.
    ///
    /// The third entry in this kit to reach #972 lesson 24's answer from its
    /// own direction (gun ports, serving hatch, vents). Stated as a
    /// prohibition because the kit now HAS a card, and the reflex to reach for
    /// one on a stone building is what the ledger keeps catching.
    #[test]
    fn the_magazine_carries_no_glazing() {
        let g = built();
        assert_no_glazing_on_solids(&g, "powder_magazine");
        assert_cards_do_not_overlap(&g, "powder_magazine");
        assert!(
            window_cards(&g).is_empty(),
            "the magazine has grown a window; its openings are splayed vents \\
             and a door, which is the entire design brief of the building"
        );
        assert!(has_emissive(&g), "the magazine lost its lantern");
    }

    /// The traverse stands ACROSS the approach at a stand-off — blocking the
    /// line to the door without touching the building.
    ///
    /// Both halves matter. Flush against the wall it is a thicker wall; off
    /// to one side it blocks nothing. And it must be wide enough to cover the
    /// doorway, or it is a decorative lump.
    #[test]
    fn the_traverse_screens_the_door_at_a_stand_off() {
        let g = built();
        let bank = measure::solids(&g)
            .into_iter()
            .find(|p| {
                // Selected by the property that DEFINES it: a tall mass
                // standing well FORWARD of the store. Matching on width alone
                // picked up part of the building itself — the fifth selector
                // fault in this kit, and the fifth time the answer was to
                // select on what makes the thing itself rather than on a
                // dimension it happens to share (#972 lesson 24).
                let s = p.bounds.size();
                s.y > 1.5 && p.bounds.center().z < FRONT_Z - 1.0
            })
            .expect("the blast traverse is in the tree");
        // Clear of the store, on the approach side.
        assert!(
            bank.bounds.max.z < FRONT_Z - 0.5,
            "the traverse reaches z = {} against a wall face at {FRONT_Z} — \\
             flush against the building it is just a thicker wall",
            bank.bounds.max.z
        );
        assert!(
            bank.bounds.max.z > FRONT_Z - TRAVERSE_OFF - 1.0,
            "the traverse has drifted {} m off the approach — it screens \\
             nothing",
            FRONT_Z - bank.bounds.max.z
        );
        // Wide enough to cover the doorway, centred on it.
        assert!(
            bank.bounds.min.x < -DOOR_W && bank.bounds.max.x > DOOR_W,
            "the traverse spans {} .. {} and the door is ±{} — it does not \\
             screen the opening",
            bank.bounds.min.x,
            bank.bounds.max.x,
            DOOR_W * 0.5
        );
    }

    /// The open leaf hangs on its own hinge, and its straps hang with it.
    ///
    /// #972 lesson 21's corollary, and the tavern's exact shipped fault
    /// (#1028): a centre placed on the swung arc with the rotation left at
    /// the identity. The straps are the extra claim here — they are placed
    /// from the same arm and turn, so if the leaf moves and they do not, the
    /// door comes apart.
    #[test]
    fn the_open_leaf_and_its_straps_hang_together() {
        use crate::pds::GeneratorKind as K;
        fn yawed(g: &Generator, at: [f32; 3], out: &mut Vec<([f32; 3], [f32; 4], [f32; 3])>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Cuboid { size, .. } = &g.kind {
                let q = g.transform.rotation.0;
                if q[1].abs() > 0.05 && q[0].abs() < 1e-4 && q[2].abs() < 1e-4 {
                    out.push((here, q, size.0));
                }
            }
            for c in &g.children {
                yawed(c, here, out);
            }
        }
        let mut parts = Vec::new();
        yawed(&built(), [0.0; 3], &mut parts);
        assert_eq!(
            parts.len(),
            3,
            "expected the leaf and two straps, found {}",
            parts.len()
        );
        // The jamb is the hinge's OWN plane, derived from the same constant
        // the placement uses. Left at a literal `FRONT_Z + 0.12` it went
        // stale the moment the door moved back into its reveal, and the
        // guard then reported a correctly-hung door as hanging on nothing.
        let jamb = [-DOOR_W * 0.5, FRONT_Z + REVEAL];
        for (c, q, s) in &parts {
            let tip = rotate_by(*q, [s[0] * 0.5, 0.0, 0.0]);
            let ends = [
                [c[0] + tip[0], c[2] + tip[2]],
                [c[0] - tip[0], c[2] - tip[2]],
            ];
            assert!(
                ends.iter()
                    .any(|e| (e[0] - jamb[0]).abs() < 0.12 && (e[1] - jamb[1]).abs() < 0.12),
                "a leaf/strap's ends {ends:?} do not reach the hinge jamb at \\
                 {jamb:?} — it is hung on nothing"
            );
            let free = ends
                .iter()
                .max_by(|a, b| {
                    (a[0] - jamb[0])
                        .abs()
                        .partial_cmp(&(b[0] - jamb[0]).abs())
                        .expect("finite")
                })
                .expect("two ends");
            assert!(
                free[1] < FRONT_Z - 0.3,
                "the free edge sits at z = {} — the leaf is not standing open",
                free[1]
            );
        }
    }

    /// The store's fit-out is down the sightline from the door and forward of
    /// the back wall.
    ///
    /// A magazine's only aperture is its door, so anything against a flank is
    /// invisible — this is #972 lesson 9 (every bay its own thing to look at)
    /// with exactly one bay, which sharpens rather than relaxes it.
    #[test]
    fn the_casks_stand_in_the_doorways_own_sightline() {
        let g = built();
        let casks: Vec<_> = measure::solids(&g)
            .into_iter()
            .filter(|p| {
                // Selected by the cask's own girth — "a cylinder in the room"
                // also matches the lantern body, which is a cylinder in the
                // room on purpose (#972 lesson 24, for the fourth time in
                // this kit).
                let c = p.bounds.center();
                let sz = p.bounds.size();
                p.kind_tag == "Cylinder"
                    && (sz.y - 0.72).abs() < 0.12
                    && c.z > FRONT_Z
                    && c.z < ROOM_BACK
                    && c.y > FLOOR
                    && c.y < SPRING
            })
            .collect();
        assert!(
            casks.len() >= 4,
            "only {} casks inside — a door onto an empty floor is a darker \\
             rectangle on the wall",
            casks.len()
        );
        for p in &casks {
            let c = p.bounds.center();
            assert!(
                c.x.abs() < DOOR_W,
                "a cask at x = {} is outside the doorway's own sightline \\
                 (±{DOOR_W}) — through a single aperture it is invisible",
                c.x
            );
            assert!(
                c.z < ROOM_BACK - 0.3,
                "a cask at z = {} is against the back lining, where it is an \\
                 unreadable speck from the apron",
                c.z
            );
        }
    }

    /// The building is a contiguous stack: apron → plinth → wall → springing
    /// → vault crown (#972 lesson 33).
    #[test]
    fn the_vault_springs_from_the_wall_it_stands_on() {
        const _: () = assert!(
            SPRING > FLOOR && CROWN > SPRING,
            "the vault does not rise above its own springing"
        );
        let g = built();
        let vault = measure::solids(&g)
            .into_iter()
            .find(|p| p.kind_tag == "Cylinder" && p.bounds.size().x > WALL[0] * 0.8)
            .expect("the barrel vault is in the tree");
        assert!(
            (vault.bounds.min.y - SPRING).abs() < 0.35,
            "the vault's springing is at {} not {SPRING} — it floats over its \\
             own walls, or sits inside them",
            vault.bounds.min.y
        );
        assert!(
            vault.bounds.max.y > SPRING + VAULT_RISE * 0.5,
            "the vault rises only to {} — a barrel roof that flat reads as a \\
             lid",
            vault.bounds.max.y
        );
    }

    /// Everything on the ground stands on the apron, and the sentry's kit
    /// clears both masses (#972 lessons 8, 19 and #1028's other half).
    #[test]
    fn every_ground_part_stands_clear_on_the_apron() {
        let g = built();
        let half = [APRON[0] * 0.5, APRON[2] * 0.5];
        let ph = [PLINTH[0] * 0.5, PLINTH[2] * 0.5];
        let mut checked = 0;
        for p in measure::solids(&g) {
            let b = &p.bounds;
            if b.center().y > FLOOR + 0.8 {
                continue;
            }
            checked += 1;
            assert!(
                b.min.x >= -half[0] - 1e-3 && b.max.x <= half[0] + 1e-3,
                "{} at {:?} overhangs the apron in X",
                p.kind_tag,
                b.center()
            );
            assert!(
                b.min.z >= -half[1] - 1e-3 && b.max.z <= half[1] + 1e-3,
                "{} at {:?} overhangs the apron in Z",
                p.kind_tag,
                b.center()
            );
            // Loose kit — revolved prims on the apron — clears the plinth.
            if matches!(p.kind_tag, "Cylinder" | "Torus") && b.min.y < FLOOR - 0.05 {
                let hits =
                    b.max.x > -ph[0] && b.min.x < ph[0] && b.max.z > -ph[1] && b.min.z < ph[1];
                assert!(
                    !hits,
                    "{} at {:?} runs into the plinth footprint",
                    p.kind_tag,
                    b.center()
                );
            }
        }
        assert!(checked > 8, "only {checked} ground parts examined");
    }
}
