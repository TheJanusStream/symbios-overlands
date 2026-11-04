//! Catalogue browser window - the client-shipped sibling of the
//! Inventory window. A master-detail browser over [`crate::catalogue::ENTRIES`]:
//! a hierarchical / sortable tree on the left (search + view-mode selector
//! above it) and a metadata detail panel on the right. Selecting an entry
//! shows its description, themes, role, socio bands and footprint. The tree
//! rows are themselves the drag source: drag one into the active room (or
//! onto a peer in People to gift) to stamp a fresh copy on viewport release.
//!
//! Since #1840 the browser also lists **avatars** - the recipes in
//! [`crate::catalogue::avatars`], one per craft type plus a spread of
//! people and beasts. They are not items: an avatar entry places nothing,
//! so its row never arms the drop bus, and the menus that add an item from
//! the Catalogue ([`catalogue_menu`]) never list one. Its detail offers
//! **Wear**, which puts it on through the Inventory's own wear path
//! ([`crate::ui::avatar::wear_avatar_on_live`]: one undo step, unsaved
//! until the Avatar editor's Save, allowed in any world), and **Copy to
//! inventory**, which inserts it as an avatar item under a free name
//! ([`crate::ui::avatar::save_avatar_to_inventory_as`]) for the Inventory's
//! Save to keep. The Hierarchy files them under Avatars, then family; the
//! name mode lists them among the items; the theme, role and size modes,
//! which have nothing to sort an avatar by, end with one Avatars group.
//!
//! Since #1841 it lists **worlds** too - the recipes in
//! [`crate::catalogue::worlds`], one procedural world per theme and one
//! square of real Berlin per borough. A world places nothing either, and is
//! never built to be looked at: its detail is a text card read off the
//! entry's pins, and the world is built - the whole procedural pipeline,
//! the re-roll's stall, said in a toast before it - only on a click.
//! **Load into my world** asks first with the Inventory's own confirmation
//! and lands the built world through the Inventory's own landing
//! ([`crate::ui::inventory::world::land_loaded_world`], so through
//! [`crate::ui::room::replace_world`], the re-roll's path): one undo step,
//! unsaved until the World Editor's Save, only in your own world.
//! **Copy to inventory** stashes the built world as a world item through
//! the World Editor's snapshot path
//! ([`crate::ui::room::stash::save_world_to_inventory_as`]) under a free
//! name built on the entry's. Both are disabled, with the reason painted,
//! for the reasons those paths give. The Hierarchy files them under
//! Worlds, then Procedural or Berlin; the other modes treat them as they
//! treat the avatars.
//!
//! Drag mechanics mirror [`crate::ui::inventory::inventory_ui`] - the drag
//! source is [`DropSource::Catalogue`], which makes
//! [`handle_generator_drop`](crate::ui::inventory::handle_generator_drop)
//! resolve the dragged slug against the catalogue registry. Browsing works
//! in any room; the place handle only arms when the active room belongs to
//! the signed-in user.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use egui_ltreeview::{Action, TreeView};

use crate::catalogue::avatars::{AVATARS, AvatarEntry, avatar_by_slug};
use crate::catalogue::worlds::{WORLDS, WorldEntry, world_by_slug};
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
    /// Pending Load into my world confirmation (#1841), carrying the world
    /// entry's slug: a load replaces everything in the world.
    load_confirm: crate::ui::confirm::ConfirmState<&'static str>,
}

impl CatalogueBrowser {
    /// The entry the detail panel is showing, if any (#1288). Read by
    /// [`crate::item_preview`] to decide what to put on its stage - the
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

    /// Drop a Load into my world parked behind its confirmation (#1841) -
    /// the logout teardown's: it is the previous account's click.
    pub(crate) fn cancel_pending_load(&mut self) {
        if self.load_confirm.is_pending() {
            self.load_confirm.cancel();
        }
    }

    /// Whether a Load into my world waits on its confirmation.
    #[cfg(test)]
    pub(crate) fn load_is_pending(&self) -> bool {
        self.load_confirm.is_pending()
    }

    /// Park a Load of `slug` behind its confirmation without a draw, for
    /// the logout test.
    #[cfg(test)]
    pub(crate) fn park_load_for_test(&mut self, slug: &'static str) {
        self.load_confirm.request("Load?", "Load.", "Load", slug);
    }
}

/// Publish what the Catalogue and the Inventory are asking the item
/// preview to show (#1297, #1301), so the preview pipeline never reads the
/// egui layer.
///
/// `PreUpdate`, unconditionally: a mirror inside either window's system
/// would run only while that window is open and latch at its last value
/// the moment it closed - which for this fact means the stage keeping the
/// last selection alive, and its camera pass with it, for the rest of the
/// session. With no panels resource (before login, the headless render
/// tool) the answer is `None`; a missing browser or stash is a window with
/// nothing to bid.
///
/// The predicate lives with the CONSUMER
/// ([`crate::item_preview::wanted_subject`]) and is called from here, so
/// the rules - a closed window shows nothing, an unresolvable pick shows
/// nothing rather than the last thing that did, the most recent pick holds
/// the one stage - have one home. The stash is handed over with its change
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
/// a taxonomy word now matches every entry filed under it - "plant" returns
/// all the plants rather than only the ones with "plant" in the name - so
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
/// item to see its details." - three widgets none of which said the search
/// had come back empty, so a blank rectangle was the whole answer.
///
/// Both arms name the RULE rather than the outcome, which is #1232 f24's
/// shape: the reachable set here is a function of what `matches` looks at,
/// and that is not a thing a user can infer from "No matches."
pub(crate) fn empty_state(query: &str) -> &'static str {
    if query.trim().is_empty() {
        // Unreachable with the shipped registry (392 entries, and every
        // browse mode covers all of them - `every_mode_covers_all_entries`).
        // It is here so a build that ships an empty registry says so instead
        // of looking broken.
        "This build shipped no catalogue entries."
    } else {
        "Nothing matches that search. The Catalogue looks at names, \
         categories, roles, theme names, kinds of avatar, and a world's \
         ground and borough - try \"wearables\", \"tools\", \"boat\", \
         \"berlin\", or a theme like \"medieval\"."
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

/// An avatar entry's leaf (#1840): its slug after the `#`, like an item's.
fn avatar_leaf(parent: &str, e: &'static AvatarEntry) -> CatNode {
    CatNode::Leaf {
        id: format!("{parent}#{}", e.slug),
        name: e.name,
    }
}

/// A world entry's leaf (#1841): its slug after the `#`, like an item's.
fn world_leaf(parent: &str, e: &'static WorldEntry) -> CatNode {
    CatNode::Leaf {
        id: format!("{parent}#{}", e.slug),
        name: e.name,
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

/// Build the browse tree for `mode`, filtered by `query`: the items, then
/// the avatars (#1840), then the worlds (#1841). Empty groups are dropped,
/// so a search that matches nothing under a group hides it.
///
/// The name mode lists every leaf in one alphabetical run, an avatar or a
/// world among the items; every other mode ends with one Avatars group and
/// one Worlds group - in the Hierarchy split by family and by ground, and
/// in the theme, role and size modes flat, because an avatar has no theme,
/// and neither an avatar nor a world has a settlement role or a footprint
/// to file it by.
fn build_nodes(mode: BrowseMode, query: &str) -> Vec<CatNode> {
    let q = query.trim().to_lowercase();
    let avatars: Vec<&'static AvatarEntry> =
        AVATARS.iter().filter(|entry| entry.matches(&q)).collect();
    let worlds: Vec<&'static WorldEntry> =
        WORLDS.iter().filter(|entry| entry.matches(&q)).collect();
    if mode == BrowseMode::Name {
        let mut leaves: Vec<(&'static str, CatNode)> = ENTRIES
            .iter()
            .copied()
            .filter(|e| matches(*e, &q))
            .map(|e| (e.name(), leaf("", e)))
            .chain(avatars.iter().map(|e| (e.name, avatar_leaf("", e))))
            .chain(worlds.iter().map(|e| (e.name, world_leaf("", e))))
            .collect();
        leaves.sort_by_key(|(name, _)| *name);
        return leaves.into_iter().map(|(_, node)| node).collect();
    }
    let mut nodes = generator_nodes(mode, query);
    nodes.extend(avatar_dir(mode, &avatars));
    nodes.extend(world_dir(mode, &worlds));
    nodes
}

/// The id prefix of `mode`'s own group of `label`, so a leaf id is unique
/// per occurrence as an item's is.
fn mode_dir_id(mode: BrowseMode, label: &str) -> String {
    match mode {
        BrowseMode::Hierarchy => format!("/{label}"),
        BrowseMode::Theme => format!("t/{label}"),
        BrowseMode::Role => format!("r/{label}"),
        BrowseMode::Name | BrowseMode::Footprint => format!("s/{label}"),
    }
}

/// The Worlds group for `mode` (#1841), or `None` when no world is listed:
/// in the Hierarchy split into Procedural and Berlin, elsewhere flat.
fn world_dir(mode: BrowseMode, worlds: &[&'static WorldEntry]) -> Option<CatNode> {
    use crate::catalogue::worlds::{CATEGORY, GROUP_ORDER};
    if worlds.is_empty() {
        return None;
    }
    let id = mode_dir_id(mode, CATEGORY);
    let children = if mode == BrowseMode::Hierarchy {
        GROUP_ORDER
            .iter()
            .filter_map(|&group| {
                let group_id = format!("{id}/{group}");
                let leaves: Vec<CatNode> = worlds
                    .iter()
                    .filter(|e| e.group() == group)
                    .map(|e| world_leaf(&group_id, e))
                    .collect();
                (!leaves.is_empty()).then(|| mk_dir(group_id, group.to_string(), leaves))
            })
            .collect()
    } else {
        worlds.iter().map(|e| world_leaf(&id, e)).collect()
    };
    Some(mk_dir(id, CATEGORY.to_string(), children))
}

/// The Avatars group for `mode` (#1840), or `None` when no avatar is
/// listed. Each mode's group id carries that mode's prefix, so a leaf id is
/// unique per occurrence as an item's is.
fn avatar_dir(mode: BrowseMode, avatars: &[&'static AvatarEntry]) -> Option<CatNode> {
    if avatars.is_empty() {
        return None;
    }
    let label = crate::catalogue::avatars::CATEGORY;
    let id = mode_dir_id(mode, label);
    let children = if mode == BrowseMode::Hierarchy {
        crate::catalogue::avatars::FAMILY_ORDER
            .iter()
            .filter_map(|&family| {
                let leaves: Vec<CatNode> = avatars
                    .iter()
                    .filter(|e| e.family() == family)
                    .map(|e| {
                        avatar_leaf(
                            &format!("{id}/{}", crate::catalogue::avatars::family_group(family)),
                            e,
                        )
                    })
                    .collect();
                if leaves.is_empty() {
                    return None;
                }
                let group = crate::catalogue::avatars::family_group(family);
                Some(mk_dir(format!("{id}/{group}"), group.to_string(), leaves))
            })
            .collect()
    } else {
        avatars.iter().map(|e| avatar_leaf(&id, e)).collect()
    };
    Some(mk_dir(id, label.to_string(), children))
}

/// The items' half of [`build_nodes`]: the browse tree over
/// [`ENTRIES`] alone - what the add-from-Catalogue menus list, since an
/// avatar or a world is not an item to add.
fn generator_nodes(mode: BrowseMode, query: &str) -> Vec<CatNode> {
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
    const ROLES: [StructureRole; 6] = [
        StructureRole::Landmark,
        StructureRole::Secondary,
        StructureRole::Prop,
        StructureRole::Gateway,
        StructureRole::Monument,
        StructureRole::Port,
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
    // social-gateway placeholder) belong here too - serving every theme
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
    const ROLES: [StructureRole; 10] = [
        StructureRole::Landmark,
        StructureRole::Secondary,
        StructureRole::Prop,
        StructureRole::Gateway,
        StructureRole::Monument,
        StructureRole::Port,
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

/// What a world entry's Load into my world reaches besides the wear
/// surface's undo labels and toasts (#1841): the live world and the World
/// Editor's view of it, which the Inventory's landing replaces and
/// re-seeds; the Inventory's world loads in flight, which a Load waits
/// for; and the commands that announce the rebuild. Bundled so
/// `catalogue_ui` stays under Bevy's parameter ceiling. Both `ResMut`s are
/// written only on a confirmed Load - reads go through `Deref` - so
/// browsing never dirties the world (the guarded-dirty rule).
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct WorldSurface<'w, 's> {
    live_room: Option<ResMut<'w, crate::state::LiveRoomRecord>>,
    room_editor: Option<ResMut<'w, crate::ui::room::RoomEditorState>>,
    loads: Query<'w, 's, (), With<crate::ui::inventory::LoadWorldTask>>,
    commands: Commands<'w, 's>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn catalogue_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    mut chrome: crate::ui::layout::WindowChrome,
    mut browser: ResMut<CatalogueBrowser>,
    mut pending_drop: ResMut<PendingGeneratorDrop>,
    // Wear wiring (#1087, #1840): the Inventory's wear surface - the live
    // avatar, what its save last wrote, the Avatar editor's state, the undo
    // labels and the toasts. The live avatar and the editor are mutated
    // only on a Wear click, so the ResMuts never dirty them from mere
    // browsing (the guarded-dirty rule) - reads go through `as_ref()`.
    mut wear: crate::ui::inventory::WearSurface,
    // The inventory is the wear surface (#1096): the catalogue copies a
    // vanilla item into it. Same guarded-dirty discipline - the stash's
    // dirty state is derived live-vs-stored, so a deref_mut on click is
    // all that ever happens.
    mut live_inventory: Option<ResMut<crate::state::LiveInventoryRecord>>,
    session: Option<Res<bevy_symbios_multiuser::auth::AtprotoSession>>,
    // Whose room this is (#1220 f132): the drag tooltip promises a
    // placement only to an owner - a visitor can gift and nothing else.
    room_did: Option<Res<crate::state::CurrentRoomDid>>,
    time: Res<Time>,
    // Per-frame cache (#639): the node tree is a pure function of (mode,
    // search) over the `const ENTRIES`; rebuild only when those keys change.
    mut node_cache: Local<NodeCache>,
    // The selected entry's live picture (#1288). `Option` because the
    // resource is inserted by `item_preview`'s Startup system, so a test
    // harness that runs this system without that plugin simply draws the
    // placeholder square.
    preview: Option<Res<crate::item_preview::ItemPreview>>,
    // Load into my world (#1841): the live world, the World Editor's view
    // of it, the loads in flight and the rebuild announcement.
    mut worlds: WorldSurface,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // Guarded-dirty (#879, generalised by #1274 f177): the search field and
    // the mode combo below take `&mut` fields of this resource, and
    // `ResMut::deref_mut` stamps the change tick on ACCESS - so drawing the
    // window marked it changed on every frame whether or not anybody typed.
    // Nothing reads this resource's change tick today, and copying its string
    // in and out each frame to find that out would cost more than the tick
    // does. If a consumer is ever added, call `set_changed()` on a real edit
    // rather than deleting this line.
    let browser = browser.bypass_change_detection();
    // A world entry's Load click (#1841), carried out of the detail panel
    // to the confirmation drawn after it.
    let mut load_request: Option<&'static WorldEntry> = None;
    // Guarded-dirty (#879): `.open(&mut panels.catalogue)` through the
    // `ResMut` would mark UiPanels changed every frame, starving the
    // prefs save debounce - local copy in, write back only on close.
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
                                        // source (#832) - the natural gesture
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
                                            if drag_placeable(slug) {
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
                                // visibly live - what was missing is the
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
                // and property grid render as separate side-by-side columns -
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
                                load_request = detail_panel(
                                    ui,
                                    browser.selected.as_deref(),
                                    preview.as_deref(),
                                    live_inventory.as_mut(),
                                    &mut wear,
                                    &worlds,
                                    session.as_deref(),
                                    room_did.as_deref().map(|room| room.0.as_str()),
                                    time.elapsed_secs_f64(),
                                );
                            });
                    },
                );
            });

            // Load into my world asks first (#1841), with the Inventory's
            // own confirmation: it replaces everything in the world. The
            // reasons it cannot are asked again on the answer, which a
            // travel or an Inventory load may have overtaken.
            if let Some(entry) = load_request.take() {
                let (title, body) = crate::ui::inventory::world::load_confirm_text(entry.name);
                browser
                    .load_confirm
                    .request(title, body, "Load", entry.slug);
            }
            if let Some(slug) = browser.load_confirm.show(ui.ctx(), "catalogue-load-world")
                && let Some(entry) = world_by_slug(slug)
            {
                let now = time.elapsed_secs_f64();
                let loaded = load_world_entry(
                    entry,
                    session
                        .as_deref()
                        .map(|s| s.did.as_str())
                        .unwrap_or_default(),
                    !worlds.loads.is_empty(),
                    room_did.as_deref().map(|room| room.0.as_str()),
                    worlds.live_room.as_mut().map(|live| live.reborrow()),
                    worlds.room_editor.as_mut().map(|editor| editor.reborrow()),
                    &mut wear.undo_labels,
                    &mut wear.toasts,
                    now,
                );
                if loaded.is_ok() {
                    worlds
                        .commands
                        .insert_resource(crate::world_builder::RebuildAnnounce {
                            what: format!("Loaded \"{}\"", entry.name),
                            since_secs: now,
                        });
                }
            }
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
/// tree - Category → (Buildings) Theme → Role → item, other categories listing
/// items directly - so the menu and the browser never drift apart.
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
    // Items only (#1840): an avatar is not something to add to a tree.
    let nodes = generator_nodes(BrowseMode::Hierarchy, "");
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

/// Whether a row dragged out of the tree arms the drop bus: an item that
/// can be point-placed. An avatar entry (#1840) never does - it places
/// nothing - and neither does a slug the registry no longer answers to.
fn drag_placeable(slug: &str) -> bool {
    by_slug(slug).is_some_and(|e| is_drop_placeable(&e.build("")))
}

/// The detail panel for the selection: an item's, an avatar's (#1840), a
/// world's (#1841), or the prompt to pick one. Returns the world entry
/// whose Load into my world was clicked, for the confirmation.
#[allow(clippy::too_many_arguments)]
fn detail_panel(
    ui: &mut egui::Ui,
    selected: Option<&str>,
    preview: Option<&crate::item_preview::ItemPreview>,
    live_inventory: Option<&mut ResMut<crate::state::LiveInventoryRecord>>,
    wear: &mut crate::ui::inventory::WearSurface,
    worlds: &WorldSurface,
    session: Option<&bevy_symbios_multiuser::auth::AtprotoSession>,
    room_did: Option<&str>,
    now: f64,
) -> Option<&'static WorldEntry> {
    if let Some(entry) = selected.and_then(world_by_slug) {
        return world_detail_panel(
            ui,
            entry,
            live_inventory,
            &mut wear.toasts,
            worlds,
            session,
            room_did,
            now,
        );
    }
    if let Some(entry) = selected.and_then(by_slug) {
        item_detail(
            ui,
            entry,
            preview,
            live_inventory,
            wear.live_avatar.as_mut(),
            session,
            &mut wear.undo_labels,
            &mut wear.toasts,
            now,
        );
    } else if let Some(entry) = selected.and_then(avatar_by_slug) {
        avatar_detail(ui, entry, preview, live_inventory, wear, session, now);
    } else {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("Select an item to see its details.")
                .italics()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
        );
    }
    None
}

/// An item entry's detail: its picture, facts, and the inventory row.
#[allow(clippy::too_many_arguments)]
fn item_detail(
    ui: &mut egui::Ui,
    entry: &'static dyn CatalogueEntry,
    preview: Option<&crate::item_preview::ItemPreview>,
    live_inventory: Option<&mut ResMut<crate::state::LiveInventoryRecord>>,
    live_avatar: Option<&mut ResMut<crate::state::LiveAvatarRecord>>,
    session: Option<&bevy_symbios_multiuser::auth::AtprotoSession>,
    undo_labels: &mut crate::ui::undo::PendingUndoLabels,
    toasts: &mut crate::notify::Toasts,
    now: f64,
) {
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
            // a fitted item, that it sizes itself - the category's
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
                        crate::catalogue::WearFit::Beast { .. } => {
                            String::from("cut for a four-legged body, and sizes itself to it")
                        }
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
/// and the one surface things are worn and taken off from - so the
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
    // Reads through `as_ref` - deref_mut would count as an edit every
    // frame the panel is open (the guarded-dirty rule).
    let cap = crate::config::state::MAX_INVENTORY_ITEMS;
    let count = inventory.as_ref().0.item_count();
    if count >= cap {
        let reason = format!("Inventory full ({cap}/{cap}) - remove something first.");
        ui.add_enabled(false, egui::Button::new("Copy to inventory"))
            .on_disabled_hover_text(&reason);
        hint(ui, &reason);
        return;
    }

    // Where the copy would go - shown so "& wear" is not a surprise.
    let name = inventory.as_ref().0.free_name(entry.name());
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
        // (#1141) - including the one all three used to miss, a rigged
        // body whose wardrobe record did not resolve.
        //
        // Reads through `as_ref` for the same reason the inventory count
        // above does: deref_mut here would dirty the avatar record every
        // frame the catalogue is open.
        // The inventory is `Some` for this whole arm - the row returned
        // early with the Catalogue's own "not loaded yet" line above, which
        // is the wording `wear_blocked_reason` adopted (#1233 f261).
        let reason = crate::ui::avatar::wear_blocked_reason(
            live_avatar.as_ref().map(|live| &live.as_ref().0),
            true,
        )
        .or_else(|| {
            let beast_mm = entry.wear_fit().map_or(0, |fit| fit.beast_mm());
            crate::ui::avatar::wear_refused_reason(
                live_avatar.as_ref().map(|live| &live.as_ref().0),
                socket.name(),
                beast_mm,
            )
        });
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
            // The copy lands either way - it is a separate, already-done
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
                    format!("Wearing \"{saved}\" - it is in your inventory to adjust or take off."),
                    now,
                );
            } else {
                toasts.warn(
                    format!(
                        "Copied to inventory as \"{saved}\", but it could not be worn - \
                         wear it from the Inventory window once your body is ready."
                    ),
                    now,
                );
            }
        }
    });
}

/// Why an avatar entry could not be built: its hunt found no seed, which
/// `catalogue::avatars`' drift test forbids - so a person reads this only
/// on a build that shipped with that test failing.
const UNBUILT_AVATAR: &str = "This avatar could not be made in this version of Overlands.";

/// Why an avatar entry (#1840) cannot be worn right now, or `None` when it
/// can: what S1's own [`crate::ui::avatar::wear_avatar_blocked_reason`]
/// says - no avatar loaded, a body this build cannot read - asked of the
/// entry's cached item, never a fresh copy. There is no world gate: a
/// person can wear an avatar in anybody's world, as from the Inventory.
pub(crate) fn avatar_wear_blocked(
    entry: &AvatarEntry,
    live: Option<&crate::pds::AvatarRecord>,
) -> Option<String> {
    let Some(built) = entry.built() else {
        return Some(UNBUILT_AVATAR.to_string());
    };
    match &built.item {
        Err(reason) => Some(reason.clone()),
        Ok(item) => crate::ui::avatar::wear_avatar_blocked_reason(live, item).map(str::to_string),
    }
}

/// Why an avatar entry (#1840) cannot be copied to the Inventory right
/// now, or `None` when it can: signed out, or what S1's
/// [`crate::ui::avatar::save_to_inventory_blocked_reason`] says - the
/// inventory has not loaded, it is full, or the body has nothing to copy.
pub(crate) fn avatar_copy_blocked(
    entry: &AvatarEntry,
    signed_in: bool,
    inventory: Option<&crate::pds::InventoryRecord>,
) -> Option<String> {
    if !signed_in {
        return Some(String::from("Sign in to own items."));
    }
    let Some(built) = entry.built() else {
        return Some(UNBUILT_AVATAR.to_string());
    };
    crate::ui::avatar::save_to_inventory_blocked_reason(&built.record, inventory)
}

/// An avatar entry's detail (#1840): its picture - or why there is none -
/// what it is, and the Wear and Copy to inventory row. Selecting the entry
/// is what builds it; every later frame reads the entry's cache.
fn avatar_detail(
    ui: &mut egui::Ui,
    entry: &'static AvatarEntry,
    preview: Option<&crate::item_preview::ItemPreview>,
    live_inventory: Option<&mut ResMut<crate::state::LiveInventoryRecord>>,
    wear: &mut crate::ui::inventory::WearSurface,
    session: Option<&bevy_symbios_multiuser::auth::AtprotoSession>,
    now: f64,
) {
    let weak = crate::ui::theme::current(ui.ctx()).text_weak;
    crate::ui::item_picture::draw_preview(
        ui,
        preview,
        crate::ui::item_picture::PictureOf::CatalogueAvatar(entry.slug),
        crate::ui::item_picture::CATALOGUE_SIDE,
    );
    ui.heading(entry.name);
    ui.add(egui::Label::new(entry.description).wrap());
    ui.add_space(4.0);
    ui.separator();

    let built = entry.built();
    egui::Grid::new("catalogue_avatar_grid")
        .num_columns(2)
        .spacing([10.0, 4.0])
        .max_col_width(280.0)
        .show(ui, |ui| {
            let row = |ui: &mut egui::Ui, k: &str, v: String| {
                ui.label(egui::RichText::new(k).strong());
                ui.add(egui::Label::new(v).wrap());
                ui.end_row();
            };
            row(
                ui,
                "Category",
                crate::catalogue::avatars::CATEGORY.to_string(),
            );
            row(ui, "Family", entry.family_label().to_string());
            if let Some(craft) = entry.craft_type() {
                row(ui, "Craft", craft.label().to_string());
            }
            if let Some(built) = built {
                row(
                    ui,
                    "Locomotion",
                    built.record.locomotion.display_label().to_string(),
                );
                if let Some(parts) = built.parts {
                    row(
                        ui,
                        "Parts",
                        format!("{parts} {}", crate::text::plural(parts, "part", "parts")),
                    );
                }
            }
            ui.label(egui::RichText::new("Slug").strong());
            ui.label(egui::RichText::new(entry.slug).monospace().small());
            ui.end_row();
        });

    ui.add_space(6.0);
    ui.separator();

    // Reads through `as_ref` - deref_mut would count as an edit every frame
    // the panel is open (the guarded-dirty rule).
    let wear_blocked = avatar_wear_blocked(
        entry,
        wear.live_avatar.as_ref().map(|live| &live.as_ref().0),
    );
    let copy_blocked = avatar_copy_blocked(
        entry,
        session.is_some(),
        live_inventory.as_ref().map(|live| &live.as_ref().0),
    );
    // Where the copy would go - named on the hover, as the item row does.
    let copy_name = live_inventory
        .as_ref()
        .map(|live| live.as_ref().0.free_name(entry.name))
        .unwrap_or_else(|| entry.name.to_string());
    let did = session.map(|s| s.did.as_str()).unwrap_or_default();

    let mut wear_clicked = false;
    let mut copy_clicked = false;
    ui.horizontal(|ui| {
        wear_clicked = ui
            .add_enabled(wear_blocked.is_none(), egui::Button::new("Wear"))
            .on_hover_text(
                "Wear this avatar instead of the one you have on - it stays unsaved \
                 until you Save in the Avatar window",
            )
            .on_disabled_hover_text(wear_blocked.as_deref().unwrap_or_default())
            .clicked();
        copy_clicked = ui
            .add_enabled(
                copy_blocked.is_none(),
                egui::Button::new("Copy to inventory"),
            )
            .on_hover_text(format!("Add a copy to your inventory as \"{copy_name}\""))
            .on_disabled_hover_text(copy_blocked.as_deref().unwrap_or_default())
            .clicked();
    });
    // One string, two surfaces (#1289): the hover a pointer reaches for,
    // and the line a reader sees. Each reason once.
    let hint = |ui: &mut egui::Ui, text: &str| {
        ui.add(egui::Label::new(egui::RichText::new(text).small().color(weak)).wrap());
    };
    if let Some(reason) = &wear_blocked {
        hint(ui, &format!("Wear: {reason}"));
    }
    if let Some(reason) = &copy_blocked {
        hint(ui, &format!("Copy: {reason}"));
    }

    if wear_clicked {
        // The toast has already said how it went.
        let _ = wear_avatar_entry(entry, wear, did, now);
    }
    if copy_clicked && let Some(inventory) = live_inventory {
        // A `ResMut`'s `deref_mut` stamps the stash's tick: on the click,
        // and only there. The toast has already said how it went.
        let _ = copy_avatar_entry(entry, &mut inventory.0, &mut wear.toasts, now);
    }
}

/// Wear an avatar entry (#1840) through the Inventory's own path
/// ([`crate::ui::avatar::wear_avatar_on_live`]): the built entry's item
/// replaces the live avatar in one write and one undo step, unsaved until
/// the Avatar editor's Save. `Ok` when it was put on. The button is
/// disabled for every refusal it can foresee; each one also toasts, and is
/// the `Err` - the sentence the agent client answers with (#1843).
pub(crate) fn wear_avatar_entry(
    entry: &AvatarEntry,
    wear: &mut crate::ui::inventory::WearSurface,
    did: &str,
    now: f64,
) -> Result<(), String> {
    let item = match entry.built().map(|built| &built.item) {
        Some(Ok(item)) => item,
        Some(Err(reason)) => {
            wear.toasts
                .warn(format!("Could not wear \"{}\" - {reason}", entry.name), now);
            return Err(reason.clone());
        }
        None => {
            wear.toasts.warn(UNBUILT_AVATAR.to_string(), now);
            return Err(UNBUILT_AVATAR.to_string());
        }
    };
    let Some(live) = wear.live_avatar.as_mut() else {
        const NO_AVATAR: &str = "No avatar loaded yet.";
        wear.toasts.warn(NO_AVATAR.to_string(), now);
        return Err(NO_AVATAR.to_owned());
    };
    crate::ui::avatar::wear_avatar_on_live(
        item,
        entry.name,
        live.reborrow(),
        wear.stored_avatar.as_deref().map(|stored| &stored.0),
        &mut wear.avatar_editor,
        did,
        &mut wear.undo_labels,
        &mut wear.toasts,
        now,
    )
}

/// Copy an avatar entry (#1840) into `inventory` through S1's insert
/// ([`crate::ui::avatar::save_avatar_to_inventory_as`]): the built record
/// as an avatar item, under a free name built on the entry's, for the
/// Inventory's Save to keep. Toasts where it landed, or why it did not.
pub(crate) fn copy_avatar_entry(
    entry: &AvatarEntry,
    inventory: &mut crate::pds::InventoryRecord,
    toasts: &mut crate::notify::Toasts,
    now: f64,
) -> Result<String, String> {
    let copied = match entry.built() {
        Some(built) => crate::ui::avatar::save_avatar_to_inventory_as(
            &built.record,
            Some(entry.name),
            inventory,
        ),
        None => Err(UNBUILT_AVATAR.to_string()),
    };
    match &copied {
        Ok(name) => toasts.success(format!("Copied to inventory as \"{name}\"."), now),
        Err(reason) => toasts.warn(format!("Could not copy \"{}\" - {reason}", entry.name), now),
    }
    copied
}

/// Why a world entry could not be built: its hunt found no seed, which
/// `catalogue::worlds`' drift test forbids - so a person reads this only on
/// a build that shipped with that test failing.
const UNBUILT_WORLD: &str = "This world could not be made in this version of Overlands.";

/// Why Load into my world is refused while the owner's world has not
/// arrived - the Inventory's landing says the same, lower-cased.
pub(crate) const WORLD_NOT_LOADED: &str = "Your world has not loaded yet - wait for it to arrive.";

/// Why a world entry (#1841) cannot be loaded into the owner's world right
/// now, or `None` when it can: the Inventory's own reasons
/// ([`crate::ui::inventory::world::world_load_blocked_reason`]) - your own
/// world only, one load at a time - and, as its landing refuses, a world
/// that has not loaded yet.
pub(crate) fn world_entry_load_blocked(
    owns_world: bool,
    loading: bool,
    world_loaded: bool,
) -> Option<&'static str> {
    crate::ui::inventory::world::world_load_blocked_reason(owns_world, loading)
        .or_else(|| (!world_loaded).then_some(WORLD_NOT_LOADED))
}

/// Why a world entry (#1841) cannot be copied to the Inventory right now,
/// or `None` when it can: signed out, or what the World Editor's Save to
/// Inventory says ([`crate::ui::room::stash::save_world_blocked_reason`]) -
/// the inventory has not loaded or is full, or the sign-in predates the
/// grant a world item's parts need. That the built world is past what an
/// item holds is known only once it is built, so the click says it.
pub(crate) fn world_copy_blocked(
    signed_in: bool,
    inventory: Option<&crate::pds::InventoryRecord>,
    parts_granted: bool,
) -> Option<String> {
    if !signed_in {
        return Some(String::from("Sign in to own items."));
    }
    crate::ui::room::stash::save_world_blocked_reason(inventory, parts_granted, None)
}

/// The world `entry` builds for `did`, held as an editor gesture leaves
/// the world ([`crate::ui::room::settle_edit`], #1707) - as the re-roll's
/// flush and the Inventory's load hold theirs. The whole procedural
/// pipeline: only on a click.
pub(crate) fn built_world(entry: &WorldEntry, did: &str) -> Option<crate::pds::RoomRecord> {
    let (_, mut world) = entry.build(did)?;
    crate::ui::room::settle_edit(&mut world);
    Some(world)
}

/// Load a world entry (#1841) into `did`'s world: refused, building
/// nothing, for the reasons [`world_entry_load_blocked`] gives; else said in
/// a toast BEFORE the build - the build is the stall, as the re-roll's is -
/// then built ([`built_world`]) and landed through the Inventory's own
/// landing ([`crate::ui::inventory::world::land_loaded_world`]), which
/// replaces the world through [`crate::ui::room::replace_world`]: one write,
/// one undo step labelled for the entry, unsaved until the World Editor's
/// Save. Toasts how it went; the caller announces the rebuild.
#[allow(clippy::too_many_arguments)]
pub(crate) fn load_world_entry(
    entry: &WorldEntry,
    did: &str,
    loading: bool,
    room_did: Option<&str>,
    live: Option<Mut<'_, crate::state::LiveRoomRecord>>,
    editor: Option<Mut<'_, crate::ui::room::RoomEditorState>>,
    undo_labels: &mut crate::ui::undo::PendingUndoLabels,
    toasts: &mut crate::notify::Toasts,
    now: f64,
) -> Result<(), String> {
    let name = entry.name;
    let owns_world = !did.is_empty() && room_did == Some(did);
    if let Some(reason) = world_entry_load_blocked(owns_world, loading, live.is_some()) {
        toasts.warn(format!("Could not load \"{name}\" - {reason}"), now);
        return Err(reason.to_owned());
    }
    toasts.info(format!("Loading \"{name}\" into your world\u{2026}"), now);
    let Some(world) = built_world(entry, did) else {
        toasts.warn(UNBUILT_WORLD.to_string(), now);
        return Err(UNBUILT_WORLD.to_owned());
    };
    let landed = crate::ui::inventory::world::land_loaded_world(
        name,
        did,
        Ok(world),
        room_did,
        live,
        editor,
        undo_labels,
    );
    match &landed {
        Ok(()) => toasts.success(
            format!(
                "Loaded \"{name}\" into your world - Save in the World Editor to keep it, or \
                 undo it."
            ),
            now,
        ),
        Err(reason) => toasts.warn(format!("Could not load \"{name}\" - {reason}"), now),
    }
    landed
}

/// Copy a world entry (#1841) into `inventory`: refused, building nothing,
/// for the reasons the World Editor's Save to Inventory gives; else said in
/// a toast before the build, built for `did` ([`built_world`]) and stashed
/// through that Save's own snapshot path
/// ([`crate::ui::room::stash::save_world_to_inventory_as`]) under a free
/// name built on the entry's, for the Inventory's Save to keep. Toasts
/// where it landed, or why it did not.
pub(crate) fn copy_world_entry(
    entry: &WorldEntry,
    did: &str,
    inventory: &mut crate::pds::InventoryRecord,
    parts_granted: bool,
    toasts: &mut crate::notify::Toasts,
    now: f64,
) -> Result<String, String> {
    let copied = match crate::ui::room::stash::save_world_blocked_reason(
        Some(inventory),
        parts_granted,
        None,
    ) {
        Some(reason) => Err(reason),
        None => {
            toasts.info(
                format!("Making \"{}\" for your inventory\u{2026}", entry.name),
                now,
            );
            match built_world(entry, did) {
                Some(world) => crate::ui::room::stash::save_world_to_inventory_as(
                    &world,
                    Some(entry.name),
                    inventory,
                    parts_granted,
                ),
                None => Err(UNBUILT_WORLD.to_owned()),
            }
        }
    };
    match &copied {
        Ok(name) => toasts.success(
            format!(
                "Copied to inventory as \"{name}\" - load it from there, and Save there to keep \
                 it."
            ),
            now,
        ),
        Err(reason) => toasts.warn(format!("Could not copy \"{}\" - {reason}", entry.name), now),
    }
    copied
}

/// A click on a world entry's detail (#1841).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorldClick {
    /// Load into my world - asked first.
    Load,
    /// Copy to inventory.
    Copy,
}

/// A world entry's detail (#1841): a text card, no picture - the sentence
/// the Inventory's pane gives a world item - read off the entry's pins
/// alone, so drawing it builds nothing; and the Load into my world and Copy
/// to inventory row, each disabled with its reason on the hover and painted
/// under the row (#1289). Which was clicked.
pub(crate) fn world_detail(
    ui: &mut egui::Ui,
    entry: &WorldEntry,
    load_blocked: Option<&str>,
    copy_blocked: Option<&str>,
    copy_name: &str,
) -> Option<WorldClick> {
    use crate::catalogue::worlds::Ground;
    let weak = crate::ui::theme::current(ui.ctx()).text_weak;
    ui.add(
        egui::Label::new(
            egui::RichText::new(crate::ui::inventory::browser::WORLD_NO_PICTURE)
                .small()
                .color(weak),
        )
        .wrap(),
    );
    ui.heading(entry.name);
    ui.add(egui::Label::new(entry.description).wrap());
    ui.add_space(4.0);
    ui.separator();

    egui::Grid::new("catalogue_world_grid")
        .num_columns(2)
        .spacing([10.0, 4.0])
        .max_col_width(280.0)
        .show(ui, |ui| {
            let row = |ui: &mut egui::Ui, k: &str, v: &str| {
                ui.label(egui::RichText::new(k).strong());
                ui.add(egui::Label::new(v).wrap());
                ui.end_row();
            };
            row(ui, "Category", crate::catalogue::worlds::CATEGORY);
            row(ui, "Kind", entry.kind_label());
            row(ui, "Theme", entry.theme.label());
            match entry.ground {
                Ground::Procedural { landform, biome } => {
                    row(ui, "Landform", landform.label());
                    row(ui, "Biome", biome.label());
                }
                Ground::Berlin(borough) => row(ui, "Borough", borough.name()),
            }
            ui.label(egui::RichText::new("Slug").strong());
            ui.label(egui::RichText::new(entry.slug).monospace().small());
            ui.end_row();
        });

    ui.add_space(6.0);
    ui.separator();

    let mut click = None;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                load_blocked.is_none(),
                egui::Button::new("Load into my world"),
            )
            .on_hover_text(
                "Replace everything in your world with this one, as a re-roll does - it asks \
                 first, can be undone, and stays unsaved until you Save in the World Editor",
            )
            .on_disabled_hover_text(load_blocked.unwrap_or_default())
            .clicked()
        {
            click = Some(WorldClick::Load);
        }
        if ui
            .add_enabled(
                copy_blocked.is_none(),
                egui::Button::new("Copy to inventory"),
            )
            .on_hover_text(format!(
                "Make this world and add it to your inventory as \"{copy_name}\""
            ))
            .on_disabled_hover_text(copy_blocked.unwrap_or_default())
            .clicked()
        {
            click = Some(WorldClick::Copy);
        }
    });
    // One string, two surfaces (#1289): the hover a pointer reaches for,
    // and the line a reader sees. Each reason once.
    let hint = |ui: &mut egui::Ui, text: &str| {
        ui.add(egui::Label::new(egui::RichText::new(text).small().color(weak)).wrap());
    };
    if let Some(reason) = load_blocked {
        hint(ui, &format!("Load: {reason}"));
    }
    if let Some(reason) = copy_blocked {
        hint(ui, &format!("Copy: {reason}"));
    }
    click
}

/// A world entry's detail in the Catalogue window (#1841): the reasons its
/// two actions are disabled, read through `Deref` every frame - nothing is
/// built or written until a click - then [`world_detail`]. A Copy click
/// copies at once; a Load click is returned for the confirmation.
#[allow(clippy::too_many_arguments)]
fn world_detail_panel(
    ui: &mut egui::Ui,
    entry: &'static WorldEntry,
    live_inventory: Option<&mut ResMut<crate::state::LiveInventoryRecord>>,
    toasts: &mut crate::notify::Toasts,
    worlds: &WorldSurface,
    session: Option<&bevy_symbios_multiuser::auth::AtprotoSession>,
    room_did: Option<&str>,
    now: f64,
) -> Option<&'static WorldEntry> {
    let did = session.map(|s| s.did.as_str());
    let owns_world = did.is_some() && did == room_did;
    let load_blocked = world_entry_load_blocked(
        owns_world,
        !worlds.loads.is_empty(),
        worlds.live_room.is_some(),
    );
    let parts_granted = session.is_some_and(|s| {
        crate::pds::inventory::world::grant_covers_parts(&s.session.token_set().scope)
    });
    let inventory = live_inventory.as_ref().map(|live| &live.as_ref().0);
    let copy_blocked = world_copy_blocked(session.is_some(), inventory, parts_granted);
    // Where the copy would go - named on the hover, as the item row does.
    let copy_name = inventory
        .map(|inventory| inventory.free_name(entry.name))
        .unwrap_or_else(|| entry.name.to_string());
    match world_detail(ui, entry, load_blocked, copy_blocked.as_deref(), &copy_name)? {
        WorldClick::Load => Some(entry),
        WorldClick::Copy => {
            if let (Some(inventory), Some(session)) = (live_inventory, session) {
                // A `ResMut`'s `deref_mut` stamps the stash's tick: on the
                // click, and only there. The toast has said how it went.
                let _ = copy_world_entry(
                    entry,
                    &session.did,
                    &mut inventory.0,
                    parts_granted,
                    toasts,
                    now,
                );
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// #1297's item-preview singleton, the `PlacementFocus` shape a
    /// second time, and since #1301 the Inventory's selection beside the
    /// Catalogue's. `restage_preview` read `UiPanels` and
    /// `CatalogueBrowser` directly to answer one question - "is anyone
    /// looking, and at what" - which pointed the arrow from a render
    /// pipeline into the egui layer for a `bool` and an `Option<&str>`.
    ///
    /// Asserted against the PREDICATE the consumer already owned
    /// ([`crate::item_preview::wanted_subject`]) rather than by restating
    /// its answers, so the mirror cannot drift from the rule it carries -
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

    /// #1301, counting the work (#1270's rule). The stage restages when -
    /// and only when - the request differs from what it holds, and an
    /// Inventory request carries the stash's change tick. So an item held
    /// selected must cost no respawn per frame, and an edit to the stash
    /// exactly one. Driven through the REAL mirror and the REAL
    /// `restage_preview` over a world with the whole spawn path in it; the
    /// counter is `item_preview`'s own, as `NODES_BUILT` is the tree's.
    ///
    /// There is no generator-compare count to take because there is no
    /// compare: the subject holds a name and a tick, never a `Generator`,
    /// and nothing on this path compares one - which is the design, since
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

    /// #1838. A vehicle avatar item is pictured from its visuals tree by
    /// the same stage a generator item uses; a rigged one is asked for as a
    /// body to build (#1842; the body is `item_preview::body_tests`').
    /// Through the REAL mirror and the REAL `restage_preview`.
    #[test]
    fn a_vehicle_avatar_item_is_pictured_and_a_rigged_one_is_asked_for() {
        use bevy::ecs::system::RunSystemOnce;
        use bevy::prelude::*;

        use crate::item_preview::{
            PreviewCamera, PreviewRequest, PreviewSubject, preview_for_test, restage_preview,
            take_restage_count,
        };
        use crate::pds::InventoryRecord;
        use crate::pds::inventory::AvatarItem;
        use crate::pds::inventory::avatar_item_tests::{generator_record, rigged_record};
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
        stash.put_avatar(
            "ride".into(),
            AvatarItem::from_record(&generator_record()).expect("copies"),
        );
        stash.put_avatar(
            "me".into(),
            AvatarItem::from_record(&rigged_record()).expect("copies"),
        );
        world.insert_resource(LiveInventoryRecord(stash));
        let mut browser = InventoryBrowser::default();
        browser.select("ride", 1.0);
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
        assert!(
            matches!(
                &world.resource::<PreviewRequest>().0,
                Some(PreviewSubject::Inventory { name, .. }) if name == "ride"
            ),
            "the vehicle is asked for"
        );
        assert_eq!(take_restage_count(), 1, "and staged");

        world.resource_mut::<InventoryBrowser>().select("me", 2.0);
        frame(world);
        assert!(
            matches!(
                &world.resource::<PreviewRequest>().0,
                Some(PreviewSubject::Inventory { name, .. }) if name == "me"
            ),
            "a person is asked for (#1842)"
        );
        assert_eq!(take_restage_count(), 1, "and restaged once");
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

    /// The app's own instruction - "copy a wearable from the Catalogue
    /// first" - has to find something when it is typed into the Catalogue
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
        // registry in every mode - no entry is ever unreachable.
        // The avatars (#1840) and the worlds (#1841) are listed in every
        // mode too.
        let want: HashSet<&str> = ENTRIES
            .iter()
            .map(|e| e.slug())
            .chain(AVATARS.iter().map(|e| e.slug))
            .chain(WORLDS.iter().map(|e| e.slug))
            .collect();
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
        // not once per theme - so its slug count across the tree is 1.
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
        // Every surviving leaf matches the query - a world (#1841) by its
        // theme.
        for s in &slugs {
            if let Some(world) = world_by_slug(s) {
                assert!(world.matches("medieval"), "{s}");
                continue;
            }
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
                CatNode::Leaf { id, .. } => {
                    let slug = leaf_slug(id).unwrap();
                    by_slug(slug)
                        .map(|e| e.name())
                        .or_else(|| avatar_by_slug(slug).map(|e| e.name))
                        .or_else(|| world_by_slug(slug).map(|e| e.name))
                        .unwrap()
                }
                _ => unreachable!(),
            })
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
        assert!(
            names.contains(&"Sloop"),
            "an avatar is listed among the items"
        );
        assert!(
            names.contains(&"Medieval world"),
            "a world is listed among the items"
        );
    }

    // ---- Avatars in the Catalogue (#1840) ----

    fn avatar(slug: &str) -> &'static AvatarEntry {
        avatar_by_slug(slug).expect("a shipped avatar entry")
    }

    /// The Hierarchy files every avatar under Avatars, then its family; the
    /// add-from-Catalogue menus list none of them; and search finds them by
    /// name and by family.
    #[test]
    fn the_browse_tree_and_search_list_the_avatars() {
        let nodes = build_nodes(BrowseMode::Hierarchy, "");
        let Some(CatNode::Dir {
            label, children, ..
        }) = nodes.iter().rev().nth(1)
        else {
            panic!("the Avatars group comes before the Worlds group");
        };
        assert_eq!(label, crate::catalogue::avatars::CATEGORY);
        for family_dir in children {
            let CatNode::Dir {
                label, children, ..
            } = family_dir
            else {
                panic!("the level under Avatars is the family");
            };
            for leaf in children {
                let CatNode::Leaf { id, .. } = leaf else {
                    panic!("a family lists its entries");
                };
                let entry = avatar(leaf_slug(id).unwrap());
                assert_eq!(
                    crate::catalogue::avatars::family_group(entry.family()),
                    label
                );
            }
        }
        let filed: HashSet<&str> = all_slugs(children).into_iter().collect();
        let all: HashSet<&str> = AVATARS.iter().map(|e| e.slug).collect();
        assert_eq!(filed, all, "every avatar, once");

        // The menus add items, and an avatar is not one.
        let menu = generator_nodes(BrowseMode::Hierarchy, "");
        assert!(
            all_slugs(&menu)
                .iter()
                .all(|slug| avatar_by_slug(slug).is_none()),
            "the add menu lists an avatar"
        );

        // By name, and by family.
        let hits = |q: &str| -> HashSet<String> {
            all_slugs(&build_nodes(BrowseMode::Name, q))
                .into_iter()
                .filter(|slug| avatar_by_slug(slug).is_some())
                .map(str::to_string)
                .collect()
        };
        assert!(hits("sloop").contains("avatar_boat_sloop"));
        assert!(hits("Tandem").contains("avatar_helicopter_tandem"));
        for (query, family) in [
            ("beast", crate::seeded_defaults::ChassisFamily::Beast),
            ("airship", crate::seeded_defaults::ChassisFamily::Airship),
            ("people", crate::seeded_defaults::ChassisFamily::Humanoid),
        ] {
            let found = hits(query);
            for entry in AVATARS.iter().filter(|e| e.family() == family) {
                assert!(found.contains(entry.slug), "{query} misses {}", entry.slug);
            }
        }
        assert_eq!(hits("avatar").len(), AVATARS.len(), "the section's word");
        assert!(hits("medieval").is_empty(), "an avatar has no theme");
    }

    /// An avatar entry places nothing, so dragging its row never arms the
    /// drop bus.
    #[test]
    fn an_avatar_entry_is_not_a_drag_source() {
        for entry in &AVATARS {
            assert!(!drag_placeable(entry.slug), "{} arms a drop", entry.slug);
        }
        assert!(
            ENTRIES.iter().any(|e| drag_placeable(e.slug())),
            "the control: an item does"
        );
    }

    /// Every reason Wear and Copy to inventory are disabled is one these
    /// answer, in S1's words.
    #[test]
    fn the_avatar_buttons_say_why_they_are_disabled() {
        use crate::pds::InventoryRecord;
        use crate::pds::inventory::avatar_item_tests::rigged_record;
        let entry = avatar("avatar_beast_dog");

        assert_eq!(
            avatar_wear_blocked(entry, None).as_deref(),
            Some("No avatar loaded yet.")
        );
        assert_eq!(avatar_wear_blocked(entry, Some(&rigged_record())), None);

        assert!(
            avatar_copy_blocked(entry, false, Some(&InventoryRecord::default()))
                .is_some_and(|why| why.contains("Sign in"))
        );
        assert!(
            avatar_copy_blocked(entry, true, None).is_some_and(|why| why.contains("not loaded"))
        );
        assert_eq!(
            avatar_copy_blocked(entry, true, Some(&InventoryRecord::default())),
            None
        );
        let mut full = InventoryRecord::default();
        for i in 0..crate::config::state::MAX_INVENTORY_ITEMS {
            full.put_item(format!("g{i}"), Generator::default_cuboid(), None);
        }
        assert!(
            avatar_copy_blocked(entry, true, Some(&full)).is_some_and(|why| why.contains("full"))
        );
    }

    /// Copy to inventory inserts exactly S1's copy of the built record,
    /// under a free name built on the entry's, and refuses when full.
    #[test]
    fn copy_to_inventory_inserts_the_built_avatar_under_a_free_name() {
        use crate::pds::InventoryRecord;
        use crate::pds::inventory::AvatarItem;
        let entry = avatar("avatar_boat_sloop");
        let want = AvatarItem::from_record(entry.record().expect("builds")).expect("copies");
        let mut inventory = InventoryRecord::default();
        inventory.put_item("Sloop".into(), Generator::default_cuboid(), None);
        let mut toasts = crate::notify::Toasts::default();

        let name = copy_avatar_entry(entry, &mut inventory, &mut toasts, 0.0).expect("copies");
        assert_eq!(name, "Sloop_1", "free across every kind of item");
        assert_eq!(inventory.avatars[&name], want);

        let person = avatar("avatar_person_older_man");
        let name = copy_avatar_entry(person, &mut inventory, &mut toasts, 0.0).expect("copies");
        assert_eq!(name, person.name, "named for the entry, not \"Wanderer\"");
        assert_eq!(
            inventory.avatars[&name],
            AvatarItem::from_record(person.record().unwrap()).unwrap()
        );

        let cap = crate::config::state::MAX_INVENTORY_ITEMS;
        let mut i = 0;
        while inventory.item_count() < cap {
            inventory.put_item(format!("g{i}"), Generator::default_cuboid(), None);
            i += 1;
        }
        let refused = copy_avatar_entry(entry, &mut inventory, &mut toasts, 0.0).expect_err("full");
        assert!(refused.contains("full"), "{refused}");
        assert_eq!(inventory.item_count(), cap);
    }

    /// Wear from the Catalogue is S1's wear: one undo step, and the live
    /// record wears the built avatar - its tree and locomotion for a
    /// vehicle, its engine body for a person.
    #[test]
    fn wearing_an_avatar_entry_is_one_undo_step_of_the_built_avatar() {
        use crate::pds::inventory::avatar_item_tests::rigged_record;
        use crate::state::LiveAvatarRecord;
        use crate::ui::inventory::WearSurface;
        use crate::ui::undo::{AvatarUndoHistory, PendingUndoLabels};
        use bevy::ecs::system::RunSystemOnce;

        let old = rigged_record();
        let mut app = App::new();
        app.insert_resource(LiveAvatarRecord(old.clone()));
        app.init_resource::<crate::ui::avatar::AvatarEditorState>();
        app.init_resource::<AvatarUndoHistory>();
        app.init_resource::<PendingUndoLabels>();
        app.init_resource::<crate::notify::Toasts>();
        app.add_systems(Update, crate::ui::undo::capture_avatar_history);
        app.update();
        let before = app.world().resource::<AvatarUndoHistory>().len();

        let wear_entry = |app: &mut App, slug: &'static str| -> bool {
            app.world_mut()
                .run_system_once(move |mut wear: WearSurface| {
                    wear_avatar_entry(avatar(slug), &mut wear, "did:plc:catalogue", 0.0).is_ok()
                })
                .expect("the wear runs")
        };
        assert!(wear_entry(&mut app, "avatar_airship_zeppelin"));
        app.update();
        app.update();
        let entry = avatar("avatar_airship_zeppelin");
        let built = entry.record().unwrap();
        {
            let live = &app.world().resource::<LiveAvatarRecord>().0;
            assert_eq!(live.body.visuals(), entry.visuals(), "the built tree");
            assert_eq!(live.locomotion, built.locomotion);
        }
        let mut history = app.world_mut().resource_mut::<AvatarUndoHistory>();
        assert_eq!(history.len(), before + 1, "one undo step");
        let (undone, _) = history.undo().expect("undoable");
        assert_eq!(undone.record, old, "and one undo takes it off again");

        // Wearing it again is refused as already on, and writes nothing.
        let tick = app
            .world()
            .resource_ref::<LiveAvatarRecord>()
            .last_changed();
        app.world_mut().increment_change_tick();
        assert!(!wear_entry(&mut app, "avatar_airship_zeppelin"));
        assert_eq!(
            app.world()
                .resource_ref::<LiveAvatarRecord>()
                .last_changed(),
            tick,
            "a refusal stamps no tick"
        );

        // A person wears the engine body the entry built, resolved at once.
        assert!(wear_entry(&mut app, "avatar_person_young_woman"));
        let person = avatar("avatar_person_young_woman");
        let live = &app.world().resource::<LiveAvatarRecord>().0;
        let worn = crate::catalogue::avatars::engine_body(live).expect("resolved");
        assert_eq!(
            Some(worn),
            crate::catalogue::avatars::engine_body(person.record().unwrap())
        );
    }

    /// A vehicle entry is pictured from the tree it builds, by the same
    /// stage an item uses; a person or a beast is asked for too (#1842),
    /// staged as a body to build, which spawns nothing at once. An idle
    /// selection builds and restages nothing more. Through the REAL mirror
    /// and the REAL `restage_preview`; the body itself is
    /// `item_preview::body_tests`'.
    #[test]
    fn a_vehicle_avatar_entry_is_staged_and_a_rigged_one_is_asked_for() {
        use crate::item_preview::{
            PreviewCamera, PreviewRequest, PreviewSubject, preview_for_test, restage_preview,
            take_restage_count,
        };
        use crate::ui::toolbar::UiPanels;
        use bevy::ecs::system::RunSystemOnce;

        let mut app = crate::player::visuals::spawn_path_app();
        let world = app.world_mut();
        world.init_resource::<PreviewRequest>();
        world.insert_resource(UiPanels {
            catalogue: true,
            ..Default::default()
        });
        let mut browser = CatalogueBrowser::default();
        browser.select_for_test(Some("avatar_skiff_rover"), 1.0);
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
        crate::catalogue::avatars::take_build_count();
        frame(world);
        assert_eq!(
            world.resource::<PreviewRequest>().0,
            Some(PreviewSubject::CatalogueAvatar("avatar_skiff_rover".into())),
            "the vehicle is asked for"
        );
        assert_eq!(take_restage_count(), 1, "and staged");
        assert!(avatar("avatar_skiff_rover").is_built());
        assert!(
            crate::catalogue::avatars::take_build_count() <= 1,
            "selecting builds the entry at most once"
        );
        for _ in 0..30 {
            frame(world);
        }
        assert_eq!(
            take_restage_count(),
            0,
            "thirty idle frames restage nothing"
        );
        assert_eq!(
            crate::catalogue::avatars::take_build_count(),
            0,
            "and build nothing"
        );

        world
            .resource_mut::<CatalogueBrowser>()
            .select_for_test(Some("avatar_beast_horse"), 2.0);
        frame(world);
        assert_eq!(
            world.resource::<PreviewRequest>().0,
            Some(PreviewSubject::CatalogueAvatar("avatar_beast_horse".into())),
            "a beast is asked for (#1842)"
        );
        assert_eq!(take_restage_count(), 1, "and restaged once");
        for _ in 0..30 {
            frame(world);
        }
        assert_eq!(take_restage_count(), 0, "and then left alone");
    }

    // ---- Worlds in the Catalogue (#1841) ----

    fn world(slug: &str) -> &'static WorldEntry {
        world_by_slug(slug).expect("a shipped world entry")
    }

    /// The Hierarchy closes with Worlds, then Procedural and Berlin, every
    /// world once under its own group; the other modes list them in one
    /// flat group; the add-from-Catalogue menus list none of them; and
    /// search finds them by name, theme, landform, biome and borough.
    #[test]
    fn the_browse_tree_and_search_list_the_worlds() {
        use crate::catalogue::worlds::{BERLIN_GROUP, CATEGORY, PROCEDURAL_GROUP};
        let nodes = build_nodes(BrowseMode::Hierarchy, "");
        let Some(CatNode::Dir {
            label, children, ..
        }) = nodes.last()
        else {
            panic!("the Worlds group closes the tree");
        };
        assert_eq!(label, CATEGORY);
        let groups: Vec<&str> = children
            .iter()
            .map(|node| match node {
                CatNode::Dir { label, .. } => label.as_str(),
                CatNode::Leaf { .. } => panic!("the level under Worlds is the ground"),
            })
            .collect();
        assert_eq!(groups, [PROCEDURAL_GROUP, BERLIN_GROUP]);
        for group in children {
            let CatNode::Dir {
                label, children, ..
            } = group
            else {
                unreachable!();
            };
            for leaf in children {
                let CatNode::Leaf { id, .. } = leaf else {
                    panic!("a ground lists its entries");
                };
                assert_eq!(world(leaf_slug(id).unwrap()).group(), label);
            }
        }
        let filed: Vec<&str> = all_slugs(children);
        let all: Vec<&str> = WORLDS.iter().map(|e| e.slug).collect();
        assert_eq!(filed, all, "every world once, in browse order");

        for mode in [BrowseMode::Theme, BrowseMode::Role, BrowseMode::Footprint] {
            let nodes = build_nodes(mode, "");
            let Some(CatNode::Dir {
                label, children, ..
            }) = nodes.last()
            else {
                panic!("{}: a Worlds group", mode.label());
            };
            assert_eq!(label, CATEGORY);
            assert!(children.iter().all(|c| matches!(c, CatNode::Leaf { .. })));
            assert_eq!(all_slugs(children), all, "{}", mode.label());
        }

        let menu = generator_nodes(BrowseMode::Hierarchy, "");
        assert!(
            all_slugs(&menu)
                .iter()
                .all(|slug| world_by_slug(slug).is_none()),
            "the add menu lists a world"
        );

        let hits = |q: &str| -> HashSet<String> {
            all_slugs(&build_nodes(BrowseMode::Name, q))
                .into_iter()
                .filter(|slug| world_by_slug(slug).is_some())
                .map(str::to_string)
                .collect()
        };
        assert!(hits("Medieval world").contains("worlds_medieval"));
        assert!(
            hits("sports & rec").contains("worlds_sports_rec"),
            "by theme"
        );
        assert!(
            hits("neukolln").contains("worlds_berlin_neukoelln"),
            "by borough"
        );
        assert!(hits("Sch\u{f6}neberg").contains("worlds_berlin_tempelhof_schoeneberg"));
        for (query, group) in [("berlin", BERLIN_GROUP), ("procedural", PROCEDURAL_GROUP)] {
            let want: HashSet<String> = WORLDS
                .iter()
                .filter(|e| e.group() == group)
                .map(|e| e.slug.to_string())
                .collect();
            assert_eq!(hits(query), want, "{query}");
        }
        for (query, slug) in [
            ("mesa", "worlds_wild_west"),
            ("glacial", "worlds_alien_monolithic"),
        ] {
            assert!(hits(query).contains(slug), "{query} misses {slug}");
        }
        assert_eq!(hits("world").len(), WORLDS.len(), "the section's word");
        let avatars_hit = all_slugs(&build_nodes(BrowseMode::Name, "berlin"))
            .into_iter()
            .any(|slug| avatar_by_slug(slug).is_some());
        assert!(!avatars_hit, "an avatar is not a borough");
    }

    /// A world entry places nothing and is never pictured: dragging its row
    /// arms no drop, and selecting it asks the stage for nothing - nor does
    /// it take the stage from an Inventory pick that has a picture.
    #[test]
    fn a_world_entry_is_not_a_drag_source_and_is_never_staged() {
        use crate::item_preview::{Pick, PreviewSubject, wanted_subject};
        for entry in &WORLDS {
            assert!(!drag_placeable(entry.slug), "{} arms a drop", entry.slug);
            let pick = Pick {
                open: true,
                selected: Some(entry.slug),
                at: 5.0,
            };
            assert_eq!(wanted_subject(pick, Pick::default(), None), None);
        }
        let mut stash = crate::pds::InventoryRecord::default();
        stash.put_item("lantern".into(), Generator::default_cuboid(), None);
        let tick = bevy::ecs::change_detection::Tick::new(1);
        let got = wanted_subject(
            Pick {
                open: true,
                selected: Some("worlds_medieval"),
                at: 5.0,
            },
            Pick {
                open: true,
                selected: Some("lantern"),
                at: 1.0,
            },
            Some((&stash, tick)),
        );
        assert!(
            matches!(got, Some(PreviewSubject::Inventory { .. })),
            "a later world pick leaves the stage to the lantern"
        );
    }

    /// Every reason Load into my world and Copy to inventory are disabled
    /// is one these answer, in S2's words.
    #[test]
    fn the_world_buttons_say_why_they_are_disabled() {
        use crate::pds::InventoryRecord;
        use crate::pds::inventory::world::SIGN_IN_AGAIN;
        assert_eq!(world_entry_load_blocked(true, false, true), None);
        assert_eq!(
            world_entry_load_blocked(false, false, true),
            crate::ui::inventory::world::world_load_blocked_reason(false, false)
        );
        assert!(
            world_entry_load_blocked(false, false, true).is_some_and(|w| w.contains("own world"))
        );
        assert!(world_entry_load_blocked(true, true, true).is_some_and(|w| w.contains("loading")));
        assert_eq!(
            world_entry_load_blocked(true, false, false),
            Some(WORLD_NOT_LOADED)
        );

        let empty = InventoryRecord::default();
        assert!(
            world_copy_blocked(false, Some(&empty), true).is_some_and(|w| w.contains("Sign in"))
        );
        assert!(world_copy_blocked(true, None, true).is_some_and(|w| w.contains("not loaded")));
        assert_eq!(
            world_copy_blocked(true, Some(&empty), false).as_deref(),
            Some(SIGN_IN_AGAIN)
        );
        assert_eq!(world_copy_blocked(true, Some(&empty), true), None);
        let mut full = InventoryRecord::default();
        for i in 0..crate::config::state::MAX_INVENTORY_ITEMS {
            full.put_item(format!("g{i}"), Generator::default_cuboid(), None);
        }
        assert!(world_copy_blocked(true, Some(&full), true).is_some_and(|w| w.contains("full")));
    }

    /// Draw [`world_detail`] headless for `frames` frames with `events` on
    /// the last, AccessKit on: what was clicked, where each button is, and
    /// the texts painted.
    fn draw_world_detail(
        ctx: &egui::Context,
        entry: &WorldEntry,
        load_blocked: Option<&str>,
        copy_blocked: Option<&str>,
        events: Vec<egui::Event>,
        frame: u32,
    ) -> (Option<WorldClick>, Vec<(String, egui::Pos2)>, Vec<String>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(480.0, 640.0),
            )),
            time: Some(f64::from(frame) / 60.0),
            events,
            ..Default::default()
        };
        let mut click = None;
        let output = ctx.run_ui(input, |ui| {
            click = world_detail(ui, entry, load_blocked, copy_blocked, "Medieval world");
        });
        let buttons = output
            .platform_output
            .accesskit_update
            .expect("AccessKit is on")
            .nodes
            .iter()
            .filter_map(|(_, node)| {
                let label = node.label()?;
                let b = node.bounds()?;
                Some((
                    label.to_string(),
                    egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32),
                ))
            })
            .collect();
        fn texts(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => out.push(text.galley.text().to_string()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| texts(s, out)),
                _ => {}
            }
        }
        let mut painted = Vec::new();
        output
            .shapes
            .iter()
            .for_each(|clipped| texts(&clipped.shape, &mut painted));
        (click, buttons, painted)
    }

    /// The detail is a text card - the Inventory's no-picture sentence, the
    /// theme, the ground - and its buttons click when they can; when they
    /// cannot, a click does nothing and the reason is painted under them.
    #[test]
    fn the_world_detail_is_a_text_card_whose_buttons_click_or_say_why_not() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let entry = world("worlds_medieval");
        let press = |at, pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let at = |buttons: &[(String, egui::Pos2)], label: &str| {
            buttons
                .iter()
                .find(|(l, _)| l == label)
                .map(|(_, at)| *at)
                .unwrap_or_else(|| panic!("the detail draws {label}"))
        };
        let mut frame = 0;
        let mut click_on = |label: &str, load: Option<&str>, copy: Option<&str>| {
            frame += 10;
            let (_, buttons, painted) = draw_world_detail(&ctx, entry, load, copy, vec![], frame);
            let pos = at(&buttons, label);
            draw_world_detail(
                &ctx,
                entry,
                load,
                copy,
                vec![egui::Event::PointerMoved(pos)],
                frame + 1,
            );
            draw_world_detail(&ctx, entry, load, copy, vec![press(pos, true)], frame + 2);
            let (click, _, _) =
                draw_world_detail(&ctx, entry, load, copy, vec![press(pos, false)], frame + 3);
            (click, painted)
        };

        let (click, painted) = click_on("Load into my world", None, None);
        assert_eq!(click, Some(WorldClick::Load));
        for text in [
            crate::ui::inventory::browser::WORLD_NO_PICTURE,
            "Medieval",
            "Craggy",
            "Temperate Forest",
        ] {
            assert!(
                painted.iter().any(|p| p.contains(text)),
                "the card says {text}: {painted:?}"
            );
        }
        let (click, _) = click_on("Copy to inventory", None, None);
        assert_eq!(click, Some(WorldClick::Copy));

        let why_load =
            crate::ui::inventory::world::world_load_blocked_reason(false, false).unwrap();
        let why_copy = "Your inventory has not loaded yet.";
        let (click, painted) = click_on("Load into my world", Some(why_load), Some(why_copy));
        assert_eq!(click, None, "a disabled Load does nothing");
        assert!(painted.iter().any(|p| p.contains(why_load)), "{painted:?}");
        assert!(painted.iter().any(|p| p.contains(why_copy)), "{painted:?}");
        let (click, _) = click_on("Copy to inventory", Some(why_load), Some(why_copy));
        assert_eq!(click, None, "a disabled Copy does nothing");
    }

    /// Nothing is built on selection or per frame: thirty frames of a world
    /// entry's detail, the tree that lists it, a search that finds it and
    /// the preview mirror holding it build no world.
    #[test]
    fn nothing_is_built_on_selection_or_per_frame() {
        use crate::item_preview::PreviewRequest;
        use crate::ui::toolbar::UiPanels;
        use bevy::ecs::system::RunSystemOnce;

        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut world_app = World::new();
        world_app.init_resource::<PreviewRequest>();
        world_app.insert_resource(UiPanels {
            catalogue: true,
            ..Default::default()
        });
        let mut browser = CatalogueBrowser::default();
        browser.select_for_test(Some("worlds_berlin_mitte"), 1.0);
        world_app.insert_resource(browser);

        crate::catalogue::worlds::take_build_count();
        for frame in 0..30 {
            for slug in ["worlds_berlin_mitte", "worlds_fantasy"] {
                let entry = world(slug);
                let blocked = world_entry_load_blocked(true, false, true);
                draw_world_detail(&ctx, entry, blocked, None, vec![], frame);
            }
            let _ = build_nodes(BrowseMode::Hierarchy, "mitte");
            world_app
                .run_system_once(mirror_preview_request)
                .expect("the mirror runs");
        }
        assert_eq!(world_app.resource::<PreviewRequest>().0, None);
        assert_eq!(
            crate::catalogue::worlds::take_build_count(),
            0,
            "a world is built only on a click"
        );
    }

    const OWNER: &str = "did:plc:catalogue-owner";

    /// An owner in their own world with the World Editor holding a
    /// selection and the room undo ring watching - as the app runs it.
    fn world_app(old: &crate::pds::RoomRecord) -> App {
        use crate::state::{CurrentRoomDid, LiveRoomRecord};
        use crate::ui::room::RoomEditorState;
        let mut app = App::new();
        app.insert_resource(LiveRoomRecord(old.clone()));
        app.insert_resource(crate::ui::room::placement_focus_tests::session(OWNER));
        app.insert_resource(CurrentRoomDid(String::from(OWNER)));
        let mut editor = RoomEditorState::default();
        editor.selected_placement = Some(0);
        editor.seed_raw_for_test(old);
        app.insert_resource(editor);
        app.init_resource::<crate::ui::undo::RoomUndoHistory>();
        app.init_resource::<crate::state::RoomWriteSignals>();
        app.init_resource::<crate::ui::undo::PendingUndoLabels>();
        app.init_resource::<crate::notify::Toasts>();
        app.add_systems(Update, crate::ui::undo::capture_room_history);
        app.update();
        app
    }

    /// Load `slug` as the confirmation does, with the owner in `room` and
    /// `loading` world loads in flight.
    fn load_entry(
        app: &mut App,
        slug: &'static str,
        room: &str,
        loading: bool,
    ) -> Result<(), String> {
        use crate::state::LiveRoomRecord;
        use crate::ui::room::RoomEditorState;
        use bevy::ecs::system::RunSystemOnce;
        let room = room.to_owned();
        app.world_mut()
            .run_system_once(
                move |mut live: Option<ResMut<LiveRoomRecord>>,
                      mut editor: Option<ResMut<RoomEditorState>>,
                      mut labels: ResMut<crate::ui::undo::PendingUndoLabels>,
                      mut toasts: ResMut<crate::notify::Toasts>| {
                    load_world_entry(
                        world(slug),
                        OWNER,
                        loading,
                        Some(room.as_str()),
                        live.as_mut().map(|live| live.reborrow()),
                        editor.as_mut().map(|editor| editor.reborrow()),
                        &mut labels,
                        &mut toasts,
                        0.0,
                    )
                },
            )
            .expect("the load runs")
    }

    fn value(record: &crate::pds::RoomRecord) -> serde_json::Value {
        serde_json::to_value(record).expect("serializes")
    }

    /// Load from the Catalogue is S2's load: the live world IS the built
    /// one, as one undo step labelled for the entry that one undo takes
    /// back, with no World Editor selection left naming the old world.
    #[test]
    fn loading_a_world_entry_replaces_the_live_world_in_one_undo_step() {
        use crate::state::LiveRoomRecord;
        use crate::ui::room::RoomEditorState;
        use crate::ui::undo::RoomUndoHistory;
        let old = crate::pds::inventory::world::tests::seeded_world(41);
        let mut app = world_app(&old);
        let before = app.world().resource::<RoomUndoHistory>().len();
        let entry = world("worlds_steampunk");
        let want = built_world(entry, OWNER).expect("builds");

        crate::catalogue::worlds::take_build_count();
        assert_eq!(load_entry(&mut app, entry.slug, OWNER, false), Ok(()));
        assert_eq!(crate::catalogue::worlds::take_build_count(), 1, "one build");
        app.update();
        app.update();

        assert_eq!(
            value(&app.world().resource::<LiveRoomRecord>().0),
            value(&want),
            "the built world is the live one"
        );
        assert!(!app.world().resource::<RoomEditorState>().has_selection());
        let mut history = app.world_mut().resource_mut::<RoomUndoHistory>();
        assert_eq!(history.len(), before + 1, "one undo step");
        let (undone, label) = history.undo().expect("undoable");
        assert_eq!(label, format!("load {}", entry.name));
        assert_eq!(value(&undone.record), value(&old), "one undo takes it back");
    }

    /// Outside your own world, while a load is in flight, or before your
    /// world has arrived, a Load is refused, builds nothing and changes
    /// nothing.
    #[test]
    fn loading_a_world_entry_is_refused_outside_your_world() {
        use crate::state::LiveRoomRecord;
        use crate::ui::undo::RoomUndoHistory;
        let old = crate::pds::inventory::world::tests::seeded_world(42);
        let mut app = world_app(&old);
        let before = app.world().resource::<RoomUndoHistory>().len();
        crate::catalogue::worlds::take_build_count();

        let elsewhere = load_entry(&mut app, "worlds_pirate", "did:plc:someone-else", false);
        assert!(elsewhere.is_err_and(|why| why.contains("own world")));
        let busy = load_entry(&mut app, "worlds_pirate", OWNER, true);
        assert!(busy.is_err_and(|why| why.contains("loading")));
        app.update();
        app.update();
        assert_eq!(
            value(&app.world().resource::<LiveRoomRecord>().0),
            value(&old)
        );
        assert_eq!(app.world().resource::<RoomUndoHistory>().len(), before);

        app.world_mut().remove_resource::<LiveRoomRecord>();
        let unloaded = load_entry(&mut app, "worlds_pirate", OWNER, false);
        assert_eq!(unloaded, Err(WORLD_NOT_LOADED.to_owned()));
        assert_eq!(
            crate::catalogue::worlds::take_build_count(),
            0,
            "a refusal builds nothing"
        );
    }

    /// Copy to inventory stashes S2's snapshot of the built world, under a
    /// free name built on the entry's; a full stash or a sign-in without
    /// the grant is refused before anything is built.
    #[test]
    fn copying_a_world_entry_stashes_the_built_world_under_a_free_name() {
        use crate::pds::InventoryRecord;
        use crate::pds::inventory::WorldItem;
        let entry = world("worlds_berlin_neukoelln");
        let built = built_world(entry, OWNER).expect("builds");
        let want = WorldItem::snapshot(&built).expect("copies");
        let mut inventory = InventoryRecord::default();
        inventory.put_item(entry.name.into(), Generator::default_cuboid(), None);
        let mut toasts = crate::notify::Toasts::default();

        crate::catalogue::worlds::take_build_count();
        let name =
            copy_world_entry(entry, OWNER, &mut inventory, true, &mut toasts, 0.0).expect("copies");
        assert_eq!(crate::catalogue::worlds::take_build_count(), 1, "one build");
        assert_eq!(name, format!("{}_1", entry.name), "free across every kind");
        let item = &inventory.worlds[&name];
        assert_eq!(item, &want, "S2's snapshot of the built world");
        assert_eq!(
            item.head().summary.borough.as_deref(),
            Some("Neuk\u{f6}lln")
        );
        let held = crate::pds::inventory::world::join_parts(
            item.head(),
            item.slices().expect("held for the save"),
        )
        .expect("loads");
        assert_eq!(
            value(&held),
            value(&crate::pds::inventory::world::tests::as_loaded(&built))
        );

        let refused = copy_world_entry(entry, OWNER, &mut inventory, false, &mut toasts, 0.0);
        assert_eq!(
            refused,
            Err(crate::pds::inventory::world::SIGN_IN_AGAIN.to_owned())
        );
        let cap = crate::config::state::MAX_INVENTORY_ITEMS;
        let mut i = 0;
        while inventory.item_count() < cap {
            inventory.put_item(format!("g{i}"), Generator::default_cuboid(), None);
            i += 1;
        }
        let full = copy_world_entry(entry, OWNER, &mut inventory, true, &mut toasts, 0.0);
        assert!(full.is_err_and(|why| why.contains("full")));
        assert_eq!(inventory.item_count(), cap);
        assert_eq!(
            crate::catalogue::worlds::take_build_count(),
            0,
            "a refusal builds nothing"
        );
    }
}
