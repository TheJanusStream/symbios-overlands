//! Lifeguard tower — a Coastal-Resort secondary. A plank lookout cabin
//! hoisted on four braced posts above the sand, its wide observation window
//! open on a manned station, with a red rescue cross on the flank, a warm eave
//! lamp, a ring buoy on the rail and a pennant on the roof. A boarding ramp
//! runs down to the beach.
//!
//! Rebuilt under #972. Three faults, and the middle one is the interesting
//! kind:
//!
//! 1. **The window was a slab on a solid box.** A `Window`-textured cuboid
//!    pinned to a plank cabin — the generator masks its panes away, so it was
//!    a frame with holes onto the planking behind it, with nothing to see
//!    through it. It is now a card in a real opening over a lit station.
//! 2. **The ramp did not reach the deck.** It was placed by eye at a round
//!    `y = deck / 2` and tilted 0.95 rad, which put its head 0.38 m *under*
//!    the deck it is supposed to land on and its foot 0.22 m above the sand.
//!    Nothing in a contact sheet shows that; the ramp reads as a ramp from
//!    every angle. Its length and pitch are now derived from the rise it has
//!    to climb, and a guard checks both ends against the things they meet.
//! 3. **The guardrail was three bars floating in mid-air.** No posts, no
//!    balusters, and nothing holding them up. It is now
//!    [`util::railing`], which is the
//!    shared answer to the same mistake made on four props.
//!
//! [`util::railing`]: crate::catalogue::items::util::railing

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    self, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, footing, glow, id_quat,
    lit_interior, nest, plane, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::pds::generator::FaceKey;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    AWNING_RED, AWNING_WHITE, BUOY_RED, DECK_PALE, DECK_WOOD, GLASS_AQUA, LAMP_WARM, STEEL_GREY,
    canvas, enamel, pane_grid, plank, steel,
};

// --- Dimensions. Everything below derives from these. ----------------------

/// Deck plan and how high the posts carry it above the sand.
const DECK_W: f32 = 3.2;
const DECK_D: f32 = 3.2;
const DECK_Y: f32 = 2.9;
const DECK_T: f32 = 0.3;
/// Top of the deck boards — the floor level, and the datum for everything
/// above.
const FLOOR: f32 = DECK_Y + DECK_T * 0.5;

/// Cabin plan and wall height. Held inside the deck on every side, so no wall
/// face is ever coplanar with a deck edge.
const CAB_W: f32 = 2.8;
const CAB_D: f32 = 2.2;
const CAB_H: f32 = 1.9;
const WALL_T: f32 = 0.14;
/// Cabin centre in Z — set back, leaving the standing platform in front.
const CAB_Z: f32 = 0.42;
const CAB_TOP: f32 = FLOOR + CAB_H;

/// Outer face of the seaward wall — the `-Z` hero direction the render tool
/// and the settlement placer both look down.
const FRONT: f32 = CAB_Z - CAB_D * 0.5;
const FRONT_MID: f32 = FRONT + WALL_T * 0.5;
/// Glazing plane and the surface behind it, inside the reveal.
const GLAZE_Z: f32 = FRONT + 0.08;
const ROOM_Z: f32 = FRONT + 0.55;

/// The observation opening: wide, and low enough to see the water over.
const WIN_W: f32 = 2.2;
const WIN_H: f32 = 1.0;
const WIN_SILL: f32 = 0.55;

/// Roof rise across the cabin and how far it oversails.
const ROOF_FALL: f32 = 0.42;
const ROOF_OVER: f32 = 0.4;

/// Guardrail height above the deck.
const RAIL_H: f32 = 0.95;
/// Clear width of the gap in the front rail the ramp lands in. Derived from
/// the ramp so the two cannot drift apart.
const RAMP_W: f32 = 1.1;

// --- Shared construction. --------------------------------------------------

/// One boarded slab of the cabin, laid in the shared world course frame. The
/// position drives both the placement and the UV frame, so the two cannot
/// drift apart.
fn wall(size: [f32; 3], center: [f32; 3], face: FaceKey) -> Generator {
    prim(
        solid(cuboid_tapered(
            size,
            0.0,
            util::bonded_siding(plank(DECK_PALE), face, center),
        )),
        center,
        id_quat(),
    )
}

/// Board thickness of the boarding ramp, and its **pitch**.
///
/// The pitch is the authored quantity and the length is derived from it —
/// deliberately that way round. A ramp's length is whatever it has to be to
/// climb the rise at a walkable angle, and picking the length instead (which
/// is what the shipped version did) leaves the angle to fall out at whatever
/// the numbers happen to give: 44°, which is a ladder.
const RAMP_T: f32 = 0.16;
const RAMP_PITCH: f32 = 0.63;

pub struct LifeguardTower;

impl CatalogueEntry for LifeguardTower {
    fn slug(&self) -> &'static str {
        "lifeguard_tower"
    }
    fn name(&self) -> &'static str {
        "Lifeguard Tower"
    }
    fn description(&self) -> &'static str {
        "Raised plank lookout with a manned station, rescue cross and rooftop pennant."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.5,
            min_spawn_dist: 28.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The tower as a tree that stands the way it does: a post at the bottom
/// carrying the other three, the deck on them, the cabin on the deck, the
/// roof on the cabin — with the ramp its own sub-assembly off the deck.
///
/// Written outermost-last, because [`nest`] rebases a subtree that already
/// carries its own world translation.
fn build_tree() -> Generator {
    let (px, pz) = (DECK_W * 0.5 - 0.4, DECK_D * 0.5 - 0.4);
    let mut parts = Vec::new();
    for (sx, sz) in [(1.0_f32, -1.0_f32), (1.0, 1.0), (-1.0, 1.0)] {
        parts.push(prim(
            solid(cylinder_tapered(0.15, DECK_Y, 8, 0.08, plank(DECK_WOOD))),
            [sx * px, DECK_Y * 0.5, sz * pz],
            id_quat(),
        ));
    }
    // Cross-bracing between the posts, on the two faces the beach sees.
    for sz in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered([px * 2.0, 0.1, 0.1], 0.0, plank(DECK_WOOD))),
            [0.0, DECK_Y * 0.55, sz * pz],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered([0.1, 0.1, pz * 2.0], 0.0, plank(DECK_WOOD))),
            [sx * px, DECK_Y * 0.55, 0.0],
            id_quat(),
        ));
    }
    parts.push(deck());
    // Buried footing under the deck's own plan, sized to the drop this
    // footprint spans (#1009): the posts stand on it, and it closes the gap a
    // terrain-snapped placement leaves under the downhill edge. Authored
    // around y=0 and rebased into the root post's frame by `nest`.
    parts.push(footing(DECK_W, DECK_D, [0.0, 0.0], 5.5));

    let root = prim(
        solid(cylinder_tapered(0.15, DECK_Y, 8, 0.08, plank(DECK_WOOD))),
        [-px, DECK_Y * 0.5, -pz],
        id_quat(),
    );
    nest(root, parts)
}

/// The deck, and on it the cabin, the guardrail and the ramp.
fn deck() -> Generator {
    let center = [0.0, DECK_Y, 0.0];
    let deck = prim(
        solid(cuboid_tapered(
            [DECK_W, DECK_T, DECK_D],
            0.0,
            util::bonded_siding(plank(DECK_PALE), FaceKey::Top, center),
        )),
        center,
        id_quat(),
    );

    let mut parts = vec![cabin(), ramp()];

    // Guardrail: full runs down both flanks and along the front, the front
    // split around the ramp's own width so the gap and the ramp cannot drift
    // apart. The back edge is closed by the cabin.
    let (hx, hz) = (DECK_W * 0.5 - 0.1, DECK_D * 0.5 - 0.1);
    let rail = |a: [f32; 3], b: [f32; 3]| util::railing(a, b, RAIL_H, 0.34, steel(STEEL_GREY));
    for sx in [-1.0_f32, 1.0] {
        parts.extend(rail([sx * hx, FLOOR, -hz], [sx * hx, FLOOR, hz]));
        parts.extend(rail([sx * hx, FLOOR, -hz], [sx * RAMP_W * 0.5, FLOOR, -hz]));
    }
    // A ring buoy hung on the front rail, facing the shore.
    parts.push(prim(
        torus(0.07, 0.26, enamel(BUOY_RED)),
        [-1.0, FLOOR + RAIL_H * 0.55, -hz - 0.1],
        quat_x(FRAC_PI_2),
    ));

    nest(deck, parts)
}

/// The cabin: the boarding that *frames* the observation opening, the card
/// that fills it, and the lit station behind it.
fn cabin() -> Generator {
    let mut parts = Vec::new();
    let mid_y = FLOOR + CAB_H * 0.5;
    let inner_d = CAB_D - WALL_T * 2.0;

    // Back and side walls — solid; only the seaward face is cut.
    parts.push(wall(
        [CAB_W, CAB_H, WALL_T],
        [0.0, mid_y, CAB_Z + CAB_D * 0.5 - WALL_T * 0.5],
        FaceKey::SidePz,
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(wall(
            [WALL_T, CAB_H, inner_d],
            [sx * (CAB_W * 0.5 - WALL_T * 0.5), mid_y, CAB_Z],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
        ));
    }

    // The seaward face: two piers, a sill wall and a head band framing the
    // opening.
    let (wa, wb) = (-WIN_W * 0.5, WIN_W * 0.5);
    let head = WIN_SILL + WIN_H;
    for (a, b) in [(-CAB_W * 0.5, wa), (wb, CAB_W * 0.5)] {
        parts.push(wall(
            [b - a, CAB_H, WALL_T],
            [(a + b) * 0.5, mid_y, FRONT_MID],
            FaceKey::SideNz,
        ));
    }
    parts.push(wall(
        [WIN_W, WIN_SILL, WALL_T],
        [0.0, FLOOR + WIN_SILL * 0.5, FRONT_MID],
        FaceKey::SideNz,
    ));
    parts.push(wall(
        [WIN_W, CAB_H - head, WALL_T],
        [0.0, FLOOR + (head + CAB_H) * 0.5, FRONT_MID],
        FaceKey::SideNz,
    ));

    let cy = FLOOR + WIN_SILL + WIN_H * 0.5;
    parts.push(prim(
        plane(
            [WIN_W + 0.06, WIN_H + 0.06],
            pane_grid(GLASS_AQUA, 0.0, (4, 1)),
        ),
        [0.0, cy, GLAZE_Z],
        quat_x(-FRAC_PI_2),
    ));
    station(&mut parts, cy);

    // Red rescue cross on the +X flank, standing proud of the boarding.
    for size in [[0.05_f32, 0.9, 0.28], [0.05, 0.28, 0.9]] {
        parts.push(prim(
            cuboid_tapered(size, 0.0, enamel(BUOY_RED)),
            [CAB_W * 0.5 + 0.03, mid_y, CAB_Z],
            id_quat(),
        ));
    }
    // Warm lamp under the front eave — the tower's emissive trim. A small
    // lens in a housing, because a broad panel at strength blooms white.
    parts.push(prim(
        solid(cuboid_tapered(
            [0.22, 0.2, 0.14],
            0.0,
            steel([0.3, 0.3, 0.32]),
        )),
        [0.85, CAB_TOP - 0.16, FRONT - 0.07],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.14, 0.12, 0.05], 0.0, glow(LAMP_WARM, 2.5)),
        [0.85, CAB_TOP - 0.2, FRONT - 0.14],
        id_quat(),
    ));

    parts.push(roof());

    let floor = prim(
        cuboid_tapered(
            [CAB_W - WALL_T * 2.0, 0.06, inner_d],
            0.0,
            lit_interior([0.36, 0.31, 0.24], 0.14),
        ),
        [0.0, FLOOR + 0.03, CAB_Z],
        id_quat(),
    );
    nest(floor, parts)
}

/// What the opening frames: a lit back lining, a desk across the window and a
/// pair of binoculars on it.
///
/// Depth discipline (#972 lesson 6) in a cabin two metres deep: the lining is
/// held just behind the glass, and the desk sits at sill height so the
/// silhouette reads through the card's cut panes rather than under them.
fn station(parts: &mut Vec<Generator>, window_cy: f32) {
    parts.push(prim(
        cuboid_tapered(
            [CAB_W - 0.4, CAB_H - 0.2, 0.08],
            0.0,
            lit_interior([0.52, 0.44, 0.32], 0.36),
        ),
        [0.0, FLOOR + CAB_H * 0.5, ROOM_Z + 0.35],
        id_quat(),
    ));
    parts.push(prim(
        solid(cuboid_tapered(
            [WIN_W - 0.2, 0.1, 0.42],
            0.0,
            lit_interior([0.44, 0.36, 0.26], 0.24),
        )),
        [0.0, FLOOR + WIN_SILL + 0.05, ROOM_Z],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cylinder_tapered(
                0.06,
                0.22,
                8,
                0.0,
                lit_interior([0.2, 0.2, 0.22], 0.2),
            )),
            [0.42 + sx * 0.08, window_cy - 0.06, ROOM_Z - 0.05],
            quat_x(FRAC_PI_2),
        ));
    }
}

/// The shed roof, pitched down toward the water, with a fascia along its low
/// edge and the pennant on its high one.
fn roof() -> Generator {
    let pitch = ROOF_FALL.atan2(CAB_D + ROOF_OVER * 2.0);
    let span = (CAB_D + ROOF_OVER * 2.0).hypot(ROOF_FALL);
    let center = [0.0, CAB_TOP + 0.14, CAB_Z];
    let mut parts = vec![prim(
        solid(cuboid_tapered_xz(
            [CAB_W + ROOF_OVER * 2.0, 0.18, span],
            [0.0, 0.0],
            util::bonded_siding(plank(DECK_WOOD), FaceKey::Top, center),
        )),
        center,
        quat_x(-pitch),
    )];
    // Fascia closing the low edge, so the roof reads as a board with a
    // thickness rather than as a floating plane.
    parts.push(prim(
        solid(cuboid_tapered(
            [CAB_W + ROOF_OVER * 2.0, 0.14, 0.08],
            0.0,
            plank(DECK_WOOD),
        )),
        [
            0.0,
            CAB_TOP + 0.14 - ROOF_FALL * 0.5 - 0.06,
            CAB_Z - CAB_D * 0.5 - ROOF_OVER,
        ],
        id_quat(),
    ));
    // Pennant: a short pole on the high edge with a tapered flag, so it reads
    // as cloth rather than as a rectangle nailed to a stick.
    let pole_y = CAB_TOP + 0.2 + ROOF_FALL * 0.5;
    parts.push(prim(
        solid(cylinder_tapered(0.045, 1.3, 6, 0.0, steel(STEEL_GREY))),
        [1.0, pole_y + 0.65, CAB_Z + CAB_D * 0.5 - 0.1],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered_xz(
            [0.85, 0.5, 0.03],
            [0.7, 0.0],
            canvas(AWNING_RED, AWNING_WHITE),
        ),
        [1.44, pole_y + 1.0, CAB_D * 0.5 + CAB_Z - 0.1],
        id_quat(),
    ));
    // The sub-root is the flat wall plate the roof lands on, not the sloping
    // board: a tilted sub-root spins everything nested under it, and the
    // fascia and the pennant are both offset from it.
    let plate = prim(
        solid(cuboid_tapered([CAB_W, 0.1, CAB_D], 0.0, plank(DECK_WOOD))),
        [0.0, CAB_TOP - 0.05, CAB_Z],
        id_quat(),
    );
    nest(plate, parts)
}

/// The boarding ramp down to the sand.
///
/// Every number here is **derived from the rise it has to climb**. The
/// original was placed at `y = DECK_Y * 0.5` with a hand-picked 0.95 rad tilt
/// and a hand-picked 3.4 m length, and the three did not agree: its head
/// landed 0.38 m *under* the deck and its foot floated 0.22 m over the sand.
/// A ramp reads as a ramp from every angle in a contact sheet whether or not
/// it touches anything at either end, which is why this is a guard's job
/// (`the_ramp_meets_the_deck_and_the_sand`) and not an eye's.
///
/// The sub-root is the head — the end that meets the deck — so dragging the
/// deck takes the ramp with it and the joint cannot open.
fn ramp() -> Generator {
    // Head at the deck's own front edge, foot wherever the pitch puts it.
    let head_z = -DECK_D * 0.5 + 0.05;
    let rise = FLOOR;
    let run = rise / RAMP_PITCH.tan();
    let foot_z = head_z - run;
    let pitch = RAMP_PITCH;
    let span = rise.hypot(run);
    let center = [0.0, rise * 0.5, (head_z + foot_z) * 0.5];

    // `quat_x(-pitch)`, not `quat_x(pitch)`. A positive X rotation turns `+Y`
    // toward `+Z`, so it sends the board's local `+Z` end *downhill* — and the
    // head of this ramp is at `+Z`. Getting it backwards points the ramp down
    // into its own deck, which reads as a perfectly ordinary ramp from every
    // angle a contact sheet takes.
    let tilt = quat_x(-pitch);
    // The board's own axes in world space, so the cleats ride the surface
    // instead of being placed beside it.
    let along = [0.0, pitch.sin(), pitch.cos()];
    let out = [0.0, pitch.cos(), -pitch.sin()];

    let mut parts = Vec::new();
    // Cleats across the slope, so it reads as a boarding ramp rather than a
    // plank. Spaced along the board's own span and lifted along its own
    // normal.
    let n = 7;
    for i in 0..n {
        let f = ((i as f32 + 0.5) / n as f32 - 0.5) * span;
        let lift = RAMP_T * 0.5 + 0.03;
        parts.push(prim(
            cuboid_tapered([RAMP_W - 0.12, 0.06, 0.1], 0.0, plank(DECK_PALE)),
            [
                center[0],
                center[1] + f * along[1] + lift * out[1],
                center[2] + f * along[2] + lift * out[2],
            ],
            tilt,
        ));
    }

    parts.push(prim(
        solid(cuboid_tapered(
            [RAMP_W, RAMP_T, span],
            0.0,
            plank(DECK_WOOD),
        )),
        center,
        tilt,
    ));

    // The sub-root is the foot kerb, not the board. A tilted sub-root spins
    // everything nested under it — the cleats came out turned twice and swung
    // clean off the surface — so the assembly hangs off the one flat thing in
    // it, which is also the thing it actually rests on.
    let kerb = prim(
        solid(cuboid_tapered(
            [RAMP_W + 0.3, 0.14, 0.45],
            0.0,
            plank(DECK_WOOD),
        )),
        [0.0, 0.07, foot_z + 0.1],
        id_quat(),
    );
    nest(kerb, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_no_glazing_on_solids, assert_no_tilted_parents, assert_sanitize_stable,
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
        assert_sanitize_stable(&LifeguardTower.build(""), "lifeguard_tower");
    }

    #[test]
    fn no_glazing_lands_on_a_solid() {
        assert_no_glazing_on_solids(&LifeguardTower.build(""), "lifeguard_tower");
    }

    /// The standing ROTATED-ROOT gotcha, finally guarded: a tilted parent
    /// spins everything it carries, and the translation-only walks every other
    /// guard here uses would report those children where they were authored
    /// rather than where they render.
    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&LifeguardTower.build(""), "lifeguard_tower");
    }

    /// #972 lesson 1: the observation window is one card on a `Plane` at
    /// `uv_scale` 1.0, over a real opening.
    #[test]
    fn the_window_is_a_card_on_a_plane() {
        let mut cards = 0;
        walk(&LifeguardTower.build(""), [0.0; 3], &mut |g, _| {
            let is_plane = matches!(g.kind, GeneratorKind::Plane { .. });
            for m in crate::pds::material_finish::node_materials_mut(&mut g.kind.clone()) {
                if matches!(m.texture, SovereignTextureConfig::Window(_)) {
                    assert!(is_plane, "Window card must sit on a Plane");
                    assert_eq!(m.uv_scale.0, 1.0, "cards are clamp-to-edge");
                    cards += 1;
                }
            }
        });
        assert_eq!(cards, 1, "the cabin has one observation window");
    }

    /// The ramp lands on the deck at one end and on the sand at the other.
    ///
    /// Read out of the **built tree** — the node's own rotation applied to its
    /// own half-extent — rather than re-derived from the constants that placed
    /// it (#972 lesson 21). The shipped version had a hand-picked length, a
    /// hand-picked tilt and a hand-picked centre that did not agree: head
    /// 0.38 m under the deck, foot 0.22 m over the sand, and it looked
    /// perfectly fine from all four angles.
    #[test]
    fn the_ramp_meets_the_deck_and_the_sand() {
        let root = LifeguardTower.build("");
        let mut ramp: Option<([f32; 3], [f32; 4], [f32; 3])> = None;
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            // The one long tilted board on the prop.
            if g.transform.rotation.0[0].abs() > 1e-4 && size.0[2] > 2.0 {
                ramp = Some((at, g.transform.rotation.0, size.0));
            }
        });
        let (at, q, size) = ramp.expect("the tower has a boarding ramp");
        // The board's long axis, turned by its OWN quaternion through the one
        // shared implementation — see `util::rotate_by` for why this is not
        // hand-rolled. Doing it by hand here is how the shipped ramp and the
        // first version of this guard managed to agree with each other while
        // both pointing downhill.
        let arm = util::rotate_by(q, [0.0, 0.0, size[2] * 0.5]);
        let head = [at[1] + arm[1], at[2] + arm[2]];
        let foot = [at[1] - arm[1], at[2] - arm[2]];
        let (head, foot) = if head[1] > foot[1] {
            (head, foot)
        } else {
            (foot, head)
        };
        assert!(
            (head[0] - FLOOR).abs() < 0.12,
            "the ramp's head is at y {}, and the deck it lands on is at {FLOOR}",
            head[0]
        );
        assert!(
            head[1] > -DECK_D * 0.5 - 0.2 && head[1] < -DECK_D * 0.5 + 0.4,
            "the ramp's head at z {} does not meet the deck's front edge at {}",
            head[1],
            -DECK_D * 0.5
        );
        assert!(
            foot[0].abs() < 0.12,
            "the ramp's foot is at y {}, not on the sand",
            foot[0]
        );
        // ...and it is walkable rather than a ladder.
        let deg = ((head[0] - foot[0]) / (head[1] - foot[1]).abs())
            .abs()
            .atan()
            .to_degrees();
        assert!(
            (25.0..42.0).contains(&deg),
            "a {deg}° ramp is a ladder, not something a lifeguard walks up — a \
             boarding ramp is steep, and the cleats are why, but it is still walked"
        );
    }

    /// #972: the guardrail is a railing, not a floating bar. The shipped one
    /// was three bars with nothing holding them up.
    #[test]
    fn the_guardrail_has_posts_and_balusters() {
        let root = LifeguardTower.build("");
        let mut balusters = 0;
        let mut posts = 0;
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            let [sx, sy, sz] = size.0;
            if at[1] < FLOOR || !(0.5..=RAIL_H + 0.01).contains(&sy) {
                return;
            }
            if (sx - 0.11).abs() < 1e-3 && (sz - 0.11).abs() < 1e-3 {
                posts += 1;
            } else if sx < 0.09 && sz < 0.09 {
                balusters += 1;
            }
        });
        assert!(posts >= 8, "only {posts} rail posts — the rail floats");
        assert!(
            balusters >= 12,
            "only {balusters} balusters — the rail is a bar"
        );
    }

    /// Nothing on the cabin shares a plane with a deck edge, and there is
    /// standing room in front of the window.
    ///
    /// Read out of the built tree rather than compared between constants: the
    /// shipped cabin was exactly as wide as its deck and flush with its back
    /// edge — a coplanar seam down two whole corners — and what has to be true
    /// is a fact about where the slabs *are*, not about which pair of numbers
    /// were typed.
    #[test]
    fn the_cabin_stands_inside_the_deck() {
        let root = LifeguardTower.build("");
        let mut deck: Option<([f32; 3], [f32; 3])> = None;
        let mut walls = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            let half = [size.0[0] * 0.5, size.0[1] * 0.5, size.0[2] * 0.5];
            if (at[1] - DECK_Y).abs() < 1e-3 && half[0] > 1.0 && half[1] < 0.2 {
                deck = Some((at, half));
            } else if at[1] > FLOOR
                && at[1] < CAB_TOP
                && half[1] > CAB_H * 0.2
                // Walls span a wall's worth of ground; a railing post lives
                // at the deck edge on purpose and is not one.
                && half[0].max(half[2]) > 0.3
            {
                walls.push((at, half));
            }
        });
        let (dc, dh) = deck.expect("the tower has a deck");
        assert!(!walls.is_empty(), "no cabin walls found");
        for (c, h) in walls {
            for axis in [0usize, 2] {
                assert!(
                    c[axis] - h[axis] > dc[axis] - dh[axis] + 0.05
                        && c[axis] + h[axis] < dc[axis] + dh[axis] - 0.05,
                    "a cabin wall at {c:?} (half {h:?}) is flush with — or past — \
                     the deck edge at {dc:?} (half {dh:?}) on axis {axis}"
                );
            }
        }
        // ...and the platform in front of the window is somewhere to stand.
        let stand = FRONT - (dc[2] - dh[2]);
        assert!(stand > 0.5, "only {stand} m of deck in front of the window");
    }

    /// The tower keeps its eave lamp — escalation's broken-emissive ruin pass
    /// needs something to snuff.
    #[test]
    fn has_an_eave_lamp() {
        assert!(crate::catalogue::items::util::has_emissive(
            &LifeguardTower.build("")
        ));
    }

    /// The editability contract: a post carries the deck, the deck carries the
    /// cabin and the ramp, the cabin carries the roof.
    #[test]
    fn subtrees_carry_what_they_hold_up() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        let root = LifeguardTower.build("");
        let deck = root
            .children
            .iter()
            .find(|c| c.children.len() > 10)
            .expect("a post carries the deck");
        let cabin = deck
            .children
            .iter()
            .find(|c| c.children.len() > 8)
            .expect("the deck carries the cabin");
        assert!(
            cabin.children.iter().any(|c| c.children.len() >= 3),
            "the cabin carries a roof that carries its fascia and pennant"
        );
        assert!(count(&root) > 45, "the tower lost most of its parts");
    }
}
