//! World-space geometry measurement for built catalogue entries (#1006,
//! #1009).
//!
//! Catalogue entries are authored as `Generator` trees of parametric
//! primitives under nested transforms, so what a built entry actually
//! *occupies* is not readable from the source: a tapered pylon is
//! narrower than its declared size, a rotated rib sweeps a wider box
//! than its extents, and a child three levels down inherits three
//! transforms. This resolves that the only reliable way — by walking the
//! tree, composing the transforms, and meshing each primitive through
//! the real mesher (`world_builder::build_primitive_mesh`).
//!
//! Consumers: [`super::gateway_fit`] measures a gateway's veil against
//! its frame, and [`super::foundation`] measures how far an entry's
//! geometry reaches below its own origin.
//!
//! Bounds are axis-aligned and therefore conservative for rotated or
//! tapered pieces: a tapered pylon measures as its untapered box.

use bevy::prelude::*;

use crate::pds::{Generator, GeneratorKind, TransformData};
use crate::world_builder::build_primitive_mesh;

/// An axis-aligned box in the prop's ground-relative frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl Bounds {
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    /// Whether `p` lies within the box, with `slack` metres of tolerance
    /// on every side. Callers probing a face centre pass a small positive
    /// slack so an edge that lands exactly on a surface still counts as
    /// covered.
    pub fn contains(&self, p: Vec3, slack: f32) -> bool {
        (0..3).all(|i| p[i] >= self.min[i] - slack && p[i] <= self.max[i] + slack)
    }

    /// Whether the two boxes overlap on axis `i`.
    pub fn overlaps_axis(&self, other: &Bounds, i: usize) -> bool {
        self.min[i] <= other.max[i] && self.max[i] >= other.min[i]
    }
}

/// One solid piece of a gateway frame, in the prop's ground-relative
/// frame. "Solid" here means *occluding geometry* — an emissive trim tube
/// is as good a thing to bury a veil edge in as a masonry jamb, so the
/// only kinds excluded are the non-geometric ones (particles, lights, the
/// veil itself).
#[derive(Clone, Debug)]
pub struct SolidPiece {
    /// Child path from the tree root, for naming the piece in a report.
    pub path: Vec<usize>,
    pub kind_tag: &'static str,
    pub bounds: Bounds,
}

/// Every solid piece of a built entry, resolved into its
/// ground-relative frame.
pub fn solids(root: &Generator) -> Vec<SolidPiece> {
    let mut out = Vec::new();
    let mut veil = None;
    walk(
        root,
        &transform_of(&root.transform),
        &mut Vec::new(),
        &mut out,
        &mut veil,
    );
    out
}

/// The box of a built entry's [`GeneratorKind::Gateway`] veil, if it has
/// one.
pub fn gateway_veil(root: &Generator) -> Option<Bounds> {
    let mut solids = Vec::new();
    let mut veil = None;
    walk(
        root,
        &transform_of(&root.transform),
        &mut Vec::new(),
        &mut solids,
        &mut veil,
    );
    veil
}

pub fn transform_of(t: &TransformData) -> Transform {
    Transform {
        translation: Vec3::from_array(t.translation.0),
        rotation: Quat::from_array(t.rotation.0),
        scale: Vec3::from_array(t.scale.0),
    }
}

pub(super) fn walk(
    node: &Generator,
    world: &Transform,
    path: &mut Vec<usize>,
    solids: &mut Vec<SolidPiece>,
    veil: &mut Option<Bounds>,
) {
    match &node.kind {
        GeneratorKind::Gateway { size } => {
            let half = Vec3::from_array(size.0) * 0.5;
            // The veil spawns as an axis-aligned Cuboid; gateways never
            // rotate it, so its own box needs no corner sweep.
            *veil = Some(Bounds {
                min: world.translation - half,
                max: world.translation + half,
            });
        }
        kind => {
            if let Some(bounds) = mesh_bounds(kind, world) {
                solids.push(SolidPiece {
                    path: path.clone(),
                    kind_tag: kind.kind_tag(),
                    bounds,
                });
            }
        }
    }

    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        let child_world = *world * transform_of(&child.transform);
        walk(child, &child_world, path, solids, veil);
        path.pop();
    }
}

/// World-space bounds of one primitive, meshed through the real mesher so
/// taper, cuts and hollows are reflected. `None` for kinds that carry no
/// occluding geometry (particles, lights, audio-only nodes, L-systems and
/// other non-primitive kinds the mesher does not own).
pub fn mesh_bounds(kind: &GeneratorKind, world: &Transform) -> Option<Bounds> {
    if !is_primitive(kind) {
        return None;
    }
    let built = build_primitive_mesh(kind);
    let positions = built
        .mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)?
        .as_float3()?;
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for p in positions {
        let w = world.transform_point(Vec3::from_array(*p));
        min = min.min(w);
        max = max.max(w);
    }
    (min.x <= max.x).then_some(Bounds { min, max })
}

/// Whether the mesher owns this kind — mirrors the primitive arm of the
/// spawn router in `world_builder::compile::dispatch`.
pub fn is_primitive(kind: &GeneratorKind) -> bool {
    matches!(
        kind,
        GeneratorKind::Cuboid { .. }
            | GeneratorKind::Sphere { .. }
            | GeneratorKind::Cylinder { .. }
            | GeneratorKind::Capsule { .. }
            | GeneratorKind::Cone { .. }
            | GeneratorKind::Torus { .. }
            | GeneratorKind::Plane { .. }
            | GeneratorKind::Tetrahedron { .. }
            | GeneratorKind::Tube { .. }
            | GeneratorKind::Bevel { .. }
            | GeneratorKind::Wedge { .. }
            | GeneratorKind::Helix { .. }
            | GeneratorKind::Superellipsoid { .. }
            | GeneratorKind::Spine { .. }
            | GeneratorKind::Lathe { .. }
            | GeneratorKind::BlobGroup { .. }
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{cuboid_tapered, id_quat, prim, solid};
    use crate::pds::SovereignMaterialSettings;

    /// Bounds come from the real mesher, so a taper narrows the measured
    /// box rather than being ignored.
    #[test]
    fn measured_bounds_track_the_mesher() {
        let mat = SovereignMaterialSettings::default();
        let straight = prim(
            solid(cuboid_tapered([2.0, 2.0, 2.0], 0.0, mat)),
            [0.0, 1.0, 0.0],
            id_quat(),
        );
        let bounds = mesh_bounds(&straight.kind, &transform_of(&straight.transform))
            .expect("cuboid has bounds");
        assert!((bounds.size().x - 2.0).abs() < 1e-4, "{bounds:?}");
        assert!((bounds.center().y - 1.0).abs() < 1e-4, "{bounds:?}");
    }

    /// A nested child's bounds carry its parents' transforms — the whole
    /// reason this measures the built tree instead of reading sizes.
    #[test]
    fn nested_children_inherit_their_parents_transforms() {
        let mat = SovereignMaterialSettings::default();
        let mut root = prim(
            solid(cuboid_tapered([1.0, 1.0, 1.0], 0.0, mat.clone())),
            [0.0, 0.5, 0.0],
            id_quat(),
        );
        root.children.push(prim(
            solid(cuboid_tapered([1.0, 1.0, 1.0], 0.0, mat)),
            [0.0, 3.0, 0.0],
            id_quat(),
        ));
        let pieces = solids(&root);
        assert_eq!(pieces.len(), 2);
        // Child authored at +3 under a root at +0.5 sits at 3.5.
        let child = pieces.iter().find(|p| p.path == vec![0]).expect("child");
        assert!((child.bounds.center().y - 3.5).abs() < 1e-4, "{child:?}");
    }
}
