//! Local player plugin: spawns and drives the local avatar, hot-swaps
//! locomotion presets when the owner edits their PDS avatar record, and
//! paints matching visuals on remote peers.
//!
//! An [`AvatarRecord`](crate::pds::AvatarRecord) has two halves, and the
//! first of them is a `$type`-tagged open union rather than one shape
//! (`body: AvatarBody`, since #1056):
//!
//! - **`Rigged`** — a parametric `symbios-avatar` body, referenced by rkey
//!   into the identity's wardrobe and built by [`rigged`]. Every humanoid is
//!   one of these since #1060. It never goes near
//!   [`visuals::spawn_avatar_visuals`].
//! - **`Generator`** — a generator tree spawned by
//!   [`visuals::spawn_avatar_visuals`] (no colliders, no per-prim markers —
//!   pure cosmetics). Vehicles, and the pre-#1060 seeded humanoids.
//! - **`Absent`** (a pre-#1056 record: treated as "no record", so the seeded
//!   default is synthesised) and **`Unknown`** (a body kind this build does
//!   not know: rendered as a bare chassis, never re-serialized).
//!
//! The `locomotion` half is independent of which body kind is worn, and
//! selects one of five physics presets:
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
pub(crate) mod attachments;
mod car;
pub(crate) mod emote;
pub(crate) mod gait;
mod helicopter;
mod hotswap;
mod hover_boat;
pub mod humanoid;
mod portal;
mod preset;
mod respawn;
mod rigged;
mod spawn;
pub mod visuals;

pub(crate) use hotswap::AppliedAvatar;
pub(crate) use portal::PORTAL_COOLDOWN_SECS;
pub use portal::PortalContact;
pub use portal::PortalCooldown;
pub use portal::PortalTravelTask;
pub(crate) use portal::begin_portal_travel;
pub use preset::{
    AirplanePreset, CarPreset, HelicopterPreset, HoverBoatPreset, HumanoidPreset, VehicleChassis,
};
pub use respawn::{PlayerMove, PlayerMoveRequest, go_to_pose, return_to_spawn_blocked};
pub use rigged::RiggedBuildFailed;
pub(crate) use rigged::RiggedRoot;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_egui::input::egui_wants_any_keyboard_input;

use crate::config::rover as cfg;
use crate::state::{AppState, LocalPlayer};

/// Whether a modal dialog owns the user's attention, so the player must
/// not drive (#852, widened by #1241 f164, inverted out of `ui` by #1297
/// group 3).
///
/// egui modals block the pointer but NOT game keys, so without this the
/// player could WASD away from a portal mid-"Unpublished changes"
/// decision and "Publish & travel" would fire from wherever they had
/// drifted. The five input-driven drive systems (and the jump latch) gate
/// on `not(guard_attention_held)`; passive physics (suspension,
/// stabilisation, gravity) keeps running, same policy as the egui-focus
/// gate.
///
/// Written once per frame by `ui::confirm::mirror_attention_held` in
/// `PreUpdate`, so the FixedUpdate steps later in the same frame read
/// this frame's answer. The `RigHold` shape of #1158 a fourth time:
/// `ui` publishes the fact, the player owns the resource, and neither
/// half has to import the other's module.
///
/// **Default false on purpose.** A frame with no mirror reports "nothing
/// is holding attention" and the player can move. The opposite default
/// would freeze anyone whose mirror had gone unregistered — a failure
/// `ui::tests::the_mirrored_consumers_do_not_import_the_ui_layer` catches
/// at test time precisely because it is invisible at run time.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttentionHeld(pub bool);

/// The predicate [`AttentionHeld`] carries: a modal is up, in either of
/// the two forms this app has one.
///
/// The two are not interchangeable and that is the whole history of this
/// gate. `confirm::note_modal_open` lives in egui's per-context store,
/// which only a system holding an egui context can read;
/// `ui::unsaved_guard::UnsavedGuard` is an ECS resource that leaves no
/// stamp at all. The gate asked about the guard alone and therefore knew
/// about one of six modals — a gift offer from a stranger blocked every
/// click while W kept walking the avatar into a portal, stacking a second
/// modal behind the first.
///
/// Pure, and shared by the mirror that writes the resource and the test
/// that asserts it, so the two read one sentence rather than two copies
/// of an `||`.
pub(crate) fn attention_is_held(modal_open: bool, guard_present: bool) -> bool {
    modal_open || guard_present
}

/// Run condition: something owns attention, so the drive systems stand
/// down. See [`AttentionHeld`].
pub(crate) fn guard_attention_held(held: Res<AttentionHeld>) -> bool {
    held.0
}

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

/// Build the ground-detection ray filter shared by the vehicle suspension
/// casts and the humanoid jump-grounding check, excluding the caster's own
/// `chassis` plus every `Sensor` collider (pass `sensors.iter()`).
///
/// Sensors — the gateway veil ([`GatewayMarker`](crate::world_builder::GatewayMarker))
/// and portal cubes — are phantom walk-in volumes: `Sensor` exempts them
/// from contact-force resolution, but avian's `cast_ray` still reports them
/// as hits. Left in the ground ray, a gateway box reads as ground and the
/// suspension spring drives the vehicle up its surface instead of letting it
/// pass through into the zone (#813). Excluding all sensors keeps the
/// invariant that ground rays only ever see solid ground, with no per-prim
/// tagging or collision-layer scheme to maintain.
pub(super) fn ground_ray_filter(
    chassis: Entity,
    sensors: impl IntoIterator<Item = Entity>,
) -> SpatialQueryFilter {
    SpatialQueryFilter::default().with_excluded_entities(std::iter::once(chassis).chain(sensors))
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
        // The sibling crate's frame order (Build → Animate → Apply), its
        // pose-apply system and, since #1171, the per-body motion driver
        // itself. Its `AnimatorPlugin` is still deliberately absent, though
        // the reason changed under #1309: it is no longer a second driver —
        // at 0.6.0 it steers the same `AvatarDriver` this file does — but a
        // control SURFACE, one set of switches and an egui panel deciding
        // what every body it owns is doing. That is a viewer's shape. Here
        // each body is told what to do by its own chassis, which is the whole
        // of what `fill_rigged_drive` says. The two cannot collide in any
        // case: the window only steers bodies with no `Drive`, and every body
        // this file installs carries one. There is no clip library to own
        // either (#1067): every motion source is the engine's procedural
        // layer, so nothing is fetched, embedded or indexed before a body can
        // move.
        app.add_plugins(bevy_symbios_avatar::AvatarPlugin)
            .add_systems(
                Update,
                (
                    rigged::kick_rigged_builds,
                    rigged::land_rigged_builds,
                    // #1255, after the land so a body installed this frame
                    // retires the stand-in in the same frame's commands:
                    // the rigged build is the one wait that escapes the
                    // loading screen, and until it lands the owner used to
                    // be nothing at all.
                    rigged::announce_slow_builds,
                    rigged::sync_local_placeholder,
                    attachments::sync_rigged_attachments,
                )
                    .chain()
                    .in_set(bevy_symbios_avatar::AvatarSystems::Build)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_message::<emote::EmoteRequest>()
            .add_systems(
                Update,
                // The fill and the sibling crate's driver are one unit
                // (#1171): `fill_rigged_drive` writes what the chassis is
                // doing and `drive_avatar_bodies` turns it into a pose, so
                // the ordering is a correctness requirement rather than a
                // preference. `start_emotes` leads, so an emote requested
                // this frame is posed this frame rather than a frame after
                // the message that asked for it; the strain count trails,
                // because `Drove` only exists once a body has been driven.
                (
                    (rigged::start_emotes, rigged::fill_rigged_drive)
                        .chain()
                        .before(bevy_symbios_avatar::drive_avatar_bodies),
                    rigged::count_motion_strain.after(bevy_symbios_avatar::drive_avatar_bodies),
                )
                    .in_set(bevy_symbios_avatar::AvatarSystems::Animate)
                    .run_if(in_state(AppState::InGame)),
            );
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
                    gait::animate_avatar_gait,
                    portal::handle_portal_interaction,
                    portal::poll_portal_travel_tasks,
                    // After the poll, so the frame the record lands cannot
                    // also release the gate it just closed (#1231 f20):
                    // `WorldCompiled` is removed by that system and the
                    // command applies at the sync point, which is after
                    // this one has run.
                    portal::release_travel_on_arrival,
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
                    // Passive stabilisation runs UNGATED, exactly like
                    // suspension/buoyancy above: helicopter hover +
                    // self-righting and airplane cruise/lift/drag are
                    // not input responses, so egui focus must not cut
                    // them (#821 — the airship used to fall out of the
                    // sky whenever a text field grabbed the keyboard).
                    helicopter::apply_helicopter_stabilization,
                    airplane::apply_airplane_aerodynamics,
                    // The fourth preset's righting assist (#1240 f162),
                    // ungated like the car's and the hover-boat's: a
                    // flipped aircraft keeps righting even while the owner
                    // types in a chat field.
                    airplane::apply_airplane_uprighting.run_if(not(avatar_visuals_row_selected)),
                    // Disable keyboard-driven control systems while the
                    // owner is typing in an egui text field — otherwise
                    // WASD-heavy chat messages steer the vehicle through
                    // walls. Physics (suspension, buoyancy, gravity,
                    // hover/lift stabilisation) and the uprighting /
                    // respawn passes still run so a vehicle left mid-air
                    // keeps obeying gravity — and an airship keeps
                    // hovering.
                    hover_boat::apply_hover_boat_drive
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected))
                        .run_if(not(guard_attention_held)),
                    hover_boat::apply_hover_boat_uprighting
                        .run_if(not(avatar_visuals_row_selected)),
                    humanoid::apply_humanoid_walk
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected))
                        .run_if(not(guard_attention_held)),
                    // Ungated + chained right after the walk system: a
                    // queued jump tap must die with the first fixed step
                    // that could have acted on it, even when the walk
                    // system itself was gated off (#852).
                    humanoid::clear_jump_queue,
                    airplane::apply_airplane_forces
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected))
                        .run_if(not(guard_attention_held)),
                    helicopter::apply_helicopter_forces
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected))
                        .run_if(not(guard_attention_held)),
                    car::apply_car_suspension,
                    car::apply_car_drive
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected))
                        .run_if(not(guard_attention_held)),
                    car::apply_car_uprighting.run_if(not(avatar_visuals_row_selected)),
                    respawn::respawn_if_fallen,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .init_resource::<RigHold>()
            .init_resource::<AttentionHeld>()
            .init_resource::<humanoid::JumpQueued>()
            .init_resource::<respawn::PlayerMoveRequest>()
            .init_resource::<crate::player::LocalMovement>()
            .add_systems(
                Update,
                (
                    freeze_local_avatar_while_editing,
                    // The owner's unstuck command (#1240 f159). `Update`,
                    // not `FixedUpdate`: it is a one-shot position write
                    // answering a click, not part of the physics step.
                    respawn::apply_player_move,
                    // The swim/wade classification, for the mode banner
                    // (#1241 f160). UNGATED on egui focus, unlike the
                    // drive systems it mirrors — see its own doc.
                    humanoid::publish_movement_facts,
                    // Same gates as `apply_humanoid_walk`, so a space typed
                    // into chat (or pressed under a guard modal) never
                    // queues a jump for the moment focus returns (#852).
                    humanoid::latch_jump_input
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected))
                        .run_if(not(guard_attention_held)),
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Facts about how the local avatar is currently moving that the movement
/// code knows and no UI could see (#1241 f160, f168).
///
/// `WaterState` was referenced outside `player::humanoid` only by
/// `player::rigged::motion` and never by `src/ui` at all, so the key remap
/// it drives had no surface anywhere; the derived walk speed existed only
/// as a local inside the drive system, so the editor could not tell the
/// owner that their Run slider had gone below it.
///
/// Written by [`humanoid::publish_movement_facts`], which is deliberately
/// NOT the drive system: the drive systems stand down while an egui text
/// field has focus, and a banner that vanished whenever the player clicked
/// into chat would be worse than none.
///
/// Lives in `player` rather than `ui::modes` (#1158): these are facts
/// about how the local body is moving, produced here and merely
/// DISPLAYED by the mode banner and the locomotion editor. It already
/// carried `player::humanoid::WaterState`, so the type pointed this way
/// before the module did. The mirror image of [`RigHold`], which the
/// editor writes and this module reads.
///
// the player clicked into chat would be worse than none.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct LocalMovement {
    /// Dry / wading / swimming, from `humanoid_water_state`.
    pub water: humanoid::WaterState,
    /// The unshifted walk this body actually walks at (m/s), derived from
    /// the built rig — `None` until the rigged body lands. Read by the
    /// locomotion editor so the Run slider can say when it has been
    /// dragged below it (#1241 f168).
    pub derived_walk: Option<f32>,
    /// The CAMERA is below a water surface (#1241 f160). Separate from
    /// [`Self::water`], which classifies the avatar: a third-person orbit
    /// camera dips under the surface on its own and, because the water
    /// plane is back-face culled (`world_builder::material`), there is
    /// nothing to see from below — no tint, no fog swap, no surface at
    /// all. The player cannot tell swimming from falling through empty
    /// space, and the flow current then moves them for no visible reason.
    pub camera_submerged: bool,
}

/// Whether, and how, the avatar editor is holding the local body still
/// this frame (#1158).
///
/// The four questions the player systems ask, answered as data. They used
/// to ask `ui::avatar::AvatarEditorState` directly, which put an egui
/// resource type in the signature of the physics and animation drivers —
/// so a UI refactor could change locomotion, `player`'s unit tests had to
/// construct an editor state to exercise a gait, and the headless render
/// tool dragged the panel's state into scope to walk a body.
///
/// Mirrored once per frame by `ui::avatar::mirror_rig_hold`, which is the
/// only writer. Absent editor state (before login, in the render tool)
/// leaves every field `false`, which is exactly what the old
/// `Option<Res<…>>` degraded to.
///
/// The distinctions are NOT interchangeable and each is load-bearing —
/// see the predicates this mirrors on [`crate::ui::avatar::AvatarEditorState`],
/// which carry the reasoning (#1103, #1106).
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RigHold {
    /// A whole-prop gizmo is aimed: snap the rig to its REST pose, so the
    /// offset being edited and the body it is measured against agree.
    pub at_rest: bool,
    /// A part gizmo is aimed: hold the body EXACTLY where it stands. A
    /// pause, not a re-pose — selecting must never move anything.
    pub pose: bool,
    /// Any avatar-side gizmo is aimed: freeze the chassis and the
    /// cosmetic sway.
    pub still: bool,
    /// A visuals ROW is selected — narrower than [`Self::still`]. Gates
    /// the drive systems, whose non-physics side effects (gait state,
    /// jump triggers) only need suppressing while a row is being edited.
    pub visuals_row: bool,
}

/// Run condition: true when the avatar editor has a visuals row
/// selected — any node in the visuals tree, root or descendant. The five
/// locomotion drive systems gate on `not(this)` so WASD input does
/// nothing while the owner is editing visuals, and the hover-boat's
/// uprighting torque is gated so a gizmo-rotated chassis stays where the
/// user put it.
///
/// The actual full-body freeze lives in
/// [`freeze_local_avatar_while_editing`], which parks the chassis with a
/// full axis lock, and the cosmetic sway hold lives in
/// [`gait::animate_avatar_gait`] — both keyed on
/// [`RigHold::still`], which since #1103 is true exactly while a gizmo
/// is aimed at the avatar or something it wears
/// (visuals row, worn prop, worn-prop part). This input gate is narrower
/// still — the visuals row only — because the drive systems have
/// non-physics side effects (gait state, jump triggers) that only need
/// suppressing while a row is actively being edited, and the freeze
/// already neutralizes any movement they would cause under a prop gizmo.
fn avatar_visuals_row_selected(hold: Res<RigHold>) -> bool {
    hold.visuals_row
}

/// Marker carried by the chassis while the visuals-edit freeze is
/// engaged, remembering the [`LockedAxes`] to restore on release (the
/// humanoid preset locks rotation; the vehicle presets carry none).
///
/// `pub(super)` since #867: the locomotion hot-swap defers its body
/// rebuild while this marker is present — replacing the `Collider` on a
/// parked, touching body corrupts avian's contact bookkeeping the
/// same way the #740 `RigidBodyDisabled` cycle does, and the corrupted
/// pair surfaces on release as a fall-through-the-world + runaway
/// respawn feedback that ends in NaN.
#[derive(Component)]
pub(super) struct VisualsEditFreeze {
    prior_locked_axes: Option<LockedAxes>,
}

/// Hold the local player's chassis fully frozen while a gizmo is aimed
/// at the avatar or at something it wears
/// ([`RigHold::still`]): lock every axis, zero
/// gravity, and re-zero momentum each frame until the selection releases.
/// Freezing the chassis (rather than just gating the drive systems) stops
/// the passive movers too — suspension, buoyancy, gravity/falling, slope
/// creep — so the avatar holds its exact pose during the edit, even
/// mid-air. That matters for correctness as well as ergonomics: the drag
/// commit's world→local conversion reads the parent chassis's
/// `GlobalTransform`, which must be stable while the gizmo is attached.
///
/// The gate matches the cosmetic gait/sway hold in
/// [`gait::animate_avatar_gait`]: both key on the same selection-scoped
/// gate, so physics and sway agree. #814 had widened both to "window
/// open" because a selection-scoped freeze under a window-wide sway hold
/// left the body translating while its sway was pinned; #1103 (owner
/// direction — the World editor's contract, pinned only under a gizmo)
/// narrowed both together, which keeps them consistent the other way
/// round. Every path that hides the window releases the selections, so a
/// closed editor never holds.
///
/// Deliberately NOT `RigidBodyDisabled` (#740): an insert/remove
/// cycle of `RigidBodyDisabled` on a body with touching
/// contacts corrupts the physics-island bookkeeping — the contact edge
/// keeps its island link across the disable, the re-enable island-links
/// it a second time, and the constraint graph is left holding manifold
/// handles past the pair's manifold list. In release builds that
/// surfaces as the solver's `manifolds[manifold_index]` index-out-of-
/// bounds panic on the next edit (the #739 UV-dropdown crash was this).
/// `tests/freeze_rigid_body.rs` carries the ignored upstream repro,
/// which STILL FAILS on avian 0.7.0 (re-run 2026-08-30, #1150) — two
/// majors and a Bevy train on, so this is not a legacy workaround; the
/// axis-lock freeze below never changes the body's simulation
/// membership, so islands and the constraint graph stay untouched.
/// Revisit when the engine moves to Bevy 0.19 / avian 0.7+.
///
/// State-synced rather than edge-triggered, so both recovery paths heal
/// on the next frame: a fresh chassis entity (room travel respawn) has
/// no marker and re-engages from scratch, while a locomotion hot-swap
/// mid-edit (record Load/Reset strips + rebuilds preset components on
/// the same entity) re-inserts the new preset's `LockedAxes` over the
/// full lock — the re-assert arm below locks it again and re-captures
/// the *new* preset's axes as the restore target. The per-frame
/// velocity re-zero (not just at engage) discards anything the
/// still-running solver injects — penetration recovery, restitution
/// residue — so nothing accumulates toward a burst on release.
#[allow(clippy::type_complexity)]
fn freeze_local_avatar_while_editing(
    mut commands: Commands,
    hold: Res<RigHold>,
    traveling: Option<Res<crate::state::TravelingTo>>,
    mut q: Query<
        (
            Entity,
            &mut LinearVelocity,
            &mut AngularVelocity,
            Option<&LockedAxes>,
            Option<&mut VisualsEditFreeze>,
        ),
        (With<LocalPlayer>, With<RigidBody>),
    >,
) {
    // Held for an avatar-editing session AND while a portal travel is in
    // flight (#842): travel suppresses the drive systems but used to
    // leave the chassis loose under gravity/physics, so aircraft sagged
    // and boats drifted through the fetch. Same park/release machinery
    // either way.
    let held = hold.still || traveling.is_some();
    for (entity, mut lin, mut ang, locked_axes, freeze) in q.iter_mut() {
        if held {
            lin.0 = Vec3::ZERO;
            ang.0 = Vec3::ZERO;
            match freeze {
                None => {
                    commands.entity(entity).try_insert((
                        VisualsEditFreeze {
                            prior_locked_axes: locked_axes.copied(),
                        },
                        LockedAxes::ALL_LOCKED,
                        GravityScale(0.0),
                    ));
                }
                // `LockedAxes` has no `PartialEq`; compare the bit masks.
                Some(mut freeze)
                    if locked_axes.map(LockedAxes::to_bits)
                        != Some(LockedAxes::ALL_LOCKED.to_bits()) =>
                {
                    // A mid-edit locomotion hot-swap replaced the lock
                    // with the new preset's axes: those are now what
                    // release must restore; lock everything again.
                    freeze.prior_locked_axes = locked_axes.copied();
                    commands.entity(entity).try_insert(LockedAxes::ALL_LOCKED);
                }
                Some(_) => {}
            }
        } else if let Some(freeze) = freeze {
            let mut entity_commands = commands.entity(entity);
            match freeze.prior_locked_axes {
                Some(prior) => {
                    entity_commands.try_insert(prior);
                }
                None => {
                    entity_commands.try_remove::<LockedAxes>();
                }
            }
            entity_commands.try_remove::<(GravityScale, VisualsEditFreeze)>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The suspension / grounding ray must skip its own chassis *and* every
    /// sensor (gateway veils, portals), so a vehicle drives into a gateway
    /// rather than climbing its surface (#813).
    #[test]
    fn ground_ray_filter_excludes_chassis_and_all_sensors() {
        let mut world = World::new();
        let chassis = world.spawn_empty().id();
        let gateway = world.spawn_empty().id();
        let portal = world.spawn_empty().id();
        let terrain = world.spawn_empty().id();

        let filter = ground_ray_filter(chassis, [gateway, portal]);

        assert!(filter.excluded_entities.contains(&chassis));
        assert!(filter.excluded_entities.contains(&gateway));
        assert!(filter.excluded_entities.contains(&portal));
        // Solid ground stays visible to the ray.
        assert!(!filter.excluded_entities.contains(&terrain));
        assert_eq!(filter.excluded_entities.len(), 3);
    }

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
