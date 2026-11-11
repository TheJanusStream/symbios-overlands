//! The ground mesh's tangents, read off its grid (#1904).
//!
//! Bevy's `Mesh::generate_tangents` runs mikktspace, and on the shipped
//! 512-square ground it took about 0.93 s of the 0.94 s the whole terrain
//! build cost (the mesh itself about 4 ms, the heightfield collider under
//! 1 ms; seed 90, `test-release`, native). Most of that is mikktspace's
//! general machinery: welding corners that share a position, normal and UV
//! (a map of every corner), sorting every edge to find each face's
//! neighbours, walking a vertex's faces into groups. The heightmap mesh
//! needs none of it - every corner is already one shared vertex, every face
//! of the grid has the same UV orientation and none is degenerate - and
//! what is left is this:
//!
//! 1. each face's tangent and bitangent, `dP/du` and `dP/dv` worked out from
//!    its positions and UVs exactly as mikktspace works them out, signed by
//!    the face's orientation;
//! 2. at each of its three corners, the tangent projected off the corner's
//!    normal, weighted by the face's angle at the corner (measured between
//!    the two edges projected off that normal) and added to the vertex's
//!    sum - in the order of the faces, as mikktspace adds them;
//! 3. each sum normalised, with the handedness Bevy writes.
//!
//! One rule of mikktspace's grouping survives, because it moves a tangent
//! on very steep ground: a face whose projected tangent or bitangent points
//! exactly against another's at a corner (their cosine rounds to -1) is not
//! summed with it there. Bevy keeps, for each vertex, what its LAST face's
//! corner was given, so a vertex sums the faces that agree with its last
//! face - every face, but on ground of slopes of tens to one (3 corners of
//! 920 on a random field 0.7 m apart with 30 m of relief). A pass from the
//! back finds each vertex's last face before the sums are made.
//!
//! The same arithmetic in the same order, so the tangents are mikktspace's
//! bit for bit: `the_ground_takes_mikktspaces_tangents_bit_for_bit`, below,
//! compares the two on flat, rough, steep, non-square and generated ground.
//! Measured when this was written: every one of the 262 144 vertices
//! matched on each of six generated worlds at the 512 grid (seeds 1, 7, 42,
//! 90, 123 and 777), in about 51 ms against mikktspace's 0.92 s. The one
//! difference found is mikktspace's own: its edge sort can leave two faces
//! that share an edge unpaired (its source says so - "if `edges` is
//! improperly sorted, neighbors may not be found"), which splits a vertex's
//! faces into two groups for no reason of shape; that happened at 1 vertex
//! of 374 288 over 300 random fields, on a map's last row, and there this
//! keeps the sum of every face, as mikktspace does wherever it pairs them.
//! The drawn ground does not change, and the web, where the build runs on
//! the main thread, saves the same.

use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;

/// The tangents mikktspace gives `mesh`, a heightmap mesh as
/// `bevy_symbios_ground`'s `HeightMapMeshBuilder` builds one, with the
/// handedness Bevy's `Mesh::generate_tangents` writes.
///
/// # Panics
///
/// When `mesh` lacks its positions, normals, UVs or `u32` indices - the
/// mesher always writes all four.
pub(super) fn of(mesh: &Mesh) -> Vec<[f32; 4]> {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("the ground mesh has positions");
    };
    let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
    else {
        panic!("the ground mesh has normals");
    };
    let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        panic!("the ground mesh has UVs");
    };
    let Some(Indices::U32(indices)) = mesh.indices() else {
        panic!("the ground mesh has u32 indices");
    };
    grid(positions, normals, uvs, indices)
}

/// A face's tangent and bitangent as mikktspace first works them out (its
/// eqs. 18 and 19), turned by the face's orientation in UV space - before
/// they are normalised - and that orientation.
fn face_frame(v: [[f32; 3]; 3], tx: [[f32; 2]; 3]) -> ([f32; 3], [f32; 3], bool) {
    // Spelled out rather than `map`ped: the hot loop is twice as fast with
    // nothing left behind a closure the optimiser declines to inline.
    let d_tx = [
        [tx[1][0] - tx[0][0], tx[1][1] - tx[0][1]],
        [tx[2][0] - tx[0][0], tx[2][1] - tx[0][1]],
    ];
    let d_v = [sub(v[1], v[0]), sub(v[2], v[0])];
    let preserving = d_tx[0][0] * d_tx[1][1] - d_tx[0][1] * d_tx[1][0] > 0.0;
    let s = sub(scale(d_tx[1][1], d_v[0]), scale(d_tx[0][1], d_v[1]));
    let t = add(scale(-d_tx[1][0], d_v[0]), scale(d_tx[0][0], d_v[1]));
    (s, t, preserving)
}

/// `a` normalised, then turned by `preserving` as mikktspace turns a face's
/// frame (a multiplication by -1, for its NaNs' sake).
fn signed(a: [f32; 3], preserving: bool) -> [f32; 3] {
    scale(if preserving { 1.0 } else { -1.0 }, normalised(a))
}

/// [`of`] from the mesh's own buffers.
///
/// What makes this mikktspace's answer is the mesh's shape, so it is said
/// here: every index names one vertex no other index's vertex shares a
/// position with; no face is degenerate, in its positions or its UVs; every
/// face has the same orientation in UV space; and the faces around a vertex
/// join it edge to edge. All four hold for any heightmap mesh of 2 x 2
/// samples or more.
fn grid(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    uvs: &[[f32; 2]],
    indices: &[u32],
) -> Vec<[f32; 4]> {
    let faces = indices.as_chunks::<3>().0;
    let corners = |face: &[u32; 3]| {
        let [a, b, c] = *face;
        let v = [
            positions[a as usize],
            positions[b as usize],
            positions[c as usize],
        ];
        let tx = [uvs[a as usize], uvs[b as usize], uvs[c as usize]];
        (v, tx)
    };
    let frame = |face: &[u32; 3]| {
        let (v, tx) = corners(face);
        face_frame(v, tx)
    };
    // Each vertex's last face - the corner Bevy writes last, so the one
    // whose tangent and handedness the vertex keeps - and that face's frame
    // projected off the vertex's normal: unnormalised, which is enough to
    // tell the frames that cannot be against it from the few that might.
    let mut last = vec![Last::NONE; positions.len()];
    for (f, face) in faces.iter().enumerate().rev() {
        if face.iter().all(|&at| last[at as usize].face != u32::MAX) {
            continue;
        }
        let (s, t, preserving) = frame(face);
        let (s, t) = (signed_raw(s, preserving), signed_raw(t, preserving));
        for &at in face {
            let at = at as usize;
            if last[at].face == u32::MAX {
                let n = normals[at];
                last[at] = Last {
                    face: f as u32,
                    s: off_normal(s, n),
                    t: off_normal(t, n),
                    preserving,
                };
            }
        }
    }
    let mut sums = vec![[0.0f32; 3]; positions.len()];
    for face in faces {
        let (v, tx) = corners(face);
        let (s_raw, t_raw, preserving) = face_frame(v, tx);
        let s = signed(s_raw, preserving);
        let t = signed_raw(t_raw, preserving);
        for corner in 0..3 {
            let at = face[corner] as usize;
            let n = normals[at];
            let s_off = off_normal(s, n);
            // `generate_tangents` sets mikktspace's grouping threshold at
            // -1: only a frame exactly against the last face's stays out
            // (and, as there, one whose cosine is not a number). Frames
            // with no negative cosine to it cannot be; the rest are
            // decided as mikktspace decides them.
            let last = &last[at];
            let clearly = dot(last.s, s_off) >= 0.0 && dot(last.t, off_normal(t, n)) >= 0.0;
            if !clearly
                && !agrees_exactly(
                    n,
                    frame(&faces[last.face as usize]),
                    (s_raw, t_raw, preserving),
                )
            {
                continue;
            }
            let edge = |to: usize| normalised(off_normal(sub(v[to], v[corner]), n));
            let (next, prev) = (edge((corner + 1) % 3), edge((corner + 2) % 3));
            // In f64, as Bevy's mikktspace takes the arc cosine.
            let angle = f64::from(dot(next, prev).clamp(-1.0, 1.0)).acos() as f32;
            sums[at] = add(sums[at], scale(angle, normalised(s_off)));
        }
    }
    sums.into_iter()
        .zip(last)
        .map(|(sum, last)| {
            let [x, y, z] = normalised(sum);
            // Bevy flips mikktspace's sign to its own handedness.
            [x, y, z, if last.preserving { -1.0 } else { 1.0 }]
        })
        .collect()
}

/// A vertex's last face, and its frame off the vertex's normal.
#[derive(Clone, Copy)]
struct Last {
    face: u32,
    s: [f32; 3],
    t: [f32; 3],
    preserving: bool,
}

impl Last {
    const NONE: Self = Self {
        face: u32::MAX,
        s: [0.0; 3],
        t: [0.0; 3],
        preserving: false,
    };
}

/// Whether the face framed `here` agrees with the vertex's `last` face at
/// the vertex's normal `n`, as mikktspace decides it: neither its tangent
/// nor its bitangent, normalised and projected off `n`, has a cosine of -1
/// (or none) to the last face's.
#[inline(never)]
#[cold]
fn agrees_exactly(
    n: [f32; 3],
    last: ([f32; 3], [f32; 3], bool),
    here: ([f32; 3], [f32; 3], bool),
) -> bool {
    let projected =
        |a: [f32; 3], preserving: bool| normalised(off_normal(signed(a, preserving), n));
    let (s_last, t_last) = (projected(last.0, last.2), projected(last.1, last.2));
    let (s_here, t_here) = (projected(here.0, here.2), projected(here.1, here.2));
    dot(s_last, s_here) > -1.0 && dot(t_last, t_here) > -1.0
}

/// `a` turned by `preserving`, unnormalised: the direction mikktspace's
/// frame takes, at another length.
fn signed_raw(a: [f32; 3], preserving: bool) -> [f32; 3] {
    if preserving { a } else { scale(-1.0, a) }
}

// mikktspace's own vector arithmetic, operand for operand: glam's might
// differ in the last place, and the point is to give its answer exactly.

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(k: f32, a: [f32; 3]) -> [f32; 3] {
    [k * a[0], k * a[1], k * a[2]]
}

/// `a` less its part along the unit `n`.
fn off_normal(a: [f32; 3], n: [f32; 3]) -> [f32; 3] {
    sub(a, scale(dot(n, a), n))
}

/// `a` at unit length, or as it is where every component is zero or
/// subnormal.
fn normalised(a: [f32; 3]) -> [f32; 3] {
    let not_zero = |c: f32| c.abs() > f32::MIN_POSITIVE;
    if not_zero(a[0]) || not_zero(a[1]) || not_zero(a[2]) {
        scale(dot(a, a).sqrt().recip(), a)
    } else {
        a
    }
}

#[cfg(test)]
mod tests {
    use bevy_symbios_ground::{HeightMap, HeightMapMeshBuilder, NormalMethod};

    use super::*;

    /// A heightmap mesh as the terrain builds it: area-weighted normals and
    /// one UV tile across the whole map.
    fn meshed(hm: &HeightMap) -> Mesh {
        HeightMapMeshBuilder::new()
            .with_normal_method(NormalMethod::AreaWeighted)
            .with_uv_tile_size((hm.width() - 1) as f32 * hm.scale())
            .build(hm)
    }

    fn filled(w: usize, h: usize, scale: f32, height: impl Fn(usize, usize) -> f32) -> HeightMap {
        let mut hm = HeightMap::new(w, h, scale);
        for z in 0..h {
            for x in 0..w {
                hm.set(x, z, height(x, z));
            }
        }
        hm
    }

    /// #1904: the ground's tangents are mikktspace's, bit for bit, on every
    /// kind of ground the terrain draws - so reading them off the grid in
    /// 51 ms rather than running mikktspace for 0.92 s leaves the drawn
    /// ground exactly as it was. Flat, rough (a hash per sample, metres of
    /// relief between neighbours), cliffs, a map whose sides differ - steep
    /// enough that mikktspace keeps three of its corners out of their
    /// vertices' sums, which a plain sum of every face misses - and a
    /// generated world's eroded ground.
    #[test]
    fn the_ground_takes_mikktspaces_tangents_bit_for_bit() {
        let rough = |x: usize, z: usize| {
            let mut n = (x as u32).wrapping_mul(0x9e37_79b9) ^ (z as u32).wrapping_mul(0x85eb_ca6b);
            n ^= n >> 15;
            n = n.wrapping_mul(0x2c1b_3c6d);
            n ^= n >> 12;
            (n % 10_000) as f32 / 1_000.0
        };
        let mut record = crate::pds::RoomRecord::default_for_seed(90, "did:render:90");
        for generator in record.generators.values_mut() {
            if let crate::pds::GeneratorKind::Terrain(terrain) = &mut generator.kind {
                terrain.grid_size = 97;
            }
        }
        let generated = super::super::rebuild_heightmap_for_record(&record);
        assert_eq!(generated.width(), 97, "the generated ground at its grid");
        let grounds = [
            ("flat", HeightMap::new(16, 16, 1.0)),
            ("rough", filled(33, 33, 2.0, rough)),
            (
                "cliffs",
                filled(24, 24, 1.5, |x, z| {
                    if (x / 5 + z / 7) % 2 == 0 { 40.0 } else { 0.0 }
                }),
            ),
            ("non-square", filled(40, 23, 0.7, |x, z| rough(z, x) * 3.0)),
            ("generated", generated),
        ];
        for (name, hm) in grounds {
            let mut mesh = meshed(&hm);
            let ours = of(&mesh);
            mesh.generate_tangents().expect("mikktspace runs");
            let Some(VertexAttributeValues::Float32x4(theirs)) =
                mesh.attribute(Mesh::ATTRIBUTE_TANGENT)
            else {
                panic!("mikktspace wrote tangents");
            };
            assert_eq!(ours.len(), theirs.len());
            let differ: Vec<usize> = (0..ours.len())
                .filter(|&i| ours[i].map(f32::to_bits) != theirs[i].map(f32::to_bits))
                .collect();
            assert!(
                differ.is_empty(),
                "{name}: {} of {} tangents differ from mikktspace's, the first at vertex {}: \
                 {:?} against {:?}",
                differ.len(),
                ours.len(),
                differ[0],
                ours[differ[0]],
                theirs[differ[0]],
            );
        }
    }
}
