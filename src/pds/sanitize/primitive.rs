//! Sanitiser for the parametric primitive variants of [`GeneratorKind`]
//! (Cuboid / Sphere / Cylinder / Capsule / Cone / Torus / Plane /
//! Tetrahedron / Tube / Bevel / Wedge / Helix). Mirrors the bounds the
//! World Editor UI exposes so a
//! hand-crafted record can't push mesh/collider builders into NaN / OOM
//! territory.

use super::Sanitize;
use super::common::{clamp_finite, sanitize_torture};
use crate::pds::generator::GeneratorKind;
use crate::pds::types::{Fp, Fp2};

pub(super) fn sanitize_primitive(kind: &mut GeneratorKind) {
    let c_dim = |v: f32| clamp_finite(v, 0.01, 100.0, 1.0);
    match kind {
        GeneratorKind::Cuboid {
            size,
            material,
            torture,
            ..
        } => {
            size.0 = [c_dim(size.0[0]), c_dim(size.0[1]), c_dim(size.0[2])];
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Sphere {
            radius,
            resolution,
            material,
            torture,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            // Ico subdivision count is exponential in triangles (~20·4ⁿ), so
            // cap it low: ico 6 is ~82k tris (already far past any shipped
            // content, which tops out at ico 4 ≈ 5k), while the old cap of 10
            // would be ~20M tris per sphere — a single-record perf cliff.
            *resolution = (*resolution).clamp(0, 6);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Cylinder {
            radius,
            height,
            resolution,
            material,
            torture,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            *height = Fp(c_dim(height.0));
            *resolution = (*resolution).clamp(3, 128);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Capsule {
            radius,
            length,
            latitudes,
            longitudes,
            material,
            torture,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            *length = Fp(c_dim(length.0));
            *latitudes = (*latitudes).clamp(2, 64);
            *longitudes = (*longitudes).clamp(4, 128);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Cone {
            radius,
            height,
            resolution,
            material,
            torture,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            *height = Fp(c_dim(height.0));
            *resolution = (*resolution).clamp(3, 128);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Torus {
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
            material,
            torture,
            ..
        } => {
            *minor_radius = Fp(c_dim(minor_radius.0));
            *major_radius = Fp(c_dim(major_radius.0));
            *minor_resolution = (*minor_resolution).clamp(3, 64);
            *major_resolution = (*major_resolution).clamp(3, 128);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Plane {
            size,
            subdivisions,
            material,
            torture,
            ..
        } => {
            *size = Fp2([c_dim(size.0[0]), c_dim(size.0[1])]);
            *subdivisions = (*subdivisions).clamp(0, 32);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Tetrahedron {
            size,
            material,
            torture,
            ..
        } => {
            *size = Fp(c_dim(size.0));
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Tube {
            radius,
            inner_radius,
            height,
            resolution,
            material,
            torture,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            *height = Fp(c_dim(height.0));
            // Bore stays strictly inside the outer wall (0 = a near-solid rod).
            *inner_radius = Fp(clamp_finite(
                inner_radius.0,
                0.0,
                radius.0 * 0.95,
                radius.0 * 0.5,
            ));
            *resolution = (*resolution).clamp(3, 128);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Bevel {
            size,
            bevel,
            bevel_segments,
            material,
            torture,
            ..
        } => {
            size.0 = [c_dim(size.0[0]), c_dim(size.0[1]), c_dim(size.0[2])];
            // The corner radius can't exceed half the smaller footprint axis.
            let max_b = (size.0[0].min(size.0[2]) * 0.5).max(0.0);
            *bevel = Fp(clamp_finite(bevel.0, 0.0, max_b, 0.0));
            *bevel_segments = (*bevel_segments).clamp(1, 16);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Wedge {
            size,
            material,
            torture,
            ..
        } => {
            size.0 = [c_dim(size.0[0]), c_dim(size.0[1]), c_dim(size.0[2])];
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Helix {
            radius,
            tube_radius,
            pitch,
            turns,
            resolution,
            material,
            torture,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            // Wire stays thinner than the helix radius so the tube can't self-
            // intersect through the axis.
            *tube_radius = Fp(clamp_finite(tube_radius.0, 0.01, radius.0 * 0.95, 0.1));
            *pitch = Fp(clamp_finite(pitch.0, 0.0, 100.0, 0.4));
            *turns = Fp(clamp_finite(turns.0, 0.05, 16.0, 3.0));
            *resolution = (*resolution).clamp(3, 128);
            material.sanitize();
            sanitize_torture(torture);
        }
        _ => {}
    }
}
