//! Catalogue browser window — the client-shipped sibling of the
//! Inventory window. A master-detail browser over [`crate::catalogue::ENTRIES`]:
//! a hierarchical / sortable tree on the left (search + view-mode selector
//! above it) and a metadata detail panel on the right. Selecting an entry
//! shows its description, themes, role, socio bands and footprint. The tree
//! rows are themselves the drag source: drag one into the active room (or
//! onto a peer in People to gift) to stamp a fresh copy on viewport release.
//!
//! Drag mechanics mirror [`crate::ui::inventory::inventory_ui`] — the drag
//! source is [`DropSource::Catalogue`], which makes
//! [`handle_generator_drop`](crate::ui::inventory::handle_generator_drop)
//! resolve the dragged slug against the catalogue registry. Browsing works
//! in any room; the place handle only arms when the active room belongs to
//! the signed-in user.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use egui_ltreeview::{Action, TreeView};

use crate::catalogue::{CatalogueCategory, CatalogueEntry, ENTRIES, StructureRole, by_slug};
use crate::pds::Generator;
use crate::pds::inventory::is_drop_placeable;
use crate::seeded_defaults::ThemeArchetype;
use crate::ui::inventory::{DropSource, PendingGeneratorDrop};

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
    /// When [`Self::selected`] was picked, in `Time::elapsed_secs_f64`
    /// seconds (#1301): the item preview has one stage, and with the
    /// Inventory also holding a selection the most recent pick is pictured.
    picked_at: f64,
}

impl CatalogueBrowser {
    /// The entry the detail panel is showing, if any (#1288). Read by
    /// [`crate::item_preview`] to decide what to put on its stage — the
    /// preview derives its subject from this rather than being pushed one
    /// per frame, so there is no change tick for a panel draw to dirty.
    pub fn selected_slug(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    /// When the selection was made (#1301).
    pub fn picked_at(&self) -> f64 {
        self.picked_at
    }

    /// Set the selection without a draw, picked `at`, for
    /// `tests::the_preview_request_mirrors_the_browser_and_the_window_including_absence`.
    /// The field is private because only the detail panel's own click
    /// handling may move it; the mirror test needs a browser in a known
    /// state and building one through egui would test the tree widget
    /// instead of the mirror.
    #[cfg(test)]
    pub(crate) fn select_for_test(&mut self, slug: Option<&str>, at: f64) {
        self.selected = slug.map(str::to_owned);
        self.picked_at = at;
    }
}

/// Publish what the Catalogue and the Inventory are asking the item
/// preview to show (#1297, #1301), so the preview pipeline never reads the
/// egui layer.
///
/// `PreUpdate`, unconditionally: a mirror inside either window's system
/// would run only while that window is open and latch at its last value
/// the moment it closed — which for this fact means the stage keeping the
/// last selection alive, and its camera pass with it, for the rest of the
/// session. With no panels resource (before login, the headless render
/// tool) the answer is `None`; a missing browser or stash is a window with
/// nothing to bid.
///
/// The predicate lives with the CONSUMER
/// ([`crate::item_preview::wanted_subject`]) and is called from here, so
/// the rules — a closed window shows nothing, an unresolvable pick shows
/// nothing rather than the last thing that did, the most recent pick holds
/// the one stage — have one home. The stash is handed over with its change
/// tick, which is what restages an edited item (#1322 made that tick move
/// only on real writes).
///
/// Guarded write (#879).
pub fn mirror_preview_request(
    panels: Option<Res<crate::ui::toolbar::UiPanels>>,
    browser: Option<Res<CatalogueBrowser>>,
    stash_browser: Option<Res<crate::ui::inventory::InventoryBrowser>>,
    stash: Option<Res<crate::state::LiveInventoryRecord>>,
    mut request: ResMut<crate::item_preview::PreviewRequest>,
) {
    use crate::item_preview::Pick;
    let wanted = panels.as_deref().and_then(|panels| {
        let catalogue = browser
            .as_deref()
            .map(|browser| Pick {
                open: panels.catalogue,
                selected: browser.selected_slug(),
                at: browser.picked_at(),
            })
            .unwrap_or_default();
        let inventory = stash_browser
            .as_deref()
            .map(|browser| Pick {
                open: panels.inventory,
                selected: browser.selected_name(),
                at: browser.picked_at(),
            })
            .unwrap_or_default();
        crate::item_preview::wanted_subject(
            catalogue,
            inventory,
            stash.as_ref().map(|stash| (&stash.0, stash.last_changed())),
        )
    });
    if request.0 != wanted {
        request.0 = wanted;
    }
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
/// matches everything; otherwise a substring of the name, slug, any theme
/// label, or the entry's own **category** and **role** labels.
///
/// The last two were added by #1275 f255. A user told by the Avatar editor
/// to "copy a wearable from the Catalogue first" types the app's own word
/// and used to get a blank pane, because the four wearables carry that word
/// only on their category. The deliberate consequence is that a search for
/// a taxonomy word now matches every entry filed under it — "plant" returns
/// all the plants rather than only the ones with "plant" in the name — so
/// the count above the tree is a count of the section, not of the name hits.
/// That is what a search box over a sectioned list is expected to do; the
/// alternative (name hits only) is what made the app's own instruction fail.
///
/// It does NOT make synonyms work: "hat" still matches nothing, because no
/// field on `Gilded Circlet` contains the word. A synonym table would be a
/// different feature and is not shipped here.
fn matches(e: &dyn CatalogueEntry, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    e.name().to_lowercase().contains(q)
        || e.slug().contains(q)
        || e.category().label().to_lowercase().contains(q)
        || e.role().label().to_lowercase().contains(q)
        || e.themes()
            .iter()
            .any(|t| t.label().to_lowercase().contains(q))
}

/// The sentence the tree column shows when nothing is listed (#1275 f255).
///
/// Zero matches used to render the count label "0 entries" over an empty
/// `TreeView` and nothing else, beside a detail pane still saying "Select an
/// item to see its details." — three widgets none of which said the search
/// had come back empty, so a blank rectangle was the whole answer.
///
/// Both arms name the RULE rather than the outcome, which is #1232 f24's
/// shape: the reachable set here is a function of what `matches` looks at,
/// and that is not a thing a user can infer from "No matches."
pub(crate) fn empty_state(query: &str) -> &'static str {
    if query.trim().is_empty() {
        // Unreachable with the shipped registry (392 entries, and every
        // browse mode covers all of them — `every_mode_covers_all_entries`).
        // It is here so a build that ships an empty registry says so instead
        // of looking broken.
        "This build shipped no catalogue entries."
    } else {
        "Nothing matches that search. The Catalogue looks at names, \
         categories, roles and theme names — try \"wearables\", \"tools\", \
         or a theme like \"medieval\"."
    }
}

/// An entry tagged with *every* theme is a cross-theme prop (the civic
/// kit); it is filed under one "Cross-theme" group rather than duplicated
/// under all 24 themes.
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
    const ROLES: [StructureRole; 5] = [
        StructureRole::Landmark,
        StructureRole::Secondary,
        StructureRole::Prop,
        StructureRole::Gateway,
        StructureRole::Monument,
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
    // Cross-theme props first. Themeless Buildings entries (the
    // social-gateway placeholder) belong here too — serving every theme
    // as a fallback is exactly what cross-theme means.
    let cross: Vec<&'static dyn CatalogueEntry> = in_cat
        .iter()
        .copied()
        .filter(|e| is_cross_theme(*e) || e.themes().is_empty())
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
    const ROLES: [StructureRole; 9] = [
        StructureRole::Landmark,
        StructureRole::Secondary,
        StructureRole::Prop,
        StructureRole::Gateway,
        StructureRole::Monument,
        StructureRole::Plant,
        StructureRole::Pattern,
        StructureRole::Tool,
        StructureRole::Attachment,
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
    mut chrome: crate::ui::layout::WindowChrome,
    mut browser: ResMut<CatalogueBrowser>,
    mut pending_drop: ResMut<PendingGeneratorDrop>,
    // Wear wiring (#1087): mutated only on a Wear click, so the ResMut
    // never dirties the live record from mere browsing (the guarded-dirty
    // rule) — reads go through `as_ref()`.
    mut live_avatar: Option<ResMut<crate::state::LiveAvatarRecord>>,
    // The inventory is the wear surface (#1096): the catalogue copies a
    // vanilla item into it. Same guarded-dirty discipline — the stash's
    // dirty state is derived live-vs-stored, so a deref_mut on click is
    // all that ever happens.
    mut live_inventory: Option<ResMut<crate::state::LiveInventoryRecord>>,
    session: Option<Res<bevy_symbios_multiuser::auth::AtprotoSession>>,
    // Whose room this is (#1220 f132): the drag tooltip promises a
    // placement only to an owner — a visitor can gift and nothing else.
    room_did: Option<Res<crate::state::CurrentRoomDid>>,
    mut undo_labels: ResMut<crate::ui::undo::PendingUndoLabels>,
    mut toasts: ResMut<crate::notify::Toasts>,
    time: Res<Time>,
    // Per-frame cache (#639): the node tree is a pure function of (mode,
    // search) over the `const ENTRIES`; rebuild only when those keys change.
    mut node_cache: Local<NodeCache>,
    // The selected entry's live picture (#1288). `Option` because the
    // resource is inserted by `item_preview`'s Startup system, so a test
    // harness that runs this system without that plugin simply draws the
    // placeholder square.
    preview: Option<Res<crate::item_preview::ItemPreview>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // Guarded-dirty (#879, generalised by #1274 f177): the search field and
    // the mode combo below take `&mut` fields of this resource, and
    // `ResMut::deref_mut` stamps the change tick on ACCESS — so drawing the
    // window marked it changed on every frame whether or not anybody typed.
    // Nothing reads this resource's change tick today, and copying its string
    // in and out each frame to find that out would cost more than the tick
    // does. If a consumer is ever added, call `set_changed()` on a real edit
    // rather than deleting this line.
    let browser = browser.bypass_change_detection();
    // Guarded-dirty (#879): `.open(&mut panels.catalogue)` through the
    // `ResMut` would mark UiPanels changed every frame, starving the
    // prefs save debounce — local copy in, write back only on close.
    let mut open = panels.catalogue;
    let (pos, size) = chrome.place(crate::ui::layout::UiWindow::Catalogue, ctx);
    let response = egui::Window::new("Catalogue")
        .open(&mut open)
        .default_pos(pos)
        .default_size(size)
        .constrain_to(chrome.available_rect(ctx))
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            // ── Top bar: search + view-mode selector ──
            ui.horizontal(|ui| {
                ui.label("Search:");
                crate::ui::affordances::text_edit(
                    ui,
                    egui::TextEdit::singleline(&mut browser.search)
                        .hint_text("name / theme")
                        .desired_width(150.0),
                );
                if !browser.search.is_empty()
                    && ui.small_button(crate::ui::affordances::CROSS).clicked()
                {
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

            ui.horizontal_top(|ui| {
                // ── Left: the tree ──
                let left_w =
                    (ui.available_width() * 0.46).clamp(crate::ui::layout::LIST_MIN_WIDTH, 300.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(left_w, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "{total} {}",
                                crate::text::plural(total, "entry", "entries")
                            ))
                            .small()
                            .color(crate::ui::theme::current(ui.ctx()).text_weak),
                        );
                        if total == 0 {
                            ui.add_space(4.0);
                            ui.add(egui::Label::new(empty_state(&browser.search)).wrap());
                            if !browser.search.is_empty() && ui.button("Clear search").clicked() {
                                browser.search.clear();
                            }
                        }
                        egui::ScrollArea::vertical()
                            .id_salt("catalogue_tree_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let (_resp, actions) =
                                    TreeView::new(ui.make_persistent_id("catalogue_tree"))
                                        // Enables dragging rows OUT of the
                                        // tree (`Action::DragExternal`) so a
                                        // leaf itself is a drag-to-place
                                        // source (#832) — the natural gesture
                                        // used to do nothing; the only handle
                                        // was buried below the detail grid.
                                        .allow_drag_and_drop(true)
                                        .show(ui, |builder| render_nodes(builder, nodes));
                                for action in actions {
                                    match action {
                                        Action::SetSelected(ids) => {
                                            if let Some(slug) =
                                                ids.first().and_then(|id| leaf_slug(id))
                                            {
                                                browser.selected = Some(slug.to_string());
                                                browser.picked_at = time.elapsed_secs_f64();
                                            }
                                        }
                                        // Fires every frame a row is dragged
                                        // outside the tree: arm the shared
                                        // drop bus once (placeable leaves
                                        // only) and select the row so the
                                        // detail panel follows the drag.
                                        Action::DragExternal(drag) | Action::MoveExternal(drag) => {
                                            let Some(slug) =
                                                drag.source.first().and_then(|id| leaf_slug(id))
                                            else {
                                                continue;
                                            };
                                            if pending_drop.generator_name.as_deref() == Some(slug)
                                            {
                                                continue;
                                            }
                                            let placeable = by_slug(slug)
                                                .map(|e| is_drop_placeable(&e.build("")))
                                                .unwrap_or(false);
                                            if placeable {
                                                browser.selected = Some(slug.to_string());
                                                browser.picked_at = time.elapsed_secs_f64();
                                                pending_drop.generator_name =
                                                    Some(slug.to_string());
                                                pending_drop.source = DropSource::Catalogue;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                // The sentence the Catalogue never had
                                // (#1220 f132). `egui_ltreeview` paints the
                                // dragged row at the cursor on its own
                                // Tooltip-order layer, so the drag WAS
                                // visibly live — what was missing is the
                                // copy saying a drop on a peer gifts it, on
                                // the surface where a new user meets the
                                // gesture and which auto-opens the People
                                // window mid-drag. Display name, never the
                                // slug: `stone_cottage_a` is not a name.
                                if pending_drop.source == DropSource::Catalogue
                                    && let Some(slug) = pending_drop.generator_name.as_deref()
                                    && let Some(entry) = by_slug(slug)
                                {
                                    let owns_room = match (session.as_deref(), room_did.as_deref())
                                    {
                                        (Some(sess), Some(room)) => room.0 == sess.did,
                                        _ => false,
                                    };
                                    crate::ui::inventory::drag_tooltip(
                                        ui,
                                        "cat_drag_tip",
                                        entry.name(),
                                        owns_room,
                                    );
                                }
                            });
                    },
                );
                ui.separator();
                // ── Right: the detail panel ──
                // Both sides live inside the `horizontal_top` above, so give
                // the detail its own top-down region (mirroring the tree's
                // `allocate_ui_with_layout`). Without it the ScrollArea
                // inherits the left-to-right flow and the heading, description
                // and property grid render as separate side-by-side columns —
                // and the description, handed an unbounded width, never wraps
                // and stretches the whole window. Claiming the remaining width
                // top-down stacks them into one wrapping column.
                let detail_w = ui.available_width();
                ui.allocate_ui_with_layout(
                    egui::vec2(detail_w, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("catalogue_detail_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                detail_panel(
                                    ui,
                                    browser.selected.as_deref(),
                                    preview.as_deref(),
                                    live_inventory.as_mut(),
                                    live_avatar.as_mut(),
                                    session.as_deref(),
                                    &mut undo_labels,
                                    &mut toasts,
                                    time.elapsed_secs_f64(),
                                );
                            });
                    },
                );
            });
        });
    if panels.catalogue && !open {
        panels.catalogue = false;
    }
    if let Some(response) = response {
        chrome.remember(
            crate::ui::layout::UiWindow::Catalogue,
            response.response.rect,
        );
    }
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

#[allow(clippy::too_many_arguments)]
fn detail_panel(
    ui: &mut egui::Ui,
    selected: Option<&str>,
    preview: Option<&crate::item_preview::ItemPreview>,
    live_inventory: Option<&mut ResMut<crate::state::LiveInventoryRecord>>,
    live_avatar: Option<&mut ResMut<crate::state::LiveAvatarRecord>>,
    session: Option<&bevy_symbios_multiuser::auth::AtprotoSession>,
    undo_labels: &mut crate::ui::undo::PendingUndoLabels,
    toasts: &mut crate::notify::Toasts,
    now: f64,
) {
    let Some(entry) = selected.and_then(by_slug) else {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("Select an item to see its details.")
                .italics()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
        );
        return;
    };
    let slug = entry.slug();

    crate::ui::item_picture::draw_preview(
        ui,
        preview,
        crate::ui::item_picture::PictureOf::Catalogue(slug),
        crate::ui::item_picture::CATALOGUE_SIDE,
    );
    ui.heading(entry.name());
    ui.add(egui::Label::new(entry.description()).wrap());
    ui.add_space(4.0);
    ui.separator();

    egui::Grid::new("catalogue_detail_grid")
        .num_columns(2)
        .spacing([10.0, 4.0])
        // Cap the value column and WRAP its text (#832 follow-up): grid
        // cells default to no-wrap, so a long Themes list used to demand
        // its full single-line width and stretch the whole Catalogue
        // window unreasonably wide.
        .max_col_width(280.0)
        .show(ui, |ui| {
            let row = |ui: &mut egui::Ui, k: &str, v: String| {
                ui.label(egui::RichText::new(k).strong());
                ui.add(egui::Label::new(v).wrap());
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
            // Wearability up front (#1096): the socket it lands on and, for
            // a fitted item, that it sizes itself — the category's
            // flagship property, visible before anything is worn.
            if let Some(socket) = entry.wear_socket() {
                row(
                    ui,
                    "Worn at",
                    crate::pds::avatar::socket_label(socket.name()).to_string(),
                );
                if let Some(fit) = entry.wear_fit() {
                    let text = match fit {
                        crate::catalogue::WearFit::HeadBand { inner_diameter } => format!(
                            "sizes itself to the wearer's head ({:.0} mm band authored)",
                            inner_diameter * 1000.0
                        ),
                    };
                    row(ui, "Fit", text);
                }
            }
            ui.label(egui::RichText::new("Slug").strong());
            ui.label(egui::RichText::new(slug).monospace().small());
            ui.end_row();
        });

    ui.add_space(6.0);
    ui.separator();
    inventory_row(
        ui,
        entry,
        live_inventory,
        live_avatar,
        session,
        undo_labels,
        toasts,
        now,
    );
}

/// The inventory row (#1096): **Copy to inventory**, and for a wearable
/// entry **Copy to inventory & wear**. The catalogue is a source of
/// vanilla items; the inventory is what the player owns and customises,
/// and the one surface things are worn and taken off from — so the
/// catalogue never dresses the body directly any more. "& wear" is the
/// two steps in one click: the copy lands in the stash under the entry's
/// name (uniquified), then that stash item is worn, so the worn prop
/// carries its provenance and Save-to-inventory has somewhere to go.
/// Placement is untouched: the tree leaf stays the drag handle.
#[allow(clippy::too_many_arguments)]
fn inventory_row(
    ui: &mut egui::Ui,
    entry: &'static dyn CatalogueEntry,
    live_inventory: Option<&mut ResMut<crate::state::LiveInventoryRecord>>,
    live_avatar: Option<&mut ResMut<crate::state::LiveAvatarRecord>>,
    session: Option<&bevy_symbios_multiuser::auth::AtprotoSession>,
    undo_labels: &mut crate::ui::undo::PendingUndoLabels,
    toasts: &mut crate::notify::Toasts,
    now: f64,
) {
    use crate::pds::inventory::WearMeta;

    let weak = crate::ui::theme::current(ui.ctx()).text_weak;
    let hint = |ui: &mut egui::Ui, text: &str| {
        ui.label(egui::RichText::new(text).small().color(weak));
    };
    let wearable = entry.wear_socket();

    // One string, two surfaces (#1289). The hint line is the reason a
    // reader sees; the hover is the one a pointer reaches for first, and
    // `on_hover_text` shows nothing on a disabled control.
    let Some(session) = session else {
        let reason = "Sign in to own items.";
        ui.add_enabled(false, egui::Button::new("Copy to inventory"))
            .on_disabled_hover_text(reason);
        hint(ui, reason);
        return;
    };
    let Some(inventory) = live_inventory else {
        let reason = "Your inventory has not loaded yet.";
        ui.add_enabled(false, egui::Button::new("Copy to inventory"))
            .on_disabled_hover_text(reason);
        hint(ui, reason);
        return;
    };
    // Reads through `as_ref` — deref_mut would count as an edit every
    // frame the panel is open (the guarded-dirty rule).
    let cap = crate::config::state::MAX_INVENTORY_ITEMS;
    let count = inventory.as_ref().0.generators.len();
    if count >= cap {
        let reason = format!("Inventory full ({cap}/{cap}) — remove something first.");
        ui.add_enabled(false, egui::Button::new("Copy to inventory"))
            .on_disabled_hover_text(&reason);
        hint(ui, &reason);
        return;
    }

    // Where the copy would go — shown so "& wear" is not a surprise.
    let name = crate::ui::room::widgets::unique_key(&inventory.as_ref().0.generators, entry.name());
    let copy = |inventory: &mut ResMut<crate::state::LiveInventoryRecord>| -> String {
        let meta = wearable.map(|socket| WearMeta::for_entry(socket, entry.wear_fit()));
        inventory
            .0
            .put_item(name.clone(), entry.build(&session.did), meta);
        name.clone()
    };

    ui.horizontal(|ui| {
        if ui
            .button("Copy to inventory")
            .on_hover_text(format!("Add a copy to your inventory as \"{name}\""))
            .clicked()
        {
            let saved = copy(inventory);
            toasts.success(format!("Copied to inventory as \"{saved}\"."), now);
            return;
        }
        let Some(socket) = wearable else {
            return;
        };
        // "& wear" needs a body to dress. The reasons it cannot are the
        // Inventory row's and the scene menu's, from one source
        // (#1141) — including the one all three used to miss, a rigged
        // body whose wardrobe record did not resolve.
        //
        // Reads through `as_ref` for the same reason the inventory count
        // above does: deref_mut here would dirty the avatar record every
        // frame the catalogue is open.
        // The inventory is `Some` for this whole arm — the row returned
        // early with the Catalogue's own "not loaded yet" line above, which
        // is the wording `wear_blocked_reason` adopted (#1233 f261).
        let reason = crate::ui::avatar::wear_blocked_reason(
            live_avatar.as_ref().map(|live| &live.as_ref().0),
            true,
        );
        if let Some(reason) = reason {
            ui.add_enabled(false, egui::Button::new("Copy to inventory & wear"))
                .on_disabled_hover_text(&reason);
            hint(ui, &reason);
            return;
        }
        let Some(live) = live_avatar else {
            return;
        };
        if ui
            .button("Copy to inventory & wear")
            .on_hover_text(format!(
                "Add a copy to your inventory as \"{name}\" and wear it on your {}",
                crate::pds::avatar::socket_label(socket.name()).to_lowercase()
            ))
            .clicked()
        {
            // The copy lands either way — it is a separate, already-done
            // thing. Only the *wear* half is conditional, and the toast
            // now reports which halves actually happened (#1141): it used
            // to say "Wearing …" unconditionally, including on the path
            // where `attach_record` returned `None` and nothing was worn.
            let saved = copy(inventory);
            let attached =
                crate::ui::avatar::record_for_inventory_item(&inventory.as_ref().0, &saved)
                    .and_then(|record| {
                        live.0.body.rigged_mut().and_then(|rig| {
                            crate::ui::avatar::attach_record(rig, record, &session.did)
                        })
                    });
            if attached.is_some() {
                undo_labels.set_avatar(format!("wear {saved}"));
                toasts.success(
                    format!("Wearing \"{saved}\" — it is in your inventory to adjust or take off."),
                    now,
                );
            } else {
                toasts.warn(
                    format!(
                        "Copied to inventory as \"{saved}\", but it could not be worn — \
                         wear it from the Inventory window once your body is ready."
                    ),
                    now,
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// #1297's item-preview singleton, the `PlacementFocus` shape a
    /// second time, and since #1301 the Inventory's selection beside the
    /// Catalogue's. `restage_preview` read `UiPanels` and
    /// `CatalogueBrowser` directly to answer one question — "is anyone
    /// looking, and at what" — which pointed the arrow from a render
    /// pipeline into the egui layer for a `bool` and an `Option<&str>`.
    ///
    /// Asserted against the PREDICATE the consumer already owned
    /// ([`crate::item_preview::wanted_subject`]) rather than by restating
    /// its answers, so the mirror cannot drift from the rule it carries —
    /// including the rule that an unresolvable pick shows nothing rather
    /// than the last thing that did. A few answers are also named outright,
    /// so a predicate that answered `None` to everything could not pass.
    #[test]
    fn the_preview_request_mirrors_the_browser_and_the_window_including_absence() {
        use bevy::ecs::system::RunSystemOnce;
        use bevy::prelude::*;

        use crate::item_preview::{Pick, PreviewRequest, PreviewSubject, wanted_subject};
        use crate::pds::InventoryRecord;
        use crate::state::LiveInventoryRecord;
        use crate::ui::inventory::InventoryBrowser;
        use crate::ui::toolbar::UiPanels;

        let a_real_slug = crate::catalogue::ENTRIES[0].slug();

        let mut world = World::new();
        world.init_resource::<PreviewRequest>();

        fn mirrored(world: &mut World) -> Option<PreviewSubject> {
            world
                .run_system_once(mirror_preview_request)
                .expect("the mirror runs");
            world.resource::<PreviewRequest>().0.clone()
        }
        let pick =
            |open: bool, selected: Option<&'static str>, at: f64| Pick { open, selected, at };

        // Absence: no panels, no browser. Before login and in the headless
        // render tool, neither resource exists, and the answer must be
        // "nobody is looking" rather than a panic.
        assert_eq!(mirrored(&mut world), None, "no panels, no browser");

        world.insert_resource(UiPanels::default());
        let mut browser = CatalogueBrowser::default();
        browser.select_for_test(Some(a_real_slug), 1.0);
        world.insert_resource(browser);

        // A closed window is the whole "is anyone looking" question: it is
        // what turns the camera off.
        world.resource_mut::<UiPanels>().catalogue = false;
        world.resource_mut::<UiPanels>().inventory = false;
        let shut = Pick::default();
        assert_eq!(
            mirrored(&mut world),
            wanted_subject(pick(false, Some(a_real_slug), 1.0), shut, None)
        );
        assert_eq!(mirrored(&mut world), None);

        world.resource_mut::<UiPanels>().catalogue = true;
        assert_eq!(
            mirrored(&mut world),
            wanted_subject(pick(true, Some(a_real_slug), 1.0), shut, None)
        );
        assert_eq!(
            mirrored(&mut world),
            Some(PreviewSubject::Catalogue(a_real_slug.to_string()))
        );

        // A selection that no longer resolves shows nothing...
        world
            .resource_mut::<CatalogueBrowser>()
            .select_for_test(Some("not-a-real-entry"), 1.0);
        assert_eq!(
            mirrored(&mut world),
            None,
            "an unresolvable slug is nothing"
        );

        // ...and so does no selection at all: the request RELEASES.
        world
            .resource_mut::<CatalogueBrowser>()
            .select_for_test(None, 1.0);
        assert_eq!(mirrored(&mut world), None, "the request releases");

        // ---- The Inventory's bid (#1301) ----
        // Absence: an open Inventory window with no browser and no stash is
        // a window with nothing to bid.
        world.resource_mut::<UiPanels>().inventory = true;
        assert_eq!(mirrored(&mut world), None, "no inventory browser, no stash");
        let mut stash_browser = InventoryBrowser::default();
        stash_browser.select("lantern", 2.0);
        world.insert_resource(stash_browser);
        assert_eq!(
            mirrored(&mut world),
            wanted_subject(
                pick(true, None, 1.0),
                pick(true, Some("lantern"), 2.0),
                None
            )
        );
        assert_eq!(
            mirrored(&mut world),
            None,
            "a selection with no stash loaded"
        );

        let mut stash = InventoryRecord::default();
        stash.put_item("lantern".into(), Generator::default_cuboid(), None);
        stash.put_item(
            "dunes".into(),
            Generator {
                kind: crate::ui::room::construct::make_default_for_kind("Terrain"),
                ..Default::default()
            },
            None,
        );
        world.insert_resource(LiveInventoryRecord(stash));
        let tick = |world: &World| world.resource_ref::<LiveInventoryRecord>().last_changed();

        world.resource_mut::<UiPanels>().inventory = false;
        assert_eq!(mirrored(&mut world), None, "the Inventory window is shut");
        world.resource_mut::<UiPanels>().inventory = true;
        let resolved = mirrored(&mut world);
        {
            let stash = world.resource::<LiveInventoryRecord>();
            assert_eq!(
                resolved,
                wanted_subject(
                    pick(true, None, 1.0),
                    pick(true, Some("lantern"), 2.0),
                    Some((&stash.0, tick(&world)))
                )
            );
        }
        assert_eq!(
            resolved,
            Some(PreviewSubject::Inventory {
                name: "lantern".into(),
                edit: tick(&world)
            }),
            "a resolving name is pictured"
        );

        world
            .resource_mut::<InventoryBrowser>()
            .select("not-in-the-stash", 2.0);
        assert_eq!(
            mirrored(&mut world),
            None,
            "an unresolvable name is nothing"
        );
        world
            .resource_mut::<InventoryBrowser>()
            .select("dunes", 2.0);
        assert_eq!(
            mirrored(&mut world),
            None,
            "a room-scoped item is never staged"
        );
        world.resource_mut::<InventoryBrowser>().clear();
        assert_eq!(mirrored(&mut world), None, "the inventory request releases");

        // An edit moves `edit`, and nothing else does.
        world
            .resource_mut::<InventoryBrowser>()
            .select("lantern", 2.0);
        let before = mirrored(&mut world);
        assert_eq!(
            mirrored(&mut world),
            before,
            "an idle frame asks for the same subject"
        );
        world.increment_change_tick();
        world.resource_mut::<LiveInventoryRecord>().0.put_item(
            "bench".into(),
            Generator::default_cuboid(),
            None,
        );
        let after = mirrored(&mut world);
        match (&before, &after) {
            (
                Some(PreviewSubject::Inventory { name: a, edit: x }),
                Some(PreviewSubject::Inventory { name: b, edit: y }),
            ) => {
                assert_eq!(a, b, "the same item");
                assert_ne!(
                    x, y,
                    "a stash write must move `edit`, or the picture goes stale"
                );
            }
            other => panic!("expected the lantern before and after, got {other:?}"),
        }

        // ---- Both open, both holding a pick: the most recent wins ----
        world
            .resource_mut::<CatalogueBrowser>()
            .select_for_test(Some(a_real_slug), 5.0);
        let both = |world: &mut World| {
            let got = mirrored(world);
            let panels = world.resource::<UiPanels>();
            let catalogue = world.resource::<CatalogueBrowser>();
            let inventory = world.resource::<InventoryBrowser>();
            let stash = world.resource_ref::<LiveInventoryRecord>();
            let expected = wanted_subject(
                Pick {
                    open: panels.catalogue,
                    selected: catalogue.selected_slug(),
                    at: catalogue.picked_at(),
                },
                Pick {
                    open: panels.inventory,
                    selected: inventory.selected_name(),
                    at: inventory.picked_at(),
                },
                Some((&stash.0, stash.last_changed())),
            );
            assert_eq!(got, expected, "the mirror and the predicate disagree");
            got
        };
        assert!(
            matches!(both(&mut world), Some(PreviewSubject::Catalogue(_))),
            "the Catalogue picked at 5.0, after the Inventory's 2.0"
        );
        world
            .resource_mut::<InventoryBrowser>()
            .select("lantern", 7.0);
        assert!(
            matches!(both(&mut world), Some(PreviewSubject::Inventory { .. })),
            "the Inventory picked at 7.0, after the Catalogue's 5.0"
        );
        world.resource_mut::<UiPanels>().inventory = false;
        assert!(
            matches!(both(&mut world), Some(PreviewSubject::Catalogue(_))),
            "closing the winner's window falls back to the other pick"
        );
        world.resource_mut::<UiPanels>().inventory = true;
        world.resource_mut::<UiPanels>().catalogue = false;
        assert!(
            matches!(both(&mut world), Some(PreviewSubject::Inventory { .. })),
            "and the other way round"
        );
        // A later pick that cannot be pictured does not take the stage.
        world.resource_mut::<UiPanels>().catalogue = true;
        world
            .resource_mut::<InventoryBrowser>()
            .select("dunes", 9.0);
        assert!(
            matches!(both(&mut world), Some(PreviewSubject::Catalogue(_))),
            "a room-scoped pick leaves the stage to one that has a picture"
        );
    }

    /// #1301, counting the work (#1270's rule). The stage restages when —
    /// and only when — the request differs from what it holds, and an
    /// Inventory request carries the stash's change tick. So an item held
    /// selected must cost no respawn per frame, and an edit to the stash
    /// exactly one. Driven through the REAL mirror and the REAL
    /// `restage_preview` over a world with the whole spawn path in it; the
    /// counter is `item_preview`'s own, as `NODES_BUILT` is the tree's.
    ///
    /// There is no generator-compare count to take because there is no
    /// compare: the subject holds a name and a tick, never a `Generator`,
    /// and nothing on this path compares one — which is the design, since
    /// a whole-tree `PartialEq` per frame is what #1135/#1292 removed.
    #[test]
    fn an_idle_selection_costs_no_restages_and_an_edit_costs_exactly_one() {
        use bevy::ecs::system::RunSystemOnce;
        use bevy::prelude::*;

        use crate::item_preview::{
            PreviewCamera, PreviewRequest, preview_for_test, restage_preview, take_restage_count,
        };
        use crate::pds::InventoryRecord;
        use crate::state::LiveInventoryRecord;
        use crate::ui::inventory::InventoryBrowser;
        use crate::ui::toolbar::UiPanels;

        let mut app = crate::player::visuals::spawn_path_app();
        let world = app.world_mut();
        world.init_resource::<PreviewRequest>();
        world.insert_resource(UiPanels {
            inventory: true,
            ..Default::default()
        });
        world.insert_resource(CatalogueBrowser::default());
        let mut stash = InventoryRecord::default();
        stash.put_item("lantern".into(), Generator::default_cuboid(), None);
        world.insert_resource(LiveInventoryRecord(stash));
        let mut browser = InventoryBrowser::default();
        browser.select("lantern", 1.0);
        world.insert_resource(browser);
        let camera = world.spawn((Camera::default(), PreviewCamera)).id();
        world.insert_resource(preview_for_test(camera));

        let frame = |world: &mut World| {
            world
                .run_system_once(mirror_preview_request)
                .expect("the mirror runs");
            world
                .run_system_once(restage_preview)
                .expect("the restage runs");
        };

        take_restage_count();
        frame(world);
        assert_eq!(take_restage_count(), 1, "selecting stages the item once");
        for _ in 0..30 {
            frame(world);
        }
        assert_eq!(
            take_restage_count(),
            0,
            "thirty idle frames restage nothing"
        );

        world.increment_change_tick();
        world.resource_mut::<LiveInventoryRecord>().0.put_item(
            "lantern".into(),
            Generator {
                children: vec![Generator::default_cuboid()],
                ..Generator::default_cuboid()
            },
            None,
        );
        for _ in 0..30 {
            frame(world);
        }
        assert_eq!(
            take_restage_count(),
            1,
            "one edit, then thirty frames: exactly one restage"
        );

        world.resource_mut::<InventoryBrowser>().clear();
        frame(world);
        assert_eq!(
            take_restage_count(),
            1,
            "releasing takes the stage down once"
        );
        for _ in 0..30 {
            frame(world);
        }
        assert_eq!(take_restage_count(), 0, "and an empty stage stays empty");
    }

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
    fn matches_is_case_insensitive_over_name_slug_theme_category_and_role() {
        let castle = by_slug("medieval_castle").unwrap();
        assert!(matches(castle, "castle"));
        assert!(matches(castle, "medieval")); // theme label + slug + name
        assert!(matches(castle, "")); // empty matches everything
        assert!(!matches(castle, "spaceship"));
        // The category and role labels are searchable (#1275 f255), so the
        // castle answers to its section as well as to its name.
        assert!(matches(castle, "buildings"));
        assert!(matches(castle, "landmark"));
    }

    /// The app's own instruction — "copy a wearable from the Catalogue
    /// first" — has to find something when it is typed into the Catalogue
    /// (#1275 f255).
    ///
    /// This is the whole point of searching the category and role labels,
    /// so it is pinned against the REGISTRY rather than against one entry:
    /// a new wearable that somehow lands outside the category would fail
    /// here, which is the drift worth catching.
    #[test]
    fn searching_for_the_word_wearable_finds_every_wearable() {
        let worn: Vec<&str> = ENTRIES
            .iter()
            .filter(|e| e.role() == crate::catalogue::StructureRole::Attachment)
            .map(|e| e.slug())
            .collect();
        assert!(!worn.is_empty(), "the registry ships wearables");
        for slug in &worn {
            let e = by_slug(slug).expect("slug from the registry");
            assert!(
                matches(e, "wearable"),
                "{slug} is not found by \"wearable\""
            );
        }
        // And the word does not drag in the other 388 entries.
        let nodes = build_nodes(BrowseMode::Name, "wearable");
        let hits = all_slugs(&nodes);
        assert_eq!(
            hits.len(),
            worn.len(),
            "\"wearable\" matched beyond the category"
        );
    }

    /// Zero matches says which fields were searched, and the no-query arm
    /// says something different (#1275 f255).
    #[test]
    fn the_empty_state_names_the_rule() {
        let searched = empty_state("zzzz");
        for field in ["names", "categories", "roles", "theme"] {
            assert!(searched.contains(field), "the sentence omits {field}");
        }
        assert_ne!(searched, empty_state(""));
        assert_ne!(searched, empty_state("   "), "whitespace is not a query");
        // The control: the shipped registry never reaches the no-query arm.
        let all = build_nodes(BrowseMode::Name, "");
        assert!(!all_slugs(&all).is_empty());
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
