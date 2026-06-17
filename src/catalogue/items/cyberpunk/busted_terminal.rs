//! Busted terminal — a Cyberpunk *poor* prop. A leaning public access
//! terminal, its cracked screen guttering a dim glow; the broken-down
//! cousin of the [`neon_kiosk`](super::neon_kiosk).

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_LIME, RUST_BROWN, fx, metal, rust};

pub struct BustedTerminal;

impl CatalogueEntry for BustedTerminal {
    fn slug(&self) -> &'static str {
        "busted_terminal"
    }
    fn name(&self) -> &'static str {
        "Busted Terminal"
    }
    fn description(&self) -> &'static str {
        "Leaning public terminal with a cracked, dimly guttering screen."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CYBER_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let body_h = 1.8;
    assemble(vec![
        // Leaning terminal body (root) — a slight permanent tilt.
        prim(
            solid(cuboid_tapered([0.9, body_h, 0.6], 0.05, metal(DARK_METAL))),
            [0.0, body_h * 0.5, 0.0],
            quat_x(0.1),
        ),
        // A rust-streaked base block.
        prim(
            solid(cuboid_tapered([1.0, 0.3, 0.7], 0.0, rust(RUST_BROWN))),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
        // Cracked screen, faintly lit, on the front face — fizzing and
        // spitting sparks from the shorted panel.
        {
            let mut screen = prim(
                cuboid_tapered([0.08, 0.9, 0.45], 0.0, glow(NEON_LIME, 2.5)),
                [0.45, body_h * 0.62, 0.0],
                quat_x(0.1),
            );
            screen.audio = fx::electric_crackle();
            screen
        },
        fx::spark_burst([0.5, body_h * 0.5, 0.0], 0xB057_ED00),
    ])
}
