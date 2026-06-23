//! Fountain — a tiered marble basin with a central jet. A prosperity-Rich
//! scatter prop: ornamental waterworks signal civic wealth in any setting.

use crate::catalogue::items::util::{cylinder_tapered, id_quat, prim, solid, sphere, torus, tube};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{MARBLE, WATER_BLUE, marble};

/// Wet water — glossy, faintly self-lit blue so the pools read clearly
/// against the pale marble instead of vanishing into it.
fn water() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(WATER_BLUE),
        emission_color: Fp3(WATER_BLUE),
        emission_strength: Fp(0.5),
        roughness: Fp(0.12),
        metallic: Fp(0.0),
        ..Default::default()
    }
}

pub struct Fountain;

impl CatalogueEntry for Fountain {
    fn slug(&self) -> &'static str {
        "fountain"
    }
    fn name(&self) -> &'static str {
        "Fountain"
    }
    fn description(&self) -> &'static str {
        "Tiered marble basin with a central water jet."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn prosperity_band(&self) -> ProsperityBand {
        ProsperityBand::only(ProsperityTier::Rich)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.0,
            min_spawn_dist: 22.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    super::assemble(vec![
        // Basin floor disc — a solid bottom so the pool never reads hollow.
        prim(
            solid(cylinder_tapered(1.45, 0.12, 24, 0.0, marble(MARBLE))),
            [0.0, 0.06, 0.0],
            id_quat(),
        ),
        // Open marble rim wall holding the lower pool (a hollow ring).
        prim(
            solid(tube(1.5, 1.28, 0.5, 24, marble(MARBLE))),
            [0.0, 0.25, 0.0],
            id_quat(),
        ),
        // Rounded coping lip proud of the wall top.
        prim(
            torus(0.08, 1.5, marble([0.8, 0.79, 0.76])),
            [0.0, 0.5, 0.0],
            id_quat(),
        ),
        // Lower pool — a broad blue disc sitting recessed below the rim.
        prim(
            cylinder_tapered(1.26, 0.26, 24, 0.0, water()),
            [0.0, 0.27, 0.0],
            id_quat(),
        ),
        // Pedestal foot drum.
        prim(
            solid(cylinder_tapered(
                0.34,
                0.18,
                16,
                0.0,
                marble([0.8, 0.79, 0.76]),
            )),
            [0.0, 0.49, 0.0],
            id_quat(),
        ),
        // Fluted baluster shaft.
        prim(
            solid(cylinder_tapered(0.20, 0.85, 12, 0.15, marble(MARBLE))),
            [0.0, 1.0, 0.0],
            id_quat(),
        ),
        // Upper bowl, its rim and its pool.
        prim(
            solid(cylinder_tapered(0.62, 0.14, 20, 0.0, marble(MARBLE))),
            [0.0, 1.49, 0.0],
            id_quat(),
        ),
        prim(
            torus(0.05, 0.62, marble([0.8, 0.79, 0.76])),
            [0.0, 1.56, 0.0],
            id_quat(),
        ),
        prim(
            cylinder_tapered(0.55, 0.07, 20, 0.0, water()),
            [0.0, 1.57, 0.0],
            id_quat(),
        ),
        // Jet rising from the bowl and the spray orb crowning it.
        prim(
            cylinder_tapered(0.045, 0.55, 8, 0.0, water()),
            [0.0, 1.85, 0.0],
            id_quat(),
        ),
        prim(sphere(0.16, 3, water()), [0.0, 2.14, 0.0], id_quat()),
    ])
}
