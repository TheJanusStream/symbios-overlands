//! Cyberpunk-theme catalogue structures — dark glossy-metal volumes
//! lit by saturated neon emissive trim. The first authored theme kit
//! (Phase 3.2): a neon megatower landmark, a data spire and arcade
//! block as secondaries, and a vending kiosk prop. Each is
//! primitive-built (see [`super::util`]); the neon comes from strongly
//! emissive [`super::util::glow`] materials so a Cyberpunk settlement
//! reads as a glowing cluster at distance, reinforced by the theme's
//! magenta fog accent ([`crate::seeded_defaults::room::accent`]).

pub mod arcade_block;
pub mod cable_arch;
pub mod data_spire;
pub mod drone_perch;
pub mod holo_billboard;
pub mod neon_kiosk;
pub mod neon_megatower;
pub mod parking_stack;

use crate::pds::{Fp, Fp3, SovereignMaterialSettings, SovereignTextureConfig};

/// Dark, glossy structural metal — the body shared by every cyberpunk
/// build. Low roughness + high metallic so neon trim reflects off it.
pub(super) fn metal(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.32),
        metallic: Fp(0.85),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

/// Near-black panelled body colour.
pub(super) const DARK_METAL: [f32; 3] = [0.06, 0.07, 0.10];
pub(super) const NEON_CYAN: [f32; 3] = [0.10, 0.95, 1.00];
pub(super) const NEON_MAGENTA: [f32; 3] = [1.00, 0.12, 0.78];
pub(super) const NEON_LIME: [f32; 3] = [0.55, 1.00, 0.20];

/// Walk a built tree and report whether any primitive is strongly
/// emissive — the shared "did the neon survive?" check for the kit's
/// tests.
#[cfg(test)]
pub(super) fn has_emissive(g: &crate::pds::Generator) -> bool {
    use crate::pds::GeneratorKind::*;
    let own = match &g.kind {
        Cuboid { material, .. }
        | Cylinder { material, .. }
        | Sphere { material, .. }
        | Cone { material, .. }
        | Torus { material, .. }
        | Capsule { material, .. } => material.emission_strength.0 > 1.0,
        _ => false,
    };
    own || g.children.iter().any(has_emissive)
}
