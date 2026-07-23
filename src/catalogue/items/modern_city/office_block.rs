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

    // Glazing envelope (shared by the interior and the glass plane).
    let gw = w - 1.0;
    let gh = body_h - 2.4;
    let gy = body_cy + 0.6;
    let g_bottom = gy - gh * 0.5;
    let bays = (4u32, 5u32);
    let row_h = gh / bays.1 as f32;

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

    // Lit lobby floor glow and a reception desk, set in the cavity so they
    // show through the storefront glazing.
    prims.push(prim(
        cuboid_tapered([gw - 0.8, 0.12, cavity - 0.4], 0.0, glow(OFFICE_WARM, 1.1)),
        [0.0, base_h + 2.15, cav_mid],
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
    // Storefront glazing — wide clear panes over the lobby, flanking the
    // central entrance portal.
    prims.push(prim(
        plane([gw, 1.7], window_card(GLASS_TEAL, 8, 1, 0.3, 0.02)),
        [0.0, base_h + 1.35, front_z],
        quat_x(-FRAC_PI_2),
    ));

    // Dark entrance portal recess + glass doors, proud of the storefront so
    // the doors read in front of the glazing.
    prims.push(prim(
        solid(cuboid_tapered(
            [3.0, 2.5, 0.4],
            0.0,
            steel([0.16, 0.17, 0.2]),
        )),
        [0.0, base_h + 1.25, front_z - 0.2],
        id_quat(),
    ));
    prims.push(prim(
        plane([2.4, 2.1], window_card([0.14, 0.18, 0.2], 2, 1, 0.32, 0.05)),
        [0.0, base_h + 1.05, front_z - 0.42],
        quat_x(-FRAC_PI_2),
    ));
    // Steel entrance canopy cantilevered over the doors.
    prims.push(prim(
        solid(cuboid_tapered([5.4, 0.3, 2.2], 0.0, steel(STEEL_GREY))),
        [0.0, base_h + 3.0, front_z - 1.0],
        id_quat(),
    ));
    // Warm lit address band above the canopy.
    prims.push(prim(
        cuboid_tapered([4.2, 0.55, 0.18], 0.0, glow(LAMP_WARM, 1.8)),
        [0.0, base_h + 3.7, front_z - 0.3],
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
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::pds::{GeneratorKind, SovereignTextureConfig};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&OfficeBlock.build(""), "office_block");
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
