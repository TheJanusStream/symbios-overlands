//! The middle ring's buildings (#1587, epic #1580): the region's catalogue
//! buildings on the lots a geodata region's ring cuts round its walkable
//! ground ([`super::geo::ring`]).
//!
//! They are the theme's own, as the road layer grows its lots
//! ([`super::lots`]): the same pools by the room's prosperity and
//! escalation, the same finish and ruin. Each lot takes one:
//!
//! - The tallest of Berlin's buildings - [`LANDMARK_STANDING_M`] or more,
//!   no two within [`LANDMARK_SPACING_M`], at most [`MAX_RING_LANDMARKS`] -
//!   take the theme's landmarks: a church tower, a dome, a high-rise.
//! - Every other lot takes a secondary building, a bigger one where
//!   Berlin's stands taller: the pool is ranked by size, and a lot aims at
//!   the place in it that its height has among the ring's lots, give or
//!   take one for variety.
//!
//! A building is drawn no bigger than its lot holds whichever way it turns:
//! at its catalogue size, or smaller in the lot layer's quarter-octave
//! steps down to [`RING_SCALE_MIN`], and an entry too big even then gives
//! way to the next smaller one.
//!
//! Seen from a kilometre off a building keeps its shape and loses what
//! only matters up close, or does something: its sounds, particles, signs,
//! portals and gateways ([`strip_for_distance`]).
//!
//! # Near and far
//!
//! A kilometre of central Berlin is about 4,000 buildings, and a catalogue
//! building is from a handful to hundreds of parts. In a browser every part
//! costs CPU each frame (see `world_builder::draw_distance`), and the whole
//! ring at full detail would be ~16M triangles. So a building is drawn in
//! one of two forms ([`merge_template`]):
//!
//! - **Near** - within [`NEAR_RING_M`] of the walls, while the copies stay
//!   inside [`RING_ENTITY_BUDGET`] entities: the building itself, its parts
//!   merged into one mesh per material, a few entities a copy.
//! - **Far** - past that: a far form of the same building, its shape filled
//!   into voxels [`FAR_VOXEL_M`] a side and its outer faces merged, each in
//!   its parts' colours - a few hundred triangles, at most 2,048, fewer in
//!   one colour past that - one entity a copy that casts no shadow, at most
//!   [`MAX_FAR_COPIES`] of them.
//!
//! Copies of one building share its meshes, so the ring's memory is its
//! distinct buildings', not its copies'.
//!
//! # Spawning
//!
//! The buildings hang under one root on the terrain, so they go with it,
//! and are spawned as a remote avatar's visuals are: no collider, no editor
//! marker, no room entity. The work runs a slice a frame,
//! [`RING_SLICE_MS`] and a few steps of each stage at most, in three
//! stages ([`spawn_ring_buildings`]):
//!
//! 1. **Templates**: each distinct building - an entry at a drawn scale -
//!    is grown once and spawned whole, hidden.
//! 2. **Merging**: each template's parts are baked into its two forms and
//!    the template despawned. A building whose parts cannot all be merged
//!    per material - one drawn with a material that is not a standard one -
//!    keeps its tree for its near copies, which are spawned whole.
//! 3. **Copies**: every lot's copy, nearest the walls first.
//!
//! None of it is in the record, and none of it is saved.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::MeshAabb;
use bevy::math::{Affine3A, Mat3};
use bevy::mesh::{Indices, MeshVertexAttributeId, PrimitiveTopology, VertexAttributeValues};
use bevy::platform::time::Instant;
use bevy::prelude::*;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use crate::catalogue::{CatalogueEntry, StructureRole, entries_for};
use crate::pds::audio::SovereignAudioConfig;
use crate::pds::{Generator, GeneratorKind};
use crate::player::visuals::AvatarSpawnDeps;
use crate::seeded_defaults::{SceneCharacter, fnv1a_64};
use crate::state::CurrentRoomDid;
use crate::world_builder::avatar_spawn::{detached_record, spawn_detached_tree};

use super::geo::ring::RingLot;
use super::lots::{
    FALLBACK_THEME, FOUNDATION_SINK_M, fitted_scale, grow_generator, pool_for, scale_e4,
};
use super::{FinishedHeightMap, TerrainMesh};

/// How high over the ground Berlin's building on a lot must rise for the
/// lot to take a landmark (m): past the city's eaves, which stand at about
/// 22 m, and past the museums and offices of its centre.
pub(crate) const LANDMARK_STANDING_M: f32 = 40.0;

/// How far apart two landmarks of the ring stand at least (m): a cluster of
/// high-rises takes one.
pub(crate) const LANDMARK_SPACING_M: f32 = 200.0;

/// The most landmarks the ring draws.
pub(crate) const MAX_RING_LANDMARKS: usize = 8;

/// The smallest a ring building is drawn, as a share of its catalogue size.
pub(crate) const RING_SCALE_MIN: f32 = 0.5;

/// How far past the walls a lot's building is drawn near (m).
pub(crate) const NEAR_RING_M: f32 = 200.0;

/// The most entities the near copies may be: a near copy past it is drawn
/// far. A merged copy is a few entities, one per material; a copy spawned
/// whole is one per part.
pub(crate) const RING_ENTITY_BUDGET: u32 = 9_000;

/// The most far copies the ring draws, one entity each.
pub(crate) const MAX_FAR_COPIES: usize = 4_000;

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
const FAR_FALLBACK_COLOUR: [f32; 4] = [0.5, 0.5, 0.5, 1.0];

/// How long the ring may spend spawning in one frame (ms).
const RING_SLICE_MS: f64 = 4.0;

/// The most of each stage's steps one frame takes, however little time
/// they took: a step runs to its end once begun, and the spawns a step
/// queues are applied after the slice's clock has stopped. A template is a
/// whole catalogue tree, a bake of a large landmark the longest single
/// step, and a copy a few spawns.
const TEMPLATES_PER_SLICE: usize = 2;
const BAKES_PER_SLICE: usize = 1;
const COPIES_PER_SLICE: usize = 256;

/// The salt of the ring's own random stream.
const RING_STREAM_SALT: u64 = 0x5249_4E47_B011_D1E5;

/// The prefix of the cache key a ring building's template files under. A
/// record's generator key holds no `/`, so no record generator files there.
const RING_KEY_PREFIX: &str = "ring/";

/// The root every ring building hangs under: a child of the terrain.
#[derive(Component)]
pub(crate) struct RingRoot;

/// One distinct building of the ring: a catalogue entry at a drawn scale.
pub(crate) struct RingBuilding {
    pub entry: &'static dyn CatalogueEntry,
    pub scale: f32,
    /// The cache key its tree's caches file under.
    pub key: String,
    /// Its generator, grown for its template.
    tree: Option<Generator>,
    /// Its template, spawned hidden to be merged, until it is.
    template: Option<Entity>,
    /// How its near copies are drawn, once its template is merged.
    form: Option<Form>,
    /// Its far form, once its template is merged; `None` where it has none.
    far: Option<Handle<Mesh>>,
}

/// How a ring building's near copies are drawn.
enum Form {
    /// One mesh per material, in the copy's frame.
    Merged(Vec<(Handle<Mesh>, Handle<StandardMaterial>)>),
    /// Spawned whole from its tree, each copy costing what its first did.
    Whole(Option<u32>),
}

/// One lot's copy of a ring building.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RingCopy {
    /// Which of the draw's buildings.
    pub building: usize,
    /// Where it stands on the ground.
    pub pose: Transform,
    /// Whether its lot is within [`NEAR_RING_M`] of the walls.
    pub near: bool,
}

/// The ring's buildings as drawn: each distinct building once, and where
/// each copy stands, nearest the walls first.
pub(crate) struct RingDraw {
    pub buildings: Vec<RingBuilding>,
    pub copies: Vec<RingCopy>,
    /// What the buildings are grown for: the room's DID, its prosperity and
    /// escalation, and the ring's seed.
    did: String,
    character: (f32, f32),
    seed: u64,
}

impl RingDraw {
    /// The generator of building `b`, grown on first asking: the lot
    /// layer's, stripped for distance.
    fn tree(&mut self, b: usize) -> &Generator {
        let (did, character, seed) = (&self.did, self.character, self.seed);
        let building = &mut self.buildings[b];
        building.tree.get_or_insert_with(|| {
            let slug = building.entry.slug();
            let mut tree = grow_generator(
                building.entry,
                did,
                seed ^ fnv1a_64(slug),
                character,
                building.scale,
            );
            strip_for_distance(&mut tree);
            tree
        })
    }
}

/// Draw the ring's buildings on `lots` (see the module docs), for the room
/// `did`, standing on `ground` - the ground's height at a world point.
pub(crate) fn draw_ring(lots: &[RingLot], did: &str, ground: &dyn Fn(f32, f32) -> f32) -> RingDraw {
    let scene = SceneCharacter::for_did(did);
    // A theme with no landmark yet borrows the settlements' fallback, as
    // the lot layer does.
    let theme = if entries_for(scene.theme, StructureRole::Landmark)
        .next()
        .is_some()
    {
        scene.theme
    } else {
        FALLBACK_THEME
    };
    let character = (scene.prosperity, scene.escalation);
    let landmarks = pool_for(theme, StructureRole::Landmark, character.0, character.1);
    let mut secondaries = pool_for(theme, StructureRole::Secondary, character.0, character.1);
    secondaries.sort_by(|a, b| {
        radius(*a)
            .total_cmp(&radius(*b))
            .then(a.slug().cmp(b.slug()))
    });

    let landmark_lots = landmark_lots(lots);
    // Each other lot's place by height among the other lots, in (0, 1).
    let mut others: Vec<usize> = (0..lots.len()).filter(|&i| !landmark_lots[i]).collect();
    others.sort_by(|&a, &b| {
        lots[a]
            .standing
            .total_cmp(&lots[b].standing)
            .then(a.cmp(&b))
    });
    let mut rank = vec![0.0; lots.len()];
    for (place, &i) in others.iter().enumerate() {
        rank[i] = (place as f32 + 0.5) / others.len() as f32;
    }

    let seed = fnv1a_64(did) ^ RING_STREAM_SALT;
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut draw = RingDraw {
        buildings: Vec::new(),
        copies: Vec::new(),
        did: did.to_owned(),
        character,
        seed,
    };
    let mut by_key: HashMap<(&'static str, i64), usize> = HashMap::new();
    for (i, lot) in lots.iter().enumerate() {
        let landmark = if landmark_lots[i] {
            pick_landmark(&landmarks, lot, &mut rng)
        } else {
            None
        };
        let Some((entry, scale)) =
            landmark.or_else(|| pick_secondary(&secondaries, lot, rank[i], &mut rng))
        else {
            continue;
        };
        let key = (entry.slug(), scale_e4(scale));
        let building = *by_key.entry(key).or_insert_with(|| {
            draw.buildings.push(RingBuilding {
                entry,
                scale,
                key: ring_key(key.0, key.1),
                tree: None,
                template: None,
                form: None,
                far: None,
            });
            draw.buildings.len() - 1
        });
        let y = footing(lot, radius(entry) * scale, ground) - FOUNDATION_SINK_M;
        let pose =
            Transform::from_xyz(lot.x, y, lot.z).with_rotation(Quat::from_rotation_y(lot.yaw));
        draw.copies.push(RingCopy {
            building,
            pose,
            near: lot.beyond <= NEAR_RING_M,
        });
    }
    draw
}

/// Which lots take a landmark: the tallest, tallest first, at least
/// [`LANDMARK_STANDING_M`] high and [`LANDMARK_SPACING_M`] from every
/// landmark taken before, at most [`MAX_RING_LANDMARKS`].
fn landmark_lots(lots: &[RingLot]) -> Vec<bool> {
    let mut tall: Vec<usize> = (0..lots.len())
        .filter(|&i| lots[i].standing >= LANDMARK_STANDING_M)
        .collect();
    tall.sort_by(|&a, &b| {
        lots[b]
            .standing
            .total_cmp(&lots[a].standing)
            .then(a.cmp(&b))
    });
    let mut taken: Vec<usize> = Vec::new();
    for i in tall {
        if taken.len() == MAX_RING_LANDMARKS {
            break;
        }
        let apart = taken.iter().all(|&j| {
            let (dx, dz) = (lots[i].x - lots[j].x, lots[i].z - lots[j].z);
            dx * dx + dz * dz >= LANDMARK_SPACING_M * LANDMARK_SPACING_M
        });
        if apart {
            taken.push(i);
        }
    }
    let mut landmark = vec![false; lots.len()];
    for i in taken {
        landmark[i] = true;
    }
    landmark
}

/// A landmark for `lot`: any of the pool's that fits it.
fn pick_landmark(
    pool: &[&'static dyn CatalogueEntry],
    lot: &RingLot,
    rng: &mut ChaCha8Rng,
) -> Option<(&'static dyn CatalogueEntry, f32)> {
    let fitting: Vec<_> = pool
        .iter()
        .filter_map(|&entry| fitted(entry, lot).map(|scale| (entry, scale)))
        .collect();
    (!fitting.is_empty()).then(|| fitting[rng.next_u32() as usize % fitting.len()])
}

/// A secondary building for `lot` from `pool`, smallest first: the one at
/// `rank` of the way up, a step either way, or the biggest smaller one
/// that fits the lot.
fn pick_secondary(
    pool: &[&'static dyn CatalogueEntry],
    lot: &RingLot,
    rank: f32,
    rng: &mut ChaCha8Rng,
) -> Option<(&'static dyn CatalogueEntry, f32)> {
    let last = pool.len().checked_sub(1)?;
    let aim = ((rank * pool.len() as f32) as usize).min(last);
    let aim = match rng.next_u32() % 3 {
        0 => aim.saturating_sub(1),
        1 => aim,
        _ => (aim + 1).min(last),
    };
    pool[..=aim]
        .iter()
        .rev()
        .find_map(|&entry| fitted(entry, lot).map(|scale| (entry, scale)))
}

/// The scale `entry` is drawn at on `lot` - its lot fit rounded down to a
/// quarter-octave, at most its catalogue size - or `None` where it outgrows
/// the lot even at [`RING_SCALE_MIN`].
fn fitted(entry: &dyn CatalogueEntry, lot: &RingLot) -> Option<f32> {
    let reach = radius(entry);
    let scale = fitted_scale(lot.room / reach, RING_SCALE_MIN, 1.0);
    (scale * reach <= lot.room).then_some(scale)
}

/// How far from its anchor an entry reaches at its catalogue size, turned
/// any way: its clearance, or the corner of its own half side where that
/// is nearer (an entry whose clearance is a spacing circle declares one).
fn radius(entry: &dyn CatalogueEntry) -> f32 {
    entry
        .footprint()
        .clearance
        .min(entry.lot_half_width() * std::f32::consts::SQRT_2)
        .max(0.5)
}

/// The lowest ground under a building reaching `reach` from `lot`'s
/// anchor: its anchor and the corners of the square inside its reach. A
/// foundation then bites into a slope rather than float over it.
fn footing(lot: &RingLot, reach: f32, ground: &dyn Fn(f32, f32) -> f32) -> f32 {
    let corner = reach * std::f32::consts::FRAC_1_SQRT_2;
    [
        (0.0, 0.0),
        (-1.0, -1.0),
        (1.0, -1.0),
        (-1.0, 1.0),
        (1.0, 1.0),
    ]
    .into_iter()
    .map(|(sx, sz)| ground(lot.x + sx * corner, lot.z + sz * corner))
    .fold(f32::INFINITY, f32::min)
}

/// The cache key the tree of `slug` drawn at `scale_e4` files under.
fn ring_key(slug: &str, scale_e4: i64) -> String {
    format!("{RING_KEY_PREFIX}{slug}@{scale_e4}")
}

/// Strip from a building what a kilometre off only costs: every sound, and
/// every node that is no geometry to see from there - particles, signs,
/// portals, gateways, and any water, terrain or road a tree should not
/// carry.
pub(crate) fn strip_for_distance(tree: &mut Generator) {
    tree.audio = SovereignAudioConfig::None;
    tree.children.retain(|child| kept_afar(&child.kind));
    for child in &mut tree.children {
        strip_for_distance(child);
    }
}

/// Whether a node of this kind stays on a ring building.
fn kept_afar(kind: &GeneratorKind) -> bool {
    !matches!(
        kind,
        GeneratorKind::Terrain(_)
            | GeneratorKind::Water { .. }
            | GeneratorKind::RoadNetwork(_)
            | GeneratorKind::Portal { .. }
            | GeneratorKind::Gateway { .. }
            | GeneratorKind::ParticleSystem(_)
            | GeneratorKind::Sign { .. }
            | GeneratorKind::Unknown
    )
}

/// The parts of a spawned tree, as [`merge_template`] reads them.
type PartQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Transform,
        Option<&'static Visibility>,
        Option<&'static Mesh3d>,
        Option<&'static MeshMaterial3d<StandardMaterial>>,
        Option<&'static Children>,
    ),
>;

/// A template baked into the two forms its copies are drawn in (see the
/// module docs).
pub(crate) struct Baked {
    /// One mesh per material; `None` where a part is drawn with a material
    /// that is not a [`StandardMaterial`].
    pub near: Option<Vec<(Mesh, Handle<StandardMaterial>)>>,
    /// The far form: one vertex-coloured mesh; `None` where no part has
    /// the positions and normals one needs.
    pub far: Option<Mesh>,
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
/// `None` where the template cannot be baked: a mesh that is not a
/// triangle list or whose data has left for the GPU, or a part missing.
pub(crate) fn merge_template(
    template: Entity,
    parts: &PartQuery,
    meshes: &Assets<Mesh>,
    materials: &Assets<StandardMaterial>,
) -> Option<Baked> {
    let mut placed_parts: Vec<(Mesh, Option<Handle<StandardMaterial>>)> = Vec::new();
    let (.., children) = parts.get(template).ok()?;
    let mut stack: Vec<(Entity, Affine3A)> = children
        .map(|c| c.iter().map(|e| (e, Affine3A::IDENTITY)).collect())
        .unwrap_or_default();
    while let Some((entity, above)) = stack.pop() {
        let (transform, visibility, mesh, material, children) = parts.get(entity).ok()?;
        if visibility == Some(&Visibility::Hidden) {
            continue;
        }
        let affine = above * transform.compute_affine();
        if let Some(mesh) = mesh {
            let part = placed(meshes.get(&mesh.0)?, affine)?;
            placed_parts.push((part, material.map(|m| m.0.clone())));
        }
        if let Some(children) = children {
            stack.extend(children.iter().map(|child| (child, affine)));
        }
    }
    Some(Baked {
        near: near_form(&placed_parts),
        far: far_form(&placed_parts, materials),
    })
}

/// The parts grouped into one mesh per material, or `None` where one is
/// not a [`StandardMaterial`] or two of a material do not merge.
fn near_form(
    parts: &[(Mesh, Option<Handle<StandardMaterial>>)],
) -> Option<Vec<(Mesh, Handle<StandardMaterial>)>> {
    let mut groups: Vec<(Handle<StandardMaterial>, Vec<AttributeKind>, Mesh)> = Vec::new();
    for (part, material) in parts {
        let material = material.as_ref()?;
        let kinds = attribute_kinds(part)?;
        match groups
            .iter_mut()
            .find(|(m, k, _)| m.id() == material.id() && *k == kinds)
        {
            Some((.., merged)) => merged.merge(part).ok()?,
            None => groups.push((material.clone(), kinds, part.clone())),
        }
    }
    Some(
        groups
            .into_iter()
            .map(|(material, _, mesh)| (mesh, material))
            .collect(),
    )
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
fn far_form(
    parts: &[(Mesh, Option<Handle<StandardMaterial>>)],
    materials: &Assets<StandardMaterial>,
) -> Option<Mesh> {
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
        .filter_map(|(mesh, material)| {
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

/// The ring being spawned onto the terrain it was drawn for.
#[derive(Resource)]
pub(crate) struct RingBuild {
    /// The terrain root the ring belongs to.
    terrain: Entity,
    /// The [`RingRoot`] the buildings hang under.
    root: Entity,
    draw: RingDraw,
    stage: Stage,
    /// The one material every far form is drawn with, its colour the
    /// form's own.
    far_material: Option<Handle<StandardMaterial>>,
    /// The entities the near copies have spawned so far.
    spawned: u32,
    /// The far copies spawned so far.
    far_copies: usize,
}

impl RingBuild {
    fn new(terrain: Entity, root: Entity, draw: RingDraw) -> Self {
        RingBuild {
            terrain,
            root,
            draw,
            stage: Stage::Templates(0),
            far_material: None,
            spawned: 0,
            far_copies: 0,
        }
    }
}

/// Where a [`RingBuild`] has got to (see the module docs), each with the
/// next index it works on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Templates(usize),
    Merging(usize),
    Copies(usize),
}

/// When a terrain lands, draw its ring - if its ground has one - and start
/// spawning it. A new terrain ends the ring of the one before, spawned or
/// not: that ring goes with its terrain.
pub(super) fn start_ring_buildings(
    mut commands: Commands,
    added: Query<Entity, Added<TerrainMesh>>,
    heightmap: Option<Res<FinishedHeightMap>>,
    did: Option<Res<CurrentRoomDid>>,
) {
    let Some(terrain) = added.iter().last() else {
        return;
    };
    commands.remove_resource::<RingBuild>();
    let Some(heightmap) = heightmap else {
        return;
    };
    let Some(ring) = heightmap.ground().and_then(|ground| ground.ring()) else {
        return;
    };
    let did = did.as_deref().map_or("", |did| did.0.as_str());
    let draw = draw_ring(ring.lots(), did, &|x, z| heightmap.view_height_at(x, z));
    if draw.copies.is_empty() {
        return;
    }
    let root = commands
        .spawn((
            RingRoot,
            Transform::IDENTITY,
            Visibility::default(),
            ChildOf(terrain),
        ))
        .id();
    commands.insert_resource(RingBuild::new(terrain, root, draw));
}

/// Work the ring's stages (see the module docs) for at most
/// [`RING_SLICE_MS`] a frame. A terrain going out, or gone, takes its ring
/// with it, so the work stops there too.
///
/// A stage ends a frame's slice when it is done, so the next stage reads
/// the world the last one's commands made: the merge reads the templates'
/// parts, which their spawn only queued.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_ring_buildings(
    mut commands: Commands,
    mut build: ResMut<RingBuild>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut deps: AvatarSpawnDeps,
    parts: PartQuery,
) {
    // `terrain_meshes` is the terrain that is neither gone nor going.
    if !deps.terrain_meshes.contains(build.terrain) {
        commands.remove_resource::<RingBuild>();
        return;
    }
    let started = Instant::now();
    let out_of_time = || started.elapsed().as_secs_f64() * 1_000.0 >= RING_SLICE_MS;
    let build = &mut *build;
    // This frame's steps, against the stage's cap.
    let mut steps = 0usize;
    loop {
        match build.stage {
            Stage::Templates(b) if b < build.draw.buildings.len() => {
                if out_of_time() || steps == TEMPLATES_PER_SLICE {
                    return;
                }
                steps += 1;
                let template = commands
                    .spawn((Transform::IDENTITY, Visibility::Hidden, ChildOf(build.root)))
                    .id();
                spawn_whole(
                    &mut commands,
                    &mut build.draw,
                    b,
                    (template, Transform::IDENTITY),
                    (&mut meshes, &mut materials, &mut images),
                    &mut deps,
                );
                build.draw.buildings[b].template = Some(template);
                build.stage = Stage::Templates(b + 1);
            }
            Stage::Templates(_) => {
                build.stage = Stage::Merging(0);
                return;
            }
            Stage::Merging(b) if b < build.draw.buildings.len() => {
                if out_of_time() || steps == BAKES_PER_SLICE {
                    return;
                }
                steps += 1;
                let building = &mut build.draw.buildings[b];
                let template = building.template.take();
                let baked = template.and_then(|t| merge_template(t, &parts, &meshes, &materials));
                let (near, far) = baked.map_or((None, None), |baked| (baked.near, baked.far));
                // Drawn, never read again: no CPU copy.
                let mut add = |mut mesh: Mesh| {
                    mesh.asset_usage = RenderAssetUsages::RENDER_WORLD;
                    meshes.add(mesh)
                };
                building.form = Some(match near {
                    Some(near) => Form::Merged(
                        near.into_iter()
                            .map(|(mesh, material)| (add(mesh), material))
                            .collect(),
                    ),
                    None => {
                        warn!(
                            "geodata ring: {} is drawn whole near, its parts not merged",
                            building.key
                        );
                        Form::Whole(None)
                    }
                });
                building.far = far.map(&mut add);
                if let Some(template) = template {
                    commands.entity(template).despawn();
                }
                build.stage = Stage::Merging(b + 1);
            }
            Stage::Merging(_) => {
                build.stage = Stage::Copies(0);
                return;
            }
            Stage::Copies(c) if c < build.draw.copies.len() => {
                if out_of_time() || steps == COPIES_PER_SLICE {
                    return;
                }
                steps += 1;
                build.stage = Stage::Copies(c + 1);
                let RingCopy {
                    building: b,
                    pose,
                    near,
                } = build.draw.copies[c];
                let near_cost = match &build.draw.buildings[b].form {
                    Some(Form::Merged(merged)) => merged.len() as u32,
                    Some(Form::Whole(cost)) => cost.unwrap_or(0),
                    None => u32::MAX,
                };
                if near && build.spawned.saturating_add(near_cost) <= RING_ENTITY_BUDGET {
                    let spent = match &build.draw.buildings[b].form {
                        Some(Form::Merged(merged)) => {
                            for (mesh, material) in merged {
                                commands.spawn((
                                    Mesh3d(mesh.clone()),
                                    MeshMaterial3d(material.clone()),
                                    pose,
                                    ChildOf(build.root),
                                ));
                            }
                            near_cost
                        }
                        _ => {
                            let (_, spent) = spawn_whole(
                                &mut commands,
                                &mut build.draw,
                                b,
                                (build.root, pose),
                                (&mut meshes, &mut materials, &mut images),
                                &mut deps,
                            );
                            build.draw.buildings[b].form = Some(Form::Whole(Some(spent)));
                            spent
                        }
                    };
                    build.spawned += spent;
                } else if build.far_copies < MAX_FAR_COPIES
                    && let Some(far) = build.draw.buildings[b].far.clone()
                {
                    let material = build
                        .far_material
                        .get_or_insert_with(|| materials.add(far_material()))
                        .clone();
                    commands.spawn((
                        Mesh3d(far),
                        MeshMaterial3d(material),
                        pose,
                        bevy::light::NotShadowCaster,
                        ChildOf(build.root),
                    ));
                    build.far_copies += 1;
                }
            }
            Stage::Copies(_) => break,
        }
    }
    info!(
        "geodata ring: {} near entities, {} far copies, of {} lots from {} distinct buildings",
        build.spawned,
        build.far_copies,
        build.draw.copies.len(),
        build.draw.buildings.len()
    );
    commands.remove_resource::<RingBuild>();
}

/// The material every far form is drawn with: white, so each part shows
/// its vertex colour, and matte, as a city a kilometre off is.
fn far_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.9,
        reflectance: 0.3,
        ..default()
    }
}

/// Spawn building `b` of `draw` whole from its tree, under `parent` at
/// `pose`: its template, or a copy that could not be merged. Answers the
/// tree's root and how many entities it spawned.
fn spawn_whole(
    commands: &mut Commands,
    draw: &mut RingDraw,
    b: usize,
    (parent, pose): (Entity, Transform),
    (meshes, materials, images): (
        &mut Assets<Mesh>,
        &mut Assets<StandardMaterial>,
        &mut Assets<Image>,
    ),
    deps: &mut AvatarSpawnDeps,
) -> (Option<Entity>, u32) {
    let key = draw.buildings[b].key.clone();
    let tree = draw.tree(b);
    spawn_detached_tree(
        commands,
        parent,
        tree,
        &key,
        pose * Transform::from(&tree.transform),
        meshes,
        materials,
        deps.water_materials.as_mut(),
        images,
        deps.palette.as_deref(),
        deps.heightmap.as_deref(),
        &deps.terrain_meshes,
        &mut deps.caches,
        deps.blob_image_cache.as_mut(),
        deps.blob_audio_cache.as_mut(),
        deps.water_surfaces.as_mut(),
        detached_record(),
        deps.current_room.as_deref(),
        false,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_defaults::ThemeArchetype;
    use crate::terrain::OutgoingTerrain;
    use crate::water::{WaterMaterial, WaterSurfaces};
    use bevy::ecs::system::RunSystemOnce;

    /// A lot at `(x, z)`, `beyond` past the walls, with the whole of its
    /// 30 m to itself, Berlin's building on it `standing` high.
    fn lot(x: f32, z: f32, standing: f32, beyond: f32) -> RingLot {
        RingLot {
            x,
            z,
            yaw: 0.0,
            room: 15.0,
            standing,
            beyond,
        }
    }

    /// A DID whose room's theme is `theme`.
    fn did_of(theme: ThemeArchetype) -> String {
        (0..10_000)
            .map(|i| format!("did:plc:ring{i}"))
            .find(|did| SceneCharacter::for_did(did).theme == theme)
            .expect("a DID of the theme")
    }

    #[test]
    fn a_ring_building_loses_its_sounds_and_what_is_not_seen_from_afar() {
        let hum = || SovereignAudioConfig::Patch {
            patch: Default::default(),
        };
        let mut tree = Generator::default_cuboid();
        tree.audio = hum();
        let mut wall = Generator::default_cuboid();
        wall.children
            .push(Generator::from_kind(GeneratorKind::default_particles()));
        wall.audio = hum();
        tree.children.push(wall);
        tree.children
            .push(Generator::from_kind(GeneratorKind::default_particles()));
        strip_for_distance(&mut tree);
        assert!(tree.audio.is_none());
        assert_eq!(tree.children.len(), 1, "the wall stays, the sparks go");
        assert!(tree.children[0].children.is_empty() && tree.children[0].audio.is_none());
    }

    #[test]
    fn every_lot_takes_a_building_that_fits_it_and_the_tallest_take_landmarks() {
        let did = did_of(ThemeArchetype::ModernCity);
        // Two rows of lots; four stand tall, two of them within a landmark's
        // spacing of each other.
        let mut lots: Vec<RingLot> = (0..40)
            .map(|i| {
                let x = (i % 20) as f32 * 30.0;
                lot(x, (i / 20) as f32 * 30.0, 10.0 + (i % 7) as f32, x)
            })
            .collect();
        for (i, standing) in [(0, 80.0), (1, 70.0), (15, 60.0), (39, 45.0)] {
            lots[i].standing = standing;
        }
        lots[7].room = 8.0;
        let ground = |x: f32, z: f32| 30.0 + 0.01 * x - 0.02 * z;
        let draw = draw_ring(&lots, &did, &ground);
        assert_eq!(draw.copies.len(), lots.len(), "a building on every lot");
        let landmarks: Vec<usize> = draw
            .copies
            .iter()
            .enumerate()
            .filter(|(_, c)| draw.buildings[c.building].entry.role() == StructureRole::Landmark)
            .map(|(i, _)| i)
            .collect();
        // Lot 1 is 30 m from lot 0, and lot 39 is 30 m from lot 15's row
        // mate but 360 m from lot 0.
        assert_eq!(landmarks, vec![0, 15], "the tallest, spaced");
        for (copy, lot) in draw.copies.iter().zip(&lots) {
            let building = &draw.buildings[copy.building];
            assert!(
                radius(building.entry) * building.scale <= lot.room,
                "{} at {} on a lot of room {}",
                building.entry.slug(),
                building.scale,
                lot.room
            );
            assert!((RING_SCALE_MIN..=1.0).contains(&building.scale));
            assert_eq!(
                (copy.pose.translation.x, copy.pose.translation.z),
                (lot.x, lot.z)
            );
            // The footing is the lowest ground under the building, sunk.
            let under = footing(lot, radius(building.entry) * building.scale, &ground);
            assert_eq!(copy.pose.translation.y, under - FOUNDATION_SINK_M);
            assert!(under <= ground(lot.x, lot.z));
            assert_eq!(copy.near, lot.beyond <= NEAR_RING_M);
        }
        // One building per entry and scale, shared by its copies.
        assert!(draw.buildings.len() < draw.copies.len());
        let mut keys: Vec<&str> = draw.buildings.iter().map(|b| b.key.as_str()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), draw.buildings.len());
        // And the same room draws the same ring.
        let again = draw_ring(&lots, &did, &ground);
        let picks = |d: &RingDraw| -> Vec<(String, RingCopy)> {
            d.copies
                .iter()
                .map(|c| (d.buildings[c.building].key.clone(), *c))
                .collect()
        };
        assert_eq!(picks(&draw), picks(&again));
    }

    #[test]
    fn a_taller_lot_takes_a_bigger_building() {
        let did = did_of(ThemeArchetype::ModernCity);
        let lots: Vec<RingLot> = (0..300)
            .map(|i| {
                let x = (i % 20) as f32 * 30.0;
                lot(x, (i / 20) as f32 * 30.0, 3.0 + 0.1 * i as f32, x)
            })
            .collect();
        let draw = draw_ring(&lots, &did, &|_, _| 0.0);
        let reach = |range: std::ops::Range<usize>| {
            let n = range.len() as f32;
            draw.copies[range]
                .iter()
                .map(|c| radius(draw.buildings[c.building].entry))
                .sum::<f32>()
                / n
        };
        assert!(
            reach(200..300) > reach(0..100),
            "{} > {}",
            reach(200..300),
            reach(0..100)
        );
    }

    /// The positions of `mesh`.
    fn positions(mesh: &Mesh) -> Vec<Vec3> {
        match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(p)) => {
                p.iter().map(|v| Vec3::from_array(*v)).collect()
            }
            _ => panic!("positions"),
        }
    }

    /// A world with the two asset stores a merge reads, and `template`'s
    /// parts baked.
    fn bake(world: &mut World, template: Entity) -> Option<Baked> {
        world
            .run_system_once(
                move |parts: PartQuery,
                      meshes: Res<Assets<Mesh>>,
                      materials: Res<Assets<StandardMaterial>>| {
                    merge_template(template, &parts, &meshes, &materials)
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
        let (stone_mesh, _) = near.iter().find(|(_, m)| *m == stone).unwrap();
        let (glass_mesh, _) = near.iter().find(|(_, m)| *m == glass).unwrap();
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
    /// lots spawned an entity drawing nothing, counted against the cap.
    #[test]
    fn a_building_that_draws_nothing_has_no_far_form() {
        let mut world = asset_world();
        let template = world.spawn((Transform::IDENTITY, Visibility::Hidden)).id();
        let baked = bake(&mut world, template).expect("an empty template bakes");
        assert!(baked.far.is_none());
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

    /// The resources the spawner reaches, as the compile's own test app has
    /// them, with no renderer.
    fn ring_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()));
        app.init_asset::<Mesh>();
        app.init_asset::<Image>();
        app.init_asset::<StandardMaterial>();
        app.init_asset::<WaterMaterial>();
        app.init_resource::<crate::world_builder::LSystemMaterialCache>();
        app.init_resource::<crate::world_builder::LSystemMeshCache>();
        app.init_resource::<crate::world_builder::ShapeMaterialCache>();
        app.init_resource::<crate::world_builder::ShapeMeshCache>();
        app.init_resource::<crate::world_builder::prim_cache::PrimMeshCache>();
        app.init_resource::<crate::world_builder::prim_cache::PrimMaterialCache>();
        app.init_resource::<bevy_symbios_shape::cache::ShapeMeshCache>();
        app.init_resource::<crate::world_builder::spatial_audio::BakedAudioCache>();
        app.insert_resource(crate::world_builder::fresh_texture_cache());
        app.init_resource::<crate::world_builder::compile::CompiledWorld>();
        app.init_resource::<crate::world_builder::compile::CompileJob>();
        app.init_resource::<WaterSurfaces>();
        app.init_resource::<crate::world_builder::image_cache::BlobImageCache>();
        app.init_resource::<crate::world_builder::audio_resolver::BlobAudioCache>();
        app.init_resource::<crate::diagnostics::SessionLog>();
        app.add_systems(
            Update,
            spawn_ring_buildings.run_if(resource_exists::<RingBuild>),
        );
        app
    }

    /// `lots` drawn for a room of `theme` and handed to the spawner, on a
    /// terrain of its own: the terrain and the ring's root.
    fn building(app: &mut App, theme: ThemeArchetype, lots: &[RingLot]) -> (Entity, Entity) {
        let world = app.world_mut();
        let terrain = world
            .spawn((TerrainMesh, Transform::IDENTITY, Visibility::default()))
            .id();
        let root = world
            .spawn((
                RingRoot,
                Transform::IDENTITY,
                Visibility::default(),
                ChildOf(terrain),
            ))
            .id();
        let draw = draw_ring(lots, &did_of(theme), &|_, _| 30.0);
        world.insert_resource(RingBuild::new(terrain, root, draw));
        (terrain, root)
    }

    fn run_to_done(app: &mut App) {
        for _ in 0..200 {
            app.update();
            if !app.world().contains_resource::<RingBuild>() {
                return;
            }
        }
        panic!("the ring is not done in 200 frames");
    }

    #[test]
    fn a_near_copy_is_its_merged_parts_and_a_far_one_its_far_form() {
        let mut app = ring_app();
        // Two rows: the near one at the walls, the far one well past.
        let lots: Vec<RingLot> = (0..24)
            .map(|i| {
                let beyond = if i < 12 { 20.0 } else { NEAR_RING_M + 300.0 };
                lot((i % 12) as f32 * 30.0, beyond, 12.0 + i as f32, beyond)
            })
            .collect();
        let (_, root) = building(&mut app, ThemeArchetype::ModernCity, &lots);
        run_to_done(&mut app);
        let world = app.world_mut();
        let children: Vec<Entity> = world
            .get::<Children>(root)
            .expect("the ring has buildings")
            .iter()
            .collect();
        let (mut near, mut far) = (Vec::new(), Vec::new());
        for child in &children {
            assert!(
                world.get::<Children>(*child).is_none(),
                "a part, not a template"
            );
            assert!(world.get::<Mesh3d>(*child).is_some());
            let t = world.get::<Transform>(*child).unwrap().translation;
            if world.get::<bevy::light::NotShadowCaster>(*child).is_some() {
                far.push(t);
            } else {
                near.push(t);
            }
        }
        // Every far lot is one entity; every near lot a handful.
        assert_eq!(far.len(), 12);
        assert!(far.iter().all(|t| t.z == NEAR_RING_M + 300.0));
        let near_lots: std::collections::BTreeSet<i32> = near.iter().map(|t| t.x as i32).collect();
        assert_eq!(near_lots.len(), 12, "a building on every near lot");
        assert!(near.iter().all(|t| t.z == 20.0));
        assert!(
            near.len() > 12 && near.len() <= 12 * 16,
            "{} near parts",
            near.len()
        );
        // Far forms share one material, which colours each by its vertices.
        let far_materials: std::collections::BTreeSet<_> = children
            .iter()
            .filter(|c| world.get::<bevy::light::NotShadowCaster>(**c).is_some())
            .map(|c| {
                world
                    .get::<MeshMaterial3d<StandardMaterial>>(*c)
                    .unwrap()
                    .0
                    .id()
            })
            .collect();
        assert_eq!(far_materials.len(), 1);
        let meshes: std::collections::BTreeSet<_> = children
            .iter()
            .map(|c| world.get::<Mesh3d>(*c).unwrap().0.id())
            .collect();
        assert!(meshes.len() < children.len(), "copies share their meshes");
    }

    /// A terrain landing with a ring starts it: the ring's root hangs on
    /// that terrain. A terrain after it ends that build and starts its own,
    /// or none where its ground has no ring.
    #[test]
    fn a_landed_terrain_starts_its_ring_and_the_next_replaces_it() {
        use crate::terrain::geo::GeoGround;
        use crate::terrain::geo::ring::Ring;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, start_ring_buildings);
        let ground = |lots: Vec<RingLot>| {
            GeoGround::from_cover(8, 2.0, vec![None; 64], None).with_ring(Ring::from_lots(lots))
        };
        let land = |app: &mut App, ground: GeoGround| {
            let world = app.world_mut();
            world.insert_resource(FinishedHeightMap(
                bevy_symbios_ground::HeightMap::new(8, 8, 2.0),
                Some(ground),
            ));
            let terrain = world
                .spawn((TerrainMesh, Transform::IDENTITY, Visibility::default()))
                .id();
            app.update();
            terrain
        };
        app.insert_resource(CurrentRoomDid(did_of(ThemeArchetype::ModernCity)));
        let lots = vec![lot(40.0, 0.0, 20.0, 30.0), lot(70.0, 0.0, 20.0, 60.0)];
        let first = land(&mut app, ground(lots.clone()));
        let build = app.world().resource::<RingBuild>();
        assert_eq!(build.terrain, first);
        assert_eq!(build.draw.copies.len(), 2);
        let root = build.root;
        assert!(app.world().get::<RingRoot>(root).is_some());
        assert_eq!(
            app.world().get::<ChildOf>(root).map(ChildOf::parent),
            Some(first)
        );
        let second = land(&mut app, ground(lots));
        assert_eq!(app.world().resource::<RingBuild>().terrain, second);
        land(&mut app, ground(Vec::new()));
        assert!(
            !app.world().contains_resource::<RingBuild>(),
            "no ring, no build"
        );
    }

    #[test]
    fn a_terrain_going_out_stops_its_ring() {
        let mut app = ring_app();
        let lots = [lot(0.0, 0.0, 20.0, 0.0), lot(30.0, 0.0, 20.0, 0.0)];
        let (terrain, _) = building(&mut app, ThemeArchetype::ModernCity, &lots);
        app.world_mut().entity_mut(terrain).insert(OutgoingTerrain);
        app.update();
        assert!(!app.world().contains_resource::<RingBuild>());
    }
}
