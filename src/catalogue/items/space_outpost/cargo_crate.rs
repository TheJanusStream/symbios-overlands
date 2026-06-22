//! Cargo crate — a Space-Outpost prop. A stack of hull supply containers with
//! hazard stencils. Scatter clutter of the base's stores.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid, sphere};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    HAZARD_YELLOW, HULL_PANEL, HULL_WHITE, SCORCH, STATUS_GREEN, STEEL_DARK, hull, painted, steel,
};

pub struct CargoCrate;

impl CatalogueEntry for CargoCrate {
    fn slug(&self) -> &'static str {
        "cargo_crate"
    }
    fn name(&self) -> &'static str {
        "Cargo Crate"
    }
    fn description(&self) -> &'static str {
        "Stack of hull supply containers with hazard stencils."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SpaceOutpost]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::OUTPOST_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.2,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// A ribbed shipping container — a body cuboid with proud corner castings and
/// vertical wall ribs on the ±Z faces — returned as positioned prims for the
/// assemble list. Reads as a real cargo container rather than a plain box.
fn crate_box(
    size: [f32; 3],
    center: [f32; 3],
    body: SovereignMaterialSettings,
    frame: SovereignMaterialSettings,
) -> Vec<Generator> {
    let [w, h, d] = size;
    let [cx, cy, cz] = center;
    let mut out = vec![prim(
        solid(cuboid_tapered(size, 0.0, body)),
        center,
        id_quat(),
    )];
    // Corner castings at the four vertical edges.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            out.push(prim(
                solid(cuboid_tapered([0.12, h + 0.05, 0.12], 0.0, frame.clone())),
                [cx + sx * w * 0.5, cy, cz + sz * d * 0.5],
                id_quat(),
            ));
        }
    }
    // Vertical corrugation ribs on the ±Z faces.
    for sz in [-1.0_f32, 1.0] {
        for fx in [-0.3_f32, 0.0, 0.3] {
            out.push(prim(
                cuboid_tapered([0.07, h * 0.85, 0.05], 0.0, frame.clone()),
                [cx + fx * w, cy, cz + sz * (d * 0.5 + 0.025)],
                id_quat(),
            ));
        }
    }
    out
}

fn build_tree() -> Generator {
    // Large base crate — the root (its body is prims[0]).
    let mut prims = crate_box(
        [1.4, 1.1, 1.4],
        [0.0, 0.55, 0.0],
        hull(HULL_PANEL),
        steel(STEEL_DARK),
    );
    // Recessed end-door panel + hazard placard on the −Z front.
    prims.push(prim(
        solid(cuboid_tapered([0.85, 0.85, 0.05], 0.0, hull(HULL_WHITE))),
        [0.0, 0.55, -0.69],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.55, 0.32, 0.04], 0.0, painted(HAZARD_YELLOW)),
        [0.0, 0.8, -0.74],
        id_quat(),
    ));

    // A second (refrigerated) crate alongside, with a green status LED.
    prims.extend(crate_box(
        [1.0, 0.9, 1.0],
        [1.3, 0.45, 0.15],
        hull(HULL_WHITE),
        steel(STEEL_DARK),
    ));
    prims.push(prim(
        sphere(0.06, 4, glow(STATUS_GREEN, 2.0)),
        [1.3, 0.72, -0.4],
        id_quat(),
    ));

    // A smaller scorched crate stacked on top.
    prims.extend(crate_box(
        [0.8, 0.6, 0.8],
        [0.05, 1.4, 0.12],
        hull(SCORCH),
        steel(STEEL_DARK),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CargoCrate.build(""), "cargo_crate");
    }
}
