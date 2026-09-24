//! Faces drawn twice in one place (#1436): z-fighting in what `room set`
//! writes.
//!
//! Two faces that lie in one plane and face one way are drawn at one depth,
//! and the renderer settles which of them is in front pixel by pixel and
//! frame by frame - a flicker a person sees at once as they move, and a
//! still picture barely shows. The agent builds from pictures, so it is told
//! instead: the owner found the first one live, on the front of a garage the
//! agent had written as JSON, where a header overlapped the panels beside a
//! door in one plane.
//!
//! Every primitive of a generator is meshed through the real mesher and
//! placed by its tree's composed transforms - the geometry the world draws,
//! taper, cut and hollow included - and every pair of primitives whose
//! triangles share a plane and a facing direction, with area in common, is
//! named. Three kinds of shared plane are left out, because nobody can see
//! them: faces that face each other, which are pressed between their two
//! solids; a shared patch buried inside a third primitive, as where a
//! lattice's braces cross inside its leg; and faces meeting wholly below
//! the generator's anchor, which stands on the ground. What is left is what
//! flickers.

use std::collections::HashMap;

use bevy::math::Affine3A;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use serde_json::{Value, json};

use crate::catalogue::items::measure::{is_primitive, transform_of};
use crate::pds::Generator;
use crate::world_builder::build_primitive_mesh;

/// Faces this close to one plane are drawn at one depth (m).
const PLANE_TOLERANCE_M: f32 = 1.0e-3;
/// Normals closer than this, as a cosine, face one way.
const SAME_WAY_COS: f32 = 0.9999;
/// A shared patch smaller than this is not named (m²): 10 cm², about a
/// 3 cm square.
const MIN_AREA_M2: f32 = 1.0e-3;
/// How far in front of a shared patch its visibility is probed (m).
const PROBE_M: f32 = 2.0e-3;
/// The direction a point-in-solid ray is cast: along no axis and in no
/// plane an authored face is likely to lie in, so it rarely grazes an edge.
const RAY: Vec3 = Vec3::new(0.296_8, 0.881_3, 0.367_7);

/// Two primitives of one generator drawing faces in one place.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Overlap {
    /// Each primitive's path of child indices from the generator's root.
    pub(super) a: Vec<usize>,
    pub(super) b: Vec<usize>,
    /// The area they draw in one place where it can be seen (m²).
    pub(super) area_m2: f32,
}

/// One triangle as the world draws it, facing out of its solid.
struct Tri {
    v: [Vec3; 3],
    normal: Vec3,
    min: Vec3,
    max: Vec3,
}

/// One primitive as the world draws it.
struct Piece {
    path: Vec<usize>,
    tris: Vec<Tri>,
    min: Vec3,
    max: Vec3,
}

impl Piece {
    /// Whether `point` is inside this primitive's solid: a ray from it
    /// crosses the surface an odd number of times.
    fn contains(&self, point: Vec3) -> bool {
        boxes_meet(point, point, self.min, self.max)
            && self
                .tris
                .iter()
                .filter(|tri| ray_crosses(point, RAY, tri))
                .count()
                % 2
                == 1
    }
}

/// At most this many pairs are named in one answer, the most area first.
const MAX_NAMED: usize = 16;

/// What `room set` answers about z-fighting: every generator the set left
/// different from what it was `before`, checked, and the pairs found named
/// by JSON pointer into the record - the most area first, at most
/// [`MAX_NAMED`] - with how many there were in all.
pub(super) fn report(
    before: &HashMap<String, Generator>,
    after: &HashMap<String, Generator>,
) -> (Vec<Value>, usize) {
    let mut changed: Vec<(&String, &Generator)> = after
        .iter()
        .filter(|(name, generator)| before.get(*name) != Some(*generator))
        .collect();
    changed.sort_by(|a, b| a.0.cmp(b.0));
    let mut found: Vec<(f32, Value)> =
        changed
            .into_iter()
            .flat_map(|(name, generator)| {
                let base = format!("/generators/{}", name.replace('~', "~0").replace('/', "~1"));
                coplanar_overlaps(generator).into_iter().map(move |overlap| {
                let pointer = |path: &[usize]| {
                    path.iter().fold(base.clone(), |p, i| format!("{p}/children/{i}"))
                };
                (
                    overlap.area_m2,
                    json!({
                        "a": pointer(&overlap.a),
                        "b": pointer(&overlap.b),
                        "area_m2": (f64::from(overlap.area_m2) * 10_000.0).round() / 10_000.0,
                    }),
                )
            })
            })
            .collect();
    let total = found.len();
    found.sort_by(|x, y| y.0.total_cmp(&x.0));
    (
        found.into_iter().take(MAX_NAMED).map(|(_, v)| v).collect(),
        total,
    )
}

/// The pairs of primitives in `root`'s tree that draw faces in one place
/// where they can be seen, the most area first.
pub(super) fn coplanar_overlaps(root: &Generator) -> Vec<Overlap> {
    let mut pieces = Vec::new();
    collect(
        root,
        transform_of(&root.transform).compute_affine(),
        &mut Vec::new(),
        &mut pieces,
    );
    let mut found = Vec::new();
    for i in 0..pieces.len() {
        for j in i + 1..pieces.len() {
            let (a, b) = (&pieces[i], &pieces[j]);
            if !boxes_meet(a.min, a.max, b.min, b.max) {
                continue;
            }
            let area = visible_shared_area(a, b, &pieces, [i, j]);
            if area >= MIN_AREA_M2 {
                found.push(Overlap {
                    a: a.path.clone(),
                    b: b.path.clone(),
                    area_m2: area,
                });
            }
        }
    }
    found.sort_by(|x, y| y.area_m2.total_cmp(&x.area_m2));
    found
}

/// Every primitive under `node`, placed as the world places it: each child
/// by its parent's transform times its own, as the spawner parents them.
fn collect(node: &Generator, world: Affine3A, path: &mut Vec<usize>, out: &mut Vec<Piece>) {
    if is_primitive(&node.kind)
        && let Some(piece) = piece(node, world, path)
    {
        out.push(piece);
    }
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        let child_world = world * transform_of(&child.transform).compute_affine();
        collect(child, child_world, path, out);
        path.pop();
    }
}

/// `node`'s primitive meshed by the real mesher and placed by `world`.
fn piece(node: &Generator, world: Affine3A, path: &[usize]) -> Option<Piece> {
    let mesh = build_primitive_mesh(&node.kind).mesh;
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return None;
    };
    let at: Vec<Vec3> = positions
        .iter()
        .map(|p| world.transform_point3(Vec3::from_array(*p)))
        .collect();
    let order: Vec<usize> = match mesh.indices() {
        Some(indices) => indices.iter().collect(),
        None => (0..at.len()).collect(),
    };
    // A mirroring transform turns every triangle's winding: turn it back, so
    // each winds counter-clockwise round the normal pointing out of its
    // solid - the normal and the clipping in `shared_patch` both rest on it.
    let mirrored = world.matrix3.determinant() < 0.0;
    let tris: Vec<Tri> = order
        .chunks_exact(3)
        .filter_map(|corners| {
            let mut v = [
                at.get(corners[0])?,
                at.get(corners[1])?,
                at.get(corners[2])?,
            ]
            .map(|p| *p);
            if mirrored {
                v.swap(1, 2);
            }
            let cross = (v[1] - v[0]).cross(v[2] - v[0]);
            let twice_area = cross.length();
            (twice_area > f32::EPSILON).then(|| Tri {
                v,
                normal: cross / twice_area,
                min: v[0].min(v[1]).min(v[2]),
                max: v[0].max(v[1]).max(v[2]),
            })
        })
        .collect();
    let min = tris.iter().fold(Vec3::INFINITY, |m, tri| m.min(tri.min));
    let max = tris
        .iter()
        .fold(Vec3::NEG_INFINITY, |m, tri| m.max(tri.max));
    (!tris.is_empty()).then(|| Piece {
        path: path.to_vec(),
        tris,
        min,
        max,
    })
}

/// The area `a` and `b` draw in one plane facing one way, less what lies
/// buried inside any other of `pieces` (`skip` names `a` and `b`).
fn visible_shared_area(a: &Piece, b: &Piece, pieces: &[Piece], skip: [usize; 2]) -> f32 {
    let slack = Vec3::splat(PLANE_TOLERANCE_M);
    let (lo, hi) = (a.min.max(b.min) - slack, a.max.min(b.max) + slack);
    let near = |tri: &&Tri| boxes_meet(tri.min, tri.max, lo, hi);
    let b_near: Vec<&Tri> = b.tris.iter().filter(near).collect();
    let mut area = 0.0;
    for ta in a.tris.iter().filter(near) {
        for tb in &b_near {
            if ta.normal.dot(tb.normal) < SAME_WAY_COS
                || !boxes_meet(ta.min - slack, ta.max + slack, tb.min, tb.max)
                || tb
                    .v
                    .iter()
                    .any(|p| ta.normal.dot(*p - ta.v[0]).abs() > PLANE_TOLERANCE_M)
            {
                continue;
            }
            let Some((patch, centre)) = shared_patch(ta, tb) else {
                continue;
            };
            // A generator stands on the ground at its anchor, its y = 0, so
            // two faces meeting wholly below that - a buried footing's
            // underside - are in the ground where nobody sees them.
            if ta.v.iter().chain(&tb.v).all(|p| p.y < 0.0) {
                continue;
            }
            let probe = centre + ta.normal * PROBE_M;
            let buried = pieces
                .iter()
                .enumerate()
                .any(|(k, other)| !skip.contains(&k) && other.contains(probe));
            if !buried {
                area += patch;
            }
        }
    }
    area
}

/// The area two coplanar triangles facing one way have in common, and its
/// centre: `ta` clipped to `tb` in `ta`'s plane. Both wind the same way
/// round their shared normal, so each is counter-clockwise in a frame
/// whose second axis is the normal crossed with the first.
fn shared_patch(ta: &Tri, tb: &Tri) -> Option<(f32, Vec3)> {
    let u = (ta.v[1] - ta.v[0]).normalize();
    let w = ta.normal.cross(u);
    let flat = |p: Vec3| {
        let d = p - ta.v[0];
        Vec2::new(d.dot(u), d.dot(w))
    };
    let mut patch: Vec<Vec2> = ta.v.iter().map(|p| flat(*p)).collect();
    let edge = [flat(tb.v[0]), flat(tb.v[1]), flat(tb.v[2])];
    for k in 0..3 {
        patch = keep_left_of(&patch, edge[k], edge[(k + 1) % 3]);
        if patch.len() < 3 {
            return None;
        }
    }
    let (area, centre) = area_and_centre(&patch);
    (area > 0.0).then(|| (area, ta.v[0] + u * centre.x + w * centre.y))
}

/// The part of the convex polygon `poly` on the left of the line from `a`
/// to `b` (Sutherland-Hodgman, one edge).
fn keep_left_of(poly: &[Vec2], a: Vec2, b: Vec2) -> Vec<Vec2> {
    let side = |p: Vec2| (b - a).perp_dot(p - a);
    let mut kept = Vec::with_capacity(poly.len() + 1);
    for (i, &p) in poly.iter().enumerate() {
        let q = poly[(i + 1) % poly.len()];
        let (sp, sq) = (side(p), side(q));
        if sp >= 0.0 {
            kept.push(p);
        }
        if (sp >= 0.0) != (sq >= 0.0) {
            kept.push(p + (q - p) * (sp / (sp - sq)));
        }
    }
    kept
}

/// A polygon's area and centroid (the shoelace formula).
fn area_and_centre(poly: &[Vec2]) -> (f32, Vec2) {
    let mut twice = 0.0;
    let mut weighted = Vec2::ZERO;
    for (i, &p) in poly.iter().enumerate() {
        let q = poly[(i + 1) % poly.len()];
        let cross = p.perp_dot(q);
        twice += cross;
        weighted += (p + q) * cross;
    }
    if twice.abs() <= f32::EPSILON {
        return (0.0, Vec2::ZERO);
    }
    (twice.abs() / 2.0, weighted / (3.0 * twice))
}

/// Whether the ray from `origin` along `dir` crosses `tri` (Moller-Trumbore).
fn ray_crosses(origin: Vec3, dir: Vec3, tri: &Tri) -> bool {
    let e1 = tri.v[1] - tri.v[0];
    let e2 = tri.v[2] - tri.v[0];
    let h = dir.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1.0e-12 {
        return false;
    }
    let s = origin - tri.v[0];
    let u = s.dot(h) / det;
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) / det;
    v >= 0.0 && u + v <= 1.0 && e2.dot(q) / det > 0.0
}

fn boxes_meet(a_min: Vec3, a_max: Vec3, b_min: Vec3, b_max: Vec3) -> bool {
    a_min.cmple(b_max).all() && b_min.cmple(a_max).all()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cuboid prim in wire form: `size` and `at` in metres.
    fn cuboid(size: [f32; 3], at: [f32; 3], children: Vec<Value>) -> Value {
        let wire = |v: [f32; 3]| v.map(|x| (x * 10_000.0).round() as i64);
        json!({
            "$type": "network.symbios.gen.cuboid",
            "size": wire(size),
            "solid": true,
            "material": {},
            "transform": { "translation": wire(at) },
            "children": children,
        })
    }

    fn generator(wire: Value) -> Generator {
        serde_json::from_value(wire).expect("a generator")
    }

    fn overlaps(wire: Value) -> Vec<Overlap> {
        coplanar_overlaps(&generator(wire))
    }

    /// The live case (#1436): a panel whose top overlaps a header in one
    /// plane - both the fronts and the backs are drawn twice, each over the
    /// strip they share.
    #[test]
    fn faces_sharing_a_plane_and_a_direction_are_named_with_their_area() {
        // A 2 m x 1 m header, and under it a 0.6 m panel whose top 0.5 m
        // runs up into the header's own plane (its sides clear of the
        // header's, which would share a plane of their own).
        let found = overlaps(cuboid(
            [2.0, 1.0, 0.1],
            [0.0, 3.0, 0.0],
            vec![cuboid([0.6, 2.0, 0.1], [0.5, -1.0, 0.0], vec![])],
        ));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            (found[0].a.as_slice(), found[0].b.as_slice()),
            (&[][..], &[0][..])
        );
        // Front and back, each 0.6 m x 0.5 m.
        assert!((found[0].area_m2 - 0.6).abs() < 1e-3, "{found:?}");
    }

    /// Two millimetres apart is two depths, not one - square to the axes,
    /// and turned 30 degrees, where each face's box spans the other's and
    /// only its distance from the plane tells them apart.
    #[test]
    fn faces_a_hair_apart_are_not_named() {
        for turned in [false, true] {
            let mut root = cuboid(
                [2.0, 1.0, 0.1],
                [0.0, 3.0, 0.0],
                vec![cuboid([0.6, 2.0, 0.1], [0.5, -1.0, 0.002], vec![])],
            );
            if turned {
                root["transform"]["rotation"] = json!([0, 2_588, 0, 9_659]);
            }
            let found = overlaps(root);
            assert!(found.is_empty(), "turned {turned}: {found:?}");
        }
    }

    /// Side by side, the tops share a plane and a direction but no area.
    #[test]
    fn faces_meeting_edge_to_edge_are_not_named() {
        let found = overlaps(cuboid(
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
            vec![cuboid([1.0, 1.0, 1.0], [1.0, 0.0, 0.0], vec![])],
        ));
        assert!(found.is_empty(), "{found:?}");
    }

    /// Stacked, one's top and the other's bottom share a plane but face
    /// each other: pressed between the two solids, never drawn to an eye.
    #[test]
    fn faces_facing_each_other_are_not_named() {
        let found = overlaps(cuboid(
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
            vec![cuboid([1.0, 1.0, 1.0], [0.0, 1.0, 0.0], vec![])],
        ));
        assert!(found.is_empty(), "{found:?}");
    }

    /// Two bars crossing at one height share their tops where they cross;
    /// a post enclosing the crossing buries it, as a lattice's leg does.
    #[test]
    fn a_patch_buried_inside_a_third_solid_is_not_named() {
        let bars = || {
            vec![
                cuboid([2.0, 0.1, 0.1], [0.0, 1.0, 0.0], vec![]),
                cuboid([0.1, 0.1, 2.0], [0.0, 1.0, 0.0], vec![]),
            ]
        };
        let open = overlaps(cuboid([0.2, 0.2, 0.2], [5.0, 0.0, 0.0], bars()));
        assert!(!open.is_empty(), "the crossing shows without the post");

        let buried = overlaps(cuboid([0.4, 3.0, 0.4], [0.0, 1.5, 0.0], {
            let mut children = bars();
            for child in &mut children {
                // Relative to the post's centre at 1.5 m.
                child["transform"]["translation"][1] = json!(-5_000);
            }
            children
        }));
        assert!(buried.is_empty(), "{buried:?}");
    }

    /// Two footings side by side, sharing their undersides a metre down,
    /// meet in the ground; raised to stand on it, they are seen.
    #[test]
    fn faces_meeting_below_the_ground_are_not_named() {
        let pair = |y: f32| {
            cuboid(
                [1.0, 1.0, 1.0],
                [0.0, y, 0.0],
                vec![cuboid([1.0, 1.0, 1.0], [0.5, 0.0, 0.2], vec![])],
            )
        };
        let buried = overlaps(pair(-1.0));
        assert!(buried.is_empty(), "{buried:?}");
        let standing = overlaps(pair(1.0));
        assert_eq!(standing.len(), 1, "{standing:?}");
    }

    /// A patch under 10 cm² is not named: a 2 cm cube's top in a box's top
    /// face is 4 cm², a 5 cm cube's is 25 cm².
    #[test]
    fn a_patch_smaller_than_ten_square_centimetres_is_not_named() {
        let cube_in_the_top = |edge: f32| {
            overlaps(cuboid(
                [1.0, 1.0, 1.0],
                [0.0, 0.5, 0.0],
                vec![cuboid([edge; 3], [0.0, 0.5 - edge / 2.0, 0.0], vec![])],
            ))
        };
        let small = cube_in_the_top(0.02);
        assert!(small.is_empty(), "{small:?}");
        let named = cube_in_the_top(0.05);
        assert_eq!(named.len(), 1, "{named:?}");
        assert!((named[0].area_m2 - 0.0025).abs() < 1e-5, "{named:?}");
    }

    /// A mirrored child's faces face out of it as it is drawn: mirrored in
    /// X under a box, its far end lands in the box's +X face, facing +X -
    /// the winding alone would point that face back into the child.
    #[test]
    fn a_mirrored_child_faces_the_way_it_is_drawn() {
        let mut child = cuboid([1.0, 1.0, 1.0], [0.5, 0.0, 0.0], vec![]);
        child["transform"]["scale"] = json!([-10_000, 10_000, 10_000]);
        let found = overlaps(cuboid([2.0, 2.0, 2.0], [0.0, 1.0, 0.0], vec![child]));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!((found[0].area_m2 - 1.0).abs() < 1e-3, "{found:?}");
    }

    /// The answer names the sixteen largest pairs by pointer - a generator's
    /// name escaped as a JSON pointer escapes it - and counts them all; a
    /// generator the set left as it was is not checked at all.
    #[test]
    fn the_report_names_the_largest_sixteen_and_counts_them_all() {
        // Seventeen small panels, each sharing its front and back with a
        // long slab and touching nothing else.
        let panels = (0..17)
            .map(|i| cuboid([0.4, 0.4, 0.1], [i as f32 - 8.0, 0.0, 0.0], vec![]))
            .collect();
        let slab = generator(cuboid([20.0, 1.0, 0.1], [0.0, 1.0, 0.0], panels));
        let after = HashMap::from([("a/b~c".to_owned(), slab)]);

        let (named, total) = report(&HashMap::new(), &after);

        assert_eq!((named.len(), total), (16, 17));
        assert_eq!(named[0]["a"], "/generators/a~1b~0c");
        assert!(
            named[0]["b"]
                .as_str()
                .is_some_and(|b| b.starts_with("/generators/a~1b~0c/children/")),
            "{}",
            named[0]
        );
        assert_eq!(report(&after, &after), (Vec::new(), 0), "nothing changed");
    }

    /// A child is placed by its parent's rotation: under a root turned a
    /// quarter about Y, a child authored against the root's +X end lands on
    /// its -Z end in the world, and the two share that face.
    #[test]
    fn a_child_is_placed_by_its_parents_rotation() {
        let mut root = cuboid(
            [4.0, 2.0, 2.0],
            [0.0, 0.0, 0.0],
            vec![cuboid([1.0, 1.0, 1.0], [1.5, 0.0, 0.0], vec![])],
        );
        root["transform"]["rotation"] = json!([0, 7071, 0, 7071]);
        let found = overlaps(root);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!((found[0].area_m2 - 1.0).abs() < 1e-3, "{found:?}");
    }

    /// What is drawn is the mesher's: a cylinder's cap in a box's top face
    /// fights over the cap's whole disc.
    #[test]
    fn a_rounded_primitive_is_read_from_its_mesh() {
        // The box spans 0..1 m up; the cylinder 0.4..1 m, its top cap in
        // the box's top face and the rest inside the box.
        let mut root = cuboid([2.0, 1.0, 2.0], [0.0, 0.5, 0.0], vec![]);
        root["children"] = json!([{
            "$type": "network.symbios.gen.cylinder",
            "radius": 4_000,
            "height": 6_000,
            "resolution": 32,
            "solid": true,
            "material": {},
            "transform": { "translation": [0, 2_000, 0] },
        }]);
        let found = overlaps(root);
        assert_eq!(found.len(), 1, "{found:?}");
        let disc = std::f32::consts::PI * 0.4 * 0.4;
        assert!(
            (found[0].area_m2 - disc).abs() < 0.02,
            "{} against a disc of {disc}",
            found[0].area_m2
        );
    }
}
