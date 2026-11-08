//! The ground under a beast's feet (#1833): the terrain's own, sampled into a
//! [`GroundPatch`], for a beast the physics has resting on the terrain and
//! nothing else, with whatever stands on the terrain within its reach read
//! in by rays; and a structure's, for one resting on static solids - a road,
//! a floor, a pad, a deck (#1834).
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
//! on static solids is handed their ground instead (#1834, below); one on a
//! boat, a kinematic prop or another body - anything that moves - keeps the
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
//! **A beast resting on a structure stands on it** (#1834). Static solids
//! under the chassis - with the terrain beside them or not - are a structure
//! ([`RestsOn::Structure`]), read by the same contacts and the same cast, and
//! the body is handed their ground ([`fill_structure_ground`]): each sample
//! read by the same rays, and the terrain. Ground joined to the structure
//! under the body's feet, neighbour to neighbour within a step, is stood on
//! as it is ([`joined`]): a road's ribbon on a hill, over a crest or in a sag,
//! its curbs and the terrain past them, the hill around a stepping stone, the
//! ground beside a log, and a beam or a thin ramp rising on past the floor,
//! which a ray up from the floor meets as it meets a table (#1884). Where
//! the structure spans the stance, the terrain is joined only within a pelvis
//! drop under the structure nearest it (#1883): the terrain beside a road
//! falling away over a crest is followed down with it, and the ground round
//! the foot of a ramp down from a deck, but not the hill round a small
//! platform down under its downhill edge, nor the ground under the deck's
//! edge beside the ramp. Any other ground is taken while it is within a
//! step's band of the plane the body stands on, and that plane otherwise -
//! the structure's where its samples span the stance, the ground's beside it
//! where they do not ([`Plane::fit`]). So a floor's walls - rays started
//! inside them - and the terrain under the floor are not stood on, and a
//! beast in a room stands on its floor; and the legs over a deck's edge, the
//! ground a metre under them, are held at the deck's plane ([`PAST_AN_EDGE`],
//! the owner's call). Before, every structure was the level floor, which on a
//! sloped road ran through it, and at a raised floor's edge the patch of a
//! body still on the terrain hung its legs.
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
//! **Leaving a ground is held, and the ground eased** (#1835, #1834). The
//! physics' answer is a step's, and a step can lose the body: the owner's
//! capsule turning on the spot on a hill or walking over a fold of it now and
//! then touches nothing for one fixed step, and its contacts say it rests on
//! nothing. Standing still it never did, measured headless. Read raw, that
//! step handed the body the level floor, and the engine rides a beast's trunk
//! over whatever ground it is handed at once, so on a slope the trunk snapped
//! level and back - 9 to 16 degrees for a frame at 60 Hz and for two at 144.
//! So a body on the terrain stands by it until the physics has said otherwise
//! for [`HOLD_SECS`], while the terrain is still under it, and then its patch
//! is eased out to the level floor over [`BLEND_SECS`] ([`RestHold`]):
//! stepping onto a pad on a hill turns the trunk level over a fifth of a
//! second rather than in a frame. Coming back onto the terrain from a pad or a
//! step down eases the patch in over the same time. A body the terrain has
//! fallen away from - in the air, or on a deck over it - is handed the level
//! floor at once, and one landing on the terrain from the air is handed the
//! terrain at once: a held patch planted a falling body's feet on the ground
//! under it, and a held level floor stood a landing body's on the air over a
//! hill (review, findings 1 and 2). A winged body taking off is not handed
//! the level floor at once (#1887): the engine's take-off holds its feet to
//! the ground through its push, so from the take-off's first frame - its
//! drive's flight a takeoff, which a landing held on the ground never is, or
//! the frame of no flight a take-off from a held landing is handed before
//! that, its chassis already leapt - the ground it rose off, terrain or
//! structure, eases out over the push ([`TAKEOFF_SECS`]), carried up with
//! the body as the level floor is, so no foot is held to the ground under
//! it. The take-off is latched until the body lifts ([`Launch`]), so a first
//! frame that dips does not lose it. One that comes down meanwhile is handed
//! the level floor then, and one the physics still reads on the ground when
//! the ease has run has that ground eased back in. A peer's cast keeps it on
//! the terrain a
//! little further over it than it takes to put it there ([`RESTING_BAND`]),
//! so a pose played out near the line does not flip it. A structure is held
//! and eased the same way, its own short cast standing for the terrain's
//! reading ([`Underfoot::structure_under`]), and between the two grounds the
//! one eases out as the other eases in, so a beast walking from a hill onto a
//! pad turns its trunk from the hill's slope to the pad's level over the
//! ease, as it once turned it to the level floor - unless the terrain is no
//! longer under it when the structure is, a pad standing proud of a hill
//! walked onto from it, when the structure's ground is handed over whole at
//! once, as the level floor was before #1834.
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
use symbios_avatar::{DriverConfig, FlightMode, FootingConfig, Rig};

use super::stands_on_four;
use crate::pds::avatar::locomotion::LyingCapsule;
use crate::terrain::{FinishedHeightMap, OutgoingTerrain, TerrainMesh};

/// How near level a flying body's root must be for it to be handed a patch
/// (#1877, [`Underfoot::fill`]), radians: about 2 degrees, over which a
/// sample at a bat's paw, half its length from the root, is off by a
/// centimetre.
pub(super) const LEVEL_WITHIN: f32 = 2.0 * std::f32::consts::PI / 180.0;

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

/// How much further than [`RESTING_WITHIN`] a collider-less body that stands
/// on the terrain may fall before its cast says it is in the air, metres
/// (#1835): two centimetres, so a pose played out near the line, a few
/// millimetres either side of it from one recast to the next, is not put on
/// the terrain and taken off it.
const RESTING_BAND: f32 = 0.02;

/// How long the physics must say a body on the terrain rests elsewhere, the
/// terrain still under it, before its patch eases out, seconds (#1835): about
/// ten of the game's fixed steps. The gaps it bridges are one step long - the
/// owner's capsule not touching the terrain for a step as it turns on a slope
/// or crosses a fold - and a step onto a pad keeps the patch, which reads the
/// pad by its rays, this long before it eases out.
pub(super) const HOLD_SECS: f32 = 0.15;

/// How long the patch takes to ease out to the level floor once a body that
/// stood on the terrain stands elsewhere, and in again once it is back on the
/// terrain without having been in the air, seconds (#1835). A beast's trunk
/// on a 20-degree hill is 19 degrees from where the level floor holds it, so
/// eased at both ends this turns it at most 2.4 degrees a frame at 60 Hz and
/// one at 144, against the 19 in a frame of a change read raw.
pub(super) const BLEND_SECS: f32 = 0.2;

/// How long the ground a winged body takes off from takes to ease out to the
/// level floor, seconds (#1887, [`RestHold::settle`]): the engine's push,
/// which holds the feet to the ground - its least, a tenth of a second, for
/// every winged body the game builds (symbios-avatar's `MIN_PUSH`) - from
/// the take-off's first frame under a chassis that launches it at once. So
/// the feet the engine lets go over the 0.08 s after (`PLANT`) leave from
/// the ground a level take-off's leave from. Eased on through that release
/// too, over 0.18 s, the planted wing-hands of a bat and a glider swung out
/// sideways and back as the release blended them out of a stance on a ground
/// half gone: 46 mm off the level take-off's and back at 60 frames a second,
/// and at 30, over two or three frames, 75 mm under the owner's chassis and
/// 126 as a peer.
///
/// **A take-off from a held landing is handed a frame of no flight first**
/// (`super::aloft`), on which its chassis has already leapt - 11 cm at 30
/// frames a second, past where the terrain is still under it. That frame
/// hands the ground as it was and carries it up with the body
/// ([`Launch`], [`RestHold::afresh`]), and the ease starts on the takeoff
/// after it, inside the engine's crouch (its `crouch_first`, at least
/// 0.18 s even under a chassis). Read as any other frame, it handed the
/// level floor at once, and the ease had nothing left to carry: a bat a
/// tenth or three tenths of a second into its landing on a 20 degree hill
/// moved a joint 89 mm further than on level ground on that frame at 30 Hz,
/// and read by its own contacts, as the game's chassis is, 94 at 30 and 103
/// at 60; carried, 44, 43 and 31.
pub(super) const TAKEOFF_SECS: f32 = 0.1;

/// How far over the terrain a body the physics has resting on it may be read
/// straight down its centre line and still be on it, metres (#1835 end
/// review): a horse's capsule across 35 degrees is 9.8 cm over it there, and
/// one over a crest's kink between two samples 12 cm on 35; a body set down a
/// metre up is not.
const STALE_WITHIN: f32 = 0.25;

/// How far apart along a body the terrain is read to say whether it is under
/// the body, metres (#1835 end review, finding 1): a crest's kink between two
/// samples is then at most 5 cm from one, a few millimetres of clearance on a
/// steep one.
const UNDER_STEP: f32 = 0.1;

/// The longest frame a hold or an ease is moved on by, seconds (#1835 review,
/// finding 4): two of the game's fixed steps, about. A browser tab brought
/// back after a pause hands over its whole absence as one frame, which would
/// have run a hold out and most of an ease over a step the physics lost.
const LONGEST_FRAME: f32 = 1.0 / 30.0;

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

/// How far under the plane of the structure a beast rests on a sample past a
/// drop stands, metres (#1834): a deck's edge a metre up, the legs over it
/// held at the deck's plane. The owner's decision; the other way is the
/// engine's own [`FootingConfig::max_pelvis_drop`], legs over the drop hung
/// down as far as a pelvis may drop to reach them, short of the ground.
pub(super) const PAST_AN_EDGE: f32 = 0.0;

/// How much of what a body's stance spans, along a way, the samples a plane
/// under its feet is fitted through must span for the plane to tilt that way
/// (#1834 review, [`Plane::fit`]): most of it - both girdles, not a log's
/// row or two between them, nor a stone under one.
const SPANS_THE_STANCE: f32 = 0.75;

/// What a chassis rests on this frame, as the physics reads it (#1833).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestsOn {
    /// The terrain, and nothing else.
    Terrain,
    /// Static solids that are not the terrain - a floor, a pad, a road, a
    /// deck - with or without the terrain beside them (#1834).
    Structure,
    /// Anything else: a body that moves - another chassis, a boat, a
    /// pushable rock - under it or beside what it rests on, a terrain on its
    /// way out, or nothing at all.
    Elsewhere,
}

/// How much of each ground a body's patch carries (#1834, [`RestHold`]): of
/// the terrain's ([`fill_terrain_ground`]'s own) and of the structure's it
/// rests on, each `0..=1`, the level floor taking what they leave. Together
/// never more than all of it: easing from one to the other, the two always
/// add up to the whole.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Share {
    pub(crate) terrain: f32,
    pub(crate) structure: f32,
}

impl Share {
    /// The terrain's ground alone, all of it. Native only, as [`Self::of`]
    /// is (its one caller, `fill_resting`, is the agent's and the tests').
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) const TERRAIN: Self = Self {
        terrain: 1.0,
        structure: 0.0,
    };

    /// The structure's ground alone, all of it. Native only, as [`Self::of`].
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) const STRUCTURE: Self = Self {
        terrain: 0.0,
        structure: 1.0,
    };

    /// The whole of what `rests` on, as a body that has stood there a while
    /// is handed it: none of either for a body resting elsewhere. Native
    /// only: `fill_resting`, its one caller, is.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn of(rests: RestsOn) -> Self {
        match rests {
            RestsOn::Terrain => Self::TERRAIN,
            RestsOn::Structure => Self::STRUCTURE,
            RestsOn::Elsewhere => Self::default(),
        }
    }
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
///
/// `open` is the ground a foot under a surface would stand on, which the
/// up-ray starts from: the terrain, for a body resting on it; for one
/// resting on a structure, the higher of the terrain and the floor it rests
/// on (#1834, [`structure_sample`]), so the deck it stands on, a metre over
/// the terrain, is not read as a table over it.
fn below(structures: &[Structure<'_>], from: Vec3, terrain: f32, open: f32) -> Below {
    below_and_over(structures, from, terrain, open).0
}

/// A surface a ray down from over a sample met standing over open space
/// from the ground under it (#1884, [`below_and_over`]): its top there, and
/// its underside the up-ray met, world heights.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Overhang {
    top: f32,
    underside: f32,
}

/// [`below`], and the highest surface over what it found that it passed by
/// as standing over open space (#1884): what a body resting on a structure
/// may still find its structure going on in ([`joined`]) - a beam or a thin
/// ramp rising past the floor a body on it stands at, its underside over that
/// floor by more than [`OPEN_UNDER`] where it has risen by that and its own
/// thickness, which a ray up from the floor meets as it meets a table's.
/// Whatever it finds is [`below`]'s, to the bit: the overhang is only kept
/// beside it.
fn below_and_over(
    structures: &[Structure<'_>],
    from: Vec3,
    terrain: f32,
    open: f32,
) -> (Below, Option<Overhang>) {
    let mut found = Below::Nothing;
    let mut over: Option<Overhang> = None;
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
            Some((distance, _)) if distance <= 0.0 => return (Below::Inside, None),
            Some((distance, _)) => {
                let top = from.y - distance;
                let higher = !matches!(found, Below::Surface(was) if was >= top);
                if higher {
                    match open_under(structure, from, open, top) {
                        None => found = Below::Surface(top),
                        Some(underside) if over.is_none_or(|was| was.top < top) => {
                            over = Some(Overhang { top, underside });
                        }
                        Some(_) => {}
                    }
                }
            }
            None => {}
        }
    }
    let over = over.filter(|over| !matches!(found, Below::Surface(was) if was >= over.top));
    (found, over)
}

/// Where `structure`'s underside is, if it stands over open space from the
/// ground at `terrain` up, its top at `top` over `from` (#1833, [`below`]:
/// the terrain, or for a body on a structure the floor under it): the world
/// height the up-ray met it at. A top too low to have room for a foot under
/// it is never asked, so a pad, a curb or a ribbon a few centimetres up costs
/// no second ray.
fn open_under(structure: &Structure<'_>, from: Vec3, terrain: f32, top: f32) -> Option<f32> {
    if top - terrain < OPEN_UNDER + ONE_SURFACE {
        return None;
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
        .filter(|&(distance, _)| distance >= OPEN_UNDER && terrain + distance < top - ONE_SURFACE)
        .map(|(distance, _)| terrain + distance)
}

/// Hand a body the physics has resting on the terrain the terrain's own
/// ground (#1833), with `structures` read in over it, one resting on a
/// structure the structure's ground (#1834, [`fill_structure_ground`]), and
/// every other body the level floor.
///
/// A beast that rests on the terrain is handed it as a [`GroundPatch`],
/// sampled over everywhere its feet can reach ([`GroundPatch::reach`]). One
/// that rests on something that moves keeps the level floor through its
/// collider, as it always has. So does one in deep water, whose stroke has
/// no floor, and a person, unless [`PEOPLE_STAND_ON_TERRAIN`].
///
/// `share` is how much of each ground the patch carries (#1835, #1834,
/// [`RestHold`]): none of either is the level floor and no patch at all, the
/// whole of the terrain's the terrain itself, to the bit, and between them
/// each sample's height over the level floor that share of the terrain's - a
/// body easing onto the terrain or off it - plus the structure's share of
/// its own.
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
    share: Share,
    structures: &[Structure<'_>],
) {
    let (Some(heightmap), Some(reach)) = (wanted(drive, rig, heightmap), GroundPatch::reach(rig))
    else {
        drive.ground = None;
        return;
    };
    if share.structure > 0.0 {
        fill_structure_ground(drive, rig, root, heightmap, reach, share, structures);
        return;
    }
    let share = share.terrain;
    if share <= 0.0 {
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
            terrain_surface(below(structures, from, terrain, terrain), terrain)
        };
        (surface - world.y) * share
    };
    // Last frame's samples are overwritten in place: this runs every frame
    // for every beast standing on the terrain, on the web too.
    let mut patch = drive.ground.take().unwrap_or_default();
    patch.resample(reach, GROUND_STEP, ground);
    drive.ground = Some(patch);
}

/// The ground a body resting on the terrain stands on at a sample, from what
/// a ray found there over the `terrain` ([`fill_terrain_ground`]): the
/// highest surface a foot stands on, never under the terrain, and the
/// terrain itself past a wall or over nothing.
fn terrain_surface(found: Below, terrain: f32) -> f32 {
    match found {
        Below::Surface(top) => top.max(terrain),
        Below::Nothing | Below::Inside => terrain,
    }
}

/// What one sample of a body resting on a structure reads (#1834,
/// [`fill_structure_ground`]): the ground a foot stands on there, by its
/// height over the root's floor - `None` inside a wall - and whether a
/// structure's own surface is what it is; and a surface over it the rays
/// passed by as standing over open space, by its top's and its underside's
/// heights over the floor (#1884, [`Overhang`]).
struct StructureSample {
    height: Option<f32>,
    on_structure: bool,
    over: Option<Overhang>,
}

/// One sample of a body resting on a structure, at `world` with the root's
/// floor at `floor` (#1834): what [`below_and_over`] finds of `structures`
/// over the `terrain` there, the up-ray started from the higher of the
/// terrain and the floor.
fn structure_sample(
    (found, over): (Below, Option<Overhang>),
    terrain: f32,
    floor: f32,
) -> StructureSample {
    let over = over.map(|over| Overhang {
        top: over.top - floor,
        underside: over.underside - floor,
    });
    match found {
        Below::Surface(top) => StructureSample {
            height: Some(top.max(terrain) - floor),
            on_structure: top >= terrain,
            over,
        },
        Below::Nothing => StructureSample {
            height: Some(terrain - floor),
            on_structure: false,
            over,
        },
        Below::Inside => StructureSample {
            height: None,
            on_structure: false,
            over: None,
        },
    }
}

/// A plane of heights over the root's floor, across and along a body's own
/// frame: `height + slope · (at - centre)`.
#[derive(Clone, Copy, Default)]
struct Plane {
    centre: Vec2,
    height: f32,
    slope: Vec2,
}

impl Plane {
    fn at(&self, at: Vec2) -> f32 {
        self.height + self.slope.dot(at - self.centre)
    }

    /// The plane a surface under a body's feet lies in (#1834): fitted, least
    /// squares, through `samples` - each a place in the body's frame and the
    /// surface's height over its floor there - and whether they span the
    /// stance, whose samples' places are `stance`, both ways.
    ///
    /// **Tilted only along a way the samples span the stance** (#1834
    /// review, findings 1 and 3). The fit is made along the two principal
    /// axes of the samples' places, along which it is two fits of a line, the
    /// least-squares plane itself, its cross terms nothing there. Each is kept
    /// only where the samples reach across [`SPANS_THE_STANCE`] of what the
    /// stance's own samples do along it, and level otherwise. A log's one or
    /// two curved rows across a horse's
    /// stance once tilted the plane with their curve, carried over the whole
    /// patch: level along the body now, they say only that the log is level
    /// across it. A beam's one column, a line - which handed the old fit no
    /// plane at all, so level - tilts it along the beam and leaves it level
    /// across. The level floor with no samples.
    fn fit(
        samples: impl Iterator<Item = (Vec2, f32)> + Clone,
        stance: impl Iterator<Item = Vec2> + Clone,
    ) -> Fit {
        let (mut count, mut sum, mut height) = (0.0f32, Vec2::ZERO, 0.0f32);
        for (at, h) in samples.clone() {
            count += 1.0;
            sum += at;
            height += h;
        }
        if count == 0.0 {
            return Fit::default();
        }
        let (centre, height) = (sum / count, height / count);
        let (mut xx, mut xz, mut zz) = (0.0f32, 0.0f32, 0.0f32);
        for (at, _) in samples.clone() {
            let d = at - centre;
            (xx, xz, zz) = (xx + d.x * d.x, xz + d.x * d.y, zz + d.y * d.y);
        }
        // The places' main axis: the eigenvector of their scatter with the
        // larger eigenvalue, taken from whichever form of it is not near
        // nothing.
        let half = (xx - zz) * 0.5;
        let spread = (half * half + xz * xz).sqrt();
        let main = if spread <= 0.0 {
            Vec2::X
        } else if half >= 0.0 {
            Vec2::new(half + spread, xz).normalize()
        } else {
            Vec2::new(xz, spread - half).normalize()
        };
        let (mut slope, mut spans) = (Vec2::ZERO, true);
        for axis in [main, main.perp()] {
            let (mut pp, mut ph, mut low, mut high) =
                (0.0f32, 0.0f32, f32::INFINITY, f32::NEG_INFINITY);
            for (at, h) in samples.clone() {
                let p = (at - centre).dot(axis);
                (pp, ph) = (pp + p * p, ph + p * (h - height));
                (low, high) = (low.min(p), high.max(p));
            }
            let (least, most) = stance
                .clone()
                .map(|at| (at - centre).dot(axis))
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(l, m), p| {
                    (l.min(p), m.max(p))
                });
            if high > low && pp > 0.0 && high - low >= SPANS_THE_STANCE * (most - least) {
                slope += axis * (ph / pp);
            } else {
                spans = false;
            }
        }
        Fit {
            plane: Self {
                centre,
                height,
                slope,
            },
            spans,
        }
    }
}

/// A [`Plane`] fitted through samples under a body's feet, and whether they
/// span its stance both ways, so that it is tilted both ways ([`Plane::fit`]).
#[derive(Clone, Copy, Default)]
struct Fit {
    plane: Plane,
    spans: bool,
}

/// A sample joined to the structure under a body's feet ([`joined`]): the
/// height it is joined at over the root's floor, and whether that is a
/// structure's surface.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Joined {
    height: f32,
    on_structure: bool,
}

/// The four neighbours of sample `i` on a grid `side` samples square, in
/// the grid's own order.
fn neighbours(i: usize, side: usize) -> [Option<usize>; 4] {
    let (column, row) = (i % side, i / side);
    [
        (column > 0).then(|| i - 1),
        (column + 1 < side).then(|| i + 1),
        (row > 0).then(|| i - side),
        (row + 1 < side).then(|| i + side),
    ]
}

/// [`joined`]'s steps: every sample of the stance whose ground is a
/// structure's surface within `band` of the root's floor, and every sample
/// reached from one of those through its four neighbours, each within
/// `band` of the last and one `admits` - by its index, its height and
/// whether that is a structure's surface - each at its own height. Which
/// samples are reached hangs on nothing but the samples, so each is joined
/// once and the order they are taken in changes nothing.
fn steps(
    read: &[(StructureSample, f32)],
    side: usize,
    band: &std::ops::RangeInclusive<f32>,
    in_stance: impl Fn(usize) -> bool,
    admits: impl Fn(usize, f32, bool) -> bool,
) -> Vec<Option<Joined>> {
    let mut joined: Vec<Option<Joined>> = vec![None; read.len()];
    let mut open: Vec<usize> = Vec::new();
    for (i, (sample, _)) in read.iter().enumerate() {
        if let Some(height) = sample
            .height
            .filter(|h| in_stance(i) && sample.on_structure && band.contains(h))
        {
            joined[i] = Some(Joined {
                height,
                on_structure: true,
            });
            open.push(i);
        }
    }
    while let Some(i) = open.pop() {
        let Some(here) = joined[i] else { continue };
        for next in neighbours(i, side).into_iter().flatten() {
            let sample = &read[next].0;
            if joined[next].is_some() {
                continue;
            }
            let Some(height) = sample.height.filter(|&to| {
                band.contains(&(to - here.height)) && admits(next, to, sample.on_structure)
            }) else {
                continue;
            };
            joined[next] = Some(Joined {
                height,
                on_structure: sample.on_structure,
            });
            open.push(next);
        }
    }
    joined
}

/// How low the terrain at each sample of a body resting on a structure is
/// joined to it where [`joined`] cuts it (#1883 review): a pelvis drop -
/// `band`'s start - under the structure nearest the sample on the grid.
///
/// **The structure is the one under the body's feet, as far as it goes by
/// its own steps**: what [`steps`] joins from the stance's structure without
/// the terrain - a deck, a ramp or a stair down from it, a road's ribbon and
/// its curbs. A curb or a step stood on the ground and reached only over the
/// terrain - at the foot of a ramp, or under a deck's edge - is joined, but
/// is not measured from: the ground under the deck's edge beside it would
/// be measured from a step down there and not from the deck.
///
/// **Nearest by steps along the grid**, through its four neighbours: every
/// sample's found at once from all of the structure, a ring at a time
/// (a breadth-first walk), a tie going to the lower - the more lenient cut,
/// and one no order can change. With no structure joined, nothing is cut.
fn lowest_joined(
    read: &[(StructureSample, f32)],
    side: usize,
    band: &std::ops::RangeInclusive<f32>,
    in_stance: impl Fn(usize) -> bool,
) -> Vec<f32> {
    let structure = steps(read, side, band, in_stance, |_, _, on_structure| {
        on_structure
    });
    let mut nearest: Vec<Option<f32>> = structure
        .iter()
        .map(|joined| joined.map(|joined| joined.height))
        .collect();
    let mut ring: Vec<usize> = (0..read.len()).filter(|&i| nearest[i].is_some()).collect();
    let mut steps_away: Vec<usize> = nearest
        .iter()
        .map(|near| if near.is_some() { 0 } else { usize::MAX })
        .collect();
    let mut away = 0;
    while !ring.is_empty() {
        away += 1;
        let mut next_ring = Vec::new();
        for &i in &ring {
            // Final: every sample of the ring before took the lowest of all
            // its own ring's neighbours before this one began.
            let Some(here) = nearest[i] else { continue };
            for next in neighbours(i, side).into_iter().flatten() {
                if steps_away[next] == usize::MAX {
                    steps_away[next] = away;
                    nearest[next] = Some(here);
                    next_ring.push(next);
                } else if steps_away[next] == away {
                    nearest[next] = nearest[next].map(|was| was.min(here));
                }
            }
        }
        ring = next_ring;
    }
    nearest
        .into_iter()
        .map(|near| near.map_or(f32::NEG_INFINITY, |height| height + *band.start()))
        .collect()
}

/// Which samples of a body resting on a structure are ground joined to the
/// structure under its feet (#1834 review, findings 1 and 2), each as it is
/// [`Joined`]: every sample of the stance whose ground is a structure's
/// surface within the step's `band` of the root's floor, and every sample
/// reached from one of those through its four neighbours, each no further
/// from the last than the same band - down by no more than a pelvis drops,
/// up by no more than a step ([`steps`]). A road over a crest or in a sag,
/// its curbs and the terrain past them, a ramp, a stair, the hill around a
/// stepping stone and the ground beside a log are joined, however far from
/// any plane they curve away; the ground a metre past a deck's edge, and
/// anything the other side of a wall (a ray started inside it reads no
/// ground), are not. `side` is the grid's, `read` in its order.
///
/// **Cut, the terrain is joined no lower than a pelvis drop under the
/// structure nearest it** (#1883: `lowest`, from [`lowest_joined`], where
/// the structure spans the stance, [`fill_structure_ground`]). A neighbour
/// at a time, a small platform on a hill whose uphill edge meets the hill,
/// short enough for the reach to go round it, joined the hill beside that
/// edge, down the hill along its sides and so the hill under its downhill
/// edge, and a foot over that edge was put down on the hill instead of being
/// held at the platform's plane (the owner's deck-edge decision on #1834):
/// the hill there is nearest the platform, and cut a pelvis drop under it.
/// The terrain beside a road falling away over a crest is nearest the
/// ribbon at its own row, a few centimetres under it all the way down, and
/// is joined however far under the floor the two go; a cut at a pelvis drop
/// under the floor held it at the plane, 0.7 m over it under a horse on 20
/// degrees. The ground round the foot of a ramp or a stair down from a deck
/// is nearest them and joined, and the ground under the deck's edge beside
/// them is nearest the edge and is not; a cut from the structure each chain
/// of steps left carried the ramp's foot along the ground and under the
/// edge, and the legs over it went down to the ground (#1883 review). A
/// structure's own surface joins step by step to any depth, and the terrain
/// rising to any height. Uncut - a body the structure does not span, on a
/// stepping stone on a hill or a log, stands on the ground beside it as
/// much as on it - the ground beside it is joined step by step to any depth.
///
/// **A surface over open space goes on from ground it continues** (#1884): a
/// sample not joined by what it reads - the ground under a beam, two metres
/// down - is joined at the top of the surface the rays passed over it as one
/// a foot stands under ([`below_and_over`]), where that is within the band
/// of a joined neighbour and its underside less than [`OPEN_UNDER`] over
/// that neighbour, no room for a foot between them. So a surface rising past
/// the floor - where it reads as a table from the floor, more than
/// [`OPEN_UNDER`] and its own thickness over it, and was held at the plane -
/// is followed up it, every sample at its own surface, while it rises by
/// less than [`OPEN_UNDER`] and its own thickness a [`GROUND_STEP`] along
/// the grid: `0.25 tan(slope) - thickness / cos(slope) < 0.1`, so a beam
/// 0.1 m thick to about 43 degrees, a ramp 3 cm thick to about 28 and a
/// sheet to about 22. A bench or a table beside the ground a body stands on
/// has that room under it, and it reads its own ground first.
fn joined(
    read: &[(StructureSample, f32)],
    side: usize,
    band: &std::ops::RangeInclusive<f32>,
    in_stance: impl Fn(usize) -> bool,
    lowest: Option<&[f32]>,
) -> Vec<Option<Joined>> {
    let mut joined = steps(read, side, band, in_stance, |next, height, on_structure| {
        on_structure || lowest.is_none_or(|lowest| height >= lowest[next])
    });
    // Surfaces over open space going on from it: each joined at its top, a
    // fixed height, so which neighbour reached it first changes nothing.
    let mut open: Vec<usize> = (0..read.len()).filter(|&i| joined[i].is_some()).collect();
    while let Some(i) = open.pop() {
        let Some(here) = joined[i] else { continue };
        for next in neighbours(i, side).into_iter().flatten() {
            let Some(over) = read[next].0.over.filter(|_| joined[next].is_none()) else {
                continue;
            };
            if over.underside - here.height < OPEN_UNDER && band.contains(&(over.top - here.height))
            {
                joined[next] = Some(Joined {
                    height: over.top,
                    on_structure: true,
                });
                open.push(next);
            }
        }
    }
    joined
}

/// [`fill_terrain_ground`] for a body resting on a structure (#1834), eased
/// with the terrain's own ground by `share`.
///
/// **Every sample is the highest ground a foot stands on there.** The
/// structure is read as a body on the terrain reads one ([`below`]), and the
/// terrain under it, and a sample takes the higher. Ground joined to the
/// structure under the body's feet, neighbour to neighbour within a step
/// ([`joined`]), is taken as it is, however it curves away from any plane:
/// a road's ribbon on a hill, over a crest or in a sag (#1834 review, finding
/// 2), its curbs and the terrain past them, the hill around a stepping stone
/// (finding 1), a beam rising past the floor (#1884). Where the structure
/// spans the stance, the terrain is joined only within a pelvis drop under
/// the structure nearest it (#1883, [`lowest_joined`]), so the hill round a
/// small platform is not followed down under its downhill edge, nor the
/// ground under a deck's edge beside a ramp down from it, and the terrain
/// beside a road over a crest is, and the ground at the ramp's foot. Ground
/// not joined to it - past a drop, or the other side of a wall - is taken if
/// it is no more than [`FootingConfig::max_pelvis_drop`] under the plane the
/// body stands on and no more than a step over it; otherwise, and inside a
/// wall, the plane, [`PAST_AN_EDGE`] under it past a drop. So a floor's
/// walls - a ray started inside a solid one - and the terrain under the
/// floor are not stood on, and a beast in a room stands on its floor; and the
/// legs over a deck's edge, the ground a metre under them, are held at the
/// deck's plane.
///
/// **The plane is the structure's under the body's feet, where it spans
/// them** ([`Plane::fit`]): fitted through every structure sample within its
/// stance joined before any cut. On a deck or a floor it is that floor; on a road
/// climbing a hill it climbs with it. Near level it is level: a floor whose
/// ray reads a millimetre off is handed that millimetre, and nothing
/// switches on it. **Where the structure does not span the stance both
/// ways** - a log under a horse's belly, a stepping stone under a dog on a
/// hill (finding 1) - the body stands on the ground beside it as much as on
/// it, and the plane is that ground's, fitted through the stance's samples
/// of the terrain within the step's band of the root's floor, if they span
/// it; if they do not either, the structure's, tilted only along a way it
/// spans - a beam's length.
///
/// A floor of a trimesh is hollow to a ray ([`below`]), so a sample in a
/// voxel shell's walls reads the shell's base, joined to the floor or not;
/// either way it is never the terrain under the floor.
fn fill_structure_ground(
    drive: &mut Drive,
    rig: &Rig,
    root: &GlobalTransform,
    heightmap: &FinishedHeightMap,
    reach: Rect,
    share: Share,
    structures: &[Structure<'_>],
) {
    let affine = root.affine();
    let (start, across, along) = (
        Vec3::from(affine.translation),
        Vec3::from(affine.matrix3.x_axis),
        Vec3::from(affine.matrix3.z_axis),
    );
    let footing = FootingConfig::default();
    let step_up = footing.max_step_up + RAY_HEADROOM;
    // Each sample read once, by its index in the grid's own order: the
    // structure's ground, and the terrain's while the terrain's is eased.
    let mut read: Vec<(StructureSample, f32)> = Vec::new();
    let mut patch = drive.ground.take().unwrap_or_default();
    patch.resample(reach, GROUND_STEP, |at| {
        let world = start + across * at.x + along * at.y;
        let terrain = heightmap.world_height_at(world.x, world.z);
        let from = Vec3::new(world.x, terrain.max(start.y) + step_up, world.z);
        let (structure, terrain_ground) = if structures.is_empty() {
            (
                structure_sample((Below::Nothing, None), terrain, world.y),
                terrain,
            )
        } else {
            let open = terrain.max(start.y);
            let found = below_and_over(structures, from, terrain, open);
            let terrain_ground = if share.terrain <= 0.0 {
                terrain
            } else if open == terrain {
                terrain_surface(found.0, terrain)
            } else {
                terrain_surface(below(structures, from, terrain, terrain), terrain)
            };
            (structure_sample(found, terrain, world.y), terrain_ground)
        };
        read.push((structure, terrain_ground - world.y));
        0.0
    });
    let (side, origin, step) = (patch.side(), patch.origin(), patch.step());
    let place = |i: usize| patch.sample_at(i % side, i / side);
    let band = -footing.max_pelvis_drop..=step_up;
    let stance = GroundPatch::stance(rig).map_or(reach, |stance| stance.inflate(GROUND_STEP * 0.5));
    let in_stance = |i: usize| stance.contains(place(i));
    let by_steps = joined(&read, side, &band, in_stance, None);
    let covered = (0..read.len()).filter(|&i| in_stance(i)).map(place);
    let on = Plane::fit(
        by_steps.iter().enumerate().filter_map(|(i, joined)| {
            joined
                .filter(|joined| joined.on_structure && in_stance(i))
                .map(|joined| (place(i), joined.height))
        }),
        covered.clone(),
    );
    // Standing on the structure, the terrain past a pelvis drop under the
    // structure nearest it is past its edge (#1883, [`joined`]): joined
    // again with the cut, where the steps reached any such terrain. Where
    // they reached none the cut join is this one: the cut admits or refuses
    // a sample by itself alone, so a walk refusing none of the samples the
    // uncut one reached reaches every one of them, and nothing else.
    let lowest = on
        .spans
        .then(|| lowest_joined(&read, side, &band, in_stance));
    let past_a_drop = lowest.as_deref().is_some_and(|lowest| {
        by_steps.iter().zip(lowest).any(|(joined, lowest)| {
            joined.is_some_and(|joined| !joined.on_structure && joined.height < *lowest)
        })
    });
    let joined_on_it;
    let (plane, joined) = if past_a_drop {
        joined_on_it = joined(&read, side, &band, in_stance, lowest.as_deref());
        (on.plane, &joined_on_it)
    } else if on.spans {
        (on.plane, &by_steps)
    } else {
        let beside = Plane::fit(
            read.iter().enumerate().filter_map(|(i, (sample, _))| {
                sample
                    .height
                    .filter(|h| !sample.on_structure && band.contains(h) && in_stance(i))
                    .map(|h| (place(i), h))
            }),
            covered,
        );
        (
            if beside.spans { beside.plane } else { on.plane },
            &by_steps,
        )
    };
    let heights = read
        .iter()
        .enumerate()
        .map(|(i, (sample, terrain))| {
            let at = place(i);
            let level = plane.at(at);
            let ground = match (joined[i], sample.height) {
                (Some(joined), _) => joined.height,
                (None, Some(h)) if h < level - footing.max_pelvis_drop => level - PAST_AN_EDGE,
                (None, Some(h)) if h <= level + step_up => h,
                _ => level,
            };
            ground * share.structure + terrain * share.terrain
        })
        .collect();
    drive.ground = Some(GroundPatch::new(origin, step, side, heights));
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

/// Whether the terrain is under a body posed at `root` (#1835 review): the
/// nearest the terrain comes to `rig`'s lying capsule, along its length from
/// cap to cap, is no further than a peer resting on it may fall
/// ([`RESTING_WITHIN`] and [`RESTING_BAND`]). A step the physics loses a body
/// on a hill leaves it millimetres over it; a body in the air, or on a deck or
/// a boat over the terrain, is further.
///
/// **Measured at right angles to the ground, every [`UNDER_STEP`]** (#1835
/// end review, finding 1). Each sample is the terrain under a point of the
/// capsule's centre line, and its clearance is how far the capsule stands off
/// the plane the ground lies in there: the distance from the nearest point of
/// its axis to that plane, less its radius. Read straight down the centre line
/// instead, a capsule lying across a hill - touching on its uphill side - read
/// its radius times `1 / cos - 1` over the ground, 6.9 cm for a horse across
/// 30 degrees and 9.8 across 35, and a crest's kink between two samples read
/// the slope over the gap: both stood a horse resting on the terrain on the
/// level floor. A crest read through the plane of one of its sides is read a
/// little nearer than it is, never further, so a body on one is held. A rig
/// with no trunk to lie along is asked at its root.
///
/// **Read loosely of a body the physics has on the terrain** (`exact` false):
/// its contacts are the answer, and asked here only because they are a step
/// old - a body set down a metre up between two steps is still read on the
/// terrain under it for the frame between. So it is five samples straight
/// down its cylinder, against [`STALE_WITHIN`], which no capsule resting on a
/// hill or a crest comes near, at a twentieth of the exact reading's cost to
/// a body on the terrain every frame.
fn terrain_under(
    rig: &Rig,
    root: &GlobalTransform,
    heightmap: &FinishedHeightMap,
    exact: bool,
) -> bool {
    let affine = root.affine();
    let (start, along) = (
        Vec3::from(affine.translation),
        Vec3::from(affine.matrix3.z_axis),
    );
    let (from, to, radius) = LyingCapsule::of(rig).map_or((0.0, 0.0, 0.0), |capsule| {
        (capsule.from, capsule.to, capsule.radius)
    });
    if !exact {
        let over = (0..=4u8)
            .map(|i| {
                let at = start + along * (from + (to - from) * f32::from(i) / 4.0);
                start.y - heightmap.world_height_at(at.x, at.z)
            })
            .fold(f32::INFINITY, f32::min);
        return over <= STALE_WITHIN;
    }
    // The ground's plane at a point: the bilinear terrain's own slope a few
    // centimetres either way, and not the heightmap's normal, which is read
    // over a whole cell and rounds a crest off.
    let ground = |x: f32, z: f32| {
        let height = |x: f32, z: f32| heightmap.world_height_at(x, z);
        let e = UNDER_STEP * 0.5;
        let slope = Vec2::new(
            (height(x + e, z) - height(x - e, z)) / (2.0 * e),
            (height(x, z + e) - height(x, z - e)) / (2.0 * e),
        );
        (
            Vec3::new(x, height(x, z), z),
            Vec3::new(-slope.x, 1.0, -slope.y).normalize(),
        )
    };
    let (low, high) = (from - radius, to + radius);
    let samples = ((high - low) / UNDER_STEP).ceil().max(0.0) as usize;
    let nearest = (0..=samples)
        .map(|i| {
            let z = if samples == 0 {
                low
            } else {
                low + (high - low) * i as f32 / samples as f32
            };
            let at = start + along * z;
            let (point, normal) = ground(at.x, at.z);
            let axis = start + along * z.clamp(from, to) + Vec3::Y * radius;
            normal.dot(axis - point) - radius
        })
        .fold(f32::INFINITY, f32::min);
    nearest <= RESTING_WITHIN + RESTING_BAND
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
/// [`Resting`]: the lying capsule it casts, cut once from the rig
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

impl RestCast {
    /// The lying capsule `rig`'s body casts, cut afresh - and the last cast
    /// forgotten - when its trunk is another shape than the one kept; `None`
    /// for a rig with no trunk to lie along. The trunk is read every frame -
    /// a few dozen joints - so a rig swapped under a kept trail is never cast
    /// as the old one's capsule.
    fn capsule_of(&mut self, rig: &Rig) -> Option<&Collider> {
        let Some(shape) = LyingCapsule::of(rig) else {
            *self = Self::default();
            return None;
        };
        if self.capsule.as_ref().is_none_or(|(cut, _)| *cut != shape) {
            *self = Self {
                capsule: Some((shape, shape.collider())),
                last: None,
            };
        }
        self.capsule.as_ref().map(|(_, capsule)| capsule)
    }
}

/// How a body moves this frame, as [`RestHold::settle`] reads it.
#[derive(Clone, Copy, Debug, Default)]
struct Moving {
    /// Its speed up, m/s.
    vertical: f32,
    /// Whether its drive carries a flight (#1877).
    aloft: bool,
    /// Whether a take-off has begun and its ease not yet (#1887, [`Launch`]):
    /// from its first frame until the body lifts. Its first frame is the one
    /// its drive's flight turns a takeoff, or - for a take-off from a held
    /// landing - the frame of no flight before that (`aloft` false), on which
    /// the chassis has already leapt. Never a landing held on the ground,
    /// which keeps the physics' vertical speed, so that one sliding up a hill
    /// rises past the launch speed with its feet on the ground.
    takes_off: bool,
}

/// What a winged body's take-off has got to (#1887): read off its drive's
/// flight once a frame by [`Underfoot::fill`], kept in [`Resting`], and
/// handed to [`RestHold::settle`] as [`Moving::takes_off`].
///
/// **A take-off is latched from its first frame until the body lifts.** It
/// begins on the frame the flight turns a takeoff, or on the frame of no
/// flight a take-off from a held landing is handed first (`super::aloft`,
/// "A takeoff from a held landing") - the frame after a landing - and is
/// pending until a takeoff frame finds the body not coming down, the frame
/// its ease starts, or until the flight is neither a takeoff nor that frame.
/// Read on its first frame alone, a take-off whose body dipped on that frame
/// (contact jitter, a peer's playout standing still) was never eased, and
/// one from a held landing found the ground already handed over on the frame
/// of no flight, its ease with nothing left to carry.
#[derive(Clone, Copy, Debug, Default)]
struct Launch {
    /// What the drive's flight was last frame.
    flew: Option<FlightMode>,
    /// Whether a take-off begun has not yet found its body lifting.
    pending: bool,
}

impl Launch {
    /// Read this frame's `flight` and the body's speed up, `vertical`: whether
    /// a take-off has begun and its ease not yet ([`Moving::takes_off`]).
    fn read(&mut self, flight: Option<FlightMode>, vertical: f32) -> bool {
        let flew = std::mem::replace(&mut self.flew, flight);
        let takeoff = flight == Some(FlightMode::Takeoff);
        // The frame of no flight between a held landing and its takeoff.
        let afresh = flight.is_none() && flew == Some(FlightMode::Land);
        let takes_off = afresh || (takeoff && (flew != Some(FlightMode::Takeoff) || self.pending));
        self.pending = takes_off && !(takeoff && vertical >= 0.0);
        takes_off
    }
}

/// The answer a body stands by of what it rests on, and how much of the
/// terrain's ground and the structure's its patch still carries (#1835,
/// #1834): what keeps a step the physics loses the body from handing it the
/// level floor for that step. Read and moved on by [`Underfoot::fill`] once
/// a frame.
///
/// Kept for every body, simulated here or not, and across a rebuild with the
/// rest of the trail. A body's first frame, and its first after it is set
/// down somewhere else ([`Resting::drawn_at`]), stands by its first answer
/// at once, so a beast arriving on a hill is drawn on it from its first
/// frame.
#[derive(Clone, Copy, Default)]
struct RestHold {
    /// The answer stood by, or `None` before the first.
    held: Option<RestsOn>,
    /// How long the physics has said otherwise, seconds.
    against: f32,
    /// How much of the terrain the patch carries, `0..=1`, before the ease at
    /// its ends: all of it once on the terrain, and easing toward none off it
    /// and toward all of it onto it.
    share: f32,
    /// How much of the structure's ground the patch carries (#1834), as
    /// [`Self::share`] does the terrain's; the two add up to no more than
    /// the whole, and easing from one to the other, to the whole.
    structure: f32,
    /// Off the terrain, whether the body has moved as the engine counts a body
    /// in the air - launched up or falling down faster than its
    /// [`DriverConfig`] says - so that its arrival is a landing.
    airborne: bool,
    /// Whether the ground is easing out under a winged body's take-off
    /// (#1887): from the take-off's first frame, over [`TAKEOFF_SECS`],
    /// carried up with the body ([`Resting::lifted_from`]), and not eased
    /// at its ends ([`Self::shares`]).
    taking_off: bool,
    /// Whether this frame is a take-off's frame of no flight, from a held
    /// landing (#1887, [`Launch`]): the shares handed as they were last
    /// frame, the ground carried up with the body as through the ease
    /// ([`Resting::lifted_from`]), and nothing else moved on - so a frame of
    /// no flight that is no take-off after all leaves the hold as it was.
    afresh: bool,
}

impl RestHold {
    /// Take this frame's answer, `delta` seconds after the last, and say how
    /// much of the terrain's ground and the structure's the patch carries
    /// this frame. `under` says whether the terrain is under the body, asked
    /// loosely of one read on the terrain and exactly of one read elsewhere
    /// while its patch carries some of it ([`terrain_under`]); `on_structure`
    /// whether a structure still is, asked only of a body standing by one
    /// while the physics says it rests elsewhere ([`Underfoot::structure_under`]).
    ///
    /// **Only leaving a ground is held, and only while it is under the
    /// body** (#1835 review, findings 1 and 2). A lost step on a hill leaves
    /// the body where it was, its capsule a few millimetres off the ground,
    /// and is held. A body that rises off it or falls from over it - a jump, a
    /// walk off a ledge, a push, a respawn set down a metre up - has the
    /// terrain fall away from under it, and is handed the level floor at
    /// once: held, its feet stayed planted on the terrain as it fell, 0.86 m
    /// under a dog's root and 1.25 m under a horse's, for the engine counts
    /// a body falling only once it falls 2 m/s. A structure is left the same
    /// way (#1834), and leaving the terrain for one is held as leaving it for
    /// the level floor is: the step a hill's contact drops for while the
    /// capsule's end touches a pad is the same lost step.
    ///
    /// **Arriving is eased in, unless it is a landing** (#1835 end review,
    /// findings 2 and 3). A body stepping off a pad's side onto the hill
    /// below, or straddling a pad's edge while the pad's contact drops for a
    /// step, comes onto the terrain from the level floor without the engine
    /// counting it in the air, and taken at once its trunk turned 11 to 18
    /// degrees in a frame; so the patch eases in over [`BLEND_SECS`], from
    /// wherever an ease out had got to. A body the engine counted in the air
    /// (`vertical`, its speed up, past [`DriverConfig`]'s launch or fall)
    /// lands, and the engine eases its own landing: held on the level floor
    /// instead, a body landing on a hill stood its soles on the air over it
    /// and in it, 0.73 m and 0.43 m, for a third of a second. So a landing is
    /// handed the terrain at once. Arriving on a structure is the same
    /// (#1834); from the terrain, its ground eases out as the structure's
    /// eases in, the two always adding up to the whole, so a beast walking
    /// from a hill onto a pad turns its trunk from the one to the other over
    /// the ease, as it once turned it level. **Not eased in: a structure
    /// arrived on once the terrain is no longer under the body** - a pad
    /// standing proud of a hill, walked onto from it - which is handed over
    /// whole at once, the terrain's share going to nothing in the same frame,
    /// as the terrain fallen away from under a body always is; before #1834
    /// the level floor was handed over there the same way.
    ///
    /// **A winged body flown there lands** (#1877): `aloft` - its drive
    /// carries a flight - counts it in the air whatever its vertical speed,
    /// as a gliding bat comes down onto a roof at about 1.3 m/s, under the
    /// engine's fall, and eased in it stood its landing on the level floor.
    ///
    /// **But a winged body taking off eases the ground out** (#1887): on the
    /// first frame of a take-off begun ([`Moving::takes_off`]) that its body
    /// is not coming down, off a ground its patch carried some of, the
    /// terrain's share and the structure's ease out over [`TAKEOFF_SECS`]
    /// from what was `handed` the frame before - which a landing's ease back
    /// in once it is level ([`Resting::levelling`]) may have held under the
    /// hold's own - whatever the physics reads meanwhile ([`Self::lift`]),
    /// until the ease has run, its flight ends or it comes down;
    /// [`Underfoot::fill`] carries the ground up with the body as it does.
    /// The engine's take-off pushes from the feet over that time, so its feet
    /// are on the ground: handed the level floor at once, a bat taking off
    /// from a 20 degree hillside under its owner's chassis moved a joint
    /// 93 mm further than on level ground on that frame at 144 Hz and 92 at
    /// 60, and a glider 147 and 138 (151 at 60 as a peer); eased, 12 and 30,
    /// and 9 and 22. A take-off from a held landing begins on its frame of
    /// no flight (`aloft` false), which hands what was handed and nothing
    /// more ([`Self::afresh`]): its chassis has leapt on it, and read as any
    /// other frame, the terrain fallen from under it was gone at once.
    /// Only a take-off begun starts the ease: a landing held on the
    /// ground keeps the physics' vertical speed, and gated on that, a bat
    /// sliding up a hill in its held landing was eased out as if taking off,
    /// passed by the withhold of a tilted body's patch, and handed the
    /// terrain back whole as the ease ran out. A take-off the physics still
    /// reads on the ground when its ease has run eases it back in over
    /// [`BLEND_SECS`]. Anything else rising off the ground or falling from
    /// over it - a body not flying, or one coming down - is handed the level
    /// floor at once, as above.
    fn settle(
        &mut self,
        rests: RestsOn,
        under: impl FnOnce(bool) -> bool,
        on_structure: impl FnOnce() -> bool,
        moving: Moving,
        handed: Share,
        delta: f32,
    ) -> Share {
        // A long frame - a tab in the background, a hitch - is not a long
        // hold: what the body did over it is not known (finding 4).
        let delta = delta.min(LONGEST_FRAME);
        self.afresh = false;
        let config = DriverConfig::default();
        let Moving {
            vertical,
            aloft,
            takes_off,
        } = moving;
        let flying = aloft || vertical > config.launch_speed || vertical < -config.fall_speed;
        // Rising is not coming down: a peer's playout, read a frame at a
        // time, stands still for a frame now and then as it rises.
        let lifting = aloft && vertical >= 0.0;
        // A take-off's ease runs on while the body flies and does not come
        // down; one whose flight ends, or that falls, meanwhile is handed the
        // level floor at once, as any body the ground has fallen away from.
        // Run out with the physics still reading it on the ground - a take-off
        // under a ceiling - the hold eases that ground back in, as an arrival
        // that is no landing does, rather than counting the body in the air,
        // which handed it back whole on the next frame.
        if self.taking_off {
            if !lifting {
                self.level(flying);
            } else if self.lift(delta) {
                let grounded = match rests {
                    RestsOn::Terrain => under(false),
                    RestsOn::Structure => true,
                    RestsOn::Elsewhere => false,
                };
                *self = Self {
                    airborne: !grounded,
                    ..Self::LEVEL
                };
            }
            return self.shares();
        }
        // A take-off begun, off a ground the patch carried: eased out from
        // what it carried as handed, whatever the physics reads meanwhile -
        // the frames a peer's cast, or a step-old contact, still reads it
        // there, held whole, put its feet on the ground it rose off.
        let carried = Share {
            terrain: self.shares().terrain.min(handed.terrain),
            structure: self.shares().structure.min(handed.structure),
        };
        let carries = carried.terrain > 0.0 || carried.structure > 0.0;
        // Its frame of no flight, from a held landing, the chassis already
        // leapt: held as handed, and the ground carried up with the body,
        // until its takeoff starts the ease on the next frame. Read as it
        // is, the 11 cm a chassis rises over that frame at 30 a second took
        // the terrain from under it and handed the level floor at once.
        if takes_off && !aloft && carries && vertical >= 0.0 {
            self.afresh = true;
            return carried;
        }
        if takes_off && lifting && carries {
            *self = Self {
                held: Some(RestsOn::Elsewhere),
                against: 0.0,
                share: carried.terrain,
                structure: carried.structure,
                airborne: true,
                taking_off: true,
                afresh: false,
            };
            if self.lift(delta) {
                self.level(true);
            }
            return self.shares();
        }
        // Asked of a body read on the terrain too: contacts are a step old.
        let under = if rests == RestsOn::Terrain {
            under(false)
        } else {
            self.share > 0.0 && under(true)
        };
        let ease = delta / BLEND_SECS;
        match (self.held, rests) {
            (_, RestsOn::Terrain) if !under => self.level(flying),
            (None, RestsOn::Terrain) => {
                *self = Self {
                    held: Some(RestsOn::Terrain),
                    share: 1.0,
                    ..Self::default()
                }
            }
            (Some(_), RestsOn::Terrain) => {
                let (share, structure) = if self.airborne {
                    (1.0, 0.0)
                } else {
                    (
                        (self.share + ease).min(1.0),
                        (self.structure - ease).max(0.0),
                    )
                };
                *self = Self {
                    held: Some(RestsOn::Terrain),
                    share,
                    structure,
                    ..Self::default()
                };
            }
            (None, RestsOn::Structure) => {
                *self = Self {
                    held: Some(RestsOn::Structure),
                    structure: 1.0,
                    ..Self::default()
                }
            }
            (Some(RestsOn::Terrain), RestsOn::Structure) if under => {
                self.hold(flying, delta, rests)
            }
            (Some(_), RestsOn::Structure) => {
                // The terrain fallen away from under a body that was on it
                // is gone at once, as it is from one off the ground.
                let (share, structure) = if self.airborne || (self.share > 0.0 && !under) {
                    (0.0, 1.0)
                } else {
                    (
                        (self.share - ease).max(0.0),
                        (self.structure + ease).min(1.0),
                    )
                };
                *self = Self {
                    held: Some(RestsOn::Structure),
                    share,
                    structure,
                    ..Self::default()
                };
            }
            (None, RestsOn::Elsewhere) => self.level(flying),
            (Some(_), RestsOn::Elsewhere) if self.share > 0.0 && !under => self.level(flying),
            (Some(_), RestsOn::Elsewhere) if self.structure > 0.0 && !on_structure() => {
                self.level(flying)
            }
            (Some(RestsOn::Terrain | RestsOn::Structure), RestsOn::Elsewhere) => {
                self.hold(flying, delta, rests)
            }
            (Some(RestsOn::Elsewhere), RestsOn::Elsewhere) => {
                self.airborne |= flying;
                self.share = (self.share - ease).max(0.0);
                self.structure = (self.structure - ease).max(0.0);
            }
        }
        self.shares()
    }

    /// The shares the patch carries this frame: eased at both ends, so the
    /// trunk starts to turn and stops turning without a jolt, nothing at all
    /// at either end, and easing from one ground to the other the two still
    /// add up to the whole - except through a take-off's ease (#1887), which
    /// is carried as it runs, straight: it has the push's two frames at 30 a
    /// second, and eased at both ends their middle step was half again as
    /// long.
    fn shares(&self) -> Share {
        if self.taking_off {
            return Share {
                terrain: self.share,
                structure: self.structure,
            };
        }
        let eased = |t: f32| t * t * (3.0 - 2.0 * t);
        Share {
            terrain: eased(self.share),
            structure: eased(self.structure),
        }
    }

    /// A take-off's ease, `delta` seconds on (#1887): both grounds' shares
    /// toward none over [`TAKEOFF_SECS`], from wherever they started;
    /// whether they have got there.
    fn lift(&mut self, delta: f32) -> bool {
        let ease = delta / TAKEOFF_SECS;
        self.share = (self.share - ease).max(0.0);
        self.structure = (self.structure - ease).max(0.0);
        self.share <= 0.0 && self.structure <= 0.0
    }

    /// Stand by the answer held while the physics says `rests`, until it has
    /// said so for [`HOLD_SECS`]; then stand by that. A share still easing
    /// in is held where it has got to.
    fn hold(&mut self, flying: bool, delta: f32, rests: RestsOn) {
        self.airborne |= flying;
        self.against += delta;
        if self.against >= HOLD_SECS {
            self.held = Some(rests);
            self.against = 0.0;
        }
    }

    /// Stand the body on the level floor at once, with nothing to ease out
    /// of, remembering whether it is moving as a body in the air does.
    fn level(&mut self, flying: bool) {
        *self = Self {
            airborne: self.airborne || flying,
            ..Self::LEVEL
        };
    }

    /// A body standing on the level floor with nothing to ease out of.
    const LEVEL: Self = Self {
        held: Some(RestsOn::Elsewhere),
        against: 0.0,
        share: 0.0,
        structure: 0.0,
        airborne: false,
        taking_off: false,
        afresh: false,
    };

    /// Whether the answer stood by is a ground a patch is handed for: the
    /// terrain or a structure.
    fn on_ground(&self) -> bool {
        matches!(self.held, Some(RestsOn::Terrain | RestsOn::Structure))
    }
}

/// What a body remembers of what it rests on, kept in its
/// [`super::RiggedTrail`]: its last cast, for one the physics does not
/// simulate here (#1833, [`RestCast`]), and the answer it stands by with how
/// much of the terrain its patch still carries, for every body (#1835,
/// [`RestHold`]) - and a winged body's ease back onto its patch once it is
/// level (#1877).
#[derive(Clone, Default)]
pub(super) struct Resting {
    cast: RestCast,
    hold: RestHold,
    /// Where the body was drawn last frame: a body drawn further than
    /// [`super::motion::PLACED_PAST`] from it has been set down somewhere
    /// else.
    drawn: Option<Vec3>,
    /// How long a flying body's root has been level since its patch was last
    /// withheld for its tilt (#1877, s), while its patch eases back in over
    /// [`BLEND_SECS`]; `None` once it has, and for every body that has not
    /// flown.
    levelling: Option<f32>,
    /// The height a winged body's root was drawn at on the last frame before
    /// its take-off's ease began (#1887), while it runs - and from the frame
    /// of no flight before it, for a take-off from a held landing
    /// ([`RestHold::afresh`]): the ground eased out
    /// is sampled through the root held at that height - its place across
    /// the ground and its turn this frame's - and so carried up with the body
    /// as the level floor is, and no foot is held to the ground it rises off.
    lifted_from: Option<f32>,
    /// The shares the patch carried last frame, as handed - after the
    /// withhold and the ease back in of [`Self::levelling`]: what a take-off's
    /// ease starts from (#1887, [`RestHold::settle`]).
    handed: Share,
    /// What its take-off has got to (#1887): latched from its first frame
    /// until the body lifts.
    launch: Launch,
}

impl Resting {
    /// Note where the body is drawn this frame, and if it has been set down
    /// somewhere else since the last (#1835 review, finding 3), forget what it
    /// stood by where it was, so it stands by this frame's answer at once.
    /// Read off the drawn pose and not the chassis' `GlobalTransform`, which
    /// for a peer is a frame behind: read there, a peer set down on a floor
    /// was held on the terrain it left for the frame between.
    ///
    /// **Set down is further than its `speed` took it over the frame's
    /// `delta`**, by [`super::motion::PLACED_PAST`] (#1835 end review,
    /// finding 4): a galloping body over a fifth of a second's hitch travels
    /// that far, and is not set down anywhere.
    fn drawn_at(&mut self, at: Vec3, speed: f32, delta: f32) {
        let travel = speed * delta + super::motion::PLACED_PAST;
        if self.drawn.is_some_and(|was| was.distance(at) > travel) {
            self.hold = RestHold::default();
        }
        self.drawn = Some(at);
    }
}

#[cfg(test)]
impl Resting {
    /// How many frames running the last cast has been read back, or `None`
    /// with no cast kept: for a test to tell a reused answer from a fresh one.
    pub(super) fn reused(&self) -> Option<u32> {
        self.cast.last.map(|last| last.reused)
    }

    /// Forget the last cast, so the next frame casts afresh: for a test to
    /// put a peer's answer through on the frame it changes.
    pub(super) fn forget_cast(&mut self) {
        self.cast.last = None;
    }

    /// How much of the terrain the patch carries, before the ease at its
    /// ends, and whether the answer stood by is the terrain.
    pub(super) fn held(&self) -> (f32, bool) {
        (self.hold.share, self.hold.held == Some(RestsOn::Terrain))
    }

    /// The answer stood by, and how much of the structure's ground the patch
    /// carries, before the ease at its ends (#1834).
    pub(super) fn stands_by(&self) -> (Option<RestsOn>, f32) {
        (self.hold.held, self.hold.structure)
    }
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

impl Underfoot<'_, '_> {
    /// Hand the body under `chassis`, posed at `root` this frame, `delta`
    /// seconds after the last, the ground under it: [`fill_terrain_ground`]
    /// with what the physics has it resting on - the terrain, a structure or
    /// elsewhere - held and eased (#1835, #1834, [`RestHold`]), and the
    /// structures within its reach. `memory` is the body's own [`Resting`].
    ///
    /// Nothing is asked of the physics for a body that does not stand on the
    /// terrain at all - a person, a swimmer - so a person's frame is what it
    /// was before #1833 to the bit.
    ///
    /// **A winged body flying, or landing, is handed no patch until its root
    /// is level** (#1877, [`LEVEL_WITHIN`]): every patch is sampled for a root
    /// turned about `Y` alone ([`fill_terrain_ground`]), and a landing's
    /// chassis is still pitched and banked for the first tenth of a second
    /// it is down. What it rests on is still read and held meanwhile, and
    /// from the frame it is level the patch eases in over [`BLEND_SECS`], as
    /// an arrival that is no landing does: handed whole on that frame, a bat
    /// come down across a 20 degree hillside moved a joint 330 mm in it,
    /// against at most 63 in the four frames before and 133 in the four
    /// after, and a glider 176 against 33 and 33 (symbios-avatar 0.23.1);
    /// eased in, 64 against 63 and 77, and 34 against 33 and 42.
    ///
    /// **Not withheld from a take-off's ease** (#1887, [`RestHold::settle`]):
    /// a winged body taking off pitches up as its feet still push, past the
    /// level within a frame or two, and withheld there its ground went in a
    /// frame as the hold's once did. Through the ease the patch is sampled
    /// through the root as it is drawn this frame - where it is across the
    /// ground and how it is turned - but held at the height it was drawn at
    /// on the last frame before the ease ([`Resting::lifted_from`]), so the
    /// ground is carried up with the body: the level floor pitches and rises
    /// with the chassis, and so does what eases out to it. Sampled at the
    /// root's own height, a glider taking off from a pitched roof at 144 Hz
    /// moved a joint 96 mm further than on level ground on one frame, against
    /// 9 held. A take-off from a held landing has its ground held at that
    /// height from its frame of no flight, before the ease
    /// ([`RestHold::afresh`]), and handed on that frame as it was handed the
    /// frame before, a landing's ease back in left where it was.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn fill(
        &self,
        drive: &mut Drive,
        rig: &Rig,
        chassis: Entity,
        root: &GlobalTransform,
        heightmap: Option<&FinishedHeightMap>,
        memory: &mut Resting,
        delta: f32,
    ) {
        let was = memory.drawn;
        memory.drawn_at(root.translation(), drive.velocity.length(), delta);
        let takes_off = memory
            .launch
            .read(drive.flight.map(|flight| flight.mode), drive.velocity.y);
        let mut lifted = None;
        let share = if let Some(heightmap) = wanted(drive, rig, heightmap) {
            let rests = self.rests_on(chassis, rig, root, memory);
            let cast = &mut memory.cast;
            let aloft = drive.flight.is_some();
            let share = memory.hold.settle(
                rests,
                |exact| terrain_under(rig, root, heightmap, exact),
                || self.structure_under(chassis, rig, root, cast),
                Moving {
                    vertical: drive.velocity.y,
                    aloft,
                    takes_off,
                },
                memory.handed,
                delta,
            );
            let carried_up = memory.hold.taking_off || memory.hold.afresh;
            memory.lifted_from = carried_up.then(|| {
                memory
                    .lifted_from
                    .or(was.map(|was| was.y))
                    .unwrap_or(root.translation().y)
            });
            lifted = memory.lifted_from;
            if memory.hold.taking_off {
                memory.levelling = None;
                share
            } else if memory.hold.afresh {
                // Handed as last frame, a landing's ease back in already
                // in it: left where it was, so a frame of no flight that is
                // no take-off goes on with it.
                share
            } else if aloft && (root.rotation() * Vec3::Y).y < LEVEL_WITHIN.cos() {
                memory.levelling = Some(0.0);
                Share::default()
            } else if let Some(since) = memory.levelling {
                let since = since + delta.min(LONGEST_FRAME);
                memory.levelling = (since < BLEND_SECS).then_some(since);
                let t = (since / BLEND_SECS).min(1.0);
                let eased = t * t * (3.0 - 2.0 * t);
                Share {
                    terrain: share.terrain * eased,
                    structure: share.structure * eased,
                }
            } else {
                share
            }
        } else {
            // A swimmer coming out is read afresh, not off a cast from
            // before, and so is a body that stood before the heightmap did.
            memory.cast.last = None;
            memory.hold = RestHold::default();
            memory.levelling = None;
            memory.lifted_from = None;
            Share::default()
        };
        memory.handed = share;
        match lifted {
            Some(from) => {
                let mut carried = root.affine();
                carried.translation.y = from;
                self.fill_share(drive, rig, &carried.into(), heightmap, share);
            }
            None => self.fill_share(drive, rig, root, heightmap, share),
        }
    }

    /// [`Self::fill`] for a body whose resting is already known: the render
    /// tool's walker, which reads it off the cast that set it down
    /// ([`Self::set_down`]). Native only, as the tool is: the game's own
    /// bodies go through [`Self::fill`], which holds and eases what they rest
    /// on (#1835), and the walker is set down afresh every frame.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn fill_resting(
        &self,
        drive: &mut Drive,
        rig: &Rig,
        root: &GlobalTransform,
        heightmap: Option<&FinishedHeightMap>,
        rests: RestsOn,
    ) {
        self.fill_share(drive, rig, root, heightmap, Share::of(rests));
    }

    /// [`fill_terrain_ground`] carrying `share` of the terrain's ground and
    /// the structure's, with the structures within the body's reach gathered
    /// whenever it carries any: a patch easing out to the level floor reads
    /// a pad by its rays to the end.
    fn fill_share(
        &self,
        drive: &mut Drive,
        rig: &Rig,
        root: &GlobalTransform,
        heightmap: Option<&FinishedHeightMap>,
        share: Share,
    ) {
        let carried = share.terrain > 0.0 || share.structure > 0.0;
        let structures = match (GroundPatch::reach(rig), wanted(drive, rig, heightmap)) {
            (Some(reach), Some(heightmap)) if carried => {
                self.structures_over(root, reach, heightmap)
            }
            _ => Vec::new(),
        };
        fill_terrain_ground(drive, rig, root, heightmap, share, &structures);
    }

    /// What the chassis rests on: its contacts, for one the physics simulates
    /// here ([`Self::contacts_say`]); for one it does not, its body's lying
    /// capsule cast down from a little above where it stands
    /// ([`Self::rest_of`]), or the last such cast's answer while it has not
    /// moved since ([`RestCast`]). This frame's answer, raw: [`RestHold`]
    /// decides what the body stands by.
    fn rests_on(
        &self,
        chassis: Entity,
        rig: &Rig,
        root: &GlobalTransform,
        memory: &mut Resting,
    ) -> RestsOn {
        // On the terrain or a structure, a peer's cast may fall a little
        // further and still rest there (#1835).
        let within = if memory.hold.on_ground() {
            RESTING_WITHIN + RESTING_BAND
        } else {
            RESTING_WITHIN
        };
        let memory = &mut memory.cast;
        if self.simulated.contains(chassis) {
            return self.contacts_say(chassis);
        }
        if memory.capsule_of(rig).is_none() {
            return RestsOn::Elsewhere;
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
        let rests = self.cast_within(capsule, root, within);
        memory.last = Some(LastRest {
            at,
            rotation,
            reused: 0,
            rests,
        });
        rests
    }

    /// What the owner's contacts say `chassis` rests on (#1833, #1834): the
    /// terrain if that is all it touches under it, a structure if it touches
    /// static solids under it, with or without the terrain, and elsewhere if
    /// it touches nothing under it, or anything that moves.
    pub(super) fn contacts_say(&self, chassis: Entity) -> RestsOn {
        let Some(contacts) = self.contacts.as_deref() else {
            return RestsOn::Elsewhere;
        };
        let (mut terrain, mut structure) = (false, false);
        for (other, _) in crate::player::resting_under(
            contacts,
            &self.sensors,
            chassis,
            crate::player::UNDER_NORMAL_Y,
        ) {
            match self.kind_of(other) {
                RestsOn::Terrain => terrain = true,
                RestsOn::Structure => structure = true,
                RestsOn::Elsewhere => return RestsOn::Elsewhere,
            }
        }
        match (terrain, structure) {
            (_, true) => RestsOn::Structure,
            (true, false) => RestsOn::Terrain,
            (false, false) => RestsOn::Elsewhere,
        }
    }

    /// What resting on the collider `entity` alone would be (#1834): the
    /// terrain, a structure - a static solid that is not a terrain, as
    /// [`Self::structures_over`] gathers them - or elsewhere: anything that
    /// moves, and a terrain on its way out, whose heightmap is no longer the
    /// one in hand.
    fn kind_of(&self, entity: Entity) -> RestsOn {
        if self.terrain.contains(entity) {
            return RestsOn::Terrain;
        }
        if self.any_terrain.contains(entity) {
            return RestsOn::Elsewhere;
        }
        let fixed = self.solids.get(entity).is_ok_and(|(.., of)| {
            of.is_none_or(|of| self.bodies.get(of.body).is_ok_and(RigidBody::is_static))
        });
        if fixed {
            RestsOn::Structure
        } else {
            RestsOn::Elsewhere
        }
    }

    /// Whether a structure is still under a body posed at `root` that stands
    /// by one while the physics says it rests elsewhere (#1834,
    /// [`RestHold`]): its lying capsule, cast down from [`CAST_RISE`] over it,
    /// comes to rest on one no further under it than a peer resting there may
    /// fall ([`RESTING_WITHIN`] and [`RESTING_BAND`]). The structure's own
    /// [`terrain_under`]: a step the physics loses the body on a road leaves
    /// it there, a jump off a deck or a fall from it does not. The chassis
    /// itself is passed over, for one simulated here carries a collider of
    /// that shape where it stands.
    fn structure_under(
        &self,
        chassis: Entity,
        rig: &Rig,
        root: &GlobalTransform,
        memory: &mut RestCast,
    ) -> bool {
        let within = RESTING_WITHIN + RESTING_BAND;
        let Some(capsule) = memory.capsule_of(rig) else {
            return false;
        };
        matches!(
            self.rest_of(capsule, root, CAST_RISE, within, Some(chassis)),
            Some((fell, RestsOn::Structure)) if fell <= within
        )
    }

    /// [`Self::cast_within`] [`RESTING_WITHIN`], as a body off the terrain is
    /// cast: for a test to read a fresh cast beside the fill's.
    #[cfg(test)]
    pub(super) fn resting_cast(&self, capsule: &Collider, root: &GlobalTransform) -> RestsOn {
        self.cast_within(capsule, root, RESTING_WITHIN)
    }

    /// What `capsule` posed at `root` rests on, cast down from [`CAST_RISE`]
    /// over it: [`Self::rest_of`]'s answer, if it fell no further than
    /// `within` - further, it is in the air. That is [`RESTING_WITHIN`], and
    /// [`RESTING_BAND`] more for a body standing by the terrain (#1835).
    fn cast_within(&self, capsule: &Collider, root: &GlobalTransform, within: f32) -> RestsOn {
        match self.rest_of(capsule, root, CAST_RISE, CAST_FALL, None) {
            Some((fell, rests)) if fell <= within => rests,
            _ => RestsOn::Elsewhere,
        }
    }

    /// Where a body's lying `capsule`, posed at `root` and raised `rise` over
    /// it, comes to rest cast straight down onto the world no further than
    /// `fall` under `root`, and what it rests on there: how far it fell below
    /// `root`, and what everything under it within [`TOUCHING_WITHIN`] of the
    /// first thing is - the terrain alone, static solids with or without the
    /// terrain (#1834), or anything else, as the owner's contacts are read
    /// ([`Self::contacts_say`]). `None` when nothing is under it that far
    /// down. `exclude` is passed over: the chassis' own collider, for one
    /// simulated here.
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
        exclude: Option<Entity>,
    ) -> Option<(f32, RestsOn)> {
        let spatial = self.spatial.as_ref()?;
        let (_, rotation, at) = root.to_scale_rotation_translation();
        let config = ShapeCastConfig {
            max_distance: rise + fall,
            ignore_origin_penetration: true,
            ..ShapeCastConfig::DEFAULT
        };
        let filter = match exclude {
            Some(entity) => SpatialQueryFilter::default().with_excluded_entities([entity]),
            None => SpatialQueryFilter::default(),
        };
        let (mut terrain, mut fixed, mut other) = (f32::INFINITY, f32::INFINITY, f32::INFINITY);
        spatial.shape_hits_callback(
            capsule,
            at + Vec3::Y * rise,
            rotation,
            Dir3::NEG_Y,
            &config,
            &filter,
            |hit| {
                if hit.distance > 0.0
                    && hit.normal1.y >= crate::player::UNDER_NORMAL_Y
                    && !self.sensors.contains(hit.entity)
                {
                    let nearest = match self.kind_of(hit.entity) {
                        RestsOn::Terrain => &mut terrain,
                        RestsOn::Structure => &mut fixed,
                        RestsOn::Elsewhere => &mut other,
                    };
                    *nearest = nearest.min(hit.distance);
                }
                true
            },
        );
        let first = terrain.min(fixed).min(other);
        if !first.is_finite() {
            return None;
        }
        let rests = if other <= first + TOUCHING_WITHIN {
            RestsOn::Elsewhere
        } else if fixed <= first + TOUCHING_WITHIN {
            RestsOn::Structure
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
        self.rest_of(&capsule, root, SET_DOWN_RISE, SET_DOWN_FALL, None)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// **What a body stands by eases between the grounds, never jumps**
    /// (#1834). A body at 60 frames a second is read on the terrain, then
    /// on a structure, then elsewhere, then on the structure again and back
    /// on the terrain, the ground it leaves always still under it: leaving
    /// the terrain or the structure is held [`HOLD_SECS`], and each change
    /// is eased over [`BLEND_SECS`], the one ground easing out as the other
    /// eases in. No frame moves either eased share more than the ease's
    /// steepest frame does, the two never add up to more than the whole, and
    /// each answer is stood on whole once the ease has run.
    #[test]
    fn the_grounds_ease_into_one_another() {
        const DELTA: f32 = 1.0 / 60.0;
        let mut hold = RestHold::default();
        let steepest = 1.5 * DELTA / BLEND_SECS + 1e-5;
        let mut was = Share::default();
        let mut first = true;
        // Each answer, how many frames it is read, whether leaving the last
        // is held, and what is stood on once the ease has run.
        for (rests, frames, held, whole) in [
            (RestsOn::Terrain, 30, false, Share::TERRAIN),
            (RestsOn::Structure, 60, true, Share::STRUCTURE),
            (RestsOn::Elsewhere, 60, true, Share::default()),
            (RestsOn::Structure, 60, false, Share::STRUCTURE),
            (RestsOn::Terrain, 60, false, Share::TERRAIN),
        ] {
            let mut share = was;
            for frame in 0..frames {
                share = hold.settle(rests, |_| true, || true, Moving::default(), was, DELTA);
                if !first {
                    let moved = (share.terrain - was.terrain)
                        .abs()
                        .max((share.structure - was.structure).abs());
                    assert!(
                        moved <= steepest,
                        "{rests:?}, frame {frame}: the shares moved {moved:.4} in a frame, from \
                         {was:?} to {share:?}"
                    );
                }
                assert!(
                    share.terrain + share.structure <= 1.0 + 1e-6,
                    "{rests:?}, frame {frame}: {share:?} adds up past the whole"
                );
                if held && frame + 1 < (HOLD_SECS / DELTA) as usize {
                    assert_eq!(
                        share, was,
                        "{rests:?}, frame {frame}: let go inside the hold"
                    );
                }
                first = false;
                was = share;
            }
            assert_eq!(share, whole, "{rests:?}: not stood on whole after the ease");
        }
    }

    /// **A winged body flown down onto the terrain lands on it at once**
    /// (#1877): told it was aloft, a body coming down at a gliding bat's
    /// 1.3 m/s - under the engine's fall speed - is handed the terrain on the
    /// frame it is on it, as a body falling faster always was. Not told, it
    /// was eased in over [`BLEND_SECS`], its landing stood on the level floor.
    #[test]
    fn a_body_flown_down_lands_on_the_terrain_at_once() {
        const DELTA: f32 = 1.0 / 60.0;
        for aloft in [true, false] {
            let mut hold = RestHold::default();
            for _ in 0..30 {
                hold.settle(
                    RestsOn::Elsewhere,
                    |_| false,
                    || false,
                    Moving {
                        vertical: -1.3,
                        aloft,
                        takes_off: false,
                    },
                    Share::default(),
                    DELTA,
                );
            }
            let share = hold.settle(
                RestsOn::Terrain,
                |_| true,
                || false,
                Moving {
                    vertical: -1.3,
                    aloft,
                    takes_off: false,
                },
                Share::default(),
                DELTA,
            );
            if aloft {
                assert_eq!(share, Share::TERRAIN, "aloft: eased in");
            } else {
                assert!(
                    share.terrain < 0.1,
                    "the premise: a slow arrival eases, {share:?}"
                );
            }
        }
    }

    /// **A landing held on a hill is no take-off** (#1887 review, finding
    /// 1): a bat come down fast onto an upslope, its landing held on the
    /// ground - its drive still carrying a flight, and the physics' speed up
    /// as it slides up the hill, past the engine's launch speed and back - is
    /// never eased out as a take-off, and stands on the terrain whole
    /// throughout; so [`Underfoot::fill`] withholds its patch while it is
    /// tilted. Gated on that speed, the ease began on the slide's first frame
    /// and ran the terrain out to nothing, handed it back whole on the next
    /// frame and began again.
    #[test]
    fn a_landing_sliding_up_a_hill_is_no_take_off() {
        const DELTA: f32 = 1.0 / 60.0;
        let mut hold = RestHold::default();
        let coming = Moving {
            vertical: -1.3,
            aloft: true,
            takes_off: false,
        };
        for _ in 0..30 {
            hold.settle(
                RestsOn::Elsewhere,
                |_| false,
                || false,
                coming,
                Share::default(),
                DELTA,
            );
        }
        let mut handed = hold.settle(
            RestsOn::Terrain,
            |_| true,
            || false,
            coming,
            Share::default(),
            DELTA,
        );
        assert_eq!(handed, Share::TERRAIN, "the premise: landed at once");
        let slide = [
            2.4f32, 2.1, 1.8, 1.4, 1.0, 0.7, 0.4, 0.2, 0.1, 0.05, 0.0, 0.0, -0.01, 0.0, 0.0, 2.1,
            1.5,
        ];
        for (frame, vertical) in slide.into_iter().enumerate() {
            let moving = Moving {
                vertical,
                aloft: true,
                takes_off: false,
            };
            handed = hold.settle(RestsOn::Terrain, |_| true, || false, moving, handed, DELTA);
            assert!(
                !hold.taking_off && handed == Share::TERRAIN,
                "frame {frame} of the slide, {vertical} m/s up: eased out as a take-off, \
                 {handed:?}"
            );
        }
    }

    /// **A take-off held on the ground eases it back in** (#1887 review,
    /// finding 1): a take-off whose body the physics still reads on the
    /// terrain when its ease has run - under a ceiling, launched and stopped
    /// at once - has the terrain eased back in over [`BLEND_SECS`], no frame
    /// moving the share further than either ease's steepest. Counted in the
    /// air instead, it was handed the terrain whole on the next frame.
    #[test]
    fn a_take_off_held_on_the_ground_eases_it_back_in() {
        const DELTA: f32 = 1.0 / 60.0;
        let mut hold = RestHold::default();
        let mut handed = Share::default();
        for _ in 0..30 {
            handed = hold.settle(
                RestsOn::Terrain,
                |_| true,
                || false,
                Moving::default(),
                handed,
                DELTA,
            );
        }
        assert_eq!(handed, Share::TERRAIN, "the premise: on the terrain");
        let steepest = (DELTA / TAKEOFF_SECS).max(1.5 * DELTA / BLEND_SECS) + 1e-5;
        let mut ran_out = false;
        for frame in 0..60 {
            let moving = Moving {
                vertical: if frame == 0 { 2.4 } else { 0.0 },
                aloft: true,
                takes_off: frame == 0,
            };
            let share = hold.settle(RestsOn::Terrain, |_| true, || false, moving, handed, DELTA);
            let moved = (share.terrain - handed.terrain).abs();
            assert!(
                moved <= steepest,
                "frame {frame}: the share moved {moved:.3} in a frame, from {handed:?} to \
                 {share:?}"
            );
            ran_out |= share.terrain == 0.0;
            handed = share;
        }
        assert!(ran_out, "the premise: the take-off's ease ran out");
        assert_eq!(handed, Share::TERRAIN, "not eased back in");
    }

    /// **A take-off eases out from what was handed** (#1887 review, LOW 3):
    /// a body taking off while a landing's ease back in has handed it a
    /// third of its terrain ([`Resting::levelling`]) carries no more than
    /// that on the take-off's first frame, and less on each after.
    #[test]
    fn a_take_off_eases_out_from_what_was_handed() {
        const DELTA: f32 = 1.0 / 60.0;
        let mut hold = RestHold::default();
        for _ in 0..30 {
            hold.settle(
                RestsOn::Terrain,
                |_| true,
                || false,
                Moving::default(),
                Share::TERRAIN,
                DELTA,
            );
        }
        let mut handed = Share {
            terrain: 0.3,
            structure: 0.0,
        };
        for frame in 0..10 {
            let moving = Moving {
                vertical: 2.4,
                aloft: true,
                takes_off: frame == 0,
            };
            // Read on the terrain on its first frame, and off it after.
            let rests = if frame == 0 {
                RestsOn::Terrain
            } else {
                RestsOn::Elsewhere
            };
            let share = hold.settle(rests, |_| frame == 0, || false, moving, handed, DELTA);
            assert!(
                share.terrain <= handed.terrain && (frame > 0 || hold.taking_off),
                "frame {frame}: {share:?} after {handed:?} was handed"
            );
            handed = share;
        }
        assert_eq!(handed, Share::default(), "the ease never ran out");
    }

    /// **A take-off is latched until its body lifts** (#1887 end review), at
    /// 60 frames a second, a body standing on the terrain:
    ///
    /// - its flight turns a takeoff on a frame it dips - contact jitter, a
    ///   peer's playout standing still - and it rises off the terrain on the
    ///   next: the ease starts there, rather than the terrain going at once;
    /// - from a held landing, its frame of no flight rises off the terrain,
    ///   the chassis already leapt: handed the terrain as it was, its ease
    ///   starting on the takeoff after;
    /// - a held landing let go standing still, two frames of no flight: the
    ///   terrain stood on whole throughout, no ease begun.
    ///
    /// Each ease runs out with no frame moving the share further than its
    /// steepest. Latched on the takeoff's first frame alone, the first two
    /// handed the level floor at once on the frame the body rose.
    #[test]
    fn a_take_off_is_latched_until_it_lifts() {
        const DELTA: f32 = 1.0 / 60.0;
        let steepest = DELTA / TAKEOFF_SECS + 1e-5;
        // Each frame: the drive's flight, the speed up, and whether the body
        // is read on the terrain - off it, it is not under the body either.
        type Frame = (Option<FlightMode>, f32, bool);
        let land = (Some(FlightMode::Land), 0.0, true);
        let cases: [(&str, Vec<Frame>, Option<usize>); 3] = [
            (
                "a takeoff whose first frame dips",
                vec![
                    (None, 0.0, true),
                    (Some(FlightMode::Takeoff), -0.02, true),
                    (Some(FlightMode::Takeoff), 2.4, false),
                ],
                Some(2),
            ),
            (
                "a takeoff from a held landing",
                vec![
                    land,
                    land,
                    (None, 3.3, false),
                    (Some(FlightMode::Takeoff), 2.9, false),
                ],
                Some(3),
            ),
            (
                "a held landing let go",
                vec![land, land, (None, 0.0, true), (None, 0.0, true)],
                None,
            ),
        ];
        for (label, frames, eases_from) in cases {
            let mut hold = RestHold::default();
            let mut launch = Launch::default();
            let mut handed = Share::default();
            for _ in 0..30 {
                launch.read(None, 0.0);
                handed = hold.settle(
                    RestsOn::Terrain,
                    |_| true,
                    || false,
                    Moving::default(),
                    handed,
                    DELTA,
                );
            }
            assert_eq!(
                handed,
                Share::TERRAIN,
                "{label}: the premise, on the terrain"
            );
            let mut ran_out = false;
            for at in 0..frames.len() + 10 {
                let (flight, vertical, on) =
                    frames
                        .get(at)
                        .copied()
                        .unwrap_or((Some(FlightMode::Takeoff), 2.0, false));
                let moving = Moving {
                    vertical,
                    aloft: flight.is_some(),
                    takes_off: launch.read(flight, vertical),
                };
                let rests = if on {
                    RestsOn::Terrain
                } else {
                    RestsOn::Elsewhere
                };
                let share = hold.settle(rests, move |_| on, || false, moving, handed, DELTA);
                match eases_from {
                    Some(from) if at >= from => {
                        let moved = handed.terrain - share.terrain;
                        assert!(
                            (0.0..=steepest).contains(&moved),
                            "{label}, frame {at}: the share went from {handed:?} to {share:?}"
                        );
                        ran_out |= share.terrain == 0.0;
                    }
                    _ if at < frames.len() => assert_eq!(
                        share,
                        Share::TERRAIN,
                        "{label}, frame {at}: not stood on whole"
                    ),
                    _ => {}
                }
                handed = share;
            }
            assert!(
                eases_from.is_none() || ran_out,
                "{label}: the ease never ran out"
            );
        }
    }
}
