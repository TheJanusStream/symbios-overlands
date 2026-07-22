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
mod uv;

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
    }
    // Metre-scale UVs for the kinds whose stock parameterisation lays one
    // tile across each face regardless of that face's size (#934). Runs
    // *after* torture so the projection follows the deformed surface, and
    // it re-generates tangents itself because a projection can split
    // vertices — which would leave any earlier tangent buffer the wrong
    // length.
    if let Some(mapping) = uv::metre_projection_for(kind) {
        uv::reproject_mesh(&mut mesh, mapping);
    } else if torture.is_identity() {
        // Non-tortured, non-reprojected path still needs tangents for the
        // PBR shader. The torture branch regenerates them itself after
        // mutating positions.
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
            uv_mapping: crate::pds::generator::UvMapping::default(),
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
    fn lathe_profile_cut_trims_the_silhouette() {
        use crate::pds::generator::LathePoint;
        // Straight-walled drum, radius 0.3, y ∈ [-0.5, 0.5]: the profile's
        // arc length is its height, so the kept band maps linearly to y.
        let mut kind = GeneratorKind::Lathe {
            points: vec![
                LathePoint {
                    radius: Fp(0.3),
                    height: Fp(-0.5),
                },
                LathePoint {
                    radius: Fp(0.3),
                    height: Fp(0.5),
                },
            ],
            resolution: 24,
            smooth: false,
            solid: true,
            material: SovereignMaterialSettings::default(),
            torture: TortureParams::default(),
        };
        if let Some(t) = kind.torture_mut() {
            t.profile_cut = Fp2([0.25, 0.75]);
        }
        let mesh = build_primitive_mesh(&kind);
        let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("no positions");
        };
        let y_min = pos.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
        let y_max = pos.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
        assert!((y_min + 0.25).abs() < 1e-3, "trim bottom at {y_min}");
        assert!((y_max - 0.25).abs() < 1e-3, "trim top at {y_max}");
        // The trimmed ends are open rings of radius 0.3 — both must be
        // capped (fan centre vertices on the axis).
        for y in [-0.25f32, 0.25] {
            assert!(
                pos.iter()
                    .any(|p| (p[1] - y).abs() < 1e-3 && p[0].abs() < 1e-3 && p[2].abs() < 1e-3),
                "no cap centre at y={y}"
            );
        }
    }

    #[test]
    fn blob_cuts_slice_wedge_and_hollow() {
        use crate::pds::generator::{BlobElement, BlobShape};
        let blob = |pc: [f32; 2], prc: [f32; 2], hollow: f32| {
            let mut kind = GeneratorKind::BlobGroup {
                elements: vec![BlobElement {
                    shape: BlobShape::Sphere,
                    radii: Fp3([0.4, 0.4, 0.4]),
                    blend: Fp(0.1),
                    ..Default::default()
                }],
                resolution: 32,
                solid: true,
                uv_mapping: crate::pds::generator::UvMapping::default(),
                material: SovereignMaterialSettings::default(),
                torture: TortureParams::default(),
            };
            if let Some(t) = kind.torture_mut() {
                t.path_cut = Fp2(pc);
                t.profile_cut = Fp2(prc);
                t.hollow = Fp(hollow);
            }
            kind
        };
        let positions = |kind: &GeneratorKind| -> Vec<[f32; 3]> {
            let mesh = build_primitive_mesh(kind);
            match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
                Some(VertexAttributeValues::Float32x3(p)) => p.clone(),
                _ => panic!("no positions"),
            }
        };

        // Slab (profile-cut [0, 0.5]): keeps the lower half of the tight
        // element bounds (y ≤ 0), with grid-cell slop.
        let sliced = positions(&blob([0.0, 1.0], [0.0, 0.5], 0.0));
        let y_max = sliced
            .iter()
            .map(|p| p[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let y_min = sliced.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
        assert!(y_max < 0.06, "slab kept material above the cut: {y_max}");
        assert!(y_min < -0.3, "slab lost the kept lobe: {y_min}");

        // Wedge (path-cut [0, 0.5]): keeps angles 0..π (from +X toward +Z),
        // i.e. the z ≥ 0 half.
        let wedged = positions(&blob([0.0, 0.5], [0.0, 1.0], 0.0));
        assert!(
            !wedged.iter().any(|p| p[2] < -0.06),
            "wedge kept the removed half"
        );
        assert!(
            wedged.iter().any(|p| p[2] > 0.1),
            "wedge lost the kept half"
        );

        // Hollow + slab: the kept hemisphere is a shell. Wall = (1 - 0.8) ×
        // half the tight extent = 0.08, so the inner surface sits at ~0.32 —
        // nothing may survive near the centre, while the solid slab above
        // fills its cut face all the way in.
        let shelled = positions(&blob([0.0, 1.0], [0.0, 0.5], 0.8));
        let r = |p: &[f32; 3]| (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        assert!(
            !shelled.iter().any(|p| r(p) < 0.25),
            "hollow left material in the void"
        );
        assert!(
            shelled.iter().any(|p| r(p) < 0.36),
            "hollow grew no inner wall"
        );
        assert!(
            sliced.iter().any(|p| r(p) < 0.25),
            "solid slab unexpectedly hollow (cut face should span inward)"
        );
    }

    #[test]
    fn blob_new_shapes_mesh_true_to_form() {
        use crate::pds::generator::{BlobElement, BlobShape};
        let one = |shape: BlobShape, radii: [f32; 3]| GeneratorKind::BlobGroup {
            elements: vec![BlobElement {
                shape,
                radii: Fp3(radii),
                blend: Fp(0.0),
                ..Default::default()
            }],
            resolution: 32,
            solid: true,
            uv_mapping: crate::pds::generator::UvMapping::default(),
            material: SovereignMaterialSettings::default(),
            torture: TortureParams::default(),
        };
        let positions = |kind: &GeneratorKind| -> Vec<[f32; 3]> {
            let mesh = build_primitive_mesh(kind);
            for n in normals(&mesh) {
                assert!(n.iter().all(|c| c.is_finite()), "non-finite normal {n:?}");
                assert!((len(n) - 1.0).abs() < 1e-2, "non-unit normal {n:?}");
            }
            match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
                Some(VertexAttributeValues::Float32x3(p)) => p.clone(),
                _ => panic!("no positions"),
            }
        };
        const SLOP: f32 = 0.06;

        // Box: bounded by its half-extents per axis, and actually boxy —
        // the +Y face is flat at full footprint width, which a sphere or
        // ellipsoid of the same bounds could never fill.
        let box_pts = positions(&one(BlobShape::Box, [0.3, 0.2, 0.1]));
        for p in &box_pts {
            assert!(
                p[0].abs() < 0.3 + SLOP && p[1].abs() < 0.2 + SLOP && p[2].abs() < 0.1 + SLOP,
                "box vertex outside extents {p:?}"
            );
        }
        assert!(
            box_pts
                .iter()
                .any(|p| p[1] > 0.2 - SLOP && p[0].abs() > 0.2),
            "box top face is not flat/full-width"
        );

        // Cylinder: radial bound + a top rim at full radius.
        let cyl_pts = positions(&one(BlobShape::Cylinder, [0.25, 0.4, 0.0]));
        for p in &cyl_pts {
            let r = (p[0] * p[0] + p[2] * p[2]).sqrt();
            assert!(r < 0.25 + SLOP, "cylinder vertex outside radius {p:?}");
            assert!(
                p[1].abs() < 0.4 + SLOP,
                "cylinder vertex outside height {p:?}"
            );
        }
        assert!(
            cyl_pts.iter().any(|p| {
                let r = (p[0] * p[0] + p[2] * p[2]).sqrt();
                p[1] > 0.4 - SLOP && r > 0.25 - SLOP
            }),
            "cylinder lost its top rim"
        );

        // Torus: tube stays in the minor-radius band around the ring, and
        // the hole is open.
        let tor_pts = positions(&one(BlobShape::Torus, [0.3, 0.1, 0.0]));
        for p in &tor_pts {
            let ring = ((p[0] * p[0] + p[2] * p[2]).sqrt() - 0.3).abs();
            let tube = (ring * ring + p[1] * p[1]).sqrt();
            assert!(tube < 0.1 + SLOP, "torus vertex off the tube {p:?}");
        }
        assert!(
            !tor_pts
                .iter()
                .any(|p| (p[0] * p[0] + p[2] * p[2]).sqrt() < 0.1),
            "torus hole is filled"
        );

        // Cone: wide base, pinched apex.
        let cone_pts = positions(&one(BlobShape::Cone, [0.3, 0.35, 0.0]));
        let r_at = |band: fn(&&[f32; 3]) -> bool| {
            cone_pts
                .iter()
                .filter(band)
                .map(|p| (p[0] * p[0] + p[2] * p[2]).sqrt())
                .fold(0.0f32, f32::max)
        };
        let base_r = r_at(|p| p[1] < -0.3);
        let tip_r = r_at(|p| p[1] > 0.25);
        assert!(base_r > 0.2, "cone base too narrow: {base_r}");
        assert!(
            tip_r < base_r * 0.5,
            "cone apex not pinched: {tip_r} vs {base_r}"
        );
    }

    /// Every UV mode (#739) meshes the same two-lobe group with agreeing
    /// attribute lengths, valid indices and finite UVs; the bounded-image
    /// modes stay in the unit square, and the discontinuous modes leave no
    /// triangle interpolating across their seam.
    #[test]
    fn blob_uv_modes_produce_sane_uvs() {
        use crate::pds::generator::{BlobElement, UvMapping};
        let group = |uv_mapping: UvMapping| GeneratorKind::BlobGroup {
            elements: vec![
                BlobElement {
                    position: Fp3([0.0, -0.3, 0.0]),
                    radii: Fp3([0.3, 0.3, 0.3]),
                    blend: Fp(0.2),
                    ..Default::default()
                },
                BlobElement {
                    position: Fp3([0.0, 0.35, 0.1]),
                    radii: Fp3([0.22, 0.22, 0.22]),
                    blend: Fp(0.2),
                    ..Default::default()
                },
            ],
            resolution: 32,
            solid: true,
            uv_mapping,
            material: SovereignMaterialSettings::default(),
            torture: TortureParams::default(),
        };
        for mode in [
            UvMapping::Spherical,
            UvMapping::Box,
            UvMapping::Cylindrical,
            UvMapping::PlanarX,
            UvMapping::PlanarY,
            UvMapping::PlanarZ,
            // A future client's mode must still mesh (as the default, Box).
            UvMapping::Unknown,
        ] {
            let mesh = build_primitive_mesh(&group(mode));
            let pos = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
                Some(VertexAttributeValues::Float32x3(p)) => p.clone(),
                _ => panic!("{mode:?}: no positions"),
            };
            let uv = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
                Some(VertexAttributeValues::Float32x2(u)) => u.clone(),
                _ => panic!("{mode:?}: no UVs"),
            };
            assert_eq!(uv.len(), pos.len(), "{mode:?}: uv/pos length mismatch");
            assert!(
                uv.iter().flatten().all(|c| c.is_finite()),
                "{mode:?}: non-finite uv"
            );
            let idx: Vec<u32> = mesh
                .indices()
                .expect("indices")
                .iter()
                .map(|i| i as u32)
                .collect();
            assert_eq!(idx.len() % 3, 0, "{mode:?}: ragged index buffer");
            assert!(
                idx.iter().all(|&i| (i as usize) < pos.len()),
                "{mode:?}: index out of range"
            );
            // Metres about the bounds centre since #933, so the bounded-
            // image invariant is now "within the mass's own half-extent"
            // rather than "within the unit square".
            let (mut blo, mut bhi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
            for p in &pos {
                for a in 0..3 {
                    blo[a] = blo[a].min(p[a]);
                    bhi[a] = bhi[a].max(p[a]);
                }
            }
            let half_span = (0..3)
                .map(|a| (bhi[a] - blo[a]) * 0.5)
                .fold(0.0f32, f32::max);
            match mode {
                // Unknown meshes as the default (Box since #742), so it
                // shares Box's bounded-image invariant.
                UvMapping::Box
                | UvMapping::Unknown
                | UvMapping::PlanarX
                | UvMapping::PlanarY
                | UvMapping::PlanarZ => {
                    assert!(
                        uv.iter().flatten().all(|c| c.abs() <= half_span + 0.01),
                        "{mode:?}: uv ran past the mass's half-extent ({half_span} m)"
                    );
                }
                UvMapping::Cylindrical => {
                    // The wrap seam must be split wherever azimuth is
                    // well-defined. Triangles hugging the axis (the caps)
                    // legitimately swirl — azimuth is degenerate there and
                    // no split can help — so only off-axis triangles are
                    // held to the no-seam invariant.
                    let (mut lo, mut hi) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
                    for p in &pos {
                        for (a, c) in [(0, p[0]), (1, p[2])] {
                            lo[a] = lo[a].min(c);
                            hi[a] = hi[a].max(c);
                        }
                    }
                    let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5];
                    let radial_of = |p: [f32; 3], c: [f32; 2]| {
                        ((p[0] - c[0]).powi(2) + (p[2] - c[1]).powi(2)).sqrt()
                    };
                    let radial = |i: u32| radial_of(pos[i as usize], c);
                    let max_r = idx.iter().map(|&i| radial(i)).fold(0.0f32, f32::max);
                    // U is metres of arc since #933, so the seam period is
                    // the mean circumference the projection measures
                    // against — same definition `uv::cylindrical` uses.
                    let mean_r =
                        pos.iter().map(|&i| radial_of(i, c)).sum::<f32>() / pos.len() as f32;
                    let period = std::f32::consts::TAU * mean_r.max(1e-5);
                    let mut checked = 0;
                    for tri in idx.chunks_exact(3) {
                        if tri.iter().any(|&i| radial(i) < 0.25 * max_r) {
                            continue;
                        }
                        checked += 1;
                        let us = tri.iter().map(|&i| uv[i as usize][0]);
                        let hi = us.clone().fold(f32::NEG_INFINITY, f32::max);
                        let lo = us.fold(f32::INFINITY, f32::min);
                        assert!(
                            hi - lo < period * 0.5,
                            "cylindrical seam not split (ΔU = {}, period {period})",
                            hi - lo
                        );
                    }
                    assert!(checked > 0, "off-axis filter left nothing to check");
                }
                _ => {}
            }
        }
    }

    #[test]
    fn cone_slice_yields_a_frustum_and_cylinder_slice_shortens() {
        // profile_cut on the frustum family is SL's vertical slice: the
        // kept band, with radii interpolated (a sliced cone gains a flat
        // top of the interpolated radius).
        let mut cone = GeneratorKind::default_primitive_for_tag("Cone").unwrap();
        if let Some(t) = cone.torture_mut() {
            t.profile_cut = Fp2([0.0, 0.5]);
        }
        let mesh = build_primitive_mesh(&cone);
        let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("no positions");
        };
        // Default cone: r 0.5, h 1.0 → slice [0, 0.5] keeps y ∈ [-0.5, 0],
        // top radius = lerp(0.5, 0, 0.5) = 0.25.
        let y_max = pos.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
        assert!((y_max - 0.0).abs() < 1e-3, "slice top at {y_max}");
        let top_r = pos
            .iter()
            .filter(|p| p[1] > -0.01)
            .map(|p| (p[0] * p[0] + p[2] * p[2]).sqrt())
            .fold(0.0_f32, f32::max);
        assert!((top_r - 0.25).abs() < 0.02, "frustum top radius {top_r}");
    }

    /// #934: a cuboid's UVs measure metres of face, not "one tile per face".
    ///
    /// The regression this pins is the corner store's brickwork: an 8 × 4 ×
    /// 0.35 slab used to wear one tile on its broad face and one on its
    /// 0.35 m end, so a pier and a lintel sharing a material could not match.
    /// Now every face's UV span equals its own metres, whatever the slab.
    #[test]
    fn cuboid_uv_span_measures_face_metres() {
        let slab = |size: [f32; 3]| {
            let mut k = GeneratorKind::default_primitive_for_tag("Cuboid").unwrap();
            if let GeneratorKind::Cuboid { size: s, .. } = &mut k {
                *s = Fp3(size);
            }
            build_primitive_mesh(&k)
        };

        // Per-face UV extent, keyed by the face's quantised normal, so each
        // of the six faces is measured independently.
        let face_spans = |mesh: &Mesh| -> Vec<(f32, f32)> {
            let Some(VertexAttributeValues::Float32x3(nor)) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!("no normals")
            };
            let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
            else {
                panic!("no uvs")
            };
            let mut by_face: std::collections::HashMap<[i8; 3], (f32, f32, f32, f32)> =
                std::collections::HashMap::new();
            for (n, t) in nor.iter().zip(uv) {
                let key = [n[0].round() as i8, n[1].round() as i8, n[2].round() as i8];
                let e = by_face.entry(key).or_insert((
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                ));
                e.0 = e.0.min(t[0]);
                e.1 = e.1.max(t[0]);
                e.2 = e.2.min(t[1]);
                e.3 = e.3.max(t[1]);
            }
            let mut out: Vec<(f32, f32)> =
                by_face.values().map(|e| (e.1 - e.0, e.3 - e.2)).collect();
            out.sort_by(|a, b| a.partial_cmp(b).unwrap());
            out
        };

        // A 4 × 2 × 0.5 slab: the six faces measure 4×2, 4×0.5 and 2×0.5,
        // and every UV span must equal one of those side lengths.
        let spans = face_spans(&slab([4.0, 2.0, 0.5]));
        assert_eq!(spans.len(), 6, "expected six distinct face normals");
        for (u, v) in &spans {
            for d in [*u, *v] {
                assert!(
                    [4.0_f32, 2.0, 0.5].iter().any(|s| (d - s).abs() < 1e-3),
                    "face UV span {d} is not one of the slab's side lengths"
                );
            }
        }

        // And the property that makes materials portable: double the slab,
        // double every span. Under the old per-face normalisation both were
        // 1.0 and a material could not tell the two apart.
        let small = face_spans(&slab([1.0, 1.0, 1.0]));
        let large = face_spans(&slab([2.0, 2.0, 2.0]));
        for ((su, sv), (lu, lv)) in small.iter().zip(&large) {
            assert!((lu - su * 2.0).abs() < 1e-3, "U span did not track size");
            assert!((lv - sv * 2.0).abs() < 1e-3, "V span did not track size");
        }
    }

    /// #935: the revolved family measures metres too, and — the part that
    /// used to be impossible — its two meshers agree. A cylinder is built by
    /// Bevy while untortured and by the swept-frustum mesher once any cut is
    /// active; before this the two disagreed on UV scale outright, so adding
    /// a hollow bore to a column visibly re-tiled its whole surface.
    #[test]
    fn cylinder_uv_metres_agree_across_both_meshers() {
        let wall_span = |mesh: &Mesh| -> (f32, f32) {
            let Some(VertexAttributeValues::Float32x3(nor)) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!("no normals")
            };
            let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
            else {
                panic!("no uvs")
            };
            // Wall vertices only — caps carry a disc in their own plane.
            let (mut u0, mut u1, mut v0, mut v1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
            for (n, t) in nor.iter().zip(uv) {
                if n[1].abs() > 0.5 {
                    continue;
                }
                u0 = u0.min(t[0]);
                u1 = u1.max(t[0]);
                v0 = v0.min(t[1]);
                v1 = v1.max(t[1]);
            }
            (u1 - u0, v1 - v0)
        };

        let cyl = |hollow: f32| {
            let mut k = GeneratorKind::default_primitive_for_tag("Cylinder").unwrap();
            if let GeneratorKind::Cylinder {
                radius,
                height,
                torture,
                ..
            } = &mut k
            {
                *radius = Fp(0.75);
                *height = Fp(3.0);
                torture.hollow = Fp(hollow);
            }
            build_primitive_mesh(&k)
        };

        let circumference = std::f32::consts::TAU * 0.75;
        // Untortured: Bevy's mesher, rescaled.
        let (u, v) = wall_span(&cyl(0.0));
        assert!(
            (u - circumference).abs() < 1e-2,
            "wall U should span one circumference ({circumference}), got {u}"
        );
        assert!(
            (v - 3.0).abs() < 1e-2,
            "wall V should span the height, got {v}"
        );

        // Hollowed: the swept-frustum mesher. Its outer wall must land on
        // the same metres — the bore adds an inner shell but cannot change
        // what a metre of outer wall is worth.
        let (hu, hv) = wall_span(&cyl(0.4));
        assert!(
            (hu - circumference).abs() < 1e-2,
            "cut mesher disagreed on U ({hu} vs {circumference})"
        );
        assert!((hv - 3.0).abs() < 1e-2, "cut mesher disagreed on V ({hv})");
    }

    /// #938: capsule and sphere agree across their two meshers too.
    ///
    /// The capsule is the one where the meshers genuinely disagreed —
    /// Bevy distributes `V` by height, our swept profile by arc length —
    /// so `rescale_capsule_uvs` re-derives Bevy's from vertex height. On a
    /// stubby capsule the two conventions differ by ~19%, which is exactly
    /// the shift a cut would have caused before.
    #[test]
    fn capsule_and_sphere_uv_metres_agree_across_both_meshers() {
        let v_span = |mesh: &Mesh| -> f32 {
            let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
            else {
                panic!("no uvs")
            };
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for t in uv {
                lo = lo.min(t[1]);
                hi = hi.max(t[1]);
            }
            hi - lo
        };

        // Capsule: r = 0.5, L = 2.0 → profile arc = πr + L ≈ 3.571.
        let capsule = |hollow: f32| {
            let mut k = GeneratorKind::default_primitive_for_tag("Capsule").unwrap();
            if let GeneratorKind::Capsule {
                radius,
                length,
                torture,
                ..
            } = &mut k
            {
                *radius = Fp(0.5);
                *length = Fp(2.0);
                torture.hollow = Fp(hollow);
            }
            build_primitive_mesh(&k)
        };
        let profile_arc = std::f32::consts::PI * 0.5 + 2.0;
        let plain = v_span(&capsule(0.0));
        assert!(
            (plain - profile_arc).abs() < 0.05,
            "Bevy capsule V should span the profile arc ({profile_arc}), got {plain}"
        );
        // Height-proportional V would have spanned L + 2r = 3.0 — the wrong
        // answer this test exists to exclude.
        assert!(
            (plain - 3.0).abs() > 0.3,
            "capsule V still looks height-proportional ({plain})"
        );
        let cut = v_span(&capsule(0.4));
        assert!(
            (cut - plain).abs() < 0.05,
            "capsule meshers disagree on V ({cut} vs {plain})"
        );

        // Sphere: both paths are equirectangular, so V spans πr either way.
        let sphere = |hollow: f32| {
            let mut k = GeneratorKind::default_primitive_for_tag("Sphere").unwrap();
            if let GeneratorKind::Sphere {
                radius, torture, ..
            } = &mut k
            {
                *radius = Fp(1.5);
                torture.hollow = Fp(hollow);
            }
            build_primitive_mesh(&k)
        };
        let half_circ = std::f32::consts::PI * 1.5;
        for (label, h) in [("ico", 0.0), ("lat/lon", 0.4)] {
            let got = v_span(&sphere(h));
            assert!(
                (got - half_circ).abs() < 0.15,
                "{label} sphere V should span πr ({half_circ}), got {got}"
            );
        }
    }

    #[test]
    fn box_pie_cut_and_hollow_carve_the_footprint() {
        // Pie path-cut [0, 0.25]: kept quarter is the +X/+Z quadrant-ish
        // sweep — vertices with strongly negative x AND z must be gone.
        let mut cuboid = GeneratorKind::default_primitive_for_tag("Cuboid").unwrap();
        if let Some(t) = cuboid.torture_mut() {
            t.path_cut = Fp2([0.0, 0.25]);
        }
        let mesh = build_primitive_mesh(&cuboid);
        let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("no positions");
        };
        assert!(
            !pos.iter().any(|p| p[0] < -0.1 && p[2] < -0.1),
            "pie cut kept the opposite quadrant"
        );
        // Hollow box: a matching inner wall must exist (vertices strictly
        // inside the outer footprint).
        let mut hollow = GeneratorKind::default_primitive_for_tag("Cuboid").unwrap();
        if let Some(t) = hollow.torture_mut() {
            t.hollow = Fp(0.5);
        }
        let mesh = build_primitive_mesh(&hollow);
        let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("no positions");
        };
        // A hollow box has no cap-centre vertex (annular caps), so any
        // vertex strictly inside the outer footprint is bore wall.
        assert!(
            pos.iter().any(|p| p[0].abs().max(p[2].abs()) < 0.3),
            "hollow box grew no bore wall"
        );
    }

    #[test]
    fn wedge_and_tetra_gain_deform_vertices() {
        // Nonlinear deforms need interior vertices; the flat subdivision
        // pass provides them only when a deform is active.
        for tag in ["Wedge", "Tetrahedron"] {
            let plain = GeneratorKind::default_primitive_for_tag(tag).unwrap();
            let mut bent = GeneratorKind::default_primitive_for_tag(tag).unwrap();
            if let Some(t) = bent.torture_mut() {
                t.bend = Fp3([0.5, 0.0, 0.0]);
            }
            let count = |k: &GeneratorKind| {
                let mesh = build_primitive_mesh(k);
                match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
                    Some(VertexAttributeValues::Float32x3(p)) => p.len(),
                    _ => 0,
                }
            };
            assert!(
                count(&bent) > count(&plain) * 16,
                "{tag} not subdivided for deforms"
            );
            // And the bend actually curves: some mid-height vertex is
            // displaced off the linear corner interpolation.
            let mesh = build_primitive_mesh(&bent);
            let Some(VertexAttributeValues::Float32x3(pos)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("no positions");
            };
            assert!(
                pos.iter().any(|p| p[1].abs() < 0.2),
                "{tag} has no mid-height vertices"
            );
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
            with_cut("Lathe", [0.0, 1.0], [0.2, 0.8], 0.0), // profile-trimmed band
            with_cut("Lathe", [0.1, 0.9], [0.3, 1.0], 0.5), // everything at once
            with_cut("BlobGroup", [0.0, 0.5], [0.0, 1.0], 0.0), // blob wedge
            with_cut("BlobGroup", [0.0, 1.0], [0.0, 0.5], 0.0), // blob slice
            with_cut("BlobGroup", [0.0, 0.75], [0.1, 0.9], 0.6), // blob everything
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
            with_cut("Cuboid", [0.0, 0.75], [0.0, 1.0], 0.0), // pie-cut box
            with_cut("Cuboid", [0.0, 1.0], [0.0, 1.0], 0.5), // hollow box
            with_cut("Cuboid", [0.1, 0.6], [0.2, 0.9], 0.4), // box everything
            with_cut("Bevel", [0.0, 0.6], [0.0, 1.0], 0.5), // cut rounded box
            with_cut("Superellipsoid", [0.0, 0.5], [0.0, 1.0], 0.0), // pillow half
            with_cut("Superellipsoid", [0.0, 1.0], [0.4, 1.0], 0.6), // hollow dome
            with_cut("Spine", [0.0, 0.5], [0.0, 1.0], 0.0), // curved gutter
            with_cut("Spine", [0.0, 1.0], [0.2, 0.8], 0.5), // trimmed shell
            with_cut("Helix", [0.0, 1.0], [0.0, 0.5], 0.0), // coiled C-channel
            with_cut("Helix", [0.2, 0.8], [0.0, 1.0], 0.6), // hollow part-coil
            with_cut("Cylinder", [0.0, 1.0], [0.25, 0.75], 0.0), // vertical slice
            with_cut("Cone", [0.0, 1.0], [0.0, 0.6], 0.0),  // cone slice = frustum
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
