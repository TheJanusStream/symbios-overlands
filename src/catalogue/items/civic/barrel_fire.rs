//! Barrel fire — a rusted oil drum burnt out into an open-topped brazier,
//! with a live fire down inside it. A prosperity-Poor scatter prop: the
//! universal sign of people keeping warm on the margins, in any setting.
//!
//! The fire is entirely particle-driven ([`super::fx`]) — two flame layers,
//! embers, and two smoke layers. The only static hot geometry is the fuel
//! bed itself: the coals, which genuinely *are* solid objects sitting still
//! at the bottom of the drum. Everything above them is combustion, and
//! combustion modelled as fixed cones reads as an orange plastic decoration
//! no matter how it is shaded.

use crate::catalogue::items::util::{
    cylinder_tapered, glow, id_quat, prim, quat_x, solid, sphere, torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{EMBER, RUST, quat_z, rust_metal};

pub struct BarrelFire;

impl CatalogueEntry for BarrelFire {
    fn slug(&self) -> &'static str {
        "barrel_fire"
    }
    fn name(&self) -> &'static str {
        "Barrel Fire"
    }
    fn description(&self) -> &'static str {
        "Rusted oil drum burnt open at the top, with a fire down inside it."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn prosperity_band(&self) -> ProsperityBand {
        ProsperityBand::only(ProsperityTier::Poor)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Deterministic salt for the emitter seeds, so two barrels in one room
/// still animate in lockstep-free but reproducible fashion.
const FX_SEED: u64 = 0x0BA9_9E1F;

fn build_tree() -> Generator {
    let drum_h = 0.9;
    let drum_r = 0.34;
    // Fraction of the radius bored out. The remaining 12% is the steel
    // wall — thin enough to read as sheet metal at the rim, thick enough
    // that the top annulus is a visible lip rather than a hairline.
    let bore = 0.88;
    // Where the fuel sits. Everything that burns lives between here and
    // the rim, i.e. *inside* the drum, lighting the bore from within.
    let bed_y = 0.26;

    let mut prims = vec![
        // The drum: a hollowed cylinder, so the top is a genuine opening
        // with an annular lip and an inner wall you can see down. The
        // hollow cut routes the collider to a convex hull of the mesh,
        // which fills the bore — a barrel you cannot step into, which is
        // the behaviour we want.
        prim(
            solid(with_cut(
                cylinder_tapered(drum_r, drum_h, 16, 0.0, rust_metal(RUST)),
                [0.0, 1.0],
                [0.0, 1.0],
                bore,
            )),
            [0.0, drum_h * 0.5, 0.0],
            id_quat(),
        ),
        // Two raised hoop bands (round, proud of the wall).
        prim(
            torus(0.035, drum_r + 0.01, rust_metal([0.3, 0.16, 0.1])),
            [0.0, drum_h * 0.3, 0.0],
            id_quat(),
        ),
        prim(
            torus(0.035, drum_r + 0.01, rust_metal([0.3, 0.16, 0.1])),
            [0.0, drum_h * 0.72, 0.0],
            id_quat(),
        ),
        // Charred rolled lip, centred on the wall annulus so it caps the
        // opening rather than floating inside or outside it.
        prim(
            torus(
                0.026,
                drum_r * (1.0 + bore) * 0.5,
                rust_metal([0.1, 0.09, 0.08]),
            ),
            [0.0, drum_h, 0.0],
            id_quat(),
        ),
        // Ash floor a little way up the bore — without it you see straight
        // through the annular bottom cap to the ground.
        prim(
            cylinder_tapered(
                drum_r * bore - 0.01,
                0.06,
                14,
                0.0,
                rust_metal([0.14, 0.12, 0.11]),
            ),
            [0.0, 0.11, 0.0],
            id_quat(),
        ),
        // The coal bed resting on the ash: a deep-saturated ember mass at
        // moderate strength so it reads hot, not a washed near-white blob.
        prim(
            sphere(0.2, 3, glow(EMBER, 3.5)),
            [0.0, bed_y, 0.0],
            id_quat(),
        ),
    ];

    // Small bright coals nestled in the bed, all well within the bore.
    for (dx, dz) in [(-0.1_f32, 0.05_f32), (0.11, -0.04), (0.0, 0.11)] {
        prims.push(prim(
            sphere(0.06, 3, glow([1.0, 0.34, 0.06], 3.0)),
            [dx, bed_y + 0.12, dz],
            id_quat(),
        ));
    }

    // Two charred fuel stubs standing in the drum, leaning on the wall.
    // Their tips stop short of the lip: fuel is *in* the barrel, and a
    // stick crossing the rim was the tell that the old fire was a hat
    // sitting on a closed drum rather than a fire burning inside one.
    for (pos, rot) in [
        ([0.09_f32, 0.56_f32, 0.02_f32], quat_z(0.34)),
        ([-0.04, 0.5, 0.08], quat_x(-0.4)),
    ] {
        prims.push(prim(
            cylinder_tapered(0.022, 0.46, 6, 0.25, rust_metal([0.16, 0.11, 0.08])),
            pos,
            rot,
        ));
    }

    // The fire itself. Layered bottom-up: body and core overlap just above
    // the coals and carry themselves out through the opening, embers shed
    // off the tips, and the smoke column hands off from soot to pale plume
    // as it climbs.
    prims.push(super::fx::flame_body([0.0, bed_y + 0.14, 0.0], FX_SEED));
    prims.push(super::fx::flame_core(
        [0.0, bed_y + 0.22, 0.0],
        FX_SEED ^ 0x11,
    ));
    prims.push(super::fx::embers([0.0, bed_y + 0.4, 0.0], FX_SEED ^ 0x22));
    prims.push(super::fx::smoke_soot([0.0, 1.15, 0.0], FX_SEED ^ 0x33));
    prims.push(super::fx::smoke_plume([0.0, 1.95, 0.0], FX_SEED ^ 0x44));

    super::assemble(prims)
}
