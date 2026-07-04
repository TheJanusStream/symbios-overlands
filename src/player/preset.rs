//! Per-preset physics components: the player-side [`PresetComponents`]
//! trait (one impl per locomotion `*Params`), the preset marker
//! components, and the build/strip pair the spawn + hot-swap paths share.
//!
//! The dispatch lives here rather than on [`LocomotionConfig`] because
//! the record
//! layer must stay Bevy/Avian-free — PDS describes *what the avatar is*;
//! the player module owns *what that means in the physics world* (the
//! same layering as [`crate::interaction`]'s `LocomotionFootprint` trait).

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::config::rover as cfg;
use crate::pds::{
    AirplaneParams, CarParams, HelicopterParams, HoverBoatParams, HumanoidParams, LocomotionConfig,
};

/// Marks the local or remote player as currently using the HoverBoat
/// preset. Inserted by [`build_preset_components`]; stripped by the
/// hot-swap system when the owner picks a different preset.
#[derive(Component)]
pub struct HoverBoatPreset;

/// Marks the player as using the Humanoid preset.
#[derive(Component)]
pub struct HumanoidPreset;

/// Marks the player as using the Airplane preset.
#[derive(Component)]
pub struct AirplanePreset;

/// Marks the player as using the Helicopter preset.
#[derive(Component)]
pub struct HelicopterPreset;

/// Marks the player as using the Car preset.
#[derive(Component)]
pub struct CarPreset;

/// Aggregate marker query target for camera follow / vehicle-yaw
/// inheritance. Covers every preset whose physics body rotates around Y
/// — i.e. anything except the upright-locked Humanoid.
#[derive(Component)]
pub struct VehicleChassis;

/// Per-preset physics component builder: each locomotion `*Params`
/// knows the Avian components its chassis needs. The caller is
/// responsible for having stripped any prior preset's components first
/// ([`strip_preset_components`]) or for the entity being fresh.
pub(super) trait PresetComponents {
    fn insert_preset(&self, commands: &mut Commands, entity: Entity);
}

/// Shared cuboid-chassis insert for the four vehicle presets — they
/// differ only in their marker component; the rig topology (collider from
/// half-extents, mass, both dampings, [`VehicleChassis`]) is identical.
fn insert_vehicle_chassis<M: Component>(
    commands: &mut Commands,
    entity: Entity,
    half: [f32; 3],
    mass: f32,
    linear_damping: f32,
    angular_damping: f32,
    marker: M,
) {
    commands.entity(entity).insert((
        Collider::cuboid(half[0] * 2.0, half[1] * 2.0, half[2] * 2.0),
        Mass(mass),
        LinearDamping(linear_damping),
        AngularDamping(angular_damping),
        marker,
        VehicleChassis,
    ));
}

impl PresetComponents for HoverBoatParams {
    fn insert_preset(&self, commands: &mut Commands, entity: Entity) {
        insert_vehicle_chassis(
            commands,
            entity,
            self.chassis_half_extents.0,
            self.mass.0,
            self.linear_damping.0,
            self.angular_damping.0,
            HoverBoatPreset,
        );
    }
}

impl PresetComponents for AirplaneParams {
    fn insert_preset(&self, commands: &mut Commands, entity: Entity) {
        insert_vehicle_chassis(
            commands,
            entity,
            self.chassis_half_extents.0,
            self.mass.0,
            self.linear_damping.0,
            self.angular_damping.0,
            AirplanePreset,
        );
    }
}

impl PresetComponents for HelicopterParams {
    fn insert_preset(&self, commands: &mut Commands, entity: Entity) {
        insert_vehicle_chassis(
            commands,
            entity,
            self.chassis_half_extents.0,
            self.mass.0,
            self.linear_damping.0,
            self.angular_damping.0,
            HelicopterPreset,
        );
    }
}

impl PresetComponents for CarParams {
    fn insert_preset(&self, commands: &mut Commands, entity: Entity) {
        insert_vehicle_chassis(
            commands,
            entity,
            self.chassis_half_extents.0,
            self.mass.0,
            self.linear_damping.0,
            self.angular_damping.0,
            CarPreset,
        );
    }
}

impl PresetComponents for HumanoidParams {
    fn insert_preset(&self, commands: &mut Commands, entity: Entity) {
        commands.entity(entity).insert((
            Collider::capsule(
                self.capsule_radius.0.max(0.05),
                self.capsule_length.0.max(0.1),
            ),
            Mass(self.mass.0),
            LinearDamping(self.linear_damping.0),
            AngularDamping(cfg::ANGULAR_DAMPING),
            // Traditional character controller: lock all three rotation
            // axes so the physics capsule slides without spinning. The
            // walk controller rotates the chassis transform itself to
            // face the movement direction.
            LockedAxes::new()
                .lock_rotation_x()
                .lock_rotation_y()
                .lock_rotation_z(),
            HumanoidPreset,
        ));
    }
}

/// Insert the physics components appropriate to the avatar's locomotion
/// preset — the per-variant dispatch into [`PresetComponents`].
pub(super) fn build_preset_components(
    commands: &mut Commands,
    entity: Entity,
    locomotion: &LocomotionConfig,
) {
    match locomotion {
        LocomotionConfig::HoverBoat(p) => p.insert_preset(commands, entity),
        LocomotionConfig::Humanoid(p) => p.insert_preset(commands, entity),
        LocomotionConfig::Airplane(p) => p.insert_preset(commands, entity),
        LocomotionConfig::Helicopter(p) => p.insert_preset(commands, entity),
        LocomotionConfig::Car(p) => p.insert_preset(commands, entity),
        LocomotionConfig::Unknown => {
            // Forward-compat shipping a record whose preset we don't model:
            // give the entity a minimal collider so the simulation does not
            // explode. The owner's editor flags the unrecognised variant.
            commands
                .entity(entity)
                .insert((Collider::cuboid(0.5, 0.5, 0.5), Mass(40.0)));
        }
    }
}

/// Remove every preset-specific component + marker from `entity`.
/// Safe to call even if the entity currently carries only a subset — Bevy's
/// `remove` no-ops when the component is absent.
///
/// KEEP IN SYNC with the [`PresetComponents`] impls above: every component
/// any preset inserts must appear in this remove list, or a hot-swap
/// leaves the old preset's physics riding along under the new one.
pub(super) fn strip_preset_components(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).remove::<(
        Collider,
        Mass,
        LinearDamping,
        AngularDamping,
        LockedAxes,
        HoverBoatPreset,
        HumanoidPreset,
        AirplanePreset,
        HelicopterPreset,
        CarPreset,
        VehicleChassis,
        super::gait::GaitAnimation,
    )>();
}
