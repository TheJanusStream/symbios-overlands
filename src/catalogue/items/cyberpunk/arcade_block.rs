//! Arcade block — a wide, low Cyberpunk secondary. A dark-metal entertainment
//! shell on a lit forecourt: two display windows with rows of cabinets burning
//! behind them, an open entrance under a wedge marquee, a clerestory over the
//! mezzanine, neon on the roof *edge* and a content-tile sign above it.
//!
//! Rebuilt as a shell under #972. What shipped was a black box with light
//! painted on:
//!
//! 1. **The "neon roofline" was the roof.** A 9.4 × 0.35 × 6.4 emissive slab at
//!    strength 6.0 — the broad-flat-panel gotcha in its purest form. It blooms
//!    to a flat white-pink lid that covers the entire plan, and in every tile of
//!    the shipped sheet it is the biggest thing on the prop. Neon is an *edge*.
//! 2. **The window bands were `Window`-textured cuboids** on the back and ends
//!    (#972 lesson 20) — and the front, the face the street sees, had no
//!    glazing at all. An arcade's whole subject is the machines glowing through
//!    the glass, and the hero elevation was a blank slab with a lit rectangle
//!    recessed into it.
//! 3. **Nothing to look at and no way in.** The "entrance" was a flat glow
//!    panel in a neon frame — no opening, no floor, no threshold, no interior —
//!    and there was no paving, so the prop stood on nothing.
//! 4. **Flat child list** with a hand-rolled `rel()` rebase (#972 lesson 3).
//!
//! Now the forecourt carries the block, the block frames three real openings
//! over an arcade floor of cabinets, and the neon runs where neon runs.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::catalogue::items::util::{
    self, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, lit_interior, nest, plane,
    prim, quat_x, quat_y, solid, wedge,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Generator, SovereignMaterialSettings, SovereignTextureConfig};
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, NEON_LIME, NEON_MAGENTA, concrete, fx, metal, pane_grid};

// --- Dimensions. Everything below derives from these. ----------------------

/// The forecourt, which is what everything outdoors is placed from.
const LOT_W: f32 = 13.0;
const LOT_D: f32 = 9.6;
const LOT_T: f32 = 0.3;
const LOT_TOP: f32 = LOT_T;
/// The lot reaches further in front of the block than behind it — the front is
/// where people queue.
const LOT_CZ: f32 = -0.7;

/// The block itself. `FRONT` is the `−Z` face the render tool and the
/// settlement placer both look down.
const W: f32 = 10.0;
const D: f32 = 6.4;
const FRONT: f32 = -D * 0.5;
const BACK: f32 = D * 0.5;
const WALL_T: f32 = 0.32;
const WALL_MID: f32 = FRONT + WALL_T * 0.5;

/// Plinth, ground storey, clerestory band, and the top.
const PLINTH_H: f32 = 0.34;
const FLOOR: f32 = LOT_TOP + PLINTH_H;
const GROUND_H: f32 = 3.7;
const GROUND_TOP: f32 = FLOOR + GROUND_H;
const CLEAR_H: f32 = 1.05;
const CLEAR_BOT: f32 = GROUND_TOP + 0.25;
const BLOCK_TOP: f32 = CLEAR_BOT + CLEAR_H + 0.35;

/// Glazing plane in the reveal, the arcade floor's rear lining, and the plane
/// proud trim stands on.
const GLAZE_Z: f32 = FRONT + WALL_T * 0.7;
const HALL_Z: f32 = FRONT + 3.4;
const TRIM_Z: f32 = FRONT - 0.06;
/// How far a card oversails its opening on every edge (#972 lesson 7).
const GLAZE_LAP: f32 = 0.06;

/// The three street-level openings: display, entrance, display.
const DISPLAY_W: f32 = 2.8;
const DISPLAY_SILL: f32 = 0.55;
const DISPLAY_H: f32 = 2.45;
const ENTRY_W: f32 = 2.6;
const ENTRY_H: f32 = 2.9;
const DISPLAY_X: f32 = 3.1;

/// Marquee over the entrance.
const MARQUEE_D: f32 = 1.5;
const MARQUEE_H: f32 = 0.5;

/// Rooftop sign.
const SIGN_W: f32 = 6.4;
const SIGN_H: f32 = 2.8;
const SIGN_Z: f32 = -1.2;

/// The strongest an emissive surface here is allowed to be broad.
///
/// Deep-saturated neon at low strength reads as neon; a *broad flat panel* at
/// strength blooms to a pale near-white slab whatever colour it started. The
/// shipped roofline was 9.4 × 6.4 m at 6.0 and read as a painted lid. Every
/// hot run here is a bar — one dimension long, the other two small — and the
/// guard below states that as a prohibition rather than trusting the numbers.
const NEON_HOT: f32 = 4.0;
const NEON_BAR: f32 = 0.2;

// --- Palette local to this entry. ------------------------------------------

/// Frame panels — a touch lighter than the kit's near-black body, so the piers
/// read as structure against the glass rather than merging with it.
const PANEL_GREY: [f32; 3] = [0.15, 0.16, 0.20];
/// The arcade floor's lining and its carpet.
const HALL_LINING: [f32; 3] = [0.20, 0.16, 0.30];
const CARPET_PLUM: [f32; 3] = [0.26, 0.10, 0.28];
/// A cabinet's body. Distinct from [`SIGN_BACK`] on purpose: the two were one
/// colour in the first draft and both of this entry's new guards picked the
/// wrong prims because of it — the cladding check flagged a cabinet and the
/// cabinet check flagged the sign (#972 lesson 24, twice in one file).
const CABINET_DARK: [f32; 3] = [0.11, 0.11, 0.15];
/// The rooftop sign's backing — darker still, so the lit tiles read against it.
const SIGN_BACK: [f32; 3] = [0.07, 0.07, 0.09];

// --- Shared construction. --------------------------------------------------

/// Dark metal in the world frame, so the panel seams run through a corner
/// instead of restarting at each slab's own centre.
fn panel(color: [f32; 3], center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    let mut m = metal(color);
    m.uv_offset = util::face_uv_offset(face, center);
    m
}

/// One clad slab of the shell. The centre is bound once and handed to the
/// material *and* the transform — passing a bonding helper a different reading
/// of "the middle of the wall" is the one way to defeat the frame guard
/// silently (#972 lesson 18).
fn wall(size: [f32; 3], center: [f32; 3], face: FaceKey) -> Generator {
    prim(
        solid(cuboid_tapered(size, 0.0, panel(PANEL_GREY, center, face))),
        center,
        id_quat(),
    )
}

/// Board-formed concrete in the world frame — the lot, the plinth, the kerbs.
fn paving(center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    let mut m = concrete([0.17, 0.18, 0.21]);
    m.uv_offset = util::face_uv_offset(face, center);
    m
}

/// A neon **bar**: long in one direction, small in the other two. There is no
/// helper for a neon panel, deliberately — see [`NEON_HOT`].
fn neon(size: [f32; 3], center: [f32; 3], color: [f32; 3], strength: f32) -> Generator {
    debug_assert!(
        strength < NEON_HOT || size.iter().filter(|d| **d > NEON_BAR).count() <= 1,
        "a hot emissive surface must be a bar, not a panel: {size:?} at {strength}"
    );
    prim(
        cuboid_tapered(size, 0.0, glow(color, strength)),
        center,
        id_quat(),
    )
}

/// Glazing opacity, deliberately **below** the `Mask(0.5)` cutoff the card
/// pipeline renders at.
///
/// The kit's [`window_wall`](super::window_wall) sits at exactly 0.5, which
/// keeps the pane — right for a megatower, whose windows have nothing behind
/// them but the shell. Here every card has an arcade floor behind it, and the
/// cabinets burning through the glass are the entire subject: at 0.5 the first
/// render of this rebuild put two flat teal sheets where eight machines were
/// standing. Below the cutoff the panes are discarded and the machines are what
/// you see (#972 lesson 25 — the opacity is a decision about what is behind the
/// opening, not a styling knob).
const GLASS_OPACITY: f32 = 0.34;

/// The kit's own grimy glass, re-cut to this opening's pane grid and opened up.
fn arcade_glass(panes: (u32, u32)) -> SovereignMaterialSettings {
    let mut m = pane_grid([0.14, 0.5, 0.6], 0.9, panes);
    if let SovereignTextureConfig::Window(cfg) = &mut m.texture {
        cfg.glass_opacity = crate::pds::Fp64((GLASS_OPACITY as f64 * 10000.0).round() / 10000.0);
    }
    m
}

/// Glazing filling one opening: a card on a flat quad in the reveal, lapped
/// into the frame either side.
fn glazing(size: [f32; 2], center: [f32; 3], panes: (u32, u32)) -> Generator {
    prim(
        plane(
            [size[0] + GLAZE_LAP, size[1] + GLAZE_LAP],
            arcade_glass(panes),
        ),
        center,
        quat_x(-FRAC_PI_2),
    )
}

/// A lit surface inside — what a card's masked-away panes actually show.
fn lit(size: [f32; 3], center: [f32; 3], color: [f32; 3], strength: f32) -> Generator {
    prim(
        cuboid_tapered(size, 0.0, lit_interior(color, strength)),
        center,
        id_quat(),
    )
}

/// The street-level openings, left to right, as `(centre_x, width, is_entry)`.
fn openings() -> [(f32, f32, bool); 3] {
    [
        (-DISPLAY_X, DISPLAY_W, false),
        (0.0, ENTRY_W, true),
        (DISPLAY_X, DISPLAY_W, false),
    ]
}

pub struct ArcadeBlock;

impl CatalogueEntry for ArcadeBlock {
    fn slug(&self) -> &'static str {
        "arcade_block"
    }
    fn name(&self) -> &'static str {
        "Arcade Block"
    }
    fn description(&self) -> &'static str {
        "Low neon entertainment block, cabinets burning behind the glass under a roof sign."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CYBER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 7.5,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The block as a tree that stands the way it does: the forecourt at the
/// bottom, the plinth on it, the shell on the plinth, the roof on the shell —
/// with the forecourt as a **sub-root that is the surface**, so everything set
/// down outside is checked against the paving rather than against the building
/// (#972 lesson 19).
fn build_tree() -> Generator {
    let center = [0.0, LOT_TOP - LOT_T * 0.5, LOT_CZ];
    let lot = prim(
        solid(cuboid_tapered(
            [LOT_W, LOT_T, LOT_D],
            0.0,
            paving(center, FaceKey::Top),
        )),
        center,
        id_quat(),
    );

    // Authored in the world frame like every other part here — `nest` rebases
    // it into the lot's local frame below.
    let base = footing(LOT_W, LOT_D, [0.0, LOT_CZ], 7.5);

    let mut parts = vec![base, block()];
    parts.extend(forecourt());
    nest(lot, parts)
}

/// What the forecourt carries: neon inlay strips in the paving, bollards along
/// the kerb line, and a stanchion pair marking the queue. Every one is placed
/// from the lot's own extent (#972 lesson 8).
fn forecourt() -> Vec<Generator> {
    let mut out = Vec::new();
    let z0 = FRONT - 0.4;
    let z1 = LOT_CZ - LOT_D * 0.5 + 0.4;
    let cz = (z0 + z1) * 0.5;
    for (i, sx) in [-1.0_f32, 1.0].iter().enumerate() {
        out.push(neon(
            [0.14, 0.03, (z0 - z1).abs()],
            [sx * 3.6, LOT_TOP + 0.015, cz],
            if i == 0 { NEON_CYAN } else { NEON_MAGENTA },
            2.2,
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        let bx = sx * (LOT_W * 0.5 - 0.7);
        out.push(prim(
            solid(cylinder_tapered(0.13, 0.95, 8, 0.1, metal(DARK_METAL))),
            [bx, LOT_TOP + 0.475, z1 + 0.6],
            id_quat(),
        ));
        out.push(neon(
            [0.28, 0.06, 0.28],
            [bx, LOT_TOP + 0.96, z1 + 0.6],
            NEON_CYAN,
            3.0,
        ));
        // Queue stanchion either side of the entrance.
        out.push(prim(
            solid(cylinder_tapered(0.05, 0.9, 8, 0.0, metal(PANEL_GREY))),
            [sx * (ENTRY_W * 0.5 + 0.5), LOT_TOP + 0.45, FRONT - 1.6],
            id_quat(),
        ));
    }
    out.push(neon(
        [ENTRY_W + 1.0, 0.04, 0.04],
        [0.0, LOT_TOP + 0.87, FRONT - 1.6],
        NEON_LIME,
        2.4,
    ));
    out
}

// --- The shell. ------------------------------------------------------------

/// The plinth — the block's sub-root, standing 60 mm proud of the cladding
/// above it, which is what a plinth actually is. Flush is a coplanar seam
/// running the whole perimeter and it is invisible in a still.
fn block() -> Generator {
    let center = [0.0, LOT_TOP + PLINTH_H * 0.5, 0.0];
    let plinth = prim(
        solid(cuboid_tapered(
            [W + 0.12, PLINTH_H, D + 0.12],
            0.0,
            paving(center, FaceKey::SideNz),
        )),
        center,
        id_quat(),
    );

    let mut parts = Vec::new();
    // Back and flanks, cut only by their own lit bands.
    parts.push(wall(
        [W, BLOCK_TOP - FLOOR, WALL_T],
        [0.0, (FLOOR + BLOCK_TOP) * 0.5, BACK - WALL_T * 0.5],
        FaceKey::SidePz,
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(wall(
            [WALL_T, BLOCK_TOP - FLOOR, D - WALL_T * 2.0],
            [
                sx * (W * 0.5 - WALL_T * 0.5),
                (FLOOR + BLOCK_TOP) * 0.5,
                0.0,
            ],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
        ));
    }
    elevation(&mut parts);
    flanks(&mut parts);
    parts.push(roof());
    nest(plinth, parts)
}

/// The street elevation: piers and spandrels framing two display windows and
/// the entrance, the arcade floor behind them, and the clerestory over it.
fn elevation(parts: &mut Vec<Generator>) {
    let ops = openings();

    // Piers, derived from the openings rather than authored beside them.
    let mut edges = vec![-W * 0.5];
    for (x, w, _) in &ops {
        edges.push(x - w * 0.5);
        edges.push(x + w * 0.5);
    }
    edges.push(W * 0.5);
    for i in (0..edges.len() - 1).step_by(2) {
        let (a, b) = (edges[i], edges[i + 1]);
        parts.push(wall(
            [b - a, BLOCK_TOP - FLOOR, WALL_T],
            [(a + b) * 0.5, (FLOOR + BLOCK_TOP) * 0.5, WALL_MID],
            FaceKey::SideNz,
        ));
    }

    // Spandrel between each opening and the clerestory over it, and a head band
    // above the clerestory. Running the spandrel all the way to the top — which
    // is what the first draft did — walls the clerestory in behind it, and the
    // sheet shows a black band where the lit lights should be.
    for (x, w, is_entry) in &ops {
        let head = FLOOR
            + if *is_entry {
                ENTRY_H
            } else {
                DISPLAY_SILL + DISPLAY_H
            };
        parts.push(wall(
            [*w, CLEAR_BOT - head, WALL_T],
            [*x, (head + CLEAR_BOT) * 0.5, WALL_MID],
            FaceKey::SideNz,
        ));
        let clear_top = CLEAR_BOT + CLEAR_H;
        parts.push(wall(
            [*w, BLOCK_TOP - clear_top, WALL_T],
            [*x, (clear_top + BLOCK_TOP) * 0.5, WALL_MID],
            FaceKey::SideNz,
        ));
        if !is_entry {
            parts.push(wall(
                [*w, DISPLAY_SILL, WALL_T],
                [*x, FLOOR + DISPLAY_SILL * 0.5, WALL_MID],
                FaceKey::SideNz,
            ));
        }
    }

    // The arcade floor, its lining and its ceiling wash — one room behind all
    // three openings, laid out bay by bay (#972 lesson 9).
    parts.push(lit(
        [W - 1.4, 0.1, D - 1.6],
        [0.0, FLOOR + 0.05, (FRONT + BACK) * 0.5 + 0.4],
        CARPET_PLUM,
        0.18,
    ));
    parts.push(lit(
        [W - 1.4, GROUND_H - 0.4, 0.1],
        [0.0, FLOOR + (GROUND_H - 0.4) * 0.5, HALL_Z],
        HALL_LINING,
        0.24,
    ));
    parts.push(neon(
        [W - 2.4, 0.1, 0.22],
        [0.0, GROUND_TOP - 0.5, FRONT + 1.9],
        NEON_MAGENTA,
        2.6,
    ));

    for (x, w, is_entry) in &ops {
        if *is_entry {
            entrance(*x, parts);
        } else {
            display(*x, *w, parts);
        }
    }

    // Clerestory over the mezzanine, in three lights matched to the bays below.
    let cy = CLEAR_BOT + CLEAR_H * 0.5;
    parts.push(lit(
        [W - 1.2, CLEAR_H + 0.5, 0.1],
        [0.0, cy, FRONT + 1.8],
        HALL_LINING,
        0.3,
    ));
    for (x, w, _) in &ops {
        parts.push(glazing([*w, CLEAR_H], [*x, cy, GLAZE_Z], (4, 1)));
        parts.push(neon(
            [*w - 0.4, 0.09, 0.12],
            [*x, cy + CLEAR_H * 0.5 - 0.2, FRONT + 1.1],
            NEON_CYAN,
            2.4,
        ));
    }
}

/// One display window: the glazing, and the row of cabinets burning behind it.
///
/// This is the whole point of the prop. An arcade seen from the street is a
/// wall of screens, and the shipped block had a black slab where they belong —
/// so the cabinets are real geometry with real lit screens, not a glow panel
/// standing in for them (the flat-lightbox gotcha).
fn display(bx: f32, bw: f32, parts: &mut Vec<Generator>) {
    let cy = FLOOR + DISPLAY_SILL + DISPLAY_H * 0.5;
    parts.push(glazing([bw, DISPLAY_H], [bx, cy, GLAZE_Z], (3, 2)));

    let cabs = 4;
    let pitch = (bw - 0.5) / cabs as f32;
    for i in 0..cabs {
        let x = bx + (i as f32 - (cabs - 1) as f32 * 0.5) * pitch;
        // Two ranks: the front one against the glass, the back one a metre in,
        // so the bay has depth rather than a single flat line of machines.
        let back = i % 2 == 1;
        let z = FRONT + if back { 2.1 } else { 1.05 };
        let h = 1.72;
        parts.push(prim(
            solid(cuboid_tapered([0.66, h, 0.6], 0.0, metal(CABINET_DARK))),
            [x, FLOOR + h * 0.5, z],
            id_quat(),
        ));
        // Screen, control deck and marquee strip — the three lit parts of a
        // cabinet, and none of them broad enough to bloom.
        parts.push(neon(
            [0.5, 0.44, 0.05],
            [x, FLOOR + 1.16, z - 0.32],
            [NEON_CYAN, NEON_LIME, NEON_MAGENTA][i % 3],
            2.6,
        ));
        parts.push(prim(
            solid(cuboid_tapered([0.62, 0.1, 0.3], 0.0, metal(PANEL_GREY))),
            [x, FLOOR + 0.86, z - 0.42],
            id_quat(),
        ));
        parts.push(neon(
            [0.56, 0.16, 0.05],
            [x, FLOOR + 1.58, z - 0.32],
            NEON_MAGENTA,
            2.2,
        ));
    }
}

/// The entrance: no glazing at all, a lit foyer, a token booth, and the wedge
/// marquee over it.
///
/// The right answer to "a card on a solid" is sometimes neither — an arcade
/// door stands open on a lit floor, so the opening is a genuine hole and the
/// alpha-card idiom never enters into it (#972, the boardwalk's lesson).
fn entrance(bx: f32, parts: &mut Vec<Generator>) {
    let head = FLOOR + ENTRY_H;
    // Reveal jambs and head, in the frame's own panel rather than raw glow.
    for sx in [-1.0_f32, 1.0] {
        parts.push(wall(
            [0.14, ENTRY_H, 1.1],
            [
                bx + sx * (ENTRY_W * 0.5 - 0.07),
                FLOOR + ENTRY_H * 0.5,
                FRONT + 0.55,
            ],
            FaceKey::SideNz,
        ));
    }
    // Hot neon round the opening — four bars, not a frame slab.
    for sy in [-1.0_f32, 1.0] {
        parts.push(neon(
            [ENTRY_W + 0.8, 0.18, 0.16],
            [
                bx,
                FLOOR + ENTRY_H * 0.5 + sy * (ENTRY_H * 0.5 + 0.2),
                TRIM_Z,
            ],
            NEON_MAGENTA,
            4.6,
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        parts.push(neon(
            [0.18, ENTRY_H + 0.58, 0.16],
            [
                bx + sx * (ENTRY_W * 0.5 + 0.31),
                FLOOR + ENTRY_H * 0.5,
                TRIM_Z,
            ],
            NEON_MAGENTA,
            4.6,
        ));
    }

    // Foyer: a lit floor strip running in, a token booth off the centreline
    // and a ceiling wash below the head, so the doorway is depth rather than a
    // black rectangle.
    parts.push(lit(
        [ENTRY_W - 0.3, 0.06, 2.6],
        [bx, FLOOR + 0.06, FRONT + 1.5],
        [0.42, 0.30, 0.48],
        0.34,
    ));
    parts.push(prim(
        solid(cuboid_tapered([1.05, 1.2, 0.7], 0.0, metal(CABINET_DARK))),
        [bx + 1.5, FLOOR + 0.6, FRONT + 2.3],
        id_quat(),
    ));
    parts.push(neon(
        [0.85, 0.3, 0.05],
        [bx + 1.5, FLOOR + 0.98, FRONT + 1.96],
        NEON_LIME,
        2.6,
    ));
    parts.push(neon(
        [ENTRY_W - 0.5, 0.08, 0.7],
        [bx, head - 0.42, FRONT + 1.3],
        [1.0, 0.72, 0.32],
        2.0,
    ));

    // Wedge marquee over the door. `quat_y(PI)` puts the thick edge against
    // the building and slopes it down to a thin front lip; a tilted node with
    // offset children would spin them out of the record and out of every
    // translation-only guard at once, so the lip is a sibling (#972 lesson 22).
    let soffit = head + 0.3;
    parts.push(prim(
        wedge([ENTRY_W + 2.2, MARQUEE_H, MARQUEE_D], metal(PANEL_GREY)),
        [bx, soffit, FRONT - MARQUEE_D * 0.5],
        quat_y(PI),
    ));
    parts.push(neon(
        [ENTRY_W + 2.2, 0.1, 0.1],
        [bx, soffit + 0.06, FRONT - MARQUEE_D + 0.05],
        NEON_CYAN,
        4.4,
    ));
    // Tie rods back to the wall, so the canopy is carried by something.
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cylinder_tapered(0.035, 1.1, 6, 0.0, metal(PANEL_GREY))),
            [
                bx + sx * (ENTRY_W * 0.5 + 0.9),
                soffit + 0.55,
                FRONT - MARQUEE_D * 0.45,
            ],
            quat_x(-0.55),
        ));
    }
}

/// Lit bands on the back and both flanks — real openings framed by the wall
/// they are cut out of, with something behind them.
fn flanks(parts: &mut Vec<Generator>) {
    let cy = FLOOR + GROUND_H * 0.55;
    for sx in [-1.0_f32, 1.0] {
        let x = sx * (W * 0.5 - WALL_T * 0.55);
        parts.push(prim(
            plane([4.4 + GLAZE_LAP, 0.9 + GLAZE_LAP], arcade_glass((6, 1))),
            [x, cy, 0.2],
            util::quat_mul(
                quat_y(if sx > 0.0 { -FRAC_PI_2 } else { FRAC_PI_2 }),
                quat_x(-FRAC_PI_2),
            ),
        ));
        parts.push(lit(
            [0.08, 1.3, 4.8],
            [sx * (W * 0.5 - WALL_T - 0.35), cy, 0.2],
            HALL_LINING,
            0.26,
        ));
    }
    for (i, y) in [cy - 0.7_f32, cy + 0.9].iter().enumerate() {
        parts.push(prim(
            plane([6.8 + GLAZE_LAP, 0.7 + GLAZE_LAP], arcade_glass((8, 1))),
            [0.0, *y, BACK - WALL_T * 0.55],
            quat_x(FRAC_PI_2),
        ));
        let _ = i;
    }
    parts.push(lit(
        [7.2, 2.6, 0.08],
        [0.0, cy + 0.1, BACK - WALL_T - 0.3],
        HALL_LINING,
        0.24,
    ));
}

// --- The roof. -------------------------------------------------------------

/// The roof deck, the neon that runs round its **edge**, the corner uprights,
/// the sign and the plant.
fn roof() -> Generator {
    let center = [0.0, BLOCK_TOP + 0.11, 0.0];
    let deck = prim(
        solid(cuboid_tapered(
            [W + 0.2, 0.22, D + 0.2],
            0.0,
            panel(PANEL_GREY, center, FaceKey::Top),
        )),
        center,
        id_quat(),
    );
    let top = BLOCK_TOP + 0.22;

    let mut parts = Vec::new();
    // Neon on the four edges. Four bars, not a lid.
    for sz in [-1.0_f32, 1.0] {
        parts.push(neon(
            [W + 0.36, 0.14, 0.16],
            [0.0, top - 0.05, sz * (D * 0.5 + 0.18)],
            NEON_MAGENTA,
            4.2,
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        parts.push(neon(
            [0.16, 0.14, D + 0.36],
            [sx * (W * 0.5 + 0.18), top - 0.05, 0.0],
            NEON_MAGENTA,
            4.2,
        ));
    }
    // Vertical accents down the four corners.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            parts.push(neon(
                [0.13, BLOCK_TOP - FLOOR - 0.4, 0.13],
                [
                    sx * (W * 0.5 + 0.03),
                    (FLOOR + BLOCK_TOP) * 0.5,
                    sz * (D * 0.5 + 0.03),
                ],
                NEON_CYAN,
                4.2,
            ));
        }
    }

    parts.push(sign(top));
    // Plant behind the sign.
    for (i, x) in [-3.6_f32, 3.4].iter().enumerate() {
        parts.push(prim(
            solid(cuboid_tapered([1.2, 0.7, 1.0], 0.0, metal(PANEL_GREY))),
            [*x, top + 0.35, 1.8 + i as f32 * 0.4],
            id_quat(),
        ));
    }
    parts.push(prim(
        solid(cylinder_tapered(0.3, 1.1, 10, 0.08, metal(DARK_METAL))),
        [1.2, top + 0.55, 2.4],
        id_quat(),
    ));
    nest(deck, parts)
}

/// The rooftop sign: two legs, a dark backing, four lit content tiles and a hot
/// frame of four bars.
fn sign(top: f32) -> Generator {
    let leg_h = 1.3;
    let sign_y = top + leg_h + SIGN_H * 0.5;
    let legs = prim(
        solid(cuboid_tapered([0.26, leg_h, 0.26], 0.0, metal(DARK_METAL))),
        [-2.4, top + leg_h * 0.5, SIGN_Z],
        id_quat(),
    );

    let mut parts = vec![prim(
        solid(cuboid_tapered([0.26, leg_h, 0.26], 0.0, metal(DARK_METAL))),
        [2.4, top + leg_h * 0.5, SIGN_Z],
        id_quat(),
    )];
    let mut backing = prim(
        solid(cuboid_tapered(
            [SIGN_W, SIGN_H, 0.25],
            0.0,
            metal(SIGN_BACK),
        )),
        [0.0, sign_y, SIGN_Z],
        id_quat(),
    );
    backing.audio = fx::neon_buzz();
    parts.push(backing);

    let tiles = [NEON_CYAN, NEON_MAGENTA, NEON_LIME, NEON_CYAN];
    for (i, c) in tiles.into_iter().enumerate() {
        parts.push(prim(
            cuboid_tapered([1.3, 2.0, 0.06], 0.0, glow(c, 2.0 + 0.1 * i as f32)),
            [-2.25 + 1.5 * i as f32, sign_y, SIGN_Z - 0.16],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        parts.push(neon(
            [SIGN_W + 0.3, 0.18, 0.18],
            [0.0, sign_y + sy * (SIGN_H * 0.5 + 0.09), SIGN_Z - 0.16],
            NEON_MAGENTA,
            4.6,
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        parts.push(neon(
            [0.18, SIGN_H + 0.36, 0.18],
            [sx * (SIGN_W * 0.5 + 0.09), sign_y, SIGN_Z - 0.16],
            NEON_MAGENTA,
            4.6,
        ));
    }
    nest(legs, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_cards_do_not_overlap, assert_no_glazing_on_solids, assert_no_tilted_parents,
        assert_sanitize_stable, has_emissive,
    };
    use crate::pds::GeneratorKind;

    fn walk(g: &Generator, at: [f32; 3], f: &mut dyn FnMut(&Generator, [f32; 3])) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        f(g, here);
        for c in &g.children {
            walk(c, here, f);
        }
    }

    fn footprint(g: &Generator) -> Option<(f32, f32, f32)> {
        if g.transform.rotation.0 != [0.0, 0.0, 0.0, 1.0] {
            return None;
        }
        match &g.kind {
            GeneratorKind::Cuboid { size, .. } => {
                Some((size.0[0] * 0.5, size.0[1] * 0.5, size.0[2] * 0.5))
            }
            GeneratorKind::Cylinder { radius, height, .. } => {
                Some((radius.0, height.0 * 0.5, radius.0))
            }
            _ => None,
        }
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ArcadeBlock.build(""), "arcade_block");
    }

    #[test]
    fn no_glazing_lands_on_a_solid() {
        assert_no_glazing_on_solids(&ArcadeBlock.build(""), "arcade_block");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&ArcadeBlock.build(""), "arcade_block");
    }

    #[test]
    fn glazed_surfaces_do_not_collide() {
        assert_cards_do_not_overlap(&ArcadeBlock.build(""), "arcade_block");
    }

    #[test]
    fn has_neon() {
        assert!(has_emissive(&ArcadeBlock.build("")));
    }

    /// **Neon is an edge.**
    ///
    /// The one guard this entry exists to carry. A deep-saturated colour driven
    /// hot reads as neon only while it is a *bar*: give it two large dimensions
    /// and it blooms to a pale near-white slab whatever colour it started, which
    /// is what the shipped 9.4 × 6.4 m "roofline trim" did at strength 6.0 — it
    /// was the biggest surface on the prop in every tile of the sheet.
    ///
    /// Stated as a prohibition rather than as a census (#972 lesson 20): count
    /// the neon and you pass happily on a lit lid, because the lid *is* one of
    /// the things you counted.
    #[test]
    fn no_hot_emissive_surface_is_a_panel() {
        let mut bars = 0;
        walk(&ArcadeBlock.build(""), [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, material, .. } = &g.kind else {
                return;
            };
            if material.emission_strength.0 < NEON_HOT {
                return;
            }
            bars += 1;
            let broad = size.0.iter().filter(|d| **d > NEON_BAR).count();
            assert!(
                broad <= 1,
                "arcade_block: a {:?} emissive surface at {at:?} runs at {} — {broad} of \
                 its dimensions are over {NEON_BAR} m, so it is a panel, not a bar, and \
                 it blooms to white",
                size.0,
                material.emission_strength.0
            );
        });
        assert!(bars >= 12, "only {bars} hot neon runs — the block is dark");
    }

    /// #972 lesson 1: every pane is a card on a flat quad at `uv_scale` 1.0 —
    /// two display windows, three clerestory lights, two flank bands and two
    /// back bands. The entrance has none, because a way in is a hole.
    #[test]
    fn every_opening_is_a_card_on_a_quad() {
        let mut cards = 0;
        walk(&ArcadeBlock.build(""), [0.0; 3], &mut |g, _| {
            let is_plane = matches!(g.kind, GeneratorKind::Plane { .. });
            for m in crate::pds::material_finish::node_materials_mut(&mut g.kind.clone()) {
                if matches!(m.texture, SovereignTextureConfig::Window(_)) {
                    assert!(is_plane, "a Window card must sit on a Plane");
                    assert_eq!(m.uv_scale.0, 1.0, "cards are clamp-to-edge");
                    cards += 1;
                }
            }
        });
        assert_eq!(cards, 9, "2 displays + 3 clerestory + 2 flanks + 2 back");
    }

    /// #972 lesson 7: each street-level card oversails the opening its piers
    /// leave it, checked against [`openings`] — which is where the piers and
    /// spandrels come from too.
    #[test]
    fn every_card_laps_its_opening() {
        let mut widths: Vec<f32> = Vec::new();
        walk(&ArcadeBlock.build(""), [0.0; 3], &mut |g, _| {
            if let GeneratorKind::Plane { size, material, .. } = &g.kind
                && matches!(material.texture, SovereignTextureConfig::Window(_))
            {
                widths.push(size.0[0]);
            }
        });
        for (_, w, is_entry) in openings() {
            if is_entry {
                continue;
            }
            assert!(
                widths.iter().any(|c| (c - w - GLAZE_LAP).abs() < 1e-4),
                "no card laps the {w} m opening"
            );
            assert!(
                !widths.iter().any(|c| (c - w).abs() < 1e-4),
                "a card sized exactly to a {w} m opening ties with its own reveal"
            );
        }
    }

    /// The display bays have **machines** behind the glass, not a glow panel
    /// standing in for them — and every cabinet stands on the arcade floor
    /// inside the shell that encloses it.
    #[test]
    fn the_cabinets_are_real_and_stand_on_the_floor() {
        let mut cabs = 0;
        let mut screens = 0;
        walk(&ArcadeBlock.build(""), [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, material, .. } = &g.kind else {
                return;
            };
            if material.base_color.0 == CABINET_DARK && size.0[0] < 1.0 && size.0[1] > 1.0 {
                cabs += 1;
                assert!(
                    (at[1] - size.0[1] * 0.5 - FLOOR).abs() < 1e-3,
                    "a cabinet at {at:?} does not stand on the arcade floor at {FLOOR}"
                );
                assert!(
                    at[0].abs() + size.0[0] * 0.5 <= W * 0.5 - WALL_T
                        && at[2] + size.0[2] * 0.5 <= BACK - WALL_T
                        && at[2] - size.0[2] * 0.5 >= FRONT,
                    "a cabinet at {at:?} is outside the shell that encloses it"
                );
            }
            // A screen is a small lit *panel*: bounded on all three axes. An
            // unbounded height caught every vertical neon accent on the prop
            // and reported sixteen screens for eight cabinets — the selector,
            // again (#972 lesson 24).
            if material.emission_strength.0 > 2.0
                && size.0[0] < 0.6
                && (0.3..0.6).contains(&size.0[1])
                && size.0[2] < 0.1
            {
                screens += 1;
            }
        });
        assert_eq!(cabs, 8, "four cabinets in each display bay");
        assert_eq!(screens, 8, "one lit screen per cabinet");
    }

    /// #972 lesson 18: every clad and paved slab's `uv_offset` is some face's
    /// projection of the position the **built tree** puts it at — read from the
    /// composed translation, not from the constants the placement used.
    #[test]
    fn every_clad_surface_shares_one_world_frame() {
        use FaceKey::*;
        let mut checked = 0;
        walk(&ArcadeBlock.build(""), [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, material, .. } = &g.kind else {
                return;
            };
            if g.transform.rotation.0 != [0.0, 0.0, 0.0, 1.0] {
                return;
            }
            // Select by what *defines* a clad surface here: the shell's own
            // panel colour, or paving. A dimension test alone caught cabinets
            // and the sign backing, neither of which is cladding and neither of
            // which is bonded (#972 lesson 24).
            let clad = material.base_color.0 == PANEL_GREY
                || matches!(material.texture, SovereignTextureConfig::Concrete(_));
            let mut dims = size.0;
            dims.sort_by(|a, b| a.partial_cmp(b).unwrap());
            if !clad || dims[2] < 1.5 || dims[1] < 0.5 || material.emission_strength.0 > 0.0 {
                return;
            }
            checked += 1;
            let agrees = [SideNz, SidePz, SideNx, SidePx, Top, Bottom]
                .into_iter()
                .any(|f| {
                    let o = util::face_uv_offset(f, at).0;
                    (o[0] - material.uv_offset.0[0]).abs() < 2e-3
                        && (o[1] - material.uv_offset.0[1]).abs() < 2e-3
                });
            assert!(
                agrees,
                "arcade_block: a clad slab at {at:?} carries uv_offset {:?}, which is no \
                 face's projection of where the built tree puts it",
                material.uv_offset.0
            );
        });
        assert!(
            checked >= 10,
            "only {checked} clad surfaces found — suspect the selector before the content"
        );
    }

    /// #972 lesson 8: everything standing on the forecourt has its footprint
    /// inside the forecourt's.
    #[test]
    fn everything_standing_on_the_lot_is_on_it() {
        let mut checked = 0;
        walk(&ArcadeBlock.build(""), [0.0; 3], &mut |g, at| {
            let Some((hx, hy, hz)) = footprint(g) else {
                return;
            };
            if (at[1] - hy - LOT_TOP).abs() > 0.03 {
                return;
            }
            checked += 1;
            assert!(
                at[0].abs() + hx <= LOT_W * 0.5 + 1e-3
                    && (at[2] - LOT_CZ).abs() + hz <= LOT_D * 0.5 + 1e-3,
                "arcade_block: a part at {at:?} (half {hx} × {hz}) stands on the forecourt \
                 and hangs off it"
            );
        });
        assert!(checked >= 5, "only {checked} parts stand on the forecourt");
    }

    /// The marquee covers the entrance it shelters and clears its head, and it
    /// is carried by tie rods that reach it. Stated as the relationships rather
    /// than the numbers, so a taller door cannot leave it short.
    #[test]
    fn the_marquee_covers_the_entrance_and_clears_its_head() {
        let root = ArcadeBlock.build("");
        let mut marquee: Option<([f32; 3], [f32; 3])> = None;
        let mut rods = 0;
        walk(&root, [0.0; 3], &mut |g, at| match &g.kind {
            GeneratorKind::Wedge { size, .. } => marquee = Some((at, size.0)),
            GeneratorKind::Cylinder { height, radius, .. }
                if (height.0 - 1.1).abs() < 1e-4 && radius.0 < 0.05 =>
            {
                rods += 1
            }
            _ => {}
        });
        let (at, size) = marquee.expect("no marquee over the entrance");
        assert!(
            at[1] - size[1] * 0.5 > FLOOR + ENTRY_H,
            "the marquee's underside at {} crosses the door head at {}",
            at[1] - size[1] * 0.5,
            FLOOR + ENTRY_H
        );
        assert!(
            size[0] >= ENTRY_W + 1.0,
            "a {} m marquee barely covers a {ENTRY_W} m entrance",
            size[0]
        );
        assert!(
            at[2] - size[2] * 0.5 < FRONT - 0.5,
            "the marquee projects only to {} and shelters nothing",
            at[2] - size[2] * 0.5
        );
        assert_eq!(rods, 2, "two tie rods carry the marquee");
    }

    /// The editability contract (#972 lesson 3): the forecourt carries the
    /// plinth, the plinth carries the shell and the roof, the roof carries the
    /// sign. Sub-roots selected by the property that defines them.
    #[test]
    fn subtrees_carry_what_they_hold_up() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        let root = ArcadeBlock.build("");
        let sized = |g: &Generator, want: [f32; 3]| match &g.kind {
            GeneratorKind::Cuboid { size, .. } => size
                .0
                .iter()
                .zip(want.iter())
                .all(|(a, b)| (a - b).abs() < 1e-3),
            _ => false,
        };
        let plinth = root
            .children
            .iter()
            .find(|c| sized(c, [W + 0.12, PLINTH_H, D + 0.12]))
            .expect("the forecourt carries the plinth");
        let roof = plinth
            .children
            .iter()
            .find(|c| sized(c, [W + 0.2, 0.22, D + 0.2]))
            .expect("the plinth carries the roof deck");
        assert!(
            roof.children.iter().any(|c| c.children.len() >= 10),
            "the roof carries a sign that carries its tiles and frame"
        );
        assert!(count(&root) > 100, "the block lost most of its parts");
    }
}
