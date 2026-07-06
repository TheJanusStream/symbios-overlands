//! Element ⇄ transform mapping and the drag-end record writeback.
//!
//! A proxy's local `Transform` (in the blob prim's mesh space) encodes the
//! element per shape:
//!
//! * translation ⇄ `position`, rotation ⇄ `rotation` — all shapes.
//! * **Sphere** — proxy is the shared unit sphere, `scale = splat(radii[0])`;
//!   commit reads the mean scale component back into `radii[0]` (the gizmo
//!   is restricted to uniform scale for spheres, so the mean is exact
//!   there and merely robust against a non-uniform parent frame).
//! * **Ellipsoid** — unit sphere, `scale = radii`; commit reads per-axis.
//! * **Capsule** — the proxy mesh is built at the element's real
//!   `(radius, half-length)` with `scale = ONE`, because non-uniformly
//!   scaling a capsule mesh would distort its caps. The drag's scale is
//!   therefore a *multiplier*: commit folds `(x+z)/2` into the tube
//!   radius and `y` into the half-length.
//!
//! All writes clamp with the same bounds as
//! `pds::sanitize::primitive`'s BlobGroup arm, so a gizmo commit can never
//! produce a record the sanitizer would have to repair.

use bevy::prelude::*;

use crate::pds::generator::{BlobElement, BlobShape, Generator, GeneratorKind};
use crate::pds::sanitize::limits::MAX_BLOB_ELEMENTS;
use crate::pds::types::{Fp3, Fp4};

use super::BlobEditKey;
use crate::editor_gizmo::ActiveTarget;

/// Element-drag session info captured at the drag's rising edge, so a
/// mid-drag selection change (GUI click) can't reroute the writeback.
#[derive(Clone)]
pub(crate) struct BlobDragInfo {
    pub(crate) key: BlobEditKey,
    pub(crate) index: usize,
    /// Shift held at drag start: insert the dragged pose as a new element
    /// after `index` (the GUI's ⧉ convention) instead of overwriting.
    pub(crate) duplicate: bool,
}

/// Position/dimension clamps mirroring `sanitize_primitive`'s BlobGroup arm.
fn clamp_pos(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(-100.0, 100.0)
    } else {
        0.0
    }
}

fn clamp_dim(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.01, 100.0)
    } else {
        1.0
    }
}

/// Unit rotation or identity — mirrors the mesher's `resolve()` guard.
fn safe_unit(q: Quat) -> Quat {
    if q.length_squared() > 1e-6 {
        q.normalize()
    } else {
        Quat::IDENTITY
    }
}

/// The local `Transform` (blob mesh space) a proxy is spawned/updated at.
pub(crate) fn proxy_local_transform(e: &BlobElement) -> Transform {
    let scale = match e.shape {
        BlobShape::Sphere | BlobShape::Unknown => Vec3::splat(e.radii.0[0]),
        BlobShape::Ellipsoid => Vec3::from_array(e.radii.0),
        // Radii are baked into the capsule proxy's mesh; see module docs.
        BlobShape::Capsule => Vec3::ONE,
    };
    Transform {
        translation: Vec3::from_array(e.position.0),
        rotation: safe_unit(Quat::from_array(e.rotation.0)),
        scale,
    }
}

/// Fold a dragged proxy-local transform back into the element (see module
/// docs for the per-shape scale mapping). Used by both the drag-end commit
/// and the in-drag preview, so the previewed surface is exactly the
/// surface a release at that instant would commit.
pub(crate) fn apply_local_to_element(e: &mut BlobElement, tf: &Transform) {
    e.position = Fp3([
        clamp_pos(tf.translation.x),
        clamp_pos(tf.translation.y),
        clamp_pos(tf.translation.z),
    ]);
    e.rotation = Fp4(safe_unit(tf.rotation).to_array());
    let s = tf.scale.abs();
    match e.shape {
        BlobShape::Sphere | BlobShape::Unknown => {
            e.radii.0[0] = clamp_dim((s.x + s.y + s.z) / 3.0);
        }
        BlobShape::Ellipsoid => {
            e.radii = Fp3([clamp_dim(s.x), clamp_dim(s.y), clamp_dim(s.z)]);
        }
        BlobShape::Capsule => {
            e.radii.0[0] = clamp_dim(e.radii.0[0] * (s.x + s.z) / 2.0);
            e.radii.0[1] = clamp_dim(e.radii.0[1] * s.y);
        }
    }
}

/// Write the committed element into the generator tree. Returns the index
/// the edit landed at (`index`, or `index + 1` for a duplicate) so the
/// caller can move the selection onto it; `None` when the tree reshaped
/// mid-drag and the write was skipped.
///
/// A duplicate against a full element list degrades to a plain move — the
/// user still gets the pose they dragged to, minus the copy.
fn commit_element_into_generator(
    root: &mut Generator,
    path: &[usize],
    index: usize,
    local: &Transform,
    duplicate: bool,
) -> Option<usize> {
    let node = super::node_at_path_mut(root, path)?;
    let GeneratorKind::BlobGroup { elements, .. } = &mut node.kind else {
        return None;
    };
    let mut edited = *elements.get(index)?;
    apply_local_to_element(&mut edited, local);
    if duplicate && elements.len() < MAX_BLOB_ELEMENTS {
        elements.insert(index + 1, edited);
        Some(index + 1)
    } else {
        elements[index] = edited;
        Some(index)
    }
}

/// Route a finished element drag into the owning record. `local` is the
/// proxy's committed blob-local transform (the caller already resolved
/// world → local against the detached parent). Returns the final element
/// index on success.
pub(crate) fn commit_blob_element_drag(
    info: &BlobDragInfo,
    local: &Transform,
    room_record: Option<&mut crate::pds::RoomRecord>,
    avatar_record: Option<&mut crate::state::LiveAvatarRecord>,
) -> Option<usize> {
    match info.key.target {
        ActiveTarget::Room => {
            let generator_ref = info.key.generator_ref.as_ref()?;
            let root = room_record?.generators.get_mut(generator_ref)?;
            commit_element_into_generator(root, &info.key.path, info.index, local, info.duplicate)
        }
        ActiveTarget::Avatar => commit_element_into_generator(
            &mut avatar_record?.0.visuals,
            &info.key.path,
            info.index,
            local,
            info.duplicate,
        ),
        ActiveTarget::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::types::Fp;

    fn sphere_at(pos: [f32; 3], r: f32) -> BlobElement {
        BlobElement {
            shape: BlobShape::Sphere,
            position: Fp3(pos),
            rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
            radii: Fp3([r, r, r]),
            subtract: false,
            blend: Fp(0.1),
        }
    }

    fn blob_node(elements: Vec<BlobElement>) -> Generator {
        let kind = GeneratorKind::default_primitive_for_tag("BlobGroup").unwrap();
        let mut node = Generator {
            kind,
            transform: Default::default(),
            children: Vec::new(),
            audio: Default::default(),
        };
        if let GeneratorKind::BlobGroup { elements: e, .. } = &mut node.kind {
            *e = elements;
        }
        node
    }

    #[test]
    fn sphere_round_trips_through_proxy_transform() {
        let mut e = sphere_at([1.0, 2.0, -3.0], 0.4);
        let tf = proxy_local_transform(&e);
        assert_eq!(tf.translation, Vec3::new(1.0, 2.0, -3.0));
        assert_eq!(tf.scale, Vec3::splat(0.4));
        // Identity edit → identical element.
        let before = e;
        apply_local_to_element(&mut e, &tf);
        assert_eq!(e, before);
    }

    #[test]
    fn ellipsoid_scale_maps_to_per_axis_radii() {
        let mut e = sphere_at([0.0; 3], 1.0);
        e.shape = BlobShape::Ellipsoid;
        e.radii = Fp3([0.5, 1.0, 2.0]);
        let mut tf = proxy_local_transform(&e);
        assert_eq!(tf.scale, Vec3::new(0.5, 1.0, 2.0));
        tf.scale = Vec3::new(0.25, 1.5, 2.0);
        apply_local_to_element(&mut e, &tf);
        assert_eq!(e.radii.0, [0.25, 1.5, 2.0]);
    }

    #[test]
    fn capsule_scale_multiplies_baked_radii() {
        let mut e = sphere_at([0.0; 3], 1.0);
        e.shape = BlobShape::Capsule;
        e.radii = Fp3([0.2, 0.8, 0.0]);
        let tf = proxy_local_transform(&e);
        // Radii live in the mesh, not the transform.
        assert_eq!(tf.scale, Vec3::ONE);
        // A 2× uniform drag doubles both tube radius and half-length.
        let dragged = Transform {
            scale: Vec3::splat(2.0),
            ..tf
        };
        apply_local_to_element(&mut e, &dragged);
        assert!((e.radii.0[0] - 0.4).abs() < 1e-6);
        assert!((e.radii.0[1] - 1.6).abs() < 1e-6);
    }

    #[test]
    fn commit_clamps_to_sanitize_bounds() {
        let mut e = sphere_at([0.0; 3], 1.0);
        let tf = Transform {
            translation: Vec3::new(1e6, f32::NAN, -1e6),
            rotation: Quat::from_array([0.0; 4]), // degenerate → identity
            scale: Vec3::splat(1e6),
        };
        apply_local_to_element(&mut e, &tf);
        assert_eq!(e.position.0, [100.0, 0.0, -100.0]);
        assert_eq!(e.rotation.0, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(e.radii.0[0], 100.0);
    }

    #[test]
    fn commit_writes_element_at_path() {
        let mut root = blob_node(vec![sphere_at([0.0; 3], 0.25), sphere_at([1.0; 3], 0.25)]);
        let local = Transform::from_xyz(2.0, 3.0, 4.0).with_scale(Vec3::splat(0.5));
        let landed = commit_element_into_generator(&mut root, &[], 1, &local, false);
        assert_eq!(landed, Some(1));
        let GeneratorKind::BlobGroup { elements, .. } = &root.kind else {
            panic!("kind changed");
        };
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[1].position.0, [2.0, 3.0, 4.0]);
        assert!((elements[1].radii.0[0] - 0.5).abs() < 1e-6);
        // Untouched sibling stays put.
        assert_eq!(elements[0].position.0, [0.0; 3]);
    }

    #[test]
    fn duplicate_inserts_after_source_and_reports_new_index() {
        let mut root = blob_node(vec![sphere_at([0.0; 3], 0.25)]);
        let local = Transform::from_xyz(5.0, 0.0, 0.0).with_scale(Vec3::splat(0.25));
        let landed = commit_element_into_generator(&mut root, &[], 0, &local, true);
        assert_eq!(landed, Some(1));
        let GeneratorKind::BlobGroup { elements, .. } = &root.kind else {
            panic!("kind changed");
        };
        assert_eq!(elements.len(), 2);
        // Original untouched, copy carries the dragged pose.
        assert_eq!(elements[0].position.0, [0.0; 3]);
        assert_eq!(elements[1].position.0, [5.0, 0.0, 0.0]);
    }

    #[test]
    fn duplicate_on_full_list_degrades_to_move() {
        let mut root = blob_node(vec![sphere_at([0.0; 3], 0.25); MAX_BLOB_ELEMENTS]);
        let local = Transform::from_xyz(5.0, 0.0, 0.0).with_scale(Vec3::splat(0.25));
        let landed = commit_element_into_generator(&mut root, &[], 0, &local, true);
        assert_eq!(landed, Some(0));
        let GeneratorKind::BlobGroup { elements, .. } = &root.kind else {
            panic!("kind changed");
        };
        assert_eq!(elements.len(), MAX_BLOB_ELEMENTS);
        assert_eq!(elements[0].position.0, [5.0, 0.0, 0.0]);
    }

    #[test]
    fn dangling_path_or_index_skips_write() {
        let mut root = blob_node(vec![sphere_at([0.0; 3], 0.25)]);
        let local = Transform::IDENTITY;
        assert_eq!(
            commit_element_into_generator(&mut root, &[3], 0, &local, false),
            None
        );
        assert_eq!(
            commit_element_into_generator(&mut root, &[], 7, &local, false),
            None
        );
    }
}
