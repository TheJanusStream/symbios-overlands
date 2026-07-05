//! Parametric primitive meshing and CPU-side vertex torture.
//!
//! Every parametric primitive `GeneratorKind` variant (Cuboid / Sphere /
//! Cylinder / Capsule / Cone / Torus / Plane / Tetrahedron / Tube / Bevel /
//! Wedge / Helix / Superellipsoid / Spine / Lathe / BlobGroup) routes through
//! [`build_primitive_mesh`] to
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
mod blob;
mod colliders;
mod cuts;
mod prisms;
mod shapes;
mod superellipsoid;
mod sweeps;
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
    fn taper_bottom_narrows_the_base_without_flipping() {
        // The #688 two-ended taper: bottom narrows, top keeps full width —
        // the shape the old top-only taper needed upside-down authoring for.
        let kind = GeneratorKind::Cuboid {
            size: Fp3([1.0, 1.0, 1.0]),
            solid: true,
            material: SovereignMaterialSettings::default(),
            torture: TortureParams {
                taper_bottom: Fp2([0.6, 0.6]),
                ..Default::default()
            },
        };
        let mesh = build_primitive_mesh(&kind);
        let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("no positions");
        };
        let width_at = |sel: fn(&&[f32; 3]) -> bool| {
            pos.iter()
                .filter(sel)
                .map(|p| p[0].abs())
                .fold(0.0_f32, f32::max)
        };
        let bot_x = width_at(|p| p[1] < -0.49);
        let top_x = width_at(|p| p[1] > 0.49);
        assert!(
            bot_x < top_x - 0.1,
            "base not tapered: bot {bot_x} top {top_x}"
        );
        assert!((top_x - 0.5).abs() < 1e-3, "top width changed: {top_x}");
    }

    #[test]
    fn bulge_swells_the_middle_and_pinch_floors_above_zero() {
        let with_bulge = |b: f32| GeneratorKind::Cylinder {
            radius: Fp(0.5),
            height: Fp(2.0),
            resolution: 24,
            solid: true,
            material: SovereignMaterialSettings::default(),
            torture: TortureParams {
                bulge: Fp2([b, b]),
                ..Default::default()
            },
        };
        let mid_radius = |kind: &GeneratorKind| {
            let mesh = build_primitive_mesh(kind);
            let Some(VertexAttributeValues::Float32x3(pos)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("no positions");
            };
            pos.iter()
                .filter(|p| p[1].abs() < 0.2)
                .map(|p| (p[0] * p[0] + p[2] * p[2]).sqrt())
                .fold(0.0_f32, f32::max)
        };
        // Positive bulge swells the waist past the end radius.
        assert!(mid_radius(&with_bulge(0.5)) > 0.6, "no mid swell");
        // A hard pinch collapses toward the axis but never inverts: every
        // mid-height radius stays non-negative (the 1e-3 scale floor).
        let pinched = build_primitive_mesh(&with_bulge(-2.0));
        let Some(VertexAttributeValues::Float32x3(pos)) =
            pinched.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("no positions");
        };
        for p in pos {
            assert!(p.iter().all(|c| c.is_finite()), "non-finite vertex {p:?}");
        }
        assert!(
            mid_radius(&with_bulge(-2.0)) < 0.05,
            "pinch did not collapse the waist"
        );
    }

    #[test]
    fn superellipsoid_stays_inside_extents_and_morphs() {
        let se = |e1: f32, e2: f32| GeneratorKind::Superellipsoid {
            half_extents: Fp3([0.5, 0.5, 0.5]),
            exponent_ns: Fp(e1),
            exponent_ew: Fp(e2),
            latitudes: 16,
            longitudes: 24,
            solid: true,
            material: SovereignMaterialSettings::default(),
            torture: TortureParams::default(),
        };
        let corner_reach = |kind: &GeneratorKind| {
            let mesh = build_primitive_mesh(kind);
            let Some(VertexAttributeValues::Float32x3(pos)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("no positions");
            };
            let mut max_reach = 0.0f32;
            for p in pos {
                for c in p {
                    assert!(c.is_finite(), "non-finite vertex {p:?}");
                    assert!(c.abs() <= 0.5 + 1e-3, "vertex outside extents {p:?}");
                }
                max_reach = max_reach.max((p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt());
            }
            max_reach
        };
        // Low exponents fill toward the box corner (√3·0.5 ≈ 0.866); high
        // exponents pull the corners in toward the octahedral form.
        assert!(corner_reach(&se(0.2, 0.2)) > 0.75, "boxy end not boxy");
        assert!(
            corner_reach(&se(2.0, 2.0)) < 0.55,
            "pinched end not pinched"
        );
        // Normals stay unit across the family, including the sphere middle.
        for kind in [se(0.2, 0.2), se(1.0, 1.0), se(2.5, 2.5), se(0.3, 2.0)] {
            let mesh = build_primitive_mesh(&kind);
            for n in normals(&mesh) {
                assert!(n.iter().all(|c| c.is_finite()), "non-finite normal {n:?}");
                assert!((len(n) - 1.0).abs() < 1e-2, "non-unit normal {n:?}");
            }
        }
    }

    #[test]
    fn spine_passes_through_control_points_with_per_point_radius() {
        use crate::pds::generator::SpinePoint;
        // An L-bend with a fat base and thin tip: the tube must reach both
        // endpoints and the surface radius near each end must match its
        // control radius (Catmull-Rom passes through every point).
        let kind = GeneratorKind::Spine {
            points: vec![
                SpinePoint {
                    position: Fp3([0.0, -0.5, 0.0]),
                    radius: Fp(0.2),
                },
                SpinePoint {
                    position: Fp3([0.0, 0.3, 0.0]),
                    radius: Fp(0.12),
                },
                SpinePoint {
                    position: Fp3([0.5, 0.5, 0.0]),
                    radius: Fp(0.05),
                },
            ],
            resolution: 16,
            samples_per_segment: 8,
            solid: true,
            material: SovereignMaterialSettings::default(),
            torture: TortureParams::default(),
        };
        let mesh = build_primitive_mesh(&kind);
        let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("no positions");
        };
        for p in pos {
            assert!(p.iter().all(|c| c.is_finite()), "non-finite vertex {p:?}");
        }
        // Base ring: vertices near y=-0.5 sit ~0.2 from the start point.
        let base_r = pos
            .iter()
            .filter(|p| p[1] < -0.45)
            .map(|p| (p[0] * p[0] + p[2] * p[2]).sqrt())
            .fold(0.0_f32, f32::max);
        assert!((base_r - 0.2).abs() < 0.05, "base radius {base_r} != 0.2");
        // The tube reaches the bent tip at (0.5, 0.5, 0).
        assert!(
            pos.iter()
                .any(|p| (p[0] - 0.5).abs() < 0.1 && (p[1] - 0.5).abs() < 0.1),
            "spine never reached its final control point"
        );
        for n in normals(&mesh) {
            assert!(n.iter().all(|c| c.is_finite()), "non-finite normal {n:?}");
            assert!((len(n) - 1.0).abs() < 1e-2, "non-unit normal {n:?}");
        }
    }

    #[test]
    fn lathe_profile_hits_its_stations_and_smooth_interpolates() {
        use crate::pds::generator::LathePoint;
        let lathe = |smooth: bool| GeneratorKind::Lathe {
            points: vec![
                LathePoint {
                    radius: Fp(0.1),
                    height: Fp(-0.5),
                },
                LathePoint {
                    radius: Fp(0.4),
                    height: Fp(0.0),
                },
                LathePoint {
                    radius: Fp(0.1),
                    height: Fp(0.5),
                },
            ],
            resolution: 24,
            smooth,
            solid: true,
            material: SovereignMaterialSettings::default(),
            torture: TortureParams::default(),
        };
        for smooth in [false, true] {
            let mesh = build_primitive_mesh(&lathe(smooth));
            let Some(VertexAttributeValues::Float32x3(pos)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("no positions");
            };
            // The belly station (r 0.4 at y 0) is on the surface either way —
            // the spline passes through every control point.
            let belly = pos
                .iter()
                .filter(|p| p[1].abs() < 0.05)
                .map(|p| (p[0] * p[0] + p[2] * p[2]).sqrt())
                .fold(0.0_f32, f32::max);
            assert!(
                (belly - 0.4).abs() < 0.05,
                "belly radius {belly} != 0.4 (smooth={smooth})"
            );
            for n in normals(&mesh) {
                assert!((len(n) - 1.0).abs() < 1e-2, "non-unit normal {n:?}");
            }
        }
    }

    #[test]
    fn blob_group_blends_carves_and_survives_empt_field() {
        use crate::pds::generator::{BlobElement, BlobShape};
        use crate::pds::types::Fp4;
        let blob = |elements: Vec<BlobElement>| GeneratorKind::BlobGroup {
            elements,
            resolution: 32,
            solid: true,
            material: SovereignMaterialSettings::default(),
            torture: TortureParams::default(),
        };
        let positions = |kind: &GeneratorKind| -> Vec<[f32; 3]> {
            let mesh = build_primitive_mesh(kind);
            match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
                Some(VertexAttributeValues::Float32x3(p)) => p.clone(),
                _ => panic!("no positions"),
            }
        };

        // Two spheres with generous blend: one connected peanut — there is
        // material at the midpoint between them (the blend neck), which a
        // hard union of these disjoint spheres would NOT have.
        let a = BlobElement {
            position: Fp3([0.0, -0.25, 0.0]),
            radii: Fp3([0.2, 0.2, 0.2]),
            blend: Fp(0.3),
            ..Default::default()
        };
        let b = BlobElement {
            position: Fp3([0.0, 0.25, 0.0]),
            blend: Fp(0.3),
            radii: Fp3([0.2, 0.2, 0.2]),
            ..Default::default()
        };
        let peanut = positions(&blob(vec![a, b]));
        assert!(peanut.len() > 50, "peanut too sparse: {}", peanut.len());
        let waist = peanut
            .iter()
            .filter(|p| p[1].abs() < 0.05)
            .map(|p| (p[0] * p[0] + p[2] * p[2]).sqrt())
            .fold(0.0_f32, f32::max);
        assert!(waist > 0.05, "blend neck missing at the waist: {waist}");
        for p in &peanut {
            assert!(p.iter().all(|c| c.is_finite()), "non-finite vertex {p:?}");
        }

        // A carve element removes material: subtracting a sphere from the
        // top lobe pulls the mesh's top below the uncarved height.
        let carve = BlobElement {
            position: Fp3([0.0, 0.45, 0.0]),
            radii: Fp3([0.15, 0.15, 0.15]),
            subtract: true,
            blend: Fp(0.05),
            ..Default::default()
        };
        let carved = positions(&blob(vec![a, b, carve]));
        let top = |pts: &[[f32; 3]]| pts.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
        assert!(
            top(&carved) < top(&peanut) - 0.02,
            "carve did not remove the top: {} vs {}",
            top(&carved),
            top(&peanut)
        );

        // All-subtract group: no surface — must fall back to the marker
        // mesh, never panic or emit an empty mesh.
        let empty = positions(&blob(vec![BlobElement {
            subtract: true,
            ..Default::default()
        }]));
        assert!(!empty.is_empty(), "all-subtract blob lost its marker mesh");

        // A rotated capsule element meshes finite with unit normals.
        let tilted = blob(vec![BlobElement {
            shape: BlobShape::Capsule,
            rotation: Fp4(bevy::math::Quat::from_rotation_z(0.7).to_array()),
            radii: Fp3([0.12, 0.3, 0.0]),
            ..Default::default()
        }]);
        let mesh = build_primitive_mesh(&tilted);
        for n in normals(&mesh) {
            assert!(n.iter().all(|c| c.is_finite()), "non-finite normal {n:?}");
            assert!((len(n) - 1.0).abs() < 1e-2, "non-unit normal {n:?}");
        }
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
            GeneratorKind::default_primitive_for_tag("Superellipsoid").unwrap(),
            GeneratorKind::default_primitive_for_tag("Spine").unwrap(),
            GeneratorKind::default_primitive_for_tag("Lathe").unwrap(),
            GeneratorKind::default_primitive_for_tag("BlobGroup").unwrap(),
            with_cut("Lathe", [0.0, 0.5], [0.0, 1.0], 0.0), // half-vase
            with_cut("Lathe", [0.0, 1.0], [0.0, 1.0], 0.5), // hollow vase shell
            with_cut("Lathe", [0.1, 0.9], [0.0, 1.0], 0.6), // cut hollow vase
            with_cut("Cylinder", [0.0, 0.5], [0.0, 1.0], 0.0), // half-cylinder
            with_cut("Cylinder", [0.0, 1.0], [0.0, 1.0], 0.5), // pipe
            with_cut("Cylinder", [0.0, 0.5], [0.0, 1.0], 0.6), // gutter
            with_cut("Sphere", [0.0, 1.0], [0.5, 1.0], 0.0), // dome
            with_cut("Sphere", [0.0, 1.0], [0.0, 0.55], 0.7), // bowl
            with_cut("Sphere", [0.0, 0.5], [0.0, 1.0], 0.0), // half-sphere
            with_cut("Torus", [0.0, 0.5], [0.0, 1.0], 0.0), // arch
            with_cut("Torus", [0.0, 1.0], [0.0, 0.5], 0.0), // C-channel
            with_cut("Cone", [0.0, 0.5], [0.0, 1.0], 0.0),  // half-cone
            with_cut("Cone", [0.0, 1.0], [0.0, 1.0], 0.5),  // funnel shell
            with_cut("Cone", [0.25, 0.75], [0.0, 1.0], 0.4), // cut funnel
            with_cut("Capsule", [0.0, 1.0], [0.5, 1.0], 0.0), // pill top half
            with_cut("Capsule", [0.0, 0.5], [0.0, 1.0], 0.0), // capsule wedge
            with_cut("Capsule", [0.0, 1.0], [0.2, 0.8], 0.5), // hollow sleeve
            with_cut("Capsule", [0.0, 0.75], [0.1, 1.0], 0.3), // everything at once
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
