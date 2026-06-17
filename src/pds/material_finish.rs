//! Socio-political material-finish pass for seeded settlement members.
//!
//! After a catalogue entry is built into a concrete generator tree (and its
//! Shape-grammar seed restamped), [`apply_socio_finish`] walks every
//! material in that tree and nudges its PBR finish by the room's two
//! continuous socio-political dials (see
//! [`SceneCharacter`](crate::seeded_defaults::SceneCharacter)):
//!
//! - **prosperity** (poor → rich): rich surfaces lose roughness, gain
//!   metallic and emissive punch, and brighten; poor surfaces gain
//!   roughness, lose emissive, and drift toward a grimy rust-brown.
//! - **escalation** (peaceful → conflict): conflict darkens and
//!   desaturates the base colour and blends it toward soot — the scorch of
//!   a fought-over settlement.
//!
//! The pass is a pure function of the two dials (no RNG), so peers deriving
//! the same room produce bit-identical finishes. It reaches every
//! material-bearing variant — the eight primitives' `material`, `Sign`'s
//! `material`, and every value in the `Shape` / `LSystem` materials maps —
//! and recurses through `children`, so a deeply-nested construct is
//! finished uniformly. Variants with no
//! [`SovereignMaterialSettings`] (terrain, water, particles, portals) are
//! left untouched.
//!
//! Magnitudes are tuned modest: a mid-prosperity, peaceful room
//! (prosperity ≈ 0.5, escalation ≈ 0) reads essentially as the catalogue's
//! authored finish, and only the axis extremes look dramatically rich,
//! destitute, or scorched.

use super::generator::{Generator, GeneratorKind};
use super::texture::SovereignMaterialSettings;
use super::types::{Fp, Fp3};

/// Roughness swing at full wealth: rich subtracts, poor adds.
const ROUGHNESS_SWING: f32 = 0.35;
/// Metallic added at full wealth (rich only — poverty doesn't strip metal).
const METALLIC_SWING: f32 = 0.30;
/// Fractional emissive scale at the wealth extremes (±). Multiplicative, so
/// a non-emissive (black / zero-strength) material stays dark either way.
const EMISSION_SWING: f32 = 0.6;
/// Base-colour lerp toward [`GRIME_RGB`] at full poverty.
const GRIME_MAX: f32 = 0.5;
/// Base-colour brighten toward white at full wealth.
const CLEAN_MAX: f32 = 0.2;
/// Base-colour darken at full conflict.
const SCORCH_DARKEN: f32 = 0.45;
/// Base-colour desaturation at full conflict.
const SCORCH_DESAT: f32 = 0.5;
/// Base-colour lerp toward [`SOOT_RGB`] at full conflict.
const SCORCH_SOOT: f32 = 0.35;

/// Desaturated rust brown poor surfaces drift toward.
const GRIME_RGB: [f32; 3] = [0.16, 0.12, 0.09];
/// Near-black, warm-biased soot conflict surfaces drift toward.
const SOOT_RGB: [f32; 3] = [0.06, 0.055, 0.05];

/// Apply the prosperity/escalation finish to every material in `gen` and
/// all its descendants, in place. `prosperity` and `escalation` are the
/// raw `[0, 1]` [`SceneCharacter`](crate::seeded_defaults::SceneCharacter)
/// dials; both are clamped defensively.
pub fn apply_socio_finish(node: &mut Generator, prosperity: f32, escalation: f32) {
    // Wealth is centred: 0.5 prosperity → 0 (neutral), 0 → -1 (destitute),
    // 1 → +1 (affluent). Scorch ramps from 0 (peace) to 1 (open conflict).
    let wealth = (prosperity.clamp(0.0, 1.0) - 0.5) * 2.0;
    let scorch = escalation.clamp(0.0, 1.0);
    if wealth == 0.0 && scorch == 0.0 {
        return; // neutral room — leave the authored finish untouched.
    }
    finish_tree(node, wealth, scorch);
}

fn finish_tree(node: &mut Generator, wealth: f32, scorch: f32) {
    for mat in node_materials_mut(&mut node.kind) {
        finish_material(mat, wealth, scorch);
    }
    for child in &mut node.children {
        finish_tree(child, wealth, scorch);
    }
}

/// Mutable borrows of every [`SovereignMaterialSettings`] carried directly
/// by one node (not its children). Material-free variants yield an empty
/// vec.
fn node_materials_mut(kind: &mut GeneratorKind) -> Vec<&mut SovereignMaterialSettings> {
    match kind {
        GeneratorKind::Cuboid { material, .. }
        | GeneratorKind::Sphere { material, .. }
        | GeneratorKind::Cylinder { material, .. }
        | GeneratorKind::Capsule { material, .. }
        | GeneratorKind::Cone { material, .. }
        | GeneratorKind::Torus { material, .. }
        | GeneratorKind::Plane { material, .. }
        | GeneratorKind::Tetrahedron { material, .. }
        | GeneratorKind::Sign { material, .. } => vec![material],
        GeneratorKind::Shape { materials, .. } => materials.values_mut().collect(),
        GeneratorKind::LSystem { materials, .. } => materials.values_mut().collect(),
        GeneratorKind::Terrain(_)
        | GeneratorKind::Water { .. }
        | GeneratorKind::Portal { .. }
        | GeneratorKind::ParticleSystem { .. }
        | GeneratorKind::Unknown => Vec::new(),
    }
}

fn finish_material(mat: &mut SovereignMaterialSettings, wealth: f32, scorch: f32) {
    // --- Prosperity: surface finish ---
    mat.roughness = Fp((mat.roughness.0 - wealth * ROUGHNESS_SWING).clamp(0.0, 1.0));
    mat.metallic = Fp((mat.metallic.0 + wealth.max(0.0) * METALLIC_SWING).clamp(0.0, 1.0));
    mat.emission_strength =
        Fp((mat.emission_strength.0 * (1.0 + wealth * EMISSION_SWING)).max(0.0));

    // --- Base colour: prosperity tint, then escalation scorch ---
    let mut c = mat.base_color.0;
    if wealth < 0.0 {
        c = lerp3(c, GRIME_RGB, (-wealth) * GRIME_MAX);
    } else if wealth > 0.0 {
        c = lerp3(c, [1.0, 1.0, 1.0], wealth * CLEAN_MAX);
    }
    if scorch > 0.0 {
        c = desaturate3(c, scorch * SCORCH_DESAT);
        c = scale3(c, 1.0 - scorch * SCORCH_DARKEN);
        c = lerp3(c, SOOT_RGB, scorch * SCORCH_SOOT);
    }
    mat.base_color = Fp3(clamp3(c));
}

/// Linear interpolate `a → b` by `t` (caller keeps `t` in `[0, 1]`).
fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Multiply every channel by `k`.
fn scale3(a: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

/// Rec. 601 relative luminance — the grey a colour desaturates toward.
fn luminance(a: [f32; 3]) -> f32 {
    0.299 * a[0] + 0.587 * a[1] + 0.114 * a[2]
}

/// Blend toward the colour's own luminance grey by `t`.
fn desaturate3(a: [f32; 3], t: f32) -> [f32; 3] {
    let g = luminance(a);
    lerp3(a, [g, g, g], t)
}

/// Clamp every channel to `[0, 1]`.
fn clamp3(a: [f32; 3]) -> [f32; 3] {
    [
        a[0].clamp(0.0, 1.0),
        a[1].clamp(0.0, 1.0),
        a[2].clamp(0.0, 1.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn cuboid(color: [f32; 3], roughness: f32, metallic: f32, emission: f32) -> Generator {
        Generator::from_kind(GeneratorKind::Cuboid {
            size: Fp3([1.0, 1.0, 1.0]),
            solid: true,
            material: SovereignMaterialSettings {
                base_color: Fp3(color),
                emission_color: Fp3([1.0, 1.0, 1.0]),
                emission_strength: Fp(emission),
                roughness: Fp(roughness),
                metallic: Fp(metallic),
                ..SovereignMaterialSettings::default()
            },
            twist: Fp(0.0),
            taper: Fp(0.0),
            bend: Fp3([0.0, 0.0, 0.0]),
        })
    }

    fn only_material(node: &Generator) -> &SovereignMaterialSettings {
        match &node.kind {
            GeneratorKind::Cuboid { material, .. } => material,
            _ => panic!("expected cuboid"),
        }
    }

    #[test]
    fn neutral_dials_are_a_no_op() {
        let base = cuboid([0.5, 0.4, 0.3], 0.5, 0.2, 1.0);
        let mut g = base.clone();
        apply_socio_finish(&mut g, 0.5, 0.0);
        assert_eq!(
            g, base,
            "prosperity 0.5 / escalation 0 must not alter materials"
        );
    }

    #[test]
    fn rich_polishes_poor_grimes() {
        let base = cuboid([0.5, 0.4, 0.3], 0.5, 0.2, 1.0);

        let mut poor = base.clone();
        apply_socio_finish(&mut poor, 0.0, 0.0);
        let mut rich = base.clone();
        apply_socio_finish(&mut rich, 1.0, 0.0);

        let (p, r) = (only_material(&poor), only_material(&rich));
        // Rich is glossier and more metallic than poor.
        assert!(r.roughness.0 < p.roughness.0, "rich should be glossier");
        assert!(r.metallic.0 > p.metallic.0, "rich should be more metallic");
        // Rich brightens emissive, poor dims it.
        assert!(r.emission_strength.0 > 1.0, "rich emissive up");
        assert!(p.emission_strength.0 < 1.0, "poor emissive down");
    }

    #[test]
    fn conflict_scorches() {
        let base = cuboid([0.6, 0.5, 0.4], 0.5, 0.0, 0.0);
        let mut peace = base.clone();
        apply_socio_finish(&mut peace, 0.5, 0.0);
        let mut war = base.clone();
        apply_socio_finish(&mut war, 0.5, 1.0);

        let before = luminance(only_material(&peace).base_color.0);
        let after = luminance(only_material(&war).base_color.0);
        assert!(after < before, "conflict should darken the base colour");
    }

    #[test]
    fn deterministic() {
        let base = cuboid([0.3, 0.6, 0.2], 0.5, 0.1, 0.5);
        let mut a = base.clone();
        let mut b = base.clone();
        apply_socio_finish(&mut a, 0.2, 0.8);
        apply_socio_finish(&mut b, 0.2, 0.8);
        assert_eq!(a, b);
    }

    #[test]
    fn reaches_primitive_shape_lsystem_and_children() {
        // A Shape root (materials map) with an LSystem child (materials map)
        // and a primitive grandchild — every material must be touched.
        let mut shape_mats = HashMap::new();
        shape_mats.insert("wall".to_string(), default_with_roughness(0.5));
        let mut lsys_mats = HashMap::new();
        lsys_mats.insert(0u16, default_with_roughness(0.5));

        let mut root = Generator::from_kind(GeneratorKind::Shape {
            grammar_source: "A --> Box".into(),
            root_rule: "A".into(),
            footprint: Fp3([1.0, 0.0, 1.0]),
            seed: 1,
            materials: shape_mats,
        });
        let mut lsys = Generator::from_kind(GeneratorKind::LSystem {
            source_code: String::new(),
            finalization_code: String::new(),
            iterations: 1,
            seed: 1,
            angle: Fp(0.0),
            step: Fp(1.0),
            width: Fp(1.0),
            elasticity: Fp(0.0),
            tropism: None,
            materials: lsys_mats,
            prop_mappings: HashMap::new(),
            prop_scale: Fp(1.0),
            mesh_resolution: 1,
        });
        lsys.children.push(cuboid([0.5, 0.5, 0.5], 0.5, 0.0, 0.0));
        root.children.push(lsys);

        // Rich → every roughness should drop below the 0.5 it started at.
        apply_socio_finish(&mut root, 1.0, 0.0);

        // Shape root material.
        let GeneratorKind::Shape { materials, .. } = &root.kind else {
            panic!("expected shape root");
        };
        assert!(
            materials.values().all(|m| m.roughness.0 < 0.5),
            "shape mat untouched"
        );
        // LSystem child material.
        let GeneratorKind::LSystem { materials, .. } = &root.children[0].kind else {
            panic!("expected lsystem child");
        };
        assert!(
            materials.values().all(|m| m.roughness.0 < 0.5),
            "lsystem mat untouched"
        );
        // Primitive grandchild material.
        assert!(
            only_material(&root.children[0].children[0]).roughness.0 < 0.5,
            "primitive grandchild mat untouched"
        );
    }

    fn default_with_roughness(r: f32) -> SovereignMaterialSettings {
        SovereignMaterialSettings {
            roughness: Fp(r),
            ..SovereignMaterialSettings::default()
        }
    }
}
