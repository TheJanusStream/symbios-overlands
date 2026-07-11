//! #740 regression coverage for the avatar visuals-edit chassis freeze.
//!
//! Root cause of the original crash: in avian 0.6, inserting and later
//! removing `RigidBodyDisabled` on a body with touching contacts corrupts
//! the physics-island bookkeeping — the contact edge keeps its island
//! link across the disable, the re-enable island-links it a second time
//! (`debug_assert!(contact.island.is_none())` in `Islands::add_contact`
//! catches it in debug builds), and the constraint graph ends up holding
//! manifold handles that outrange their pair's manifold list. In release
//! builds that surfaces later as the solver's
//! `pair.manifolds[manifold_index]` index-out-of-bounds panic
//! (`dynamics/solver/plugin.rs:398`) — the crash the #739 UV-mapping
//! dropdown edit exposed. `plain_rigid_body_disabled_cycle` reproduces
//! the upstream bug and stays `#[ignore]`d as a canary for future avian
//! upgrades.
//!
//! The fix (`player::freeze_local_avatar_on_visuals_select`) freezes via
//! `LockedAxes::ALL_LOCKED` + `GravityScale(0)` + per-frame velocity
//! zeroing instead — the body never leaves the simulation, so islands
//! and the constraint graph stay untouched. The two sequence tests here
//! drive that recipe through the full reported scenario (walk on bumpy
//! terrain → freeze → visuals-children rebuilds → unfreeze → walk) and
//! assert the solver invariant after every step.

use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;

/// Gentle sine-bump heightfield (the app's terrain collider class):
/// cell-scale relief so a walking capsule's contact spans a varying
/// number of heightfield triangles — multi-manifold contact pairs are a
/// precondition of the original panic (`index 2` needs ≥3 manifolds).
fn heightfield_ground() -> Collider {
    const N: usize = 33;
    let heights: Vec<Vec<f32>> = (0..N)
        .map(|x| {
            (0..N)
                .map(|z| {
                    let (x, z) = (x as f32, z as f32);
                    (x * 1.3).sin() * 0.12 + (z * 0.9).cos() * 0.12
                })
                .collect()
        })
        .collect();
    Collider::heightfield(heights, Vec3::new(32.0, 1.0, 32.0))
}

fn app_with_physics() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        bevy::asset::AssetPlugin::default(),
        bevy::scene::ScenePlugin,
    ));
    // Avian's collider backend expects mesh assets and the scene spawner
    // to exist even when no mesh-backed or scene-built colliders do.
    app.init_asset::<Mesh>();
    app.add_plugins(PhysicsPlugins::default());
    // Deterministic 64 Hz stepping, one fixed step per `app.update()`,
    // matching the app's tick rate.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
        1.0 / 64.0,
    )));
    app.insert_resource(Time::<Fixed>::from_hz(64.0));
    // Plugins register their diagnostics resources in `Plugin::finish`,
    // which `app.update()` alone never triggers.
    app.finish();
    app.cleanup();
    app
}

/// Humanoid-preset chassis (mirrors `player::preset`): dynamic capsule,
/// rotation locked, interpolated like the local player since #670.
const HUMANOID_AXES: LockedAxes = LockedAxes::ROTATION_LOCKED;

fn spawn_chassis(app: &mut App) -> Entity {
    app.world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::capsule(0.35, 0.9),
            Mass(75.0),
            LinearDamping(0.2),
            AngularDamping(1.0),
            HUMANOID_AXES,
            TransformInterpolation,
            Transform::from_xyz(0.0, 1.5, 0.0),
        ))
        .id()
}

fn spawn_visual_children(app: &mut App, chassis: Entity) {
    for i in 0..8 {
        let child = app
            .world_mut()
            .spawn(Transform::from_xyz(0.0, 0.15 * i as f32, 0.0))
            .id();
        app.world_mut().entity_mut(chassis).add_child(child);
    }
}

/// The app's visuals rebuild: despawn every chassis child, spawn a fresh
/// set (`player::visuals::spawn_avatar_visuals`).
fn rebuild_children(app: &mut App, chassis: Entity) {
    let children: Vec<Entity> = app
        .world()
        .entity(chassis)
        .get::<Children>()
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    for c in children {
        app.world_mut().entity_mut(c).despawn();
    }
    spawn_visual_children(app, chassis);
}

fn set_velocity(app: &mut App, chassis: Entity, v: Vec3) {
    if let Some(mut lin) = app
        .world_mut()
        .entity_mut(chassis)
        .get_mut::<LinearVelocity>()
    {
        lin.0 = v;
    }
}

/// Engage the visuals-edit freeze the way
/// `player::freeze_local_avatar_on_visuals_select` does since #740:
/// full axis lock + zero gravity, body stays in the simulation.
fn engage_freeze(app: &mut App, chassis: Entity) {
    set_velocity(app, chassis, Vec3::ZERO);
    app.world_mut()
        .entity_mut(chassis)
        .insert((LockedAxes::ALL_LOCKED, GravityScale(0.0)));
}

/// Release the freeze: restore the preset's axes, drop the gravity
/// override.
fn release_freeze(app: &mut App, chassis: Entity) {
    let mut e = app.world_mut().entity_mut(chassis);
    e.insert(HUMANOID_AXES);
    e.remove::<GravityScale>();
}

/// The exact invariant whose violation was the reported panic: every
/// manifold handle in the constraint graph must resolve to an existing
/// manifold of its contact pair (`prepare_contact_constraints` indexes
/// `pair.manifolds[handle.manifold_index]` unconditionally). Sleeping
/// pairs holding zero handles are fine — sleeping *removes* constraints —
/// the bug is a handle that outlives or outranges its manifold list.
fn assert_graph_in_sync(app: &mut App, phase: &str, step: usize) {
    use avian3d::dynamics::solver::constraint_graph::ConstraintGraph;
    let contact_graph = app.world().resource::<ContactGraph>();
    let constraint_graph = app.world().resource::<ConstraintGraph>();
    for (color_i, color) in constraint_graph.colors.iter().enumerate() {
        for handle in &color.manifold_handles {
            let Some((_, pair)) = contact_graph.get_by_id(handle.contact_id) else {
                panic!(
                    "constraint-graph desync at {phase} step {step}: \
                     color {color_i} holds a handle for contact {:?} that no \
                     longer exists in the contact graph",
                    handle.contact_id,
                );
            };
            assert!(
                handle.manifold_index < pair.manifolds.len(),
                "constraint-graph desync at {phase} step {step}: color {color_i} \
                 handle points at manifold {} but the pair has {} manifolds \
                 (touching={})",
                handle.manifold_index,
                pair.manifolds.len(),
                pair.is_touching(),
            );
        }
    }
}

fn step_checked(app: &mut App, n: usize, phase: &str) {
    for i in 0..n {
        app.update();
        assert_graph_in_sync(app, phase, i);
    }
}

/// Step while frozen: the app's freeze system re-zeroes momentum every
/// frame, so the harness does too.
fn step_frozen(app: &mut App, chassis: Entity, n: usize, phase: &str) {
    for i in 0..n {
        set_velocity(app, chassis, Vec3::ZERO);
        app.update();
        assert_graph_in_sync(app, phase, i);
    }
}

/// Max manifold count seen on any touching pair — used to prove the
/// harness actually exercises multi-manifold contacts.
fn max_manifolds(app: &App) -> usize {
    let graph = app.world().resource::<ContactGraph>();
    graph
        .iter_active_touching()
        .chain(graph.iter_sleeping_touching())
        .map(|p| p.manifolds.len())
        .max()
        .unwrap_or(0)
}

/// The reported crash sequence on the fixed freeze recipe: walk (manifold
/// churn) → idle to sleep → freeze → visuals rebuilds (edit flushes) →
/// unfreeze → walk again. Passes when the constraint graph stays in sync
/// and no avian system panics.
#[test]
fn axis_lock_freeze_edit_unfreeze_keeps_constraint_graph_in_sync() {
    let mut app = app_with_physics();
    app.world_mut()
        .spawn((RigidBody::Static, heightfield_ground(), Transform::IDENTITY));
    let chassis = spawn_chassis(&mut app);
    spawn_visual_children(&mut app, chassis);

    // Land on the ground.
    step_checked(&mut app, 60, "settle");

    // Walk across cell boundaries for 3 s — manifold counts churn.
    let mut seen_manifolds = 0;
    for i in 0..192 {
        let dir = if (i / 48) % 2 == 0 {
            Vec3::new(2.2, 0.0, 1.4)
        } else {
            Vec3::new(-1.8, 0.0, 2.0)
        };
        set_velocity(&mut app, chassis, dir);
        app.update();
        assert_graph_in_sync(&mut app, "walk", i);
        seen_manifolds = seen_manifolds.max(max_manifolds(&app));
    }
    assert!(
        seen_manifolds >= 2,
        "harness never produced a multi-manifold pair (max {seen_manifolds}); \
         ground too flat to exercise the crash preconditions"
    );

    // Stop and idle long enough for the island to sleep.
    set_velocity(&mut app, chassis, Vec3::ZERO);
    step_checked(&mut app, 128, "idle");

    // Row selected.
    engage_freeze(&mut app, chassis);
    step_frozen(&mut app, chassis, 20, "frozen");

    // Debounced edit flushes: full visual-children rebuild while frozen
    // (the reporter clicked through several dropdown modes).
    for round in 0..3 {
        rebuild_children(&mut app, chassis);
        step_frozen(&mut app, chassis, 20, &format!("rebuild-{round}"));
    }

    // Deselect, then walk again.
    release_freeze(&mut app, chassis);
    step_checked(&mut app, 60, "unfrozen");
    for i in 0..128 {
        set_velocity(&mut app, chassis, Vec3::new(-2.0, 0.0, -1.2));
        app.update();
        assert_graph_in_sync(&mut app, "walk-after", i);
    }
}

/// Freeze engaging while the capsule is still sliding (selection during
/// motion), plus rapid select/deselect toggles — the freeze/release edges
/// land on consecutive steps with live, changing contacts.
#[test]
fn axis_lock_freeze_toggles_during_motion_keep_constraint_graph_in_sync() {
    let mut app = app_with_physics();
    app.world_mut()
        .spawn((RigidBody::Static, heightfield_ground(), Transform::IDENTITY));
    let chassis = spawn_chassis(&mut app);
    spawn_visual_children(&mut app, chassis);

    step_checked(&mut app, 60, "settle");

    for round in 0..6 {
        // Get moving.
        for i in 0..24 {
            set_velocity(&mut app, chassis, Vec3::new(2.5, 0.0, -1.0));
            app.update();
            assert_graph_in_sync(&mut app, "toggle-walk", round * 100 + i);
        }
        // Freeze mid-slide, edit, unfreeze after a couple of steps.
        engage_freeze(&mut app, chassis);
        step_frozen(&mut app, chassis, 2, "toggle-frozen");
        rebuild_children(&mut app, chassis);
        step_frozen(&mut app, chassis, 2, "toggle-rebuilt");
        release_freeze(&mut app, chassis);
        step_checked(&mut app, 2, "toggle-unfrozen");
    }
}

/// Upstream canary, kept `#[ignore]`d: the raw `RigidBodyDisabled`
/// insert→remove cycle this file's freeze recipe exists to avoid. On
/// avian 0.6.1 it dies in `Islands::add_contact`
/// (`debug_assert!(contact.island.is_none())`) when the re-enabled
/// body's contacts are island-linked a second time — the debug-build
/// tripwire of the release-mode solver OOB panic from #740. Un-ignore
/// after an avian upgrade (0.7+/Bevy 0.19): if it passes, the visuals
/// freeze can go back to `RigidBodyDisabled`.
#[test]
#[ignore = "reproduces the upstream avian 0.6 island-corruption bug (#740)"]
fn plain_rigid_body_disabled_cycle() {
    let mut app = app_with_physics();
    app.world_mut()
        .spawn((RigidBody::Static, heightfield_ground(), Transform::IDENTITY));
    let chassis = spawn_chassis(&mut app);

    step_checked(&mut app, 60, "settle");

    app.world_mut()
        .entity_mut(chassis)
        .insert(RigidBodyDisabled);
    step_checked(&mut app, 5, "disabled");

    app.world_mut()
        .entity_mut(chassis)
        .remove::<RigidBodyDisabled>();
    step_checked(&mut app, 60, "re-enabled");
}
