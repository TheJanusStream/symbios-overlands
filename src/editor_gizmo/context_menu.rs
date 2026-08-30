//! In-scene right-click context menu (#720, extended by #824): fast
//! in-world workflows. Right-clicking the ground, an object, or your own
//! avatar opens a small menu offering:
//!
//! * **Select part** — (any room, #824) open the Avatar editor on the
//!   exact visuals node of your OWN avatar under the cursor.
//! * **Select item** — open the World Editor on the Region Assets tab and
//!   select the exact sub-part under the cursor (identical to the left-click
//!   picker's Generators branch, but it also *opens* the editor).
//! * **Select placement** — open the World Editor on the Placements tab and
//!   select the enclosing placement.
//! * **Duplicate item / placement** — (#824) in-place clone, selection
//!   moved to the copy (gizmo + highlight attached, ready to drag apart)
//!   — the discoverable twin of Shift-copy-drag.
//! * **Delete item / placement** — (#824) remove the sub-part or the
//!   enclosing placement; a root "Delete item" sweeps its placements like
//!   the tree's `− Delete` (confirmation treatment arrives with #838).
//! * **Create new…** — a submenu mirroring the tree's `+ New` /
//!   `+ From Catalogue` / `+ From Inventory` add-root menus. Picking one
//!   builds the region asset, appends a `Placement::Absolute` at the exact
//!   ray-hit point, and lands on the new asset in the editor — collapsing the
//!   old "make asset → make placement → drag it off the origin" sequence into
//!   one click.
//!
//! * **Worn item** (#1097) — right-clicking one of your OWN worn props:
//!   **Edit worn item** (Avatar editor, Attachments tab, that row selected
//!   with the gizmo armed), **Re-seat**, **Save to inventory**, **Take
//!   off**. Right-clicking your own rigged body: **Edit avatar** (Body
//!   tab) and **Wear from inventory…**, a submenu of your wearable stash
//!   items. Like Select part, these work in ANY room — they edit your
//!   avatar and inventory records, never the room.
//!
//! Everything except the avatar entries is owner-only; those work for
//! visitors too (they edit the visitor's own records, not the room).
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
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;
use transform_gizmo_bevy::GizmoTarget;

use crate::pds::{Fp, Fp3, Fp4, Generator, Placement, RoomRecord, TransformData};
use crate::player::RiggedRoot;
use crate::player::attachments::LocalAttachment;
use crate::state::{
    CurrentRoomDid, LiveAvatarRecord, LiveInventoryRecord, LiveRoomRecord, LocalPlayer,
};
use crate::ui::avatar::AvatarEditorState;
use crate::ui::catalogue::catalogue_menu;
use crate::ui::room::construct::{ROOM_ROOT_KINDS, make_default_for_kind};
use crate::ui::room::generators::{GeneratorTreeSource, RoomTreeSource};
use crate::ui::room::{EditorTab, GenNodeId, RoomEditorState};
use crate::ui::toolbar::UiPanels;
use crate::world_builder::{AvatarVisualPrim, PlacementMarker, PrimMarker};

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
    /// The local avatar's visuals node under the cursor, if the click
    /// landed on the player's own avatar (#824 / W1). Drives the
    /// "Select part" entry — available in ANY room, ownership is
    /// irrelevant for one's own avatar.
    avatar_prim: Option<Vec<usize>>,
    /// One of the player's own worn props under the cursor (#1097): its
    /// record key and inventory provenance. Only the local body's props
    /// carry [`LocalAttachment`], so a peer's outfit never lands here.
    worn: Option<WornHit>,
    /// The click landed on the player's own RIGGED body (#1097) — a
    /// [`RiggedRoot`] whose chassis is the [`LocalPlayer`]. Drives "Edit
    /// avatar" and "Wear from inventory…".
    own_body: bool,
}

/// A worn prop under the cursor: what the menu needs to act on it.
#[derive(Clone, Debug)]
struct WornHit {
    rkey: String,
    source: Option<String>,
    /// The part of the prop under the cursor (#1098): its path into the
    /// record's item tree, when the hit landed on a marked node.
    part: Option<Vec<usize>>,
}

/// The avatar-side hits the detector looks for (#1097), bundled to stay
/// under Bevy's 16-parameter system ceiling.
#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct AvatarHits<'w, 's> {
    worn_props: Query<'w, 's, &'static LocalAttachment>,
    part_prims: Query<'w, 's, &'static crate::world_builder::AttachmentPrim>,
    rigged_roots: Query<'w, 's, &'static ChildOf, With<RiggedRoot>>,
    local_players: Query<'w, 's, (), With<LocalPlayer>>,
}

/// The action a menu click selected, applied after the popup releases its
/// borrow of the resource. `Create` carries the fully-built generator so the
/// (borrow-checked) egui closures only ever *record* a choice.
enum MenuChoice {
    SelectItem,
    SelectPlacement,
    /// Open the Avatar editor on the clicked visuals node (#824 / W1).
    SelectAvatarPart,
    /// In-place sibling clone of the clicked sub-part — the record-level
    /// twin of Shift-copy-drag; the clone spawns coincident with the
    /// original and becomes the selection (gizmo + highlight attached),
    /// ready to drag apart.
    DuplicateItem,
    /// Clone the enclosing placement in place and select the clone.
    DuplicatePlacement,
    /// Remove the clicked sub-part from the blueprint (root delete sweeps
    /// every referencing placement — the same cascade as the tree's
    /// `− Delete`; confirmation treatment arrives with #838).
    DeleteItem,
    /// Remove the enclosing placement.
    DeletePlacement,
    /// Open the Avatar editor's Attachments tab on the clicked worn prop,
    /// gizmo armed (#1097).
    EditWorn,
    /// Open the worn prop's PARTS editor — its generator tree — on the
    /// exact part under the cursor (#1098).
    EditWornPart,
    /// Zero the worn prop's offset so the engine re-seats it.
    ReseatWorn,
    /// Write the worn prop back to the inventory (its source item, or a
    /// new one).
    SaveWornToInventory,
    /// Detach the worn prop; its record joins the publish delete queue.
    TakeOffWorn,
    /// Open the Avatar editor's Body tab (#1097).
    EditAvatar,
    /// Wear the named inventory item (#1097).
    WearFromInventory(String),
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
    mut pick: super::ScenePick,
    prim_markers: Query<&PrimMarker>,
    placement_markers: Query<&PlacementMarker>,
    avatar_prims: Query<&AvatarVisualPrim>,
    parents: Query<&ChildOf>,
    avatar_hits: AvatarHits,
    mut menu: ResMut<SceneContextMenu>,
) {
    let cursor_now = pick.cursor_position();

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
    if ctx.is_pointer_over_egui() {
        return;
    }
    // Never open mid-gizmo-drag (parity with the left-click picker).
    if gizmo_targets
        .iter()
        .any(|t| t.is_focused() || t.is_active())
    {
        return;
    }
    // Room editing is owner-only, but "Select part" on one's OWN avatar
    // is not (#824) — so ownership no longer gates the raycast, only
    // which hits may open the menu (checked after the walk below).
    let owns_room = matches!(
        (session.as_deref(), room_did.as_deref()),
        (Some(s), Some(r)) if s.did == r.0
    );

    let Some(cursor) = cursor_now else {
        return;
    };
    let Some(ray) = pick.cursor_ray() else {
        return;
    };

    // Nearest surface under the cursor: a rendered mesh by mesh raycast (most
    // catalogue props carry no collider, same rationale as the picker), or the
    // ground by a physics ray against the heightfield collider, because the
    // terrain mesh holds no main-world vertices since #1134. Merged by
    // distance, so a hill still hides what stands behind it.
    let (hit_entity, hit_point, hit_is_terrain) = {
        match pick.hit_along(ray) {
            Some(super::SceneHit::Mesh { entity, point, .. }) => (Some(entity), point, false),
            // A ground hit is the menu's "place an object here" anchor, and
            // its world point is what the placement transform is built from —
            // so it has to be the terrain ray's point, not the mesh ray's.
            Some(super::SceneHit::Terrain { point }) => (None, point, true),
            None => {
                // Empty sky — dismiss any open menu.
                menu.open = false;
                return;
            }
        }
    };

    // Walk from the hit mesh up the hierarchy: the deepest `PrimMarker` is
    // the sub-part, the enclosing `PlacementMarker` is the placement, and an
    // `AvatarVisualPrim` marks the player's own avatar (#824 — the marker
    // is only ever attached to LOCAL-player visuals). A ground hit never
    // enters this loop: it comes from the terrain ray above, which answers
    // "was this the ground" directly instead of by finding `TerrainMesh` on
    // an ancestor path.
    let mut picked_prim: Option<PrimMarker> = None;
    let mut picked_placement: Option<usize> = None;
    let mut picked_avatar: Option<Vec<usize>> = None;
    let mut picked_worn: Option<WornHit> = None;
    let mut picked_part: Option<Vec<usize>> = None;
    let mut own_body = false;
    let mut is_terrain = hit_is_terrain;
    let mut cursor_entity = hit_entity;
    while let Some(entity) = cursor_entity {
        // The deepest part marker on the path is the part under the cursor
        // (#1098); it is attached to its prop once the prop is found.
        if picked_part.is_none()
            && let Ok(part) = avatar_hits.part_prims.get(entity)
        {
            picked_part = Some(part.path.clone());
        }
        if picked_prim.is_none()
            && let Ok(marker) = prim_markers.get(entity)
        {
            picked_prim = Some(marker.clone());
        }
        if picked_avatar.is_none()
            && let Ok(marker) = avatar_prims.get(entity)
        {
            picked_avatar = Some(marker.path.clone());
        }
        // A worn prop (#1097): `LocalAttachment` sits on the prop's root,
        // between its meshes and the rig joint. Only the local body's
        // props carry it. The rigged body itself: a `RiggedRoot` whose
        // chassis is the local player — a peer's rig has no such parent.
        if picked_worn.is_none()
            && let Ok(worn) = avatar_hits.worn_props.get(entity)
        {
            picked_worn = Some(WornHit {
                rkey: worn.rkey.clone(),
                source: worn.source.clone(),
                part: picked_part.take(),
            });
        }
        if let Ok(child_of) = avatar_hits.rigged_roots.get(entity)
            && avatar_hits.local_players.contains(child_of.parent())
        {
            own_body = true;
        }
        if let Ok(marker) = placement_markers.get(entity) {
            picked_placement = Some(marker.0);
            break; // The anchor is the top of a placement's subtree.
        }
        cursor_entity = parents.get(entity).ok().map(ChildOf::parent);
    }

    // What may open the menu: the player's own avatar (any room), or —
    // owner only — ground / room objects. A hit on water, the sky cuboid,
    // a cloud plane or a REMOTE peer is none of these; dismiss instead of
    // placing an object 2 km up on the skybox. Room hits are cleared for
    // non-owners so the render pass can key every room entry off the
    // fields it received.
    if !owns_room {
        picked_prim = None;
        picked_placement = None;
        is_terrain = false;
    }
    if picked_prim.is_none()
        && picked_placement.is_none()
        && picked_avatar.is_none()
        && picked_worn.is_none()
        && !own_body
        && !is_terrain
    {
        menu.open = false;
        return;
    }

    menu.open = true;
    menu.anchor = cursor;
    menu.world_pos = hit_point;
    menu.placement = picked_placement;
    menu.prim = picked_prim;
    menu.avatar_prim = picked_avatar;
    menu.worn = picked_worn;
    menu.own_body = own_body;
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
    mut avatar_editor: ResMut<AvatarEditorState>,
    mut room: Option<ResMut<LiveRoomRecord>>,
    // Mutable for the two avatar-side writes (#1097): Save to inventory
    // and Wear from inventory. Guarded-dirty: a menu click is the only
    // deref_mut, and the stash's dirty state is derived live-vs-stored.
    mut inventory: Option<ResMut<LiveInventoryRecord>>,
    mut live_avatar: Option<ResMut<LiveAvatarRecord>>,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut undo_labels: ResMut<crate::ui::undo::PendingUndoLabels>,
    mut toasts: ResMut<crate::ui::toast::Toasts>,
    time: Res<Time>,
) {
    if !menu.open {
        return;
    }
    // Ownership can lapse while the menu is open (portal, logout); the record
    // mutations below must never touch a room the user doesn't own. Same gate
    // as the detector, re-checked here as the security boundary. "Select
    // part" targets the user's OWN avatar, so an avatar hit keeps the menu
    // alive without ownership (#824) — every room entry below additionally
    // keys on `room_available`. The worn-prop and own-body entries (#1097)
    // are avatar hits too.
    let owns_room = matches!(
        (session.as_deref(), room_did.as_deref()),
        (Some(s), Some(r)) if s.did == r.0
    );
    let room_available = owns_room && room.is_some();
    let avatar_hit = menu.avatar_prim.is_some() || menu.worn.is_some() || menu.own_body;
    if !room_available && !avatar_hit {
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
    let picked_prim = if room_available {
        menu.prim.clone()
    } else {
        None
    };
    let picked_placement = if room_available { menu.placement } else { None };
    let picked_avatar = menu.avatar_prim.clone();
    let picked_worn = menu.worn.clone();
    let own_body = menu.own_body;
    let has_object = picked_prim.is_some() || picked_placement.is_some();
    let did = session
        .as_deref()
        .map(|s| s.did.clone())
        .unwrap_or_default();
    // The wearable stash, sorted, for the "Wear from inventory…" submenu.
    let wearables: Vec<String> = inventory
        .as_deref()
        .map(|inv| {
            let mut names: Vec<String> = inv.0.wear.keys().cloned().collect();
            names.sort_by_key(|name| name.to_lowercase());
            names
        })
        .unwrap_or_default();
    let now = time.elapsed_secs_f64();

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
        // --- Worn prop (#1097) ------------------------------------------
        if let Some(worn) = &picked_worn {
            let what = worn.source.as_deref().unwrap_or("worn item");
            if ui
                .button(format!("Edit \"{what}\""))
                .on_hover_text("Open the Avatar editor on this worn item, gizmo armed")
                .clicked()
            {
                *chosen.borrow_mut() = Some(MenuChoice::EditWorn);
                ui.close();
            }
            if worn.part.is_some()
                && ui
                    .button("Edit this part")
                    .on_hover_text(
                        "Open the item's part tree on the exact part under the cursor — \
                         the region-asset editor, on your worn copy",
                    )
                    .clicked()
            {
                *chosen.borrow_mut() = Some(MenuChoice::EditWornPart);
                ui.close();
            }
            if ui
                .button("Re-seat")
                .on_hover_text("Zero its offset — the engine seats it just outside the body again")
                .clicked()
            {
                *chosen.borrow_mut() = Some(MenuChoice::ReseatWorn);
                ui.close();
            }
            let save_label = match worn.source.as_deref() {
                Some(source) => format!("Save to inventory as \"{source}\""),
                None => String::from("Save to inventory"),
            };
            if ui
                .add_enabled(inventory.is_some(), egui::Button::new(save_label))
                .on_hover_text("Write it back — geometry, socket and offset — so wearing it again looks like this")
                .clicked()
            {
                *chosen.borrow_mut() = Some(MenuChoice::SaveWornToInventory);
                ui.close();
            }
            if crate::ui::affordances::danger_menu_button(ui, "Take off").clicked() {
                *chosen.borrow_mut() = Some(MenuChoice::TakeOffWorn);
                ui.close();
            }
            ui.separator();
        }
        // --- Own rigged body (#1097) ------------------------------------
        if own_body {
            if ui
                .button("Edit avatar")
                .on_hover_text("Open the Avatar editor on your body")
                .clicked()
            {
                *chosen.borrow_mut() = Some(MenuChoice::EditAvatar);
                ui.close();
            }
            if wearables.is_empty() {
                ui.add_enabled(false, egui::Button::new("Wear from inventory…"))
                    .on_disabled_hover_text(
                        "Nothing wearable in your inventory — copy a wearable from the Catalogue",
                    );
            } else {
                ui.menu_button("Wear from inventory…", |ui| {
                    for name in &wearables {
                        if ui.button(name).clicked() {
                            *chosen.borrow_mut() = Some(MenuChoice::WearFromInventory(name.clone()));
                            ui.close();
                        }
                    }
                });
            }
            if picked_avatar.is_some() || has_object || room_available {
                ui.separator();
            }
        }
        if picked_avatar.is_some() {
            if ui
                .button("Select part")
                .on_hover_text("Open the Avatar editor on this part of your avatar")
                .clicked()
            {
                *chosen.borrow_mut() = Some(MenuChoice::SelectAvatarPart);
                ui.close();
            }
            if has_object || room_available {
                ui.separator();
            }
        }
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
            if let Some(prim) = &picked_prim {
                // A blueprint root has no sibling slot to clone into —
                // same restriction as Shift-copy-drag; duplicating the
                // PLACEMENT is the meaningful operation there.
                let can_dup = !prim.path.is_empty();
                let dup = ui.add_enabled(can_dup, egui::Button::new("Duplicate item"));
                let dup = if can_dup {
                    dup.on_hover_text(
                        "Clone this sub-part in place (edits every instance) — \
                         then drag the copy where you want it",
                    )
                } else {
                    dup.on_disabled_hover_text(
                        "A blueprint root has no sibling slot — duplicate the placement instead",
                    )
                };
                if dup.clicked() {
                    *chosen.borrow_mut() = Some(MenuChoice::DuplicateItem);
                    ui.close();
                }
            }
            if picked_placement.is_some()
                && ui
                    .button("Duplicate placement")
                    .on_hover_text(
                        "Clone this placement in place and select the copy — \
                         then drag it where you want it",
                    )
                    .clicked()
            {
                *chosen.borrow_mut() = Some(MenuChoice::DuplicatePlacement);
                ui.close();
            }
            if let Some(prim) = &picked_prim {
                let label = if prim.path.is_empty() {
                    "Delete item (and its placements)"
                } else {
                    "Delete item"
                };
                if crate::ui::affordances::danger_menu_button(ui, label).clicked() {
                    *chosen.borrow_mut() = Some(MenuChoice::DeleteItem);
                    ui.close();
                }
            }
            if picked_placement.is_some()
                && crate::ui::affordances::danger_menu_button(ui, "Delete placement").clicked()
            {
                *chosen.borrow_mut() = Some(MenuChoice::DeletePlacement);
                ui.close();
            }
            ui.separator();
        }
        if !room_available {
            return;
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
        MenuChoice::EditWorn => {
            let Some(worn) = picked_worn else {
                return;
            };
            panels.avatar = true;
            avatar_editor.select_attachment_from_scene_pick(worn.rkey);
            if editor.has_selection() {
                editor.clear_selection();
            }
        }
        MenuChoice::EditWornPart => {
            let Some(worn) = picked_worn else {
                return;
            };
            panels.avatar = true;
            avatar_editor
                .select_attachment_part_from_scene_pick(worn.rkey, worn.part.unwrap_or_default());
            if editor.has_selection() {
                editor.clear_selection();
            }
        }
        MenuChoice::ReseatWorn => {
            let (Some(worn), Some(live)) = (picked_worn, live_avatar.as_mut()) else {
                return;
            };
            if let Some(resolved) = live
                .0
                .body
                .rigged_mut()
                .and_then(|rig| rig.resolved.as_mut())
                && let Some(attachment) = resolved
                    .attachments
                    .iter_mut()
                    .find(|a| a.rkey == worn.rkey)
            {
                attachment.record.offset = TransformData::default();
                undo_labels.set_avatar("re-seat prop");
            }
        }
        MenuChoice::SaveWornToInventory => {
            let (Some(worn), Some(live), Some(inv)) =
                (picked_worn, live_avatar.as_deref(), inventory.as_mut())
            else {
                return;
            };
            let Some(record) = live
                .0
                .body
                .rigged_ref()
                .and_then(|rig| rig.resolved.as_ref())
                .and_then(|resolved| resolved.attachments.iter().find(|a| a.rkey == worn.rkey))
                .map(|a| a.record.clone())
            else {
                return;
            };
            match crate::ui::avatar::save_worn_to_inventory(&record, &mut inv.0) {
                Ok(name) => toasts.success(
                    format!("Saved as \"{name}\" — wear it again from your inventory."),
                    now,
                ),
                Err(reason) => toasts.warn(reason, now),
            }
        }
        MenuChoice::TakeOffWorn => {
            let (Some(worn), Some(live)) = (picked_worn, live_avatar.as_mut()) else {
                return;
            };
            let Some(rig) = live.0.body.rigged_mut() else {
                return;
            };
            if crate::ui::avatar::take_off_rkey(rig, &worn.rkey) {
                avatar_editor.forget_attachments([worn.rkey.clone()]);
                undo_labels.set_avatar(format!(
                    "take off {}",
                    worn.source.as_deref().unwrap_or("prop")
                ));
            }
        }
        MenuChoice::EditAvatar => {
            panels.avatar = true;
            avatar_editor.open_body_tab();
            if editor.has_selection() {
                editor.clear_selection();
            }
        }
        MenuChoice::WearFromInventory(name) => {
            let (Some(live), Some(inv)) = (live_avatar.as_mut(), inventory.as_deref()) else {
                return;
            };
            // The cap ladder this arm used to carry by hand, from the one
            // source the Inventory row and the catalogue also read
            // (#1141) — so an unresolved body now says so here too
            // instead of returning silently from `rigged_mut`.
            if let Some(reason) = crate::ui::avatar::wear_blocked_reason(Some(&live.0)) {
                toasts.warn(reason, now);
                return;
            }
            let Some(record) = crate::ui::avatar::record_for_inventory_item(&inv.0, &name) else {
                toasts.warn(format!("\"{name}\" is not wearable."), now);
                return;
            };
            if let Some(rig) = live.0.body.rigged_mut()
                && crate::ui::avatar::attach_record(rig, record, &did).is_some()
            {
                undo_labels.set_avatar(format!("wear {name}"));
            }
        }
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
        MenuChoice::SelectAvatarPart => {
            let Some(path) = picked_avatar else {
                return;
            };
            // Open the Avatar editor and select the clicked node exactly
            // like an in-world left-click pick (#823); the room selection
            // yields per the cross-editor mutex so the gizmo dispatch is
            // unambiguous.
            panels.avatar = true;
            avatar_editor.select_from_scene_pick(path);
            if editor.has_selection() {
                editor.clear_selection();
            }
        }
        MenuChoice::DuplicateItem => {
            let (Some(prim), Some(room)) = (picked_prim, room.as_mut()) else {
                return;
            };
            if let Some(new_path) = duplicate_prim(&mut room.0, &prim.generator_ref, &prim.path) {
                undo_labels.set_room(format!("duplicate of {}", prim.generator_ref));
                // Land the editor on the clone (it spawns coincident with
                // the original — the selection highlight + gizmo make it
                // grabbable despite the overlap).
                panels.world_editor = true;
                editor.selected_tab = EditorTab::Generators;
                editor.selected_placement = None;
                editor.selected_generator = Some(prim.generator_ref.clone());
                editor.selected_prim_path = Some(new_path.clone());
                for depth in 0..new_path.len() {
                    editor.tree_view_state.set_openness(
                        GenNodeId::child(prim.generator_ref.clone(), new_path[..depth].to_vec()),
                        true,
                    );
                }
                editor
                    .tree_view_state
                    .set_selected(vec![GenNodeId::child(prim.generator_ref.clone(), new_path)]);
                editor.pending_tree_focus = true;
            }
        }
        MenuChoice::DuplicatePlacement => {
            let (Some(idx), Some(room)) = (picked_placement, room.as_mut()) else {
                return;
            };
            if let Some(new_idx) = duplicate_placement(&mut room.0, idx) {
                undo_labels.set_room(format!("duplicate of placement {idx}"));
                panels.world_editor = true;
                editor.selected_tab = EditorTab::Placements;
                editor.selected_generator = None;
                editor.selected_prim_path = None;
                editor.tree_view_state.set_selected(Vec::new());
                editor.selected_placement = Some(new_idx);
            }
        }
        MenuChoice::DeleteItem => {
            let (Some(prim), Some(room)) = (picked_prim, room.as_mut()) else {
                return;
            };
            if delete_prim(&mut room.0, &prim.generator_ref, &prim.path) {
                undo_labels.set_room(format!("delete of {}", prim.generator_ref));
                // Sibling indices (and, for a root, placement indices)
                // shifted under whatever was selected — clear rather than
                // leave a stale path pointing at the wrong node.
                editor.clear_selection();
            }
        }
        MenuChoice::DeletePlacement => {
            let (Some(idx), Some(room)) = (picked_placement, room.as_mut()) else {
                return;
            };
            if delete_placement(&mut room.0, idx) {
                undo_labels.set_room(format!("delete of placement {idx}"));
                editor.clear_selection();
            }
        }
        MenuChoice::Create { prefix, generator } => {
            // `room.is_none()` was rejected above, so this always resolves.
            let Some(room) = room.as_mut() else {
                return;
            };
            if let Some(key) = create_at_point(
                &prefix,
                *generator,
                world_pos,
                &mut panels,
                &mut editor,
                &mut room.0,
            ) {
                undo_labels.set_room(format!("create of {key}"));
            }
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

/// In-place sibling clone of the node at `path` inside the named
/// generator (#824) — the record-level twin of Shift-copy-drag, keeping
/// the original's transform verbatim. Returns the clone's path. `None`
/// for a root (no sibling slot), an unknown generator, or a stale path.
fn duplicate_prim(
    record: &mut RoomRecord,
    generator_ref: &str,
    path: &[usize],
) -> Option<Vec<usize>> {
    let generator = record.generators.get_mut(generator_ref)?;
    let new_idx = super::commit::append_sibling_at_path(generator, path, None)?;
    let mut new_path = path.to_vec();
    *new_path.last_mut()? = new_idx;
    Some(new_path)
}

/// Remove the node at `path` from the named generator (#824). An empty
/// path removes the whole root through [`RoomTreeSource::remove_root`],
/// which also sweeps every referencing placement and trait — the same
/// cascade as the tree's `− Delete`. Returns `true` when the record was
/// mutated.
fn delete_prim(record: &mut RoomRecord, generator_ref: &str, path: &[usize]) -> bool {
    if path.is_empty() {
        return RoomTreeSource::new(record)
            .remove_root(generator_ref)
            .is_some();
    }
    let Some(generator) = record.generators.get_mut(generator_ref) else {
        return false;
    };
    let mut parent = generator;
    for &idx in &path[..path.len() - 1] {
        if idx >= parent.children.len() {
            return false;
        }
        parent = &mut parent.children[idx];
    }
    let child_idx = *path.last().expect("non-empty path");
    if child_idx >= parent.children.len() {
        return false;
    }
    parent.children.remove(child_idx);
    true
}

/// Clone the placement at `index` in place and append it (#824).
/// Returns the clone's index.
fn duplicate_placement(record: &mut RoomRecord, index: usize) -> Option<usize> {
    let clone = record.placements.get(index)?.clone();
    record.placements.push(clone);
    Some(record.placements.len() - 1)
}

/// Remove the placement at `index` (#824). Returns `true` when the
/// record was mutated.
fn delete_placement(record: &mut RoomRecord, index: usize) -> bool {
    if index >= record.placements.len() {
        return false;
    }
    record.placements.remove(index);
    true
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
            opaque_refs: Default::default(),
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

    /// Seed a record with one root ("thing") carrying two children, plus an
    /// Absolute placement referencing it — the fixture for the #824 verbs.
    fn record_with_children() -> RoomRecord {
        let mut record = empty_record();
        let mut editor = RoomEditorState::default();
        let mut panels = UiPanels::default();
        let mut root = Generator::from_kind(make_default_for_kind("Cuboid"));
        root.children
            .push(Generator::from_kind(make_default_for_kind("Sphere")));
        root.children
            .push(Generator::from_kind(make_default_for_kind("Cylinder")));
        create_at_point(
            "thing",
            root,
            Vec3::ZERO,
            &mut panels,
            &mut editor,
            &mut record,
        );
        record
    }

    #[test]
    fn duplicate_prim_appends_a_coincident_sibling_and_reports_its_path() {
        let mut record = record_with_children();
        let original_tf = record.generators["thing"].children[0].transform.clone();

        let new_path = duplicate_prim(&mut record, "thing", &[0]).expect("clone path");
        assert_eq!(new_path, vec![2], "clone appended after both children");
        let root = &record.generators["thing"];
        assert_eq!(root.children.len(), 3);
        // In-place: the clone keeps the original's transform verbatim.
        assert_eq!(root.children[2].transform, original_tf);

        // Roots and stale paths refuse.
        assert!(duplicate_prim(&mut record, "thing", &[]).is_none());
        assert!(duplicate_prim(&mut record, "thing", &[9]).is_none());
        assert!(duplicate_prim(&mut record, "missing", &[0]).is_none());
    }

    #[test]
    fn delete_prim_removes_children_and_root_delete_sweeps_placements() {
        let mut record = record_with_children();

        // Child delete: node gone, root + placement intact.
        assert!(delete_prim(&mut record, "thing", &[0]));
        assert_eq!(record.generators["thing"].children.len(), 1);
        assert_eq!(record.placements.len(), 1);

        // Stale path / unknown generator are no-ops.
        assert!(!delete_prim(&mut record, "thing", &[7]));
        assert!(!delete_prim(&mut record, "missing", &[0]));

        // Root delete: generator gone AND the referencing placement swept —
        // the same cascade as the tree's `− Delete`.
        assert!(delete_prim(&mut record, "thing", &[]));
        assert!(record.generators.is_empty());
        assert!(record.placements.is_empty());
    }

    #[test]
    fn placement_duplicate_and_delete_round_trip() {
        let mut record = record_with_children();
        assert_eq!(record.placements.len(), 1);

        let clone_idx = duplicate_placement(&mut record, 0).expect("clone index");
        assert_eq!(clone_idx, 1);
        assert_eq!(record.placements.len(), 2);
        assert!(duplicate_placement(&mut record, 99).is_none());

        assert!(delete_placement(&mut record, 1));
        assert_eq!(record.placements.len(), 1);
        assert!(!delete_placement(&mut record, 5));
    }
}
