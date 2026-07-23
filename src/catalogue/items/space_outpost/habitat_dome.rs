//! Habitat dome — the Space-Outpost landmark and the kit's lit hero. A white
//! hull module under a glazed pressure dome, a lit viewport band around its
//! waist, an airlock on one side and a beacon-topped antenna mast. ~9 m
//! across, so it anchors the base and reads as the colony from across the home
//! region. Its viewports, interior glow and beacon are the trim escalation's
//! ruin pass snuffs to a cold, dead shell.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the pad.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, foundation_disc, glow, id_quat, prim, solid,
    sphere, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BEACON_RED, GLASS_CYAN, HULL_PANEL, HULL_WHITE, PAD_GREY, STEEL_DARK, VIEWPORT_LIT, concrete,
    dome_ribs, fx, hull, pressure_hatch, steel,
};

pub struct HabitatDome;

impl CatalogueEntry for HabitatDome {
    fn slug(&self) -> &'static str {
        "habitat_dome"
    }
    fn name(&self) -> &'static str {
        "Habitat Dome"
    }
    fn description(&self) -> &'static str {
        "White hull module under a glazed pressure dome with a lit viewport band and beacon."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SpaceOutpost]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::OUTPOST_BAND
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
    let pad_h = 0.6_f32;
    let module_h = 3.0_f32;
    let module_r = 4.0_f32;
    let module_top = pad_h + module_h;
    // Drum ring seating the dome, sunk a touch into the module so their caps
    // don't sit coplanar.
    let drum_top = module_top + 0.5;
    let dome_r = 4.0_f32;
    let apex = drum_top + dome_r;

    let mut prims = vec![
        // Ceramic concrete pad — the root.
        prim(
            solid(cuboid_tapered([9.0, pad_h, 9.0], 0.0, concrete(PAD_GREY))),
            [0.0, pad_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    prims.push(foundation_disc(4.6, 1.0));

    // White hull module.
    prims.push(prim(
        solid(cylinder_tapered(
            module_r,
            module_h,
            20,
            0.0,
            hull(HULL_WHITE),
        )),
        [0.0, pad_h + module_h * 0.5, 0.0],
        id_quat(),
    ));
    // Lit viewport band around the waist — a smooth emissive ring. (A `Window`
    // texture here would tile ~1/m around the drum and read as a tinted band,
    // not glazing; the ports read from the emissive glow + the porthole ring.)
    prims.push(prim(
        cylinder_tapered(module_r + 0.08, 1.0, 20, 0.0, glow(GLASS_CYAN, 1.3)),
        [0.0, pad_h + 1.5, 0.0],
        id_quat(),
    ));
    // A ring of lit portholes around the upper module — emissive.
    for i in 0..8 {
        let a = i as f32 / 8.0 * TAU;
        prims.push(prim(
            sphere(0.3, 4, glow(VIEWPORT_LIT, 1.6)),
            [a.cos() * module_r, pad_h + 2.5, a.sin() * module_r],
            id_quat(),
        ));
    }

    // Drum collar seating the dome — stood a touch proud of the module (radius
    // module_r + 0.12) so its side never sits coplanar with the hull below it.
    // Both were module_r before, which z-fought where they overlap.
    prims.push(prim(
        solid(cylinder_tapered(
            module_r + 0.12,
            0.6,
            20,
            0.0,
            hull(HULL_PANEL),
        )),
        [0.0, module_top + 0.2, 0.0],
        id_quat(),
    ));

    // Glazed pressure dome — an upper hemisphere on the drum, not a buried
    // sphere — lit from within so it glows cyan. This is an emissive glaze, not
    // a `Window` texture (which would tile in postage-stamp panes over the
    // whole sphere); the geodesic ribs below give it its paneling.
    prims.push(prim(
        solid(with_cut(
            sphere(dome_r - 0.08, 6, glow(GLASS_CYAN, 1.2)),
            [0.0, 1.0],
            [0.5, 1.0],
            0.0,
        )),
        [0.0, drum_top, 0.0],
        id_quat(),
    ));
    // Geodesic rib cage standing proud of the glass — the habitat signature.
    for rib in dome_ribs([0.0, drum_top, 0.0], dome_r, 6, steel(STEEL_DARK)) {
        prims.push(rib);
    }
    // Apex hub finial capping the ribs.
    prims.push(prim(
        solid(cylinder_tapered(0.4, 0.4, 12, 0.4, steel(STEEL_DARK))),
        [0.0, apex - 0.1, 0.0],
        id_quat(),
    ));

    // Airlock module protruding on the −Z hero face, with a round pressure
    // hatch + lit port.
    prims.push(prim(
        solid(cuboid_tapered([2.6, 2.4, 2.0], 0.0, hull(HULL_WHITE))),
        [0.0, pad_h + 1.2, -4.4],
        id_quat(),
    ));
    for piece in pressure_hatch(
        [0.0, pad_h + 1.2, -5.45],
        0.85,
        -1.0,
        hull(HULL_PANEL),
        steel(STEEL_DARK),
        glow(VIEWPORT_LIT, 2.0),
    ) {
        prims.push(piece);
    }

    // Antenna mast topped by a red beacon — emissive.
    prims.push(prim(
        solid(cylinder_tapered(0.1, 2.4, 8, 0.0, steel(STEEL_DARK))),
        [0.0, apex + 1.0, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        sphere(0.3, 4, glow(BEACON_RED, 2.6)),
        [0.0, apex + 2.4, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: the reactor hum and skating regolith dust.
    root.audio = fx::reactor_hum();
    root.children
        .push(fx::regolith_dust([0.0, pad_h + 0.3, -6.0], 0x5EA0_D03E));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::pds::{GeneratorKind, SovereignTextureConfig};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&HabitatDome.build(""), "habitat_dome");
    }

    #[test]
    fn has_lit_viewports() {
        assert!(crate::catalogue::items::util::has_emissive(
            &HabitatDome.build("")
        ));
    }

    /// #951: no `Window` alpha-card is left on a curved surface — every one (if
    /// any) must sit on a `Plane` at `uv_scale` 1.0 — and, this being a
    /// landmark embedded in room records, the tree survives a serde round-trip.
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
        let mut g = HabitatDome.build("");
        walk(&mut g);
        let back: Generator = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert!(
            !crate::state::records_differ(&g, &back),
            "habitat dome must survive a serde round-trip"
        );
    }
}
