//! Parametric primitive meshing and CPU-side vertex torture.
//!
//! Every `GeneratorKind::{Cuboid, Sphere, Cylinder, Capsule, Cone, Torus,
//! Plane, Tetrahedron}` variant routes through [`build_primitive_mesh`] to
//! produce a Bevy `Mesh`. When the variant's
//! [`TortureParams`](crate::pds::TortureParams) are non-identity,
//! [`apply_vertex_torture`] mutates the mesh's `ATTRIBUTE_POSITION` buffer
//! (and re-derives normals from the deformation's Jacobian) before it lands
//! in `Assets<Mesh>`, so the visible geometry and the Avian collider stay in
//! lock-step.
//!
//! The tortured-collider path swaps the fast analytical collider (e.g.
//! `Collider::cuboid`) for a convex hull built from the mutated mesh — the
//! analytic shape would cut corners on a twisted/bent primitive, leaving the
//! visual mesh penetrating the world while the collider sits undisturbed.
//! Trimesh colliders are avoided for now because Avian rejects them on
//! dynamic rigid bodies; a child prim that later gains a dynamic body
//! trait would panic the physics step otherwise.

mod base;
mod colliders;
mod cuts;
mod prisms;
mod shapes;
mod torture;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::GeneratorKind;

use colliders::convex_hull_from_mesh;
pub(super) use shapes::prim_parts;
use torture::{apply_vertex_torture, torture_of};

/// Build the parametric mesh for a primitive [`GeneratorKind`] variant and
/// apply vertex torture when non-trivial. Returns the raw `Mesh`; the caller
/// registers it in `Assets<Mesh>` so a single mesh can be reused when a
/// material cache hit bypasses the allocation hot path.
pub fn build_primitive_mesh(kind: &GeneratorKind) -> Mesh {
    let mut mesh = match prim_parts(kind) {
        Some(parts) => parts.shape.base_mesh(),
        // Non-primitive kinds never reach here on the spawn path (the
        // router gates on the variant list); a defensive unit cube keeps a
        // stray caller rendering something visible.
        None => Cuboid::new(1.0, 1.0, 1.0).mesh().build(),
    };
    let torture = torture_of(kind);
    if !torture.is_identity() {
        apply_vertex_torture(&mut mesh, torture);
    } else {
        // Non-tortured path still needs tangents for the PBR shader. The
        // torture branch regenerates them itself after mutating positions.
        let _ = mesh.generate_tangents();
    }
    mesh
}

/// Build the Avian collider that matches a primitive's mesh — analytical
/// when the shape is untortured (cheap, allocates nothing), or a convex
/// hull of the mutated vertex cloud when torture is active (the analytical
/// hull would diverge from the visible geometry). Returns `None` for shapes
/// with no meaningful solid collider.
pub(super) fn collider_for_primitive(kind: &GeneratorKind, mesh: &Mesh) -> Option<Collider> {
    let torture = torture_of(kind);
    // A topology cut (path-cut / profile-cut / hollow) makes the analytical hull
    // diverge from the visible geometry just like a vertex deformation does, so
    // both route to a convex hull of the actual mesh. (A convex hull still fills
    // a hollow bore or an arch's opening — consistent with the Tube's "bore is
    // not a walk-through" standoff; true walk-throughs would want a trimesh.)
    let cuts_active = kind
        .torture()
        .map(|t| !t.cuts_are_identity())
        .unwrap_or(false);
    let analytical = || prim_parts(kind).and_then(|p| p.shape.analytical_collider());
    if torture.is_identity() && !cuts_active {
        analytical()
    } else {
        convex_hull_from_mesh(mesh).or_else(analytical)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::TortureParams;
    use crate::pds::texture::SovereignMaterialSettings;
    use crate::pds::types::{Fp, Fp2, Fp3};
    use bevy::mesh::VertexAttributeValues;

    fn cuboid(size: [f32; 3], twist: f32, taper: f32, bend: [f32; 3]) -> GeneratorKind {
        GeneratorKind::Cuboid {
            size: Fp3(size),
            solid: true,
            material: SovereignMaterialSettings::default(),
            torture: TortureParams {
                twist: Fp(twist),
                taper: Fp2([taper, taper]),
                bend: Fp3(bend),
                ..Default::default()
            },
        }
    }

    fn cylinder(radius: f32, height: f32, twist: f32, taper: f32, bend: [f32; 3]) -> GeneratorKind {
        GeneratorKind::Cylinder {
            radius: Fp(radius),
            height: Fp(height),
            resolution: 24,
            solid: true,
            material: SovereignMaterialSettings::default(),
            torture: TortureParams {
                twist: Fp(twist),
                taper: Fp2([taper, taper]),
                bend: Fp3(bend),
                ..Default::default()
            },
        }
    }

    fn normals(mesh: &Mesh) -> Vec<[f32; 3]> {
        let Some(VertexAttributeValues::Float32x3(n)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("mesh lost its normals");
        };
        n.clone()
    }

    fn len(n: [f32; 3]) -> f32 {
        (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
    }

    #[test]
    fn untortured_keeps_unit_normals() {
        let mesh = build_primitive_mesh(&cuboid([1.0, 1.0, 1.0], 0.0, 0.0, [0.0, 0.0, 0.0]));
        for n in normals(&mesh) {
            assert!((len(n) - 1.0).abs() < 1e-3, "non-unit normal {n:?}");
        }
    }

    #[test]
    fn tortured_normals_stay_unit_and_finite() {
        // A fully tortured frustum-ish cylinder: every normal must remain a
        // finite unit vector after the Jacobian transform.
        let mesh = build_primitive_mesh(&cylinder(0.5, 2.0, 1.2, 0.3, [0.4, 0.0, 0.2]));
        for n in normals(&mesh) {
            assert!(n.iter().all(|c| c.is_finite()), "non-finite normal {n:?}");
            assert!((len(n) - 1.0).abs() < 1e-3, "non-unit normal {n:?}");
        }
    }

    #[test]
    fn taper_preserves_horizontal_cap_and_tilts_sides() {
        // A tapered cylinder is a frustum: the top cap stays horizontal so its
        // normal must remain ~+Y, while the slanted side normals gain a +Y
        // tilt (the bug the flat-normal recompute used to wash out).
        let mesh = build_primitive_mesh(&cylinder(0.6, 2.0, 0.0, 0.4, [0.0, 0.0, 0.0]));
        let ns = normals(&mesh);
        assert!(
            ns.iter().any(|n| n[1] > 0.99),
            "tapered cylinder lost its vertical top-cap normal"
        );
        assert!(
            ns.iter().any(|n| (0.02..0.95).contains(&n[1])),
            "tapered side normals were not tilted by the Jacobian"
        );
    }

    #[test]
    fn build_is_deterministic() {
        let kind = cuboid([1.5, 1.0, 0.8], 0.7, 0.2, [0.3, 0.0, 0.1]);
        let a = build_primitive_mesh(&kind);
        let b = build_primitive_mesh(&kind);
        let pos = |m: &Mesh| match m.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(p)) => p.clone(),
            _ => panic!("no positions"),
        };
        assert_eq!(pos(&a), pos(&b), "positions not deterministic");
        assert_eq!(normals(&a), normals(&b), "normals not deterministic");
    }

    #[test]
    fn per_axis_taper_narrows_one_axis() {
        // Taper X only → a wedge: the top narrows in X but keeps full Z width.
        let kind = GeneratorKind::Cuboid {
            size: Fp3([1.0, 1.0, 1.0]),
            solid: true,
            material: SovereignMaterialSettings::default(),
            torture: TortureParams {
                taper: Fp2([0.6, 0.0]),
                ..Default::default()
            },
        };
        let mesh = build_primitive_mesh(&kind);
        let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("no positions");
        };
        let top_x = pos
            .iter()
            .filter(|p| p[1] > 0.49)
            .map(|p| p[0].abs())
            .fold(0.0_f32, f32::max);
        let bot_x = pos
            .iter()
            .filter(|p| p[1] < -0.49)
            .map(|p| p[0].abs())
            .fold(0.0_f32, f32::max);
        let max_z = pos.iter().map(|p| p[2].abs()).fold(0.0_f32, f32::max);
        assert!(
            top_x < bot_x - 0.1,
            "X not tapered: top {top_x} bot {bot_x}"
        );
        assert!((max_z - 0.5).abs() < 1e-3, "Z width changed: {max_z}");
    }

    #[test]
    fn tube_mesh_is_finite_unit_and_bounded() {
        // Default tube: outer 0.5, height 1.0.
        let kind = GeneratorKind::default_primitive_for_tag("Tube").unwrap();
        let mesh = build_primitive_mesh(&kind);
        let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("no positions");
        };
        assert!(!pos.is_empty(), "tube produced no geometry");
        for n in normals(&mesh) {
            assert!(n.iter().all(|c| c.is_finite()), "non-finite normal {n:?}");
            assert!((len(n) - 1.0).abs() < 1e-3, "non-unit normal {n:?}");
        }
        for p in pos {
            let r = (p[0] * p[0] + p[2] * p[2]).sqrt();
            assert!(r <= 0.5 + 1e-3, "vertex outside outer radius: {r}");
            assert!(p[1].abs() <= 0.5 + 1e-3, "vertex outside height: {}", p[1]);
        }
    }

    #[test]
    fn bevel_mesh_is_bounded_and_chamfered() {
        // Default bevel: 1³ box, 0.15 corner, 3 segments.
        let kind = GeneratorKind::default_primitive_for_tag("Bevel").unwrap();
        let mesh = build_primitive_mesh(&kind);
        let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("no positions");
        };
        for n in normals(&mesh) {
            assert!((len(n) - 1.0).abs() < 1e-3, "non-unit normal {n:?}");
        }
        let mut max_corner = 0.0f32;
        for p in pos {
            assert!(
                p[0].abs() <= 0.5 + 1e-3 && p[1].abs() <= 0.5 + 1e-3 && p[2].abs() <= 0.5 + 1e-3,
                "vertex out of size box: {p:?}"
            );
            max_corner = max_corner.max((p[0] * p[0] + p[2] * p[2]).sqrt());
        }
        // A square corner sits at √(0.5²+0.5²) ≈ 0.707; the bevel pulls it in.
        assert!(max_corner < 0.707, "corner not chamfered: {max_corner}");
    }

    #[test]
    fn new_prims_and_cuts_build_valid_meshes() {
        let with_cut = |tag: &str, pc: [f32; 2], prc: [f32; 2], hollow: f32| {
            let mut k = GeneratorKind::default_primitive_for_tag(tag).unwrap();
            if let Some(t) = k.torture_mut() {
                t.path_cut = Fp2(pc);
                t.profile_cut = Fp2(prc);
                t.hollow = Fp(hollow);
            }
            k
        };
        let kinds = [
            GeneratorKind::default_primitive_for_tag("Wedge").unwrap(),
            GeneratorKind::default_primitive_for_tag("Helix").unwrap(),
            with_cut("Cylinder", [0.0, 0.5], [0.0, 1.0], 0.0), // half-cylinder
            with_cut("Cylinder", [0.0, 1.0], [0.0, 1.0], 0.5), // pipe
            with_cut("Cylinder", [0.0, 0.5], [0.0, 1.0], 0.6), // gutter
            with_cut("Sphere", [0.0, 1.0], [0.5, 1.0], 0.0),   // dome
            with_cut("Sphere", [0.0, 1.0], [0.0, 0.55], 0.7),  // bowl
            with_cut("Sphere", [0.0, 0.5], [0.0, 1.0], 0.0),   // half-sphere
            with_cut("Torus", [0.0, 0.5], [0.0, 1.0], 0.0),    // arch
            with_cut("Torus", [0.0, 1.0], [0.0, 0.5], 0.0),    // C-channel
        ];
        for k in &kinds {
            let tag = k.kind_tag();
            let mesh = build_primitive_mesh(k);
            let Some(VertexAttributeValues::Float32x3(pos)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("no positions for {tag}");
            };
            assert!(pos.len() >= 3, "too few verts for {tag}");
            for p in pos {
                assert!(
                    p[0].is_finite() && p[1].is_finite() && p[2].is_finite(),
                    "non-finite vertex in {tag}: {p:?}"
                );
            }
            assert!(
                mesh.indices().map(|i| i.len() >= 3).unwrap_or(false),
                "no indices for {tag}"
            );
            for n in normals(&mesh) {
                assert!(
                    (len(n) - 1.0).abs() < 1e-2,
                    "non-unit normal {n:?} in {tag}"
                );
            }
        }
    }
}
