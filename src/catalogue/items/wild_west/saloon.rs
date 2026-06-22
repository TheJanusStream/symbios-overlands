//! Saloon — the Wild-West landmark and the kit's lit hero. A two-storey red
//! clapboard saloon with a tall false-front parapet, a covered porch and
//! upstairs gallery, lit amber windows and a hanging sign. ~10 m wide, so it
//! anchors the boomtown and reads as the saloon from across the home region.
//! Its windows are the trim escalation's ruin pass snuffs to a dark front.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`]. The false front is only the
//! parapet *above* the roofline so it never buries the storefront; the lit
//! windows and doorway are proud panels on the body's −Z hero wall (render
//! FRONT = −Z).

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, foundation_block, glow, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CLAP_RED, CLAP_TAN, CLAP_WHITE, GLASS_WARM, IRON_DARK, TIN_GREY, WOOD_RAW, clapboard, fx,
    glass, iron, tin,
};

pub struct Saloon;

impl CatalogueEntry for Saloon {
    fn slug(&self) -> &'static str {
        "saloon"
    }
    fn name(&self) -> &'static str {
        "Saloon"
    }
    fn description(&self) -> &'static str {
        "Two-storey clapboard saloon with a false front, porch gallery and lit windows."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FRONTIER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 11.0,
            min_spawn_dist: 52.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let slab_h = 0.3_f32;
    let body_w = 8.0_f32;
    let body_h = 6.0_f32;
    let body_d = 7.0_f32;
    let body_top = slab_h + body_h; // 6.3
    // Render FRONT = −Z — the body's front wall is the lit hero face; openings
    // sit proud of it (more negative Z) so they are never buried.
    let front_z = -body_d * 0.5; // -3.5
    let open_z = front_z - 0.12; // proud storefront panes/doors
    let frame_z = front_z - 0.05; // frames sit just behind the panes

    let mut prims = vec![
        // Clapboard floor slab — the root.
        prim(
            solid(cuboid_tapered(
                [10.0, slab_h, 8.0],
                0.0,
                clapboard(WOOD_RAW),
            )),
            [0.0, slab_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    prims.push(foundation_block(10.0, 8.0, [0.0, 0.0], 1.2));

    // Red clapboard body.
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w, body_h, body_d],
            0.0,
            clapboard(CLAP_RED),
        )),
        [0.0, slab_h + body_h * 0.5, 0.0],
        id_quat(),
    ));
    // White corner pilasters framing the front wall.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.32, body_h, 0.32],
                0.0,
                clapboard(CLAP_WHITE),
            )),
            [
                sx * (body_w * 0.5 - 0.04),
                slab_h + body_h * 0.5,
                front_z + 0.08,
            ],
            id_quat(),
        ));
    }
    // Low tin roof.
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w + 0.4, 0.4, body_d + 0.4],
            0.0,
            tin(TIN_GREY),
        )),
        [0.0, body_top + 0.2, 0.0],
        id_quat(),
    ));

    // False front: a tall parapet rising ABOVE the roofline (so it never
    // buries the storefront below), with an overhanging cornice + sign band.
    let para_z = front_z - 0.15;
    let para_face = para_z - 0.2;
    let para_h = 3.3_f32;
    let para_cy = body_top + para_h * 0.5;
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w + 0.8, para_h, 0.4],
            0.0,
            clapboard(CLAP_RED),
        )),
        [0.0, para_cy, para_z],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w + 1.3, 0.34, 0.8],
            0.0,
            clapboard(CLAP_WHITE),
        )),
        [0.0, body_top + para_h + 0.15, para_z],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([5.4, 1.2, 0.16], 0.0, clapboard(CLAP_WHITE))),
        [0.0, body_top + 1.4, para_face - 0.08],
        id_quat(),
    ));

    // Ground floor: a glowing batwing doorway flanked by two tall lit windows.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.8, 2.5, 0.12],
            0.0,
            clapboard([0.42, 0.3, 0.18]),
        )),
        [0.0, slab_h + 1.25, frame_z],
        id_quat(),
    ));
    // Warm light spilling out of the saloon behind the batwings.
    prims.push(prim(
        cuboid_tapered([1.2, 1.9, 0.06], 0.0, glow([1.0, 0.6, 0.25], 2.4)),
        [0.0, slab_h + 1.1, front_z - 0.02],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.72, 1.4, 0.12], 0.0, clapboard(CLAP_TAN))),
            [sx * 0.45, slab_h + 1.05, open_z],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        // White frame surround behind a bright amber pane.
        prims.push(prim(
            solid(cuboid_tapered(
                [1.95, 2.45, 0.08],
                0.0,
                clapboard(CLAP_WHITE),
            )),
            [sx * 2.5, slab_h + 1.65, frame_z],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([1.7, 2.2, 0.12], 0.0, glass(GLASS_WARM, 2.8)),
            [sx * 2.5, slab_h + 1.65, open_z],
            id_quat(),
        ));
        // Warm porch lantern on an iron bracket.
        prims.push(prim(
            solid(cuboid_tapered([0.08, 0.08, 0.45], 0.0, iron(IRON_DARK))),
            [sx * 1.4, slab_h + 2.45, front_z - 0.3],
            id_quat(),
        ));
        prims.push(prim(
            solid(sphere(0.17, 3, glow([1.0, 0.5, 0.14], 3.4))),
            [sx * 1.4, slab_h + 2.35, front_z - 0.55],
            id_quat(),
        ));
    }

    // Upstairs gallery: floor on posts, a balustrade, lit windows + a door.
    let gallery_y = slab_h + 3.4;
    let gallery_front = front_z - 1.3;
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w + 0.6, 0.22, 1.3],
            0.0,
            clapboard(WOOD_RAW),
        )),
        [0.0, gallery_y, front_z - 0.65],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.2, gallery_y, 0.2],
                0.0,
                clapboard(WOOD_RAW),
            )),
            [sx * 3.7, gallery_y * 0.5, gallery_front + 0.1],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w + 0.6, 0.16, 1.0],
            0.0,
            clapboard(WOOD_RAW),
        )),
        [0.0, slab_h + 0.08, gallery_front + 0.35],
        id_quat(),
    ));
    // Balustrade: top rail + turned balusters.
    prims.push(prim(
        cuboid_tapered([body_w + 0.6, 0.12, 0.12], 0.0, clapboard(CLAP_WHITE)),
        [0.0, gallery_y + 0.65, gallery_front],
        id_quat(),
    ));
    let balusters = 11;
    for i in 0..balusters {
        let t = i as f32 / (balusters - 1) as f32;
        prims.push(prim(
            cuboid_tapered([0.06, 0.55, 0.06], 0.0, clapboard(CLAP_WHITE)),
            [-3.8 + t * 7.6, gallery_y + 0.35, gallery_front],
            id_quat(),
        ));
    }
    // Upper floor: a balcony door flanked by two more lit windows.
    prims.push(prim(
        solid(cuboid_tapered([1.0, 1.9, 0.12], 0.0, clapboard(CLAP_TAN))),
        [0.0, gallery_y + 1.15, frame_z],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [1.45, 1.65, 0.08],
                0.0,
                clapboard(CLAP_WHITE),
            )),
            [sx * 2.6, gallery_y + 1.25, frame_z],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([1.25, 1.45, 0.12], 0.0, glass(GLASS_WARM, 2.5)),
            [sx * 2.6, gallery_y + 1.25, open_z],
            id_quat(),
        ));
    }

    // Hanging perpendicular sign on an iron bracket at the corner.
    prims.push(prim(
        solid(cuboid_tapered([0.1, 0.1, 1.0], 0.0, iron(IRON_DARK))),
        [-3.6, slab_h + 5.0, front_z - 0.5],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.12, 0.9, 1.4], 0.0, clapboard(CLAP_WHITE))),
        [-3.6, slab_h + 4.3, front_z - 1.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: a dry prairie wind, dust skating the street.
    root.audio = fx::prairie_wind();
    root.children
        .push(fx::dust_drift([0.0, 0.3, front_z - 3.5], 0x0DE5_5A12));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Saloon.build(""), "saloon");
    }

    #[test]
    fn has_lit_windows() {
        assert!(crate::catalogue::items::util::has_emissive(
            &Saloon.build("")
        ));
    }
}
