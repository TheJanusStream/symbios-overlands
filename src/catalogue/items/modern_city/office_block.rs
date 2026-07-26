//! Office block — a Modern-City secondary. A mid-rise box whose street face
//! is a glazed curtain wall over lit office floors, with concrete flanks, an
//! entrance canopy, and a parapet roof with a humming rooftop unit. The
//! everyday downtown building that rings the landmark tower.
//!
//! The glazing follows the `Window`-card idiom of
//! [`corner_store`](super::corner_store): the curtain wall is a
//! [`window_card`] on a [`plane`], its panes cut open over a recessed
//! interior of floor slabs and warm ceiling strips, so the tower reads as
//! lit floors seen through glass rather than a teal slab stuck on a solid
//! box (the shared [`curtain_wall`](super::curtain_wall) helper still slabs
//! its glass — see its note — so this entry builds its own, #942).

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, glow, id_quat, plane, prim, quat_x, solid, window_card,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, GLASS_TEAL, LAMP_WARM, STEEL_GREY, concrete, fx, steel};

/// Warm office interior light — the glow strip along each floor's ceiling,
/// the warmth that reads through the cut panes as "the lights are on".
const OFFICE_WARM: [f32; 3] = [1.0, 0.87, 0.62];
/// Steel mullion / transom grey — the proud curtain-wall grid.
const MULLION: [f32; 3] = [0.34, 0.36, 0.40];

/// Clear height of the ground storey, above the plinth top — the band the
/// shopfront glazing and the entrance live in, and the datum the curtain wall
/// above now starts from.
///
/// It used to be implicit and much too small. The curtain wall was sized as
/// "the body less 2.4", which put its bottom edge 1.8 m above the lobby
/// floor, while the shopfront band was placed by eye at 1.0–2.7 m: a metre of
/// blank concrete under the glazing, and 0.4 m of the band running *up into*
/// the curtain wall on the same plane, which is two alpha-masked frames tying
/// for depth (see [`util::assert_cards_do_not_overlap`]). Giving the storey a
/// real height puts the glazing down on the floor where a lobby's glazing
/// belongs and leaves the curtain wall a clean edge to start from.
///
/// [`util::assert_cards_do_not_overlap`]: crate::catalogue::items::util
const GROUND_H: f32 = 3.4;
/// Height of the shopfront glazing's sill above the plinth top — a low kerb,
/// not a stall riser: this is a lobby, so the glass runs nearly to the floor.
const STORE_SILL: f32 = 0.28;
/// Spandrel left between the shopfront head and the curtain wall's bottom
/// transom, so the two glazed surfaces never meet on one plane.
const STORE_SPANDREL: f32 = 0.55;

/// Push a curtain-wall mullion grid — `cols + 1` verticals and `rows + 1`
/// transoms, standing `proud` of the glass plane at `cz` toward the front —
/// into `prims`. The glass itself is a separate [`plane`]; this is only the
/// steel that divides it.
fn mullion_grid(
    prims: &mut Vec<Generator>,
    center: [f32; 3],
    size: [f32; 2],
    bays: (u32, u32),
    proud: f32,
    mat: &SovereignMaterialSettings,
) {
    let [cx, cy, cz] = center;
    let [w, h] = size;
    let (cols, rows) = bays;
    let bar = 0.16_f32;
    let depth = proud.abs().max(0.18);
    let grid_z = cz + proud;
    for i in 0..=cols {
        let x = cx - w * 0.5 + w * (i as f32 / cols as f32);
        prims.push(prim(
            solid(cuboid_tapered([bar, h + bar, depth], 0.0, mat.clone())),
            [x, cy, grid_z],
            id_quat(),
        ));
    }
    for j in 0..=rows {
        let y = cy - h * 0.5 + h * (j as f32 / rows as f32);
        prims.push(prim(
            solid(cuboid_tapered([w + bar, bar, depth], 0.0, mat.clone())),
            [cx, y, grid_z],
            id_quat(),
        ));
    }
}

pub struct OfficeBlock;

impl CatalogueEntry for OfficeBlock {
    fn slug(&self) -> &'static str {
        "office_block"
    }
    fn name(&self) -> &'static str {
        "Office Block"
    }
    fn description(&self) -> &'static str {
        "Mid-rise office with a glass street facade, concrete flanks, and a roof unit."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CITY_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 32.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let w = 14.0_f32;
    let d = 10.0_f32;
    let base_h = 0.5;
    let body_h = 16.0;

    let body_cy = base_h + body_h * 0.5;
    let front_z = -d * 0.5; // the −Z render front is the glazed street face

    // The core is pulled back off the street face so a shallow interior sits
    // behind the glazing; the flank returns close the front corners.
    let cavity = 1.6_f32;
    let core_d = d - cavity;
    let core_cz = cavity * 0.5; // front face lands at front_z + cavity
    let core_front = front_z + cavity;
    let cav_mid = (front_z + core_front) * 0.5;

    let mut prims = vec![
        // Concrete base — the root.
        prim(
            solid(cuboid_tapered(
                [w + 1.0, base_h, d + 1.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
        // Concrete core box — the flanks and back stay solid masonry; the
        // street face is open to the glazing cavity in front.
        prim(
            solid(cuboid_tapered(
                [w, body_h, core_d],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, body_cy, core_cz],
            id_quat(),
        ),
    ];
    // Flank returns closing the front corners the recessed core leaves open.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.5, body_h, cavity],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [sx * (w * 0.5 - 0.25), body_cy, cav_mid],
            id_quat(),
        ));
    }

    // --- The lit interior seen through the curtain wall.

    // Glazing envelope (shared by the interior and the glass plane). The
    // curtain wall starts at the first-floor line — one clear ground storey
    // above the plinth — and runs to just under the parapet.
    let gw = w - 1.0;
    let g_bottom = base_h + GROUND_H;
    let g_top = base_h + body_h - 0.6;
    let gh = g_top - g_bottom;
    let gy = (g_bottom + g_top) * 0.5;
    let bays = (4u32, 5u32);
    let row_h = gh / bays.1 as f32;

    // The shopfront band fills what is left below it: sill just clear of the
    // plinth, head held a spandrel short of the curtain wall's own edge.
    let store_sill = base_h + STORE_SILL;
    let store_head = g_bottom - STORE_SPANDREL;
    let store_h = store_head - store_sill;
    let store_cy = (store_sill + store_head) * 0.5;

    // Warm interior back wall behind the floors, so the offices read as a
    // pale lit space through the cut panes rather than a cold recess. (The
    // ceiling strips below also light it, but emissive can't be judged from
    // the render tool's flat ambient, so the tone carries the read too.)
    prims.push(prim(
        solid(cuboid_tapered(
            [gw, body_h - 1.0, 0.1],
            0.0,
            concrete([0.66, 0.62, 0.55]),
        )),
        [0.0, body_cy + 0.2, core_front - 0.06],
        id_quat(),
    ));
    // Floor slabs at each interior storey line, set mid-cavity so they read
    // as floor plates behind the glass.
    for k in 1..bays.1 {
        let y = g_bottom + k as f32 * row_h;
        prims.push(prim(
            solid(cuboid_tapered(
                [gw - 0.6, 0.3, cavity - 0.4],
                0.0,
                concrete([0.62, 0.60, 0.55]),
            )),
            [0.0, y, cav_mid],
            id_quat(),
        ));
    }
    // Warm ceiling strip near the top of each storey — the lit-office glow.
    for k in 0..bays.1 {
        let y = g_bottom + (k as f32 + 0.85) * row_h;
        prims.push(prim(
            cuboid_tapered([gw - 0.8, 0.2, 0.16], 0.0, glow(OFFICE_WARM, 2.4)),
            [0.0, y, front_z + 0.4],
            id_quat(),
        ));
    }

    // --- The curtain wall itself: clear glazing on a plane + steel grid.

    prims.push(prim(
        plane([gw, gh], window_card(GLASS_TEAL, bays.0, bays.1, 0.3, 0.02)),
        [0.0, gy, front_z],
        quat_x(-FRAC_PI_2),
    ));
    mullion_grid(
        &mut prims,
        [0.0, gy, front_z],
        [gw, gh],
        bays,
        -0.34,
        &steel(MULLION),
    );

    // --- Ground-floor lobby: a storefront over a lit reception.

    // Lit lobby ceiling and its furniture, set in the cavity so they show
    // through the storefront glazing. The ceiling sits just under the
    // shopfront head, where a real lobby soffit is: hung any higher it is
    // outside the opening and lights nothing the street can see.
    prims.push(prim(
        cuboid_tapered([gw - 0.8, 0.12, cavity - 0.4], 0.0, glow(OFFICE_WARM, 1.1)),
        [0.0, store_head - 0.25, cav_mid],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [3.2, 1.0, 0.8],
            0.0,
            steel([0.5, 0.42, 0.3]),
        )),
        [3.5, base_h + 0.5, cav_mid],
        id_quat(),
    ));
    // A planter run in the left half of the lobby. The desk is a right-hand
    // object, and a shopfront this wide otherwise leaves four bays of glazing
    // with nothing behind them but the back wall.
    prims.push(prim(
        solid(cuboid_tapered(
            [3.4, 0.55, 0.7],
            0.0,
            concrete([0.52, 0.51, 0.49]),
        )),
        [-3.8, base_h + 0.28, cav_mid],
        id_quat(),
    ));
    // Storefront glazing — wide clear panes over the lobby, flanking the
    // central entrance portal. Two pane rows rather than one, because a
    // full-height lobby bay split once lands the panes near square.
    prims.push(prim(
        plane([gw, store_h], window_card(GLASS_TEAL, 8, 2, 0.3, 0.02)),
        [0.0, store_cy, front_z],
        quat_x(-FRAC_PI_2),
    ));

    // Dark entrance portal recess + glass doors, proud of the storefront so
    // the doors read in front of the glazing. Both are derived from the
    // shopfront band rather than placed by eye, so the entrance can never
    // again stand taller than the glazing it is cut into.
    let door_head = store_head - 0.45;
    prims.push(prim(
        solid(cuboid_tapered(
            [3.2, door_head - base_h + 0.35, 0.4],
            0.0,
            steel([0.16, 0.17, 0.2]),
        )),
        [0.0, (base_h + door_head + 0.35) * 0.5, front_z - 0.2],
        id_quat(),
    ));
    // The leaves lap 0.04 below the lobby floor so their bottom edge is not
    // coplanar with the plinth top (the card rule, #972 lesson 7).
    prims.push(prim(
        plane(
            [2.6, door_head - base_h + 0.04],
            window_card([0.14, 0.18, 0.2], 2, 2, 0.32, 0.05),
        ),
        [0.0, (base_h - 0.04 + door_head) * 0.5, front_z - 0.42],
        quat_x(-FRAC_PI_2),
    ));
    // Steel entrance canopy cantilevered over the doors, in the spandrel
    // between the shopfront head and the curtain wall.
    prims.push(prim(
        solid(cuboid_tapered([5.4, 0.3, 2.2], 0.0, steel(STEEL_GREY))),
        [0.0, store_head + 0.2, front_z - 1.0],
        id_quat(),
    ));
    // Warm lit address band above the canopy, held clear of the mullion grid
    // it would otherwise be embedded in.
    prims.push(prim(
        cuboid_tapered([4.2, 0.42, 0.16], 0.0, glow(LAMP_WARM, 1.8)),
        [0.0, store_head + 0.62, front_z - 0.62],
        id_quat(),
    ));

    // Parapet coping ringing the roof, held proud of the body.
    prims.push(prim(
        solid(cuboid_tapered(
            [w + 0.5, 0.7, d + 0.5],
            0.0,
            concrete([0.6, 0.6, 0.61]),
        )),
        [0.0, base_h + body_h + 0.35, 0.0],
        id_quat(),
    ));
    // Rooftop air-handling unit, set toward the back.
    prims.push(prim(
        solid(cuboid_tapered([2.4, 1.2, 2.0], 0.0, steel(STEEL_GREY))),
        [-2.5, base_h + body_h + 1.2, 1.6],
        id_quat(),
    ));
    // A vent stack beside it.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.5, 1.6, 0.5],
            0.0,
            steel([0.45, 0.46, 0.48]),
        )),
        [1.8, base_h + body_h + 1.4, 1.6],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: the rooftop unit steaming with a steady hum.
    root.children.push(fx::vent_steam(
        [-2.5, base_h + body_h + 2.4, 1.6],
        0x0FF1_CE10,
    ));
    root.audio = fx::ac_hum();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_cards_do_not_overlap, assert_sanitize_stable, window_cards,
    };
    use crate::pds::{GeneratorKind, SovereignTextureConfig};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&OfficeBlock.build(""), "office_block");
    }

    /// #972: the ground-storey glazing meets the lobby floor.
    ///
    /// The band used to be placed by eye at 1.0 m, so from the pavement the
    /// building showed half a metre of blank plinth and then another half
    /// metre of blank concrete before any glass — the one thing the user
    /// asked to have fixed. It is now derived from the plinth, and this pins
    /// the derivation: the lowest card's sill sits within a low kerb's height
    /// of the floor, and never below it.
    #[test]
    fn shopfront_glazing_sits_on_the_lobby_floor() {
        let cards = window_cards(&OfficeBlock.build(""));
        let sill = cards
            .iter()
            .map(|c| c.center[1] - c.size[1] * 0.5)
            .fold(f32::MAX, f32::min);
        let floor = 0.5; // plinth top
        assert!(
            sill > floor - 0.1,
            "glazing sill at {sill} runs below the lobby floor at {floor}"
        );
        assert!(
            sill < floor + 0.45,
            "glazing sill at {sill} floats {} above the lobby floor — the \
             street sees blank concrete where the shopfront should be",
            sill - floor
        );
    }

    /// #972: no two glazed surfaces share a plane and overlap. The shopfront
    /// band used to run 0.4 m up into the bottom of the curtain wall, both on
    /// `front_z`, which is two alpha-masked frames tying for depth.
    #[test]
    fn glazed_surfaces_do_not_collide() {
        assert_cards_do_not_overlap(&OfficeBlock.build(""), "office_block");
    }

    /// The entrance is cut *into* the shopfront, so its head must stay under
    /// the glazing's. Both are derived from [`GROUND_H`]; this pins that they
    /// stay derived from each other rather than drifting apart by hand.
    #[test]
    fn entrance_head_stays_under_the_shopfront_head() {
        let cards = window_cards(&OfficeBlock.build(""));
        let doors = cards
            .iter()
            .find(|c| c.center[2] < -5.0)
            .expect("the entrance leaves stand proud of the glazing plane");
        let band = cards
            .iter()
            .find(|c| (c.center[2] + 5.0).abs() < 1e-4 && c.center[1] < 3.0)
            .expect("the shopfront band sits on the wall plane");
        let door_head = doors.center[1] + doors.size[1] * 0.5;
        let band_head = band.center[1] + band.size[1] * 0.5;
        assert!(
            door_head < band_head,
            "entrance head {door_head} stands above the shopfront head {band_head}"
        );
    }

    /// #942: every `Window` card sits on a `Plane` at `uv_scale` 1.0, so the
    /// glazing spans its opening once instead of tiling per-metre on a slab.
    #[test]
    fn glazing_cards_are_unscaled_planes() {
        use crate::pds::material_finish::node_materials_mut;

        fn walk(g: &mut Generator) {
            let tag = g.kind.kind_tag();
            let is_plane = matches!(g.kind, GeneratorKind::Plane { .. });
            for m in node_materials_mut(&mut g.kind) {
                if matches!(m.texture, SovereignTextureConfig::Window(_)) {
                    assert!(is_plane, "Window card must sit on a Plane, found {tag}");
                    assert_eq!(
                        m.uv_scale.0, 1.0,
                        "Window cards upload clamp-to-edge; uv_scale must stay 1.0"
                    );
                }
            }
            for c in &mut g.children {
                walk(c);
            }
        }
        walk(&mut OfficeBlock.build(""));
    }
}
