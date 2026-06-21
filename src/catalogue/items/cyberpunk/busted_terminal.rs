//! Busted terminal — a Cyberpunk *poor* prop. A leaning public access
//! terminal: a cracked pixel screen guttering a dim glow over a dead keypad
//! and a card slot, with a torn cable dangling from its flank. The
//! broken-down cousin of the [`neon_kiosk`](super::neon_kiosk).

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, NEON_LIME, RUST_BROWN, fx, metal, rust};

pub struct BustedTerminal;

impl CatalogueEntry for BustedTerminal {
    fn slug(&self) -> &'static str {
        "busted_terminal"
    }
    fn name(&self) -> &'static str {
        "Busted Terminal"
    }
    fn description(&self) -> &'static str {
        "Leaning public terminal with a cracked, dimly guttering pixel screen."
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
    let body_h = 1.8_f32;
    let zf = 0.3_f32; // body front face (local)

    // The body leans; built as a subtree so the screen/keypad/cable lean with
    // it, while the rust base (the assemble root) stays flat on the ground.
    let mut body = prim(
        solid(cuboid_tapered([0.9, body_h, 0.6], 0.05, metal(DARK_METAL))),
        [0.0, body_h * 0.5, 0.0],
        quat_x(0.1),
    );

    // Screen bezel.
    body.children.push(prim(
        cuboid_tapered([0.74, 0.6, 0.05], 0.0, metal([0.04, 0.04, 0.05])),
        [0.0, 0.32, zf + 0.01],
        id_quat(),
    ));
    // Cracked pixel screen — a 3×2 mosaic split by a crack (the right column
    // shoved out of line), most pixels dead, a couple still guttering. The
    // brightest carries the electrical fizz.
    let pix = [
        ([-0.22_f32, 0.45_f32], NEON_LIME, 2.5, true),
        ([0.0, 0.45], [0.03, 0.04, 0.05], 0.0, false),
        ([0.24, 0.49], NEON_CYAN, 1.6, false),
        ([-0.22, 0.21], [0.03, 0.04, 0.05], 0.0, false),
        ([0.0, 0.21], NEON_LIME, 1.2, false),
        ([0.24, 0.17], [0.03, 0.04, 0.05], 0.0, false),
    ];
    for ([px, py], col, str_, fizz) in pix {
        let mut p = prim(
            cuboid_tapered([0.2, 0.2, 0.04], 0.0, glow(col, str_)),
            [px, py, zf + 0.04],
            id_quat(),
        );
        if fizz {
            p.audio = fx::electric_crackle();
        }
        body.children.push(p);
    }
    // Dead keypad — a 3×3 grid of dim buttons.
    for r in 0..3 {
        for c in 0..3 {
            body.children.push(prim(
                cuboid_tapered([0.08, 0.07, 0.03], 0.0, metal([0.1, 0.1, 0.11])),
                [-0.16 + 0.16 * c as f32, -0.18 - 0.13 * r as f32, zf + 0.02],
                id_quat(),
            ));
        }
    }
    // Card slot + a faint green ready light.
    body.children.push(prim(
        cuboid_tapered([0.3, 0.05, 0.04], 0.0, metal([0.02, 0.02, 0.03])),
        [0.0, 0.02, zf + 0.02],
        id_quat(),
    ));
    body.children.push(prim(
        cuboid_tapered([0.06, 0.04, 0.03], 0.0, glow([0.2, 1.0, 0.3], 4.0)),
        [0.27, 0.02, zf + 0.02],
        id_quat(),
    ));
    // Torn cable dangling from the flank.
    body.children.push(prim(
        cylinder_tapered(0.035, 0.7, 6, 0.0, metal([0.05, 0.05, 0.06])),
        [0.46, -0.45, 0.0],
        quat_z(0.7),
    ));

    assemble(vec![
        // Rust-streaked base block (root, flat on the ground).
        prim(
            solid(cuboid_tapered([1.0, 0.3, 0.7], 0.0, rust(RUST_BROWN))),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
        body,
        fx::spark_burst([0.5, body_h * 0.55, 0.1], 0xB057_ED00),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&BustedTerminal.build(""), "busted_terminal");
    }

    #[test]
    fn has_neon() {
        assert!(crate::catalogue::items::util::has_emissive(
            &BustedTerminal.build("")
        ));
    }
}
