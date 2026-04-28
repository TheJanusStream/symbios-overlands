//! Avatar record — player vessel / body definition.
//!
//! Each player's avatar is published to their own PDS at
//! `collection = network.symbios.overlands.avatar, rkey = self`. The record
//! is split into two disjoint halves:
//!
//!   - `visuals` — a hierarchical [`Generator`] tree describing the cosmetic
//!     mesh (cuboids, capsules, lsystems, …). Identical machinery to room
//!     generators, with avatar-specific allowed kinds enforced by
//!     [`super::sanitize::sanitize_avatar_visuals`] (no Terrain/Water/Portal).
//!     Remote peers render this.
//!   - `locomotion` — a tagged-union [`LocomotionConfig`] selecting one of
//!     five physics presets (HoverBoat / Humanoid / Airplane / Helicopter /
//!     Car), each carrying its own collider dimensions + tuning. Remote
//!     peers *deserialize but ignore* this — only the local player's
//!     locomotion drives the rigid body.
//!
//! Legacy `network.symbios.avatar.hover_rover` / `…humanoid` body records
//! published before this schema land deserialize to
//! [`LocomotionConfig::Unknown`] / [`Generator::Unknown`] respectively, and
//! the fetch path falls through to [`AvatarRecord::default_for_did`]. There
//! is no automatic migration — old records require a manual republish.

use super::AVATAR_COLLECTION;
use super::generator::{Generator, GeneratorKind};
use super::sanitize::sanitize_avatar_visuals;
use super::texture::SovereignMaterialSettings;
use super::types::{Fp, Fp3, TransformData};
use super::xrpc::{FetchError, PutOutcome, XrpcError, resolve_pds};
use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Locomotion presets
// ---------------------------------------------------------------------------

/// One row in the locomotion-picker table — `(kind_tag, display_label,
/// default_constructor)`. The avatar editor uses this to render the
/// preset selector and to materialise a fresh default-tuned variant
/// when the user picks a new preset.
pub type LocomotionPickerEntry = (&'static str, &'static str, fn() -> LocomotionConfig);

/// Open-union locomotion preset. Each variant carries its own collider
/// dimensions + physics tuning so the chassis is fully self-describing —
/// the visuals tree is independent of the physics body.
///
/// Future presets add new `#[serde(rename)]` arms; older clients fall
/// through to `Unknown`, which the player module treats as "no
/// locomotion" and gives the entity a minimal placeholder collider so
/// the simulation does not explode.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "$type")]
pub enum LocomotionConfig {
    #[serde(rename = "network.symbios.locomotion.hover_boat")]
    HoverBoat(Box<HoverBoatParams>),

    #[serde(rename = "network.symbios.locomotion.humanoid")]
    Humanoid(Box<HumanoidParams>),

    #[serde(rename = "network.symbios.locomotion.airplane")]
    Airplane(Box<AirplaneParams>),

    #[serde(rename = "network.symbios.locomotion.helicopter")]
    Helicopter(Box<HelicopterParams>),

    #[serde(rename = "network.symbios.locomotion.car")]
    Car(Box<CarParams>),

    #[serde(other)]
    Unknown,
}

impl LocomotionConfig {
    /// Stable string tag used by hot-swap detection so a variant change
    /// (HoverBoat → Humanoid) can be seen without a full `==` compare.
    pub fn kind_tag(&self) -> &'static str {
        match self {
            LocomotionConfig::HoverBoat(_) => "hover_boat",
            LocomotionConfig::Humanoid(_) => "humanoid",
            LocomotionConfig::Airplane(_) => "airplane",
            LocomotionConfig::Helicopter(_) => "helicopter",
            LocomotionConfig::Car(_) => "car",
            LocomotionConfig::Unknown => "unknown",
        }
    }

    /// Human-readable label for the locomotion picker UI.
    pub fn display_label(&self) -> &'static str {
        match self {
            LocomotionConfig::HoverBoat(_) => "Hover-Boat",
            LocomotionConfig::Humanoid(_) => "Humanoid",
            LocomotionConfig::Airplane(_) => "Airplane",
            LocomotionConfig::Helicopter(_) => "Helicopter",
            LocomotionConfig::Car(_) => "Car",
            LocomotionConfig::Unknown => "Unknown",
        }
    }

    /// Ordered list of preset constructors used by the locomotion picker
    /// to enumerate every selectable preset. Each entry returns a fresh
    /// default-tuned variant.
    pub fn pickers() -> &'static [LocomotionPickerEntry] {
        &[
            ("hover_boat", "Hover-Boat", || {
                LocomotionConfig::HoverBoat(Box::<HoverBoatParams>::default())
            }),
            ("humanoid", "Humanoid", || {
                LocomotionConfig::Humanoid(Box::<HumanoidParams>::default())
            }),
            ("airplane", "Airplane", || {
                LocomotionConfig::Airplane(Box::<AirplaneParams>::default())
            }),
            ("helicopter", "Helicopter", || {
                LocomotionConfig::Helicopter(Box::<HelicopterParams>::default())
            }),
            ("car", "Car", || {
                LocomotionConfig::Car(Box::<CarParams>::default())
            }),
        ]
    }

    /// In-place sanitisation. Delegates to the per-variant clamp helper;
    /// `Unknown` is left as-is.
    pub fn sanitize(&mut self) {
        match self {
            LocomotionConfig::HoverBoat(p) => p.sanitize(),
            LocomotionConfig::Humanoid(p) => p.sanitize(),
            LocomotionConfig::Airplane(p) => p.sanitize(),
            LocomotionConfig::Helicopter(p) => p.sanitize(),
            LocomotionConfig::Car(p) => p.sanitize(),
            LocomotionConfig::Unknown => {}
        }
    }
}

// ---------------------------------------------------------------------------
// HoverBoat — current rover physics, lifted from RoverKinematics
// ---------------------------------------------------------------------------

/// Hover-boat preset: 4-corner raycast suspension + buoyancy + WASD drive,
/// matching the legacy hover-rover physics. Collider is a chassis cuboid
/// with author-tunable half-extents.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HoverBoatParams {
    /// Chassis collider half-extents (m). Avian's `Collider::cuboid` takes
    /// full extents, so the spawner doubles each component before insert.
    pub chassis_half_extents: Fp3,
    pub mass: Fp,
    pub linear_damping: Fp,
    pub angular_damping: Fp,
    pub suspension_rest_length: Fp,
    pub suspension_stiffness: Fp,
    pub suspension_damping: Fp,
    pub drive_force: Fp,
    pub turn_torque: Fp,
    pub lateral_grip: Fp,
    pub jump_force: Fp,
    pub uprighting_torque: Fp,
    pub water_rest_length: Fp,
    pub buoyancy_strength: Fp,
    pub buoyancy_damping: Fp,
    pub buoyancy_max_depth: Fp,
}

impl Default for HoverBoatParams {
    fn default() -> Self {
        use crate::config::rover as cfg;
        Self {
            chassis_half_extents: Fp3([cfg::CHASSIS_X, cfg::CHASSIS_Y, cfg::CHASSIS_Z]),
            mass: Fp(cfg::MASS),
            linear_damping: Fp(cfg::LINEAR_DAMPING),
            angular_damping: Fp(cfg::ANGULAR_DAMPING),
            suspension_rest_length: Fp(cfg::SUSPENSION_REST_LENGTH),
            suspension_stiffness: Fp(cfg::SUSPENSION_STIFFNESS),
            suspension_damping: Fp(cfg::SUSPENSION_DAMPING),
            drive_force: Fp(cfg::DRIVE_FORCE),
            turn_torque: Fp(cfg::TURN_TORQUE),
            lateral_grip: Fp(cfg::LATERAL_GRIP),
            jump_force: Fp(cfg::JUMP_FORCE),
            uprighting_torque: Fp(cfg::UPRIGHTING_TORQUE),
            water_rest_length: Fp(cfg::WATER_REST_LENGTH),
            buoyancy_strength: Fp(cfg::BUOYANCY_STRENGTH),
            buoyancy_damping: Fp(cfg::BUOYANCY_DAMPING),
            buoyancy_max_depth: Fp(cfg::BUOYANCY_MAX_DEPTH),
        }
    }
}

impl HoverBoatParams {
    fn sanitize(&mut self) {
        clamp_half_extents(&mut self.chassis_half_extents);
        self.mass = clamp_pos(self.mass, 0.1, 10_000.0);
        self.linear_damping = clamp_pos(self.linear_damping, 0.0, 100.0);
        self.angular_damping = clamp_pos(self.angular_damping, 0.0, 100.0);
        self.suspension_rest_length = clamp_pos(self.suspension_rest_length, 0.0, 5.0);
        self.suspension_stiffness = clamp_pos(self.suspension_stiffness, 0.0, 50_000.0);
        self.suspension_damping = clamp_pos(self.suspension_damping, 0.0, 5_000.0);
        self.drive_force = clamp_pos(self.drive_force, 0.0, 50_000.0);
        self.turn_torque = clamp_pos(self.turn_torque, 0.0, 50_000.0);
        self.lateral_grip = clamp_pos(self.lateral_grip, 0.0, 50_000.0);
        self.jump_force = clamp_pos(self.jump_force, 0.0, 50_000.0);
        self.uprighting_torque = clamp_pos(self.uprighting_torque, 0.0, 50_000.0);
        self.water_rest_length = clamp_pos(self.water_rest_length, 0.0, 10.0);
        self.buoyancy_strength = clamp_pos(self.buoyancy_strength, 0.0, 100_000.0);
        self.buoyancy_damping = clamp_pos(self.buoyancy_damping, 0.0, 10_000.0);
        self.buoyancy_max_depth = clamp_pos(self.buoyancy_max_depth, 0.001, 50.0);
    }
}

// ---------------------------------------------------------------------------
// Humanoid — current walk/swim physics, lifted from HumanoidKinematics
// ---------------------------------------------------------------------------

/// Humanoid preset: vertical capsule rigid body with `LockedAxes` keeping
/// it upright, velocity-driven walk controller, jump impulse, swim/wading
/// modes triggered by water depth.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HumanoidParams {
    /// Capsule collider radius (m).
    pub capsule_radius: Fp,
    /// Capsule cylindrical length (m). Total height ≈ length + 2·radius.
    pub capsule_length: Fp,
    pub mass: Fp,
    pub linear_damping: Fp,
    pub walk_speed: Fp,
    pub acceleration: Fp,
    pub jump_impulse: Fp,
    pub swim_speed: Fp,
    pub swim_vertical_speed: Fp,
    pub wading_speed_factor: Fp,
}

impl Default for HumanoidParams {
    fn default() -> Self {
        Self {
            capsule_radius: Fp(0.28),
            capsule_length: Fp(1.24),
            mass: Fp(80.0),
            linear_damping: Fp(0.3),
            walk_speed: Fp(4.0),
            acceleration: Fp(12.0),
            jump_impulse: Fp(450.0),
            swim_speed: Fp(2.5),
            swim_vertical_speed: Fp(1.8),
            wading_speed_factor: Fp(0.5),
        }
    }
}

impl HumanoidParams {
    fn sanitize(&mut self) {
        self.capsule_radius = clamp_pos(self.capsule_radius, 0.05, 2.0);
        self.capsule_length = clamp_pos(self.capsule_length, 0.1, 8.0);
        self.mass = clamp_pos(self.mass, 0.1, 10_000.0);
        self.linear_damping = clamp_pos(self.linear_damping, 0.0, 100.0);
        self.walk_speed = clamp_pos(self.walk_speed, 0.0, 50.0);
        self.acceleration = clamp_pos(self.acceleration, 0.0, 200.0);
        self.jump_impulse = clamp_pos(self.jump_impulse, 0.0, 50_000.0);
        self.swim_speed = clamp_pos(self.swim_speed, 0.0, 50.0);
        self.swim_vertical_speed = clamp_pos(self.swim_vertical_speed, 0.0, 50.0);
        self.wading_speed_factor = clamp_unit(self.wading_speed_factor);
    }

    /// Total standing height (m). Convenience for systems that need to
    /// know whether the player's head/feet cross a water surface — the
    /// avatar's vertical extent comes purely from the capsule collider.
    pub fn total_height(&self) -> f32 {
        self.capsule_length.0 + 2.0 * self.capsule_radius.0
    }
}

// ---------------------------------------------------------------------------
// Airplane — arcade flight: continuous thrust + control surfaces
// ---------------------------------------------------------------------------

/// Airplane preset: arcade flight model. W/S pitch, A/D roll, Q/E yaw,
/// Space throttle up, Shift throttle down. Lift = `lift_per_speed` ×
/// forward airspeed (no AOA simulation); drag damps motion along the
/// negative-velocity direction. No take-off mechanic — the avatar is
/// always "airborne" and crashes on terrain contact like any other
/// physics body.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AirplaneParams {
    /// Fuselage cuboid collider half-extents (m).
    pub chassis_half_extents: Fp3,
    pub mass: Fp,
    pub linear_damping: Fp,
    pub angular_damping: Fp,
    /// Continuous forward thrust applied by the throttle input (N).
    pub thrust: Fp,
    /// Pitch input torque (N·m). Positive W input pitches nose down.
    pub pitch_torque: Fp,
    /// Roll input torque (N·m).
    pub roll_torque: Fp,
    /// Yaw / rudder input torque (N·m).
    pub yaw_torque: Fp,
    /// Lift force per (m/s) of forward airspeed (N·s/m). Multiplied by
    /// the chassis-local forward-velocity component each fixed step.
    pub lift_per_speed: Fp,
    /// Air-resistance coefficient applied along the velocity vector (N·s/m).
    pub drag_coefficient: Fp,
    /// Minimum forward airspeed (m/s) below which lift drops to zero —
    /// approximates a stall without simulating AOA.
    pub min_airspeed: Fp,
}

impl Default for AirplaneParams {
    fn default() -> Self {
        Self {
            chassis_half_extents: Fp3([0.6, 0.3, 1.5]),
            mass: Fp(40.0),
            linear_damping: Fp(0.05),
            angular_damping: Fp(2.0),
            thrust: Fp(1_500.0),
            pitch_torque: Fp(900.0),
            roll_torque: Fp(900.0),
            yaw_torque: Fp(450.0),
            lift_per_speed: Fp(45.0),
            drag_coefficient: Fp(0.6),
            min_airspeed: Fp(6.0),
        }
    }
}

impl AirplaneParams {
    fn sanitize(&mut self) {
        clamp_half_extents(&mut self.chassis_half_extents);
        self.mass = clamp_pos(self.mass, 0.1, 10_000.0);
        self.linear_damping = clamp_pos(self.linear_damping, 0.0, 100.0);
        self.angular_damping = clamp_pos(self.angular_damping, 0.0, 100.0);
        self.thrust = clamp_pos(self.thrust, 0.0, 100_000.0);
        self.pitch_torque = clamp_pos(self.pitch_torque, 0.0, 50_000.0);
        self.roll_torque = clamp_pos(self.roll_torque, 0.0, 50_000.0);
        self.yaw_torque = clamp_pos(self.yaw_torque, 0.0, 50_000.0);
        self.lift_per_speed = clamp_pos(self.lift_per_speed, 0.0, 1_000.0);
        self.drag_coefficient = clamp_pos(self.drag_coefficient, 0.0, 100.0);
        self.min_airspeed = clamp_pos(self.min_airspeed, 0.0, 100.0);
    }
}

// ---------------------------------------------------------------------------
// Helicopter — auto-stabilizing arcade hover
// ---------------------------------------------------------------------------

/// Helicopter preset: arcade hover model. The chassis auto-stabilises to
/// upright (no torque-induced spin), `hover_thrust` exactly cancels
/// gravity at idle, Space ascends, Shift descends, W/S apply a tilt-cyclic
/// horizontal force, A/D yaw, Q/E strafe.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HelicopterParams {
    pub chassis_half_extents: Fp3,
    pub mass: Fp,
    pub linear_damping: Fp,
    pub angular_damping: Fp,
    /// Vertical force that holds the helicopter at hover when no input is
    /// pressed (N). Set so `hover_thrust ≈ mass · 9.81` for equilibrium.
    pub hover_thrust: Fp,
    /// Vertical climb / descend speed when Space / Shift is held (m/s).
    /// Drives a target-velocity lerp rather than a raw force so input
    /// feels responsive without overshooting.
    pub vertical_speed: Fp,
    /// Forward / backward (cyclic) horizontal force on W/S (N).
    pub cyclic_force: Fp,
    /// Lateral strafe force on Q/E (N).
    pub strafe_force: Fp,
    /// Yaw input torque on A/D (N·m).
    pub yaw_torque: Fp,
}

impl Default for HelicopterParams {
    fn default() -> Self {
        // hover_thrust default ≈ mass · 9.81 so a fresh helicopter floats
        // at idle without sinking or climbing.
        Self {
            chassis_half_extents: Fp3([0.7, 0.6, 1.4]),
            mass: Fp(60.0),
            linear_damping: Fp(0.8),
            angular_damping: Fp(4.0),
            hover_thrust: Fp(60.0 * 9.81),
            vertical_speed: Fp(6.0),
            cyclic_force: Fp(900.0),
            strafe_force: Fp(800.0),
            yaw_torque: Fp(400.0),
        }
    }
}

impl HelicopterParams {
    fn sanitize(&mut self) {
        clamp_half_extents(&mut self.chassis_half_extents);
        self.mass = clamp_pos(self.mass, 0.1, 10_000.0);
        self.linear_damping = clamp_pos(self.linear_damping, 0.0, 100.0);
        self.angular_damping = clamp_pos(self.angular_damping, 0.0, 100.0);
        self.hover_thrust = clamp_pos(self.hover_thrust, 0.0, 200_000.0);
        self.vertical_speed = clamp_pos(self.vertical_speed, 0.0, 50.0);
        self.cyclic_force = clamp_pos(self.cyclic_force, 0.0, 50_000.0);
        self.strafe_force = clamp_pos(self.strafe_force, 0.0, 50_000.0);
        self.yaw_torque = clamp_pos(self.yaw_torque, 0.0, 50_000.0);
    }
}

// ---------------------------------------------------------------------------
// Car — ground vehicle with raycast suspension + steering
// ---------------------------------------------------------------------------

/// Car preset: ground vehicle. 4-corner raycast suspension (same approach
/// as the hover-boat, no buoyancy), W/S throttle/reverse, A/D steer
/// (yaw torque only when grounded), Space handbrake (zero forward force +
/// extra lateral grip). Sinks in water.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CarParams {
    pub chassis_half_extents: Fp3,
    pub mass: Fp,
    pub linear_damping: Fp,
    pub angular_damping: Fp,
    pub suspension_rest_length: Fp,
    pub suspension_stiffness: Fp,
    pub suspension_damping: Fp,
    pub drive_force: Fp,
    pub turn_torque: Fp,
    pub lateral_grip: Fp,
    /// Extra lateral-grip multiplier applied when Space (handbrake) is held.
    pub handbrake_grip_factor: Fp,
}

impl Default for CarParams {
    fn default() -> Self {
        use crate::config::rover as cfg;
        Self {
            chassis_half_extents: Fp3([0.8, 0.4, 1.6]),
            mass: Fp(900.0),
            linear_damping: Fp(0.8),
            angular_damping: Fp(4.0),
            suspension_rest_length: Fp(0.6),
            // Stiffer than hover-boat: cars need quick response on terrain.
            suspension_stiffness: Fp(cfg::SUSPENSION_STIFFNESS * 4.0),
            suspension_damping: Fp(cfg::SUSPENSION_DAMPING * 2.5),
            drive_force: Fp(8_000.0),
            turn_torque: Fp(1_800.0),
            lateral_grip: Fp(20_000.0),
            handbrake_grip_factor: Fp(0.25),
        }
    }
}

impl CarParams {
    fn sanitize(&mut self) {
        clamp_half_extents(&mut self.chassis_half_extents);
        self.mass = clamp_pos(self.mass, 0.1, 50_000.0);
        self.linear_damping = clamp_pos(self.linear_damping, 0.0, 100.0);
        self.angular_damping = clamp_pos(self.angular_damping, 0.0, 100.0);
        self.suspension_rest_length = clamp_pos(self.suspension_rest_length, 0.001, 5.0);
        self.suspension_stiffness = clamp_pos(self.suspension_stiffness, 0.0, 200_000.0);
        self.suspension_damping = clamp_pos(self.suspension_damping, 0.0, 20_000.0);
        self.drive_force = clamp_pos(self.drive_force, 0.0, 200_000.0);
        self.turn_torque = clamp_pos(self.turn_torque, 0.0, 50_000.0);
        self.lateral_grip = clamp_pos(self.lateral_grip, 0.0, 200_000.0);
        self.handbrake_grip_factor = clamp_pos(self.handbrake_grip_factor, 0.0, 100.0);
    }
}

// ---------------------------------------------------------------------------
// Sanitiser primitives — shared by every *Params impl
// ---------------------------------------------------------------------------

fn clamp_pos(v: Fp, lo: f32, hi: f32) -> Fp {
    let x = v.0;
    Fp(if x.is_finite() { x.clamp(lo, hi) } else { lo })
}

fn clamp_unit(v: Fp) -> Fp {
    clamp_pos(v, 0.0, 1.0)
}

fn clamp_half_extents(e: &mut Fp3) {
    let mut a = e.0;
    for c in a.iter_mut() {
        *c = if c.is_finite() {
            c.clamp(0.05, 50.0)
        } else {
            0.5
        };
    }
    *e = Fp3(a);
}

// ---------------------------------------------------------------------------
// AvatarRecord
// ---------------------------------------------------------------------------

/// The top-level avatar record. Stored at
/// `network.symbios.overlands.avatar / self` on the player's PDS.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Resource)]
pub struct AvatarRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    /// Hierarchical visuals — the cosmetic mesh tree. Sanitised by
    /// [`super::sanitize::sanitize_avatar_visuals`] which excludes
    /// Terrain/Water/Portal kinds.
    pub visuals: Generator,
    /// Physics preset selecting the player's chassis collider + control
    /// scheme + tuning. Local-only — remote peers ignore this.
    pub locomotion: LocomotionConfig,
}

impl AvatarRecord {
    /// Synthesise a starting avatar with a deterministic palette derived
    /// from the owner's DID — every fresh player gets a unique-coloured
    /// hover-boat without ever touching the editor.
    ///
    /// The visual tree mirrors the spirit of the legacy hover-rover: a
    /// cuboid hull with two capsule pontoons, an upright cylinder mast
    /// crowned by a sphere finial, and a flat sail. Materials carry the
    /// DID-hashed palette (`hue(0)` hull, `hue(3)` pontoons, `hue(7)` mast,
    /// `hue(11)` accents) so two peers spawning side-by-side never look
    /// identical.
    pub fn default_for_did(did: &str) -> Self {
        let hash = fnv1a_64(did);
        let hue = |n: u32| -> [f32; 3] {
            let r = ((hash.rotate_left(n) & 0xFF) as f32) / 255.0;
            let g = ((hash.rotate_left(n + 8) & 0xFF) as f32) / 255.0;
            let b = ((hash.rotate_left(n + 16) & 0xFF) as f32) / 255.0;
            // Bias away from near-black so new players aren't invisible.
            [0.25 + r * 0.70, 0.25 + g * 0.70, 0.25 + b * 0.70]
        };
        let hull_color = hue(0);
        let pontoon_color = hue(3);
        let mast_color = hue(7);
        let accent_color = hue(11);

        let metal_mat = |color: [f32; 3]| SovereignMaterialSettings {
            base_color: Fp3(color),
            metallic: Fp(0.4),
            roughness: Fp(0.45),
            ..Default::default()
        };
        let cloth_mat = |color: [f32; 3]| SovereignMaterialSettings {
            base_color: Fp3(color),
            metallic: Fp(0.0),
            roughness: Fp(0.85),
            ..Default::default()
        };

        let hull = Generator {
            kind: GeneratorKind::Cuboid {
                size: Fp3([1.6, 0.4, 2.4]),
                solid: false,
                material: metal_mat(hull_color),
                twist: Fp(0.0),
                taper: Fp(0.0),
                bend: Fp3([0.0, 0.0, 0.0]),
            },
            transform: TransformData::default(),
            children: vec![
                // Left pontoon — capsule lying on its side.
                Generator {
                    kind: GeneratorKind::Capsule {
                        radius: Fp(0.18),
                        length: Fp(2.0),
                        latitudes: 8,
                        longitudes: 16,
                        solid: false,
                        material: metal_mat(pontoon_color),
                        twist: Fp(0.0),
                        taper: Fp(0.0),
                        bend: Fp3([0.0, 0.0, 0.0]),
                    },
                    transform: TransformData {
                        translation: Fp3([-0.85, -0.25, 0.0]),
                        rotation: quat_xyzw(quat_x(std::f32::consts::FRAC_PI_2)),
                        scale: Fp3([1.0, 1.0, 1.0]),
                    },
                    children: Vec::new(),
                },
                // Right pontoon.
                Generator {
                    kind: GeneratorKind::Capsule {
                        radius: Fp(0.18),
                        length: Fp(2.0),
                        latitudes: 8,
                        longitudes: 16,
                        solid: false,
                        material: metal_mat(pontoon_color),
                        twist: Fp(0.0),
                        taper: Fp(0.0),
                        bend: Fp3([0.0, 0.0, 0.0]),
                    },
                    transform: TransformData {
                        translation: Fp3([0.85, -0.25, 0.0]),
                        rotation: quat_xyzw(quat_x(std::f32::consts::FRAC_PI_2)),
                        scale: Fp3([1.0, 1.0, 1.0]),
                    },
                    children: Vec::new(),
                },
                // Mast — vertical cylinder rising from the deck.
                Generator {
                    kind: GeneratorKind::Cylinder {
                        radius: Fp(0.06),
                        height: Fp(1.4),
                        resolution: 16,
                        solid: false,
                        material: metal_mat(mast_color),
                        twist: Fp(0.0),
                        taper: Fp(0.0),
                        bend: Fp3([0.0, 0.0, 0.0]),
                    },
                    transform: TransformData {
                        translation: Fp3([0.0, 0.9, 0.0]),
                        rotation: quat_xyzw([0.0, 0.0, 0.0, 1.0]),
                        scale: Fp3([1.0, 1.0, 1.0]),
                    },
                    children: vec![
                        // Sphere finial perched at the very top. Centred at
                        // the mast's local +Y so it caps the cylinder.
                        Generator {
                            kind: GeneratorKind::Sphere {
                                radius: Fp(0.12),
                                resolution: 3,
                                solid: false,
                                material: metal_mat(accent_color),
                                twist: Fp(0.0),
                                taper: Fp(0.0),
                                bend: Fp3([0.0, 0.0, 0.0]),
                            },
                            transform: TransformData {
                                translation: Fp3([0.0, 0.7, 0.0]),
                                rotation: quat_xyzw([0.0, 0.0, 0.0, 1.0]),
                                scale: Fp3([1.0, 1.0, 1.0]),
                            },
                            children: Vec::new(),
                        },
                    ],
                },
                // Sail — flat plane hanging beside the mast, cloth-like.
                Generator {
                    kind: GeneratorKind::Cuboid {
                        size: Fp3([0.05, 0.9, 0.9]),
                        solid: false,
                        material: cloth_mat([0.95, 0.95, 0.92]),
                        twist: Fp(0.0),
                        taper: Fp(0.0),
                        bend: Fp3([0.0, 0.0, 0.0]),
                    },
                    transform: TransformData {
                        translation: Fp3([0.0, 1.05, -0.5]),
                        rotation: quat_xyzw([0.0, 0.0, 0.0, 1.0]),
                        scale: Fp3([1.0, 1.0, 1.0]),
                    },
                    children: Vec::new(),
                },
            ],
        };

        Self {
            lex_type: AVATAR_COLLECTION.into(),
            visuals: hull,
            locomotion: LocomotionConfig::HoverBoat(Box::<HoverBoatParams>::default()),
        }
    }

    /// Clamp every numeric field so a malicious PDS (or a forward-compat
    /// client shipping a record we cannot fully model) cannot weaponise the
    /// record to panic Bevy primitive constructors.
    pub fn sanitize(&mut self) {
        sanitize_avatar_visuals(&mut self.visuals);
        self.locomotion.sanitize();
    }
}

/// FNV-1a 64-bit hash of a string. Matches the hash used by
/// [`crate::pds::room::RoomRecord::default_for_did`] so peer rooms and
/// avatars derive their colour palettes from the same DID-derived seed.
fn fnv1a_64(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Build a normalised quaternion `[x, y, z, w]` from a half-angle rotation
/// around the X axis. Used by [`AvatarRecord::default_for_did`] to lay
/// pontoon capsules on their side without re-deriving the math at every
/// call site.
fn quat_x(angle_rad: f32) -> [f32; 4] {
    let half = angle_rad * 0.5;
    [half.sin(), 0.0, 0.0, half.cos()]
}

fn quat_xyzw(q: [f32; 4]) -> super::types::Fp4 {
    super::types::Fp4(q)
}

// ---------------------------------------------------------------------------
// Avatar record fetch / publish
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GetAvatarResponse {
    value: AvatarRecord,
}

/// Fetch a player's avatar record from their PDS. Result semantics mirror
/// [`super::fetch_room_record`]: `Ok(None)` is a clean 404 ("no record
/// yet"), and any other failure returns an `Err` the caller distinguishes
/// so it does not silently overwrite a live record with the default.
pub async fn fetch_avatar_record(
    client: &reqwest::Client,
    did: &str,
) -> Result<Option<AvatarRecord>, FetchError> {
    let pds = resolve_pds(client, did)
        .await
        .ok_or(FetchError::DidResolutionFailed)?;
    let url = format!(
        "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey=self",
        pds, did, AVATAR_COLLECTION
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| FetchError::Network(e.to_string()))?;
    let status = resp.status();
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        if let Ok(xrpc) = serde_json::from_str::<XrpcError>(&body)
            && let Some(err) = xrpc.error.as_deref()
            && (err == "RecordNotFound"
                || (err == "InvalidRequest" && body.contains("RecordNotFound")))
        {
            return Ok(None);
        }
        return Err(FetchError::PdsError(status.as_u16()));
    }
    let wrapper: GetAvatarResponse = resp
        .json()
        .await
        .map_err(|e| FetchError::Decode(e.to_string()))?;
    let mut record = wrapper.value;
    record.sanitize();
    Ok(Some(record))
}

#[derive(Serialize)]
struct PutAvatarRequest<'a> {
    repo: &'a str,
    collection: &'a str,
    rkey: &'a str,
    record: &'a AvatarRecord,
}

async fn try_put_avatar(
    _client: &reqwest::Client,
    pds: &str,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: &AvatarRecord,
) -> PutOutcome {
    let url = format!("{}/xrpc/com.atproto.repo.putRecord", pds);
    let body = PutAvatarRequest {
        repo: &session.did,
        collection: AVATAR_COLLECTION,
        rkey: "self",
        record,
    };
    let body_json = match serde_json::to_value(&body) {
        Ok(v) => v,
        Err(e) => return PutOutcome::Transport(format!("serialize: {e}")),
    };
    let (status, body) =
        match crate::oauth::oauth_post_with_refresh(&session.session, refresh, &url, &body_json)
            .await
        {
            Ok(pair) => pair,
            Err(e) => return PutOutcome::Transport(e),
        };
    if status.is_success() {
        return PutOutcome::Ok;
    }
    let msg = format!("putRecord (avatar) failed: {} — {}", status, body);
    if status.is_server_error() {
        PutOutcome::ServerError(msg)
    } else {
        PutOutcome::ClientError(msg)
    }
}

#[derive(Serialize)]
struct DeleteAvatarRequest<'a> {
    repo: &'a str,
    collection: &'a str,
    rkey: &'a str,
}

async fn delete_avatar_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;
    let url = format!("{}/xrpc/com.atproto.repo.deleteRecord", pds);
    let body = DeleteAvatarRequest {
        repo: &session.did,
        collection: AVATAR_COLLECTION,
        rkey: "self",
    };
    let body_json = serde_json::to_value(&body).map_err(|e| e.to_string())?;
    let (status, body) =
        crate::oauth::oauth_post_with_refresh(&session.session, refresh, &url, &body_json).await?;
    if status.is_success() || status.as_u16() == 404 {
        Ok(())
    } else {
        Err(format!(
            "deleteRecord (avatar) failed: {} — {}",
            status, body
        ))
    }
}

/// Upsert the avatar record to the authenticated user's own PDS. Uses the
/// same 5xx → delete-then-put recovery path as
/// [`super::publish_room_record`].
pub async fn publish_avatar_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: &AvatarRecord,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;
    match try_put_avatar(client, &pds, session, refresh, record).await {
        PutOutcome::Ok => Ok(()),
        PutOutcome::ClientError(msg) => Err(msg),
        PutOutcome::Transport(msg) => Err(msg),
        PutOutcome::ServerError(first_err) => {
            warn!("{first_err} — retrying via delete+put for avatar");
            delete_avatar_record(client, session, refresh)
                .await
                .map_err(|e| format!("{first_err}; fallback delete failed: {e}"))?;
            match try_put_avatar(client, &pds, session, refresh, record).await {
                PutOutcome::Ok => Ok(()),
                PutOutcome::ClientError(m)
                | PutOutcome::ServerError(m)
                | PutOutcome::Transport(m) => Err(format!("{first_err}; fallback put failed: {m}")),
            }
        }
    }
}
