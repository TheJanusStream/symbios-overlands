//! In-scene right-click context menu (#720): a fast create-and-position
//! workflow for the room owner. Right-clicking the ground or an object opens
//! a small menu offering:
//!
//! * **Select item** — open the World Editor on the Region Assets tab and
//!   select the exact sub-part under the cursor (identical to the left-click
//!   picker's Generators branch, but it also *opens* the editor).
//! * **Select placement** — open the World Editor on the Placements tab and
//!   select the enclosing placement.
//! * **Create new…** — a submenu mirroring the tree's `+ New` /
//!   `+ From Catalogue` / `+ From Inventory` add-root menus. Picking one
//!   builds the region asset, appends a `Placement::Absolute` at the exact
//!   ray-hit point, and lands on the new asset in the editor — collapsing the
//!   old "make asset → make placement → drag it off the origin" sequence into
//!   one click.
//!
//! **Right-button conflict.** Camera orbit is bound to the right mouse button
//! (`camera::gate_camera_on_gui`, `bevy_panorbit_camera`), so the menu cannot
//! open on right-*press*. [`detect_scene_right_click`] instead discriminates a
//! click from a drag: it records the cursor at press and opens the menu on
//! release only when the pointer stayed within [`DRAG_THRESHOLD_PX`]. A real
//! drag orbits the camera and never spawns a menu, so the two never fight.
//!
//! The menu itself is an egui [`egui::Popup`] of kind [`egui::PopupKind::Menu`]
//! anchored at the click position; egui owns its close behaviour (click,
//! click-outside, Escape) via `open_bool`, and its submenu-aware close logic
//! keeps the parent open while the user is inside `Create new…`.

use std::cell::RefCell;

use bevy::ecs::hierarchy::ChildOf;
use bevy::picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;
use transform_gizmo_bevy::GizmoTarget;

use crate::pds::{Fp, Fp3, Fp4, Generator, Placement, RoomRecord, TransformData};
use crate::state::{CurrentRoomDid, LiveInventoryRecord, LiveRoomRecord};
use crate::terrain::TerrainMesh;
use crate::ui::catalogue::catalogue_menu;
use crate::ui::room::construct::{ROOM_ROOT_KINDS, make_default_for_kind};
use crate::ui::room::generators::{GeneratorTreeSource, RoomTreeSource};
use crate::ui::room::{EditorTab, GenNodeId, RoomEditorState};
use crate::ui::toolbar::UiPanels;
use crate::world_builder::{PlacementMarker, PrimMarker};

/// Screen-space travel (px) beyond which a held right button is an orbit
/// DRAG rather than a click. Below it, the release opens the context menu.
const DRAG_THRESHOLD_PX: f32 = 6.0;

/// State backing the in-scene right-click menu: the press-phase click-vs-drag
/// tracker plus the resolved hit that an open menu acts on.
#[derive(Resource, Default)]
pub(super) struct SceneContextMenu {
    /// Cursor position at the last right-button press; `None` between a
    /// release and the next press. Seeds the click-vs-drag comparison.
    press_origin: Option<Vec2>,
    /// Set once the pointer travels past [`DRAG_THRESHOLD_PX`] while the
    /// right button is held — the gesture is an orbit drag, not a click.
    dragged: bool,
    /// Whether the menu is currently shown. Driven open by
    /// [`detect_scene_right_click`]; egui's `open_bool` flips it back to
    /// closed on click / click-outside / Escape.
    open: bool,
    /// Screen-space anchor for the popup — the release-frame cursor.
    anchor: Vec2,
    /// World-space ray hit under the cursor: the spawn point for a
    /// `Create new…` placement.
    world_pos: Vec3,
    /// Placement index under the cursor, if an object was hit. Drives the
    /// "Select placement" entry.
    placement: Option<usize>,
    /// The exact sub-part (generator ref + prim path) under the cursor, if
    /// an object was hit. Drives the "Select item" entry.
    prim: Option<PrimMarker>,
}

/// The action a menu click selected, applied after the popup releases its
/// borrow of the resource. `Create` carries the fully-built generator so the
/// (borrow-checked) egui closures only ever *record* a choice.
enum MenuChoice {
    SelectItem,
    SelectPlacement,
    Create {
        prefix: String,
        // Boxed: a built `Generator` (esp. a Shape-grammar / L-system
        // blueprint) dwarfs the empty Select variants.
        generator: Box<Generator>,
    },
}

/// Update-schedule detector: tracks the right-button click-vs-drag gesture and,
/// on a clean click over the world, raycasts and arms [`SceneContextMenu`].
/// Owner-gated exactly like the World Editor and the left-click picker.
#[allow(clippy::too_many_arguments)]
pub(super) fn detect_scene_right_click(
    mut contexts: EguiContexts,
    mouse: Res<ButtonInput<MouseButton>>,
    gizmo_targets: Query<&GizmoTarget>,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut raycast: MeshRayCast,
    prim_markers: Query<&PrimMarker>,
    placement_markers: Query<&PlacementMarker>,
    parents: Query<&ChildOf>,
    terrain: Query<(), With<TerrainMesh>>,
    mut menu: ResMut<SceneContextMenu>,
) {
    let cursor_now = windows.single().ok().and_then(|w| w.cursor_position());

    // Click-vs-drag bookkeeping. The right button also drives camera orbit,
    // so a gesture only counts as a menu click if the pointer barely moved
    // between press and release.
    if mouse.just_pressed(MouseButton::Right) {
        menu.press_origin = cursor_now;
        menu.dragged = false;
    }
    if mouse.pressed(MouseButton::Right)
        && let (Some(origin), Some(now)) = (menu.press_origin, cursor_now)
        && origin.distance(now) > DRAG_THRESHOLD_PX
    {
        menu.dragged = true;
    }

    if !mouse.just_released(MouseButton::Right) {
        return;
    }
    let was_click = menu.press_origin.is_some() && !menu.dragged;
    menu.press_origin = None;
    if !was_click {
        return;
    }

    // A right-click on the toolbar or an editor window is a UI interaction,
    // not a click into the world — leave any open menu to egui's own close
    // handling and don't spawn a world menu.
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if ctx.is_pointer_over_area() {
        return;
    }
    // Never open mid-gizmo-drag (parity with the left-click picker).
    if gizmo_targets
        .iter()
        .any(|t| t.is_focused() || t.is_active())
    {
        return;
    }
    // Editing a room is owner-only, like the World Editor window and the
    // left-click picker. The menu opens the editor, so gate it here.
    let owns_room = matches!(
        (session.as_deref(), room_did.as_deref()),
        (Some(s), Some(r)) if s.did == r.0
    );
    if !owns_room {
        return;
    }

    let Some(cursor) = cursor_now else {
        return;
    };
    let Ok((camera, cam_tf)) = cameras.single() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else {
        return;
    };

    // Nearest rendered mesh under the cursor. Mesh raycast, not physics —
    // most catalogue props carry no collider (same rationale as the picker).
    let (hit_entity, hit_point) = {
        let hits = raycast.cast_ray(ray, &MeshRayCastSettings::default());
        match hits.first() {
            Some((entity, hit)) => (*entity, hit.point),
            None => {
                // Empty sky — dismiss any open menu.
                menu.open = false;
                return;
            }
        }
    };

    // Walk from the hit mesh up the hierarchy: the deepest `PrimMarker` is
    // the sub-part, the enclosing `PlacementMarker` is the placement, and any
    // `TerrainMesh` on the path marks a ground hit.
    let mut picked_prim: Option<PrimMarker> = None;
    let mut picked_placement: Option<usize> = None;
    let mut is_terrain = false;
    let mut cursor_entity = Some(hit_entity);
    while let Some(entity) = cursor_entity {
        if picked_prim.is_none()
            && let Ok(marker) = prim_markers.get(entity)
        {
            picked_prim = Some(marker.clone());
        }
        if terrain.get(entity).is_ok() {
            is_terrain = true;
        }
        if let Ok(marker) = placement_markers.get(entity) {
            picked_placement = Some(marker.0);
            break; // The anchor is the top of a placement's subtree.
        }
        cursor_entity = parents.get(entity).ok().map(ChildOf::parent);
    }

    // Only "ground or object" opens the menu. A hit on water, the sky cuboid,
    // a cloud plane or an avatar is neither — dismiss instead of placing an
    // object 2 km up on the skybox.
    if picked_prim.is_none() && picked_placement.is_none() && !is_terrain {
        menu.open = false;
        return;
    }

    menu.open = true;
    menu.anchor = cursor;
    menu.world_pos = hit_point;
    menu.placement = picked_placement;
    menu.prim = picked_prim;
}

/// Egui-pass renderer + action applier for the armed [`SceneContextMenu`].
/// Runs before `room_admin_ui` so a chosen selection/creation is reflected in
/// the same frame's editor draw (including the one-shot tree focus).
#[allow(clippy::too_many_arguments)]
pub(super) fn scene_context_menu_ui(
    mut contexts: EguiContexts,
    mut menu: ResMut<SceneContextMenu>,
    mut panels: ResMut<UiPanels>,
    mut editor: ResMut<RoomEditorState>,
    mut room: Option<ResMut<LiveRoomRecord>>,
    inventory: Option<Res<LiveInventoryRecord>>,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
) {
    if !menu.open {
        return;
    }
    // Ownership can lapse while the menu is open (portal, logout); the record
    // mutation below must never touch a room the user doesn't own. Same gate
    // as the detector, re-checked here as the security boundary.
    let owns_room = matches!(
        (session.as_deref(), room_did.as_deref()),
        (Some(s), Some(r)) if s.did == r.0
    );
    if !owns_room || room.is_none() {
        menu.open = false;
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // Copy out everything the popup reads so the only live borrow of `menu`
    // during the popup is `&mut menu.open` (via `open_bool`).
    let anchor = menu.anchor;
    let world_pos = menu.world_pos;
    let picked_prim = menu.prim.clone();
    let picked_placement = menu.placement;
    let has_object = picked_prim.is_some() || picked_placement.is_some();
    let did = session
        .as_deref()
        .map(|s| s.did.clone())
        .unwrap_or_default();

    // Shared into every (nested) menu closure; drained after the popup returns.
    // The `RefCell` sidesteps capturing `&mut` in sibling closures — the same
    // idiom the generator tree's context menus use.
    let chosen: RefCell<Option<MenuChoice>> = RefCell::new(None);

    egui::Popup::new(
        egui::Id::new("scene_context_menu"),
        ctx.clone(),
        egui::pos2(anchor.x, anchor.y),
        egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("scene_context_menu_layer"),
        ),
    )
    .kind(egui::PopupKind::Menu)
    .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
    .layout(egui::Layout::top_down_justified(egui::Align::Min))
    .open_bool(&mut menu.open)
    .show(|ui| {
        ui.set_min_width(170.0);
        if has_object {
            if picked_prim.is_some() && ui.button("Select item").clicked() {
                *chosen.borrow_mut() = Some(MenuChoice::SelectItem);
                ui.close();
            }
            if picked_placement.is_some() && ui.button("Select placement").clicked() {
                *chosen.borrow_mut() = Some(MenuChoice::SelectPlacement);
                ui.close();
            }
            ui.separator();
        }
        ui.menu_button("Create new…", |ui| {
            for kind_tag in ROOM_ROOT_KINDS {
                if ui.button(*kind_tag).clicked() {
                    *chosen.borrow_mut() = Some(MenuChoice::Create {
                        prefix: kind_tag.to_lowercase(),
                        generator: Box::new(Generator::from_kind(make_default_for_kind(kind_tag))),
                    });
                    ui.close();
                }
            }
            if !crate::catalogue::ENTRIES.is_empty() {
                ui.separator();
                ui.menu_button("From Catalogue", |ui| {
                    catalogue_menu(ui, &did, |slug, g| {
                        *chosen.borrow_mut() = Some(MenuChoice::Create {
                            prefix: slug,
                            generator: Box::new(g),
                        });
                    });
                });
            }
            if let Some(inv) = inventory.as_deref()
                && !inv.0.generators.is_empty()
            {
                ui.menu_button("From Inventory", |ui| {
                    let mut names: Vec<&String> = inv.0.generators.keys().collect();
                    names.sort();
                    for inv_name in names {
                        if ui.button(inv_name).clicked() {
                            if let Some(g) = inv.0.generators.get(inv_name) {
                                *chosen.borrow_mut() = Some(MenuChoice::Create {
                                    prefix: inv_name.clone(),
                                    generator: Box::new(g.clone()),
                                });
                            }
                            ui.close();
                        }
                    }
                });
            }
        });
    });

    let Some(choice) = chosen.into_inner() else {
        return;
    };
    menu.open = false;

    match choice {
        MenuChoice::SelectItem => {
            let Some(prim) = picked_prim else {
                return;
            };
            // Mirror the left-click picker's Generators branch: open the
            // ancestors so the picked row is visible in the collapse-by-default
            // tree, select it, and request focus so it highlights brightly.
            panels.world_editor = true;
            editor.selected_tab = EditorTab::Generators;
            editor.selected_placement = None;
            editor.selected_generator = Some(prim.generator_ref.clone());
            editor.selected_prim_path = Some(prim.path.clone());
            for depth in 0..prim.path.len() {
                editor.tree_view_state.set_openness(
                    GenNodeId::child(prim.generator_ref.clone(), prim.path[..depth].to_vec()),
                    true,
                );
            }
            editor.tree_view_state.set_selected(vec![GenNodeId::child(
                prim.generator_ref.clone(),
                prim.path.clone(),
            )]);
            editor.pending_tree_focus = true;
        }
        MenuChoice::SelectPlacement => {
            let Some(idx) = picked_placement else {
                return;
            };
            panels.world_editor = true;
            editor.selected_tab = EditorTab::Placements;
            editor.selected_generator = None;
            editor.selected_prim_path = None;
            editor.tree_view_state.set_selected(Vec::new());
            editor.selected_placement = Some(idx);
        }
        MenuChoice::Create { prefix, generator } => {
            // `room.is_none()` was rejected above, so this always resolves.
            let Some(room) = room.as_mut() else {
                return;
            };
            create_at_point(
                &prefix,
                *generator,
                world_pos,
                &mut panels,
                &mut editor,
                &mut room.0,
            );
        }
    }
}

/// Insert `generator` under a fresh unique key, anchor an `Absolute`
/// placement at `world_pos`, and land the editor on the new region asset
/// (Region Assets tab). Returns the assigned key, or `None` if the source
/// refused the insert.
///
/// Reuses the tree's exact add-root path (collision-safe unique key + insert)
/// and the same `Absolute`-placement shape as the inventory/catalogue drop, so
/// a right-click create is indistinguishable from `+ New` + a manual drop —
/// except the placement lands at the ray hit instead of the origin. Pure over
/// its inputs (no ECS world, no egui) so the create behaviour is unit-tested.
fn create_at_point(
    prefix: &str,
    generator: Generator,
    world_pos: Vec3,
    panels: &mut UiPanels,
    editor: &mut RoomEditorState,
    record: &mut RoomRecord,
) -> Option<String> {
    let key = RoomTreeSource::new(record).add_root(prefix, generator)?;
    record.placements.push(Placement::Absolute {
        generator_ref: key.clone(),
        transform: TransformData {
            translation: Fp3([world_pos.x, world_pos.y, world_pos.z]),
            rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
            scale: Fp3([1.0, 1.0, 1.0]),
        },
        avoid_water: false,
        avoid_water_clearance: Fp(0.0),
        snap_to_terrain: false,
    });
    // Land on the new region asset. It has exactly one instance, so the
    // proximity gizmo attaches to the fresh placement automatically.
    panels.world_editor = true;
    editor.selected_tab = EditorTab::Generators;
    editor.selected_placement = None;
    editor.selected_generator = Some(key.clone());
    editor.selected_prim_path = Some(Vec::new());
    editor
        .tree_view_state
        .set_one_selected(GenNodeId::root(key.clone()));
    editor.pending_tree_focus = true;
    Some(key)
}

/// Reset the menu on leaving gameplay (portal, logout) so a stale open flag
/// can't resurrect the menu in the next room.
pub(super) fn close_scene_context_menu(mut menu: ResMut<SceneContextMenu>) {
    *menu = SceneContextMenu::default();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Minimal empty room to add into. `RoomRecord` has no `Default`, but its
    /// `Environment` / `ContactEffects` sub-records do, so we only spell out
    /// the collections.
    fn empty_record() -> RoomRecord {
        RoomRecord {
            lex_type: String::new(),
            environment: Default::default(),
            generators: HashMap::new(),
            placements: Vec::new(),
            traits: HashMap::new(),
            contact_effects: Default::default(),
            default_landing: None,
        }
    }

    #[test]
    fn create_at_point_adds_asset_and_placement_at_the_hit_and_selects_it() {
        let mut record = empty_record();
        let mut editor = RoomEditorState::default();
        let mut panels = UiPanels::default();
        let hit = Vec3::new(1.5, 2.0, -3.0);

        let generator = Generator::from_kind(make_default_for_kind("Cuboid"));
        let key = create_at_point(
            "cuboid",
            generator,
            hit,
            &mut panels,
            &mut editor,
            &mut record,
        )
        .expect("add_root should assign a key");

        // Asset inserted under the returned key.
        assert!(record.generators.contains_key(&key));
        assert_eq!(record.generators.len(), 1);

        // Exactly one placement — Absolute, at the hit point, snap OFF (an
        // explicit ray hit must not be re-snapped to the terrain height).
        assert_eq!(record.placements.len(), 1);
        match &record.placements[0] {
            Placement::Absolute {
                generator_ref,
                transform,
                snap_to_terrain,
                ..
            } => {
                assert_eq!(generator_ref, &key);
                assert_eq!(transform.translation.0, [1.5, 2.0, -3.0]);
                assert!(!snap_to_terrain);
            }
            other => panic!("expected an Absolute placement, got {other:?}"),
        }

        // Editor lands on the new asset: World Editor open, Region Assets tab,
        // the new root selected (empty prim path), placement selection cleared.
        assert!(panels.world_editor);
        assert!(matches!(editor.selected_tab, EditorTab::Generators));
        assert_eq!(editor.selected_generator.as_deref(), Some(key.as_str()));
        assert_eq!(editor.selected_prim_path, Some(Vec::new()));
        assert_eq!(editor.selected_placement, None);
        assert!(editor.pending_tree_focus);
    }

    #[test]
    fn a_second_create_gets_a_distinct_key_and_its_own_placement() {
        let mut record = empty_record();
        let mut editor = RoomEditorState::default();
        let mut panels = UiPanels::default();

        let k1 = create_at_point(
            "cuboid",
            Generator::from_kind(make_default_for_kind("Cuboid")),
            Vec3::ZERO,
            &mut panels,
            &mut editor,
            &mut record,
        )
        .unwrap();
        let k2 = create_at_point(
            "cuboid",
            Generator::from_kind(make_default_for_kind("Cuboid")),
            Vec3::new(5.0, 0.0, 0.0),
            &mut panels,
            &mut editor,
            &mut record,
        )
        .unwrap();

        assert_ne!(k1, k2, "unique_key must not collide on the second create");
        assert_eq!(record.generators.len(), 2);
        assert_eq!(record.placements.len(), 2);
        // Selection follows the most recent create.
        assert_eq!(editor.selected_generator.as_deref(), Some(k2.as_str()));
    }
}
