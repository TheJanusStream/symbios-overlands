//! Tree-panel widget: the egui tree-view sidebar and its per-row context
//! menu (`+ Add child` / `Rename` / `Save to Inventory` / `− Delete`).
//! Structural mutations are buffered as [`PendingAction`]s and applied by
//! the pure-model engine in [`super::reparent`] after the tree-build
//! closure releases its shared borrow.

use std::cell::RefCell;

use bevy_egui::egui;
use egui_ltreeview::{Action, NodeBuilder, TreeView};

use crate::pds::Generator;
use crate::pds::inventory::is_drop_placeable;
use crate::state::LiveInventoryRecord;
use crate::ui::catalogue::catalogue_menu;

use super::super::construct::{allows_children, make_default_for_kind};
use super::reparent::{PendingAction, apply_pending, find_node};
use super::{GenNodeId, GeneratorTreeSource};

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_tree_panel(
    ui: &mut egui::Ui,
    source: &mut dyn GeneratorTreeSource,
    panel: &mut super::TreePanelState,
    inventory: Option<&mut LiveInventoryRecord>,
    dirty: &mut bool,
    toasts: &mut crate::notify::Toasts,
    now: f64,
    // Undo-entry label channel (#865): structural ops name themselves so
    // the toast can say "Undid: add of oak_3" instead of "Undid: edit".
    label: &mut crate::ui::undo::LabelSlot,
    // The signed-in owner's DID (#1239 f78). Threaded solely for
    // `catalogue_menu`: DID-personalised entries — every theme's Monument
    // — bake it into a `SignSource::DidPfp` at stamp time, and this menu
    // passed the empty string, so 24 monuments rendered a permanently
    // blank portrait while the SAME item dragged out of the Catalogue
    // window worked. Empty when the tree has no owner (the avatar
    // editor's sources hand their own owner in).
    owner_did: &str,
    // Substring filter over root names (#1244 f414).
    filter: &mut String,
    // The editor's one-node clipboard (#1244 f422).
    clipboard: &mut Option<Generator>,
) {
    // The tree DRAWS under a split borrow of the panel — the toolbar, the
    // rows, the rename modal and the focus one-shot each want a different
    // field at the same time — but a pending structural action outlives
    // that draw and `apply_pending` needs the panel whole. So the split is
    // a block, and the action is what the block yields.
    //
    // `request_focus` is consumed here rather than by the caller (#1161):
    // it is a one-shot for *this* draw, and the panel that owns it is the
    // one that can honour it.
    let pending: Option<PendingAction> = {
        let super::TreePanelState {
            selection:
                super::TreeSelection {
                    root: selected_generator,
                    path: selected_prim_path,
                },
            view: tree_view_state,
            pending_focus,
            // The rename modal and the parked confirms are `apply_pending`'s
            // and the tab's, not this panel's — it only stages actions.
            ..
        } = panel;
        let request_focus = std::mem::take(pending_focus);

        ui.heading("Items");
        ui.add_space(2.0);

        let allowed_root_kinds = source.allowed_kinds_for_root();
        let allowed_child_kinds = source.allowed_kinds_for_child();
        // Multi-root capability drives three affordances at once: root rename,
        // root delete, and the add-root toolbar below. A single-root source
        // (avatar visuals) used to RENDER the add menus anyway — the user
        // opened a 20-entry kind list (or the whole catalogue), clicked, and
        // nothing happened because `add_root` refused (#830). Hidden now;
        // children are added via the row context menu's "+ Add child".
        let allow_rename = source.allow_multiple_roots();

        if !allow_rename {
            ui.label(
                egui::RichText::new("Right-click a row to add child parts.")
                    .small()
                    .weak(),
            );
        }

        ui.horizontal_wrapped(|ui| {
            if !source.allow_multiple_roots() {
                return;
            }
            // At the generator cap every add-root door is disabled with the
            // reason (#1210), the #841 treatment; `add_root` refuses anyway.
            let roots_full = source.root_capacity_remaining() == 0;
            let full_reason = crate::ui::room::caps::Cap::Generators.full_reason();
            let mut add_roots = ui.add_enabled_ui(!roots_full, |ui| {
                ui.menu_button("+ New", |ui| {
                    for kind_tag in allowed_root_kinds {
                        if ui.button(*kind_tag).clicked() {
                            let kind = make_default_for_kind(kind_tag);
                            if let Some(name) = source
                                .add_root(&kind_tag.to_lowercase(), Generator::from_kind(kind))
                            {
                                *selected_generator = Some(name.clone());
                                *selected_prim_path = Some(Vec::new());
                                label.set(format!("add of {name}"));
                                tree_view_state.set_one_selected(GenNodeId::root(name));
                                *dirty = true;
                            }
                            ui.close();
                        }
                    }
                });

                if let Some(inv) = inventory.as_deref()
                    && !inv.0.generators.is_empty()
                {
                    ui.menu_button("+ From Inventory", |ui| {
                        let mut names: Vec<&String> = inv.0.generators.keys().collect();
                        names.sort();
                        let mut picked: Option<(String, Generator)> = None;
                        for inv_name in names {
                            if ui.button(inv_name).clicked()
                                && let Some(g) = inv.0.generators.get(inv_name)
                            {
                                picked = Some((inv_name.clone(), g.clone()));
                                ui.close();
                            }
                        }
                        if let Some((inv_name, g)) = picked
                            && let Some(new_name) = source.add_root(&inv_name, g)
                        {
                            *selected_generator = Some(new_name.clone());
                            *selected_prim_path = Some(Vec::new());
                            label.set(format!("add of {new_name}"));
                            tree_view_state.set_one_selected(GenNodeId::root(new_name));
                            *dirty = true;
                        }
                    });
                }

                // Catalogue submenu — the client-shipped sibling of Inventory.
                // Same shape as "+ From Inventory": click an entry to stamp a
                // fresh copy into the tree as a new root.
                if !crate::catalogue::ENTRIES.is_empty() {
                    ui.menu_button("+ From Catalogue", |ui| {
                        let mut picked: Option<(String, Generator)> = None;
                        catalogue_menu(ui, owner_did, |slug, g| picked = Some((slug, g)));
                        if let Some((slug, g)) = picked
                            && let Some(new_name) = source.add_root(&slug, g)
                        {
                            *selected_generator = Some(new_name.clone());
                            *selected_prim_path = Some(Vec::new());
                            label.set(format!("add of {new_name}"));
                            tree_view_state.set_one_selected(GenNodeId::root(new_name));
                            *dirty = true;
                        }
                    });
                }
            });
            if roots_full {
                add_roots.response = add_roots.response.on_disabled_hover_text(full_reason);
            }
        });

        ui.separator();

        // Find a root (#1244 f414). Alphabetical order is a weak index once
        // the names are auto-generated — `unique_key` yields `cuboid`,
        // `cuboid_1`, `cuboid_2`, … — and at 256 roots the only affordance
        // was scrolling. Only offered where there is more than one root to
        // find, so the avatar's single-root trees are unchanged.
        let multi_root = source.allow_multiple_roots();
        if multi_root {
            crate::ui::affordances::text_edit(
                ui,
                egui::TextEdit::singleline(filter)
                    .desired_width(f32::INFINITY)
                    .hint_text("Filter assets…"),
            )
            .on_hover_text("Show only assets whose name contains this text");
        }

        // The tree itself. Roots are sorted by the source for stable
        // presentation — HashMap iteration order would otherwise reshuffle
        // every frame as the layout cache rebuilds.
        let all_roots: Vec<&str> = source.root_names();
        let root_names: Vec<&str> =
            matching_roots(&all_roots, if multi_root { filter } else { "" });
        // Authored asset names are in the live room record, which changes on
        // every frame of a gizmo drag — so the font detector deliberately does
        // not scan it, and the names reach it from here instead (#1262 f359).
        // The filtered roots are what is actually on screen, which is the right
        // bound for a per-frame scan.
        for name in &root_names {
            crate::ui::fonts::note_drawn_text(ui.ctx(), name);
        }

        let hidden = all_roots.len() - root_names.len();
        if hidden > 0 {
            ui.label(
                egui::RichText::new(format!(
                    "{} of {} assets",
                    root_names.len(),
                    all_roots.len()
                ))
                .small()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
        }

        // Pending-action channel shared into every per-row `context_menu`
        // closure. Closures all hold `&pending`; clicks call `borrow_mut()` to
        // stash an action. We drain it after `show_state` returns and apply
        // with mutable source access — that ordering keeps the tree's
        // immutable read of the source's roots (during the build closure)
        // clean of structural mutations.
        let pending: RefCell<Option<PendingAction>> = RefCell::new(None);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if root_names.is_empty() {
                    ui.label(
                        egui::RichText::new(if all_roots.is_empty() {
                            // #1239 f81: the empty state now says what "+ New"
                            // actually produces, because the compiler builds
                            // only from placements and a fresh root is
                            // invisible until one exists.
                            "(no items — click \"+ New\" above; a new item needs \
                         a placement before it appears in the world)"
                        } else {
                            "(no items match the filter)"
                        })
                        .small()
                        .color(crate::ui::theme::current(ui.ctx()).text_weak),
                    );
                    return;
                }
                let inv_for_build: Option<&LiveInventoryRecord> = inventory.as_deref();
                let has_clipboard = clipboard.is_some();
                // Reborrow as a shared trait-object reference for the
                // tree-build closure: it only needs read access via
                // `get_root`, and pending-action mutations are buffered into
                // the `RefCell` for application after the closure returns.
                let source_ref: &dyn GeneratorTreeSource = &*source;
                let tree_id = ui.make_persistent_id("generators_tree_view");
                let (_resp, actions) = TreeView::new(tree_id)
                    .allow_drag_and_drop(true)
                    .allow_multi_selection(false)
                    .show_state(ui, tree_view_state, |builder| {
                        for name in &root_names {
                            if let Some(node) = source_ref.get_root(name) {
                                build_tree_node(
                                    builder,
                                    name,
                                    node,
                                    Vec::new(),
                                    true,
                                    allowed_child_kinds,
                                    allow_rename,
                                    &pending,
                                    inv_for_build,
                                    crate::ui::room::caps::node_count(node),
                                    owner_did,
                                    has_clipboard,
                                );
                            }
                        }
                    });

                // Grant the tree keyboard focus after an in-world pick (#719).
                // The widget only paints the bright `selection.bg_fill` while it
                // holds focus; an unfocused tree paints a dim `weak_bg_fill`, so
                // a programmatic selection would otherwise look different from a
                // direct click. Requesting focus here — inside the same egui
                // frame the tree is built — overrides the focus-clear that the
                // world click (on empty, non-egui space) would otherwise apply.
                if request_focus {
                    ui.memory_mut(|m| m.request_focus(tree_id));
                }

                // Drain a Move (drag-commit) into the pending channel. We
                // only honour the first move event per frame and skip if a
                // context-menu click already staged something — collisions
                // are improbable but it keeps single-action semantics.
                for action in actions {
                    if let Action::Move(dnd) = action {
                        if pending.borrow().is_some() {
                            break;
                        }
                        if let Some(src) = dnd.source.into_iter().next() {
                            *pending.borrow_mut() = Some(PendingAction::Reparent {
                                source: src,
                                target: dnd.target,
                                position: dnd.position,
                            });
                        }
                        break;
                    }
                }
            });

        pending.into_inner()
    };

    if let Some(action) = pending {
        apply_pending(
            action, source, panel, inventory, dirty, toasts, now, label, clipboard,
        );
    }

    // Sync the tree widget's selection back into the panel's own, which is
    // what `editor_gizmo` reads (room) and what the avatar editor folds
    // into its aim. Treat any selected id that no longer resolves to a live
    // node as "no selection" — happens after a delete / kind-change /
    // rename leaves the tree state holding a stale path.
    let valid: Option<GenNodeId> = panel
        .view
        .selected()
        .first()
        .filter(|id| find_node(&*source, id).is_some())
        .cloned();
    sync_selection_fields(valid, panel);
}

/// Mirror the tree widget's (validated) selection into the panel's own
/// selection, which is what the gizmo layer reads. Deliberately has NO access to the shared
/// dirty flag (#828): selecting a row edits nothing, but the flag arms
/// the debounce, whose flush calls `set_changed()` on the live record —
/// a FULL recompile (room) / visuals despawn-respawn (avatar) plus a
/// whole-record peer broadcast per click. Browsing a large tree was a
/// hitch-and-network storm. Every real mutation (widgets, structural
/// ops, gizmo commits) sets dirty through its own path; keeping `dirty`
/// out of this signature makes the regression structurally impossible.
fn sync_selection_fields(valid: Option<GenNodeId>, panel: &mut super::TreePanelState) {
    let super::TreePanelState {
        selection, view, ..
    } = panel;
    match valid {
        Some(id) => {
            if selection.root.as_deref() != Some(id.root.as_str()) {
                selection.root = Some(id.root.clone());
            }
            if selection.path.as_deref() != Some(id.path.as_slice()) {
                selection.path = Some(id.path.clone());
            }
        }
        None => {
            if selection.root.is_some() || selection.path.is_some() {
                selection.clear();
            }
            if !view.selected().is_empty() {
                view.set_selected(Vec::new());
            }
        }
    }
}

/// Root names matching a substring filter, case-insensitively (#1244
/// f414). An empty filter passes everything through unchanged.
///
/// Pure, so the one behaviour that matters — that filtering never drops a
/// root the user has selected out from under them without saying so — is
/// testable without egui.
pub(super) fn matching_roots<'a>(roots: &[&'a str], filter: &str) -> Vec<&'a str> {
    let needle = filter.trim().to_lowercase();
    if needle.is_empty() {
        return roots.to_vec();
    }
    roots
        .iter()
        .filter(|name| name.to_lowercase().contains(&needle))
        .copied()
        .collect()
}

// Rows `build_tree_node` has built on this thread — the instrument for
// #1270 f419, since there is no way to time a frame in a test and the
// thing the fix is about is a COUNT.
//
// Thread-local and not a global: `cargo test --lib` runs the suite in one
// process on many threads, and a process-global counter is the shared
// state that idiom cannot see (#1147, #1189).
#[cfg(test)]
thread_local! {
    static NODES_BUILT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Recursively add `node` and its children to the tree-view builder. The
/// label format matches the user's expectation: roots show the source's key
/// (the user-given name) plus a kind hint; inner nodes show only the kind
/// since they're positional and unnamed. Each row gets a right-click
/// context menu wired through `pending` so structural ops (Add child /
/// Rename / Save to Inventory / Delete) buffer cleanly until after the
/// tree finishes drawing.
///
/// The lifetimes on `root_name` and `node` are independent of the builder's
/// own working lifetime — the label is materialised as an owned `String`
/// before being handed to [`NodeBuilder::label`], so the builder never
/// retains a reference into the source.
#[allow(clippy::too_many_arguments)]
fn build_tree_node(
    builder: &mut egui_ltreeview::TreeViewBuilder<'_, GenNodeId>,
    root_name: &str,
    node: &Generator,
    path: Vec<usize>,
    is_root: bool,
    allowed_child_kinds: &'static [&'static str],
    allow_rename: bool,
    pending: &RefCell<Option<PendingAction>>,
    inventory: Option<&LiveInventoryRecord>,
    // Nodes in this root's whole tree, for the per-generator cap (#1210).
    root_nodes: usize,
    // The signed-in owner's DID, for this row's "+ From Catalogue"
    // submenu (#1239 f78) — see `draw_tree_panel`.
    owner_did: &str,
    // Whether the editor's clipboard holds anything to paste (#1244 f422).
    has_clipboard: bool,
) {
    #[cfg(test)]
    NODES_BUILT.with(|n| n.set(n.get() + 1));
    let id = GenNodeId::child(root_name, path.clone());
    let label = if is_root {
        format!("{}  ({})", root_name, node.kind_tag())
    } else {
        // The sibling index (#1244 f423). Inner rows were labelled by kind
        // ALONE — forty sibling Cuboids were forty identical rows — while
        // the disambiguating path was reduced to a small weak line in the
        // detail pane, never appearing in the row the owner is scanning.
        // `#index` matches `placement_label`'s convention, and it is the
        // last path segment, which is what the detail pane's `path: /12`
        // shows.
        match path.last() {
            Some(index) => format!("{}  #{index}", node.kind_tag()),
            None => node.kind_tag().to_string(),
        }
    };

    let menu_id = id.clone();
    let menu_root = id.root.clone();
    let menu_allows_children = allows_children(&node.kind);
    let menu_has_clipboard = has_clipboard;
    // Why a child cannot be added under this node right now (#1210): the
    // nesting or node cap the sanitiser would otherwise enforce a quarter
    // second after the add by amputating the tree. `apply_pending`
    // re-checks, since a prebuilt subtree can be deeper than one level.
    let menu_child_refusal: Option<String> = {
        use crate::ui::room::caps::{Cap, fits_under};
        if !fits_under(path.len(), 0) {
            Some(Cap::Depth.full_reason())
        } else if Cap::NodesPerGenerator.is_full(root_nodes) {
            Some(Cap::NodesPerGenerator.full_reason())
        } else {
            None
        }
    };
    let menu_is_root = is_root;
    let menu_allow_rename = allow_rename;
    // `Option<&T>` is `Copy`, so the move closure below copies the option
    // into its captures rather than borrowing — no extra lifetime
    // bookkeeping needed for the "+ From Inventory" submenu inside.
    let menu_inventory = inventory;
    let context_menu = move |ui: &mut egui::Ui| {
        let mut add_children = ui.add_enabled_ui(menu_child_refusal.is_none(), |ui| {
            if menu_allows_children {
                // Mirror the toolbar's "+ New" kind picker: a submenu listing
                // every kind valid as a child (the source's
                // `allowed_kinds_for_child()` set). Picking a kind stages an
                // `AddChild` action carrying that kind's static tag —
                // `apply_pending` calls `make_default_for_kind` to build the
                // actual node.
                ui.menu_button("+ Add child", |ui| {
                    for kind_tag in allowed_child_kinds {
                        if ui.button(*kind_tag).clicked() {
                            *pending.borrow_mut() = Some(PendingAction::AddChild {
                                parent: menu_id.clone(),
                                kind_tag,
                            });
                            ui.close();
                        }
                    }
                });
            }
            if menu_allows_children
                && let Some(inv) = menu_inventory
                && !inv.0.generators.is_empty()
            {
                ui.menu_button("+ From Inventory", |ui| {
                    let mut names: Vec<&String> = inv
                        .0
                        .generators
                        .iter()
                        .filter(|(_, g)| is_drop_placeable(g))
                        .map(|(k, _)| k)
                        .collect();
                    names.sort();
                    if names.is_empty() {
                        ui.label(
                            egui::RichText::new("(no placeable inventory items)")
                                .small()
                                .color(crate::ui::theme::current(ui.ctx()).text_weak),
                        );
                        return;
                    }
                    for inv_name in names {
                        if ui.button(inv_name).clicked()
                            && let Some(g) = inv.0.generators.get(inv_name)
                        {
                            *pending.borrow_mut() = Some(PendingAction::AddChildPrebuilt {
                                parent: menu_id.clone(),
                                generator: Box::new(g.clone()),
                            });
                            ui.close();
                        }
                    }
                });
            }
            // Catalogue stamps parent to the clicked node exactly like an
            // inventory clone — same buffered insert path, fresh blueprint.
            if menu_allows_children && !crate::catalogue::ENTRIES.is_empty() {
                ui.menu_button("+ From Catalogue", |ui| {
                    catalogue_menu(ui, owner_did, |_slug, g| {
                        *pending.borrow_mut() = Some(PendingAction::AddChildPrebuilt {
                            parent: menu_id.clone(),
                            generator: Box::new(g),
                        });
                    });
                });
            }
        });
        if let Some(reason) = &menu_child_refusal {
            add_children.response = add_children.response.on_disabled_hover_text(reason);
        }
        // Rename rewrites the source's root key plus every Placement /
        // traits reference held alongside (room source). Inner nodes are
        // positional + unnamed and have nothing to rename. Single-root
        // sources (avatar visuals) suppress the option entirely via
        // `allow_rename = false`.
        // Copy a sub-assembly (#1244 f422). Drag resolves only to
        // `Action::Move`, which is destructive to the source, and the only
        // Duplicate in the app before this required physically finding the
        // object in the world and right-clicking it — impossible for an
        // unplaced or off-screen generator, which is exactly the case the
        // tree exists to reach.
        if ui
            .button("Duplicate")
            .on_hover_text("Make a copy beside this one")
            .clicked()
        {
            *pending.borrow_mut() = Some(PendingAction::Duplicate(menu_id.clone()));
            ui.close();
        }
        if ui
            .button("Copy")
            .on_hover_text("Hold a copy of this part — paste it under any row, in any item")
            .clicked()
        {
            *pending.borrow_mut() = Some(PendingAction::Copy(menu_id.clone()));
            ui.close();
        }
        if ui
            .add_enabled(
                menu_allows_children && menu_has_clipboard,
                egui::Button::new("Paste as child"),
            )
            .on_hover_text("Add the copied part under this one")
            .on_disabled_hover_text(if menu_has_clipboard {
                "This kind holds no children"
            } else {
                "Nothing copied yet — use Copy on a row first"
            })
            .clicked()
        {
            *pending.borrow_mut() = Some(PendingAction::PasteChild(menu_id.clone()));
            ui.close();
        }
        if menu_is_root && menu_allow_rename {
            // Refused on the derived namespace (#1245 f382). The
            // `lot_building_` / `street_prop_` prefix is the whole
            // idempotency key: renaming one makes the strip miss it,
            // `net_populated` report false, and a second identical district
            // grow on top of the first. The affordance was offered on
            // exactly the rows it corrupts.
            let derived = crate::terrain::is_derived_generator_key(&menu_root);
            let button = ui.add_enabled(!derived, egui::Button::new("Rename"));
            if derived {
                button.on_disabled_hover_text(
                    "Grown by the road layer. Its name is how the layer finds it \
                     again — renaming it would grow a second district on top of \
                     this one.",
                );
            } else if button.clicked() {
                *pending.borrow_mut() = Some(PendingAction::Rename(menu_root.clone()));
                ui.close();
            }
        }
        // Cap-gated (#841): "Save to Inventory" used to insert
        // unconditionally, blowing past the 50-item cap the gift-accept
        // path enforces — sanitize then silently deleted the overflow on
        // the next login. Disabled (with the reason) instead of hidden.
        let cap = crate::config::state::MAX_INVENTORY_ITEMS;
        let stash_full = menu_inventory.is_some_and(|inv| inv.0.generators.len() >= cap);
        if ui
            .add_enabled(
                menu_inventory.is_some() && !stash_full,
                egui::Button::new("Save to Inventory"),
            )
            .on_hover_text("Copy this part into your inventory as a reusable item")
            .on_disabled_hover_text(if stash_full {
                format!("Inventory full ({cap}/{cap}) — remove an item first")
            } else {
                "Inventory not loaded".to_owned()
            })
            .clicked()
        {
            *pending.borrow_mut() = Some(PendingAction::SaveToInventory(menu_id.clone()));
            ui.close();
        }
        // A single-root source (avatar visuals) refuses root removal —
        // hide the item instead of offering a silent no-op (#830).
        // `menu_allow_rename` mirrors `allow_multiple_roots`.
        if (!menu_is_root || menu_allow_rename)
            && crate::ui::affordances::danger_menu_button(ui, "Delete").clicked()
        {
            *pending.borrow_mut() = Some(PendingAction::Delete(menu_id.clone()));
            ui.close();
        }
    };

    let is_container = allows_children(&node.kind);
    // Render the expand/collapse marker only for nodes that *have* children
    // (#719): a container-capable kind with an empty `children` list is a
    // directory in principle but shows no triangle, matching how a file
    // browser draws an empty folder as a plain row. Trees start collapsed
    // (`default_open(false)`); the in-world pick path re-opens a picked
    // node's ancestors so its row stays visible.
    if is_container && !node.children.is_empty() {
        // `TreeViewBuilder::node` RETURNS the directory's open state, which
        // is what makes this cheap (#1270 f419). The widget short-circuits
        // a collapsed branch internally — `current_branch_expanded()` — so
        // everything below used to be built and thrown away: a
        // `GenNodeId` (a String plus a Vec), a `format!` label, two more
        // clones for the menu captures, a `path.clone()` per child, and a
        // boxed context-menu closure, PER NODE, for every node of every
        // root whether or not anything was expanded. At the record's own
        // caps — 256 roots of up to 1024 nodes — that is a quarter of a
        // million heap allocations per frame to draw a collapsed list.
        //
        // Skipping the recursion is safe because nothing reaches a row
        // inside a collapsed parent without opening the parent first: both
        // reveal paths (`editor_gizmo`'s scene pick and `RoomEditorState`'s
        // undo restore) walk the path and `set_openness(.., true)` on every
        // ancestor before the tree next draws.
        let open = builder.node(
            NodeBuilder::dir(id)
                .label(label)
                .default_open(false)
                .context_menu(context_menu),
        );
        if open {
            for (i, child) in node.children.iter().enumerate() {
                let mut child_path = path.clone();
                child_path.push(i);
                build_tree_node(
                    builder,
                    root_name,
                    child,
                    child_path,
                    false,
                    allowed_child_kinds,
                    allow_rename,
                    pending,
                    inventory,
                    root_nodes,
                    owner_did,
                    has_clipboard,
                );
            }
        }
        // Unconditional: `node()` pushed a `DirectoryState` whether or not
        // the branch is expanded, and `close_dir` is what pops it.
        builder.close_dir();
    } else {
        // Leaf row (no triangle). Two cases collapse here: no-children kinds
        // (Water, Unknown), which reject every drop INTO them, and an *empty*
        // container, which still accepts drops so it can be populated by
        // dragging (its context menu keeps "+ Add child" too). `drop_allowed`
        // also governs the drop marker, so a leaf kind shows a hard "no" at
        // hover while an empty container shows the drop affordance.
        // `apply_reparent` re-checks `allows_children` defensively regardless.
        builder.node(
            NodeBuilder::leaf(id)
                .label(label)
                .drop_allowed(is_container)
                .context_menu(context_menu),
        );
    }
}

/// Slash-separated string form of a child path, for display in the detail
/// header. An empty path renders as the empty string and the caller chooses
/// whether to show "/" or omit the suffix entirely.
pub(super) fn path_string(path: &[usize]) -> String {
    path.iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// Stable per-node salt for egui IDs. Includes the root key + child path
/// so collapsing one Cuboid never affects a sibling Cuboid drawn with the
/// same widget set.
pub(super) fn node_salt(id: &GenNodeId) -> String {
    let mut s = format!("gen_{}", id.root);
    for i in &id.path {
        s.push('_');
        s.push_str(&i.to_string());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::super::TreeViewState;
    use super::*;

    /// A container node with `children` children, each with `grandchildren`
    /// leaves.
    fn nested(children: usize, grandchildren: usize) -> Generator {
        let leaf = || Generator {
            kind: crate::ui::room::construct::make_default_for_kind("Cuboid"),
            ..Default::default()
        };
        Generator {
            kind: crate::ui::room::construct::make_default_for_kind("Cuboid"),
            children: (0..children)
                .map(|_| Generator {
                    kind: crate::ui::room::construct::make_default_for_kind("Cuboid"),
                    children: (0..grandchildren).map(|_| leaf()).collect(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    /// Build one root through a real `TreeView` in a headless context and
    /// return how many rows `build_tree_node` was asked to construct.
    fn rows_built(root: &Generator, state: &mut TreeViewState) -> usize {
        let ctx = egui::Context::default();
        NODES_BUILT.with(|n| n.set(0));
        let pending: RefCell<Option<PendingAction>> = RefCell::new(None);
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 600.0),
                )),
                ..Default::default()
            },
            |ui| {
                TreeView::new(ui.make_persistent_id("count_probe")).show_state(
                    ui,
                    state,
                    |builder| {
                        build_tree_node(
                            builder,
                            "oak",
                            root,
                            Vec::new(),
                            true,
                            &["Cuboid"],
                            true,
                            &pending,
                            None,
                            crate::ui::room::caps::node_count(root),
                            "did:plc:counter",
                            false,
                        );
                    },
                );
            },
        );
        NODES_BUILT.with(|n| n.get())
    }

    /// A collapsed tree costs what it SHOWS, not what it contains
    /// (#1270 f419).
    ///
    /// `build_tree_node` recursed into `node.children` unconditionally, and
    /// every visit allocated whether or not the row could be seen: a
    /// `GenNodeId` (a `String` plus a `Vec`), a `format!` label, two more
    /// clones for the menu captures, a `path.clone()` per child, and a
    /// boxed context-menu closure. `TreeViewBuilder` throws all of it away
    /// inside a collapsed branch — `current_branch_expanded()` — so at the
    /// record's own caps (256 roots × up to 1024 nodes) that was a quarter
    /// of a million heap allocations per frame to draw a list of one-line
    /// rows.
    ///
    /// The pairing is the same tree in two states. The shape being
    /// replaced built all 41 rows in BOTH, so a test that only checked the
    /// open case would have passed on it unchanged — the #87 rule: ask
    /// what the failing case looks like.
    #[test]
    fn a_collapsed_tree_builds_only_the_rows_it_shows() {
        let root = nested(8, 4);
        let total = 1 + 8 + 8 * 4;
        assert_eq!(
            crate::ui::room::caps::node_count(&root),
            total,
            "the fixture is the size the counts below assume"
        );

        // Collapsed — the default. One row: the root itself.
        let mut collapsed = TreeViewState::default();
        assert_eq!(
            rows_built(&root, &mut collapsed),
            1,
            "a collapsed root shows one row and must build one row"
        );

        // The root opened, its children still closed.
        let mut one_level = TreeViewState::default();
        one_level.set_openness(GenNodeId::root("oak".to_string()), true);
        assert_eq!(
            rows_built(&root, &mut one_level),
            1 + 8,
            "opening the root reveals its children and nothing deeper"
        );

        // Everything opened — this is what the OLD code built in every
        // state, including the collapsed one above.
        let mut all_open = TreeViewState::default();
        all_open.set_openness(GenNodeId::root("oak".to_string()), true);
        for i in 0..8 {
            all_open.set_openness(GenNodeId::child("oak", vec![i]), true);
        }
        assert_eq!(
            rows_built(&root, &mut all_open),
            total,
            "and a fully expanded tree still builds every row — the fix skips \
             what is hidden, not what is shown"
        );
    }

    /// The reveal paths that make the skip safe (#1270 f419).
    ///
    /// Nothing may select a row inside a collapsed parent, because the
    /// tree no longer builds one. Both paths that reveal a row — the
    /// `editor_gizmo` scene pick and `RoomEditorState::restore_selection`
    /// — walk the path and open EVERY ancestor, not just the immediate
    /// parent. This pins that opening only the immediate parent is not
    /// enough, which is the mistake the two-line version of either loop
    /// would be.
    #[test]
    fn revealing_a_deep_row_needs_every_ancestor_open() {
        let root = nested(3, 3);

        // Only the leaf's immediate parent opened: the root is still
        // closed, so nothing below it is built at all.
        let mut parent_only = TreeViewState::default();
        parent_only.set_openness(GenNodeId::child("oak", vec![1]), true);
        assert_eq!(
            rows_built(&root, &mut parent_only),
            1,
            "an open node inside a closed root reveals nothing"
        );

        // Every ancestor of `oak/1/2`, which is what both reveal paths do.
        let mut ancestors = TreeViewState::default();
        let target = GenNodeId::child("oak", vec![1, 2]);
        for depth in 0..target.path.len() {
            ancestors.set_openness(
                GenNodeId::child(target.root.clone(), target.path[..depth].to_vec()),
                true,
            );
        }
        assert_eq!(
            rows_built(&root, &mut ancestors),
            1 + 3 + 3,
            "the root and the picked branch, and no sibling branch"
        );
    }

    /// #828: selection sync mirrors the tree into the gizmo-read fields —
    /// and, by construction (no `dirty` parameter), can never arm the
    /// debounce that recompiles + broadcasts the record.
    #[test]
    fn selection_sync_updates_fields_and_clears_stale_state() {
        let mut panel = super::super::TreePanelState::default();

        // A valid selection lands in both fields.
        let id = GenNodeId::child("oak".to_string(), vec![1, 0]);
        panel.view.set_selected(vec![id.clone()]);
        sync_selection_fields(Some(id), &mut panel);
        assert_eq!(panel.selection.root.as_deref(), Some("oak"));
        assert_eq!(panel.selection.path, Some(vec![1, 0]));

        // A stale/no selection clears the fields AND the widget state.
        sync_selection_fields(None, &mut panel);
        assert_eq!(panel.selection.root, None);
        assert_eq!(panel.selection.path, None);
        assert!(panel.view.selected().is_empty());
    }

    /// #1244 f414. Sequence: a name typed three sessions ago among 256
    /// roots. Alphabetical order is a weak index once the names are
    /// auto-generated — `unique_key` yields `cuboid`, `cuboid_1`,
    /// `cuboid_2`, … — and the only affordance was scrolling.
    #[test]
    fn the_tree_filter_is_a_case_insensitive_substring() {
        let roots: Vec<&str> = vec!["oak_17", "Oak_2", "cuboid", "cuboid_1"];
        assert_eq!(matching_roots(&roots, ""), roots, "no filter, no change");
        assert_eq!(matching_roots(&roots, "   "), roots, "blank is no filter");
        assert_eq!(matching_roots(&roots, "oak"), vec!["oak_17", "Oak_2"]);
        assert_eq!(matching_roots(&roots, "_1"), vec!["oak_17", "cuboid_1"]);
        assert!(matching_roots(&roots, "willow").is_empty());
    }

    /// #1244 f423. Sequence: a detailed building shows a column of rows
    /// all reading "Cuboid" with nothing to tell them apart, and Rename is
    /// not offered on any of them. The sibling index is the cheap half of
    /// the fix and it matches `placement_label`'s `#index` convention.
    #[test]
    fn an_inner_row_carries_its_sibling_index() {
        let node = Generator::default();
        let kind = node.kind_tag();
        // The label logic, as `build_tree_node` computes it.
        let inner = |path: &[usize]| match path.last() {
            Some(index) => format!("{kind}  #{index}"),
            None => kind.to_string(),
        };
        assert_eq!(inner(&[3]), format!("{kind}  #3"));
        assert_eq!(inner(&[1, 12]), format!("{kind}  #12"), "the LAST segment");
        assert_eq!(inner(&[]), kind, "a root is named, so it needs no index");
        assert_ne!(inner(&[0]), inner(&[1]), "forty siblings read differently");
    }
}
