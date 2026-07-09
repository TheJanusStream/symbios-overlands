//! Catalogue browser window — the client-shipped sibling of the
//! Inventory window. A master-detail browser over [`crate::catalogue::ENTRIES`]:
//! a hierarchical / sortable tree on the left (search + view-mode selector
//! above it) and a metadata detail panel on the right. Selecting an entry
//! shows its description, themes, role, socio bands and footprint, and a
//! drag-to-place handle that stamps a fresh copy into the active room on
//! viewport release.
//!
//! Drag mechanics mirror [`crate::ui::inventory::inventory_ui`] — the drag
//! source is [`DropSource::Catalogue`], which makes
//! [`handle_generator_drop`](crate::ui::inventory::handle_generator_drop)
//! resolve the dragged slug against the catalogue registry. Browsing works
//! in any room; the place handle only arms when the active room belongs to
//! the signed-in user.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;
use egui_ltreeview::{Action, TreeView};

use crate::catalogue::{CatalogueCategory, CatalogueEntry, ENTRIES, StructureRole, by_slug};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;
use crate::state::CurrentRoomDid;
use crate::ui::inventory::{DropSource, PendingGeneratorDrop, is_drop_placeable};

/// How the browser groups / orders entries. The hierarchy is the default;
/// the others are alternate flat or single-axis groupings.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum BrowseMode {
    /// Category → Theme → Role tree (Buildings nest by theme then role;
    /// Plants / Patterns / Tools list directly under their category).
    #[default]
    Hierarchy,
    /// Grouped by theme (cross-theme props collapsed into one group).
    Theme,
    /// Grouped by structural role.
    Role,
    /// Flat, alphabetical by name.
    Name,
    /// Flat, largest footprint clearance first.
    Footprint,
}

impl BrowseMode {
    const ALL: [Self; 5] = [
        Self::Hierarchy,
        Self::Theme,
        Self::Role,
        Self::Name,
        Self::Footprint,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Hierarchy => "Hierarchy",
            Self::Theme => "By theme",
            Self::Role => "By role",
            Self::Name => "By name",
            Self::Footprint => "By size",
        }
    }
}

/// Persistent browser UI state. The tree's open/closed + selection state
/// lives in egui memory (keyed by the tree's id); this only carries the
/// search string, the view mode, and the slug whose details are shown.
#[derive(Resource, Default)]
pub struct CatalogueBrowser {
    search: String,
    mode: BrowseMode,
    /// Slug of the entry shown in the detail panel.
    selected: Option<String>,
}

// ---------------------------------------------------------------------------
// Pure grouping / filtering (unit-tested without egui)
// ---------------------------------------------------------------------------

/// A node in the browse tree: a directory (group) or an entry leaf. Leaf
/// ids are `"<dir-path>#<slug>"` so an entry that appears under several
/// groups (e.g. a multi-theme entry) gets a unique id per occurrence while
/// the trailing slug stays recoverable via [`leaf_slug`].
pub(crate) enum CatNode {
    Dir {
        id: String,
        label: String,
        count: usize,
        children: Vec<CatNode>,
    },
    Leaf {
        id: String,
        name: &'static str,
    },
}

/// Recover the entry slug from a leaf node id (the segment after the last
/// `#`). `None` for directory ids, which carry no `#`.
fn leaf_slug(id: &str) -> Option<&str> {
    id.rsplit_once('#').map(|(_, slug)| slug)
}

/// Whether `e` matches the (already lower-cased) search query. Empty query
/// matches everything; otherwise a substring of the name, slug, or any
/// theme label.
fn matches(e: &dyn CatalogueEntry, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    e.name().to_lowercase().contains(q)
        || e.slug().contains(q)
        || e.themes()
            .iter()
            .any(|t| t.label().to_lowercase().contains(q))
}

/// An entry tagged with *every* theme is a cross-theme prop (the civic
/// kit); it is filed under one "Cross-theme" group rather than duplicated
/// under all 23 themes.
fn is_cross_theme(e: &dyn CatalogueEntry) -> bool {
    e.themes().len() >= ThemeArchetype::ALL.len()
}

fn leaf(parent: &str, e: &'static dyn CatalogueEntry) -> CatNode {
    CatNode::Leaf {
        id: format!("{parent}#{}", e.slug()),
        name: e.name(),
    }
}

fn count_leaves(node: &CatNode) -> usize {
    match node {
        CatNode::Leaf { .. } => 1,
        CatNode::Dir { children, .. } => children.iter().map(count_leaves).sum(),
    }
}

fn mk_dir(id: String, label: String, children: Vec<CatNode>) -> CatNode {
    let count = children.iter().map(count_leaves).sum();
    CatNode::Dir {
        id,
        label,
        count,
        children,
    }
}

/// Build the browse tree for `mode`, filtered by `query`. Empty groups are
/// dropped, so a search that matches nothing under a group hides it.
fn build_nodes(mode: BrowseMode, query: &str) -> Vec<CatNode> {
    let q = query.trim().to_lowercase();
    let entries: Vec<&'static dyn CatalogueEntry> = ENTRIES
        .iter()
        .copied()
        .filter(|e| matches(*e, &q))
        .collect();
    match mode {
        BrowseMode::Hierarchy => hierarchy(&entries),
        BrowseMode::Theme => by_theme(&entries),
        BrowseMode::Role => by_role(&entries),
        BrowseMode::Name => {
            let mut es = entries;
            es.sort_by_key(|e| e.name());
            es.iter().map(|e| leaf("", *e)).collect()
        }
        BrowseMode::Footprint => {
            let mut es = entries;
            es.sort_by(|a, b| {
                b.footprint()
                    .clearance
                    .total_cmp(&a.footprint().clearance)
                    .then(a.name().cmp(b.name()))
            });
            es.iter().map(|e| leaf("", *e)).collect()
        }
    }
}

fn hierarchy(entries: &[&'static dyn CatalogueEntry]) -> Vec<CatNode> {
    let mut cats = Vec::new();
    for cat in CatalogueCategory::ALL {
        let in_cat: Vec<&'static dyn CatalogueEntry> = entries
            .iter()
            .copied()
            .filter(|e| e.category() == cat)
            .collect();
        if in_cat.is_empty() {
            continue;
        }
        let cat_id = format!("/{}", cat.label());
        let children = if cat == CatalogueCategory::Buildings {
            building_theme_dirs(&in_cat, &cat_id)
        } else {
            let mut es = in_cat.clone();
            es.sort_by_key(|e| e.name());
            es.iter().map(|e| leaf(&cat_id, *e)).collect()
        };
        cats.push(mk_dir(cat_id, cat.label().to_string(), children));
    }
    cats
}

/// Theme dirs (each split into Landmark / Secondary / Prop role dirs) for
/// the Buildings category, plus a leading "Cross-theme" group for the
/// all-theme civic props.
fn building_theme_dirs(in_cat: &[&'static dyn CatalogueEntry], cat_id: &str) -> Vec<CatNode> {
    const ROLES: [StructureRole; 3] = [
        StructureRole::Landmark,
        StructureRole::Secondary,
        StructureRole::Prop,
    ];
    let role_dirs = |scope: &[&'static dyn CatalogueEntry], parent: &str| -> Vec<CatNode> {
        ROLES
            .iter()
            .filter_map(|&role| {
                let in_role: Vec<&'static dyn CatalogueEntry> =
                    scope.iter().copied().filter(|e| e.role() == role).collect();
                if in_role.is_empty() {
                    return None;
                }
                let role_id = format!("{parent}/{}", role.label());
                let leaves = in_role.iter().map(|e| leaf(&role_id, *e)).collect();
                Some(mk_dir(role_id, role.label().to_string(), leaves))
            })
            .collect()
    };

    let mut dirs = Vec::new();
    // Cross-theme props first.
    let cross: Vec<&'static dyn CatalogueEntry> = in_cat
        .iter()
        .copied()
        .filter(|e| is_cross_theme(*e))
        .collect();
    if !cross.is_empty() {
        let id = format!("{cat_id}/Cross-theme");
        let children = role_dirs(&cross, &id);
        dirs.push(mk_dir(id, "Cross-theme".to_string(), children));
    }
    // Then one dir per theme for the theme-specific entries.
    for theme in ThemeArchetype::ALL {
        let in_theme: Vec<&'static dyn CatalogueEntry> = in_cat
            .iter()
            .copied()
            .filter(|e| !is_cross_theme(*e) && e.themes().contains(&theme))
            .collect();
        if in_theme.is_empty() {
            continue;
        }
        let id = format!("{cat_id}/{}", theme.label());
        let children = role_dirs(&in_theme, &id);
        dirs.push(mk_dir(id, theme.label().to_string(), children));
    }
    dirs
}

fn by_theme(entries: &[&'static dyn CatalogueEntry]) -> Vec<CatNode> {
    let mut dirs = Vec::new();
    let cross: Vec<&'static dyn CatalogueEntry> = entries
        .iter()
        .copied()
        .filter(|e| is_cross_theme(*e))
        .collect();
    if !cross.is_empty() {
        let id = "t/Cross-theme".to_string();
        let leaves = cross.iter().map(|e| leaf(&id, *e)).collect();
        dirs.push(mk_dir(id, "Cross-theme".to_string(), leaves));
    }
    for theme in ThemeArchetype::ALL {
        let in_theme: Vec<&'static dyn CatalogueEntry> = entries
            .iter()
            .copied()
            .filter(|e| !is_cross_theme(*e) && e.themes().contains(&theme))
            .collect();
        if in_theme.is_empty() {
            continue;
        }
        let id = format!("t/{}", theme.label());
        let leaves = in_theme.iter().map(|e| leaf(&id, *e)).collect();
        dirs.push(mk_dir(id, theme.label().to_string(), leaves));
    }
    // Entries with no theme at all (plants / patterns / tools).
    let others: Vec<&'static dyn CatalogueEntry> = entries
        .iter()
        .copied()
        .filter(|e| e.themes().is_empty())
        .collect();
    if !others.is_empty() {
        let id = "t/Other".to_string();
        let leaves = others.iter().map(|e| leaf(&id, *e)).collect();
        dirs.push(mk_dir(id, "Other".to_string(), leaves));
    }
    dirs
}

fn by_role(entries: &[&'static dyn CatalogueEntry]) -> Vec<CatNode> {
    const ROLES: [StructureRole; 6] = [
        StructureRole::Landmark,
        StructureRole::Secondary,
        StructureRole::Prop,
        StructureRole::Plant,
        StructureRole::Pattern,
        StructureRole::Tool,
    ];
    ROLES
        .iter()
        .filter_map(|&role| {
            let in_role: Vec<&'static dyn CatalogueEntry> = entries
                .iter()
                .copied()
                .filter(|e| e.role() == role)
                .collect();
            if in_role.is_empty() {
                return None;
            }
            let id = format!("r/{}", role.label());
            let leaves = in_role.iter().map(|e| leaf(&id, *e)).collect();
            Some(mk_dir(id, role.label().to_string(), leaves))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// UI
// ---------------------------------------------------------------------------

/// Cached catalogue node tree keyed on the `(mode, search)` that built it, with
/// the precomputed leaf `total` alongside (#639).
type NodeCache = Option<(BrowseMode, String, Vec<CatNode>, usize)>;

#[allow(clippy::too_many_arguments)]
pub(crate) fn catalogue_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    mut browser: ResMut<CatalogueBrowser>,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut pending_drop: ResMut<PendingGeneratorDrop>,
    // Per-frame caches (#639): the node tree is a pure function of (mode,
    // search) over the `const ENTRIES`; the placeability bool is a pure
    // function of the selected slug. Rebuild only when those keys change.
    mut node_cache: Local<NodeCache>,
    mut placeable_cache: Local<Option<(String, bool)>>,
) {
    let can_drag_place = match (session.as_ref(), room_did.as_ref()) {
        (Some(s), Some(r)) => s.did == r.0,
        _ => false,
    };

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let mut open = panels.catalogue;
    egui::Window::new("Catalogue")
        .open(&mut open)
        .default_pos([390.0, 420.0])
        .default_size([560.0, 440.0])
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            // ── Top bar: search + view-mode selector ──
            ui.horizontal(|ui| {
                ui.label("Search:");
                ui.add(
                    egui::TextEdit::singleline(&mut browser.search)
                        .hint_text("name / theme")
                        .desired_width(150.0),
                );
                if !browser.search.is_empty() && ui.small_button("✕").clicked() {
                    browser.search.clear();
                }
                ui.separator();
                egui::ComboBox::from_id_salt("catalogue_mode")
                    .selected_text(browser.mode.label())
                    .show_ui(ui, |ui| {
                        for m in BrowseMode::ALL {
                            ui.selectable_value(&mut browser.mode, m, m.label());
                        }
                    });
            });
            ui.separator();

            if node_cache
                .as_ref()
                .map(|(m, s, ..)| *m != browser.mode || *s != browser.search)
                .unwrap_or(true)
            {
                let built = build_nodes(browser.mode, &browser.search);
                let total: usize = built.iter().map(count_leaves).sum();
                *node_cache = Some((browser.mode, browser.search.clone(), built, total));
            }
            let (.., nodes, total) = node_cache.as_ref().expect("node cache just populated");
            let total = *total;

            // Placeability is a pure function of the selected slug — cache it so
            // the detail panel doesn't deep-build the whole `Generator` tree
            // every frame just to read one enum discriminant (#639).
            let placeable = match browser.selected.as_deref() {
                Some(slug) => {
                    if placeable_cache
                        .as_ref()
                        .map(|(s, _)| s != slug)
                        .unwrap_or(true)
                    {
                        let p = by_slug(slug)
                            .map(|e| is_drop_placeable(&e.build("")))
                            .unwrap_or(false);
                        *placeable_cache = Some((slug.to_string(), p));
                    }
                    placeable_cache.as_ref().map(|(_, p)| *p).unwrap_or(false)
                }
                None => false,
            };

            ui.horizontal_top(|ui| {
                // ── Left: the tree ──
                let left_w = (ui.available_width() * 0.46).clamp(180.0, 300.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(left_w, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.label(
                            egui::RichText::new(format!("{total} entries"))
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                        egui::ScrollArea::vertical()
                            .id_salt("catalogue_tree_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let (_resp, actions) =
                                    TreeView::new(ui.make_persistent_id("catalogue_tree"))
                                        .show(ui, |builder| render_nodes(builder, nodes));
                                for action in actions {
                                    if let Action::SetSelected(ids) = action
                                        && let Some(slug) = ids.first().and_then(|id| leaf_slug(id))
                                    {
                                        browser.selected = Some(slug.to_string());
                                    }
                                }
                            });
                    },
                );
                ui.separator();
                // ── Right: the detail panel ──
                egui::ScrollArea::vertical()
                    .id_salt("catalogue_detail_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        detail_panel(
                            ui,
                            browser.selected.as_deref(),
                            &mut pending_drop,
                            can_drag_place,
                            placeable,
                        );
                    });
            });
        });
    panels.catalogue = open;
}

/// Hierarchical "+ From Catalogue" menu shared by every add-catalogue call
/// site (the generator tree's root and per-node add menus, and the in-scene
/// right-click "Create new…"). Mirrors the browser's default [`BrowseMode::Hierarchy`]
/// tree — Category → (Buildings) Theme → Role → item, other categories listing
/// items directly — so the menu and the browser never drift apart.
///
/// Each [`CatNode::Dir`] becomes a nested submenu (labelled with the same
/// `name  (count)` the browser shows); clicking a leaf hands `on_pick` the
/// entry's slug plus a freshly-built [`Generator`]. `did` is stamped into the
/// build: pass `""` to seed a blank blueprint (personalisable entries like the
/// Teleporter get their DID filled in at gift/drop time) or the owner's DID to
/// place a live working copy directly (the scene right-click, #720/#722).
pub(crate) fn catalogue_menu(
    ui: &mut egui::Ui,
    did: &str,
    mut on_pick: impl FnMut(String, Generator),
) {
    let nodes = build_nodes(BrowseMode::Hierarchy, "");
    render_menu_nodes(ui, &nodes, did, &mut on_pick);
}

/// Recursive body of [`catalogue_menu`]: `Dir`s open submenus, `Leaf`s are
/// buttons. `on_pick` is a `&mut dyn` so the sibling submenu closures in a loop
/// can each reborrow it (a generic `&mut impl` would be moved by the first).
fn render_menu_nodes(
    ui: &mut egui::Ui,
    nodes: &[CatNode],
    did: &str,
    on_pick: &mut dyn FnMut(String, Generator),
) {
    for node in nodes {
        match node {
            CatNode::Dir {
                label,
                count,
                children,
                ..
            } => {
                ui.menu_button(format!("{label}  ({count})"), |ui| {
                    render_menu_nodes(ui, children, did, on_pick);
                });
            }
            CatNode::Leaf { id, name } => {
                if let Some(slug) = leaf_slug(id)
                    && let Some(entry) = by_slug(slug)
                    && ui
                        .button(*name)
                        .on_hover_text(entry.description())
                        .clicked()
                {
                    on_pick(slug.to_string(), entry.build(did));
                    ui.close();
                }
            }
        }
    }
}

/// Recursively emit the [`CatNode`] tree into the tree-view builder.
fn render_nodes(builder: &mut egui_ltreeview::TreeViewBuilder<'_, String>, nodes: &[CatNode]) {
    for node in nodes {
        match node {
            CatNode::Dir {
                id,
                label,
                count,
                children,
            } => {
                builder.dir(id.clone(), format!("{label}  ({count})"));
                render_nodes(builder, children);
                builder.close_dir();
            }
            CatNode::Leaf { id, name } => {
                builder.leaf(id.clone(), *name);
            }
        }
    }
}

fn detail_panel(
    ui: &mut egui::Ui,
    selected: Option<&str>,
    pending_drop: &mut PendingGeneratorDrop,
    can_drag_place: bool,
    // Precomputed + cached by the caller (#639) so the whole generator tree
    // isn't deep-built every frame just to read its kind discriminant.
    placeable: bool,
) {
    let Some(entry) = selected.and_then(by_slug) else {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("Select an item to see its details.")
                .italics()
                .color(egui::Color32::GRAY),
        );
        return;
    };
    let slug = entry.slug();

    ui.heading(entry.name());
    ui.label(entry.description());
    ui.add_space(4.0);
    ui.separator();

    egui::Grid::new("catalogue_detail_grid")
        .num_columns(2)
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            let row = |ui: &mut egui::Ui, k: &str, v: String| {
                ui.label(egui::RichText::new(k).strong());
                ui.label(v);
                ui.end_row();
            };
            row(ui, "Category", entry.category().label().to_string());
            row(ui, "Role", entry.role().label().to_string());
            if !entry.themes().is_empty() {
                let themes = entry
                    .themes()
                    .iter()
                    .map(|t| t.label())
                    .collect::<Vec<_>>()
                    .join(", ");
                row(ui, "Themes", themes);
            }
            row(ui, "Prosperity", entry.prosperity_band().label());
            row(ui, "Escalation", entry.escalation_band().label());
            let fp = entry.footprint();
            row(ui, "Clearance", format!("{:.1} m", fp.clearance));
            row(ui, "Spawn dist", format!("{:.0} m", fp.min_spawn_dist));
            ui.label(egui::RichText::new("Slug").strong());
            ui.label(egui::RichText::new(slug).monospace().small());
            ui.end_row();
        });

    ui.add_space(6.0);
    ui.separator();

    // Placement affordance — a drag handle that arms the same
    // PendingGeneratorDrop the inventory/viewport path consumes. `placeable`
    // is passed in (cached by the caller) rather than deep-building the
    // generator here every frame.
    if !placeable {
        ui.label(
            egui::RichText::new("Room-scoped — not point-placeable.")
                .small()
                .color(egui::Color32::GRAY),
        );
    } else if can_drag_place {
        let handle =
            egui::Label::new(egui::RichText::new("⠿  Drag into the room to place").strong())
                .sense(egui::Sense::click_and_drag());
        let resp = ui.add(handle);
        if resp.drag_started() {
            pending_drop.generator_name = Some(slug.to_string());
            pending_drop.source = DropSource::Catalogue;
        }
        if resp.dragged() && pending_drop.generator_name.as_deref() == Some(slug) {
            egui::Tooltip::always_open(
                ui.ctx().clone(),
                ui.layer_id(),
                egui::Id::new(("catalogue_drag_tip", slug)),
                egui::PopupAnchor::Pointer,
            )
            .show(|ui| {
                ui.label(format!("Place “{}”", entry.name()));
            });
        }
    } else {
        ui.label(
            egui::RichText::new("Open a room you own to place items.")
                .small()
                .color(egui::Color32::GRAY),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn collect_slugs<'a>(nodes: &'a [CatNode], out: &mut Vec<&'a str>) {
        for n in nodes {
            match n {
                CatNode::Leaf { id, .. } => out.push(leaf_slug(id).expect("leaf id has a slug")),
                CatNode::Dir { children, .. } => collect_slugs(children, out),
            }
        }
    }

    fn all_slugs(nodes: &[CatNode]) -> Vec<&str> {
        let mut v = Vec::new();
        collect_slugs(nodes, &mut v);
        v
    }

    #[test]
    fn matches_is_case_insensitive_over_name_slug_and_theme() {
        let castle = by_slug("medieval_castle").unwrap();
        assert!(matches(castle, "castle"));
        assert!(matches(castle, "medieval")); // theme label + slug + name
        assert!(matches(castle, "")); // empty matches everything
        assert!(!matches(castle, "spaceship"));
    }

    #[test]
    fn every_mode_covers_all_entries() {
        // The union of leaf slugs across the whole tree must equal the full
        // registry in every mode — no entry is ever unreachable.
        let want: HashSet<&str> = ENTRIES.iter().map(|e| e.slug()).collect();
        for mode in BrowseMode::ALL {
            let nodes = build_nodes(mode, "");
            let got: HashSet<&str> = all_slugs(&nodes).into_iter().collect();
            assert_eq!(got, want, "{} dropped or invented entries", mode.label());
        }
    }

    #[test]
    fn hierarchy_nests_buildings_by_theme_then_role() {
        let nodes = build_nodes(BrowseMode::Hierarchy, "");
        let buildings = nodes
            .iter()
            .find_map(|n| match n {
                CatNode::Dir {
                    label, children, ..
                } if label == "Buildings" => Some(children),
                _ => None,
            })
            .expect("Buildings group present");
        // Every Buildings child is a theme dir whose children are role dirs
        // whose children are leaves.
        for theme in buildings {
            let CatNode::Dir {
                children: roles, ..
            } = theme
            else {
                panic!("theme level must be a dir");
            };
            for role in roles {
                let CatNode::Dir {
                    children: leaves, ..
                } = role
                else {
                    panic!("role level must be a dir");
                };
                assert!(leaves.iter().all(|c| matches!(c, CatNode::Leaf { .. })));
            }
        }
        // Medieval theme exists with its castle landmark.
        let medieval = buildings
            .iter()
            .find(|n| matches!(n, CatNode::Dir { label, .. } if label == "Medieval"))
            .expect("Medieval theme dir");
        assert!(all_slugs(std::slice::from_ref(medieval)).contains(&"medieval_castle"));
    }

    #[test]
    fn cross_theme_props_are_not_duplicated_under_every_theme() {
        // A civic cross-theme prop appears once (in the Cross-theme group),
        // not once per theme — so its slug count across the tree is 1.
        let nodes = build_nodes(BrowseMode::Theme, "");
        let count = all_slugs(&nodes)
            .into_iter()
            .filter(|s| *s == "fountain")
            .count();
        assert_eq!(count, 1, "cross-theme prop duplicated across theme dirs");
    }

    #[test]
    fn filter_prunes_nonmatching_and_drops_empty_groups() {
        let nodes = build_nodes(BrowseMode::Hierarchy, "medieval");
        let slugs = all_slugs(&nodes);
        assert!(!slugs.is_empty());
        // Every surviving leaf matches the query.
        for s in &slugs {
            let e = by_slug(s).unwrap();
            assert!(
                matches(e, "medieval"),
                "{s} survived the filter but doesn't match"
            );
        }
        // No empty directories survive.
        fn no_empty_dirs(nodes: &[CatNode]) -> bool {
            nodes.iter().all(|n| match n {
                CatNode::Leaf { .. } => true,
                CatNode::Dir { children, .. } => !children.is_empty() && no_empty_dirs(children),
            })
        }
        assert!(no_empty_dirs(&nodes));
    }

    #[test]
    fn name_mode_is_flat_and_alphabetical() {
        let nodes = build_nodes(BrowseMode::Name, "");
        assert!(
            nodes.iter().all(|n| matches!(n, CatNode::Leaf { .. })),
            "name mode is a flat leaf list"
        );
        let names: Vec<&str> = nodes
            .iter()
            .map(|n| match n {
                CatNode::Leaf { id, .. } => by_slug(leaf_slug(id).unwrap()).unwrap().name(),
                _ => unreachable!(),
            })
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }
}
