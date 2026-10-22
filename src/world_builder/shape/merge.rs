//! A grammar building drawn in as many meshes as it has materials (#1578).
//!
//! The mesher gives each terminal UVs in metres from its own corner
//! (`bevy_symbios_shape::mesh`: U viewer-left to viewer-right, V down from
//! the top), so every bay, pier and storey restarts its texture and two
//! pieces side by side do not continue one pattern - coursed stone breaks at
//! every piece's edge. Here a node's terminals that tile their material are
//! carried into the node's frame and merged into one mesh per material, and
//! each face's UVs are anchored to the node: the face's own mapping, in the
//! same metres and directions, measured from the node's origin instead of
//! the terminal's corner. Two faces in one plane that face one way continue
//! one pattern across their edge, and the building draws as many meshes as
//! it has materials rather than as it has terminals.
//!
//! Anchored to the node and not the world, so a placement moved or turned
//! carries its texture with it, as before; and a face's mapping keeps its
//! direction, so its tangents hold as the mesher made them.
//!
//! What stays a terminal of its own ([`mergeable`]): an alpha card, whose
//! texture spans its face once; a turned terminal, whose side wraps its
//! texture round it; a material only one terminal wears, which merging would
//! only copy; and every terminal of a node with more than
//! [`MERGE_MAX_TERMINALS`] that would merge, or with more than the room's
//! buildings leave room for ([`MERGE_ROOM_MAX_TERMINALS`]), so that the
//! merged meshes stay a bounded size. Those draw as before, sharing one mesh
//! per profile and size across every building.

use bevy::asset::RenderAssetUsages;
use bevy::math::{Affine3A, Mat3, Vec3};
use bevy::mesh::{Indices, Mesh, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::{Assets, Handle};

use super::ShapeInstance;

/// The most terminals a node merges (#1578): a node with more that would
/// merge draws each as its own mesh, as before. A merged terminal costs its
/// vertices once more on the GPU and once on the CPU, where the scene's
/// picking reads them - a box about 1.3 KiB each way - so this holds one
/// building's merged meshes under 8 MiB each way. The largest grammar the
/// catalogue ships, the castle, merges 4,032.
pub(super) const MERGE_MAX_TERMINALS: usize = 6144;

/// The most terminals the meshes merged for a room's buildings draw, every
/// variant the room's own compile draws together (#1578): a variant that
/// would take them past it draws each terminal as its own mesh, as before.
/// A building's merged meshes are its variant's own, so a seed of its own on
/// each of many copies draws one set for each - a record's placements, each
/// a castle with its own seed, would hold gigabytes - and a merged building
/// charges the room's entity budget a mesh a material, not a terminal, so
/// that budget does not stop them. About 40 MiB each way, eight castles. An
/// avatar, a preview and an instance set's template are not counted: each is
/// one tree, merged only as far as [`MERGE_MAX_TERMINALS`] lets it.
pub(crate) const MERGE_ROOM_MAX_TERMINALS: usize = 32_768;

/// How far apart two anchored UVs at one vertex may be and still be one
/// (m): a vertex two faces share keeps one copy where they agree.
const SAME_UV: f32 = 1.0e-4;

/// One merged mesh of a node: its terminals of one material, in the node's
/// frame, the material slot they wear, and how many they are.
#[derive(Clone, Debug)]
pub struct MergedPart {
    pub mesh: Handle<Mesh>,
    pub material_id: Option<String>,
    pub terminals: usize,
}

/// Whether a terminal the mesher draws with `stretch_uvs` and
/// `round_segments` may merge: not an alpha card, whose texture spans its
/// face once, and not turned, whose side wraps its texture round it.
pub(super) fn mergeable(stretch_uvs: bool, round_segments: u32) -> bool {
    !stretch_uvs && round_segments == 0
}

/// Merge the terminals of `instances` that `merging` marks into one mesh per
/// material slot, each in the node's frame with its UVs anchored to the node
/// (see the module docs), and mark each one merged. A slot only one of them
/// wears stays its own mesh, and so does every one of them where more than
/// [`MERGE_MAX_TERMINALS`], or more than `room`, would merge; so does a slot
/// whose meshes are not the mesher's (a triangle list of positions, normals,
/// UVs and tangents).
///
/// Its merged meshes, and how many terminals would merge with room for them
/// where `room` alone was short of them: zero where they merged, or where
/// none would.
pub(super) fn merge(
    instances: &mut [ShapeInstance],
    merging: &[bool],
    room: usize,
    meshes: &mut Assets<Mesh>,
) -> (Vec<MergedPart>, usize) {
    let mut slots: Vec<(Option<String>, Vec<usize>)> = Vec::new();
    for (i, instance) in instances.iter().enumerate() {
        if !merging[i] {
            continue;
        }
        match slots
            .iter_mut()
            .find(|(slot, _)| *slot == instance.material_id)
        {
            Some((_, members)) => members.push(i),
            None => slots.push((instance.material_id.clone(), vec![i])),
        }
    }
    slots.retain(|(_, members)| members.len() > 1);
    let wanted: usize = slots.iter().map(|(_, members)| members.len()).sum();
    if wanted > MERGE_MAX_TERMINALS {
        return (Vec::new(), 0);
    }
    if wanted > room {
        return (Vec::new(), wanted);
    }
    let mut parts = Vec::with_capacity(slots.len());
    for (material_id, members) in slots {
        let mut merged = Merged::default();
        let whole = members.iter().all(|&i| {
            meshes
                .get(&instances[i].mesh)
                .and_then(|mesh| merged.append(mesh, instances[i].transform.compute_affine()))
                .is_some()
        });
        if !whole {
            continue;
        }
        for &i in &members {
            instances[i].merged = true;
        }
        parts.push(MergedPart {
            mesh: meshes.add(merged.into_mesh()),
            material_id,
            terminals: members.len(),
        });
    }
    (parts, 0)
}

/// A merged mesh being built: the attributes the mesher gives a terminal,
/// and 32-bit indices.
#[derive(Default)]
pub(super) struct Merged {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    tangents: Vec<[f32; 4]>,
    indices: Vec<u32>,
    scratch: Scratch,
}

/// What adding a terminal works in, kept from one terminal to the next so
/// that a node's thousands of them do not each allocate it again: the
/// terminal's indices, its vertices placed in the node, and each vertex's
/// copies so far - the anchored UV and the merged index of each.
#[derive(Default)]
struct Scratch {
    corners: Vec<u32>,
    placed: Vec<Vec3>,
    copies: Vec<Vec<([f32; 2], u32)>>,
}

impl Merged {
    /// Add `mesh`, a terminal's unit mesh, carried into the node's frame by
    /// `affine` - its transform there - as the renderer carries it:
    /// positions by the affine, normals by its inverse transpose, tangents
    /// by its linear part with their handedness flipped where it mirrors.
    /// The index order is kept, so a mirroring terminal keeps the winding
    /// it is drawn with on its own. Each face's UVs are anchored to the node
    /// ([`anchored`]); a vertex that two faces anchoring differently share
    /// is split, one copy each.
    ///
    /// `None`, adding nothing, where `mesh` is not what the mesher makes: a
    /// triangle list with exactly positions, normals, UVs and tangents.
    pub(super) fn append(&mut self, mesh: &Mesh, affine: Affine3A) -> Option<()> {
        if mesh.primitive_topology() != PrimitiveTopology::TriangleList
            || mesh.attributes().count() != 4
        {
            return None;
        }
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            return None;
        };
        let Some(VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            return None;
        };
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            return None;
        };
        let Some(VertexAttributeValues::Float32x4(tangents)) =
            mesh.attribute(Mesh::ATTRIBUTE_TANGENT)
        else {
            return None;
        };
        let Scratch {
            corners,
            placed,
            copies,
        } = &mut self.scratch;
        corners.clear();
        corners.extend(mesh.indices()?.iter().map(|i| i as u32));
        let (triangles, rest) = corners.as_chunks::<3>();
        if !rest.is_empty() || corners.iter().any(|&i| i as usize >= positions.len()) {
            return None;
        }

        let linear = Mat3::from(affine.matrix3);
        let normal_matrix = linear.inverse().transpose();
        let mirrored = linear.determinant() < 0.0;
        placed.clear();
        placed.extend(
            positions
                .iter()
                .map(|p| affine.transform_point3(Vec3::from_array(*p))),
        );
        copies.iter_mut().for_each(Vec::clear);
        copies.resize_with(positions.len(), Vec::new);
        for corners in triangles {
            let ends = corners.map(|i| i as usize);
            let at = anchored(ends.map(|i| placed[i]), ends.map(|i| uvs[i]));
            for (k, &v) in ends.iter().enumerate() {
                let uv = at[k];
                let index = match copies[v].iter().find(|(seen, _)| {
                    (seen[0] - uv[0]).abs() < SAME_UV && (seen[1] - uv[1]).abs() < SAME_UV
                }) {
                    Some(&(_, index)) => index,
                    None => {
                        let index = self.positions.len() as u32;
                        self.positions.push(placed[v].to_array());
                        self.normals.push(
                            (normal_matrix * Vec3::from_array(normals[v]))
                                .normalize_or_zero()
                                .to_array(),
                        );
                        let t = tangents[v];
                        let along = (linear * Vec3::new(t[0], t[1], t[2])).normalize_or_zero();
                        self.tangents.push([
                            along.x,
                            along.y,
                            along.z,
                            if mirrored { -t[3] } else { t[3] },
                        ]);
                        self.uvs.push(uv);
                        copies[v].push((uv, index));
                        index
                    }
                };
                self.indices.push(index);
            }
        }
        Some(())
    }

    /// The mesh: kept on the CPU as well as uploaded, as a terminal's is,
    /// so the scene's picking can read it (`editor_gizmo::ScenePick`) and an
    /// instance set can bake it (`instances::bake`).
    pub(super) fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, self.tangents);
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }
}

/// A triangle's corners' UVs anchored to the node: each the corner's place
/// in the node, `at`, read along the directions in which the mesher's own
/// `uvs` grow across the triangle - its U and V axes, in the node, in the
/// metres the mesher measured. So the same face of two terminals in one
/// plane reads one mapping, where each counted from its own corner.
///
/// A triangle too thin to have directions keeps the mesher's UVs.
fn anchored(at: [Vec3; 3], uvs: [[f32; 2]; 3]) -> [[f32; 2]; 3] {
    let (e1, e2) = (at[1] - at[0], at[2] - at[0]);
    let n = e1.cross(e2);
    let area = n.length_squared();
    if !area.is_finite() || area <= f32::EPSILON * f32::EPSILON {
        return uvs;
    }
    // The gradient G with G.e1 = d1 and G.e2 = d2 in the triangle's plane.
    let along = |k: usize| {
        let (d1, d2) = (uvs[1][k] - uvs[0][k], uvs[2][k] - uvs[0][k]);
        (e2.cross(n) * d1 + n.cross(e1) * d2) / area
    };
    let (u, v) = (along(0), along(1));
    at.map(|p| [p.dot(u), p.dot(v)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{Quat, Transform};
    use bevy_symbios_shape::mesh::build_profiled_mesh_with;
    use symbios_shape::FaceProfile;

    /// A unit box as the mesher makes it for a scope of `size`.
    fn box_mesh(size: Vec3) -> Mesh {
        build_profiled_mesh_with(&FaceProfile::Rectangle, size, false, 0)
    }

    fn attribute3(mesh: &Mesh, id: bevy::mesh::MeshVertexAttribute) -> Vec<Vec3> {
        match mesh.attribute(id) {
            Some(VertexAttributeValues::Float32x3(v)) => {
                v.iter().map(|p| Vec3::from_array(*p)).collect()
            }
            _ => panic!("a three-component attribute"),
        }
    }

    fn uvs(mesh: &Mesh) -> Vec<[f32; 2]> {
        match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(VertexAttributeValues::Float32x2(v)) => v.clone(),
            _ => panic!("UVs"),
        }
    }

    fn tangents(mesh: &Mesh) -> Vec<[f32; 4]> {
        match mesh.attribute(Mesh::ATTRIBUTE_TANGENT) {
            Some(VertexAttributeValues::Float32x4(v)) => v.clone(),
            _ => panic!("tangents"),
        }
    }

    /// Two 1 m bays side by side, as a wall split in two lays them.
    fn two_bays() -> Mesh {
        let size = Vec3::ONE;
        let mut merged = Merged::default();
        for x in [-0.5f32, 0.5] {
            merged
                .append(
                    &box_mesh(size),
                    Transform::from_xyz(x, 0.5, 0.0)
                        .with_scale(size)
                        .compute_affine(),
                )
                .expect("the mesher's box merges");
        }
        merged.into_mesh()
    }

    /// **Two pieces in one plane continue one pattern** (#1578): where two
    /// bays of a wall meet, every vertex their faces share in place reads
    /// one UV from either side - the face's own mapping measured from the
    /// node, where each bay counted from its own corner and its front's U
    /// ran 0 to 1 again from the edge the other bay's ended at.
    #[test]
    fn two_pieces_in_one_plane_continue_one_pattern() {
        let mesh = two_bays();
        let (positions, normals, uvs) = (
            attribute3(&mesh, Mesh::ATTRIBUTE_POSITION),
            attribute3(&mesh, Mesh::ATTRIBUTE_NORMAL),
            uvs(&mesh),
        );
        let mut shared = 0;
        for i in 0..positions.len() {
            for j in i + 1..positions.len() {
                let same_place = positions[i].distance(positions[j]) < 1e-5;
                let same_face = normals[i].dot(normals[j]) > 0.999;
                if same_place && same_face {
                    shared += 1;
                    let (a, b) = (uvs[i], uvs[j]);
                    assert!(
                        (a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5,
                        "{:?} reads {a:?} on one bay and {b:?} on the other",
                        positions[i]
                    );
                }
            }
        }
        // The front, back, top and bottom faces each meet along an edge of
        // two vertices.
        assert_eq!(shared, 8, "the bays' faces meet at their edge");
        // And the mapping is the mesher's, in metres: the front face (-Z)
        // reads U along -X and V down from the top, from the node's origin.
        for (k, p) in positions.iter().enumerate() {
            if normals[k].z < -0.999 {
                assert!(
                    (uvs[k][0] + p.x).abs() < 1e-5,
                    "front U at {p}: {:?}",
                    uvs[k]
                );
                assert!(
                    (uvs[k][1] + p.y).abs() < 1e-5,
                    "front V at {p}: {:?}",
                    uvs[k]
                );
            }
        }
    }

    /// **A merged mesh draws what its terminals drew** (#1578): the same
    /// triangles in the node's frame, in the same winding, with normals
    /// carried by the inverse transpose and tangents by the linear part -
    /// and a mirroring terminal's tangents flipped in handedness, as the
    /// renderer flips them for a mirroring transform, its index order kept
    /// as the renderer draws it.
    #[test]
    fn a_merged_mesh_draws_what_its_terminals_drew() {
        let size = Vec3::new(2.0, 1.0, 0.5);
        let source = box_mesh(size);
        for mirror in [1.0f32, -1.0] {
            let placed = Transform::from_xyz(3.0, 1.0, -2.0)
                .with_rotation(Quat::from_rotation_y(0.7))
                .with_scale(size * Vec3::new(mirror, 1.0, 1.0));
            let affine = placed.compute_affine();
            let mut merged = Merged::default();
            merged.append(&source, affine).expect("merges");
            let mesh = merged.into_mesh();
            let (src_p, out_p) = (
                attribute3(&source, Mesh::ATTRIBUTE_POSITION),
                attribute3(&mesh, Mesh::ATTRIBUTE_POSITION),
            );
            let src_i: Vec<usize> = source.indices().expect("indexed").iter().collect();
            let out_i: Vec<usize> = mesh.indices().expect("indexed").iter().collect();
            assert_eq!(src_i.len(), out_i.len(), "every triangle");
            for (a, b) in src_i.iter().zip(&out_i) {
                let want = affine.transform_point3(src_p[*a]);
                assert!(
                    want.distance(out_p[*b]) < 1e-5,
                    "{want} drawn at {}",
                    out_p[*b]
                );
            }
            let (src_t, out_t) = (tangents(&source), tangents(&mesh));
            for (a, b) in src_i.iter().zip(&out_i) {
                assert_eq!(
                    out_t[*b][3],
                    src_t[*a][3] * mirror,
                    "mirror {mirror}: the tangent's handedness"
                );
            }
            let normal_matrix = Mat3::from(affine.matrix3).inverse().transpose();
            let src_n = attribute3(&source, Mesh::ATTRIBUTE_NORMAL);
            let out_n = attribute3(&mesh, Mesh::ATTRIBUTE_NORMAL);
            for (a, b) in src_i.iter().zip(&out_i) {
                let want = (normal_matrix * src_n[*a]).normalize();
                assert!(want.dot(out_n[*b]) > 0.9999, "a normal turned off its face");
            }
        }
    }

    fn instance(mesh: Handle<Mesh>, x: f32, material: Option<&str>) -> ShapeInstance {
        ShapeInstance {
            transform: Transform::from_xyz(x, 0.5, 0.0),
            mesh,
            material_id: material.map(str::to_string),
            solid: false,
            merged: false,
        }
    }

    /// **What stays a terminal of its own** (#1578): a material only one
    /// terminal wears, one that `merging` leaves out (a card or a turned
    /// terminal, [`mergeable`]), and every terminal of a node with more than
    /// [`MERGE_MAX_TERMINALS`] that would merge, or more than the room's
    /// buildings leave room for - which says how many would merge with room
    /// for them. The rest merge, one mesh a material.
    #[test]
    fn lone_left_out_and_oversized_terminals_stay_their_own() {
        let mut meshes = Assets::<Mesh>::default();
        let unit = meshes.add(box_mesh(Vec3::ONE));
        let layout = || {
            vec![
                instance(unit.clone(), 0.0, Some("Stone")),
                instance(unit.clone(), 1.0, Some("Stone")),
                instance(unit.clone(), 2.0, Some("Stone")),
                instance(unit.clone(), 3.0, Some("Window")),
                instance(unit.clone(), 4.0, Some("Window")),
                instance(unit.clone(), 5.0, Some("Trim")),
                instance(unit.clone(), 6.0, None),
                instance(unit.clone(), 7.0, None),
            ]
        };
        // The windows are cards.
        let merging = [true, true, true, false, false, true, true, true];
        let mut instances = layout();
        let (parts, starved) = merge(&mut instances, &merging, 5, &mut meshes);
        assert_eq!(starved, 0, "with room for all five");
        let slots: Vec<(Option<&str>, usize)> = parts
            .iter()
            .map(|p| (p.material_id.as_deref(), p.terminals))
            .collect();
        assert_eq!(
            slots,
            [(Some("Stone"), 3), (None, 2)],
            "one mesh a material that merges, with room for all five"
        );
        let merged: Vec<bool> = instances.iter().map(|i| i.merged).collect();
        assert_eq!(
            merged,
            [true, true, true, false, false, false, true, true],
            "the cards and the lone trim stay their own"
        );
        assert!(mergeable(false, 0));
        assert!(!mergeable(true, 0), "a card");
        assert!(!mergeable(false, 24), "a turned terminal");

        let mut instances = layout();
        let (parts, starved) = merge(&mut instances, &merging, 4, &mut meshes);
        assert!(
            parts.is_empty(),
            "with room for four of the five, none merges"
        );
        assert_eq!(starved, 5, "and five would, with room for them");
        assert!(instances.iter().all(|i| !i.merged));

        let mut many: Vec<ShapeInstance> = (0..=MERGE_MAX_TERMINALS)
            .map(|k| instance(unit.clone(), k as f32, Some("Stone")))
            .collect();
        let merging = vec![true; many.len()];
        let (parts, starved) = merge(&mut many, &merging, usize::MAX, &mut meshes);
        assert!(parts.is_empty(), "a node past the cap merges nothing");
        assert_eq!(starved, 0, "however much room it is given");
        assert!(many.iter().all(|i| !i.merged));
    }
}
