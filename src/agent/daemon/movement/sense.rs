//! What the movement reads of the agent's body (#1430): where it is, how it
//! moves, and how high it is above what is below it - for a flight, which
//! holds its own height, and for `status`.
//!
//! The height is how far the body could come straight down before it
//! touched something solid - the ground, or whatever stands on it, a roof or
//! a tree - or how far its lowest point is above water, where water lies
//! higher. A body resting on something - the physics has it touching a
//! thing under it - is at nought. Otherwise its own collider is swept down:
//! a ray from its middle is not enough, since an airship held level on a
//! hillside rests on its uphill edge, and a ray from its middle read half a
//! metre under it (#1430). A body standing, parked or landed reads about
//! nothing.

use avian3d::prelude::*;
use bevy::ecs::system::{SystemParam, SystemState};
use bevy::prelude::*;

use crate::state::LocalPlayer;
use crate::terrain::FinishedHeightMap;
use crate::water::WaterSurfaces;

use super::flight::{Craft, Terrain};

/// How far down the body looks for what is below it (m): further than any
/// world's ground falls.
const LOOK_DOWN_M: f32 = 4000.0;

/// The least a contact's normal may lean from straight up for what it
/// touches to be under the body (the cosine of 60 degrees): a wall beside it
/// or a roof over it is not what it rests on.
const UNDER_NORMAL_Y: f32 = 0.5;

/// How much of its width and length the body's footprint keeps when it is
/// swept down (0-1): a wall it leans on touches its side, and a footprint
/// that still touched the wall would find it "under" the body at once.
const FOOTPRINT_SHARE: f32 = 0.9;

/// The agent's body, and the world around it, as the movement reads them.
/// Its `below` is where the body's underside would be once it came down
/// onto what it would touch, so a craft's height is how far it can come
/// down.
#[derive(SystemParam)]
pub(in super::super) struct Sensing<'w, 's> {
    #[allow(clippy::type_complexity)]
    body: Query<
        'w,
        's,
        (
            Entity,
            &'static Position,
            &'static Rotation,
            &'static LinearVelocity,
            &'static AngularVelocity,
            &'static Collider,
        ),
        With<LocalPlayer>,
    >,
    sensors: Query<'w, 's, Entity, With<Sensor>>,
    spatial: SpatialQuery<'w, 's>,
    contacts: Option<Res<'w, ContactGraph>>,
    heightmap: Option<Res<'w, FinishedHeightMap>>,
    water: Option<Res<'w, WaterSurfaces>>,
}

impl Sensing<'_, '_> {
    /// The body as a flight reads it - or `None` before it has one, or
    /// before there is anything under it to measure from.
    pub(super) fn craft(&self) -> Option<Craft> {
        let (entity, position, rotation, velocity, spin, collider) = self.body.single().ok()?;
        let underside = collider.aabb(position.0, *rotation).min.y;
        // Resting on something, it is down. The sweep alone cannot say so:
        // a body pressed into a slope by its edge can have its shallowest
        // way out sideways, and a sweep straight down then passes through
        // what it stands on (#1430).
        let ground = if self.resting_on_something(entity) {
            underside
        } else {
            let mut footprint = collider.clone();
            footprint.set_scale(
                collider.scale() * Vec3::new(FOOTPRINT_SHARE, 1.0, FOOTPRINT_SHARE),
                8,
            );
            // A roof it is pressed up against is left behind by a sweep
            // down, and is not what is under it.
            let leaving_what_it_touches = ShapeCastConfig {
                ignore_origin_penetration: true,
                ..ShapeCastConfig::from_max_distance(LOOK_DOWN_M)
            };
            match self.spatial.cast_shape(
                &footprint,
                position.0,
                rotation.0,
                Dir3::NEG_Y,
                &leaving_what_it_touches,
                &self.filter(entity),
            ) {
                Some(hit) => underside - hit.distance,
                None => self.ground_at(position.xz())?,
            }
        };
        let water = self.water_at(position.xz()).filter(|water| *water > ground);
        Some(Craft {
            position: position.0,
            forward: rotation.0 * Vec3::NEG_Z,
            up: rotation.0 * Vec3::Y,
            velocity: velocity.0,
            yaw_rate: spin.0.y,
            spin: spin.0,
            underside,
            below: water.unwrap_or(ground),
            on_water: water.is_some(),
        })
    }

    /// Whether the physics has `body` touching something under it - a
    /// contact whose normal, turned to point out of the thing touched, leans
    /// less than 60 degrees from straight up.
    fn resting_on_something(&self, body: Entity) -> bool {
        let Some(contacts) = self.contacts.as_ref() else {
            return false;
        };
        contacts
            .contact_pairs_with(body)
            .filter(|pair| pair.is_touching())
            .any(|pair| {
                let (other, out_of_other) = if pair.collider1 == body {
                    (pair.collider2, -1.0)
                } else {
                    (pair.collider1, 1.0)
                };
                // A manifold's normal points from the first collider to the
                // second: turned to point out of the other, it points up out
                // of a thing under the body.
                !self.sensors.contains(other)
                    && pair
                        .manifolds
                        .iter()
                        .any(|manifold| out_of_other * manifold.normal.y >= UNDER_NORMAL_Y)
            })
    }

    /// The body itself, and the gateway veils and portal volumes a body
    /// passes through, are not what it stands on or runs into - the filter
    /// the game's own ground rays use (#813).
    fn filter(&self, body: Entity) -> SpatialQueryFilter {
        SpatialQueryFilter::default()
            .with_excluded_entities(std::iter::once(body).chain(self.sensors.iter()))
    }

    fn ground_at(&self, xz: Vec2) -> Option<f32> {
        self.heightmap
            .as_ref()
            .map(|heightmap| heightmap.world_height_at(xz.x, xz.y))
    }

    fn water_at(&self, xz: Vec2) -> Option<f32> {
        self.water.as_ref()?.surface_at(xz).map(|(_, y)| y)
    }
}

impl Terrain for Sensing<'_, '_> {
    fn surface_at(&self, xz: Vec2) -> f32 {
        let ground = self.ground_at(xz).unwrap_or(f32::NEG_INFINITY);
        self.water_at(xz).map_or(ground, |water| water.max(ground))
    }

    /// Sweeps the body's own collider along `toward`, as it stands.
    fn clear_along(&self, toward: Vec2, reach: f32, lift: f32) -> f32 {
        let Ok((entity, position, rotation, .., collider)) = self.body.single() else {
            return reach;
        };
        let Ok(direction) = Dir3::new(Vec3::new(toward.x, 0.0, toward.y)) else {
            return reach;
        };
        self.spatial
            .cast_shape(
                collider,
                position.0 + Vec3::Y * lift,
                rotation.0,
                direction,
                &ShapeCastConfig::from_max_distance(reach),
                &self.filter(entity),
            )
            .map_or(reach, |hit| hit.distance)
    }
}

/// How high the agent's body is above what is below it (m): its underside
/// over the ground, anything standing on it, or water - negative in water.
/// `None` before it has a body.
pub(in super::super) fn height(world: &mut World) -> Option<f32> {
    craft(world).map(|craft| craft.height())
}

/// The agent's body as a flight reads it, read from `world` - for a command,
/// which is answered outside the systems that steer.
pub(super) fn craft(world: &mut World) -> Option<Craft> {
    let mut state = SystemState::<Sensing>::new(world);
    state.get(world).ok()?.craft()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::avatar::AvatarRecord;
    use crate::player::sim::FlightBench;

    fn airship_at(at: Vec3) -> FlightBench {
        FlightBench::new(
            &AvatarRecord::default_for_did("did:plc:agentofflineair222222222"),
            at,
        )
    }

    /// Hold `keys` for `secs` of physics.
    fn hold(bench: &mut FlightBench, keys: &[KeyCode], secs: f64) {
        bench.hold(keys);
        for _ in 0..(secs * crate::player::sim::BENCH_HZ) as usize {
            bench.step();
        }
    }

    fn height_over(bench: &mut FlightBench) -> f32 {
        height(bench.world_mut()).expect("a body over the floor")
    }

    /// Pressed up against a roof, a body is as high over the ground as it
    /// is: a contact over it is not what it stands on. (It read nought,
    /// and a flight stuck under a roof 10 m up said it had landed.)
    #[test]
    fn a_roof_pressed_against_from_below_is_not_the_ground() {
        let mut bench = airship_at(Vec3::new(0.0, 10.0, 0.0));
        bench.block(
            Transform::from_xyz(0.0, 12.0, 0.0),
            Vec3::new(10.0, 0.5, 10.0),
        );
        hold(&mut bench, &[KeyCode::Space], 2.0);

        let height = height_over(&mut bench);

        assert!(
            (height - bench.underside()).abs() < 0.05 && height > 9.0,
            "{height:.2} m up under a roof, its underside at {:.2}",
            bench.underside()
        );
    }

    /// Pressed against a wall, a body is as high over the ground as it is.
    #[test]
    fn a_wall_leant_on_is_not_the_ground() {
        let mut bench = airship_at(Vec3::new(0.0, 5.0, 0.0));
        // Its right, as it faces -Z, is +X.
        bench.block(
            Transform::from_xyz(3.0, 10.0, 0.0),
            Vec3::new(2.0, 10.0, 10.0),
        );
        hold(&mut bench, &[KeyCode::KeyE], 2.0);

        let height = height_over(&mut bench);

        assert!(
            (height - bench.underside()).abs() < 0.05 && height > 4.0,
            "{height:.2} m up against a wall, its underside at {:.2}",
            bench.underside()
        );
    }

    /// Down on a hillside - level, resting on its uphill edge - a body is
    /// down: nought, where a ray from its middle read half a metre.
    #[test]
    fn a_body_resting_on_a_slope_reads_nought() {
        let mut bench = airship_at(Vec3::new(0.0, 8.0, 0.0));
        let tilt = Quat::from_rotation_x(-15f32.to_radians());
        bench.block(
            Transform::from_translation(Vec3::new(0.0, 5.0, 0.0) - tilt * Vec3::Y * 2.0)
                .with_rotation(tilt),
            Vec3::new(40.0, 2.0, 40.0),
        );
        // Held down onto it: let go after pressing, the ground springs a
        // body back up.
        hold(&mut bench, &[KeyCode::ShiftLeft], 3.0);

        let height = height_over(&mut bench);

        assert!(
            height.abs() < 0.05,
            "{height:.2} m over the slope it rests on"
        );
    }
}
