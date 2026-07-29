//! Factory — the Industrial-Park landmark. A long clad works on a brick
//! dado, presenting three raised loading docks and a clerestory band to the
//! yard, one shutter rolled up on a lit shop floor, under a low-pitch roof
//! with a glazed monitor along its ridge. A tall brick stack pours smoke over
//! a heavy machine hum.
//!
//! Rebuilt as a **shell** under the standing lessons of #972:
//!
//! 1. **The glazing fills real holes.** The clerestory band and the roof
//!    monitor are alpha cards on flat quads over a lit shop floor. They
//!    used to be `Window`-textured *slabs* pinned to a solid box — and the
//!    generator masks its panes away, so each was a frame with holes onto the
//!    cladding behind it. The old "lit window band" was worse still: a flat
//!    amber lightbox 18 m long, which is exactly the thing the standing
//!    gotcha about flat lightboxes warns off.
//! 2. **The docks are docks.** Three roller shutters used to be flat panels
//!    laid on the wall of a solid mass, at ground level, with nothing behind
//!    them and no way for a lorry to reach them. They are now real openings
//!    in a raised dock face — bumpers, levellers, a canopy — with the middle
//!    shutter rolled up on the shop floor, which is where the whole prop's
//!    depth comes from.
//! 3. **The brick lies flat, and the works stands the way it does.** The
//!    dado and the stack go through [`util::bonded_brick`]; the tree runs
//!    yard → shell → roof → monitor, with the stack and the pipe gantry their
//!    own sub-assemblies.
//!
//! [`util::bonded_brick`]: crate::catalogue::items::util::bonded_brick

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    self, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, lit_interior, nest, plane,
    prim, quat_x, solid, torus, tube,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRICK_DARK, CONCRETE_GREY, LAMP_AMBER, PIPE_GREY, STEEL_BLUE, WINDOW_LIT, brick, cladding,
    concrete, fx, glass, tank_steel,
};

// --- Shell dimensions. Everything below derives from these. ----------------

/// Shed width along the yard (X) and depth (Z).
const W: f32 = 22.0;
const D: f32 = 13.0;
/// Concrete yard slab: its top is the yard level and the datum for
/// everything below, and its plan is what every piece of yard furniture is
/// derived from.
const YARD_H: f32 = 0.5;
const YARD_W: f32 = W + 5.0;
const YARD_D: f32 = D + 8.0;
/// The yard slab is pushed forward of the shed so the apron is in front of
/// the docks rather than centred on the building.
const YARD_Z: f32 = -1.4;
/// Front edge of the yard slab — where the kerb goes, and the line every
/// marking has to stay inside.
const YARD_FRONT: f32 = YARD_Z - YARD_D * 0.5;
/// Brick dado, and so the height of the raised dock floor above the yard.
const DADO_H: f32 = 1.55;
/// Cladding height above the dado — the eaves.
const CLAD_H: f32 = 6.45;
/// Wall thickness, and so the depth of every reveal.
const WALL_T: f32 = 0.34;
/// Shop-floor level and eaves level, above the yard slab.
const FLOOR: f32 = DADO_H;
const EAVES: f32 = DADO_H + CLAD_H;

/// Outer face of the yard elevation — the `-Z` hero direction the render tool
/// and the settlement placer both look down.
const FRONT: f32 = -D * 0.5;
const FRONT_MID: f32 = FRONT + WALL_T * 0.5;
/// How far the spandrel bands sit back from the piers' plane. Enough to read
/// as a shadow line, and enough that no two clad faces are ever coplanar.
const RECESS: f32 = 0.06;
/// Glazing plane, set back inside the reveal.
const GLAZE_Z: f32 = FRONT + 0.2;
/// Where the shop-floor lining stands behind an opening.
const ROOM_Z: f32 = FRONT + 1.9;

/// Dock openings: three of them, this wide and this tall above the floor.
const DOCK_X: [f32; 3] = [-6.6, 0.0, 6.6];
const DOCK_W: f32 = 3.4;
const DOCK_H: f32 = 4.2;
/// The dock whose shutter stands rolled up. Everything visible through the
/// building comes through this one opening, so it is named rather than
/// implied.
const OPEN_DOCK: usize = 1;
/// Personnel door at the far end of the elevation.
const DOOR_X: f32 = -9.9;
const DOOR_W: f32 = 1.2;
const DOOR_H: f32 = 2.2;
/// Clerestory band over the whole elevation: sill and head above the floor.
const CLERE_SILL: f32 = 5.1;
const CLERE_HEAD: f32 = 6.15;

/// Roof rise from the eaves to the ridge, and the overhang at the eaves.
const ROOF_RISE: f32 = 1.7;
const EAVE_OVER: f32 = 0.7;
const ROOF_T: f32 = 0.26;
/// The roof monitor — a raised glazed lantern along the ridge.
const MON_W: f32 = 14.0;
const MON_D: f32 = 3.4;
const MON_H: f32 = 1.9;

/// Brick length in metres — a real 215 mm brick. The kit's shared sizing lays
/// a 172 mm one, standing every brick on end into the bargain.
const BRICK_LEN: f32 = 0.215;

// --- Palette local to this entry. ------------------------------------------

/// Ochre process pipework on the external gantry.
const PIPE_OCHRE: [f32; 3] = [0.62, 0.5, 0.2];
/// Shutter slats — a paler grey than the wall, so a closed bay still reads as
/// a door rather than as more cladding.
const SHUTTER_GREY: [f32; 3] = [0.52, 0.54, 0.56];
/// Safety yellow: dock edge, bollards, the leveller lip.
const HAZARD: [f32; 3] = [0.78, 0.62, 0.12];

// --- Shared construction. --------------------------------------------------

/// The works' brickwork, bonded into the shared world course frame.
fn bonded(color: [f32; 3], center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    util::bonded_brick(brick(color), BRICK_LEN, face, center)
}

/// One brick slab of the dado or the stack.
fn brick_slab(size: [f32; 3], center: [f32; 3], face: FaceKey) -> Generator {
    prim(
        solid(cuboid_tapered(size, 0.0, bonded(BRICK_DARK, center, face))),
        center,
        id_quat(),
    )
}

/// One clad slab of the shell. Profiled sheet is ribbed along U, and every
/// side face reads U horizontally, so wall cladding comes out vertically
/// ribbed without any help — which is how it is actually hung.
fn clad(size: [f32; 3], center: [f32; 3], face: FaceKey) -> Generator {
    prim(
        solid(cuboid_tapered(size, 0.0, {
            let mut m = cladding(STEEL_BLUE);
            m.uv_offset = util::face_uv_offset(face, center);
            m
        })),
        center,
        id_quat(),
    )
}

/// How far a glazing card oversails its opening on every edge (#972 lesson 7).
const GLAZE_LAP: f32 = 0.06;

/// Industrial glazing on a flat quad — the kit's own grimy [`glass`], with
/// its pane grid re-cut to the opening.
///
/// The kit material is already card-shaped (`uv_scale` 1.0, alpha-masked
/// panes) and carries the theme's grime, which is worth more here than
/// a bare card's cleaner default; what it cannot know is the aspect of
/// the hole it is filling. Pane counts are what tell a viewer how big an
/// opening is, so they are picked per opening and everything else is
/// inherited.
fn glazing(panes: (u32, u32), size: [f32; 2], center: [f32; 3], rot: crate::pds::Fp4) -> Generator {
    let mut mat = glass(WINDOW_LIT, 0.0);
    if let crate::pds::SovereignTextureConfig::Window(cfg) = &mut mat.texture {
        cfg.panes_x = panes.0;
        cfg.panes_y = panes.1;
    }
    prim(
        plane([size[0] + GLAZE_LAP, size[1] + GLAZE_LAP], mat),
        center,
        rot,
    )
}

/// A lit shop-floor surface — what a card's masked-away panes actually show.
/// Nothing lights the inside of an enclosed prop, so these carry a low
/// self-lit term of their own; without it every opening is a black rectangle.
fn shop(size: [f32; 3], center: [f32; 3], lit: f32) -> Generator {
    prim(
        cuboid_tapered(size, 0.0, lit_interior([0.44, 0.38, 0.30], lit)),
        center,
        id_quat(),
    )
}

pub struct Factory;

impl CatalogueEntry for Factory {
    fn slug(&self) -> &'static str {
        "factory"
    }
    fn name(&self) -> &'static str {
        "Factory"
    }
    fn description(&self) -> &'static str {
        "Clad works with raised loading docks, a lit shop floor and a smoking brick stack."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::INDUSTRIAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 17.0,
            min_spawn_dist: 55.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The works as a tree that stands the way it does: the yard slab at the
/// bottom, the shell on it (carrying the roof and the monitor), and the stack
/// and the pipe gantry as their own sub-assemblies.
///
/// Written outermost-last, because [`nest`] rebases a subtree that already
/// carries its own world translation.
fn build_tree() -> Generator {
    let yard = prim(
        solid(cuboid_tapered(
            [YARD_W, YARD_H, YARD_D],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, YARD_H * 0.5, YARD_Z],
        id_quat(),
    );
    let mut root = nest(
        yard,
        vec![
            shell(),
            stack(),
            gantry(),
            yard_marks(),
            // Buried plinth under the yard slab, deep enough for the ground
            // the works gets dropped on (#1009).
            footing(YARD_W, YARD_D, [0.0, YARD_Z], 17.0),
        ],
    );
    // Signature life: smoke from the stack, over the plant's heavy hum.
    root.children.push(fx::stack_smoke(
        [stack_x(), YARD_H + STACK_H + 0.5, stack_z()],
        0x5AC0_5E11,
    ));
    root.audio = fx::machine_hum();
    root
}

// --- The shell. ------------------------------------------------------------

/// Shop floor, and on it everything the works is: the dado, the cladding that
/// frames the openings, the glazing, the fit-out behind it, the docks, and —
/// on the eaves — the roof.
fn shell() -> Generator {
    let mut parts = Vec::new();
    let base = YARD_H;
    let clad_mid = base + FLOOR + CLAD_H * 0.5;
    let inner_d = D - WALL_T * 2.0;

    // Brick dado, ringing all four elevations. A base course must stand
    // *proud* of the wall above it, never flush: flush is a coplanar seam
    // running the whole perimeter on the most looked-at part of the building.
    for sz in [-1.0_f32, 1.0] {
        parts.push(brick_slab(
            [W + 0.12, DADO_H, WALL_T + 0.12],
            [0.0, base + DADO_H * 0.5, sz * (D * 0.5 - WALL_T * 0.5)],
            if sz > 0.0 {
                FaceKey::SidePz
            } else {
                FaceKey::SideNz
            },
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        parts.push(brick_slab(
            [WALL_T + 0.12, DADO_H, inner_d],
            [sx * (W * 0.5 - WALL_T * 0.5), base + DADO_H * 0.5, 0.0],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
        ));
    }

    // Back and flank cladding — solid; only the yard elevation is cut.
    parts.push(clad(
        [W, CLAD_H, WALL_T],
        [0.0, clad_mid, D * 0.5 - WALL_T * 0.5],
        FaceKey::SidePz,
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(clad(
            [WALL_T, CLAD_H, inner_d],
            [sx * (W * 0.5 - WALL_T * 0.5), clad_mid, 0.0],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
        ));
    }

    yard_elevation(&mut parts);
    docks(&mut parts);
    fit_out(&mut parts);
    parts.push(roof());

    let floor = prim(
        cuboid_tapered(
            [W - WALL_T * 2.0, 0.14, inner_d],
            0.0,
            lit_interior([0.30, 0.29, 0.28], 0.1),
        ),
        [0.0, base + FLOOR - 0.07, 0.0],
        id_quat(),
    );
    nest(floor, parts)
}

/// The bay edges of the yard elevation, left to right — the frame every
/// opening is cut out of. One list, because the piers, the bands, the
/// glazing, the shutters and the guards all have to agree about where the
/// holes are.
fn bay_edges() -> Vec<(f32, f32)> {
    let mut open: Vec<(f32, f32)> = vec![(DOOR_X - DOOR_W * 0.5, DOOR_X + DOOR_W * 0.5)];
    for &x in DOCK_X.iter() {
        open.push((x - DOCK_W * 0.5, x + DOCK_W * 0.5));
    }
    open.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    open
}

/// The hero elevation: five full-height clad piers standing [`RECESS`] proud
/// of the spandrel bands behind them, framing the personnel door, three dock
/// openings and a clerestory band that runs the length of the works.
///
/// The piers-proud-of-bands scheme is what keeps this to eight wall slabs
/// instead of the seventeen a per-bay frame would need, and it gives a clad
/// elevation the one thing it otherwise lacks: a vertical shadow line at
/// structural-bay spacing, which is what a portal frame actually looks like.
fn yard_elevation(parts: &mut Vec<Generator>) {
    let base = YARD_H;
    let open = bay_edges();

    // Piers: wall end → first opening, between openings, last → wall end.
    let mut piers = vec![(-W * 0.5, open[0].0)];
    for pair in open.windows(2) {
        piers.push((pair[0].1, pair[1].0));
    }
    piers.push((open[open.len() - 1].1, W * 0.5));
    for (a, b) in piers {
        parts.push(clad(
            [b - a, CLAD_H, WALL_T],
            [(a + b) * 0.5, base + FLOOR + CLAD_H * 0.5, FRONT_MID],
            FaceKey::SideNz,
        ));
    }

    // Recessed spandrel bands, full width behind the piers: over the docks up
    // to the clerestory sill, and over the clerestory up to the eaves. `band`
    // is (low, high) above the shop floor.
    let band = |lo: f32, hi: f32| {
        clad(
            [W - 0.2, hi - lo, WALL_T],
            [0.0, base + FLOOR + (lo + hi) * 0.5, FRONT_MID + RECESS],
            FaceKey::SideNz,
        )
    };
    parts.push(band(DOCK_H, CLERE_SILL));
    parts.push(band(CLERE_HEAD, CLAD_H));
    // The personnel door is shorter than a dock, so its bay needs filling
    // between the two.
    parts.push(clad(
        [DOOR_W, DOCK_H - DOOR_H, WALL_T],
        [
            DOOR_X,
            base + FLOOR + (DOOR_H + DOCK_H) * 0.5,
            FRONT_MID + RECESS,
        ],
        FaceKey::SideNz,
    ));

    // Clerestory glazing, one card per bay between the piers, each over the
    // lit shop floor.
    for (a, b) in bay_edges() {
        parts.push(glazing(
            (3, 2),
            [b - a, CLERE_HEAD - CLERE_SILL],
            [
                (a + b) * 0.5,
                base + FLOOR + (CLERE_SILL + CLERE_HEAD) * 0.5,
                GLAZE_Z,
            ],
            quat_x(-FRAC_PI_2),
        ));
    }
    // The lining the clerestory looks onto — held near the glass, because the
    // works is 13 m deep and a wall at the back of it is an unreadable speck
    // (#972 lesson 6).
    parts.push(shop(
        [W - 1.5, CLERE_HEAD - CLERE_SILL + 0.7, 0.12],
        [
            0.0,
            base + FLOOR + (CLERE_SILL + CLERE_HEAD) * 0.5,
            FRONT + 1.1,
        ],
        0.42,
    ));

    // The personnel door: a steel leaf in its reveal, and the flight up to it
    // — the shop floor is a dock height off the yard, so a door with no steps
    // opens onto a drop.
    parts.push(prim(
        solid(cuboid_tapered(
            [DOOR_W - 0.08, DOOR_H - 0.06, 0.09],
            0.0,
            tank_steel([0.36, 0.37, 0.38]),
        )),
        [DOOR_X, base + FLOOR + DOOR_H * 0.5, GLAZE_Z - 0.05],
        id_quat(),
    ));
    parts.push(shop(
        [DOOR_W + 0.5, DOOR_H + 0.4, 0.1],
        [DOOR_X, base + FLOOR + DOOR_H * 0.5, ROOM_Z],
        0.3,
    ));
    for i in 0..3 {
        let top = FLOOR * (3 - i) as f32 / 3.0;
        parts.push(prim(
            solid(cuboid_tapered(
                [1.5, top, 0.32],
                0.0,
                concrete([0.5, 0.5, 0.51]),
            )),
            [DOOR_X, base + top * 0.5, FRONT - 0.16 - 0.32 * i as f32],
            id_quat(),
        ));
    }
}

/// The three loading docks: a hazard-striped dock edge, rubber bumpers, a
/// leveller lip, and the roller shutters — two down, one rolled up on the
/// shop floor.
///
/// Rolling one shutter open is the same call the detached garage made: closed,
/// the elevation is a wall with three flat panels on it and nothing the cards
/// or the lighting can be about; open, the bay gains four metres of real depth
/// and the shop floor becomes the point of the prop.
fn docks(parts: &mut Vec<Generator>) {
    let base = YARD_H;
    let sill = base + FLOOR;
    for (i, &x) in DOCK_X.iter().enumerate() {
        // Dock edge: a hazard-painted nosing on the slab lip.
        parts.push(prim(
            solid(cuboid_tapered(
                [DOCK_W + 0.5, 0.12, 0.3],
                0.0,
                glow(HAZARD, 0.0),
            )),
            [x, sill - 0.06, FRONT - 0.14],
            id_quat(),
        ));
        // Rubber bumpers each side of the opening.
        for sx in [-1.0_f32, 1.0] {
            parts.push(prim(
                solid(cuboid_tapered(
                    [0.28, 0.5, 0.22],
                    0.0,
                    tank_steel([0.1, 0.1, 0.11]),
                )),
                [x + sx * (DOCK_W * 0.5 + 0.2), sill - 0.34, FRONT - 0.11],
                id_quat(),
            ));
        }
        // Jamb tracks the shutter runs in, both sides.
        for sx in [-1.0_f32, 1.0] {
            parts.push(prim(
                solid(cuboid_tapered(
                    [0.14, DOCK_H + 0.2, 0.16],
                    0.0,
                    tank_steel([0.34, 0.35, 0.36]),
                )),
                [
                    x + sx * (DOCK_W * 0.5 + 0.07),
                    sill + DOCK_H * 0.5,
                    FRONT - 0.1,
                ],
                id_quat(),
            ));
        }

        if i == OPEN_DOCK {
            // Rolled drum and the last slat hanging under it.
            parts.push(prim(
                solid(cylinder_tapered(
                    0.4,
                    DOCK_W,
                    12,
                    0.0,
                    tank_steel(SHUTTER_GREY),
                )),
                [x, sill + DOCK_H - 0.45, FRONT - 0.24],
                util::quat_z(FRAC_PI_2),
            ));
            parts.push(prim(
                solid(cuboid_tapered(
                    [DOCK_W, 0.16, 0.08],
                    0.0,
                    tank_steel([0.3, 0.31, 0.32]),
                )),
                [x, sill + DOCK_H - 0.86, FRONT - 0.24],
                id_quat(),
            ));
            // Leveller lip folded down into the bay.
            parts.push(prim(
                solid(cuboid_tapered(
                    [DOCK_W - 0.4, 0.09, 0.9],
                    0.0,
                    glow(HAZARD, 0.0),
                )),
                [x, sill + 0.02, FRONT + 0.5],
                id_quat(),
            ));
        } else {
            // Shutter down: a slatted leaf, with ribs so it reads as rolled
            // steel rather than as a painted panel.
            parts.push(prim(
                solid(cuboid_tapered(
                    [DOCK_W, DOCK_H, 0.1],
                    0.0,
                    tank_steel(SHUTTER_GREY),
                )),
                [x, sill + DOCK_H * 0.5, FRONT - 0.19],
                id_quat(),
            ));
            for r in 0..7 {
                parts.push(prim(
                    cuboid_tapered(
                        [DOCK_W - 0.06, 0.07, 0.06],
                        0.0,
                        tank_steel([0.4, 0.41, 0.42]),
                    ),
                    [
                        x,
                        sill + 0.35 + r as f32 * (DOCK_H - 0.7) / 6.0,
                        FRONT - 0.25,
                    ],
                    id_quat(),
                ));
            }
        }
    }

    // Canopy over the whole dock run, on two tie rods.
    parts.push(prim(
        solid(cuboid_tapered(
            [W - 3.0, 0.22, 2.2],
            0.0,
            tank_steel([0.46, 0.48, 0.5]),
        )),
        [1.0, sill + DOCK_H + 0.55, FRONT - 1.0],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            cuboid_tapered([0.08, 1.5, 0.08], 0.0, tank_steel(PIPE_GREY)),
            [1.0 + sx * 8.0, sill + DOCK_H + 1.3, FRONT - 1.5],
            util::quat_x(-0.55),
        ));
    }
}

/// What the open dock shows: the shop floor, a lining held close behind the
/// opening, a machine line, and the lit strip that says the plant is working.
///
/// Depth discipline (#972 lessons 6 and 10): the lining is 3.5 m in, not
/// against the back wall thirteen metres away, and the strip light hangs
/// *below* the rolled drum that crosses the sightline from the yard.
fn fit_out(parts: &mut Vec<Generator>) {
    let base = YARD_H;
    let sill = base + FLOOR;
    let x = DOCK_X[OPEN_DOCK];
    // Back lining of the visible bay, in a warmer tone than the floor and the
    // ceiling — three interior surfaces at one tone are a flat grey box
    // however well lit they are.
    parts.push(prim(
        cuboid_tapered(
            [W - 3.0, CLAD_H - 0.4, 0.16],
            0.0,
            lit_interior([0.40, 0.30, 0.22], 0.2),
        ),
        [0.0, sill + (CLAD_H - 0.4) * 0.5, ROOM_Z + 1.6],
        id_quat(),
    ));
    // A machine line across the bay, and a stack of crated goods beside it.
    parts.push(shop(
        [6.0, 1.5, 1.6],
        [x - 0.4, sill + 0.75, ROOM_Z + 0.6],
        0.22,
    ));
    parts.push(shop(
        [1.6, 0.9, 1.2],
        [x + 2.6, sill + 0.45, ROOM_Z - 0.3],
        0.22,
    ));
    // Ceiling strip, and — because the drum crosses the head — a second
    // source down at bench height that the yard can actually see.
    parts.push(prim(
        cuboid_tapered([8.0, 0.16, 0.3], 0.0, glow(WINDOW_LIT, 2.0)),
        [x, sill + DOCK_H - 0.25, ROOM_Z + 1.0],
        id_quat(),
    ));
    parts.push(prim(
        solid(cuboid_tapered(
            [0.3, 0.36, 0.24],
            0.0,
            tank_steel([0.2, 0.2, 0.22]),
        )),
        [x - 1.4, sill + 1.9, ROOM_Z - 0.1],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.2, 0.22, 0.1], 0.0, glow(LAMP_AMBER, 2.6)),
        [x - 1.4, sill + 1.86, ROOM_Z - 0.24],
        id_quat(),
    ));
}

// --- The roof. -------------------------------------------------------------

/// Low-pitch double roof, its ridge along the works, carrying the glazed
/// monitor and the extract ducting.
///
/// The two planes run **long along X**, which is what puts the sheet's ribs
/// down the pitch without any help: a Box `Top` face reads U along the slab's
/// own X, the corrugated generator varies its ribs along U, and rainwater and
/// rolled sheet both run down a roof. Orient the slab the other way and the
/// ribs come out horizontal — the mistake the barn had to correct with a
/// quarter turn.
fn roof() -> Generator {
    let base = YARD_H + EAVES;
    let half = D * 0.5 + EAVE_OVER;
    let slope = ROOF_RISE.hypot(half);
    let pitch = ROOF_RISE.atan2(half);

    // The ridge cap is the sub-root: dragging the ridge takes the roof, the
    // monitor and the ducting with it.
    let ridge = prim(
        solid(cuboid_tapered(
            [W + 0.4, 0.22, 0.7],
            0.0,
            tank_steel([0.4, 0.42, 0.44]),
        )),
        [0.0, base + ROOF_RISE + 0.06, 0.0],
        id_quat(),
    );

    let mut parts = Vec::new();
    for sz in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered([W + 0.4, ROOF_T, slope], 0.0, {
                let mut m = cladding([0.44, 0.46, 0.48]);
                m.uv_offset = util::face_uv_offset(FaceKey::Top, [0.0, 0.0, sz * half * 0.5]);
                m
            })),
            [0.0, base + ROOF_RISE * 0.5, sz * half * 0.5],
            quat_x(sz * pitch),
        ));
        // Eaves gutter.
        parts.push(prim(
            solid(cuboid_tapered(
                [W + 0.4, 0.2, 0.24],
                0.0,
                tank_steel([0.42, 0.44, 0.46]),
            )),
            [0.0, base - 0.06, sz * (half + 0.1)],
            id_quat(),
        ));
    }

    parts.push(monitor(base + ROOF_RISE));
    // Extract ducting either side of the monitor.
    for vx in [-8.4_f32, 8.4] {
        parts.push(prim(
            solid(tube(0.44, 0.32, 1.7, 14, tank_steel(PIPE_GREY))),
            [vx, base + ROOF_RISE * 0.4 + 0.9, 1.6],
            id_quat(),
        ));
        parts.push(prim(
            solid(cylinder_tapered(0.56, 0.16, 14, 0.0, tank_steel(PIPE_GREY))),
            [vx, base + ROOF_RISE * 0.4 + 1.8, 1.6],
            id_quat(),
        ));
    }
    nest(ridge, parts)
}

/// The roof monitor — a raised lantern along the ridge, glazed on both long
/// sides over the shop floor, with its own little pitched cap.
///
/// This is the second reason the shed has an interior: from any angle above
/// eye level the monitor is the most visible glazing on the prop, and a
/// clerestory with nothing behind it is a row of holes onto sky.
fn monitor(ridge_y: f32) -> Generator {
    let sill = ridge_y - 0.15;
    let cy = sill + MON_H * 0.5;
    let mut parts = Vec::new();
    // End walls.
    for sx in [-1.0_f32, 1.0] {
        parts.push(clad(
            [0.24, MON_H, MON_D],
            [sx * (MON_W * 0.5 - 0.12), cy, 0.0],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
        ));
    }
    // Glazing both long sides, over a lit lining down the middle.
    for sz in [-1.0_f32, 1.0] {
        parts.push(glazing(
            (12, 1),
            [MON_W - 0.6, MON_H - 0.5],
            [0.0, cy, sz * (MON_D * 0.5 - 0.05)],
            quat_x(-FRAC_PI_2),
        ));
    }
    parts.push(prim(
        cuboid_tapered(
            [MON_W - 1.2, MON_H - 0.7, 0.5],
            0.0,
            lit_interior([0.52, 0.46, 0.34], 0.5),
        ),
        [0.0, cy, 0.0],
        id_quat(),
    ));
    // Its own pitched cap, oversailing on both sides.
    let cap = prim(
        solid(cuboid_tapered(
            [MON_W + 0.5, 0.24, MON_D + 0.9],
            0.0,
            tank_steel([0.4, 0.42, 0.44]),
        )),
        [0.0, sill + MON_H + 0.12, 0.0],
        id_quat(),
    );
    nest(cap, parts)
}

// --- The stack, the gantry and the yard. -----------------------------------

/// Stack height above the yard slab.
const STACK_H: f32 = 18.0;

fn stack_x() -> f32 {
    -W * 0.5 + 2.2
}
fn stack_z() -> f32 {
    D * 0.5 - 2.2
}

/// The brick smokestack: a tapered shaft on a square plinth, banded with
/// steel hoops and finished with a corbelled cap.
///
/// The hoops are `torus` rings rather than cuboids — a box of half-extent `r`
/// reaches `r·√2` at its corners, so a square band on a round shaft juts 40 %
/// past the brickwork it is supposed to hug, which is the kit's own
/// [`tank_hoops`](super::tank_hoops) note arrived at again.
fn stack() -> Generator {
    let x = stack_x();
    let z = stack_z();
    let base = YARD_H;
    let plinth_h = 1.6;
    let plinth = brick_slab(
        [3.3, plinth_h, 3.3],
        [x, base + plinth_h * 0.5, z],
        FaceKey::SideNz,
    );
    // One expression for the shaft's centre, feeding both the placement and
    // the course frame. Writing it twice is how a bonded surface silently
    // stops sharing the frame it is supposed to be in — which is exactly what
    // the guard below caught here.
    let shaft_c = [x, base + plinth_h + (STACK_H - plinth_h) * 0.5, z];
    let mut parts = vec![prim(
        solid(cylinder_tapered(
            1.35,
            STACK_H - plinth_h,
            16,
            0.24,
            bonded(BRICK_DARK, shaft_c, FaceKey::SideNz),
        )),
        shaft_c,
        id_quat(),
    )];
    // Corbelled cap, and a soot band under it.
    let cap_c = [x, base + STACK_H - 0.28, z];
    parts.push(prim(
        solid(cylinder_tapered(
            1.28,
            0.55,
            16,
            -0.12,
            bonded([0.3, 0.19, 0.16], cap_c, FaceKey::SideNz),
        )),
        cap_c,
        id_quat(),
    ));
    for f in [0.34_f32, 0.62, 0.88] {
        let y = base + plinth_h + (STACK_H - plinth_h) * f;
        let r = 1.35 * (1.0 - 0.24 * f) + 0.06;
        parts.push(prim(
            torus(0.08, r, tank_steel(PIPE_GREY)),
            [x, y, z],
            id_quat(),
        ));
    }
    nest(plinth, parts)
}

/// External process pipework climbing the `+X` gable — three risers, two
/// horizontal runs on brackets, and the elbow that turns onto the roof.
fn gantry() -> Generator {
    let gx = W * 0.5 + 0.55;
    let base = YARD_H;
    let mut parts = Vec::new();
    for pz in [-3.8_f32, 0.0, 3.8] {
        parts.push(prim(
            solid(cylinder_tapered(
                0.17,
                EAVES - 1.2,
                8,
                0.0,
                tank_steel(PIPE_GREY),
            )),
            [gx, base + (EAVES - 1.2) * 0.5, pz],
            id_quat(),
        ));
    }
    for (py, color) in [(3.6_f32, PIPE_OCHRE), (4.5, PIPE_GREY)] {
        parts.push(prim(
            solid(cylinder_tapered(0.21, D - 0.6, 12, 0.0, tank_steel(color))),
            [gx, base + py, 0.0],
            quat_x(FRAC_PI_2),
        ));
    }
    // Riser onto the roof, and a hand-wheel valve on the lower run.
    parts.push(prim(
        solid(cylinder_tapered(
            0.21,
            EAVES - 3.6,
            12,
            0.0,
            tank_steel(PIPE_OCHRE),
        )),
        [gx - 0.32, base + 3.6 + (EAVES - 3.6) * 0.5, 3.8],
        id_quat(),
    ));
    parts.push(super::valve_wheel(
        [gx - 0.5, base + 3.6, -1.9],
        util::quat_z(FRAC_PI_2),
        0.42,
        tank_steel([0.62, 0.2, 0.18]),
    ));

    // The bracket the run sits on is the sub-root, so the whole gantry moves
    // with the wall it is bolted to.
    let bracket = prim(
        solid(cuboid_tapered(
            [0.5, 0.3, D - 0.6],
            0.0,
            tank_steel([0.4, 0.42, 0.44]),
        )),
        [gx - 0.42, base + 2.9, 0.0],
        id_quat(),
    );
    nest(bracket, parts)
}

/// Yard markings and furniture at the dock: painted bay lines and a row of
/// bollards, both derived from the dock openings so they cannot drift off
/// them.
fn yard_marks() -> Generator {
    // A bay line runs from the dock face out to the kerb — both ends derived,
    // so it can neither stop short of the dock nor run off the concrete.
    let line_far = YARD_FRONT + 0.6;
    let line_len = FRONT - line_far;
    let mut parts = Vec::new();
    for &x in DOCK_X.iter() {
        for sx in [-1.0_f32, 1.0] {
            parts.push(prim(
                cuboid_tapered([0.16, 0.03, line_len], 0.0, glow([0.86, 0.86, 0.82], 0.0)),
                [
                    x + sx * (DOCK_W * 0.5 + 0.6),
                    YARD_H + 0.02,
                    (FRONT + line_far) * 0.5,
                ],
                id_quat(),
            ));
        }
    }
    for bx in [-12.0_f32, 12.0] {
        parts.push(prim(
            solid(cylinder_tapered(0.16, 1.0, 10, 0.06, glow(HAZARD, 0.0))),
            [bx, YARD_H + 0.5, FRONT - 1.4],
            id_quat(),
        ));
    }
    // The kerb the markings are painted on is the sub-root, and it sits on
    // the yard slab's *own* front edge. Measured off the building instead —
    // a tidy `FRONT - 6.6` — it landed 1.2 m past the end of the concrete it
    // was supposed to be cast into, which is the #972 lesson-8 failure and is
    // invisible unless a tile happens to look along that edge.
    let kerb = prim(
        solid(cuboid_tapered(
            [YARD_W - 1.0, 0.12, 0.34],
            0.0,
            concrete([0.62, 0.62, 0.6]),
        )),
        [0.0, YARD_H + 0.06, YARD_FRONT + 0.3],
        id_quat(),
    );
    nest(kerb, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_cards_do_not_overlap, assert_no_glazing_on_solids, assert_no_tilted_parents,
        assert_sanitize_stable, window_cards,
    };
    use crate::pds::{GeneratorKind, SovereignTextureConfig};

    fn walk(g: &Generator, at: [f32; 3], f: &mut dyn FnMut(&Generator, [f32; 3])) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        f(g, here);
        for c in &g.children {
            walk(c, here, f);
        }
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Factory.build(""), "factory");
    }

    #[test]
    fn has_lit_windows() {
        assert!(crate::catalogue::items::util::has_emissive(
            &Factory.build("")
        ));
    }

    #[test]
    fn glazed_surfaces_do_not_collide() {
        assert_cards_do_not_overlap(&Factory.build(""), "factory");
    }

    /// #972 lesson 1, as a prohibition: no *solid* wears a `Window` texture.
    /// The card counts above check what they find; this checks that nothing
    /// was found in the wrong place at all.
    #[test]
    fn no_glazing_lands_on_a_solid() {
        assert_no_glazing_on_solids(&Factory.build(""), "factory");
    }

    /// The standing ROTATED-ROOT gotcha, finally guarded: a tilted parent
    /// spins everything it carries, and the translation-only walks every other
    /// guard here uses would report those children where they were authored
    /// rather than where they render.
    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&Factory.build(""), "factory");
    }

    /// #972 lesson 1: every `Window` card sits on a `Plane` at `uv_scale` 1.0
    /// — one per clerestory bay plus the monitor's two sides. The works used
    /// to carry four of them as slabs stuck to a solid mass, where the
    /// generator's masked-away panes cut holes onto the cladding behind.
    #[test]
    fn every_opening_is_a_card_on_a_plane() {
        let mut cards = 0;
        walk(&Factory.build(""), [0.0; 3], &mut |g, _| {
            let is_plane = matches!(g.kind, GeneratorKind::Plane { .. });
            for m in crate::pds::material_finish::node_materials_mut(&mut g.kind.clone()) {
                if matches!(m.texture, SovereignTextureConfig::Window(_)) {
                    assert!(
                        is_plane,
                        "Window card must sit on a Plane, found {}",
                        g.kind.kind_tag()
                    );
                    assert_eq!(m.uv_scale.0, 1.0, "cards are clamp-to-edge");
                    cards += 1;
                }
            }
        });
        assert_eq!(
            cards,
            bay_edges().len() + 2,
            "one clerestory card per bay, plus both sides of the monitor"
        );
    }

    /// #972 lesson 7: cards lap their openings and stand clear of what they
    /// frame.
    #[test]
    fn cards_lap_their_openings() {
        for c in window_cards(&Factory.build("")) {
            // The monitor's cards are the pair up on the ridge.
            if c.center[1] > YARD_H + EAVES {
                continue;
            }
            assert!(
                c.center[2] < FRONT + 0.9,
                "a clerestory card at z {} has slid off the wall plane",
                c.center[2]
            );
        }
    }

    /// #972 lesson 2: the brickwork is laid flat, at a real brick, in one
    /// shared world course frame — so every brick surface's `uv_offset` must
    /// equal its own face's projection of its own position.
    #[test]
    fn brickwork_sits_in_the_shared_course_frame() {
        let mut checked = 0;
        walk(&Factory.build(""), [0.0; 3], &mut |g, at| {
            let mats = match &g.kind {
                GeneratorKind::Cuboid { material, .. } => material,
                GeneratorKind::Cylinder { material, .. } => material,
                _ => return,
            };
            if !matches!(mats.texture, SovereignTextureConfig::Brick(_)) {
                return;
            }
            let want: Vec<_> = [
                FaceKey::SideNz,
                FaceKey::SidePz,
                FaceKey::SideNx,
                FaceKey::SidePx,
            ]
            .into_iter()
            .map(|f| util::face_uv_offset(f, at).0)
            .collect();
            let got = mats.uv_offset.0;
            assert!(
                want.iter()
                    .any(|w| (w[0] - got[0]).abs() < 1e-3 && (w[1] - got[1]).abs() < 1e-3),
                "brickwork at {at:?} carries uv_offset {got:?}, which is no face's \
                 projection of its own position"
            );
            checked += 1;
        });
        assert!(checked >= 5, "only {checked} brick surfaces found");
    }

    /// The dock is a *dock*: the shop floor stands a lorry bed off the yard,
    /// and the personnel door beside it gets a flight down to the same yard.
    /// A door onto a 1.5 m drop is invisible in every contact-sheet angle.
    #[test]
    fn the_dock_is_raised_and_the_door_has_steps() {
        assert!(
            (1.1..1.8).contains(&FLOOR),
            "a {FLOOR} m dock floor is not a lorry bed"
        );
        let root = Factory.build("");
        let mut treads: Vec<f32> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            if (size.0[0] - 1.5).abs() < 1e-3 && size.0[2] < 0.4 && at[2] < FRONT {
                treads.push(at[1] + size.0[1] * 0.5);
            }
        });
        assert_eq!(
            treads.len(),
            3,
            "the personnel door has a three-step flight"
        );
        let top = treads.iter().cloned().fold(f32::MIN, f32::max);
        assert!(
            (top - (YARD_H + FLOOR)).abs() < 1e-3,
            "the top tread at {top} does not reach the floor at {}",
            YARD_H + FLOOR
        );
    }

    /// #972 lesson 10: the lit strip that says "this plant is working" hangs
    /// where the rolled drum crosses the sightline from the yard, so there is
    /// a second source *below* the opening's head. Without it the one element
    /// justifying the whole open bay is invisible from the ground.
    #[test]
    fn the_open_bay_carries_a_light_below_its_head() {
        let root = Factory.build("");
        let drum_y = YARD_H + FLOOR + DOCK_H - 0.45;
        let mut below = false;
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { material, .. } = &g.kind else {
                return;
            };
            if material.emission_strength.0 > 2.0
                && at[1] < drum_y - 0.8
                && at[2] > FRONT
                && (at[0] - DOCK_X[OPEN_DOCK]).abs() < 4.0
            {
                below = true;
            }
        });
        assert!(
            below,
            "every light in the open bay sits at or above the drum at {drum_y}"
        );
    }

    /// #972 lesson 8, both halves: the dock markings are derived from the
    /// dock openings, so a bay line can never end up painted across a pier —
    /// and everything standing on the yard stays inside the yard's own
    /// footprint. The kerb failed the second half when it was measured off the
    /// building instead of off the concrete it is cast into.
    #[test]
    fn the_yard_furniture_stays_on_the_yard() {
        let root = Factory.build("");
        let (yc, yh) = (
            [0.0, YARD_H * 0.5, YARD_Z],
            [YARD_W * 0.5, 0.0, YARD_D * 0.5],
        );
        let mut checked = 0;
        walk(
            &root.children[3],
            root.transform.translation.0,
            &mut |g, at| {
                let half = match &g.kind {
                    GeneratorKind::Cuboid { size, .. } => [size.0[0] * 0.5, 0.0, size.0[2] * 0.5],
                    GeneratorKind::Cylinder { radius, .. } => [radius.0, 0.0, radius.0],
                    _ => return,
                };
                for axis in [0usize, 2] {
                    assert!(
                        at[axis] - half[axis] > yc[axis] - yh[axis] - 1e-3
                            && at[axis] + half[axis] < yc[axis] + yh[axis] + 1e-3,
                        "yard furniture at {at:?} (half {half:?}) hangs off the slab \
                     centred {yc:?} (half {yh:?}) on axis {axis}"
                    );
                }
                checked += 1;
            },
        );
        assert!(
            checked >= 7,
            "only {checked} pieces of yard furniture found"
        );
    }

    /// The bay lines are derived from the dock openings.
    #[test]
    fn the_bay_lines_flank_their_own_docks() {
        let root = Factory.build("");
        let mut lines = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            if size.0[1] < 0.06 && size.0[0] < 0.2 {
                lines.push(at[0]);
            }
        });
        assert_eq!(lines.len(), DOCK_X.len() * 2, "two lines per dock");
        for &x in &lines {
            let nearest = DOCK_X
                .iter()
                .map(|d| (d - x).abs())
                .fold(f32::MAX, f32::min);
            assert!(
                (nearest - (DOCK_W * 0.5 + 0.6)).abs() < 1e-3,
                "a bay line at {x} is not derived from a dock edge"
            );
        }
    }

    /// The editability contract: dragging the ridge takes the roof, the
    /// monitor and the ducting; dragging the stack's plinth takes its shaft
    /// and hoops.
    #[test]
    fn subtrees_carry_what_they_hold_up() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        let root = Factory.build("");
        assert_eq!(
            root.children.len(),
            6,
            "yard carries shell, stack, gantry, marks, the footing and the stack plume"
        );
        let shell = &root.children[0];
        let ridge = shell
            .children
            .iter()
            .find(|c| c.children.len() >= 8)
            .expect("the ridge carries the roof");
        assert!(
            ridge.children.iter().any(|c| c.children.len() >= 4),
            "the monitor cap carries its walls and glazing"
        );
        assert!(
            root.children[1].children.len() >= 5,
            "the stack carries its shaft"
        );
        assert!(count(&root) > 90, "the works lost most of its parts");
    }
}
