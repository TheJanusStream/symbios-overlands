//! Local player plugin: spawns and drives the local avatar, hot-swaps
//! locomotion presets when the owner edits their PDS avatar record, and
//! paints matching visuals on remote peers.
//!
//! Avatars are now uniform: the `visuals` half of
//! [`AvatarRecord`](crate::pds::AvatarRecord) is a
//! generator tree spawned by [`visuals::spawn_avatar_visuals`] (no
//! colliders, no per-prim markers — pure cosmetics), and the
//! `locomotion` half selects one of five physics presets:
//!
//! - **HoverBoat** — `RigidBody::Dynamic` cuboid chassis with four
//!   raycast-suspension corners + buoyancy + WASD drive (Hooke's-law
//!   spring, lateral grip, jump impulse).
//! - **Humanoid** — capsule rigid body with `LockedAxes` keeping it
//!   upright, velocity-driven walk controller, jump impulse, swim/wading
//!   modes triggered by water depth.
//! - **Airplane** — cuboid fuselage, continuous thrust, lift proportional
//!   to forward airspeed, pitch / roll / yaw torque from input.
//! - **Helicopter** — cuboid fuselage, auto-stabilising hover thrust,
//!   cyclic + strafe + yaw input, vertical climb/descend on Space/Shift.
//! - **Car** — cuboid chassis, four-corner raycast suspension, ground
//!   drive + steering + handbrake, no buoyancy.
//!
//! All five read their tuning from the live
//! [`LiveAvatarRecord`](crate::state::LiveAvatarRecord), so UI
//! edits take effect the same frame the slider moves. Changing the
//! locomotion *variant* triggers the hot-swap system, which tears down
//! all preset-specific components (collider, markers, locked axes) and
//! rebuilds them in the new preset's shape without disturbing the parent
//! `Transform` or rigid-body identity.
//!
//! ## Sub-module map
//!
//! * [`spawn`] — `OnEnter(InGame)` local-avatar spawn + the chassis root
//!   bundle (#670 easing guard).
//! * [`preset`] — per-preset physics components: the `PresetComponents`
//!   trait (one impl per locomotion `*Params`), preset markers, and the
//!   build/strip pair.
//! * [`hotswap`] — locomotion-variant rebuild, visuals repaint,
//!   remote-peer mirroring, and the terrain-hot-load lift.
//! * [`respawn`] — fall-through recovery.
//! * [`visuals`] — generator-tree visual spawner (`spawn_avatar_visuals`).
//! * [`gait`] — cosmetic bounce / sway / look-around animation on the
//!   humanoid visual root, driven by the seeded `AvatarGait`.
//! * [`hover_boat`] — HoverBoat preset: suspension / buoyancy / drive /
//!   uprighting systems.
//! * [`humanoid`] — Humanoid preset: walk controller (dry/wading/swim
//!   modes) and the `humanoid_water_state` classifier.
//! * [`airplane`] — Airplane preset: thrust + control-surface forces.
//! * [`helicopter`] — Helicopter preset: auto-stabilised hover + cyclic.
//! * [`car`] — Car preset: ground drive + steering + handbrake.
//! * [`portal`] — `handle_portal_interaction`,
//!   `poll_portal_travel_tasks`, and the `PortalTravelTask` async job.
//!   `begin_portal_travel` / `PortalCooldown` are re-exported for the
//!   unsaved-edits guard in [`crate::ui::unsaved_guard`], which owns the
//!   confirm step between portal contact and the actual travel fetch.

mod airplane;
mod car;
mod gait;
mod helicopter;
mod hotswap;
mod hover_boat;
mod humanoid;
mod portal;
mod preset;
mod respawn;
mod spawn;
pub mod visuals;

pub use portal::PortalCooldown;
pub(crate) use portal::begin_portal_travel;
pub use preset::{
    AirplanePreset, CarPreset, HelicopterPreset, HoverBoatPreset, HumanoidPreset, VehicleChassis,
};

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_egui::input::egui_wants_any_keyboard_input;

use crate::config::rover as cfg;
use crate::state::{AppState, LocalPlayer};
use crate::ui::avatar::AvatarEditorState;

// Corner offsets in local space for the four suspension rays. The
// hover-boat and car presets share the same four-corner pattern; their
// chassis half-extents differ but the rig topology does not.
pub(super) const CORNER_OFFSETS_RAW: [[f32; 3]; 4] = [
    [1.0, -1.0, 1.0],
    [-1.0, -1.0, 1.0],
    [1.0, -1.0, -1.0],
    [-1.0, -1.0, -1.0],
];

/// Multiply the canonical `CORNER_OFFSETS_RAW` by the preset's chassis
/// half-extents to get the four world-local suspension-ray origins. Both
/// [`hover_boat`] and [`car`] share this helper because the suspension
/// math is identical — only the chassis size differs.
pub(super) fn chassis_corners(half_extents: Vec3) -> [Vec3; 4] {
    CORNER_OFFSETS_RAW.map(|raw| Vec3::new(raw[0], raw[1], raw[2]) * half_extents)
}

/// Steering-direction multiplier from a vehicle's signed longitudinal speed:
/// `-1` while genuinely reversing (below `-REVERSE_STEER_SPEED`), else `+1`.
/// Both [`car`] and [`hover_boat`] multiply their A/D yaw torque by it so the
/// heading response inverts in reverse — with the wheels/rudder held one way a
/// real vehicle turns the opposite way backing up — while the deadband keeps
/// the forward sign (and so turn-in-place) around a standstill, so the sign
/// doesn't flip on sub-m/s creep.
pub(super) fn reverse_steer_sign(forward_speed: f32) -> f32 {
    if forward_speed < -cfg::REVERSE_STEER_SPEED {
        -1.0
    } else {
        1.0
    }
}

/// Draw an (x, z) pair uniformly distributed inside a square of
/// `SPAWN_SCATTER_SIZE` metres per side, centred on the origin.
pub(super) fn random_spawn_xz() -> (f32, f32) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEED: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);
    let s = SEED.fetch_add(0xDA94_2042_E4DD_58B5, Ordering::Relaxed);
    let mut z = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    let u = (z as u32 as f32) / (u32::MAX as f32);
    let v = ((z >> 32) as u32 as f32) / (u32::MAX as f32);
    let side = cfg::SPAWN_SCATTER_SIZE;
    ((u - 0.5) * side, (v - 0.5) * side)
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn::spawn_local_player)
            .add_systems(
                Update,
                (
                    hotswap::detect_local_locomotion_change,
                    hotswap::apply_local_locomotion_rebuild,
                    hotswap::detect_remote_change,
                    hotswap::rebuild_local_visuals,
                    hotswap::lift_player_above_new_ground,
                    gait::attach_gait_animation,
                    gait::animate_humanoid_gait,
                    portal::handle_portal_interaction,
                    portal::poll_portal_travel_tasks,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    hover_boat::sync_hover_boat_physics,
                    hover_boat::apply_hover_boat_suspension,
                    hover_boat::apply_hover_boat_buoyancy,
                    // Disable keyboard-driven control systems while the
                    // owner is typing in an egui text field — otherwise
                    // WASD-heavy chat messages steer the vehicle through
                    // walls. Physics (suspension, buoyancy, gravity) and
                    // the uprighting / respawn passes still run so a
                    // vehicle left mid-air keeps obeying gravity.
                    hover_boat::apply_hover_boat_drive
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected)),
                    hover_boat::apply_hover_boat_uprighting
                        .run_if(not(avatar_visuals_row_selected)),
                    humanoid::apply_humanoid_walk
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected)),
                    airplane::apply_airplane_forces
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected)),
                    helicopter::apply_helicopter_forces
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected)),
                    car::apply_car_suspension,
                    car::apply_car_drive
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected)),
                    respawn::respawn_if_fallen,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                freeze_local_avatar_on_visuals_select.run_if(in_state(AppState::InGame)),
            );
    }
}

/// Run condition: true when the avatar editor has a visuals row
/// selected. The five locomotion drive systems gate on
/// `not(this)` so the avatar stays still while the owner is editing
/// its visuals — both for ergonomics (the gizmo can't track a moving
/// chassis precisely) and for correctness (the drag commit's
/// world→local conversion uses the parent chassis's `GlobalTransform`,
/// which is unstable while physics is integrating).
///
/// The uprighting torque on the hover-boat is also gated so the avatar
/// doesn't slowly tip itself back upright during a long edit; the user
/// can rotate the chassis with the gizmo and it stays where they put
/// it. Suspension and gravity-style passive systems remain on so a
/// floating avatar doesn't levitate during the edit.
fn avatar_visuals_row_selected(avatar_editor: Option<Res<AvatarEditorState>>) -> bool {
    avatar_editor
        .map(|e| e.has_visuals_selection())
        .unwrap_or(false)
}

/// On the rising edge of "avatar visuals row selected", zero the local
/// player's linear and angular velocity. Without this, residual momentum
/// from the moment of click drifts the chassis (and the gizmo target)
/// for a few seconds after the freeze gate engages. The locomotion
/// drive systems are already gated off, so they won't push velocity
/// back up — we just need the one-shot to clear what was there.
fn freeze_local_avatar_on_visuals_select(
    avatar_editor: Option<Res<AvatarEditorState>>,
    mut last_selected: Local<bool>,
    mut q: Query<(&mut LinearVelocity, &mut AngularVelocity), With<LocalPlayer>>,
) {
    let now_selected = avatar_editor
        .as_ref()
        .map(|e| e.has_visuals_selection())
        .unwrap_or(false);
    if now_selected && !*last_selected {
        for (mut lin, mut ang) in q.iter_mut() {
            lin.0 = Vec3::ZERO;
            ang.0 = Vec3::ZERO;
        }
    }
    *last_selected = now_selected;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The steer-sign shared by the car (#723) and hover-boat (#724) drives:
    /// forward sign held through a standstill (turn-in-place), inverted only
    /// once clearly reversing past the deadband.
    #[test]
    fn steer_sign_holds_forward_and_flips_only_when_clearly_reversing() {
        // Driving forward — normal steering.
        assert_eq!(reverse_steer_sign(5.0), 1.0);
        // Clearly reversing — inverted.
        assert_eq!(reverse_steer_sign(-5.0), -1.0);
        // Stopped — forward sign, so turn-in-place is preserved.
        assert_eq!(reverse_steer_sign(0.0), 1.0);
        // Within the reverse deadband (creep) — still forward sign.
        let deadband = cfg::REVERSE_STEER_SPEED;
        assert_eq!(reverse_steer_sign(-deadband * 0.5), 1.0);
        // Just past the deadband — inverted.
        assert_eq!(reverse_steer_sign(-deadband - 0.1), -1.0);
    }
}
