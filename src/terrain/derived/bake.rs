//! Baking a derived building's template (#1587, #1588): its parts, spawned
//! once and hidden, merged into the two forms its copies are drawn in.
//!
//! - **Near**: the parts merged into one mesh per material (and per sway:
//!   a tree's foliage keeps its [`WindSway`]), carried into the template's
//!   frame by the affine product of the transforms down to each part, as
//!   the renderer carries it.
//! - **Far**: the parts filled into voxels [`FAR_VOXEL_M`] a side, each in
//!   its material's base colour, and the voxels' outer faces merged into
//!   rectangles: a few hundred triangles with no holes where a facade is
//!   tiled from small parts. The same shell is a solid copy's collider.
//!
//! The box the parts fill comes with them: a tree's height scales its
//! copies to the inventory's, and a prop's size picks its draw distance and
//! its collider.

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::MeshAabb;
use bevy::math::{Affine3A, Mat3};
use bevy::mesh::{Indices, MeshVertexAttributeId, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;

use crate::wind::{VegetationWindMaterial, WindSway};

/// The edge of a far form's voxel (m): a storey's height is three.
pub(crate) const FAR_VOXEL_M: f32 = 1.0;

/// The most voxels a far form's grid has a side, its padding included: a
/// building bigger than that is filled in bigger voxels.
const FAR_GRID_MAX: usize = 64;

/// The most rectangles a far form keeps its colours in; past it, it is
/// drawn in its most common colour, which merges its faces to its shape.
pub(crate) const FAR_MAX_QUADS: usize = 1_024;

/// The thinnest a part counts as, each way, when the far form ranks parts
/// by the room they take (m): a flat roof is no less a roof for having no
/// depth.
const FAR_MIN_THICKNESS_M: f32 = 0.5;

/// The colour a far form gives a part whose material it cannot read.
pub(crate) const FAR_FALLBACK_COLOUR: [f32; 4] = [0.5, 0.5, 0.5, 1.0];

/// The parts of a spawned tree, as [`merge_template`] reads them.
pub(crate) type PartQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Transform,
        Option<&'static Visibility>,
        Option<&'static Mesh3d>,
        Option<&'static MeshMaterial3d<StandardMaterial>>,
        Option<&'static MeshMaterial3d<VegetationWindMaterial>>,
        Option<&'static WindSway>,
        Option<&'static Children>,
    ),
>;

/// One merged part of a near form: its mesh, its material, and how it
/// sways in the wind, where it does - the foliage of a tree (#916), which
/// the wind's own material swaps in once it spawns.
pub(crate) type NearPart<M> = (M, Handle<StandardMaterial>, Option<WindSway>);

/// A template baked into the two forms its copies are drawn in (see the
/// module docs), and how much room it takes.
pub(crate) struct Baked {
    /// One mesh per material and sway; `None` where a part is drawn with a
    /// material that is not a [`StandardMaterial`].
    pub near: Option<Vec<NearPart<Mesh>>>,
    /// The far form: one vertex-coloured mesh; `None` where no part has
    /// the positions and normals one needs.
    pub far: Option<Mesh>,
    /// The box the template's parts fill, in its frame: a tree's height
    /// scales its copies to theirs, and a prop's size picks its draw
    /// distance and its collider. `None` where nothing was drawn.
    pub bounds: Option<(Vec3, Vec3)>,
}

/// Bake the parts under `template`, in the template's frame: each part's
/// mesh carried there by the affine product of the transforms down to it,
/// as the renderer carries it, so a sheared part lands where it is drawn.
/// Parts that are hidden are left out with what hangs under them.
///
/// The near form groups the parts by material. The far form fills them
/// into voxels ([`far_form`]), each in its material's base colour - which
/// every catalogue material sets to what its texture tints.
///
/// A swaying part has had its material swapped for the wind's by the time
/// the bake reads it ([`crate::wind::attach_wind_materials`]); it is read as
/// the standard material the wind's wraps, which a merged part spawned
/// with its sway is swapped back from in turn.
///
/// `None` where the template cannot be baked: a mesh that is not a
/// triangle list or whose data has left for the GPU, or a part missing.
pub(crate) fn merge_template(
    template: Entity,
    parts: &PartQuery,
    meshes: &Assets<Mesh>,
    materials: &Assets<StandardMaterial>,
    wind_materials: Option<&Assets<VegetationWindMaterial>>,
) -> Option<Baked> {
    let mut placed_parts: Vec<Placed> = Vec::new();
    let (.., children) = parts.get(template).ok()?;
    let mut stack: Vec<(Entity, Affine3A)> = children
        .map(|c| c.iter().map(|e| (e, Affine3A::IDENTITY)).collect())
        .unwrap_or_default();
    while let Some((entity, above)) = stack.pop() {
        let (transform, visibility, mesh, material, wind, sway, children) =
            parts.get(entity).ok()?;
        if visibility == Some(&Visibility::Hidden) {
            continue;
        }
        let affine = above * transform.compute_affine();
        if let Some(mesh) = mesh {
            let part = placed(meshes.get(&mesh.0)?, affine)?;
            let material = material.map(|m| m.0.clone()).or_else(|| {
                let wind = wind_materials?.get(&wind?.0)?;
                Some(wind.extension.source.clone())
            });
            placed_parts.push((part, material, sway.copied()));
        }
        if let Some(children) = children {
            stack.extend(children.iter().map(|child| (child, affine)));
        }
    }
    Some(Baked {
        near: near_form(&placed_parts),
        far: far_form(&placed_parts, materials),
        bounds: bounds(&placed_parts),
    })
}

/// A part carried into its template's frame: its mesh, its material where
/// that is a standard one, and its sway.
type Placed = (Mesh, Option<Handle<StandardMaterial>>, Option<WindSway>);

/// The box the parts fill, or `None` where none has a position.
fn bounds(parts: &[Placed]) -> Option<(Vec3, Vec3)> {
    parts
        .iter()
        .filter_map(|(mesh, ..)| {
            let aabb = mesh.compute_aabb()?;
            let (centre, half) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
            Some((centre - half, centre + half))
        })
        .reduce(|(lo, hi), (l, h)| (lo.min(l), hi.max(h)))
}

/// The parts grouped into one mesh per material and sway, or `None` where
/// one is not a [`StandardMaterial`] or two of a group do not merge.
fn near_form(parts: &[Placed]) -> Option<Vec<NearPart<Mesh>>> {
    let mut groups: Vec<(Vec<AttributeKind>, NearPart<Mesh>)> = Vec::new();
    for (part, material, sway) in parts {
        let material = material.as_ref()?;
        let kinds = attribute_kinds(part)?;
        match groups
            .iter_mut()
            .find(|(k, (_, m, s))| m.id() == material.id() && *k == kinds && s == sway)
        {
            Some((_, (merged, ..))) => merged.merge(part).ok()?,
            None => groups.push((kinds, (part.clone(), material.clone(), *sway))),
        }
    }
    Some(groups.into_iter().map(|(_, part)| part).collect())
}

/// The far form of the parts (see [`merge_template`]): the building
/// filled into voxels [`FAR_VOXEL_M`] a side, each in the colour of the
/// part that fills it - the bigger parts last, so a wall's colour wins over
/// its windows' - and the voxels' outer faces merged into as few
/// rectangles as their colours allow. A window hole a voxel wide stays a
/// hole; a facade tiled from small parts comes out whole, as a part
/// subset does not.
///
/// Past [`FAR_MAX_QUADS`] rectangles the colours are dropped for the most
/// common one, which merges the faces to the building's shape alone.
fn far_form(parts: &[Placed], materials: &Assets<StandardMaterial>) -> Option<Mesh> {
    let colour_of = |material: &Option<Handle<StandardMaterial>>| {
        material
            .as_ref()
            .and_then(|m| materials.get(m))
            .map_or(FAR_FALLBACK_COLOUR, |m| {
                let [r, g, b, _] = m.base_color.to_linear().to_f32_array();
                [r, g, b, 1.0]
            })
    };
    let mut solids: Vec<Solid> = parts
        .iter()
        .filter_map(|(mesh, material, _)| {
            let Some(VertexAttributeValues::Float32x3(positions)) =
                mesh.try_attribute_option(Mesh::ATTRIBUTE_POSITION).ok()?
            else {
                return None;
            };
            let indices: Vec<u32> = mesh
                .try_indices_option()
                .ok()??
                .iter()
                .map(|i| i as u32)
                .collect();
            let aabb = mesh.compute_aabb()?;
            let size = (Vec3::from(aabb.half_extents) * 2.0).max(Vec3::splat(FAR_MIN_THICKNESS_M));
            Some(Solid {
                room: size.x * size.y * size.z,
                colour: colour_of(material),
                positions,
                indices,
            })
        })
        .collect();
    solids.sort_by(|a, b| a.room.total_cmp(&b.room));
    let grid = VoxelGrid::fill(&solids)?;
    let quads = grid.faces(false);
    let quads = if quads.len() > FAR_MAX_QUADS {
        grid.faces(true)
    } else {
        quads
    };
    Some(grid.mesh(&quads))
}

/// One part as the far form fills it in: the room it takes, its colour,
/// and its triangles.
struct Solid<'a> {
    room: f32,
    colour: [f32; 4],
    positions: &'a [[f32; 3]],
    indices: Vec<u32>,
}

/// A building filled into voxels, padded by an empty voxel all round.
struct VoxelGrid {
    /// The world point of the corner of voxel (1, 1, 1), the first inside
    /// the padding.
    origin: Vec3,
    /// A voxel's edge (m).
    voxel: f32,
    /// Voxels each way, the padding included.
    dims: [usize; 3],
    /// Per voxel, `0` for empty, else its colour's index in `palette` plus
    /// one.
    cells: Vec<u16>,
    palette: Vec<[f32; 4]>,
    /// Per voxel, whether it is empty and reached from outside.
    outside: Vec<bool>,
}

/// One merged face of a far form: the axis it faces along and which way,
/// the voxel layer it bounds, its rectangle in the other two axes, and its
/// colour index.
#[derive(Clone, Copy, Debug)]
struct Quad {
    axis: usize,
    positive: bool,
    layer: usize,
    from: [usize; 2],
    to: [usize; 2],
    colour: u16,
}

impl VoxelGrid {
    /// Fill the voxels the triangles of `solids` pass through, each solid
    /// in its colour, later solids over earlier ones; then find what is
    /// outside. `None` where there is no triangle to fill: a building that
    /// draws nothing has no far form.
    fn fill(solids: &[Solid]) -> Option<Self> {
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        let mut triangles = 0;
        for solid in solids {
            triangles += solid.indices.len() / 3;
            for p in solid.positions {
                lo = lo.min(Vec3::from_array(*p));
                hi = hi.max(Vec3::from_array(*p));
            }
        }
        if triangles == 0 || lo.cmpgt(hi).any() || !(lo.is_finite() && hi.is_finite()) {
            return None;
        }
        let size = hi - lo;
        let voxel = FAR_VOXEL_M.max(size.max_element() / (FAR_GRID_MAX - 3) as f32);
        let inner = |extent: f32| (extent / voxel).floor() as usize + 1;
        let dims = [inner(size.x) + 2, inner(size.y) + 2, inner(size.z) + 2];
        let mut grid = VoxelGrid {
            origin: lo,
            voxel,
            dims,
            cells: vec![0; dims[0] * dims[1] * dims[2]],
            palette: Vec::new(),
            outside: Vec::new(),
        };
        for Solid {
            colour,
            positions,
            indices,
            ..
        } in solids
        {
            let k = match grid.palette.iter().position(|c| c == colour) {
                Some(k) => k,
                None => {
                    grid.palette.push(*colour);
                    grid.palette.len() - 1
                }
            };
            let k = u16::try_from(k + 1).ok()?;
            for triangle in indices.chunks_exact(3) {
                let [a, b, c] =
                    [0, 1, 2].map(|i| Vec3::from_array(positions[triangle[i] as usize]));
                let longest = (b - a).length().max((c - a).length()).max((c - b).length());
                // Samples half a voxel apart reach every voxel the triangle
                // passes through.
                let steps = ((longest / (voxel * 0.5)).ceil() as usize).clamp(1, 512);
                for i in 0..=steps {
                    for j in 0..=steps - i {
                        let (u, v) = (i as f32 / steps as f32, j as f32 / steps as f32);
                        let cell = grid.cell_at(a + (b - a) * u + (c - a) * v);
                        grid.cells[cell] = k;
                    }
                }
            }
        }
        grid.outside = grid.reach_outside();
        Some(grid)
    }

    /// The voxel holding world point `p`, inside the padding.
    fn cell_at(&self, p: Vec3) -> usize {
        let g = (p - self.origin) / self.voxel;
        let axis = |v: f32, n: usize| (v.floor().max(0.0) as usize).min(n - 3) + 1;
        let [x, y, z] = [
            axis(g.x, self.dims[0]),
            axis(g.y, self.dims[1]),
            axis(g.z, self.dims[2]),
        ];
        self.index([x, y, z])
    }

    fn index(&self, [x, y, z]: [usize; 3]) -> usize {
        (z * self.dims[1] + y) * self.dims[0] + x
    }

    /// The empty voxels reached from the padding, six ways.
    fn reach_outside(&self) -> Vec<bool> {
        let mut outside = vec![false; self.cells.len()];
        let mut queue = std::collections::VecDeque::from([0usize]);
        outside[0] = true;
        let [nx, ny, nz] = self.dims;
        while let Some(i) = queue.pop_front() {
            let (x, y, z) = (i % nx, (i / nx) % ny, i / (nx * ny));
            let next = [
                (x > 0).then(|| i - 1),
                (x + 1 < nx).then(|| i + 1),
                (y > 0).then(|| i - nx),
                (y + 1 < ny).then(|| i + nx),
                (z > 0).then(|| i - nx * ny),
                (z + 1 < nz).then(|| i + nx * ny),
            ];
            for j in next.into_iter().flatten() {
                if !outside[j] && self.cells[j] == 0 {
                    outside[j] = true;
                    queue.push_back(j);
                }
            }
        }
        outside
    }

    /// The faces between filled voxels and the outside, merged into
    /// rectangles of one colour - or, `one_colour`, of the most common.
    fn faces(&self, one_colour: bool) -> Vec<Quad> {
        let common = one_colour.then(|| {
            let mut counts = vec![0usize; self.palette.len() + 1];
            for &c in &self.cells {
                counts[usize::from(c)] += 1;
            }
            counts[0] = 0;
            (0..counts.len()).max_by_key(|&k| counts[k]).unwrap_or(0) as u16
        });
        let mut quads = Vec::new();
        for axis in 0..3 {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let (nu, nv) = (self.dims[u], self.dims[v]);
            for positive in [false, true] {
                for layer in 1..self.dims[axis] - 1 {
                    let mut mask = vec![0u16; nu * nv];
                    for b in 0..nv {
                        for a in 0..nu {
                            let mut at = [0; 3];
                            at[axis] = layer;
                            at[u] = a;
                            at[v] = b;
                            let cell = self.cells[self.index(at)];
                            if cell == 0 {
                                continue;
                            }
                            at[axis] = if positive { layer + 1 } else { layer - 1 };
                            if self.outside[self.index(at)] {
                                mask[b * nu + a] = common.unwrap_or(cell);
                            }
                        }
                    }
                    greedy(&mut mask, nu, nv, |from, to, colour| {
                        quads.push(Quad {
                            axis,
                            positive,
                            layer,
                            from,
                            to,
                            colour,
                        });
                    });
                }
            }
        }
        quads
    }

    /// The far form's mesh from its `quads`: positions in the building's
    /// frame, flat normals facing out, wound counter-clockwise seen from
    /// outside, and each quad's colour.
    fn mesh(&self, quads: &[Quad]) -> Mesh {
        let (mut positions, mut normals, mut colours, mut indices) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        // A voxel coordinate to the world: (1, 1, 1) is the origin.
        let world = |c: [usize; 3]| {
            self.origin
                + Vec3::new(c[0] as f32 - 1.0, c[1] as f32 - 1.0, c[2] as f32 - 1.0) * self.voxel
        };
        for q in quads {
            let (u, v) = ((q.axis + 1) % 3, (q.axis + 2) % 3);
            let plane = if q.positive { q.layer + 1 } else { q.layer };
            let corner = |a: usize, b: usize| {
                let mut c = [0; 3];
                c[q.axis] = plane;
                c[u] = a;
                c[v] = b;
                world(c)
            };
            let quad = [
                corner(q.from[0], q.from[1]),
                corner(q.to[0], q.from[1]),
                corner(q.to[0], q.to[1]),
                corner(q.from[0], q.to[1]),
            ];
            let mut normal = Vec3::ZERO;
            normal[q.axis] = if q.positive { 1.0 } else { -1.0 };
            let base = positions.len() as u32;
            positions.extend(quad.map(|p| p.to_array()));
            normals.extend([normal.to_array(); 4]);
            colours.extend([self.palette[usize::from(q.colour) - 1]; 4]);
            let facing_out = (quad[1] - quad[0]).cross(quad[3] - quad[0]).dot(normal) > 0.0;
            indices.extend(if facing_out {
                [base, base + 1, base + 2, base, base + 2, base + 3]
            } else {
                [base, base + 2, base + 1, base, base + 3, base + 2]
            });
        }
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
        mesh.insert_indices(Indices::U32(indices));
        mesh
    }
}

/// Cover the non-zero cells of `mask` (`nu` x `nv`, rows of `nu`) with
/// rectangles of one value each, greedily: each grown along its row, then
/// down while whole rows match. Each rectangle goes to `emit` as its first
/// cell, one past its last, and its value; the mask is cleared as it goes.
fn greedy(
    mask: &mut [u16],
    nu: usize,
    nv: usize,
    mut emit: impl FnMut([usize; 2], [usize; 2], u16),
) {
    for b in 0..nv {
        let mut a = 0;
        while a < nu {
            let value = mask[b * nu + a];
            if value == 0 {
                a += 1;
                continue;
            }
            let mut width = 1;
            while a + width < nu && mask[b * nu + a + width] == value {
                width += 1;
            }
            let mut height = 1;
            while b + height < nv && (a..a + width).all(|x| mask[(b + height) * nu + x] == value) {
                height += 1;
            }
            for row in b..b + height {
                mask[row * nu + a..row * nu + a + width].fill(0);
            }
            emit([a, b], [a + width, b + height], value);
            a += width;
        }
    }
}

/// A vertex attribute's id and the variant its values are stored as: two
/// meshes merge only where these agree.
type AttributeKind = (
    MeshVertexAttributeId,
    std::mem::Discriminant<VertexAttributeValues>,
);

/// The attributes of `mesh`, in id order.
fn attribute_kinds(mesh: &Mesh) -> Option<Vec<AttributeKind>> {
    let mut kinds: Vec<AttributeKind> = mesh
        .try_attributes()
        .ok()?
        .map(|(attribute, values)| (attribute.id, std::mem::discriminant(values)))
        .collect();
    kinds.sort_by_key(|(id, _)| *id);
    Some(kinds)
}

/// `source` carried by `affine`, with 32-bit indices: positions by the
/// affine, normals by its inverse transpose, tangents by its linear part
/// with their handedness flipped where it mirrors - what the renderer does
/// to a part drawn at `affine`. The index order is kept, so a mirroring
/// part keeps the winding it is drawn with.
fn placed(source: &Mesh, affine: Affine3A) -> Option<Mesh> {
    if source.primitive_topology() != PrimitiveTopology::TriangleList {
        return None;
    }
    let mut mesh = source.clone();
    let linear = Mat3::from(affine.matrix3);
    let normal_matrix = linear.inverse().transpose();
    let mirrored = linear.determinant() < 0.0;
    match mesh
        .try_attribute_mut_option(Mesh::ATTRIBUTE_POSITION)
        .ok()?
    {
        Some(VertexAttributeValues::Float32x3(positions)) => {
            for p in positions {
                *p = affine.transform_point3(Vec3::from_array(*p)).to_array();
            }
        }
        _ => return None,
    }
    if let Some(VertexAttributeValues::Float32x3(normals)) =
        mesh.try_attribute_mut_option(Mesh::ATTRIBUTE_NORMAL).ok()?
    {
        for n in normals {
            *n = (normal_matrix * Vec3::from_array(*n))
                .normalize_or_zero()
                .to_array();
        }
    }
    if let Some(VertexAttributeValues::Float32x4(tangents)) = mesh
        .try_attribute_mut_option(Mesh::ATTRIBUTE_TANGENT)
        .ok()?
    {
        for t in tangents {
            let v = (linear * Vec3::new(t[0], t[1], t[2])).normalize_or_zero();
            let w = if mirrored { -t[3] } else { t[3] };
            *t = [v.x, v.y, v.z, w];
        }
    }
    let indices: Vec<u32> = match mesh.try_indices_option().ok()? {
        Some(indices) => indices.iter().map(|i| i as u32).collect(),
        None => (0..mesh.count_vertices() as u32).collect(),
    };
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    /// The positions of `mesh`.
    fn positions(mesh: &Mesh) -> Vec<Vec3> {
        match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(p)) => {
                p.iter().map(|v| Vec3::from_array(*v)).collect()
            }
            _ => panic!("positions"),
        }
    }

    /// `template`'s parts baked, in a world with the two asset stores a
    /// merge reads.
    fn bake(world: &mut World, template: Entity) -> Option<Baked> {
        world
            .run_system_once(
                move |parts: PartQuery,
                      meshes: Res<Assets<Mesh>>,
                      materials: Res<Assets<StandardMaterial>>,
                      wind: Option<Res<Assets<VegetationWindMaterial>>>| {
                    merge_template(template, &parts, &meshes, &materials, wind.as_deref())
                },
            )
            .expect("the system runs")
    }

    fn asset_world() -> World {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<StandardMaterial>>();
        world
    }

    /// The renderer carries a part by the affine product of the transforms
    /// down to it, shear and all; the merge must land every vertex there.
    #[test]
    fn a_merged_building_is_its_parts_where_they_are_drawn() {
        let mut world = asset_world();
        let cube = world
            .resource_mut::<Assets<Mesh>>()
            .add(Mesh::from(Cuboid::new(1.0, 2.0, 3.0)));
        let (stone, glass) = {
            let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
            (
                materials.add(StandardMaterial::default()),
                materials.add(StandardMaterial::default()),
            )
        };
        // A non-uniform scale under a turn, then a turn under that: the
        // product shears, which no single `Transform` can say.
        let wing = Transform::from_xyz(4.0, 0.0, -2.0)
            .with_rotation(Quat::from_rotation_y(0.7))
            .with_scale(Vec3::new(2.0, 1.0, 0.5));
        let part = Transform::from_xyz(0.0, 1.0, 0.0).with_rotation(Quat::from_rotation_z(0.4));
        let template = world.spawn((Transform::IDENTITY, Visibility::Hidden)).id();
        let wing_entity = world.spawn((wing, ChildOf(template))).id();
        let parts = [
            (stone.clone(), part, wing_entity, Visibility::Inherited),
            (
                stone.clone(),
                Transform::IDENTITY,
                template,
                Visibility::Inherited,
            ),
            (
                glass.clone(),
                Transform::IDENTITY,
                template,
                Visibility::Inherited,
            ),
            // Hidden, so left out.
            (
                glass.clone(),
                Transform::IDENTITY,
                template,
                Visibility::Hidden,
            ),
        ];
        for (material, transform, parent, visibility) in parts {
            world.spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(material),
                transform,
                visibility,
                ChildOf(parent),
            ));
        }

        let near = bake(&mut world, template)
            .and_then(|baked| baked.near)
            .expect("standard materials merge");
        assert_eq!(near.len(), 2, "one mesh per material");
        let meshes = world.resource::<Assets<Mesh>>();
        let source = meshes.get(&cube).unwrap();
        let cube_points = positions(source);
        let n = cube_points.len();
        let (stone_mesh, ..) = near.iter().find(|(_, m, _)| *m == stone).unwrap();
        let (glass_mesh, ..) = near.iter().find(|(_, m, _)| *m == glass).unwrap();
        assert_eq!(
            glass_mesh.count_vertices(),
            n,
            "the hidden glass is left out"
        );
        assert_eq!(stone_mesh.count_vertices(), 2 * n);
        let drawn = wing.compute_affine() * part.compute_affine();
        let stone_points = positions(stone_mesh);
        // The walk takes the parts in no promised order: find each one's run.
        let sheared: Vec<Vec3> = cube_points
            .iter()
            .map(|&p| drawn.transform_point3(p))
            .collect();
        let runs = [&stone_points[..n], &stone_points[n..]];
        let lands =
            |run: &[Vec3], at: &[Vec3]| run.iter().zip(at).all(|(a, b)| a.distance(*b) < 1e-5);
        assert!(
            runs.iter().any(|run| lands(run, &sheared))
                && runs.iter().any(|run| lands(run, &cube_points)),
            "the sheared part lands where it is drawn, the plain one where it was"
        );
        // Normals by the inverse transpose, unit length.
        let Some(VertexAttributeValues::Float32x3(normals)) =
            stone_mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("normals");
        };
        assert!(
            normals
                .iter()
                .all(|n| (Vec3::from_array(*n).length() - 1.0).abs() < 1e-5)
        );
        // Indices index the merged vertices.
        let indices = stone_mesh.indices().expect("indexed");
        assert!(indices.iter().all(|i| i < 2 * n));
        assert_eq!(indices.len(), 2 * source.indices().unwrap().len());

        // A part drawn with a material the merge cannot carry leaves the
        // building whole near, and grey far.
        world.spawn((
            Mesh3d(cube),
            Transform::from_xyz(10.0, 0.0, 0.0),
            ChildOf(template),
        ));
        let baked = bake(&mut world, template).expect("bakes");
        assert!(baked.near.is_none());
        let far = baked.far.expect("a far form");
        let Some(VertexAttributeValues::Float32x4(colours)) = far.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colours");
        };
        assert!(colours.contains(&FAR_FALLBACK_COLOUR));
    }

    /// A tree's foliage sways and its bark does not: the near form keeps
    /// them apart, each part with the sway it had, and the box they fill
    /// comes with them.
    #[test]
    fn a_swaying_part_is_merged_apart_and_keeps_its_sway() {
        let mut world = asset_world();
        let (trunk, crown) = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            (
                meshes.add(Mesh::from(Cuboid::new(0.4, 4.0, 0.4))),
                meshes.add(Mesh::from(Cuboid::new(3.0, 2.0, 3.0))),
            )
        };
        let leaf = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let template = world.spawn((Transform::IDENTITY, Visibility::Hidden)).id();
        world.spawn((
            Mesh3d(trunk),
            MeshMaterial3d(leaf.clone()),
            Transform::from_xyz(0.0, 2.0, 0.0),
            ChildOf(template),
        ));
        let foliage = world
            .spawn((
                Mesh3d(crown),
                MeshMaterial3d(leaf.clone()),
                Transform::from_xyz(0.0, 5.0, 0.0),
                crate::wind::WindSway::Branch,
                ChildOf(template),
            ))
            .id();
        let grouped = |baked: Baked| {
            let near = baked.near.expect("merges");
            let mut parts: Vec<_> = near
                .iter()
                .map(|(_, material, sway)| (material.id(), *sway))
                .collect();
            parts.sort_by_key(|(_, sway)| sway.is_some());
            parts
        };
        let both = vec![
            (leaf.id(), None),
            (leaf.id(), Some(crate::wind::WindSway::Branch)),
        ];
        let baked = bake(&mut world, template).expect("bakes");
        let (lo, hi) = baked.bounds.expect("a box");
        assert!(lo.distance(Vec3::new(-1.5, 0.0, -1.5)) < 1e-5, "{lo}");
        assert!(hi.distance(Vec3::new(1.5, 6.0, 1.5)) < 1e-5, "{hi}");
        assert_eq!(grouped(baked), both, "one material, two sways: two parts");

        // The wind has swapped the foliage onto its own material by the
        // time a bake reads it: read as the leaf it wraps, as before.
        world.init_resource::<Assets<VegetationWindMaterial>>();
        let base = world
            .resource::<Assets<StandardMaterial>>()
            .get(&leaf)
            .unwrap()
            .clone();
        let wind =
            world
                .resource_mut::<Assets<VegetationWindMaterial>>()
                .add(VegetationWindMaterial {
                    base,
                    extension: crate::wind::WindExtension {
                        uniforms: Default::default(),
                        source: leaf.clone(),
                    },
                });
        world
            .entity_mut(foliage)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert(MeshMaterial3d(wind));
        let baked = bake(&mut world, template).expect("bakes");
        let Some(VertexAttributeValues::Float32x4(colours)) = baked
            .far
            .as_ref()
            .and_then(|far| far.attribute(Mesh::ATTRIBUTE_COLOR))
        else {
            panic!("a coloured far form");
        };
        assert!(!colours.contains(&FAR_FALLBACK_COLOUR), "no grey crown");
        assert_eq!(grouped(baked), both);
    }

    /// The far form is the building's outer shell in voxels: a box is its
    /// six faces, two boxes meeting are one shell with nothing inside it,
    /// and each face takes the colour of the biggest part that fills it.
    #[test]
    fn a_far_form_is_the_buildings_shell_in_its_colours() {
        let mut world = asset_world();
        let (hall, wing, sill) = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            (
                meshes.add(Mesh::from(Cuboid::new(20.0, 12.0, 10.0))),
                meshes.add(Mesh::from(Cuboid::new(10.0, 6.0, 10.0))),
                meshes.add(Mesh::from(Cuboid::new(1.0, 0.2, 0.3))),
            )
        };
        let red = Color::linear_rgb(0.8, 0.1, 0.1);
        let blue = Color::linear_rgb(0.1, 0.1, 0.8);
        let (brick, trim) = {
            let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
            (
                materials.add(StandardMaterial::from_color(red)),
                materials.add(StandardMaterial::from_color(blue)),
            )
        };
        let template = world.spawn((Transform::IDENTITY, Visibility::Hidden)).id();
        // The hall alone first.
        let hall_entity = world
            .spawn((
                Mesh3d(hall),
                MeshMaterial3d(brick.clone()),
                Transform::IDENTITY,
                ChildOf(template),
            ))
            .id();
        let far = bake(&mut world, template)
            .and_then(|baked| baked.far)
            .expect("a far form");
        assert_eq!(far.count_vertices(), 6 * 4, "a box is six faces");
        let points = positions(&far);
        let (lo, hi) = points.iter().fold(
            (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        );
        // The shell bounds the box to within a voxel.
        assert!(lo.distance(Vec3::new(-10.0, -6.0, -5.0)) < 1e-4, "{lo}");
        assert!(
            (hi - Vec3::new(10.0, 6.0, 5.0)).max_element() <= FAR_VOXEL_M + 1e-4,
            "{hi}"
        );
        // Every face is wound to face out, along its normal.
        let Some(VertexAttributeValues::Float32x3(normals)) = far.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("normals");
        };
        let indices: Vec<usize> = far.indices().unwrap().iter().collect();
        for t in indices.chunks_exact(3) {
            let [a, b, c] = [points[t[0]], points[t[1]], points[t[2]]];
            assert!((b - a).cross(c - a).dot(Vec3::from_array(normals[t[0]])) > 0.0);
        }

        // A wing beside it and a sill on its wall: one shell, no faces
        // inside, the wall's colour over the sill's.
        world.spawn((
            Mesh3d(wing),
            MeshMaterial3d(brick.clone()),
            Transform::from_xyz(15.0, -3.0, 0.0),
            ChildOf(template),
        ));
        world.spawn((
            Mesh3d(sill),
            MeshMaterial3d(trim),
            Transform::from_xyz(0.0, 0.0, 5.0),
            ChildOf(hall_entity),
        ));
        let far = bake(&mut world, template)
            .and_then(|baked| baked.far)
            .expect("a far form");
        let points = positions(&far);
        let inside = points
            .iter()
            .filter(|p| p.x > -9.0 && p.x < 9.0 && p.y > -5.0 && p.y < 5.0 && p.z.abs() < 4.0)
            .count();
        assert_eq!(inside, 0, "no face inside the shell");
        let Some(VertexAttributeValues::Float32x4(colours)) = far.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colours");
        };
        let [r, g, b, _] = red.to_linear().to_f32_array();
        assert!(
            colours.iter().all(|c| *c == [r, g, b, 1.0]),
            "brick all over"
        );
        assert!(
            far.count_vertices() < 4 * 20,
            "{} vertices",
            far.count_vertices()
        );
        let kinds: Vec<_> = far.attributes().map(|(a, _)| a.id).collect();
        assert_eq!(kinds.len(), 3, "positions, normals and colours: {kinds:?}");
    }

    /// The critic's finding (#1587): a template that drew nothing - a
    /// grammar that failed - baked to an empty far form, and each of its far
    /// copies spawned an entity drawing nothing, counted against the cap.
    #[test]
    fn a_building_that_draws_nothing_has_no_far_form() {
        let mut world = asset_world();
        let template = world.spawn((Transform::IDENTITY, Visibility::Hidden)).id();
        let baked = bake(&mut world, template).expect("an empty template bakes");
        assert!(baked.far.is_none() && baked.bounds.is_none());
        assert!(baked.near.is_some_and(|near| near.is_empty()));
    }

    #[test]
    fn greedy_covers_each_run_of_one_value_with_rectangles() {
        // 1 1 2
        // 1 1 0
        let mut mask = vec![1, 1, 2, 1, 1, 0];
        let mut rects = Vec::new();
        greedy(&mut mask, 3, 2, |from, to, value| {
            rects.push((from, to, value))
        });
        assert_eq!(rects, vec![([0, 0], [2, 2], 1), ([2, 0], [3, 1], 2)]);
        assert!(mask.iter().all(|&v| v == 0));
    }
}
