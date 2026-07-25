//! Per-face material grouping (#959): the record analysis that decides which
//! faces share a material, and the mesh surgery that splits a primitive into
//! one sub-mesh per group.
//!
//! # Groups, not faces
//!
//! Draw calls scale with *materials*, not faces. A cuboid whose six sides all
//! wear the prim's own material stays exactly one mesh and one entity — the
//! pre-#959 spawn path, byte for byte — and a cuboid with one painted face
//! becomes two. Two faces that resolve to the same material and the same
//! projection share a group, so a "front and back are glass, the rest is
//! concrete" prim costs two draw calls, not six.
//!
//! # Why the plan is separate from the split
//!
//! [`plan_faces`] reads only the record: it is cheap, allocation-light, and
//! runs on every spawn to decide the mesh-cache key and to resolve materials.
//! [`build_primitive_groups`] does the meshing and runs only on a cache miss.
//!
//! # Cache keying
//!
//! [`FacePlan::signature`] hashes the *structure* of the split — which faces
//! land in which group, and each group's projection — and deliberately not
//! the materials themselves. Recolouring a face therefore reuses its cached
//! mesh, while genuinely re-partitioning (two faces becoming one material,
//! say) mints a new key. A prim with no overrides keeps the plain geometry
//! fingerprint, so every already-cached entry stays valid.

use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;

use crate::pds::GeneratorKind;
use crate::pds::generator::{FaceKey, UvMapping};

use super::faces::FaceTable;

/// One material group: where its material comes from, and the projection its
/// triangles carry.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupPlan {
    /// `None` — the prim's own `material`; `Some(i)` — the material of the
    /// record's `faces[i]` override.
    pub source: Option<usize>,
    /// Effective projection: the override's `uv_mapping`, or the prim's own
    /// when the override inherits.
    pub mapping: UvMapping,
}

/// How a primitive's faces partition into material groups.
#[derive(Clone, Debug, PartialEq)]
pub struct FacePlan {
    /// Groups in first-appearance order; group `0` is always the prim's base
    /// material, so a face with no override needs no lookup.
    pub groups: Vec<GroupPlan>,
    /// Face → group, for the faces an override moves off the base. Kept in
    /// the record's own order so two records that differ only in the order
    /// of their overrides hash differently (their group *sources* differ).
    assignment: Vec<(FaceKey, usize)>,
}

impl FacePlan {
    /// The group a face's triangles belong to; faces with no override fall
    /// to the base group.
    pub fn group_of(&self, face: FaceKey) -> usize {
        self.assignment
            .iter()
            .find(|(f, _)| *f == face)
            .map(|(_, g)| *g)
            .unwrap_or(0)
    }

    /// `true` when the prim renders as one mesh with one material — no
    /// overrides, or overrides that all resolve to the base material and
    /// projection. This is the path that must stay identical to pre-#959.
    pub fn is_whole(&self) -> bool {
        self.groups.len() == 1
    }

    /// Structure-only hash for the mesh cache key: the partition and the
    /// projections, never the materials (see the [module docs](self)).
    pub fn signature(&self) -> u64 {
        use std::hash::{DefaultHasher, Hash, Hasher};
        let mut h = DefaultHasher::new();
        for (face, group) in &self.assignment {
            face.hash(&mut h);
            group.hash(&mut h);
        }
        for g in &self.groups {
            // `UvMapping` is a fieldless open union; its discriminant is the
            // whole value and hashes without needing a derive on the record
            // type.
            std::mem::discriminant(&g.mapping).hash(&mut h);
        }
        h.finish()
    }
}

/// Partition a primitive's faces into material groups.
///
/// Non-primitive kinds (which never reach the primitive spawn path) plan as
/// a single base group, so callers need no separate guard.
pub fn plan_faces(kind: &GeneratorKind) -> FacePlan {
    let base_mapping = kind.uv_mapping().unwrap_or_default();
    let mut groups = vec![GroupPlan {
        source: None,
        mapping: base_mapping,
    }];
    let mut assignment = Vec::new();

    let (Some(base_material), Some(overrides)) = (kind.material(), kind.faces()) else {
        return FacePlan { groups, assignment };
    };

    for (i, ov) in overrides.iter().enumerate() {
        let mapping = ov.uv_mapping.unwrap_or(base_mapping);
        // An override that lands on the base material *and* the base
        // projection is a no-op — it must not split the prim into two
        // identical draw calls.
        if &ov.material == base_material && mapping == base_mapping {
            continue;
        }
        let existing = groups.iter().position(|g| {
            g.mapping == mapping && group_material(kind, g).is_some_and(|m| m == &ov.material)
        });
        let group = existing.unwrap_or_else(|| {
            groups.push(GroupPlan {
                source: Some(i),
                mapping,
            });
            groups.len() - 1
        });
        assignment.push((ov.face, group));
    }

    FacePlan { groups, assignment }
}

/// The material a planned group draws with: the prim's own, or the
/// override's that founded the group.
pub fn group_material<'a>(
    kind: &'a GeneratorKind,
    group: &GroupPlan,
) -> Option<&'a crate::pds::texture::SovereignMaterialSettings> {
    match group.source {
        None => kind.material(),
        Some(i) => kind.faces().and_then(|f| f.get(i)).map(|o| &o.material),
    }
}

/// One built group: its mesh, the face each of *its* triangles belongs to
/// (for click-picking, #961), and which planned group it came from.
pub struct GroupMesh {
    pub mesh: Mesh,
    pub faces: FaceTable,
    /// Index into [`FacePlan::groups`] — how the spawner finds the material.
    pub group: usize,
}

/// Build one mesh per material group.
///
/// A group whose faces are all *dormant* — an override addressing a face the
/// current cut state does not produce — yields no triangles and is dropped,
/// which is exactly the "the override waits until the face comes back"
/// behaviour the record model promises.
pub fn build_primitive_groups(kind: &GeneratorKind, plan: &FacePlan) -> Vec<GroupMesh> {
    let (mut built, tortured) = super::build_primitive_raw(kind);

    // Whole-prim fast path: finish in place, no copy and no surgery.
    if plan.is_whole() {
        let projection = super::projection_for(kind, plan.groups[0].mapping);
        super::finish_uvs(&mut built.mesh, projection, tortured);
        return vec![GroupMesh {
            mesh: built.mesh,
            faces: built.faces,
            group: 0,
        }];
    }

    // One finished copy per *distinct* projection the plan asks for. Doing
    // the projection on the whole prim and slicing afterwards is what keeps
    // adjacent faces sharing a mode aligned: every projection derives its
    // frame from the mesh it is given, so projecting a lone face's triangles
    // would re-centre the pattern on that face alone.
    let mut finished: Vec<(UvMapping, Mesh)> = Vec::new();
    for g in &plan.groups {
        if finished.iter().any(|(m, _)| *m == g.mapping) {
            continue;
        }
        let mut mesh = built.mesh.clone();
        super::finish_uvs(&mut mesh, super::projection_for(kind, g.mapping), tortured);
        finished.push((g.mapping, mesh));
    }

    let mut out = Vec::with_capacity(plan.groups.len());
    for (gi, g) in plan.groups.iter().enumerate() {
        let src = finished
            .iter()
            .find(|(m, _)| *m == g.mapping)
            .map(|(_, mesh)| mesh)
            .expect("every group's projection was finished above");
        let mut tris = Vec::new();
        let mut faces = FaceTable::default();
        for t in 0..built.faces.triangle_count() {
            let Some(face) = built.faces.face_of(t) else {
                continue;
            };
            if plan.group_of(face) == gi {
                tris.push(t);
                faces.push(face, 1);
            }
        }
        if tris.is_empty() {
            continue;
        }
        out.push(GroupMesh {
            mesh: extract_triangles(src, &tris),
            faces,
            group: gi,
        });
    }
    out
}

/// Copy `tris` (indices into `src`'s triangle list) into a new mesh with a
/// compacted vertex buffer — only the vertices those triangles reference,
/// renumbered.
///
/// Compaction rather than "same vertices, fewer indices" because a split
/// prim would otherwise carry a full copy of the vertex buffer per group;
/// unreferenced vertices cost no shading, but they do cost the memory this
/// project tracks closely on wasm.
fn extract_triangles(src: &Mesh, tris: &[u32]) -> Mesh {
    let idx: Vec<u32> = match src.indices() {
        Some(i) => i.iter().map(|v| v as u32).collect(),
        None => Vec::new(),
    };
    let mut remap: Vec<Option<u32>> = vec![None; src.count_vertices()];
    let mut keep: Vec<u32> = Vec::new();
    let mut out_idx: Vec<u32> = Vec::with_capacity(tris.len() * 3);
    for &t in tris {
        for k in 0..3 {
            let old = idx[t as usize * 3 + k];
            let slot = match remap.get(old as usize).copied().flatten() {
                Some(v) => v,
                None => {
                    let v = keep.len() as u32;
                    if let Some(entry) = remap.get_mut(old as usize) {
                        *entry = Some(v);
                    }
                    keep.push(old);
                    v
                }
            };
            out_idx.push(slot);
        }
    }

    let mut out = Mesh::new(
        src.primitive_topology(),
        bevy::asset::RenderAssetUsages::MAIN_WORLD | bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    // The four attributes every primitive mesher emits. Gathering by name
    // rather than iterating `attributes()` because rebuilding an attribute
    // needs its `MeshVertexAttribute`, not just the id the iterator yields.
    if let Some(VertexAttributeValues::Float32x3(v)) = src.attribute(Mesh::ATTRIBUTE_POSITION) {
        out.insert_attribute(Mesh::ATTRIBUTE_POSITION, gather(v, &keep));
    }
    if let Some(VertexAttributeValues::Float32x3(v)) = src.attribute(Mesh::ATTRIBUTE_NORMAL) {
        out.insert_attribute(Mesh::ATTRIBUTE_NORMAL, gather(v, &keep));
    }
    if let Some(VertexAttributeValues::Float32x2(v)) = src.attribute(Mesh::ATTRIBUTE_UV_0) {
        out.insert_attribute(Mesh::ATTRIBUTE_UV_0, gather(v, &keep));
    }
    // Tangents are copied rather than regenerated: recomputing them on a
    // sub-mesh would average over fewer neighbours at the cut edges and
    // shade differently from the unsplit prim.
    if let Some(VertexAttributeValues::Float32x4(v)) = src.attribute(Mesh::ATTRIBUTE_TANGENT) {
        out.insert_attribute(Mesh::ATTRIBUTE_TANGENT, gather(v, &keep));
    }
    out.insert_indices(Indices::U32(out_idx));
    out
}

/// Pick `keep`'s entries out of `src`, in order.
fn gather<T: Copy>(src: &[T], keep: &[u32]) -> Vec<T> {
    keep.iter().map(|&i| src[i as usize]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::generator::FaceOverride;
    use crate::pds::texture::SovereignMaterialSettings;
    use crate::pds::types::Fp3;

    fn painted(face: FaceKey, color: [f32; 3]) -> FaceOverride {
        FaceOverride {
            face,
            material: SovereignMaterialSettings {
                base_color: Fp3(color),
                ..Default::default()
            },
            uv_mapping: None,
        }
    }

    fn cuboid_with(overrides: Vec<FaceOverride>) -> GeneratorKind {
        let mut kind = GeneratorKind::default_primitive_for_tag("Cuboid").unwrap();
        *kind.faces_mut().unwrap() = overrides;
        kind
    }

    #[test]
    fn a_plain_prim_plans_as_one_whole_group() {
        let plan = plan_faces(&GeneratorKind::default_primitive_for_tag("Cuboid").unwrap());
        assert!(plan.is_whole());
        assert_eq!(plan.groups.len(), 1);
        assert_eq!(plan.group_of(FaceKey::Top), 0);
    }

    /// An override that repaints a face in the prim's *own* material changes
    /// nothing — splitting there would buy a second draw call for an
    /// identical pixel.
    #[test]
    fn a_no_op_override_does_not_split() {
        let kind = cuboid_with(vec![FaceOverride {
            face: FaceKey::Top,
            material: SovereignMaterialSettings::default(),
            uv_mapping: None,
        }]);
        assert!(plan_faces(&kind).is_whole());
    }

    /// Faces sharing a material share a group: two painted sides cost two
    /// draw calls (base + paint), not three.
    #[test]
    fn faces_sharing_a_material_share_a_group() {
        let kind = cuboid_with(vec![
            painted(FaceKey::SidePx, [1.0, 0.0, 0.0]),
            painted(FaceKey::SideNx, [1.0, 0.0, 0.0]),
            painted(FaceKey::Top, [0.0, 1.0, 0.0]),
        ]);
        let plan = plan_faces(&kind);
        assert_eq!(plan.groups.len(), 3, "base + red + green");
        assert_eq!(
            plan.group_of(FaceKey::SidePx),
            plan.group_of(FaceKey::SideNx)
        );
        assert_ne!(plan.group_of(FaceKey::SidePx), plan.group_of(FaceKey::Top));
        assert_eq!(plan.group_of(FaceKey::Bottom), 0, "unpainted stays base");
    }

    /// The same material under a different projection is a different mesh,
    /// so it cannot share a group.
    #[test]
    fn a_projection_override_splits_even_at_equal_material() {
        let kind = cuboid_with(vec![FaceOverride {
            face: FaceKey::Top,
            material: SovereignMaterialSettings::default(),
            uv_mapping: Some(UvMapping::PlanarY),
        }]);
        let plan = plan_faces(&kind);
        assert!(!plan.is_whole());
        assert_eq!(plan.groups[1].mapping, UvMapping::PlanarY);
    }

    /// The signature keys the mesh cache, so it must ignore colour (a
    /// recolour reuses the mesh) and track the partition (a re-partition
    /// does not).
    #[test]
    fn signature_tracks_structure_not_colour() {
        let red = plan_faces(&cuboid_with(vec![painted(FaceKey::Top, [1.0, 0.0, 0.0])]));
        let blue = plan_faces(&cuboid_with(vec![painted(FaceKey::Top, [0.0, 0.0, 1.0])]));
        assert_eq!(
            red.signature(),
            blue.signature(),
            "a recolour must not re-key the mesh"
        );

        let other_face = plan_faces(&cuboid_with(vec![painted(
            FaceKey::Bottom,
            [1.0, 0.0, 0.0],
        )]));
        assert_ne!(
            red.signature(),
            other_face.signature(),
            "painting a different face is a different split"
        );

        let two = plan_faces(&cuboid_with(vec![
            painted(FaceKey::Top, [1.0, 0.0, 0.0]),
            painted(FaceKey::Bottom, [0.0, 1.0, 0.0]),
        ]));
        assert_ne!(red.signature(), two.signature());
    }

    /// The split must conserve geometry: every triangle of the whole prim
    /// lands in exactly one group, and each group's own table still names
    /// its triangles.
    #[test]
    fn groups_partition_every_triangle() {
        let kind = cuboid_with(vec![
            painted(FaceKey::Top, [1.0, 0.0, 0.0]),
            painted(FaceKey::SidePx, [0.0, 0.0, 1.0]),
        ]);
        let plan = plan_faces(&kind);
        let whole = super::super::build_primitive_mesh(&kind);
        let groups = build_primitive_groups(&kind, &plan);
        assert_eq!(groups.len(), 3, "base + top + side");

        let total: u32 = groups.iter().map(|g| g.faces.triangle_count()).sum();
        assert_eq!(
            total,
            whole.faces.triangle_count(),
            "split lost or duplicated triangles"
        );
        for g in &groups {
            let tris = g.mesh.indices().map(|i| i.len() / 3).unwrap_or(0) as u32;
            assert_eq!(tris, g.faces.triangle_count(), "table/mesh disagree");
            // Every group's faces all map back to that group.
            for f in g.faces.faces() {
                assert_eq!(plan.group_of(f), g.group);
            }
            // Compaction must not leave a dangling index.
            let verts = g.mesh.count_vertices();
            for i in g.mesh.indices().expect("indexed").iter() {
                assert!(i < verts, "index {i} past {verts} vertices");
            }
        }
    }

    /// An override addressing a face the current cuts do not produce is
    /// dormant: it costs no entity, and the prim still renders whole.
    #[test]
    fn a_dormant_override_yields_no_group() {
        // A plain cuboid has no bore, so a Bore override has no triangles.
        let kind = cuboid_with(vec![painted(FaceKey::Bore, [1.0, 0.0, 0.0])]);
        let plan = plan_faces(&kind);
        assert!(!plan.is_whole(), "the plan still carries the override");
        let groups = build_primitive_groups(&kind, &plan);
        assert_eq!(groups.len(), 1, "only the base group has triangles");
        assert_eq!(groups[0].group, 0);
    }
}
