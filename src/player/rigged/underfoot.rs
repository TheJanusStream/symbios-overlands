//! The ground under a beast's feet (#1833): the terrain's own, sampled into a
//! [`GroundPatch`], for a beast the physics has resting on the terrain and
//! nothing else, with whatever stands on the terrain within its reach read
//! in by rays.
//!
//! **Why a beast needs one.** The engine plants feet on whatever ground
//! [`Drive::ground`] answers, and without one on a level plane at the rigged
//! root's `y = 0`, the chassis collider's bottom. A beast's chassis is held
//! level in pitch and roll (`preset::Locks::Level`), so on a slope that plane
//! runs through the hill: its uphill feet stood in it and its downhill ones
//! over it, a quarter of a metre each way under a horse.
//!
//! **What it rests on is the physics' answer, not a height's.** The patch
//! knows the heightmap and nothing else, so it is handed over only while the
//! terrain is all the chassis rests on: its contacts for a chassis that is
//! simulated here ([`crate::player::resting_under`]), and for one that is
//! not, a peer played out rather than simulated and the render tool's walker,
//! the same capsule cast down onto the world ([`Underfoot::rest_of`]). A beast
//! on a floor, a pad, a road, a deck, a boat, a prop or another body keeps the
//! level floor its collider stands it on, as it always has. A height band
//! against the heightmap used to decide this, and passed a beast on a level
//! floor whose uphill edge meets a hillside - which is what every seeded
//! building pad is, set at the highest terrain under it and not flattening it
//! (`world_builder::compile::pad`) - and the patch then put its feet down
//! through the floor to the hill under it, 0.79 m under a horse on 20 degrees.
//! The band is not kept beside the physics as a second check: the heightmap is
//! not quite the collider ([`fill_terrain_ground`]), so on a twisted cell a
//! band would refuse a beast the collider holds and hand the patch over and
//! take it back under its feet, and it can say nothing the contacts do not.
//!
//! **What stands on the terrain is read by rays.** A beast on the terrain
//! beside a pad, a road ribbon, a curb or a rock reaches over it, and a foot
//! there belongs on it, not on the terrain inside it. So every static solid
//! that is not the terrain and has some of its shape in a box over the reach,
//! from the terrain to a little above the highest a foot may step, is
//! gathered, and each sample over one of them is read by a ray cast straight
//! down onto it from the top of that band. In open terrain nothing is
//! gathered and no ray is cast, and neither is one between the streets of a
//! road network whose one trimesh's bounds span the district (#1833 review,
//! finding 2: gathered by bounds, every sample there was rayed against it).
//! A surface a ray finds over open space - a low table or a bench over the
//! body, its underside clear of the terrain - is not a floor: an up-ray from
//! the terrain meets its underside first, and the foot stays on the terrain
//! (finding 1, [`below`]).
//!
//! **A peer's cast is kept while it stands still.** Casting the capsule
//! against the heightfield costs about as much as the whole fill, every frame
//! for every peer beast, so a body that is not simulated here keeps the last
//! answer in its trail ([`RestCast`]) and casts again only once its drawn
//! pose has moved a centimetre or turned a degree, or every
//! [`RECAST_EVERY`] frames standing still (finding 3). A peer walking off the
//! terrain onto anything moves more than that a frame and is read again on
//! the frame it does.
//!
//! **Sampled through this frame's pose.** In `Update` a chassis' own
//! `GlobalTransform` is last frame's propagation - avian writes back only the
//! `Transform`, and its interpolation eases that before `Update` - while the
//! body is drawn from this frame's `Transform` after `PostUpdate`. A patch
//! sampled through the stale pose is the ground where the body was a frame
//! ago, about 5 cm into or over the ground at a slope break at a gallop. So
//! the fill hands this module the chassis' current `Transform` through the
//! rigged root's own, and leaves everything else it reads - [`Drive::at`]
//! among them, which every person's motion is measured against - as it was.

use avian3d::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_symbios_avatar::{Drive, GroundPatch};
use symbios_avatar::{FootingConfig, Rig};

use super::stands_on_four;
use crate::pds::avatar::locomotion::LyingCapsule;
use crate::terrain::{FinishedHeightMap, OutgoingTerrain, TerrainMesh};

/// Whether a person stands on the terrain's own ground as a beast does
/// (#1833), rather than on the level floor through its collider's bottom.
///
/// Off, the owner's default for now: a person's two feet are a hand apart,
/// so the level floor under an upright capsule is off a slope by a
/// centimetre or two, where a beast's four are a body's length apart and a
/// horse's were a quarter of a metre into a hill and over it. Turned on, a
/// person is filled exactly as a beast is ([`fill_terrain_ground`]).
pub(crate) const PEOPLE_STAND_ON_TERRAIN: bool = false;

/// The spacing of the ground handed to a body standing on the terrain,
/// metres (#1833): a quarter of the heightmap's own metre, so the patch
/// follows every fold of it, and about a foot's length, so no foot falls
/// between two samples and reads the ground as level under it.
const GROUND_STEP: f32 = 0.25;

/// How far above the highest a foot may step a ray down onto a structure
/// starts, metres (#1833). The highest is the engine's own
/// [`FootingConfig::max_step_up`] over the higher of the root's floor and
/// the terrain under the sample: a surface higher than that is not one a
/// foot is lifted onto, and a ray started inside a solid one says so
/// ([`Below::Inside`]) - a convex one; a triangle mesh is hollow to a ray
/// ([`below`]).
const RAY_HEADROOM: f32 = 0.05;

/// How much open space a structure must stand over, from the terrain to its
/// underside, for a foot under it to stay on the terrain, metres (#1833
/// review, finding 1): room for a paw or a hoof. A boardwalk a few
/// centimetres over the ground is stood on; a bench, a low table or a slab
/// over a cat's back is stood under.
const OPEN_UNDER: f32 = 0.1;

/// How near two hits on one structure along one vertical line are one
/// surface met from both sides rather than a top and an underside, metres
/// (#1833): a sheet - a road deck's trimesh, which has no thickness - or a
/// hollow trimesh whose inside the terrain point is in.
const ONE_SURFACE: f32 = 0.01;

/// How far beyond the terrain under a body's reach, below the lowest of it
/// and above where the highest ray starts, the box a structure is gathered
/// by reaches, metres (#1833 review, finding 2): the terrain is read at nine
/// points of the reach, and this takes in a fold between them.
const BAND_MARGIN: f32 = 0.25;

/// How far above where it stands a collider-less body's capsule is cast down
/// from, metres (#1833, [`Underfoot::rest_of`]): clear of the ground a body
/// played out a few centimetres into a slope is in, and short of a step.
const CAST_RISE: f32 = 0.25;

/// How far under where it stands a collider-less body's capsule is cast to,
/// metres (#1833 review, finding 5): well past the [`RESTING_WITHIN`] it may
/// fall and still rest, and no further, so a crowded spot far under it is
/// never visited.
const CAST_FALL: f32 = 1.0;

/// How far a peer's drawn pose moves, metres, or turns, radians, before what
/// it rests on is cast again rather than read off the last cast (#1833
/// review, finding 3): a centimetre, and a degree, which swings a horse's
/// capsule end about as far. A body walking moves two centimetres a frame.
const RECAST_MOVED: f32 = 0.01;
const RECAST_TURNED: f32 = 1.0 * std::f32::consts::PI / 180.0;

/// How many frames running a peer standing still reuses its last cast before
/// it is cast again anyway (#1833): what is under a body can change without
/// it moving - a prop set down, a terrain tile swapped - and this is how
/// long that may go unseen.
pub(super) const RECAST_EVERY: u32 = 8;

/// How far a collider-less body's capsule may fall from where it stands and
/// still be resting there, metres (#1833): the jump's own 5 cm of ground
/// under the feet (`humanoid::feet_on_the_ground`), which takes in a peer
/// played out a little over a slope between two of its samples. Further, it
/// is in the air.
const RESTING_WITHIN: f32 = 0.05;

/// How near the first thing under a cast capsule something else must be to
/// be under it too, metres (#1833): the contact graph's own reach
/// ([`crate::config::rover::BODY_TOUCH_M`]), so a peer is read as resting on
/// what its owner's physics has it touching. A capsule on a floor whose
/// uphill edge meets a hillside touches both within the solver's few
/// millimetres; one on the terrain whose end hangs 4 cm over a pad is not on
/// the pad.
const TOUCHING_WITHIN: f32 = crate::config::rover::BODY_TOUCH_M;

/// How far over where it stands a walker's capsule is cast down from to set
/// it down, metres (#1833, [`Underfoot::set_down`]): over the uphill end of a
/// horse on 30 degrees, and over a pad or a ribbon a step high, and short of
/// most of what a body walks under.
#[cfg(not(target_arch = "wasm32"))]
const SET_DOWN_RISE: f32 = 1.0;

/// How far under where it stands a walker's capsule is cast to set it down,
/// metres (#1833): a walker coming off anything set down wherever under it it
/// lands. The tool's cost, not a frame's.
#[cfg(not(target_arch = "wasm32"))]
const SET_DOWN_FALL: f32 = 100.0;

/// What a chassis rests on this frame, as the physics reads it (#1833).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestsOn {
    /// The terrain, and nothing else.
    Terrain,
    /// Anything else: a floor, a pad, a road, a deck, a body, the terrain
    /// together with any of those, or nothing at all.
    Elsewhere,
}

/// One static solid that is not the terrain, near a body standing on it
/// (#1833): what a ray from over a sample inside its bounds is cast onto.
pub(crate) struct Structure<'a> {
    /// Its bounds in the world.
    pub(crate) aabb: ColliderAabb,
    pub(crate) collider: &'a Collider,
    pub(crate) position: Vec3,
    pub(crate) rotation: Quat,
}

/// What a ray straight down from over a sample finds on the structures there.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Below {
    /// Nothing between its start and the terrain.
    Nothing,
    /// A surface at this world height.
    Surface(f32),
    /// The ray started inside one: something taller than a step - a wall, a
    /// trunk, a pad's downhill face - that no foot is lifted onto.
    Inside,
}

/// The highest surface on `structures` straight down from `from` and over
/// the `terrain` height under it, that a foot stands on rather than under.
///
/// **A surface over open space is not one** (#1833 review, finding 1). A
/// ray down from a step over the body meets the top of anything over it
/// lower than that - a bench, a low table, a slab over a cat's back - and a
/// foot under it was lifted onto it. So a structure whose top is high enough
/// over the terrain to stand clear of it is met again by a ray up from the
/// terrain there: one that meets its underside [`OPEN_UNDER`] or more over
/// the terrain and clearly under its top is over open space, and the foot
/// stays on the terrain. One that starts inside it (a solid standing on the
/// terrain, read where the ray starts), meets it lower, or meets the top
/// itself from under it (a sheet, or a trimesh the terrain point is inside:
/// [`ONE_SURFACE`]) has it standing on the terrain there.
///
/// **A triangle mesh is hollow to a ray** (finding 4, left as it is): parry
/// casts a trimesh's triangles and not its inside, so a ray down from inside
/// a closed one - an instance set's voxel shell, a road's curbs and skirts -
/// is never [`Below::Inside`] and meets its bottom instead, and the up-ray
/// meets the same face from under it. A sample inside a shell's walls reads
/// the shell's base where that stands over the terrain, by less than a step,
/// where a convex wall keeps the terrain there; and a sheet trimesh over open
/// space - a deck with no thickness - is stood on. Telling a sheet from a
/// shell would take the mesh's winding, which the double-sided road meshes
/// do not keep.
fn below(structures: &[Structure<'_>], from: Vec3, terrain: f32) -> Below {
    let mut found = Below::Nothing;
    for structure in structures {
        let (min, max) = (structure.aabb.min, structure.aabb.max);
        if !(min.x..=max.x).contains(&from.x) || !(min.z..=max.z).contains(&from.z) {
            continue;
        }
        // Solid, so a ray that starts inside reads as a hit where it starts.
        match structure.collider.cast_ray(
            structure.position,
            structure.rotation,
            from,
            Vec3::NEG_Y,
            from.y - terrain,
            true,
        ) {
            Some((distance, _)) if distance <= 0.0 => return Below::Inside,
            Some((distance, _)) => {
                let top = from.y - distance;
                let higher = !matches!(found, Below::Surface(was) if was >= top);
                if higher && !over_open_space(structure, from, terrain, top) {
                    found = Below::Surface(top);
                }
            }
            None => {}
        }
    }
    found
}

/// Whether `structure`, its top at `top` over `from`, stands there over open
/// space from the `terrain` up (#1833, [`below`]). A top too low to have
/// room for a foot under it is never asked, so a pad, a curb or a ribbon a
/// few centimetres up costs no second ray.
fn over_open_space(structure: &Structure<'_>, from: Vec3, terrain: f32, top: f32) -> bool {
    if top - terrain < OPEN_UNDER + ONE_SURFACE {
        return false;
    }
    structure
        .collider
        .cast_ray(
            structure.position,
            structure.rotation,
            Vec3::new(from.x, terrain, from.z),
            Vec3::Y,
            top - terrain,
            true,
        )
        .is_some_and(|(distance, _)| {
            distance >= OPEN_UNDER && terrain + distance < top - ONE_SURFACE
        })
}

/// Hand a body the physics has resting on the terrain the terrain's own
/// ground (#1833), with `structures` read in over it, and every other body
/// the level floor.
///
/// A beast that `rests` on the terrain is handed it as a [`GroundPatch`],
/// sampled over everywhere its feet can reach ([`GroundPatch::reach`]). One
/// that rests anywhere else stands on something the heightmap does not know,
/// and keeps the level floor through its collider, as it always has. So does
/// one in deep water, whose stroke has no floor, and a person, unless
/// [`PEOPLE_STAND_ON_TERRAIN`].
///
/// **Each sample is the terrain under it, unless a structure is**: a sample
/// inside one of `structures`' bounds is read by a ray straight down from a
/// little above the highest a foot may step ([`RAY_HEADROOM`]), and takes
/// the highest surface it meets that stands on something rather than over
/// open space ([`below`]) - never lower than the terrain there. A ray that
/// starts inside a solid structure leaves the terrain's height: a wall or a
/// trunk is not a step, and the body's own collider keeps it from walking
/// into one. Inside a trimesh no ray knows it is (see [`below`]): a sample
/// in a voxel shell's walls reads the shell's base where it stands over the
/// terrain.
///
/// `root` is the rigged root's world pose this frame - the chassis' current
/// `Transform` through the root's own offset and half turn (see the module
/// doc). Mapping each sample through it is what keeps a slope rising toward
/// the body's front rising toward its front in the patch. Heights are the
/// ground's under each sample less the sample's own world height, which is
/// the root frame's `y` for a chassis turned about `Y` alone, as every
/// on-foot one is.
///
/// **The heightmap is not quite the collider** (defect 4 of the #1833
/// review, left as it is): it answers bilinearly, while the heightfield
/// collider the chassis rests on and the drawn mesh are two triangles a cell,
/// and the two differ by up to `|h00 - h10 - h01 + h11| / 4` a cell - nothing
/// on a plane, a few centimetres on a twisted cell.
pub(crate) fn fill_terrain_ground(
    drive: &mut Drive,
    rig: &Rig,
    root: &GlobalTransform,
    heightmap: Option<&FinishedHeightMap>,
    rests: RestsOn,
    structures: &[Structure<'_>],
) {
    let (Some(heightmap), Some(reach)) = (wanted(drive, rig, heightmap), GroundPatch::reach(rig))
    else {
        drive.ground = None;
        return;
    };
    if rests != RestsOn::Terrain {
        drive.ground = None;
        return;
    }
    let affine = root.affine();
    let (start, across, along) = (
        Vec3::from(affine.translation),
        Vec3::from(affine.matrix3.x_axis),
        Vec3::from(affine.matrix3.z_axis),
    );
    let step_up = FootingConfig::default().max_step_up + RAY_HEADROOM;
    let ground = |at: Vec2| {
        let world = start + across * at.x + along * at.y;
        let terrain = heightmap.world_height_at(world.x, world.z);
        let surface = if structures.is_empty() {
            terrain
        } else {
            let from = Vec3::new(world.x, terrain.max(start.y) + step_up, world.z);
            match below(structures, from, terrain) {
                Below::Surface(top) => top.max(terrain),
                Below::Nothing | Below::Inside => terrain,
            }
        };
        surface - world.y
    };
    // Last frame's samples are overwritten in place: this runs every frame
    // for every beast standing on the terrain, on the web too.
    let mut patch = drive.ground.take().unwrap_or_default();
    patch.resample(reach, GROUND_STEP, ground);
    drive.ground = Some(patch);
}

/// The heightmap, if `drive`'s body is one that stands on the terrain at all:
/// a beast, or a person while [`PEOPLE_STAND_ON_TERRAIN`], and not swimming.
fn wanted<'h>(
    drive: &Drive,
    rig: &Rig,
    heightmap: Option<&'h FinishedHeightMap>,
) -> Option<&'h FinishedHeightMap> {
    heightmap.filter(|_| (stands_on_four(rig) || PEOPLE_STAND_ON_TERRAIN) && !drive.swimming)
}

/// What a structure is read off: its shape, where it is, its bounds, and the
/// body it is part of, if any.
type Solid = (
    &'static Collider,
    &'static Position,
    &'static Rotation,
    &'static ColliderAabb,
    Option<&'static ColliderOf>,
);

/// What the fill reads of the world to hand a body the ground under it
/// (#1833): what its chassis rests on, and the structures within its reach.
///
/// Every part is optional in effect: an app with no physics - a test, or a
/// tool that runs none - has no contacts and no collider trees, and every
/// body in it keeps the level floor.
#[derive(SystemParam)]
pub(crate) struct Underfoot<'w, 's> {
    contacts: Option<Res<'w, ContactGraph>>,
    spatial: Option<SpatialQuery<'w, 's>>,
    sensors: Query<'w, 's, Entity, With<Sensor>>,
    /// The terrain a body may stand on: not one on its way out, whose
    /// heightmap is no longer the one in hand.
    terrain: Query<'w, 's, (), (With<TerrainMesh>, Without<OutgoingTerrain>)>,
    /// Every terrain, outgoing or not: none is a structure.
    any_terrain: Query<'w, 's, (), With<TerrainMesh>>,
    simulated: Query<'w, 's, (), With<Collider>>,
    solids: Query<'w, 's, Solid, Without<Sensor>>,
    bodies: Query<'w, 's, &'static RigidBody>,
}

/// What a body the physics does not simulate here remembers of the cast that
/// last said what it rests on (#1833 review, finding 3), kept in its
/// [`super::RiggedTrail`]: the lying capsule it casts, cut once from the rig
/// and again only when the rig's trunk changes, and the last answer with the
/// drawn pose it was cast from. [`Underfoot::fill`] reads the answer back
/// while the body stands within [`RECAST_MOVED`] and [`RECAST_TURNED`] of
/// that pose, for up to [`RECAST_EVERY`] frames, and casts again otherwise.
///
/// It goes across a beast's rebuild with the rest of the trail; a new rig
/// whose trunk is another shape is cut a new capsule and cast afresh on its
/// first frame.
#[derive(Clone, Default)]
pub(super) struct RestCast {
    capsule: Option<(LyingCapsule, Collider)>,
    last: Option<LastRest>,
}

/// One cast's answer (#1833, [`RestCast`]): the drawn pose it was cast from,
/// how many frames have read it back since, and what the body rested on.
#[derive(Clone, Copy)]
struct LastRest {
    at: Vec3,
    rotation: Quat,
    reused: u32,
    rests: RestsOn,
}

impl RestCast {
    /// How many frames running the last cast has been read back, or `None`
    /// with no cast kept: for a test to tell a reused answer from a fresh one.
    #[cfg(test)]
    pub(super) fn reused(&self) -> Option<u32> {
        self.last.map(|last| last.reused)
    }
}

impl Underfoot<'_, '_> {
    /// Hand the body under `chassis`, posed at `root` this frame, the ground
    /// under it: [`fill_terrain_ground`] with what the physics has it resting
    /// on and the structures within its reach. `memory` is the body's own
    /// [`RestCast`], for one the physics does not simulate.
    ///
    /// Nothing is asked of the physics for a body that does not stand on the
    /// terrain at all - a person, a swimmer - so a person's frame is what it
    /// was before #1833 to the bit.
    pub(super) fn fill(
        &self,
        drive: &mut Drive,
        rig: &Rig,
        chassis: Entity,
        root: &GlobalTransform,
        heightmap: Option<&FinishedHeightMap>,
        memory: &mut RestCast,
    ) {
        let rests = if wanted(drive, rig, heightmap).is_some() {
            self.rests_on(chassis, rig, root, memory)
        } else {
            // A swimmer coming out is read afresh, not off a cast from before.
            memory.last = None;
            RestsOn::Elsewhere
        };
        self.fill_resting(drive, rig, root, heightmap, rests);
    }

    /// [`Self::fill`] for a body whose resting is already known: the render
    /// tool's walker, which reads it off the cast that set it down
    /// ([`Self::set_down`]).
    pub(crate) fn fill_resting(
        &self,
        drive: &mut Drive,
        rig: &Rig,
        root: &GlobalTransform,
        heightmap: Option<&FinishedHeightMap>,
        rests: RestsOn,
    ) {
        let structures = match (
            rests,
            GroundPatch::reach(rig),
            wanted(drive, rig, heightmap),
        ) {
            (RestsOn::Terrain, Some(reach), Some(heightmap)) => {
                self.structures_over(root, reach, heightmap)
            }
            _ => Vec::new(),
        };
        fill_terrain_ground(drive, rig, root, heightmap, rests, &structures);
    }

    /// What the chassis rests on: its contacts, for one the physics simulates
    /// here; for one it does not, its body's lying capsule cast down from a
    /// little above where it stands ([`Self::rest_of`]), or the last such
    /// cast's answer while it has not moved since ([`RestCast`]).
    fn rests_on(
        &self,
        chassis: Entity,
        rig: &Rig,
        root: &GlobalTransform,
        memory: &mut RestCast,
    ) -> RestsOn {
        if self.simulated.contains(chassis) {
            let Some(contacts) = self.contacts.as_deref() else {
                return RestsOn::Elsewhere;
            };
            let mut terrain = false;
            for (other, _) in crate::player::resting_under(
                contacts,
                &self.sensors,
                chassis,
                crate::player::UNDER_NORMAL_Y,
            ) {
                if !self.terrain.contains(other) {
                    return RestsOn::Elsewhere;
                }
                terrain = true;
            }
            return if terrain {
                RestsOn::Terrain
            } else {
                RestsOn::Elsewhere
            };
        }
        // The trunk is read every frame - a few dozen joints - so a rig
        // swapped under a kept trail is never cast as the old one's capsule.
        let Some(shape) = LyingCapsule::of(rig) else {
            *memory = RestCast::default();
            return RestsOn::Elsewhere;
        };
        if memory.capsule.as_ref().is_none_or(|(cut, _)| *cut != shape) {
            *memory = RestCast {
                capsule: Some((shape, shape.collider())),
                last: None,
            };
        }
        let (_, rotation, at) = root.to_scale_rotation_translation();
        if let Some(last) = memory.last.as_mut().filter(|last| {
            last.reused + 1 < RECAST_EVERY
                && last.at.distance(at) < RECAST_MOVED
                && last.rotation.angle_between(rotation) < RECAST_TURNED
        }) {
            last.reused += 1;
            return last.rests;
        }
        let Some((_, capsule)) = memory.capsule.as_ref() else {
            return RestsOn::Elsewhere;
        };
        let rests = self.resting_cast(capsule, root);
        memory.last = Some(LastRest {
            at,
            rotation,
            reused: 0,
            rests,
        });
        rests
    }

    /// What `capsule` posed at `root` rests on, cast down from [`CAST_RISE`]
    /// over it: [`Self::rest_of`]'s answer, if it fell no further than
    /// [`RESTING_WITHIN`] - further, it is in the air.
    pub(super) fn resting_cast(&self, capsule: &Collider, root: &GlobalTransform) -> RestsOn {
        match self.rest_of(capsule, root, CAST_RISE, CAST_FALL) {
            Some((fell, rests)) if fell <= RESTING_WITHIN => rests,
            _ => RestsOn::Elsewhere,
        }
    }

    /// Where a body's lying `capsule`, posed at `root` and raised `rise` over
    /// it, comes to rest cast straight down onto the world no further than
    /// `fall` under `root`, and what it rests on there: how far it fell below
    /// `root`, and whether everything under it within [`TOUCHING_WITHIN`] of
    /// the first thing is the terrain. `None` when nothing is under it that
    /// far down.
    ///
    /// The capsule is the one [`crate::pds::avatar::locomotion::fit_on_foot`]
    /// cuts from the rig ([`LyingCapsule`]), the chassis' own shape, held
    /// level as the chassis is: on a slope it comes to rest on its uphill
    /// end, as the chassis does. Only what is under it counts - a contact
    /// facing up within 60 degrees ([`crate::player::UNDER_NORMAL_Y`]); a wall
    /// it was already touching at the start is beside it, not under it.
    ///
    /// **Every hit is read, nearest first by distance, none kept** (#1833
    /// review, finding 5): avian hands its hits over in no promised order,
    /// and a cap on how many it hands over could drop the nearest - the
    /// terrain - in a cluttered spot. So the callback keeps the nearest
    /// terrain hit and the nearest of anything else, and a sensor is passed
    /// over there rather than gathered into a filter each call.
    fn rest_of(
        &self,
        capsule: &Collider,
        root: &GlobalTransform,
        rise: f32,
        fall: f32,
    ) -> Option<(f32, RestsOn)> {
        let spatial = self.spatial.as_ref()?;
        let (_, rotation, at) = root.to_scale_rotation_translation();
        let config = ShapeCastConfig {
            max_distance: rise + fall,
            ignore_origin_penetration: true,
            ..ShapeCastConfig::DEFAULT
        };
        let (mut terrain, mut other) = (f32::INFINITY, f32::INFINITY);
        spatial.shape_hits_callback(
            capsule,
            at + Vec3::Y * rise,
            rotation,
            Dir3::NEG_Y,
            &config,
            &SpatialQueryFilter::default(),
            |hit| {
                if hit.distance > 0.0
                    && hit.normal1.y >= crate::player::UNDER_NORMAL_Y
                    && !self.sensors.contains(hit.entity)
                {
                    let nearest = if self.terrain.contains(hit.entity) {
                        &mut terrain
                    } else {
                        &mut other
                    };
                    *nearest = nearest.min(hit.distance);
                }
                true
            },
        );
        let first = terrain.min(other);
        if !first.is_finite() {
            return None;
        }
        let rests = if other <= first + TOUCHING_WITHIN {
            RestsOn::Elsewhere
        } else {
            RestsOn::Terrain
        };
        Some((first - rise, rests))
    }

    /// Where the render tool's walker posed at `root` - over the terrain under
    /// its centre, or where it rested last frame if that is higher - comes to
    /// rest, as a game chassis rests (#1833): how far under `root` its body's
    /// lying capsule comes to rest cast down from [`SET_DOWN_RISE`] over it,
    /// and what it rests on there. `None` for a body that is not a beast,
    /// which walks the raw ground, and before the physics has a collider to
    /// rest on within [`SET_DOWN_FALL`].
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn set_down(&self, rig: &Rig, root: &GlobalTransform) -> Option<(f32, RestsOn)> {
        if !stands_on_four(rig) {
            return None;
        }
        let capsule = LyingCapsule::of(rig)?.collider();
        self.rest_of(&capsule, root, SET_DOWN_RISE, SET_DOWN_FALL)
    }

    /// Every static solid that is not the terrain with some of its shape in
    /// the band a foot over `reach` of a body posed at `root` may stand in:
    /// what its feet may stand on over the terrain. A sensor is a walk-in
    /// volume and never ground (#813); a dynamic or kinematic body - another
    /// body's chassis, a boat, a pushable rock - is not a floor to plant a
    /// foot on.
    ///
    /// **By its shape, not its bounds** (#1833 review, finding 2). A road
    /// network's deck and its structure are one trimesh each
    /// (`terrain::roads`), whose bounds span every street of it, so a beast
    /// anywhere between them had every sample rayed against a mesh with no
    /// triangle under it: 54.7 us a fill against 7.96 in open terrain. So the
    /// box is the reach turned with the body and padded a sample's step, from
    /// [`BAND_MARGIN`] under the lowest of the terrain at nine points of it
    /// to that far over where the highest ray starts, and a solid is gathered
    /// only if it meets that box.
    pub(super) fn structures_over(
        &self,
        root: &GlobalTransform,
        reach: Rect,
        heightmap: &FinishedHeightMap,
    ) -> Vec<Structure<'_>> {
        use avian3d::parry::{math::Pose, query::intersection_test, shape::Cuboid};
        let Some(spatial) = self.spatial.as_ref() else {
            return Vec::new();
        };
        // The samples run up to half a step past the reach.
        let reach = reach.inflate(GROUND_STEP);
        let affine = root.affine();
        let (start, along) = (
            Vec3::from(affine.translation),
            Vec3::from(affine.matrix3.z_axis),
        );
        let turn = Quat::from_rotation_y(along.x.atan2(along.z));
        let point = |at: Vec2| start + turn * Vec3::new(at.x, 0.0, at.y);
        let (mut low, mut high) = (f32::MAX, start.y);
        for u in [0.0, 0.5, 1.0] {
            for v in [0.0, 0.5, 1.0] {
                let at = point(reach.min + reach.size() * Vec2::new(u, v));
                let terrain = heightmap.world_height_at(at.x, at.z);
                (low, high) = (low.min(terrain), high.max(terrain));
            }
        }
        let step_up = FootingConfig::default().max_step_up + RAY_HEADROOM;
        let (bottom, top) = (low - BAND_MARGIN, high + step_up + BAND_MARGIN);
        let half = Vec3::new(
            reach.half_size().x,
            (top - bottom) * 0.5,
            reach.half_size().y,
        );
        let centre = point(reach.center()).with_y((top + bottom) * 0.5);
        let (band, pose) = (Cuboid::new(half), Pose::from_parts(centre, turn));
        let corners = [
            reach.min,
            reach.max,
            Vec2::new(reach.min.x, reach.max.y),
            Vec2::new(reach.max.x, reach.min.y),
        ]
        .map(point);
        let (min, max) = corners
            .iter()
            .fold((Vec3::MAX, Vec3::MIN), |(min, max), &at| {
                (min.min(at), max.max(at))
            });
        let area = ColliderAabb::from_min_max(min.with_y(bottom), max.with_y(top));
        let mut found = Vec::new();
        spatial.aabb_intersections_with_aabb_callback(area, |entity| {
            if self.any_terrain.contains(entity) {
                return true;
            }
            let Ok((collider, position, rotation, aabb, of)) = self.solids.get(entity) else {
                return true;
            };
            let fixed =
                of.is_none_or(|of| self.bodies.get(of.body).is_ok_and(RigidBody::is_static));
            // A shape parry cannot test against a box is gathered, as every
            // one was before.
            let meets = || {
                intersection_test(
                    &Pose::from_parts(position.0, rotation.0),
                    collider.shape_scaled().as_ref(),
                    &pose,
                    &band,
                )
                .unwrap_or(true)
            };
            if fixed && meets() {
                found.push(Structure {
                    aabb: *aabb,
                    collider,
                    position: position.0,
                    rotation: rotation.0,
                });
            }
            true
        });
        found
    }
}
