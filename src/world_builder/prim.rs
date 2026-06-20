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

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::math::Mat3;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;

use crate::pds::GeneratorKind;

/// Torture parameters resolved from a primitive generator's
/// [`TortureParams`](crate::pds::TortureParams) into compute-friendly types.
/// Each is applied to the mesh's vertex positions along the shape's Y extent
/// (`t = normalised height ∈ [0, 1]`):
///
/// * `twist` — radians of rotation around Y, linear in `t`.
/// * `taper` — per-axis scale `1 - taper[axis] * t` (`.x` → X, `.y` → Z).
///   Equal components taper uniformly (cone / frustum); unequal ones give a
///   wedge / fin.
/// * `bend` — quadratic top displacement `bend * t²` on all three axes (the
///   `.y` component now lengthens / shortens the shape's top).
/// * `s_bend` — a `sin(2π t)` lateral wave of amplitude `(x, z)` layered on
///   top of `bend`, so a column can snake into an S rather than only arc.
#[derive(Clone, Copy)]
pub(super) struct Torture {
    pub twist: f32,
    pub taper: Vec2,
    pub bend: Vec3,
    pub s_bend: Vec2,
}

impl Torture {
    pub fn is_identity(&self) -> bool {
        self.twist.abs() < 1e-6
            && self.taper.length_squared() < 1e-12
            && self.bend.length_squared() < 1e-12
            && self.s_bend.length_squared() < 1e-12
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
    match kind.torture() {
        Some(t) => Torture {
            twist: t.twist.0,
            taper: Vec2::from_array(t.taper.0),
            bend: Vec3::from_array(t.bend.0),
            s_bend: Vec2::from_array(t.s_bend.0),
        },
        None => Torture {
            twist: 0.0,
            taper: Vec2::ZERO,
            bend: Vec3::ZERO,
            s_bend: Vec2::ZERO,
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
        GeneratorKind::Tube {
            radius,
            inner_radius,
            height,
            resolution,
            ..
        } => build_tube_mesh(radius.0, inner_radius.0, height.0, *resolution),
        GeneratorKind::Bevel {
            size,
            bevel,
            bevel_segments,
            ..
        } => build_bevel_mesh(size.0, bevel.0, *bevel_segments),
        _ => Cuboid::new(1.0, 1.0, 1.0).mesh().build(),
    }
}

/// Re-wind every triangle so its face winding agrees with the supplied vertex
/// normals (front faces stay visible under back-face culling). Lets the
/// hand-built [`build_tube_mesh`] / [`build_bevel_mesh`] emit correct normals
/// without also hand-proving every triangle's index order.
fn orient_to_normals(pos: &[[f32; 3]], nor: &[[f32; 3]], idx: &mut [u32]) {
    for tri in idx.chunks_exact_mut(3) {
        let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let p0 = Vec3::from_array(pos[a]);
        let face = (Vec3::from_array(pos[b]) - p0).cross(Vec3::from_array(pos[c]) - p0);
        let vn = Vec3::from_array(nor[a]) + Vec3::from_array(nor[b]) + Vec3::from_array(nor[c]);
        if face.dot(vn) < 0.0 {
            tri.swap(1, 2);
        }
    }
}

/// Assemble a CPU mesh from raw attribute buffers, fixing winding against the
/// normals and generating tangents — the shared tail of the hand-built tube /
/// bevel builders.
fn mesh_from_parts(
    pos: Vec<[f32; 3]>,
    nor: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    mut idx: Vec<u32>,
) -> Mesh {
    orient_to_normals(&pos, &nor, &mut idx);
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_indices(Indices::U32(idx));
    let _ = mesh.generate_tangents();
    mesh
}

/// Hollow cylinder: outer + inner walls (smooth radial normals) closed by two
/// annular caps. `inner` is clamped just inside `outer` so the bore never
/// inverts.
fn build_tube_mesh(outer: f32, inner: f32, height: f32, resolution: u32) -> Mesh {
    use std::f32::consts::TAU;
    let res = resolution.max(3) as usize;
    let inner = inner.clamp(0.0, outer * 0.999);
    let h2 = height * 0.5;

    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nor: Vec<[f32; 3]> = Vec::new();
    let mut uv: Vec<[f32; 2]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();

    // Walls: outer normal points out (+1), inner points in (-1).
    for &(radius, sign) in &[(outer, 1.0f32), (inner, -1.0f32)] {
        let base = pos.len() as u32;
        for i in 0..=res {
            let a = i as f32 / res as f32 * TAU;
            let (s, c) = a.sin_cos();
            let n = [sign * c, 0.0, sign * s];
            let u = i as f32 / res as f32;
            pos.push([radius * c, h2, radius * s]);
            nor.push(n);
            uv.push([u, 1.0]);
            pos.push([radius * c, -h2, radius * s]);
            nor.push(n);
            uv.push([u, 0.0]);
        }
        for i in 0..res as u32 {
            let t0 = base + i * 2;
            let (b0, t1, b1) = (t0 + 1, t0 + 2, t0 + 3);
            idx.extend_from_slice(&[b0, b1, t1, b0, t1, t0]);
        }
    }

    // Annular caps: top (+Y) then bottom (-Y).
    for &(y, ny) in &[(h2, 1.0f32), (-h2, -1.0f32)] {
        let base = pos.len() as u32;
        for i in 0..=res {
            let a = i as f32 / res as f32 * TAU;
            let (s, c) = a.sin_cos();
            pos.push([outer * c, y, outer * s]);
            nor.push([0.0, ny, 0.0]);
            uv.push([0.5 + 0.5 * c, 0.5 + 0.5 * s]);
            pos.push([inner * c, y, inner * s]);
            nor.push([0.0, ny, 0.0]);
            uv.push([0.5 + 0.25 * c, 0.5 + 0.25 * s]);
        }
        for i in 0..res as u32 {
            let o0 = base + i * 2;
            let (in0, o1, in1) = (o0 + 1, o0 + 2, o0 + 3);
            idx.extend_from_slice(&[o0, o1, in1, o0, in1, in0]);
        }
    }

    mesh_from_parts(pos, nor, uv, idx)
}

/// Box with chamfered / rounded vertical edges — an extruded rounded-rectangle
/// prism. `bevel` is the corner radius (clamped inside the footprint);
/// `segments` is `1` for a flat chamfer (octagonal prism), higher for a
/// rounded corner. Side normals follow the profile (smooth on arcs, flat on
/// the straight runs); caps are flat fans.
fn build_bevel_mesh(size: [f32; 3], bevel: f32, segments: u32) -> Mesh {
    use std::f32::consts::{FRAC_PI_2, PI};
    let [sx, sy, sz] = size;
    let (hx, hy, hz) = (sx * 0.5, sy * 0.5, sz * 0.5);
    let b = bevel.clamp(0.0, (hx.min(hz) - 1e-3).max(0.0));
    let seg = segments.max(1) as usize;

    // Rounded-rectangle profile (XZ), CCW: four corner arcs whose endpoints
    // meet the straight edges. Each entry is (x, z, outward nx, outward nz).
    let centers = [
        (hx - b, hz - b, 0.0f32),
        (-(hx - b), hz - b, FRAC_PI_2),
        (-(hx - b), -(hz - b), PI),
        (hx - b, -(hz - b), 3.0 * FRAC_PI_2),
    ];
    let mut profile: Vec<(f32, f32, f32, f32)> = Vec::new();
    for (cx, cz, a0) in centers {
        for k in 0..=seg {
            let a = a0 + (k as f32 / seg as f32) * FRAC_PI_2;
            let (s, c) = a.sin_cos();
            profile.push((cx + b * c, cz + b * s, c, s));
        }
    }
    let n = profile.len();

    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nor: Vec<[f32; 3]> = Vec::new();
    let mut uv: Vec<[f32; 2]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();

    // Side wall.
    for (i, &(x, z, nx, nz)) in profile.iter().enumerate() {
        let u = i as f32 / n as f32;
        pos.push([x, hy, z]);
        nor.push([nx, 0.0, nz]);
        uv.push([u, 1.0]);
        pos.push([x, -hy, z]);
        nor.push([nx, 0.0, nz]);
        uv.push([u, 0.0]);
    }
    for i in 0..n {
        let i1 = (i + 1) % n;
        let t0 = (i as u32) * 2;
        let b0 = t0 + 1;
        let t1 = (i1 as u32) * 2;
        let b1 = t1 + 1;
        idx.extend_from_slice(&[b0, b1, t1, b0, t1, t0]);
    }

    // Caps: a centre-fan over the profile, top (+Y) then bottom (-Y).
    let inv_hx = 1.0 / hx.max(1e-3);
    let inv_hz = 1.0 / hz.max(1e-3);
    for &(y, ny) in &[(hy, 1.0f32), (-hy, -1.0f32)] {
        let center = pos.len() as u32;
        pos.push([0.0, y, 0.0]);
        nor.push([0.0, ny, 0.0]);
        uv.push([0.5, 0.5]);
        let rim = pos.len() as u32;
        for &(x, z, _, _) in &profile {
            pos.push([x, y, z]);
            nor.push([0.0, ny, 0.0]);
            uv.push([0.5 + 0.5 * x * inv_hx, 0.5 + 0.5 * z * inv_hz]);
        }
        for i in 0..n as u32 {
            let i1 = (i + 1) % n as u32;
            idx.extend_from_slice(&[center, rim + i, rim + i1]);
        }
    }

    mesh_from_parts(pos, nor, uv, idx)
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
        // The bore is not a walk-through volume — a solid outer cylinder is the
        // right standoff for a pipe / curb prop.
        GeneratorKind::Tube { radius, height, .. } => Collider::cylinder(radius.0, height.0),
        // The bevel's footprint is within its size box; the box is a tight
        // enough standoff (the chamfer only shaves the corners).
        GeneratorKind::Bevel { size, .. } => Collider::cuboid(size.0[0], size.0[1], size.0[2]),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::TortureParams;
    use crate::pds::texture::SovereignMaterialSettings;
    use crate::pds::types::{Fp, Fp2, Fp3};

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

/// The pure torture deformation `D(p)`: twist + taper + bend applied to a
/// vertex, parametrised by the vertex's normalised Y height `t = (y - y_min) /
/// y_range`. Factored out of [`apply_vertex_torture`] so the same map drives
/// both the position warp and the normal transform (via its Jacobian), and so
/// future multi-axis torture only has to extend one function.
///
/// `t` is clamped to `[0, 1]`, pinning `t = 0` at the lowest vertex and
/// `t = 1` at the highest — well-defined for any primitive whether its origin
/// sits at the base or the centre.
fn deform_vertex(p: Vec3, y_min: f32, y_range: f32, torture: Torture) -> Vec3 {
    let t = ((p.y - y_min) / y_range).clamp(0.0, 1.0);

    // Per-axis taper: scale X/Z independently around the central axis.
    // Bounded away from zero by the sanitizer (|taper| ≤ 0.99) so a face never
    // collapses to a point. Equal components = uniform taper (cone/frustum);
    // unequal = wedge / fin.
    let mut x = p.x * (1.0 - torture.taper.x * t);
    let mut z = p.z * (1.0 - torture.taper.y * t);

    // Twist: rotate around Y by an angle linear in normalised height.
    if torture.twist.abs() > 1e-6 {
        let (s, c) = (torture.twist * t).sin_cos();
        let (ox, oz) = (x, z);
        x = c * ox - s * oz;
        z = s * ox + c * oz;
    }

    // Bend: quadratic displacement, tangent to vertical at the base and
    // peaking at the top — now on all three axes (`.y` lengthens the top).
    // S-bend: a sin(2π t) lateral wave layered on top for a serpentine column.
    let t2 = t * t;
    let wave = (std::f32::consts::TAU * t).sin();
    Vec3::new(
        x + torture.bend.x * t2 + torture.s_bend.x * wave,
        p.y + torture.bend.y * t2,
        z + torture.bend.z * t2 + torture.s_bend.y * wave,
    )
}

/// Mutate `mesh`'s vertex positions in-place through [`deform_vertex`], then
/// transform each vertex normal by the **inverse-transpose Jacobian** of that
/// deformation so the original shading character survives the warp.
///
/// This replaces the old flat-normal recompute, which faceted every tortured
/// shape (a twisted sphere went low-poly) — and a naive smooth recompute would
/// instead round a cuboid's hard edges. Transforming the *existing* per-vertex
/// normals by the local Jacobian preserves both: a cuboid's per-face normals
/// stay per-face (sharp), a sphere's stay smooth, and both tilt correctly with
/// the taper/twist/bend. The Jacobian is taken numerically (the map is smooth
/// and cheap to sample), with a fallback to the untransformed normal where the
/// local Jacobian is near-singular. Tangents are regenerated afterwards.
fn apply_vertex_torture(mesh: &mut Mesh, torture: Torture) {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return;
    };
    if positions.is_empty() {
        return;
    }
    let orig: Vec<Vec3> = positions.iter().map(|p| Vec3::from_array(*p)).collect();

    let (mut y_min, mut y_max) = (f32::INFINITY, f32::NEG_INFINITY);
    for p in &orig {
        y_min = y_min.min(p.y);
        y_max = y_max.max(p.y);
    }
    let y_range = (y_max - y_min).max(1e-6);
    let deform = |p: Vec3| deform_vertex(p, y_min, y_range, torture);

    let new_pos: Vec<[f32; 3]> = orig.iter().map(|p| deform(*p).to_array()).collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, new_pos);

    // Normals follow the deformation's inverse-transpose Jacobian, sampled
    // numerically per vertex. `eps` is small relative to any primitive extent
    // (sanitiser floors dimensions at 0.01), so the central map stays linear
    // across the stencil.
    if let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
    {
        const EPS: f32 = 1e-3;
        let new_norms: Vec<[f32; 3]> = orig
            .iter()
            .zip(normals.iter())
            .map(|(p, n)| {
                let n0 = Vec3::from_array(*n);
                let d = deform(*p);
                let jacobian = Mat3::from_cols(
                    (deform(*p + Vec3::X * EPS) - d) / EPS,
                    (deform(*p + Vec3::Y * EPS) - d) / EPS,
                    (deform(*p + Vec3::Z * EPS) - d) / EPS,
                );
                if jacobian.determinant().abs() < 1e-9 {
                    return n0.normalize_or_zero().to_array();
                }
                jacobian
                    .inverse()
                    .transpose()
                    .mul_vec3(n0)
                    .normalize_or_zero()
                    .to_array()
            })
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, new_norms);
    }

    let _ = mesh.generate_tangents();
}
