//! Parametric primitive meshing and CPU-side vertex torture.
//!
//! Every `GeneratorKind::{Cuboid, Sphere, Cylinder, Capsule, Cone, Torus,
//! Plane, Tetrahedron}` variant routes through [`build_primitive_mesh`] to
//! produce a Bevy `Mesh`. When the variant's `(twist, taper, bend)` triple
//! is non-zero, [`apply_vertex_torture`] mutates the mesh's
//! `ATTRIBUTE_POSITION` buffer before it lands in `Assets<Mesh>`, so the
//! visible geometry and the Avian collider stay in lock-step.
//!
//! The tortured-collider path swaps the fast analytical collider (e.g.
//! `Collider::cuboid`) for a convex hull built from the mutated mesh — the
//! analytic shape would cut corners on a twisted/bent primitive, leaving the
//! visual mesh penetrating the world while the collider sits undisturbed.
//! Trimesh colliders are avoided for now because Avian rejects them on
//! dynamic rigid bodies; a child prim that later gains a dynamic body
//! trait would panic the physics step otherwise.

use avian3d::prelude::*;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;

use crate::pds::GeneratorKind;

/// Torture parameters extracted from a primitive generator. Each parameter
/// is applied to the mesh's vertex positions along the shape's Y extent:
///
/// * `twist` — radians of rotation around Y, linear in normalised Y height.
///   `twist = π` means the top face is rotated 180° relative to the bottom.
/// * `taper` — fraction of `1 - taper * t` scale on X/Z at normalised Y
///   height `t ∈ [0, 1]`. `taper = 0.5` → top is half-width.
/// * `bend` — world-space translation of the top face. At normalised height
///   `t`, vertices are displaced by `bend * t²` on X/Z (quadratic arc). The
///   `bend.y` component is reserved for future rotational bends.
#[derive(Clone, Copy)]
pub(super) struct Torture {
    pub twist: f32,
    pub taper: f32,
    pub bend: Vec3,
}

impl Torture {
    pub fn is_identity(&self) -> bool {
        self.twist.abs() < 1e-6 && self.taper.abs() < 1e-6 && self.bend.length_squared() < 1e-12
    }
}

/// Build the parametric mesh for a primitive [`GeneratorKind`] variant and
/// apply vertex torture when non-trivial. Returns the raw `Mesh`; the caller
/// registers it in `Assets<Mesh>` so a single mesh can be reused when a
/// material cache hit bypasses the allocation hot path.
pub fn build_primitive_mesh(kind: &GeneratorKind) -> Mesh {
    let mut mesh = base_primitive_mesh(kind);
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
    if torture.is_identity() {
        analytical_collider(kind)
    } else {
        convex_hull_from_mesh(mesh).or_else(|| analytical_collider(kind))
    }
}

fn torture_of(kind: &GeneratorKind) -> Torture {
    match kind {
        GeneratorKind::Cuboid {
            twist, taper, bend, ..
        }
        | GeneratorKind::Sphere {
            twist, taper, bend, ..
        }
        | GeneratorKind::Cylinder {
            twist, taper, bend, ..
        }
        | GeneratorKind::Capsule {
            twist, taper, bend, ..
        }
        | GeneratorKind::Cone {
            twist, taper, bend, ..
        }
        | GeneratorKind::Torus {
            twist, taper, bend, ..
        }
        | GeneratorKind::Plane {
            twist, taper, bend, ..
        }
        | GeneratorKind::Tetrahedron {
            twist, taper, bend, ..
        } => Torture {
            twist: twist.0,
            taper: taper.0,
            bend: Vec3::from_array(bend.0),
        },
        _ => Torture {
            twist: 0.0,
            taper: 0.0,
            bend: Vec3::ZERO,
        },
    }
}

fn base_primitive_mesh(kind: &GeneratorKind) -> Mesh {
    match kind {
        GeneratorKind::Cuboid { size, .. } => {
            Cuboid::new(size.0[0], size.0[1], size.0[2]).mesh().build()
        }
        GeneratorKind::Sphere {
            radius, resolution, ..
        } => Sphere::new(radius.0)
            .mesh()
            .ico(*resolution)
            .unwrap_or_else(|_| Sphere::new(radius.0).mesh().build()),
        GeneratorKind::Cylinder {
            radius,
            height,
            resolution,
            ..
        } => Cylinder::new(radius.0, height.0)
            .mesh()
            .resolution(*resolution)
            .build(),
        GeneratorKind::Capsule {
            radius,
            length,
            latitudes,
            longitudes,
            ..
        } => Capsule3d::new(radius.0, length.0)
            .mesh()
            .latitudes(*latitudes)
            .longitudes(*longitudes)
            .build(),
        GeneratorKind::Cone {
            radius,
            height,
            resolution,
            ..
        } => Cone::new(radius.0, height.0)
            .mesh()
            .resolution(*resolution)
            .build(),
        GeneratorKind::Torus {
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
            ..
        } => Torus {
            minor_radius: minor_radius.0,
            major_radius: major_radius.0,
        }
        .mesh()
        .minor_resolution(*minor_resolution as usize)
        .major_resolution(*major_resolution as usize)
        .build(),
        GeneratorKind::Plane {
            size, subdivisions, ..
        } => Plane3d::new(Vec3::Y, Vec2::new(size.0[0] / 2.0, size.0[1] / 2.0))
            .mesh()
            .subdivisions(*subdivisions)
            .build(),
        GeneratorKind::Tetrahedron { size, .. } => {
            let s = size.0;
            let p0 = Vec3::new(0.0, 1.0, 0.0) * s;
            let p1 = Vec3::new(-1.0, -1.0, 1.0).normalize() * s;
            let p2 = Vec3::new(1.0, -1.0, 1.0).normalize() * s;
            let p3 = Vec3::new(0.0, -1.0, -1.0).normalize() * s;
            Tetrahedron::new(p0, p1, p2, p3).mesh().build()
        }
        _ => Cuboid::new(1.0, 1.0, 1.0).mesh().build(),
    }
}

fn analytical_collider(kind: &GeneratorKind) -> Option<Collider> {
    Some(match kind {
        GeneratorKind::Cuboid { size, .. } => Collider::cuboid(size.0[0], size.0[1], size.0[2]),
        GeneratorKind::Sphere { radius, .. } => Collider::sphere(radius.0),
        GeneratorKind::Cylinder { radius, height, .. } => Collider::cylinder(radius.0, height.0),
        GeneratorKind::Capsule { radius, length, .. } => Collider::capsule(radius.0, length.0),
        GeneratorKind::Cone { radius, height, .. } => Collider::cone(radius.0, height.0),
        GeneratorKind::Torus {
            minor_radius,
            major_radius,
            ..
        } => Collider::cuboid(
            major_radius.0 + minor_radius.0,
            minor_radius.0 * 2.0,
            major_radius.0 + minor_radius.0,
        ),
        GeneratorKind::Plane { size, .. } => Collider::cuboid(size.0[0], 0.01, size.0[1]),
        GeneratorKind::Tetrahedron { size, .. } => {
            let s = size.0;
            let p0 = Vec3::new(0.0, 1.0, 0.0) * s;
            let p1 = Vec3::new(-1.0, -1.0, 1.0).normalize() * s;
            let p2 = Vec3::new(1.0, -1.0, 1.0).normalize() * s;
            let p3 = Vec3::new(0.0, -1.0, -1.0).normalize() * s;
            Collider::convex_hull(vec![p0, p1, p2, p3]).unwrap_or_else(|| Collider::sphere(s))
        }
        _ => return None,
    })
}

fn convex_hull_from_mesh(mesh: &Mesh) -> Option<Collider> {
    let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION)?;
    let VertexAttributeValues::Float32x3(verts) = positions else {
        return None;
    };
    if verts.len() < 4 {
        return None;
    }
    let points: Vec<Vec3> = verts.iter().map(|p| Vec3::from_array(*p)).collect();
    Collider::convex_hull(points)
}

/// Mutate `mesh`'s vertex positions in-place, applying twist, taper, and
/// bend proportionally to each vertex's normalised Y height. Regenerates
/// normals + tangents afterwards so the lighting and UV pipeline stay valid.
///
/// The Y normalisation uses the mesh's pre-torture bounding box — we pin
/// `t = 0` at the lowest vertex and `t = 1` at the highest, which keeps the
/// math well-defined for any primitive regardless of whether its origin
/// sits at the base or the centre.
fn apply_vertex_torture(mesh: &mut Mesh, torture: Torture) {
    let Some(positions_attr) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION) else {
        return;
    };
    let VertexAttributeValues::Float32x3(positions) = positions_attr else {
        return;
    };
    if positions.is_empty() {
        return;
    }

    let (mut y_min, mut y_max) = (f32::INFINITY, f32::NEG_INFINITY);
    for p in positions.iter() {
        y_min = y_min.min(p[1]);
        y_max = y_max.max(p[1]);
    }
    let y_range = (y_max - y_min).max(1e-6);

    for p in positions.iter_mut() {
        let t = ((p[1] - y_min) / y_range).clamp(0.0, 1.0);

        // Taper: scale X/Z around the shape's central axis. Bounded away
        // from zero by the sanitizer (|taper| ≤ 0.99) so the taper factor
        // never collapses the face to a point — that would kill convex-hull
        // construction and leave the collider with zero volume.
        let taper_scale = 1.0 - torture.taper * t;
        p[0] *= taper_scale;
        p[2] *= taper_scale;

        // Twist: rotate vertices around the Y axis by an angle linear in
        // normalised height. Vertex at y_max gets the full `twist` angle.
        if torture.twist.abs() > 1e-6 {
            let angle = torture.twist * t;
            let (s, c) = angle.sin_cos();
            let (x, z) = (p[0], p[2]);
            p[0] = c * x - s * z;
            p[2] = s * x + c * z;
        }

        // Bend: quadratic horizontal displacement so the bend curve is
        // tangent to vertical at the base and peaks at the top. Using t²
        // (rather than linear t) keeps the base lined up with the parent
        // transform while the top face arcs away cleanly.
        let t2 = t * t;
        p[0] += torture.bend.x * t2;
        p[2] += torture.bend.z * t2;
    }

    // Flat normals pair well with the tortured geometry — smooth normals
    // baked against the original (un-twisted) surface would point the wrong
    // way once the vertices shift, producing shading seams. `duplicate_vertices`
    // guarantees indexed meshes have one normal per face before the
    // `compute_flat_normals` pass, which would otherwise panic on shared
    // index fan-outs.
    if mesh.indices().is_some() {
        mesh.duplicate_vertices();
    }
    mesh.compute_flat_normals();
    let _ = mesh.generate_tangents();
}
