//! Where the world's solid shapes stand on the ground (#1674): the outline
//! each collider casts straight down, kept only where the shape reaches
//! down to the ground under it.
//!
//! A shape is read as the physics reads it - [`Collider::shape`], posed by
//! the entity's `GlobalTransform` - so what keeps grass off a spot is what
//! a player walking there would bump into. The pose is the transform's
//! whole affine, scale included, which is what avian applies to a collider
//! and what holds in an app that runs no physics (the render tool's
//! `--world`).
//!
//! * **A convex shape** (box, ball, cylinder, capsule, cone, hull) is
//!   outlined by its support points in [`OUTLINE_DIRECTIONS`] horizontal
//!   directions, mapped through the pose. A linear map carries a support
//!   point to a support point, so an upright box's outline is exact (a
//!   tilted one can lose a corner narrower than the directions' step) and a
//!   ball's is a polygon inscribed in its circle.
//! * **A compound** (a grammar building's walls and floors) is outlined
//!   piece by piece, so a high roof piece casts nothing while the walls
//!   under it do.
//! * **A triangle mesh** (a road deck, a building shell) is outlined
//!   triangle by triangle: a road's deck where it lies on the ground, a
//!   shell's floor. A shell's walls stand edge-on to the sky and outline
//!   nothing, and its roof stands too high to count.
//! * **A heightfield** is the ground itself, and **a half-space** has no
//!   outline; they cast nothing. Anything else is outlined by its box.
//!
//! [`OUTLINE_DIRECTIONS`]: crate::config::terrain::ground_cover::OUTLINE_DIRECTIONS
//! [`Collider::shape`]: avian3d::prelude::Collider::shape

use avian3d::parry::math::{Pose, Vector};
use avian3d::parry::shape::{SharedShape, TypedShape};
use bevy::math::{Affine3A, Vec2, Vec3};

use super::mask::convex_hull;
use crate::config::terrain::ground_cover as cfg;

/// The convex outlines `shape`, posed by `pose`, stands on: each handed to
/// `out` when the part of the shape it outlines reaches within
/// [`cfg::GROUNDED_REACH`] of the highest ground under it. Handed
/// over as found rather than collected: a Berlin square's road decks and
/// shells come to some seventy thousand.
pub(super) fn outlines(
    shape: &SharedShape,
    pose: Affine3A,
    ground: &impl Fn(Vec2) -> f32,
    out: &mut impl FnMut(&[Vec2]),
) {
    match shape.as_typed_shape() {
        TypedShape::HeightField(_) | TypedShape::HalfSpace(_) => {}
        TypedShape::Compound(compound) => {
            for (part_pose, part) in compound.shapes() {
                outlines(part, pose * affine(part_pose), ground, out);
            }
        }
        TypedShape::TriMesh(mesh) => {
            let vertices = mesh.vertices();
            for [a, b, c] in mesh.indices() {
                let corners = [a, b, c].map(|&i| pose.transform_point3(vertices[i as usize]));
                push_grounded(&corners, ground, out);
            }
        }
        _ => match shape.as_support_map() {
            Some(support) => {
                // A support point of the posed shape in world direction d is
                // the pose of the shape's support point in L^T d, where L is
                // the pose's linear part (rotation and scale).
                let linear_t = pose.matrix3.transpose();
                let local = |world_dir: Vec3| {
                    let dir = (linear_t * world_dir).normalize_or_zero();
                    pose.transform_point3(support.local_support_point(dir))
                };
                let mut points: Vec<Vec3> = (0..cfg::OUTLINE_DIRECTIONS)
                    .map(|k| {
                        let a = std::f32::consts::TAU * k as f32 / cfg::OUTLINE_DIRECTIONS as f32;
                        local(Vec3::new(a.cos(), 0.0, a.sin()))
                    })
                    .collect();
                points.push(local(Vec3::NEG_Y));
                push_grounded(&points, ground, out);
            }
            None => {
                let aabb = shape.compute_local_aabb();
                let corners: Vec<Vec3> = (0..8)
                    .map(|k| {
                        let pick =
                            |bit: usize, lo: f32, hi: f32| if k & bit == 0 { lo } else { hi };
                        pose.transform_point3(Vec3::new(
                            pick(1, aabb.mins.x, aabb.maxs.x),
                            pick(2, aabb.mins.y, aabb.maxs.y),
                            pick(4, aabb.mins.z, aabb.maxs.z),
                        ))
                    })
                    .collect();
                push_grounded(&corners, ground, out);
            }
        },
    }
}

/// A parry pose as the affine it is.
fn affine(pose: &Pose) -> Affine3A {
    Affine3A::from_rotation_translation(pose.rotation, pose.translation)
}

/// Hand over the outline of `points` seen from above, when their lowest
/// reaches within the grounded reach of the highest ground under them. A
/// triangle is its own outline, and goes over as it is; anything more is
/// hulled.
fn push_grounded(points: &[Vector], ground: &impl Fn(Vec2) -> f32, out: &mut impl FnMut(&[Vec2])) {
    if points.is_empty() || points.iter().any(|p| !p.is_finite()) {
        return;
    }
    let flat = |p: &Vector| Vec2::new(p.x, p.z);
    // A triangle standing edge-on to the sky - a wall - has no outline, and
    // walls are half a shell's triangles: rejected before any ground is
    // read.
    let triangle = match points {
        [a, b, c] => {
            let t = [flat(a), flat(b), flat(c)];
            if (t[1] - t[0]).perp_dot(t[2] - t[0]).abs() <= f32::EPSILON {
                return;
            }
            Some(t)
        }
        _ => None,
    };
    let lowest = points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
    let middle = points.iter().map(flat).sum::<Vec2>() / points.len() as f32;
    // Grounded within reach of the highest ground under the outline: a
    // building's pad and a road's deck stand on the highest ground under
    // them, so on a slope a floor meets its uphill edge's ground and stands
    // clear of its middle's. Read point by point until one grounds it.
    let reaches = |xz: Vec2| lowest <= ground(xz) + cfg::GROUNDED_REACH;
    if !(reaches(middle) || points.iter().any(|p| reaches(flat(p)))) {
        return;
    }
    match triangle {
        Some(t) => out(&t),
        None => {
            let hull = convex_hull(points.iter().map(flat).collect());
            if hull.len() >= 3 {
                out(&hull);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use avian3d::prelude::Collider;
    use bevy::math::Quat;

    /// Flat ground at height 0.
    fn flat(_: Vec2) -> f32 {
        0.0
    }

    fn outline(collider: &Collider, pose: Affine3A) -> Vec<Vec<Vec2>> {
        let mut out = Vec::new();
        outlines(collider.shape(), pose, &flat, &mut |o: &[Vec2]| {
            out.push(o.to_vec())
        });
        out
    }

    fn area(p: &[Vec2]) -> f32 {
        0.5 * p
            .iter()
            .zip(p.iter().cycle().skip(1))
            .map(|(a, b)| a.perp_dot(*b))
            .sum::<f32>()
            .abs()
    }

    /// A box on the ground outlines its own footprint, turned as it is
    /// turned and scaled as it is scaled.
    #[test]
    fn a_box_outlines_its_footprint() {
        let cuboid = Collider::cuboid(4.0, 3.0, 2.0);
        let standing = outline(
            &cuboid,
            Affine3A::from_translation(Vec3::new(10.0, 1.5, 0.0)),
        );
        assert_eq!(standing.len(), 1);
        assert_eq!(standing[0].len(), 4, "{standing:?}");
        assert!((area(&standing[0]) - 8.0).abs() < 1e-3, "{standing:?}");
        let turned = outline(
            &cuboid,
            Affine3A::from_scale_rotation_translation(
                Vec3::splat(2.0),
                Quat::from_rotation_y(0.6),
                Vec3::new(0.0, 3.0, 0.0),
            ),
        );
        assert!((area(&turned[0]) - 32.0).abs() < 1e-2, "{turned:?}");
    }

    /// A ball resting on the ground outlines a polygon inside its circle.
    #[test]
    fn a_ball_outlines_its_circle() {
        let ball = outline(&Collider::sphere(1.0), Affine3A::from_translation(Vec3::Y));
        let a = area(&ball[0]);
        assert!(
            a < std::f32::consts::PI && a > 0.95 * std::f32::consts::PI,
            "{a}"
        );
    }

    /// A shape held clear of the ground casts nothing; one just above it,
    /// within the reach, still does.
    #[test]
    fn only_a_grounded_shape_casts() {
        let slab = Collider::cuboid(4.0, 0.2, 4.0);
        assert!(outline(&slab, Affine3A::from_translation(Vec3::Y * 3.0)).is_empty());
        assert_eq!(
            outline(&slab, Affine3A::from_translation(Vec3::Y * 0.8)).len(),
            1
        );
    }

    /// A compound is outlined piece by piece: its high piece casts
    /// nothing.
    #[test]
    fn a_compound_is_outlined_piece_by_piece() {
        let building = Collider::compound(vec![
            (
                Vec3::new(0.0, 1.5, 0.0),
                Quat::IDENTITY,
                Collider::cuboid(4.0, 3.0, 4.0),
            ),
            (
                Vec3::new(0.0, 6.0, 0.0),
                Quat::IDENTITY,
                Collider::cuboid(8.0, 0.5, 8.0),
            ),
        ]);
        let out = outline(&building, Affine3A::IDENTITY);
        assert_eq!(out.len(), 1, "{out:?}");
        assert!((area(&out[0]) - 16.0).abs() < 1e-3);
    }

    /// A triangle mesh lying on the ground outlines its triangles; one
    /// standing on edge outlines nothing.
    #[test]
    fn a_road_deck_outlines_its_triangles() {
        let deck = Collider::trimesh(
            vec![
                Vec3::new(0.0, 0.1, 0.0),
                Vec3::new(10.0, 0.1, 0.0),
                Vec3::new(10.0, 0.1, 4.0),
                Vec3::new(0.0, 0.1, 4.0),
            ],
            vec![[0, 1, 2], [0, 2, 3]],
        );
        let out = outline(&deck, Affine3A::IDENTITY);
        assert_eq!(out.len(), 2);
        assert!((out.iter().map(|t| area(t)).sum::<f32>() - 40.0).abs() < 1e-3);
        let wall = Collider::trimesh(
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(10.0, 0.0, 0.0),
                Vec3::new(10.0, 3.0, 0.0),
            ],
            vec![[0, 1, 2]],
        );
        assert!(outline(&wall, Affine3A::IDENTITY).is_empty());
    }

    /// On a slope a building stands on the highest ground under it (its
    /// pad does), its floor well clear of its middle's ground: it casts all
    /// the same, turned and scaled as it is.
    #[test]
    fn a_hillside_floor_casts() {
        // Ground rising 0.28 m a metre east, the steepest a building is
        // stood on; a floor 12 m across whose underside meets its uphill
        // edge's ground, 1.68 m over its middle's.
        let slope = |xz: Vec2| 0.28 * xz.x;
        let floor = Collider::cuboid(12.0, 0.4, 12.0);
        let mut out = Vec::new();
        outlines(
            floor.shape(),
            Affine3A::from_translation(Vec3::new(0.0, 1.68 + 0.2, 0.0)),
            &slope,
            &mut |o: &[Vec2]| out.push(o.to_vec()),
        );
        assert_eq!(out.len(), 1, "the floor casts");
        let mut turned = Vec::new();
        outlines(
            Collider::cuboid(8.0, 0.4, 8.0).shape(),
            Affine3A::from_scale_rotation_translation(
                Vec3::new(1.5, 1.0, 1.5),
                Quat::from_rotation_y(0.5),
                Vec3::new(0.0, 0.28 * 8.0 + 0.2, 0.0),
            ),
            &slope,
            &mut |o: &[Vec2]| turned.push(o.to_vec()),
        );
        assert_eq!(turned.len(), 1, "turned and scaled, it casts");
        assert!((area(&turned[0]) - 144.0).abs() < 0.1, "{turned:?}");
    }

    /// A building shell's floor casts, its roof does not, and its walls,
    /// edge-on to the sky, outline nothing.
    #[test]
    fn a_shell_casts_its_floor_alone() {
        let shell = Collider::trimesh(
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(4.0, 0.0, 0.0),
                Vec3::new(4.0, 0.0, 4.0),
                Vec3::new(0.0, 0.0, 4.0),
                Vec3::new(0.0, 3.0, 0.0),
                Vec3::new(4.0, 3.0, 0.0),
                Vec3::new(4.0, 3.0, 4.0),
                Vec3::new(0.0, 3.0, 4.0),
            ],
            vec![
                [0, 1, 2],
                [0, 2, 3],
                [4, 5, 6],
                [4, 6, 7],
                [0, 1, 5],
                [0, 5, 4],
            ],
        );
        let out = outline(&shell, Affine3A::IDENTITY);
        assert_eq!(out.len(), 2, "the floor's two triangles: {out:?}");
        assert!((out.iter().map(|t| area(t)).sum::<f32>() - 16.0).abs() < 1e-3);
    }

    /// The ground itself casts nothing.
    #[test]
    fn the_heightfield_casts_nothing() {
        let field = Collider::heightfield(vec![vec![0.0; 4]; 4], Vec3::new(10.0, 1.0, 10.0));
        assert!(outline(&field, Affine3A::IDENTITY).is_empty());
    }
}
