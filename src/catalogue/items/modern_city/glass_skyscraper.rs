//! Glass skyscraper — the Modern-City landmark. A curtain-wall tower of lit
//! blue glass banded by steel spandrels, stepping back once near the top to
//! a flat roof of mechanical units, an antenna mast, and an aircraft-warning
//! beacon. Rooftop steam drifts over a low air-handler hum. ~46 m tall, so
//! it anchors the district and reads as a glowing tower across the region.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, foundation_block, glow, id_quat, prim, solid,
    sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, GLASS_BLUE, STEEL_GREY, concrete, fx, glass, steel};

pub struct GlassSkyscraper;

impl CatalogueEntry for GlassSkyscraper {
    fn slug(&self) -> &'static str {
        "glass_skyscraper"
    }
    fn name(&self) -> &'static str {
        "Glass Skyscraper"
    }
    fn description(&self) -> &'static str {
        "Curtain-wall tower of lit glass with a stepped roof and antenna mast."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CITY_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 16.0,
            min_spawn_dist: 65.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let plaza_h = 0.5;
    let lower_w = 12.0_f32;
    let lower_h = 30.0_f32;
    let upper_w = 9.0_f32;
    let upper_h = 16.0_f32;

    let mut prims = vec![
        // Concrete plaza base — the root.
        prim(
            solid(cuboid_tapered(
                [16.0, plaza_h, 16.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, plaza_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    let mut base = foundation_block(16.0, 16.0, [0.0, 0.0], 3.0);
    base.transform.translation.0[1] -= plaza_h * 0.5;
    prims.push(base);

    // Lower glass shaft.
    let lower_y0 = plaza_h;
    prims.push(prim(
        solid(cuboid_tapered(
            [lower_w, lower_h, lower_w],
            0.0,
            glass(GLASS_BLUE, 2.5),
        )),
        [0.0, lower_y0 + lower_h * 0.5, 0.0],
        id_quat(),
    ));
    // Steel spandrel floor bands.
    let bands = 8;
    for k in 1..bands {
        let y = lower_y0 + lower_h * (k as f32 / bands as f32);
        prims.push(prim(
            cuboid_tapered([lower_w + 0.3, 0.35, lower_w + 0.3], 0.0, steel(STEEL_GREY)),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }
    // Corner mullions up the lower shaft.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered([0.4, lower_h, 0.4], 0.0, steel(STEEL_GREY))),
            [
                sx * lower_w * 0.5,
                lower_y0 + lower_h * 0.5,
                sz * lower_w * 0.5,
            ],
            id_quat(),
        ));
    }

    // Setback ledge.
    let upper_y0 = lower_y0 + lower_h;
    prims.push(prim(
        solid(cuboid_tapered(
            [lower_w + 1.0, 0.4, lower_w + 1.0],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, upper_y0 + 0.2, 0.0],
        id_quat(),
    ));
    // Upper glass shaft.
    prims.push(prim(
        solid(cuboid_tapered(
            [upper_w, upper_h, upper_w],
            0.0,
            glass(GLASS_BLUE, 2.5),
        )),
        [0.0, upper_y0 + 0.4 + upper_h * 0.5, 0.0],
        id_quat(),
    ));
    for k in 1..5 {
        let y = upper_y0 + 0.4 + upper_h * (k as f32 / 5.0);
        prims.push(prim(
            cuboid_tapered([upper_w + 0.3, 0.3, upper_w + 0.3], 0.0, steel(STEEL_GREY)),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }

    // Flat roof with mechanical units, mast, and warning beacon.
    let roof_y = upper_y0 + 0.4 + upper_h;
    prims.push(prim(
        solid(cuboid_tapered(
            [upper_w + 0.6, 0.4, upper_w + 0.6],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, roof_y + 0.2, 0.0],
        id_quat(),
    ));
    for (cx, cz, w) in [(-2.0_f32, -1.5_f32, 2.2_f32), (1.8, 1.4, 1.6)] {
        prims.push(prim(
            solid(cuboid_tapered([w, 1.2, w], 0.0, steel(STEEL_GREY))),
            [cx, roof_y + 1.0, cz],
            id_quat(),
        ));
    }
    let mast_h = 6.0;
    prims.push(prim(
        solid(cylinder_tapered(0.16, mast_h, 8, 0.3, steel(STEEL_GREY))),
        [0.0, roof_y + 0.4 + mast_h * 0.5, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        sphere(0.3, 3, glow([1.0, 0.2, 0.15], 6.0)),
        [0.0, roof_y + 0.4 + mast_h + 0.3, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: a rooftop AC unit steaming, with its steady hum.
    root.children
        .push(fx::vent_steam([-2.0, roof_y + 1.7, -1.5], 0xC17_57EA));
    root.audio = fx::ac_hum();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&GlassSkyscraper.build(""), "glass_skyscraper");
    }

    #[test]
    fn has_lit_glass() {
        assert!(super::super::has_emissive(&GlassSkyscraper.build("")));
    }
}
