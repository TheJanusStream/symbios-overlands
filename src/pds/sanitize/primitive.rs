//! Sanitiser for the parametric primitive variants of [`GeneratorKind`]
//! (Cuboid / Sphere / Cylinder / Capsule / Cone / Torus / Plane /
//! Tetrahedron). Mirrors the bounds the World Editor UI exposes so a
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
            twist,
            taper,
            bend,
            ..
        } => {
            size.0 = [c_dim(size.0[0]), c_dim(size.0[1]), c_dim(size.0[2])];
            material.sanitize();
            sanitize_torture(twist, taper, bend);
        }
        GeneratorKind::Sphere {
            radius,
            resolution,
            material,
            twist,
            taper,
            bend,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            *resolution = (*resolution).clamp(0, 10);
            material.sanitize();
            sanitize_torture(twist, taper, bend);
        }
        GeneratorKind::Cylinder {
            radius,
            height,
            resolution,
            material,
            twist,
            taper,
            bend,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            *height = Fp(c_dim(height.0));
            *resolution = (*resolution).clamp(3, 128);
            material.sanitize();
            sanitize_torture(twist, taper, bend);
        }
        GeneratorKind::Capsule {
            radius,
            length,
            latitudes,
            longitudes,
            material,
            twist,
            taper,
            bend,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            *length = Fp(c_dim(length.0));
            *latitudes = (*latitudes).clamp(2, 64);
            *longitudes = (*longitudes).clamp(4, 128);
            material.sanitize();
            sanitize_torture(twist, taper, bend);
        }
        GeneratorKind::Cone {
            radius,
            height,
            resolution,
            material,
            twist,
            taper,
            bend,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            *height = Fp(c_dim(height.0));
            *resolution = (*resolution).clamp(3, 128);
            material.sanitize();
            sanitize_torture(twist, taper, bend);
        }
        GeneratorKind::Torus {
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
            material,
            twist,
            taper,
            bend,
            ..
        } => {
            *minor_radius = Fp(c_dim(minor_radius.0));
            *major_radius = Fp(c_dim(major_radius.0));
            *minor_resolution = (*minor_resolution).clamp(3, 64);
            *major_resolution = (*major_resolution).clamp(3, 128);
            material.sanitize();
            sanitize_torture(twist, taper, bend);
        }
        GeneratorKind::Plane {
            size,
            subdivisions,
            material,
            twist,
            taper,
            bend,
            ..
        } => {
            *size = Fp2([c_dim(size.0[0]), c_dim(size.0[1])]);
            *subdivisions = (*subdivisions).clamp(0, 32);
            material.sanitize();
            sanitize_torture(twist, taper, bend);
        }
        GeneratorKind::Tetrahedron {
            size,
            material,
            twist,
            taper,
            bend,
            ..
        } => {
            *size = Fp(c_dim(size.0));
            material.sanitize();
            sanitize_torture(twist, taper, bend);
        }
        _ => {}
    }
}
