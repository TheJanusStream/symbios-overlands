//! Gas lamp — a Steampunk prop. A wrought-iron lamppost on a stepped base: a
//! fluted column with brass collars, a gas riser and stopcock, a lamplighter's
//! rest bar, four scroll volutes under a glazed lantern, and a mantle burning
//! behind real panes in a brass cage.
//!
//! Rebuilt under #972. The prop was already a decent silhouette; three things
//! were wrong underneath it:
//!
//! 1. **The panes were solids.** Four `Window`-textured **cuboids** stood in
//!    for the glazing (#972 lesson 20). The generator masks its panes away, so
//!    each was a frame with holes onto whatever lay behind it — which on a
//!    lantern is the far pane, i.e. sky. A lantern is the one prop whose whole
//!    point is a light seen *through* glass, so the card belongs on a flat quad
//!    over the real opening between the corner posts, with the mantle behind.
//! 2. **There was no cage.** The doc comment promised "a glowing gas mantle in
//!    a brass cage" and the geometry had four corner posts and nothing else.
//! 3. **Flat 20-prim list** (#972 lesson 3), so dragging the column left the
//!    lantern, the scrolls and the roof behind — on a prop whose only likely
//!    edit is "move it" or "make it taller".
//!
//! Now the tree stands the way the lamp does — base → plinth → column →
//! bracket → lantern → roof → finial — and every part above the bracket is
//! carried by the thing under it.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    cone, cuboid_tapered, cylinder_tapered, glow, id_quat, lit_interior, nest, plane, prim,
    quat_mul, quat_x, quat_y, quat_z, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp4, Generator};
use crate::seeded_defaults::ThemeArchetype;

use super::{BRASS, IRON_DARK, LAMP_GAS, brass, iron, pane_grid};

// --- Dimensions. Everything below derives from these. ----------------------

/// Stepped iron base — the root, and the only thing that touches the ground.
const BASE_W: f32 = 0.74;
const BASE_T: f32 = 0.26;
/// Moulded plinth on it.
const PLINTH_W: f32 = 0.52;
const PLINTH_H: f32 = 0.32;
const PLINTH_TOP: f32 = BASE_T + PLINTH_H;

/// Fluted column.
const SHAFT_R: f32 = 0.095;
const SHAFT_H: f32 = 2.28;
const SHAFT_TOP: f32 = PLINTH_TOP + SHAFT_H;

/// The scroll bracket's collar, and how far the volutes reach out from it.
const BRACKET_Y: f32 = SHAFT_TOP - 0.14;
const ARM_LEN: f32 = 0.34;

/// The **gallery** — the stepped moulding that carries the lantern off the
/// column, and the reason there is no daylight between them.
///
/// The first build set `CAGE_BOT = SHAFT_TOP + 0.1` and left the 100 mm
/// between them empty: the lantern's bottom collar is a *ring*, so its
/// underside is open, and from any angle the whole lamp head floated clear of
/// the post it stands on. A lantern is carried by something. The gallery laps
/// **into** the shaft (so there is no coplanar tie either) and flares out in
/// three courses to the lantern's own plan.
const GALLERY_LAP: f32 = 0.05;
const GALLERY_BOT: f32 = SHAFT_TOP - GALLERY_LAP;
const GALLERY_STEP: f32 = 0.07;
const GALLERY_H: f32 = GALLERY_STEP * 3.0;

/// The lantern: a brass cage of four glazed faces between corner posts,
/// standing on the gallery.
const CAGE_BOT: f32 = GALLERY_BOT + GALLERY_H;
const CAGE_H: f32 = 0.9;
const CAGE_TOP: f32 = CAGE_BOT + CAGE_H;
/// Half the lantern's plan width, and its corner-post stock.
const LANT_HALF: f32 = 0.3;
const POST: f32 = 0.065;
/// Brass collars top and bottom of the cage.
const COLLAR: f32 = 0.065;
/// How far the glazing sits inside the posts' outer face — the putty rebate,
/// and what makes the card's lap invisible: the post's own face is nearer the
/// viewer than the glass it holds (#972 lesson 7).
const REBATE: f32 = 0.025;
/// How far a card oversails its opening on every edge.
const GLAZE_LAP: f32 = 0.05;

/// Peaked roof over the cage, and the finial on it.
const ROOF_R: f32 = 0.46;
const ROOF_H: f32 = 0.44;
const FINIAL_H: f32 = 0.3;
/// The cowl's radius, and how far it and the spike above it **sink into** the
/// roof.
///
/// A cone comes to a point, so anything set at its apex is balancing on that
/// point — a spike resting on nothing, which is what the first build looked
/// like. Seat the cowl where the cone is still wider than the cowl is: at
/// `FINIAL_SINK` below the apex the cone's radius is
/// `ROOF_R · FINIAL_SINK / ROOF_H` = 94 mm, comfortably outside the cowl's 75.
/// The relationship, not the number, is what the guard states.
const COWL_R: f32 = 0.075;
const COWL_H: f32 = 0.15;
const FINIAL_SINK: f32 = 0.09;

/// Height of the lamplighter's rest bar — the crossbar a ladder hooks over,
/// and the one detail that says this lamp is lit by hand.
const REST_Y: f32 = 2.0;

// --- Shared construction. --------------------------------------------------

/// Which way a pane looks. A lantern is glazed on all four faces, so unlike
/// the one-hero-face entries this needs every upright turn — and the `±X` ones
/// are a composition rather than a single-axis rotation.
#[derive(Clone, Copy)]
enum Look {
    Nz,
    Pz,
    Nx,
    Px,
}

impl Look {
    /// The rotation that stands a [`plane`] up facing this way, with the quad's
    /// local Z extent on world `+Y`, so `size` reads as `[width, height]`.
    fn quat(self) -> Fp4 {
        match self {
            Look::Nz => quat_x(-FRAC_PI_2),
            Look::Pz => quat_x(FRAC_PI_2),
            Look::Nx => quat_mul(quat_y(FRAC_PI_2), quat_x(-FRAC_PI_2)),
            Look::Px => quat_mul(quat_y(-FRAC_PI_2), quat_x(-FRAC_PI_2)),
        }
    }
    /// Outward direction in the XZ plane.
    fn out(self) -> [f32; 2] {
        match self {
            Look::Nz => [0.0, -1.0],
            Look::Pz => [0.0, 1.0],
            Look::Nx => [-1.0, 0.0],
            Look::Px => [1.0, 0.0],
        }
    }
}

/// The clear opening one lantern face leaves between its two corner posts.
fn pane_size() -> [f32; 2] {
    [(LANT_HALF - POST) * 2.0, CAGE_H - COLLAR * 2.0]
}

pub struct GasLamp;

impl CatalogueEntry for GasLamp {
    fn slug(&self) -> &'static str {
        "gas_lamp"
    }
    fn name(&self) -> &'static str {
        "Gas Lamp"
    }
    fn description(&self) -> &'static str {
        "Wrought-iron lamppost with brass scrolls and a mantle glazed in a brass cage."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Steampunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::STEAM_BAND
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

/// The lamp as a tree that stands the way it does: the base at the bottom, the
/// plinth on it, the column on that, the bracket on the column, the lantern on
/// the bracket and the roof on the lantern (#972 lesson 3). On a prop whose
/// only likely edits are "move it" and "make it taller", that is the whole
/// difference between one gizmo drag and twenty.
fn build_tree() -> Generator {
    let base = prim(
        solid(cuboid_tapered(
            [BASE_W, BASE_T, BASE_W],
            0.1,
            iron(IRON_DARK),
        )),
        [0.0, BASE_T * 0.5, 0.0],
        id_quat(),
    );
    nest(base, vec![plinth()])
}

/// Moulded plinth, carrying the column and a small maker's plate.
fn plinth() -> Generator {
    let root = prim(
        solid(cuboid_tapered(
            [PLINTH_W, PLINTH_H, PLINTH_W],
            0.18,
            iron(IRON_DARK),
        )),
        [0.0, BASE_T + PLINTH_H * 0.5, 0.0],
        id_quat(),
    );
    let plate = prim(
        solid(cuboid_tapered([0.2, 0.13, 0.02], 0.0, brass(BRASS))),
        [0.0, BASE_T + PLINTH_H * 0.55, -(PLINTH_W * 0.5 - 0.02)],
        id_quat(),
    );
    nest(root, vec![plate, column()])
}

/// Fluted column with its collars, the gas riser that feeds the lantern, the
/// lamplighter's rest bar — and the bracket at the top.
fn column() -> Generator {
    let shaft = prim(
        solid(cylinder_tapered(
            SHAFT_R,
            SHAFT_H,
            10,
            0.12,
            iron(IRON_DARK),
        )),
        [0.0, PLINTH_TOP + SHAFT_H * 0.5, 0.0],
        id_quat(),
    );

    let mut parts = Vec::new();
    for y in [PLINTH_TOP + 0.14, SHAFT_TOP - 0.24] {
        parts.push(prim(
            solid(torus(0.035, 0.125, brass(BRASS))),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }
    // Gas riser up the street face, with a stopcock a lamplighter can reach.
    let riser_x = SHAFT_R + 0.045;
    let riser_bot = PLINTH_TOP + 0.05;
    let riser_top = BRACKET_Y - 0.08;
    parts.push(prim(
        solid(cylinder_tapered(
            0.026,
            riser_top - riser_bot,
            8,
            0.0,
            brass(BRASS),
        )),
        [riser_x, (riser_bot + riser_top) * 0.5, 0.0],
        id_quat(),
    ));
    parts.push(prim(
        solid(torus(0.022, 0.055, brass(BRASS))),
        [riser_x, PLINTH_TOP + 0.5, 0.0],
        quat_z(FRAC_PI_2),
    ));
    parts.push(prim(
        solid(cuboid_tapered([0.03, 0.03, 0.17], 0.0, iron(IRON_DARK))),
        [riser_x, PLINTH_TOP + 0.5, 0.06],
        id_quat(),
    ));
    // Lamplighter's rest bar — what a ladder hooks over.
    parts.push(prim(
        solid(cuboid_tapered([0.5, 0.04, 0.045], 0.0, brass(BRASS))),
        [0.0, REST_Y, 0.0],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(torus(0.018, 0.045, brass(BRASS))),
            [sx * 0.25, REST_Y, 0.0],
            quat_x(FRAC_PI_2),
        ));
    }

    parts.push(bracket());
    nest(shaft, parts)
}

/// The scroll bracket: a brass collar round the column with four volutes
/// radiating from it, carrying the lantern.
///
/// Each arm is authored from **one direction vector**, so the bar, its curl and
/// the tip stay on the same ray however the reach changes — the shipped version
/// crossed two flat bars and hung four rings under them at a fixed offset, and
/// the rings did not line up with the arms they were meant to curl off.
fn bracket() -> Generator {
    let collar = prim(
        solid(torus(0.045, 0.16, brass(BRASS))),
        [0.0, BRACKET_Y, 0.0],
        id_quat(),
    );

    let mut parts = Vec::new();
    for look in [Look::Nz, Look::Pz, Look::Nx, Look::Px] {
        let [ox, oz] = look.out();
        let mid = ARM_LEN * 0.5 + 0.1;
        let along_x = ox.abs() > 0.5;
        parts.push(prim(
            solid(cuboid_tapered(
                if along_x {
                    [ARM_LEN, 0.045, 0.05]
                } else {
                    [0.05, 0.045, ARM_LEN]
                },
                0.0,
                brass(BRASS),
            )),
            [ox * mid, BRACKET_Y + 0.02, oz * mid],
            id_quat(),
        ));
        // The volute at the arm's tip, curling in the arm's own vertical plane.
        let tip = ARM_LEN + 0.1;
        parts.push(prim(
            solid(torus(0.024, 0.086, brass(BRASS))),
            [ox * tip, BRACKET_Y - 0.05, oz * tip],
            if along_x {
                quat_x(FRAC_PI_2)
            } else {
                quat_z(FRAC_PI_2)
            },
        ));
    }
    parts.push(lantern());
    nest(collar, parts)
}

// --- The lantern. ----------------------------------------------------------

/// The lantern: bottom collar (the sub-root), four corner posts, four glazed
/// faces, the cage bars over them, the burner and mantle inside, and the roof.
fn lantern() -> Generator {
    // The gallery's lowest course is the sub-root: it laps into the shaft, and
    // everything above stands on it.
    let root = prim(
        solid(cuboid_tapered(
            [0.24, GALLERY_STEP, 0.24],
            0.0,
            iron(IRON_DARK),
        )),
        [0.0, GALLERY_BOT + GALLERY_STEP * 0.5, 0.0],
        id_quat(),
    );

    let mut parts = Vec::new();
    for (i, w) in [0.42_f32, LANT_HALF * 2.0 + 0.02].iter().enumerate() {
        parts.push(prim(
            solid(cuboid_tapered([*w, GALLERY_STEP, *w], 0.0, iron(IRON_DARK))),
            [0.0, GALLERY_BOT + GALLERY_STEP * (1.5 + i as f32), 0.0],
            id_quat(),
        ));
    }
    parts.push(prim(
        solid(torus(COLLAR * 0.5, LANT_HALF + 0.03, brass(BRASS))),
        [0.0, CAGE_BOT + COLLAR * 0.5, 0.0],
        id_quat(),
    ));
    parts.push(prim(
        solid(torus(COLLAR * 0.5, LANT_HALF + 0.03, brass(BRASS))),
        [0.0, CAGE_TOP - COLLAR * 0.5, 0.0],
        id_quat(),
    ));

    // Corner posts, held so their outer faces are the lantern's own plan.
    let pc = LANT_HALF - POST * 0.5;
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        parts.push(prim(
            solid(cuboid_tapered([POST, CAGE_H, POST], 0.0, iron(IRON_DARK))),
            [sx * pc, CAGE_BOT + CAGE_H * 0.5, sz * pc],
            id_quat(),
        ));
    }

    // Glazing: a card on a flat quad over each real opening, set into the
    // rebate, with the burning mantle behind it.
    let size = pane_size();
    let cy = CAGE_BOT + CAGE_H * 0.5;
    for look in [Look::Nz, Look::Pz, Look::Nx, Look::Px] {
        let [ox, oz] = look.out();
        parts.push(prim(
            plane(
                [size[0] + GLAZE_LAP, size[1] + GLAZE_LAP],
                pane_grid(LAMP_GAS, 1.5, (2, 3)),
            ),
            [ox * (LANT_HALF - REBATE), cy, oz * (LANT_HALF - REBATE)],
            look.quat(),
        ));
        // Two cage bars across each face, **outside** the glass — the brass
        // cage the doc comment always promised and the geometry never had.
        for f in [-0.22_f32, 0.22] {
            let along_x = ox.abs() > 0.5;
            parts.push(prim(
                solid(cuboid_tapered(
                    if along_x {
                        [0.03, 0.03, LANT_HALF * 2.0]
                    } else {
                        [LANT_HALF * 2.0, 0.03, 0.03]
                    },
                    0.0,
                    brass(BRASS),
                )),
                [
                    ox * (LANT_HALF + 0.02),
                    cy + f * CAGE_H,
                    oz * (LANT_HALF + 0.02),
                ],
                id_quat(),
            ));
        }
    }

    // The burner: a gas pipe up through the collar, the mantle on it, and a
    // pale reflector above so the light reads as coming off something.
    parts.push(prim(
        solid(cylinder_tapered(0.028, 0.34, 6, 0.0, brass(BRASS))),
        [0.0, CAGE_BOT + 0.17, 0.0],
        id_quat(),
    ));
    parts.push(prim(
        sphere(0.115, 4, glow(LAMP_GAS, 3.0)),
        [0.0, CAGE_BOT + 0.46, 0.0],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered(
            [0.34, 0.03, 0.34],
            0.3,
            lit_interior([0.86, 0.8, 0.66], 0.5),
        ),
        [0.0, CAGE_TOP - COLLAR - 0.06, 0.0],
        id_quat(),
    ));

    parts.push(roof());
    nest(root, parts)
}

/// The peaked roof, its vent cowl and the finial.
///
/// `ROOF_R` is checked against the lantern it covers rather than picked by eye:
/// a four-sided pyramid's flat faces sit at `r · cos 45°` from the axis, so the
/// cage's half width is what sets the minimum (#972 lesson 11, turned upward).
fn roof() -> Generator {
    let root = prim(
        solid(cone(ROOF_R, ROOF_H, 4, iron(IRON_DARK))),
        [0.0, CAGE_TOP + ROOF_H * 0.5, 0.0],
        id_quat(),
    );
    let apex = CAGE_TOP + ROOF_H;
    // Both are seated `FINIAL_SINK` **below** the apex, so the cowl's base is
    // inside the cone and the spike's base is inside the cowl. Set at the apex
    // instead — which is where they were — each balances on a point.
    let cowl_bot = apex - FINIAL_SINK;
    let parts = vec![
        prim(
            solid(cylinder_tapered(COWL_R, COWL_H, 8, 0.18, brass(BRASS))),
            [0.0, cowl_bot + COWL_H * 0.5, 0.0],
            id_quat(),
        ),
        prim(
            solid(cylinder_tapered(0.038, FINIAL_H, 6, 0.55, brass(BRASS))),
            [0.0, cowl_bot + COWL_H * 0.4 + FINIAL_H * 0.5, 0.0],
            id_quat(),
        ),
    ];
    nest(root, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_cards_do_not_overlap, assert_no_glazing_on_solids, assert_no_tilted_parents,
        assert_sanitize_stable, has_emissive,
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
        assert_sanitize_stable(&GasLamp.build(""), "gas_lamp");
    }

    #[test]
    fn no_glazing_lands_on_a_solid() {
        assert_no_glazing_on_solids(&GasLamp.build(""), "gas_lamp");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&GasLamp.build(""), "gas_lamp");
    }

    #[test]
    fn glazed_surfaces_do_not_collide() {
        assert_cards_do_not_overlap(&GasLamp.build(""), "gas_lamp");
    }

    #[test]
    fn keeps_its_mantle() {
        assert!(
            has_emissive(&GasLamp.build("")),
            "the lamp lost the mantle the ruin pass is supposed to snuff"
        );
    }

    /// #972 lesson 1: four panes, each a card on a flat quad at `uv_scale` 1.0,
    /// each strictly larger than the opening its posts leave it (lesson 7).
    #[test]
    fn every_pane_is_a_card_lapping_its_own_opening() {
        let mut cards = 0;
        let clear = pane_size();
        walk(&GasLamp.build(""), [0.0; 3], &mut |g, _| {
            let GeneratorKind::Plane { size, material, .. } = &g.kind else {
                // Nothing else in the tree may wear the card texture; that is
                // `assert_no_glazing_on_solids`' job, and it runs above.
                return;
            };
            if !matches!(material.texture, SovereignTextureConfig::Window(_)) {
                return;
            }
            cards += 1;
            assert_eq!(material.uv_scale.0, 1.0, "cards are clamp-to-edge");
            assert!(
                (size.0[0] - clear[0] - GLAZE_LAP).abs() < 1e-4
                    && (size.0[1] - clear[1] - GLAZE_LAP).abs() < 1e-4,
                "a {:?} card does not lap the {clear:?} opening its posts leave it",
                size.0
            );
        });
        assert_eq!(cards, 4, "a lantern is glazed on all four faces");
    }

    /// The mantle burns **inside** the lantern.
    ///
    /// This is the check a lantern actually needs and the one a render cannot
    /// make: an emissive sphere a few centimetres outside its own glazing looks
    /// identical from three of the four angles in a contact sheet, and reads as
    /// a bare bulb from the fourth. Expressed against the cage's own plan and
    /// the glazing planes rather than against the constants that place it.
    #[test]
    fn the_mantle_burns_behind_the_glass() {
        let root = GasLamp.build("");
        let mut mantle: Option<([f32; 3], f32)> = None;
        let mut panes: Vec<[f32; 3]> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| match &g.kind {
            GeneratorKind::Sphere {
                radius, material, ..
            } if material.emission_strength.0 > 1.0 => mantle = Some((at, radius.0)),
            GeneratorKind::Plane { material, .. }
                if matches!(material.texture, SovereignTextureConfig::Window(_)) =>
            {
                panes.push(at)
            }
            _ => {}
        });
        let (at, r) = mantle.expect("no mantle in the lantern");
        assert_eq!(panes.len(), 4);
        assert!(
            at[1] - r > CAGE_BOT && at[1] + r < CAGE_TOP,
            "the mantle at {at:?} is not between the lantern's collars \
             ({CAGE_BOT}..{CAGE_TOP})"
        );
        for p in &panes {
            // Each pane sits on one axis; the mantle must be on the inboard
            // side of every one of them, by its own radius.
            let axis = if p[0].abs() > p[2].abs() { 0 } else { 2 };
            let inboard = (p[axis] - at[axis]).abs();
            assert!(
                inboard > r,
                "the mantle at {at:?} reaches through the pane at {p:?}"
            );
        }
    }

    /// The roof covers the lantern it sits on.
    ///
    /// A four-sided pyramid's flat faces lie at `r · cos 45°` from its axis, so
    /// the covering radius is smaller than the radius you author — which is the
    /// kind of standoff that gets picked by eye and left 30 mm short (#972
    /// lesson 11). Stated as the relationship, not as the number.
    #[test]
    fn the_roof_covers_the_cage_it_caps() {
        let root = GasLamp.build("");
        let mut roof: Option<([f32; 3], f32)> = None;
        walk(&root, [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cone { radius, .. } = &g.kind {
                roof = Some((at, radius.0));
            }
        });
        let (at, r) = roof.expect("no roof on the lantern");
        let covers = r * std::f32::consts::FRAC_1_SQRT_2;
        assert!(
            covers >= LANT_HALF,
            "a {r} m pyramid covers {covers} m of a {LANT_HALF} m half-width lantern"
        );
        assert!(
            (at[1] - ROOF_H * 0.5 - CAGE_TOP).abs() < 1e-4,
            "the roof's base at {} does not sit on the cage top at {CAGE_TOP}",
            at[1] - ROOF_H * 0.5
        );
    }

    /// **Nothing on the axis floats.**
    ///
    /// The lamp is one unbroken column of solid from the ground to the lantern
    /// floor, and this walks it: every upright box and drum standing on the
    /// lamp's own axis, sorted by height, must chain from `y = 0` to the cage
    /// bottom with no gap between one and the next.
    ///
    /// The shipped build had 100 mm of daylight between the top of the column
    /// and the lantern, because the lantern's bottom collar is a *ring* — its
    /// underside is open, so nothing in the record says the head is unsupported
    /// and nothing in a four-angle sheet shows it either, unless a tile happens
    /// to catch that 10 cm against the sky. Stated as a chain rather than as a
    /// pair of numbers, so raising the lantern or shortening the column cannot
    /// open the gap again.
    #[test]
    fn the_lantern_is_carried_by_an_unbroken_column() {
        let mut spans: Vec<(f32, f32)> = Vec::new();
        walk(&GasLamp.build(""), [0.0; 3], &mut |g, at| {
            if g.transform.rotation.0 != [0.0, 0.0, 0.0, 1.0]
                || at[0].abs() > 0.05
                || at[2].abs() > 0.05
            {
                return;
            }
            let half = match &g.kind {
                GeneratorKind::Cuboid { size, .. } => size.0[1] * 0.5,
                GeneratorKind::Cylinder { height, .. } => height.0 * 0.5,
                GeneratorKind::Cone { height, .. } => height.0 * 0.5,
                _ => return,
            };
            spans.push((at[1] - half, at[1] + half));
        });
        assert!(
            spans.len() >= 6,
            "only {} axial members found — suspect the selector before the content",
            spans.len()
        );
        spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let mut reach = 0.0_f32;
        for (lo, hi) in &spans {
            if *lo > reach + 1e-4 {
                break;
            }
            reach = reach.max(*hi);
        }
        assert!(
            reach >= CAGE_BOT - 1e-4,
            "gas_lamp: the solid column reaches {reach} and the lantern floor is at \
             {CAGE_BOT} — the head floats on {} m of air",
            CAGE_BOT - reach
        );
    }

    /// The cowl and the spike are **seated in** the roof, not balanced on its
    /// point.
    ///
    /// A cone narrows to nothing, so a part set at its apex touches it at a
    /// single vertex however solid the record looks. The check is the one
    /// relationship that matters: at the height the cowl's base sits, the cone
    /// must still be wider than the cowl — and the spike's base must be inside
    /// the cowl, not on top of it.
    #[test]
    fn the_finial_is_seated_in_the_roof_not_balanced_on_it() {
        let root = GasLamp.build("");
        let mut cone: Option<([f32; 3], f32, f32)> = None;
        let mut drums: Vec<([f32; 3], f32, f32)> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| match &g.kind {
            GeneratorKind::Cone { radius, height, .. } => cone = Some((at, radius.0, height.0)),
            GeneratorKind::Cylinder { radius, height, .. } if at[1] > CAGE_TOP => {
                drums.push((at, radius.0, height.0))
            }
            _ => {}
        });
        let (cat, cr, ch) = cone.expect("no roof cone");
        let apex = cat[1] + ch * 0.5;
        assert_eq!(drums.len(), 2, "a cowl and a spike over the roof");
        drums.sort_by(|a, b| a.0[1].partial_cmp(&b.0[1]).unwrap());
        let (cowl, cowl_r, cowl_h) = drums[0];
        let (spike, _, spike_h) = drums[1];

        let cowl_bot = cowl[1] - cowl_h * 0.5;
        let drop = apex - cowl_bot;
        assert!(drop > 0.0, "the cowl's base is above the roof's own apex");
        // Radius the cone still has at the cowl's base.
        let cone_r_there = cr * drop / ch;
        assert!(
            cone_r_there >= cowl_r,
            "gas_lamp: at the cowl's base the roof is {cone_r_there} m across and the \
             cowl is {cowl_r} — it is balanced on the cone's point, not seated in it"
        );
        assert!(
            spike[1] - spike_h * 0.5 < cowl[1] + cowl_h * 0.5,
            "gas_lamp: the spike's base sits on the cowl's top rather than inside it"
        );
    }

    /// The four volutes are symmetric about the column, and each one's curl
    /// lies on its own arm's ray. The shipped bracket crossed two flat bars and
    /// hung the rings at a fixed offset, so the curls and the arms were two
    /// independent decisions that happened to look plausible.
    #[test]
    fn the_scrolls_are_symmetric_and_sit_on_their_own_arms() {
        let root = GasLamp.build("");
        let mut curls: Vec<[f32; 3]> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Torus {
                major_radius,
                minor_radius,
                ..
            } = &g.kind
                && at[1] > BRACKET_Y - 0.2
                && at[1] < BRACKET_Y + 0.1
                && (major_radius.0 - 0.086).abs() < 1e-4
                && minor_radius.0 < 0.03
            {
                curls.push(at);
            }
        });
        assert_eq!(curls.len(), 4, "four volutes under the lantern");
        let reach = ARM_LEN + 0.1;
        for c in &curls {
            let on_axis = (c[0].abs() < 1e-4) != (c[2].abs() < 1e-4);
            assert!(on_axis, "a volute at {c:?} is not on one of the four arms");
            assert!(
                (c[0].hypot(c[2]) - reach).abs() < 1e-4,
                "a volute at {c:?} is {} from the column, not the arm's own {reach}",
                c[0].hypot(c[2])
            );
        }
        // Symmetric: the four reaches sum to zero on both axes.
        let sx: f32 = curls.iter().map(|c| c[0]).sum();
        let sz: f32 = curls.iter().map(|c| c[2]).sum();
        assert!(
            sx.abs() < 1e-4 && sz.abs() < 1e-4,
            "the bracket is lopsided"
        );
    }

    /// #972 lesson 8: everything standing on the base has its footprint inside
    /// the base's — the one containment a free-standing prop can get wrong.
    #[test]
    fn everything_standing_on_the_base_is_on_it() {
        let mut checked = 0;
        walk(&GasLamp.build(""), [0.0; 3], &mut |g, at| {
            let (hx, hy, hz) = match &g.kind {
                GeneratorKind::Cuboid { size, .. } => {
                    (size.0[0] * 0.5, size.0[1] * 0.5, size.0[2] * 0.5)
                }
                GeneratorKind::Cylinder { radius, height, .. } => {
                    (radius.0, height.0 * 0.5, radius.0)
                }
                _ => return,
            };
            if g.transform.rotation.0 != [0.0, 0.0, 0.0, 1.0] || (at[1] - hy - BASE_T).abs() > 0.02
            {
                return;
            }
            checked += 1;
            assert!(
                at[0].abs() + hx <= BASE_W * 0.5 + 1e-3 && at[2].abs() + hz <= BASE_W * 0.5 + 1e-3,
                "gas_lamp: a part at {at:?} stands on the base and hangs off it"
            );
        });
        assert!(checked >= 1, "nothing found standing on the base");
    }

    /// The editability contract (#972 lesson 3): base → plinth → column →
    /// bracket → lantern → roof, each carrying everything above it. Selected by
    /// the property that defines each sub-root rather than by child count,
    /// which changes the moment a part is added.
    #[test]
    fn subtrees_carry_what_they_hold_up() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        let root = GasLamp.build("");
        let cuboid_w = |g: &Generator, w: f32| match &g.kind {
            GeneratorKind::Cuboid { size, .. } => (size.0[0] - w).abs() < 1e-4,
            _ => false,
        };
        let plinth = root
            .children
            .iter()
            .find(|c| cuboid_w(c, PLINTH_W))
            .expect("the base carries the plinth");
        let shaft = plinth
            .children
            .iter()
            .find(|c| {
                matches!(&c.kind, GeneratorKind::Cylinder { height, .. }
                if (height.0 - SHAFT_H).abs() < 1e-4)
            })
            .expect("the plinth carries the column");
        let bracket = shaft
            .children
            .iter()
            .find(|c| {
                matches!(&c.kind, GeneratorKind::Torus { major_radius, .. }
                if (major_radius.0 - 0.16).abs() < 1e-4)
            })
            .expect("the column carries the bracket collar");
        let lantern = bracket
            .children
            .iter()
            .find(|c| !c.children.is_empty())
            .expect("the bracket carries the lantern");
        assert!(
            lantern
                .children
                .iter()
                .any(|c| matches!(c.kind, GeneratorKind::Cone { .. }) && !c.children.is_empty()),
            "the lantern carries a roof that carries its cowl and finial"
        );
        assert!(count(&root) > 35, "the lamp lost most of its parts");
    }
}
