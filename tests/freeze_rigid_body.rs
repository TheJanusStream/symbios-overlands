//! The avatar visuals-edit chassis freeze, and the avian behaviour it rests
//! on (#740, #867, #1894).
//!
//! The freeze (`player::freeze_local_avatar_while_editing`) takes the local
//! chassis out of the simulation with avian's `RigidBodyDisabled` while a
//! gizmo is aimed at the avatar, and puts it back on release
//! (`player::rejoin_broad_phase` then hands it to the broad phase, so its
//! ground contact is found on the first step).
//!
//! From #740 (2026-07-11) until #1894 it could not. Through avian 0.7.0 an
//! insert/remove cycle of `RigidBodyDisabled` on a body with touching
//! contacts corrupted the physics-island bookkeeping: the contact kept its
//! island link across the disable and the re-enable linked it a second time
//! (`debug_assert!(contact.island.is_none())` in `Islands::add_contact` on
//! 0.6.1, at `dynamics/solver/islands/mod.rs:518` on 0.7.0), leaving the
//! constraint graph holding manifold handles past their pair's manifold
//! list - the solver's `pair.manifolds[manifold_index]` out-of-bounds panic
//! in release builds, the crash the #739 UV-mapping dropdown exposed. The
//! freeze "parked" the body instead - a full axis lock, zero gravity and
//! per-frame momentum zeroing, the body left in the simulation - and #867
//! deferred the locomotion rebuild's collider replace to the release.
//!
//! Avian 0.8.0 rebuilt that bookkeeping. A contact's island link moved out
//! of the contact edge into `PhysicsIslands::contact_nodes`, indexed by
//! contact id; every constraint and island change now goes through the
//! narrow phase's `ContactStatusChangeQueue`, applied by
//! `apply_contact_status_changes` (`dynamics/solver/plugin.rs`), which
//! unlinks a contact only if `contact_node` holds one; and a new observer,
//! `on_body_remove_rigid_body_disabled`
//! (`collision/narrow_phase/mod.rs:478`), drops a re-enabled body's
//! contacts so they are re-created fresh. [`plain_rigid_body_disabled_cycle`]
//! holds avian to it in every gate, with a live, island-linked ground
//! contact on both sides of the cycle.
//!
//! The sequence tests below drive the freeze recipe through the reported
//! scenarios (walk on bumpy terrain, freeze, visual-children rebuilds or a
//! collider replace, release, walk) and assert the solver invariant after
//! every step.

use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;

/// Gentle sine-bump heightfield (the shape the app's terrain collider has,
/// though the app's carries parry's internal-edge fix since #1538):
/// cell-scale relief so a walking capsule's contact spans a varying
/// number of heightfield triangles - multi-manifold contact pairs are a
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
    ));
    // Avian's collider backend expects mesh assets to exist even when no
    // mesh-backed collider does. It asked for a scene spawner too until
    // avian 0.8, built without its `bevy_scene` feature (#1576): its
    // scene-hierarchy colliders are compiled out, and with the feature on
    // it takes the spawner as optional anyway.
    app.init_asset::<Mesh>();
    app.add_plugins(symbios_overlands::player::GamePhysics);
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
/// `player::freeze_local_avatar_while_editing` does: momentum cleared and
/// the body disabled.
fn engage_freeze(app: &mut App, chassis: Entity) {
    set_velocity(app, chassis, Vec3::ZERO);
    app.world_mut()
        .entity_mut(chassis)
        .insert(RigidBodyDisabled);
}

/// Release the freeze the way the game does: enable the body, then hand it
/// back to the broad phase.
fn release_freeze(app: &mut App, chassis: Entity) {
    app.world_mut()
        .entity_mut(chassis)
        .remove::<RigidBodyDisabled>();
    symbios_overlands::player::rejoin_broad_phase(app.world_mut(), chassis);
}

/// The exact invariant whose violation was the reported panic: every
/// manifold handle in the constraint graph must resolve to an existing
/// manifold of its contact pair (`prepare_contact_constraints` indexes
/// `pair.manifolds[handle.manifold_index]` unconditionally). Sleeping
/// pairs holding zero handles are fine - sleeping *removes* constraints -
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

/// Step while frozen: the game's freeze clears the momentum the drives
/// write under a prop gizmo, so the harness does too.
fn step_frozen(app: &mut App, chassis: Entity, n: usize, phase: &str) {
    for i in 0..n {
        set_velocity(app, chassis, Vec3::ZERO);
        app.update();
        assert_graph_in_sync(app, phase, i);
    }
}

/// Max manifold count seen on any touching pair - used to prove the
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

/// What the contact graph and the island manager say about one collider
/// pair: the state the #740 defect needed (a touching pair with manifolds
/// whose contact is linked into an island).
#[derive(Debug, Clone, Copy, Default)]
struct GroundContact {
    /// The pair exists in `ContactGraph` at all.
    exists: bool,
    touching: bool,
    manifolds: usize,
    generates_constraints: bool,
    /// `PhysicsIslands::contact_node` holds a link for this contact - the
    /// link avian 0.7 added a second time on re-enable.
    island_linked: bool,
}

impl GroundContact {
    fn is_live(&self) -> bool {
        self.exists
            && self.touching
            && self.manifolds > 0
            && self.generates_constraints
            && self.island_linked
    }
}

fn ground_contact(app: &App, body: Entity, ground: Entity) -> GroundContact {
    use avian3d::dynamics::solver::islands::PhysicsIslands;
    let graph = app.world().resource::<ContactGraph>();
    let Some((edge, pair)) = graph.get(body, ground) else {
        return GroundContact::default();
    };
    let island_linked = app
        .world()
        .get_resource::<PhysicsIslands>()
        .is_some_and(|islands| islands.contact_node(edge.id).is_some());
    GroundContact {
        exists: true,
        touching: pair.is_touching(),
        manifolds: pair.manifolds.len(),
        generates_constraints: pair.generates_constraints(),
        island_linked,
    }
}

/// The reported crash sequence on the freeze recipe: walk (manifold
/// churn) → idle to sleep → freeze → visuals rebuilds (edit flushes) →
/// unfreeze → walk again. Passes when the constraint graph stays in sync
/// and no avian system panics.
#[test]
fn freeze_edit_unfreeze_keeps_constraint_graph_in_sync() {
    let mut app = app_with_physics();
    app.world_mut()
        .spawn((RigidBody::Static, heightfield_ground(), Transform::IDENTITY));
    let chassis = spawn_chassis(&mut app);
    spawn_visual_children(&mut app, chassis);

    // Land on the ground.
    step_checked(&mut app, 60, "settle");

    // Walk across cell boundaries for 3 s - manifold counts churn.
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
/// motion), plus rapid select/deselect toggles - the freeze/release edges
/// land on consecutive steps with live, changing contacts.
#[test]
fn freeze_toggles_during_motion_keep_constraint_graph_in_sync() {
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

/// #867 on the freeze: replace the COLLIDER (the locomotion rebuild's strip
/// and build) on a chassis held by the freeze - the sequence behind the
/// avatar re-roll meltdown (a re-seed changes the chassis family while the
/// Avatar editor's freeze holds the body, then the editor closes). Mimics
/// the game: `strip_preset_components` removes `Collider`, `Mass` and
/// `LockedAxes` and the build inserts the new family's in the same command
/// flush, with the visual children rebuilt alongside.
///
/// The game has run this in-game since #1894, which ended #867's deferral
/// of the rebuild to the release (the game-side test with the real systems
/// is `a_re_seed_under_the_freeze_stands_on_the_ground_after_release`, in
/// `player`'s freeze tests). The body
/// starts on a live ground contact, and after the release it must find the
/// ground again, stay finite, stay above it and keep the constraint graph
/// in sync.
#[test]
fn collider_replace_while_frozen_survives_release() {
    let mut app = app_with_physics();
    let ground = app
        .world_mut()
        .spawn((RigidBody::Static, heightfield_ground(), Transform::IDENTITY))
        .id();
    let chassis = spawn_chassis(&mut app);
    spawn_visual_children(&mut app, chassis);

    // Land and settle into a touching contact.
    step_checked(&mut app, 60, "settle");
    let settled = ground_contact(&app, chassis, ground);
    assert!(
        settled.is_live(),
        "the fixture: no live ground contact before the freeze ({settled:?})"
    );
    engage_freeze(&mut app, chassis);
    step_frozen(&mut app, chassis, 20, "frozen");

    // Four kind-changing re-seeds while frozen: strip + rebuild with a
    // different collider shape each round (capsule, cuboid hull), visuals
    // churning too.
    let shapes = [
        Collider::cuboid(1.6, 0.6, 0.8), // boat-ish hull
        Collider::capsule(0.35, 0.9),    // humanoid
        Collider::cuboid(1.1, 0.5, 2.2), // car-ish hull
        Collider::capsule(0.4, 1.1),     // humanoid re-roll
    ];
    for (round, shape) in shapes.into_iter().enumerate() {
        {
            let mut e = app.world_mut().entity_mut(chassis);
            // strip_preset_components: collider + mass + axes gone...
            e.remove::<(Collider, Mass, LockedAxes)>();
            // ...build_preset_components: new family, preset axes.
            e.insert((shape, Mass(60.0), HUMANOID_AXES));
        }
        rebuild_children(&mut app, chassis);
        step_frozen(&mut app, chassis, 20, &format!("reseed-frozen-{round}"));
    }

    // Close the editor: the freeze releases.
    release_freeze(&mut app, chassis);

    // ~5 s of live simulation: the meltdown signature was a clean free
    // fall through the ground within the first second, NaN inside two.
    for i in 0..300 {
        app.update();
        assert_graph_in_sync(&mut app, "released", i);
        let tf = *app.world().entity(chassis).get::<Transform>().unwrap();
        assert!(
            tf.translation.is_finite(),
            "chassis went non-finite after release at step {i}: {:?}",
            tf.translation
        );
        assert!(
            tf.translation.y > -2.0,
            "chassis fell through the ground after release at step {i}: y={}",
            tf.translation.y
        );
    }
    let after = ground_contact(&app, chassis, ground);
    assert!(
        after.is_live(),
        "no live ground contact five seconds after the release ({after:?})"
    );
}

/// The raw `RigidBodyDisabled` insert/remove cycle on a body resting on the
/// ground, which through avian 0.7.0 died on
/// `debug_assert!(contact.island.is_none())` when the re-enabled body's
/// contact was linked into an island a second time - the debug-build
/// tripwire of #740's release-mode solver panic. It failed on every run
/// from #740 to 0.7.0 (last on 2026-09-17, #1358) and passes on 0.8.0.
///
/// The game's freeze relies on it since #1894, so it runs in every gate.
/// It asserts what a pass needs in order to mean anything: a touching,
/// constraint-making, island-linked ground contact right before the
/// disable (an empty pass is what an avian change of contact defaults
/// could otherwise hand it), the contact gone while the body is disabled,
/// and the contact re-formed, linked again, after the re-enable. If it
/// fails after an avian bump, the freeze in `player` is what breaks.
#[test]
fn plain_rigid_body_disabled_cycle() {
    let mut app = app_with_physics();
    let ground = app
        .world_mut()
        .spawn((RigidBody::Static, heightfield_ground(), Transform::IDENTITY))
        .id();
    let chassis = spawn_chassis(&mut app);

    step_checked(&mut app, 60, "settle");

    // The precondition of the defect: a live, touching, island-linked
    // contact at the moment `RigidBodyDisabled` goes in.
    let before = ground_contact(&app, chassis, ground);
    assert!(
        before.is_live(),
        "canary is vacuous: no live chassis-ground contact before the disable ({before:?})"
    );

    app.world_mut()
        .entity_mut(chassis)
        .insert(RigidBodyDisabled);
    step_checked(&mut app, 5, "disabled");
    let during = ground_contact(&app, chassis, ground);
    assert!(
        !during.exists,
        "the disabled body kept its ground contact ({during:?})"
    );

    app.world_mut()
        .entity_mut(chassis)
        .remove::<RigidBodyDisabled>();
    step_checked(&mut app, 60, "re-enabled");

    // The re-enable is where the double island link happened: the contact
    // must come back, touching and linked, not merely not panic.
    let after = ground_contact(&app, chassis, ground);
    assert!(
        after.is_live(),
        "the chassis-ground contact did not re-form after the re-enable ({after:?})"
    );
}

/// Why the freeze's release calls `player::rejoin_broad_phase`, pinned on
/// avian as it stands: a body re-enabled where it rests has no contact
/// until its AABB leaves the enlarged one avian keeps for it, so it falls
/// into the ground first (the humanoid capsule here: no contact for seven
/// steps, 6 cm in); marked moved, it has its contact on the first step and
/// does not move.
///
/// If the unmarked half starts finding the ground on the first step, avian
/// re-pairs a re-enabled body itself and `rejoin_broad_phase` can go.
#[test]
fn a_re_enabled_body_needs_the_broad_phase_to_find_the_ground_at_once() {
    for rejoin in [false, true] {
        let mut app = app_with_physics();
        let ground = app
            .world_mut()
            .spawn((RigidBody::Static, heightfield_ground(), Transform::IDENTITY))
            .id();
        let chassis = spawn_chassis(&mut app);
        step_checked(&mut app, 60, "settle");
        let rest = app.world().get::<Position>(chassis).unwrap().0;

        app.world_mut()
            .entity_mut(chassis)
            .insert(RigidBodyDisabled);
        step_checked(&mut app, 5, "disabled");
        app.world_mut()
            .entity_mut(chassis)
            .remove::<RigidBodyDisabled>();
        if rejoin {
            symbios_overlands::player::rejoin_broad_phase(app.world_mut(), chassis);
        }
        step_checked(&mut app, 1, "re-enabled");

        let contact = ground_contact(&app, chassis, ground);
        let sunk = rest.y - app.world().get::<Position>(chassis).unwrap().0.y;
        if rejoin {
            assert!(
                contact.is_live(),
                "rejoined, no ground contact on the first step ({contact:?})"
            );
            assert!(sunk.abs() < 1e-4, "rejoined, it moved {sunk} m");
        } else {
            assert!(
                !contact.exists,
                "avian found a re-enabled body's ground contact on the first \
                 step by itself: `player::rejoin_broad_phase` can go ({contact:?})"
            );
        }
    }
}

/// Why `player::rejoin_broad_phase` clears the proxy's `BODY_DISABLED` flag,
/// pinned on avian as it stands: the flag is set by an observer of
/// `Discard<RigidBodyDisabled>` that reads whether the body has the
/// component, which it still has while the observer runs
/// (`collider_tree/update.rs`, Case 11), so a body re-enabled is left
/// flagged as a disabled one, and swept CCD of other bodies passes over it
/// (`dynamics/ccd/mod.rs`). Rejoined, the flag is clear.
///
/// If the half without the rejoin starts reading clear, avian sets the flag
/// right on re-enable and the clear in `rejoin_broad_phase` can go.
#[test]
fn a_re_enabled_body_is_left_flagged_disabled_without_the_rejoin() {
    use avian3d::collider_tree::{ColliderTreeProxyFlags, ColliderTreeProxyKey, ColliderTrees};
    let flagged = |app: &App, chassis: Entity| {
        let key = *app
            .world()
            .get::<ColliderTreeProxyKey>(chassis)
            .expect("the collider is in a tree");
        app.world()
            .resource::<ColliderTrees>()
            .get_proxy(key)
            .expect("a proxy")
            .flags
            .contains(ColliderTreeProxyFlags::BODY_DISABLED)
    };
    for rejoin in [false, true] {
        let mut app = app_with_physics();
        app.world_mut()
            .spawn((RigidBody::Static, heightfield_ground(), Transform::IDENTITY));
        let chassis = spawn_chassis(&mut app);
        step_checked(&mut app, 60, "settle");
        assert!(
            !flagged(&app, chassis),
            "the fixture: a live body's proxy is flagged disabled before any freeze"
        );

        app.world_mut()
            .entity_mut(chassis)
            .insert(RigidBodyDisabled);
        step_checked(&mut app, 5, "disabled");
        app.world_mut()
            .entity_mut(chassis)
            .remove::<RigidBodyDisabled>();
        if rejoin {
            symbios_overlands::player::rejoin_broad_phase(app.world_mut(), chassis);
        }
        step_checked(&mut app, 2, "re-enabled");

        if rejoin {
            assert!(
                !flagged(&app, chassis),
                "rejoined, the body's proxy is still flagged as a disabled body's"
            );
        } else {
            assert!(
                flagged(&app, chassis),
                "avian cleared a re-enabled body's BODY_DISABLED flag by itself: the clear in \
                 `player::rejoin_broad_phase` can go"
            );
        }
    }
}
