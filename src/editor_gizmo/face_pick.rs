//! Click-to-pick face selection (#961): the bridge between a viewport
//! click and the Faces panel's per-face override editor (#960).
//!
//! Naming a face from a dropdown means reading "Side −X" and rotating the
//! camera in your head. Picking it means clicking the face you can see. The
//! panel arms [`FacePick`]; the next scene click that lands on a primitive
//! resolves *which face* it hit, selects the owning node in the tree, and
//! hands the face to the panel, which focuses its override (creating it
//! first, if new).
//!
//! **Why a mesh raycast.** The hit must name a *triangle*, and only
//! [`bevy::picking::mesh_picking::ray_cast::MeshRayCast`]
//! reports `triangle_index`. Avian's rays hit each prim's convex hull —
//! which is not the visible surface at all once a cut is active — so the
//! physics query cannot answer this question even in principle.
//!
//! **Why the triangles are copied, not referenced.** The highlight captures
//! world-space triangle positions at pick time instead of holding the hit
//! entity. Creating an override marks the record dirty, and the rebuild
//! that follows despawns and respawns the prim — an entity-keyed highlight
//! would blink out just as the user looks at it.

use bevy::mesh::{Mesh, VertexAttributeValues};
use bevy::prelude::*;

use crate::config::ui::face_pick as cfg;
use crate::pds::generator::FaceKey;
use crate::world_builder::FaceTable;

/// Shared state between the Faces panel (egui) and the scene click handler
/// (`super::pick_on_scene_click`).
///
/// One-shot by design: arming survives exactly until the next click that
/// resolves a face. A sticky picking mode would keep re-interpreting later
/// viewport clicks long after the user had moved on.
#[derive(Resource, Default)]
pub struct FacePick {
    /// Set by the panel's "Pick from scene" button; cleared by the click
    /// that resolves a face (a click that hits nothing keeps it armed, so a
    /// near-miss costs one more click rather than a silent cancel).
    pub armed: bool,
    /// What the last click resolved, awaiting the panel draw that consumes
    /// it via [`take_for`](Self::take_for).
    picked: Option<Picked>,
    /// Brief in-scene confirmation of the picked face.
    highlight: Option<Highlight>,
}

/// A resolved pick, addressed to one node of one tree.
struct Picked {
    /// Generator name (room) or [`AvatarVisualsTreeSource::ROOT_NAME`]
    /// (avatar) — the `root` half of the panel's node id.
    ///
    /// [`AvatarVisualsTreeSource::ROOT_NAME`]:
    ///     crate::ui::room::generators::AvatarVisualsTreeSource::ROOT_NAME
    root: String,
    /// Child-index chain from that root.
    path: Vec<usize>,
    face: FaceKey,
}

/// World-space wireframe of the picked face, with the moment it stops
/// drawing. Positions rather than an entity: see the module docs.
struct Highlight {
    triangles: Vec<[Vec3; 3]>,
    /// `Time::elapsed_secs_f64` at which the highlight expires.
    expires_at: f64,
    /// Start of the fade window, so alpha can ramp instead of blinking off.
    shown_at: f64,
}

impl FacePick {
    /// Record what a scene click resolved and disarm.
    pub(super) fn record(
        &mut self,
        root: String,
        path: Vec<usize>,
        face: FaceKey,
        triangles: Vec<[Vec3; 3]>,
        now: f64,
    ) {
        self.armed = false;
        self.picked = Some(Picked { root, path, face });
        self.highlight = Some(Highlight {
            triangles,
            expires_at: now + cfg::HIGHLIGHT_SECS,
            shown_at: now,
        });
    }

    /// The face picked *for this node*, consumed so the panel acts on it
    /// exactly once.
    ///
    /// Addressed rather than global: a pick moves the tree selection to the
    /// prim that was clicked, so by the time the panel draws it is drawing
    /// that node — but only a match may consume the pick, or a stale one
    /// (world editor closed between click and draw) would paint whichever
    /// node happened to be selected later.
    pub fn take_for(&mut self, root: &str, path: &[usize]) -> Option<FaceKey> {
        let p = self.picked.as_ref()?;
        if p.root != root || p.path != path {
            return None;
        }
        self.picked.take().map(|p| p.face)
    }
}

/// The face a ray hit belongs to: the hit entity's group table, indexed by
/// the hit triangle.
///
/// `None` when the raycast reported no triangle index (an untriangulated
/// hit) or the index is past the table — a mesh and a table that disagree
/// mean the hit entity's mesh was swapped without its
/// [`PrimFaceGroup`](crate::world_builder::PrimFaceGroup), so refusing to
/// answer beats naming an arbitrary face.
pub(super) fn face_at(faces: &FaceTable, triangle_index: Option<usize>) -> Option<FaceKey> {
    let index = u32::try_from(triangle_index?).ok()?;
    faces.face_of(index)
}

/// World-space triangles of one face, for the pick highlight.
///
/// Capped at [`cfg::MAX_HIGHLIGHT_TRIANGLES`]: a `Surface` face on a
/// subdivided sphere is thousands of triangles, and the confirmation is
/// worth a bounded number of lines, not all of them.
pub(super) fn face_triangles(
    mesh: &Mesh,
    faces: &FaceTable,
    face: FaceKey,
    to_world: &GlobalTransform,
) -> Vec<[Vec3; 3]> {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return Vec::new();
    };
    // Every primitive mesh is indexed; an unindexed one would have to be
    // read as consecutive vertex triples, which no mesher here produces.
    let Some(indices) = mesh.indices() else {
        return Vec::new();
    };
    let indices: Vec<usize> = indices.iter().collect();
    let mut out = Vec::new();
    for (key, start, end) in faces.spans() {
        if key != face {
            continue;
        }
        for tri in start as usize..end as usize {
            if out.len() >= cfg::MAX_HIGHLIGHT_TRIANGLES {
                return out;
            }
            let Some(corners) = indices.get(tri * 3..tri * 3 + 3) else {
                continue;
            };
            let mut world = [Vec3::ZERO; 3];
            let mut complete = true;
            for (slot, &vertex) in world.iter_mut().zip(corners) {
                match positions.get(vertex) {
                    Some(p) => *slot = to_world.transform_point(Vec3::from_array(*p)),
                    None => complete = false,
                }
            }
            if complete {
                out.push(lift(world));
            }
        }
    }
    out
}

/// Lift a triangle off the surface it belongs to, along its own normal, so
/// the depth-tested outline doesn't z-fight with the face it outlines. A
/// degenerate triangle (the zero-width quads the swept meshers emit at
/// profile corners) has no normal to lift along and is left where it is —
/// it draws no visible line anyway.
fn lift([a, b, c]: [Vec3; 3]) -> [Vec3; 3] {
    let normal = (b - a).cross(c - a);
    let Some(normal) = normal.try_normalize() else {
        return [a, b, c];
    };
    let d = normal * cfg::HIGHLIGHT_LIFT_M;
    [a + d, b + d, c + d]
}

/// Draw the picked face's wireframe until it expires, fading as it goes.
///
/// Runs in `PostUpdate` with the other gizmo drawing so it sees this
/// frame's transforms — though the triangles are already in world space,
/// so it is really just sharing the schedule slot.
///
/// The expiry doubles as the pick's own deadline: an unconsumed pick dies
/// with its highlight. The panel consumes on its very next draw, so the only
/// way one survives that long is that nobody was there to take it (the
/// editor closed between the click and the draw) — and a pick that outlived
/// its window would paint a face the user has stopped thinking about.
pub(super) fn draw_face_pick_highlight(
    mut gizmos: Gizmos,
    mut pick: ResMut<FacePick>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    let Some(highlight) = pick.highlight.as_ref() else {
        return;
    };
    if now >= highlight.expires_at {
        pick.highlight = None;
        pick.picked = None;
        return;
    }
    let life = (highlight.expires_at - highlight.shown_at).max(f64::EPSILON);
    let remaining = ((highlight.expires_at - now) / life).clamp(0.0, 1.0) as f32;
    let [r, g, b, a] = cfg::HIGHLIGHT_COLOR;
    let color = Color::srgba(r, g, b, a * remaining);
    for [p0, p1, p2] in &highlight.triangles {
        gizmos.linestrip([*p0, *p1, *p2, *p0], color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::GeneratorKind;
    use crate::world_builder::build_primitive_mesh;

    /// The resolution the whole feature rests on: a hit triangle index,
    /// looked up in the hit entity's own group table, names a face.
    #[test]
    fn a_hit_triangle_resolves_to_its_face() {
        let mut faces = FaceTable::default();
        faces.push(FaceKey::Top, 2);
        faces.push(FaceKey::Wall, 3);
        assert_eq!(face_at(&faces, Some(0)), Some(FaceKey::Top));
        assert_eq!(face_at(&faces, Some(1)), Some(FaceKey::Top));
        assert_eq!(face_at(&faces, Some(2)), Some(FaceKey::Wall));
        assert_eq!(face_at(&faces, Some(4)), Some(FaceKey::Wall));
    }

    /// A hit the raycaster couldn't attribute to a triangle, and one past
    /// the table, both refuse rather than guess.
    #[test]
    fn an_unattributable_hit_names_no_face() {
        let faces = FaceTable::single(FaceKey::Surface, 4);
        assert_eq!(face_at(&faces, None), None);
        assert_eq!(face_at(&faces, Some(4)), None);
        assert_eq!(face_at(&faces, Some(9999)), None);
    }

    /// The highlight must outline the face that was picked and nothing
    /// else. A cuboid's `Top` is the one face whose every corner sits at
    /// the box's own +Y, which makes the claim checkable without trusting
    /// the table twice.
    #[test]
    fn the_highlight_outlines_only_the_picked_face() {
        let kind = GeneratorKind::default_primitive_for_tag("Cuboid").unwrap();
        let GeneratorKind::Cuboid { size, .. } = &kind else {
            panic!("default cuboid");
        };
        let top_y = size.0[1] * 0.5;
        let built = build_primitive_mesh(&kind);
        let tris = face_triangles(
            &built.mesh,
            &built.faces,
            FaceKey::Top,
            &GlobalTransform::IDENTITY,
        );
        assert!(!tris.is_empty(), "the Top face outlined nothing");
        // Lifted a hair along the face normal (+Y here) so the outline
        // doesn't z-fight with the surface it outlines.
        let drawn_y = top_y + cfg::HIGHLIGHT_LIFT_M;
        for corners in &tris {
            for c in corners {
                assert!(
                    (c.y - drawn_y).abs() < 1e-4,
                    "a highlighted corner at y={} is not on the +Y face (y={drawn_y})",
                    c.y
                );
            }
        }
    }

    /// The outline follows the entity it was picked from — the prim may sit
    /// anywhere in the room, and the highlight is drawn in world space.
    #[test]
    fn the_highlight_is_in_world_space() {
        let kind = GeneratorKind::default_primitive_for_tag("Cuboid").unwrap();
        let built = build_primitive_mesh(&kind);
        let offset = Vec3::new(10.0, -3.0, 2.0);
        let local = face_triangles(
            &built.mesh,
            &built.faces,
            FaceKey::Top,
            &GlobalTransform::IDENTITY,
        );
        let moved = face_triangles(
            &built.mesh,
            &built.faces,
            FaceKey::Top,
            &GlobalTransform::from_translation(offset),
        );
        assert_eq!(local.len(), moved.len());
        for (l, m) in local.iter().zip(&moved) {
            for (lc, mc) in l.iter().zip(m) {
                assert!((*mc - (*lc + offset)).length() < 1e-4);
            }
        }
    }

    /// A pick is addressed to one node: the panel drawing a different node
    /// must not consume it, and the node it *is* for consumes it once.
    #[test]
    fn a_pick_is_consumed_once_and_only_by_its_own_node() {
        let mut pick = FacePick {
            armed: true,
            ..Default::default()
        };
        pick.record(
            "house".to_string(),
            vec![1, 0],
            FaceKey::SidePx,
            Vec::new(),
            0.0,
        );
        assert!(!pick.armed, "resolving a pick disarms");
        assert_eq!(pick.take_for("house", &[0]), None, "wrong path");
        assert_eq!(pick.take_for("shed", &[1, 0]), None, "wrong root");
        assert_eq!(pick.take_for("house", &[1, 0]), Some(FaceKey::SidePx));
        assert_eq!(pick.take_for("house", &[1, 0]), None, "consumed once");
    }
}
