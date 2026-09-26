//! One generator's parts as the world draws them, and which of them touch.
//!
//! Every primitive is meshed through the real mesher and placed by its
//! tree's composed transforms, as the agent's z-fighting check places them
//! (`agent::daemon::edit::zfight`, whose walk this copies): taper, bend,
//! bulge and cut are in the mesh, so a limb set for a trunk's untapered
//! radius stands off the trunk here exactly as it does in the world. The
//! frame is the generator's own - its anchor at the origin, the ground it
//! is snapped to at y = 0.
//!
//! Two parts touch when their surfaces come within [`CONTACT_M`] of each
//! other, or when one lies wholly inside the other's closed solid (a knot
//! buried in a trunk has no surface near the trunk's).

use std::collections::HashMap;

use bevy::math::Affine3A;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;

use super::geometry::{
    Tri, box_distance, boxes_distance, boxes_meet, closest_on_tri, ray_hit, tris_within,
};
use crate::catalogue::items::measure::{is_primitive, transform_of};
use crate::pds::{Generator, GeneratorKind};
use crate::world_builder::build_primitive_mesh;

/// Two surfaces this close touch (m): a few centimetres, about what a
/// coarse mesh's facets stand off the round surface they approximate.
pub(super) const CONTACT_M: f32 = 0.03;

/// The direction a point-in-solid ray is cast: along no axis and in no
/// plane an authored face is likely to lie in, so it rarely grazes an edge
/// (the z-fighting check's).
const RAY: Vec3 = Vec3::new(0.296_8, 0.881_3, 0.367_7);

/// One primitive as the world draws it, in its generator's frame.
pub(super) struct Part {
    /// Its path of child indices from the generator's root.
    pub(super) path: Vec<usize>,
    /// Its kind, as the record tags it (`Sphere`, `Spine`, ...).
    pub(super) kind: &'static str,
    pub(super) tris: Vec<Tri>,
    /// Its corners, each once.
    pub(super) points: Vec<Vec3>,
    pub(super) min: Vec3,
    pub(super) max: Vec3,
    /// Whether its surface closes round a solid - every edge shared by an
    /// even number of triangles - so a point can be inside it.
    pub(super) closed: bool,
}

impl Part {
    /// Whether `point` is inside this part's closed solid: a ray from it
    /// crosses the surface an odd number of times.
    pub(super) fn contains(&self, point: Vec3) -> bool {
        self.closed
            && boxes_meet(point, point, self.min, self.max)
            && self
                .tris
                .iter()
                .filter(|t| ray_hit(point, RAY, t).is_some())
                .count()
                % 2
                == 1
    }

    /// Where the part is sampled against the ground: its corners and the
    /// middle of each face, so a broad face over a bump is read too.
    pub(super) fn samples(&self) -> impl Iterator<Item = Vec3> + '_ {
        self.points
            .iter()
            .copied()
            .chain(self.tris.iter().map(Tri::centre))
    }

    /// How far down from `origin` along `down` this part's surface is, if
    /// it is below it at all.
    pub(super) fn below(&self, origin: Vec3, down: Vec3) -> Option<f32> {
        self.tris
            .iter()
            .filter_map(|t| ray_hit(origin, down, t))
            .min_by(f32::total_cmp)
    }
}

/// A generator's parts and what each touches.
pub(super) struct Body {
    pub(super) parts: Vec<Part>,
    /// Each part's neighbours: the parts it touches, by index.
    pub(super) touches: Vec<Vec<usize>>,
    /// Nodes that draw something this does not mesh - an L-system, a
    /// shape, a sign - which a part could rest on unseen here.
    pub(super) unmeshed: usize,
}

impl Body {
    pub(super) fn of(root: &Generator) -> Self {
        let mut parts = Vec::new();
        let mut unmeshed = 0;
        collect(
            root,
            transform_of(&root.transform).compute_affine(),
            &mut Vec::new(),
            &mut parts,
            &mut unmeshed,
        );
        let mut touches = vec![Vec::new(); parts.len()];
        for i in 0..parts.len() {
            for j in i + 1..parts.len() {
                if touch(&parts[i], &parts[j], CONTACT_M) {
                    touches[i].push(j);
                    touches[j].push(i);
                }
            }
        }
        Self {
            parts,
            touches,
            unmeshed,
        }
    }

    /// Each part's component: parts that touch, directly or through
    /// others, share a number.
    pub(super) fn components(&self) -> Vec<usize> {
        let mut component = vec![usize::MAX; self.parts.len()];
        for start in 0..self.parts.len() {
            if component[start] != usize::MAX {
                continue;
            }
            component[start] = start;
            let mut stack = vec![start];
            while let Some(i) = stack.pop() {
                for &j in &self.touches[i] {
                    if component[j] == usize::MAX {
                        component[j] = start;
                        stack.push(j);
                    }
                }
            }
        }
        component
    }
}

/// Whether two parts touch: surfaces within `tol`, or one wholly inside
/// the other's closed solid.
pub(super) fn touch(a: &Part, b: &Part, tol: f32) -> bool {
    let slack = Vec3::splat(tol);
    if !boxes_meet(a.min - slack, a.max + slack, b.min, b.max) {
        return false;
    }
    let (lo, hi) = (a.min.max(b.min) - slack, a.max.min(b.max) + slack);
    let near = |t: &&Tri| boxes_meet(t.min, t.max, lo, hi);
    let b_near: Vec<&Tri> = b.tris.iter().filter(near).collect();
    for ta in a.tris.iter().filter(near) {
        for tb in &b_near {
            if boxes_meet(ta.min - slack, ta.max + slack, tb.min, tb.max)
                && tris_within(ta, tb, tol)
            {
                return true;
            }
        }
    }
    // No surface near: either wholly inside or wholly outside, which the
    // box alone settles for most pairs.
    wholly_inside(a, b, tol) || wholly_inside(b, a, tol)
}

/// Whether `inner`, whose surface does not come near `outer`'s, lies inside
/// `outer`'s closed solid: two of three corners, taken apart, say so.
fn wholly_inside(inner: &Part, outer: &Part, tol: f32) -> bool {
    let slack = Vec3::splat(tol);
    if !outer.closed
        || inner.points.is_empty()
        || !(inner.min.cmpge(outer.min - slack).all() && inner.max.cmple(outer.max + slack).all())
    {
        return false;
    }
    let n = inner.points.len();
    [0, n / 2, n - 1]
        .iter()
        .filter(|&&k| outer.contains(inner.points[k]))
        .count()
        >= 2
}

/// The distance between two parts' surfaces, or `best` if it is not less:
/// each one's corners against the other's faces.
pub(super) fn distance(a: &Part, b: &Part, best: f32) -> f32 {
    if boxes_distance(a.min, a.max, b.min, b.max) >= best {
        return best;
    }
    let mut best = best;
    for (from, to) in [(a, b), (b, a)] {
        for &p in &from.points {
            if box_distance(p, to.min, to.max) >= best {
                continue;
            }
            for t in &to.tris {
                if box_distance(p, t.min, t.max) < best {
                    best = best.min((p - closest_on_tri(p, t)).length());
                }
            }
        }
    }
    best
}

/// Every primitive under `node`, placed as the world places it: each child
/// by its parent's transform times its own, as the spawner parents them.
fn collect(
    node: &Generator,
    world: Affine3A,
    path: &mut Vec<usize>,
    out: &mut Vec<Part>,
    unmeshed: &mut usize,
) {
    if is_primitive(&node.kind) {
        if let Some(part) = part(node, world, path) {
            out.push(part);
        }
    } else if matches!(
        node.kind,
        GeneratorKind::LSystem { .. } | GeneratorKind::Shape { .. } | GeneratorKind::Sign { .. }
    ) {
        *unmeshed += 1;
    }
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        let child_world = world * transform_of(&child.transform).compute_affine();
        collect(child, child_world, path, out, unmeshed);
        path.pop();
    }
}

/// `node`'s primitive meshed by the real mesher and placed by `world`.
fn part(node: &Generator, world: Affine3A, path: &[usize]) -> Option<Part> {
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
    // Corners that meet at one place are one corner: a mesher splits them
    // along a seam for its normals and UVs.
    let key = |p: Vec3| (p * 1.0e4).round().as_ivec3();
    let mut welded: HashMap<IVec3, usize> = HashMap::new();
    let mut points = Vec::new();
    let mut tris = Vec::new();
    let mut edges: HashMap<(usize, usize), u32> = HashMap::new();
    for corners in order.as_chunks::<3>().0 {
        let Some(v) = corners
            .iter()
            .map(|&c| at.get(c).copied())
            .collect::<Option<Vec<Vec3>>>()
        else {
            continue;
        };
        let ids = v.iter().map(|&p| {
            *welded.entry(key(p)).or_insert_with(|| {
                points.push(p);
                points.len() - 1
            })
        });
        let ids: Vec<usize> = ids.collect();
        if ids[0] == ids[1] || ids[1] == ids[2] || ids[0] == ids[2] {
            continue;
        }
        for k in 0..3 {
            let (a, b) = (ids[k], ids[(k + 1) % 3]);
            *edges.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
        tris.push(Tri::new([v[0], v[1], v[2]]));
    }
    if tris.is_empty() {
        return None;
    }
    let min = points.iter().fold(Vec3::INFINITY, |m, p| m.min(*p));
    let max = points.iter().fold(Vec3::NEG_INFINITY, |m, p| m.max(*p));
    Some(Part {
        path: path.to_vec(),
        kind: node.kind.kind_tag(),
        closed: edges.values().all(|n| n % 2 == 0),
        tris,
        points,
        min,
        max,
    })
}

#[cfg(test)]
pub(super) mod tests {
    use serde_json::{Value, json};

    use super::*;

    /// A primitive in wire form: `$type`'s last word, its own fields, and
    /// where it stands (m).
    pub(in crate::render_tool::floating) fn prim(
        kind: &str,
        fields: Value,
        at: [f32; 3],
        children: Vec<Value>,
    ) -> Value {
        let mut wire = fields;
        wire["$type"] = json!(format!("network.symbios.gen.{kind}"));
        wire["material"] = json!({});
        wire["transform"] = json!({ "translation": at.map(fp) });
        wire["children"] = json!(children);
        wire
    }

    /// Metres as the wire's fixed point.
    pub(in crate::render_tool::floating) fn fp(m: f32) -> i64 {
        (m * 10_000.0).round() as i64
    }

    pub(in crate::render_tool::floating) fn cuboid(
        size: [f32; 3],
        at: [f32; 3],
        children: Vec<Value>,
    ) -> Value {
        prim(
            "cuboid",
            json!({ "size": size.map(fp), "solid": true }),
            at,
            children,
        )
    }

    pub(in crate::render_tool::floating) fn sphere(radius: f32, at: [f32; 3]) -> Value {
        prim(
            "sphere",
            json!({ "radius": fp(radius), "resolution": 3, "solid": true }),
            at,
            vec![],
        )
    }

    pub(in crate::render_tool::floating) fn generator(wire: Value) -> Generator {
        serde_json::from_value(wire).expect("a generator")
    }

    fn body(wire: Value) -> Body {
        Body::of(&generator(wire))
    }

    /// A box, and beside it a box a centimetre off, then ten.
    #[test]
    fn parts_touch_within_a_few_centimetres() {
        let pair = |gap: f32| {
            body(cuboid(
                [1.0, 1.0, 1.0],
                [0.0, 0.5, 0.0],
                vec![cuboid([1.0, 1.0, 1.0], [1.0 + gap, 0.0, 0.0], vec![])],
            ))
        };
        assert_eq!(pair(0.01).touches[0], vec![1]);
        assert!(pair(0.10).touches[0].is_empty());
    }

    /// A small box wholly inside a large one has no surface near it, and
    /// still touches it.
    #[test]
    fn a_part_wholly_inside_another_touches_it() {
        let b = body(cuboid(
            [2.0, 2.0, 2.0],
            [0.0, 1.0, 0.0],
            vec![cuboid([0.2, 0.2, 0.2], [0.1, 0.2, 0.0], vec![])],
        ));
        assert!(b.parts.iter().all(|p| p.closed));
        assert_eq!(b.touches[1], vec![0]);
    }

    /// Meshed by the real mesher: every roster primitive at its default
    /// meshes, and the round closed ones close.
    #[test]
    fn the_real_meshes_close_where_they_are_solid() {
        for wire in [
            cuboid([1.0, 1.0, 1.0], [0.0; 3], vec![]),
            sphere(0.5, [0.0; 3]),
        ] {
            let b = body(wire);
            assert_eq!(b.parts.len(), 1);
            assert!(b.parts[0].closed, "{}", b.parts[0].kind);
        }
    }

    /// The distance between two parts is between their surfaces.
    #[test]
    fn the_distance_between_parts_is_between_their_surfaces() {
        let b = body(cuboid(
            [1.0, 1.0, 1.0],
            [0.0, 0.5, 0.0],
            vec![cuboid([1.0, 1.0, 1.0], [1.4, 0.0, 0.0], vec![])],
        ));
        let d = distance(&b.parts[0], &b.parts[1], f32::INFINITY);
        assert!((d - 0.4).abs() < 1e-4, "{d}");
    }
}
