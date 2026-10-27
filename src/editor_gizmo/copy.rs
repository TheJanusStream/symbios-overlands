//! One copy of an instance set in the World Editor (#1644, P2.2 of epic
//! #1636): picked in the scene by its collider or its baked box - its
//! merged meshes keep nothing on the CPU for a mesh ray to read - and moved
//! with the gizmo through a proxy.
//!
//! The proxy ([`InstanceCopyProxy`]) is an entity of its own at the copy's
//! pose, with no mesh, that carries the set's [`PlacementMarker`]: the
//! gizmo's placement drag carries it as it carries an anchor, and its
//! falling edge hands it here ([`commit_copy_drag`]) instead of to the
//! set's anchor. While it moves, the copy's entities move with it
//! ([`follow_copy_proxy`]); on release the drag writes the copy's numbers -
//! a press that moved nothing writes nothing (#1396) - and the set's build
//! redraws that copy alone (`world_builder::instances`).

use bevy::math::Affine3A;
use bevy::prelude::*;
use transform_gizmo_bevy::{EnumSet, GizmoMode, GizmoTarget};

use crate::config::ui::selection_highlight as cfg;
use crate::pds::instances::{COPY_SCALE_ONE, centimetres, tenths_of_degree};
use crate::pds::sanitize::limits;
use crate::pds::{InstanceCopy, Placement, RoomRecord};
use crate::state::LiveRoomRecord;
use crate::ui::room::RoomEditorState;
use crate::world_builder::PlacementMarker;
use crate::world_builder::instances::InstanceBuilds;

/// The stand-in the gizmo moves for a picked copy (#1644): at the copy's
/// pose, carrying its set's [`PlacementMarker`].
#[derive(Component, Clone, Debug)]
pub(crate) struct InstanceCopyProxy {
    pub(crate) placement: usize,
    pub(crate) copy: usize,
    /// Where the record stands the copy, when the proxy was last put there.
    rest: Transform,
    /// Where the set's anchor stands.
    anchor: Transform,
    /// Each of the copy's entities with its frame in the copy's own, read
    /// while the copy stood at rest: what follows the proxy.
    parts: Vec<(Entity, Affine3A)>,
    /// Whether the gizmo held it when last looked at.
    held: bool,
}

/// Forget a picked copy the selection or the record has left behind
/// (#1644) - the selection moved on by a row, a menu or an undo, or the
/// set no longer holds the copy - before its proxy, the gizmo or the
/// highlight read it. The state is borrowed mutably only to forget, so it
/// is not stamped changed every frame.
pub(super) fn forget_stale_copy_pick(
    mut room_state: ResMut<RoomEditorState>,
    record: Option<Res<LiveRoomRecord>>,
) {
    if let Some(record) = record
        && room_state.copy_pick_is_stale(&record.0)
    {
        room_state.forget_copy_pick();
    }
}

/// Keep one proxy at the copy the owner picked, where the record stands it,
/// and none while no copy is picked. It is put back where the record stands
/// the copy only when that changes - the record edited, the copy numbered
/// anew - never because it stands elsewhere: the gizmo moved it there. The
/// frame the gizmo lets go it is left alone whatever changed, as the drag's
/// release reads it later that frame. A proxy no longer wanted puts its
/// copy's entities back where the record stands the copy: a drag that left
/// the copy where it was - a Shift-drag's original - moved them with it.
#[allow(clippy::type_complexity)]
pub(super) fn sync_copy_proxy(
    mut commands: Commands,
    room_state: Res<RoomEditorState>,
    record: Option<Res<LiveRoomRecord>>,
    builds: Option<Res<InstanceBuilds>>,
    heightmap: Option<Res<crate::terrain::FinishedHeightMap>>,
    mut proxies: Query<(
        Entity,
        &mut InstanceCopyProxy,
        &mut Transform,
        Option<&GizmoTarget>,
    )>,
    mut part_poses: Query<&mut Transform, Without<InstanceCopyProxy>>,
) {
    let wanted = room_state.picked_copy().and_then(|(placement, copy)| {
        let Placement::Instances { copies, .. } = record.as_ref()?.0.placements.get(placement)?
        else {
            return None;
        };
        Some((placement, copy, *copies.get(copy)?))
    });
    let ground = heightmap.as_deref().map(|heightmap| &heightmap.0);
    let at_rest = |placement: usize, copy: usize, numbers: &InstanceCopy| {
        let builds = builds.as_deref()?;
        let rest = builds.stand(placement, numbers, ground)?;
        let anchor = builds.anchor_of(placement)?;
        let to_copy = rest.compute_affine().inverse() * anchor.compute_affine();
        let parts = builds
            .copy_entities(placement, copy)
            .unwrap_or_default()
            .iter()
            .filter_map(|entity| {
                let local = part_poses.get(*entity).ok()?;
                Some((*entity, to_copy * local.compute_affine()))
            })
            .collect::<Vec<_>>();
        Some((rest, anchor, parts))
    };
    let record_changed = record.as_ref().is_some_and(|record| record.is_changed());
    let mut kept = false;
    let mut put_back = Vec::new();
    for (entity, mut proxy, mut transform, gizmo) in &mut proxies {
        let Some((placement, copy, numbers)) = wanted.filter(|(placement, copy, _)| {
            !kept && proxy.placement == *placement && proxy.copy == *copy
        }) else {
            commands.entity(entity).try_despawn();
            put_back.push((proxy.parts.clone(), proxy.anchor, proxy.rest));
            continue;
        };
        kept = true;
        let held = gizmo.is_some_and(GizmoTarget::is_active);
        if held || proxy.held {
            if proxy.held != held {
                proxy.held = held;
            }
            continue;
        }
        let Some((rest, anchor, parts)) = at_rest(placement, copy, &numbers) else {
            continue;
        };
        let rest_moved = !same_pose(&proxy.rest, &rest);
        // Written only where it differs: a write stamps a change, which
        // the follow reads as a move.
        if (rest_moved || record_changed) && !same_pose(&transform, &rest) {
            *transform = rest;
        }
        if rest_moved {
            proxy.rest = rest;
        }
        let same_parts = proxy.parts.len() == parts.len()
            && proxy
                .parts
                .iter()
                .zip(&parts)
                .all(|((a, _), (b, _))| a == b);
        if !same_parts || proxy.anchor != anchor {
            proxy.parts = parts;
            proxy.anchor = anchor;
        }
    }
    if !kept
        && let Some((placement, copy, numbers)) = wanted
        && let Some((rest, anchor, parts)) = at_rest(placement, copy, &numbers)
    {
        commands.spawn((
            rest,
            Visibility::default(),
            PlacementMarker(placement),
            InstanceCopyProxy {
                placement,
                copy,
                rest,
                anchor,
                parts,
                held: false,
            },
        ));
    }
    for (parts, anchor, rest) in put_back {
        pose_parts(&parts, &anchor, &rest, &mut part_poses);
    }
}

/// Whether two poses are one, within float noise.
fn same_pose(a: &Transform, b: &Transform) -> bool {
    a.translation.abs_diff_eq(b.translation, 1e-5)
        && a.rotation.abs_diff_eq(b.rotation, 1e-6)
        && a.scale.abs_diff_eq(b.scale, 1e-6)
}

/// Stand a copy's `parts` where its proxy at `at` carries them, in the
/// set's `anchor`, writing only a part that moves.
fn pose_parts(
    parts: &[(Entity, Affine3A)],
    anchor: &Transform,
    at: &Transform,
    poses: &mut Query<&mut Transform, Without<InstanceCopyProxy>>,
) {
    let in_anchor = anchor.compute_affine().inverse() * at.compute_affine();
    for (entity, in_copy) in parts {
        let Ok(mut part) = poses.get_mut(*entity) else {
            continue;
        };
        let next = Transform::from_matrix(Mat4::from(in_anchor * *in_copy));
        if !same_pose(&part, &next) {
            *part = next;
        }
    }
}

/// Move a picked copy's entities with its proxy, as the gizmo drags it -
/// and back where it stood when a drag is let go of with Escape.
pub(super) fn follow_copy_proxy(
    proxies: Query<(&InstanceCopyProxy, &Transform), Changed<Transform>>,
    mut parts: Query<&mut Transform, Without<InstanceCopyProxy>>,
) {
    for (proxy, transform) in &proxies {
        pose_parts(&proxy.parts, &proxy.anchor, transform, &mut parts);
    }
}

/// Draw the picked copy's box, in the selection's colour, where its proxy
/// stands - so it follows a drag.
pub(super) fn draw_picked_copy(
    mut gizmos: Gizmos<super::EditorOverlayGizmos>,
    room_state: Res<RoomEditorState>,
    record: Option<Res<LiveRoomRecord>>,
    builds: Option<Res<InstanceBuilds>>,
    proxies: Query<(&InstanceCopyProxy, &Transform)>,
) {
    let Some((placement, copy)) = room_state.picked_copy() else {
        return;
    };
    let Some(Placement::Instances { copies, .. }) =
        record.as_ref().and_then(|r| r.0.placements.get(placement))
    else {
        return;
    };
    let Some(variant) = copies.get(copy).map(|copy| copy.variant as usize) else {
        return;
    };
    let Some((lo, hi)) = builds
        .as_deref()
        .and_then(|builds| builds.variant_bounds(placement, variant))
    else {
        return;
    };
    let Some((_, at)) = proxies
        .iter()
        .find(|(proxy, _)| proxy.placement == placement && proxy.copy == copy)
    else {
        return;
    };
    let colour = Color::srgba(
        cfg::SELECTED_COLOR[0],
        cfg::SELECTED_COLOR[1],
        cfg::SELECTED_COLOR[2],
        cfg::SELECTED_COLOR[3],
    );
    let boxed = *at * Transform::from_translation((lo + hi) / 2.0).with_scale(hi - lo);
    gizmos.cube(boxed, colour);
}

/// The gizmo's handles for a copy: it moves anywhere, turns about the up
/// axis alone - a copy has a turn, not a tilt - and grows or shrinks
/// evenly, as its one size.
pub(super) fn copy_modes() -> EnumSet<GizmoMode> {
    let mut modes = EnumSet::new();
    modes.insert_all(GizmoMode::all_translate());
    modes.insert(GizmoMode::RotateY);
    modes.insert(GizmoMode::ScaleUniform);
    modes
}

/// Write a copy's drag into the record (#1644): copy `copy` of the set
/// `placement`, dragged to the world pose `dragged` from `original`. Its
/// place and turn are read in the set's anchor; a snapped copy's height is
/// its lift, read as an absolute placement's is - the dragged height less
/// the ground under where the drag BEGAN (#701, #1011) - so a sideways drag
/// keeps the lift it had. `duplicate` (Shift) leaves the copy where it was
/// and adds another where it was dropped, of the same variant. The index
/// written, or `None` where nothing was: no such copy, or no room for
/// another ([`crate::pds::instances::copy_refusal`]).
pub(super) fn commit_copy_drag(
    record: &mut RoomRecord,
    (placement, copy): (usize, usize),
    dragged: &Transform,
    original: &Transform,
    heightmap: Option<&crate::terrain::FinishedHeightMap>,
    duplicate: bool,
) -> Option<usize> {
    let full = duplicate && crate::pds::instances::copy_refusal(record, placement).is_some();
    let Some(Placement::Instances {
        generator_ref,
        transform,
        snap_to_terrain,
        variants,
        copies,
        ..
    }) = record.placements.get_mut(placement)
    else {
        return None;
    };
    let was = *copies.get(copy)?;
    let anchor = Transform::from(&*transform).with_scale(Vec3::ONE);
    let local = anchor.rotation.inverse() * (dragged.translation - anchor.translation);
    let lift = if *snap_to_terrain {
        // The ground it was drawn on: a seeded hamlet's house's is the
        // highest within its reach (#1797).
        let footing = crate::world_builder::compile::pad::copy_footing(
            generator_ref,
            usize::try_from(was.variant)
                .ok()
                .and_then(|variant| variants.get(variant)),
            &was,
        );
        let ground = heightmap.map_or(0.0, |heightmap| {
            crate::world_builder::snapped_ground_y(
                &heightmap.0,
                original.translation.x,
                original.translation.z,
                footing,
            )
        });
        dragged.translation.y - ground
    } else {
        local.y
    };
    let front = anchor.rotation.inverse() * dragged.rotation * Vec3::NEG_Z;
    let scale_pm = (dragged.scale.x * COPY_SCALE_ONE as f32).round();
    let moved = InstanceCopy {
        x_cm: centimetres(local.x),
        z_cm: centimetres(local.z),
        yaw_ddeg: tenths_of_degree(front.x.atan2(-front.z).to_degrees()),
        variant: was.variant,
        scale_pm: if scale_pm.is_finite() {
            scale_pm.clamp(1.0, limits::MAX_INSTANCE_SCALE_PM as f32) as u32
        } else {
            was.scale_pm
        },
        y_cm: centimetres(lift),
    };
    if duplicate {
        if full {
            return None;
        }
        copies.push(moved);
        Some(copies.len() - 1)
    } else {
        copies[copy] = moved;
        Some(copy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{InstanceSolid, TransformData};

    fn set(snap: bool, anchor: TransformData) -> RoomRecord {
        RoomRecord {
            placements: vec![Placement::Instances {
                generator_ref: "post".into(),
                transform: anchor,
                snap_to_terrain: snap,
                solid: InstanceSolid::Shell,
                variants: Vec::new(),
                copies: vec![InstanceCopy {
                    y_cm: 50,
                    variant: 0,
                    ..InstanceCopy::at(2.0, 3.0, 0.0)
                }],
            }],
            ..RoomRecord::default()
        }
    }

    fn copy_of(record: &RoomRecord, c: usize) -> InstanceCopy {
        let Placement::Instances { copies, .. } = &record.placements[0] else {
            panic!("a set");
        };
        copies[c]
    }

    /// A copy dragged in a turned anchor is written in the anchor's frame:
    /// its place, its turn clockwise and its size; a snapped copy keeps its
    /// lift for a sideways drag.
    #[test]
    fn a_dragged_copy_is_written_in_its_anchors_frame() {
        let turned = TransformData {
            translation: crate::pds::Fp3([10.0, 0.0, 0.0]),
            rotation: crate::pds::Fp4(
                Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2).to_array(),
            ),
            ..TransformData::default()
        };
        let mut record = set(true, turned);
        // Where the copy stood, the ground flat at 0: anchor + rot * (2, 0, 3).
        let anchor = Transform::from_xyz(10.0, 0.0, 0.0)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2));
        let original = Transform::from_translation(anchor * Vec3::new(2.0, 0.5, 3.0));
        // Moved one metre along the anchor's own X, turned a quarter
        // clockwise more, and grown to 1.5.
        let dragged = Transform::from_translation(anchor * Vec3::new(3.0, 0.5, 3.0))
            .with_rotation(anchor.rotation * Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::splat(1.5));
        let written = commit_copy_drag(&mut record, (0, 0), &dragged, &original, None, false);
        assert_eq!(written, Some(0));
        let copy = copy_of(&record, 0);
        assert_eq!((copy.x_cm, copy.z_cm), (300, 300));
        assert_eq!(copy.yaw_ddeg, 900, "a quarter turn clockwise");
        assert_eq!(copy.scale_pm, 1_500);
        assert_eq!(copy.y_cm, 50, "the lift it had");
    }

    /// Shift adds a copy where it was dropped, of the same variant, and
    /// leaves the dragged one where it stood.
    #[test]
    fn a_shift_drag_adds_a_copy_and_leaves_the_first() {
        let mut record = set(false, TransformData::default());
        let before = copy_of(&record, 0);
        let original = Transform::from_xyz(2.0, 0.5, 3.0);
        let dragged = Transform::from_xyz(7.0, 1.5, 3.0);
        let written = commit_copy_drag(&mut record, (0, 0), &dragged, &original, None, true);
        assert_eq!(written, Some(1));
        assert_eq!(copy_of(&record, 0), before);
        let added = copy_of(&record, 1);
        assert_eq!((added.x_cm, added.z_cm, added.y_cm), (700, 300, 150));
    }

    /// Two copies of a set drawn, the second picked, and its proxy.
    fn picked_second_copy() -> (App, Entity, Entity) {
        use crate::world_builder::instances::test_support::{anchor, instances_app, run_to_idle};
        let mut app = instances_app(vec![vec![
            InstanceCopy::at(0.0, 0.0, 0.0),
            InstanceCopy::at(5.0, 0.0, 90.0),
        ]]);
        app.init_resource::<RoomEditorState>();
        app.add_systems(PostUpdate, sync_copy_proxy);
        let root = anchor(&mut app, 0);
        run_to_idle(&mut app);
        app.world_mut()
            .resource_mut::<RoomEditorState>()
            .pick_copy(0, Some(1));
        app.update();
        let proxy = app
            .world_mut()
            .query_filtered::<Entity, With<InstanceCopyProxy>>()
            .single(app.world())
            .expect("one proxy");
        (app, root, proxy)
    }

    /// Each drawn copy's parts under `root`, by copy, with how far along Z
    /// each stands.
    fn parts_z(app: &App, root: Entity) -> Vec<(usize, f32)> {
        let world = app.world();
        world
            .get::<Children>(root)
            .expect("copies")
            .iter()
            .filter_map(|child| {
                let copy = world.get::<crate::world_builder::instances::InstanceCopyOf>(child)?;
                Some((copy.copy, world.get::<Transform>(child)?.translation.z))
            })
            .collect()
    }

    /// #1644: a picked copy's proxy stands where the copy does and carries
    /// its entities; moved as the gizmo moves it, the copy moves with it and
    /// no other copy does. Another copy picked - as a Shift-drag picks the
    /// copy it added - the first copy's parts go back where the record
    /// stands it; unpicked, the proxy goes.
    #[test]
    fn a_picked_copys_proxy_carries_the_copy_with_it() {
        use bevy::ecs::system::RunSystemOnce;
        let (mut app, root, proxy) = picked_second_copy();
        let at = *app.world().get::<Transform>(proxy).expect("a pose");
        assert!(at.translation.abs_diff_eq(Vec3::new(5.0, 0.0, 0.0), 1e-5));
        assert_eq!(
            app.world().get::<PlacementMarker>(proxy).map(|m| m.0),
            Some(0)
        );

        // Moved two metres along +Z, as a drag would move it.
        app.world_mut()
            .get_mut::<Transform>(proxy)
            .expect("a pose")
            .translation
            .z += 2.0;
        app.world_mut()
            .run_system_once(follow_copy_proxy)
            .expect("the follow runs");
        for (copy, z) in parts_z(&app, root) {
            if copy == 1 {
                assert!((z - 2.0).abs() < 1e-4, "the picked copy moved: {z}");
            } else {
                assert!(z.abs() < 1e-4, "no other copy moved: {z}");
            }
        }

        // The record left as it was, another copy picked.
        app.world_mut()
            .resource_mut::<RoomEditorState>()
            .pick_copy(0, Some(0));
        app.update();
        for (copy, z) in parts_z(&app, root) {
            assert!(
                z.abs() < 1e-4,
                "copy {copy} stands where the record does: {z}"
            );
        }

        app.world_mut()
            .resource_mut::<RoomEditorState>()
            .pick_copy(0, None);
        app.update();
        let left = app
            .world_mut()
            .query_filtered::<Entity, With<InstanceCopyProxy>>()
            .iter(app.world())
            .count();
        assert_eq!(left, 0, "no copy picked, no proxy");
    }

    /// The frame the gizmo lets go, the proxy stays where the drag left it
    /// whatever changed that frame - the drag's release reads it later in
    /// the frame - and the record changing after puts it back where the
    /// record stands the copy.
    #[test]
    fn a_proxy_let_go_of_waits_for_the_drags_release() {
        let (mut app, _, proxy) = picked_second_copy();
        let mut held = app.world_mut().entity_mut(proxy);
        held.get_mut::<Transform>().expect("a pose").translation.z += 2.0;
        held.get_mut::<InstanceCopyProxy>().expect("a proxy").held = true;
        let proxy_z = |app: &App| {
            app.world()
                .get::<Transform>(proxy)
                .map(|at| at.translation.z)
        };
        app.world_mut()
            .resource_mut::<LiveRoomRecord>()
            .set_changed();
        app.update();
        assert_eq!(proxy_z(&app), Some(2.0), "left for the release");
        app.world_mut()
            .resource_mut::<LiveRoomRecord>()
            .set_changed();
        app.update();
        assert_eq!(proxy_z(&app), Some(0.0), "back where the record stands it");
    }

    /// #1644: a picked copy the selection has left - another placement
    /// selected, by whatever path - or its set has lost is forgotten, so
    /// selecting the set again picks no copy; a pick still good is left
    /// alone, its state not stamped changed.
    #[test]
    fn a_stale_copy_pick_is_forgotten() {
        let mut app = App::new();
        app.insert_resource(LiveRoomRecord(set(false, TransformData::default())));
        app.init_resource::<RoomEditorState>();
        app.add_systems(Update, forget_stale_copy_pick);
        let picked = |app: &App| app.world().resource::<RoomEditorState>().picked_copy();
        app.world_mut()
            .resource_mut::<RoomEditorState>()
            .pick_copy(0, Some(0));
        app.update();
        let stamped = app.world().resource_ref::<RoomEditorState>().last_changed();
        app.update();
        assert_eq!(picked(&app), Some((0, 0)));
        assert_eq!(
            app.world().resource_ref::<RoomEditorState>().last_changed(),
            stamped,
            "a good pick is left alone"
        );

        app.world_mut()
            .resource_mut::<RoomEditorState>()
            .selected_placement = Some(3);
        app.update();
        app.world_mut()
            .resource_mut::<RoomEditorState>()
            .selected_placement = Some(0);
        assert_eq!(picked(&app), None, "selecting the set again picks no copy");

        app.world_mut()
            .resource_mut::<RoomEditorState>()
            .pick_copy(0, Some(0));
        if let Placement::Instances { copies, .. } = &mut app
            .world_mut()
            .resource_mut::<LiveRoomRecord>()
            .0
            .placements[0]
        {
            copies.clear();
        }
        app.update();
        assert_eq!(picked(&app), None, "the set lost the copy");
    }

    #[test]
    fn a_copy_turns_about_the_up_axis_alone() {
        let modes = copy_modes();
        assert!(modes.contains(GizmoMode::RotateY));
        assert!(!modes.contains(GizmoMode::RotateX) && !modes.contains(GizmoMode::RotateZ));
        assert!(modes.contains(GizmoMode::ScaleUniform) && !modes.contains(GizmoMode::ScaleX));
    }
}
