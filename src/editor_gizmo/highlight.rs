//! In-scene selection highlight (#822 / W5): wire boxes around what the
//! gizmo will affect.
//!
//! Before this existed the gizmo itself was the only in-scene selection
//! indicator — for a large construct, a buried anchor, or a node whose
//! gizmo sits on a *different* scatter instance, the owner couldn't see
//! what a drag was about to move. Each frame this module draws, via
//! [`Gizmos`] retained-free line rendering (WebGL2-safe — the same path
//! the copy-drag ghost uses):
//!
//! * a bright wire box around the merged world bounds of the selected
//!   node **and its whole subtree** on the gizmo-hosting instance, and
//! * a dim box around every *other* live instance of the same blueprint
//!   node — a blueprint edit rewrites all of them, so the blast radius
//!   is shown honestly (one box per instance, subtree-merged).
//!
//! Placement selections get the bright box around the placement's
//! spawned subtree; avatar selections around the selected visuals node's
//! subtree. While a BlobGroup **element** is under edit the highlight
//! stands down entirely — the wireframe surface + red/green proxies
//! (#705) already own that picture.
//!
//! Bounds come from each mesh entity's render [`Aabb`] transformed to
//! world space and merged over the subtree (ECS descendants — which
//! keeps working mid-drag, when the hosted prim is detached from its
//! parent but keeps its own children). The box follows the gizmo's
//! frame preference (#871): in World mode it is world-axis-aligned; in
//! Local mode it is oriented to the boxed instance's accumulated
//! rotation — the same frame the gizmo's handles use — so the toggle is
//! legible at a glance. An indicator, not a fitted hull, either way.

use bevy::camera::primitives::Aabb;
use bevy::ecs::hierarchy::Children;
use bevy::prelude::*;
use transform_gizmo_bevy::GizmoTarget;

use crate::config::ui::selection_highlight as cfg;
use crate::ui::avatar::AvatarEditorState;
use crate::ui::room::{EditorTab, RoomEditorState};
use crate::world_builder::{AttachmentPrim, AvatarVisualPrim, PlacementMarker, PrimMarker};

use super::blob::BlobEditContext;
use super::{ActiveTarget, GizmoDetachedPrim, GizmoFramePref, determine_active_target};

/// Draw the selection wire boxes. Runs after [`super::sync`] in
/// `PostUpdate` (transforms propagated, this frame's gizmo host known).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn draw_selection_highlight(
    mut gizmos: Gizmos<crate::editor_gizmo::EditorOverlayGizmos>,
    access: crate::ui::toolbar::RoomEditAccess,
    room_state: Res<RoomEditorState>,
    avatar_state: Res<AvatarEditorState>,
    blob_ctx: Res<BlobEditContext>,
    prim_query: Query<(
        Entity,
        &PrimMarker,
        Has<GizmoTarget>,
        Has<GizmoDetachedPrim>,
    )>,
    avatar_prim_query: Query<(Entity, &AvatarVisualPrim)>,
    worn_props: Query<(Entity, &crate::player::attachments::LocalAttachment)>,
    part_prims: Query<(Entity, &AttachmentPrim)>,
    placement_query: Query<(Entity, &PlacementMarker)>,
    children: Query<&Children>,
    bounds_query: Query<(&Aabb, &GlobalTransform)>,
    frame_pref: Res<GizmoFramePref>,
    transforms: Query<&GlobalTransform>,
    proxies: Query<(Entity, &crate::editor_gizmo::blob::proxy::BlobElementProxy)>,
) {
    // Element sculpting owns the in-scene picture (#705): wireframe +
    // proxies. A whole-node box on top would only add noise — but the
    // SELECTED element gets the ordinary amber box (#1243 f151). Element
    // identity was carried entirely by hue and alpha: add is green, carve
    // is red — the canonical unsafe pair — and "selected" is the same hue
    // at 0.55 instead of 0.28, read through a translucent wireframe shell.
    // For concentric elements the gizmo handles sit at the same point too,
    // so there was no way at all to tell which of eight overlapping ghosts
    // a drag would reshape. One box, in the app's one selection colour, is
    // a non-colour, non-alpha channel and unifies the vocabulary.
    if blob_ctx.active.is_some()
        && let Some(index) = blob_ctx.selected_element
    {
        let selected = Color::srgba(
            cfg::SELECTED_COLOR[0],
            cfg::SELECTED_COLOR[1],
            cfg::SELECTED_COLOR[2],
            cfg::SELECTED_COLOR[3],
        );
        for (entity, proxy) in proxies.iter() {
            if !blob_ctx.hosts(proxy.blob_entity) {
                continue;
            }
            if proxy.index == index {
                draw_subtree_box(
                    &mut gizmos,
                    entity,
                    selected,
                    None,
                    &children,
                    &bounds_query,
                );
            }
            // The carve cue: a cross through the element's centre, in the
            // wire colour. A SHAPE channel, so add-vs-carve no longer rests
            // on green-against-red alone.
            if proxy.is_carve()
                && let Ok(tf) = transforms.get(entity)
            {
                draw_carve_cross(&mut gizmos, tf, entity, &children, &bounds_query);
            }
        }
        return;
    }

    let mut active = determine_active_target(&room_state, &avatar_state);
    // Mirror the sync gate exactly, ownership included (#1237 f142): the
    // room gizmo — and therefore its highlight — exists only while the
    // World-editor window is open on a room the user owns.
    if active == ActiveTarget::Room && !access.can_edit_room() {
        active = ActiveTarget::None;
    }

    let selected = Color::srgba(
        cfg::SELECTED_COLOR[0],
        cfg::SELECTED_COLOR[1],
        cfg::SELECTED_COLOR[2],
        cfg::SELECTED_COLOR[3],
    );
    let sibling = Color::srgba(
        cfg::SIBLING_COLOR[0],
        cfg::SIBLING_COLOR[1],
        cfg::SIBLING_COLOR[2],
        cfg::SIBLING_COLOR[3],
    );

    // Local mode boxes each instance in ITS OWN accumulated rotation
    // (#871) — for a scattered blueprint every dim sibling shows its own
    // orientation, matching what a local-frame drag of that instance
    // would do. World mode keeps the axis-aligned merge.
    let local = frame_pref.orientation == transform_gizmo_bevy::GizmoOrientation::Local;
    let frame_of = |entity: Entity| -> Option<Quat> {
        local
            .then(|| transforms.get(entity).ok().map(|gt| gt.rotation()))
            .flatten()
    };

    match active {
        ActiveTarget::Room => match room_state.selected_tab {
            EditorTab::Generators => {
                let (Some(generator_ref), Some(path)) = (
                    room_state.tree.selection.root.as_ref(),
                    room_state.tree.selection.path.as_ref(),
                ) else {
                    return;
                };
                // The gizmo-hosting instance gets the bright subtree box;
                // every other live instance of the same node gets a dim
                // one — the edit will rewrite them all.
                for (entity, marker, has_gizmo, is_detached) in prim_query.iter() {
                    if marker.generator_ref != *generator_ref || marker.path != *path {
                        continue;
                    }
                    let color = if has_gizmo || is_detached {
                        selected
                    } else {
                        sibling
                    };
                    draw_subtree_box(
                        &mut gizmos,
                        entity,
                        color,
                        frame_of(entity),
                        &children,
                        &bounds_query,
                    );
                }
            }
            EditorTab::Placements => {
                let Some(index) = room_state.selected_placement else {
                    return;
                };
                for (entity, marker) in placement_query.iter() {
                    if marker.0 == index {
                        draw_subtree_box(
                            &mut gizmos,
                            entity,
                            selected,
                            frame_of(entity),
                            &children,
                            &bounds_query,
                        );
                    }
                }
            }
            _ => {}
        },
        ActiveTarget::Avatar => {
            let Some(path) = avatar_state.gizmo().visuals_path() else {
                return;
            };
            // Local-only and singular (see `sync`) — at most one match.
            for (entity, marker) in avatar_prim_query.iter() {
                if marker.path == *path {
                    draw_subtree_box(
                        &mut gizmos,
                        entity,
                        selected,
                        frame_of(entity),
                        &children,
                        &bounds_query,
                    );
                }
            }
        }
        ActiveTarget::Attachment => {
            let Some(rkey) = avatar_state.gizmo().worn_prop() else {
                return;
            };
            // Local-only and unique by record key (see `sync`) — at most one
            // match, so no proximity scan and no sibling dimming: a worn prop
            // has exactly one instance in the world.
            for (entity, worn) in worn_props.iter() {
                if worn.rkey == rkey {
                    draw_subtree_box(
                        &mut gizmos,
                        entity,
                        selected,
                        frame_of(entity),
                        &children,
                        &bounds_query,
                    );
                }
            }
        }
        ActiveTarget::AttachmentPart => {
            let Some((rkey, path)) = avatar_state.gizmo().worn_part() else {
                return;
            };
            // Local-only and unique by (record, path) — one match.
            for (entity, marker) in part_prims.iter() {
                if marker.rkey == rkey && marker.path == path {
                    draw_subtree_box(
                        &mut gizmos,
                        entity,
                        selected,
                        frame_of(entity),
                        &children,
                        &bounds_query,
                    );
                }
            }
        }
        ActiveTarget::None => {}
    }
}

/// Merge the world bounds of `root` and every ECS descendant into one
/// `(min, max)` pair, or `None` when the subtree carries no render
/// [`Aabb`] at all.
///
/// `frame: Some(rotation)` folds the corners in that rotated basis, so the
/// caller gets a tight OBB in that frame rather than the world-axis-aligned
/// merge; the caller is responsible for rotating the result back.
///
/// Shared (#1226) with the peer nametag surface, which needs the same
/// question answered for a different reason: where the top of somebody's
/// body is, so a name can be hung above it. One walk, one definition of
/// "the bounds of this thing" — a second copy would drift the moment a
/// chassis family changed how it nests its meshes.
pub(crate) fn subtree_world_bounds(
    root: Entity,
    frame: Option<Quat>,
    children: &Query<&Children>,
    bounds_query: &Query<(&Aabb, &GlobalTransform)>,
) -> Option<(Vec3, Vec3)> {
    let inv = frame.map(|q| q.inverse());
    let mut merged: Option<(Vec3, Vec3)> = None;
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if let Ok((aabb, gt)) = bounds_query.get(entity) {
            for corner in aabb_world_corners(aabb, gt) {
                let p = match inv {
                    Some(inv) => inv * corner,
                    None => corner,
                };
                merged = Some(merge_bounds(merged, p));
            }
        }
        if let Ok(kids) = children.get(entity) {
            stack.extend(kids.iter());
        }
    }
    merged
}

/// Draw one wire box around [`subtree_world_bounds`]. Entities without a
/// render [`Aabb`] (bare containers, anchors) contribute nothing; a
/// subtree with no meshes at all draws nothing rather than a zero box at
/// the origin.
///
/// `frame: Some(rotation)` (#871, gizmo in Local mode) folds the corners
/// in that rotated basis and draws the box oriented to it — a tight OBB
/// for the instance instead of the world-axis-aligned merge. For a
/// non-uniformly scaled *rotated* parent chain the extracted rotation is
/// an approximation (shear is not representable) — the same
/// approximation the gizmo handles themselves live with.
fn draw_subtree_box(
    gizmos: &mut Gizmos<crate::editor_gizmo::EditorOverlayGizmos>,
    root: Entity,
    color: Color,
    frame: Option<Quat>,
    children: &Query<&Children>,
    bounds_query: &Query<(&Aabb, &GlobalTransform)>,
) {
    let Some((min, max)) = subtree_world_bounds(root, frame, children, bounds_query) else {
        return;
    };
    let center = (min + max) * 0.5;
    let size = (max - min).max(Vec3::splat(cfg::MIN_BOX_EXTENT));
    let (translation, rotation) = match frame {
        Some(q) => (q * center, q),
        None => (center, Quat::IDENTITY),
    };
    // `cube` draws a unit wire cube through the Transform, so the scale
    // carries the box size — same idiom as the copy-drag ghost.
    gizmos.cube(
        Transform {
            translation,
            rotation,
            scale: size,
        },
        color,
    );
}

/// The eight corners of a local-space render [`Aabb`] in world space.
/// Corners (not center+extents) because a rotated transform must expand
/// the world box to *contain* the oriented body.
fn aabb_world_corners(aabb: &Aabb, gt: &GlobalTransform) -> [Vec3; 8] {
    let center = Vec3::from(aabb.center);
    let he = Vec3::from(aabb.half_extents);
    let signs = [
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(-1.0, -1.0, 1.0),
        Vec3::new(-1.0, 1.0, -1.0),
        Vec3::new(-1.0, 1.0, 1.0),
        Vec3::new(1.0, -1.0, -1.0),
        Vec3::new(1.0, -1.0, 1.0),
        Vec3::new(1.0, 1.0, -1.0),
        Vec3::new(1.0, 1.0, 1.0),
    ];
    signs.map(|s| gt.transform_point(center + he * s))
}

/// Fold one point into an accumulating (min, max) pair.
fn merge_bounds(acc: Option<(Vec3, Vec3)>, point: Vec3) -> (Vec3, Vec3) {
    match acc {
        None => (point, point),
        Some((min, max)) => (min.min(point), max.max(point)),
    }
}

/// Mark a carving blob element with a cross through its centre (#1243
/// f151). A shape channel beside the hue one, sized to the element's own
/// bounds so a small carve gets a small cross.
fn draw_carve_cross(
    gizmos: &mut Gizmos<crate::editor_gizmo::EditorOverlayGizmos>,
    tf: &GlobalTransform,
    entity: Entity,
    children: &Query<&Children>,
    bounds_query: &Query<(&Aabb, &GlobalTransform)>,
) {
    let Some((min, max)) = subtree_world_bounds(entity, None, children, bounds_query) else {
        return;
    };
    let half = ((max - min) * 0.5).max_element().max(0.02);
    let centre = tf.translation();
    let carve = crate::config::ui::blob_edit::PROXY_CARVE_COLOR;
    let colour = Color::srgba(carve[0], carve[1], carve[2], 1.0);
    for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
        gizmos.line(centre - axis * half, centre + axis * half, colour);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_of_a_translated_unit_box_span_the_expected_bounds() {
        let aabb = Aabb {
            center: Vec3::ZERO.into(),
            half_extents: Vec3::splat(0.5).into(),
        };
        let gt = GlobalTransform::from(Transform::from_xyz(10.0, 2.0, -3.0));
        let corners = aabb_world_corners(&aabb, &gt);
        let (min, max) = corners
            .iter()
            .fold(None, |acc, &p| Some(merge_bounds(acc, p)))
            .unwrap();
        assert!((min - Vec3::new(9.5, 1.5, -3.5)).length() < 1e-5);
        assert!((max - Vec3::new(10.5, 2.5, -2.5)).length() < 1e-5);
    }

    #[test]
    fn rotation_expands_the_world_bounds_to_contain_the_oriented_box() {
        // A unit box yawed 45° needs a √2-wide world AABB on X/Z.
        let aabb = Aabb {
            center: Vec3::ZERO.into(),
            half_extents: Vec3::splat(0.5).into(),
        };
        let gt = GlobalTransform::from(Transform::from_rotation(Quat::from_rotation_y(
            std::f32::consts::FRAC_PI_4,
        )));
        let (min, max) = aabb_world_corners(&aabb, &gt)
            .iter()
            .fold(None, |acc, &p| Some(merge_bounds(acc, p)))
            .unwrap();
        let half_diag = (0.5f32 * 0.5 + 0.5 * 0.5).sqrt();
        assert!((max.x - half_diag).abs() < 1e-5, "max.x = {}", max.x);
        assert!((min.x + half_diag).abs() < 1e-5);
        // Y is rotation-invariant for a yaw.
        assert!((max.y - 0.5).abs() < 1e-5);
    }

    #[test]
    fn scale_rides_through_the_world_corners() {
        let aabb = Aabb {
            center: Vec3::ZERO.into(),
            half_extents: Vec3::splat(0.5).into(),
        };
        let gt = GlobalTransform::from(Transform::from_scale(Vec3::new(2.0, 4.0, 6.0)));
        let (min, max) = aabb_world_corners(&aabb, &gt)
            .iter()
            .fold(None, |acc, &p| Some(merge_bounds(acc, p)))
            .unwrap();
        assert!((max - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-5);
        assert!((min + Vec3::new(1.0, 2.0, 3.0)).length() < 1e-5);
    }

    #[test]
    fn folding_in_the_matching_frame_gives_a_tight_box() {
        // A yawed unit box folded in ITS OWN frame (#871, gizmo Local)
        // stays 1×1×1 — the world-axis fold of the same corners widens
        // to √2 on X/Z (asserted by the rotation test above). This is
        // the visible difference between the two toggle modes.
        let aabb = Aabb {
            center: Vec3::ZERO.into(),
            half_extents: Vec3::splat(0.5).into(),
        };
        let yaw = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
        let gt = GlobalTransform::from(Transform::from_rotation(yaw));
        let inv = yaw.inverse();
        let (min, max) = aabb_world_corners(&aabb, &gt)
            .iter()
            .fold(None, |acc, &p| Some(merge_bounds(acc, inv * p)))
            .unwrap();
        let size = max - min;
        assert!(
            (size - Vec3::ONE).length() < 1e-5,
            "local-frame fold should be tight, got {size:?}"
        );
    }

    #[test]
    fn merge_accumulates_disjoint_boxes() {
        let a = merge_bounds(None, Vec3::new(-1.0, 0.0, 0.0));
        let b = merge_bounds(Some(a), Vec3::new(5.0, 2.0, -7.0));
        assert_eq!(b.0, Vec3::new(-1.0, 0.0, -7.0));
        assert_eq!(b.1, Vec3::new(5.0, 2.0, 0.0));
    }
}
