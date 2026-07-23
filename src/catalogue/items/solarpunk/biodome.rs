//! Biodome — the Solarpunk landmark and the kit's lit hero. A faceted glass
//! geodesic dome over a ring of planted soil, banded by white steel frame
//! rings and lit from within by a soft green glow. ~13 m across, so it
//! anchors the eco-quarter and reads as the conservatory from across the home
//! region. Its dome glass and interior glow are the trim escalation's ruin
//! pass snuffs to a dark, dead shell.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the concrete ring.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::space_outpost::dome_ribs;
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, foundation_disc, glow, id_quat, plane, prim,
    quat_x, solid, sphere, torus, window_card, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_PALE, CROP_GREEN, DOME_GLOW, GLASS_CLEAN, LEAF_GREEN, STEEL_WHITE, concrete,
    crop_tufts, foliage, fx, steel,
};

pub struct Biodome;

impl CatalogueEntry for Biodome {
    fn slug(&self) -> &'static str {
        "biodome"
    }
    fn name(&self) -> &'static str {
        "Biodome"
    }
    fn description(&self) -> &'static str {
        "Faceted glass geodesic dome over planted soil, steel-banded and lit from within."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SOLAR_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 50.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let ring_h = 1.0_f32;
    let ring_top = ring_h;
    let drum_r = 6.2_f32; // round concrete planter drum
    let dome_r = 5.8_f32; // geodesic rib-cage radius
    let glass_r = 5.7_f32; // glass shell just inside the ribs so they stand proud

    let mut prims = vec![
        // Round concrete planter drum — the root (a real ring base, not a flat
        // square slab).
        prim(
            solid(cylinder_tapered(
                drum_r,
                ring_h,
                28,
                0.0,
                concrete(CONCRETE_PALE),
            )),
            [0.0, ring_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    prims.push(foundation_disc(drum_r - 0.4, 1.2));

    // Planted soil inside the drum + a leafy interior garden seen through the
    // glass.
    prims.push(prim(
        solid(cylinder_tapered(
            dome_r - 0.5,
            0.4,
            28,
            0.0,
            foliage(LEAF_GREEN),
        )),
        [0.0, ring_top + 0.15, 0.0],
        id_quat(),
    ));
    prims.extend(crop_tufts(
        [0.0, ring_top + 0.35, 0.0],
        [8.0, 8.0],
        5,
        5,
        1.0,
        foliage(CROP_GREEN),
    ));

    // Faceted glass dome — an upper hemisphere seated on the drum, lit from
    // within so it glows green. An emissive glaze, not a `Window` texture
    // (which would tile in postage-stamp panes over the sphere and can't be
    // translucent anyway); the geodesic ribs below give it its faceting.
    prims.push(prim(
        solid(with_cut(
            sphere(glass_r, 6, glow(DOME_GLOW, 0.9)),
            [0.0, 1.0],
            [0.5, 1.0],
            0.0,
        )),
        [0.0, ring_top, 0.0],
        id_quat(),
    ));
    // Steel base ring where the dome springs from the drum.
    prims.push(prim(
        solid(torus(0.12, dome_r, steel(STEEL_WHITE))),
        [0.0, ring_top, 0.0],
        id_quat(),
    ));
    // Geodesic steel rib cage standing proud of the glass — the paneled
    // habitat-dome read (reused from the space-outpost habitat dome).
    prims.extend(dome_ribs(
        [0.0, ring_top, 0.0],
        dome_r,
        8,
        steel(STEEL_WHITE),
    ));

    // Glazed entrance on the -Z hero front, steel-framed: a cut window card on
    // a plane over a green-lit chamber, so the panes reveal the garden light
    // inside rather than tiling a `Window` texture across a slab.
    let zf = -(drum_r + 0.02);
    prims.push(prim(
        cuboid_tapered([1.8, 2.0, 0.12], 0.0, glow(DOME_GLOW, 1.3)),
        [0.0, ring_top + 0.1, zf + 0.28],
        id_quat(),
    ));
    prims.push(prim(
        plane([2.0, 2.2], window_card(GLASS_CLEAN, 3, 3, 0.35, 0.06)),
        [0.0, ring_top + 0.1, zf + 0.12],
        quat_x(-FRAC_PI_2),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.2, 2.5, 0.32], 0.0, steel(STEEL_WHITE))),
            [sx * 1.1, ring_top + 0.15, zf],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([2.5, 0.24, 0.32], 0.0, steel(STEEL_WHITE))),
        [0.0, ring_top + 1.4, zf],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: clean-air breeze and drifting pollen.
    root.audio = fx::breeze_calm();
    root.children
        .push(fx::pollen_drift([0.0, ring_top + 2.0, 0.0], 0x501A_D03E));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::pds::{GeneratorKind, SovereignTextureConfig};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Biodome.build(""), "biodome");
    }

    #[test]
    fn has_glowing_dome() {
        assert!(crate::catalogue::items::util::has_emissive(
            &Biodome.build("")
        ));
    }

    /// #953: every `Window` card sits on a `Plane` at `uv_scale` 1.0 (the flat
    /// entrance; the curved dome carries no card), and — a landmark embedded in
    /// room records — the tree survives a serde round-trip.
    #[test]
    fn glazing_is_planes_and_round_trips() {
        use crate::pds::material_finish::node_materials_mut;
        fn walk(g: &mut Generator) {
            let tag = g.kind.kind_tag();
            let is_plane = matches!(g.kind, GeneratorKind::Plane { .. });
            for m in node_materials_mut(&mut g.kind) {
                if matches!(m.texture, SovereignTextureConfig::Window(_)) {
                    assert!(is_plane, "Window card must sit on a Plane, found {tag}");
                    assert_eq!(m.uv_scale.0, 1.0, "Window cards must stay at uv_scale 1.0");
                }
            }
            for c in &mut g.children {
                walk(c);
            }
        }
        let mut g = Biodome.build("");
        walk(&mut g);
        let back: Generator = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert!(
            !crate::state::records_differ(&g, &back),
            "biodome must survive a serde round-trip"
        );
    }
}
