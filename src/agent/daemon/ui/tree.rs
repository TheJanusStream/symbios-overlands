//! Reading the interface (#1424): egui's AccessKit tree as the agent may
//! see it.
//!
//! Once AccessKit is on, egui reports every widget of a pass - its words,
//! role, bounds and state - as one tree whose root holds the pass's layers:
//! each window, each open menu, each dialog, the toasts. The agent reads
//! the windows on the allow-list ([`policy`]) and the dialogs and menus its
//! own clicks raised from them, and nothing else. A refused window's
//! subtree is never walked, so no word of it is copied into anything - a
//! listing, a lookup, an error's near misses. Only its title, which the
//! game writes, is read, to say it is there.
//!
//! **Names.** A control is addressed by where a person finds it:
//! `Window > section > row > control`. The section is an open collapsing
//! header's name; the row, the text to the control's left on its row - how
//! `Theme:` names the combo box beside it, and a grid row names its padlock;
//! the control, its own label, a text field's placeholder, or what kind of
//! control it is. Never its value: a name that changed on the first edit
//! could not be used twice. Controls that still share a name are told apart
//! by `#1`, `#2` in draw order.
//!
//! **What the tree does not say.** A closed collapsing header is a plain
//! button in it - an open one is a button followed by its body, which is how
//! it is known. And bounds are unclipped: a control scrolled out of its area
//! is still laid out and still reported, so whether it is in view is worked
//! out from its window's rect and the scroll bar beside the area it sits in.
//! egui_ltreeview's rows are words with no action of their own under a
//! clickable area beside them; they are `row`s, which a pointer selects.

use std::collections::{HashMap, HashSet};

use bevy_egui::egui;
use bevy_egui::egui::accesskit::{Action, Node, NodeId, Role, Toggled, TreeUpdate};
use serde_json::{Map, Value, json};

use super::policy::{self, Refusal};

/// The egui area the game's toasts draw in (`ui::toast`).
pub(super) const TOASTS_AREA: &str = "overlands-toasts";

/// The most of a value, or of a text used as a name, a listing shows.
const SHOWN_CHARS: usize = 400;

/// Between two parts of a path.
pub(super) const SEPARATOR: &str = " > ";

/// What a control is, in the answers' words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Button,
    Checkbox,
    Radio,
    ComboBox,
    TextField,
    Slider,
    Number,
    Colour,
    Text,
    /// A row of a tree (egui_ltreeview): words a pointer selects.
    Row,
    Link,
    Image,
    Progress,
}

impl Kind {
    pub(super) fn word(self) -> &'static str {
        match self {
            Self::Button => "button",
            Self::Checkbox => "checkbox",
            Self::Radio => "radio button",
            Self::ComboBox => "combo box",
            Self::TextField => "text field",
            Self::Slider => "slider",
            Self::Number => "number",
            Self::Colour => "colour",
            Self::Text => "text",
            Self::Row => "row",
            Self::Link => "link",
            Self::Image => "image",
            Self::Progress => "progress",
        }
    }

    fn of(node: &Node) -> Option<Self> {
        Some(match node.role() {
            Role::Button => Self::Button,
            Role::CheckBox => Self::Checkbox,
            Role::RadioButton => Self::Radio,
            Role::ComboBox => Self::ComboBox,
            Role::TextInput | Role::MultilineTextInput | Role::PasswordInput => Self::TextField,
            Role::Slider => Self::Slider,
            Role::SpinButton => Self::Number,
            Role::ColorWell => Self::Colour,
            Role::Label => Self::Text,
            Role::Link => Self::Link,
            Role::Image => Self::Image,
            Role::ProgressIndicator => Self::Progress,
            _ => return None,
        })
    }
}

/// A place the interface draws that the agent may read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Surface {
    /// A window on the allow-list, by its title.
    Window(String),
    /// A dialog the agent's own click raised in `from`.
    Dialog { from: String },
    /// A menu or a combo box's list the agent's own click opened in `from`.
    Menu { from: String },
    /// The game's notices.
    Toasts,
}

impl Surface {
    /// The first part of every path in it.
    pub(super) fn name(&self) -> String {
        match self {
            Self::Window(title) => title.clone(),
            Self::Dialog { from } => format!("{from} dialog"),
            Self::Menu { from } => format!("{from} menu"),
            Self::Toasts => "toasts".to_owned(),
        }
    }

    /// The window whose refusal list applies in it.
    pub(super) fn window(&self) -> &str {
        match self {
            Self::Window(title) => title,
            Self::Dialog { from } | Self::Menu { from } => from,
            Self::Toasts => "",
        }
    }
}

/// A surface as this pass drew it.
#[derive(Clone, Debug)]
pub(super) struct SurfaceInfo {
    pub surface: Surface,
    /// Its node straight under the tree's root.
    pub node: NodeId,
    /// Where it is drawn, in egui points, when it has bounds of its own.
    pub rect: Option<egui::Rect>,
}

impl SurfaceInfo {
    /// Whether `layer` - egui's top layer at some point - is this surface's
    /// own. A window's or a menu's node is its layer's id salted `"move"`
    /// (egui's `Area` gives its move handle that id, and a window's node is
    /// its move handle); a dialog's or the toasts' node is its layer's id.
    pub(super) fn owns(&self, layer: egui::LayerId) -> bool {
        match self.surface {
            Surface::Window(_) | Surface::Menu { .. } => {
                layer.id.with("move").value() == self.node.0
            }
            Surface::Dialog { .. } | Surface::Toasts => layer.id.value() == self.node.0,
        }
    }
}

/// One thing a surface shows: a control, or words.
#[derive(Clone, Debug)]
pub(super) struct Entry {
    pub node: NodeId,
    /// Which of [`UiTree::surfaces`] it is in.
    pub surface: usize,
    pub path: String,
    pub kind: Kind,
    /// Its own label, when it has one.
    pub label: Option<String>,
    /// A field's contents, a combo box's choice, a number as it is shown.
    pub value: Option<String>,
    pub number: Option<f64>,
    pub range: Option<(f64, f64)>,
    /// Whether a checkbox is ticked, or a selectable button selected.
    pub checked: Option<bool>,
    pub enabled: bool,
    pub focused: bool,
    /// A collapsing header whose section is open, its contents listed after it.
    pub open: bool,
    pub in_view: bool,
    pub multiline: bool,
    pub password: bool,
    /// Where it is drawn, in egui points.
    pub rect: egui::Rect,
    /// Whether it sits in a scroll area.
    pub scrolls: bool,
    pub clickable: bool,
    pub settable: bool,
    pub focusable: bool,
    pub refused: Option<Refusal>,
}

impl Entry {
    /// The entry as a listing shows it: its path and kind, and only the
    /// state that is not the usual.
    pub(super) fn to_json(&self) -> Value {
        let mut out = Map::new();
        out.insert("path".into(), json!(self.path));
        out.insert("kind".into(), json!(self.kind.word()));
        if let Some(value) = &self.value
            && !self.password
            && !matches!(self.kind, Kind::Text | Kind::Row)
        {
            out.insert("value".into(), json!(shown(value)));
        }
        if let Some(number) = self.number {
            out.insert("number".into(), json!(number));
        }
        if let Some((min, max)) = self.range {
            out.insert("min".into(), json!(min));
            out.insert("max".into(), json!(max));
        }
        if let Some(checked) = self.checked {
            out.insert("checked".into(), json!(checked));
        }
        if self.open {
            out.insert("open".into(), json!(true));
        }
        if !self.enabled {
            out.insert("disabled".into(), json!(true));
        }
        if self.focused {
            out.insert("focused".into(), json!(true));
        }
        if !self.in_view {
            out.insert("out_of_view".into(), json!(true));
        }
        if let Some(refused) = self.refused {
            out.insert("refused".into(), refused.to_json());
        }
        Value::Object(out)
    }

    /// Whether an action on it is aimed at its words rather than a control.
    pub(super) fn is_words(&self) -> bool {
        matches!(self.kind, Kind::Text)
    }
}

/// `text`, cut to what a listing shows.
fn shown(text: &str) -> String {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(SHOWN_CHARS).collect();
    let rest = chars.count();
    if rest == 0 {
        head
    } else {
        format!("{head}... ({rest} more characters)")
    }
}

/// One pass's interface, as the agent may read it.
#[derive(Clone, Debug, Default)]
pub(super) struct UiTree {
    pub surfaces: Vec<SurfaceInfo>,
    pub entries: Vec<Entry>,
    /// Open windows the agent may not read, by title.
    pub refused_windows: Vec<String>,
    /// Every node straight under the root. What a click raised is what was
    /// not here before it.
    pub roots: Vec<NodeId>,
}

impl UiTree {
    /// Read `update`, a pass's tree, laid out on `screen`. `raised` names
    /// the dialogs and menus the agent's own clicks raised, by their node
    /// under the root; any other one is not read.
    pub(super) fn read(
        update: &TreeUpdate,
        screen: egui::Rect,
        raised: &HashMap<NodeId, Surface>,
    ) -> Option<Self> {
        let root = update.tree.as_ref()?.root;
        let nodes: HashMap<NodeId, &Node> = update.nodes.iter().map(|(id, n)| (*id, n)).collect();
        let root_node = nodes.get(&root)?;
        let toasts = NodeId(egui::Id::new(TOASTS_AREA).value());
        let mut walker = Walker {
            nodes: &nodes,
            parents: HashMap::new(),
            drafts: Vec::new(),
        };
        let mut out = Self {
            roots: root_node.children().to_vec(),
            ..Self::default()
        };
        for &child in root_node.children() {
            let Some(node) = nodes.get(&child) else {
                continue;
            };
            // Decided by role and by id alone. A window's title is the one
            // thing read before this line, and the game writes titles.
            let surface = if node.role() == Role::Window {
                let title = node.label().unwrap_or_default();
                if !policy::readable(title) {
                    out.refused_windows.push(title.to_owned());
                    continue;
                }
                Surface::Window(title.to_owned())
            } else if child == toasts {
                Surface::Toasts
            } else if let Some(surface) = raised.get(&child) {
                surface.clone()
            } else {
                continue;
            };
            let rect = node.bounds().map(rect_of);
            let view = rect.map_or(screen, |r| r.intersect(screen));
            out.surfaces.push(SurfaceInfo {
                surface,
                node: child,
                rect,
            });
            let index = out.surfaces.len() - 1;
            walker.visit(child, index, &[], view, false, None);
        }
        out.entries = walker.entries(&out.surfaces, update.focus);
        Some(out)
    }

    /// The surface named `name` - a window by its title, in any case, or a
    /// dialog or menu by the name its paths begin with.
    pub(super) fn surface_named(&self, name: &str) -> Option<usize> {
        let wanted = name.trim().to_lowercase();
        self.surfaces
            .iter()
            .position(|s| s.surface.name().to_lowercase() == wanted)
    }

    /// Every entry of the surface `index`, in draw order.
    pub(super) fn entries_of(&self, index: usize) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |e| e.surface == index)
    }

    /// A surface's listing: its name and everything it shows.
    pub(super) fn surface_json(&self, index: usize) -> Value {
        let entries: Vec<Value> = self.entries_of(index).map(Entry::to_json).collect();
        json!({
            "surface": self.surfaces[index].surface.name(),
            "entries": entries,
        })
    }

    /// The one control `address` names, tried three ways in turn: its whole
    /// path; the end of its path; and, for a control with no name of its
    /// own, the words to its left on its row - so `Theme:` names the combo
    /// box beside it. Words are never an action's target (a tree's rows,
    /// which a pointer selects, are). Only readable surfaces are searched,
    /// so a miss's near misses are readable too.
    pub(super) fn resolve(&self, address: &str) -> Result<&Entry, String> {
        let wanted = segments(address);
        if wanted.is_empty() {
            return Err(
                "name a control by its path, as `agent ui show <window>` lists them".to_owned(),
            );
        }
        let controls: Vec<&Entry> = self.entries.iter().filter(|e| !e.is_words()).collect();
        let whole = |e: &Entry| segments(&e.path) == wanted;
        let ending = |e: &Entry| segments(&e.path).ends_with(&wanted);
        let captioned = |e: &Entry| {
            let parts = segments(&e.path);
            e.label.is_none()
                && e.kind != Kind::Row
                && parts.len() > 1
                && parts[..parts.len() - 1].ends_with(&wanted)
        };
        // Last, the same ignoring the symbols a label is dressed in: an
        // icon before it ("\u{270F} Edit audio\u{2026}"), an ellipsis after.
        let loose_wanted: Vec<String> = wanted.iter().map(|w| loose(w)).collect();
        let loosely = |e: &Entry| {
            !loose_wanted.iter().any(String::is_empty)
                && segments(&e.path)
                    .iter()
                    .map(|p| loose(p))
                    .collect::<Vec<_>>()
                    .ends_with(&loose_wanted)
        };
        let stages: [&dyn Fn(&Entry) -> bool; 4] = [&whole, &ending, &captioned, &loosely];
        for stage in stages {
            let found: Vec<&Entry> = controls.iter().copied().filter(|e| stage(e)).collect();
            match found.as_slice() {
                [one] => return Ok(one),
                [] => {}
                many => {
                    let paths: Vec<&str> = many.iter().take(10).map(|e| e.path.as_str()).collect();
                    return Err(format!(
                        "{address:?} names {} controls: {}; give the whole path",
                        many.len(),
                        paths.join(" | ")
                    ));
                }
            }
        }
        let last = wanted.last().map(|s| s.to_lowercase()).unwrap_or_default();
        let mut near: Vec<&str> = self
            .entries
            .iter()
            .filter(|e| e.path.to_lowercase().contains(&last))
            .map(|e| e.path.as_str())
            .collect();
        near.truncate(8);
        if near.is_empty() {
            Err(format!(
                "nothing open is called {address:?}; `agent ui` says what is open"
            ))
        } else {
            Err(format!(
                "nothing open is called {address:?}; near it: {}",
                near.join(" | ")
            ))
        }
    }
}

/// A path part without the symbols around its words, in lower case.
fn loose(part: &str) -> String {
    part.trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

/// A path's parts, trimmed.
pub(super) fn segments(path: &str) -> Vec<String> {
    path.split(SEPARATOR.trim())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

fn rect_of(b: bevy_egui::egui::accesskit::Rect) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(b.x0 as f32, b.y0 as f32),
        egui::pos2(b.x1 as f32, b.y1 as f32),
    )
}

/// An entry before it has a path.
struct Draft {
    node: NodeId,
    surface: usize,
    kind: Kind,
    sections: Vec<String>,
    view: egui::Rect,
    open: bool,
    scrolls: bool,
}

struct Walker<'a> {
    nodes: &'a HashMap<NodeId, &'a Node>,
    parents: HashMap<NodeId, NodeId>,
    drafts: Vec<Draft>,
}

impl Walker<'_> {
    /// Walk `id`'s subtree: `sections` are the open headers it sits under,
    /// `view` the part of the screen its surface and scroll areas show,
    /// `rows` the clickable area of a tree whose rows these may be.
    fn visit(
        &mut self,
        id: NodeId,
        surface: usize,
        sections: &[String],
        view: egui::Rect,
        scrolls: bool,
        rows: Option<egui::Rect>,
    ) {
        let Some(node) = self.nodes.get(&id).copied() else {
            return;
        };
        if matches!(
            node.role(),
            Role::TextRun | Role::Splitter | Role::ScrollBar
        ) {
            return;
        }
        if let Some(mut kind) = Kind::of(node) {
            if kind == Kind::Text {
                if node.value().is_none_or(|v| v.trim().is_empty()) {
                    return;
                }
                let in_rows = rows.is_some_and(|area| {
                    node.bounds()
                        .is_some_and(|b| area.contains(rect_of(b).center()))
                });
                if in_rows && !node.supports_action(Action::Click) {
                    kind = Kind::Row;
                }
            }
            self.drafts.push(Draft {
                node: id,
                surface,
                kind,
                sections: sections.to_vec(),
                view,
                open: false,
                scrolls,
            });
        }
        let children = node.children();
        let folded = folded_into_sliders(self.nodes, children);
        let section = open_section(self.nodes, children);
        let viewports = viewports(self.nodes, children, view);
        let mut rows_here = rows;
        for (index, &child) in children.iter().enumerate() {
            self.parents.insert(child, id);
            if folded.contains(&child) {
                continue;
            }
            // A clickable area with no role of its own, with more after it:
            // egui_ltreeview's tree, whose rows are drawn after it.
            if let Some(area) = self.nodes.get(&child)
                && area.role() == Role::Unknown
                && area.supports_action(Action::Click)
                && index + 1 < children.len()
                && let Some(b) = area.bounds()
            {
                rows_here = Some(rect_of(b));
            }
            let (child_view, child_scrolls) = match viewports.get(&child) {
                Some(viewport) => (view.intersect(*viewport), true),
                None => (view, scrolls),
            };
            let body: Vec<String>;
            let child_sections = match &section {
                Some((_, name)) if index == 1 => {
                    body = sections.iter().cloned().chain([name.clone()]).collect();
                    body.as_slice()
                }
                _ => sections,
            };
            let before = self.drafts.len();
            self.visit(
                child,
                surface,
                child_sections,
                child_view,
                child_scrolls,
                rows_here,
            );
            if index == 0
                && let Some((header, _)) = &section
                && let Some(draft) = self.drafts.get_mut(before)
                && draft.node == *header
            {
                draft.open = true;
            }
        }
    }

    /// The drafts as entries, each with its path.
    fn entries(self, surfaces: &[SurfaceInfo], focus: NodeId) -> Vec<Entry> {
        let mut entries: Vec<Entry> = Vec::with_capacity(self.drafts.len());
        for draft in &self.drafts {
            let Some(node) = self.nodes.get(&draft.node).copied() else {
                continue;
            };
            let rect = node.bounds().map(rect_of).unwrap_or(egui::Rect::NOTHING);
            let label = node
                .label()
                .filter(|l| !l.trim().is_empty())
                .map(str::to_owned);
            let words = matches!(draft.kind, Kind::Text | Kind::Row);
            let name = if words {
                node.value().map(|v| shown(v.trim()))
            } else if draft.kind == Kind::TextField {
                label.clone().or_else(|| {
                    node.placeholder()
                        .map(str::trim)
                        .filter(|p| !p.is_empty())
                        .map(str::to_owned)
                })
            } else {
                label.clone()
            };
            let caption = if words {
                None
            } else {
                self.caption(draft.node, rect)
            };
            let surface = &surfaces[draft.surface];
            let mut parts = vec![surface.surface.name()];
            parts.extend(draft.sections.iter().cloned());
            if let Some(caption) = &caption
                && caption.as_str() != name.as_deref().unwrap_or_default()
            {
                parts.push(caption.clone());
            }
            parts.push(name.clone().unwrap_or_else(|| draft.kind.word().to_owned()));
            let role = node.role();
            let number = node.numeric_value().map(tidy);
            let range = node
                .min_numeric_value()
                .zip(node.max_numeric_value())
                .filter(|(min, max)| min.is_finite() && max.is_finite())
                .map(|(min, max)| (tidy(min), tidy(max)));
            let checked = node.toggled().map(|t| t == Toggled::True);
            entries.push(Entry {
                node: draft.node,
                surface: draft.surface,
                path: parts.join(SEPARATOR),
                kind: draft.kind,
                refused: policy::control_refusal(
                    surface.surface.window(),
                    draft.kind,
                    name.as_deref(),
                ),
                label,
                value: node.value().map(str::to_owned),
                number,
                range,
                checked,
                enabled: !node.is_disabled(),
                focused: focus == draft.node,
                open: draft.open,
                in_view: rect.is_positive() && draft.view.contains(rect.center()),
                multiline: role == Role::MultilineTextInput,
                password: role == Role::PasswordInput,
                rect,
                scrolls: draft.scrolls,
                clickable: node.supports_action(Action::Click),
                settable: node.supports_action(Action::SetValue),
                focusable: node.supports_action(Action::Focus),
            });
        }
        number_the_duplicates(&mut entries);
        entries
    }

    /// The words a person reads a control by, when it has none of its own
    /// or shares its own with others: the text to its left on its row - a
    /// preceding sibling label, looked for up to three levels out, that
    /// shares the control's row and ends before it begins - or, failing
    /// that, the words just above the group it starts ("Ground
    /// avoidance:" over a row of choices).
    fn caption(&self, id: NodeId, rect: egui::Rect) -> Option<String> {
        if !rect.is_positive() {
            return None;
        }
        self.caption_beside(id, rect)
            .or_else(|| self.caption_above(id, rect))
    }

    fn caption_beside(&self, id: NodeId, rect: egui::Rect) -> Option<String> {
        let mut current = id;
        for _ in 0..3 {
            let parent = *self.parents.get(&current)?;
            let siblings = self.nodes.get(&parent)?.children();
            let at = siblings.iter().position(|s| *s == current)?;
            for sibling in siblings[..at].iter().rev() {
                let Some(node) = self.nodes.get(sibling) else {
                    continue;
                };
                if node.role() != Role::Label {
                    continue;
                }
                let Some(b) = node.bounds().map(rect_of) else {
                    continue;
                };
                let same_row =
                    b.y_range().contains(rect.center().y) || rect.y_range().contains(b.center().y);
                if same_row && b.max.x <= rect.min.x + 1.0 {
                    return words_of(node);
                }
            }
            current = parent;
        }
        None
    }

    /// The label just before the group a control belongs to - the row or
    /// column of choices it is one of - drawn just above it, with nothing
    /// between them. The controls before it on its own row are its group,
    /// not above it, so they are passed over.
    fn caption_above(&self, id: NodeId, rect: egui::Rect) -> Option<String> {
        let mut current = id;
        for _ in 0..3 {
            let parent = *self.parents.get(&current)?;
            let siblings = self.nodes.get(&parent)?.children();
            let at = siblings.iter().position(|s| *s == current)?;
            let before = siblings[..at].iter().rev().find(|s| {
                !self
                    .nodes
                    .get(s)
                    .and_then(|n| n.bounds())
                    .map(rect_of)
                    .is_some_and(|b| b.y_range().contains(rect.center().y))
            });
            let Some(before) = before else {
                // It opens its group: the words, if any, are above that.
                current = parent;
                continue;
            };
            let node = self.nodes.get(before)?;
            let b = node.bounds().map(rect_of)?;
            let above = b.max.y <= rect.min.y + 2.0 && rect.min.y - b.max.y < 24.0;
            return (node.role() == Role::Label && above)
                .then(|| words_of(node))
                .flatten()
                .filter(|words| is_heading(words));
        }
        None
    }
}

/// Whether words read as a heading over a group - "Ground avoidance:",
/// "Interface size" - rather than a sentence or a question above it, or a
/// line of figures ("6 instruments · 21 notes"), which changes with what it
/// counts: each would make a poor name for what follows.
fn is_heading(words: &str) -> bool {
    words.chars().count() <= 48
        && !words.ends_with(['.', '?', '!'])
        && !words.chars().any(|c| c.is_ascii_digit())
}

/// A label's words, as a caption.
fn words_of(node: &Node) -> Option<String> {
    node.value()
        .map(|v| shown(v.trim()))
        .filter(|v| !v.is_empty())
}

/// A slider's own number box and label, which egui reports beside it:
/// the label it is labelled by, and the number box labelled by the same.
fn folded_into_sliders(nodes: &HashMap<NodeId, &Node>, children: &[NodeId]) -> HashSet<NodeId> {
    let labels: HashSet<NodeId> = children
        .iter()
        .filter_map(|c| nodes.get(c))
        .filter(|n| n.role() == Role::Slider)
        .flat_map(|n| n.labelled_by().iter().copied())
        .collect();
    children
        .iter()
        .copied()
        .filter(|c| {
            labels.contains(c)
                || nodes.get(c).is_some_and(|n| {
                    n.role() == Role::SpinButton
                        && n.labelled_by().iter().any(|l| labels.contains(l))
                })
        })
        .collect()
}

/// An open collapsing header among `children`: exactly its button and,
/// below it, the body container. Returns the button and its name.
fn open_section(nodes: &HashMap<NodeId, &Node>, children: &[NodeId]) -> Option<(NodeId, String)> {
    let [header, body] = children else {
        return None;
    };
    let header_node = nodes.get(header)?;
    let body_node = nodes.get(body)?;
    if header_node.role() != Role::Button || body_node.role() != Role::GenericContainer {
        return None;
    }
    let name = header_node.label()?.trim().to_owned();
    if name.is_empty() {
        return None;
    }
    let header_rect = rect_of(header_node.bounds()?);
    let body_top = top_of(nodes, *body)?;
    (body_top >= header_rect.max.y - 1.0).then_some((*header, name))
}

/// The top of everything drawn under `id`.
fn top_of(nodes: &HashMap<NodeId, &Node>, id: NodeId) -> Option<f32> {
    let node = nodes.get(&id)?;
    let own = node.bounds().map(|b| b.y0 as f32);
    node.children()
        .iter()
        .filter_map(|c| top_of(nodes, *c))
        .chain(own)
        .reduce(f32::min)
}

/// What each scroll area among `children` shows: the container before a
/// scroll bar, limited to the bar's span - vertically for a tall bar,
/// horizontally for a wide one.
fn viewports(
    nodes: &HashMap<NodeId, &Node>,
    children: &[NodeId],
    view: egui::Rect,
) -> HashMap<NodeId, egui::Rect> {
    let mut out: HashMap<NodeId, egui::Rect> = HashMap::new();
    for (index, child) in children.iter().enumerate() {
        let Some(bar) = nodes.get(child) else {
            continue;
        };
        if bar.role() != Role::ScrollBar {
            continue;
        }
        let Some(b) = bar.bounds().map(rect_of) else {
            continue;
        };
        let Some(content) = children[..index].iter().rev().find(|c| {
            nodes
                .get(c)
                .is_some_and(|n| n.role() == Role::GenericContainer)
        }) else {
            continue;
        };
        let span = if b.height() >= b.width() {
            egui::Rect::from_x_y_ranges(view.x_range(), b.y_range())
        } else {
            egui::Rect::from_x_y_ranges(b.x_range(), view.y_range())
        };
        out.entry(*content)
            .and_modify(|r| *r = r.intersect(span))
            .or_insert(span);
    }
    out
}

/// Give every path that more than one control shares a `#n`, in draw
/// order - and likewise words. A control and words may share a path (a
/// section header and a label reading the same): only controls are ever
/// aimed at, so that is no ambiguity, and numbering them against each
/// other would make the control's name hang on the words beside it.
fn number_the_duplicates(entries: &mut [Entry]) {
    let key = |entry: &Entry| (entry.is_words(), entry.path.clone());
    let mut counts: HashMap<(bool, String), usize> = HashMap::new();
    for entry in entries.iter() {
        *counts.entry(key(entry)).or_default() += 1;
    }
    let mut seen: HashMap<(bool, String), usize> = HashMap::new();
    for entry in entries.iter_mut() {
        let key = key(entry);
        if counts[&key] > 1 {
            let n = seen.entry(key).or_default();
            *n += 1;
            entry.path = format!("{} #{n}", entry.path);
        }
    }
}

/// A number as the control holds it. egui hands AccessKit an `f64`, and an
/// `f32` widened to one prints its binary noise (-0.584 as
/// -0.5839999914169312, #1428): a value that is exactly an `f32` is given
/// as that `f32`'s shortest decimal, which reads back to the same bits.
fn tidy(n: f64) -> f64 {
    let single = n as f32;
    if f64::from(single) == n {
        single.to_string().parse().unwrap_or(n)
    } else {
        n
    }
}

#[cfg(test)]
mod tests {
    use super::tidy;

    /// An `f32` slider's value reads as the `f32` it is - not its widened
    /// binary noise - and a value no `f32` holds is left as it came (#1428).
    #[test]
    fn a_number_reads_as_the_control_holds_it() {
        assert_eq!(tidy(f64::from(-0.584_f32)), -0.584);
        assert_eq!(tidy(f64::from(0.317_f32)).to_string(), "0.317");
        assert_eq!(tidy(0.1), 0.1, "an f64 that no f32 holds exactly");
        let seed = 6_599_735_983_035_016_542_u64 as f64;
        assert_eq!(tidy(seed), seed);
    }
}
