//! Per-variant primitive dispatch (#644): one [`PrimitiveShape`] impl per
//! parametric [`GeneratorKind`] variant, produced by the single
//! [`prim_parts`] constructor match. Everything downstream — mesh build,
//! analytical collider, the spawner's `(solid, material)` split — reads the
//! trait object, so adding a primitive means one impl + one constructor arm
//! here (plus its line in `spawn_generator`'s exhaustive router list),
//! instead of four hand-synced `match GeneratorKind` sites.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::{GeneratorKind, TortureParams};

use super::base::subdivide_flat;
use super::blob::{blob_hull_points, build_blob_mesh};
use super::cuts::{
    build_profile_sweep, build_swept_capsule, build_swept_frustum, build_torus, build_uv_sphere,
    path_cut_angles, rect_profile, rounded_rect_profile,
};
use super::prisms::{build_bevel_mesh, build_helix_mesh, build_tube_mesh, build_wedge_mesh};
use super::superellipsoid::{build_superellipsoid, superellipsoid_hull_points};
use super::sweeps::{build_lathe_mesh, build_spine_mesh, lathe_hull_points, spine_hull_points};

/// Vertical wall subdivisions used when a vertex deform is active: the
/// nonlinear deforms (bulge / bend / S-bend / twist) need mid-height
/// vertices to move — a 2-ring wall renders a `sin(π t)` bulge as nothing.
/// Deform-free prims keep their old minimal layouts.
const DEFORM_ROWS: u32 = 16;

/// Flat-subdivision levels applied to the faceted low-poly prims (Wedge /
/// Tetrahedron) when a deform is active: 4 halvings ≈ the same edge density
/// as [`DEFORM_ROWS`] gives the swept walls.
const DEFORM_SUBDIV_LEVELS: u32 = 4;

/// One parametric primitive's shape behavior: its base mesh (pre-torture)
/// and the cheap analytical collider matching the *untortured* shape
/// (`None` when no meaningful solid exists).
pub(in crate::world_builder) trait PrimitiveShape {
    fn base_mesh(&self) -> Mesh;
    fn analytical_collider(&self) -> Option<Collider>;
}

/// A primitive variant split into the pieces every consumer needs: the
/// shape behavior plus the `solid` / `material` fields shared by all
/// sixteen variants. `None` for non-primitive kinds — the router's
/// primitive test.
pub(in crate::world_builder) struct PrimParts<'a> {
    pub shape: Box<dyn PrimitiveShape + 'a>,
    pub solid: bool,
    pub material: &'a SovereignMaterialSettings,
}

/// The one place a primitive variant is destructured. Every arm borrows the
/// variant's fields into its shape impl; `solid` / `material` ride alongside
/// so the spawner needs no second extraction match.
pub(in crate::world_builder) fn prim_parts(kind: &GeneratorKind) -> Option<PrimParts<'_>> {
    fn parts<'a>(
        shape: Box<dyn PrimitiveShape + 'a>,
        solid: &bool,
        material: &'a SovereignMaterialSettings,
    ) -> Option<PrimParts<'a>> {
        Some(PrimParts {
            shape,
            solid: *solid,
            material,
        })
    }
    match kind {
        GeneratorKind::Cuboid {
            size,
            torture,
            solid,
            material,
        } => parts(
            Box::new(CuboidShape {
                size: size.0,
                torture,
            }),
            solid,
            material,
        ),
        GeneratorKind::Sphere {
            radius,
            resolution,
            torture,
            solid,
            material,
        } => parts(
            Box::new(SphereShape {
                radius: radius.0,
                resolution: *resolution,
                torture,
            }),
            solid,
            material,
        ),
        GeneratorKind::Cylinder {
            radius,
            height,
            resolution,
            torture,
            solid,
            material,
        } => parts(
            Box::new(CylinderShape {
                radius: radius.0,
                height: height.0,
                resolution: *resolution,
                torture,
            }),
            solid,
            material,
        ),
        GeneratorKind::Capsule {
            radius,
            length,
            latitudes,
            longitudes,
            torture,
            solid,
            material,
        } => parts(
            Box::new(CapsuleShape {
                radius: radius.0,
                length: length.0,
                latitudes: *latitudes,
                longitudes: *longitudes,
                torture,
            }),
            solid,
            material,
        ),
        GeneratorKind::Cone {
            radius,
            height,
            resolution,
            torture,
            solid,
            material,
        } => parts(
            Box::new(ConeShape {
                radius: radius.0,
                height: height.0,
                resolution: *resolution,
                torture,
            }),
            solid,
            material,
        ),
        GeneratorKind::Torus {
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
            torture,
            solid,
            material,
        } => parts(
            Box::new(TorusShape {
                minor_radius: minor_radius.0,
                major_radius: major_radius.0,
                minor_resolution: *minor_resolution,
                major_resolution: *major_resolution,
                torture,
            }),
            solid,
            material,
        ),
        GeneratorKind::Plane {
            size,
            subdivisions,
            solid,
            material,
            ..
        } => parts(
            Box::new(PlaneShape {
                size: size.0,
                subdivisions: *subdivisions,
            }),
            solid,
            material,
        ),
        GeneratorKind::Tetrahedron {
            size,
            torture,
            solid,
            material,
        } => parts(
            Box::new(TetrahedronShape {
                size: size.0,
                torture,
            }),
            solid,
            material,
        ),
        GeneratorKind::Tube {
            radius,
            inner_radius,
            height,
            resolution,
            torture,
            solid,
            material,
        } => parts(
            Box::new(TubeShape {
                radius: radius.0,
                inner_radius: inner_radius.0,
                height: height.0,
                resolution: *resolution,
                torture,
            }),
            solid,
            material,
        ),
        GeneratorKind::Bevel {
            size,
            bevel,
            bevel_segments,
            torture,
            solid,
            material,
        } => parts(
            Box::new(BevelShape {
                size: size.0,
                bevel: bevel.0,
                bevel_segments: *bevel_segments,
                torture,
            }),
            solid,
            material,
        ),
        GeneratorKind::Wedge {
            size,
            torture,
            solid,
            material,
        } => parts(
            Box::new(WedgeShape {
                size: size.0,
                torture,
            }),
            solid,
            material,
        ),
        GeneratorKind::Helix {
            radius,
            tube_radius,
            pitch,
            turns,
            resolution,
            torture,
            solid,
            material,
        } => parts(
            Box::new(HelixShape {
                radius: radius.0,
                tube_radius: tube_radius.0,
                pitch: pitch.0,
                turns: turns.0,
                resolution: *resolution,
                torture,
            }),
            solid,
            material,
        ),
        GeneratorKind::Superellipsoid {
            half_extents,
            exponent_ns,
            exponent_ew,
            latitudes,
            longitudes,
            torture,
            solid,
            material,
        } => parts(
            Box::new(SuperellipsoidShape {
                half_extents: half_extents.0,
                exponent_ns: exponent_ns.0,
                exponent_ew: exponent_ew.0,
                latitudes: *latitudes,
                longitudes: *longitudes,
                torture,
            }),
            solid,
            material,
        ),
        GeneratorKind::Spine {
            points,
            resolution,
            samples_per_segment,
            torture,
            solid,
            material,
        } => parts(
            Box::new(SpineShape {
                points: points
                    .iter()
                    .map(|p| (Vec3::from_array(p.position.0), p.radius.0))
                    .collect(),
                resolution: *resolution,
                samples_per_segment: *samples_per_segment,
                torture,
            }),
            solid,
            material,
        ),
        GeneratorKind::Lathe {
            points,
            resolution,
            smooth,
            torture,
            solid,
            material,
        } => parts(
            Box::new(LatheShape {
                points: points.iter().map(|p| (p.radius.0, p.height.0)).collect(),
                resolution: *resolution,
                smooth: *smooth,
                torture,
            }),
            solid,
            material,
        ),
        GeneratorKind::BlobGroup {
            elements,
            resolution,
            uv_mapping,
            torture,
            solid,
            material,
        } => parts(
            Box::new(BlobGroupShape {
                elements,
                resolution: *resolution,
                uv_mapping: *uv_mapping,
                torture,
            }),
            solid,
            material,
        ),
        _ => None,
    }
}

struct CuboidShape<'a> {
    size: [f32; 3],
    torture: &'a TortureParams,
}

impl PrimitiveShape for CuboidShape<'_> {
    fn base_mesh(&self) -> Mesh {
        use std::f32::consts::TAU;
        if self.torture.cuts_are_identity() && self.torture.deforms_are_identity() {
            return Cuboid::new(self.size[0], self.size[1], self.size[2])
                .mesh()
                .build();
        }
        // SL box cuts (#691): the cuboid becomes a swept rectangular
        // profile — pie path-cut, matching rectangular bore, vertical
        // slice — with wall rows for the nonlinear deforms.
        let (a0, a1) = if self.torture.cuts_are_identity() {
            (0.0, TAU)
        } else {
            path_cut_angles(self.torture)
        };
        let rows = if self.torture.deforms_are_identity() {
            1
        } else {
            DEFORM_ROWS
        };
        build_profile_sweep(
            &rect_profile(self.size[0] * 0.5, self.size[2] * 0.5),
            self.size[1],
            rows,
            self.torture.hollow.0,
            a0,
            a1,
            self.torture.profile_cut.0[0],
            self.torture.profile_cut.0[1],
        )
    }
    fn analytical_collider(&self) -> Option<Collider> {
        Some(Collider::cuboid(self.size[0], self.size[1], self.size[2]))
    }
}

struct SphereShape<'a> {
    radius: f32,
    resolution: u32,
    torture: &'a TortureParams,
}

impl PrimitiveShape for SphereShape<'_> {
    fn base_mesh(&self) -> Mesh {
        if self.torture.cuts_are_identity() {
            // Bevy's icosphere UVs are equirectangular (azimuth, inclination
            // normalised to 0..1), so metres are one uniform scale: a full
            // equatorial circumference across U, a pole-to-pole
            // half-circumference down V — the same figures the cut lat/lon
            // path uses, so the two agree without sharing a mesher (#938).
            let mut mesh = Sphere::new(self.radius)
                .mesh()
                .ico(self.resolution)
                .unwrap_or_else(|_| Sphere::new(self.radius).mesh().build());
            super::uv::scale_uvs(
                &mut mesh,
                std::f32::consts::TAU * self.radius,
                std::f32::consts::PI * self.radius,
            );
            mesh
        } else {
            let (lon0, lon1) = path_cut_angles(self.torture);
            build_uv_sphere(
                self.radius,
                self.resolution,
                lon0,
                lon1,
                self.torture.profile_cut.0[0],
                self.torture.profile_cut.0[1],
                self.torture.hollow.0,
            )
        }
    }
    fn analytical_collider(&self) -> Option<Collider> {
        Some(Collider::sphere(self.radius))
    }
}

struct CylinderShape<'a> {
    radius: f32,
    height: f32,
    resolution: u32,
    torture: &'a TortureParams,
}

impl PrimitiveShape for CylinderShape<'_> {
    fn base_mesh(&self) -> Mesh {
        let rows = if self.torture.deforms_are_identity() {
            1
        } else {
            DEFORM_ROWS
        };
        if self.torture.cuts_are_identity() {
            // Bevy's builder lays one tile across the wall and one across
            // each cap whatever the cylinder measures; rescale both into
            // the metre convention (#935). The uncut branch is the only one
            // that runs here, so the kind's own radius/height *are* the
            // geometry — no post-cut adjustment to account for.
            let mut mesh = Cylinder::new(self.radius, self.height)
                .mesh()
                .resolution(self.resolution)
                .segments(rows)
                .build();
            super::uv::rescale_revolved_uvs(
                &mut mesh,
                std::f32::consts::TAU * self.radius,
                self.height,
                self.radius,
            );
            mesh
        } else {
            let (a0, a1) = path_cut_angles(self.torture);
            build_swept_frustum(
                self.radius,
                self.radius,
                self.height,
                self.resolution,
                rows,
                self.torture.hollow.0,
                a0,
                a1,
                self.torture.profile_cut.0[0],
                self.torture.profile_cut.0[1],
            )
        }
    }
    fn analytical_collider(&self) -> Option<Collider> {
        Some(Collider::cylinder(self.radius, self.height))
    }
}

struct CapsuleShape<'a> {
    radius: f32,
    length: f32,
    latitudes: u32,
    longitudes: u32,
    torture: &'a TortureParams,
}

impl PrimitiveShape for CapsuleShape<'_> {
    fn base_mesh(&self) -> Mesh {
        if self.torture.cuts_are_identity() {
            let rings = if self.torture.deforms_are_identity() {
                0
            } else {
                DEFORM_ROWS
            };
            let mut mesh = Capsule3d::new(self.radius, self.length)
                .mesh()
                .latitudes(self.latitudes)
                .longitudes(self.longitudes)
                .rings(rings)
                .build();
            super::uv::rescale_capsule_uvs(&mut mesh, self.radius, self.length);
            mesh
        } else {
            let (lon0, lon1) = path_cut_angles(self.torture);
            build_swept_capsule(
                self.radius,
                self.length,
                self.latitudes,
                self.longitudes,
                lon0,
                lon1,
                self.torture.profile_cut.0[0],
                self.torture.profile_cut.0[1],
                self.torture.hollow.0,
            )
        }
    }
    fn analytical_collider(&self) -> Option<Collider> {
        Some(Collider::capsule(self.radius, self.length))
    }
}

struct ConeShape<'a> {
    radius: f32,
    height: f32,
    resolution: u32,
    torture: &'a TortureParams,
}

impl PrimitiveShape for ConeShape<'_> {
    fn base_mesh(&self) -> Mesh {
        if self.torture.cuts_are_identity() && self.torture.deforms_are_identity() {
            let mut mesh = Cone::new(self.radius, self.height)
                .mesh()
                .resolution(self.resolution)
                .build();
            // A cone's wall tapers to a point, so its mean circumference is
            // that of the half-radius — the same figure the swept mesher
            // uses for a frustum whose top radius is zero.
            super::uv::rescale_revolved_uvs(
                &mut mesh,
                std::f32::consts::PI * self.radius,
                self.height,
                self.radius,
            );
            mesh
        } else {
            let (a0, a1) = path_cut_angles(self.torture);
            build_swept_frustum(
                self.radius,
                0.0,
                self.height,
                self.resolution,
                DEFORM_ROWS,
                self.torture.hollow.0,
                a0,
                a1,
                self.torture.profile_cut.0[0],
                self.torture.profile_cut.0[1],
            )
        }
    }
    fn analytical_collider(&self) -> Option<Collider> {
        Some(Collider::cone(self.radius, self.height))
    }
}

struct TorusShape<'a> {
    minor_radius: f32,
    major_radius: f32,
    minor_resolution: u32,
    major_resolution: u32,
    torture: &'a TortureParams,
}

impl PrimitiveShape for TorusShape<'_> {
    fn base_mesh(&self) -> Mesh {
        if self.torture.cuts_are_identity() {
            // Bevy lays U along the major circle and V around the minor
            // one; both become arc metres (#938) so the ring agrees with
            // the cut path in `build_torus`.
            let mut mesh = Torus {
                minor_radius: self.minor_radius,
                major_radius: self.major_radius,
            }
            .mesh()
            .minor_resolution(self.minor_resolution as usize)
            .major_resolution(self.major_resolution as usize)
            .build();
            super::uv::scale_uvs(
                &mut mesh,
                std::f32::consts::TAU * self.major_radius,
                std::f32::consts::TAU * self.minor_radius,
            );
            mesh
        } else {
            use std::f32::consts::TAU;
            let (maj0, maj1) = path_cut_angles(self.torture);
            // Profile-cut convention for a torus: the band endpoints `0.0`
            // and `1.0` sit on the **top flat pole** of the tube cross-section
            // (`+Y`, the donut's broad face), not on the outer perimeter. So a
            // `[0.0, 0.5]` band keeps the inner-radius half (toward the major
            // axis) and `[0.5, 1.0]` keeps the outer-radius half — letting a
            // single profile-cut remove the inner half of a ring (e.g. a
            // wheel-fender hugging only the outer tread). The `+TAU/4` phase
            // rotates the band start from the outer equator (the bare-sweep
            // zero) to that top pole.
            let phase = TAU / 4.0;
            let (min0, min1) = (
                self.torture.profile_cut.0[0] * TAU + phase,
                self.torture.profile_cut.0[1] * TAU + phase,
            );
            build_torus(
                self.major_radius,
                self.minor_radius,
                self.major_resolution,
                self.minor_resolution,
                maj0,
                maj1,
                min0,
                min1,
                self.torture.hollow.0,
            )
        }
    }
    fn analytical_collider(&self) -> Option<Collider> {
        Some(Collider::cuboid(
            self.major_radius + self.minor_radius,
            self.minor_radius * 2.0,
            self.major_radius + self.minor_radius,
        ))
    }
}

struct PlaneShape {
    size: [f32; 2],
    subdivisions: u32,
}

impl PrimitiveShape for PlaneShape {
    fn base_mesh(&self) -> Mesh {
        Plane3d::new(Vec3::Y, Vec2::new(self.size[0] / 2.0, self.size[1] / 2.0))
            .mesh()
            .subdivisions(self.subdivisions)
            .build()
    }
    fn analytical_collider(&self) -> Option<Collider> {
        Some(Collider::cuboid(self.size[0], 0.01, self.size[1]))
    }
}

struct TetrahedronShape<'a> {
    size: f32,
    torture: &'a TortureParams,
}

impl TetrahedronShape<'_> {
    /// Apex + the three base corners, shared by mesh and collider.
    fn corners(&self) -> [Vec3; 4] {
        let s = self.size;
        [
            Vec3::new(0.0, 1.0, 0.0) * s,
            Vec3::new(-1.0, -1.0, 1.0).normalize() * s,
            Vec3::new(1.0, -1.0, 1.0).normalize() * s,
            Vec3::new(0.0, -1.0, -1.0).normalize() * s,
        ]
    }
}

impl PrimitiveShape for TetrahedronShape<'_> {
    fn base_mesh(&self) -> Mesh {
        let [p0, p1, p2, p3] = self.corners();
        let mut mesh = Tetrahedron::new(p0, p1, p2, p3).mesh().build();
        // Four flat faces have no interior vertices — subdivide so the
        // nonlinear deforms (twist / bend / bulge) have something to move.
        if !self.torture.deforms_are_identity() {
            subdivide_flat(&mut mesh, DEFORM_SUBDIV_LEVELS);
        }
        mesh
    }
    fn analytical_collider(&self) -> Option<Collider> {
        Some(
            Collider::convex_hull(self.corners().to_vec())
                .unwrap_or_else(|| Collider::sphere(self.size)),
        )
    }
}

struct TubeShape<'a> {
    radius: f32,
    inner_radius: f32,
    height: f32,
    resolution: u32,
    torture: &'a TortureParams,
}

impl PrimitiveShape for TubeShape<'_> {
    fn base_mesh(&self) -> Mesh {
        if self.torture.cuts_are_identity() && self.torture.deforms_are_identity() {
            build_tube_mesh(self.radius, self.inner_radius, self.height, self.resolution)
        } else {
            let (a0, a1) = path_cut_angles(self.torture);
            let inner_frac = (self.inner_radius / self.radius.max(1e-4)).clamp(0.0, 0.999);
            build_swept_frustum(
                self.radius,
                self.radius,
                self.height,
                self.resolution,
                DEFORM_ROWS,
                inner_frac,
                a0,
                a1,
                self.torture.profile_cut.0[0],
                self.torture.profile_cut.0[1],
            )
        }
    }
    fn analytical_collider(&self) -> Option<Collider> {
        // The bore is not a walk-through volume — a solid outer cylinder is
        // the right standoff for a pipe / curb prop.
        Some(Collider::cylinder(self.radius, self.height))
    }
}

struct BevelShape<'a> {
    size: [f32; 3],
    bevel: f32,
    bevel_segments: u32,
    torture: &'a TortureParams,
}

impl PrimitiveShape for BevelShape<'_> {
    fn base_mesh(&self) -> Mesh {
        use std::f32::consts::TAU;
        if self.torture.cuts_are_identity() && self.torture.deforms_are_identity() {
            return build_bevel_mesh(self.size, self.bevel, self.bevel_segments);
        }
        // Same SL box cuts as the Cuboid, on the rounded-rect footprint.
        let (a0, a1) = if self.torture.cuts_are_identity() {
            (0.0, TAU)
        } else {
            path_cut_angles(self.torture)
        };
        let rows = if self.torture.deforms_are_identity() {
            1
        } else {
            DEFORM_ROWS
        };
        build_profile_sweep(
            &rounded_rect_profile(
                self.size[0] * 0.5,
                self.size[2] * 0.5,
                self.bevel,
                self.bevel_segments,
            ),
            self.size[1],
            rows,
            self.torture.hollow.0,
            a0,
            a1,
            self.torture.profile_cut.0[0],
            self.torture.profile_cut.0[1],
        )
    }
    fn analytical_collider(&self) -> Option<Collider> {
        // The bevel's footprint is within its size box; the box is a tight
        // enough standoff (the chamfer only shaves the corners).
        Some(Collider::cuboid(self.size[0], self.size[1], self.size[2]))
    }
}

struct WedgeShape<'a> {
    size: [f32; 3],
    torture: &'a TortureParams,
}

impl PrimitiveShape for WedgeShape<'_> {
    fn base_mesh(&self) -> Mesh {
        let mut mesh = build_wedge_mesh(self.size);
        if !self.torture.deforms_are_identity() {
            subdivide_flat(&mut mesh, DEFORM_SUBDIV_LEVELS);
        }
        mesh
    }
    fn analytical_collider(&self) -> Option<Collider> {
        let (w, h, d) = (self.size[0] * 0.5, self.size[1] * 0.5, self.size[2] * 0.5);
        let corners = vec![
            Vec3::new(-w, -h, -d),
            Vec3::new(-w, -h, d),
            Vec3::new(-w, h, -d),
            Vec3::new(w, -h, -d),
            Vec3::new(w, -h, d),
            Vec3::new(w, h, -d),
        ];
        Some(
            Collider::convex_hull(corners)
                .unwrap_or_else(|| Collider::cuboid(self.size[0], self.size[1], self.size[2])),
        )
    }
}

struct SuperellipsoidShape<'a> {
    half_extents: [f32; 3],
    exponent_ns: f32,
    exponent_ew: f32,
    latitudes: u32,
    longitudes: u32,
    torture: &'a TortureParams,
}

impl PrimitiveShape for SuperellipsoidShape<'_> {
    fn base_mesh(&self) -> Mesh {
        use std::f32::consts::TAU;
        let (lon0, lon1) = if self.torture.cuts_are_identity() {
            (0.0, TAU)
        } else {
            path_cut_angles(self.torture)
        };
        build_superellipsoid(
            self.half_extents,
            self.exponent_ns,
            self.exponent_ew,
            self.latitudes,
            self.longitudes,
            lon0,
            lon1,
            self.torture.profile_cut.0[0],
            self.torture.profile_cut.0[1],
            self.torture.hollow.0,
        )
    }
    fn analytical_collider(&self) -> Option<Collider> {
        // Convex for exponents ≤ 2 (the sanitiser tops out at 2.5, where the
        // hull mildly over-covers the pinch — the usual standoff trade). A
        // coarse analytic sampling keeps this cheap; degenerate extents fall
        // back to a bounding sphere.
        let points =
            superellipsoid_hull_points(self.half_extents, self.exponent_ns, self.exponent_ew);
        Some(Collider::convex_hull(points).unwrap_or_else(|| {
            Collider::sphere(
                self.half_extents[0]
                    .max(self.half_extents[1])
                    .max(self.half_extents[2]),
            )
        }))
    }
}

struct SpineShape<'a> {
    points: Vec<(Vec3, f32)>,
    resolution: u32,
    samples_per_segment: u32,
    torture: &'a TortureParams,
}

impl PrimitiveShape for SpineShape<'_> {
    fn base_mesh(&self) -> Mesh {
        use std::f32::consts::TAU;
        let (a0, a1) = if self.torture.cuts_are_identity() {
            (0.0, TAU)
        } else {
            path_cut_angles(self.torture)
        };
        build_spine_mesh(
            &self.points,
            self.resolution,
            self.samples_per_segment,
            a0,
            a1,
            self.torture.profile_cut.0[0],
            self.torture.profile_cut.0[1],
            self.torture.hollow.0,
        )
    }
    fn analytical_collider(&self) -> Option<Collider> {
        // Hull of a coarse resample of the same stations the mesh uses; the
        // concave side of a bend hull-fills (standoff-over-fidelity, like
        // every tortured prim).
        let points = spine_hull_points(&self.points);
        Some(Collider::convex_hull(points).unwrap_or_else(|| Collider::sphere(0.5)))
    }
}

struct LatheShape<'a> {
    points: Vec<(f32, f32)>,
    resolution: u32,
    smooth: bool,
    torture: &'a TortureParams,
}

impl PrimitiveShape for LatheShape<'_> {
    fn base_mesh(&self) -> Mesh {
        use std::f32::consts::TAU;
        let (a0, a1) = if self.torture.cuts_are_identity() {
            (0.0, TAU)
        } else {
            path_cut_angles(self.torture)
        };
        build_lathe_mesh(
            &self.points,
            self.resolution,
            self.smooth,
            self.torture.hollow.0,
            a0,
            a1,
            self.torture.profile_cut.0[0],
            self.torture.profile_cut.0[1],
        )
    }
    fn analytical_collider(&self) -> Option<Collider> {
        let points = lathe_hull_points(&self.points, self.smooth);
        Some(Collider::convex_hull(points).unwrap_or_else(|| Collider::sphere(0.5)))
    }
}

struct BlobGroupShape<'a> {
    elements: &'a [crate::pds::generator::BlobElement],
    resolution: u32,
    uv_mapping: crate::pds::generator::UvMapping,
    torture: &'a TortureParams,
}

impl PrimitiveShape for BlobGroupShape<'_> {
    fn base_mesh(&self) -> Mesh {
        use std::f32::consts::TAU;
        let (a0, a1) = if self.torture.cuts_are_identity() {
            (0.0, TAU)
        } else {
            path_cut_angles(self.torture)
        };
        build_blob_mesh(
            self.elements,
            self.resolution,
            a0,
            a1,
            self.torture.profile_cut.0[0],
            self.torture.profile_cut.0[1],
            self.torture.hollow.0,
            self.uv_mapping,
        )
    }
    fn analytical_collider(&self) -> Option<Collider> {
        // Hull of the additive elements' support samples — carves are
        // interior detail a standoff hull rightly ignores.
        let points = blob_hull_points(self.elements);
        if points.is_empty() {
            return None;
        }
        Some(Collider::convex_hull(points).unwrap_or_else(|| Collider::sphere(0.25)))
    }
}

struct HelixShape<'a> {
    radius: f32,
    tube_radius: f32,
    pitch: f32,
    turns: f32,
    resolution: u32,
    torture: &'a TortureParams,
}

impl PrimitiveShape for HelixShape<'_> {
    fn base_mesh(&self) -> Mesh {
        use std::f32::consts::{FRAC_PI_2, TAU};
        // Minor-arc profile-cut shares the torus convention: the band's 0/1
        // endpoints sit on the tube's frame-up pole (the +FRAC_PI_2 phase).
        let (min0, min1) = (
            self.torture.profile_cut.0[0] * TAU + FRAC_PI_2,
            self.torture.profile_cut.0[1] * TAU + FRAC_PI_2,
        );
        build_helix_mesh(
            self.radius,
            self.tube_radius,
            self.pitch,
            self.turns,
            self.resolution,
            self.torture.path_cut.0[0],
            self.torture.path_cut.0[1],
            min0,
            min1,
            self.torture.hollow.0,
        )
    }
    fn analytical_collider(&self) -> Option<Collider> {
        // Decorative; a bounding cylinder is a cheap standoff for a
        // spring/rail.
        Some(Collider::cylinder(
            self.radius + self.tube_radius,
            (self.turns.abs() * self.pitch).max(self.tube_radius * 2.0),
        ))
    }
}
